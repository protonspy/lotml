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
#[derive(Clone, Debug, Default)]
pub struct Interface {
    pub(crate) functions: BTreeMap<String, FnSig>,
}

impl Interface {
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.functions.keys().map(String::as_str)
    }
}

/// The interfaces a file may import from, by module name.
pub type Interfaces = HashMap<String, Interface>;

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
    (Interface { functions }, diagnostics)
}

/// The functions of each interface, as the checker keeps them.
pub(crate) fn functions(interfaces: &Interfaces) -> HashMap<String, BTreeMap<String, FnSig>> {
    interfaces.iter().map(|(module, i)| (module.clone(), i.functions.clone())).collect()
}
