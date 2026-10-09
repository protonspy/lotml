//! Checking one function body, test block or method: expressions with an expected type
//! where there is one (bidirectional), locals inferred, optionals narrowed by `is not None`.

use std::collections::{BTreeSet, HashMap};

use lotml_diag::{Applicability, Diagnostic};
use lotml_syntax::ast::*;
use lotml_syntax::span::Span;

use crate::builtins;
use crate::closest;
use crate::program::{FnSig, Method, Program, TypeDef};
use crate::ty::{F64, INT, Infer, IntKind, TYPE_LIMIT, Ty};

/// The most type nodes one body keeps for its expressions, in all: each keeps a whole tree, so
/// many mentions of a type under [`TYPE_LIMIT`] add up.
const BODY_TYPES: usize = 1 << 19;

/// The note every operation refused on a `PyObject` carries (specs/python-object R1.2).
const OPAQUE: &str = "a `PyObject` is opaque: take a LotML value out of it with `o.value()`, its type annotated";

/// The note an operation refused on a Python class's value carries (specs/python-classes R2.5).
const CLASS: &str = "a Python object has only the methods and attributes its interface declares";

#[derive(Clone, Debug)]
pub struct Local {
    pub ty: Ty,
    pub mutable: bool,
    pub span: Span,
    /// Where `var ` would go to make it mutable, when it was declared by `x = …`.
    pub declared_at: Option<u32>,
    pub moved: bool,
    pub origin: Origin,
}

/// How a local came to be, which decides what to say when it is changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// `x = …` or `var x = …`.
    Declared,
    /// A parameter of the function.
    Param,
    /// Bound by `for`, a `case` pattern or a lambda's parameter.
    Bound,
    /// Seen from inside a lambda, which holds a copy.
    Captured,
}

#[derive(Default, Clone)]
struct Scope {
    locals: HashMap<String, Local>,
    narrowed: HashMap<String, Ty>,
}

pub struct Body<'p> {
    pub program: &'p Program,
    pub text: &'p str,
    pub infer: Infer,
    scopes: Vec<Scope>,
    ret: Ty,
    error: Option<Ty>,
    in_test: bool,
    type_params: HashMap<String, Option<String>>,
    /// Parameters taken by copy: a `var` copy of one that is changed and dropped is reported.
    params: HashMap<String, Convention>,
    /// `var copy = param` declarations: the copy, the parameter, and where.
    copies: Vec<(String, String, Span)>,
    /// The locals changed in place or through a field or element.
    mutated: BTreeSet<String>,
    /// The names returned as they are: `return xs`.
    returned: BTreeSet<String>,
    /// The type of every expression checked, by its span; resolved by `types`.
    seen: HashMap<Span, Ty>,
    /// Which expression each span of `seen` was recorded from, by address, and whether it stands
    /// for text the parser could not read. Keying types by span is sound only while no two
    /// expressions share one, which a debug build checks; two such error nodes at one place,
    /// both typed as errors, are the exception.
    #[cfg(debug_assertions)]
    typed_by: HashMap<Span, (usize, bool)>,
    /// How many type nodes `seen` holds, up to [`BODY_TYPES`]: past it, an expression keeps an
    /// error, and the first one past it is reported.
    stored: usize,
    /// Whether the types have passed [`BODY_TYPES`], and that is reported.
    full: bool,
    /// Each name that resolved to a local, with where that local was declared; a declaration
    /// refers to itself.
    locals: Vec<(Span, Span)>,
    /// The overload each overloaded Python call was given, by the call's span (adr:0035).
    overloads: Vec<(Span, usize)>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Facts a condition establishes: paths that are known not to be `None` (or to be).
#[derive(Default, Clone)]
struct Facts {
    then: Vec<(String, Ty)>,
    otherwise: Vec<(String, Ty)>,
}

