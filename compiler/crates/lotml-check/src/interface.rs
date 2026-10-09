//! Interfaces: the bindings `lotml bind` writes for a Python module (adr:0012). An interface is a
//! file of function signatures with no body, over the types every program has, each returning
//! `T ! PyError` — a stub says nothing about what a Python call raises, so every one can fail.

use std::collections::{BTreeMap, HashMap, HashSet};

use lotml_diag::Diagnostic;
use lotml_syntax::ast::{ClassDef, Item};
use lotml_syntax::parse_interface;
use lotml_syntax::span::Span;

use crate::program::{FnSig, Method, Program};
use crate::ty::Ty;

/// A Python module's functions and classes, as its interface declares them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Interface {
    pub(crate) functions: BTreeMap<String, FnSig>,
    /// Its classes by the name the module gives them (adr:0034).
    pub(crate) classes: BTreeMap<String, PyClass>,
    /// What the compiler said of it on its first line, for the checker to warn at the import.
    pub(crate) mark: Mark,
}

/// A Python class an interface declares, its types written by their module (`py.datetime.date`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PyClass {
    /// Its bases among the interface's classes, by their full names.
    pub bases: Vec<String>,
    pub attributes: BTreeMap<String, Ty>,
    pub constructor: Option<FnSig>,
    /// Its methods (with a receiver) and static methods (without).
    pub methods: BTreeMap<String, Method>,
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

    /// The signature of its function `name`, its overloads after the first in it.
    pub fn function(&self, name: &str) -> Option<&FnSig> {
        self.functions.get(name)
    }

    /// The classes it declares, by the name the module gives them.
    pub fn classes(&self) -> impl Iterator<Item = (&str, &PyClass)> {
        self.classes.iter().map(|(name, class)| (name.as_str(), class))
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
    if is_c_library(module) { c_interface(text) } else { python_interface(module, text) }
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
    (Interface { functions, classes: BTreeMap::new(), mark: Mark::None }, diagnostics)
}

/// Read an interface: the functions and classes it declares, and what is wrong with it. A
/// declaration that is wrong is left out, so the rest stay usable. With no module named, a class's
/// type is written `py._.<Class>`; [`interface_of`] writes it by the module it is read for.
pub fn interface(text: &str) -> (Interface, Vec<Diagnostic>) {
    python_interface("py._", text)
}

/// A Python interface read for `module` (`py.datetime`): each class it declares is the type
/// `py.datetime.<Class>` (adr:0034).
fn python_interface(module: &str, text: &str) -> (Interface, Vec<Diagnostic>) {
    let parsed = parse_interface(text);
    let mut diagnostics: Vec<Diagnostic> = parsed.errors.iter().map(crate::syntax).collect();
    let mut program = Program::with_prelude();
    for item in &parsed.module.items {
        if let Item::Class(c) = item
            && program.py_classes.insert(c.name.name.clone(), format!("{module}.{}", c.name.name)).is_some()
        {
            diagnostics.push(Diagnostic::error("E0210", c.name.span, format!("`{}` is declared twice", c.name.name)));
        }
    }
    let mut functions: BTreeMap<String, FnSig> = BTreeMap::new();
    let mut classes = BTreeMap::new();
    let mut closed = HashSet::new();
    for item in &parsed.module.items {
        let f = match item {
            Item::Fn(f) => f,
            Item::Class(c) => {
                let class = python_class(c, &mut program, &mut diagnostics);
                classes.insert(c.name.name.clone(), class);
                continue;
            }
            Item::Error(_) => continue,
            other => {
                diagnostics.push(Diagnostic::error(
                    "E0221",
                    other.span(),
                    "an interface declares Python functions and classes: signatures, nothing else",
                ));
                continue;
            }
        };
        if closed.contains(&f.name.name) {
            continue;
        }
        let Some(sig) = python_signature(f, None, &mut program, &mut diagnostics) else {
            closed.insert(f.name.name.clone());
            continue;
        };
        if classes.contains_key(&f.name.name) {
            diagnostics.push(Diagnostic::error("E0210", f.name.span, format!("`{}` is declared twice", f.name.name)));
            continue;
        }
        match functions.get_mut(&f.name.name) {
            Some(first) => overload(first, sig, f.name.span, &mut closed, &mut diagnostics),
            None => {
                functions.insert(f.name.name.clone(), sig);
            }
        }
    }
    diagnostics.append(&mut program.diagnostics);
    diagnostics.sort_by_key(|d| d.span.start);
    (Interface { functions, classes, mark: mark_of(text) }, diagnostics)
}

