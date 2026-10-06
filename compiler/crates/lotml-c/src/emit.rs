//! Writing C from the intermediate form: one C function per function, its locals declared at the
//! top, every statement after a `#line` naming its `.lotml` line (R2.3).

use std::fmt::Write;

use lotml_check::ty::{FloatKind, IntKind, Ty};

use crate::lower::{Lowered, function_name};
use crate::mir::{BinOp, Block, CmpOp, Const, Expr, Function, Local, Operand, Panic, StmtKind, UnOp};

pub fn program(lowered: &Lowered, file: &str) -> String {
    let file = c_string_text(file.as_bytes());
    let mut out = String::from("#include \"lotml.h\"\n#include \"lotml.c\"\n\n");
    for f in &lowered.functions {
        let _ = writeln!(out, "{};", signature(f));
    }
    out.push('\n');
    for f in &lowered.functions {
        Writer { out: &mut out, function: f, file: &file, depth: 1 }.function();
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

fn signature(f: &Function) -> String {
    let params: Vec<String> = f
        .params
        .iter()
        .filter(|&&p| !is_unit(&f.locals[p].ty))
        .map(|&p| format!("{} {}", c_type(&f.locals[p].ty), local_name(f, p)))
        .collect();
    let params = if params.is_empty() { "void".to_string() } else { params.join(", ") };
    format!("static {} {}({params})", return_type(&f.ret), f.name)
}

fn return_type(ty: &Ty) -> &'static str {
    if is_unit(ty) || matches!(ty, Ty::Never) { "void" } else { c_type(ty) }
}

fn is_unit(ty: &Ty) -> bool {
    matches!(ty, Ty::Unit)
}

/// The C type of a value of `ty`.
pub fn c_type(ty: &Ty) -> &'static str {
    match ty {
        Ty::Int(IntKind::I8) => "int8_t",
        Ty::Int(IntKind::I16) => "int16_t",
        Ty::Int(IntKind::I32) => "int32_t",
        Ty::Int(IntKind::I64) => "int64_t",
        Ty::Int(IntKind::U8) => "uint8_t",
        Ty::Int(IntKind::U16) => "uint16_t",
        Ty::Int(IntKind::U32) => "uint32_t",
        Ty::Int(IntKind::U64) => "uint64_t",
        // An `f32` is a Python float on the Python target, so it is a double here too.
        Ty::Float(_) => "double",
        Ty::Bool => "bool",
        _ => "int64_t",
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

    fn function(&mut self) {
        let f = self.function;
        self.at(f.line);
        let _ = writeln!(self.out, "{} {{", signature(f));
        let source = c_string_text(f.source_name.as_bytes());
        self.line(&format!("static const char lt_fn[] = \"{source}\";"));
        self.line("(void)lt_fn;");
        for (i, info) in f.locals.iter().enumerate() {
            if f.params.contains(&i) || is_unit(&info.ty) || matches!(info.ty, Ty::Never) {
                continue;
            }
            let decl = format!("{} {} = 0;", c_type(&info.ty), self.name(i));
            self.line(&decl);
        }
        self.block(&f.body);
        self.out.push_str("}\n\n");
    }

    fn block(&mut self, block: &Block) {
        for stmt in block {
            self.at(stmt.line);
            match &stmt.kind {
                StmtKind::Let(local, expr) => {
                    let value = self.expr(expr);
                    if is_unit(&self.function.locals[*local].ty) {
                        self.line(&format!("{value};"));
                    } else {
                        let target = self.name(*local);
                        self.line(&format!("{target} = {value};"));
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
                        self.line("return 0;");
                    } else {
                        let value = self.operand(value);
                        self.line(&format!("return {value};"));
                    }
                }
                StmtKind::ForRange { var, start, stop, step, body } => {
                    let walk = format!("r{var}");
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

    fn operand(&self, operand: &Operand) -> String {
        match operand {
            Operand::Local(local) => self.name(*local),
            Operand::Const(c) => constant(c),
        }
    }

    fn operand_type(&self, operand: &Operand) -> Ty {
        match operand {
            Operand::Local(local) => self.function.locals[*local].ty.clone(),
            Operand::Const(Const::Int(_, kind)) => Ty::Int(*kind),
            Operand::Const(Const::Float(_)) => Ty::Float(FloatKind::F64),
            Operand::Const(Const::Bool(_)) => Ty::Bool,
            Operand::Const(Const::Str(_)) => Ty::Str,
            Operand::Const(Const::Unit) => Ty::Unit,
        }
    }

    fn expr(&mut self, expr: &Expr) -> String {
        match expr {
            Expr::Use(operand) => self.operand(operand),
            Expr::Binary(op, a, b, ty) => self.binary(*op, a, b, ty),
            Expr::Unary(op, a, ty) => self.unary(*op, a, ty),
            Expr::Compare(op, a, b, ty) => {
                let symbol = match op {
                    CmpOp::Lt => "<",
                    CmpOp::Le => "<=",
                    CmpOp::Gt => ">",
                    CmpOp::Ge => ">=",
                    CmpOp::Eq => "==",
                    CmpOp::Ne => "!=",
                };
                let (a, b) = (self.operand(a), self.operand(b));
                match ty {
                    Ty::Str => format!("(lt_str_compare({a}, {b}) {symbol} 0)"),
                    _ => format!("({a} {symbol} {b})"),
                }
            }
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
            Expr::Print { args, sep, end } => self.print(args, sep.as_ref(), end.as_ref()),
        }
    }

    fn print(&mut self, args: &[(Operand, Ty)], sep: Option<&Operand>, end: Option<&Operand>) -> String {
        let mut text = String::from("do { lt_buf b = LT_BUF; ");
        for (i, (value, ty)) in args.iter().enumerate() {
            if i > 0 {
                text.push_str(&self.put_text(sep, " "));
            }
            text.push_str(&self.put_value(value, ty));
        }
        text.push_str(&self.put_text(end, "\n"));
        text.push_str("lt_write(b.data, b.len); lt_buf_free(&b); } while (0)");
        text
    }

    fn put_text(&self, text: Option<&Operand>, default: &str) -> String {
        match text {
            Some(Operand::Const(Const::Str(s))) => put_literal(s),
            _ => put_literal(default),
        }
    }

    fn put_value(&self, value: &Operand, ty: &Ty) -> String {
        if let Operand::Const(Const::Str(s)) = value {
            return put_literal(s);
        }
        let v = self.operand(value);
        match ty {
            Ty::Int(IntKind::U64) => format!("lt_buf_u64(&b, {v}); "),
            Ty::Int(_) => format!("lt_buf_i64(&b, (int64_t){v}); "),
            Ty::Float(_) => format!("lt_buf_f64(&b, {v}); "),
            Ty::Bool => format!("lt_buf_bool(&b, {v}); "),
            _ => put_literal("None"),
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
            let ty = c_type(&Ty::Int(kind));
            format!("(({ty})lt_fit({computed}, {low}LL, {high}LL, \"{name}\", LT_HERE))")
        }
        None => computed,
    }
}

fn put_literal(text: &str) -> String {
    format!("lt_buf_put(&b, \"{}\", {}); ", c_string_text(text.as_bytes()), text.len())
}

fn constant(c: &Const) -> String {
    match c {
        Const::Int(v, IntKind::U64) => format!("{v}ULL"),
        Const::Int(v, _) if *v == i128::from(i64::MIN) => "INT64_MIN".to_string(),
        Const::Int(v, _) => format!("{v}LL"),
        Const::Float(v) => float_literal(*v),
        Const::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
        Const::Unit => "0".to_string(),
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
