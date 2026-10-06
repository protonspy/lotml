//! Writing C from the intermediate form: one C function per function, its locals declared at the
//! top, every statement after a `#line` naming its `.lotml` line (R2.3); one struct and one
//! descriptor per tuple type the program uses.

use std::collections::BTreeMap;
use std::fmt::Write;

use lotml_check::ty::{FloatKind, IntKind, Ty};

use crate::lower::{Lowered, function_name, lambda_name};
use crate::mir::{
    Arg, BinOp, Block, CmpOp, Const, Expr, FormatPart, Function, Local, Operand, Panic, Place, Proj, StmtKind, UnOp,
    block_exprs, block_operands, block_types,
};
use crate::types::Types;

/// The C of the program; with `tests`, its `main` runs the `test` blocks and reports them.
pub fn program(lowered: &Lowered, file: &str, tests: bool) -> String {
    let file = c_string_text(file.as_bytes());
    let mut literals = BTreeMap::new();
    let mut types = Types::new(&lowered.declared, &lowered.traits);
    for vtable in &lowered.vtables {
        types.register(&vtable.ty);
        for slot in vtable.slots.iter().flatten() {
            slot.params.iter().for_each(|t| types.register(t));
            types.register(&slot.ret);
        }
    }
    for f in &lowered.functions {
        block_operands(&f.body, &mut |o| {
            if let Operand::Const(Const::Str(text)) = o {
                let next = literals.len();
                literals.entry(text.clone()).or_insert(next);
            }
        });
        for info in &f.locals {
            types.register(&info.ty);
        }
        types.register(&f.ret);
        block_types(&f.body, &mut |ty| types.register(ty));
    }
    let mut out = String::from("#include \"lotml.h\"\n#include \"lotml.c\"\n\n");
    let mut ordered: Vec<(&String, &usize)> = literals.iter().collect();
    ordered.sort_by_key(|(_, i)| **i);
    for (text, i) in ordered {
        let length = text.chars().count();
        let _ = writeln!(
            out,
            "LT_STR_LITERAL(lt_lit{i}, {}, {length}, \"{}\");",
            text.len(),
            c_string_text(text.as_bytes())
        );
    }
    out.push('\n');
    out.push_str(&types.definitions());
    c_functions(&mut out, lowered);
    for f in &lowered.functions {
        let _ = writeln!(out, "{};", signature(f, &types));
    }
    out.push('\n');
    vtables(&mut out, lowered, &types);
    closures(&mut out, lowered, &types);
    let tasks = task_runners(&mut out, lowered, &types);
    for f in &lowered.functions {
        Writer {
            out: &mut out,
            declared: &lowered.declared,
            function: f,
            file: &file,
            depth: 1,
            literals: &literals,
            types: &types,
            fresh: 0,
            tasks: &tasks,
        }
        .function();
    }
    out.push_str("int main(void) {\n    lt_init();\n");
    if tests {
        for (name, function) in &lowered.tests {
            let _ = writeln!(out, "    lt_run_test(\"{}\", {function});", c_string_text(name.as_bytes()));
        }
        out.push_str("    lt_test_report();\n    return lt_exit(0);\n}\n");
        return out;
    }
    let main = lowered.functions.iter().find(|f| f.name == function_name("main"));
    if let Some(Ty::Result(_, error)) = main.map(|f| &f.ret) {
        let result = types.c_type(&main.expect("main").ret);
        let _ = writeln!(out, "    {result} r = {}();", function_name("main"));
        let drop = types.count(&main.expect("main").ret, false, "r");
        let _ = writeln!(
            out,
            "    if (!r.ok) {{\n        lt_buf b = LT_BUF;\n        lt_buf_puts(&b, \"error: \");\n        ({})->repr(&b, &r.error);\n        \
             lt_flush();\n        fprintf(stderr, \"%.*s\\n\", (int)b.len, b.data);\n        lt_buf_free(&b);\n        {drop}\n        \
             return lt_exit(1);\n    }}",
            types.desc(error)
        );
        let _ = writeln!(out, "    {drop}");
        out.push_str("    return lt_exit(0);\n");
    } else if lowered.main {
        let _ = writeln!(out, "    {}();", function_name("main"));
        out.push_str("    return lt_exit(0);\n");
    } else {
        out.push_str("    fputs(\"the program has no `fn main()`\\n\", stderr);\n    return lt_exit(2);\n");
    }
    out.push_str("}\n");
    out
}

/// The C function type a closure of `ty` is called through.
fn closure_call_type(ty: &Ty, types: &Types) -> String {
    let Ty::Func(params, ret) = ty else { return "void (*)(void)".to_string() };
    let ret = if is_unit(ret) || matches!(**ret, Ty::Never) { "void".to_string() } else { types.c_type(ret) };
    let mut all = vec!["lt_closure *".to_string()];
    all.extend(params.iter().filter(|p| !is_unit(p)).map(|p| types.c_type(p)));
    format!("{ret} (*)({})", all.join(", "))
}

/// Each table of a type's methods for a trait: a function per slot taking the cell untyped and
/// calling the type's method.
fn vtables(out: &mut String, lowered: &Lowered, types: &Types) {
    for (k, vtable) in lowered.vtables.iter().enumerate() {
        let mut slots = Vec::new();
        for (i, slot) in vtable.slots.iter().enumerate() {
            let Some(slot) = slot else {
                slots.push("NULL".to_string());
                continue;
            };
            let params: Vec<&Ty> = slot.params.iter().filter(|t| !is_unit(t)).collect();
            let mut declared = vec!["void *self".to_string()];
            declared.extend(params.iter().enumerate().map(|(i, t)| format!("{} a{i}", types.c_type(t))));
            let mut passed = vec![format!("({})self", types.c_type(&vtable.ty))];
            passed.extend((0..params.len()).map(|i| format!("a{i}")));
            let ret = c_ret(&slot.ret, types);
            let call = format!("{}({})", slot.function, passed.join(", "));
            let body = if ret == "void" { format!("{call};") } else { format!("return {call};") };
            let _ = writeln!(out, "static {ret} lt_vm{k}_{i}({}) {{ {body} }}", declared.join(", "));
            slots.push(format!("(void (*)(void))lt_vm{k}_{i}"));
        }
        if slots.is_empty() {
            slots.push("NULL".to_string());
        }
        let _ = writeln!(
            out,
            "static const lt_vt_{} lt_vt{k} = {{{}, {{{}}}}};",
            vtable.trait_name,
            types.desc(&vtable.ty),
            slots.join(", ")
        );
    }
}

