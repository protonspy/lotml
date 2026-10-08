//! What a module declares: types, variants, functions, traits, impls and imports, with their
//! signatures resolved — everything a body is checked against.

use std::collections::{BTreeMap, HashMap, HashSet};

use lotml_diag::{Applicability, Diagnostic};
use lotml_syntax::ast::{self, Convention, Item, TypeExpr, TypeKind};
use lotml_syntax::span::Span;

use crate::ty::Ty;

#[derive(Clone, Debug, PartialEq)]
pub struct ParamSig {
    pub name: String,
    pub ty: Ty,
    pub convention: Convention,
    pub has_default: bool,
    pub span: Span,
}

impl ParamSig {
    /// Whether the parameter lends the caller's own slot: `inout`.
    pub fn is_inout(&self) -> bool {
        self.convention == Convention::Inout
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FnSig {
    pub name: String,
    pub type_params: Vec<(String, Option<String>)>,
    pub params: Vec<ParamSig>,
    pub ret: Ty,
    pub error: Option<Ty>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FieldSig {
    pub name: Option<String>,
    pub ty: Ty,
    pub has_default: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VariantSig {
    pub name: String,
    pub fields: Option<Vec<FieldSig>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TypeDef {
    Record { params: Vec<String>, fields: Vec<FieldSig> },
    Sum { params: Vec<String>, variants: Vec<VariantSig> },
}

impl TypeDef {
    pub fn params(&self) -> &[String] {
        match self {
            TypeDef::Record { params, .. } | TypeDef::Sum { params, .. } => params,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Method {
    pub sig: FnSig,
    /// None for a function called on the type: `Counter.new()`.
    pub receiver: Option<Convention>,
    /// The impl's own type parameters: `T` in `impl Stack[T]`.
    pub owner_params: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Program {
    pub types: BTreeMap<String, TypeDef>,
    pub variant_of: HashMap<String, String>,
    pub functions: BTreeMap<String, FnSig>,
    pub methods: HashMap<String, BTreeMap<String, Method>>,
    pub traits: BTreeMap<String, BTreeMap<String, Method>>,
    pub implements: HashSet<(String, String)>,
    /// The methods of each trait that have a body, which an `impl` may leave out.
    pub trait_defaults: HashMap<String, HashSet<String>>,
    /// Names imported into scope, each with the module it came from.
    pub imported: HashMap<String, String>,
    pub modules: HashSet<String>,
    /// The Python modules there are interfaces for, each with its functions (adr:0012).
    pub available: HashMap<String, BTreeMap<String, FnSig>>,
    /// The Python modules imported, with their functions.
    pub foreign: BTreeMap<String, BTreeMap<String, FnSig>>,
    /// The modules whose bindings file shadows the interface the compiler would generate.
    pub shadowed: HashSet<String>,
    pub diagnostics: Vec<Diagnostic>,
}

pub const MODULES: &[&str] = &["math"];

impl Program {
    /// What a module declares, with the Python modules it may import through `interfaces`.
    pub fn collect(
        module: &ast::Module,
        interfaces: &HashMap<String, BTreeMap<String, FnSig>>,
        shadowed: &HashSet<String>,
    ) -> Program {
        let mut program = Program::with_prelude();
        program.available = interfaces.clone();
        program.shadowed = shadowed.clone();
        let mut seen: HashMap<String, Span> = HashMap::new();
        let mut declare = |program: &mut Program, name: &ast::Ident| {
            if name.name.is_empty() {
                return;
            }
            crate::report_reserved(&mut program.diagnostics, name);
            if let Some(first) = seen.get(&name.name) {
                program.diagnostics.push(
                    Diagnostic::error("E0210", name.span, format!("`{}` is declared twice", name.name))
                        .label(*first, "first declared here"),
                );
            } else {
                seen.insert(name.name.clone(), name.span);
            }
        };
        // Names first, so signatures can refer to types declared further down.
        for item in &module.items {
            match item {
                Item::Record(r) => {
                    declare(&mut program, &r.name);
                    reserved_params(&mut program.diagnostics, &r.type_params);
                    program.types.insert(
                        r.name.name.clone(),
                        TypeDef::Record { params: params(&r.type_params), fields: vec![] },
                    );
                }
                Item::Sum(s) => {
                    declare(&mut program, &s.name);
                    reserved_params(&mut program.diagnostics, &s.type_params);
                    program
                        .types
                        .insert(s.name.name.clone(), TypeDef::Sum { params: params(&s.type_params), variants: vec![] });
                    for v in &s.variants {
                        declare(&mut program, &v.name);
                        program.variant_of.insert(v.name.name.clone(), s.name.name.clone());
                    }
                }
                Item::Fn(f) => declare(&mut program, &f.name),
                Item::Trait(t) => declare(&mut program, &t.name),
                _ => {}
            }
        }
        for item in &module.items {
            match item {
                Item::Record(r) => {
                    let scope = params(&r.type_params);
                    let fields = r.fields.iter().map(|f| program.field(f, &scope)).collect();
                    program.types.insert(r.name.name.clone(), TypeDef::Record { params: scope, fields });
                }
                Item::Sum(s) => {
                    let scope = params(&s.type_params);
                    let variants = s
                        .variants
                        .iter()
                        .map(|v| VariantSig {
                            name: v.name.name.clone(),
                            fields: v.fields.as_ref().map(|fs| fs.iter().map(|f| program.field(f, &scope)).collect()),
                        })
                        .collect();
                    program.types.insert(s.name.name.clone(), TypeDef::Sum { params: scope, variants });
                }
                Item::Fn(f) => {
                    let sig = program.signature(f, &[], None);
                    program.functions.insert(f.name.name.clone(), sig);
                }
                Item::Trait(t) => {
                    for m in &t.methods {
                        crate::report_reserved(&mut program.diagnostics, &m.name);
                    }
                    let methods = t
                        .methods
                        .iter()
                        .map(|m| {
                            let sig = program.signature(m, &[], Some(&Ty::Param("Self".into())));
                            let receiver = m.params.first().filter(|p| p.name.name == "self").map(|p| p.convention);
                            (m.name.name.clone(), Method { sig, receiver, owner_params: vec![] })
                        })
                        .collect();
                    program.traits.insert(t.name.name.clone(), methods);
                    let defaults = t.methods.iter().filter(|m| m.body.is_some()).map(|m| m.name.name.clone()).collect();
                    program.trait_defaults.insert(t.name.name.clone(), defaults);
                }
                _ => {}
            }
        }
        // Imports last, so a name imported and declared too is seen as both.
        for item in &module.items {
            if let Item::Import(import) = item {
                program.import(import);
            }
        }
        for item in &module.items {
            if let Item::Impl(imp) = item {
                program.implementation(imp);
            }
        }
        program
    }

    fn field(&mut self, field: &ast::Field, scope: &[String]) -> FieldSig {
        if let Some(name) = &field.name {
            crate::report_reserved(&mut self.diagnostics, name);
        }
        FieldSig {
            name: field.name.as_ref().map(|n| n.name.clone()),
            ty: self.lower(&field.ty, scope),
            has_default: field.default.is_some(),
        }
    }

    /// The types every program has without declaring them: `PyError`, what a call into Python
    /// fails with.
    pub fn with_prelude() -> Program {
        let mut program = Program::default();
        let text = |name: &str| FieldSig { name: Some(name.into()), ty: Ty::Str, has_default: false };
        program
            .types
            .insert("PyError".into(), TypeDef::Record { params: vec![], fields: vec![text("kind"), text("message")] });
        program
    }

    fn import(&mut self, import: &ast::Import) {
        let module: Vec<&str> = import.module.iter().map(|i| i.name.as_str()).collect();
        let path = module.join(".");
        let span = import
            .module
            .first()
            .map_or(import.span, |m| m.span)
            .to(import.module.last().map_or(import.span, |m| m.span));
        // A foreign module is imported by its origin, `py.` or `c.` (adr:0029).
        let python = path.strip_prefix("py.").filter(|m| !m.is_empty());
        let foreign = python.is_some() || crate::interface::is_c_library(&path);
        if foreign && let Some(functions) = self.available.get(&path).cloned() {
            if let Some(module) = python.filter(|_| self.shadowed.contains(&path)) {
                self.diagnostics.push(
                    Diagnostic::warning(
                        "E0224",
                        span,
                        format!(
                            "bindings/py.{module}.lotmli shadows the interface lotml generates from the module's stub"
                        ),
                    )
                    .note("delete the file to use the generated interface, or keep it to type the module by hand"),
                );
            }
            self.import_python(import, &path, functions);
            return;
        }
        if let Some(module) = python {
            self.diagnostics.push(
                Diagnostic::error("E0216", span, format!("there is no interface of the Python module `{module}`"))
                    .alternatives(self.available.keys().filter(|k| k.starts_with("py.")).cloned())
                    .note(format!(
                        "no stub bound it on import; `lotml bind {module}` tells why, and writes bindings/py.{module}.lotmli when it can"
                    )),
            );
            return;
        }
        if !foreign && self.available.contains_key(&path) {
            self.diagnostics.push(
                Diagnostic::error(
                    "E0216",
                    span,
                    format!("`{path}` is a Python module's interface named without its origin"),
                )
                .note(format!(
                    "a Python module is imported as `py.{path}`: rename bindings/{path}.lotmli to bindings/py.{path}.lotmli"
                )),
            );
            return;
        }
        if !MODULES.contains(&path.as_str()) {
            let known = MODULES.iter().map(ToString::to_string).chain(self.available.keys().cloned());
            let mut d = Diagnostic::error("E0216", span, format!("there is no module `{path}` to import from"))
                .alternatives(known)
                .note("the common names are in the prelude and need no import")
                .note(format!("a Python module is imported as `py.{path}`, its interface generated on import"));
            let origin = format!("py.{path}");
            let own_syntax =
                matches!(path.as_str(), "typing" | "__future__" | "dataclasses" | "enum" | "collections.abc");
            if self.available.contains_key(&origin) && !own_syntax {
                d = d.fix(format!("write `{origin}`"), Applicability::MachineApplicable, vec![(span, origin)]);
            }
            if own_syntax {
                d = d.fix(
                    "remove the import: lotml writes these in its own syntax",
                    Applicability::MachineApplicable,
                    vec![(import.span, String::new())],
                );
            }
            self.diagnostics.push(d);
            return;
        }
        if import.names.is_empty() {
            self.modules.insert(path);
            return;
        }
        for name in &import.names {
            if crate::builtins::module_member(&path, &name.name).is_none() {
                self.diagnostics.push(
                    Diagnostic::error("E0216", name.span, format!("`{path}` has no `{}`", name.name))
                        .alternatives(crate::builtins::module_members(&path).iter().map(ToString::to_string)),
                );
                continue;
            }
            self.imported.insert(name.name.clone(), path.clone());
        }
    }

    /// `import m` or `from m import f` of a Python module with an interface: its functions are
    /// called like lotml's own, and each returns `T ! PyError`.
    fn import_python(&mut self, import: &ast::Import, path: &str, functions: BTreeMap<String, FnSig>) {
        if import.names.is_empty() {
            self.modules.insert(path.to_string());
        }
        for name in &import.names {
            let Some(sig) = functions.get(&name.name) else {
                self.diagnostics.push(
                    Diagnostic::error("E0216", name.span, format!("`{path}`'s interface has no `{}`", name.name))
                        .alternatives(closest_names(&name.name, functions.keys()))
                        .note("a function the binding could not type is listed in the interface's comments"),
                );
                continue;
            };
            if self.functions.contains_key(&name.name) || self.types.contains_key(&name.name) {
                self.diagnostics.push(
                    Diagnostic::error("E0210", name.span, format!("`{}` is declared twice", name.name)).note(format!(
                        "it is imported from `{path}` and declared in this file: import `{path}` and call `{path}.{}`",
                        name.name
                    )),
                );
                continue;
            }
            self.functions.insert(name.name.clone(), sig.clone());
            self.imported.insert(name.name.clone(), path.to_string());
        }
        self.foreign.insert(path.to_string(), functions);
    }

    fn implementation(&mut self, imp: &ast::ImplDef) {
        let TypeKind::Named { name, .. } = &imp.target.kind else {
            self.diagnostics.push(Diagnostic::error(
                "E0202",
                imp.target.span,
                "methods can be given to a record or a sum type declared here",
            ));
            return;
        };
        let target = name.name.clone();
        if !self.types.contains_key(&target) {
            self.diagnostics.push(
                Diagnostic::error("E0202", imp.target.span, format!("`{target}` is not a type declared in this file"))
                    .alternatives(self.types.keys().cloned()),
            );
            return;
        }
        let params = self.types[&target].params().to_vec();
        let self_ty = Ty::Adt(target.clone(), params.iter().map(|p| Ty::Param(p.clone())).collect());
        let mut implemented = None;
        if let Some(trait_expr) = &imp.trait_name
            && let TypeKind::Named { name, .. } = &trait_expr.kind
        {
            if self.traits.contains_key(&name.name) {
                implemented = Some(name.name.clone());
                self.implements.insert((name.name.clone(), target.clone()));
            } else {
                self.diagnostics.push(
                    Diagnostic::error("E0202", name.span, format!("`{}` is not a trait", name.name))
                        .alternatives(self.traits.keys().cloned()),
                );
            }
        }
        let mut seen = HashSet::new();
        for method in &imp.methods {
            crate::report_reserved(&mut self.diagnostics, &method.name);
            if !seen.insert(method.name.name.clone()) {
                self.diagnostics.push(Diagnostic::error(
                    "E0210",
                    method.name.span,
                    format!("`{}` is declared twice in this `impl`", method.name.name),
                ));
            }
            let sig = self.signature(method, &params, Some(&self_ty));
            let receiver = method.params.first().filter(|p| p.name.name == "self").map(|p| p.convention);
            self.methods
                .entry(target.clone())
                .or_default()
                .insert(method.name.name.clone(), Method { sig, receiver, owner_params: params.clone() });
        }
        // The trait's other methods: its defaults become the type's; a required one left out
        // is an error.
        let Some(trait_name) = implemented else { return };
        let methods = self.traits[&trait_name].clone();
        for (name, method) in methods {
            if seen.contains(&name) {
                continue;
            }
            if !self.trait_defaults.get(&trait_name).is_some_and(|d| d.contains(&name)) {
                self.diagnostics.push(Diagnostic::error(
                    "E0213",
                    imp.span,
                    format!("`{target}` implements `{trait_name}` without its method `{name}`"),
                ));
            }
            let substitute = |t: &Ty| t.substitute(&["Self".to_string()], std::slice::from_ref(&self_ty));
            let sig = FnSig {
                params: method.sig.params.iter().map(|p| ParamSig { ty: substitute(&p.ty), ..p.clone() }).collect(),
                ret: substitute(&method.sig.ret),
                error: method.sig.error.as_ref().map(substitute),
                ..method.sig.clone()
            };
            self.methods
                .entry(target.clone())
                .or_default()
                .insert(name, Method { sig, receiver: method.receiver, owner_params: params.clone() });
        }
    }

    pub fn signature(&mut self, f: &ast::FnDef, outer: &[String], self_ty: Option<&Ty>) -> FnSig {
        let mut scope: Vec<String> = outer.to_vec();
        reserved_params(&mut self.diagnostics, &f.type_params);
        scope.extend(f.type_params.iter().map(|p| p.name.name.clone()));
        let mut params = Vec::new();
        for p in &f.params {
            if p.name.name != "self" {
                crate::report_reserved(&mut self.diagnostics, &p.name);
            }
            if p.name.name == "self"
                && p.ty.is_none()
                && let Some(ty) = self_ty
            {
                params.push(ParamSig {
                    name: "self".into(),
                    ty: ty.clone(),
                    convention: p.convention,
                    has_default: false,
                    span: p.name.span,
                });
                continue;
            }
            let ty = match &p.ty {
                Some(t) => self.lower(t, &scope),
                None => {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E0217",
                            p.name.span,
                            format!("the parameter `{}` needs a type", p.name.name),
                        )
                        .fix(
                            "annotate it",
                            Applicability::HasPlaceholders,
                            vec![(Span { start: p.name.span.end, end: p.name.span.end }, ": T".into())],
                        ),
                    );
                    Ty::Error
                }
            };
            params.push(ParamSig {
                name: p.name.name.clone(),
                ty,
                convention: p.convention,
                has_default: p.default.is_some(),
                span: p.name.span,
            });
        }
        let ret = f.returns.as_ref().map_or(Ty::Unit, |t| self.lower(t, &scope));
        let error = f.error.as_ref().map(|t| self.lower(t, &scope));
        FnSig {
            name: f.name.name.clone(),
            type_params: f
                .type_params
                .iter()
                .map(|p| (p.name.name.clone(), p.bound.as_ref().map(|b| b.name.clone())))
                .collect(),
            params,
            ret,
            error,
            span: f.span,
        }
    }

    /// A written type as a type, reporting the names that are not types.
    pub fn lower(&mut self, t: &TypeExpr, scope: &[String]) -> Ty {
        match &t.kind {
            TypeKind::Unit => Ty::Unit,
            TypeKind::Error => Ty::Error,
            TypeKind::List(item) => Ty::list(self.lower(item, scope)),
            TypeKind::Set(item) => Ty::Set(Box::new(self.lower(item, scope))),
            TypeKind::Dict(k, v) => Ty::Dict(Box::new(self.lower(k, scope)), Box::new(self.lower(v, scope))),
            TypeKind::Tuple(items) => Ty::Tuple(items.iter().map(|i| self.lower(i, scope)).collect()),
            TypeKind::Optional(inner) => Ty::optional(self.lower(inner, scope)),
            TypeKind::Dyn(name) => {
                if self.traits.contains_key(&name.name) || crate::builtins::BUILTIN_TRAITS.contains(&name.name.as_str())
                {
                    Ty::Dyn(name.name.clone())
                } else {
                    self.diagnostics.push(
                        Diagnostic::error("E0202", name.span, format!("`{}` is not a trait", name.name))
                            .alternatives(self.traits.keys().cloned()),
                    );
                    Ty::Error
                }
            }
            TypeKind::Named { name, args } => {
                if let Some(primitive) = Ty::primitive(&name.name) {
                    return primitive;
                }
                if scope.contains(&name.name) {
                    return Ty::Param(name.name.clone());
                }
                let lowered: Vec<Ty> = args.iter().map(|a| self.lower(a, scope)).collect();
                if name.name == "Heap" && lowered.len() == 1 {
                    return Ty::Heap(Box::new(lowered[0].clone()));
                }
                if let Some(def) = self.types.get(&name.name) {
                    let wanted = def.params().len();
                    if lowered.len() == wanted {
                        return Ty::Adt(name.name.clone(), lowered);
                    }
                    if lowered.is_empty() {
                        return Ty::Adt(name.name.clone(), vec![Ty::Error; wanted]);
                    }
                    self.diagnostics.push(Diagnostic::error(
                        "E0202",
                        t.span,
                        format!("`{}` takes {wanted} type arguments", name.name),
                    ));
                    return Ty::Error;
                }
                let mut known: Vec<String> = ["int", "f64", "str", "bool", "bytes", "i32", "u8", "f32", "Heap"]
                    .iter()
                    .map(ToString::to_string)
                    .collect();
                known.extend(self.types.keys().cloned());
                known.extend(scope.iter().cloned());
                let fix = match name.name.as_str() {
                    "float" => Some("f64"),
                    "string" | "String" => Some("str"),
                    "List" | "list" | "Dict" | "dict" | "Optional" | "Set" | "set" | "Tuple" | "tuple" => None,
                    _ => None,
                };
                let mut d = Diagnostic::error("E0202", name.span, format!("`{}` is not a type", name.name))
                    .alternatives(crate::closest(&name.name, &known));
                if let Some(fix) = fix {
                    d = d.fix(
                        format!("write `{fix}`"),
                        Applicability::MachineApplicable,
                        vec![(name.span, fix.into())],
                    );
                }
                if let Some(spelled) = typing_spelling(&name.name, &lowered) {
                    d = d.fix(
                        format!("write `{spelled}`"),
                        Applicability::MachineApplicable,
                        vec![(t.span, spelled.clone())],
                    );
                    self.diagnostics.push(d);
                    return typing_type(&name.name, lowered).unwrap_or(Ty::Error);
                }
                if matches!(
                    name.name.as_str(),
                    "List" | "list" | "Dict" | "dict" | "Optional" | "Set" | "set" | "Tuple" | "tuple"
                ) {
                    d = d.note(
                        "lotml writes collection types as `[T]`, `{K: V}`, `{T}`, `(A, B)` and an optional as `T?`",
                    );
                }
                self.diagnostics.push(d);
                Ty::Error
            }
        }
    }

    /// Whether `ty` implements `trait_name`, built-in traits included.
    pub fn satisfies(&self, ty: &Ty, trait_name: &str, bounds: &HashMap<String, Option<String>>) -> bool {
        // A trait declared in the file shadows a built-in one of the same name.
        if self.traits.contains_key(trait_name) {
            return match ty {
                Ty::Error | Ty::Never | Ty::Var(_) => true,
                Ty::Param(p) => bounds.get(p).cloned().flatten().is_some_and(|b| b == trait_name),
                Ty::Dyn(t) => t == trait_name,
                Ty::Adt(name, _) => self.implements.contains(&(trait_name.to_string(), name.clone())),
                _ => false,
            };
        }
        match ty {
            Ty::Error | Ty::Never | Ty::Var(_) => true,
            Ty::Param(p) => {
                bounds.get(p).cloned().flatten().is_some_and(|b| b == trait_name)
                    || crate::builtins::BUILTIN_TRAITS.contains(&trait_name)
            }
            Ty::Dyn(t) => t == trait_name,
            Ty::Adt(name, _) => {
                crate::builtins::BUILTIN_TRAITS.contains(&trait_name)
                    || self.implements.contains(&(trait_name.to_string(), name.clone()))
            }
            _ => crate::builtins::BUILTIN_TRAITS.contains(&trait_name),
        }
    }
}

/// The lotml type for a `typing` (or builtin generic) spelling: `List[int]` is `[int]`.
fn typing_type(name: &str, args: Vec<Ty>) -> Option<Ty> {
    if args.iter().any(|a| matches!(a, Ty::Error)) {
        return None;
    }
    let mut args = args.into_iter();
    Some(match (name, args.len()) {
        ("List" | "list", 1) => Ty::list(args.next()?),
        ("Set" | "set" | "FrozenSet" | "frozenset", 1) => Ty::Set(Box::new(args.next()?)),
        ("Dict" | "dict", 2) => Ty::Dict(Box::new(args.next()?), Box::new(args.next()?)),
        ("Optional", 1) => Ty::optional(args.next()?),
        ("Tuple" | "tuple", n) if n > 0 => Ty::Tuple(args.collect()),
        _ => return None,
    })
}

fn typing_spelling(name: &str, args: &[Ty]) -> Option<String> {
    typing_type(name, args.to_vec()).map(|t| t.to_string())
}

/// Report a type parameter that takes a name reserved for the compiler.
fn reserved_params(diagnostics: &mut Vec<Diagnostic>, type_params: &[ast::TypeParam]) {
    for p in type_params {
        crate::report_reserved(diagnostics, &p.name);
    }
}

fn params(type_params: &[ast::TypeParam]) -> Vec<String> {
    type_params.iter().map(|p| p.name.name.clone()).collect()
}

/// The names closest to `name` among `names`, for an alternative.
fn closest_names<'a>(name: &str, names: impl Iterator<Item = &'a String>) -> Vec<String> {
    crate::closest(name, &names.cloned().collect::<Vec<_>>())
}