/// The signature of `f`, a function of the interface or a member of a class whose type is
/// `self_ty`, or `None` with the reason reported: it has a body, type parameters, or does not
/// fail with `PyError`.
fn python_signature(
    f: &lotml_syntax::ast::FnDef,
    self_ty: Option<&Ty>,
    program: &mut Program,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<FnSig> {
    if f.body.is_some() {
        diagnostics.push(Diagnostic::error(
            "E0221",
            f.name.span,
            format!("`{}` has a body: an interface declares only its signature", f.name.name),
        ));
        return None;
    }
    if !f.type_params.is_empty() {
        diagnostics.push(Diagnostic::error(
            "E0221",
            f.name.span,
            format!("`{}` has type parameters, which a Python function's binding cannot check", f.name.name),
        ));
        return None;
    }
    let before = program.diagnostics.len();
    let sig = program.signature(f, &[], self_ty);
    if program.diagnostics.len() > before {
        return None;
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
        return None;
    }
    Some(sig)
}

/// A class block read: its bases among the interface's classes, its attributes, its constructor
/// (the function named after it), its methods (taking `self`) and its static methods.
fn python_class(c: &ClassDef, program: &mut Program, diagnostics: &mut Vec<Diagnostic>) -> PyClass {
    let own = Ty::Adt(program.py_classes[&c.name.name].clone(), vec![]);
    let mut class = PyClass::default();
    for base in &c.bases {
        match program.py_classes.get(&base.name) {
            Some(qualified) => class.bases.push(qualified.clone()),
            None => diagnostics.push(Diagnostic::error(
                "E0221",
                base.span,
                format!("`{}` is no class this interface declares, so it cannot be a base", base.name),
            )),
        }
    }
    for attribute in &c.attributes {
        let Some(name) = &attribute.name else { continue };
        let before = program.diagnostics.len();
        let ty = program.lower(&attribute.ty, &[]);
        if program.diagnostics.len() == before && class.attributes.insert(name.name.clone(), ty).is_some() {
            diagnostics.push(Diagnostic::error("E0210", name.span, format!("`{}` is declared twice", name.name)));
        }
    }
    let mut closed = HashSet::new();
    for m in &c.methods {
        if closed.contains(&m.name.name) {
            continue;
        }
        let has_self = m.params.first().is_some_and(|p| p.name.name == "self");
        let Some(sig) = python_signature(m, has_self.then_some(&own), program, diagnostics) else {
            closed.insert(m.name.name.clone());
            continue;
        };
        if m.name.name == c.name.name {
            if has_self || sig.ret != own {
                diagnostics.push(Diagnostic::error(
                    "E0221",
                    m.name.span,
                    format!("the constructor `{}` takes no `self` and returns `{}`", m.name.name, c.name.name),
                ));
                closed.insert(m.name.name.clone());
                continue;
            }
            match &mut class.constructor {
                Some(first) => overload(first, sig, m.name.span, &mut closed, diagnostics),
                None => class.constructor = Some(sig),
            }
            continue;
        }
        let receiver = has_self.then(|| m.params[0].convention);
        if class.attributes.contains_key(&m.name.name) {
            diagnostics.push(Diagnostic::error("E0210", m.name.span, format!("`{}` is declared twice", m.name.name)));
            continue;
        }
        match class.methods.get_mut(&m.name.name) {
            Some(first) if first.receiver.is_some() != receiver.is_some() => {
                diagnostics.push(Diagnostic::error(
                    "E0221",
                    m.name.span,
                    format!(
                        "an overload of `{}` takes `self` where its first does not, or the other way round",
                        m.name.name
                    ),
                ));
                closed.insert(m.name.name.clone());
            }
            Some(first) => overload(&mut first.sig, sig, m.name.span, &mut closed, diagnostics),
            None => {
                class.methods.insert(m.name.name.clone(), Method { sig, receiver, owner_params: Vec::new() });
            }
        }
    }
    class
}

/// The most overloads one name keeps (adr:0035), past the 43 of the largest set the binding
/// coverage corpus declares, so a stub cannot make every call try thousands.
pub const OVERLOADS: usize = 64;

/// `sig`, declared again under `first`'s name, kept as its next overload; past [`OVERLOADS`], it
/// and every later one are left out and the name is closed.
fn overload(first: &mut FnSig, sig: FnSig, at: Span, closed: &mut HashSet<String>, diagnostics: &mut Vec<Diagnostic>) {
    if first.overloads.len() + 1 < OVERLOADS {
        first.overloads.push(sig);
        return;
    }
    diagnostics.push(Diagnostic::error(
        "E0221",
        at,
        format!("`{}` has more than {OVERLOADS} overloads, which lotml does not try", sig.name),
    ));
    closed.insert(sig.name);
}

/// The modules whose interface carries a mark, with it.
pub(crate) fn marks(interfaces: &Interfaces) -> HashMap<String, Mark> {
    interfaces.iter().filter(|(_, i)| i.mark != Mark::None).map(|(module, i)| (module.clone(), i.mark)).collect()
}

/// `class` and the bases it declares, through theirs, nearest first and each once: the order a
/// member is looked up in (adr:0034). `bases` gives a class's bases by full name; a cycle a hostile
/// interface writes ends where it repeats.
pub fn py_lineage<'a>(class: &str, bases: impl Fn(&str) -> Option<&'a [String]>) -> Vec<String> {
    let mut found = vec![class.to_string()];
    let mut seen: std::collections::HashSet<String> = found.iter().cloned().collect();
    let mut at = 0;
    while at < found.len() {
        for base in bases(&found[at]).into_iter().flatten() {
            if seen.insert(base.clone()) {
                found.push(base.clone());
            }
        }
        at += 1;
    }
    found
}