/// The C library functions the program calls, each declared as its interface declares it, under
/// a name of its own bound to the library's symbol: a header's declaration of the same name, with
/// C's own types, then cannot conflict with it (adr:0013).
fn c_functions(out: &mut String, lowered: &Lowered) {
    for (symbol, (params, ret)) in &lowered.c_functions {
        let params: Vec<&str> = params.iter().filter(|t| !is_unit(t)).map(ffi_type).collect();
        let params = if params.is_empty() { "void".to_string() } else { params.join(", ") };
        let declaration = format!("{} lt_ffi_{symbol}({params})", ffi_type(ret));
        let _ = writeln!(
            out,
            "#if defined(_MSC_VER) && !defined(__clang__)\n#pragma comment(linker, \"/alternatename:lt_ffi_{symbol}={symbol}\")\n\
             extern {declaration};\n#else\nextern {declaration} __asm__(LT_C_SYMBOL(\"{symbol}\"));\n#endif"
        );
    }
}

/// The C type a value of `ty` crosses into a C library as.
fn ffi_type(ty: &Ty) -> &'static str {
    match ty {
        Ty::Int(IntKind::I8) => "int8_t",
        Ty::Int(IntKind::I16) => "int16_t",
        Ty::Int(IntKind::I32) => "int32_t",
        Ty::Int(IntKind::I64) => "int64_t",
        Ty::Int(IntKind::U8) => "uint8_t",
        Ty::Int(IntKind::U16) => "uint16_t",
        Ty::Int(IntKind::U32) => "uint32_t",
        Ty::Int(IntKind::U64) => "uint64_t",
        Ty::Float(FloatKind::F32) => "float",
        Ty::Float(_) => "double",
        Ty::Bool => "bool",
        Ty::Str => "const char *",
        _ => "void",
    }
}

/// For each result type of a task of `parallel`, the function the runtime runs a task with:
/// it calls the closure and stores the result. The types, in the order the functions are numbered.
fn task_runners(out: &mut String, lowered: &Lowered, types: &Types) -> Vec<Ty> {
    let mut results: Vec<Ty> = Vec::new();
    for f in &lowered.functions {
        block_exprs(&f.body, &mut |e| {
            if let Expr::Parallel { result, .. } = e
                && !results.contains(result)
            {
                results.push(result.clone());
            }
        });
    }
    for (k, ty) in results.iter().enumerate() {
        let ret = c_ret(ty, types);
        let call = format!("(({ret} (*)(lt_closure *))task->fn)(task)");
        let body = if ret == "void" {
            format!("{call}; (void)out;")
        } else {
            format!("*({} *)out = {call};", types.c_type(ty))
        };
        let _ = writeln!(out, "static void lt_task{k}(lt_closure *task, void *out) {{ {body} }}");
    }
    results
}

/// The C result type of a function returning `ty`: `void` for a unit or one that never returns.
fn c_ret(ty: &Ty, types: &Types) -> String {
    if is_unit(ty) || matches!(ty, Ty::Never) { "void".to_string() } else { types.c_type(ty) }
}

/// Each lambda's closure struct and the function dropping what it captured; each function used
/// as a value, a static closure calling it through a function taking the closure first.
fn closures(out: &mut String, lowered: &Lowered, types: &Types) {
    for (k, (captures, _)) in lowered.lambdas.iter().enumerate() {
        let fields: String = captures.iter().enumerate().map(|(i, t)| format!(" {} c{i};", types.c_type(t))).collect();
        let _ = writeln!(
            out,
            "typedef struct lt_c{k} {{ lt_cell cell; void *fn; void (*drop)(lt_closure *self); \
             void (*share)(lt_closure *self);{fields} }} lt_c{k};"
        );
        let drops: String = captures
            .iter()
            .enumerate()
            .filter(|(_, t)| types.counted(t))
            .map(|(i, t)| format!(" ({})->dec(&c->c{i});", types.desc(t)))
            .collect();
        let _ = writeln!(
            out,
            "static void lt_drop_c{k}(lt_closure *self) {{ lt_c{k} *c = (lt_c{k} *)self; (void)c;{drops} }}"
        );
        let shares: String = captures
            .iter()
            .enumerate()
            .filter(|(_, t)| types.counted(t))
            .map(|(i, t)| format!(" ({})->share(&c->c{i});", types.desc(t)))
            .collect();
        let _ = writeln!(
            out,
            "static void lt_share_c{k}(lt_closure *self) {{ lt_c{k} *c = (lt_c{k} *)self; (void)c;{shares} }}"
        );
    }
    for name in &lowered.fn_refs {
        let Some(f) = lowered.functions.iter().find(|f| f.name == *name) else { continue };
        let params: Vec<&Local> = f.params.iter().filter(|&&p| !is_unit(&f.locals[p].ty)).collect();
        let declared: Vec<String> =
            params.iter().enumerate().map(|(i, &&p)| format!("{} a{i}", types.c_type(&f.locals[p].ty))).collect();
        let passed: Vec<String> = (0..params.len()).map(|i| format!("a{i}")).collect();
        let ret = if is_unit(&f.ret) || matches!(f.ret, Ty::Never) { "void".to_string() } else { types.c_type(&f.ret) };
        let mut all = vec!["lt_closure *self".to_string()];
        all.extend(declared);
        let call = format!("{}({})", f.name, passed.join(", "));
        let body = if ret == "void" { format!("{call};") } else { format!("return {call};") };
        let _ = writeln!(out, "static {ret} lt_tramp_{name}({}) {{ (void)self; {body} }}", all.join(", "));
        let _ = writeln!(out, "static lt_closure lt_fnref_{name} = {{{{0, 0}}, (void *)lt_tramp_{name}, NULL, NULL}};");
    }
    out.push('\n');
}

fn signature(f: &Function, types: &Types) -> String {
    let params: Vec<String> = f
        .params
        .iter()
        .filter(|&&p| !is_unit(&f.locals[p].ty))
        .map(|&p| {
            let star = if f.locals[p].by_ref { "*" } else { "" };
            format!("{} {star}{}", types.c_type(&f.locals[p].ty), local_name(f, p))
        })
        .collect();
    let params = if params.is_empty() { "void".to_string() } else { params.join(", ") };
    let ret = if is_unit(&f.ret) || matches!(f.ret, Ty::Never) { "void".to_string() } else { types.c_type(&f.ret) };
    format!("static LT_UNUSED {ret} {}({params})", f.name)
}

