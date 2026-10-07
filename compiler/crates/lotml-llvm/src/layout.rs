//! The emitter's layouts against C's (specs/llvm-parity R3.1): the runtime's structs as the module's
//! header defines them and the structs and cells of a program's types as `types` defines them,
//! measured in the IR by `getelementptr` and in C by `sizeof` and `offsetof`, both compiled by the
//! same `clang`; with the sizes and offsets the emitter computes itself and the offsets it reads
//! cells at.

use std::fmt::Write;
use std::path::Path;
use std::process::Command;

use lotml_check::check_resolved_with;
use lotml_check::ty::Ty;
use lotml_ir::lower::{self, Lowered};
use lotml_syntax::parse;

use crate::emit::{DICT_LEN, LIST_LEN, SET_USED, STR_BYTES, STR_LENGTH, STR_SIZE};
use crate::module::Module;
use crate::types::Types;

const PROGRAM: &str = "type Point(x: f64, y: i32)\ntype Shape = Circle(r: f64) | Pair(a: i8, b: str) | Dot\n\n\
fn f(t: (i8, str, bool, i32), o: int?, p: Point, s: Shape, n: (bool, (i16, f64)?), d: (u8, u16)) -> int ! str:\n    \
return 0\n\nfn main():\n    print(1)\n";

/// The C the checks compare with: a struct per type of `PROGRAM`'s `f`, its result third.
const C_TYPES: &str = "typedef struct { int8_t f0; lt_str *f1; bool f2; int32_t f3; } t_tuple;\n\
typedef struct { bool some; int64_t value; } t_optional;\n\
typedef struct { bool ok; int64_t value; lt_str *error; } t_result;\n\
typedef struct { lt_cell cell; double x; int32_t y; } t_point;\n\
typedef struct { lt_cell cell; double r; } t_circle;\n\
typedef struct { lt_cell cell; int8_t a; lt_str *b; } t_pair;\n\
typedef struct { int16_t f0; double f1; } t_inner;\n\
typedef struct { bool some; t_inner value; } t_inner_optional;\n\
typedef struct { bool f0; t_inner_optional f1; } t_nested;\n\
typedef struct { uint8_t f0; uint16_t f1; } t_small;\n";

/// One measurement: what it is, its C expression, the IR's (an LLVM type and the field, or `None`
/// for its size), and what the emitter itself computes, when it does.
struct Check {
    name: String,
    c: String,
    ir: (String, Option<usize>),
    rust: Option<u64>,
}

