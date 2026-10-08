//! Interfaces: the bindings `lotml bind` writes for a Python module (adr:0012). An interface is a
//! file of function signatures with no body, over the types every program has, each returning
//! `T ! PyError` — a stub says nothing about what a Python call raises, so every one can fail.

use std::collections::{BTreeMap, HashMap};

use lotml_diag::Diagnostic;
use lotml_syntax::ast::Item;
use lotml_syntax::parse_interface;

use crate::program::{FnSig, Program};
use crate::ty::Ty;

/// A Python module's functions, as its interface declares them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Interface {
    pub(crate) functions: BTreeMap<String, FnSig>,
    /// What the compiler said of it on its first line, for the checker to warn at the import.
    pub(crate) mark: Mark,
}

/// What the compiler says of an interface on its first line (specs/bind-on-import/ R1.6, R2.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mark {
    #[default]
    None,
    /// A bindings file, given where the module's stub would generate an interface ([`shadowing`]).
    Shadows,
    /// Generated from a stub that differs from the one `lotml.lock` records ([`unlocked`]).
    Unlocked,
}

impl Interface {
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.functions.keys().map(String::as_str)
    }
}

/// The first line the compiler puts before a bindings file's text when the module's stub would
/// give an interface of its own (specs/bind-on-import/ R1.6).
const SHADOWS: &str = "# This file shadows the interface lotml generates from the module's stub.";

/// `text`, a bindings file's, marked as shadowing the interface the compiler would generate.
pub fn shadowing(text: &str) -> String {
    format!("{SHADOWS}\n{text}")
}

/// The first line the compiler puts before an interface generated from a stub that differs from
/// the one `lotml.lock` records.
const UNLOCKED: &str = "# The module's stub differs from the one lotml.lock records.";

/// `text`, a generated interface's, marked as bound from a stub the lock does not record.
pub fn unlocked(text: &str) -> String {
    format!("{UNLOCKED}\n{text}")
}

/// The mark on `text`'s first line.
fn mark_of(text: &str) -> Mark {
    match text.lines().next() {
        Some(SHADOWS) => Mark::Shadows,
        Some(UNLOCKED) => Mark::Unlocked,
        _ => Mark::None,
    }
}

/// The interfaces a file may import from, by module name.
pub type Interfaces = HashMap<String, Interface>;

/// Whether a module is a C library, which an interface named `c.<library>` binds (adr:0013).
pub fn is_c_library(module: &str) -> bool {
    module.starts_with("c.") && module.len() > 2
}

/// Read the interface of `module`: a C library's when it is named `c.<library>`, a Python
/// module's otherwise.
pub fn interface_of(module: &str, text: &str) -> (Interface, Vec<Diagnostic>) {
    if is_c_library(module) { c_interface(text) } else { interface(text) }
}

/// The types C and lotml pass by value: what a C function may take, and what it may return.
fn c_type(ty: &Ty, parameter: bool) -> bool {
    match ty {
        Ty::Int(_) | Ty::Float(_) | Ty::Bool => true,
        Ty::Str => parameter,
        Ty::Unit => !parameter,
        _ => false,
    }
}