fn is_unit(ty: &Ty) -> bool {
    matches!(ty, Ty::Unit)
}

fn local_name(f: &Function, local: Local) -> String {
    match &f.locals[local].name {
        Some(name) => {
            let clean: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
            format!("l{local}_{clean}")
        }
        None => format!("t{local}"),
    }
}

/// The range of a narrower integer kind, and its name, for `lt_fit`.
fn range(kind: IntKind) -> Option<(i64, i64, &'static str)> {
    Some(match kind {
        IntKind::I8 => (i64::from(i8::MIN), i64::from(i8::MAX), "i8"),
        IntKind::I16 => (i64::from(i16::MIN), i64::from(i16::MAX), "i16"),
        IntKind::I32 => (i64::from(i32::MIN), i64::from(i32::MAX), "i32"),
        IntKind::U8 => (0, i64::from(u8::MAX), "u8"),
        IntKind::U16 => (0, i64::from(u16::MAX), "u16"),
        IntKind::U32 => (0, i64::from(u32::MAX), "u32"),
        IntKind::I64 | IntKind::U64 => return None,
    })
}

/// Text as the body of a C string literal: printable ASCII as is, every other byte in octal.
pub fn c_string_text(bytes: &[u8]) -> String {
    let mut out = String::new();
    for &b in bytes {
        match b {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            b'?' => out.push_str("\\?"),
            0x20..=0x7e => out.push(b as char),
            _ => {
                let _ = write!(out, "\\{b:03o}");
            }
        }
    }
    out
}

struct Writer<'a> {
    out: &'a mut String,
    declared: &'a BTreeMap<String, lotml_check::TypeDef>,
    function: &'a Function,
    file: &'a str,
    depth: usize,
    literals: &'a BTreeMap<String, usize>,
    types: &'a Types<'a>,
    /// The next number for a name the C needs: a buffer, a range.
    fresh: usize,
    /// The result type of each task runner, by its number.
    tasks: &'a [Ty],
}

