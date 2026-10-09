//! A module checked a part at a time: each function, method, record and test is checked against
//! the module's declarations on its own, and the parts' results are assembled into the module's
//! (specs/incremental-check/). The whole-file check is this, every part at its own place.

use lotml_diag::Diagnostic;
use lotml_syntax::ast::{FnDef, Item, Module, RecordDef, TestDef, TypeKind};
use lotml_syntax::shift::Shift;
use lotml_syntax::span::Span;

use crate::body::Body;
use crate::program::{FnSig, Program, TypeDef};
use crate::ty::Ty;
use crate::{Checked, Interfaces, MODULE_TYPES, interface};

/// What a part is, which with its name tells it from the module's other parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PartKind {
    Fn,
    Method,
    TraitMethod,
    Record,
    Test,
}

/// A part of a module that is checked on its own: a body, or a record's field defaults.
#[derive(Clone, Copy, Debug)]
pub enum Part<'a> {
    Fn(&'a FnDef),
    /// A method of an `impl` for the type named `owner`.
    Method {
        owner: &'a str,
        f: &'a FnDef,
    },
    /// A method of the trait named `owner`.
    TraitMethod {
        owner: &'a str,
        f: &'a FnDef,
    },
    Record(&'a RecordDef),
    Test(&'a TestDef),
}

impl Part<'_> {
    pub fn kind(&self) -> PartKind {
        match self {
            Part::Fn(_) => PartKind::Fn,
            Part::Method { .. } => PartKind::Method,
            Part::TraitMethod { .. } => PartKind::TraitMethod,
            Part::Record(_) => PartKind::Record,
            Part::Test(_) => PartKind::Test,
        }
    }

    /// The part's name: a method's is its owner's and its own, `Stack.push`.
    pub fn name(&self) -> String {
        match self {
            Part::Fn(f) => f.name.name.clone(),
            Part::Method { owner, f } | Part::TraitMethod { owner, f } => format!("{owner}.{}", f.name.name),
            Part::Record(r) => r.name.name.clone(),
            Part::Test(t) => t.name.clone(),
        }
    }

    pub fn span(&self) -> Span {
        match self {
            Part::Fn(f) | Part::Method { f, .. } | Part::TraitMethod { f, .. } => f.span,
            Part::Record(r) => r.span,
            Part::Test(t) => t.span,
        }
    }
}

/// The parts of a module, in source order: every function, each method of an `impl` of a named
/// type or of a trait, every record and every test.
pub fn parts(module: &Module) -> Vec<Part<'_>> {
    let mut found = Vec::new();
    for item in &module.items {
        match item {
            Item::Fn(f) => found.push(Part::Fn(f)),
            Item::Impl(imp) => {
                let TypeKind::Named { name, .. } = &imp.target.kind else { continue };
                found.extend(imp.methods.iter().map(|f| Part::Method { owner: &name.name, f }));
            }
            Item::Trait(t) => found.extend(t.methods.iter().map(|f| Part::TraitMethod { owner: &t.name.name, f })),
            Item::Record(r) => found.push(Part::Record(r)),
            Item::Test(t) => found.push(Part::Test(t)),
            Item::Sum(_) | Item::Import(_) | Item::Class(_) | Item::Error(_) => {}
        }
    }
    found
}

/// What a module declares, with the diagnostics found collecting it: every part is checked
/// against it.
#[derive(Clone, Debug, PartialEq)]
pub struct Declarations {
    program: Program,
}

/// The declarations of a module that may import the Python modules in `interfaces`.
pub fn declarations(module: &Module, interfaces: &Interfaces) -> Declarations {
    Declarations {
        program: Program::collect(
            module,
            &interface::functions(interfaces),
            &interface::classes(interfaces),
            &interface::marks(interfaces),
        ),
    }
}

