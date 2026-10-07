//! Writing LLVM IR from the counted IR (specs/llvm-backend/design.md): one `define` per function,
//! its locals as `alloca`s in the entry block that `-O2`'s `mem2reg` turns into registers, each
//! structured statement walked once into basic blocks. Numbers, `bool`, control flow, calls and
//! `print` are written here; any other construct is refused at its statement (R2.6).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use lotml_check::ty::{IntKind, Ty};
use lotml_diag::Diagnostic;
use lotml_ir::ir::{Arg, BinOp, Block, Builtin, CmpOp, Const, Expr, Function, Local, Operand, Panic, StmtKind, UnOp};
use lotml_ir::lower::Lowered;
use lotml_ir::symbol;
use lotml_syntax::span::Span;

/// The LLVM IR of the program, or a diagnostic at each construct this backend does not compile.
pub fn program(lowered: &Lowered, file: &str) -> Result<String, Vec<Diagnostic>> {
    let mut module = Module::new(file);
    let mut refused = Vec::new();
    let mut bodies = String::new();
    for f in &lowered.functions {
        match Writer::new(lowered, f, &mut module).function() {
            Ok(text) => bodies.push_str(&text),
            Err((span, what)) => refused.push(Diagnostic::error(
                "E0402",
                span,
                format!("`--target llvm` does not compile {what} yet: build with `--target python` or `--target c`"),
            )),
        }
    }
    let main = lowered.functions.iter().find(|f| f.name == symbol::function("main"));
    if let Some(f) = main
        && !matches!(f.ret, Ty::Unit | Ty::Never)
    {
        refused.push(Diagnostic::error(
            "E0402",
            f.span,
            "`--target llvm` does not compile a `main` that returns a value yet: build with `--target python` or \
             `--target c`"
                .to_string(),
        ));
    }
    if !refused.is_empty() {
        return Err(refused);
    }
    if main.is_none() {
        module.declare("declare i32 @lt_no_main()");
    }
    let mut out = module.header();
    out.push_str(&bodies);
    out.push_str("define i32 @main() {\nentry:\n  call void @lt_init()\n");
    if main.is_some() {
        let _ = writeln!(out, "  call void @{}()", symbol::function("main"));
        out.push_str("  %status = call i32 @lt_exit(i32 0)\n  ret i32 %status\n}\n");
    } else {
        out.push_str("  %status = call i32 @lt_no_main()\n  ret i32 %status\n}\n");
    }
    Ok(out)
}

/// What a construct the backend does not compile is: its span and what to call it.
type Refusal = (Span, String);

/// The module's constants and the runtime functions it calls, gathered while functions are written.
struct Module {
    file: String,
    /// The text of each string the IR holds as bytes, by its global's name.
    texts: BTreeMap<Vec<u8>, String>,
    /// Each string literal as a runtime `lt_str` cell, by its text.
    literals: BTreeMap<String, String>,
    /// Each place a program can stop, by line and function: its global's name.
    sites: BTreeMap<(u32, String), String>,
    /// The runtime functions and intrinsics called, as their declarations.
    declared: BTreeSet<String>,
    /// The runtime's type descriptors named, by symbol.
    descriptors: BTreeSet<&'static str>,
}

impl Module {
    fn new(file: &str) -> Module {
        let mut module = Module {
            file: String::new(),
            texts: BTreeMap::new(),
            literals: BTreeMap::new(),
            sites: BTreeMap::new(),
            declared: BTreeSet::new(),
            descriptors: BTreeSet::new(),
        };
        let mut name = file.as_bytes().to_vec();
        name.push(0);
        module.file = module.text(&name);
        for d in ["declare void @lt_init()", "declare i32 @lt_exit(i32)"] {
            module.declared.insert(d.to_string());
        }
        module
    }

    /// The global holding `bytes`, as they are: a NUL is the caller's to add.
    fn text(&mut self, bytes: &[u8]) -> String {
        let next = format!("@lt_text.{}", self.texts.len());
        self.texts.entry(bytes.to_vec()).or_insert(next).clone()
    }

    /// The global `lt_str` cell of the literal `text`.
    fn literal(&mut self, text: &str) -> String {
        let next = format!("@lt_lit.{}", self.literals.len());
        self.literals.entry(text.to_string()).or_insert(next).clone()
    }

    /// The global `lt_at` of `line` in the function whose LotML name is `function`.
    fn site(&mut self, line: u32, function: &str) -> String {
        let next = format!("@lt_site.{}", self.sites.len());
        self.sites.entry((line, function.to_string())).or_insert(next).clone()
    }

    fn declare(&mut self, declaration: &str) {
        self.declared.insert(declaration.to_string());
    }

    /// The module's head: its types, constants and declarations.
    fn header(&mut self) -> String {
        let mut functions: Vec<(u32, String)> = Vec::new();
        for (line, function) in self.sites.keys() {
            functions.push((*line, function.clone()));
        }
        let mut fn_names = BTreeMap::new();
        for (_, function) in &functions {
            if !fn_names.contains_key(function) {
                let mut bytes = function.as_bytes().to_vec();
                bytes.push(0);
                let global = self.text(&bytes);
                fn_names.insert(function.clone(), global);
            }
        }
        let mut out = String::from("; The LLVM IR of a LotML program, written by `lotml build --target llvm`.\n\n");
        out.push_str("%lt_at = type { ptr, i32, ptr }\n%lt_buf = type { ptr, i64, i64 }\n\n");
        for (bytes, name) in &self.texts {
            let _ =
                writeln!(out, "{name} = private unnamed_addr constant [{} x i8] c\"{}\"", bytes.len(), ir_bytes(bytes));
        }
        for (text, name) in &self.literals {
            let size = text.len();
            let _ = writeln!(
                out,
                "{name} = private global {{ i32, i32, i64, i64, i64, [{} x i8] }} {{ i32 0, i32 0, i64 {size}, i64 {}, i64 0, [{} x i8] c\"{}\\00\" }}, align 8",
                size + 1,
                text.chars().count(),
                size + 1,
                ir_bytes(text.as_bytes())
            );
        }
        for ((line, function), name) in &self.sites {
            let _ = writeln!(
                out,
                "{name} = private unnamed_addr constant %lt_at {{ ptr {}, i32 {line}, ptr {} }}",
                self.file, fn_names[function]
            );
        }
        for d in &self.descriptors {
            let _ = writeln!(out, "@{d} = external constant i8");
        }
        out.push('\n');
        for d in &self.declared {
            out.push_str(d);
            out.push('\n');
        }
        out.push('\n');
        out
    }
}

/// Bytes as the body of an LLVM `c"…"` constant: printable ASCII as is, every other byte as `\XX`.
fn ir_bytes(bytes: &[u8]) -> String {
    let mut out = String::new();
    for &b in bytes {
        if (0x20..=0x7e).contains(&b) && b != b'"' && b != b'\\' {
            out.push(b as char);
        } else {
            let _ = write!(out, "\\{b:02X}");
        }
    }
    out
}