impl Writer<'_> {
    fn line(&mut self, text: &str) {
        for _ in 0..self.depth {
            self.out.push_str("    ");
        }
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn at(&mut self, line: u32) {
        let _ = writeln!(self.out, "#line {line} \"{}\"", self.file);
    }

    /// The local as a C lvalue: an `inout` parameter through its pointer.
    fn name(&self, local: Local) -> String {
        let name = local_name(self.function, local);
        if self.function.locals[local].by_ref { format!("(*{name})") } else { name }
    }

    fn fresh(&mut self, prefix: &str) -> String {
        self.fresh += 1;
        format!("{prefix}{}", self.fresh)
    }

    fn c_type(&self, ty: &Ty) -> String {
        self.types.c_type(ty)
    }

    fn function(&mut self) {
        let f = self.function;
        self.at(f.line);
        let _ = writeln!(self.out, "{} {{", signature(f, self.types));
        let source = c_string_text(f.source_name.as_bytes());
        self.line(&format!("static const char lt_fn[] = \"{source}\";"));
        self.line("(void)lt_fn;");
        for (i, info) in f.locals.iter().enumerate() {
            if f.params.contains(&i) || is_unit(&info.ty) || matches!(info.ty, Ty::Never) {
                continue;
            }
            let init = if self.types.by_value(&info.ty) { "{0}" } else { "0" };
            let decl = format!("{} {} = {init};", self.c_type(&info.ty), self.name(i));
            self.line(&decl);
        }
        self.block(&f.body);
        self.out.push_str("}\n\n");
    }

    fn block(&mut self, block: &Block) {
        for stmt in block {
            self.at(stmt.line);
            match &stmt.kind {
                StmtKind::Let(local, Expr::Format(parts)) => {
                    let target = self.name(*local);
                    self.line("{");
                    self.depth += 1;
                    self.line("lt_buf b = LT_BUF;");
                    self.format_into("b", parts);
                    self.line(&format!("{target} = lt_str_from_buf(&b);"));
                    self.depth -= 1;
                    self.line("}");
                }
                StmtKind::Let(local, Expr::Closure { lambda, captures, .. }) => {
                    let target = self.name(*local);
                    let values: Vec<String> = captures.iter().map(|c| self.operand(c)).collect();
                    self.line("{");
                    self.line(&format!("    lt_c{lambda} *c = lt_alloc(sizeof(lt_c{lambda}));"));
                    self.line(&format!("    c->fn = (void *){};", lambda_name(*lambda)));
                    self.line(&format!("    c->drop = lt_drop_c{lambda};"));
                    self.line(&format!("    c->share = lt_share_c{lambda};"));
                    for (i, v) in values.iter().enumerate() {
                        self.line(&format!("    c->c{i} = {v};"));
                    }
                    self.line(&format!("    {target} = (lt_closure *)c;"));
                    self.line("}");
                }
                StmtKind::Let(local, Expr::DictNew { key, value, items }) => {
                    let target = self.name(*local);
                    let (kd, vd) = (self.types.desc(key), self.types.desc(value));
                    self.line(&format!("{target} = lt_dict_new({kd}, {vd});"));
                    for (k, v) in items {
                        let (k, v) = (self.address(k, key), self.address(v, value));
                        self.line(&format!("lt_dict_set(&{target}, {k}, {v});"));
                    }
                }
                StmtKind::Let(local, Expr::SetNew { elem, items }) => {
                    let target = self.name(*local);
                    let desc = self.types.desc(elem);
                    self.line(&format!("{target} = lt_set_new({desc});"));
                    for item in items {
                        let address = self.address(item, elem);
                        self.line(&format!("lt_set_add(&{target}, {address});"));
                    }
                }
                StmtKind::Let(local, Expr::ListNew { elem, items }) => {
                    let target = self.name(*local);
                    let desc = self.types.desc(elem);
                    self.line(&format!("{target} = lt_list_new({desc}, {});", items.len()));
                    for item in items {
                        let address = self.address(item, elem);
                        self.line(&format!("lt_list_push(&{target}, {address});"));
                    }
                }
                StmtKind::Let(local, Expr::Construct { ty, variant, fields, reuse }) => {
                    let target = self.name(*local);
                    let values: Vec<String> = fields.iter().map(|f| self.operand(f)).collect();
                    let alloc = |size: &str| match reuse {
                        Some(n) => format!("lt_reuse_or_alloc(lt_tok{n}, lt_toksz{n}, sizeof({size}))"),
                        None => format!("lt_alloc(sizeof({size}))"),
                    };
                    match variant {
                        None => {
                            let c = self.types.c_type(ty);
                            let c = c.trim_end_matches(" *").to_string();
                            self.line(&format!("{target} = {};", alloc(&c)));
                            for (i, v) in values.iter().enumerate() {
                                self.line(&format!("{target}->f{i} = {v};"));
                            }
                        }
                        Some(k) => {
                            let v = self.types.variant(ty, *k);
                            self.line("{");
                            self.line(&format!("    {v} *c = {};", alloc(&v)));
                            self.line(&format!("    c->cell.aux = {k};"));
                            for (i, value) in values.iter().enumerate() {
                                self.line(&format!("    c->f{i} = {value};"));
                            }
                            self.line(&format!("    {target} = (void *)c;"));
                            self.line("}");
                        }
                    }
                }
                StmtKind::Let(local, expr) => {
                    let value = self.expr(expr);
                    if is_unit(&self.function.locals[*local].ty) {
                        self.line(&format!("{value};"));
                    } else {
                        let target = self.name(*local);
                        self.line(&format!("{target} = {value};"));
                    }
                }
                StmtKind::Store(place, value) => {
                    let (ty, slot) = self.place(place);
                    let value = self.operand(value);
                    if self.types.counted(&ty) {
                        let c = self.c_type(&ty);
                        let drop = self.types.count(&ty, false, "lt_old");
                        self.line(&format!("{{ {c} *lt_s = {slot}; {c} lt_old = *lt_s; *lt_s = {value}; {drop} }}"));
                    } else {
                        self.line(&format!("*{slot} = {value};"));
                    }
                }
                StmtKind::DropReuse { local, token } => {
                    let ty = self.function.locals[*local].ty.clone();
                    let id = self.types.ids[&ty];
                    let name = self.name(*local);
                    self.line(&format!("size_t lt_toksz{token} = 0;"));
                    self.line(&format!("void *lt_tok{token} = lt_reuse_t{id}(&{name}, &lt_toksz{token});"));
                }
                StmtKind::Inc(local) | StmtKind::Dec(local) => {
                    let ty = self.function.locals[*local].ty.clone();
                    let name = self.name(*local);
                    let text = self.types.count(&ty, matches!(stmt.kind, StmtKind::Inc(_)), &name);
                    if !text.is_empty() {
                        self.line(&text);
                    }
                }
                StmtKind::Mutate { name, place, args, at, result } => {
                    let slot = self.slot_of_container(place);
                    let mut all = vec![slot];
                    all.extend(args.iter().map(|a| self.arg(a)));
                    if *at {
                        all.push("LT_HERE".to_string());
                    }
                    match result {
                        Some(r) => {
                            let r = self.name(*r);
                            self.line(&format!("{r} = {name}({});", all.join(", ")));
                        }
                        None => self.line(&format!("{name}({});", all.join(", "))),
                    }
                }
                StmtKind::Do(expr) => {
                    let value = self.expr(expr);
                    self.line(&format!("{value};"));
                }
                StmtKind::If(test, then, otherwise) => {
                    let test = self.operand(test);
                    self.line(&format!("if ({test}) {{"));
                    self.nested(then);
                    if otherwise.is_empty() {
                        self.line("}");
                    } else {
                        self.line("} else {");
                        self.nested(otherwise);
                        self.line("}");
                    }
                }
                StmtKind::Loop(body) => {
                    self.line("for (;;) {");
                    self.nested(body);
                    self.line("}");
                }
                StmtKind::Break => self.line("break;"),
                StmtKind::Continue => self.line("continue;"),
                StmtKind::Return(None) => self.line("return;"),
                StmtKind::Return(Some(value)) => {
                    if is_unit(&self.function.ret) {
                        self.line("return;");
                    } else if matches!(value, Operand::Const(Const::Unit)) {
                        // After a panic: never reached, but C wants a value.
                        let zero = if self.types.by_value(&self.function.ret) {
                            format!("({}){{0}}", self.c_type(&self.function.ret))
                        } else {
                            "0".to_string()
                        };
                        self.line(&format!("return {zero};"));
                    } else {
                        let value = self.operand(value);
                        self.line(&format!("return {value};"));
                    }
                }
                StmtKind::ForRange { var, start, stop, step, body, exit } => {
                    let walk = self.fresh("r");
                    let (start, stop, step) = (self.operand(start), self.operand(stop), self.operand(step));
                    self.line("{");
                    self.depth += 1;
                    self.line(&format!("lt_range {walk} = lt_range_new({start}, {stop}, {step}, LT_HERE);"));
                    let var = self.name(*var);
                    self.line("for (;;) {");
                    self.depth += 1;
                    self.line(&format!("if (!lt_range_step(&{walk}, &{var})) {{"));
                    self.nested(exit);
                    self.line("    break;");
                    self.line("}");
                    self.depth -= 1;
                    self.nested(body);
                    self.line("}");
                    self.depth -= 1;
                    self.line("}");
                }
                StmtKind::ForStr { var, over, body, exit } => {
                    let at = self.fresh("p");
                    let width = self.fresh("n");
                    let over = self.operand(over);
                    let var = self.name(*var);
                    self.line("{");
                    self.depth += 1;
                    self.line(&format!("int64_t {at} = 0;"));
                    self.line("for (;;) {");
                    self.depth += 1;
                    self.line(&format!("if ({at} >= ({over})->size) {{"));
                    self.nested(exit);
                    self.line("    break;");
                    self.line("}");
                    self.line(&format!("int {width};"));
                    self.line(&format!("lt_decode(({over})->bytes + {at}, &{width});"));
                    self.line(&format!("{var} = lt_str_new(({over})->bytes + {at}, {width});"));
                    self.line(&format!("{at} += {width};"));
                    self.depth -= 1;
                    self.nested(body);
                    self.line("}");
                    self.depth -= 1;
                    self.line("}");
                }
                StmtKind::Panic(Panic::Todo) => self.line("lt_todo(LT_HERE);"),
                StmtKind::Panic(Panic::Value(message)) => {
                    let text = c_string_text(message.as_bytes());
                    self.line(&format!("lt_value_error(LT_HERE, \"{text}\");"));
                }
                StmtKind::Panic(Panic::Assert(expression)) => {
                    let text = c_string_text(expression.as_bytes());
                    self.line(&format!("lt_assert_failed(LT_HERE, \"{text}\");"));
                }
            }
        }
    }

    fn nested(&mut self, block: &Block) {
        self.depth += 1;
        self.block(block);
        self.depth -= 1;
    }

    /// The C type of what `place` holds, and a pointer to its slot.
    fn place(&self, place: &Place) -> (Ty, String) {
        let mut ty = self.function.locals[place.local].ty.clone();
        let mut slot = format!("&{}", self.name(place.local));
        for proj in &place.proj {
            match proj {
                Proj::Index(i) => {
                    let elem = match &ty {
                        Ty::List(t) => (**t).clone(),
                        _ => Ty::Unit,
                    };
                    let i = self.operand(i);
                    slot = format!("(({} *)lt_list_slot({slot}, {i}, LT_HERE))", self.c_type(&elem));
                    ty = elem;
                }
                Proj::Field(index) => {
                    let id = self.types.ids[&ty];
                    let field = self.record_field_ty(&ty, *index);
                    slot = format!("(&lt_own_t{id}({slot})->f{index})");
                    ty = field;
                }
                Proj::Key(k) | Proj::SetDefault(k, _) => {
                    let (key, value) = match &ty {
                        Ty::Dict(k, v) => ((**k).clone(), (**v).clone()),
                        _ => (Ty::Unit, Ty::Unit),
                    };
                    let k = self.address(k, &key);
                    let c = self.c_type(&value);
                    slot = match proj {
                        Proj::SetDefault(_, d) => {
                            let d = self.address(d, &value);
                            format!("(({c} *)lt_dict_setdefault({slot}, {k}, {d}))")
                        }
                        _ => format!("(({c} *)lt_dict_slot({slot}, {k}, LT_HERE))"),
                    };
                    ty = value;
                }
            }
        }
        (ty, slot)
    }

    /// The type of the field `index` of the record type `ty`.
    fn record_field_ty(&self, ty: &Ty, index: usize) -> Ty {
        let Ty::Adt(name, args) = ty else { return Ty::Unit };
        match self.declared.get(name) {
            Some(lotml_check::TypeDef::Record { params, fields }) => {
                fields.get(index).map(|f| f.ty.substitute(params, args)).unwrap_or(Ty::Unit)
            }
            _ => Ty::Unit,
        }
    }

    /// A pointer to the slot of the container `place` names, for a runtime function changing it.
    fn slot_of_container(&self, place: &Place) -> String {
        self.place(place).1
    }

    fn operand(&self, operand: &Operand) -> String {
        match operand {
            Operand::Local(local) => self.name(*local),
            Operand::Const(Const::Str(text)) => format!("((lt_str *)&lt_lit{})", self.literals[text]),
            Operand::Const(c) => constant(c),
        }
    }

    /// A pointer to the value of `operand`, of type `ty`: the local's own address, or a
    /// compound literal holding a constant.
    fn address(&self, operand: &Operand, ty: &Ty) -> String {
        match operand {
            Operand::Local(local) => format!("&{}", self.name(*local)),
            Operand::Const(_) => format!("&({}){{{}}}", self.c_type(ty), self.operand(operand)),
        }
    }

    fn arg(&self, arg: &Arg) -> String {
        match arg {
            Arg::Value(o) => self.operand(o),
            Arg::Address(o, ty) => self.address(o, ty),
            Arg::Out(local, _) => format!("&{}", self.name(*local)),
            Arg::Desc(ty) => self.types.desc(ty),
            Arg::Offset(ty) => format!("offsetof({}, f1)", self.c_type(ty)),
            Arg::Slot(place) => self.place(place).1,
        }
    }

    fn operand_type(&self, operand: &Operand) -> Ty {
        match operand {
            Operand::Local(local) => self.function.locals[*local].ty.clone(),
            Operand::Const(Const::Int(_, kind)) => Ty::Int(*kind),
            Operand::Const(Const::Float(_)) => Ty::Float(FloatKind::F64),
            Operand::Const(Const::Bool(_)) => Ty::Bool,
            Operand::Const(Const::Str(_)) => Ty::Str,
            Operand::Const(Const::Unit | Const::Null | Const::Char(_)) => Ty::Unit,
        }
    }

    fn expr(&mut self, expr: &Expr) -> String {
        match expr {
            Expr::Use(operand) => self.operand(operand),
            Expr::Binary(op, a, b, ty) => self.binary(*op, a, b, ty),
            Expr::Unary(op, a, ty) => self.unary(*op, a, ty),
            Expr::Compare(op, a, b, ty) => self.compare(*op, a, b, ty),
            Expr::Convert(value, from, to) => self.convert(value, from, to),
            Expr::MinMax { max, a, b, ty } => {
                let (a, b) = (self.operand(a), self.operand(b));
                match (ty, max) {
                    (Ty::Float(_), false) => format!("lt_min_f64({a}, {b})"),
                    (Ty::Float(_), true) => format!("lt_max_f64({a}, {b})"),
                    (_, false) => format!("({b} < {a} ? {b} : {a})"),
                    (_, true) => format!("({b} > {a} ? {b} : {a})"),
                }
            }
            Expr::CallSlots(name, args) => {
                let args: Vec<String> = args
                    .iter()
                    .filter(|a| !matches!(a, Arg::Value(Operand::Const(Const::Unit))))
                    .map(|a| self.arg(a))
                    .collect();
                format!("{name}({})", args.join(", "))
            }
            Expr::Call(name, args) => {
                let args: Vec<String> = args
                    .iter()
                    .filter(|a| !matches!(a, Operand::Const(Const::Unit)))
                    .map(|a| self.operand(a))
                    .collect();
                format!("{name}({})", args.join(", "))
            }
            Expr::Rt { name, args, at } => {
                let mut args: Vec<String> = args.iter().map(|a| self.arg(a)).collect();
                if *at {
                    args.push("LT_HERE".to_string());
                }
                format!("{name}({})", args.join(", "))
            }
            Expr::RtValue { name, args, at, ty } => {
                let mut args: Vec<String> = args.iter().map(|a| self.arg(a)).collect();
                if *at {
                    args.push("LT_HERE".to_string());
                }
                format!("(*({} const *){name}({}))", self.c_type(ty), args.join(", "))
            }
            Expr::Contains { container, item, ty } => {
                let c = self.operand(container);
                match ty {
                    Ty::Str => format!("lt_str_contains({c}, {})", self.operand(item)),
                    Ty::List(elem) => format!("lt_list_contains({c}, {})", self.address(item, elem)),
                    Ty::Dict(k, _) => format!("lt_dict_contains({c}, {})", self.address(item, k)),
                    Ty::Set(t) => format!("lt_set_contains({c}, {})", self.address(item, t)),
                    _ => format!("false /* `in` on {ty} */"),
                }
            }
            Expr::Print { args, sep, end } => self.print(args, sep.as_ref(), end.as_ref()),
            Expr::Format(_) | Expr::ListNew { .. } | Expr::DictNew { .. } | Expr::SetNew { .. } => {
                "0 /* written by its Let */".to_string()
            }
            Expr::ReadPlace(place) => format!("(*{})", self.place(place).1),
            Expr::Closure { .. } => "0 /* written by its Let */".to_string(),
            Expr::FnRef(name) => format!("((lt_closure *)&lt_fnref_{name})"),
            Expr::CallC { symbol, args, params, .. } => {
                let mut passed = Vec::new();
                for (a, t) in args.iter().zip(params) {
                    match t {
                        Ty::Unit => {}
                        Ty::Str => passed.push(format!("(const char *)({})->bytes", self.operand(a))),
                        _ => passed.push(self.operand(a)),
                    }
                }
                format!("lt_ffi_{symbol}({})", passed.join(", "))
            }
            Expr::Parallel { tasks, result } => {
                let k = self.tasks.iter().position(|t| t == result).expect("a runner per result type");
                format!("lt_parallel({}, {}, lt_task{k}, LT_HERE)", self.operand(tasks), self.types.desc(result))
            }
            Expr::ToDyn { value, ty, vtable } => {
                format!("(({}){{(void *){}, &lt_vt{vtable}}})", self.c_type(ty), self.operand(value))
            }
            Expr::CallDyn { receiver, slot, args, params, ret, .. } => {
                let r = self.operand(receiver);
                let mut types = vec!["void *".to_string()];
                let mut passed = vec![format!("{r}.cell")];
                for (a, t) in args.iter().zip(params) {
                    if !is_unit(t) {
                        types.push(self.c_type(t));
                        passed.push(self.operand(a));
                    }
                }
                let ret = c_ret(ret, self.types);
                format!("(({ret} (*)({})){r}.v->m[{slot}])({})", types.join(", "), passed.join(", "))
            }
            Expr::Capture { closure, lambda, index } => {
                format!("((lt_c{lambda} *)({}))->c{index}", self.operand(closure))
            }
            Expr::CallClosure { callee, args, ty } => {
                let f = self.operand(callee);
                let mut all = vec![f.clone()];
                all.extend(args.iter().filter(|a| !matches!(a, Operand::Const(Const::Unit))).map(|a| self.operand(a)));
                format!("(({}){f}->fn)({})", closure_call_type(ty, self.types), all.join(", "))
            }
            Expr::ToStr(value, ty) => {
                let v = self.operand(value);
                match ty {
                    Ty::Str => v,
                    Ty::Int(IntKind::U64) => format!("lt_str_of_u64({v})"),
                    Ty::Int(_) => format!("lt_str_of_i64((int64_t){v})"),
                    Ty::Float(_) => format!("lt_str_of_f64({v})"),
                    Ty::Bool => format!("lt_str_of_bool({v})"),
                    Ty::Unit => "lt_str_none()".to_string(),
                    _ => format!("lt_str_of_value({}, {})", self.types.desc(ty), self.address(value, ty)),
                }
            }
            Expr::Len(value, ty) => {
                let v = self.operand(value);
                match ty {
                    Ty::Str => format!("{v}->length"),
                    Ty::List(_) | Ty::Heap(_) | Ty::Dict(..) => format!("{v}->len"),
                    Ty::Set(_) => format!("{v}->used"),
                    _ => format!("0 /* len of {ty} */"),
                }
            }
            Expr::ListGet { list, index, elem } => {
                let (l, i) = (self.operand(list), self.operand(index));
                format!("(({} *)({l})->data)[lt_index(({l})->len, {i}, LT_HERE)]", self.c_type(elem))
            }
            Expr::TupleNew { ty, items } => {
                let items: Vec<String> = items.iter().map(|i| self.operand(i)).collect();
                format!("(({}){{{}}})", self.c_type(ty), items.join(", "))
            }
            Expr::TupleGet { tuple, index } => format!("({}).f{index}", self.operand(tuple)),
            Expr::Construct { .. } => "0 /* written by its Let */".to_string(),
            Expr::UnitVariant { ty, variant } => {
                format!("(({}) &{})", self.c_type(ty), self.types.variant(ty, *variant))
            }
            Expr::Field { value, ty, variant, index } => {
                let v = self.operand(value);
                match variant {
                    None => format!("({v})->f{index}"),
                    Some(k) => format!("(({} *)({v}))->f{index}", self.types.variant(ty, *k)),
                }
            }
            Expr::Tag(value) => format!("((lt_cell *)({}))->aux", self.operand(value)),
            Expr::OptNew { ty, value } => match value {
                Some(v) => format!("(({}){{true, {}}})", self.c_type(ty), self.operand(v)),
                None => format!("(({}){{false}})", self.c_type(ty)),
            },
            Expr::OptIsSome(v) => format!("({}).some", self.operand(v)),
            Expr::OptIf { ty, cond, value } => {
                let t = self.c_type(ty);
                format!("({} ? ({t}){{true, {}}} : ({t}){{false}})", self.operand(cond), self.operand(value))
            }
            Expr::OptValue(v) => format!("({}).value", self.operand(v)),
            Expr::ResultNew { ty, ok, value } => {
                let v = self.operand(value);
                if *ok {
                    format!("(({}){{.ok = true, .value = {v}}})", self.c_type(ty))
                } else {
                    format!("(({}){{.ok = false, .error = {v}}})", self.c_type(ty))
                }
            }
            Expr::ResultIsOk(v) => format!("({}).ok", self.operand(v)),
            Expr::ResultValue(v) => format!("({}).value", self.operand(v)),
            Expr::ResultError(v) => format!("({}).error", self.operand(v)),
        }
    }

    fn compare(&self, op: CmpOp, a: &Operand, b: &Operand, ty: &Ty) -> String {
        let symbol = match op {
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
            CmpOp::Gt => ">",
            CmpOp::Ge => ">=",
            CmpOp::Eq => "==",
            CmpOp::Ne => "!=",
        };
        match ty {
            Ty::Str => {
                let (a, b) = (self.operand(a), self.operand(b));
                match op {
                    CmpOp::Eq => format!("lt_str_eq({a}, {b})"),
                    CmpOp::Ne => format!("(!lt_str_eq({a}, {b}))"),
                    _ => format!("(lt_str_compare({a}, {b}) {symbol} 0)"),
                }
            }
            Ty::List(_)
            | Ty::Tuple(_)
            | Ty::Adt(..)
            | Ty::Optional(_)
            | Ty::Result(..)
            | Ty::Dict(..)
            | Ty::Set(_)
            | Ty::Heap(_)
            | Ty::Dyn(_) => {
                let desc = self.types.desc(ty);
                let (a, b) = (self.address(a, ty), self.address(b, ty));
                match op {
                    CmpOp::Eq => format!("({desc})->eq({a}, {b})"),
                    CmpOp::Ne => format!("(!({desc})->eq({a}, {b}))"),
                    _ => format!("(({desc})->cmp({a}, {b}, LT_HERE) {symbol} 0)"),
                }
            }
            Ty::Unit => (if matches!(op, CmpOp::Eq) { "true" } else { "false" }).to_string(),
            _ => format!("({} {symbol} {})", self.operand(a), self.operand(b)),
        }
    }

    /// Write the parts of an f-string into the buffer named `buf`.
    fn format_into(&mut self, buf: &str, parts: &[FormatPart]) {
        for part in parts {
            match part {
                FormatPart::Text(text) => {
                    let put = put_literal_into(buf, text);
                    self.line(&put);
                }
                FormatPart::Value { value, ty, conversion, spec } => {
                    let (spec_text, spec_len, spec_buf) = self.spec(spec);
                    let v = self.operand(value);
                    match conversion {
                        Some(conversion) => {
                            let shown = self.fresh("c");
                            self.line(&format!("lt_buf {shown} = LT_BUF;"));
                            let put = if *conversion == 's' {
                                self.put_value_into(&shown, value, ty)
                            } else {
                                self.repr_into(&shown, value, ty)
                            };
                            self.line(&put);
                            if spec.is_empty() {
                                self.line(&format!("lt_buf_put(&{buf}, {shown}.data, {shown}.len);"));
                                self.line(&format!("lt_buf_free(&{shown});"));
                            } else {
                                let text = self.fresh("s");
                                self.line(&format!("lt_str *{text} = lt_str_from_buf(&{shown});"));
                                self.line(&format!("lt_format_str(&{buf}, {text}, {spec_text}, {spec_len}, LT_HERE);"));
                                self.line(&format!("lt_str_drop({text});"));
                            }
                        }
                        None => {
                            let call = match ty {
                                Ty::Int(IntKind::U64) => format!("lt_format_u64(&{buf}, {v}, "),
                                Ty::Int(_) => format!("lt_format_i64(&{buf}, (int64_t){v}, "),
                                Ty::Float(_) => format!("lt_format_f64(&{buf}, {v}, "),
                                Ty::Bool => format!("lt_format_bool(&{buf}, {v}, "),
                                Ty::Str => format!("lt_format_str(&{buf}, {v}, "),
                                Ty::Unit => format!("lt_format_none(&{buf}, "),
                                _ => format!(
                                    "lt_format_value(&{buf}, {}, {}, ",
                                    self.types.desc(ty),
                                    self.address(value, ty)
                                ),
                            };
                            self.line(&format!("{call}{spec_text}, {spec_len}, LT_HERE);"));
                        }
                    }
                    if let Some(spec_buf) = spec_buf {
                        self.line(&format!("lt_buf_free(&{spec_buf});"));
                    }
                }
            }
        }
    }

    /// A format spec as the C text of its bytes and length: a literal when it holds no value,
    /// else a buffer filled now, whose name is returned to free after use.
    fn spec(&mut self, spec: &[FormatPart]) -> (String, String, Option<String>) {
        if spec.iter().all(|p| matches!(p, FormatPart::Text(_))) {
            let text: String = spec.iter().map(|p| if let FormatPart::Text(t) = p { t.as_str() } else { "" }).collect();
            return (format!("\"{}\"", c_string_text(text.as_bytes())), format!("{}", text.len()), None);
        }
        let buf = self.fresh("f");
        self.line(&format!("lt_buf {buf} = LT_BUF;"));
        self.format_into(&buf, spec);
        (format!("({buf}.data == NULL ? \"\" : {buf}.data)"), format!("{buf}.len"), Some(buf))
    }

    fn repr_into(&self, buf: &str, value: &Operand, ty: &Ty) -> String {
        match ty {
            Ty::Str => format!("lt_buf_str_repr(&{buf}, {});", self.operand(value)),
            _ => self.put_value_into(buf, value, ty),
        }
    }

    fn put_value_into(&self, buf: &str, value: &Operand, ty: &Ty) -> String {
        if let Operand::Const(Const::Str(s)) = value {
            return put_literal_into(buf, s);
        }
        let v = self.operand(value);
        match ty {
            Ty::Int(IntKind::U64) => format!("lt_buf_u64(&{buf}, {v});"),
            Ty::Int(_) => format!("lt_buf_i64(&{buf}, (int64_t){v});"),
            Ty::Float(_) => format!("lt_buf_f64(&{buf}, {v});"),
            Ty::Bool => format!("lt_buf_bool(&{buf}, {v});"),
            Ty::Str => format!("lt_buf_str(&{buf}, {v});"),
            Ty::Unit => put_literal_into(buf, "None"),
            _ => format!("lt_buf_value(&{buf}, {}, {});", self.types.desc(ty), self.address(value, ty)),
        }
    }

    fn print(&mut self, args: &[(Operand, Ty)], sep: Option<&Operand>, end: Option<&Operand>) -> String {
        let mut text = String::from("do { lt_buf b = LT_BUF; ");
        for (i, (value, ty)) in args.iter().enumerate() {
            if i > 0 {
                text.push_str(&self.put_text(sep, " "));
            }
            text.push_str(&self.put_value_into("b", value, ty));
            text.push(' ');
        }
        text.push_str(&self.put_text(end, "\n"));
        text.push_str("lt_write(b.data, b.len); lt_buf_free(&b); } while (0)");
        text
    }

    fn put_text(&self, text: Option<&Operand>, default: &str) -> String {
        match text {
            None => format!("{} ", put_literal_into("b", default)),
            Some(Operand::Const(Const::Str(s))) => format!("{} ", put_literal_into("b", s)),
            Some(other) => format!("lt_buf_str(&b, {}); ", self.operand(other)),
        }
    }

    fn binary(&self, op: BinOp, a: &Operand, b: &Operand, ty: &Ty) -> String {
        let (x, y) = (self.operand(a), self.operand(b));
        match ty {
            Ty::Float(_) => {
                let y = if matches!(self.operand_type(b), Ty::Int(_)) { format!("(double){y}") } else { y };
                match op {
                    BinOp::Add => format!("({x} + {y})"),
                    BinOp::Sub => format!("({x} - {y})"),
                    BinOp::Mul => format!("({x} * {y})"),
                    BinOp::TrueDiv => format!("lt_truediv_f64({x}, {y}, LT_HERE)"),
                    BinOp::FloorDiv => format!("lt_floordiv_f64({x}, {y}, LT_HERE)"),
                    BinOp::Mod => format!("lt_mod_f64({x}, {y}, LT_HERE)"),
                    BinOp::Pow => format!("lt_pow_f64({x}, {y}, LT_HERE)"),
                    _ => format!("({x} + {y})"),
                }
            }
            Ty::Int(IntKind::U64) => {
                let call = |name: &str| format!("lt_{name}_u64({x}, (uint64_t){y}, LT_HERE)");
                match op {
                    BinOp::Add => call("add"),
                    BinOp::Sub => call("sub"),
                    BinOp::Mul => call("mul"),
                    BinOp::TrueDiv => call("truediv"),
                    BinOp::FloorDiv => call("floordiv"),
                    BinOp::Mod => call("mod"),
                    BinOp::Pow => call("pow"),
                    BinOp::Shl => call("shl"),
                    BinOp::Shr => call("shr"),
                    BinOp::BitAnd => format!("({x} & {y})"),
                    BinOp::BitOr => format!("({x} | {y})"),
                    BinOp::BitXor => format!("({x} ^ {y})"),
                }
            }
            Ty::Int(kind) => {
                let wide = |name: &str| format!("lt_{name}_i64((int64_t){x}, (int64_t){y}, LT_HERE)");
                let computed = match op {
                    BinOp::Add => wide("add"),
                    BinOp::Sub => wide("sub"),
                    BinOp::Mul => wide("mul"),
                    BinOp::TrueDiv => return wide("truediv"),
                    BinOp::FloorDiv => wide("floordiv"),
                    BinOp::Mod => wide("mod"),
                    BinOp::Pow => wide("pow"),
                    BinOp::Shl => wide("shl"),
                    BinOp::Shr => wide("shr"),
                    BinOp::BitAnd => return format!("({x} & {y})"),
                    BinOp::BitOr => return format!("({x} | {y})"),
                    BinOp::BitXor => return format!("({x} ^ {y})"),
                };
                fitted(computed, *kind)
            }
            _ => format!("({x} + {y})"),
        }
    }

    fn unary(&self, op: UnOp, a: &Operand, ty: &Ty) -> String {
        let x = self.operand(a);
        match (op, ty) {
            (UnOp::Not, _) => format!("(!{x})"),
            (UnOp::Neg, Ty::Float(_)) => format!("(-{x})"),
            (UnOp::Abs, Ty::Float(_)) => format!("fabs({x})"),
            (UnOp::Neg, Ty::Int(IntKind::U64)) => format!("lt_neg_u64({x}, LT_HERE)"),
            (UnOp::Abs, Ty::Int(IntKind::U64)) => x,
            (UnOp::Invert, Ty::Int(IntKind::U64)) => format!("(lt_overflow(LT_HERE, \"u64\"), (uint64_t){x})"),
            (UnOp::Neg, Ty::Int(kind)) => fitted(format!("lt_neg_i64((int64_t){x}, LT_HERE)"), *kind),
            (UnOp::Abs, Ty::Int(kind)) => fitted(format!("lt_abs_i64((int64_t){x}, LT_HERE)"), *kind),
            (UnOp::Invert, Ty::Int(kind)) => fitted(format!("(~(int64_t){x})"), *kind),
            _ => x,
        }
    }

    fn convert(&self, value: &Operand, from: &Ty, to: &Ty) -> String {
        let v = self.operand(value);
        match (from, to) {
            (_, Ty::Float(_)) => format!("((double){v})"),
            (Ty::Float(_), Ty::Int(IntKind::U64)) => format!("lt_f64_to_u64({v}, LT_HERE)"),
            (Ty::Float(_), Ty::Int(kind)) => fitted(format!("lt_f64_to_i64({v}, LT_HERE)"), *kind),
            (Ty::Int(IntKind::U64), Ty::Int(IntKind::U64)) => v,
            (Ty::Int(IntKind::U64), Ty::Int(kind)) => fitted(format!("lt_u64_to_i64({v}, LT_HERE)"), *kind),
            (_, Ty::Int(IntKind::U64)) => format!("lt_i64_to_u64((int64_t){v}, LT_HERE)"),
            (_, Ty::Int(kind)) => fitted(format!("((int64_t){v})"), *kind),
            _ => v,
        }
    }
}