impl Declarations {
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.program.diagnostics
    }

    /// What a part is checked against, equal across an edit inside a body: these declarations
    /// without their diagnostics, the spans of each signature moved to start at the function that
    /// declares it, and the signature's own span, which takes in the body, kept as that start.
    pub fn signatures(&self) -> Declarations {
        let mut program = Program { diagnostics: Vec::new(), ..self.program.clone() };
        let Program { functions, methods, traits, available, foreign, .. } = &mut program;
        let methods = methods.values_mut().chain(traits.values_mut()).flat_map(|m| m.values_mut()).map(|m| &mut m.sig);
        let imported = available.values_mut().chain(foreign.values_mut()).flat_map(|f| f.values_mut());
        for sig in functions.values_mut().chain(methods).chain(imported) {
            relative(sig);
        }
        Declarations { program }
    }
}

fn relative(sig: &mut FnSig) {
    let start = sig.span.start;
    for param in &mut sig.params {
        param.span.shift(start, 0);
    }
    sig.span = Span::default();
}

/// What checking one part found, every span relative to where the part's text starts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PartChecked {
    pub diagnostics: Vec<Diagnostic>,
    /// The type of each expression, in source order.
    pub types: Vec<(Span, Ty)>,
    /// Each name that resolved to a local, with the span of its declaration.
    pub locals: Vec<(Span, Span)>,
    /// The overload each overloaded Python call was given, by the call's span (adr:0035).
    pub overloads: Vec<(Span, usize)>,
}

impl PartChecked {
    fn of(mut body: Body) -> PartChecked {
        let types = body.types();
        let overloads = body.overloads().to_vec();
        PartChecked { types, locals: body.locals().to_vec(), overloads, diagnostics: body.diagnostics }
    }
}

/// Check one part against `declarations`, where `text` is the text its spans index.
pub fn check_part(declarations: &Declarations, part: Part<'_>, text: &str) -> PartChecked {
    let program = &declarations.program;
    let body = match part {
        Part::Fn(f) => function(program, text, f, &[], None, &[]),
        Part::Method { owner, f } => {
            let Some(def) = program.types.get(owner) else { return PartChecked::default() };
            let params = def.params().to_vec();
            let self_ty = Ty::Adt(owner.to_string(), params.iter().map(|p| Ty::Param(p.clone())).collect());
            let outer: Vec<(String, Option<String>)> = params.iter().map(|p| (p.clone(), None)).collect();
            function(program, text, f, &params, Some(&self_ty), &outer)
        }
        Part::TraitMethod { owner, f } => {
            let outer = vec![("Self".to_string(), Some(owner.to_string()))];
            function(program, text, f, &[], Some(&Ty::Param("Self".into())), &outer)
        }
        Part::Record(r) => {
            let mut body = Body::new(program, text, None, &[], false);
            if let Some(TypeDef::Record { fields, .. }) = program.types.get(&r.name.name) {
                for (field, sig) in r.fields.iter().zip(fields) {
                    if let Some(default) = &field.default {
                        let found = body.expr(default, Some(&sig.ty));
                        body.coerce(&found, &sig.ty, default.span);
                    }
                }
            }
            body
        }
        Part::Test(t) => {
            let mut body = Body::new(program, text, None, &[], true);
            body.block(&t.body);
            body
        }
    };
    PartChecked::of(body)
}

/// A signature already reported on when the program was collected, lowered again without
/// reporting: a duplicate declaration keeps its own signature.
fn quiet_signature(program: &Program, f: &FnDef, outer: &[String], self_ty: Option<&Ty>) -> FnSig {
    let mut scratch = Program {
        types: program.types.clone(),
        traits: program.traits.clone(),
        py_classes: program.py_classes.clone(),
        ..Program::default()
    };
    scratch.signature(f, outer, self_ty)
}

