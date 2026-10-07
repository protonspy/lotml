//! Writing LLVM IR from the counted IR (specs/llvm-backend/design.md, specs/llvm-parity/design.md):
//! one `define` per function, its locals as `alloca`s in the entry block that `-O2`'s `mem2reg`
//! turns into registers, each structured statement walked once into basic blocks. Numbers and the
//! checks on them are written in IR; strings, collections, text, hashing and the rest are calls
//! into the runtime, with the descriptors and functions of the program's types beside them.

use std::fmt::Write;

use lotml_check::ty::{FloatKind, IntKind, Ty};
use lotml_diag::Diagnostic;
use lotml_ir::ir::{
    Arg, BinOp, Block, CmpOp, Const, Expr, FormatPart, Function, Local, Operand, Panic, Place, Proj, StmtKind, UnOp,
    block_types,
};
use lotml_ir::lower::Lowered;
use lotml_ir::symbol;
use lotml_runtime::abi::CType;
use lotml_syntax::span::Span;

use crate::export::Export;
use crate::module::Module;
use crate::types::{self, Types, int_bits};

/// Where the emitter reads the runtime's cells by offset (specs/llvm-parity R3.1, checked against
/// `offsetof` by `layout`): an `lt_str`'s size in bytes, its length in characters and its bytes,
/// the length of an `lt_list` and of an `lt_dict`, and the used slots of an `lt_set`.
pub(crate) const STR_SIZE: u64 = 8;
pub(crate) const STR_LENGTH: u64 = 16;
pub(crate) const STR_BYTES: u64 = 32;
pub(crate) const LIST_LEN: u64 = 8;
pub(crate) const DICT_LEN: u64 = 24;
pub(crate) const SET_USED: u64 = 24;

const F64: Ty = Ty::Float(FloatKind::F64);
const I64: Ty = Ty::Int(IntKind::I64);
const U64: Ty = Ty::Int(IntKind::U64);

/// Where a module is entered: the `main` of a program, which runs its `fn main()` or its `test`
/// blocks, or the functions a library exports (specs/c-abi-export).
pub enum Entry<'e> {
    Main,
    Tests,
    Library(&'e [Export]),
}

/// The LLVM IR of the module, entered by `entry`; with `lines`, it carries line tables naming the
/// `.lot` file. A construct this backend does not compile is a diagnostic at its statement.
pub fn program(lowered: &Lowered, file: &str, entry: Entry, lines: bool) -> Result<String, Vec<Diagnostic>> {
    let mut types = Types::new(&lowered.declared, &lowered.traits);
    for vtable in &lowered.vtables {
        types.register(&vtable.ty);
        for slot in vtable.slots.iter().flatten() {
            slot.params.iter().for_each(|t| types.register(t));
            types.register(&slot.ret);
        }
    }
    for (captures, ty) in &lowered.lambdas {
        captures.iter().for_each(|t| types.register(t));
        types.register(ty);
    }
    for f in &lowered.functions {
        for info in &f.locals {
            types.register(&info.ty);
        }
        types.register(&f.ret);
        block_types(&f.body, &mut |ty| types.register(ty));
    }
    let mut module = if lines { Module::with_lines(file) } else { Module::new(file) };
    let mut refused = Vec::new();
    let mut bodies = String::new();
    let mut results = Vec::new();
    for f in &lowered.functions {
        let scope = module.subprogram(&f.source_name, lowered.line(f.span));
        let writer = Writer {
            lowered,
            types: &types,
            f,
            module: &mut module,
            allocas: Vec::new(),
            lines: Vec::new(),
            fresh: 0,
            ended: false,
            loops: Vec::new(),
            span: f.span,
            results: &mut results,
            scope,
        };
        match writer.function() {
            Ok(text) => bodies.push_str(&text),
            Err((span, what)) => refused.push(Diagnostic::error(
                "E0402",
                span,
                format!("`--target llvm` does not compile {what} yet: run it with `--target python`"),
            )),
        }
    }
    if !refused.is_empty() {
        return Err(refused);
    }
    types.define(&mut module);
    let mut extra = String::new();
    c_functions(&mut module, lowered);
    closures(&mut extra, lowered, &types);
    vtables(&mut extra, lowered, &types);
    task_runners(&mut extra, &types, &results);
    let main = match entry {
        Entry::Main => main(&mut module, lowered, &types, false),
        Entry::Tests => main(&mut module, lowered, &types, true),
        Entry::Library(exports) => wrappers(&mut module, lowered, &types, exports),
    };
    module.definitions.push_str(&extra);
    let mut out = module.header();
    out.push_str(&bodies);
    out.push_str(&main);
    out.push_str(&module.metadata());
    Ok(out)
}

/// The program's `main`: the runtime started, the program's `main` or its tests run, its status.
fn main(module: &mut Module, lowered: &Lowered, types: &Types, tests: bool) -> String {
    module.runtime("lt_init");
    module.runtime("lt_exit");
    let mut out = String::from("define i32 @main() {\nentry:\n  call void @lt_init()\n");
    if tests {
        module.runtime("lt_run_test");
        module.runtime("lt_test_report");
        for (name, function) in &lowered.tests {
            let text = module.text_z(name);
            let _ = writeln!(out, "  call void @lt_run_test(ptr {text}, ptr @{function})");
        }
        out.push_str("  call void @lt_test_report()\n  %status = call i32 @lt_exit(i32 0)\n  ret i32 %status\n}\n");
        return out;
    }
    let f = lowered.functions.iter().find(|f| f.name == symbol::function("main"));
    match f {
        Some(f) if matches!(f.ret, Ty::Result(..)) => {
            let Ty::Result(_, error) = &f.ret else { unreachable!() };
            let ty = types.memory(&f.ret);
            module.runtime("lt_main_error");
            let desc = types.desc(&f.ret);
            let error_desc = types.desc(error);
            let _ = writeln!(
                out,
                "  %r = call {ty} @{}()\n  %slot = alloca {ty}\n  store {ty} %r, ptr %slot\n  %ok.byte = extractvalue {ty} %r, 0\n  \
                 %ok = icmp ne i8 %ok.byte, 0\n  br i1 %ok, label %fine, label %failed\nfailed:\n  \
                 %error = getelementptr {ty}, ptr %slot, i32 0, i32 2\n  call void @lt_main_error(ptr {error_desc}, ptr %error)\n  \
                 %dec.f = getelementptr %lt_type, ptr {desc}, i32 0, i32 {}\n  %dec = load ptr, ptr %dec.f\n  call void %dec(ptr %slot)\n  \
                 %one = call i32 @lt_exit(i32 1)\n  ret i32 %one\nfine:\n  %dec2.f = getelementptr %lt_type, ptr {desc}, i32 0, i32 {}\n  \
                 %dec2 = load ptr, ptr %dec2.f\n  call void %dec2(ptr %slot)",
                f.name,
                types::DEC,
                types::DEC
            );
            out.push_str("  %status = call i32 @lt_exit(i32 0)\n  ret i32 %status\n}\n");
        }
        Some(f) => {
            let _ = writeln!(out, "  call void @{}()", f.name);
            out.push_str("  %status = call i32 @lt_exit(i32 0)\n  ret i32 %status\n}\n");
        }
        None => {
            module.runtime("lt_no_main");
            out.push_str("  %status = call i32 @lt_no_main()\n  ret i32 %status\n}\n");
        }
    }
    out
}

/// A library's exported functions (specs/c-abi-export R2.1-R2.3): each an external function C
/// calls, with the attributes `clang` gives the same C declaration, that tells the runtime it is
/// called, copies each `str` argument into a string, validated, calls the module's function, which
/// takes the strings over, and writes out what it printed.
fn wrappers(module: &mut Module, lowered: &Lowered, types: &Types, exports: &[Export]) -> String {
    module.runtime("lt_library_call");
    module.runtime("lt_library_return");
    let linkage = if cfg!(windows) { "dllexport " } else { "" };
    let mut out = String::new();
    for e in exports {
        let Some(f) = lowered.functions.iter().find(|f| f.name == symbol::function(&e.name)) else { continue };
        let mut declared = Vec::new();
        let mut body = String::from("  call void @lt_library_call()\n");
        let mut passed = Vec::new();
        for (i, ((name, ty), &p)) in e.params.iter().zip(&f.params).enumerate() {
            declared.push(format!("{} %c{i}", ffi(ty)));
            let inner = types.value(&f.locals[p].ty);
            let value = match ty {
                Ty::Str => {
                    module.runtime("lt_str_from_c");
                    let param = module.text_z(name);
                    let site = module.site(lowered.line(e.span), &f.source_name);
                    let _ = writeln!(body, "  %a{i} = call ptr @lt_str_from_c(ptr %c{i}, ptr {param}, ptr {site})");
                    format!("%a{i}")
                }
                Ty::Float(FloatKind::F32) => {
                    let _ = writeln!(body, "  %a{i} = fpext float %c{i} to double");
                    format!("%a{i}")
                }
                _ => format!("%c{i}"),
            };
            passed.push(format!("{inner} {value}"));
        }
        let inner = ret_ty(types, &f.ret);
        let call = format!("call {inner} @{}({})", f.name, passed.join(", "));
        let returned = match &e.ret {
            Ty::Unit => {
                let _ = writeln!(body, "  {call}\n  call void @lt_library_return()");
                "  ret void".to_string()
            }
            Ty::Float(FloatKind::F32) => {
                let _ = writeln!(
                    body,
                    "  %r = {call}\n  call void @lt_library_return()\n  %n = fptrunc double %r to float"
                );
                "  ret float %n".to_string()
            }
            ty => {
                let _ = writeln!(body, "  %r = {call}\n  call void @lt_library_return()");
                format!("  ret {} %r", ffi(ty).split(' ').next().unwrap_or("void"))
            }
        };
        let _ = writeln!(
            out,
            "define {linkage}{} @{}({}) {{\nentry:\n{body}{returned}\n}}\n",
            ffi_returned(&e.ret),
            e.symbol,
            declared.join(", ")
        );
    }
    out
}

/// The type a value of `ty` crosses into a C library as (adr:0013).
fn ffi(ty: &Ty) -> &'static str {
    match ty {
        Ty::Int(IntKind::I8) => "i8 signext",
        Ty::Int(IntKind::U8) => "i8 zeroext",
        Ty::Int(IntKind::I16) => "i16 signext",
        Ty::Int(IntKind::U16) => "i16 zeroext",
        Ty::Int(IntKind::I32 | IntKind::U32) => "i32",
        Ty::Int(_) => "i64",
        Ty::Float(FloatKind::F32) => "float",
        Ty::Float(_) => "double",
        Ty::Bool => "i1 zeroext",
        Ty::Str => "ptr",
        _ => "void",
    }
}