/// `computed`, an i64 expression, as a value of `kind`: checked against its range when narrower.
fn fitted(computed: String, kind: IntKind) -> String {
    match range(kind) {
        Some((low, high, name)) => {
            let ty = match kind {
                IntKind::I8 => "int8_t",
                IntKind::I16 => "int16_t",
                IntKind::I32 => "int32_t",
                IntKind::U8 => "uint8_t",
                IntKind::U16 => "uint16_t",
                _ => "uint32_t",
            };
            format!("(({ty})lt_fit({computed}, {low}LL, {high}LL, \"{name}\", LT_HERE))")
        }
        None => computed,
    }
}

fn put_literal_into(buf: &str, text: &str) -> String {
    format!("lt_buf_put(&{buf}, \"{}\", {});", c_string_text(text.as_bytes()), text.len())
}

fn constant(c: &Const) -> String {
    match c {
        Const::Int(v, IntKind::U64) => format!("{v}ULL"),
        Const::Int(v, _) if *v == i128::from(i64::MIN) => "INT64_MIN".to_string(),
        Const::Int(v, _) => format!("{v}LL"),
        Const::Float(v) => float_literal(*v),
        Const::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
        Const::Unit => "0".to_string(),
        Const::Null => "NULL".to_string(),
        Const::Char(c) => format!("'{c}'"),
        Const::Str(s) => format!("\"{}\"", c_string_text(s.as_bytes())),
    }
}

/// A double as a C literal that reads back exactly.
fn float_literal(v: f64) -> String {
    if v.is_nan() {
        "NAN".to_string()
    } else if v.is_infinite() {
        if v > 0.0 { "INFINITY".to_string() } else { "(-INFINITY)".to_string() }
    } else {
        format!("{v:e}")
    }
}