/// The constructor of the Python class `class`, by its full name: its own, else the nearest one a
/// base declares, returning `class` (adr:0034). It is made when a program calls it, never kept for
/// every subclass, so a module of many subclasses of one wide base costs what it is.
pub fn py_constructor<'a>(
    class: &str,
    constructors: impl Fn(&str) -> Option<&'a FnSig>,
    bases: impl Fn(&str) -> Option<&'a [String]>,
) -> Option<FnSig> {
    let found = py_lineage(class, bases).into_iter().find_map(|c| constructors(&c))?;
    let mut sig = found.clone();
    let name = class.rsplit('.').next().unwrap_or(class);
    let mut overloads = std::mem::take(&mut sig.overloads);
    for each in std::iter::once(&mut sig).chain(&mut overloads) {
        each.ret = Ty::Adt(class.to_string(), Vec::new());
        each.name = name.to_string();
    }
    sig.overloads = overloads;
    Some(sig)
}

/// The classes of each interface, by the name its module gives them.
pub(crate) fn classes(interfaces: &Interfaces) -> HashMap<String, BTreeMap<String, PyClass>> {
    interfaces
        .iter()
        .filter(|(_, i)| !i.classes.is_empty())
        .map(|(module, i)| (module.clone(), i.classes.clone()))
        .collect()
}

/// The functions of each interface, as the checker keeps them.
pub(crate) fn functions(interfaces: &Interfaces) -> HashMap<String, BTreeMap<String, FnSig>> {
    interfaces.iter().map(|(module, i)| (module.clone(), i.functions.clone())).collect()
}