impl<'p> Body<'p> {
    /// `outer` are the type parameters in scope from around the function, each with its bound:
    /// an impl's `T`, or a trait's `Self`.
    pub fn new(
        program: &'p Program,
        text: &'p str,
        sig: Option<&FnSig>,
        outer: &[(String, Option<String>)],
        in_test: bool,
    ) -> Body<'p> {
        let mut type_params: HashMap<String, Option<String>> = outer.iter().cloned().collect();
        let mut body = Body {
            program,
            text,
            infer: Infer::default(),
            scopes: vec![Scope::default()],
            ret: Ty::Unit,
            error: None,
            in_test,
            type_params: HashMap::new(),
            params: HashMap::new(),
            copies: Vec::new(),
            mutated: BTreeSet::new(),
            returned: BTreeSet::new(),
            seen: HashMap::new(),
            #[cfg(debug_assertions)]
            typed_by: HashMap::new(),
            stored: 0,
            full: false,
            locals: Vec::new(),
            overloads: Vec::new(),
            diagnostics: Vec::new(),
        };
        if let Some(sig) = sig {
            for (name, bound) in &sig.type_params {
                type_params.insert(name.clone(), bound.clone());
            }
            body.ret = sig.ret.clone();
            body.error = sig.error.clone();
            for p in &sig.params {
                let mutable = matches!(p.convention, Convention::Inout | Convention::Var);
                body.params.insert(p.name.clone(), p.convention);
                body.locals.push((p.span, p.span));
                body.scopes[0].locals.insert(
                    p.name.clone(),
                    Local {
                        ty: p.ty.clone(),
                        mutable,
                        span: p.span,
                        declared_at: None,
                        moved: false,
                        origin: Origin::Param,
                    },
                );
            }
        }
        body.type_params = type_params;
        body
    }

    fn report(&mut self, d: Diagnostic) {
        self.diagnostics.push(d);
    }

    /// Whether `path` leads to a module imported by its full path, as `py` and `py.os` lead to
    /// `py.os.path` after `import py.os.path`: a package, which an attribute descends.
    fn is_package(&self, path: &str) -> bool {
        let prefix = format!("{path}.");
        self.program.modules.iter().any(|m| m.starts_with(&prefix))
    }

    /// `o.value()` on a `PyObject`: the LotML value of the type its context expects, which the
    /// boundary checks, so `T ! PyError` (specs/python-object R2.1, R2.2). Any other method is one
    /// a `PyObject` does not have.
    fn py_value(&mut self, name: &Ident, args: &[Arg], span: Span, expected: Option<&Ty>) -> Ty {
        self.arg_types(args, &[]);
        if name.name != "value" {
            self.opaque("E0205", name.span, format!("a `PyObject` has no method `{}`", name.name));
            return Ty::Error;
        }
        if !args.is_empty() {
            self.report(Diagnostic::error("E0203", span, "`value()` takes no arguments"));
        }
        let wanted = expected.map(|e| self.resolve(e)).map(|e| match e {
            Ty::Result(value, _) => *value,
            other => other,
        });
        match wanted {
            Some(ty) if self.known(&ty) && !matches!(ty, Ty::Error) && self.carried(&ty) => {
                Ty::Result(Box::new(ty), Box::new(Ty::Adt("PyError".into(), vec![])))
            }
            Some(ty) if self.known(&ty) && !matches!(ty, Ty::Error) => {
                self.report(
                    Diagnostic::error("E0204", span, format!("a `PyObject` cannot become a `{ty}`"))
                        .note("it can become what the boundary carries: numbers, `bool`, `str`, `bytes`, `None`, and lists, sets, dicts, tuples and optionals of them"),
                );
                Ty::Error
            }
            _ => {
                self.report(
                    Diagnostic::error("E0205", span, "`value()` needs to know the type to take out of the `PyObject`")
                        .note("annotate the binding: `n: int = o.value()?`"),
                );
                Ty::Error
            }
        }
    }

    /// Report `message` at `span` as an operation a `PyObject` does not have.
    fn opaque(&mut self, code: &'static str, span: Span, message: String) {
        self.report(Diagnostic::error(code, span, message).note(OPAQUE));
    }

    /// `d`, with the note on converting a `PyObject` when `ty` is one, or on a Python class's
    /// members when it is a value of one.
    fn noting(&self, d: Diagnostic, ty: &Ty) -> Diagnostic {
        if matches!(self.resolve(ty), Ty::PyObject) {
            d.note(OPAQUE)
        } else if self.py_class(ty).is_some() {
            d.note(CLASS)
        } else {
            d
        }
    }

    /// Whether `ty` is a Python object: a `PyObject` or a value of a Python class (adr:0034).
    fn is_python(&self, ty: &Ty) -> bool {
        matches!(self.resolve(ty), Ty::PyObject) || self.py_class(ty).is_some()
    }

    /// Report `message` at `span`, an operation the Python object `ty` holds does not have, with
    /// the note that says what it has.
    fn refuse_python(&mut self, span: Span, message: String, ty: &Ty) {
        let note = if self.holds_py_handle(ty) { OPAQUE } else { CLASS };
        self.report(Diagnostic::error("E0204", span, message).note(note));
    }

    /// Whether `ty` is or holds a `PyObject` proper, as against only Python classes' values.
    fn holds_py_handle(&self, ty: &Ty) -> bool {
        self.holds(ty, false)
    }

    /// Whether `found` fits `expected`, a type holding a `PyObject`, part by part: `[int]` where
    /// Python takes a `[PyObject]` (specs/python-object R1.3).
    fn fits_shape(&mut self, found: &Ty, expected: &Ty) -> bool {
        match (self.resolve(found), self.resolve(expected)) {
            // The element of `[]`, what `None` holds: a `PyObject` from here on.
            (f @ Ty::Var(_), Ty::PyObject) => self.infer.unify(&f, &Ty::PyObject),
            (f, Ty::PyObject) => self.carried(&f),
            (Ty::List(a), Ty::List(b)) | (Ty::Set(a), Ty::Set(b)) | (Ty::Optional(a), Ty::Optional(b)) => {
                self.fits_shape(&a, &b)
            }
            (f, Ty::Optional(b)) => self.fits_shape(&f, &b),
            (Ty::Dict(k, v), Ty::Dict(ek, ev)) => self.fits_shape(&k, &ek) && self.fits_shape(&v, &ev),
            (Ty::Tuple(a), Ty::Tuple(b)) if a.len() == b.len() => a.iter().zip(&b).all(|(x, y)| self.fits_shape(x, y)),
            (f, e) => self.infer.unify(&f, &e),
        }
    }

    /// Whether the boundary of adr:0012 carries a value of `ty` into Python as a `PyObject`.
    fn carried(&self, ty: &Ty) -> bool {
        match self.resolve(ty) {
            Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Str | Ty::Bytes | Ty::Unit | Ty::PyObject => true,
            // Inside a value, a part not yet known, the element of `[]` or what `None` holds.
            Ty::Var(_) => true,
            Ty::List(t) | Ty::Set(t) | Ty::Optional(t) => self.carried(&t),
            Ty::Dict(k, v) => self.carried(&k) && self.carried(&v),
            Ty::Tuple(items) => items.iter().all(|t| self.carried(t)),
            _ => false,
        }
    }

    /// Whether `ty` is or holds a `PyObject`, a record's and a variant's fields included: what
    /// printing, formatting, comparing, hashing or ordering would hand to Python's own methods.
    fn holds_object(&self, ty: &Ty) -> bool {
        self.holds(ty, true)
    }

    /// Whether `ty` is or holds a `PyObject`, or, with `classes`, a Python class's value too: a
    /// record's and a variant's fields included.
    fn holds(&self, ty: &Ty, classes: bool) -> bool {
        fn walk(b: &Body<'_>, ty: &Ty, classes: bool, seen: &mut Vec<String>) -> bool {
            match b.resolve(ty) {
                Ty::PyObject => true,
                Ty::Adt(name, _) if b.program.py_attributes.contains_key(&name) => classes,
                Ty::List(t) | Ty::Set(t) | Ty::Optional(t) | Ty::Heap(t) => walk(b, &t, classes, seen),
                Ty::Dict(k, v) | Ty::Result(k, v) => walk(b, &k, classes, seen) || walk(b, &v, classes, seen),
                Ty::Tuple(items) => items.iter().any(|t| walk(b, t, classes, seen)),
                Ty::Adt(name, args) => {
                    if args.iter().any(|t| walk(b, t, classes, seen)) {
                        return true;
                    }
                    if seen.contains(&name) {
                        return false;
                    }
                    seen.push(name.clone());
                    let fields: Vec<Ty> = match b.program.types.get(&name) {
                        Some(TypeDef::Record { fields, .. }) => fields.iter().map(|f| f.ty.clone()).collect(),
                        Some(TypeDef::Sum { variants, .. }) => {
                            variants.iter().flat_map(|v| v.fields.iter().flatten()).map(|f| f.ty.clone()).collect()
                        }
                        None => Vec::new(),
                    };
                    fields.iter().any(|t| walk(b, t, classes, seen))
                }
                _ => false,
            }
        }
        walk(self, ty, classes, &mut Vec::new())
    }

    /// Whether `ty` is known in full, no part of it left to inference.
    fn known(&self, ty: &Ty) -> bool {
        match self.resolve(ty) {
            Ty::Var(_) => false,
            Ty::List(t) | Ty::Set(t) | Ty::Optional(t) | Ty::Heap(t) => self.known(&t),
            Ty::Dict(a, b) | Ty::Result(a, b) => self.known(&a) && self.known(&b),
            Ty::Tuple(items) | Ty::Adt(_, items) => items.iter().all(|t| self.known(t)),
            _ => true,
        }
    }

    /// Whether this body already has an error, so a program it belongs to cannot compile.
    fn has_error(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == lotml_diag::Severity::Error)
    }

    /// A member of a value whose type is not known: what it would reach is not known either.
    /// An error type counts once nothing has been reported for it, since it then came from
    /// calling or indexing such a value rather than from a mistake already shown; reading
    /// through it is how a program would reach the interpreter's objects.
    fn unknown_member(&mut self, name: &Ident, what: &str, verb: &str) {
        self.report(
            Diagnostic::error(
                "E0205",
                name.span,
                format!("the type here is not known, so {what} `{}` cannot be {verb}", name.name),
            )
            .note("annotate the value, or let where a lambda is used pin its parameter's type"),
        );
    }

    fn resolve(&self, ty: &Ty) -> Ty {
        self.infer.resolve(ty)
    }

    fn source(&self, span: Span) -> &'p str {
        self.text.get(span.range()).unwrap_or("")
    }

    // Scopes ------------------------------------------------------------------------

    fn lookup(&self, name: &str) -> Option<&Local> {
        self.scopes.iter().rev().find_map(|s| s.locals.get(name))
    }

    fn lookup_mut(&mut self, name: &str) -> Option<&mut Local> {
        self.scopes.iter_mut().rev().find_map(|s| s.locals.get_mut(name))
    }

    fn narrowed(&self, path: &str) -> Option<Ty> {
        self.scopes.iter().rev().find_map(|s| s.narrowed.get(path).cloned())
    }

    fn declare(&mut self, name: &Ident, ty: Ty, mutable: bool, declared_at: Option<u32>) {
        if name.name.is_empty() {
            return;
        }
        crate::report_reserved(&mut self.diagnostics, name);
        let scope = self.scopes.last_mut().expect("a scope");
        scope.narrowed.remove(&name.name);
        let origin = if mutable || declared_at.is_some() { Origin::Declared } else { Origin::Bound };
        scope
            .locals
            .insert(name.name.clone(), Local { ty, mutable, span: name.span, declared_at, moved: false, origin });
        self.locals.push((name.span, name.span));
    }

    fn with_scope<T>(&mut self, facts: &[(String, Ty)], f: impl FnOnce(&mut Self) -> T) -> (T, Scope) {
        let mut scope = Scope::default();
        for (path, ty) in facts {
            scope.narrowed.insert(path.clone(), ty.clone());
        }
        self.scopes.push(scope);
        let out = f(self);
        let scope = self.scopes.pop().expect("the scope pushed");
        (out, scope)
    }

    /// After branches that all complete: names every branch declared become declared here.
    fn merge(&mut self, branches: &[Scope]) {
        let Some(first) = branches.first() else { return };
        for (name, local) in &first.locals {
            if self.lookup(name).is_some() {
                continue;
            }
            if branches[1..].iter().all(|b| b.locals.get(name).is_some_and(|l| self.infer.unify(&l.ty, &local.ty))) {
                self.scopes.last_mut().expect("a scope").locals.insert(name.clone(), local.clone());
                // One local, declared in each branch: the later declarations, and what referred
                // to them, refer to the first.
                for branch in &branches[1..] {
                    let declared = branch.locals[name].span;
                    for (_, to) in &mut self.locals {
                        if *to == declared {
                            *to = local.span;
                        }
                    }
                }
            }
        }
    }

    fn all_names(&self) -> Vec<String> {
        let mut names: BTreeSet<String> = BTreeSet::new();
        for scope in &self.scopes {
            names.extend(scope.locals.keys().cloned());
        }
        names.extend(self.program.functions.keys().cloned());
        names.extend(self.program.variant_of.keys().cloned());
        names.extend(self.program.types.keys().cloned());
        names.extend(self.program.imported.keys().cloned());
        names.extend(builtins::PRELUDE.iter().map(ToString::to_string));
        names.into_iter().collect()
    }

    // Statements --------------------------------------------------------------------

    /// Check a block; true when it never completes normally (it returns, fails or loops forever).
    pub fn block(&mut self, block: &Block) -> bool {
        let mut diverges = false;
        for stmt in &block.stmts {
            if self.stmt(stmt) {
                diverges = true;
            }
        }
        diverges
    }

    pub fn function_body(&mut self, block: &Block, sig: &FnSig) {
        let diverges = self.block(block);
        let unit = matches!(self.resolve(&self.ret), Ty::Unit);
        if unit {
            for (copy, param, span) in std::mem::take(&mut self.copies) {
                if self.mutated.contains(&copy) {
                    self.report(
                        Diagnostic::warning("E0308", span, format!("`{copy}` is a copy of `{param}`, changed and then dropped: the caller never sees the change"))
                            .note(format!("if the caller should see it, declare `inout {param}` and pass `&` at the call; otherwise return `{copy}`")),
                    );
                }
            }
        }
        // A `var` parameter is itself a copy; a `var self` that a method changes and does not
        // return is the Python habit of a method changing its object.
        for p in &sig.params {
            let shared =
                matches!(self.resolve(&p.ty), Ty::List(_) | Ty::Dict(..) | Ty::Set(_) | Ty::Heap(_) | Ty::Adt(..));
            let dropped = unit || (p.name == "self" && !self.returned.contains("self"));
            if p.convention == Convention::Var && shared && dropped && self.mutated.contains(&p.name) {
                let note = if p.name == "self" {
                    "declare the receiver `inout self` for a method that changes its object".to_string()
                } else {
                    format!("if the caller should see it, declare `inout {}` and pass `&` at the call", p.name)
                };
                self.report(
                    Diagnostic::warning(
                        "E0308",
                        p.span,
                        format!(
                            "`var {}` is a copy, changed and then dropped: the caller never sees the change",
                            p.name
                        ),
                    )
                    .note(note),
                );
            }
        }
        let needs_value = !matches!(self.resolve(&self.ret), Ty::Unit | Ty::Never | Ty::Error);
        if needs_value && !diverges && !unreadable(block) {
            let end = Span { start: block.span.end, end: block.span.end };
            self.report(
                Diagnostic::error(
                    "E0209",
                    sig.span,
                    format!("`{}` may end without returning a `{}`", sig.name, self.resolve(&self.ret)),
                )
                .label(end, "the end of the body is reachable here")
                .note("return a value on every path, or `todo()` for a path not written yet"),
            );
        }
    }

    fn stmt(&mut self, stmt: &Stmt) -> bool {
        match &stmt.kind {
            StmtKind::Expr(expr) => {
                let ty = self.expr(expr, None);
                if let Ty::Result(_, error) = &ty {
                    let d = Diagnostic::error(
                        "E0219",
                        expr.span,
                        format!("this drops a result, and the `{}` error with it", self.resolve(error)),
                    );
                    let d = self.unwrap_fix(d, error, expr.span);
                    self.report(d);
                }
                matches!(self.resolve(&ty), Ty::Never)
            }
            StmtKind::Var { name, ty, value } => {
                let declared = ty.as_ref().map(|t| self.lower(t));
                let found = self.expr(value, declared.as_ref());
                if let Some(declared) = &declared {
                    self.coerce(&found, declared, value.span);
                }
                let ty = declared.unwrap_or(found);
                self.seen.insert(name.span, ty.clone());
                if let ExprKind::Name(param) = &value.kind {
                    let by_copy =
                        self.params.get(param).is_some_and(|c| matches!(c, Convention::Default | Convention::Sink));
                    let shared = matches!(
                        self.resolve(&ty),
                        Ty::List(_) | Ty::Dict(..) | Ty::Set(_) | Ty::Heap(_) | Ty::Adt(..)
                    );
                    if by_copy && shared && self.lookup(param).is_some_and(|l| l.origin == Origin::Param) {
                        self.copies.push((name.name.clone(), param.clone(), stmt.span));
                    }
                }
                self.declare(name, ty, true, None);
                false
            }
            StmtKind::Annotated { target, ty, value } => {
                let declared = self.lower(ty);
                let found = self.expr(value, Some(&declared));
                self.coerce(&found, &declared, value.span);
                // The declared type, by the name's span: what a backend gives the local.
                self.seen.insert(target.span, declared.clone());
                self.assign_name(target, declared, Some(stmt.span.start));
                false
            }
            StmtKind::Assign { target, value } => {
                let expected = self.target_type(target);
                let found = self.expr(value, expected.as_ref());
                self.assign(target, found, Some(stmt.span.start));
                false
            }
            StmtKind::AugAssign { target, op, value } => {
                let current = self.expr(target, None);
                let found = self.expr(value, None);
                let result = self.binary(*op, &current, &found, stmt.span);
                if !self.infer.unify(&current, &result) && !result.is_poison() {
                    let current = self.resolve(&current);
                    self.report(Diagnostic::error(
                        "E0204",
                        stmt.span,
                        format!("`{}=` would change `{current}` into `{}`", op.text(), self.resolve(&result)),
                    ));
                }
                self.mutate(target, stmt.span, true);
                false
            }
            StmtKind::Return(value) => {
                self.ret(value.as_ref(), stmt.span);
                true
            }
            StmtKind::Assert { test, message } => {
                self.condition(test);
                if let Some(m) = message {
                    self.expr(m, None);
                }
                false
            }
            StmtKind::Pass | StmtKind::Break | StmtKind::Continue | StmtKind::Error => false,
            StmtKind::If { branches, orelse } => self.if_stmt(branches, orelse.as_ref()),
            StmtKind::While { test, body } => {
                // A later iteration sees what the body assigned: forget it before the first.
                for root in assigned(body) {
                    self.forget(&root);
                }
                let facts = self.condition(test);
                let forever = matches!(test.kind, ExprKind::Bool(true));
                let ((), _) = self.with_scope(&facts.then, |b| {
                    b.block(body);
                });
                let exits = breaks(body);
                if !exits {
                    // The loop ends only when its test is false.
                    let scope = self.scopes.last_mut().expect("a scope");
                    scope.narrowed.extend(facts.otherwise);
                }
                forever && !exits
            }
            StmtKind::For { target, iter, body } => {
                let iterable = self.expr(iter, None);
                let item = match builtins::element(&iterable, &mut self.infer) {
                    Some(t) => t,
                    None => {
                        let ty = self.resolve(&iterable);
                        let d = Diagnostic::error("E0204", iter.span, format!("`{ty}` cannot be iterated"));
                        let d = self.noting(d, &ty);
                        self.report(d);
                        Ty::Error
                    }
                };
                for root in assigned(body) {
                    self.forget(&root);
                }
                let ((), _) = self.with_scope(&[], |b| {
                    b.bind_target(target, &item);
                    b.block(body);
                });
                false
            }
            StmtKind::Match { subject, arms } => self.match_stmt(subject, arms, stmt.span),
        }
    }

    fn ret(&mut self, value: Option<&Expr>, span: Span) {
        if let Some(Expr { kind: ExprKind::Name(n), .. }) = value {
            self.returned.insert(n.clone());
        }
        let expected = self.resolve(&self.ret);
        match value {
            None => {
                if !matches!(expected, Ty::Unit | Ty::Error | Ty::Never) && !self.in_test {
                    self.report(Diagnostic::error(
                        "E0218",
                        span,
                        format!("this function returns a `{expected}`: `return` needs a value"),
                    ));
                }
            }
            Some(v) => {
                if matches!(expected, Ty::Unit) {
                    self.expr(v, None);
                    self.report(
                        Diagnostic::error("E0218", span, "this function returns nothing, so `return` takes no value")
                            .note("declare the return type with `-> T` if it should return one"),
                    );
                    return;
                }
                let found = self.expr(v, Some(&expected));
                self.coerce(&found, &expected, v.span);
            }
        }
    }

    fn if_stmt(&mut self, branches: &[(Expr, Block)], orelse: Option<&Block>) -> bool {
        let mut negated: Vec<(String, Ty)> = Vec::new();
        let mut completed = Vec::new();
        let mut all_diverge = true;
        for (test, body) in branches {
            let facts = self.with_scope(&negated, |b| b.condition(test)).0;
            let mut inside = negated.clone();
            inside.extend(facts.then.clone());
            let (diverges, scope) = self.with_scope(&inside, |b| b.block(body));
            if !diverges {
                completed.push(scope);
            }
            all_diverge &= diverges;
            negated.extend(facts.otherwise);
        }
        // The narrowings at the end of every path that reaches the next statement.
        let mut ends: Vec<HashMap<String, Ty>> = completed.iter().map(|s| s.narrowed.clone()).collect();
        let diverges = match orelse {
            Some(body) => {
                let (diverges, scope) = self.with_scope(&negated, |b| b.block(body));
                if !diverges {
                    ends.push(scope.narrowed.clone());
                    completed.push(scope);
                }
                all_diverge &= diverges;
                if !all_diverge {
                    self.merge(&completed);
                }
                all_diverge
            }
            None => {
                ends.push(negated.into_iter().collect());
                false
            }
        };
        self.join_narrowings(&ends);
        diverges
    }

    /// After paths join, a path is narrowed when it was narrowed at the end of each of them,
    /// in that path's own scope or in one still around it.
    fn join_narrowings(&mut self, ends: &[HashMap<String, Ty>]) {
        let Some(first) = ends.first() else { return };
        let mut candidates: BTreeSet<String> = first.keys().cloned().collect();
        for end in &ends[1..] {
            candidates.extend(end.keys().cloned());
        }
        for path in candidates {
            if self.narrowed(&path).is_some() {
                continue;
            }
            let found: Vec<Option<&Ty>> = ends.iter().map(|end| end.get(&path)).collect();
            if found.iter().all(Option::is_some) {
                let ty = found[0].cloned().expect("checked");
                self.scopes.last_mut().expect("a scope").narrowed.insert(path, ty);
            }
        }
    }

    /// Inside a lambda every changeable local around it is a copy: immutable there.
    fn capture(&mut self) {
        let mut seen: HashMap<String, Local> = HashMap::new();
        for scope in &self.scopes {
            for (name, local) in &scope.locals {
                seen.insert(name.clone(), local.clone());
            }
        }
        let inner = self.scopes.last_mut().expect("a scope");
        for (name, local) in seen {
            if local.mutable {
                inner
                    .locals
                    .insert(name, Local { mutable: false, declared_at: None, origin: Origin::Captured, ..local });
            }
        }
    }

    /// Forget what was known about `root` and every path through it: it was assigned.
    fn forget(&mut self, root: &str) {
        let prefix = format!("{root}.");
        for scope in &mut self.scopes {
            scope.narrowed.retain(|path, _| path != root && !path.starts_with(&prefix));
        }
    }

    /// An optional place given a value that is not `None` is known to hold one.
    fn narrow_assigned(&mut self, place: &Expr, declared: &Ty, value: &Ty) {
        let Some(path) = path_of(place) else { return };
        let (declared, value) = (self.resolve(declared), self.resolve(value));
        if let Ty::Optional(inner) = declared
            && !matches!(value, Ty::Optional(_) | Ty::Var(_) | Ty::Error | Ty::Never | Ty::Unit)
        {
            self.scopes.last_mut().expect("a scope").narrowed.insert(path, *inner);
        }
    }

    fn match_stmt(&mut self, subject: &Expr, arms: &[Arm], span: Span) -> bool {
        let ty = self.expr(subject, None);
        let ty = self.resolve(&ty);
        let mut all_diverge = !arms.is_empty();
        let mut completed = Vec::new();
        for arm in arms {
            let (diverges, scope) = self.with_scope(&[], |b| {
                b.pattern(&arm.pattern, &ty);
                b.block(&arm.body)
            });
            if !diverges {
                completed.push(scope);
            }
            all_diverge &= diverges;
        }
        let exhaustive = self.exhaustive(&ty, arms, span);
        if !all_diverge {
            self.merge(&completed);
        }
        all_diverge && exhaustive
    }

    /// Whether the arms cover every value; reports what they miss.
    fn exhaustive(&mut self, ty: &Ty, arms: &[Arm], span: Span) -> bool {
        let catch_all = arms.iter().any(|a| irrefutable(&a.pattern, self.program));
        if catch_all || ty.is_poison() || matches!(ty, Ty::Var(_)) {
            return true;
        }
        let covered: Vec<String> = arms
            .iter()
            .filter_map(|a| match &a.pattern.kind {
                PatternKind::Variant { name, args } if args.iter().all(|p| irrefutable(p, self.program)) => {
                    Some(name.name.clone())
                }
                PatternKind::Name(name) => Some(name.name.clone()),
                PatternKind::Literal(Expr { kind: ExprKind::None, .. }) => Some("None".into()),
                PatternKind::Literal(Expr { kind: ExprKind::Bool(b), .. }) => {
                    Some(if *b { "True" } else { "False" }.into())
                }
                _ => None,
            })
            .collect();
        let all: Vec<String> = match ty {
            Ty::Adt(name, _) => match self.program.types.get(name) {
                Some(TypeDef::Sum { variants, .. }) => variants.iter().map(|v| v.name.clone()).collect(),
                _ => vec!["_".into()],
            },
            Ty::Result(..) => vec!["Ok".into(), "Err".into()],
            Ty::Bool => vec!["True".into(), "False".into()],
            Ty::Optional(_) => vec!["None".into(), "_".into()],
            _ => vec!["_".into()],
        };
        let missing: Vec<String> = all.into_iter().filter(|v| !covered.contains(v)).collect();
        if missing.is_empty() {
            return true;
        }
        let indent = " ".repeat(self.column_of(arms.first().map_or(span, |a| a.span)));
        let arms_text: String = missing.iter().map(|m| format!("{indent}case {m}:\n{indent}    todo()\n")).collect();
        let end = arms.last().map_or(span.end, |a| a.span.end);
        let at = self.text[end as usize..].find('\n').map_or(self.text.len(), |i| end as usize + i + 1);
        self.report(
            Diagnostic::error(
                "E0206",
                span,
                format!(
                    "this `match` does not cover {}",
                    missing.iter().map(|m| format!("`{m}`")).collect::<Vec<_>>().join(", ")
                ),
            )
            .alternatives(missing.clone())
            .fix(
                "add an arm for each",
                Applicability::HasPlaceholders,
                vec![(Span::new(at, at), arms_text)],
            ),
        );
        false
    }

    fn column_of(&self, span: Span) -> usize {
        let start = span.start as usize;
        let line_start = self.text[..start.min(self.text.len())].rfind('\n').map_or(0, |i| i + 1);
        start - line_start
    }

    fn pattern(&mut self, pattern: &Pattern, ty: &Ty) {
        match &pattern.kind {
            PatternKind::Wildcard | PatternKind::Error => {}
            PatternKind::Name(name) => {
                if self.program.variant_of.contains_key(&name.name) {
                    self.variant_pattern(name, &[], ty, pattern.span);
                } else {
                    let inner = match ty {
                        Ty::Optional(t) => (**t).clone(),
                        other => other.clone(),
                    };
                    let narrowed = if arms_none(ty) { inner } else { ty.clone() };
                    self.declare(name, narrowed, false, None);
                }
            }
            PatternKind::Variant { name, args } => self.variant_pattern(name, args, ty, pattern.span),
            PatternKind::Tuple(items) => {
                let element_types = match ty {
                    Ty::Tuple(ts) if ts.len() == items.len() => ts.clone(),
                    Ty::Error | Ty::Never | Ty::Var(_) => vec![Ty::Error; items.len()],
                    other => {
                        self.report(Diagnostic::error(
                            "E0211",
                            pattern.span,
                            format!("a tuple pattern of {} items cannot match `{other}`", items.len()),
                        ));
                        vec![Ty::Error; items.len()]
                    }
                };
                for (p, t) in items.iter().zip(element_types) {
                    self.pattern(p, &t);
                }
            }
            PatternKind::Literal(expr) => {
                if matches!(expr.kind, ExprKind::None) {
                    return;
                }
                let found = self.expr(expr, None);
                let target = match ty {
                    Ty::Optional(t) => (**t).clone(),
                    other => other.clone(),
                };
                if !self.infer.unify(&found, &target) {
                    let found = self.resolve(&found);
                    self.report(Diagnostic::error(
                        "E0211",
                        pattern.span,
                        format!("a `{found}` pattern cannot match `{target}`"),
                    ));
                }
            }
        }
    }

    fn variant_pattern(&mut self, name: &Ident, args: &[Pattern], ty: &Ty, span: Span) {
        let fields: Vec<Ty> = match (name.name.as_str(), ty) {
            ("Ok", Ty::Result(t, _)) => vec![(**t).clone()],
            ("Err", Ty::Result(_, e)) => vec![(**e).clone()],
            ("Ok" | "Err", Ty::Error | Ty::Never | Ty::Var(_)) => vec![Ty::Error],
            (_, Ty::Adt(owner, type_args)) => match self.program.types.get(owner) {
                Some(TypeDef::Sum { params, variants }) => match variants.iter().find(|v| v.name == name.name) {
                    Some(v) => v
                        .fields
                        .as_ref()
                        .map(|fs| fs.iter().map(|f| f.ty.substitute(params, type_args)).collect())
                        .unwrap_or_default(),
                    None => {
                        self.report(
                            Diagnostic::error("E0211", name.span, format!("`{owner}` has no variant `{}`", name.name))
                                .alternatives(variants.iter().map(|v| v.name.clone())),
                        );
                        vec![Ty::Error; args.len()]
                    }
                },
                _ => {
                    self.report(Diagnostic::error(
                        "E0211",
                        name.span,
                        format!("`{owner}` is a record, not a sum type: it has no variants to match"),
                    ));
                    vec![Ty::Error; args.len()]
                }
            },
            (_, Ty::Error | Ty::Never | Ty::Var(_)) => vec![Ty::Error; args.len()],
            (_, other) => {
                self.report(Diagnostic::error(
                    "E0211",
                    span,
                    format!("the pattern `{}` cannot match a `{other}`", name.name),
                ));
                vec![Ty::Error; args.len()]
            }
        };
        if fields.len() != args.len() && !(args.is_empty() && fields.is_empty()) {
            self.report(Diagnostic::error(
                "E0211",
                span,
                format!(
                    "`{}` has {} field{}, and the pattern gives {}",
                    name.name,
                    fields.len(),
                    if fields.len() == 1 { "" } else { "s" },
                    args.len()
                ),
            ));
            for a in args {
                self.pattern(a, &Ty::Error);
            }
            return;
        }
        for (a, t) in args.iter().zip(fields) {
            self.pattern(a, &t);
        }
    }

    fn bind_target(&mut self, target: &Target, ty: &Ty) {
        match target {
            Target::Name(name) => self.declare(name, ty.clone(), false, None),
            Target::Tuple(items, span) => {
                let ty = self.resolve(ty);
                let element_types = match &ty {
                    Ty::Tuple(ts) if ts.len() == items.len() => ts.clone(),
                    Ty::Var(_) => {
                        let fresh: Vec<Ty> = items.iter().map(|_| self.infer.fresh()).collect();
                        self.infer.unify(&ty, &Ty::Tuple(fresh.clone()));
                        fresh
                    }
                    Ty::Error | Ty::Never => vec![Ty::Error; items.len()],
                    other => {
                        self.report(Diagnostic::error(
                            "E0204",
                            *span,
                            format!("{} names cannot unpack a `{other}`", items.len()),
                        ));
                        vec![Ty::Error; items.len()]
                    }
                };
                for (item, t) in items.iter().zip(element_types) {
                    self.bind_target(item, &t);
                }
            }
        }
    }

    /// The declared type of an assignment's target, when it already has one.
    fn target_type(&mut self, target: &Expr) -> Option<Ty> {
        match &target.kind {
            ExprKind::Name(name) => self.lookup(name).map(|l| l.ty.clone()),
            ExprKind::Attr { .. } | ExprKind::Index { .. } => {
                let saved = self.diagnostics.len();
                let ty = self.expr(target, None);
                self.diagnostics.truncate(saved);
                Some(ty)
            }
            _ => None,
        }
    }

    fn assign_name(&mut self, name: &Ident, ty: Ty, at: Option<u32>) {
        match self.lookup(&name.name).cloned() {
            None => self.declare(name, ty, false, at),
            Some(local) => {
                self.locals.push((name.span, local.span));
                if !local.mutable {
                    self.immutable(&name.name, &local, name.span, "assigned again");
                    return;
                }
                if !self.fits(&ty, &local.ty) && !ty.is_poison() {
                    let (want, got) = (self.resolve(&local.ty), self.resolve(&ty));
                    self.report(Diagnostic::error(
                        "E0204",
                        name.span,
                        format!("`{}` holds a `{want}`; it cannot be given a `{got}`", name.name),
                    ));
                }
                self.forget(&name.name);
                let place = Expr { span: name.span, kind: ExprKind::Name(name.name.clone()) };
                self.narrow_assigned(&place, &local.ty, &ty);
            }
        }
    }

    fn assign(&mut self, target: &Expr, ty: Ty, at: Option<u32>) {
        match &target.kind {
            ExprKind::Name(name) => self.assign_name(&Ident { name: name.clone(), span: target.span }, ty, at),
            ExprKind::Tuple(items) => {
                let resolved = self.resolve(&ty);
                let types = match resolved {
                    Ty::Tuple(ts) if ts.len() == items.len() => ts,
                    _ => vec![Ty::Error; items.len()],
                };
                for (item, t) in items.iter().zip(types) {
                    self.assign(item, t, None);
                }
            }
            ExprKind::Attr { .. } | ExprKind::Index { .. } => {
                // The declared type of the place, not what a test narrowed it to.
                if let Some(path) = path_of(target) {
                    self.forget(&path);
                }
                let current = self.expr(target, None);
                if let ExprKind::Attr { object, name } = &target.kind
                    && let Some(class) = self.seen.get(&object.span).cloned().and_then(|t| self.py_class(&t))
                {
                    self.report(
                        Diagnostic::error(
                            "E0205",
                            name.span,
                            format!(
                                "`{}` is an attribute of a `{class}`, a Python object: it is read, never assigned",
                                name.name
                            ),
                        )
                        .note("a method of the class that changes it, if its interface declares one, is the way"),
                    );
                    return;
                }
                if !self.fits(&ty, &current) && !ty.is_poison() {
                    let (want, got) = (self.resolve(&current), self.resolve(&ty));
                    self.report(Diagnostic::error(
                        "E0204",
                        target.span,
                        format!("this place holds a `{want}`; it cannot be given a `{got}`"),
                    ));
                }
                self.mutate(target, target.span, false);
                self.narrow_assigned(target, &current, &ty);
            }
            _ => {
                self.report(Diagnostic::error(
                    "E0204",
                    target.span,
                    "only a name, a field or an element can be assigned",
                ));
            }
        }
    }

    /// The local a place belongs to: `xs` in `xs[0].name`.
    fn root(expr: &Expr) -> Option<(&str, Span)> {
        match &expr.kind {
            ExprKind::Name(n) => Some((n, expr.span)),
            ExprKind::Attr { object, .. } | ExprKind::Index { object, .. } | ExprKind::Slice { object, .. } => {
                Self::root(object)
            }
            _ => None,
        }
    }

    /// Changing `place` needs its root to be mutable.
    fn mutate(&mut self, place: &Expr, span: Span, aug: bool) {
        let Some((name, _)) = Self::root(place) else { return };
        let Some(local) = self.lookup(name).cloned() else { return };
        if local.mutable {
            self.mutated.insert(name.to_string());
            return;
        }
        let whole = matches!(place.kind, ExprKind::Name(_));
        if whole && aug {
            self.immutable(name, &local, span, "changed");
        } else {
            self.immutable_mutation(name, &local, span);
        }
    }

    fn immutable(&mut self, name: &str, local: &Local, span: Span, what: &str) {
        let d = Diagnostic::error("E0301", span, format!("`{name}` is immutable and cannot be {what}"))
            .label(local.span, "declared here");
        let d = match (local.declared_at, local.origin) {
            (Some(at), _) => d.fix(
                "declare it with `var`",
                Applicability::MachineApplicable,
                vec![(Span { start: at, end: at }, "var ".into())],
            ),
            (None, Origin::Param) => d.note(
                "a parameter is read-only: copy it into a `var`, or declare it `inout` to change the caller's value",
            ),
            (None, Origin::Captured) => {
                d.note(format!("a lambda captures a copy of `{name}`: a change made here would never reach it"))
            }
            (None, _) => {
                d.note(format!("`{name}` is bound anew each time: assign the new value to a `var` of another name"))
            }
        };
        self.report(d);
    }

    fn immutable_mutation(&mut self, name: &str, local: &Local, span: Span) {
        let d = Diagnostic::error("E0302", span, format!("`{name}` is immutable: it cannot be changed in place"))
            .label(local.span, "declared here");
        let d = match (local.declared_at, local.origin) {
            (Some(at), _) => d.fix("declare it with `var`", Applicability::MachineApplicable, vec![(Span { start: at, end: at }, "var ".into())]),
            (None, Origin::Param) if name == "self" => d.note("declare the receiver `inout self` for a method that changes it"),
            (None, Origin::Param) => d.note("a parameter is read-only: to change the caller's value, declare it `inout` and pass `&x`; to change a copy, write `var mine = param`"),
            (None, Origin::Captured) => d.note(format!("a lambda captures a copy of `{name}`: a change made here would never reach it")),
            (None, _) => d.note(format!("`{name}` is bound anew each time: copy it into a `var` to change it")),
        };
        self.report(d);
    }

    // Conditions and narrowing ------------------------------------------------------

    fn condition(&mut self, test: &Expr) -> Facts {
        let ty = self.expr(test, Some(&Ty::Bool));
        let resolved = self.resolve(&ty);
        if !self.infer.unify(&resolved, &Ty::Bool) && !resolved.is_poison() {
            self.truthiness(test, &resolved);
        }
        self.facts(test)
    }

    fn truthiness(&mut self, test: &Expr, ty: &Ty) {
        let (negated, inner) = match &test.kind {
            ExprKind::Not(inner) => (true, inner.as_ref()),
            _ => (false, test),
        };
        let text = self.source(inner.span);
        let wrap = if matches!(
            inner.kind,
            ExprKind::Name(_) | ExprKind::Attr { .. } | ExprKind::Call { .. } | ExprKind::Index { .. }
        ) {
            text.to_string()
        } else {
            format!("({text})")
        };
        let (fix, exact) = match ty {
            Ty::List(_) | Ty::Dict(..) | Ty::Set(_) | Ty::Heap(_) | Ty::Tuple(_) => {
                (format!("len({text}) {} 0", if negated { "==" } else { ">" }), true)
            }
            Ty::Str => (format!("{wrap} {} \"\"", if negated { "==" } else { "!=" }), true),
            Ty::Int(_) | Ty::Float(_) => (format!("{wrap} {} 0", if negated { "==" } else { "!=" }), true),
            Ty::Optional(t) => {
                let exact = matches!(**t, Ty::Adt(..));
                (format!("{wrap} is {}None", if negated { "" } else { "not " }), exact)
            }
            _ => (String::new(), false),
        };
        let mut d =
            Diagnostic::error("E0208", test.span, format!("a condition must be a `bool`, and this is a `{ty}`"))
                .note("lotml has no truthiness");
        if !fix.is_empty() {
            let applicability = if exact { Applicability::MachineApplicable } else { Applicability::MaybeIncorrect };
            d = d.fix(format!("write `{fix}`"), applicability, vec![(test.span, fix)]);
        }
        self.report(d);
    }

    /// What `test` being true (or false) says about optionals: `x is not None` makes `x` a `T`.
    fn facts(&mut self, test: &Expr) -> Facts {
        match &test.kind {
            ExprKind::Compare { first, rest } if rest.len() == 1 => {
                let (op, other) = (&rest[0].0, &rest[0].1);
                let none_test = matches!(other.kind, ExprKind::None)
                    && matches!(op, CmpOp::Is | CmpOp::IsNot | CmpOp::Eq | CmpOp::NotEq);
                let Some(path) = path_of(first) else { return Facts::default() };
                if !none_test {
                    return Facts::default();
                }
                let current = self.path_type(first);
                let Ty::Optional(inner) = self.resolve(&current) else { return Facts::default() };
                let present = (path.clone(), (*inner).clone());
                if matches!(op, CmpOp::IsNot | CmpOp::NotEq) {
                    Facts { then: vec![present], otherwise: vec![] }
                } else {
                    Facts { then: vec![], otherwise: vec![present] }
                }
            }
            ExprKind::Not(inner) => {
                let facts = self.facts(inner);
                Facts { then: facts.otherwise, otherwise: facts.then }
            }
            ExprKind::Logical { op: BoolOp::And, operands } => {
                let mut then = Vec::new();
                for o in operands {
                    then.extend(self.facts(o).then);
                }
                Facts { then, otherwise: vec![] }
            }
            ExprKind::Logical { op: BoolOp::Or, operands } => {
                let mut otherwise = Vec::new();
                for o in operands {
                    otherwise.extend(self.facts(o).otherwise);
                }
                Facts { then: vec![], otherwise }
            }
            _ => Facts::default(),
        }
    }

    fn path_type(&mut self, expr: &Expr) -> Ty {
        let saved = self.diagnostics.len();
        let ty = self.expr(expr, None);
        self.diagnostics.truncate(saved);
        ty
    }

    // Expressions -------------------------------------------------------------------

    fn lower(&mut self, t: &TypeExpr) -> Ty {
        let scope: Vec<String> = self.type_params.keys().cloned().collect();
        // Lowering only reads the program's declarations; diagnostics go to this body.
        let mut scratch = Program {
            types: self.program.types.clone(),
            traits: self.program.traits.clone(),
            py_classes: self.program.py_classes.clone(),
            ..Program::default()
        };
        let ty = scratch.lower(t, &scope);
        self.diagnostics.extend(scratch.diagnostics);
        ty
    }

    /// The type of `expr`, checked against `expected` where one is known.
    pub fn expr(&mut self, expr: &Expr, expected: Option<&Ty>) -> Ty {
        let ty = self.expr_inner(expr, expected);
        let ty = self.resolve(&ty);
        // Resolving gives an error past TYPE_LIMIT, here or in what checking this expression
        // unified: reported at the expression, and checked on as an error so nothing is built on it.
        if self.infer.take_overflow() {
            self.too_large(expr.span, format!("this type has more than {TYPE_LIMIT} parts"));
        }
        let ty = self.kept(expr.span, ty);
        #[cfg(debug_assertions)]
        {
            let node = (std::ptr::from_ref(expr) as usize, matches!(expr.kind, ExprKind::Error));
            let before = self.typed_by.insert(expr.span, node);
            debug_assert!(
                before.is_none_or(|b| b.0 == node.0 || (b.1 && node.1)),
                "two expressions share the span {:?}, and the type map keeps one of their types",
                expr.span
            );
        }
        self.seen.insert(expr.span, ty.clone());
        ty
    }

    /// Report a field's format spec that the value it formats would refuse when the program runs
    /// (plans/frontend-robustness.md 2.2); a value whose type is not known yet is left to the run.
    fn format_spec(&mut self, value: &Expr, ty: &Ty, conversion: Option<char>, spec: &[StrPart]) {
        let text: String = spec
            .iter()
            .map(|p| match p {
                StrPart::Text(t) => t.as_str(),
                StrPart::Expr { .. } => "{…}",
            })
            .collect();
        let resolved = self.resolve(ty);
        let Some(kind) = crate::format::kind(&resolved, conversion) else { return };
        if let Some(why) = crate::format::check(&text, kind) {
            self.report(
                Diagnostic::error("E0223", value.span, format!("`{text}` is no format spec for this value: {why}"))
                    .note("the spec after `:` follows Python's format mini-language for the value's type"),
            );
        }
    }

    /// `ty`, counted against [`BODY_TYPES`]: an error once the body's types pass it, the first
    /// time reported.
    fn kept(&mut self, span: Span, ty: Ty) -> Ty {
        self.stored = self.stored.saturating_add(ty.size());
        if self.stored <= BODY_TYPES {
            return ty;
        }
        if !self.full {
            self.full = true;
            self.too_large(span, format!("the types of this function have more than {BODY_TYPES} parts in all"));
        }
        Ty::Error
    }

    fn too_large(&mut self, span: Span, message: String) {
        self.report(
            Diagnostic::error("E0222", span, message)
                .note("name the shape with a record type, or keep the values in a list"),
        );
    }

    /// The type of every expression in the body, as inference finally resolved it, in source
    /// order. A variable bound after an expression was checked can make its type larger than it
    /// was then, so the limits hold here again, each reported once.
    pub fn types(&mut self) -> Vec<(Span, Ty)> {
        let mut spans: Vec<Span> = self.seen.keys().copied().collect();
        spans.sort();
        self.stored = 0;
        let mut overflowed = false;
        let mut out = Vec::with_capacity(spans.len());
        for span in spans {
            let ty = self.infer.resolve(&self.seen[&span]);
            if self.infer.take_overflow() && !overflowed {
                overflowed = true;
                self.too_large(span, format!("this type has more than {TYPE_LIMIT} parts"));
            }
            let ty = self.kept(span, ty);
            out.push((span, ty));
        }
        out
    }

    /// Each name that resolved to a local, with the span of its declaration.
    pub fn locals(&self) -> &[(Span, Span)] {
        &self.locals
    }

    /// The overload each overloaded Python call was given, by the call's span.
    pub fn overloads(&self) -> &[(Span, usize)] {
        &self.overloads
    }

    /// Accept `found` where `expected` is wanted, allowing the coercions lotml has: a value
    /// where an optional is wanted, a type implementing a trait where `dyn` is.
    pub fn coerce(&mut self, found: &Ty, expected: &Ty, span: Span) -> bool {
        if self.fits(found, expected) {
            return true;
        }
        let (want, got) = (self.resolve(expected), self.resolve(found));
        if let Ty::Result(value, _) = &got
            && !matches!(want, Ty::Result(..))
            && self.fits(value, &want)
        {
            self.result_value(got.clone(), span);
            return false;
        }
        let mut d = Diagnostic::error("E0204", span, format!("expected `{want}`, found `{got}`"));
        if matches!(want, Ty::Float(_)) && matches!(got, Ty::Int(_)) {
            let text = self.source(span);
            if text.chars().all(|c| c.is_ascii_digit()) {
                d = d.fix(
                    format!("write `{text}.0`"),
                    Applicability::MachineApplicable,
                    vec![(span, format!("{text}.0"))],
                );
            } else {
                d = d.fix(
                    format!("write `float({text})`"),
                    Applicability::MachineApplicable,
                    vec![(span, format!("float({text})"))],
                );
            }
            d = d.note("lotml converts no type implicitly: an `int` becomes an `f64` with `float(n)`");
        }
        if matches!(got, Ty::Optional(_)) && self.infer.unify(&Ty::optional(want.clone()), &got) {
            d = Diagnostic::error(
                "E0207",
                span,
                format!("this is a `{got}`, which may be `None`; a `{want}` is expected"),
            )
            .note(
                "test it with `if x is not None:`, give a default with `x ?? default`, or fail with `x ?? fail error`",
            );
        }
        self.report(d);
        false
    }

    fn fits(&mut self, found: &Ty, expected: &Ty) -> bool {
        let (f, e) = (self.resolve(found), self.resolve(expected));
        match (&f, &e) {
            (Ty::Error | Ty::Never, _) | (_, Ty::Error) => true,
            (Ty::Optional(_), Ty::Optional(_)) => self.infer.unify(&f, &e),
            (_, Ty::Optional(inner)) if !matches!(f, Ty::Var(_)) => self.fits(&f, inner),
            (Ty::Adt(name, _), Ty::Dyn(trait_name)) => {
                self.program.implements.contains(&(trait_name.clone(), name.clone()))
            }
            // A Python class where a base its interface declares is expected (adr:0034), and where
            // any Python object is: it is one.
            (Ty::Adt(sub, _), Ty::Adt(base, _)) if sub != base && self.py_class(&f).is_some() => {
                self.py_lineage(sub).contains(base)
            }
            (Ty::Adt(..), Ty::PyObject) if self.py_class(&f).is_some() => true,
            (_, Ty::PyObject) if !matches!(f, Ty::Var(_)) => self.carried(&f),
            (_, e) if e.holds_py_object() && !matches!(f, Ty::Var(_)) => self.fits_shape(&f, e),
            (Ty::List(a), Ty::List(b)) if matches!(self.resolve(b), Ty::Dyn(_)) => {
                let item = self.resolve(a);
                self.fits(&item, b)
            }
            _ => self.infer.unify(&f, &e),
        }
    }

    /// An integer literal: an `int`, or the integer type expected where it fits — `limit: u8 =
    /// 200`, an `i32` argument — and reported where it does not.
    fn int_literal(&mut self, text: &str, span: Span, expected: Option<&Ty>) -> Ty {
        let wanted = match expected.map(|e| self.resolve(e)) {
            Some(Ty::Int(kind)) => kind,
            Some(Ty::Optional(inner)) => match *inner {
                Ty::Int(kind) => kind,
                _ => return INT,
            },
            _ => return INT,
        };
        if wanted == IntKind::I64 {
            return INT;
        }
        let digits = text.replace('_', "").to_ascii_lowercase();
        let (negative, lower) = match digits.strip_prefix('-') {
            Some(rest) => (true, rest.to_string()),
            None => (false, digits),
        };
        let value = match lower.get(..2) {
            Some("0x") => i128::from_str_radix(&lower[2..], 16),
            Some("0o") => i128::from_str_radix(&lower[2..], 8),
            Some("0b") => i128::from_str_radix(&lower[2..], 2),
            _ => lower.parse::<i128>(),
        }
        .map(|v| if negative { -v } else { v });
        let (low, high) = int_range(wanted);
        if value.is_ok_and(|v| v < low || v > high) {
            let ty = Ty::Int(wanted);
            self.report(Diagnostic::error(
                "E0204",
                span,
                format!("`{text}` does not fit in `{ty}`, from {low} to {high}"),
            ));
        }
        Ty::Int(wanted)
    }

    fn expr_inner(&mut self, expr: &Expr, expected: Option<&Ty>) -> Ty {
        match &expr.kind {
            ExprKind::Error => Ty::Error,
            ExprKind::Int(text) => self.int_literal(text, expr.span, expected),
            ExprKind::Float(_) => match expected.map(|e| self.resolve(e)) {
                Some(Ty::Float(kind)) => Ty::Float(kind),
                Some(Ty::Optional(inner)) if matches!(*inner, Ty::Float(_)) => *inner,
                _ => F64,
            },
            ExprKind::Str(literals) => {
                for literal in literals {
                    for part in &literal.parts {
                        if let StrPart::Expr { expr, conversion, spec } = part {
                            let ty = self.expr(expr, None);
                            if self.holds_object(&ty) {
                                self.refuse_python(expr.span, "a Python object cannot be formatted".into(), &ty);
                                continue;
                            }
                            self.format_spec(expr, &ty, *conversion, spec);
                        }
                    }
                }
                if literals.iter().any(|l| l.bytes) { Ty::Bytes } else { Ty::Str }
            }
            ExprKind::Bool(_) => Ty::Bool,
            ExprKind::None => match expected.map(|e| self.resolve(e)) {
                Some(Ty::Optional(t)) => Ty::Optional(t),
                Some(Ty::Unit) => Ty::Unit,
                _ => Ty::optional(self.infer.fresh()),
            },
            ExprKind::Unit => Ty::Unit,
            ExprKind::Name(name) => self.name(name, expr.span),
            ExprKind::Tuple(items) => {
                let expected_items = match expected.map(|e| self.resolve(e)) {
                    Some(Ty::Tuple(ts)) if ts.len() == items.len() => ts,
                    _ => vec![],
                };
                Ty::Tuple(items.iter().enumerate().map(|(i, e)| self.expr(e, expected_items.get(i))).collect())
            }
            ExprKind::List(items) => {
                let item = match expected.map(|e| self.resolve(e)) {
                    Some(Ty::List(t)) => *t,
                    _ => self.infer.fresh(),
                };
                for e in items {
                    let found = self.expr(e, Some(&item));
                    self.element(&found, &item, e.span);
                }
                Ty::list(item)
            }
            ExprKind::Set(items) => {
                let item = match expected.map(|e| self.resolve(e)) {
                    Some(Ty::Set(t)) => *t,
                    _ => self.infer.fresh(),
                };
                for e in items {
                    let found = self.expr(e, Some(&item));
                    self.element(&found, &item, e.span);
                    if self.holds_object(&found) {
                        self.refuse_python(
                            e.span,
                            "a set cannot hold a Python object: it would hash it".into(),
                            &found,
                        );
                    }
                }
                Ty::Set(Box::new(item))
            }
            ExprKind::Dict(pairs) => {
                let (k, v) = match expected.map(|e| self.resolve(e)) {
                    Some(Ty::Dict(k, v)) => (*k, *v),
                    _ => (self.infer.fresh(), self.infer.fresh()),
                };
                for (key, value) in pairs {
                    let found = self.expr(key, Some(&k));
                    self.element(&found, &k, key.span);
                    if self.holds_object(&found) {
                        self.refuse_python(
                            key.span,
                            "a dict's key cannot hold a Python object: it would hash it".into(),
                            &found,
                        );
                    }
                    let found = self.expr(value, Some(&v));
                    self.element(&found, &v, value.span);
                }
                Ty::Dict(Box::new(k), Box::new(v))
            }
            ExprKind::ListComp { element, loops } | ExprKind::Generator { element, loops } => {
                let item = self.comprehension(loops, |b| b.expr(element, None));
                Ty::list(item)
            }
            ExprKind::SetComp { element, loops } => {
                let item = self.comprehension(loops, |b| b.expr(element, None));
                Ty::Set(Box::new(item))
            }
            ExprKind::DictComp { key, value, loops } => {
                let pair = self.comprehension(loops, |b| Ty::Tuple(vec![b.expr(key, None), b.expr(value, None)]));
                match pair {
                    Ty::Tuple(kv) => Ty::Dict(Box::new(kv[0].clone()), Box::new(kv[1].clone())),
                    _ => Ty::Error,
                }
            }
            ExprKind::Unary { op: UnaryOp::Neg, operand }
                if matches!(operand.kind, ExprKind::Int(_))
                    && matches!(expected.map(|e| self.resolve(e)), Some(Ty::Int(_) | Ty::Optional(_))) =>
            {
                // `-128` is one literal for its range: an `i8`, though `128` alone is not.
                let ExprKind::Int(text) = &operand.kind else { unreachable!("matched above") };
                let ty = self.int_literal(&format!("-{text}"), expr.span, expected);
                self.seen.insert(operand.span, ty.clone());
                ty
            }
            ExprKind::Unary { op, operand } => {
                let numeric = expected.filter(|e| self.resolve(e).is_numeric());
                let ty = self.expr(operand, numeric);
                let ok = match op {
                    UnaryOp::Neg | UnaryOp::Pos => ty.is_numeric() || ty.is_poison() || matches!(ty, Ty::Var(_)),
                    UnaryOp::Invert => matches!(ty, Ty::Int(_) | Ty::Error | Ty::Never | Ty::Var(_)),
                };
                if !ok {
                    self.report(Diagnostic::error(
                        "E0204",
                        expr.span,
                        format!("this operator needs a number, not a `{ty}`"),
                    ));
                    return Ty::Error;
                }
                ty
            }
            ExprKind::Binary { op, left, right } => {
                let l = self.expr(left, None);
                let l = self.result_value(l, left.span);
                let r = self.expr(right, if l.is_numeric() { Some(&l) } else { None });
                let r = self.result_value(r, right.span);
                self.binary(*op, &l, &r, expr.span)
            }
            ExprKind::Compare { first, rest } => {
                let mut left = self.expr(first, None);
                let mut left_span = first.span;
                for (op, right) in rest {
                    let r = self.expr(right, None);
                    // A result compares with `Ok(v)` or `Err(e)`; ordering needs its value.
                    if matches!(op, CmpOp::Lt | CmpOp::Gt | CmpOp::Le | CmpOp::Ge | CmpOp::In | CmpOp::NotIn) {
                        left = self.result_value(left, left_span);
                    }
                    let r = if matches!(op, CmpOp::Lt | CmpOp::Gt | CmpOp::Le | CmpOp::Ge) {
                        self.result_value(r, right.span)
                    } else {
                        r
                    };
                    left_span = right.span;
                    self.compare(*op, &left, &r, expr.span);
                    left = r;
                }
                Ty::Bool
            }
            ExprKind::Logical { op, operands } => {
                let mut facts: Vec<(String, Ty)> = Vec::new();
                for operand in operands {
                    let f = self.with_scope(&facts, |b| b.condition(operand)).0;
                    facts.extend(if *op == BoolOp::And { f.then } else { f.otherwise });
                }
                Ty::Bool
            }
            ExprKind::Not(inner) => {
                let ty = self.expr(inner, Some(&Ty::Bool));
                if !self.infer.unify(&ty, &Ty::Bool) && !ty.is_poison() {
                    self.truthiness(expr, &ty);
                }
                Ty::Bool
            }
            ExprKind::Coalesce { value, default } => {
                let ty = self.expr(value, None);
                match ty {
                    Ty::Optional(inner) => {
                        let d = self.expr(default, Some(&inner));
                        self.coerce(&d, &inner, default.span);
                        *inner
                    }
                    Ty::Error | Ty::Never => {
                        self.expr(default, None);
                        Ty::Error
                    }
                    Ty::Var(_) => {
                        let inner = self.infer.fresh();
                        self.infer.unify(&ty, &Ty::optional(inner.clone()));
                        let d = self.expr(default, Some(&inner));
                        self.coerce(&d, &inner, default.span);
                        inner
                    }
                    other => {
                        self.expr(default, None);
                        let text = self.source(value.span).to_string();
                        // An optional already tested is only redundant here: a warning, not an error.
                        let narrowed = path_of(value).is_some_and(|p| self.narrowed(&p).is_some());
                        let d = if narrowed {
                            Diagnostic::warning(
                                "E0215",
                                expr.span,
                                format!("`{text}` was tested and is not `None` here, so `??` never applies"),
                            )
                        } else {
                            Diagnostic::error(
                                "E0215",
                                expr.span,
                                format!(
                                    "`??` gives the value inside an optional, and this is a `{other}`, never `None`"
                                ),
                            )
                        };
                        self.report(d.fix(
                            format!("write `{text}`"),
                            Applicability::MachineApplicable,
                            vec![(expr.span, text)],
                        ));
                        other
                    }
                }
            }
            ExprKind::IfExp { test, then, orelse } => {
                let facts = self.condition(test);
                let a = self.with_scope(&facts.then, |b| b.expr(then, expected)).0;
                let b = self.with_scope(&facts.otherwise, |b| b.expr(orelse, expected)).0;
                self.join(&a, &b, expr.span)
            }
            ExprKind::Lambda { params, body } => {
                let (param_types, ret) = match expected.map(|e| self.resolve(e)) {
                    Some(Ty::Func(ps, r)) if ps.len() == params.len() => (ps, Some(*r)),
                    _ => (params.iter().map(|_| self.infer.fresh()).collect(), None),
                };
                let body_ty = self
                    .with_scope(&[], |b| {
                        b.capture();
                        for (p, t) in params.iter().zip(&param_types) {
                            b.declare(p, t.clone(), false, None);
                        }
                        b.expr(body, ret.as_ref())
                    })
                    .0;
                if let Some(r) = &ret
                    && !matches!(r, Ty::Error)
                {
                    self.coerce(&body_ty, r, body.span);
                }
                Ty::Func(param_types, Box::new(body_ty))
            }
            ExprKind::Call { func, args } => self.call(func, args, expr.span, expected),
            ExprKind::Index { object, index } => {
                let ty = self.expr(object, None);
                let ty = self.result_value(ty, object.span);
                let ty = self.present(ty, object.span);
                self.index(&ty, index, expr.span)
            }
            ExprKind::Slice { object, lower, upper, step } => {
                let ty = self.expr(object, None);
                for part in [lower, upper, step].into_iter().flatten() {
                    let t = self.expr(part, Some(&INT));
                    self.coerce(&t, &INT, part.span);
                }
                match ty {
                    Ty::List(_) | Ty::Str | Ty::Bytes | Ty::Tuple(_) | Ty::Error | Ty::Never | Ty::Var(_) => ty,
                    other => {
                        self.report(Diagnostic::error("E0204", expr.span, format!("a `{other}` cannot be sliced")));
                        Ty::Error
                    }
                }
            }
            ExprKind::Attr { object, name } => self.attribute(object, name, expr.span),
            ExprKind::Try(inner) => {
                // `o.value()?` takes its type from where the value goes (specs/python-object R2.1).
                let value_call = matches!(&inner.kind, ExprKind::Call { func, .. }
                    if matches!(&func.kind, ExprKind::Attr { name, .. } if name.name == "value"));
                let ty = self.expr(inner, if value_call { expected } else { None });
                match ty {
                    Ty::Result(value, error) => {
                        if !self.in_test {
                            match self.error.clone() {
                                Some(own) => {
                                    if !self.infer.unify(&error, &own) {
                                        let (e, own) = (self.resolve(&error), self.resolve(&own));
                                        self.report(Diagnostic::error(
                                            "E0214",
                                            expr.span,
                                            format!("`?` passes on a `{e}`, but this function fails with `{own}`"),
                                        ));
                                    }
                                }
                                None => self.report(
                                    Diagnostic::error(
                                        "E0214",
                                        expr.span,
                                        "`?` returns the error from this function, which cannot fail",
                                    )
                                    .note("declare the error type: `-> T ! E`, or match on `Ok(v)` and `Err(e)`"),
                                ),
                            }
                        }
                        *value
                    }
                    Ty::Error | Ty::Never | Ty::Var(_) => Ty::Error,
                    other => {
                        self.report(Diagnostic::error(
                            "E0214",
                            expr.span,
                            format!("`?` unwraps a result, and this is a `{other}`"),
                        ));
                        Ty::Error
                    }
                }
            }
            ExprKind::Fail(error) => {
                let ty = self.expr(error, self.error.clone().as_ref());
                match self.error.clone() {
                    Some(own) if !self.in_test => {
                        if !self.infer.unify(&ty, &own) {
                            let (e, own) = (self.resolve(&ty), self.resolve(&own));
                            self.report(Diagnostic::error(
                                "E0214",
                                expr.span,
                                format!("`fail` returns a `{e}`, but this function fails with `{own}`"),
                            ));
                        }
                    }
                    _ if self.in_test => {}
                    _ => self.report(
                        Diagnostic::error(
                            "E0214",
                            expr.span,
                            "`fail` returns an error from this function, which cannot fail",
                        )
                        .note("declare the error type: `-> T ! E`"),
                    ),
                }
                Ty::Never
            }
        }
    }

    fn element(&mut self, found: &Ty, item: &Ty, span: Span) {
        if !self.fits(found, item) {
            let (a, b) = (self.resolve(item), self.resolve(found));
            self.report(Diagnostic::error(
                "E0204",
                span,
                format!("the other elements are `{a}`, and this one is `{b}`"),
            ));
        }
    }

    fn join(&mut self, a: &Ty, b: &Ty, span: Span) -> Ty {
        let (ra, rb) = (self.resolve(a), self.resolve(b));
        if matches!(ra, Ty::Never) {
            return rb;
        }
        if self.infer.unify(&ra, &rb) {
            return self.resolve(&ra);
        }
        if matches!(rb, Ty::Optional(_)) && self.fits(&ra, &rb) {
            return rb;
        }
        if matches!(ra, Ty::Optional(_)) && self.fits(&rb, &ra) {
            return ra;
        }
        self.report(Diagnostic::error("E0204", span, format!("the two branches give `{ra}` and `{rb}`")));
        Ty::Error
    }

    fn comprehension(&mut self, loops: &[Comprehension], element: impl FnOnce(&mut Self) -> Ty) -> Ty {
        self.with_scope(&[], |b| {
            for l in loops {
                let iterable = b.expr(&l.iter, None);
                let item = builtins::element(&iterable, &mut b.infer).unwrap_or_else(|| {
                    let d = Diagnostic::error("E0204", l.iter.span, format!("`{iterable}` cannot be iterated"));
                    let d = b.noting(d, &iterable);
                    b.report(d);
                    Ty::Error
                });
                b.bind_target(&l.target, &item);
                for c in &l.conditions {
                    let facts = b.condition(c);
                    let scope = b.scopes.last_mut().expect("a scope");
                    for (path, ty) in facts.then {
                        scope.narrowed.insert(path, ty);
                    }
                }
            }
            element(b)
        })
        .0
    }

    fn name(&mut self, name: &str, span: Span) -> Ty {
        if let Some(local) = self.lookup(name) {
            self.locals.push((span, local.span));
        }
        if let Some(ty) = self.narrowed(name) {
            return ty;
        }
        if let Some(local) = self.lookup(name).cloned() {
            if local.moved {
                self.report(Diagnostic::error(
                    "E0306",
                    span,
                    format!("`{name}` was given to a `sink` parameter and can no longer be used"),
                ));
            }
            return local.ty;
        }
        if let Some(sig) = self.program.functions.get(name) {
            return self.named_function(sig, span);
        }
        if let Some(owner) = self.program.variant_of.get(name).cloned() {
            return self.variant_value(name, &owner, span);
        }
        if self.program.types.contains_key(name) {
            return Ty::TypeName(name.to_string());
        }
        if let Some(module) = self.program.imported.get(name) {
            return builtins::module_member(module, name).unwrap_or(Ty::Error);
        }
        if self.program.modules.contains(name) || self.is_package(name) {
            return Ty::Module(name.to_string());
        }
        if builtins::is_prelude(name) {
            return Ty::Func(vec![], Box::new(Ty::Error));
        }
        let constant = match name {
            "true" => Some("True"),
            "false" => Some("False"),
            "null" | "nil" | "undefined" | "none" => Some("None"),
            _ => None,
        };
        if let Some(constant) = constant {
            self.report(
                Diagnostic::error("E0201", span, format!("`{name}` is not defined: lotml writes `{constant}`"))
                    .alternatives([constant.to_string()])
                    .fix(
                        format!("write `{constant}`"),
                        Applicability::MachineApplicable,
                        vec![(span, constant.into())],
                    ),
            );
            return match constant {
                "None" => Ty::optional(self.infer.fresh()),
                _ => Ty::Bool,
            };
        }
        let names = self.all_names();
        self.report(
            Diagnostic::error("E0201", span, format!("`{name}` is not defined"))
                .alternatives(closest(name, &names))
                .note("a name is in scope after the statement that declares it"),
        );
        // Declare it as unknown so the same mistake is reported once.
        self.locals.push((span, span));
        self.scopes[0].locals.insert(
            name.to_string(),
            Local { ty: Ty::Error, mutable: true, span, declared_at: None, moved: false, origin: Origin::Declared },
        );
        Ty::Error
    }

    fn variant_value(&mut self, name: &str, owner: &str, span: Span) -> Ty {
        let Some(TypeDef::Sum { params, variants }) = self.program.types.get(owner) else { return Ty::Error };
        let args: Vec<Ty> = params.iter().map(|_| self.infer.fresh()).collect();
        let variant = variants.iter().find(|v| v.name == name);
        match variant.and_then(|v| v.fields.as_ref()) {
            None => Ty::Adt(owner.to_string(), args),
            Some(fields) => {
                let _ = span;
                Ty::Func(
                    fields.iter().map(|f| f.ty.substitute(params, &args)).collect(),
                    Box::new(Ty::Adt(owner.to_string(), args)),
                )
            }
        }
    }

    /// The value of a result used where a value is needed, reporting the missing unwrap.
    fn result_value(&mut self, ty: Ty, span: Span) -> Ty {
        let Ty::Result(value, error) = ty else { return ty };
        let text = self.source(span).to_string();
        let d = Diagnostic::error(
            "E0219",
            span,
            format!(
                "`{text}` is a `{} ! {}`, not a `{}`",
                self.resolve(&value),
                self.resolve(&error),
                self.resolve(&value)
            ),
        );
        let d = self.unwrap_fix(d, &error, span);
        self.report(d);
        *value
    }

    /// `?` after `span` when this function can pass the error on; otherwise what to do instead.
    fn unwrap_fix(&mut self, d: Diagnostic, error: &Ty, span: Span) -> Diagnostic {
        let passes = self.in_test || self.error.clone().is_some_and(|own| self.infer.unify(error, &own));
        if passes {
            d.fix(
                "unwrap it with `?`",
                Applicability::MachineApplicable,
                vec![(Span { start: span.end, end: span.end }, "?".into())],
            )
        } else {
            d.note("this function cannot pass the error on: `match` on `Ok(v)` and `Err(e)`, or declare `-> T ! E`")
        }
    }

    /// The value inside an optional used where a value is needed, reporting that it may be `None`.
    fn present(&mut self, ty: Ty, span: Span) -> Ty {
        match ty {
            Ty::Optional(inner) => {
                self.report(
                    Diagnostic::error("E0207", span, format!("this is a `{}?`, which may be `None`", self.resolve(&inner)))
                        .note("test it with `if x is not None:`, give a default with `x ?? default`, or fail with `x ?? fail error`"),
                );
                *inner
            }
            other => other,
        }
    }

    fn binary(&mut self, op: BinOp, l: &Ty, r: &Ty, span: Span) -> Ty {
        let (l, r) = (self.resolve(l), self.resolve(r));
        if self.is_python(&l) || self.is_python(&r) {
            let python = if self.is_python(&l) { l.clone() } else { r.clone() };
            self.refuse_python(span, format!("`{}` has no operands of type `{python}`", op.text()), &python);
            return Ty::Error;
        }
        if matches!(l, Ty::Error) || matches!(r, Ty::Error) {
            return Ty::Error;
        }
        if l.is_poison() || r.is_poison() {
            return if l.is_poison() { r } else { l };
        }
        // One report per operator, even when both operands are optional.
        let (l, r) = match (l, r) {
            (Ty::Optional(l), Ty::Optional(r)) => (self.present(Ty::Optional(l), span), *r),
            (l, r) => (self.present(l, span), self.present(r, span)),
        };
        let mismatch = |b: &mut Self| {
            let mut d = Diagnostic::error("E0204", span, format!("`{}` cannot combine `{l}` and `{r}`", op.text()));
            if (matches!(l, Ty::Int(_)) && matches!(r, Ty::Float(_)))
                || (matches!(l, Ty::Float(_)) && matches!(r, Ty::Int(_)))
            {
                d = d.note("lotml converts no type implicitly: write `float(n)` to make an `int` an `f64`");
            }
            b.report(d);
            Ty::Error
        };
        match op {
            BinOp::Div => {
                if matches!((&l, &r), (Ty::Int(_), Ty::Int(_))) {
                    return F64;
                }
                if l.is_numeric() && self.infer.unify(&l, &r) {
                    return l;
                }
                if matches!(l, Ty::Var(_)) || matches!(r, Ty::Var(_)) {
                    self.infer.unify(&l, &r);
                    return l;
                }
                mismatch(self)
            }
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::FloorDiv | BinOp::Mod | BinOp::Pow => {
                if op == BinOp::Mul && matches!((&l, &r), (Ty::Str | Ty::List(_), Ty::Int(_))) {
                    return l;
                }
                if op == BinOp::Mul && matches!((&l, &r), (Ty::Int(_), Ty::Str | Ty::List(_))) {
                    return r;
                }
                if op == BinOp::Add
                    && matches!(l, Ty::Str | Ty::List(_) | Ty::Tuple(_) | Ty::Bytes)
                    && self.infer.unify(&l, &r)
                {
                    return self.resolve(&l);
                }
                if op == BinOp::Sub && matches!(l, Ty::Set(_)) && self.infer.unify(&l, &r) {
                    return l;
                }
                if op == BinOp::Pow && matches!((&l, &r), (Ty::Int(_) | Ty::Float(_), Ty::Int(_))) {
                    return l;
                }
                if (l.is_numeric() || matches!(l, Ty::Var(_))) && self.infer.unify(&l, &r) {
                    return self.resolve(&l);
                }
                mismatch(self)
            }
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => {
                if matches!(l, Ty::Set(_) | Ty::Int(_) | Ty::Bool | Ty::Var(_)) && self.infer.unify(&l, &r) {
                    return self.resolve(&l);
                }
                mismatch(self)
            }
            BinOp::LShift | BinOp::RShift => {
                if matches!(l, Ty::Int(_) | Ty::Var(_)) && matches!(r, Ty::Int(_) | Ty::Var(_)) {
                    return l;
                }
                mismatch(self)
            }
        }
    }

    fn compare(&mut self, op: CmpOp, l: &Ty, r: &Ty, span: Span) {
        let (l, r) = (self.resolve(l), self.resolve(r));
        if self.holds_object(&l) || self.holds_object(&r) {
            let python = if self.holds_object(&l) { l.clone() } else { r.clone() };
            self.refuse_python(
                span,
                format!("`{}` cannot compare a Python object, nor a value holding one", op.text()),
                &python,
            );
            return;
        }
        if l.is_poison() || r.is_poison() {
            return;
        }
        match op {
            CmpOp::In | CmpOp::NotIn => {
                let item = match &r {
                    Ty::Str => Some(Ty::Str),
                    other => builtins::element(other, &mut self.infer),
                };
                match item {
                    Some(item) => {
                        if !self.infer.unify(&item, &l) && !self.fits(&l, &item) {
                            let item = self.resolve(&item);
                            self.report(Diagnostic::error(
                                "E0204",
                                span,
                                format!("`in` looks for a `{item}` here, not a `{l}`"),
                            ));
                        }
                    }
                    None => self.report(Diagnostic::error(
                        "E0204",
                        span,
                        format!("`in` needs a collection or a string on the right, not a `{r}`"),
                    )),
                }
            }
            CmpOp::Is | CmpOp::IsNot => {}
            CmpOp::Lt | CmpOp::Gt | CmpOp::Le | CmpOp::Ge
                if matches!(l, Ty::Optional(_)) || matches!(r, Ty::Optional(_)) =>
            {
                let (l, r) = match (l, r) {
                    (Ty::Optional(l), Ty::Optional(r)) => (self.present(Ty::Optional(l), span), *r),
                    (l, r) => (self.present(l, span), self.present(r, span)),
                };
                self.compare(op, &l, &r, span);
            }
            _ => {
                if self.infer.unify(&l, &r) || self.fits(&l, &r) || self.fits(&r, &l) {
                    return;
                }
                let mut d = Diagnostic::error(
                    "E0204",
                    span,
                    format!("`{}` compares values of one type, not `{l}` and `{r}`", op.text()),
                );
                if l.is_numeric() && r.is_numeric() {
                    d = d.note("write `float(n)` to compare an `int` with an `f64`");
                }
                self.report(d);
            }
        }
    }

    fn index(&mut self, ty: &Ty, index: &Expr, span: Span) -> Ty {
        match self.resolve(ty) {
            Ty::List(t) => {
                let i = self.expr(index, Some(&INT));
                self.coerce(&i, &INT, index.span);
                *t
            }
            Ty::Str | Ty::Bytes => {
                let i = self.expr(index, Some(&INT));
                self.coerce(&i, &INT, index.span);
                if matches!(self.resolve(ty), Ty::Bytes) { INT } else { Ty::Str }
            }
            Ty::Dict(k, v) => {
                let i = self.expr(index, Some(&k));
                self.coerce(&i, &k, index.span);
                *v
            }
            Ty::Tuple(items) => {
                self.expr(index, Some(&INT));
                let position = match &index.kind {
                    ExprKind::Int(text) => text.parse::<i64>().ok(),
                    ExprKind::Unary { op: UnaryOp::Neg, operand } => match &operand.kind {
                        ExprKind::Int(text) => text.parse::<i64>().ok().map(|n| -n),
                        _ => None,
                    },
                    _ => None,
                };
                match position {
                    Some(p) => {
                        let at = if p < 0 { items.len() as i64 + p } else { p };
                        items.get(at as usize).cloned().unwrap_or_else(|| {
                            self.report(Diagnostic::error(
                                "E0204",
                                span,
                                format!("this tuple has {} items", items.len()),
                            ));
                            Ty::Error
                        })
                    }
                    None if !items.is_empty() && items.iter().all(|t| *t == items[0]) => items[0].clone(),
                    None => {
                        self.report(Diagnostic::error(
                            "E0204",
                            span,
                            "a tuple of mixed types is indexed by a literal position",
                        ));
                        Ty::Error
                    }
                }
            }
            Ty::Var(_) | Ty::Error | Ty::Never => {
                self.expr(index, None);
                Ty::Error
            }
            other => {
                self.expr(index, None);
                self.report(Diagnostic::error("E0204", span, format!("a `{other}` cannot be indexed")));
                Ty::Error
            }
        }
    }

    fn attribute(&mut self, object: &Expr, name: &Ident, span: Span) -> Ty {
        if let Some(path) =
            path_of(&Expr { span, kind: ExprKind::Attr { object: Box::new(object.clone()), name: name.clone() } })
            && let Some(ty) = self.narrowed(&path)
        {
            return ty;
        }
        let ty = self.expr(object, None);
        let ty = self.result_value(ty, object.span);
        let ty = self.present(ty, object.span);
        if matches!(ty, Ty::PyObject) {
            self.opaque("E0205", name.span, format!("a `PyObject` has no attribute `{}`", name.name));
            return Ty::Error;
        }
        if let Some(class) = self.py_class(&ty) {
            if let Some(attribute) = self.py_attribute(&class, &name.name) {
                return Ty::Result(Box::new(attribute), Box::new(Ty::Adt("PyError".into(), vec![])));
            }
            let names = self.py_members(&class);
            let mut d = Diagnostic::error("E0205", name.span, format!("`{class}` has no attribute `{}`", name.name))
                .alternatives(closest(&name.name, &names));
            if self.py_method(&class, &name.name).is_some() {
                d = d.note(format!("`{}` is a method: call it, `{}(…)`", name.name, name.name));
            }
            self.report(d);
            return Ty::Error;
        }
        match &ty {
            Ty::Adt(type_name, args) => match self.program.types.get(type_name) {
                Some(TypeDef::Record { params, fields }) => {
                    match fields.iter().find(|f| f.name.as_deref() == Some(&name.name)) {
                        Some(field) => field.ty.substitute(params, args),
                        None => {
                            let fields: Vec<String> = fields.iter().filter_map(|f| f.name.clone()).collect();
                            self.report(
                                Diagnostic::error(
                                    "E0205",
                                    name.span,
                                    format!("`{type_name}` has no field `{}`", name.name),
                                )
                                .alternatives(fields),
                            );
                            Ty::Error
                        }
                    }
                }
                _ => {
                    self.report(
                        Diagnostic::error(
                            "E0205",
                            name.span,
                            format!("`{type_name}` is a sum type: match on it to reach its fields"),
                        )
                        .note("`match value:` with a `case Variant(field):` arm binds the field"),
                    );
                    Ty::Error
                }
            },
            Ty::Module(module) if self.is_package(&format!("{module}.{}", name.name)) => {
                Ty::Module(format!("{module}.{}", name.name))
            }
            Ty::Module(module) if self.program.modules.contains(&format!("{module}.{}", name.name)) => {
                Ty::Module(format!("{module}.{}", name.name))
            }
            Ty::Module(module) if self.program.foreign.contains_key(module) => {
                match self.program.foreign[module].get(&name.name) {
                    Some(sig) => self.named_function(&sig.clone(), name.span),
                    None => {
                        let names: Vec<String> = self.program.foreign[module].keys().cloned().collect();
                        self.report(
                            Diagnostic::error(
                                "E0205",
                                name.span,
                                format!("`{module}`'s interface has no `{}`", name.name),
                            )
                            .alternatives(closest(&name.name, &names)),
                        );
                        Ty::Error
                    }
                }
            }
            Ty::Module(module) => builtins::module_member(module, &name.name).unwrap_or_else(|| {
                self.report(
                    Diagnostic::error("E0205", name.span, format!("`{module}` has no `{}`", name.name)).alternatives(
                        closest(
                            &name.name,
                            &builtins::module_members(module).iter().map(ToString::to_string).collect::<Vec<_>>(),
                        ),
                    ),
                );
                Ty::Error
            }),
            Ty::Never => Ty::Error,
            Ty::Error => {
                if !self.has_error() {
                    self.unknown_member(name, "the field", "read");
                }
                Ty::Error
            }
            Ty::Var(_) => {
                // The value's type was never pinned, so its fields are not known either. Reading
                // one anyway is how a lambda parameter reached `.__class__`.
                self.unknown_member(name, "the field", "read");
                Ty::Error
            }
            other => {
                let mut names: Vec<String> = builtins::methods_of(other).iter().map(ToString::to_string).collect();
                if let Ty::Adt(n, _) = other {
                    names.extend(
                        self.program.methods.get(n).map(|m| m.keys().cloned().collect::<Vec<_>>()).unwrap_or_default(),
                    );
                }
                self.report(
                    Diagnostic::error("E0205", name.span, format!("a `{other}` has no field `{}`", name.name))
                        .alternatives(closest(&name.name, &names)),
                );
                Ty::Error
            }
        }
    }

    // Calls ------------------------------------------------------------------------

    fn call(&mut self, func: &Expr, args: &[Arg], span: Span, expected: Option<&Ty>) -> Ty {
        match &func.kind {
            ExprKind::Name(name) if self.lookup(name).is_none() && self.narrowed(name).is_none() => {
                if let Some(sig) = self.program.functions.get(name).cloned() {
                    return self.call_signature(&sig, &[], &[], args, span, None);
                }
                if self.program.types.contains_key(name) && !self.program.variant_of.contains_key(name) {
                    return self.construct(name, args, span, expected);
                }
                if let Some(owner) = self.program.variant_of.get(name).cloned() {
                    return self.construct_variant(name, &owner, args, span, expected);
                }
                if self.program.imported.contains_key(name) {
                    let ty = self.name(name, func.span);
                    return self.call_value(&ty, args, span, Some(name));
                }
                if builtins::is_prelude(name) {
                    return self.call_builtin(name, args, span, expected);
                }
                let ty = self.name(name, func.span);
                self.call_value(&ty, args, span, None)
            }
            ExprKind::Attr { object, name } => self.method_call(object, name, args, span, expected),
            ExprKind::Index { object, index } => {
                let generic = match &object.kind {
                    ExprKind::Name(n) if self.lookup(n).is_none() => Some(n.clone()),
                    _ => None,
                };
                let record =
                    generic.as_ref().filter(|n| matches!(self.program.types.get(*n), Some(TypeDef::Record { .. })));
                if let Some(name) = record {
                    let given = self.type_arguments(index);
                    let wanted = self.program.types[name].params().len();
                    if given.len() != wanted {
                        self.report(Diagnostic::error(
                            "E0202",
                            index.span,
                            format!("`{name}` takes {wanted} type argument{}", if wanted == 1 { "" } else { "s" }),
                        ));
                        return self.construct(name, args, span, None);
                    }
                    return self.construct(name, args, span, Some(&Ty::Adt(name.clone(), given)));
                }
                if let Some(sig) = generic.as_ref().and_then(|n| self.program.functions.get(n)).cloned() {
                    let given = self.type_arguments(index);
                    if given.len() != sig.type_params.len() {
                        self.report(Diagnostic::error(
                            "E0202",
                            index.span,
                            format!("`{}` takes {} type arguments", sig.name, sig.type_params.len()),
                        ));
                        return self.call_signature(&sig, &[], &[], args, span, None);
                    }
                    for ((name, bound), ty) in sig.type_params.iter().zip(&given) {
                        if let Some(bound) = bound
                            && !self.program.satisfies(ty, bound, &self.type_params)
                        {
                            self.report(Diagnostic::error(
                                "E0213",
                                index.span,
                                format!("`{ty}` does not implement `{bound}`, which `{name}` of `{}` needs", sig.name),
                            ));
                        }
                    }
                    let names: Vec<String> = sig.type_params.iter().map(|(n, _)| n.clone()).collect();
                    return self.call_signature(&sig, &names, &given, args, span, None);
                }
                let ty = self.expr(func, None);
                self.call_value(&ty, args, span, None)
            }
            _ => {
                let ty = self.expr(func, None);
                self.call_value(&ty, args, span, None)
            }
        }
    }

    /// The type arguments in `Stack[int]`, which the parser read as an index expression.
    fn type_arguments(&mut self, index: &Expr) -> Vec<Ty> {
        match &index.kind {
            ExprKind::Tuple(items) => items.iter().map(|i| self.type_of(i)).collect(),
            _ => vec![self.type_of(index)],
        }
    }

    fn type_of(&mut self, expr: &Expr) -> Ty {
        let not_a_type = |b: &mut Self| {
            let text = b.source(expr.span).to_string();
            b.report(Diagnostic::error("E0202", expr.span, format!("`{text}` is not a type")));
            Ty::Error
        };
        match &expr.kind {
            ExprKind::Name(n) => {
                if let Some(t) = Ty::primitive(n) {
                    t
                } else if self.type_params.contains_key(n) {
                    Ty::Param(n.clone())
                } else if let Some(def) = self.program.types.get(n) {
                    Ty::Adt(n.clone(), def.params().iter().map(|_| self.infer.fresh()).collect())
                } else {
                    not_a_type(self)
                }
            }
            ExprKind::None => Ty::Unit,
            ExprKind::List(items) if items.len() == 1 => Ty::list(self.type_of(&items[0])),
            ExprKind::Set(items) if items.len() == 1 => Ty::Set(Box::new(self.type_of(&items[0]))),
            ExprKind::Dict(pairs) if pairs.len() == 1 => {
                Ty::Dict(Box::new(self.type_of(&pairs[0].0)), Box::new(self.type_of(&pairs[0].1)))
            }
            ExprKind::Tuple(items) => Ty::Tuple(items.iter().map(|i| self.type_of(i)).collect()),
            ExprKind::Index { object, index } => match &object.kind {
                ExprKind::Name(n) if n == "Heap" => Ty::Heap(Box::new(self.type_of(index))),
                ExprKind::Name(n) if self.program.types.contains_key(n) => {
                    let args = self.type_arguments(index);
                    Ty::Adt(n.clone(), args)
                }
                _ => not_a_type(self),
            },
            _ => not_a_type(self),
        }
    }

    fn arg_types(&mut self, args: &[Arg], expected: &[Option<Ty>]) -> Vec<Ty> {
        args.iter().enumerate().map(|(i, a)| self.expr(a.expr(), expected.get(i).cloned().flatten().as_ref())).collect()
    }

    fn call_value(&mut self, ty: &Ty, args: &[Arg], span: Span, module_name: Option<&str>) -> Ty {
        match self.resolve(ty) {
            Ty::Func(params, ret) => {
                let expected: Vec<Option<Ty>> = params.iter().map(|p| Some(p.clone())).collect();
                let found = self.arg_types(args, &expected);
                if found.len() != params.len() {
                    self.report(Diagnostic::error(
                        "E0203",
                        span,
                        format!(
                            "this takes {} argument{}, and {} were given",
                            params.len(),
                            if params.len() == 1 { "" } else { "s" },
                            found.len()
                        ),
                    ));
                    return *ret;
                }
                for ((p, f), a) in params.iter().zip(&found).zip(args) {
                    if *p == Ty::Param("Num".into()) {
                        if !f.is_numeric() && !f.is_poison() && !matches!(f, Ty::Var(_)) {
                            self.report(Diagnostic::error(
                                "E0204",
                                a.expr().span,
                                format!("`{}` takes a number, not a `{f}`", module_name.unwrap_or("this")),
                            ));
                        }
                        continue;
                    }
                    self.coerce(f, p, a.expr().span);
                }
                *ret
            }
            Ty::Error | Ty::Never | Ty::Var(_) => {
                self.arg_types(args, &[]);
                Ty::Error
            }
            other => {
                self.arg_types(args, &[]);
                self.report(Diagnostic::error("E0212", span, format!("a `{other}` cannot be called")));
                Ty::Error
            }
        }
    }

    fn call_builtin(&mut self, name: &str, args: &[Arg], span: Span, expected: Option<&Ty>) -> Ty {
        let mut types = Vec::new();
        let mut keywords = Vec::new();
        // Values first, then lambdas, which are checked against what the values imply.
        let mut pending = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            match arg {
                Arg::Keyword(k, e) if matches!(e.kind, ExprKind::Lambda { .. }) => {
                    pending.push((i, Some(k.name.clone()), e))
                }
                Arg::Positional(e) if matches!(e.kind, ExprKind::Lambda { .. }) => {
                    types.push(Ty::Error);
                    pending.push((i, None, e));
                }
                Arg::Keyword(k, e) => {
                    let t = self.expr(e, None);
                    keywords.push((k.name.clone(), t));
                }
                Arg::Positional(e) => {
                    let wanted = if name == "Ok" || name == "Err" { None } else { expected.cloned().filter(|_| false) };
                    types.push(self.expr(e, wanted.as_ref()));
                }
                Arg::Inout(e, amp) => {
                    self.expr(e, None);
                    self.report(Diagnostic::error("E0305", *amp, format!("`{name}` takes no `inout` argument")).fix(
                        "remove the `&`",
                        Applicability::MachineApplicable,
                        vec![(*amp, String::new())],
                    ));
                    types.push(Ty::Error);
                }
            }
        }
        let positional_values: Vec<Ty> = types.iter().filter(|t| **t != Ty::Error).cloned().collect();
        for (i, keyword, e) in pending {
            let position = args[..i].iter().filter(|a| matches!(a, Arg::Positional(_))).count();
            let want = builtins::lambda_expectation(
                name,
                keyword.as_deref(),
                position,
                &positional_values_for(name, &positional_values, &types),
                &mut self.infer,
            );
            let t = self.expr(e, want.as_ref());
            match keyword {
                Some(k) => keywords.push((k, t)),
                None => types[position] = t,
            }
        }
        // These hand a value to Python's own `repr`, `str`, `==`, `<`, `hash` or `+`.
        let into_python = matches!(name, "print" | "str" | "repr" | "sorted" | "min" | "max" | "sum" | "set");
        let opaque = types
            .iter()
            .chain(keywords.iter().map(|(_, t)| t))
            .find(|t| self.is_python(t) || (into_python && self.holds_object(t)))
            .cloned();
        if let Some(python) = opaque
            && !matches!(name, "Ok" | "Err")
        {
            self.refuse_python(span, format!("`{name}` cannot take a Python object"), &python);
            return Ty::Error;
        }
        match builtins::call(name, &types, &keywords, &mut self.infer) {
            Ok(ty) => {
                if let Some(e) = expected
                    && (name == "Ok"
                        || name == "Err"
                        || matches!(ty, Ty::List(_) | Ty::Set(_) | Ty::Dict(..) | Ty::Heap(_) | Ty::Optional(_)))
                {
                    let resolved = self.resolve(e);
                    if !matches!(resolved, Ty::Optional(_) | Ty::Dyn(_)) {
                        self.infer.unify(&ty, &resolved);
                    }
                }
                ty
            }
            Err(message) => {
                let code = if message.contains("takes") && message.contains("argument") { "E0203" } else { "E0204" };
                let mut d = Diagnostic::error(code, span, message);
                // Python's habit, `gather(f(a), f(b))`: the calls given where tasks are wanted.
                let calls: Vec<&Expr> = args
                    .iter()
                    .filter_map(|a| match a {
                        Arg::Positional(e) if matches!(e.kind, ExprKind::Call { .. }) => Some(e),
                        _ => None,
                    })
                    .collect();
                if name == "parallel" && !calls.is_empty() && calls.len() == args.len() {
                    let tasks: Vec<String> = calls.iter().map(|e| format!("lambda: {}", self.source(e.span))).collect();
                    let whole = calls[0].span.to(calls[calls.len() - 1].span);
                    d = d.fix(
                        "make each call a task",
                        Applicability::MaybeIncorrect,
                        vec![(whole, format!("[{}]", tasks.join(", ")))],
                    );
                }
                // `map`'s habit, `[lambda k: f(k) for k in keys]`: a task given the loop's
                // variable as a parameter, where it should capture it.
                if let (true, [Arg::Positional(list)]) = (name == "parallel", args)
                    && let ExprKind::ListComp { element, loops } = &list.kind
                    && let ExprKind::Lambda { params, body } = &element.kind
                    && !params.is_empty()
                    && params.iter().all(|p| loops.iter().any(|l| l.target.names().iter().any(|n| n.name == p.name)))
                {
                    d = d.fix(
                        "capture the loop's variable instead: a task takes no parameter",
                        Applicability::MaybeIncorrect,
                        vec![(Span { start: element.span.start, end: body.span.start }, "lambda: ".into())],
                    );
                }
                self.report(d);
                Ty::Error
            }
        }
    }

    fn construct(&mut self, name: &str, args: &[Arg], span: Span, expected: Option<&Ty>) -> Ty {
        let Some(TypeDef::Record { params, fields }) = self.program.types.get(name).cloned() else { return Ty::Error };
        let type_args: Vec<Ty> = match expected.map(|e| self.resolve(e)) {
            Some(Ty::Adt(n, a)) if n == name => a,
            _ => params.iter().map(|_| self.infer.fresh()).collect(),
        };
        let fields: Vec<(String, Ty, bool)> = fields
            .iter()
            .map(|f| (f.name.clone().unwrap_or_default(), f.ty.substitute(&params, &type_args), f.has_default))
            .collect();
        self.bind_arguments(name, &fields, args, span);
        Ty::Adt(name.to_string(), type_args)
    }

    fn construct_variant(&mut self, name: &str, owner: &str, args: &[Arg], span: Span, expected: Option<&Ty>) -> Ty {
        let Some(TypeDef::Sum { params, variants }) = self.program.types.get(owner).cloned() else { return Ty::Error };
        let type_args: Vec<Ty> = match expected.map(|e| self.resolve(e)) {
            Some(Ty::Adt(n, a)) if n == owner => a,
            _ => params.iter().map(|_| self.infer.fresh()).collect(),
        };
        let variant = variants.iter().find(|v| v.name == name).cloned();
        let fields: Vec<(String, Ty, bool)> = variant
            .and_then(|v| v.fields)
            .unwrap_or_default()
            .iter()
            .enumerate()
            .map(|(i, f)| {
                (f.name.clone().unwrap_or_else(|| format!("_{i}")), f.ty.substitute(&params, &type_args), f.has_default)
            })
            .collect();
        self.bind_arguments(name, &fields, args, span);
        Ty::Adt(owner.to_string(), type_args)
    }

    /// Match positional and keyword arguments to named slots with defaults.
    fn bind_arguments(&mut self, callee: &str, slots: &[(String, Ty, bool)], args: &[Arg], span: Span) {
        let mut filled = vec![false; slots.len()];
        let mut position = 0;
        for arg in args {
            match arg {
                Arg::Positional(e) | Arg::Inout(e, _) => {
                    if let Some((_, ty, _)) = slots.get(position) {
                        let found = self.expr(e, Some(ty));
                        self.coerce(&found, ty, e.span);
                        filled[position] = true;
                    } else {
                        self.expr(e, None);
                    }
                    position += 1;
                }
                Arg::Keyword(k, e) => match slots.iter().position(|(n, _, _)| *n == k.name) {
                    Some(i) => {
                        let found = self.expr(e, Some(&slots[i].1));
                        self.coerce(&found, &slots[i].1, e.span);
                        filled[i] = true;
                    }
                    None => {
                        self.expr(e, None);
                        let names: Vec<String> = slots.iter().map(|s| s.0.clone()).collect();
                        self.report(
                            Diagnostic::error("E0203", k.span, format!("`{callee}` has no parameter `{}`", k.name))
                                .alternatives(closest(&k.name, &names)),
                        );
                    }
                },
            }
        }
        let missing: Vec<&str> =
            slots.iter().zip(&filled).filter(|((_, _, d), f)| !**f && !*d).map(|((n, _, _), _)| n.as_str()).collect();
        let positional = args.iter().filter(|a| !matches!(a, Arg::Keyword(..))).count();
        if positional > slots.len() || !missing.is_empty() {
            let mut d = Diagnostic::error(
                "E0203",
                span,
                format!(
                    "`{callee}` takes {} argument{}, and {} were given",
                    slots.len(),
                    if slots.len() == 1 { "" } else { "s" },
                    args.len()
                ),
            );
            if !missing.is_empty() {
                d = d.note(format!("missing: {}", missing.join(", ")));
            }
            self.report(d);
        }
    }

    fn call_signature(
        &mut self,
        sig: &FnSig,
        owner_params: &[String],
        owner_args: &[Ty],
        args: &[Arg],
        span: Span,
        skip_self: Option<()>,
    ) -> Ty {
        if !sig.overloads.is_empty() {
            return self.call_overloaded(sig, args, span, skip_self);
        }
        let fresh: Vec<Ty> = sig.type_params.iter().map(|_| self.infer.fresh()).collect();
        let mut names: Vec<String> = owner_params.to_vec();
        names.extend(sig.type_params.iter().map(|(n, _)| n.clone()));
        let mut values: Vec<Ty> = owner_args.to_vec();
        values.extend(fresh.iter().cloned());
        let params: Vec<_> = sig.params.iter().skip(usize::from(skip_self.is_some())).collect();
        let mut filled = vec![false; params.len()];
        let mut position = 0;
        let mut lent: Vec<(String, Span)> = Vec::new();
        for arg in args {
            let (slot, expr) = match arg {
                Arg::Keyword(k, e) => match params.iter().position(|p| p.name == k.name) {
                    Some(i) => (Some(i), e),
                    None => {
                        self.expr(e, None);
                        let names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
                        self.report(
                            Diagnostic::error("E0203", k.span, format!("`{}` has no parameter `{}`", sig.name, k.name))
                                .alternatives(closest(&k.name, &names)),
                        );
                        continue;
                    }
                },
                Arg::Positional(e) | Arg::Inout(e, _) => {
                    position += 1;
                    (if position <= params.len() { Some(position - 1) } else { None }, e)
                }
            };
            let Some(i) = slot else {
                self.expr(expr, None);
                continue;
            };
            filled[i] = true;
            let param = params[i];
            let want = param.ty.substitute(&names, &values);
            let found = self.expr(expr, Some(&want));
            self.coerce(&found, &want, expr.span);
            self.convention(param.convention, arg, &param.name, &sig.name);
            if let (Convention::Inout, Arg::Inout(place, _)) = (param.convention, arg) {
                let key: String = self.source(place.span).chars().filter(|c| !c.is_whitespace()).collect();
                if let Some((_, first)) = lent.iter().find(|(other, _)| overlaps(other, &key)) {
                    self.report(
                        Diagnostic::error(
                            "E0307",
                            place.span,
                            format!(
                                "`{key}` is lent to `{}` twice: two `inout` arguments may not share a place",
                                sig.name
                            ),
                        )
                        .label(*first, "first lent here"),
                    );
                } else {
                    lent.push((key, place.span));
                }
            }
        }
        let given = args.len();
        let missing: Vec<&str> =
            params.iter().zip(&filled).filter(|(p, f)| !**f && !p.has_default).map(|(p, _)| p.name.as_str()).collect();
        if position > params.len() || !missing.is_empty() {
            let mut d = Diagnostic::error(
                "E0203",
                span,
                format!(
                    "`{}` takes {} argument{}, and {given} were given",
                    sig.name,
                    params.len(),
                    if params.len() == 1 { "" } else { "s" }
                ),
            );
            if !missing.is_empty() {
                d = d.note(format!("missing: {}", missing.join(", ")));
            }
            self.report(d);
        }
        for ((name, bound), ty) in sig.type_params.iter().zip(&fresh) {
            let Some(bound) = bound else { continue };
            let resolved = self.resolve(ty);
            let resolved = match &resolved {
                Ty::Dyn(_) => resolved.clone(),
                other => other.clone(),
            };
            if !self.program.satisfies(&resolved, bound, &self.type_params) {
                self.report(Diagnostic::error(
                    "E0213",
                    span,
                    format!("`{resolved}` does not implement `{bound}`, which `{name}` of `{}` needs", sig.name),
                ));
            }
        }
        let ret = sig.ret.substitute(&names, &values);
        match &sig.error {
            Some(e) => Ty::Result(Box::new(ret), Box::new(e.substitute(&names, &values))),
            None => ret,
        }
    }

    /// The function `sig` named other than to call it: a value of its type, or, overloaded, no
    /// value, since it has no one type (adr:0035).
    fn named_function(&mut self, sig: &FnSig, span: Span) -> Ty {
        if sig.overloads.is_empty() {
            return function_value(sig);
        }
        self.report(
            Diagnostic::error(
                "E0226",
                span,
                format!("`{}` is overloaded, so it has no one type: call it, or wrap the call in a lambda", sig.name),
            )
            .note(format!("it has {} overloads", sig.overloads.len() + 1)),
        );
        Ty::Error
    }

    /// A call of an overloaded Python function, constructor or method (adr:0035). Each argument is
    /// checked once, with no expected type, so a nest of such calls stays linear; the call is then
    /// given the first overload that takes the arguments without passing a value that is not a
    /// `PyObject` to a `PyObject` parameter, else the first that takes them at all, and the choice
    /// is recorded by the call's span for the lowering.
    fn call_overloaded(&mut self, sig: &FnSig, args: &[Arg], span: Span, skip_self: Option<()>) -> Ty {
        let found: Vec<Ty> = args.iter().map(|a| self.expr(a.expr(), None)).collect();
        let skip = usize::from(skip_self.is_some());
        let all: Vec<&FnSig> = std::iter::once(sig).chain(&sig.overloads).collect();
        let mut loose = None;
        let mut exact = None;
        for (i, candidate) in all.iter().enumerate() {
            match self.takes(candidate, skip, args, &found) {
                Some((false, _)) => {
                    exact = Some(i);
                    break;
                }
                Some((true, _)) if loose.is_none() => loose = Some(i),
                _ => {}
            }
        }
        let Some(chosen) = exact.or(loose) else {
            let mut d = Diagnostic::error(
                "E0204",
                span,
                format!("no overload of `{}` takes these arguments, of {}", sig.name, self.listed(&found)),
            );
            for candidate in &all {
                d = d.note(format!("`{}`", overload_text(candidate, skip)));
            }
            self.report(d);
            return Ty::Error;
        };
        self.overloads.push((span, chosen));
        let chosen = all[chosen];
        let (_, slots) = self.takes(chosen, skip, args, &found).expect("it was taken");
        let params: Vec<_> = chosen.params.iter().skip(skip).collect();
        for ((arg, ty), i) in args.iter().zip(&found).zip(slots) {
            self.coerce(ty, &params[i].ty, arg.expr().span);
            self.convention(params[i].convention, arg, &params[i].name, &chosen.name);
        }
        match &chosen.error {
            Some(e) => Ty::Result(Box::new(chosen.ret.clone()), Box::new(e.clone())),
            None => chosen.ret.clone(),
        }
    }

    /// Whether the overload `sig` takes arguments of the types `found`, its first `skip`
    /// parameters aside: by count, by keyword and by type, with whether a value that is not a
    /// `PyObject` would go to a `PyObject` parameter, and each argument's parameter. A trial: it
    /// binds no inference variable and reports nothing.
    fn takes(&mut self, sig: &FnSig, skip: usize, args: &[Arg], found: &[Ty]) -> Option<(bool, Vec<usize>)> {
        let params: Vec<_> = sig.params.iter().skip(skip).collect();
        let mut filled = vec![false; params.len()];
        let mut slots = Vec::with_capacity(args.len());
        let mut position = 0;
        for arg in args {
            let slot = match arg {
                Arg::Keyword(k, _) => params.iter().position(|p| p.name == k.name)?,
                Arg::Positional(_) | Arg::Inout(..) => {
                    position += 1;
                    position - 1
                }
            };
            if slot >= params.len() || std::mem::replace(&mut filled[slot], true) {
                return None;
            }
            slots.push(slot);
        }
        if params.iter().zip(&filled).any(|(p, f)| !f && !p.has_default) {
            return None;
        }
        let said = self.diagnostics.len();
        self.infer.begin_trial();
        let mut loose = false;
        let mut fits = true;
        for (ty, &slot) in found.iter().zip(&slots) {
            let want = &params[slot].ty;
            loose |= *want == Ty::PyObject && self.resolve(ty) != Ty::PyObject;
            if !self.fits(ty, want) {
                fits = false;
                break;
            }
        }
        self.infer.undo_trial();
        self.diagnostics.truncate(said);
        fits.then_some((loose, slots))
    }

    /// The types of a call's arguments, as a diagnostic lists them.
    fn listed(&mut self, found: &[Ty]) -> String {
        if found.is_empty() {
            return "none".to_string();
        }
        found.iter().map(|t| format!("`{}`", self.resolve(t))).collect::<Vec<_>>().join(", ")
    }

    /// `inout` parameters take `&place`, of a mutable place; others take no `&`.
    fn convention(&mut self, convention: Convention, arg: &Arg, param: &str, callee: &str) {
        match (convention, arg) {
            (Convention::Inout, Arg::Inout(place, _)) => {
                if let Some((name, _)) = Self::root(place) {
                    self.forget(name);
                    if let Some(local) = self.lookup(name).cloned()
                        && !local.mutable
                    {
                        let mut d = Diagnostic::error(
                            "E0303",
                            place.span,
                            format!("`&{name}` lends `{name}` to be changed, but `{name}` is immutable"),
                        )
                        .label(local.span, "declared here");
                        if let Some(at) = local.declared_at {
                            d = d.fix(
                                "declare it with `var`",
                                Applicability::MachineApplicable,
                                vec![(Span { start: at, end: at }, "var ".into())],
                            );
                        }
                        self.report(d);
                    }
                }
            }
            (Convention::Inout, other) => {
                let e = other.expr();
                self.report(
                    Diagnostic::error(
                        "E0304",
                        e.span,
                        format!(
                            "`{param}` of `{callee}` is `inout`: pass `&{}` so the change is visible",
                            self.source(e.span)
                        ),
                    )
                    .fix(
                        "add `&`",
                        Applicability::MachineApplicable,
                        vec![(Span { start: e.span.start, end: e.span.start }, "&".into())],
                    ),
                );
            }
            (_, Arg::Inout(_, amp)) => {
                self.report(
                    Diagnostic::error(
                        "E0305",
                        *amp,
                        format!("`{param}` of `{callee}` is not `inout`, so it gets a copy"),
                    )
                    .fix(
                        "remove the `&`",
                        Applicability::MachineApplicable,
                        vec![(*amp, String::new())],
                    ),
                );
            }
            (Convention::Sink, Arg::Positional(Expr { kind: ExprKind::Name(name), .. })) => {
                if let Some(local) = self.lookup_mut(name) {
                    local.moved = true;
                }
            }
            _ => {}
        }
    }

    fn method_call(&mut self, object: &Expr, name: &Ident, args: &[Arg], span: Span, expected: Option<&Ty>) -> Ty {
        if let ExprKind::Name(class) = &object.kind
            && self.lookup(class).is_none()
            && let Some(qualified) = self.program.py_classes.get(class).cloned()
        {
            return match self.py_method(&qualified, &name.name) {
                Some(m) if m.receiver.is_none() => self.call_signature(&m.sig, &[], &[], args, span, None),
                Some(_) => {
                    self.arg_types(args, &[]);
                    self.report(Diagnostic::error(
                        "E0205",
                        name.span,
                        format!("`{}` is a method: call it on a `{class}` value", name.name),
                    ));
                    Ty::Error
                }
                None => {
                    self.arg_types(args, &[]);
                    let names = self.py_members(&qualified);
                    self.report(
                        Diagnostic::error("E0205", name.span, format!("`{class}` has no function `{}`", name.name))
                            .alternatives(closest(&name.name, &names)),
                    );
                    Ty::Error
                }
            };
        }
        let receiver = self.expr(object, None);
        let receiver = self.result_value(receiver, object.span);
        let receiver = self.present(receiver, object.span);
        if matches!(receiver, Ty::PyObject) {
            return self.py_value(name, args, span, expected);
        }
        match receiver.clone() {
            Ty::TypeName(type_name) => {
                let method = self.program.methods.get(&type_name).and_then(|m| m.get(&name.name)).cloned();
                match method {
                    Some(m) if m.receiver.is_none() => {
                        let owner_args: Vec<Ty> = m.owner_params.iter().map(|_| self.infer.fresh()).collect();
                        self.call_signature(&m.sig, &m.owner_params, &owner_args, args, span, None)
                    }
                    Some(_) => {
                        self.report(Diagnostic::error(
                            "E0205",
                            name.span,
                            format!("`{}` is a method: call it on a `{type_name}` value", name.name),
                        ));
                        Ty::Error
                    }
                    None => {
                        let names: Vec<String> = self
                            .program
                            .methods
                            .get(&type_name)
                            .map(|m| m.keys().cloned().collect())
                            .unwrap_or_default();
                        self.report(
                            Diagnostic::error(
                                "E0205",
                                name.span,
                                format!("`{type_name}` has no function `{}`", name.name),
                            )
                            .alternatives(closest(&name.name, &names)),
                        );
                        Ty::Error
                    }
                }
            }
            Ty::Module(module) if self.program.foreign.contains_key(&module) => {
                if let Some(sig) = self.program.foreign[&module].get(&name.name).cloned() {
                    return self.call_signature(&sig, &[], &[], args, span, None);
                }
                // `py.m.C(...)`: a class's constructor, its own or a base's (adr:0034).
                if let Some(sig) = self.program.py_constructor(&format!("{module}.{}", name.name)) {
                    return self.call_signature(&sig, &[], &[], args, span, None);
                }
                self.arg_types(args, &[]);
                let names: Vec<String> = self.program.foreign[&module].keys().cloned().collect();
                self.report(
                    Diagnostic::error("E0205", name.span, format!("`{module}`'s interface has no `{}`", name.name))
                        .alternatives(closest(&name.name, &names))
                        .note("a function the binding could not type is listed in the interface's comments"),
                );
                Ty::Error
            }
            Ty::Module(module) => {
                let ty = builtins::module_member(&module, &name.name);
                match ty {
                    Some(ty) => self.call_value(&ty, args, span, Some(&name.name)),
                    None => {
                        self.arg_types(args, &[]);
                        self.report(
                            Diagnostic::error("E0205", name.span, format!("`{module}` has no `{}`", name.name))
                                .alternatives(closest(
                                    &name.name,
                                    &builtins::module_members(&module)
                                        .iter()
                                        .map(ToString::to_string)
                                        .collect::<Vec<_>>(),
                                )),
                        );
                        Ty::Error
                    }
                }
            }
            Ty::Adt(type_name, _) if self.program.py_attributes.contains_key(&type_name) => {
                match self.py_method(&type_name, &name.name) {
                    Some(m) => self.user_method(&m, object, &[], args, span, name),
                    None => {
                        self.arg_types(args, &[]);
                        let names = self.py_members(&type_name);
                        let mut d = Diagnostic::error(
                            "E0205",
                            name.span,
                            format!("`{type_name}` has no method `{}`", name.name),
                        )
                        .alternatives(closest(&name.name, &names));
                        if self.py_attribute(&type_name, &name.name).is_some() {
                            d = d.note(format!("`{}` is an attribute: read it without parentheses", name.name));
                        }
                        self.report(d);
                        Ty::Error
                    }
                }
            }
            Ty::Adt(type_name, type_args) => {
                let method = self.program.methods.get(&type_name).and_then(|m| m.get(&name.name)).cloned();
                match method {
                    Some(m) => self.user_method(&m, object, &type_args, args, span, name),
                    None => {
                        self.arg_types(args, &[]);
                        let mut names: Vec<String> = self
                            .program
                            .methods
                            .get(&type_name)
                            .map(|m| m.keys().cloned().collect())
                            .unwrap_or_default();
                        if let Some(TypeDef::Record { fields, .. }) = self.program.types.get(&type_name) {
                            names.extend(fields.iter().filter_map(|f| f.name.clone()));
                        }
                        self.report(
                            Diagnostic::error(
                                "E0205",
                                name.span,
                                format!("`{type_name}` has no method `{}`", name.name),
                            )
                            .alternatives(closest(&name.name, &names)),
                        );
                        Ty::Error
                    }
                }
            }
            Ty::Param(param) => {
                let bound = self.type_params.get(&param).cloned().flatten();
                self.trait_method(bound.as_deref(), &receiver, name, args, span)
            }
            Ty::Dyn(trait_name) => self.trait_method(Some(&trait_name), &receiver, name, args, span),
            Ty::Never => {
                self.arg_types(args, &[]);
                Ty::Error
            }
            Ty::Error => {
                self.arg_types(args, &[]);
                if !self.has_error() {
                    self.unknown_member(name, "the method", "called");
                }
                Ty::Error
            }
            Ty::Var(_) => {
                self.arg_types(args, &[]);
                self.unknown_member(name, "the method", "called");
                Ty::Error
            }
            builtin => {
                let mut types = Vec::new();
                for arg in args {
                    let lambda_want = match arg {
                        Arg::Keyword(k, e) if matches!(e.kind, ExprKind::Lambda { .. }) => {
                            builtins::method_lambda(&builtin, &name.name, Some(&k.name))
                        }
                        Arg::Positional(e) if matches!(e.kind, ExprKind::Lambda { .. }) => {
                            builtins::method_lambda(&builtin, &name.name, None)
                        }
                        _ => None,
                    };
                    let t = self.expr(arg.expr(), lambda_want.as_ref());
                    if !matches!(arg, Arg::Keyword(..)) {
                        types.push(t);
                    }
                }
                match builtins::method(&builtin, &name.name, &types, &mut self.infer) {
                    Some(Ok(ty)) => {
                        if builtins::mutates(&builtin, &name.name) {
                            self.mutate(object, span, false);
                        }
                        ty
                    }
                    Some(Err(message)) => {
                        self.report(Diagnostic::error("E0204", span, message));
                        Ty::Error
                    }
                    None => {
                        let names: Vec<String> =
                            builtins::methods_of(&builtin).iter().map(ToString::to_string).collect();
                        self.report(
                            Diagnostic::error(
                                "E0205",
                                name.span,
                                format!("a `{builtin}` has no method `{}`", name.name),
                            )
                            .alternatives(closest(&name.name, &names)),
                        );
                        Ty::Error
                    }
                }
            }
        }
    }

    /// The full name of the Python class `ty` is a value of, if it is one (adr:0034).
    fn py_class(&self, ty: &Ty) -> Option<String> {
        match self.resolve(ty) {
            Ty::Adt(name, _) if self.program.py_attributes.contains_key(&name) => Some(name),
            _ => None,
        }
    }

    /// `class` and its declared bases, nearest first, each once.
    fn py_lineage(&self, class: &str) -> Vec<String> {
        crate::interface::py_lineage(class, |c| self.program.py_bases.get(c).map(Vec::as_slice))
    }

    /// The method or static method `name` of the Python class `class` or of a base it declares.
    fn py_method(&self, class: &str, name: &str) -> Option<Method> {
        self.py_lineage(class).iter().find_map(|c| self.program.methods.get(c).and_then(|m| m.get(name)).cloned())
    }

    /// The type of the attribute `name` of the Python class `class` or of a base it declares.
    fn py_attribute(&self, class: &str, name: &str) -> Option<Ty> {
        self.py_lineage(class).iter().find_map(|c| self.program.py_attributes.get(c).and_then(|a| a.get(name)).cloned())
    }

    /// Every member name of the Python class `class` and its bases, for an alternative.
    fn py_members(&self, class: &str) -> Vec<String> {
        let mut names = Vec::new();
        for c in self.py_lineage(class) {
            names.extend(self.program.methods.get(&c).into_iter().flat_map(|m| m.keys().cloned()));
            names.extend(self.program.py_attributes.get(&c).into_iter().flat_map(|a| a.keys().cloned()));
        }
        names
    }

    fn user_method(
        &mut self,
        m: &Method,
        object: &Expr,
        type_args: &[Ty],
        args: &[Arg],
        span: Span,
        name: &Ident,
    ) -> Ty {
        match m.receiver {
            None => {
                self.report(Diagnostic::error(
                    "E0205",
                    name.span,
                    format!("`{}` takes no `self`: call it on the type", name.name),
                ));
                Ty::Error
            }
            Some(convention) => {
                if matches!(convention, Convention::Inout | Convention::Var) {
                    self.mutate(object, span, false);
                }
                self.call_signature(&m.sig, &m.owner_params, type_args, args, span, Some(()))
            }
        }
    }

    fn trait_method(&mut self, bound: Option<&str>, receiver: &Ty, name: &Ident, args: &[Arg], span: Span) -> Ty {
        let Some(trait_name) = bound else {
            self.arg_types(args, &[]);
            self.report(Diagnostic::error(
                "E0205",
                name.span,
                format!("a `{receiver}` has no methods: bound the type parameter by a trait"),
            ));
            return Ty::Error;
        };
        let method = self.program.traits.get(trait_name).and_then(|t| t.get(&name.name)).cloned();
        match method {
            Some(m) => {
                let sig = FnSig {
                    params: m
                        .sig
                        .params
                        .iter()
                        .map(|p| crate::program::ParamSig {
                            ty: p.ty.substitute(&["Self".into()], std::slice::from_ref(receiver)),
                            ..p.clone()
                        })
                        .collect(),
                    ret: m.sig.ret.substitute(&["Self".into()], std::slice::from_ref(receiver)),
                    ..m.sig.clone()
                };
                self.call_signature(&sig, &[], &[], args, span, Some(()))
            }
            None => {
                self.arg_types(args, &[]);
                let names: Vec<String> =
                    self.program.traits.get(trait_name).map(|t| t.keys().cloned().collect()).unwrap_or_default();
                self.report(
                    Diagnostic::error("E0205", name.span, format!("`{trait_name}` has no method `{}`", name.name))
                        .alternatives(closest(&name.name, &names)),
                );
                Ty::Error
            }
        }
    }
}

