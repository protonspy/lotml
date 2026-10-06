//! Writing C from the intermediate form: one C function per function, its locals declared at the
//! top, every statement after a `#line` naming its `.lotml` line (R2.3); one struct and one
//! descriptor per tuple type the program uses.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

use lotml_check::ty::{FloatKind, IntKind, Ty};

use crate::lower::{Lowered, function_name};
use crate::mir::{
    Arg, BinOp, Block, CmpOp, Const, Expr, FormatPart, Function, Local, Operand, Panic, Place, Proj, StmtKind, UnOp,
    block_operands, block_types,
};

pub fn program(lowered: &Lowered, file: &str) -> String {
    let file = c_string_text(file.as_bytes());
    let mut literals = BTreeMap::new();
    let mut types = Types::default();
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
    for f in &lowered.functions {
        let _ = writeln!(out, "{};", signature(f, &types));
    }
    out.push('\n');
    for f in &lowered.functions {
        Writer { out: &mut out, function: f, file: &file, depth: 1, literals: &literals, types: &types, fresh: 0 }
            .function();
    }
    out.push_str("int main(void) {\n    lt_init();\n");
    if lowered.main {
        let _ = writeln!(out, "    {}();", function_name("main"));
        out.push_str("    return lt_exit(0);\n");
    } else {
        out.push_str("    fputs(\"the program has no `fn main()`\\n\", stderr);\n    return lt_exit(2);\n");
    }
    out.push_str("}\n");
    out
}

fn signature(f: &Function, types: &Types) -> String {
    let params: Vec<String> = f
        .params
        .iter()
        .filter(|&&p| !is_unit(&f.locals[p].ty))
        .map(|&p| format!("{} {}", types.c_type(&f.locals[p].ty), local_name(f, p)))
        .collect();
    let params = if params.is_empty() { "void".to_string() } else { params.join(", ") };
    let ret = if is_unit(&f.ret) || matches!(f.ret, Ty::Never) { "void".to_string() } else { types.c_type(&f.ret) };
    format!("static LT_UNUSED {ret} {}({params})", f.name)
}

fn is_unit(ty: &Ty) -> bool {
    matches!(ty, Ty::Unit)
}

/// The types the C defines for this program: one struct and one descriptor per tuple type, in
/// the order they were met, so a part is defined before what holds it.
#[derive(Default)]
struct Types {
    order: Vec<Ty>,
    ids: HashMap<Ty, usize>,
}

impl Types {
    fn register(&mut self, ty: &Ty) {
        match ty {
            Ty::List(t) | Ty::Set(t) | Ty::Heap(t) | Ty::Optional(t) => self.register(t),
            Ty::Dict(k, v) | Ty::Result(k, v) => {
                self.register(k);
                self.register(v);
            }
            Ty::Tuple(items) => {
                for t in items {
                    self.register(t);
                }
                if !self.ids.contains_key(ty) {
                    self.ids.insert(ty.clone(), self.order.len());
                    self.order.push(ty.clone());
                }
            }
            _ => {}
        }
    }

    fn c_type(&self, ty: &Ty) -> String {
        match ty {
            Ty::Int(IntKind::I8) => "int8_t".into(),
            Ty::Int(IntKind::I16) => "int16_t".into(),
            Ty::Int(IntKind::I32) => "int32_t".into(),
            Ty::Int(IntKind::I64) => "int64_t".into(),
            Ty::Int(IntKind::U8) => "uint8_t".into(),
            Ty::Int(IntKind::U16) => "uint16_t".into(),
            Ty::Int(IntKind::U32) => "uint32_t".into(),
            Ty::Int(IntKind::U64) => "uint64_t".into(),
            // An `f32` is a Python float on the Python target, so it is a double here too.
            Ty::Float(_) => "double".into(),
            Ty::Bool => "bool".into(),
            Ty::Str => "lt_str *".into(),
            Ty::List(_) => "lt_list *".into(),
            Ty::Tuple(_) => format!("lt_t{}", self.ids[ty]),
            Ty::Unit => "uint8_t".into(),
            _ => "int64_t".into(),
        }
    }