/// The type a value of `ty` comes back from a C library as: an attribute before the type, where
/// LLVM wants a return attribute.
fn ffi_returned(ty: &Ty) -> String {
    match ffi(ty).split_once(' ') {
        Some((ty, attribute)) => format!("{attribute} {ty}"),
        None => ffi(ty).to_string(),
    }
}

/// The C library functions the program calls, declared as their interfaces declare them.
fn c_functions(module: &mut Module, lowered: &Lowered) {
    for (symbol, (params, ret)) in &lowered.c_functions {
        let params: Vec<&str> = params.iter().filter(|t| !matches!(t, Ty::Unit)).map(ffi).collect();
        module.declare(&format!("declare {} @{symbol}({})", ffi_returned(ret), params.join(", ")));
    }
}

/// What a function returns in LLVM: `void` for a unit or one that never returns.
fn ret_ty(types: &Types, ty: &Ty) -> String {
    if is_unit(ty) { "void".into() } else { types.value(ty) }
}

fn is_unit(ty: &Ty) -> bool {
    matches!(ty, Ty::Unit | Ty::Never)
}

/// Each lambda's closure type and the functions dropping and sharing what it captured; each
/// function used as a value, a static closure calling it through a function taking the closure.
fn closures(out: &mut String, lowered: &Lowered, types: &Types) {
    for (k, (captures, _)) in lowered.lambdas.iter().enumerate() {
        let fields: String = captures.iter().map(|t| format!(", {}", types.memory(t))).collect();
        let _ = writeln!(out, "%lt_c{k} = type {{ %lt_cell, ptr, ptr, ptr{fields} }}");
        for (slot, name) in [(types::DEC, "drop"), (types::SHARE, "share")] {
            let _ = writeln!(out, "define internal void @lt_{name}_c{k}(ptr %self) {{\nentry:");
            for (i, t) in captures.iter().enumerate() {
                if Types::counted(t) {
                    let desc = types.desc(t);
                    let _ = writeln!(
                        out,
                        "  %c{i} = getelementptr %lt_c{k}, ptr %self, i32 0, i32 {}\n  %f{i}.at = getelementptr %lt_type, ptr {desc}, i32 0, i32 {slot}\n  \
                         %f{i} = load ptr, ptr %f{i}.at\n  call void %f{i}(ptr %c{i})",
                        4 + i
                    );
                }
            }
            out.push_str("  ret void\n}\n\n");
        }
    }
    for name in &lowered.fn_refs {
        let Some(f) = lowered.functions.iter().find(|f| f.name == *name) else { continue };
        let params: Vec<String> =
            f.params.iter().filter(|&&p| !is_unit(&f.locals[p].ty)).map(|&p| types.value(&f.locals[p].ty)).collect();
        let declared: Vec<String> = params.iter().enumerate().map(|(i, t)| format!("{t} %a{i}")).collect();
        let passed = declared.join(", ");
        let ret = ret_ty(types, &f.ret);
        let mut all = vec!["ptr %self".to_string()];
        all.extend(declared.iter().cloned());
        if ret == "void" {
            let _ = writeln!(
                out,
                "define internal void @lt_tramp_{name}({}) {{\nentry:\n  call void @{name}({passed})\n  ret void\n}}\n",
                all.join(", ")
            );
        } else {
            let _ = writeln!(
                out,
                "define internal {ret} @lt_tramp_{name}({}) {{\nentry:\n  %r = call {ret} @{name}({passed})\n  ret {ret} %r\n}}\n",
                all.join(", ")
            );
        }
        let _ = writeln!(
            out,
            "@lt_fnref_{name} = internal global %lt_closure {{ %lt_cell zeroinitializer, ptr @lt_tramp_{name}, ptr null, ptr null }}"
        );
    }
}

/// Each table of a type's methods for a trait: a function per slot taking the cell and calling the
/// type's method.
fn vtables(out: &mut String, lowered: &Lowered, types: &Types) {
    for (k, vtable) in lowered.vtables.iter().enumerate() {
        let mut slots = Vec::new();
        for (i, slot) in vtable.slots.iter().enumerate() {
            let Some(slot) = slot else {
                slots.push("ptr null".to_string());
                continue;
            };
            let params: Vec<&Ty> = slot.params.iter().filter(|t| !is_unit(t)).collect();
            let mut declared = vec!["ptr %self".to_string()];
            declared.extend(params.iter().enumerate().map(|(i, t)| format!("{} %a{i}", types.value(t))));
            let mut passed = vec!["ptr %self".to_string()];
            passed.extend(params.iter().enumerate().map(|(i, t)| format!("{} %a{i}", types.value(t))));
            let ret = ret_ty(types, &slot.ret);
            let call = format!("call {ret} @{}({})", slot.function, passed.join(", "));
            let body =
                if ret == "void" { format!("  {call}\n  ret void") } else { format!("  %r = {call}\n  ret {ret} %r") };
            let _ =
                writeln!(out, "define internal {ret} @lt_vm{k}_{i}({}) {{\nentry:\n{body}\n}}\n", declared.join(", "));
            slots.push(format!("ptr @lt_vm{k}_{i}"));
        }
        if slots.is_empty() {
            slots.push("ptr null".to_string());
        }
        let _ = writeln!(
            out,
            "@lt_vt{k} = internal constant %lt_vt_{} {{ ptr {}, [{} x ptr] [{}] }}",
            vtable.trait_name,
            types.desc(&vtable.ty),
            slots.len(),
            slots.join(", ")
        );
    }
}

/// For each result type of a task of `parallel`, the function the runtime runs a task with.
fn task_runners(out: &mut String, types: &Types, results: &[Ty]) {
    for (k, ty) in results.iter().enumerate() {
        let ret = ret_ty(types, ty);
        let _ = writeln!(out, "define internal void @lt_task{k}(ptr %task, ptr %out) {{\nentry:");
        out.push_str("  %fn.at = getelementptr %lt_closure, ptr %task, i32 0, i32 1\n  %fn = load ptr, ptr %fn.at\n");
        if ret == "void" {
            out.push_str("  call void %fn(ptr %task)\n");
        } else {
            let _ = writeln!(out, "  %r = call {ret} %fn(ptr %task)");
            if matches!(ty, Ty::Bool) {
                out.push_str("  %byte = zext i1 %r to i8\n  store i8 %byte, ptr %out\n");
            } else {
                let _ = writeln!(out, "  store {ret} %r, ptr %out");
            }
        }
        out.push_str("  ret void\n}\n\n");
    }
}

/// The width of an integer kind, and whether it is signed.
fn int_kind(kind: IntKind) -> (u32, bool) {
    (int_bits(kind), matches!(kind, IntKind::I8 | IntKind::I16 | IntKind::I32 | IntKind::I64))
}

/// The range of a kind narrower than 64 bits, and its name, as the runtime's `lt_fit` checks it.
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

/// What a construct the backend does not compile is: its span and what to call it.
type Refusal = (Span, String);

/// A value in a register: its text and its LotML type.
#[derive(Clone)]
struct Value {
    text: String,
    ty: Ty,
}

impl Value {
    fn unit() -> Value {
        Value { text: String::new(), ty: Ty::Unit }
    }
}

/// The targets a `continue` and a `break` jump to, for the loop being written.
struct Loop {
    next: String,
    out: String,
}

struct Writer<'a, 't> {
    lowered: &'a Lowered,
    types: &'t Types<'a>,
    f: &'a Function,
    module: &'t mut Module,
    allocas: Vec<String>,
    lines: Vec<String>,
    fresh: usize,
    /// Whether the block being written has ended in a terminator.
    ended: bool,
    loops: Vec<Loop>,
    /// The span of the statement being written: where a refusal and a panic point.
    span: Span,
    /// The result type of each task runner `parallel` needs, by its number.
    results: &'t mut Vec<Ty>,
    /// The function's `DISubprogram`, when the module carries line tables.
    scope: Option<usize>,
}