/// The lambda checks of map and filter look at the iterable after them, so the positional
/// values are passed with a placeholder where the lambda goes.
fn positional_values_for(name: &str, values: &[Ty], all: &[Ty]) -> Vec<Ty> {
    if matches!(name, "map" | "filter") { all.to_vec() } else { values.to_vec() }
}

/// Whether two places, as written, may be the same memory: one is the other or inside it.
/// `xs[i]` and `xs[j]` are taken to overlap, since `i` may equal `j`.
/// An overload as a diagnostic lists it: `joined(other: str) -> py.os.Path`.
fn overload_text(sig: &FnSig, skip: usize) -> String {
    let params: Vec<String> = sig.params.iter().skip(skip).map(|p| format!("{}: {}", p.name, p.ty)).collect();
    format!("{}({}) -> {}", sig.name, params.join(", "), sig.ret)
}

fn overlaps(a: &str, b: &str) -> bool {
    let root = |s: &str| s.split(['.', '[']).next().unwrap_or("").to_string();
    if root(a) != root(b) {
        return false;
    }
    let within = |outer: &str, inner: &str| {
        inner == outer || inner.starts_with(&format!("{outer}.")) || inner.starts_with(&format!("{outer}["))
    };
    within(a, b) || within(b, a) || (a.contains('[') && b.contains('[') && a.split('[').next() == b.split('[').next())
}