    /// The descriptor of `ty`, as a C pointer.
    fn desc(&self, ty: &Ty) -> String {
        match ty {
            Ty::Int(IntKind::I8) => "&lt_type_i8".into(),
            Ty::Int(IntKind::I16) => "&lt_type_i16".into(),
            Ty::Int(IntKind::I32) => "&lt_type_i32".into(),
            Ty::Int(IntKind::I64) => "&lt_type_i64".into(),
            Ty::Int(IntKind::U8) => "&lt_type_u8".into(),
            Ty::Int(IntKind::U16) => "&lt_type_u16".into(),
            Ty::Int(IntKind::U32) => "&lt_type_u32".into(),
            Ty::Int(IntKind::U64) => "&lt_type_u64".into(),
            Ty::Float(_) => "&lt_type_f64".into(),
            Ty::Bool => "&lt_type_bool".into(),
            Ty::Str => "&lt_type_str".into(),
            Ty::List(_) => "&lt_type_list".into(),
            Ty::Tuple(_) => format!("&lt_type_t{}", self.ids[ty]),
            _ => "&lt_type_none".into(),
        }
    }

    /// Whether a value of `ty` holds a count.
    fn counted(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Str | Ty::List(_) => true,
            Ty::Tuple(items) => items.iter().any(|t| self.counted(t)),
            _ => false,
        }
    }

    /// The structs, functions and descriptors of the tuple types.
    fn definitions(&self) -> String {
        let mut out = String::new();
        for (id, ty) in self.order.iter().enumerate() {
            let Ty::Tuple(items) = ty else { continue };
            let fields: Vec<String> =
                items.iter().enumerate().map(|(i, t)| format!("{} f{i};", self.c_type(t))).collect();
            let _ = writeln!(out, "typedef struct lt_t{id} {{ {} }} lt_t{id};", fields.join(" "));
        }
        for id in 0..self.order.len() {
            let _ = writeln!(
                out,
                "static void lt_inc_t{id}(void *p);\nstatic void lt_dec_t{id}(void *p);\n\
                 static bool lt_eq_t{id}(const void *a, const void *b);\n\
                 static int lt_cmp_t{id}(const void *a, const void *b, lt_at at);\n\
                 static int64_t lt_hash_t{id}(const void *p);\nstatic void lt_repr_t{id}(lt_buf *b, const void *p);\n\
                 static void lt_share_t{id}(void *p);"
            );
        }
        for id in 0..self.order.len() {
            let _ = writeln!(
                out,
                "static const lt_type lt_type_t{id} LT_UNUSED = {{sizeof(lt_t{id}), lt_inc_t{id}, lt_dec_t{id}, lt_eq_t{id}, \
                 lt_cmp_t{id}, lt_hash_t{id}, lt_repr_t{id}, lt_repr_t{id}, lt_share_t{id}}};"
            );
        }
        for (id, ty) in self.order.iter().enumerate() {
            let Ty::Tuple(items) = ty else { continue };
            let each = |call: &str| -> String {
                items
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| self.counted(t))
                    .map(|(i, t)| format!(" ({})->{call}(&v->f{i});", self.desc(t)))
                    .collect()
            };
            let _ = writeln!(out, "static void lt_inc_t{id}(void *p) {{ lt_t{id} *v = p; (void)v;{} }}", each("inc"));
            let _ = writeln!(out, "static void lt_dec_t{id}(void *p) {{ lt_t{id} *v = p; (void)v;{} }}", each("dec"));
            let _ =
                writeln!(out, "static void lt_share_t{id}(void *p) {{ lt_t{id} *v = p; (void)v;{} }}", each("share"));
            let eq: Vec<String> =
                items.iter().enumerate().map(|(i, t)| format!("({})->eq(&x->f{i}, &y->f{i})", self.desc(t))).collect();
            let eq = if eq.is_empty() { "true".to_string() } else { eq.join(" && ") };
            let _ = writeln!(
                out,
                "static bool lt_eq_t{id}(const void *a, const void *b) {{ const lt_t{id} *x = a, *y = b; (void)x; (void)y; return {eq}; }}"
            );
            let cmp: String = items
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let d = self.desc(t);
                    format!(" if (!({d})->eq(&x->f{i}, &y->f{i})) return ({d})->cmp(&x->f{i}, &y->f{i}, at);")
                })
                .collect();
            let _ = writeln!(
                out,
                "static int lt_cmp_t{id}(const void *a, const void *b, lt_at at) {{ const lt_t{id} *x = a, *y = b; (void)x; (void)y; (void)at;{cmp} return 0; }}"
            );
            let lanes: String = items
                .iter()
                .enumerate()
                .map(|(i, t)| format!(" lanes[{i}] = lt_hash_part({}, &v->f{i});", self.desc(t)))
                .collect();
            let n = items.len();
            let _ = writeln!(
                out,
                "static int64_t lt_hash_t{id}(const void *p) {{ const lt_t{id} *v = p; int64_t lanes[{}]; (void)v;{lanes} return lt_hash_tuple(lanes, {n}); }}",
                n.max(1)
            );
            let mut repr = String::from(" lt_buf_put(b, \"(\", 1);");
            for (i, t) in items.iter().enumerate() {
                if i > 0 {
                    repr.push_str(" lt_buf_put(b, \", \", 2);");
                }
                let _ = write!(repr, " ({})->repr(b, &v->f{i});", self.desc(t));
            }
            if n == 1 {
                repr.push_str(" lt_buf_put(b, \",\", 1);");
            }
            repr.push_str(" lt_buf_put(b, \")\", 1);");
            let _ =
                writeln!(out, "static void lt_repr_t{id}(lt_buf *b, const void *p) {{ const lt_t{id} *v = p;{repr} }}");
        }
        out.push('\n');
        out
    }
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
    function: &'a Function,
    file: &'a str,
    depth: usize,
    literals: &'a BTreeMap<String, usize>,
    types: &'a Types,
    /// The next number for a name the C needs: a buffer, a range.
    fresh: usize,
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

    fn name(&self, local: Local) -> String {
        local_name(self.function, local)
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
            let init = if matches!(info.ty, Ty::Tuple(_)) { "{0}" } else { "0" };
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
                StmtKind::Let(local, Expr::ListNew { elem, items }) => {
                    let target = self.name(*local);
                    let desc = self.types.desc(elem);
                    self.line(&format!("{target} = lt_list_new({desc}, {});", items.len()));
                    for item in items {
                        let address = self.address(item, elem);
                        self.line(&format!("lt_list_push(&{target}, {address});"));
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
                    let slot = self.slot(place);
                    let value = self.operand(value);
                    self.line(&format!("*{slot} = {value};"));
                }
                StmtKind::Mutate { name, place, args, at } => {
                    let slot = self.slot_of_container(place);
                    let mut all = vec![slot];
                    all.extend(args.iter().map(|a| self.arg(a)));
                    if *at {
                        all.push("LT_HERE".to_string());
                    }
                    self.line(&format!("{name}({});", all.join(", ")));
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
                        let zero = if matches!(self.function.ret, Ty::Tuple(_)) {
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
                StmtKind::ForRange { var, start, stop, step, body } => {
                    let walk = self.fresh("r");
                    let (start, stop, step) = (self.operand(start), self.operand(stop), self.operand(step));
                    self.line("{");
                    self.depth += 1;
                    self.line(&format!("lt_range {walk} = lt_range_new({start}, {stop}, {step}, LT_HERE);"));
                    let var = self.name(*var);
                    self.line(&format!("while (lt_range_step(&{walk}, &{var})) {{"));
                    self.nested(body);
                    self.line("}");
                    self.depth -= 1;
                    self.line("}");
                }
                StmtKind::ForStr { var, over, body } => {
                    let at = self.fresh("p");
                    let width = self.fresh("n");
                    let over = self.operand(over);
                    let var = self.name(*var);
                    self.line("{");
                    self.depth += 1;
                    self.line(&format!("int64_t {at} = 0;"));
                    self.line(&format!("while ({at} < ({over})->size) {{"));
                    self.depth += 1;
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
            let Proj::Index(i) = proj;
            let elem = match &ty {
                Ty::List(t) => (**t).clone(),
                _ => Ty::Unit,
            };
            let i = self.operand(i);
            slot = format!("(({} *)lt_list_slot({slot}, {i}, LT_HERE))", self.c_type(&elem));
            ty = elem;
        }
        (ty, slot)
    }

    /// A pointer to the slot `place` names, for a store.
    fn slot(&self, place: &Place) -> String {
        self.place(place).1
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
                format!("(*(const {} *){name}({}))", self.c_type(ty), args.join(", "))
            }
            Expr::Contains { container, item, ty } => {
                let c = self.operand(container);
                match ty {
                    Ty::Str => format!("lt_str_contains({c}, {})", self.operand(item)),
                    Ty::List(elem) => format!("lt_list_contains({c}, {})", self.address(item, elem)),
                    _ => format!("false /* `in` on {ty} */"),
                }
            }
            Expr::Print { args, sep, end } => self.print(args, sep.as_ref(), end.as_ref()),
            Expr::Format(_) | Expr::ListNew { .. } => "0 /* written by its Let */".to_string(),
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
                    Ty::List(_) => format!("{v}->len"),
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
            Ty::List(_) | Ty::Tuple(_) => {
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