fn lowered(source: &str) -> Lowered {
    let parsed = parse(source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let checked = check_resolved_with(&parsed.module, source, &lotml_check::Interfaces::new());
    lower::lower(&parsed.module, &checked, source, false).unwrap_or_else(|_| panic!("{source} lowers"))
}

/// The checks of the runtime's structs, as the module's header defines them.
fn runtime_checks() -> Vec<Check> {
    let mut checks = Vec::new();
    for (c, ir, fields) in [
        ("lt_at", "%lt_at", &["file", "line", "function"][..]),
        ("lt_buf", "%lt_buf", &["data", "len", "cap"]),
        ("lt_cell", "%lt_cell", &["count", "aux"]),
        ("lt_type", "%lt_type", &["size", "inc", "dec", "eq", "cmp", "hash", "repr", "str", "share", "show"]),
        ("lt_list", "%lt_list", &["cell", "len", "cap", "type", "data"]),
        ("lt_str", "%lt_str", &["cell", "size", "length", "hash"]),
        ("lt_closure", "%lt_closure", &["cell", "fn", "drop", "share"]),
    ] {
        checks.push(Check {
            name: format!("sizeof {c}"),
            c: format!("sizeof({c})"),
            ir: (ir.into(), None),
            rust: None,
        });
        for (i, field) in fields.iter().enumerate() {
            checks.push(Check {
                name: format!("{c}.{field}"),
                c: format!("offsetof({c}, {field})"),
                ir: (ir.into(), Some(i)),
                rust: None,
            });
        }
    }
    for (c, rust) in [
        ("offsetof(lt_str, size)", STR_SIZE),
        ("offsetof(lt_str, length)", STR_LENGTH),
        ("offsetof(lt_str, bytes)", STR_BYTES),
        ("offsetof(lt_list, len)", LIST_LEN),
        ("offsetof(lt_dict, len)", DICT_LEN),
        ("offsetof(lt_set, used)", SET_USED),
    ] {
        checks.push(Check {
            name: format!("the emitter's {c}"),
            c: c.into(),
            ir: ("i8".into(), None),
            rust: Some(rust),
        });
    }
    checks
}

/// The checks of each struct and cell a program's types are, with the sizes and offsets `types`
/// computes for them.
fn program_checks(types: &Types, params: &[Ty]) -> Vec<Check> {
    let mut checks = Vec::new();
    let mut by_value = |name: &str, ty: &Ty, fields: &[&str]| {
        let ir = types.memory(ty);
        let field_types = types.fields_of(ty);
        let rust = types.layout(ty).0;
        checks.push(Check {
            name: format!("sizeof {name}"),
            c: format!("sizeof({name})"),
            ir: (ir.clone(), None),
            rust: Some(rust),
        });
        for (i, field) in fields.iter().enumerate() {
            checks.push(Check {
                name: format!("{name}.{field}"),
                c: format!("offsetof({name}, {field})"),
                ir: (ir.clone(), Some(i)),
                rust: Some(types.offset(&field_types, i)),
            });
        }
    };
    by_value("t_tuple", &params[0], &["f0", "f1", "f2", "f3"]);
    by_value("t_optional", &params[1], &["some", "value"]);
    by_value("t_result", &params[2], &["ok", "value", "error"]);
    by_value("t_nested", &params[5], &["f0", "f1"]);
    by_value("t_small", &params[6], &["f0", "f1"]);
    let Ty::Tuple(nested) = &params[5] else { panic!("a tuple") };
    by_value("t_inner_optional", &nested[1], &["some", "value"]);
    for (name, ty, variant, fields) in [
        ("t_point", &params[3], None, &["x", "y"][..]),
        ("t_circle", &params[4], Some(0), &["r"]),
        ("t_pair", &params[4], Some(1), &["a", "b"]),
    ] {
        let ir = types.cell(ty, variant);
        let field_types = types.cell_fields(ty, variant);
        let mut all = vec![Ty::Int(lotml_check::ty::IntKind::U32), Ty::Int(lotml_check::ty::IntKind::U32)];
        all.extend(field_types.iter().cloned());
        checks.push(Check {
            name: format!("sizeof {name}"),
            c: format!("sizeof({name})"),
            ir: (ir.clone(), None),
            rust: Some(types.cell_size(&field_types)),
        });
        for (i, field) in fields.iter().enumerate() {
            checks.push(Check {
                name: format!("{name}.{field}"),
                c: format!("offsetof({name}, {field})"),
                ir: (ir.clone(), Some(i + 1)),
                rust: Some(types.offset(&all, i + 2)),
            });
        }
    }
    checks
}

#[test]
fn the_emitter_lays_out_every_struct_as_c_does() {
    let clang = match crate::driver::find() {
        Ok(clang) => clang,
        Err(e) => {
            assert!(std::env::var_os("CI").is_none(), "CI has no clang: {e}");
            eprintln!("skipped: {e}");
            return;
        }
    };
    let lowered = lowered(PROGRAM);
    let f = lowered.functions.iter().find(|f| f.source_name == "f").expect("f");
    let mut params: Vec<Ty> = f.params.iter().map(|&p| f.locals[p].ty.clone()).collect();
    params.insert(2, f.ret.clone());
    let mut types = Types::new(&lowered.declared, &lowered.traits);
    params.iter().for_each(|t| types.register(t));
    let mut checks = runtime_checks();
    checks.extend(program_checks(&types, &params));

    let mut module = Module::new("layout.lot");
    types.define(&mut module);
    let mut ir = module.header();
    let mut c = format!("#include <stddef.h>\n#include <stdio.h>\n#include \"lotml.h\"\n\n{C_TYPES}\n");
    for (k, check) in checks.iter().enumerate() {
        let (ty, field) = &check.ir;
        let gep = match field {
            Some(i) => format!("getelementptr {ty}, ptr null, i32 0, i32 {i}"),
            None => format!("getelementptr {ty}, ptr null, i32 1"),
        };
        let _ =
            writeln!(ir, "define i64 @check{k}() {{\n  %p = {gep}\n  %n = ptrtoint ptr %p to i64\n  ret i64 %n\n}}");
        let _ = writeln!(c, "int64_t check{k}(void);");
    }
    c.push_str("\nint main(void) {\n");
    for (k, check) in checks.iter().enumerate() {
        let _ = writeln!(c, "    printf(\"%lld %lld\\n\", (long long)({}), (long long)check{k}());", check.c);
    }
    c.push_str("    return 0;\n}\n");

    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp/layout");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    lotml_runtime::write(&dir).unwrap();
    std::fs::write(dir.join("layout.ll"), &ir).unwrap();
    std::fs::write(dir.join("layout.c"), &c).unwrap();
    let exe = dir.join(if cfg!(windows) { "layout.exe" } else { "layout" });
    let mut command = Command::new(&clang.program);
    command.args(["-O0", "-w", "-o"]).arg(&exe).arg(dir.join("layout.ll")).arg(dir.join("layout.c"));
    command.arg(dir.join("lotml.c")).arg("-I").arg(&dir);
    if !cfg!(windows) {
        command.args(["-lm", "-pthread"]);
    }
    let built = command.output().expect("clang runs");
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    let out = Command::new(&exe).output().expect("the check runs");
    let lines: Vec<String> = String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect();
    assert_eq!(lines.len(), checks.len(), "{}", String::from_utf8_lossy(&out.stderr));
    let mut wrong = Vec::new();
    for (check, line) in checks.iter().zip(&lines) {
        let (c_value, ir_value) = line.split_once(' ').expect("two numbers");
        let (c_value, ir_value): (u64, u64) = (c_value.parse().unwrap(), ir_value.parse().unwrap());
        // A raw offset the emitter reads at is checked against C alone; the IR names no type.
        let ir_value = if check.ir.0 == "i8" { c_value } else { ir_value };
        if c_value != ir_value || check.rust.is_some_and(|r| r != c_value) {
            wrong.push(format!("{}: C {c_value}, the IR {ir_value}, the emitter {:?}", check.name, check.rust));
        }
    }
    assert!(wrong.is_empty(), "layouts that differ from C's:\n{}", wrong.join("\n"));
}