/// Read a C library's interface: signatures over the types C passes by value, none of which
/// can fail, since C reports nothing a call raises.
pub fn c_interface(text: &str) -> (Interface, Vec<Diagnostic>) {
    let parsed = parse_interface(text);
    let mut diagnostics: Vec<Diagnostic> = parsed.errors.iter().map(crate::syntax).collect();
    let mut program = Program::with_prelude();
    let mut functions = BTreeMap::new();
    for item in &parsed.module.items {
        let f = match item {
            Item::Fn(f) => f,
            Item::Error(_) => continue,
            other => {
                diagnostics.push(Diagnostic::error(
                    "E0221",
                    other.span(),
                    "a C library's interface declares functions: signatures, nothing else",
                ));
                continue;
            }
        };
        let refuse = |why: String| Diagnostic::error("E0221", f.name.span, why);
        if f.body.is_some() || !f.type_params.is_empty() {
            diagnostics.push(refuse(format!(
                "`{}` is a C function: a signature, with no body or type parameters",
                f.name.name
            )));
            continue;
        }
        if f.params.iter().any(|p| p.default.is_some() || p.convention != lotml_syntax::ast::Convention::Default) {
            diagnostics.push(refuse(format!(
                "`{}` is a C function: its parameters take values, with no default",
                f.name.name
            )));
            continue;
        }
        let before = program.diagnostics.len();
        let sig = program.signature(f, &[], None);
        if program.diagnostics.len() > before {
            continue;
        }
        if sig.error.is_some() {
            diagnostics.push(
                refuse(format!("`{}` is a C function, which cannot fail: write its result alone", f.name.name))
                    .note("C reports nothing a call raises; a missing library stops the program when it loads"),
            );
            continue;
        }
        let wrong = sig.params.iter().find(|p| !c_type(&p.ty, true)).map(|p| (p.name.clone(), p.ty.clone()));
        if let Some((name, ty)) = wrong {
            diagnostics.push(
                refuse(format!("`{name}` is a `{ty}`, which C does not take by value"))
                    .note("a C function takes integers, `f32`, `f64`, `bool` and `str`"),
            );
            continue;
        }
        if !c_type(&sig.ret, false) {
            diagnostics.push(
                refuse(format!("`{}` returns a `{}`, which C does not return by value", f.name.name, sig.ret))
                    .note("a C function returns an integer, `f32`, `f64`, `bool` or nothing"),
            );
            continue;
        }
        if functions.insert(f.name.name.clone(), sig).is_some() {
            diagnostics.push(Diagnostic::error("E0210", f.name.span, format!("`{}` is declared twice", f.name.name)));
        }
    }
    diagnostics.append(&mut program.diagnostics);
    diagnostics.sort_by_key(|d| d.span.start);
    (Interface { functions, mark: Mark::None }, diagnostics)
}

/// Read an interface: the functions it declares, and what is wrong with it. A function that
/// is wrong is left out, so the rest stay usable.
pub fn interface(text: &str) -> (Interface, Vec<Diagnostic>) {
    let parsed = parse_interface(text);
    let mut diagnostics: Vec<Diagnostic> = parsed.errors.iter().map(crate::syntax).collect();
    let mut program = Program::with_prelude();
    let mut functions = BTreeMap::new();
    for item in &parsed.module.items {
        let f = match item {
            Item::Fn(f) => f,
            Item::Error(_) => continue,
            other => {
                diagnostics.push(Diagnostic::error(
                    "E0221",
                    other.span(),
                    "an interface declares Python functions: signatures, nothing else",
                ));
                continue;
            }
        };
        if f.body.is_some() {
            diagnostics.push(Diagnostic::error(
                "E0221",
                f.name.span,
                format!("`{}` has a body: an interface declares only its signature", f.name.name),
            ));
            continue;
        }
        if !f.type_params.is_empty() {
            diagnostics.push(Diagnostic::error(
                "E0221",
                f.name.span,
                format!("`{}` has type parameters, which a Python function's binding cannot check", f.name.name),
            ));
            continue;
        }
        let before = program.diagnostics.len();
        let sig = program.signature(f, &[], None);
        if program.diagnostics.len() > before {
            continue;
        }
        if sig.error != Some(Ty::Adt("PyError".into(), vec![])) {
            diagnostics.push(
                Diagnostic::error(
                    "E0221",
                    f.name.span,
                    format!("`{}` must return `T ! PyError`: any call into Python can fail", f.name.name),
                )
                .note("write `-> None ! PyError` for a function with no value to return"),
            );
            continue;
        }
        if functions.insert(f.name.name.clone(), sig).is_some() {
            diagnostics.push(Diagnostic::error("E0210", f.name.span, format!("`{}` is declared twice", f.name.name)));
        }
    }
    diagnostics.append(&mut program.diagnostics);
    diagnostics.sort_by_key(|d| d.span.start);
    (Interface { functions, mark: mark_of(text) }, diagnostics)
}

/// The modules whose interface carries a mark, with it.
pub(crate) fn marks(interfaces: &Interfaces) -> HashMap<String, Mark> {
    interfaces.iter().filter(|(_, i)| i.mark != Mark::None).map(|(module, i)| (module.clone(), i.mark)).collect()
}

/// The functions of each interface, as the checker keeps them.
pub(crate) fn functions(interfaces: &Interfaces) -> HashMap<String, BTreeMap<String, FnSig>> {
    interfaces.iter().map(|(module, i)| (module.clone(), i.functions.clone())).collect()
}