/// A dotted path for narrowing: `x`, `u.email`.
fn path_of(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Name(n) => Some(n.clone()),
        ExprKind::Attr { object, name } => path_of(object).map(|p| format!("{p}.{}", name.name)),
        _ => None,
    }
}

fn irrefutable(pattern: &Pattern, program: &Program) -> bool {
    match &pattern.kind {
        PatternKind::Wildcard | PatternKind::Error => true,
        PatternKind::Name(name) => {
            !program.variant_of.contains_key(&name.name) && !matches!(name.name.as_str(), "Ok" | "Err")
        }
        PatternKind::Tuple(items) => items.iter().all(|p| irrefutable(p, program)),
        _ => false,
    }
}

fn arms_none(ty: &Ty) -> bool {
    matches!(ty, Ty::Optional(_))
}

/// Whether the parser lost part of a block, so what its flow looks like is not known.
fn unreadable(block: &Block) -> bool {
    block.stmts.iter().any(|s| match &s.kind {
        StmtKind::Error => true,
        StmtKind::If { branches, orelse } => {
            branches.iter().any(|(_, b)| unreadable(b)) || orelse.as_ref().is_some_and(unreadable)
        }
        StmtKind::While { body, .. } | StmtKind::For { body, .. } => unreadable(body),
        StmtKind::Match { arms, .. } => arms.iter().any(|a| unreadable(&a.body)),
        _ => false,
    })
}