/// The width of an integer kind, and whether it is signed.
fn int_kind(kind: IntKind) -> (u32, bool) {
    match kind {
        IntKind::I8 => (8, true),
        IntKind::I16 => (16, true),
        IntKind::I32 => (32, true),
        IntKind::I64 => (64, true),
        IntKind::U8 => (8, false),
        IntKind::U16 => (16, false),
        IntKind::U32 => (32, false),
        IntKind::U64 => (64, false),
    }
}

/// The range of a kind narrower than 64 bits, and its name, as `lt_fit` checks it in the C target.
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

/// The LLVM type of a value of `ty` in a register, when this backend compiles values of `ty`:
/// every float is a `double`, as the C target and the Python target keep it.
fn value_ty(ty: &Ty) -> Option<String> {
    match ty {
        Ty::Int(kind) => Some(format!("i{}", int_kind(*kind).0)),
        Ty::Float(_) => Some("double".into()),
        Ty::Bool => Some("i1".into()),
        _ => None,
    }
}

/// The LLVM type a value of `ty` is kept in memory as: a `bool` is a byte, as C keeps it.
fn memory_ty(ty: &Ty) -> Option<String> {
    match ty {
        Ty::Bool => Some("i8".into()),
        other => value_ty(other),
    }
}

/// The name of the runtime's descriptor of a scalar type.
fn descriptor(ty: &Ty) -> Option<&'static str> {
    Some(match ty {
        Ty::Int(IntKind::I8) => "lt_type_i8",
        Ty::Int(IntKind::I16) => "lt_type_i16",
        Ty::Int(IntKind::I32) => "lt_type_i32",
        Ty::Int(IntKind::I64) => "lt_type_i64",
        Ty::Int(IntKind::U8) => "lt_type_u8",
        Ty::Int(IntKind::U16) => "lt_type_u16",
        Ty::Int(IntKind::U32) => "lt_type_u32",
        Ty::Int(IntKind::U64) => "lt_type_u64",
        Ty::Float(_) => "lt_type_f64",
        Ty::Bool => "lt_type_bool",
        Ty::Unit => "lt_type_none",
        _ => return None,
    })
}

fn is_unit(ty: &Ty) -> bool {
    matches!(ty, Ty::Unit | Ty::Never)
}

/// What the function `f` returns in LLVM: `void` for a unit or one that never returns.
fn ret_ty(ty: &Ty) -> Option<String> {
    if is_unit(ty) { Some("void".into()) } else { value_ty(ty) }
}

/// A value in a register: its text and its LotML type.
#[derive(Clone)]
struct Value {
    text: String,
    ty: Ty,
}

/// The targets a `continue` and a `break` jump to, for the loop being written.
struct Loop {
    next: String,
    out: String,
}

struct Writer<'a> {
    lowered: &'a Lowered,
    f: &'a Function,
    module: &'a mut Module,
    allocas: Vec<String>,
    lines: Vec<String>,
    fresh: usize,
    /// Whether the block being written has ended in a terminator.
    ended: bool,
    loops: Vec<Loop>,
    /// The span of the statement being written: where a refusal and a panic point.
    span: Span,
}