impl Writer<'_, '_> {
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
        let line = line.into();
        match self.scope {
            Some(scope) => {
                let at = self.lowered.line(self.span);
                let start = self.lowered.line_starts.get(at as usize - 1).copied().unwrap_or(0);
                let location = self.module.location(at, self.span.start - start + 1, scope);
                self.lines.push(format!("  {line}, !dbg !{location}"));
            }
            None => self.lines.push(format!("  {line}")),
        }
    }

    /// `text`, an instruction producing a value, into a fresh register.
    fn value(&mut self, text: impl AsRef<str>) -> String {
        let r = self.name("%v");
        self.emit(format!("{r} = {}", text.as_ref()));
        r
    }

    /// Start the block `label`, falling into it from the one before.
    fn label(&mut self, label: &str) {
        if !self.ended {
            self.lines.push(format!("  br label %{label}"));
        }
        self.lines.push(format!("{label}:"));
        self.ended = false;
    }

    /// Start the block `label`, the one before having ended.
    fn start(&mut self, label: &str) {
        self.lines.push(format!("{label}:"));
        self.ended = false;
    }

    fn terminate(&mut self, line: impl Into<String>) {
        self.emit(line);
        self.ended = true;
    }

    /// A new stack slot of `ty`'s memory type, in the entry block.
    fn slot(&mut self, ty: &str) -> String {
        let slot = self.name("%s");
        self.allocas.push(format!("  {slot} = alloca {ty}"));
        slot
    }

    fn function(mut self) -> Result<String, Refusal> {
        let f = self.f;
        let ret = ret_ty(self.types, &f.ret);
        let mut params = Vec::new();
        for &p in &f.params {
            let info = &f.locals[p];
            if is_unit(&info.ty) {
                continue;
            }
            if info.by_ref {
                params.push(format!("ptr %p{p}"));
            } else {
                params.push(format!("{} %p{p}", self.types.value(&info.ty)));
            }
        }
        for (i, info) in f.locals.iter().enumerate() {
            if is_unit(&info.ty) {
                continue;
            }
            if info.by_ref {
                self.allocas.push(format!("  %l{i} = getelementptr i8, ptr %p{i}, i64 0"));
                continue;
            }
            let ty = self.types.memory(&info.ty);
            self.allocas.push(format!("  %l{i} = alloca {ty}"));
            self.allocas.push(format!("  store {ty} {}, ptr %l{i}", zero(&ty)));
        }
        for &p in &f.params {
            let info = &f.locals[p];
            if is_unit(&info.ty) || info.by_ref {
                continue;
            }
            self.store(p, Value { text: format!("%p{p}"), ty: info.ty.clone() })?;
        }
        self.block(&f.body)?;
        if !self.ended {
            if ret == "void" {
                self.terminate("ret void");
            } else {
                self.terminate(format!("ret {ret} {}", zero(&ret)));
            }
        }
        let debug = self.scope.map_or(String::new(), |scope| format!(" !dbg !{scope}"));
        let mut out = format!("define internal {ret} @{}({}){debug} {{\nentry:\n", f.name, params.join(", "));
        for line in self.allocas.iter().chain(&self.lines) {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str("}\n\n");
        Ok(out)
    }

    // Values in memory and in registers ------------------------------------------------------

    /// A value as its memory type holds it: a `bool` widened to a byte, a unit a zero byte.
    fn stored(&mut self, v: &Value) -> String {
        match &v.ty {
            Ty::Bool => {
                if v.text == "true" || v.text == "false" {
                    return if v.text == "true" { "1".into() } else { "0".into() };
                }
                self.value(format!("zext i1 {} to i8", v.text))
            }
            Ty::Unit | Ty::Never if v.text.is_empty() => "0".into(),
            _ => v.text.clone(),
        }
    }

    /// What memory of `ty` holds, read as a register value.
    fn loaded(&mut self, text: String, ty: &Ty) -> Value {
        if matches!(ty, Ty::Bool) {
            let bit = self.value(format!("trunc i8 {text} to i1"));
            return Value { text: bit, ty: Ty::Bool };
        }
        Value { text, ty: ty.clone() }
    }

    fn load_at(&mut self, ptr: &str, ty: &Ty) -> Value {
        if is_unit(ty) {
            return Value::unit();
        }
        let memory = self.types.memory(ty);
        let loaded = self.value(format!("load {memory}, ptr {ptr}"));
        self.loaded(loaded, ty)
    }

    fn store_at(&mut self, ptr: &str, v: Value, ty: &Ty) -> Result<(), Refusal> {
        if is_unit(ty) {
            return Ok(());
        }
        let v = self.convert(v, ty)?;
        let memory = self.types.memory(ty);
        let text = self.stored(&v);
        self.emit(format!("store {memory} {text}, ptr {ptr}"));
        Ok(())
    }

    fn load(&mut self, l: Local) -> Result<Value, Refusal> {
        let ty = self.f.locals[l].ty.clone();
        Ok(self.load_at(&format!("%l{l}"), &ty))
    }

    fn store(&mut self, l: Local, v: Value) -> Result<(), Refusal> {
        let ty = self.f.locals[l].ty.clone();
        self.store_at(&format!("%l{l}"), v, &ty)
    }

    /// `v` in a new stack slot of its own: a pointer the runtime reads it through.
    fn spill(&mut self, v: Value) -> String {
        let memory = self.types.memory(&v.ty);
        let slot = self.slot(&memory);
        let text = self.stored(&v);
        self.emit(format!("store {memory} {text}, ptr {slot}"));
        slot
    }

    /// A pointer to the value of `o`, of type `ty`: the local's own slot, or a slot holding it.
    fn address(&mut self, o: &Operand, ty: &Ty) -> Result<String, Refusal> {
        if let Operand::Local(l) = o
            && self.types.memory(&self.f.locals[*l].ty) == self.types.memory(ty)
        {
            return Ok(format!("%l{l}"));
        }
        let v = self.operand(o)?;
        let v = self.convert(v, ty)?;
        Ok(self.spill(v))
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
            Operand::Const(Const::Float(v)) => Ok(Value { text: format!("0x{:016X}", v.to_bits()), ty: F64 }),
            Operand::Const(Const::Bool(b)) => Ok(Value { text: b.to_string(), ty: Ty::Bool }),
            Operand::Const(Const::Unit) => Ok(Value::unit()),
            Operand::Const(Const::Str(text)) => Ok(Value { text: self.module.literal(text), ty: Ty::Str }),
            Operand::Const(Const::Null) => Ok(Value { text: "null".into(), ty: Ty::Str }),
            Operand::Const(Const::Bytes(_)) => self.refuse("a bytes value"),
            Operand::Const(Const::Char(c)) => {
                Ok(Value { text: (u32::from(*c) as u8 as i8).to_string(), ty: Ty::Int(IntKind::I8) })
            }
        }
    }

    /// `v` as a value of `to`, as C converts it on assignment and in a call.
    fn convert(&mut self, v: Value, to: &Ty) -> Result<Value, Refusal> {
        let out = |text: String| Value { text, ty: to.clone() };
        if v.ty == *to {
            return Ok(v);
        }
        Ok(match (&v.ty, to) {
            (Ty::Float(_), Ty::Float(_)) => out(v.text),
            (_, Ty::Unit) => Value::unit(),
            (Ty::Never | Ty::Unit, _) => out(zero(&self.types.memory(to))),
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
            (Ty::Float(_), Ty::Int(b)) => {
                let op = if int_kind(*b).1 { "fptosi" } else { "fptoui" };
                out(self.value(format!("{op} double {} to i{}", v.text, int_bits(*b))))
            }
            (Ty::Bool, Ty::Int(b)) => out(self.value(format!("zext i1 {} to i{}", v.text, int_bits(*b)))),
            (Ty::Bool, Ty::Float(_)) => out(self.value(format!("uitofp i1 {} to double", v.text))),
            (Ty::Int(a), Ty::Bool) => out(self.value(format!("icmp ne i{} {}, 0", int_bits(*a), v.text))),
            (from, to) if self.types.memory(from) == self.types.memory(to) && !matches!(to, Ty::Bool) => out(v.text),
            (from, to) => return self.refuse(format!("a {from} used as a {to}")),
        })
    }

    /// The global `lt_at` of the statement being written.
    fn site(&mut self) -> String {
        let line = self.lowered.line(self.span);
        self.module.site(line, &self.f.source_name)
    }

    // Calls into the runtime ------------------------------------------------------------------

    /// A call of the runtime function `name` with `args` already in its parameters' types: its
    /// result, as a value of the LotML type its C type is.
    fn runtime(&mut self, name: &str, args: &[String]) -> Value {
        let sig = self.module.runtime(name);
        let passed: Vec<String> = sig.params.iter().zip(args).map(|(p, a)| format!("{} {a}", p.llvm())).collect();
        let call = format!("call {} @{name}({})", sig.ret.returned(), passed.join(", "));
        match sig.ret {
            CType::Void => {
                self.emit(call);
                if sig.noreturn {
                    self.terminate("unreachable");
                }
                Value::unit()
            }
            ret => {
                let text = self.value(call);
                Value { text, ty: c_ty(ret) }
            }
        }
    }

    /// The value of `arg` as the runtime parameter of C type `param` takes it.
    fn runtime_arg(&mut self, arg: &Arg, param: CType) -> Result<String, Refusal> {
        Ok(match arg {
            Arg::Value(o) => {
                let v = self.operand(o)?;
                self.as_c(v, param)?
            }
            Arg::Address(o, ty) => self.address(o, ty)?,
            Arg::Out(l, _) => format!("%l{l}"),
            Arg::Desc(ty) => self.types.desc(ty),
            Arg::Offset(ty) => {
                let fields = self.types.fields_of(ty);
                self.types.offset(&fields, 1).to_string()
            }
            Arg::Slot(place) => self.place(place)?.1,
        })
    }

    /// `v` as C's `param` takes it: an integer of its width, a `bool` bit, a double, a pointer.
    fn as_c(&mut self, v: Value, param: CType) -> Result<String, Refusal> {
        let target = c_ty(param);
        let v = self.convert(v, &target)?;
        Ok(v.text)
    }

    /// A call of the built-in operation `op` with `args`, given the place when `at` is set.
    fn builtin(&mut self, op: lotml_ir::ir::Builtin, args: &[Arg], at: bool) -> Result<Value, Refusal> {
        let name = lotml_runtime::function(op);
        let sig = self.module.runtime(name);
        let mut passed = Vec::new();
        for (a, &p) in args.iter().zip(&sig.params) {
            passed.push(self.runtime_arg(a, p)?);
        }
        if at {
            passed.push(self.site());
        }
        Ok(self.runtime(name, &passed))
    }

    // Places ------------------------------------------------------------------------------------

    /// The type of what `place` holds, and a pointer to its slot.
    fn place(&mut self, place: &Place) -> Result<(Ty, String), Refusal> {
        let mut ty = self.f.locals[place.local].ty.clone();
        let mut slot = format!("%l{}", place.local);
        for proj in &place.proj {
            match proj {
                Proj::Index(i) => {
                    let elem = element(&ty);
                    let i = self.operand(i)?;
                    let i = self.convert(i, &I64)?.text;
                    let site = self.site();
                    slot = self.runtime("lt_list_slot", &[slot, i, site]).text;
                    ty = elem;
                }
                Proj::OwnedIndex(i) => {
                    let elem = element(&ty);
                    let i = self.operand(i)?;
                    let i = self.convert(i, &I64)?.text;
                    let list = self.value(format!("load ptr, ptr {slot}"));
                    let at = self.index(&list, &i);
                    let data = self.list_data(&list);
                    let memory = self.types.memory(&elem);
                    slot = self.value(format!("getelementptr {memory}, ptr {data}, i64 {at}"));
                    ty = elem;
                }
                Proj::Field(index) => {
                    let id = self.types.ids[&ty];
                    let field = self.types.cell_fields(&ty, None).get(*index).cloned().unwrap_or(Ty::Unit);
                    let cell = self.value(format!("call ptr @lt_own_t{id}(ptr {slot})"));
                    let cell_ty = self.types.cell(&ty, None);
                    slot = self.value(format!("getelementptr {cell_ty}, ptr {cell}, i32 0, i32 {}", index + 1));
                    ty = field;
                }
                Proj::Key(k) => {
                    let (key, value) = dict_types(&ty);
                    let k = self.address(k, &key)?;
                    let site = self.site();
                    slot = self.runtime("lt_dict_slot", &[slot, k, site]).text;
                    ty = value;
                }
                Proj::SetDefault(k, d) => {
                    let (key, value) = dict_types(&ty);
                    let k = self.address(k, &key)?;
                    let d = self.address(d, &value)?;
                    slot = self.runtime("lt_dict_setdefault", &[slot, k, d]).text;
                    ty = value;
                }
            }
        }
        Ok((ty, slot))
    }

    /// The element `i` of a list of `len`, negative from its end, checked as `lt_index` checks it.
    fn index(&mut self, list: &str, i: &str) -> String {
        let len_at = self.value(format!("getelementptr %lt_list, ptr {list}, i32 0, i32 1"));
        let len = self.value(format!("load i64, ptr {len_at}"));
        let negative = self.value(format!("icmp slt i64 {i}, 0"));
        let from_end = self.value(format!("add i64 {i}, {len}"));
        let at = self.value(format!("select i1 {negative}, i64 {from_end}, i64 {i}"));
        let outside = self.value(format!("icmp uge i64 {at}, {len}"));
        let site = self.site();
        let message = self.module.text_z("list index out of range");
        self.module.runtime("lt_index_error");
        self.check(&outside, format!("call void @lt_index_error(ptr {site}, ptr {message})"));
        at
    }

    fn list_data(&mut self, list: &str) -> String {
        let at = self.value(format!("getelementptr %lt_list, ptr {list}, i32 0, i32 4"));
        self.value(format!("load ptr, ptr {at}"))
    }

    // Checks and panics -------------------------------------------------------------------------

    /// A branch to a block that stops the program with `call` when `failing` holds.
    fn check(&mut self, failing: &str, call: String) {
        let (bad, good) = (self.name("stop"), self.name("ok"));
        self.terminate(format!("br i1 {failing}, label %{bad}, label %{good}"));
        self.start(&bad);
        self.emit(call);
        self.terminate("unreachable");
        self.start(&good);
    }

    fn overflow(&mut self, failing: &str, type_name: &str) {
        let site = self.site();
        let name = self.module.text_z(type_name);
        self.module.runtime("lt_overflow");
        self.check(failing, format!("call void @lt_overflow(ptr {site}, ptr {name})"));
    }

    fn zero_division(&mut self, failing: &str, message: &str) {
        let site = self.site();
        let text = self.module.text_z(message);
        self.module.runtime("lt_zero_division");
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

    fn runtime2(&mut self, name: &str, a: &str, b: &str) -> String {
        let site = self.site();
        self.runtime(name, &[a.to_string(), b.to_string(), site]).text
    }

    // Arithmetic --------------------------------------------------------------------------------

    fn binary(&mut self, op: BinOp, a: &Operand, b: &Operand, ty: &Ty) -> Result<Value, Refusal> {
        let (x, y) = (self.operand(a)?, self.operand(b)?);
        match ty {
            Ty::Float(_) => {
                let (x, y) = (self.convert(x, &F64)?.text, self.convert(y, &F64)?.text);
                let text = match op {
                    BinOp::Add => self.value(format!("fadd double {x}, {y}")),
                    BinOp::Sub => self.value(format!("fsub double {x}, {y}")),
                    BinOp::Mul => self.value(format!("fmul double {x}, {y}")),
                    BinOp::TrueDiv => {
                        let zero = self.value(format!("fcmp oeq double {y}, 0.0"));
                        self.zero_division(&zero, "float division by zero");
                        self.value(format!("fdiv double {x}, {y}"))
                    }
                    BinOp::FloorDiv => self.runtime2("lt_floordiv_f64", &x, &y),
                    BinOp::Mod => self.runtime2("lt_mod_f64", &x, &y),
                    BinOp::Pow => self.runtime2("lt_pow_f64", &x, &y),
                    other => return self.refuse(format!("`{other:?}` on a float")),
                };
                Ok(Value { text, ty: F64 })
            }
            Ty::Int(IntKind::U64) => {
                let (x, y) = (self.convert(x, &U64)?.text, self.convert(y, &U64)?.text);
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
                        return Ok(Value { text, ty: F64 });
                    }
                    BinOp::FloorDiv | BinOp::Mod => {
                        let zero = self.value(format!("icmp eq i64 {y}, 0"));
                        self.zero_division(&zero, "integer division or modulo by zero");
                        let inst = if op == BinOp::FloorDiv { "udiv" } else { "urem" };
                        self.value(format!("{inst} i64 {x}, {y}"))
                    }
                    BinOp::Pow => self.runtime2("lt_pow_u64", &x, &y),
                    BinOp::Shl => self.runtime2("lt_shl_u64", &x, &y),
                    BinOp::Shr => self.runtime2("lt_shr_u64", &x, &y),
                    BinOp::BitAnd => self.value(format!("and i64 {x}, {y}")),
                    BinOp::BitOr => self.value(format!("or i64 {x}, {y}")),
                    BinOp::BitXor => self.value(format!("xor i64 {x}, {y}")),
                };
                Ok(Value { text, ty: U64 })
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
                let (x, y) = (self.convert(x, &I64)?.text, self.convert(y, &I64)?.text);
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
                        return Ok(Value { text, ty: F64 });
                    }
                    BinOp::FloorDiv => self.floordiv(&x, &y),
                    BinOp::Mod => self.modulo(&x, &y),
                    BinOp::Pow => self.runtime2("lt_pow_i64", &x, &y),
                    BinOp::Shl => self.runtime2("lt_shl_i64", &x, &y),
                    BinOp::Shr => self.runtime2("lt_shr_i64", &x, &y),
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
        match (op, ty) {
            (UnOp::Not, _) => {
                let x = self.convert(x, &Ty::Bool)?;
                let text = self.value(format!("xor i1 {}, true", x.text));
                Ok(Value { text, ty: Ty::Bool })
            }
            (UnOp::Neg, Ty::Float(_)) => {
                let x = self.convert(x, &F64)?;
                let text = self.value(format!("fneg double {}", x.text));
                Ok(Value { text, ty: F64 })
            }
            (UnOp::Abs, Ty::Float(_)) => {
                let x = self.convert(x, &F64)?;
                self.module.declare("declare double @llvm.fabs.f64(double)");
                let text = self.value(format!("call double @llvm.fabs.f64(double {})", x.text));
                Ok(Value { text, ty: F64 })
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
                let x = self.convert(x, &I64)?.text;
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
        let order_of = |op: CmpOp| match op {
            CmpOp::Lt => "slt",
            CmpOp::Le => "sle",
            CmpOp::Gt => "sgt",
            CmpOp::Ge => "sge",
            CmpOp::Eq => "eq",
            CmpOp::Ne => "ne",
        };
        match ty {
            Ty::Float(_) => {
                let (x, y) = (self.operand(a)?, self.operand(b)?);
                let (x, y) = (self.convert(x, &F64)?.text, self.convert(y, &F64)?.text);
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
                let (x, y) = (self.operand(a)?, self.operand(b)?);
                let (width, signed) = match ty {
                    Ty::Int(kind) => int_kind(*kind),
                    _ => (1, false),
                };
                let (x, y) = (self.convert(x, ty)?.text, self.convert(y, ty)?.text);
                let cond = match (op, signed) {
                    (CmpOp::Eq | CmpOp::Ne, _) | (_, true) => order_of(op).to_string(),
                    (_, false) => order_of(op).replacen('s', "u", 1),
                };
                Ok(bool_value(self.value(format!("icmp {cond} i{width} {x}, {y}"))))
            }
            Ty::Str => {
                let (x, y) = (self.operand(a)?, self.operand(b)?);
                let (x, y) = (self.convert(x, ty)?.text, self.convert(y, ty)?.text);
                match op {
                    CmpOp::Eq | CmpOp::Ne => {
                        let eq = self.runtime("lt_str_eq", &[x, y]).text;
                        if op == CmpOp::Eq {
                            Ok(bool_value(eq))
                        } else {
                            Ok(bool_value(self.value(format!("xor i1 {eq}, true"))))
                        }
                    }
                    _ => {
                        let c = self.runtime("lt_str_compare", &[x, y]).text;
                        Ok(bool_value(self.value(format!("icmp {} i32 {c}, 0", order_of(op)))))
                    }
                }
            }
            _ => {
                let desc = self.types.desc(ty);
                let (x, y) = (self.address(a, ty)?, self.address(b, ty)?);
                match op {
                    CmpOp::Eq | CmpOp::Ne => {
                        let eq = self.desc_call(&desc, types::EQ, &[x, y]);
                        if op == CmpOp::Eq {
                            Ok(bool_value(eq))
                        } else {
                            Ok(bool_value(self.value(format!("xor i1 {eq}, true"))))
                        }
                    }
                    _ => {
                        let site = self.site();
                        let c = self.desc_call(&desc, types::CMP, &[x, y, site]);
                        Ok(bool_value(self.value(format!("icmp {} i32 {c}, 0", order_of(op)))))
                    }
                }
            }
        }
    }

    /// A call of the descriptor `desc`'s function in `slot` with pointer `args`: its result.
    fn desc_call(&mut self, desc: &str, slot: usize, args: &[String]) -> String {
        let at = self.value(format!("getelementptr %lt_type, ptr {desc}, i32 0, i32 {slot}"));
        let f = self.value(format!("load ptr, ptr {at}"));
        let args: Vec<String> = args.iter().map(|a| format!("ptr {a}")).collect();
        let ret = match slot {
            types::EQ => "zeroext i1",
            types::CMP => "i32",
            types::HASH => "i64",
            _ => {
                self.emit(format!("call void {f}({})", args.join(", ")));
                return String::new();
            }
        };
        self.value(format!("call {ret} {f}({})", args.join(", ")))
    }

    /// `int(x)`, `float(n)`, `i32(n)` and the rest: checked as the runtime's conversions check them.
    fn convert_expr(&mut self, value: &Operand, from: &Ty, to: &Ty) -> Result<Value, Refusal> {
        let v = self.operand(value)?;
        match (from, to) {
            (_, Ty::Float(_)) => self.convert(v, &F64),
            (Ty::Float(_), Ty::Int(kind)) => {
                let v = self.convert(v, &F64)?.text;
                let site = self.site();
                let name = if *kind == IntKind::U64 { "lt_f64_to_u64" } else { "lt_f64_to_i64" };
                let text = self.runtime(name, &[v, site]).text;
                if *kind == IntKind::U64 { Ok(Value { text, ty: U64 }) } else { Ok(self.fit(text, *kind)) }
            }
            (Ty::Int(IntKind::U64), Ty::Int(IntKind::U64)) => self.convert(v, to),
            (Ty::Int(IntKind::U64), Ty::Int(kind)) => {
                let v = self.convert(v, &U64)?.text;
                let too_big = self.value(format!("icmp ugt i64 {v}, {}", i64::MAX));
                self.overflow(&too_big, "int");
                Ok(self.fit(v, *kind))
            }
            (_, Ty::Int(IntKind::U64)) => {
                let v = self.convert(v, &I64)?.text;
                let negative = self.value(format!("icmp slt i64 {v}, 0"));
                self.overflow(&negative, "u64");
                Ok(Value { text: v, ty: U64 })
            }
            (_, Ty::Int(kind)) => {
                let v = self.convert(v, &I64)?.text;
                Ok(self.fit(v, *kind))
            }
            _ => self.convert(v, to),
        }
    }

    fn min_max(&mut self, max: bool, a: &Operand, b: &Operand, ty: &Ty) -> Result<Value, Refusal> {
        let (x, y) = (self.operand(a)?, self.operand(b)?);
        let (x, y) = (self.convert(x, ty)?, self.convert(y, ty)?);
        let vty = self.types.value(ty);
        // `b < a ? b : a` for `min` and `b > a ? b : a` for `max`, as the runtime's do.
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

    // Calls ---------------------------------------------------------------------------------

    fn call(&mut self, name: &str, args: &[Arg]) -> Result<Value, Refusal> {
        let callee = self.lowered.functions.iter().find(|f| f.name == name);
        let Some(callee) = callee else { return self.refuse(format!("a call of {name}")) };
        let mut passed = Vec::new();
        for (a, &p) in args.iter().zip(&callee.params) {
            let info = &callee.locals[p];
            if is_unit(&info.ty) {
                continue;
            }
            if info.by_ref {
                let slot = match a {
                    Arg::Slot(place) => self.place(place)?.1,
                    Arg::Value(Operand::Local(l)) => format!("%l{l}"),
                    other => return self.refuse(format!("the argument {other:?} of an `inout` parameter")),
                };
                passed.push(format!("ptr {slot}"));
                continue;
            }
            let v = match a {
                Arg::Value(o) => self.operand(o)?,
                Arg::Slot(place) => {
                    let (ty, slot) = self.place(place)?;
                    self.load_at(&slot, &ty)
                }
                other => return self.refuse(format!("the argument {other:?} of a call")),
            };
            let v = self.convert(v, &info.ty)?;
            passed.push(format!("{} {}", self.types.value(&info.ty), v.text));
        }
        let ret = ret_ty(self.types, &callee.ret);
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

    /// A call through a function pointer `f` of the type `params` -> `ret`, the closure or cell
    /// `first` passed before the arguments.
    fn call_through(
        &mut self,
        f: &str,
        first: &str,
        args: &[Operand],
        params: &[Ty],
        ret: &Ty,
    ) -> Result<Value, Refusal> {
        let mut passed = vec![format!("ptr {first}")];
        for (a, p) in args.iter().zip(params) {
            if is_unit(p) {
                continue;
            }
            let v = self.operand(a)?;
            let v = self.convert(v, p)?;
            passed.push(format!("{} {}", self.types.value(p), v.text));
        }
        let rt = ret_ty(self.types, ret);
        let call = format!("call {rt} {f}({})", passed.join(", "));
        if rt == "void" {
            self.emit(call);
            return Ok(Value { text: String::new(), ty: ret.clone() });
        }
        let text = self.value(call);
        Ok(Value { text, ty: ret.clone() })
    }

    // Expressions -----------------------------------------------------------------------------

    fn expr(&mut self, e: &Expr) -> Result<Value, Refusal> {
        match e {
            Expr::Use(o) => self.operand(o),
            Expr::Binary(op, a, b, ty) => self.binary(*op, a, b, ty),
            Expr::Unary(op, a, ty) => self.unary(*op, a, ty),
            Expr::Compare(op, a, b, ty) => self.compare(*op, a, b, ty),
            Expr::Convert(v, from, to) => self.convert_expr(v, from, to),
            Expr::MinMax { max, a, b, ty } => self.min_max(*max, a, b, ty),
            Expr::Call(name, args) => {
                let args: Vec<Arg> = args.iter().cloned().map(Arg::Value).collect();
                self.call(name, &args)
            }
            Expr::CallSlots(name, args) => self.call(name, args),
            Expr::Print { args, sep, end } => {
                self.print(args, sep.as_ref(), end.as_ref())?;
                Ok(Value::unit())
            }
            Expr::Rt { op, args, at } => self.builtin(*op, args, *at),
            Expr::RtValue { op, args, at, ty } => {
                let p = self.builtin(*op, args, *at)?;
                Ok(self.load_at(&p.text, ty))
            }
            Expr::Contains { container, item, ty } => {
                let c = self.operand(container)?.text;
                let found = match ty {
                    Ty::Str => {
                        let item = self.operand(item)?.text;
                        self.runtime("lt_str_contains", &[c, item])
                    }
                    Ty::List(elem) => {
                        let a = self.address(item, elem)?;
                        self.runtime("lt_list_contains", &[c, a])
                    }
                    Ty::Dict(k, _) => {
                        let a = self.address(item, k)?;
                        self.runtime("lt_dict_contains", &[c, a])
                    }
                    Ty::Set(t) => {
                        let a = self.address(item, t)?;
                        self.runtime("lt_set_contains", &[c, a])
                    }
                    other => return self.refuse(format!("`in` on {other}")),
                };
                Ok(Value { text: found.text, ty: Ty::Bool })
            }
            Expr::ToStr(value, ty) => {
                let v = self.operand(value)?;
                let s = match ty {
                    Ty::Str => return Ok(v),
                    Ty::Int(IntKind::U64) => {
                        let v = self.convert(v, &U64)?.text;
                        self.runtime("lt_str_of_u64", &[v])
                    }
                    Ty::Int(_) => {
                        let v = self.convert(v, &I64)?.text;
                        self.runtime("lt_str_of_i64", &[v])
                    }
                    Ty::Float(_) => {
                        let v = self.convert(v, &F64)?.text;
                        self.runtime("lt_str_of_f64", &[v])
                    }
                    Ty::Bool => {
                        let v = self.convert(v, &Ty::Bool)?.text;
                        self.runtime("lt_str_of_bool", &[v])
                    }
                    Ty::Unit => self.runtime("lt_str_none", &[]),
                    _ => {
                        let desc = self.types.desc(ty);
                        let a = self.spill(v);
                        self.runtime("lt_str_of_value", &[desc, a])
                    }
                };
                Ok(Value { text: s.text, ty: Ty::Str })
            }
            Expr::Len(value, ty) => {
                let v = self.operand(value)?.text;
                let offset = match ty {
                    Ty::Str => STR_LENGTH,
                    Ty::List(_) | Ty::Heap(_) => LIST_LEN,
                    Ty::Dict(..) => DICT_LEN,
                    Ty::Set(_) => SET_USED,
                    other => return self.refuse(format!("`len()` of {other}")),
                };
                let at = self.value(format!("getelementptr i8, ptr {v}, i64 {offset}"));
                let text = self.value(format!("load i64, ptr {at}"));
                Ok(Value { text, ty: I64 })
            }
            Expr::ListGet { list, index, elem, checked } => {
                let l = self.operand(list)?.text;
                let i = self.operand(index)?;
                let i = self.convert(i, &I64)?.text;
                let at = if *checked { self.index(&l, &i) } else { i };
                let data = self.list_data(&l);
                let memory = self.types.memory(elem);
                let slot = self.value(format!("getelementptr {memory}, ptr {data}, i64 {at}"));
                Ok(self.load_at(&slot, elem))
            }
            Expr::TupleNew { ty, items } => {
                let fields = self.types.fields_of(ty);
                let mut items_v = Vec::new();
                for (item, f) in items.iter().zip(&fields) {
                    let v = self.operand(item)?;
                    items_v.push(self.convert(v, f)?);
                }
                Ok(self.make_struct(ty, &items_v))
            }
            Expr::TupleGet { tuple, index } => {
                let t = self.operand(tuple)?;
                let fields = self.types.fields_of(&t.ty);
                let ty = fields.get(*index).cloned().unwrap_or(Ty::Unit);
                Ok(self.extract(&t, *index, &ty))
            }
            Expr::UnitVariant { ty, variant } => {
                Ok(Value { text: self.types.unit_variant(ty, *variant), ty: ty.clone() })
            }
            Expr::Field { value, ty, variant, index } => {
                let v = self.operand(value)?.text;
                let cell = self.types.cell(ty, *variant);
                let field = self.types.cell_fields(ty, *variant).get(*index).cloned().unwrap_or(Ty::Unit);
                let at = self.value(format!("getelementptr {cell}, ptr {v}, i32 0, i32 {}", index + 1));
                Ok(self.load_at(&at, &field))
            }
            Expr::Tag(value) => {
                let v = self.operand(value)?.text;
                let at = self.value(format!("getelementptr %lt_cell, ptr {v}, i32 0, i32 1"));
                let text = self.value(format!("load i32, ptr {at}"));
                Ok(Value { text, ty: Ty::Int(IntKind::U32) })
            }
            Expr::OptNew { ty, value } => {
                let inner = self.types.fields_of(ty).get(1).cloned().unwrap_or(Ty::Unit);
                let some = Value { text: value.is_some().to_string(), ty: Ty::Bool };
                let v = match value {
                    Some(v) => {
                        let v = self.operand(v)?;
                        self.convert(v, &inner)?
                    }
                    None => Value { text: zero(&self.types.memory(&inner)), ty: inner.clone() },
                };
                Ok(self.make_struct(ty, &[some, v]))
            }
            Expr::OptIsSome(v) | Expr::ResultIsOk(v) => {
                let v = self.operand(v)?;
                Ok(self.extract(&v, 0, &Ty::Bool))
            }
            Expr::OptValue(v) | Expr::ResultValue(v) => {
                let v = self.operand(v)?;
                let inner = self.types.fields_of(&v.ty).get(1).cloned().unwrap_or(Ty::Unit);
                Ok(self.extract(&v, 1, &inner))
            }
            Expr::ResultError(v) => {
                let v = self.operand(v)?;
                let error = self.types.fields_of(&v.ty).get(2).cloned().unwrap_or(Ty::Unit);
                Ok(self.extract(&v, 2, &error))
            }
            Expr::OptIf { ty, cond, value } => {
                let inner = self.types.fields_of(ty).get(1).cloned().unwrap_or(Ty::Unit);
                let c = self.operand(cond)?;
                let c = self.convert(c, &Ty::Bool)?;
                let v = self.operand(value)?;
                let v = self.convert(v, &inner)?;
                let some = self.make_struct(ty, &[Value { text: "true".into(), ty: Ty::Bool }, v]);
                let memory = self.types.memory(ty);
                let text =
                    self.value(format!("select i1 {}, {memory} {}, {memory} zeroinitializer", c.text, some.text));
                Ok(Value { text, ty: ty.clone() })
            }
            Expr::ResultNew { ty, ok, value } => {
                let fields = self.types.fields_of(ty);
                let (value_ty, error_ty) = (fields[1].clone(), fields[2].clone());
                let v = self.operand(value)?;
                let flag = Value { text: ok.to_string(), ty: Ty::Bool };
                let (value_v, error_v) = if *ok {
                    let v = self.convert(v, &value_ty)?;
                    (v, Value { text: zero(&self.types.memory(&error_ty)), ty: error_ty })
                } else {
                    let v = self.convert(v, &error_ty)?;
                    (Value { text: zero(&self.types.memory(&value_ty)), ty: value_ty }, v)
                };
                Ok(self.make_struct(ty, &[flag, value_v, error_v]))
            }
            Expr::ReadPlace(place) => {
                let (ty, slot) = self.place(place)?;
                Ok(self.load_at(&slot, &ty))
            }
            Expr::FnRef(name) => {
                Ok(Value { text: format!("@lt_fnref_{name}"), ty: Ty::Func(Vec::new(), Box::new(Ty::Unit)) })
            }
            Expr::CallC { symbol, args, params, ret } => {
                let mut passed = Vec::new();
                for (a, t) in args.iter().zip(params) {
                    match t {
                        Ty::Unit => {}
                        Ty::Str => {
                            let s = self.operand(a)?.text;
                            let bytes = self.value(format!("getelementptr i8, ptr {s}, i64 {STR_BYTES}"));
                            passed.push(format!("ptr {bytes}"));
                        }
                        Ty::Float(FloatKind::F32) => {
                            let v = self.operand(a)?;
                            let v = self.convert(v, &F64)?.text;
                            let narrow = self.value(format!("fptrunc double {v} to float"));
                            passed.push(format!("float {narrow}"));
                        }
                        _ => {
                            let v = self.operand(a)?;
                            let v = self.convert(v, t)?;
                            passed.push(format!("{} {}", ffi(t), v.text));
                        }
                    }
                }
                let ret_ffi = ffi_returned(ret);
                let call = format!("call {ret_ffi} @{symbol}({})", passed.join(", "));
                if ret_ffi == "void" {
                    self.emit(call);
                    return Ok(Value::unit());
                }
                let text = self.value(call);
                let ty = if matches!(ret, Ty::Float(FloatKind::F32)) {
                    let wide = self.value(format!("fpext float {text} to double"));
                    return Ok(Value { text: wide, ty: F64 });
                } else {
                    ret.clone()
                };
                Ok(Value { text, ty })
            }
            Expr::Parallel { tasks, result } => {
                let k = match self.results.iter().position(|t| t == result) {
                    Some(k) => k,
                    None => {
                        self.results.push(result.clone());
                        self.results.len() - 1
                    }
                };
                let t = self.operand(tasks)?.text;
                let desc = self.types.desc(result);
                let site = self.site();
                let list = self.runtime("lt_parallel", &[t, desc, format!("@lt_task{k}"), site]);
                Ok(Value { text: list.text, ty: Ty::List(Box::new(result.clone())) })
            }
            Expr::ToDyn { value, ty, vtable } => {
                let v = self.operand(value)?.text;
                let memory = self.types.memory(ty);
                let one = self.value(format!("insertvalue {memory} zeroinitializer, ptr {v}, 0"));
                let text = self.value(format!("insertvalue {memory} {one}, ptr @lt_vt{vtable}, 1"));
                Ok(Value { text, ty: ty.clone() })
            }
            Expr::CallDyn { receiver, ty, slot, args, params, ret } => {
                let r = self.operand(receiver)?;
                let memory = self.types.memory(ty);
                let cell = self.value(format!("extractvalue {memory} {}, 0", r.text));
                let vt = self.value(format!("extractvalue {memory} {}, 1", r.text));
                let Ty::Dyn(trait_name) = ty else { return self.refuse("a call through a value that is not `dyn`") };
                let at = self.value(format!("getelementptr %lt_vt_{trait_name}, ptr {vt}, i32 0, i32 1, i32 {slot}"));
                let f = self.value(format!("load ptr, ptr {at}"));
                self.call_through(&f, &cell, args, params, ret)
            }
            Expr::Capture { closure, lambda, index } => {
                let c = self.operand(closure)?.text;
                let captured = self.lowered.lambdas[*lambda].0[*index].clone();
                let at = self.value(format!("getelementptr %lt_c{lambda}, ptr {c}, i32 0, i32 {}", 4 + index));
                Ok(self.load_at(&at, &captured))
            }
            Expr::CallClosure { callee, args, ty } => {
                let c = self.operand(callee)?.text;
                let Ty::Func(params, ret) = ty else { return self.refuse("a call of a value that is not a function") };
                let at = self.value(format!("getelementptr %lt_closure, ptr {c}, i32 0, i32 1"));
                let f = self.value(format!("load ptr, ptr {at}"));
                self.call_through(&f, &c, args, params, ret)
            }
            Expr::CallPython { .. } => self.refuse("a call into a Python module"),
            Expr::Method { method, .. } => self.refuse(format!("the method `{method}` here")),
            Expr::CallGeneric { .. } | Expr::FnRefGeneric { .. } | Expr::ToDynOf { .. } => {
                self.refuse("a generic call `mono` did not resolve")
            }
            Expr::Format(_)
            | Expr::ListNew { .. }
            | Expr::DictNew { .. }
            | Expr::SetNew { .. }
            | Expr::Closure { .. }
            | Expr::Construct { .. } => self.refuse("this expression outside a `let`"),
        }
    }

    /// A struct of type `ty` holding `fields`, each already of its field's type.
    fn make_struct(&mut self, ty: &Ty, fields: &[Value]) -> Value {
        let memory = self.types.memory(ty);
        let field_types = self.types.fields_of(ty);
        let mut acc = "zeroinitializer".to_string();
        for (i, (v, f)) in fields.iter().zip(&field_types).enumerate() {
            let text = self.stored(v);
            let f_memory = self.types.memory(f);
            acc = self.value(format!("insertvalue {memory} {acc}, {f_memory} {text}, {i}"));
        }
        Value { text: if fields.is_empty() { "zeroinitializer".into() } else { acc }, ty: ty.clone() }
    }

    /// The field `index`, of type `ty`, of the struct value `v`.
    fn extract(&mut self, v: &Value, index: usize, ty: &Ty) -> Value {
        if is_unit(ty) {
            return Value::unit();
        }
        let memory = self.types.memory(&v.ty);
        let text = self.value(format!("extractvalue {memory} {}, {index}", v.text));
        self.loaded(text, ty)
    }

    // Text --------------------------------------------------------------------------------------

    fn put_literal(&mut self, buf: &str, text: &str) {
        if text.is_empty() {
            return;
        }
        let global = self.module.text(text.as_bytes());
        self.runtime("lt_buf_put", &[buf.to_string(), global, text.len().to_string()]);
    }

    /// A value of `ty` into the buffer `buf` as `print` and `str` write it.
    fn put_value(&mut self, buf: &str, value: &Operand, ty: &Ty) -> Result<(), Refusal> {
        if let Operand::Const(Const::Str(text)) = value {
            self.put_literal(buf, text);
            return Ok(());
        }
        if matches!(ty, Ty::Unit) {
            self.put_literal(buf, "None");
            return Ok(());
        }
        let v = self.operand(value)?;
        let b = buf.to_string();
        match ty {
            Ty::Int(IntKind::U64) => {
                let v = self.convert(v, &U64)?.text;
                self.runtime("lt_buf_u64", &[b, v]);
            }
            Ty::Int(_) => {
                let v = self.convert(v, &I64)?.text;
                self.runtime("lt_buf_i64", &[b, v]);
            }
            Ty::Float(_) => {
                let v = self.convert(v, &F64)?.text;
                self.runtime("lt_buf_f64", &[b, v]);
            }
            Ty::Bool => {
                let v = self.convert(v, &Ty::Bool)?.text;
                self.runtime("lt_buf_bool", &[b, v]);
            }
            Ty::Str => {
                self.runtime("lt_buf_str", &[b, v.text]);
            }
            _ => {
                let desc = self.types.desc(ty);
                let a = self.spill(v);
                self.runtime("lt_buf_value", &[b, desc, a]);
            }
        }
        Ok(())
    }

    fn repr_value(&mut self, buf: &str, value: &Operand, ty: &Ty) -> Result<(), Refusal> {
        if matches!(ty, Ty::Str) {
            let v = self.operand(value)?.text;
            self.runtime("lt_buf_str_repr", &[buf.to_string(), v]);
            return Ok(());
        }
        self.put_value(buf, value, ty)
    }

    fn new_buf(&mut self) -> String {
        let buf = self.slot("%lt_buf");
        self.emit(format!("store %lt_buf zeroinitializer, ptr {buf}"));
        buf
    }

    fn buf_parts(&mut self, buf: &str) -> (String, String) {
        let data_at = self.value(format!("getelementptr %lt_buf, ptr {buf}, i32 0, i32 0"));
        let data = self.value(format!("load ptr, ptr {data_at}"));
        let len_at = self.value(format!("getelementptr %lt_buf, ptr {buf}, i32 0, i32 1"));
        let len = self.value(format!("load i64, ptr {len_at}"));
        (data, len)
    }

    fn separator(&mut self, buf: &str, given: Option<&Operand>, default: &str) -> Result<(), Refusal> {
        match given {
            None => self.put_literal(buf, default),
            Some(Operand::Const(Const::Str(text))) => self.put_literal(buf, text),
            Some(other) => {
                let s = self.operand(other)?.text;
                self.runtime("lt_buf_str", &[buf.to_string(), s]);
            }
        }
        Ok(())
    }

    /// `print(args, sep=…, end=…)`: the text built in a buffer, then written in one piece.
    fn print(&mut self, args: &[(Operand, Ty)], sep: Option<&Operand>, end: Option<&Operand>) -> Result<(), Refusal> {
        let buf = self.new_buf();
        for (i, (value, ty)) in args.iter().enumerate() {
            if i > 0 {
                self.separator(&buf, sep, " ")?;
            }
            self.put_value(&buf, value, ty)?;
        }
        self.separator(&buf, end, "\n")?;
        let (data, len) = self.buf_parts(&buf);
        self.runtime("lt_write", &[data, len]);
        self.runtime("lt_buf_free", &[buf]);
        Ok(())
    }

    /// The spec of a format part as its bytes and length; a buffer to free once used, if built.
    fn spec(&mut self, spec: &[FormatPart]) -> Result<(String, String, Option<String>), Refusal> {
        if spec.iter().all(|p| matches!(p, FormatPart::Text(_))) {
            let text: String = spec.iter().map(|p| if let FormatPart::Text(t) = p { t.as_str() } else { "" }).collect();
            let global = self.module.text_z(&text);
            return Ok((global, text.len().to_string(), None));
        }
        let buf = self.new_buf();
        self.format_into(&buf, spec)?;
        let (data, len) = self.buf_parts(&buf);
        let empty = self.module.text_z("");
        let null = self.value(format!("icmp eq ptr {data}, null"));
        let bytes = self.value(format!("select i1 {null}, ptr {empty}, ptr {data}"));
        Ok((bytes, len, Some(buf)))
    }

    /// The parts of an f-string into the buffer `buf`.
    fn format_into(&mut self, buf: &str, parts: &[FormatPart]) -> Result<(), Refusal> {
        for part in parts {
            match part {
                FormatPart::Text(text) => self.put_literal(buf, text),
                FormatPart::Value { value, ty, conversion, spec } => {
                    let (spec_text, spec_len, spec_buf) = self.spec(spec)?;
                    let site = self.site();
                    match conversion {
                        Some(conversion) => {
                            let shown = self.new_buf();
                            if *conversion == 's' {
                                self.put_value(&shown, value, ty)?;
                            } else {
                                self.repr_value(&shown, value, ty)?;
                            }
                            if spec.is_empty() {
                                let (data, len) = self.buf_parts(&shown);
                                self.runtime("lt_buf_put", &[buf.to_string(), data, len]);
                                self.runtime("lt_buf_free", &[shown]);
                            } else {
                                let s = self.runtime("lt_str_from_buf", &[shown]).text;
                                self.runtime("lt_format_str", &[buf.to_string(), s.clone(), spec_text, spec_len, site]);
                                self.runtime("lt_str_drop", &[s]);
                            }
                        }
                        None => {
                            let b = buf.to_string();
                            match ty {
                                Ty::Int(IntKind::U64) => {
                                    let v = self.operand(value)?;
                                    let v = self.convert(v, &U64)?.text;
                                    self.runtime("lt_format_u64", &[b, v, spec_text, spec_len, site]);
                                }
                                Ty::Int(_) => {
                                    let v = self.operand(value)?;
                                    let v = self.convert(v, &I64)?.text;
                                    self.runtime("lt_format_i64", &[b, v, spec_text, spec_len, site]);
                                }
                                Ty::Float(_) => {
                                    let v = self.operand(value)?;
                                    let v = self.convert(v, &F64)?.text;
                                    self.runtime("lt_format_f64", &[b, v, spec_text, spec_len, site]);
                                }
                                Ty::Bool => {
                                    let v = self.operand(value)?;
                                    let v = self.convert(v, &Ty::Bool)?.text;
                                    self.runtime("lt_format_bool", &[b, v, spec_text, spec_len, site]);
                                }
                                Ty::Str => {
                                    let v = self.operand(value)?.text;
                                    self.runtime("lt_format_str", &[b, v, spec_text, spec_len, site]);
                                }
                                Ty::Unit => {
                                    self.runtime("lt_format_none", &[b, spec_text, spec_len, site]);
                                }
                                _ => {
                                    let desc = self.types.desc(ty);
                                    let a = self.address(value, ty)?;
                                    self.runtime("lt_format_value", &[b, desc, a, spec_text, spec_len, site]);
                                }
                            }
                        }
                    }
                    if let Some(spec_buf) = spec_buf {
                        self.runtime("lt_buf_free", &[spec_buf]);
                    }
                }
            }
        }
        Ok(())
    }

    // Counting ----------------------------------------------------------------------------------

    /// A count added to (`inc`) or taken from the value in the slot `slot`, of type `ty`.
    fn count(&mut self, slot: &str, ty: &Ty, inc: bool) {
        match ty {
            Ty::Str | Ty::List(_) | Ty::Heap(_) | Ty::Dict(..) | Ty::Set(_) | Ty::Adt(..) | Ty::Func(..) if inc => {
                let v = self.value(format!("load ptr, ptr {slot}"));
                let null = self.value(format!("icmp eq ptr {v}, null"));
                let (held, done) = (self.name("held"), self.name("counted"));
                self.terminate(format!("br i1 {null}, label %{done}, label %{held}"));
                self.start(&held);
                self.runtime("lt_inc", &[v]);
                self.label(&done);
            }
            Ty::Func(..) | Ty::Str | Ty::List(_) | Ty::Heap(_) | Ty::Dict(..) | Ty::Set(_) => {
                let v = self.value(format!("load ptr, ptr {slot}"));
                let name = match ty {
                    Ty::Func(..) => "lt_closure_drop",
                    Ty::Str => "lt_str_drop",
                    Ty::List(_) | Ty::Heap(_) => "lt_list_drop",
                    Ty::Dict(..) => "lt_dict_drop",
                    _ => "lt_set_drop",
                };
                self.runtime(name, &[v]);
            }
            _ if Types::counted(ty) => {
                let id = self.types.ids[ty];
                let which = if inc { "inc" } else { "dec" };
                self.emit(format!("call void @lt_{which}_t{id}(ptr {slot})"));
            }
            _ => {}
        }
    }

    // Statements --------------------------------------------------------------------------------

    /// A panic: the runtime function that stops the program, given the place.
    fn panic(&mut self, panic: &Panic) {
        let site = self.site();
        match panic {
            Panic::Todo => {
                self.runtime("lt_todo", &[site]);
            }
            Panic::Value(message) | Panic::Assert(message) => {
                let name = if matches!(panic, Panic::Value(_)) { "lt_value_error" } else { "lt_assert_failed" };
                let text = self.module.text_z(message);
                self.runtime(name, &[site, text]);
            }
        }
        if !self.ended {
            self.terminate("unreachable");
        }
    }

    fn let_special(&mut self, l: Local, e: &Expr) -> Result<bool, Refusal> {
        let target = format!("%l{l}");
        match e {
            Expr::Format(parts) => {
                let buf = self.new_buf();
                self.format_into(&buf, parts)?;
                let s = self.runtime("lt_str_from_buf", &[buf]);
                self.store(l, s)?;
            }
            Expr::Closure { lambda, captures, .. } => {
                let captured = self.lowered.lambdas[*lambda].0.clone();
                let mut fields = vec![Ty::Int(IntKind::U32), Ty::Int(IntKind::U32), Ty::Str, Ty::Str, Ty::Str];
                fields.extend(captured.iter().cloned());
                let size = self.types.struct_layout(&fields).0;
                let c = self.runtime("lt_alloc", &[size.to_string()]).text;
                let fn_at = self.value(format!("getelementptr %lt_c{lambda}, ptr {c}, i32 0, i32 1"));
                self.emit(format!("store ptr @{}, ptr {fn_at}", symbol::lambda(*lambda)));
                let drop_at = self.value(format!("getelementptr %lt_c{lambda}, ptr {c}, i32 0, i32 2"));
                self.emit(format!("store ptr @lt_drop_c{lambda}, ptr {drop_at}"));
                let share_at = self.value(format!("getelementptr %lt_c{lambda}, ptr {c}, i32 0, i32 3"));
                self.emit(format!("store ptr @lt_share_c{lambda}, ptr {share_at}"));
                for (i, (capture, ty)) in captures.iter().zip(&captured).enumerate() {
                    let v = self.operand(capture)?;
                    let at = self.value(format!("getelementptr %lt_c{lambda}, ptr {c}, i32 0, i32 {}", 4 + i));
                    self.store_at(&at, v, ty)?;
                }
                self.emit(format!("store ptr {c}, ptr {target}"));
            }
            Expr::DictNew { key, value, items } => {
                let (kd, vd) = (self.types.desc(key), self.types.desc(value));
                let d = self.runtime("lt_dict_new", &[kd, vd]);
                self.store(l, d)?;
                for (k, v) in items {
                    let (k, v) = (self.address(k, key)?, self.address(v, value)?);
                    self.runtime("lt_dict_set", &[target.clone(), k, v]);
                }
            }
            Expr::SetNew { elem, items, folded } => {
                let desc = self.types.desc(elem);
                let s = self.runtime("lt_set_new", &[desc]);
                self.store(l, s)?;
                for item in items {
                    let a = self.address(item, elem)?;
                    self.runtime("lt_set_add", &[target.clone(), a]);
                }
                if *folded {
                    let built = self.value(format!("load ptr, ptr {target}"));
                    let s = self.runtime("lt_set_folded", &[built]);
                    self.store(l, s)?;
                }
            }
            Expr::ListNew { elem, items } => {
                let desc = self.types.desc(elem);
                let list = self.runtime("lt_list_new", &[desc, items.len().to_string()]);
                self.store(l, list)?;
                for item in items {
                    let a = self.address(item, elem)?;
                    self.runtime("lt_list_push", &[target.clone(), a]);
                }
            }
            Expr::Construct { ty, variant, fields, reuse } => {
                let types = self.types.cell_fields(ty, *variant);
                let size = self.types.cell_size(&types);
                let cell = match reuse {
                    Some(n) => {
                        let token = self.value(format!("load ptr, ptr %tok{n}"));
                        let token_size = self.value(format!("load i64, ptr %toksz{n}"));
                        self.runtime("lt_reuse_or_alloc", &[token, token_size, size.to_string()]).text
                    }
                    None => self.runtime("lt_alloc", &[size.to_string()]).text,
                };
                let cell_ty = self.types.cell(ty, *variant);
                if let Some(k) = variant {
                    let aux = self.value(format!("getelementptr %lt_cell, ptr {cell}, i32 0, i32 1"));
                    self.emit(format!("store i32 {k}, ptr {aux}"));
                }
                for (i, (field, fty)) in fields.iter().zip(&types).enumerate() {
                    let v = self.operand(field)?;
                    let at = self.value(format!("getelementptr {cell_ty}, ptr {cell}, i32 0, i32 {}", i + 1));
                    self.store_at(&at, v, fty)?;
                }
                self.emit(format!("store ptr {cell}, ptr {target}"));
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn block(&mut self, block: &Block) -> Result<(), Refusal> {
        for stmt in block {
            self.span = stmt.span;
            match &stmt.kind {
                StmtKind::Let(l, e) => {
                    if !self.let_special(*l, e)? {
                        let v = self.expr(e)?;
                        if !self.ended {
                            self.store(*l, v)?;
                        }
                    }
                }
                StmtKind::Do(e) => {
                    self.expr(e)?;
                }
                StmtKind::Store(place, v) => {
                    let (ty, slot) = self.place(place)?;
                    let v = self.operand(v)?;
                    if Types::counted(&ty) {
                        let memory = self.types.memory(&ty);
                        let old = self.value(format!("load {memory}, ptr {slot}"));
                        self.store_at(&slot, v, &ty)?;
                        let held = self.slot(&memory);
                        self.emit(format!("store {memory} {old}, ptr {held}"));
                        self.count(&held, &ty, false);
                    } else {
                        self.store_at(&slot, v, &ty)?;
                    }
                }
                StmtKind::Mutate { op, place, args, at, result } => {
                    let slot = self.place(place)?.1;
                    let name = lotml_runtime::function(*op);
                    let sig = self.module.runtime(name);
                    let mut passed = vec![slot];
                    for (a, &p) in args.iter().zip(sig.params.iter().skip(1)) {
                        passed.push(self.runtime_arg(a, p)?);
                    }
                    if *at {
                        passed.push(self.site());
                    }
                    let r = self.runtime(name, &passed);
                    if let Some(r_local) = result {
                        self.store(*r_local, r)?;
                    }
                }
                StmtKind::DropReuse { local, token } => {
                    let ty = self.f.locals[*local].ty.clone();
                    let id = self.types.ids[&ty];
                    self.allocas.push(format!("  %tok{token} = alloca ptr"));
                    self.allocas.push(format!("  %toksz{token} = alloca i64"));
                    let t = self.value(format!("call ptr @lt_reuse_t{id}(ptr %l{local}, ptr %toksz{token})"));
                    self.emit(format!("store ptr {t}, ptr %tok{token}"));
                }
                StmtKind::Inc(l) | StmtKind::Dec(l) => {
                    let ty = self.f.locals[*l].ty.clone();
                    self.count(&format!("%l{l}"), &ty, matches!(stmt.kind, StmtKind::Inc(_)));
                }
                StmtKind::If(test, then, otherwise) => {
                    let test = self.operand(test)?;
                    let test = self.convert(test, &Ty::Bool)?.text;
                    let (yes, no, join) = (self.name("then"), self.name("else"), self.name("join"));
                    self.terminate(format!("br i1 {test}, label %{yes}, label %{no}"));
                    self.start(&yes);
                    self.block(then)?;
                    if !self.ended {
                        self.terminate(format!("br label %{join}"));
                    }
                    self.start(&no);
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
                    self.start(&out);
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
                        Some(Operand::Const(Const::Unit)) | None => {
                            let ty = self.types.value(&ret);
                            self.terminate(format!("ret {ty} {}", zero(&ty)));
                        }
                        Some(v) => {
                            let v = self.operand(v)?;
                            let v = self.convert(v, &ret)?;
                            let ty = self.types.value(&ret);
                            self.terminate(format!("ret {ty} {}", v.text));
                        }
                    }
                }
                StmtKind::ForRange { var, start, stop, step, body, exit } => {
                    self.for_range(*var, start, stop, step, body, exit)?;
                }
                StmtKind::ForStr { var, over, body, exit } => self.for_str(*var, over, body, exit)?,
                StmtKind::Panic(p) => self.panic(p),
            }
        }
        Ok(())
    }

    /// `for var in range(start, stop, step)`, its `exit` once the range is spent: no value past
    /// `stop`, and the step that would overflow ends the walk, as `lt_range_step` does. A step of
    /// one is a counter that cannot pass `stop`, so cannot overflow.
    fn for_range(
        &mut self,
        var: Local,
        start: &Operand,
        stop: &Operand,
        step: &Operand,
        body: &Block,
        exit: &Block,
    ) -> Result<(), Refusal> {
        let start = self.operand(start)?;
        let start = self.convert(start, &I64)?.text;
        let stop = self.operand(stop)?;
        let stop = self.convert(stop, &I64)?.text;
        let unit_step = matches!(step, Operand::Const(Const::Int(1, _)));
        let step = self.operand(step)?;
        let step = self.convert(step, &I64)?.text;
        let next = self.slot("i64");
        self.emit(format!("store i64 {start}, ptr {next}"));
        let (head, advance, spent, out) = (self.name("range"), self.name("step"), self.name("spent"), self.name("out"));
        if unit_step {
            self.label(&head);
            let at = self.value(format!("load i64, ptr {next}"));
            let past = self.value(format!("icmp sge i64 {at}, {stop}"));
            self.terminate(format!("br i1 {past}, label %{spent}, label %{advance}"));
            self.start(&advance);
            let after = self.value(format!("add i64 {at}, 1"));
            self.emit(format!("store i64 {after}, ptr {next}"));
            self.store(var, Value { text: at, ty: I64 })?;
        } else {
            let zero = self.value(format!("icmp eq i64 {step}, 0"));
            let site = self.site();
            let message = self.module.text_z("range() arg 3 must not be zero");
            self.module.runtime("lt_value_error");
            self.check(&zero, format!("call void @lt_value_error(ptr {site}, ptr {message})"));
            let done = self.slot("i1");
            self.emit(format!("store i1 false, ptr {done}"));
            self.label(&head);
            let finished = self.value(format!("load i1, ptr {done}"));
            let at = self.value(format!("load i64, ptr {next}"));
            let upward = self.value(format!("icmp sgt i64 {step}, 0"));
            let past_up = self.value(format!("icmp sge i64 {at}, {stop}"));
            let past_down = self.value(format!("icmp sle i64 {at}, {stop}"));
            let past = self.value(format!("select i1 {upward}, i1 {past_up}, i1 {past_down}"));
            let over = self.value(format!("or i1 {finished}, {past}"));
            self.terminate(format!("br i1 {over}, label %{spent}, label %{advance}"));
            self.start(&advance);
            self.module.declare("declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)");
            let pair = self.value(format!("call {{ i64, i1 }} @llvm.sadd.with.overflow.i64(i64 {at}, i64 {step})"));
            let advanced = self.value(format!("extractvalue {{ i64, i1 }} {pair}, 0"));
            let overflowed = self.value(format!("extractvalue {{ i64, i1 }} {pair}, 1"));
            self.emit(format!("store i64 {advanced}, ptr {next}"));
            self.emit(format!("store i1 {overflowed}, ptr {done}"));
            self.store(var, Value { text: at, ty: I64 })?;
        }
        self.loops.push(Loop { next: head.clone(), out: out.clone() });
        self.block(body)?;
        self.loops.pop();
        if !self.ended {
            self.terminate(format!("br label %{head}"));
        }
        self.start(&spent);
        self.block(exit)?;
        self.label(&out);
        Ok(())
    }

    /// `for var in over`: each character of the string as a string of its own.
    fn for_str(&mut self, var: Local, over: &Operand, body: &Block, exit: &Block) -> Result<(), Refusal> {
        let s = self.operand(over)?.text;
        let at = self.slot("i64");
        self.emit(format!("store i64 0, ptr {at}"));
        let (head, advance, spent, out) = (self.name("chars"), self.name("char"), self.name("spent"), self.name("out"));
        self.label(&head);
        let size_at = self.value(format!("getelementptr i8, ptr {s}, i64 {STR_SIZE}"));
        let size = self.value(format!("load i64, ptr {size_at}"));
        let here = self.value(format!("load i64, ptr {at}"));
        let past = self.value(format!("icmp sge i64 {here}, {size}"));
        self.terminate(format!("br i1 {past}, label %{spent}, label %{advance}"));
        self.start(&advance);
        let c = self.runtime("lt_str_char_at", &[s.clone(), at.clone()]);
        self.store(var, Value { text: c.text, ty: Ty::Str })?;
        self.loops.push(Loop { next: head.clone(), out: out.clone() });
        self.block(body)?;
        self.loops.pop();
        if !self.ended {
            self.terminate(format!("br label %{head}"));
        }
        self.start(&spent);
        self.block(exit)?;
        self.label(&out);
        Ok(())
    }
}

/// The type a value of `ty`'s elements are.
fn element(ty: &Ty) -> Ty {
    match ty {
        Ty::List(t) | Ty::Heap(t) | Ty::Set(t) => (**t).clone(),
        _ => Ty::Unit,
    }
}

fn dict_types(ty: &Ty) -> (Ty, Ty) {
    match ty {
        Ty::Dict(k, v) => ((**k).clone(), (**v).clone()),
        _ => (Ty::Unit, Ty::Unit),
    }
}

/// The LotML type a value of the C type `c` is, as the emitter holds it.
fn c_ty(c: CType) -> Ty {
    match c {
        CType::Void => Ty::Unit,
        CType::Bool => Ty::Bool,
        CType::Int { bits, signed } => Ty::Int(match (bits, signed) {
            (8, true) => IntKind::I8,
            (8, false) => IntKind::U8,
            (16, true) => IntKind::I16,
            (16, false) => IntKind::U16,
            (32, true) => IntKind::I32,
            (32, false) => IntKind::U32,
            (_, true) => IntKind::I64,
            (_, false) => IntKind::U64,
        }),
        CType::Double | CType::Float => F64,
        CType::Ptr | CType::Struct => Ty::Str,
    }
}

/// The zero of a memory type, as LLVM writes it.
fn zero(ty: &str) -> String {
    match ty {
        "double" => "0.0".into(),
        "ptr" => "null".into(),
        t if t.starts_with('i') && t[1..].chars().all(|c| c.is_ascii_digit()) => "0".into(),
        _ => "zeroinitializer".into(),
    }
}