fn breaks(block: &Block) -> bool {
    block.stmts.iter().any(|s| match &s.kind {
        StmtKind::Break => true,
        StmtKind::If { branches, orelse } => {
            branches.iter().any(|(_, b)| breaks(b)) || orelse.as_ref().is_some_and(breaks)
        }
        StmtKind::Match { arms, .. } => arms.iter().any(|a| breaks(&a.body)),
        _ => false,
    })
}

/// The locals a block may assign or lend to be changed, nested blocks included.
fn assigned(block: &Block) -> BTreeSet<String> {
    fn place(expr: &Expr, out: &mut BTreeSet<String>) {
        match &expr.kind {
            ExprKind::Name(n) => {
                out.insert(n.clone());
            }
            ExprKind::Attr { object, .. } | ExprKind::Index { object, .. } | ExprKind::Slice { object, .. } => {
                place(object, out)
            }
            ExprKind::Tuple(items) => items.iter().for_each(|i| place(i, out)),
            _ => {}
        }
    }
    fn lent(expr: &Expr, out: &mut BTreeSet<String>) {
        if let ExprKind::Call { func, args } = &expr.kind {
            for arg in args {
                if let Arg::Inout(e, _) = arg {
                    place(e, out);
                }
                lent(arg.expr(), out);
            }
            lent(func, out);
        }
    }
    fn walk(block: &Block, out: &mut BTreeSet<String>) {
        for stmt in &block.stmts {
            match &stmt.kind {
                StmtKind::Assign { target, value } => {
                    place(target, out);
                    lent(value, out);
                }
                StmtKind::AugAssign { target, value, .. } => {
                    place(target, out);
                    lent(value, out);
                }
                StmtKind::Annotated { target, value, .. } | StmtKind::Var { name: target, value, .. } => {
                    out.insert(target.name.clone());
                    lent(value, out);
                }
                StmtKind::Expr(e) | StmtKind::Return(Some(e)) => lent(e, out),
                StmtKind::If { branches, orelse } => {
                    for (test, body) in branches {
                        lent(test, out);
                        walk(body, out);
                    }
                    if let Some(body) = orelse {
                        walk(body, out);
                    }
                }
                StmtKind::While { test, body } => {
                    lent(test, out);
                    walk(body, out);
                }
                StmtKind::For { body, .. } => walk(body, out),
                StmtKind::Match { arms, .. } => arms.iter().for_each(|a| walk(&a.body, out)),
                _ => {}
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(block, &mut out);
    out
}

/// A function used as a value: its parameters and, for one that can fail, its result.
fn function_value(sig: &FnSig) -> Ty {
    let ret = match &sig.error {
        Some(error) => Ty::Result(Box::new(sig.ret.clone()), Box::new(error.clone())),
        None => sig.ret.clone(),
    };
    Ty::Func(sig.params.iter().map(|p| p.ty.clone()).collect(), Box::new(ret))
}

/// The values an integer type holds.
fn int_range(kind: IntKind) -> (i128, i128) {
    match kind {
        IntKind::I8 => (i8::MIN.into(), i8::MAX.into()),
        IntKind::I16 => (i16::MIN.into(), i16::MAX.into()),
        IntKind::I32 => (i32::MIN.into(), i32::MAX.into()),
        IntKind::I64 => (i64::MIN.into(), i64::MAX.into()),
        IntKind::U8 => (0, u8::MAX.into()),
        IntKind::U16 => (0, u16::MAX.into()),
        IntKind::U32 => (0, u32::MAX.into()),
        IntKind::U64 => (0, u64::MAX.into()),
    }
}