impl<'a> Writer<'a> {
    fn new(lowered: &'a Lowered, f: &'a Function, module: &'a mut Module) -> Writer<'a> {
        Writer {
            lowered,
            f,
            module,
            allocas: Vec::new(),
            lines: Vec::new(),
            fresh: 0,
            ended: false,
            loops: Vec::new(),
            span: f.span,
        }
    }

    fn refuse<T>(&self, what: impl Into<String>) -> Result<T, Refusal> {
        Err((self.span, what.into()))
    }

    fn name(&mut self, prefix: &str) -> String {
        self.fresh += 1;
        format!("{prefix}{}", self.fresh)
    }

    fn emit(&mut self, line: impl Into<String>) {
        if self.ended {
            let dead = self.name("dead");
            self.lines.push(format!("{dead}:"));
            self.ended = false;
        }
        self.lines.push(format!("  {}", line.into()));
    }

    /// `text`, an instruction producing a value, into a fresh register.
    fn value(&mut self, text: impl AsRef<str>) -> String {
        let r = self.name("%v");
        self.emit(format!("{r} = {}", text.as_ref()));
        r
    }

    fn label(&mut self, label: &str) {
        if !self.ended {
            self.lines.push(format!("  br label %{label}"));
        }
        self.lines.push(format!("{label}:"));
        self.ended = false;
    }

    fn terminate(&mut self, line: impl Into<String>) {
        self.emit(line);
        self.ended = true;
    }

    fn function(mut self) -> Result<String, Refusal> {
        let f = self.f;
        let ret = ret_ty(&f.ret).map_or_else(|| self.refuse(format!("a function returning {}", kind(&f.ret))), Ok)?;
        let mut params = Vec::new();
        for &p in &f.params {
            let info = &f.locals[p];
            if info.by_ref {
                return self.refuse("an `inout` parameter");
            }
            if is_unit(&info.ty) {
                continue;
            }
            let ty = value_ty(&info.ty)
                .map_or_else(|| self.refuse(format!("a parameter that is {}", kind(&info.ty))), Ok)?;
            params.push(format!("{ty} %p{p}"));
        }
        for (i, info) in f.locals.iter().enumerate() {
            if is_unit(&info.ty) {
                continue;
            }
            let Some(ty) = memory_ty(&info.ty) else { continue };
            let zero = if ty == "double" { "0.0" } else { "0" };
            self.allocas.push(format!("  %l{i} = alloca {ty}"));
            self.allocas.push(format!("  store {ty} {zero}, ptr %l{i}"));
        }
        for &p in &f.params {
            if is_unit(&f.locals[p].ty) {
                continue;
            }
            let ty = f.locals[p].ty.clone();
            self.store(p, Value { text: format!("%p{p}"), ty })?;
        }
        self.block(&f.body)?;
        if !self.ended {
            if ret == "void" {
                self.terminate("ret void");
            } else {
                self.terminate("unreachable");
            }
        }
        let mut out = format!("define internal {ret} @{}({}) {{\nentry:\n", f.name, params.join(", "));
        for line in self.allocas.iter().chain(&self.lines) {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str("}\n\n");
        Ok(out)
    }

    /// The local `l`, read into a register.
    fn load(&mut self, l: Local) -> Result<Value, Refusal> {
        let ty = self.f.locals[l].ty.clone();
        if is_unit(&ty) {
            return Ok(Value { text: String::new(), ty });
        }
        let memory = memory_ty(&ty).map_or_else(|| self.refuse(kind(&ty)), Ok)?;
        let loaded = self.value(format!("load {memory}, ptr %l{l}"));
        if matches!(ty, Ty::Bool) {
            let bit = self.value(format!("trunc i8 {loaded} to i1"));
            return Ok(Value { text: bit, ty });
        }
        Ok(Value { text: loaded, ty })
    }

    /// `v` into the local `l`, converted to the local's type as C converts on assignment.
    fn store(&mut self, l: Local, v: Value) -> Result<(), Refusal> {
        let ty = self.f.locals[l].ty.clone();
        if is_unit(&ty) {
            return Ok(());
        }
        let v = self.convert(v, &ty)?;
        let memory = memory_ty(&ty).map_or_else(|| self.refuse(kind(&ty)), Ok)?;
        let text = if matches!(ty, Ty::Bool) { self.value(format!("zext i1 {} to i8", v.text)) } else { v.text };
        self.emit(format!("store {memory} {text}, ptr %l{l}"));
        Ok(())
    }

    fn operand(&mut self, o: &Operand) -> Result<Value, Refusal> {
        match o {
            Operand::Local(l) => self.load(*l),
            Operand::Const(Const::Int(v, kind)) => {
                let (width, _) = int_kind(*kind);
                // The bits of the value, written signed: how LLVM reads an integer of that width.
                let bits = (*v as i64) << (64 - width) >> (64 - width);
                Ok(Value { text: bits.to_string(), ty: Ty::Int(*kind) })
            }
            Operand::Const(Const::Float(v)) => {
                Ok(Value { text: format!("0x{:016X}", v.to_bits()), ty: Ty::Float(lotml_check::ty::FloatKind::F64) })
            }
            Operand::Const(Const::Bool(b)) => Ok(Value { text: b.to_string(), ty: Ty::Bool }),
            Operand::Const(Const::Unit) => Ok(Value { text: String::new(), ty: Ty::Unit }),
            Operand::Const(Const::Str(_)) => self.refuse("a `str` value"),
            Operand::Const(other) => self.refuse(format!("the constant {other:?} here")),
        }
    }

    /// `v` as a value of `to`, as C converts it on assignment and in a call.
    fn convert(&mut self, v: Value, to: &Ty) -> Result<Value, Refusal> {
        let out = |text: String| Value { text, ty: to.clone() };
        Ok(match (&v.ty, to) {
            (a, b) if a == b => v,
            (Ty::Float(_), Ty::Float(_)) => out(v.text),
            (Ty::Never, _) | (_, Ty::Unit) => out(v.text),
            (Ty::Int(a), Ty::Int(b)) => {
                let ((wa, signed), (wb, _)) = (int_kind(*a), int_kind(*b));
                if wa == wb {
                    out(v.text)
                } else if wb > wa {
                    let ext = if signed { "sext" } else { "zext" };
                    out(self.value(format!("{ext} i{wa} {} to i{wb}", v.text)))
                } else {
                    out(self.value(format!("trunc i{wa} {} to i{wb}", v.text)))
                }
            }
            (Ty::Int(a), Ty::Float(_)) => {
                let (wa, signed) = int_kind(*a);
                let op = if signed { "sitofp" } else { "uitofp" };
                out(self.value(format!("{op} i{wa} {} to double", v.text)))
            }
            (Ty::Bool, Ty::Int(b)) => out(self.value(format!("zext i1 {} to i{}", v.text, int_kind(*b).0))),
            (Ty::Bool, Ty::Float(_)) => out(self.value(format!("uitofp i1 {} to double", v.text))),
            (Ty::Int(a), Ty::Bool) => out(self.value(format!("icmp ne i{} {}, 0", int_kind(*a).0, v.text))),
            (from, to) => return self.refuse(format!("a {from} used as a {to}")),
        })
    }

    /// The global `lt_at` of the statement being written.
    fn site(&mut self) -> String {
        let line = self.lowered.line(self.span);
        self.module.site(line, &self.f.source_name)
    }

    /// A branch to a block that stops the program with `call` when `failing` holds.
    fn check(&mut self, failing: &str, call: String) {
        let (bad, good) = (self.name("stop"), self.name("ok"));
        self.terminate(format!("br i1 {failing}, label %{bad}, label %{good}"));
        self.lines.push(format!("{bad}:"));
        self.ended = false;
        self.emit(call);
        self.terminate("unreachable");
        self.lines.push(format!("{good}:"));
        self.ended = false;
    }

    fn overflow(&mut self, failing: &str, type_name: &str) {
        let site = self.site();
        let mut name = type_name.as_bytes().to_vec();
        name.push(0);
        let name = self.module.text(&name);
        self.module.declare("declare void @lt_overflow(ptr, ptr) noreturn");
        self.check(failing, format!("call void @lt_overflow(ptr {site}, ptr {name})"));
    }

    fn zero_division(&mut self, failing: &str, message: &str) {
        let site = self.site();
        let mut text = message.as_bytes().to_vec();
        text.push(0);
        let text = self.module.text(&text);
        self.module.declare("declare void @lt_zero_division(ptr, ptr) noreturn");
        self.check(failing, format!("call void @lt_zero_division(ptr {site}, ptr {text})"));
    }

    /// `a op b` on i64 or u64 with LLVM's checked intrinsic, stopping on overflow.
    fn checked(&mut self, op: &str, a: &str, b: &str, type_name: &str) -> String {
        let intrinsic = format!("llvm.{op}.with.overflow.i64");
        self.module.declare(&format!("declare {{ i64, i1 }} @{intrinsic}(i64, i64)"));
        let pair = self.value(format!("call {{ i64, i1 }} @{intrinsic}(i64 {a}, i64 {b})"));
        let flag = self.value(format!("extractvalue {{ i64, i1 }} {pair}, 1"));
        self.overflow(&flag, type_name);
        self.value(format!("extractvalue {{ i64, i1 }} {pair}, 0"))
    }

    /// An i64 computed for a narrower `kind`, checked against its range and truncated to it.
    fn fit(&mut self, v: String, kind: IntKind) -> Value {
        let Some((low, high, name)) = range(kind) else { return Value { text: v, ty: Ty::Int(kind) } };
        let below = self.value(format!("icmp slt i64 {v}, {low}"));
        let above = self.value(format!("icmp sgt i64 {v}, {high}"));
        let outside = self.value(format!("or i1 {below}, {above}"));
        self.overflow(&outside, name);
        let width = int_kind(kind).0;
        let text = self.value(format!("trunc i64 {v} to i{width}"));
        Value { text, ty: Ty::Int(kind) }
    }

    /// A call of a runtime function taking two values and the place, after which nothing is checked.
    fn runtime2(&mut self, ret: &str, name: &str, a: &str, b: &str, ty: &str) -> String {
        self.module.declare(&format!("declare {ret} @{name}({ty}, {ty}, ptr)"));
        let site = self.site();
        self.value(format!("call {ret} @{name}({ty} {a}, {ty} {b}, ptr {site})"))
    }

    fn binary(&mut self, op: BinOp, a: &Operand, b: &Operand, ty: &Ty) -> Result<Value, Refusal> {
        let (x, y) = (self.operand(a)?, self.operand(b)?);
        let f64_ty = Ty::Float(lotml_check::ty::FloatKind::F64);
        match ty {
            Ty::Float(_) => {
                let (x, y) = (self.convert(x, &f64_ty)?.text, self.convert(y, &f64_ty)?.text);
                let text = match op {
                    BinOp::Add => self.value(format!("fadd double {x}, {y}")),
                    BinOp::Sub => self.value(format!("fsub double {x}, {y}")),
                    BinOp::Mul => self.value(format!("fmul double {x}, {y}")),
                    BinOp::TrueDiv => {
                        let zero = self.value(format!("fcmp oeq double {y}, 0.0"));
                        self.zero_division(&zero, "float division by zero");
                        self.value(format!("fdiv double {x}, {y}"))
                    }
                    BinOp::FloorDiv => self.runtime2("double", "lt_floordiv_f64", &x, &y, "double"),
                    BinOp::Mod => self.runtime2("double", "lt_mod_f64", &x, &y, "double"),
                    BinOp::Pow => self.runtime2("double", "lt_pow_f64", &x, &y, "double"),
                    _ => return self.refuse(format!("`{op:?}` on floats")),
                };
                Ok(Value { text, ty: f64_ty })
            }
            Ty::Int(IntKind::U64) => {
                let u64_ty = Ty::Int(IntKind::U64);
                let (x, y) = (self.convert(x, &u64_ty)?.text, self.convert(y, &u64_ty)?.text);
                let text = match op {
                    BinOp::Add => self.checked("uadd", &x, &y, "u64"),
                    BinOp::Sub => self.checked("usub", &x, &y, "u64"),
                    BinOp::Mul => self.checked("umul", &x, &y, "u64"),
                    BinOp::TrueDiv => {
                        let zero = self.value(format!("icmp eq i64 {y}, 0"));
                        self.zero_division(&zero, "division by zero");
                        let (fx, fy) = (
                            self.value(format!("uitofp i64 {x} to double")),
                            self.value(format!("uitofp i64 {y} to double")),
                        );
                        let text = self.value(format!("fdiv double {fx}, {fy}"));
                        return Ok(Value { text, ty: f64_ty });
                    }
                    BinOp::FloorDiv | BinOp::Mod => {
                        let zero = self.value(format!("icmp eq i64 {y}, 0"));
                        self.zero_division(&zero, "integer division or modulo by zero");
                        let inst = if op == BinOp::FloorDiv { "udiv" } else { "urem" };
                        self.value(format!("{inst} i64 {x}, {y}"))
                    }
                    BinOp::Pow => self.runtime2("i64", "lt_pow_u64", &x, &y, "i64"),
                    BinOp::Shl => self.runtime2("i64", "lt_shl_u64", &x, &y, "i64"),
                    BinOp::Shr => self.runtime2("i64", "lt_shr_u64", &x, &y, "i64"),
                    BinOp::BitAnd => self.value(format!("and i64 {x}, {y}")),
                    BinOp::BitOr => self.value(format!("or i64 {x}, {y}")),
                    BinOp::BitXor => self.value(format!("xor i64 {x}, {y}")),
                };
                Ok(Value { text, ty: u64_ty })
            }
            Ty::Int(kind) => {
                let kind = *kind;
                if matches!(op, BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor) {
                    let (x, y) = (self.convert(x, ty)?.text, self.convert(y, ty)?.text);
                    let inst = match op {
                        BinOp::BitAnd => "and",
                        BinOp::BitOr => "or",
                        _ => "xor",
                    };
                    let text = self.value(format!("{inst} i{} {x}, {y}", int_kind(kind).0));
                    return Ok(Value { text, ty: ty.clone() });
                }
                let i64_ty = Ty::Int(IntKind::I64);
                let (x, y) = (self.convert(x, &i64_ty)?.text, self.convert(y, &i64_ty)?.text);
                let wide = match op {
                    BinOp::Add => self.checked("sadd", &x, &y, "int"),
                    BinOp::Sub => self.checked("ssub", &x, &y, "int"),
                    BinOp::Mul => self.checked("smul", &x, &y, "int"),
                    BinOp::TrueDiv => {
                        let zero = self.value(format!("icmp eq i64 {y}, 0"));
                        self.zero_division(&zero, "division by zero");
                        let (fx, fy) = (
                            self.value(format!("sitofp i64 {x} to double")),
                            self.value(format!("sitofp i64 {y} to double")),
                        );
                        let text = self.value(format!("fdiv double {fx}, {fy}"));
                        return Ok(Value { text, ty: f64_ty });
                    }
                    BinOp::FloorDiv => self.floordiv(&x, &y),
                    BinOp::Mod => self.modulo(&x, &y),
                    BinOp::Pow => self.runtime2("i64", "lt_pow_i64", &x, &y, "i64"),
                    BinOp::Shl => self.runtime2("i64", "lt_shl_i64", &x, &y, "i64"),
                    BinOp::Shr => self.runtime2("i64", "lt_shr_i64", &x, &y, "i64"),
                    BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => unreachable!("written above"),
                };
                Ok(self.fit(wide, kind))
            }
            other => self.refuse(format!("arithmetic on {other}")),
        }
    }

    /// `x // y` on i64, rounded toward negative infinity as Python rounds it.
    fn floordiv(&mut self, x: &str, y: &str) -> String {
        let zero = self.value(format!("icmp eq i64 {y}, 0"));
        self.zero_division(&zero, "integer division or modulo by zero");
        // `y == -1` is a negation, which overflows on the smallest int; any other `y` cannot.
        let minus_one = self.value(format!("icmp eq i64 {y}, -1"));
        let smallest = self.value(format!("icmp eq i64 {x}, {}", i64::MIN));
        let overflows = self.value(format!("and i1 {minus_one}, {smallest}"));
        self.overflow(&overflows, "int");
        let safe_y = self.value(format!("select i1 {minus_one}, i64 1, i64 {y}"));
        let quotient = self.value(format!("sdiv i64 {x}, {safe_y}"));
        let negated = self.value(format!("sub i64 0, {x}"));
        let q = self.value(format!("select i1 {minus_one}, i64 {negated}, i64 {quotient}"));
        let remainder = self.value(format!("srem i64 {x}, {safe_y}"));
        let inexact = self.value(format!("icmp ne i64 {remainder}, 0"));
        let x_negative = self.value(format!("icmp slt i64 {x}, 0"));
        let y_negative = self.value(format!("icmp slt i64 {y}, 0"));
        let signs_differ = self.value(format!("xor i1 {x_negative}, {y_negative}"));
        let adjust = self.value(format!("and i1 {inexact}, {signs_differ}"));
        let down = self.value(format!("sub i64 {q}, 1"));
        self.value(format!("select i1 {adjust}, i64 {down}, i64 {q}"))
    }

    /// `x % y` on i64, with the divisor's sign as Python gives it.
    fn modulo(&mut self, x: &str, y: &str) -> String {
        let zero = self.value(format!("icmp eq i64 {y}, 0"));
        self.zero_division(&zero, "integer division or modulo by zero");
        let minus_one = self.value(format!("icmp eq i64 {y}, -1"));
        let safe_y = self.value(format!("select i1 {minus_one}, i64 1, i64 {y}"));
        let r = self.value(format!("srem i64 {x}, {safe_y}"));
        let nonzero = self.value(format!("icmp ne i64 {r}, 0"));
        let r_negative = self.value(format!("icmp slt i64 {r}, 0"));
        let y_negative = self.value(format!("icmp slt i64 {y}, 0"));
        let signs_differ = self.value(format!("xor i1 {r_negative}, {y_negative}"));
        let adjust = self.value(format!("and i1 {nonzero}, {signs_differ}"));
        let up = self.value(format!("add i64 {r}, {y}"));
        self.value(format!("select i1 {adjust}, i64 {up}, i64 {r}"))
    }

    fn unary(&mut self, op: UnOp, a: &Operand, ty: &Ty) -> Result<Value, Refusal> {
        let x = self.operand(a)?;
        let f64_ty = Ty::Float(lotml_check::ty::FloatKind::F64);
        match (op, ty) {
            (UnOp::Not, _) => {
                let x = self.convert(x, &Ty::Bool)?;
                let text = self.value(format!("xor i1 {}, true", x.text));
                Ok(Value { text, ty: Ty::Bool })
            }
            (UnOp::Neg, Ty::Float(_)) => {
                let x = self.convert(x, &f64_ty)?;
                let text = self.value(format!("fneg double {}", x.text));
                Ok(Value { text, ty: f64_ty })
            }
            (UnOp::Abs, Ty::Float(_)) => {
                let x = self.convert(x, &f64_ty)?;
                self.module.declare("declare double @llvm.fabs.f64(double)");
                let text = self.value(format!("call double @llvm.fabs.f64(double {})", x.text));
                Ok(Value { text, ty: f64_ty })
            }
            (UnOp::Neg, Ty::Int(IntKind::U64)) => {
                let x = self.convert(x, ty)?;
                let nonzero = self.value(format!("icmp ne i64 {}, 0", x.text));
                self.overflow(&nonzero, "u64");
                Ok(Value { text: "0".into(), ty: ty.clone() })
            }
            (UnOp::Abs, Ty::Int(IntKind::U64)) => self.convert(x, ty),
            (UnOp::Invert, Ty::Int(IntKind::U64)) => {
                self.convert(x, ty)?;
                self.overflow("true", "u64");
                Ok(Value { text: "0".into(), ty: ty.clone() })
            }
            (UnOp::Neg | UnOp::Abs | UnOp::Invert, Ty::Int(kind)) => {
                let x = self.convert(x, &Ty::Int(IntKind::I64))?.text;
                let wide = if op == UnOp::Invert {
                    self.value(format!("xor i64 {x}, -1"))
                } else {
                    let smallest = self.value(format!("icmp eq i64 {x}, {}", i64::MIN));
                    self.overflow(&smallest, "int");
                    let negated = self.value(format!("sub i64 0, {x}"));
                    if op == UnOp::Neg {
                        negated
                    } else {
                        let negative = self.value(format!("icmp slt i64 {x}, 0"));
                        self.value(format!("select i1 {negative}, i64 {negated}, i64 {x}"))
                    }
                };
                Ok(self.fit(wide, *kind))
            }
            _ => self.convert(x, ty),
        }
    }

    fn compare(&mut self, op: CmpOp, a: &Operand, b: &Operand, ty: &Ty) -> Result<Value, Refusal> {
        let bool_value = |text: String| Value { text, ty: Ty::Bool };
        if matches!(ty, Ty::Unit) {
            return Ok(bool_value((op == CmpOp::Eq).to_string()));
        }
        let (x, y) = (self.operand(a)?, self.operand(b)?);
        match ty {
            Ty::Float(_) => {
                let f64_ty = Ty::Float(lotml_check::ty::FloatKind::F64);
                let (x, y) = (self.convert(x, &f64_ty)?.text, self.convert(y, &f64_ty)?.text);
                let cond = match op {
                    CmpOp::Lt => "olt",
                    CmpOp::Le => "ole",
                    CmpOp::Gt => "ogt",
                    CmpOp::Ge => "oge",
                    CmpOp::Eq => "oeq",
                    CmpOp::Ne => "une",
                };
                Ok(bool_value(self.value(format!("fcmp {cond} double {x}, {y}"))))
            }
            Ty::Int(_) | Ty::Bool => {
                let (width, signed) = match ty {
                    Ty::Int(kind) => int_kind(*kind),
                    _ => (1, false),
                };
                let (x, y) = (self.convert(x, ty)?.text, self.convert(y, ty)?.text);
                let cond = match (op, signed) {
                    (CmpOp::Eq, _) => "eq",
                    (CmpOp::Ne, _) => "ne",
                    (CmpOp::Lt, true) => "slt",
                    (CmpOp::Le, true) => "sle",
                    (CmpOp::Gt, true) => "sgt",
                    (CmpOp::Ge, true) => "sge",
                    (CmpOp::Lt, false) => "ult",
                    (CmpOp::Le, false) => "ule",
                    (CmpOp::Gt, false) => "ugt",
                    (CmpOp::Ge, false) => "uge",
                };
                Ok(bool_value(self.value(format!("icmp {cond} i{width} {x}, {y}"))))
            }
            other => self.refuse(format!("comparing values of type {other}")),
        }
    }

    /// `int(x)`, `float(n)`, `i32(n)` and the rest: checked as the C target checks them.
    fn convert_expr(&mut self, value: &Operand, from: &Ty, to: &Ty) -> Result<Value, Refusal> {
        let v = self.operand(value)?;
        let f64_ty = Ty::Float(lotml_check::ty::FloatKind::F64);
        let i64_ty = Ty::Int(IntKind::I64);
        match (from, to) {
            (_, Ty::Float(_)) => self.convert(v, &f64_ty),
            (Ty::Float(_), Ty::Int(kind)) => {
                let v = self.convert(v, &f64_ty)?.text;
                let site = self.site();
                let (name, ret) = if *kind == IntKind::U64 {
                    ("lt_f64_to_u64", Ty::Int(IntKind::U64))
                } else {
                    ("lt_f64_to_i64", i64_ty)
                };
                self.module.declare(&format!("declare i64 @{name}(double, ptr)"));
                let text = self.value(format!("call i64 @{name}(double {v}, ptr {site})"));
                if *kind == IntKind::U64 { Ok(Value { text, ty: ret }) } else { Ok(self.fit(text, *kind)) }
            }
            (Ty::Int(IntKind::U64), Ty::Int(IntKind::U64)) => self.convert(v, to),
            (Ty::Int(IntKind::U64), Ty::Int(kind)) => {
                let v = self.convert(v, &Ty::Int(IntKind::U64))?.text;
                let too_big = self.value(format!("icmp ugt i64 {v}, {}", i64::MAX));
                self.overflow(&too_big, "int");
                Ok(self.fit(v, *kind))
            }
            (_, Ty::Int(IntKind::U64)) => {
                let v = self.convert(v, &i64_ty)?.text;
                let negative = self.value(format!("icmp slt i64 {v}, 0"));
                self.overflow(&negative, "u64");
                Ok(Value { text: v, ty: Ty::Int(IntKind::U64) })
            }
            (_, Ty::Int(kind)) => {
                let v = self.convert(v, &i64_ty)?.text;
                Ok(self.fit(v, *kind))
            }
            _ => self.convert(v, to),
        }
    }

    fn min_max(&mut self, max: bool, a: &Operand, b: &Operand, ty: &Ty) -> Result<Value, Refusal> {
        let (x, y) = (self.operand(a)?, self.operand(b)?);
        let (x, y) = (self.convert(x, ty)?, self.convert(y, ty)?);
        let vty = value_ty(ty).map_or_else(|| self.refuse(format!("`min` or `max` of {ty}")), Ok)?;
        // `b < a ? b : a` for `min` and `b > a ? b : a` for `max`, as the C target writes them.
        let cond = match (ty, max) {
            (Ty::Float(_), false) => "fcmp olt",
            (Ty::Float(_), true) => "fcmp ogt",
            (Ty::Int(kind), false) => {
                if int_kind(*kind).1 {
                    "icmp slt"
                } else {
                    "icmp ult"
                }
            }
            (Ty::Int(kind), true) => {
                if int_kind(*kind).1 {
                    "icmp sgt"
                } else {
                    "icmp ugt"
                }
            }
            (_, false) => "icmp ult",
            (_, true) => "icmp ugt",
        };
        let pick = self.value(format!("{cond} {vty} {}, {}", y.text, x.text));
        let text = self.value(format!("select i1 {pick}, {vty} {}, {vty} {}", y.text, x.text));
        Ok(Value { text, ty: ty.clone() })
    }

    fn call(&mut self, name: &str, args: &[Operand]) -> Result<Value, Refusal> {
        let callee = self.lowered.functions.iter().find(|f| f.name == name);
        let Some(callee) = callee else { return self.refuse(format!("a call of {name}")) };
        let params: Vec<Ty> = callee.params.iter().map(|&p| callee.locals[p].ty.clone()).collect();
        let mut passed = Vec::new();
        for (a, p) in args.iter().zip(&params) {
            if is_unit(p) {
                continue;
            }
            let v = self.operand(a)?;
            let v = self.convert(v, p)?;
            let ty = value_ty(p).map_or_else(|| self.refuse(format!("passing {}", kind(p))), Ok)?;
            passed.push(format!("{ty} {}", v.text));
        }
        let ret =
            ret_ty(&callee.ret).map_or_else(|| self.refuse(format!("a call returning {}", kind(&callee.ret))), Ok)?;
        let call = format!("call {ret} @{name}({})", passed.join(", "));
        if ret == "void" {
            self.emit(call);
            if matches!(callee.ret, Ty::Never) {
                self.terminate("unreachable");
            }
            return Ok(Value { text: String::new(), ty: callee.ret.clone() });
        }
        let text = self.value(call);
        Ok(Value { text, ty: callee.ret.clone() })
    }

    /// The value of `arg` as a runtime function takes it: its text with its LLVM type.
    fn runtime_arg(&mut self, arg: &Arg) -> Result<String, Refusal> {
        match arg {
            Arg::Value(Operand::Const(Const::Str(text))) => Ok(format!("ptr {}", self.module.literal(text))),
            Arg::Value(Operand::Const(Const::Null)) => Ok("ptr null".into()),
            Arg::Desc(ty) => {
                let d = descriptor(ty).map_or_else(|| self.refuse(format!("the descriptor of {ty}")), Ok)?;
                self.module.descriptors.insert(d);
                Ok(format!("ptr @{d}"))
            }
            Arg::Address(Operand::Local(l), _) if memory_ty(&self.f.locals[*l].ty).is_some() => {
                Ok(format!("ptr %l{l}"))
            }
            Arg::Address(o, ty) => {
                let memory = memory_ty(ty).map_or_else(|| self.refuse(format!("the address of a {ty}")), Ok)?;
                let v = self.operand(o)?;
                let v = self.convert(v, ty)?;
                let text =
                    if matches!(ty, Ty::Bool) { self.value(format!("zext i1 {} to i8", v.text)) } else { v.text };
                let slot = self.name("%a");
                self.allocas.push(format!("  {slot} = alloca {memory}"));
                self.emit(format!("store {memory} {text}, ptr {slot}"));
                Ok(format!("ptr {slot}"))
            }
            other => self.refuse(format!("the argument {other:?} of a runtime function")),
        }
    }

    /// A built-in operation: those of numbers and `bool` this increment compiles (R2.1, R2.4).
    fn builtin(&mut self, op: Builtin, args: &[Arg], at: bool) -> Result<Value, Refusal> {
        match op {
            Builtin::AssertCompared if at && args.len() == 8 => {
                let mut passed = Vec::new();
                for a in args {
                    passed.push(self.runtime_arg(a)?);
                }
                let site = self.site();
                passed.push(format!("ptr {site}"));
                self.module
                    .declare("declare void @lt_assert_compared(ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr) noreturn");
                self.emit(format!("call void @lt_assert_compared({})", passed.join(", ")));
                self.terminate("unreachable");
                Ok(Value { text: String::new(), ty: Ty::Never })
            }
            other => self.refuse(format!("`{other:?}`")),
        }
    }

    fn expr(&mut self, e: &Expr) -> Result<Value, Refusal> {
        match e {
            Expr::Use(o) => self.operand(o),
            Expr::Binary(op, a, b, ty) => self.binary(*op, a, b, ty),
            Expr::Unary(op, a, ty) => self.unary(*op, a, ty),
            Expr::Compare(op, a, b, ty) => self.compare(*op, a, b, ty),
            Expr::Convert(v, from, to) => self.convert_expr(v, from, to),
            Expr::MinMax { max, a, b, ty } => self.min_max(*max, a, b, ty),
            Expr::Call(name, args) => self.call(name, args),
            Expr::Print { args, sep, end } => {
                self.print(args, sep.as_ref(), end.as_ref())?;
                Ok(Value { text: String::new(), ty: Ty::Unit })
            }
            Expr::Rt { op, args, at } => self.builtin(*op, args, *at),
            other => self.refuse(expression_name(other)),
        }
    }

    /// Text into the buffer `b`: a literal's bytes, or a value of `ty` as `print` writes it.
    fn put(&mut self, buf: &str, value: &Operand, ty: &Ty) -> Result<(), Refusal> {
        if let Operand::Const(Const::Str(text)) = value {
            self.put_literal(buf, text);
            return Ok(());
        }
        if matches!(ty, Ty::Unit) {
            self.put_literal(buf, "None");
            return Ok(());
        }
        let v = self.operand(value)?;
        match ty {
            Ty::Int(IntKind::U64) => {
                let v = self.convert(v, ty)?;
                self.module.declare("declare void @lt_buf_u64(ptr, i64)");
                self.emit(format!("call void @lt_buf_u64(ptr {buf}, i64 {})", v.text));
            }
            Ty::Int(_) => {
                let v = self.convert(v, &Ty::Int(IntKind::I64))?;
                self.module.declare("declare void @lt_buf_i64(ptr, i64)");
                self.emit(format!("call void @lt_buf_i64(ptr {buf}, i64 {})", v.text));
            }
            Ty::Float(_) => {
                let v = self.convert(v, &Ty::Float(lotml_check::ty::FloatKind::F64))?;
                self.module.declare("declare void @lt_buf_f64(ptr, double)");
                self.emit(format!("call void @lt_buf_f64(ptr {buf}, double {})", v.text));
            }
            Ty::Bool => {
                let v = self.convert(v, &Ty::Bool)?;
                self.module.declare("declare void @lt_buf_bool(ptr, i1 zeroext)");
                self.emit(format!("call void @lt_buf_bool(ptr {buf}, i1 zeroext {})", v.text));
            }
            other => return self.refuse(format!("printing a {other}")),
        }
        Ok(())
    }

    fn put_literal(&mut self, buf: &str, text: &str) {
        if text.is_empty() {
            return;
        }
        let global = self.module.text(text.as_bytes());
        self.module.declare("declare void @lt_buf_put(ptr, ptr, i64)");
        self.emit(format!("call void @lt_buf_put(ptr {buf}, ptr {global}, i64 {})", text.len()));
    }

    fn separator(&mut self, buf: &str, given: Option<&Operand>, default: &str) -> Result<(), Refusal> {
        match given {
            None => self.put_literal(buf, default),
            Some(Operand::Const(Const::Str(text))) => self.put_literal(buf, text),
            Some(_) => return self.refuse("a `sep` or `end` that is not a literal"),
        }
        Ok(())
    }

    /// `print(args, sep=…, end=…)`: the text built in a buffer, then written in one piece.
    fn print(&mut self, args: &[(Operand, Ty)], sep: Option<&Operand>, end: Option<&Operand>) -> Result<(), Refusal> {
        let buf = self.name("%buf");
        self.allocas.push(format!("  {buf} = alloca %lt_buf"));
        self.emit(format!("store %lt_buf zeroinitializer, ptr {buf}"));
        for (i, (value, ty)) in args.iter().enumerate() {
            if i > 0 {
                self.separator(&buf, sep, " ")?;
            }
            self.put(&buf, value, ty)?;
        }
        self.separator(&buf, end, "\n")?;
        let data_at = self.value(format!("getelementptr %lt_buf, ptr {buf}, i32 0, i32 0"));
        let data = self.value(format!("load ptr, ptr {data_at}"));
        let len_at = self.value(format!("getelementptr %lt_buf, ptr {buf}, i32 0, i32 1"));
        let len = self.value(format!("load i64, ptr {len_at}"));
        self.module.declare("declare void @lt_write(ptr, i64)");
        self.module.declare("declare void @lt_buf_free(ptr)");
        self.emit(format!("call void @lt_write(ptr {data}, i64 {len})"));
        self.emit(format!("call void @lt_buf_free(ptr {buf})"));
        Ok(())
    }

    /// A panic: the runtime function that stops the program, given the place.
    fn panic(&mut self, panic: &Panic) {
        let site = self.site();
        match panic {
            Panic::Todo => {
                self.module.declare("declare void @lt_todo(ptr) noreturn");
                self.emit(format!("call void @lt_todo(ptr {site})"));
            }
            Panic::Value(message) | Panic::Assert(message) => {
                let name = if matches!(panic, Panic::Value(_)) { "lt_value_error" } else { "lt_assert_failed" };
                let mut text = message.as_bytes().to_vec();
                text.push(0);
                let text = self.module.text(&text);
                self.module.declare(&format!("declare void @{name}(ptr, ptr) noreturn"));
                self.emit(format!("call void @{name}(ptr {site}, ptr {text})"));
            }
        }
        self.terminate("unreachable");
    }

    fn block(&mut self, block: &Block) -> Result<(), Refusal> {
        for stmt in block {
            self.span = stmt.span;
            match &stmt.kind {
                StmtKind::Let(l, e) => {
                    let v = self.expr(e)?;
                    if !matches!(v.ty, Ty::Never) || !self.ended {
                        self.store(*l, v)?;
                    }
                }
                StmtKind::Do(e) => {
                    self.expr(e)?;
                }
                StmtKind::Store(place, v) if place.proj.is_empty() => {
                    let v = self.operand(v)?;
                    self.store(place.local, v)?;
                }
                StmtKind::If(test, then, otherwise) => {
                    let test = self.operand(test)?;
                    let test = self.convert(test, &Ty::Bool)?.text;
                    let (yes, no, join) = (self.name("then"), self.name("else"), self.name("join"));
                    self.terminate(format!("br i1 {test}, label %{yes}, label %{no}"));
                    self.lines.push(format!("{yes}:"));
                    self.ended = false;
                    self.block(then)?;
                    if !self.ended {
                        self.terminate(format!("br label %{join}"));
                    }
                    self.lines.push(format!("{no}:"));
                    self.ended = false;
                    self.block(otherwise)?;
                    self.label(&join);
                }
                StmtKind::Loop(body) => {
                    let (head, out) = (self.name("loop"), self.name("out"));
                    self.label(&head);
                    self.loops.push(Loop { next: head.clone(), out: out.clone() });
                    self.block(body)?;
                    self.loops.pop();
                    if !self.ended {
                        self.terminate(format!("br label %{head}"));
                    }
                    self.lines.push(format!("{out}:"));
                    self.ended = false;
                }
                StmtKind::Break => {
                    let out = self.loops.last().map(|l| l.out.clone());
                    let Some(out) = out else { return self.refuse("a `break` outside a loop") };
                    self.terminate(format!("br label %{out}"));
                }
                StmtKind::Continue => {
                    let next = self.loops.last().map(|l| l.next.clone());
                    let Some(next) = next else { return self.refuse("a `continue` outside a loop") };
                    self.terminate(format!("br label %{next}"));
                }
                StmtKind::Return(v) => {
                    let ret = self.f.ret.clone();
                    match v {
                        _ if is_unit(&ret) => self.terminate("ret void"),
                        Some(Operand::Const(Const::Unit)) | None => self.terminate("unreachable"),
                        Some(v) => {
                            let v = self.operand(v)?;
                            let v = self.convert(v, &ret)?;
                            let ty =
                                value_ty(&ret).map_or_else(|| self.refuse(format!("returning {}", kind(&ret))), Ok)?;
                            self.terminate(format!("ret {ty} {}", v.text));
                        }
                    }
                }
                StmtKind::ForRange { var, start, stop, step, body, exit } => {
                    self.for_range(*var, start, stop, step, body, exit)?;
                }
                StmtKind::Panic(p) => self.panic(p),
                StmtKind::Inc(_) | StmtKind::Dec(_) | StmtKind::DropReuse { .. } => {
                    return self.refuse("a value held by a count");
                }
                StmtKind::Store(..) => return self.refuse("storing into an element or a field"),
                StmtKind::Mutate { op, .. } => return self.refuse(format!("`{op:?}`")),
                StmtKind::ForStr { .. } => return self.refuse("a loop over a string"),
            }
        }
        Ok(())
    }

    /// `for var in range(start, stop, step)`, its `exit` once the range is spent: no value past
    /// `stop`, and the step that would overflow ends the walk, as `lt_range_step` does.
    fn for_range(
        &mut self,
        var: Local,
        start: &Operand,
        stop: &Operand,
        step: &Operand,
        body: &Block,
        exit: &Block,
    ) -> Result<(), Refusal> {
        let i64_ty = Ty::Int(IntKind::I64);
        let start = self.operand(start)?;
        let start = self.convert(start, &i64_ty)?.text;
        let stop = self.operand(stop)?;
        let stop = self.convert(stop, &i64_ty)?.text;
        let step = self.operand(step)?;
        let step = self.convert(step, &i64_ty)?.text;
        let zero = self.value(format!("icmp eq i64 {step}, 0"));
        let site = self.site();
        let message = self.module.text(b"range() arg 3 must not be zero\0");
        self.module.declare("declare void @lt_value_error(ptr, ptr) noreturn");
        self.check(&zero, format!("call void @lt_value_error(ptr {site}, ptr {message})"));
        let (next, done) = (self.name("%next"), self.name("%done"));
        self.allocas.push(format!("  {next} = alloca i64"));
        self.allocas.push(format!("  {done} = alloca i1"));
        self.emit(format!("store i64 {start}, ptr {next}"));
        self.emit(format!("store i1 false, ptr {done}"));
        let (head, step_block, spent, out) =
            (self.name("range"), self.name("step"), self.name("spent"), self.name("out"));
        self.label(&head);
        let finished = self.value(format!("load i1, ptr {done}"));
        let at = self.value(format!("load i64, ptr {next}"));
        let upward = self.value(format!("icmp sgt i64 {step}, 0"));
        let past_up = self.value(format!("icmp sge i64 {at}, {stop}"));
        let past_down = self.value(format!("icmp sle i64 {at}, {stop}"));
        let past = self.value(format!("select i1 {upward}, i1 {past_up}, i1 {past_down}"));
        let over = self.value(format!("or i1 {finished}, {past}"));
        self.terminate(format!("br i1 {over}, label %{spent}, label %{step_block}"));
        self.lines.push(format!("{step_block}:"));
        self.ended = false;
        self.module.declare("declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)");
        let pair = self.value(format!("call {{ i64, i1 }} @llvm.sadd.with.overflow.i64(i64 {at}, i64 {step})"));
        let advanced = self.value(format!("extractvalue {{ i64, i1 }} {pair}, 0"));
        let overflowed = self.value(format!("extractvalue {{ i64, i1 }} {pair}, 1"));
        self.emit(format!("store i64 {advanced}, ptr {next}"));
        self.emit(format!("store i1 {overflowed}, ptr {done}"));
        self.store(var, Value { text: at, ty: i64_ty })?;
        self.loops.push(Loop { next: head.clone(), out: out.clone() });
        self.block(body)?;
        self.loops.pop();
        if !self.ended {
            self.terminate(format!("br label %{head}"));
        }
        self.lines.push(format!("{spent}:"));
        self.ended = false;
        self.block(exit)?;
        self.label(&out);
        Ok(())
    }
}

/// What to call a value of `ty` this backend does not compile, in a refusal.
fn kind(ty: &Ty) -> String {
    match ty {
        Ty::Str => "a `str` value".into(),
        Ty::Bytes => "a `bytes` value".into(),
        Ty::List(_) => "a list".into(),
        Ty::Heap(_) => "a heap".into(),
        Ty::Dict(..) => "a dict".into(),
        Ty::Set(_) => "a set".into(),
        Ty::Tuple(_) => "a tuple".into(),
        Ty::Optional(_) => "an optional".into(),
        Ty::Result(..) => "a result".into(),
        Ty::Adt(..) => "a record or a variant".into(),
        Ty::Func(..) => "a closure".into(),
        Ty::Dyn(_) => "a `dyn` value".into(),
        other => format!("a value of type {other}"),
    }
}

/// What to call an expression this backend does not compile, in a refusal.
fn expression_name(e: &Expr) -> String {
    let name = match e {
        Expr::Format(_) => "an f-string",
        Expr::ToStr(..) => "`str()`",
        Expr::Len(..) => "`len()`",
        Expr::ListNew { .. } | Expr::ListGet { .. } => "a list",
        Expr::TupleNew { .. } | Expr::TupleGet { .. } => "a tuple",
        Expr::Construct { .. } | Expr::UnitVariant { .. } | Expr::Field { .. } | Expr::Tag(_) => {
            "a record or a variant"
        }
        Expr::OptNew { .. } | Expr::OptIsSome(_) | Expr::OptValue(_) | Expr::OptIf { .. } => "an optional",
        Expr::ResultNew { .. } | Expr::ResultIsOk(_) | Expr::ResultValue(_) | Expr::ResultError(_) => "a result",
        Expr::DictNew { .. } => "a dict",
        Expr::SetNew { .. } => "a set",
        Expr::Closure { .. } | Expr::CallClosure { .. } | Expr::Capture { .. } | Expr::FnRef(_) => "a closure",
        Expr::CallC { .. } => "a call into a C library",
        Expr::CallPython { .. } => "a call into a Python module",
        Expr::Parallel { .. } => "`parallel`",
        Expr::ToDyn { .. } | Expr::CallDyn { .. } => "a `dyn` value",
        Expr::Contains { .. } => "`in`",
        Expr::ReadPlace(_) => "reading an element or a field",
        Expr::CallSlots(..) => "an `inout` argument",
        Expr::RtValue { .. } => "this built-in operation",
        _ => "this expression",
    };
    name.to_string()
}