fn function<'p>(
    program: &'p Program,
    text: &'p str,
    f: &FnDef,
    owner_params: &[String],
    self_ty: Option<&Ty>,
    outer: &[(String, Option<String>)],
) -> Body<'p> {
    let sig = quiet_signature(program, f, owner_params, self_ty);
    let mut body = Body::new(program, text, Some(&sig), outer, false);
    for (param, param_sig) in f.params.iter().zip(&sig.params) {
        if let Some(default) = &param.default {
            let found = body.expr(default, Some(&param_sig.ty));
            body.coerce(&found, &param_sig.ty, default.span);
        }
    }
    if let Some(block) = &f.body {
        body.function_body(block, &sig);
    }
    body
}

/// A module's [`Checked`], assembled from its parts' checks in source order.
pub struct Assembly {
    checked: Checked,
    keep: bool,
}

impl Assembly {
    /// An assembly starting from the diagnostics of `declarations`. Unless `keep`, only the
    /// diagnostics are assembled: the types are counted against the module's limit, not kept.
    pub fn new(declarations: &Declarations, keep: bool) -> Assembly {
        Assembly { checked: Checked { diagnostics: declarations.diagnostics().to_vec(), ..Checked::default() }, keep }
    }

    /// Add the check of the next part, its spans moved from the part's start to `at`.
    pub fn absorb(&mut self, part: &PartChecked, at: u32) {
        let checked = &mut self.checked;
        for (span, ty) in &part.types {
            let span = span.shifted(0, at);
            let before = checked.stored;
            checked.stored = checked.stored.saturating_add(ty.size());
            if checked.stored <= MODULE_TYPES {
                if self.keep {
                    checked.types.insert(span, ty.clone());
                }
                continue;
            }
            if before <= MODULE_TYPES {
                checked.diagnostics.push(
                    Diagnostic::error(
                        "E0222",
                        span,
                        format!("the types of this module have more than {MODULE_TYPES} parts in all"),
                    )
                    .note("name the shape with a record type, or keep the values in a list"),
                );
            }
            if self.keep {
                checked.types.insert(span, Ty::Error);
            }
        }
        if self.keep {
            checked.locals.extend(part.locals.iter().map(|(name, local)| (name.shifted(0, at), local.shifted(0, at))));
        }
        checked.py_overloads.extend(part.overloads.iter().map(|(call, chosen)| (call.shifted(0, at), *chosen)));
        checked.diagnostics.extend(part.diagnostics.iter().map(|d| {
            let mut d = d.clone();
            d.shift(0, at);
            d
        }));
    }

    /// The diagnostics assembled, in the order they were found.
    pub fn diagnostics(self) -> Vec<Diagnostic> {
        self.checked.diagnostics
    }

    /// The module's `Checked`: what was assembled, with what the module declares.
    pub fn finish(self, module: &Module, declarations: &Declarations) -> Checked {
        let Assembly { mut checked, .. } = self;
        let program = &declarations.program;
        let declared_here = |name: &String| {
            module.items.iter().any(|item| match item {
                Item::Fn(f) => f.name.name == *name,
                Item::Record(r) => r.name.name == *name,
                Item::Sum(s) => s.name.name == *name,
                _ => false,
            })
        };
        checked.functions =
            program.functions.iter().filter(|(n, _)| declared_here(n)).map(|(n, s)| (n.clone(), s.clone())).collect();
        checked.declared =
            program.types.iter().filter(|(n, _)| declared_here(n)).map(|(n, t)| (n.clone(), t.clone())).collect();
        checked.methods = program
            .methods
            .iter()
            .filter(|(n, _)| checked.declared.contains_key(*n))
            .map(|(n, m)| (n.clone(), m.clone()))
            .collect();
        checked.traits = program.traits.clone();
        checked.foreign = program.foreign.clone();
        checked.py_classes = program.py_classes.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        for class in program.py_attributes.keys() {
            let own = program.methods.get(class).cloned().unwrap_or_default();
            checked.py_methods.insert(class.clone(), own);
        }
        checked.py_bases = program.py_bases.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        checked.py_constructors = program.py_constructors.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        checked
    }
}
