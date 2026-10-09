//! Lowering: the checked syntax tree to the intermediate form, every call resolved and every
//! intermediate value named (specs/c-backend/design.md).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use lotml_check::ty::{F64, INT, IntKind, Ty};
use lotml_check::{Checked, FieldSig, FnSig, Method, ParamSig, TypeDef};
use lotml_diag::Diagnostic;
use lotml_syntax::ast::{
    self, Arg as AstArg, BoolOp, ExprKind, FnDef, Item, Module, PatternKind, StmtKind as Ast, StrPart,
};
use lotml_syntax::span::Span;

use crate::ir::{
    Arg, BinOp, Block, Builtin, Callee, CmpOp, Const, Expr, FormatPart, Function, Local, LocalInfo, Operand, Panic,
    Place, Proj, Stmt, StmtKind, UnOp, counted,
};
use crate::symbol;

/// The program as functions of the IR.
pub struct Lowered {
    pub functions: Vec<Function>,
    /// Whether the module declares `fn main()`.
    pub main: bool,
    /// The records and sum types the module declares.
    pub declared: BTreeMap<String, TypeDef>,
    /// Each lambda: the types of what it captures, and its function type; its code is the
    /// function `symbol::lambda(index)`.
    pub lambdas: Vec<(Vec<Ty>, Ty)>,
    /// The module's functions used as values, by their C names.
    pub fn_refs: BTreeSet<String>,
    /// How many methods each trait's table holds.
    pub traits: BTreeMap<String, usize>,
    /// Each trait's methods in its table's order, a method a `dyn` value cannot call `None`: what
    /// `mono` fills a table with.
    pub dyn_methods: BTreeMap<String, Vec<Option<String>>>,
    /// The table of each type a `dyn` value was made of, one per trait.
    pub vtables: Vec<VTable>,
    /// Each `test` block, when they were asked for: its name and its C function.
    pub tests: Vec<(String, String)>,
    /// Each C library function the program calls, by its symbol: its parameters and result.
    pub c_functions: BTreeMap<String, (Vec<Ty>, Ty)>,
    /// The C libraries the program imports, by the name the linker knows: `m` for `c.m`.
    pub libraries: BTreeSet<String>,
    /// Where each line of the source starts, as a byte offset: what turns a span into a line.
    pub line_starts: Vec<u32>,
    /// Each Python module the program imports, with the span of its import: what a target that
    /// cannot call Python refuses.
    pub python_imports: Vec<(String, Span)>,
    /// Each default a parameter or a field has, for a Python caller that leaves it out: a LotML
    /// call fills a default in where it is made, so only the Python target reads these.
    pub defaults: Vec<DefaultValue>,
    /// The C library each C function the program calls is in, by its symbol.
    pub c_libraries: BTreeMap<String, String>,
}

/// A default a parameter or a field is given when a caller leaves it out: the function of no
/// parameters that computes it.
pub struct DefaultValue {
    pub of: DefaultOf,
    pub function: Function,
}

/// What a default is the default of.
pub enum DefaultOf {
    /// The parameter `param` of the function or method whose symbol is `function`.
    Param { function: String, param: String },
    /// The field `field` of the record or variant `owner`, by name.
    Field { owner: String, field: String },
}

impl Lowered {
    /// The line, from 1, where `span` starts.
    pub fn line(&self, span: Span) -> u32 {
        self.line_starts.partition_point(|&start| start <= span.start) as u32
    }
}

/// The table of one type's methods for one trait, a slot per method of the trait in name order:
/// the function a `dyn` call reaches, `None` for a method no `dyn` call can reach.
pub struct VTable {
    pub trait_name: String,
    pub ty: Ty,
    pub slots: Vec<Option<VSlot>>,
}

pub struct VSlot {
    pub function: String,
    /// The parameters after `self`, and the result.
    pub params: Vec<Ty>,
    pub ret: Ty,
}

/// Type parameters and the types they stand for in one instance of a generic function.
#[derive(Clone, Default)]
struct Subst {
    names: Vec<String>,
    types: Vec<Ty>,
}

impl Subst {
    fn apply(&self, ty: Ty) -> Ty {
        if self.names.is_empty() { ty } else { ty.substitute(&self.names, &self.types) }
    }

    fn sig(&self, sig: &FnSig) -> FnSig {
        FnSig {
            params: sig.params.iter().map(|p| ParamSig { ty: self.apply(p.ty.clone()), ..p.clone() }).collect(),
            ret: self.apply(sig.ret.clone()),
            error: sig.error.clone().map(|e| self.apply(e)),
            ..sig.clone()
        }
    }
}

/// A record every program has without declaring it: `PyError`, what a call into Python fails
/// with, as the checker's prelude declares it.
pub fn prelude_type(name: &str) -> Option<TypeDef> {
    let text = |n: &str| FieldSig { name: Some(n.into()), ty: Ty::Str, has_default: false };
    (name == "PyError").then(|| TypeDef::Record { params: Vec::new(), fields: vec![text("kind"), text("message")] })
}

/// The type parameters a method is generic over: its type's, then its own.
fn method_params(method: &Method) -> Vec<String> {
    let mut names = method.owner_params.clone();
    names.extend(method.sig.type_params.iter().map(|(n, _)| n.clone()));
    names
}

/// The call `call` made, of a function or method only known with its type arguments, a call of
/// `callee`.
fn generic_call(call: Value, callee: Callee) -> Value {
    match call {
        Value::Expr(Expr::Call(_, operands)) => {
            Value::Expr(Expr::CallGeneric { callee, args: operands.into_iter().map(Arg::Value).collect() })
        }
        Value::Expr(Expr::CallSlots(_, args)) => Value::Expr(Expr::CallGeneric { callee, args }),
        other => other,
    }
}

/// The name of the record or sum type `item` declares, if it declares one.
fn item_type(item: &Item) -> Option<&str> {
    match item {
        Item::Record(r) => Some(&r.name.name),
        Item::Sum(s) => Some(&s.name.name),
        _ => None,
    }
}

/// What a call of a function of `sig` gives: its result, or a result type when it can fail.
fn call_ret(sig: &FnSig) -> Ty {
    match &sig.error {
        Some(error) => Ty::Result(Box::new(sig.ret.clone()), Box::new(error.clone())),
        None => sig.ret.clone(),
    }
}

/// Whether `ty` names no type left to infer: a type argument of a call, which may name the type
/// parameters of the function the call is in.
fn settled(ty: &Ty) -> bool {
    match ty {
        Ty::Var(_) | Ty::Error | Ty::Never | Ty::TypeName(_) | Ty::Module(_) => false,
        Ty::List(t) | Ty::Set(t) | Ty::Heap(t) | Ty::Optional(t) => settled(t),
        Ty::Dict(a, b) | Ty::Result(a, b) => settled(a) && settled(b),
        Ty::Tuple(items) | Ty::Adt(_, items) => items.iter().all(settled),
        Ty::Func(params, ret) => params.iter().all(settled) && settled(ret),
        _ => true,
    }
}

/// Whether `ty` names no type parameter and no type left to infer.
fn concrete(ty: &Ty) -> bool {
    match ty {
        Ty::Param(_) | Ty::Var(_) | Ty::Error | Ty::Never | Ty::TypeName(_) | Ty::Module(_) => false,
        Ty::List(t) | Ty::Set(t) | Ty::Heap(t) | Ty::Optional(t) => concrete(t),
        Ty::Dict(a, b) | Ty::Result(a, b) => concrete(a) && concrete(b),
        Ty::Tuple(items) | Ty::Adt(_, items) => items.iter().all(concrete),
        Ty::Func(params, ret) => params.iter().all(concrete) && concrete(ret),
        _ => true,
    }
}

/// The record and sum types named anywhere in `ty`, with their type arguments.
fn named_types<'t>(ty: &'t Ty, found: &mut Vec<(&'t str, &'t [Ty])>) {
    match ty {
        Ty::Adt(name, args) => {
            found.push((name, args));
            args.iter().for_each(|t| named_types(t, found));
        }
        Ty::List(t) | Ty::Set(t) | Ty::Heap(t) | Ty::Optional(t) => named_types(t, found),
        Ty::Dict(a, b) | Ty::Result(a, b) => {
            named_types(a, found);
            named_types(b, found);
        }
        Ty::Tuple(items) => items.iter().for_each(|t| named_types(t, found)),
        Ty::Func(params, ret) => {
            params.iter().for_each(|t| named_types(t, found));
            named_types(ret, found);
        }
        _ => {}
    }
}

/// The declared types whose recursion grows a type argument, `Deep(Nest[(T, T)])` inside
/// `Nest[T]`: each instance's fields name a larger instance, so the C types of one use never end.
/// A type its own fields lead back to may be given its parameters or closed types, nothing else.
fn growing_types(declared: &BTreeMap<String, TypeDef>) -> BTreeSet<String> {
    let mut named: HashMap<&str, Vec<(&str, &[Ty])>> = HashMap::new();
    for (name, def) in declared {
        let fields: Vec<&FieldSig> = match def {
            TypeDef::Record { fields, .. } => fields.iter().collect(),
            TypeDef::Sum { variants, .. } => variants.iter().flat_map(|v| v.fields.iter().flatten()).collect(),
        };
        let found = named.entry(name).or_default();
        fields.iter().for_each(|f| named_types(&f.ty, found));
    }
    let reaches = |from: &str, to: &str| {
        let mut seen = HashSet::new();
        let mut next = vec![from];
        while let Some(n) = next.pop() {
            if n == to {
                return true;
            }
            if seen.insert(n) {
                next.extend(named.get(n).into_iter().flatten().map(|&(m, _)| m));
            }
        }
        false
    };
    let grows = |args: &[Ty]| args.iter().any(|a| !matches!(a, Ty::Param(_)) && !concrete(a));
    named
        .iter()
        .filter(|(name, found)| found.iter().any(|&(other, args)| grows(args) && reaches(other, name)))
        .map(|(name, _)| name.to_string())
        .collect()
}

/// Whether `ty` mentions the type parameter `name`.
fn mentions(ty: &Ty, name: &str) -> bool {
    match ty {
        Ty::Param(p) => p == name,
        Ty::List(t) | Ty::Set(t) | Ty::Heap(t) | Ty::Optional(t) => mentions(t, name),
        Ty::Dict(a, b) | Ty::Result(a, b) => mentions(a, name) || mentions(b, name),
        Ty::Tuple(items) | Ty::Adt(_, items) => items.iter().any(|t| mentions(t, name)),
        Ty::Func(params, ret) => params.iter().any(|t| mentions(t, name)) || mentions(ret, name),
        _ => false,
    }
}

/// The type arguments `pattern`, written with the type parameters `names`, takes where a value
/// of `actual` is: each parameter bound the first time it is matched.
fn bind(pattern: &Ty, actual: &Ty, names: &[String], map: &mut HashMap<String, Ty>) {
    match (pattern, actual) {
        (Ty::Param(p), _) if names.contains(p) => {
            if settled(actual) {
                map.entry(p.clone()).or_insert_with(|| actual.clone());
            }
        }
        (Ty::List(a), Ty::List(b))
        | (Ty::Set(a), Ty::Set(b))
        | (Ty::Heap(a), Ty::Heap(b))
        | (Ty::Optional(a), Ty::Optional(b)) => bind(a, b, names, map),
        (Ty::Optional(a), _) => bind(a, actual, names, map),
        (Ty::Dict(a, b), Ty::Dict(c, d)) | (Ty::Result(a, b), Ty::Result(c, d)) => {
            bind(a, c, names, map);
            bind(b, d, names, map);
        }
        (Ty::Tuple(a), Ty::Tuple(b)) if a.len() == b.len() => {
            a.iter().zip(b).for_each(|(a, b)| bind(a, b, names, map));
        }
        (Ty::Adt(n, a), Ty::Adt(m, b)) if n == m && a.len() == b.len() => {
            a.iter().zip(b).for_each(|(a, b)| bind(a, b, names, map));
        }
        (Ty::Func(a, r), Ty::Func(b, s)) if a.len() == b.len() => {
            a.iter().zip(b).for_each(|(a, b)| bind(a, b, names, map));
            bind(r, s, names, map);
        }
        _ => {}
    }
}

/// Which argument each of `params` is given: by position, by name or, `None`, its default.
fn arg_slots<'e>(params: &[ast::Param], args: &'e [AstArg]) -> Vec<Option<&'e AstArg>> {
    let mut slots: Vec<Option<&AstArg>> = vec![None; params.len()];
    let mut next = 0;
    for a in args {
        match a {
            AstArg::Positional(_) | AstArg::Inout(..) => {
                if next < slots.len() {
                    slots[next] = Some(a);
                }
                next += 1;
            }
            AstArg::Keyword(key, _) => {
                if let Some(i) = params.iter().position(|p| p.name.name == key.name) {
                    slots[i] = Some(a);
                }
            }
        }
    }
    slots
}

/// Whether a `dyn` value can call the trait method `m`: one taking `self` not as `inout`, generic
/// over nothing and naming `Self` nowhere else.
fn dyn_callable(m: &Method) -> bool {
    let rest = &m.sig.params[usize::from(m.receiver.is_some())..];
    m.sig.type_params.is_empty()
        && m.receiver.is_some_and(|c| c != ast::Convention::Inout)
        && !rest.iter().any(|p| mentions(&p.ty, "Self"))
        && !mentions(&call_ret(&m.sig), "Self")
}

/// Whether `import` is of the `math` module, which the C target compiles to its own functions.
fn is_math(import: &ast::Import) -> bool {
    matches!(import.module.as_slice(), [m] if m.name == "math")
}

/// Each Python module `module` imports — neither a module of the language, as `math` is, nor a C
/// library — with the span of its import: what a target that cannot call Python refuses, whether
/// or not the rest of the module lowers.
pub fn python_imports(module: &Module) -> Vec<(String, Span)> {
    let path = |import: &ast::Import| import.module.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join(".");
    module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Import(import) if !is_math(import) && !lotml_check::is_c_library(&path(import)) => {
                Some((path(import), import.span))
            }
            _ => None,
        })
        .collect()
}

/// The module as functions of the IR; with `tests`, its `test` blocks too.
pub fn lower(module: &Module, checked: &Checked, text: &str, tests: bool) -> Result<Lowered, Vec<Diagnostic>> {
    let mut cx = Context {
        text,
        checked,
        line_starts: std::iter::once(0).chain(text.match_indices('\n').map(|(i, _)| i as u32 + 1)).collect(),
        resolved: checked.locals.iter().copied().collect(),
        fns: HashMap::new(),
        variant_of: HashMap::new(),
        defaults: HashMap::new(),
        lambdas: Vec::new(),
        fn_refs: BTreeSet::new(),
        methods: HashMap::new(),
        trait_fns: HashMap::new(),
        math_names: HashSet::new(),
        c_imports: HashMap::new(),
        py_imports: HashMap::new(),
        c_functions: BTreeMap::new(),
        c_libraries: BTreeMap::new(),
        libraries: BTreeSet::new(),
        diagnostics: Vec::new(),
    };
    for item in &module.items {
        match item {
            Item::Fn(f) => {
                cx.fns.insert(f.name.name.clone(), f);
            }
            Item::Impl(imp) => {
                if let ast::TypeKind::Named { name, .. } = &imp.target.kind {
                    for m in &imp.methods {
                        cx.methods.insert((name.name.clone(), m.name.name.clone()), (m, false));
                    }
                }
            }
            Item::Trait(t) => {
                for m in &t.methods {
                    cx.trait_fns.insert((t.name.name.clone(), m.name.name.clone()), m);
                }
            }
            Item::Import(import) if is_math(import) => {
                cx.math_names.extend(import.names.iter().map(|n| n.name.clone()));
            }
            Item::Import(import) => cx.import(import),
            Item::Record(r) => {
                let defaults = r.fields.iter().map(|f| f.default.as_ref()).collect();
                cx.defaults.insert((r.name.name.clone(), None), defaults);
            }
            Item::Sum(s) => {
                for (k, v) in s.variants.iter().enumerate() {
                    cx.variant_of.insert(v.name.name.clone(), (s.name.name.clone(), k));
                    let defaults = v.fields.iter().flatten().map(|f| f.default.as_ref()).collect();
                    cx.defaults.insert((s.name.name.clone(), Some(k)), defaults);
                }
            }
            _ => {}
        }
    }
    let growing = growing_types(&checked.declared);
    for item in &module.items {
        let name = match item {
            Item::Record(r) => &r.name,
            Item::Sum(s) => &s.name,
            _ => continue,
        };
        if growing.contains(&name.name) {
            cx.unsupported(name.span, "a type whose recursion grows its type arguments");
        }
    }
    // A trait's default method is the method of each type implementing it without its own.
    for item in &module.items {
        if let Item::Impl(imp) = item
            && let Some(ast::TypeExpr { kind: ast::TypeKind::Named { name: trait_name, .. }, .. }) = &imp.trait_name
            && let ast::TypeKind::Named { name, .. } = &imp.target.kind
        {
            let defaults: Vec<(String, &FnDef)> = cx
                .trait_fns
                .iter()
                .filter(|((t, _), def)| *t == trait_name.name && def.body.is_some())
                .map(|((_, m), def)| (m.clone(), *def))
                .collect();
            for (m, def) in defaults {
                cx.methods.entry((name.name.clone(), m)).or_insert((def, true));
            }
        }
    }
    let mut functions = Vec::new();
    let mut test_names = Vec::new();
    for item in &module.items {
        match item {
            Item::Fn(f) => {
                if let Some(function) = cx.function(f) {
                    functions.push(function);
                }
            }
            Item::Test(t) if tests => {
                let k = test_names.len() + 1;
                let function = cx.test(t, k);
                test_names.push((t.name.clone(), function.name.clone()));
                functions.push(function);
            }
            Item::Impl(imp) => {
                let ast::TypeKind::Named { name, .. } = &imp.target.kind else {
                    cx.unsupported(imp.span, "this impl");
                    continue;
                };
                let owner = &name.name;
                for m in &imp.methods {
                    let method = cx.checked.methods.get(owner).and_then(|ms| ms.get(&m.name.name)).cloned();
                    let Some(method) = method else { continue };
                    let name = symbol::method(owner, &m.name.name);
                    let type_params = method_params(&method);
                    if let Some(function) = cx.lower_function(m, &method.sig, name, Subst::default(), type_params) {
                        functions.push(function);
                    }
                }
                // A trait's default method, for a type implementing the trait without its own: lowered
                // for that type, `Self` standing for it.
                let Some(ast::TypeExpr { kind: ast::TypeKind::Named { name: trait_name, .. }, .. }) = &imp.trait_name
                else {
                    continue;
                };
                let mut defaults: Vec<(String, &FnDef)> = cx
                    .methods
                    .iter()
                    .filter(|((o, m), (_, default))| {
                        *default && o == owner && cx.trait_fns.contains_key(&(trait_name.name.clone(), m.clone()))
                    })
                    .map(|((_, m), (def, _))| (m.clone(), *def))
                    .collect();
                defaults.sort_by(|a, b| a.0.cmp(&b.0));
                for (m, def) in defaults {
                    let Some(method) = cx.checked.methods.get(owner).and_then(|ms| ms.get(&m)).cloned() else {
                        continue;
                    };
                    let owner_ty = Ty::Adt(owner.clone(), method.owner_params.iter().cloned().map(Ty::Param).collect());
                    let subst = Subst { names: vec!["Self".to_string()], types: vec![owner_ty] };
                    let sig = subst.sig(&method.sig);
                    let name = symbol::method(owner, &m);
                    if let Some(function) = cx.lower_function(def, &sig, name, subst, method_params(&method)) {
                        functions.push(function);
                    }
                }
            }
            Item::Trait(t) => {
                if !t.type_params.is_empty() {
                    cx.unsupported(t.name.span, "a generic trait");
                }
            }
            Item::Import(_) | Item::Test(_) | Item::Error(_) | Item::Record(_) | Item::Sum(_) | Item::Class(_) => {}
        }
    }
    let defaults = cx.defaults_of(module);
    if !cx.diagnostics.is_empty() {
        return Err(cx.diagnostics);
    }
    let mut lambdas = Vec::new();
    for (function, captures, ty) in std::mem::take(&mut cx.lambdas).into_iter().flatten() {
        functions.push(function);
        lambdas.push((captures, ty));
    }
    for f in &functions {
        crate::verify::assert_valid(f, "lowering");
    }
    let mut lowered = Lowered {
        main: cx.fns.contains_key("main"),
        functions,
        declared: checked.declared.clone(),
        lambdas,
        fn_refs: std::mem::take(&mut cx.fn_refs),
        traits: checked.traits.iter().map(|(name, methods)| (name.clone(), methods.len())).collect(),
        dyn_methods: checked
            .traits
            .iter()
            .map(|(name, methods)| {
                let slots = methods.iter().map(|(m, method)| dyn_callable(method).then(|| m.clone())).collect();
                (name.clone(), slots)
            })
            .collect(),
        vtables: Vec::new(),
        tests: test_names,
        c_functions: std::mem::take(&mut cx.c_functions),
        libraries: std::mem::take(&mut cx.libraries),
        python_imports: python_imports(module),
        defaults,
        c_libraries: std::mem::take(&mut cx.c_libraries),
        line_starts: std::mem::take(&mut cx.line_starts),
    };
    crate::depth::count(&mut lowered);
    for f in &lowered.functions {
        crate::verify::assert_valid(f, "counting the recursion depth");
    }
    Ok(lowered)
}

struct Context<'a> {
    text: &'a str,
    checked: &'a Checked,
    line_starts: Vec<u32>,
    /// Each name that refers to a local, with the span of its declaration.
    resolved: HashMap<Span, Span>,
    fns: HashMap<String, &'a FnDef>,
    /// Each variant of a declared sum type: its type and its index.
    variant_of: HashMap<String, (String, usize)>,
    /// The default of each field of a record (`None` variant) or of a variant.
    defaults: HashMap<(String, Option<usize>), Vec<Option<&'a ast::Expr>>>,
    /// The lambdas lowered so far: each one's function, the types it captures and its type; `None`
    /// while its body, which may hold lambdas of its own, is being lowered.
    lambdas: Vec<Option<(Function, Vec<Ty>, Ty)>>,
    /// Each method of a type, by the type and its name, and whether it is a trait's default.
    methods: HashMap<(String, String), (&'a FnDef, bool)>,
    /// Each method of a trait, by the trait and its name.
    trait_fns: HashMap<(String, String), &'a FnDef>,
    /// The names `from math import …` brought into scope.
    math_names: HashSet<String>,
    /// The names `from c.<library> import …` brought into scope, with their signatures.
    c_imports: HashMap<String, FnSig>,
    /// The names `from <module> import …` brought into scope from a Python module, with the
    /// module and their signatures.
    py_imports: HashMap<String, (String, FnSig)>,
    /// The C functions called so far, by symbol.
    c_functions: BTreeMap<String, (Vec<Ty>, Ty)>,
    /// The C library each C function imported is in, by its symbol.
    c_libraries: BTreeMap<String, String>,
    libraries: BTreeSet<String>,
    fn_refs: BTreeSet<String>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Context<'a> {
    fn unsupported(&mut self, span: Span, what: &str) {
        self.diagnostics.push(Diagnostic::error(
            "E0402",
            span,
            format!("`--target llvm` does not compile {what} yet: run it with `--target python`"),
        ));
    }

    fn source(&self, span: Span) -> &'a str {
        self.text.get(span.range()).unwrap_or("")
    }

    /// The function `f`, generic over its type parameters when it has them.
    fn function(&mut self, f: &'a FnDef) -> Option<Function> {
        let sig = self.checked.functions.get(&f.name.name)?.clone();
        let type_params = sig.type_params.iter().map(|(n, _)| n.clone()).collect();
        self.lower_function(f, &sig, symbol::function(&f.name.name), Subst::default(), type_params)
    }

    /// `from c.<library> import f, g`: the functions called directly and the library linked
    /// (R5.4). Any other module is Python's, which the C target has none of (R5.3).
    fn import(&mut self, import: &'a ast::Import) {
        let path = import.module.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join(".");
        if !lotml_check::is_c_library(&path) {
            for name in &import.names {
                if let Some(sig) = self.checked.foreign.get(&path).and_then(|fs| fs.get(&name.name)) {
                    self.py_imports.insert(name.name.clone(), (path.clone(), sig.clone()));
                } else if let Some(sig) = self.checked.py_constructor(&format!("{path}.{}", name.name)) {
                    // A class imported by name is called through its constructor (adr:0034).
                    self.py_imports.insert(name.name.clone(), (path.clone(), sig));
                }
            }
            return;
        }
        let Some(functions) = self.checked.foreign.get(&path) else { return };
        self.libraries.insert(path["c.".len()..].to_string());
        for name in &import.names {
            if let Some(sig) = functions.get(&name.name) {
                self.c_imports.insert(name.name.clone(), sig.clone());
                self.c_libraries.insert(name.name.clone(), path["c.".len()..].to_string());
            }
        }
    }

    /// Each default of a parameter of the module's functions and methods and of a field of its
    /// records and variants, as a function of no parameters returning it.
    fn defaults_of(&mut self, module: &'a Module) -> Vec<DefaultValue> {
        let mut out = Vec::new();
        let mut functions: Vec<(String, &'a FnDef, FnSig)> = Vec::new();
        for item in &module.items {
            match item {
                Item::Fn(f) => {
                    if let Some(sig) = self.checked.functions.get(&f.name.name) {
                        functions.push((symbol::function(&f.name.name), f, sig.clone()));
                    }
                }
                Item::Impl(imp) => {
                    let ast::TypeKind::Named { name, .. } = &imp.target.kind else { continue };
                    for m in &imp.methods {
                        if let Some(method) = self.checked.methods.get(&name.name).and_then(|ms| ms.get(&m.name.name)) {
                            functions.push((symbol::method(&name.name, &m.name.name), m, method.sig.clone()));
                        }
                    }
                }
                _ => {}
            }
        }
        for (function, def, sig) in functions {
            for (param, p) in def.params.iter().zip(&sig.params) {
                if let Some(default) = &param.default {
                    let lowered = self.default(default, &p.ty, out.len());
                    out.push(DefaultValue {
                        of: DefaultOf::Param { function: function.clone(), param: p.name.clone() },
                        function: lowered,
                    });
                }
            }
        }
        let mut fields: Vec<(String, String, &'a ast::Expr, Ty)> = Vec::new();
        let mut field_defaults = |owner: &str, defs: &'a [ast::Field], types: &[FieldSig]| {
            for (i, (f, t)) in defs.iter().zip(types).enumerate() {
                if let Some(default) = &f.default {
                    let field = f.name.as_ref().map_or(format!("_{i}"), |n| n.name.clone());
                    fields.push((owner.to_string(), field, default, t.ty.clone()));
                }
            }
        };
        for item in &module.items {
            match (item, item_type(item).and_then(|n| self.checked.declared.get(n))) {
                (Item::Record(r), Some(TypeDef::Record { fields: types, .. })) => {
                    field_defaults(&r.name.name, &r.fields, types.as_slice());
                }
                (Item::Sum(s), Some(TypeDef::Sum { variants, .. })) => {
                    for (v, sig) in s.variants.iter().zip(variants) {
                        if let (Some(defs), Some(types)) = (&v.fields, &sig.fields) {
                            field_defaults(&v.name.name, defs.as_slice(), types.as_slice());
                        }
                    }
                }
                _ => {}
            }
        }
        for (owner, field, default, ty) in fields {
            let lowered = self.default(default, &ty, out.len());
            out.push(DefaultValue { of: DefaultOf::Field { owner, field }, function: lowered });
        }
        out
    }

    /// The default `e`, of type `ty`, as the `index`th function of no parameters returning it.
    fn default(&mut self, e: &'a ast::Expr, ty: &Ty, index: usize) -> Function {
        let span = e.span;
        let mut b = Builder {
            cx: self,
            function: Function {
                name: symbol::default(index),
                source_name: "<default>".to_string(),
                type_params: Vec::new(),
                params: Vec::new(),
                ret: ty.clone(),
                locals: Vec::new(),
                body: Vec::new(),
                span,
            },
            vars: HashMap::new(),
            blocks: vec![Vec::new()],
            span,
            subst: Subst::default(),
            bounds: HashMap::new(),
            at: None,
        };
        let v = b.value(e);
        let v = b.coerce(v, ty);
        let returned = if is_unit(ty) { None } else { Some(v) };
        b.push(StmtKind::Return(returned));
        let mut function = b.function;
        function.body = b.blocks.pop().unwrap_or_default();
        function
    }

    /// The `test` block `t`, the `k`th, as a function of no parameters: named in a report's trace
    /// as the Python target names it.
    fn test(&mut self, t: &'a ast::TestDef, k: usize) -> Function {
        let span = t.span;
        let mut b = Builder {
            cx: self,
            function: Function {
                name: symbol::test(k),
                source_name: format!("__test_{k}"),
                type_params: Vec::new(),
                params: Vec::new(),
                ret: Ty::Unit,
                locals: Vec::new(),
                body: Vec::new(),
                span,
            },
            vars: HashMap::new(),
            blocks: vec![Vec::new()],
            span,
            subst: Subst::default(),
            bounds: HashMap::new(),
            at: None,
        };
        b.statements(&t.body.stmts);
        let mut function = b.function;
        function.body = b.blocks.pop().unwrap_or_default();
        function
    }

    /// The function `f`, of signature `sig`, as the C function `name`, generic over
    /// `type_params`, its expressions' types read through `subst`: an `inout` parameter is a
    /// pointer to the caller's slot.
    fn lower_function(
        &mut self,
        f: &'a FnDef,
        sig: &FnSig,
        name: String,
        subst: Subst,
        type_params: Vec<String>,
    ) -> Option<Function> {
        let span = f.span;
        let ret = match &sig.error {
            Some(error) => Ty::Result(Box::new(sig.ret.clone()), Box::new(error.clone())),
            None => sig.ret.clone(),
        };
        let mut b = Builder {
            cx: self,
            function: Function {
                name,
                source_name: f.name.name.clone(),
                type_params,
                params: Vec::new(),
                ret,
                locals: Vec::new(),
                body: Vec::new(),
                span,
            },
            vars: HashMap::new(),
            blocks: vec![Vec::new()],
            span,
            subst,
            bounds: sig.type_params.iter().filter_map(|(n, b)| Some((n.clone(), b.clone()?))).collect(),
            at: None,
        };
        for p in &sig.params {
            let local = b.declare(p.span, &p.name, p.ty.clone());
            b.function.locals[local].by_ref = p.convention == ast::Convention::Inout;
            b.function.params.push(local);
        }
        if let Some(body) = &f.body {
            b.statements(&body.stmts);
            if let Ty::Result(ok, _) = &b.function.ret
                && is_unit(ok)
            {
                b.span = Span { start: body.end(), end: body.end() };
                let done = b.ok_result(Operand::Const(Const::Unit));
                b.push(StmtKind::Return(Some(done)));
            }
        }
        let mut function = b.function;
        function.body = b.blocks.pop().unwrap_or_default();
        Some(function)
    }
}

struct Builder<'c, 'a> {
    cx: &'c mut Context<'a>,
    function: Function,
    /// The local of each declaration, by the span of the declaring name.
    vars: HashMap<Span, Local>,
    blocks: Vec<Block>,
    /// The span of the statement being lowered, which every statement it lowers to carries.
    span: Span,
    /// What `Self` stands for in a trait's default method lowered for one type.
    subst: Subst,
    /// The trait each bounded type parameter of the function is bound by.
    bounds: HashMap<String, String>,
    /// The span of the expression whose value is being held, while it is.
    at: Option<Span>,
}

/// What a loop gives its body: the element, and its type.
type Each<'f, 'c, 'a> = &'f mut dyn FnMut(&mut Builder<'c, 'a>, Operand, Ty);

fn int(v: i128) -> Operand {
    Operand::Const(Const::Int(v, IntKind::I64))
}

fn flag(b: bool) -> Operand {
    Operand::Const(Const::Bool(b))
}

fn rt(op: Builtin, args: Vec<Operand>, at: bool) -> Expr {
    Expr::Rt { op, args: args.into_iter().map(Arg::Value).collect(), at }
}

fn rt_args(op: Builtin, args: Vec<Arg>, at: bool) -> Expr {
    Expr::Rt { op, args, at }
}

/// Whether the checker left `ty` to infer: a type, or the element of a list, it did not settle.
fn unknown(ty: &Ty) -> bool {
    match ty {
        Ty::Var(_) | Ty::Error => true,
        Ty::List(t) => matches!(**t, Ty::Var(_) | Ty::Error),
        _ => false,
    }
}

/// The type of what iterating a value of `ty` gives.
fn element(ty: &Ty) -> Ty {
    match ty {
        Ty::List(t) | Ty::Set(t) | Ty::Heap(t) => (**t).clone(),
        Ty::Dict(k, _) => (**k).clone(),
        Ty::Str => Ty::Str,
        Ty::Tuple(ts) if !ts.is_empty() => ts[0].clone(),
        _ => Ty::Error,
    }
}

impl<'c, 'a> Builder<'c, 'a> {
    fn push(&mut self, kind: StmtKind) {
        let span = self.span;
        let at = self.at.unwrap_or(span);
        self.blocks.last_mut().expect("a block").push(Stmt { span, at, kind });
    }

    fn new_local(&mut self, ty: Ty, name: Option<&str>) -> Local {
        self.function.locals.push(LocalInfo { ty, name: name.map(ToString::to_string), by_ref: false });
        self.function.locals.len() - 1
    }

    fn temp(&mut self, ty: Ty) -> Local {
        self.new_local(ty, None)
    }

    /// `expr`, of type `ty`, held in a new temporary.
    fn hold(&mut self, ty: Ty, expr: Expr) -> Operand {
        let t = self.temp(ty);
        self.push(StmtKind::Let(t, expr));
        Operand::Local(t)
    }

    fn declare(&mut self, span: Span, name: &str, ty: Ty) -> Local {
        if let Some(&local) = self.vars.get(&span) {
            return local;
        }
        // A name declared in each branch of an `if` is one local: the checker has the later
        // declarations refer to the first.
        if let Some(decl) = self.cx.resolved.get(&span)
            && *decl != span
            && let Some(&local) = self.vars.get(decl)
        {
            self.vars.insert(span, local);
            return local;
        }
        let local = self.new_local(ty, Some(name));
        self.vars.insert(span, local);
        local
    }

    /// The local a name refers to, if it is one.
    fn local_of(&self, span: Span) -> Option<Local> {
        self.cx.resolved.get(&span).and_then(|decl| self.vars.get(decl)).copied()
    }

    /// Whether `func` names the prelude's `name`: neither a local nor a function of the module.
    fn is_prelude(&self, func: &ast::Expr, name: &str) -> bool {
        matches!(&func.kind, ExprKind::Name(n) if n == name)
            && self.local_of(func.span).is_none()
            && !self.cx.fns.contains_key(name)
    }

    fn block(&mut self, f: impl FnOnce(&mut Self)) -> Block {
        self.blocks.push(Vec::new());
        f(self);
        self.blocks.pop().expect("the block pushed")
    }

    fn ty(&self, e: &ast::Expr) -> Ty {
        let ty = self.subst.apply(self.cx.checked.types.get(&e.span).cloned().unwrap_or(Ty::Error));
        if unknown(&ty) { self.left_to_infer(e).unwrap_or(ty) } else { ty }
    }

    /// The type of `e` where the checker left it to infer, as for a name unpacked from a list and
    /// what is computed from it: a name's local's, an operation's from its operands', a `map` or a
    /// `sum` over a prelude function's.
    fn left_to_infer(&self, e: &ast::Expr) -> Option<Ty> {
        let ty = match &e.kind {
            ExprKind::Name(_) => self.function.locals[self.local_of(e.span)?].ty.clone(),
            ExprKind::Call { func, args } => {
                let ExprKind::Name(name) = &func.kind else { return None };
                if self.local_of(func.span).is_some() {
                    return None;
                }
                match (name.as_str(), args.as_slice()) {
                    ("map", [AstArg::Positional(f), AstArg::Positional(xs)]) => {
                        Ty::list(self.key_type(f, &element(&self.ty(xs))))
                    }
                    ("sum", [AstArg::Positional(xs)]) => element(&self.ty(xs)),
                    _ => return None,
                }
            }
            ExprKind::Binary { op, left, right } => {
                let (l, r) = (self.ty(left), self.ty(right));
                match (op, &l, &r) {
                    (ast::BinOp::Div, Ty::Int(_), Ty::Int(_)) => F64,
                    (_, Ty::Int(_), Ty::Float(_)) => r,
                    _ => l,
                }
            }
            ExprKind::Unary { operand, .. } => self.ty(operand),
            ExprKind::Compare { .. } | ExprKind::Not(_) => Ty::Bool,
            _ => return None,
        };
        (!unknown(&ty)).then_some(ty)
    }

    fn local_ty(&self, operand: &Operand) -> Ty {
        match operand {
            Operand::Local(l) => self.function.locals[*l].ty.clone(),
            Operand::Const(Const::Int(_, k)) => Ty::Int(*k),
            Operand::Const(Const::Float(_)) => Ty::Float(lotml_check::ty::FloatKind::F64),
            Operand::Const(Const::Bool(_)) => Ty::Bool,
            Operand::Const(Const::Str(_)) => Ty::Str,
            Operand::Const(Const::Bytes(_)) => Ty::Bytes,
            Operand::Const(_) => Ty::Unit,
        }
    }

    fn unsupported(&mut self, span: Span, what: &str) -> Operand {
        self.cx.unsupported(span, what);
        Operand::Const(Const::Unit)
    }

    // Statements ------------------------------------------------------------------------

    fn statements(&mut self, stmts: &[ast::Stmt]) {
        for stmt in stmts {
            self.statement(stmt);
        }
    }

    fn statement(&mut self, stmt: &ast::Stmt) {
        self.span = stmt.span;
        match &stmt.kind {
            Ast::Expr(e) => self.effect(e),
            Ast::Var { name, value, .. } => {
                let declared = self.var_type(name.span).unwrap_or_else(|| self.ty(value));
                let value = self.value(value);
                let value = self.coerce(value, &declared);
                let local = self.declare(name.span, &name.name, declared);
                self.push(StmtKind::Let(local, Expr::Use(value)));
            }
            Ast::Annotated { target, value, .. } => {
                let declared = self.var_type(target.span).unwrap_or_else(|| self.ty(value));
                let value = self.value(value);
                let value = self.coerce(value, &declared);
                let local = self.declare(target.span, &target.name, declared);
                self.push(StmtKind::Let(local, Expr::Use(value)));
            }
            Ast::Assign { target, value } => {
                let ty = self.ty(value);
                let v = self.value(value);
                self.assign_to(target, v, ty);
            }
            Ast::AugAssign { target, op, value } => self.aug_assign(target, *op, value),
            Ast::Return(value) => {
                let ret = self.function.ret.clone();
                let value = match (&ret, value) {
                    (Ty::Result(ok, _), _) => {
                        let v = match value {
                            Some(v) => {
                                let v = self.value(v);
                                self.coerce(v, ok)
                            }
                            None => Operand::Const(Const::Unit),
                        };
                        Some(self.ok_result(v))
                    }
                    (_, Some(v)) if !is_unit(&ret) => {
                        let v = self.value(v);
                        Some(self.coerce(v, &ret))
                    }
                    (_, Some(v)) => {
                        self.effect(v);
                        None
                    }
                    (_, None) => None,
                };
                self.push(StmtKind::Return(value));
            }
            Ast::Assert { test, message } => self.assert(test, message.as_ref()),
            Ast::Pass => {}
            Ast::Break => self.push(StmtKind::Break),
            Ast::Continue => self.push(StmtKind::Continue),
            Ast::If { branches, orelse } => self.if_chain(branches, orelse.as_ref()),
            Ast::While { test, body } => {
                let saved = self.span;
                let body = self.block(|b| {
                    let ok = b.condition(test);
                    let stop = b.block(|b| b.push(StmtKind::Break));
                    b.span = saved;
                    b.push(StmtKind::If(ok, Vec::new(), stop));
                    b.statements(&body.stmts);
                });
                self.span = saved;
                self.push(StmtKind::Loop(body));
            }
            Ast::For { target, iter, body } => {
                let saved = self.span;
                self.each(iter, &mut |b, element, ty| {
                    b.bind(target, element, &ty);
                    b.statements(&body.stmts);
                    b.span = saved;
                });
            }
            Ast::Match { subject, arms } => self.match_stmt(subject, arms),
            Ast::Error => {}
        }
    }

    /// `assert test, message`: a failed one reports its text and, for one comparison, each side
    /// as it was compared, left first, the message evaluated only then — as on the Python target.
    fn assert(&mut self, test: &ast::Expr, message: Option<&ast::Expr>) {
        let expression = self.cx.source(test.span).to_string();
        let text = Operand::Const(Const::Str(expression.clone()));
        let null = || Arg::Value(Operand::Const(Const::Null));
        if let ExprKind::Compare { first, rest } = &test.kind
            && let [(op, _)] = rest.as_slice()
        {
            let result = self.temp(Ty::Bool);
            let mut left_ty = self.ty(first);
            let mut left = self.value(first);
            let (seen, seen_ty) = (left.clone(), left_ty.clone());
            let (right, right_ty) = self.compare_rest(result, &mut left, &mut left_ty, rest);
            let fail = self.block(|b| {
                let (md, m) = b.assert_message(message);
                let args = vec![
                    Arg::Value(text),
                    Arg::Value(Operand::Const(Const::Str(op.text().to_string()))),
                    Arg::Desc(seen_ty.clone()),
                    Arg::Address(seen, seen_ty),
                    Arg::Desc(right_ty.clone()),
                    Arg::Address(right, right_ty),
                    md,
                    m,
                ];
                b.push(StmtKind::Do(rt_args(Builtin::AssertCompared, args, true)));
            });
            self.push(StmtKind::If(Operand::Local(result), Vec::new(), fail));
            return;
        }
        let ok = self.condition(test);
        let fail = self.block(|b| match message {
            None => b.push(StmtKind::Panic(Panic::Assert(expression))),
            Some(_) => {
                let (md, m) = b.assert_message(message);
                let args = vec![Arg::Value(text), null(), null(), null(), null(), null(), md, m];
                b.push(StmtKind::Do(rt_args(Builtin::AssertCompared, args, true)));
            }
        });
        self.push(StmtKind::If(ok, Vec::new(), fail));
    }

    /// The descriptor and address of an `assert`'s message, or two NULLs when it has none.
    fn assert_message(&mut self, message: Option<&ast::Expr>) -> (Arg, Arg) {
        match message {
            Some(m) => {
                let ty = self.ty(m);
                let v = self.value(m);
                (Arg::Desc(ty.clone()), Arg::Address(v, ty))
            }
            None => (Arg::Value(Operand::Const(Const::Null)), Arg::Value(Operand::Const(Const::Null))),
        }
    }

    /// The type a `var` or annotated local was declared with, as the checker resolved it.
    fn var_type(&self, span: Span) -> Option<Ty> {
        self.cx.checked.types.get(&span).cloned().map(|t| self.subst.apply(t))
    }

    /// `target = value`: a name declared or assigned again, a tuple unpacked, an element set.
    fn assign_to(&mut self, target: &ast::Expr, value: Operand, ty: Ty) {
        match &target.kind {
            ExprKind::Name(name) => {
                let local = match self.local_of(target.span) {
                    Some(local) => local,
                    None => {
                        let decl = self.cx.resolved.get(&target.span).copied().unwrap_or(target.span);
                        self.declare(decl, name, ty)
                    }
                };
                let to = self.function.locals[local].ty.clone();
                let value = self.coerce(value, &to);
                self.push(StmtKind::Let(local, Expr::Use(value)));
            }
            ExprKind::Tuple(items) => {
                for (part, t) in self.unpack(value, &ty, items.len(), target.span) {
                    let item = &items[part.0];
                    self.assign_to(item, part.1, t);
                }
            }
            ExprKind::Index { object, index } if matches!(self.ty(object), Ty::Dict(..)) => {
                let Ty::Dict(key_ty, value_ty) = self.ty(object) else { return };
                let Some(place) = self.place(object) else {
                    self.unsupported(target.span, "this assignment target");
                    return;
                };
                let k = self.value(index);
                let k = self.coerce(k, &key_ty);
                let value = self.coerce(value, &value_ty);
                self.push(StmtKind::Mutate {
                    op: Builtin::DictSet,
                    place,
                    args: vec![Arg::Address(k, *key_ty), Arg::Address(value, *value_ty)],
                    at: false,
                    result: None,
                });
            }
            ExprKind::Index { .. } | ExprKind::Attr { .. } => match self.place(target) {
                Some(place) => {
                    let to = self.place_ty(&place);
                    let value = self.coerce(value, &to);
                    self.push(StmtKind::Store(place, value));
                }
                None => {
                    self.unsupported(target.span, "this assignment target");
                }
            },
            _ => {
                self.unsupported(target.span, "this assignment target");
            }
        }
    }

    fn aug_assign(&mut self, target: &ast::Expr, op: ast::BinOp, value: &ast::Expr) {
        let Some(place) = self.place(target) else {
            self.unsupported(target.span, "this assignment target");
            return;
        };
        let ty = self.ty(target);
        let right_ty = self.ty(value);
        let current = self.read_place(&place);
        let right = self.value(value);
        let combined = self.binary(op, current, right, &ty, &right_ty);
        if place.proj.is_empty() {
            self.push(StmtKind::Let(place.local, combined));
        } else {
            let new = self.hold(ty, combined);
            self.push(StmtKind::Store(place, new));
        }
    }

    /// Where `e` is stored: a local, or an element of a list reached from one.
    fn place(&mut self, e: &ast::Expr) -> Option<Place> {
        match &e.kind {
            ExprKind::Name(_) => self.local_of(e.span).map(|local| Place { local, proj: Vec::new() }),
            ExprKind::Index { object, index } if matches!(self.ty(object), Ty::List(_)) => {
                let mut place = self.place(object)?;
                let i = self.value(index);
                place.proj.push(Proj::Index(i));
                Some(place)
            }
            ExprKind::Index { object, index } if matches!(self.ty(object), Ty::Dict(..)) => {
                let mut place = self.place(object)?;
                let k = self.value(index);
                place.proj.push(Proj::Key(k));
                Some(place)
            }
            ExprKind::Attr { object, name } => {
                let ty = self.ty(object);
                let (index, _) = self.record_field(&ty, &name.name)?;
                let mut place = self.place(object)?;
                place.proj.push(Proj::Field(index));
                Some(place)
            }
            ExprKind::Call { func, args } => {
                let ExprKind::Attr { object, name } = &func.kind else { return None };
                let Ty::Dict(_, value_ty) = self.ty(object) else { return None };
                if name.name != "setdefault" || args.len() != 2 {
                    return None;
                }
                let mut place = self.place(object)?;
                let k = self.value(args[0].expr());
                let d = self.value(args[1].expr());
                let d = self.coerce(d, &value_ty);
                place.proj.push(Proj::SetDefault(k, d));
                Some(place)
            }
            _ => None,
        }
    }

    /// The type of what `place` holds.
    fn place_ty(&self, place: &Place) -> Ty {
        let mut ty = self.function.locals[place.local].ty.clone();
        for proj in &place.proj {
            ty = match proj {
                Proj::Index(_) | Proj::OwnedIndex(_) => element(&ty),
                Proj::Field(i) => self.fields(&ty, None).get(*i).cloned().unwrap_or(Ty::Error),
                Proj::Key(_) | Proj::SetDefault(..) => match &ty {
                    Ty::Dict(_, v) => (**v).clone(),
                    _ => Ty::Error,
                },
            };
        }
        ty
    }

    /// The value at `place`, read.
    fn read_place(&mut self, place: &Place) -> Operand {
        if place.proj.iter().any(|p| matches!(p, Proj::Key(_) | Proj::SetDefault(..))) {
            let ty = self.place_ty(place);
            return self.hold(ty, Expr::ReadPlace(place.clone()));
        }
        let mut value = Operand::Local(place.local);
        let mut ty = self.function.locals[place.local].ty.clone();
        for proj in &place.proj {
            match proj {
                Proj::Index(i) | Proj::OwnedIndex(i) => {
                    let elem = element(&ty);
                    value = self.hold(
                        elem.clone(),
                        Expr::ListGet { list: value, index: i.clone(), elem: elem.clone(), checked: true },
                    );
                    ty = elem;
                }
                Proj::Field(index) => {
                    let field = self.fields(&ty, None)[*index].clone();
                    value =
                        self.hold(field.clone(), Expr::Field { value, ty: ty.clone(), variant: None, index: *index });
                    ty = field;
                }
                Proj::Key(_) | Proj::SetDefault(..) => {}
            }
        }
        value
    }

    // Values of declared types, optionals and results ------------------------------------

    /// The types of the fields of a record (`variant` None) or of a variant of `ty`, with its
    /// type arguments substituted.
    fn fields(&self, ty: &Ty, variant: Option<usize>) -> Vec<Ty> {
        let Ty::Adt(name, args) = ty else { return Vec::new() };
        let substitute = |fields: &[FieldSig], params: &[String]| {
            fields.iter().map(|f| f.ty.substitute(params, args)).collect::<Vec<Ty>>()
        };
        let def = self.cx.checked.declared.get(name).cloned().or_else(|| prelude_type(name));
        match (def.as_ref(), variant) {
            (Some(TypeDef::Record { params, fields }), None) => substitute(fields, params),
            (Some(TypeDef::Sum { params, variants }), Some(k)) => {
                variants.get(k).and_then(|v| v.fields.as_deref()).map(|f| substitute(f, params)).unwrap_or_default()
            }
            _ => Vec::new(),
        }
    }

    /// The names of the fields of a record or of a variant: `None` for a positional one.
    fn field_names(&self, ty: &Ty, variant: Option<usize>) -> Vec<Option<String>> {
        let Ty::Adt(name, _) = ty else { return Vec::new() };
        let def = self.cx.checked.declared.get(name).cloned().or_else(|| prelude_type(name));
        match (def.as_ref(), variant) {
            (Some(TypeDef::Record { fields, .. }), None) => fields.iter().map(|f| f.name.clone()).collect(),
            (Some(TypeDef::Sum { variants, .. }), Some(k)) => variants
                .get(k)
                .and_then(|v| v.fields.as_deref())
                .map(|f| f.iter().map(|f| f.name.clone()).collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    /// The index and type of the field `name` of the record type `ty`.
    fn record_field(&self, ty: &Ty, name: &str) -> Option<(usize, Ty)> {
        let index = self.field_names(ty, None).iter().position(|n| n.as_deref() == Some(name))?;
        Some((index, self.fields(ty, None)[index].clone()))
    }

    /// `op` as a value of `to`: a value put into an optional where one is wanted, or taken out
    /// of an optional the checker narrowed.
    fn coerce(&mut self, op: Operand, to: &Ty) -> Operand {
        let from = self.local_ty(&op);
        match to {
            Ty::Optional(inner) if !matches!(from, Ty::Optional(_) | Ty::Never | Ty::Error) => {
                if matches!(op, Operand::Const(Const::Unit)) {
                    return self.hold(to.clone(), Expr::OptNew { ty: to.clone(), value: None });
                }
                let v = self.coerce(op, inner);
                self.hold(to.clone(), Expr::OptNew { ty: to.clone(), value: Some(v) })
            }
            Ty::Dyn(_) if matches!(from, Ty::Adt(..) | Ty::Param(_)) => {
                self.hold(to.clone(), Expr::ToDynOf { value: op, ty: to.clone(), from })
            }
            Ty::List(to_elem)
                if matches!(**to_elem, Ty::Dyn(_))
                    && matches!(&from, Ty::List(e) if matches!(**e, Ty::Adt(..) | Ty::Param(_))) =>
            {
                let (to_elem, from_elem) = ((**to_elem).clone(), element(&from));
                let list = self.new_list(&to_elem);
                self.indexed(&[(op, from_elem)], false, &mut |b, x, _| {
                    let d = b.coerce(x, &to_elem);
                    b.push_element(list, d, &to_elem);
                });
                Operand::Local(list)
            }
            _ if matches!(&from, Ty::Optional(inner) if **inner == *to) => self.hold(to.clone(), Expr::OptValue(op)),
            _ => op,
        }
    }

    /// `Ok(value)` of the current function's result type.
    fn ok_result(&mut self, value: Operand) -> Operand {
        let ty = self.function.ret.clone();
        self.hold(ty.clone(), Expr::ResultNew { ty, ok: true, value })
    }

    /// Return the error `value` from the current function.
    fn fail_with(&mut self, value: Operand) {
        let ty = self.function.ret.clone();
        let Ty::Result(_, error) = &ty else {
            // In a `test` block, which returns nothing: the test ends with this error.
            let error = self.local_ty(&value);
            let args = vec![Arg::Desc(error.clone()), Arg::Address(value, error)];
            self.push(StmtKind::Do(rt_args(Builtin::TestError, args, true)));
            return;
        };
        let value = self.coerce(value, error);
        let failed = self.hold(ty.clone(), Expr::ResultNew { ty, ok: false, value });
        self.push(StmtKind::Return(Some(failed)));
    }

    /// A new record or variant of `ty` from the arguments of its constructor, by position and by
    /// name, the fields left out given their defaults.
    fn construct(&mut self, whole: &ast::Expr, ty: Ty, variant: Option<usize>, args: &[AstArg]) -> Value {
        let names = self.field_names(&ty, variant);
        let types = self.fields(&ty, variant);
        let mut slots: Vec<Option<&ast::Expr>> = vec![None; names.len()];
        let mut next = 0;
        for a in args {
            match a {
                AstArg::Positional(e) => {
                    if next < slots.len() {
                        slots[next] = Some(e);
                    }
                    next += 1;
                }
                AstArg::Keyword(key, e) => {
                    if let Some(i) = names.iter().position(|n| n.as_deref() == Some(key.name.as_str())) {
                        slots[i] = Some(e);
                    }
                }
                AstArg::Inout(e, _) => return Value::Done(self.unsupported(e.span, "an `inout` argument")),
            }
        }
        let Ty::Adt(owner, _) = &ty else { return Value::Done(self.unsupported(whole.span, "this constructor")) };
        let defaults = self.cx.defaults.get(&(owner.clone(), variant)).cloned().unwrap_or_default();
        let mut fields = Vec::new();
        for (i, slot) in slots.iter().enumerate() {
            let Some(e) = slot.or(defaults.get(i).copied().flatten()) else {
                return Value::Done(self.unsupported(whole.span, "a constructor missing a field"));
            };
            let v = self.value(e);
            fields.push(self.coerce(v, &types[i]));
        }
        Value::Expr(Expr::Construct { ty, variant, fields, reuse: None })
    }

    // match -----------------------------------------------------------------------------

    /// `match`: each arm tried in order, its body run when its pattern matches the subject and
    /// no arm before it did.
    fn match_stmt(&mut self, subject: &ast::Expr, arms: &[ast::Arm]) {
        let ty = self.ty(subject);
        let value = self.value(subject);
        if let Some(variants) = self.simple_arms(&ty, arms) {
            let tag = self.hold(Ty::Int(IntKind::U32), Expr::Tag(value.clone()));
            let saved = self.span;
            self.variant_chain(&variants, &value, &ty, &tag, saved);
            return;
        }
        let narrow =
            arms.iter().any(|a| matches!(&a.pattern.kind, PatternKind::Literal(e) if matches!(e.kind, ExprKind::None)));
        let done = self.temp(Ty::Bool);
        self.push(StmtKind::Let(done, Expr::Use(flag(false))));
        let saved = self.span;
        for arm in arms {
            let body = self.block(|b| {
                b.pattern(&arm.pattern, value.clone(), &ty, narrow, &mut |b| {
                    b.push(StmtKind::Let(done, Expr::Use(flag(true))));
                    b.statements(&arm.body.stmts);
                });
            });
            self.span = saved;
            let open = self.hold(Ty::Bool, Expr::Unary(UnOp::Not, Operand::Local(done), Ty::Bool));
            self.push(StmtKind::If(open, body, Vec::new()));
        }
    }

    /// The variant each arm takes apart, `None` for an arm matching anything, when every arm is
    /// a variant of the sum type `ty` whose fields are only named or ignored: such a match is a
    /// chain of tests of the tag, each arm a branch of its own.
    fn simple_arms<'m>(&self, ty: &Ty, arms: &'m [ast::Arm]) -> Option<Vec<(Option<usize>, &'m ast::Arm)>> {
        let Ty::Adt(owner, _) = ty else { return None };
        if !matches!(self.cx.checked.declared.get(owner), Some(TypeDef::Sum { .. })) {
            return None;
        }
        let variant = |name: &str| self.cx.variant_of.get(name).filter(|(o, _)| o == owner).map(|(_, k)| *k);
        let irrefutable = |p: &ast::Pattern| match &p.kind {
            PatternKind::Wildcard => true,
            PatternKind::Name(n) => !self.cx.variant_of.contains_key(&n.name),
            _ => false,
        };
        arms.iter()
            .map(|arm| match &arm.pattern.kind {
                PatternKind::Variant { name, args } if args.iter().all(irrefutable) => {
                    Some((Some(variant(&name.name)?), arm))
                }
                PatternKind::Name(n) if self.cx.variant_of.contains_key(&n.name) => {
                    Some((Some(variant(&n.name)?), arm))
                }
                PatternKind::Wildcard => Some((None, arm)),
                _ => None,
            })
            .collect()
    }

    fn variant_chain(
        &mut self,
        arms: &[(Option<usize>, &ast::Arm)],
        value: &Operand,
        ty: &Ty,
        tag: &Operand,
        saved: Span,
    ) {
        let Some(((variant, arm), rest)) = arms.split_first() else { return };
        let body = |b: &mut Self| {
            if let (Some(k), PatternKind::Variant { args, .. }) = (variant, &arm.pattern.kind) {
                let types = b.fields(ty, Some(*k));
                for (i, p) in args.iter().enumerate() {
                    let PatternKind::Name(name) = &p.kind else { continue };
                    let t = types.get(i).cloned().unwrap_or(Ty::Error);
                    let part = b.hold(
                        t.clone(),
                        Expr::Field { value: value.clone(), ty: ty.clone(), variant: Some(*k), index: i },
                    );
                    let local = b.declare(name.span, &name.name, t);
                    b.push(StmtKind::Let(local, Expr::Use(part)));
                }
            }
            b.statements(&arm.body.stmts);
        };
        match variant {
            None => body(self),
            Some(k) => {
                let k = *k;
                let test = self.hold(
                    Ty::Bool,
                    Expr::Compare(
                        CmpOp::Eq,
                        tag.clone(),
                        Operand::Const(Const::Int(k as i128, IntKind::U32)),
                        Ty::Int(IntKind::U32),
                    ),
                );
                let then = self.block(body);
                let otherwise = self.block(|b| b.variant_chain(rest, value, ty, tag, saved));
                self.span = saved;
                self.push(StmtKind::If(test, then, otherwise));
            }
        }
    }

    /// Run `inner` when `pattern` matches `value`, a value of `ty`, with its names bound.
    fn pattern(
        &mut self,
        pattern: &ast::Pattern,
        value: Operand,
        ty: &Ty,
        narrow: bool,
        inner: &mut dyn FnMut(&mut Self),
    ) {
        match &pattern.kind {
            PatternKind::Wildcard | PatternKind::Error => inner(self),
            PatternKind::Name(name) if self.cx.variant_of.contains_key(&name.name) => {
                self.variant_pattern(&name.name, &[], value, ty, inner);
            }
            PatternKind::Name(name) => {
                let (bound, bound_ty) = match ty {
                    Ty::Optional(t) if narrow => (self.hold((**t).clone(), Expr::OptValue(value)), (**t).clone()),
                    _ => (value, ty.clone()),
                };
                let local = self.declare(name.span, &name.name, bound_ty);
                self.push(StmtKind::Let(local, Expr::Use(bound)));
                inner(self);
            }
            PatternKind::Variant { name, args } => self.variant_pattern(&name.name, args, value, ty, inner),
            PatternKind::Tuple(items) => {
                let Ty::Tuple(types) = ty else {
                    self.unsupported(pattern.span, "this tuple pattern");
                    return;
                };
                let mut parts = Vec::new();
                for (i, (item, t)) in items.iter().zip(types).enumerate() {
                    let part = self.hold(t.clone(), Expr::TupleGet { tuple: value.clone(), index: i });
                    parts.push((item, part, t.clone()));
                }
                self.patterns(&parts, inner);
            }
            PatternKind::Literal(e) => {
                let test = match (&e.kind, ty) {
                    (ExprKind::None, _) => {
                        let some = self.hold(Ty::Bool, Expr::OptIsSome(value));
                        self.hold(Ty::Bool, Expr::Unary(UnOp::Not, some, Ty::Bool))
                    }
                    (_, Ty::Optional(t)) => {
                        let test = self.temp(Ty::Bool);
                        let some = self.hold(Ty::Bool, Expr::OptIsSome(value.clone()));
                        self.push(StmtKind::Let(test, Expr::Use(some.clone())));
                        let inner_ty = (**t).clone();
                        let then = self.block(|b| {
                            let v = b.hold(inner_ty.clone(), Expr::OptValue(value));
                            let lit = b.value(e);
                            b.push(StmtKind::Let(test, Expr::Compare(CmpOp::Eq, v, lit, inner_ty)));
                        });
                        self.push(StmtKind::If(some, then, Vec::new()));
                        Operand::Local(test)
                    }
                    _ => {
                        let lit = self.value(e);
                        self.hold(Ty::Bool, Expr::Compare(CmpOp::Eq, value, lit, ty.clone()))
                    }
                };
                let then = self.block(|b| inner(b));
                self.push(StmtKind::If(test, then, Vec::new()));
            }
        }
    }

    fn patterns(&mut self, parts: &[(&ast::Pattern, Operand, Ty)], inner: &mut dyn FnMut(&mut Self)) {
        let Some(((pattern, value, ty), rest)) = parts.split_first() else {
            inner(self);
            return;
        };
        self.pattern(pattern, value.clone(), ty, false, &mut |b| b.patterns(rest, inner));
    }

    /// A variant pattern: `Ok(p)`, `Err(p)`, or a variant of a sum type with its fields.
    fn variant_pattern(
        &mut self,
        name: &str,
        args: &[ast::Pattern],
        value: Operand,
        ty: &Ty,
        inner: &mut dyn FnMut(&mut Self),
    ) {
        if let (Ty::Result(ok_ty, err_ty), "Ok" | "Err") = (ty, name) {
            let ok = self.hold(Ty::Bool, Expr::ResultIsOk(value.clone()));
            let test = if name == "Ok" { ok } else { self.hold(Ty::Bool, Expr::Unary(UnOp::Not, ok, Ty::Bool)) };
            let (field_ty, read) = if name == "Ok" {
                ((**ok_ty).clone(), Expr::ResultValue(value))
            } else {
                ((**err_ty).clone(), Expr::ResultError(value))
            };
            let then = self.block(|b| match args.first() {
                Some(p) if !matches!(p.kind, PatternKind::Wildcard) => {
                    let part = b.hold(field_ty.clone(), read);
                    b.patterns(&[(p, part, field_ty)], inner);
                }
                _ => inner(b),
            });
            self.push(StmtKind::If(test, then, Vec::new()));
            return;
        }
        let Some((_, k)) = self.cx.variant_of.get(name).cloned() else {
            self.cx.unsupported(Span::new(0, 0), "this variant pattern");
            return;
        };
        let tag = self.hold(Ty::Int(IntKind::U32), Expr::Tag(value.clone()));
        let test = self.hold(
            Ty::Bool,
            Expr::Compare(CmpOp::Eq, tag, Operand::Const(Const::Int(k as i128, IntKind::U32)), Ty::Int(IntKind::U32)),
        );
        let types = self.fields(ty, Some(k));
        let ty = ty.clone();
        let then = self.block(|b| {
            let mut parts = Vec::new();
            for (i, p) in args.iter().enumerate() {
                if matches!(p.kind, PatternKind::Wildcard) {
                    continue;
                }
                let t = types.get(i).cloned().unwrap_or(Ty::Error);
                let part =
                    b.hold(t.clone(), Expr::Field { value: value.clone(), ty: ty.clone(), variant: Some(k), index: i });
                parts.push((p, part, t));
            }
            b.patterns(&parts, inner);
        });
        self.push(StmtKind::If(test, then, Vec::new()));
    }

    fn if_chain(&mut self, branches: &[(ast::Expr, ast::Block)], orelse: Option<&ast::Block>) {
        let Some(((test, body), rest)) = branches.split_first() else {
            if let Some(block) = orelse {
                self.statements(&block.stmts);
            }
            return;
        };
        let saved = self.span;
        let ok = self.condition(test);
        let then = self.block(|b| b.statements(&body.stmts));
        let otherwise = self.block(|b| b.if_chain(rest, orelse));
        self.span = saved;
        self.push(StmtKind::If(ok, then, otherwise));
    }

    /// Give `target` the parts of `value`: a name, or a tuple of targets taking its fields.
    fn bind(&mut self, target: &ast::Target, value: Operand, ty: &Ty) {
        match target {
            ast::Target::Name(name) => {
                let local = self.declare(name.span, &name.name, ty.clone());
                self.push(StmtKind::Let(local, Expr::Use(value)));
            }
            ast::Target::Tuple(items, span) => {
                for (part, t) in self.unpack(value, ty, items.len(), *span) {
                    let item = &items[part.0];
                    self.bind(item, part.1, &t);
                }
            }
        }
    }

    /// The `n` parts `value` unpacks into, each with its index and type: a tuple's fields, or a
    /// list's elements once its length is checked to be `n`, as Python checks it.
    fn unpack(&mut self, value: Operand, ty: &Ty, n: usize, span: Span) -> Vec<((usize, Operand), Ty)> {
        match ty {
            Ty::Tuple(types) => types
                .iter()
                .take(n)
                .enumerate()
                .map(|(i, t)| ((i, self.hold(t.clone(), Expr::TupleGet { tuple: value.clone(), index: i })), t.clone()))
                .collect(),
            Ty::List(elem) => {
                let elem = (**elem).clone();
                self.push(StmtKind::Do(rt(Builtin::ListUnpack, vec![value.clone(), int(n as i128)], true)));
                (0..n)
                    .map(|i| {
                        let get = Expr::ListGet {
                            list: value.clone(),
                            index: int(i as i128),
                            elem: elem.clone(),
                            checked: true,
                        };
                        ((i, self.hold(elem.clone(), get)), elem.clone())
                    })
                    .collect()
            }
            _ => {
                self.unsupported(span, "unpacking this value");
                Vec::new()
            }
        }
    }

    // Loops -----------------------------------------------------------------------------

    /// Run `body` once per element of `iter`: a range counted, `enumerate`, `zip` and
    /// `reversed` walked in place, a list by index, a string by character.
    fn each(&mut self, iter: &ast::Expr, body: Each<'_, 'c, 'a>) {
        if let ExprKind::Call { func, args } = &iter.kind {
            let positional: Vec<&ast::Expr> = args.iter().map(AstArg::expr).collect();
            if self.is_prelude(func, "range") {
                let values: Vec<Operand> = positional.iter().map(|a| self.value(a)).collect();
                let (start, stop, step) = match values.as_slice() {
                    [stop] => (int(0), stop.clone(), int(1)),
                    [start, stop] => (start.clone(), stop.clone(), int(1)),
                    [start, stop, step] => (start.clone(), stop.clone(), step.clone()),
                    _ => {
                        self.unsupported(iter.span, "this `range`");
                        return;
                    }
                };
                let var = self.temp(INT);
                let saved = self.span;
                let inner = self.block(|b| body(b, Operand::Local(var), INT));
                self.span = saved;
                self.push(StmtKind::ForRange { var, start, stop, step, body: inner, exit: Vec::new() });
                return;
            }
            if self.is_prelude(func, "enumerate") && !positional.is_empty() {
                let start = match positional.get(1) {
                    Some(s) => self.value(s),
                    None => int(0),
                };
                let counter = self.temp(INT);
                self.push(StmtKind::Let(counter, Expr::Use(start)));
                self.each(positional[0], &mut |b, element, ty| {
                    let index = b.hold(INT, Expr::Use(Operand::Local(counter)));
                    b.push(StmtKind::Let(counter, Expr::Binary(BinOp::Add, Operand::Local(counter), int(1), INT)));
                    let pair_ty = Ty::Tuple(vec![INT, ty]);
                    let pair =
                        b.hold(pair_ty.clone(), Expr::TupleNew { ty: pair_ty.clone(), items: vec![index, element] });
                    body(b, pair, pair_ty);
                });
                return;
            }
            if self.is_prelude(func, "zip") && !positional.is_empty() {
                let lists: Vec<(Operand, Ty)> = positional
                    .iter()
                    .map(|a| {
                        let list = self.walked(a);
                        let elem = element(&self.local_ty(&list));
                        (list, elem)
                    })
                    .collect();
                self.indexed(&lists, false, body);
                return;
            }
            if self.is_prelude(func, "reversed") && positional.len() == 1 {
                let list = self.walked(positional[0]);
                let elem = element(&self.local_ty(&list));
                self.indexed(&[(list, elem)], true, body);
                return;
            }
        }
        let ty = self.ty(iter);
        match ty {
            Ty::List(_) => {
                let snapshot = self.walked(iter);
                let elem = element(&ty);
                self.indexed(&[(snapshot, elem)], false, body);
            }
            Ty::Dict(..) | Ty::Set(_) => {
                let list = self.materialize(iter);
                let elem = element(&ty);
                self.indexed(&[(list, elem)], false, body);
            }
            Ty::Tuple(ref items) if !items.is_empty() && items.iter().all(|t| *t == items[0]) => {
                let list = self.materialize(iter);
                self.indexed(&[(list, items[0].clone())], false, body);
            }
            Ty::Str => {
                let over = self.value(iter);
                let snapshot = self.hold(Ty::Str, Expr::Use(over));
                let var = self.temp(Ty::Str);
                let saved = self.span;
                let inner = self.block(|b| body(b, Operand::Local(var), Ty::Str));
                self.span = saved;
                self.push(StmtKind::ForStr { var, over: snapshot, body: inner, exit: Vec::new() });
            }
            _ => {
                self.unsupported(iter.span, "a loop over this value");
            }
        }
    }

    /// Walk lists by index, together, as long as the shortest; backwards when `reverse`.
    fn indexed(&mut self, lists: &[(Operand, Ty)], reverse: bool, body: Each<'_, 'c, 'a>) {
        let k = self.temp(INT);
        if reverse {
            let (list, ty) = &lists[0];
            let n = self.hold(INT, Expr::Len(list.clone(), Ty::List(Box::new(ty.clone()))));
            self.push(StmtKind::Let(k, Expr::Binary(BinOp::Sub, n, int(1), INT)));
        } else {
            self.push(StmtKind::Let(k, Expr::Use(int(0))));
        }
        let saved = self.span;
        let inner = self.block(|b| {
            for (list, elem) in lists {
                let more = if reverse {
                    b.hold(Ty::Bool, Expr::Compare(CmpOp::Ge, Operand::Local(k), int(0), INT))
                } else {
                    let n = b.hold(INT, Expr::Len(list.clone(), Ty::List(Box::new(elem.clone()))));
                    b.hold(Ty::Bool, Expr::Compare(CmpOp::Lt, Operand::Local(k), n, INT))
                };
                let stop = b.block(|b| b.push(StmtKind::Break));
                b.push(StmtKind::If(more, Vec::new(), stop));
            }
            let elements: Vec<(Operand, Ty)> = lists
                .iter()
                .map(|(list, elem)| {
                    let get = Expr::ListGet {
                        list: list.clone(),
                        index: Operand::Local(k),
                        elem: elem.clone(),
                        checked: false,
                    };
                    (b.hold(elem.clone(), get), elem.clone())
                })
                .collect();
            let name = if reverse { Builtin::WrappingSub } else { Builtin::WrappingAdd };
            b.push(StmtKind::Let(k, rt(name, vec![Operand::Local(k), int(1)], false)));
            if let [(element, ty)] = elements.as_slice() {
                body(b, element.clone(), ty.clone());
            } else {
                let tuple_ty = Ty::Tuple(elements.iter().map(|(_, t)| t.clone()).collect());
                let items = elements.into_iter().map(|(e, _)| e).collect();
                let tuple = b.hold(tuple_ty.clone(), Expr::TupleNew { ty: tuple_ty.clone(), items });
                body(b, tuple, tuple_ty);
            }
        });
        self.span = saved;
        self.push(StmtKind::Loop(inner));
    }

    /// `e` as the list a loop walks by index: a list is held, so the loop's body can neither
    /// shrink nor free what it reads unchecked, as the Python target walks a copy; any other
    /// iterable is made into a new list.
    fn walked(&mut self, e: &ast::Expr) -> Operand {
        let ty = self.ty(e);
        if !matches!(ty, Ty::List(_)) {
            return self.materialize(e);
        }
        let v = self.value(e);
        self.hold(ty, Expr::Use(v))
    }

    /// `e` as a list: a list as it is, a string as its characters, a dict as its keys, a set as
    /// its elements in table order.
    fn materialize(&mut self, e: &ast::Expr) -> Operand {
        let ty = self.ty(e);
        let v = self.value(e);
        self.as_list(v, &ty)
    }

    fn as_list(&mut self, v: Operand, ty: &Ty) -> Operand {
        match ty {
            Ty::Tuple(items) if !items.is_empty() && items.iter().all(|t| *t == items[0]) => {
                let elem = items[0].clone();
                let parts = (0..items.len())
                    .map(|index| self.hold(elem.clone(), Expr::TupleGet { tuple: v.clone(), index }))
                    .collect();
                self.hold(Ty::list(elem.clone()), Expr::ListNew { elem, items: parts })
            }
            Ty::Str => self.hold(Ty::list(Ty::Str), rt(Builtin::StrChars, vec![v], false)),
            Ty::Dict(k, _) => self.hold(Ty::list((**k).clone()), rt(Builtin::DictKeys, vec![v], false)),
            Ty::Set(t) => self.hold(Ty::list((**t).clone()), rt(Builtin::SetList, vec![v], false)),
            _ => v,
        }
    }

    /// A new list of `elem`, filled by `fill` with `push_element`.
    fn new_list(&mut self, elem: &Ty) -> Local {
        let list = self.temp(Ty::list(elem.clone()));
        self.push(StmtKind::Let(list, Expr::ListNew { elem: elem.clone(), items: Vec::new() }));
        list
    }

    fn push_element(&mut self, list: Local, value: Operand, elem: &Ty) {
        self.push(StmtKind::Mutate {
            op: Builtin::ListPush,
            place: Place { local: list, proj: Vec::new() },
            args: vec![Arg::Address(value, elem.clone())],
            at: false,
            result: None,
        });
    }

    /// An optional `ty` from a runtime call that returns whether it found a value and sets it in
    /// an output: `args` and then the output.
    fn optional_from(&mut self, ty: &Ty, op: Builtin, mut args: Vec<Arg>, at: bool) -> Value {
        let inner = match ty {
            Ty::Optional(t) => (**t).clone(),
            other => other.clone(),
        };
        let found = self.temp(inner.clone());
        args.push(Arg::Out(found, inner));
        let ok = self.hold(Ty::Bool, Expr::Rt { op, args, at });
        Value::Expr(Expr::OptIf { ty: ty.clone(), cond: ok, value: Operand::Local(found) })
    }

    /// Every element of `iter`, in a new list.
    fn collect(&mut self, iter: &ast::Expr, elem: &Ty) -> Operand {
        let list = self.new_list(elem);
        let target = elem.clone();
        self.each(iter, &mut |b, element, _| b.push_element(list, element, &target));
        Operand::Local(list)
    }

    fn comprehension(&mut self, whole: &ast::Expr, element: &ast::Expr, loops: &[ast::Comprehension]) -> Operand {
        let elem = match self.ty(whole) {
            Ty::List(t) => *t,
            _ => self.ty(element),
        };
        let list = self.new_list(&elem);
        self.comprehension_loops(loops, &mut |b| {
            let v = b.value(element);
            b.push_element(list, v, &elem);
        });
        Operand::Local(list)
    }

    fn comprehension_loops(&mut self, loops: &[ast::Comprehension], inner: &mut dyn FnMut(&mut Self)) {
        let Some((first, rest)) = loops.split_first() else {
            inner(self);
            return;
        };
        self.each(&first.iter, &mut |b, element, ty| {
            b.bind(&first.target, element, &ty);
            b.conditions(&first.conditions, &mut |b| b.comprehension_loops(rest, inner));
        });
    }

    fn conditions(&mut self, conditions: &[ast::Expr], inner: &mut dyn FnMut(&mut Self)) {
        let Some((test, rest)) = conditions.split_first() else {
            inner(self);
            return;
        };
        let ok = self.condition(test);
        let then = self.block(|b| b.conditions(rest, inner));
        self.push(StmtKind::If(ok, then, Vec::new()));
    }

    // Expressions -----------------------------------------------------------------------

    /// Evaluate `e` for its effect; a counted value it gives is held, so that it is dropped.
    fn effect(&mut self, e: &ast::Expr) {
        if counted(&self.ty(e)) {
            self.value(e);
            return;
        }
        match self.expr(e) {
            Value::Done(_) => {}
            Value::Expr(expr) => self.push(StmtKind::Do(expr)),
        }
    }

    /// `e` as an operand: a constant, a local, or a temporary holding its value.
    fn value(&mut self, e: &ast::Expr) -> Operand {
        match self.expr(e) {
            Value::Done(operand) => operand,
            Value::Expr(expr) => {
                let ty = self.ty(e);
                let outer = self.at.replace(e.span);
                let held = if is_unit(&ty) || matches!(ty, Ty::Never) {
                    self.push(StmtKind::Do(expr));
                    Operand::Const(Const::Unit)
                } else {
                    self.hold(ty, expr)
                };
                self.at = outer;
                held
            }
        }
    }

    fn condition(&mut self, e: &ast::Expr) -> Operand {
        self.value(e)
    }

    fn expr(&mut self, e: &ast::Expr) -> Value {
        match &e.kind {
            ExprKind::Int(text) => {
                let kind = match self.ty(e) {
                    Ty::Int(kind) => kind,
                    _ => IntKind::I64,
                };
                Value::Done(Operand::Const(Const::Int(parse_int(text), kind)))
            }
            ExprKind::Float(text) => {
                let value: f64 = text.replace('_', "").parse().unwrap_or(f64::NAN);
                Value::Done(Operand::Const(Const::Float(value)))
            }
            ExprKind::Bool(v) => Value::Done(Operand::Const(Const::Bool(*v))),
            ExprKind::None => match self.ty(e) {
                ty @ Ty::Optional(_) => Value::Expr(Expr::OptNew { ty, value: None }),
                _ => Value::Done(Operand::Const(Const::Unit)),
            },
            ExprKind::Unit => Value::Done(Operand::Const(Const::Unit)),
            ExprKind::Str(literals) if literals.iter().any(|l| l.bytes) => {
                let bytes = literals
                    .iter()
                    .flat_map(|l| &l.parts)
                    .filter_map(
                        |p| if let StrPart::Text(t) = p { Some(t.chars().map(|c| c as u32 as u8)) } else { None },
                    )
                    .flatten()
                    .collect();
                Value::Done(Operand::Const(Const::Bytes(bytes)))
            }
            ExprKind::Str(literals) => {
                let parts: Vec<&StrPart> = literals.iter().flat_map(|l| &l.parts).collect();
                if parts.iter().all(|p| matches!(p, StrPart::Text(_))) {
                    let text: String =
                        parts.iter().map(|p| if let StrPart::Text(t) = p { t.as_str() } else { "" }).collect();
                    return Value::Done(Operand::Const(Const::Str(text)));
                }
                let parts = self.format_parts(&parts);
                Value::Expr(Expr::Format(parts))
            }
            ExprKind::Name(name) => match self.local_of(e.span) {
                Some(local) => {
                    let declared = self.function.locals[local].ty.clone();
                    let seen = self.ty(e);
                    match &declared {
                        Ty::Optional(inner) if **inner == seen => Value::Expr(Expr::OptValue(Operand::Local(local))),
                        _ => Value::Done(Operand::Local(local)),
                    }
                }
                None => match self.cx.variant_of.get(name).cloned() {
                    Some((_, variant)) => Value::Expr(Expr::UnitVariant { ty: self.ty(e), variant }),
                    None if self.cx.fns.contains_key(name.as_str()) => self.fn_ref(e, name),
                    None if self.cx.math_names.contains(name.as_str()) => self.math_constant(e, name),
                    None => Value::Done(self.unsupported(e.span, &format!("the value `{name}`"))),
                },
            },
            ExprKind::Attr { object, name } if matches!(self.ty(object), Ty::Module(_)) => {
                self.math_constant(e, &name.name)
            }
            ExprKind::Attr { object, name } => {
                let ty = self.ty(object);
                if let Ty::Adt(class, _) = &ty
                    && self.cx.checked.py_methods.contains_key(class)
                {
                    let target = self.value(object);
                    return Value::Expr(Expr::PyAttribute { object: target, name: name.name.clone(), ret: self.ty(e) });
                }
                match self.record_field(&ty, &name.name) {
                    Some((index, _)) => {
                        let o = self.value(object);
                        Value::Expr(Expr::Field { value: o, ty, variant: None, index })
                    }
                    None => Value::Done(self.unsupported(e.span, &format!("the field `{}` here", name.name))),
                }
            }
            ExprKind::Coalesce { value, default } => {
                let ty = self.ty(e);
                let v = self.value(value);
                let result = self.temp(ty.clone());
                let some = self.hold(Ty::Bool, Expr::OptIsSome(v.clone()));
                let inner_ty = ty.clone();
                let then = self.block(|b| {
                    let inner = b.hold(inner_ty.clone(), Expr::OptValue(v));
                    b.push(StmtKind::Let(result, Expr::Use(inner)));
                });
                let never = matches!(self.ty(default), Ty::Never);
                let otherwise = self.block(|b| {
                    let d = b.value(default);
                    if !never {
                        let d = b.coerce(d, &ty);
                        b.push(StmtKind::Let(result, Expr::Use(d)));
                    }
                });
                self.push(StmtKind::If(some, then, otherwise));
                Value::Done(Operand::Local(result))
            }
            ExprKind::Try(inner) => {
                let rty = self.ty(inner);
                let Ty::Result(_, error) = &rty else {
                    return Value::Done(self.unsupported(e.span, "`?` on this value"));
                };
                let error = (**error).clone();
                let r = self.value(inner);
                let ok = self.hold(Ty::Bool, Expr::ResultIsOk(r.clone()));
                let failed = self.block(|b| {
                    let err = b.hold(error, Expr::ResultError(r.clone()));
                    b.fail_with(err);
                });
                self.push(StmtKind::If(ok, Vec::new(), failed));
                Value::Expr(Expr::ResultValue(r))
            }
            ExprKind::Fail(inner) => {
                let v = self.value(inner);
                self.fail_with(v);
                Value::Done(Operand::Const(Const::Unit))
            }
            ExprKind::List(items) => {
                let elem = element(&self.ty(e));
                let mut values = Vec::new();
                for item in items {
                    let v = self.value(item);
                    values.push(self.coerce(v, &elem));
                }
                Value::Expr(Expr::ListNew { elem, items: values })
            }
            ExprKind::Tuple(items) => {
                let ty = self.ty(e);
                let types = if let Ty::Tuple(types) = &ty { types.clone() } else { Vec::new() };
                let mut values = Vec::new();
                for (i, item) in items.iter().enumerate() {
                    let v = self.value(item);
                    let v = match types.get(i) {
                        Some(t) => self.coerce(v, t),
                        None => v,
                    };
                    values.push(v);
                }
                Value::Expr(Expr::TupleNew { ty, items: values })
            }
            ExprKind::ListComp { element, loops } | ExprKind::Generator { element, loops } => {
                Value::Done(self.comprehension(e, element, loops))
            }
            ExprKind::Dict(pairs) => {
                let Ty::Dict(k, v) = self.ty(e) else { return Value::Done(self.unsupported(e.span, "this dict")) };
                let mut items = Vec::new();
                for (key, value) in pairs {
                    let key = self.value(key);
                    let key = self.coerce(key, &k);
                    let value = self.value(value);
                    let value = self.coerce(value, &v);
                    items.push((key, value));
                }
                Value::Expr(Expr::DictNew { key: *k, value: *v, items })
            }
            ExprKind::Set(items) => {
                let Ty::Set(t) = self.ty(e) else { return Value::Done(self.unsupported(e.span, "this set")) };
                let mut values = Vec::new();
                for item in items {
                    let v = self.value(item);
                    values.push(self.coerce(v, &t));
                }
                Value::Expr(Expr::SetNew { elem: *t, items: values })
            }
            ExprKind::DictComp { key, value, loops } => {
                let Ty::Dict(k, v) = self.ty(e) else { return Value::Done(self.unsupported(e.span, "this dict")) };
                let (k, v) = (*k, *v);
                let dict_ty = Ty::Dict(Box::new(k.clone()), Box::new(v.clone()));
                let d = self.temp(dict_ty);
                self.push(StmtKind::Let(d, Expr::DictNew { key: k.clone(), value: v.clone(), items: Vec::new() }));
                self.comprehension_loops(loops, &mut |b| {
                    let kv = b.value(key);
                    let kv = b.coerce(kv, &k);
                    let vv = b.value(value);
                    let vv = b.coerce(vv, &v);
                    b.push(StmtKind::Mutate {
                        op: Builtin::DictSet,
                        place: Place { local: d, proj: Vec::new() },
                        args: vec![Arg::Address(kv, k.clone()), Arg::Address(vv, v.clone())],
                        at: false,
                        result: None,
                    });
                });
                Value::Done(Operand::Local(d))
            }
            ExprKind::SetComp { element, loops } => {
                let Ty::Set(t) = self.ty(e) else { return Value::Done(self.unsupported(e.span, "this set")) };
                let t = *t;
                let s = self.temp(Ty::Set(Box::new(t.clone())));
                self.push(StmtKind::Let(s, Expr::SetNew { elem: t.clone(), items: Vec::new() }));
                self.comprehension_loops(loops, &mut |b| {
                    let v = b.value(element);
                    let v = b.coerce(v, &t);
                    b.push(StmtKind::Mutate {
                        op: Builtin::SetAdd,
                        place: Place { local: s, proj: Vec::new() },
                        args: vec![Arg::Address(v, t.clone())],
                        at: false,
                        result: None,
                    });
                });
                Value::Done(Operand::Local(s))
            }
            ExprKind::Binary { op, left, right } => {
                let ty = self.ty(left);
                let right_ty = self.ty(right);
                let a = self.value(left);
                let b = self.value(right);
                Value::Expr(self.binary(*op, a, b, &ty, &right_ty))
            }
            ExprKind::Unary { op, operand } => {
                let ty = self.ty(operand);
                if let (ast::UnaryOp::Neg, ExprKind::Int(text)) = (op, &operand.kind) {
                    // `-9223372036854775808` is a literal, not the negation of one out of range.
                    let kind = if let Ty::Int(kind) = ty { kind } else { IntKind::I64 };
                    return Value::Done(Operand::Const(Const::Int(-parse_int(text), kind)));
                }
                let a = self.value(operand);
                match op {
                    ast::UnaryOp::Pos => Value::Done(a),
                    ast::UnaryOp::Neg => Value::Expr(Expr::Unary(UnOp::Neg, a, ty)),
                    ast::UnaryOp::Invert => Value::Expr(Expr::Unary(UnOp::Invert, a, ty)),
                }
            }
            ExprKind::Not(operand) => {
                let a = self.value(operand);
                Value::Expr(Expr::Unary(UnOp::Not, a, Ty::Bool))
            }
            ExprKind::Compare { first, rest } => Value::Done(self.compare(first, rest)),
            ExprKind::Logical { op, operands } => Value::Done(self.logical(*op, operands)),
            ExprKind::IfExp { test, then, orelse } => {
                let ty = self.ty(e);
                let ok = self.condition(test);
                let t = self.temp(ty.clone());
                let a = self.block(|b| {
                    let v = b.value(then);
                    let v = b.coerce(v, &ty);
                    b.push(StmtKind::Let(t, Expr::Use(v)));
                });
                let c = self.block(|b| {
                    let v = b.value(orelse);
                    let v = b.coerce(v, &ty);
                    b.push(StmtKind::Let(t, Expr::Use(v)));
                });
                self.push(StmtKind::If(ok, a, c));
                Value::Done(Operand::Local(t))
            }
            ExprKind::Call { func, args } => self.call(e, func, args),
            ExprKind::Lambda { params, body } => self.lambda(e, params, body),
            ExprKind::Index { object, index } => {
                let ty = self.ty(object);
                match &ty {
                    Ty::Tuple(items) => {
                        let at = match &index.kind {
                            ExprKind::Int(text) => Some(parse_int(text)),
                            ExprKind::Unary { op: ast::UnaryOp::Neg, operand } => match &operand.kind {
                                ExprKind::Int(text) => Some(items.len() as i128 - parse_int(text)),
                                _ => None,
                            },
                            _ => None,
                        };
                        let Some(at) = at.filter(|&i| i >= 0 && (i as usize) < items.len()) else {
                            return Value::Done(self.unsupported(index.span, "a tuple index that is not a literal"));
                        };
                        let o = self.value(object);
                        Value::Expr(Expr::TupleGet { tuple: o, index: at as usize })
                    }
                    Ty::Str => {
                        let o = self.value(object);
                        let i = self.value(index);
                        Value::Expr(rt(Builtin::StrIndex, vec![o, i], true))
                    }
                    Ty::List(elem) => {
                        let elem = (**elem).clone();
                        let o = self.value(object);
                        let i = self.value(index);
                        Value::Expr(Expr::ListGet { list: o, index: i, elem, checked: true })
                    }
                    Ty::Dict(k, v) => {
                        let (k, v) = ((**k).clone(), (**v).clone());
                        let o = self.value(object);
                        let i = self.value(index);
                        let i = self.coerce(i, &k);
                        Value::Expr(Expr::RtValue {
                            op: Builtin::DictGet,
                            args: vec![Arg::Value(o), Arg::Address(i, k)],
                            at: true,
                            ty: v,
                        })
                    }
                    _ => Value::Done(self.unsupported(e.span, "indexing this value")),
                }
            }
            ExprKind::Slice { object, lower, upper, step } => {
                let ty = self.ty(object);
                let o = self.value(object);
                let mut args = vec![o];
                for bound in [lower, upper, step] {
                    match bound {
                        Some(b) => {
                            let v = self.value(b);
                            args.push(flag(true));
                            args.push(v);
                        }
                        None => {
                            args.push(flag(false));
                            args.push(int(0));
                        }
                    }
                }
                match ty {
                    Ty::Str => Value::Expr(rt(Builtin::StrSlice, args, true)),
                    Ty::List(_) => Value::Expr(rt(Builtin::ListSlice, args, true)),
                    _ => Value::Done(self.unsupported(e.span, "slicing this value")),
                }
            }
            _ => Value::Done(self.unsupported(e.span, "this expression")),
        }
    }

    /// `a op b` for operands of `left` and `right`: arithmetic, or strings and lists joined
    /// and repeated.
    fn binary(&mut self, op: ast::BinOp, a: Operand, b: Operand, left: &Ty, right: &Ty) -> Expr {
        match (op, left, right) {
            (ast::BinOp::Add, Ty::Str, _) => rt(Builtin::StrConcat, vec![a, b], false),
            (ast::BinOp::Mul, Ty::Str, _) => rt(Builtin::StrRepeat, vec![a, b], true),
            (ast::BinOp::Mul, _, Ty::Str) => rt(Builtin::StrRepeat, vec![b, a], true),
            (ast::BinOp::Add, Ty::List(_), _) => rt(Builtin::ListConcat, vec![a, b], false),
            (ast::BinOp::BitOr, Ty::Set(_), _) => rt(Builtin::SetUnion, vec![a, b], false),
            (ast::BinOp::BitAnd, Ty::Set(_), _) => rt(Builtin::SetIntersection, vec![a, b], false),
            (ast::BinOp::Sub, Ty::Set(_), _) => rt(Builtin::SetDifference, vec![a, b], false),
            (ast::BinOp::Mul, Ty::List(_), _) => rt(Builtin::ListRepeat, vec![a, b], true),
            (ast::BinOp::Mul, _, Ty::List(_)) => rt(Builtin::ListRepeat, vec![b, a], true),
            _ => Expr::Binary(binop(op), a, b, left.clone()),
        }
    }

    fn compare(&mut self, first: &ast::Expr, rest: &[(ast::CmpOp, ast::Expr)]) -> Operand {
        let result = self.temp(Ty::Bool);
        let mut left_ty = self.ty(first);
        let mut left = self.value(first);
        self.compare_rest(result, &mut left, &mut left_ty, rest);
        Operand::Local(result)
    }

    /// The comparisons of `rest` from `left` on, into `result`; the last right side and its type.
    fn compare_rest(
        &mut self,
        result: Local,
        left: &mut Operand,
        left_ty: &mut Ty,
        rest: &[(ast::CmpOp, ast::Expr)],
    ) -> (Operand, Ty) {
        let Some(((op, right), more)) = rest.split_first() else { return (left.clone(), left_ty.clone()) };
        let right_ty = self.ty(right);
        let right_value = self.value(right);
        let none_on_right = matches!(right.kind, ExprKind::None);
        let compared = match op {
            ast::CmpOp::Is | ast::CmpOp::IsNot | ast::CmpOp::Eq | ast::CmpOp::NotEq
                if none_on_right && matches!(left_ty, Ty::Optional(_)) =>
            {
                let some = self.hold(Ty::Bool, Expr::OptIsSome(left.clone()));
                if matches!(op, ast::CmpOp::IsNot | ast::CmpOp::NotEq) {
                    Expr::Use(some)
                } else {
                    Expr::Unary(UnOp::Not, some, Ty::Bool)
                }
            }
            ast::CmpOp::Is | ast::CmpOp::IsNot if none_on_right => Expr::Use(flag(*op == ast::CmpOp::IsNot)),
            ast::CmpOp::In | ast::CmpOp::NotIn => {
                let contains = match &right_ty {
                    Ty::Str | Ty::List(_) | Ty::Dict(..) | Ty::Set(_) => {
                        Expr::Contains { container: right_value.clone(), item: left.clone(), ty: right_ty.clone() }
                    }
                    Ty::Tuple(items) => {
                        // Equal to one of the tuple's items, compared in order until one is.
                        let found = self.temp(Ty::Bool);
                        self.push(StmtKind::Let(found, Expr::Use(flag(false))));
                        for (index, t) in items.iter().enumerate() {
                            let next = self.block(|b| {
                                let item = b.hold(t.clone(), Expr::TupleGet { tuple: right_value.clone(), index });
                                let equal = b.compare_values(CmpOp::Eq, left.clone(), left_ty, item, t);
                                b.push(StmtKind::Let(found, equal));
                            });
                            self.push(StmtKind::If(Operand::Local(found), Vec::new(), next));
                        }
                        Expr::Use(Operand::Local(found))
                    }
                    _ => {
                        self.unsupported(right.span, "`in` on this value");
                        Expr::Use(flag(false))
                    }
                };
                if *op == ast::CmpOp::In {
                    contains
                } else {
                    let t = self.hold(Ty::Bool, contains);
                    Expr::Unary(UnOp::Not, t, Ty::Bool)
                }
            }
            _ => {
                let op = match op {
                    ast::CmpOp::Lt => CmpOp::Lt,
                    ast::CmpOp::Le => CmpOp::Le,
                    ast::CmpOp::Gt => CmpOp::Gt,
                    ast::CmpOp::Ge => CmpOp::Ge,
                    ast::CmpOp::Eq => CmpOp::Eq,
                    ast::CmpOp::NotEq => CmpOp::Ne,
                    _ => {
                        self.unsupported(right.span, "this comparison");
                        CmpOp::Eq
                    }
                };
                self.compare_values(op, left.clone(), left_ty, right_value.clone(), &right_ty)
            }
        };
        self.push(StmtKind::Let(result, compared));
        if more.is_empty() {
            return (right_value, right_ty);
        }
        *left = right_value;
        *left_ty = right_ty.clone();
        let mut last = (left.clone(), right_ty);
        let next = self.block(|b| last = b.compare_rest(result, left, left_ty, more));
        self.push(StmtKind::If(Operand::Local(result), next, Vec::new()));
        last
    }

    /// `left op right`: an optional compared with a plain value compares with the value as an
    /// optional, as Python compares `None` or the value it holds.
    fn compare_values(&mut self, op: CmpOp, left: Operand, left_ty: &Ty, right: Operand, right_ty: &Ty) -> Expr {
        match (left_ty, right_ty) {
            (Ty::Optional(_), r) if !matches!(r, Ty::Optional(_) | Ty::Unit | Ty::Never | Ty::Error) => {
                let right = self.coerce(right, left_ty);
                Expr::Compare(op, left, right, left_ty.clone())
            }
            (l, Ty::Optional(_)) if !matches!(l, Ty::Optional(_) | Ty::Unit | Ty::Never | Ty::Error) => {
                let left = self.coerce(left, right_ty);
                Expr::Compare(op, left, right, right_ty.clone())
            }
            _ => Expr::Compare(op, left, right, left_ty.clone()),
        }
    }

    fn logical(&mut self, op: BoolOp, operands: &[ast::Expr]) -> Operand {
        let result = self.temp(Ty::Bool);
        self.logical_rest(result, op, operands);
        Operand::Local(result)
    }

    fn logical_rest(&mut self, result: Local, op: BoolOp, operands: &[ast::Expr]) {
        let Some((first, more)) = operands.split_first() else { return };
        let v = self.value(first);
        self.push(StmtKind::Let(result, Expr::Use(v)));
        if more.is_empty() {
            return;
        }
        let next = self.block(|b| b.logical_rest(result, op, more));
        let stmt = match op {
            BoolOp::And => StmtKind::If(Operand::Local(result), next, Vec::new()),
            BoolOp::Or => StmtKind::If(Operand::Local(result), Vec::new(), next),
        };
        self.push(stmt);
    }

    /// The parts of an f-string: text, and each value with its conversion and its spec.
    fn format_parts(&mut self, parts: &[&StrPart]) -> Vec<FormatPart> {
        let mut out = Vec::new();
        for part in parts {
            match part {
                StrPart::Text(t) => out.push(FormatPart::Text(t.clone())),
                StrPart::Expr { expr, conversion, spec } => {
                    let ty = self.ty(expr);
                    let value = self.value(expr);
                    let spec_parts: Vec<&StrPart> = spec.iter().collect();
                    let spec = self.format_parts(&spec_parts);
                    out.push(FormatPart::Value { value, ty, conversion: *conversion, spec });
                }
            }
        }
        out
    }

    // Calls -----------------------------------------------------------------------------

    fn call(&mut self, whole: &ast::Expr, func: &ast::Expr, args: &[AstArg]) -> Value {
        if let ExprKind::Attr { object, name } = &func.kind {
            return self.method(whole, object, &name.name, args);
        }
        let call_value = !matches!(&func.kind, ExprKind::Name(_)) || self.local_of(func.span).is_some();
        if call_value {
            let ty = self.ty(func);
            let Ty::Func(params, _) = &ty else {
                return Value::Done(self.unsupported(func.span, "a call of this value"));
            };
            let params = params.clone();
            let callee = self.value(func);
            let mut operands = Vec::new();
            for (a, p) in args.iter().zip(&params) {
                let v = self.value(a.expr());
                operands.push(self.coerce(v, p));
            }
            return Value::Expr(Expr::CallClosure { callee, args: operands, ty });
        }
        let ExprKind::Name(name) = &func.kind else {
            return Value::Done(self.unsupported(func.span, "a call of this value"));
        };
        if let Some(f) = self.cx.fns.get(name.as_str()).copied() {
            return self.call_function(whole, name, f, args);
        }
        if self.cx.math_names.contains(name.as_str()) {
            return self.math_call(whole, name, args);
        }
        if let Some(sig) = self.cx.c_imports.get(name.as_str()).cloned() {
            let mut operands = Vec::new();
            for (a, p) in args.iter().zip(&sig.params) {
                let v = self.value(a.expr());
                operands.push(self.coerce(v, &p.ty));
            }
            let params: Vec<Ty> = sig.params.iter().map(|p| p.ty.clone()).collect();
            self.cx.c_functions.insert(name.clone(), (params.clone(), sig.ret.clone()));
            return Value::Expr(Expr::CallC { symbol: name.clone(), args: operands, params, ret: sig.ret });
        }
        if let Some((module, sig)) = self.cx.py_imports.get(name.as_str()).cloned() {
            return self.python_call(whole.span, module, name, &sig, args);
        }
        if matches!(self.cx.checked.declared.get(name), Some(TypeDef::Record { .. })) {
            let ty = self.ty(whole);
            return self.construct(whole, ty, None, args);
        }
        if let Some((_, k)) = self.cx.variant_of.get(name).cloned() {
            let ty = self.ty(whole);
            return self.construct(whole, ty, Some(k), args);
        }
        if let ("Ok" | "Err", [AstArg::Positional(inner)]) = (name.as_str(), args) {
            let ty = self.ty(whole);
            let Ty::Result(ok_ty, err_ty) = &ty else {
                return Value::Done(self.unsupported(whole.span, "this result"));
            };
            let to = if name == "Ok" { (**ok_ty).clone() } else { (**err_ty).clone() };
            let v = self.value(inner);
            let v = self.coerce(v, &to);
            return Value::Expr(Expr::ResultNew { ty, ok: name == "Ok", value: v });
        }
        let positional: Vec<&ast::Expr> =
            args.iter().filter_map(|a| if let AstArg::Positional(e) = a { Some(e) } else { None }).collect();
        let keyword = |key: &str| {
            args.iter().find_map(|a| match a {
                AstArg::Keyword(k, e) if k.name == key => Some(e),
                _ => None,
            })
        };
        let result_ty = self.ty(whole);
        match (name.as_str(), positional.as_slice()) {
            ("print", _) => {
                let mut values = Vec::new();
                for e in &positional {
                    let ty = self.ty(e);
                    values.push((self.value(e), ty));
                }
                let mut sep = None;
                let mut end = None;
                for a in args {
                    if let AstArg::Keyword(key, e) = a {
                        let v = self.value(e);
                        match key.name.as_str() {
                            "sep" => sep = Some(v),
                            "end" => end = Some(v),
                            _ => return Value::Done(self.unsupported(key.span, "this keyword of `print`")),
                        }
                    }
                }
                Value::Expr(Expr::Print { args: values, sep, end })
            }
            ("todo", _) => {
                self.push(StmtKind::Panic(Panic::Todo));
                Value::Done(Operand::Const(Const::Unit))
            }
            ("abs", [x]) => {
                let ty = self.ty(x);
                let v = self.value(x);
                Value::Expr(Expr::Unary(UnOp::Abs, v, ty))
            }
            ("min" | "max", [single]) if keyword("key").is_some() => {
                let key = keyword("key").expect("a key");
                Value::Done(self.extreme_by(single, key, name == "max", &result_ty))
            }
            ("map", [f, x]) => {
                let list = self.materialize(x);
                let elem = element(&self.local_ty(&list));
                let out_elem = match element(&result_ty) {
                    Ty::Error | Ty::Var(_) => self.key_type(f, &elem),
                    known => known,
                };
                let out = self.new_list(&out_elem);
                self.indexed(&[(list, elem)], false, &mut |b, item, ty| {
                    let (v, _) = b.apply(f, vec![(item, ty)]);
                    let v = b.coerce(v, &out_elem);
                    b.push_element(out, v, &out_elem);
                });
                Value::Done(Operand::Local(out))
            }
            ("filter", [f, x]) => {
                let list = self.materialize(x);
                let elem = element(&self.local_ty(&list));
                let out = self.new_list(&elem);
                self.indexed(&[(list, elem.clone())], false, &mut |b, item, ty| {
                    let (keep, _) = b.apply(f, vec![(item.clone(), ty)]);
                    let then = b.block(|b| b.push_element(out, item, &elem));
                    b.push(StmtKind::If(keep, then, Vec::new()));
                });
                Value::Done(Operand::Local(out))
            }
            ("min" | "max", [single]) => {
                let elem = result_ty;
                let list = self.materialize(single);
                Value::Expr(Expr::RtValue {
                    op: Builtin::ListExtreme,
                    args: vec![Arg::Value(list), Arg::Value(flag(name == "max"))],
                    at: true,
                    ty: elem,
                })
            }
            ("min" | "max", [first, rest @ ..]) if self.ty(first).is_numeric() => {
                let ty = result_ty;
                let mut best = self.value(first);
                for e in rest {
                    let v = self.value(e);
                    best = self.hold(ty.clone(), Expr::MinMax { max: name == "max", a: best, b: v, ty: ty.clone() });
                }
                Value::Done(best)
            }
            ("min" | "max", many) => {
                let ty = result_ty;
                let mut items = Vec::new();
                for e in many {
                    items.push(self.value(e));
                }
                let list = self.hold(Ty::list(ty.clone()), Expr::ListNew { elem: ty.clone(), items });
                Value::Expr(Expr::RtValue {
                    op: Builtin::ListExtreme,
                    args: vec![Arg::Value(list), Arg::Value(flag(name == "max"))],
                    at: true,
                    ty,
                })
            }
            ("sum", [x, rest @ ..]) if rest.len() <= 1 => {
                let elem = result_ty;
                let list = self.materialize(x);
                let total = match &elem {
                    Ty::Float(_) => rt(Builtin::SumF64, vec![list], false),
                    Ty::Int(IntKind::U64) => rt(Builtin::SumU64, vec![list], true),
                    _ => rt(Builtin::SumI64, vec![list], true),
                };
                match rest.first() {
                    None => Value::Expr(total),
                    Some(start) => {
                        let start = self.value(start);
                        let total = self.hold(elem.clone(), total);
                        Value::Expr(Expr::Binary(BinOp::Add, start, total, elem))
                    }
                }
            }
            ("any" | "all", [x]) => {
                let any = name == "any";
                if let ExprKind::Generator { element, loops } = &x.kind
                    && loops.len() == 1
                {
                    let result = self.temp(Ty::Bool);
                    self.push(StmtKind::Let(result, Expr::Use(flag(!any))));
                    self.comprehension_loops(loops, &mut |b| {
                        let v = b.value(element);
                        let stop = b.block(|b| {
                            b.push(StmtKind::Let(result, Expr::Use(flag(any))));
                            b.push(StmtKind::Break);
                        });
                        if any {
                            b.push(StmtKind::If(v, stop, Vec::new()));
                        } else {
                            b.push(StmtKind::If(v, Vec::new(), stop));
                        }
                    });
                    return Value::Done(Operand::Local(result));
                }
                let list = self.materialize(x);
                Value::Expr(rt(if any { Builtin::ListAny } else { Builtin::ListAll }, vec![list], false))
            }
            ("sorted", [x]) => {
                let reverse = match keyword("reverse") {
                    Some(r) => self.value(r),
                    None => flag(false),
                };
                let list = self.materialize(x);
                let Some(key) = keyword("key") else {
                    return Value::Expr(rt(Builtin::ListSorted, vec![list, reverse], true));
                };
                let ty = self.local_ty(&list);
                let Operand::Local(copy) = self.hold(ty, rt(Builtin::ListCopy, vec![list], false)) else {
                    unreachable!("a held value")
                };
                let place = Place { local: copy, proj: Vec::new() };
                self.sort_by_key(place, key, reverse);
                Value::Done(Operand::Local(copy))
            }
            ("reversed", [x]) => {
                let list = self.materialize(x);
                Value::Expr(rt(Builtin::ListReversed, vec![list], false))
            }
            ("range", values) => {
                let values: Vec<Operand> = values.iter().map(|v| self.value(v)).collect();
                let (start, stop, step) = match values.as_slice() {
                    [stop] => (int(0), stop.clone(), int(1)),
                    [start, stop] => (start.clone(), stop.clone(), int(1)),
                    [start, stop, step] => (start.clone(), stop.clone(), step.clone()),
                    _ => return Value::Done(self.unsupported(whole.span, "this `range`")),
                };
                Value::Expr(rt(Builtin::RangeList, vec![start, stop, step], true))
            }
            ("list", []) => Value::Expr(Expr::ListNew { elem: element(&result_ty), items: Vec::new() }),
            ("set", []) => Value::Expr(Expr::SetNew { elem: element(&result_ty), items: Vec::new() }),
            ("set", [x]) => {
                if let Ty::Set(_) = self.ty(x) {
                    let v = self.value(x);
                    return Value::Expr(rt(Builtin::SetCopy, vec![v], false));
                }
                let elem = element(&result_ty);
                let list = match &x.kind {
                    ExprKind::Call { .. } | ExprKind::Generator { .. } => self.collect(x, &elem),
                    _ => self.materialize(x),
                };
                Value::Expr(rt_args(Builtin::SetFromList, vec![Arg::Desc(elem), Arg::Value(list)], false))
            }
            ("dict", []) => {
                let Ty::Dict(k, v) = result_ty else { return Value::Done(self.unsupported(whole.span, "this dict")) };
                Value::Expr(Expr::DictNew { key: *k, value: *v, items: Vec::new() })
            }
            ("dict", [x]) => {
                let Ty::Dict(k, v) = result_ty else { return Value::Done(self.unsupported(whole.span, "this dict")) };
                let pair = Ty::Tuple(vec![(*k).clone(), (*v).clone()]);
                let list = self.materialize(x);
                Value::Expr(rt_args(
                    Builtin::DictFromPairs,
                    vec![Arg::Desc(*k), Arg::Desc(*v), Arg::Value(list), Arg::Offset(pair)],
                    false,
                ))
            }
            ("list", [x]) => match &x.kind {
                ExprKind::Call { func, .. }
                    if ["range", "enumerate", "zip", "reversed"].iter().any(|n| self.is_prelude(func, n)) =>
                {
                    let elem = element(&result_ty);
                    Value::Done(self.collect(x, &elem))
                }
                ExprKind::Generator { .. } => {
                    let elem = element(&result_ty);
                    Value::Done(self.collect(x, &elem))
                }
                _ => {
                    let ty = self.ty(x);
                    let v = self.value(x);
                    match ty {
                        Ty::List(_) => Value::Expr(rt(Builtin::ListCopy, vec![v], false)),
                        _ => Value::Done(self.as_list(v, &ty)),
                    }
                }
            },
            ("enumerate" | "zip", _) => {
                let elem = element(&result_ty);
                Value::Done(self.collect(whole, &elem))
            }
            ("len", [x]) => {
                let ty = self.ty(x);
                if let Ty::Tuple(items) = &ty {
                    return Value::Done(int(items.len() as i128));
                }
                let v = self.value(x);
                Value::Expr(Expr::Len(v, ty))
            }
            ("str", [x]) => {
                let ty = self.ty(x);
                let v = self.value(x);
                Value::Expr(Expr::ToStr(v, ty))
            }
            ("ord", [x]) => {
                let v = self.value(x);
                Value::Expr(rt(Builtin::StrOrd, vec![v], true))
            }
            ("chr", [x]) => {
                let v = self.value(x);
                Value::Expr(rt(Builtin::StrChr, vec![v], true))
            }
            ("int" | "float", [x]) if self.ty(x) == Ty::Str => {
                let v = self.value(x);
                Value::Expr(rt(if name == "int" { Builtin::StrInt } else { Builtin::StrFloat }, vec![v], true))
            }
            ("int" | "float" | "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "f32", [x])
                if self.ty(x).is_numeric() || self.ty(x) == Ty::Bool =>
            {
                let from = self.ty(x);
                let v = self.value(x);
                Value::Expr(Expr::Convert(v, from, result_ty))
            }
            ("round", [x]) => {
                let ty = self.ty(x);
                let v = self.value(x);
                match ty {
                    Ty::Float(_) => Value::Expr(rt(Builtin::RoundI64, vec![v], true)),
                    // An integer rounds to itself.
                    _ => Value::Expr(Expr::Convert(v, ty, INT)),
                }
            }
            ("round", [x, digits]) => {
                let x = self.number(x, &F64);
                let digits = self.number(digits, &INT);
                Value::Expr(rt(Builtin::RoundF64, vec![x, digits], false))
            }
            ("bool", [x]) => {
                let ty = self.ty(x);
                let v = self.value(x);
                Value::Done(self.truthy(v, &ty))
            }
            ("hash", [x]) => {
                let ty = self.ty(x);
                let v = self.value(x);
                Value::Expr(rt_args(Builtin::HashValue, vec![Arg::Desc(ty.clone()), Arg::Address(v, ty)], true))
            }
            ("pow", [a, b]) => {
                let (left, right) = (self.ty(a), self.ty(b));
                let (a, b) = (self.value(a), self.value(b));
                Value::Expr(self.binary(ast::BinOp::Pow, a, b, &left, &right))
            }
            ("pow", [a, b, m]) if [a, b, m].iter().all(|e| matches!(self.ty(e), Ty::Int(_))) => {
                let (a, b, m) = (self.number(a, &INT), self.number(b, &INT), self.number(m, &INT));
                Value::Expr(rt(Builtin::PowMod, vec![a, b, m], true))
            }
            ("divmod", [a, b]) => {
                let (left, right) = (self.ty(a), self.ty(b));
                let (a, b) = (self.value(a), self.value(b));
                let quotient = self.binary(ast::BinOp::FloorDiv, a.clone(), b.clone(), &left, &right);
                let quotient = self.hold(left.clone(), quotient);
                let remainder = self.binary(ast::BinOp::Mod, a, b, &left, &right);
                let remainder = self.hold(left, remainder);
                Value::Expr(Expr::TupleNew { ty: result_ty, items: vec![quotient, remainder] })
            }
            ("wrapping_add" | "wrapping_sub" | "wrapping_mul" | "gcd", [a, b]) => {
                let (a, b) = (self.number(a, &INT), self.number(b, &INT));
                let (function, at) = match name.as_str() {
                    "wrapping_add" => (Builtin::WrappingAdd, false),
                    "wrapping_sub" => (Builtin::WrappingSub, false),
                    "wrapping_mul" => (Builtin::WrappingMul, false),
                    _ => (Builtin::Gcd, true),
                };
                Value::Expr(rt(function, vec![a, b], at))
            }
            ("isqrt", [x]) => {
                let x = self.number(x, &INT);
                Value::Expr(rt(Builtin::Isqrt, vec![x], true))
            }
            ("parallel", [tasks]) => {
                let tasks = self.value(tasks);
                Value::Expr(Expr::Parallel { tasks, result: element(&result_ty) })
            }
            ("Heap", []) => Value::Expr(Expr::ListNew { elem: element(&result_ty), items: Vec::new() }),
            ("Heap", [x]) => {
                let list = self.materialize(x);
                let heap = self.temp(result_ty);
                self.push(StmtKind::Let(heap, Expr::Use(list)));
                self.push(StmtKind::Mutate {
                    op: Builtin::Heapify,
                    place: Place { local: heap, proj: Vec::new() },
                    args: Vec::new(),
                    at: true,
                    result: None,
                });
                Value::Done(Operand::Local(heap))
            }
            _ => Value::Done(self.unsupported(whole.span, &format!("`{name}` here"))),
        }
    }

    /// The number `e` as a value of `to`: converted when it is another numeric type.
    fn number(&mut self, e: &ast::Expr, to: &Ty) -> Operand {
        let ty = self.ty(e);
        let v = self.value(e);
        if ty == *to { v } else { self.hold(to.clone(), Expr::Convert(v, ty, to.clone())) }
    }

    /// `bool(v)`: Python's truth of a value of `ty` — a number not zero, a collection not empty,
    /// an optional holding a true value, and anything else true.
    fn truthy(&mut self, v: Operand, ty: &Ty) -> Operand {
        match ty {
            Ty::Bool => v,
            Ty::Int(kind) => {
                let zero = Operand::Const(Const::Int(0, *kind));
                self.hold(Ty::Bool, Expr::Compare(CmpOp::Ne, v, zero, ty.clone()))
            }
            Ty::Float(_) => {
                let zero = Operand::Const(Const::Float(0.0));
                self.hold(Ty::Bool, Expr::Compare(CmpOp::Ne, v, zero, ty.clone()))
            }
            Ty::Str | Ty::List(_) | Ty::Dict(..) | Ty::Set(_) | Ty::Heap(_) => {
                let n = self.hold(INT, Expr::Len(v, ty.clone()));
                self.hold(Ty::Bool, Expr::Compare(CmpOp::Ne, n, int(0), INT))
            }
            Ty::Unit => flag(false),
            Ty::Tuple(items) => flag(!items.is_empty()),
            Ty::Optional(inner) => {
                let inner = (**inner).clone();
                let result = self.temp(Ty::Bool);
                let some = self.hold(Ty::Bool, Expr::OptIsSome(v.clone()));
                let then = self.block(|b| {
                    let x = b.hold(inner.clone(), Expr::OptValue(v));
                    let t = b.truthy(x, &inner);
                    b.push(StmtKind::Let(result, Expr::Use(t)));
                });
                let otherwise = self.block(|b| b.push(StmtKind::Let(result, Expr::Use(flag(false)))));
                self.push(StmtKind::If(some, then, otherwise));
                Operand::Local(result)
            }
            _ => flag(true),
        }
    }

    /// A constant of `math`: `pi`, `e`, `inf`.
    fn math_constant(&mut self, e: &ast::Expr, name: &str) -> Value {
        let value = match name {
            "pi" => std::f64::consts::PI,
            "e" => std::f64::consts::E,
            "inf" => f64::INFINITY,
            _ => return Value::Done(self.unsupported(e.span, &format!("`math.{name}` as a value"))),
        };
        Value::Done(Operand::Const(Const::Float(value)))
    }

    /// `math.name(args)`, or `name(args)` imported from `math`: a function of the runtime that
    /// stops where CPython's module raises.
    fn math_call(&mut self, whole: &ast::Expr, name: &str, args: &[AstArg]) -> Value {
        let mut exprs = Vec::new();
        for a in args {
            match a {
                AstArg::Positional(e) => exprs.push(e),
                _ => return Value::Done(self.unsupported(whole.span, "this argument of a `math` function")),
            }
        }
        let floats = |b: &mut Self| exprs.iter().map(|e| b.number(e, &F64)).collect::<Vec<_>>();
        let ints = |b: &mut Self| exprs.iter().map(|e| b.number(e, &INT)).collect::<Vec<_>>();
        let function = match (name, exprs.len()) {
            ("floor" | "ceil" | "trunc", 1) if !matches!(self.ty(exprs[0]), Ty::Float(_)) => {
                return Value::Done(self.number(exprs[0], &INT));
            }
            ("sqrt", 1) => Builtin::MathSqrt,
            ("exp", 1) => Builtin::MathExp,
            ("sin", 1) => Builtin::MathSin,
            ("cos", 1) => Builtin::MathCos,
            ("tan", 1) => Builtin::MathTan,
            ("atan", 1) => Builtin::MathAtan,
            ("fabs", 1) => Builtin::MathFabs,
            ("log", 1) => Builtin::MathLn,
            ("log2", 1) => Builtin::MathLog2,
            ("log10", 1) => Builtin::MathLog10,
            ("floor", 1) => Builtin::MathFloor,
            ("ceil", 1) => Builtin::MathCeil,
            ("trunc", 1) => Builtin::MathTrunc,
            ("pow", 2) => Builtin::MathPow,
            ("atan2", 2) => Builtin::MathAtan2,
            ("hypot", 2) => Builtin::MathHypot,
            ("gcd", 2) => Builtin::Gcd,
            ("isqrt", 1) => Builtin::Isqrt,
            ("factorial", 1) => Builtin::Factorial,
            ("comb", 2) => Builtin::Comb,
            ("perm", 2) => Builtin::Perm,
            _ => return Value::Done(self.unsupported(whole.span, &format!("`math.{name}` here"))),
        };
        let integral = matches!(name, "gcd" | "isqrt" | "factorial" | "comb" | "perm");
        let values = if integral { ints(self) } else { floats(self) };
        Value::Expr(rt(function, values, true))
    }

    /// `heap.name(args)`: `push`, `pop_min` and `peek` of a `Heap[T]`.
    fn heap_method(&mut self, whole: &ast::Expr, object: &ast::Expr, elem: &Ty, name: &str, args: &[AstArg]) -> Value {
        match (name, args) {
            ("push", [AstArg::Positional(x)]) => {
                let place = self.place_or_hold(object);
                let v = self.value(x);
                let v = self.coerce(v, elem);
                self.push(StmtKind::Mutate {
                    op: Builtin::HeapPush,
                    place,
                    args: vec![Arg::Address(v, elem.clone())],
                    at: true,
                    result: None,
                });
                Value::Done(Operand::Const(Const::Unit))
            }
            ("pop_min", []) => {
                let ty = self.ty(whole);
                let place = self.place_or_hold(object);
                let found = self.temp(elem.clone());
                let ok = self.temp(Ty::Bool);
                self.push(StmtKind::Mutate {
                    op: Builtin::HeapPop,
                    place,
                    args: vec![Arg::Out(found, elem.clone())],
                    at: true,
                    result: Some(ok),
                });
                Value::Expr(Expr::OptIf { ty, cond: Operand::Local(ok), value: Operand::Local(found) })
            }
            ("peek", []) => {
                let heap = self.value(object);
                let ty = self.ty(whole);
                self.optional_from(&ty, Builtin::HeapPeek, vec![Arg::Value(heap)], false)
            }
            _ => Value::Done(self.unsupported(whole.span, &format!("the method `{name}` of a heap"))),
        }
    }

    /// A lambda as a closure: its body lowered as a function of its own, taking the closure
    /// and its parameters, which reads what it captured from the closure; the closure holds
    /// copies of the outer locals the body uses, taken now.
    fn lambda(&mut self, whole: &ast::Expr, params: &[ast::Ident], body: &ast::Expr) -> Value {
        let ty = self.ty(whole);
        let Ty::Func(param_types, ret) = &ty else {
            return Value::Done(self.unsupported(whole.span, "this lambda"));
        };
        let (param_types, ret) = (param_types.clone(), (**ret).clone());
        let mut names = Vec::new();
        names_in(body, &mut names);
        let mut captures: Vec<(Span, String, Local)> = Vec::new();
        for (name, span) in names {
            if let Some(decl) = self.cx.resolved.get(&span).copied()
                && let Some(&outer) = self.vars.get(&decl)
                && !captures.iter().any(|(d, ..)| *d == decl)
            {
                captures.push((decl, name, outer));
            }
        }
        let index = self.cx.lambdas.len();
        self.cx.lambdas.push(None);
        let capture_types: Vec<Ty> = captures.iter().map(|(_, _, l)| self.function.locals[*l].ty.clone()).collect();
        let span = self.span;
        let mut b = Builder {
            cx: &mut *self.cx,
            function: Function {
                name: symbol::lambda(index),
                source_name: "<lambda>".to_string(),
                type_params: self.function.type_params.clone(),
                params: Vec::new(),
                ret: ret.clone(),
                locals: Vec::new(),
                body: Vec::new(),
                span,
            },
            vars: HashMap::new(),
            blocks: vec![Vec::new()],
            span,
            subst: self.subst.clone(),
            bounds: self.bounds.clone(),
            at: None,
        };
        let closure = b.new_local(ty.clone(), Some("self"));
        b.function.params.push(closure);
        for (i, ((decl, name, _), t)) in captures.iter().zip(&capture_types).enumerate() {
            let local = b.declare(*decl, name, t.clone());
            b.push(StmtKind::Let(local, Expr::Capture { closure: Operand::Local(closure), lambda: index, index: i }));
        }
        for (p, t) in params.iter().zip(&param_types) {
            let local = b.declare(p.span, &p.name, t.clone());
            b.function.params.push(local);
        }
        let v = b.value(body);
        let v = b.coerce(v, &ret);
        let returned = if is_unit(&ret) { None } else { Some(v) };
        b.push(StmtKind::Return(returned));
        let mut function = b.function;
        function.body = b.blocks.pop().unwrap_or_default();
        self.cx.lambdas[index] = Some((function, capture_types, ty.clone()));
        let operands = captures.iter().map(|(_, _, l)| Operand::Local(*l)).collect();
        Value::Expr(Expr::Closure { lambda: index, ty, captures: operands })
    }

    /// `func(args)` where `func` is what a prelude function was given: a lambda applied in place,
    /// a function of the module, a prelude function or a closure. The result and its type.
    fn apply(&mut self, func: &ast::Expr, args: Vec<(Operand, Ty)>) -> (Operand, Ty) {
        match &func.kind {
            ExprKind::Lambda { params, body } => {
                for (p, (a, t)) in params.iter().zip(args) {
                    let local = self.declare(p.span, &p.name, t);
                    self.push(StmtKind::Let(local, Expr::Use(a)));
                }
                let ty = self.ty(body);
                (self.value(body), ty)
            }
            ExprKind::Name(name) if self.local_of(func.span).is_none() => {
                if let Some(sig) = self.cx.checked.functions.get(name).cloned() {
                    let names: Vec<String> = sig.type_params.iter().map(|(n, _)| n.clone()).collect();
                    let mut map = HashMap::new();
                    for ((_, t), p) in args.iter().zip(&sig.params) {
                        bind(&p.ty, t, &names, &mut map);
                    }
                    let Some(subst) = self.subst_for(names, &map) else {
                        return (self.unsupported(func.span, "this generic function here"), Ty::Error);
                    };
                    let generic = !subst.names.is_empty();
                    let sig = subst.sig(&sig);
                    let operands: Vec<Operand> =
                        args.into_iter().zip(&sig.params).map(|((a, _), p)| self.coerce(a, &p.ty)).collect();
                    let ty = sig.ret.clone();
                    let call = if generic {
                        let callee = Callee::Function { name: name.to_string(), type_args: subst.types };
                        Expr::CallGeneric { callee, args: operands.into_iter().map(Arg::Value).collect() }
                    } else {
                        Expr::Call(symbol::function(name), operands)
                    };
                    return (self.hold(ty.clone(), call), ty);
                }
                let Some((a, t)) = args.into_iter().next() else {
                    return (self.unsupported(func.span, "this function"), Ty::Error);
                };
                let (expr, ty) = match name.as_str() {
                    "len" => (Expr::Len(a, t), INT),
                    "str" => (Expr::ToStr(a, t), Ty::Str),
                    "int" if t == Ty::Str => (rt(Builtin::StrInt, vec![a], true), INT),
                    "float" if t == Ty::Str => {
                        (rt(Builtin::StrFloat, vec![a], true), Ty::Float(lotml_check::ty::FloatKind::F64))
                    }
                    "int" => (Expr::Convert(a, t, INT), INT),
                    "float" => (
                        Expr::Convert(a, t, Ty::Float(lotml_check::ty::FloatKind::F64)),
                        Ty::Float(lotml_check::ty::FloatKind::F64),
                    ),
                    "abs" => (Expr::Unary(UnOp::Abs, a, t.clone()), t),
                    "sum" => {
                        let elem = element(&t);
                        let list = self.as_list(a, &t);
                        let total = match &elem {
                            Ty::Float(_) => rt(Builtin::SumF64, vec![list], false),
                            Ty::Int(IntKind::U64) => rt(Builtin::SumU64, vec![list], true),
                            _ => rt(Builtin::SumI64, vec![list], true),
                        };
                        (total, elem)
                    }
                    "sorted" => {
                        let elem = element(&t);
                        let list = self.as_list(a, &t);
                        (rt(Builtin::ListSorted, vec![list, flag(false)], true), Ty::list(elem))
                    }
                    "ord" => (rt(Builtin::StrOrd, vec![a], true), INT),
                    "chr" => (rt(Builtin::StrChr, vec![a], true), Ty::Str),
                    _ => return (self.unsupported(func.span, &format!("`{name}` as a value here")), Ty::Error),
                };
                (self.hold(ty.clone(), expr), ty)
            }
            _ => {
                let ty = self.ty(func);
                let Ty::Func(params, ret) = &ty else {
                    return (self.unsupported(func.span, "calling this value"), Ty::Error);
                };
                let (params, ret) = (params.clone(), (**ret).clone());
                let callee = self.value(func);
                let operands = args.into_iter().zip(&params).map(|((a, _), p)| self.coerce(a, p)).collect();
                (self.hold(ret.clone(), Expr::CallClosure { callee, args: operands, ty }), ret)
            }
        }
    }

    /// Sort the list at `place` by the key `key` gives each element, computed once each, in a
    /// stable order: Python's `sort(key=…)`.
    fn sort_by_key(&mut self, place: Place, key: &ast::Expr, reverse: Operand) {
        let list = self.read_place(&place);
        let elem = element(&self.local_ty(&list));
        let key_ty = self.key_type(key, &elem);
        let keys = self.new_list(&key_ty);
        let target = key_ty.clone();
        self.indexed(&[(list, elem)], false, &mut |b, item, ty| {
            let (k, _) = b.apply(key, vec![(item, ty)]);
            let k = b.coerce(k, &target);
            b.push_element(keys, k, &target);
        });
        self.push(StmtKind::Mutate {
            op: Builtin::ListSortByKeys,
            place,
            args: vec![Arg::Value(Operand::Local(keys)), Arg::Value(reverse)],
            at: true,
            result: None,
        });
    }

    /// The type of what `func` gives for an argument of `arg`.
    fn key_type(&self, func: &ast::Expr, arg: &Ty) -> Ty {
        match &func.kind {
            ExprKind::Lambda { body, .. } => self.ty(body),
            ExprKind::Name(name) if self.local_of(func.span).is_none() => {
                if let Some(sig) = self.cx.checked.functions.get(name) {
                    let names: Vec<String> = sig.type_params.iter().map(|(n, _)| n.clone()).collect();
                    let mut map = HashMap::new();
                    if let Some(p) = sig.params.first() {
                        bind(&p.ty, arg, &names, &mut map);
                    }
                    return match self.subst_for(names, &map) {
                        Some(subst) => subst.apply(sig.ret.clone()),
                        None => Ty::Error,
                    };
                }
                match name.as_str() {
                    "len" | "ord" | "int" => INT,
                    "sum" => element(arg),
                    "sorted" => Ty::list(element(arg)),
                    "str" | "chr" => Ty::Str,
                    "float" => Ty::Float(lotml_check::ty::FloatKind::F64),
                    _ => arg.clone(),
                }
            }
            _ => match self.ty(func) {
                Ty::Func(_, ret) => *ret,
                _ => Ty::Error,
            },
        }
    }

    /// `min(x, key=f)` and `max(x, key=f)`: the first element whose key is smallest (largest).
    fn extreme_by(&mut self, x: &ast::Expr, key: &ast::Expr, max: bool, ty: &Ty) -> Operand {
        let list = self.materialize(x);
        let elem = element(&self.local_ty(&list));
        let key_ty = self.key_type(key, &elem);
        let n = self.hold(INT, Expr::Len(list.clone(), Ty::list(elem.clone())));
        let empty = self.hold(Ty::Bool, Expr::Compare(CmpOp::Eq, n.clone(), int(0), INT));
        let message = if max { "max() iterable argument is empty" } else { "min() iterable argument is empty" };
        let stop = self.block(|b| b.push(StmtKind::Panic(Panic::Value(message.to_string()))));
        self.push(StmtKind::If(empty, stop, Vec::new()));
        let best = self.temp(ty.clone());
        let first = self
            .hold(elem.clone(), Expr::ListGet { list: list.clone(), index: int(0), elem: elem.clone(), checked: true });
        self.push(StmtKind::Let(best, Expr::Use(first.clone())));
        let (k0, _) = self.apply(key, vec![(first, elem.clone())]);
        let best_key = self.temp(key_ty.clone());
        self.push(StmtKind::Let(best_key, Expr::Use(k0)));
        let i = self.temp(INT);
        self.push(StmtKind::Let(i, Expr::Use(int(1))));
        let saved = self.span;
        let body = self.block(|b| {
            let more = b.hold(Ty::Bool, Expr::Compare(CmpOp::Lt, Operand::Local(i), n.clone(), INT));
            let done = b.block(|b| b.push(StmtKind::Break));
            b.push(StmtKind::If(more, Vec::new(), done));
            let item = b.hold(
                elem.clone(),
                Expr::ListGet { list: list.clone(), index: Operand::Local(i), elem: elem.clone(), checked: true },
            );
            b.push(StmtKind::Let(i, Expr::Binary(BinOp::Add, Operand::Local(i), int(1), INT)));
            let (k, _) = b.apply(key, vec![(item.clone(), elem.clone())]);
            let op = if max { CmpOp::Gt } else { CmpOp::Lt };
            let better = b.hold(Ty::Bool, Expr::Compare(op, k.clone(), Operand::Local(best_key), key_ty.clone()));
            let then = b.block(|b| {
                b.push(StmtKind::Let(best, Expr::Use(item)));
                b.push(StmtKind::Let(best_key, Expr::Use(k)));
            });
            b.push(StmtKind::If(better, then, Vec::new()));
        });
        self.span = saved;
        self.push(StmtKind::Loop(body));
        Operand::Local(best)
    }

    /// A call of `function` of the Python module `module` through its interface's `sig` (R1.3).
    fn python_call(&mut self, call: Span, module: String, function: &str, sig: &FnSig, args: &[AstArg]) -> Value {
        // An overloaded function is called through the overload the checker gave the call (adr:0035).
        let sig = &self.cx.checked.py_overload(call, sig).clone();
        let (operands, keywords) = self.python_args(&sig.params, args);
        let params = sig.params.iter().map(|p| p.ty.clone()).collect();
        let ret = call_ret(sig);
        let function = function.to_string();
        Value::Expr(Expr::CallPython { module, function, args: operands, keywords, params, ret })
    }

    /// The arguments of a call into Python, each converted to its parameter of `params`: those
    /// given by position, then those given by keyword, with their names, which Python is passed
    /// by name, since a parameter left to its default may come between.
    fn python_args(&mut self, params: &[ParamSig], args: &[AstArg]) -> (Vec<Operand>, Vec<String>) {
        let mut operands = Vec::new();
        let mut named = Vec::new();
        for (i, a) in args.iter().enumerate() {
            let (param, keyword) = match a {
                AstArg::Keyword(k, _) => (params.iter().find(|p| p.name == k.name), Some(k.name.clone())),
                _ => (params.get(i), None),
            };
            let v = match param {
                Some(p) => {
                    let v = self.value(a.expr());
                    self.coerce(v, &p.ty)
                }
                None => self.materialize(a.expr()),
            };
            match keyword {
                Some(name) => named.push((name, v)),
                None => operands.push(v),
            }
        }
        let keywords = named.iter().map(|(name, _)| name.clone()).collect();
        operands.extend(named.into_iter().map(|(_, v)| v));
        (operands, keywords)
    }

    /// `object.method(args)` of a value of a Python class through the method's `sig`, `self` its
    /// first parameter (adr:0034).
    fn python_method(&mut self, call: Span, object: &ast::Expr, method: &str, sig: &FnSig, args: &[AstArg]) -> Value {
        let sig = &self.cx.checked.py_overload(call, sig).clone();
        let target = self.value(object);
        let (operands, keywords) = self.python_args(sig.params.get(1..).unwrap_or_default(), args);
        let params = sig.params.iter().skip(1).map(|p| p.ty.clone()).collect();
        let ret = call_ret(sig);
        let method = method.to_string();
        Value::Expr(Expr::CallPyMethod { object: target, method, args: operands, keywords, params, ret })
    }

    /// `object.name(args)`: a method of a declared type, or of a built-in one.
    fn method(&mut self, whole: &ast::Expr, object: &ast::Expr, name: &str, args: &[AstArg]) -> Value {
        // `date.today()`: a static method of an imported Python class, called through its module
        // (adr:0034). The checker gave the class's name no type, as it gives one to a local.
        if let ExprKind::Name(class) = &object.kind
            && !self.cx.checked.types.contains_key(&object.span)
            && let Some(qualified) = self.cx.checked.py_classes.get(class).cloned()
            && let Some(m) = self.cx.checked.py_method(&qualified, name).cloned()
            && let Some((module, class)) = qualified.rsplit_once('.')
        {
            return self.python_call(whole.span, module.to_string(), &format!("{class}.{name}"), &m.sig, args);
        }
        let ty = self.ty(object);
        if let Ty::Adt(class, _) = &ty
            && let Some(m) = self.cx.checked.py_method(class, name).cloned()
        {
            return self.python_method(whole.span, object, name, &m.sig, args);
        }
        let owner = match &ty {
            Ty::Adt(owner, _) | Ty::TypeName(owner) => Some(owner.clone()),
            Ty::PyObject => {
                // `o.value()`, the one method a `PyObject` has (specs/python-object R2.1).
                let Ty::Result(target, _) = self.ty(whole) else {
                    return Value::Done(self.unsupported(whole.span, "this conversion"));
                };
                let value = self.materialize(object);
                return Value::Expr(Expr::PyValue { value, ty: *target });
            }
            Ty::Dyn(trait_name) => return self.dyn_method(whole, object, trait_name, name, args),
            Ty::Module(module) => {
                if let Some(sig) = self.cx.checked.foreign.get(module).and_then(|fs| fs.get(name)).cloned() {
                    return self.python_call(whole.span, module.clone(), name, &sig, args);
                }
                // `py.m.C(...)`: a class's constructor, its own or a base's (adr:0034).
                if let Some(sig) = self.cx.checked.py_constructor(&format!("{module}.{name}")) {
                    return self.python_call(whole.span, module.clone(), name, &sig, args);
                }
                return self.math_call(whole, name, args);
            }
            Ty::Heap(elem) => return self.heap_method(whole, object, elem, name, args),
            Ty::Param(param) => return self.param_method(whole, object, &param.clone(), name, args),
            _ => None,
        };
        if let Some(owner) = owner
            && let Some(value) = self.user_method(whole, object, &owner, name, args)
        {
            return value;
        }
        let mut keywords = Vec::new();
        let mut positional = Vec::new();
        for a in args {
            match a {
                AstArg::Positional(e) => positional.push(e),
                AstArg::Keyword(k, e) => keywords.push((k.name.as_str(), e)),
                AstArg::Inout(e, _) => return Value::Done(self.unsupported(e.span, "an `inout` argument")),
            }
        }
        match &ty {
            Ty::Str => self.str_method(whole, object, name, &positional, &keywords),
            Ty::List(elem) => {
                let elem = (**elem).clone();
                self.list_method(whole, object, name, &elem, &positional, &keywords)
            }
            Ty::Dict(k, v) => {
                let (k, v) = ((**k).clone(), (**v).clone());
                self.dict_method(whole, object, name, &k, &v, &positional)
            }
            Ty::Set(t) => {
                let t = (**t).clone();
                self.set_method(whole, object, name, &t, &positional)
            }
            _ => Value::Done(self.unsupported(whole.span, &format!("the method `{name}` here"))),
        }
    }

    /// `object.name(args, keywords)`, a built-in method the IR has no operation for, on the value
    /// at `object`'s place.
    fn builtin_method(
        &mut self,
        object: &ast::Expr,
        name: &str,
        positional: &[&ast::Expr],
        keywords: &[(&str, &ast::Expr)],
    ) -> Value {
        let place = self.place_or_hold(object);
        let ty = self.ty(object);
        let args = positional.iter().map(|e| self.value(e)).collect();
        let keywords = keywords.iter().map(|(k, e)| (k.to_string(), self.value(e))).collect();
        Value::Expr(Expr::Method { place, ty, method: name.to_string(), args, keywords })
    }

    /// `receiver.name(args)`, a built-in method the IR has no operation for, on a value read.
    fn method_of_value(&mut self, receiver: Operand, ty: Ty, name: &str, args: Vec<Operand>) -> Value {
        let Operand::Local(local) = self.hold(ty.clone(), Expr::Use(receiver)) else { unreachable!("a held value") };
        let place = Place { local, proj: Vec::new() };
        Value::Expr(Expr::Method { place, ty, method: name.to_string(), args, keywords: Vec::new() })
    }

    /// The place of a container a method changes: the place it is, or a temporary holding it.
    fn place_or_hold(&mut self, object: &ast::Expr) -> Place {
        match self.place(object) {
            Some(place) => place,
            None => {
                let v = self.value(object);
                let ty = self.ty(object);
                let Operand::Local(local) = self.hold(ty, Expr::Use(v)) else { unreachable!("a held value") };
                Place { local, proj: Vec::new() }
            }
        }
    }

    fn dict_method(
        &mut self,
        whole: &ast::Expr,
        object: &ast::Expr,
        name: &str,
        k: &Ty,
        v: &Ty,
        positional: &[&ast::Expr],
    ) -> Value {
        let ty = self.ty(whole);
        match (name, positional) {
            ("setdefault", [_, _]) => {
                let Some(place) = self.place(whole) else {
                    return Value::Done(self.unsupported(whole.span, "`setdefault` on this value"));
                };
                Value::Expr(Expr::ReadPlace(place))
            }
            ("pop", [key, rest @ ..]) if rest.len() <= 1 => {
                let place = self.place_or_hold(object);
                let key = self.value(key);
                let key = self.coerce(key, k);
                let found = self.temp(v.clone());
                let ok = self.temp(Ty::Bool);
                self.push(StmtKind::Mutate {
                    op: Builtin::DictPop,
                    place,
                    args: vec![Arg::Address(key, k.clone()), Arg::Out(found, v.clone())],
                    at: false,
                    result: Some(ok),
                });
                match rest.first() {
                    None => Value::Expr(Expr::OptIf { ty, cond: Operand::Local(ok), value: Operand::Local(found) }),
                    Some(default) => {
                        let result = self.temp(v.clone());
                        let then = self.block(|b| b.push(StmtKind::Let(result, Expr::Use(Operand::Local(found)))));
                        let otherwise = self.block(|b| {
                            let d = b.value(default);
                            let d = b.coerce(d, v);
                            b.push(StmtKind::Let(result, Expr::Use(d)));
                        });
                        self.push(StmtKind::If(Operand::Local(ok), then, otherwise));
                        Value::Done(Operand::Local(result))
                    }
                }
            }
            ("clear", []) => {
                let place = self.place_or_hold(object);
                self.push(StmtKind::Mutate {
                    op: Builtin::DictClear,
                    place,
                    args: Vec::new(),
                    at: false,
                    result: None,
                });
                Value::Done(Operand::Const(Const::Unit))
            }
            _ => {
                let d = self.value(object);
                let mut values = Vec::new();
                for e in positional {
                    values.push(self.value(e));
                }
                match (name, values.as_slice()) {
                    ("get", [key]) => {
                        let key = self.coerce(key.clone(), k);
                        self.optional_from(
                            &ty,
                            Builtin::DictGetOptional,
                            vec![Arg::Value(d), Arg::Address(key, k.clone())],
                            false,
                        )
                    }
                    ("get", [key, default]) => {
                        let key = self.coerce(key.clone(), k);
                        let default = self.coerce(default.clone(), v);
                        Value::Expr(Expr::RtValue {
                            op: Builtin::DictGetOr,
                            args: vec![Arg::Value(d), Arg::Address(key, k.clone()), Arg::Address(default, v.clone())],
                            at: false,
                            ty: v.clone(),
                        })
                    }
                    ("keys", []) => Value::Expr(rt(Builtin::DictKeys, vec![d], false)),
                    ("values", []) => Value::Expr(rt(Builtin::DictValues, vec![d], false)),
                    ("items", []) => {
                        let pair = Ty::Tuple(vec![k.clone(), v.clone()]);
                        Value::Expr(rt_args(
                            Builtin::DictItems,
                            vec![Arg::Value(d), Arg::Desc(pair.clone()), Arg::Offset(pair)],
                            false,
                        ))
                    }
                    ("contains", [key]) => {
                        let key = self.coerce(key.clone(), k);
                        Value::Expr(rt_args(
                            Builtin::DictContains,
                            vec![Arg::Value(d), Arg::Address(key, k.clone())],
                            false,
                        ))
                    }
                    ("copy", []) => Value::Expr(rt(Builtin::DictCopy, vec![d], false)),
                    _ => {
                        let place = self.place_or_hold(object);
                        let ty = self.ty(object);
                        Value::Expr(Expr::Method {
                            place,
                            ty,
                            method: name.to_string(),
                            args: values,
                            keywords: Vec::new(),
                        })
                    }
                }
            }
        }
    }

    fn set_method(
        &mut self,
        whole: &ast::Expr,
        object: &ast::Expr,
        name: &str,
        t: &Ty,
        positional: &[&ast::Expr],
    ) -> Value {
        let ty = self.ty(whole);
        if matches!(name, "add" | "remove" | "discard" | "pop") {
            let place = self.place_or_hold(object);
            let mut values = Vec::new();
            for e in positional {
                let v = self.value(e);
                values.push(self.coerce(v, t));
            }
            let mutate = |op: Builtin, args: Vec<Arg>, at: bool| StmtKind::Mutate {
                op,
                place: place.clone(),
                args,
                at,
                result: None,
            };
            let stmt = match (name, values.as_slice()) {
                ("add", [x]) => mutate(Builtin::SetAdd, vec![Arg::Address(x.clone(), t.clone())], false),
                ("remove", [x]) => mutate(Builtin::SetRemove, vec![Arg::Address(x.clone(), t.clone())], true),
                ("discard", [x]) => mutate(Builtin::SetDiscard, vec![Arg::Address(x.clone(), t.clone())], false),
                ("pop", []) => {
                    let found = self.temp(t.clone());
                    let ok = self.temp(Ty::Bool);
                    self.push(StmtKind::Mutate {
                        op: Builtin::SetPop,
                        place: place.clone(),
                        args: vec![Arg::Out(found, t.clone())],
                        at: false,
                        result: Some(ok),
                    });
                    return Value::Expr(Expr::OptIf { ty, cond: Operand::Local(ok), value: Operand::Local(found) });
                }
                _ => {
                    let ty = self.ty(object);
                    return Value::Expr(Expr::Method {
                        place,
                        ty,
                        method: name.to_string(),
                        args: values,
                        keywords: Vec::new(),
                    });
                }
            };
            self.push(stmt);
            return Value::Done(Operand::Const(Const::Unit));
        }
        let s = self.value(object);
        let mut values = Vec::new();
        for e in positional {
            values.push(self.value(e));
        }
        match (name, values.as_slice()) {
            ("contains", [x]) => {
                let x = self.coerce(x.clone(), t);
                Value::Expr(rt_args(Builtin::SetContains, vec![Arg::Value(s), Arg::Address(x, t.clone())], false))
            }
            ("issubset", [other]) => Value::Expr(rt(Builtin::SetIssubset, vec![s, other.clone()], false)),
            ("issuperset", [other]) => Value::Expr(rt(Builtin::SetIssubset, vec![other.clone(), s], false)),
            ("union", [other]) => Value::Expr(rt(Builtin::SetUnion, vec![s, other.clone()], false)),
            ("intersection", [other]) => Value::Expr(rt(Builtin::SetIntersection, vec![s, other.clone()], false)),
            ("difference", [other]) => Value::Expr(rt(Builtin::SetDifference, vec![s, other.clone()], false)),
            ("copy", []) => Value::Expr(rt(Builtin::SetCopy, vec![s], false)),
            _ => {
                let place = self.place_or_hold(object);
                let ty = self.ty(object);
                Value::Expr(Expr::Method { place, ty, method: name.to_string(), args: values, keywords: Vec::new() })
            }
        }
    }

    fn str_method(
        &mut self,
        whole: &ast::Expr,
        object: &ast::Expr,
        name: &str,
        positional: &[&ast::Expr],
        keywords: &[(&str, &ast::Expr)],
    ) -> Value {
        if name == "split" && !keywords.is_empty() {
            // `sep` and `maxsplit` by name too, as Python's `split` takes them, each left out
            // taking its default; Python's other `str` methods take their arguments by position.
            let mut sep = positional.first().copied();
            let mut maxsplit = positional.get(1).copied();
            for (key, e) in keywords {
                match *key {
                    "sep" if sep.is_none() => sep = Some(e),
                    "maxsplit" if maxsplit.is_none() => maxsplit = Some(e),
                    _ => return Value::Done(self.unsupported(whole.span, "this keyword argument of `split`")),
                }
            }
            let receiver = self.value(object);
            let sep = match sep {
                Some(e) => self.value(e),
                None => Operand::Const(Const::Null),
            };
            let maxsplit = match maxsplit {
                Some(e) => self.value(e),
                None => int(-1),
            };
            return Value::Expr(rt(Builtin::StrSplit, vec![receiver, sep, maxsplit], true));
        }
        if !keywords.is_empty() {
            return self.builtin_method(object, name, positional, keywords);
        }
        let receiver = self.value(object);
        let mut values = Vec::new();
        for e in positional {
            values.push(self.value(e));
        }
        let null = Operand::Const(Const::Null);
        let mut all = vec![receiver.clone()];
        let call = match (name, values.as_slice()) {
            (
                "lower" | "upper" | "swapcase" | "title" | "capitalize" | "isalpha" | "isdigit" | "isspace" | "isalnum"
                | "isupper" | "islower",
                [],
            ) => {
                let rt_name = match name {
                    "lower" => Builtin::StrLower,
                    "upper" => Builtin::StrUpper,
                    "swapcase" => Builtin::StrSwapcase,
                    "title" => Builtin::StrTitle,
                    "capitalize" => Builtin::StrCapitalize,
                    "isalpha" => Builtin::StrIsalpha,
                    "isdigit" => Builtin::StrIsdigit,
                    "isspace" => Builtin::StrIsspace,
                    "isalnum" => Builtin::StrIsalnum,
                    "isupper" => Builtin::StrIsupper,
                    _ => Builtin::StrIslower,
                };
                rt(rt_name, all, false)
            }
            ("strip" | "lstrip" | "rstrip", rest @ ([] | [_])) => {
                all.push(rest.first().cloned().unwrap_or(null));
                all.push(flag(name != "rstrip"));
                all.push(flag(name != "lstrip"));
                rt(Builtin::StrStrip, all, false)
            }
            ("startswith" | "endswith" | "count", [x]) => {
                all.push(x.clone());
                let rt_name = match name {
                    "startswith" => Builtin::StrStartswith,
                    "endswith" => Builtin::StrEndswith,
                    _ => Builtin::StrCount,
                };
                rt(rt_name, all, false)
            }
            ("replace", [old, with, rest @ ..]) if rest.len() <= 1 => {
                all.push(old.clone());
                all.push(with.clone());
                all.push(rest.first().cloned().unwrap_or(int(-1)));
                rt(Builtin::StrReplace, all, false)
            }
            ("zfill", [width]) => {
                all.push(width.clone());
                rt(Builtin::StrZfill, all, false)
            }
            ("center" | "ljust" | "rjust", [width, rest @ ..]) if rest.len() <= 1 => {
                all.push(width.clone());
                all.push(rest.first().cloned().unwrap_or(null));
                let align = match name {
                    "center" => '^',
                    "ljust" => '<',
                    _ => '>',
                };
                all.push(Operand::Const(Const::Char(align)));
                rt(Builtin::StrPad, all, true)
            }
            ("split", rest) if rest.len() <= 2 => {
                all.push(rest.first().cloned().unwrap_or(null));
                all.push(rest.get(1).cloned().unwrap_or(int(-1)));
                rt(Builtin::StrSplit, all, true)
            }
            ("splitlines", []) => rt(Builtin::StrSplitlines, all, false),
            ("join", [parts]) => {
                all.push(parts.clone());
                rt(Builtin::StrJoin, all, false)
            }
            ("to_int", []) => {
                return self.optional_from(&self.ty(whole), Builtin::StrToInt, vec![Arg::Value(receiver)], true);
            }
            ("to_float", []) => {
                return self.optional_from(&self.ty(whole), Builtin::StrToFloat, vec![Arg::Value(receiver)], false);
            }
            ("find" | "rfind", [sub]) => {
                let args = vec![Arg::Value(receiver), Arg::Value(sub.clone()), Arg::Value(flag(name == "rfind"))];
                return self.optional_from(&self.ty(whole), Builtin::StrFind, args, false);
            }
            ("split_once", [sep]) => {
                let (head, tail) = (self.temp(Ty::Str), self.temp(Ty::Str));
                let args = vec![
                    Arg::Value(receiver),
                    Arg::Value(sep.clone()),
                    Arg::Out(head, Ty::Str),
                    Arg::Out(tail, Ty::Str),
                ];
                let ok = self.hold(Ty::Bool, Expr::Rt { op: Builtin::StrSplitOnce, args, at: true });
                let pair_ty = Ty::Tuple(vec![Ty::Str, Ty::Str]);
                let pair = self.hold(
                    pair_ty.clone(),
                    Expr::TupleNew { ty: pair_ty, items: vec![Operand::Local(head), Operand::Local(tail)] },
                );
                return Value::Expr(Expr::OptIf { ty: self.ty(whole), cond: ok, value: pair });
            }
            ("partition", [sep]) => {
                let mut parts = Vec::new();
                for which in 0..3 {
                    let part = self.hold(
                        Ty::Str,
                        rt(Builtin::StrPartitionPart, vec![receiver.clone(), sep.clone(), int(which)], true),
                    );
                    parts.push(part);
                }
                return Value::Expr(Expr::TupleNew { ty: self.ty(whole), items: parts });
            }
            _ => return self.method_of_value(receiver, Ty::Str, name, values),
        };
        Value::Expr(call)
    }

    fn list_method(
        &mut self,
        whole: &ast::Expr,
        object: &ast::Expr,
        name: &str,
        elem: &Ty,
        positional: &[&ast::Expr],
        keywords: &[(&str, &ast::Expr)],
    ) -> Value {
        let mutating = matches!(name, "append" | "extend" | "insert" | "remove" | "clear" | "sort" | "reverse" | "pop");
        if !mutating {
            let list = self.value(object);
            let mut values = Vec::new();
            if name != "find" {
                for e in positional {
                    values.push(self.value(e));
                }
            }
            return match (name, values.as_slice()) {
                ("count" | "contains", [x]) => Value::Expr(rt_args(
                    if name == "count" { Builtin::ListCount } else { Builtin::ListContains },
                    vec![Arg::Value(list), Arg::Address(x.clone(), elem.clone())],
                    false,
                )),
                ("copy", []) => Value::Expr(rt(Builtin::ListCopy, vec![list], false)),
                ("index", [x]) => {
                    let args = vec![Arg::Value(list), Arg::Address(x.clone(), elem.clone())];
                    self.optional_from(&self.ty(whole), Builtin::ListIndex, args, false)
                }
                ("last", []) => self.optional_from(&self.ty(whole), Builtin::ListLast, vec![Arg::Value(list)], false),
                ("find", []) if positional.len() == 1 => {
                    let ty = self.ty(whole);
                    let result = self.temp(ty.clone());
                    self.push(StmtKind::Let(result, Expr::OptNew { ty: ty.clone(), value: None }));
                    let pred = positional[0];
                    self.indexed(&[(list, elem.clone())], false, &mut |b, item, item_ty| {
                        let (hit, _) = b.apply(pred, vec![(item.clone(), item_ty)]);
                        let then = b.block(|b| {
                            b.push(StmtKind::Let(result, Expr::OptNew { ty: ty.clone(), value: Some(item) }));
                            b.push(StmtKind::Break);
                        });
                        b.push(StmtKind::If(hit, then, Vec::new()));
                    });
                    Value::Done(Operand::Local(result))
                }
                _ => {
                    let ty = Ty::list(elem.clone());
                    self.method_of_value(list, ty, name, values)
                }
            };
        }
        let place = match self.place(object) {
            Some(place) => place,
            None => {
                let v = self.value(object);
                let ty = self.ty(object);
                let Operand::Local(local) = self.hold(ty, Expr::Use(v)) else { unreachable!("a held value") };
                Place { local, proj: Vec::new() }
            }
        };
        let mut values = Vec::new();
        for e in positional {
            values.push(self.value(e));
        }
        let mutate = |op: Builtin, args: Vec<Arg>, at: bool| StmtKind::Mutate {
            op,
            place: place.clone(),
            args,
            at,
            result: None,
        };
        let stmt = match (name, values.as_slice()) {
            ("append", [x]) => mutate(Builtin::ListPush, vec![Arg::Address(x.clone(), elem.clone())], false),
            ("extend", [other]) => mutate(Builtin::ListExtend, vec![Arg::Value(other.clone())], false),
            ("insert", [i, x]) => {
                mutate(Builtin::ListInsert, vec![Arg::Value(i.clone()), Arg::Address(x.clone(), elem.clone())], false)
            }
            ("remove", [x]) => mutate(Builtin::ListRemove, vec![Arg::Address(x.clone(), elem.clone())], true),
            ("clear", []) => mutate(Builtin::ListClear, Vec::new(), false),
            ("pop", rest @ ([] | [_])) => {
                let ty = self.ty(whole);
                let found = self.temp(elem.clone());
                let ok = self.temp(Ty::Bool);
                let (has, index) = match rest.first() {
                    Some(i) => (flag(true), i.clone()),
                    None => (flag(false), int(0)),
                };
                self.push(StmtKind::Mutate {
                    op: Builtin::ListPop,
                    place: place.clone(),
                    args: vec![Arg::Value(has), Arg::Value(index), Arg::Out(found, elem.clone())],
                    at: true,
                    result: Some(ok),
                });
                return Value::Expr(Expr::OptIf { ty, cond: Operand::Local(ok), value: Operand::Local(found) });
            }
            ("reverse", []) => mutate(Builtin::ListReverse, Vec::new(), false),
            ("sort", []) => {
                let reverse = match keywords.iter().find(|(k, _)| *k == "reverse") {
                    Some((_, r)) => self.value(r),
                    None => flag(false),
                };
                if let Some((_, key)) = keywords.iter().find(|(k, _)| *k == "key") {
                    self.sort_by_key(place.clone(), key, reverse);
                    return Value::Done(Operand::Const(Const::Unit));
                }
                mutate(Builtin::ListSort, vec![Arg::Value(reverse)], true)
            }
            _ => {
                let ty = self.ty(object);
                let keywords = keywords.iter().map(|(k, e)| (k.to_string(), self.value(e))).collect();
                return Value::Expr(Expr::Method { place, ty, method: name.to_string(), args: values, keywords });
            }
        };
        self.push(stmt);
        Value::Done(Operand::Const(Const::Unit))
    }

    fn call_function(&mut self, whole: &ast::Expr, name: &str, f: &'a FnDef, args: &[AstArg]) -> Value {
        let Some(sig) = self.cx.checked.functions.get(name).cloned() else {
            return Value::Done(self.unsupported(f.name.span, "this function"));
        };
        let names: Vec<String> = sig.type_params.iter().map(|(n, _)| n.clone()).collect();
        let mut map = HashMap::new();
        if !names.is_empty() {
            self.bind_args(&f.params, &sig.params, args, &names, &mut map);
            bind(&call_ret(&sig), &self.ty(whole), &names, &mut map);
        }
        let Some(subst) = self.subst_for(names, &map) else {
            return Value::Done(self.unsupported(whole.span, "this call of a generic function"));
        };
        let generic = !subst.names.is_empty();
        let sig = subst.sig(&sig);
        let call = self.call_with(symbol::function(name), &f.params, &sig.params, None, args);
        if !generic {
            return call;
        }
        generic_call(call, Callee::Function { name: name.to_string(), type_args: subst.types })
    }

    /// The type arguments the arguments of a call give its type parameters `names`.
    fn bind_args(
        &self,
        params: &[ast::Param],
        sigs: &[ParamSig],
        args: &[AstArg],
        names: &[String],
        map: &mut HashMap<String, Ty>,
    ) {
        for (slot, sig) in arg_slots(params, args).into_iter().zip(sigs) {
            if let Some(a) = slot {
                bind(&sig.ty, &self.ty(a.expr()), names, map);
            }
        }
    }

    /// The instance `map` makes of the type parameters `names`, if it gives each a type.
    fn subst_for(&self, names: Vec<String>, map: &HashMap<String, Ty>) -> Option<Subst> {
        let types = names.iter().map(|n| map.get(n).cloned()).collect::<Option<Vec<Ty>>>()?;
        Some(Subst { names, types })
    }

    /// The module's function `name` as a value: an instance of a generic one for the function
    /// type it is used as.
    fn fn_ref(&mut self, e: &ast::Expr, name: &str) -> Value {
        let (Some(_), Some(sig)) = (self.cx.fns.get(name), self.cx.checked.functions.get(name).cloned()) else {
            return Value::Done(self.unsupported(e.span, &format!("the value `{name}`")));
        };
        let names: Vec<String> = sig.type_params.iter().map(|(n, _)| n.clone()).collect();
        let mut map = HashMap::new();
        let pattern = Ty::Func(sig.params.iter().map(|p| p.ty.clone()).collect(), Box::new(call_ret(&sig)));
        bind(&pattern, &self.ty(e), &names, &mut map);
        let Some(subst) = self.subst_for(names, &map) else {
            return Value::Done(self.unsupported(e.span, "this generic function as a value"));
        };
        if !subst.names.is_empty() {
            return Value::Expr(Expr::FnRefGeneric { name: name.to_string(), type_args: subst.types });
        }
        let c_name = symbol::function(name);
        self.cx.fn_refs.insert(c_name.clone());
        Value::Expr(Expr::FnRef(c_name))
    }

    /// A call of the C function `c_name`, whose lotml parameters are `params` (`sigs` their
    /// types and conventions): `receiver` first when there is one, then the arguments by
    /// position and by name, the missing ones given their defaults, an `inout` one as its slot.
    fn call_with(
        &mut self,
        c_name: String,
        params: &[ast::Param],
        sigs: &[lotml_check::ParamSig],
        receiver: Option<Arg>,
        args: &[AstArg],
    ) -> Value {
        let slots = arg_slots(params, args);
        let mut all: Vec<Arg> = receiver.into_iter().collect();
        for (i, (slot, param)) in slots.iter().zip(params).enumerate() {
            let ty = sigs.get(i).map(|s| s.ty.clone()).unwrap_or(Ty::Error);
            match slot {
                Some(AstArg::Inout(e, _)) => match self.place(e) {
                    Some(place) => all.push(Arg::Slot(place)),
                    None => return Value::Done(self.unsupported(e.span, "this `inout` argument")),
                },
                Some(a) => {
                    let v = self.value(a.expr());
                    all.push(Arg::Value(self.coerce(v, &ty)));
                }
                None => match &param.default {
                    Some(e) => {
                        let v = self.value(e);
                        all.push(Arg::Value(self.coerce(v, &ty)));
                    }
                    None => return Value::Done(Operand::Const(Const::Unit)),
                },
            }
        }
        if all.iter().any(|a| matches!(a, Arg::Slot(_))) {
            return Value::Expr(Expr::CallSlots(c_name, all));
        }
        let operands = all.into_iter().filter_map(|a| if let Arg::Value(o) = a { Some(o) } else { None }).collect();
        Value::Expr(Expr::Call(c_name, operands))
    }

    /// `object.name(args)` where `object` is a value or the type of a declared record or sum
    /// type with that method: the instance for the receiver's type arguments and the method's.
    fn user_method(
        &mut self,
        whole: &ast::Expr,
        object: &ast::Expr,
        owner: &str,
        name: &str,
        args: &[AstArg],
    ) -> Option<Value> {
        let method = self.cx.checked.methods.get(owner)?.get(name)?.clone();
        let (f, _) = *self.cx.methods.get(&(owner.to_string(), name.to_string()))?;
        let mut names = method.owner_params.clone();
        names.extend(method.sig.type_params.iter().map(|(n, _)| n.clone()));
        let mut map = HashMap::new();
        if let Ty::Adt(_, owner_args) = self.ty(object) {
            for (p, a) in method.owner_params.iter().zip(owner_args) {
                map.insert(p.clone(), a);
            }
        }
        let skip = usize::from(method.receiver.is_some());
        self.bind_args(&f.params[skip..], &method.sig.params[skip..], args, &names, &mut map);
        bind(&call_ret(&method.sig), &self.ty(whole), &names, &mut map);
        let Some(subst) = self.subst_for(names, &map) else {
            return Some(Value::Done(self.unsupported(whole.span, "this call of a generic method")));
        };
        let (owner_args, own) = subst.types.split_at(method.owner_params.len());
        let owner_ty = Ty::Adt(owner.to_string(), owner_args.to_vec());
        let mut subst = subst.clone();
        if self.cx.methods.get(&(owner.to_string(), name.to_string())).is_some_and(|(_, default)| *default) {
            subst.names.push("Self".to_string());
            subst.types.push(owner_ty.clone());
        }
        let sig = subst.sig(&method.sig);
        let generic = !method_params(&method).is_empty();
        let callee = Callee::Method { owner: owner_ty, method: name.to_string(), own: own.to_vec() };
        let c_name = symbol::method(owner, name);
        let receiver = match method.receiver {
            None => None,
            Some(ast::Convention::Inout) => match self.place(object) {
                Some(place) => Some(Arg::Slot(place)),
                None => return Some(Value::Done(self.unsupported(object.span, "this receiver"))),
            },
            Some(_) => Some(Arg::Value(self.value(object))),
        };
        let call = self.call_with(c_name, &f.params[skip..], &sig.params[skip..], receiver, args);
        Some(if generic { generic_call(call, callee) } else { call })
    }

    /// `object.name(args)` where `object` is of the type parameter `param`, bound by a trait
    /// declaring the method: a call of the method of whichever type `param` stands for.
    fn param_method(
        &mut self,
        whole: &ast::Expr,
        object: &ast::Expr,
        param: &str,
        name: &str,
        args: &[AstArg],
    ) -> Value {
        let found = self.bounds.get(param).and_then(|trait_name| {
            let method = self.cx.checked.traits.get(trait_name)?.get(name)?.clone();
            let def = *self.cx.trait_fns.get(&(trait_name.clone(), name.to_string()))?;
            Some((method, def))
        });
        let Some((method, def)) = found else {
            return Value::Done(self.unsupported(whole.span, &format!("the method `{name}` here")));
        };
        let subst = Subst { names: vec!["Self".to_string()], types: vec![Ty::Param(param.to_string())] };
        let sig = subst.sig(&method.sig);
        let skip = usize::from(method.receiver.is_some());
        let receiver = match method.receiver {
            None => None,
            Some(ast::Convention::Inout) => match self.place(object) {
                Some(place) => Some(Arg::Slot(place)),
                None => return Value::Done(self.unsupported(object.span, "this receiver")),
            },
            Some(_) => Some(Arg::Value(self.value(object))),
        };
        let call = self.call_with(String::new(), &def.params[skip..], &sig.params[skip..], receiver, args);
        let callee = Callee::Method { owner: Ty::Param(param.to_string()), method: name.to_string(), own: Vec::new() };
        generic_call(call, callee)
    }

    /// `object.name(args)` where `object` is a `dyn trait_name`: a call through its table.
    fn dyn_method(
        &mut self,
        whole: &ast::Expr,
        object: &ast::Expr,
        trait_name: &str,
        name: &str,
        args: &[AstArg],
    ) -> Value {
        let found = self.cx.checked.traits.get(trait_name).and_then(|methods| {
            let slot = methods.keys().position(|k| k == name)?;
            Some((slot, methods[name].clone()))
        });
        let def = self.cx.trait_fns.get(&(trait_name.to_string(), name.to_string())).copied();
        let (Some((slot, method)), Some(def)) = (found, def) else {
            return Value::Done(self.unsupported(whole.span, "this method of a `dyn` value"));
        };
        if !dyn_callable(&method) {
            return Value::Done(self.unsupported(whole.span, "this call through `dyn`"));
        }
        let ty = self.ty(object);
        let receiver = self.value(object);
        let sigs = &method.sig.params[1..];
        match self.call_with(String::new(), &def.params[1..], sigs, None, args) {
            Value::Expr(Expr::Call(_, args)) => Value::Expr(Expr::CallDyn {
                receiver,
                ty,
                slot,
                args,
                params: sigs.iter().map(|p| p.ty.clone()).collect(),
                ret: call_ret(&method.sig),
            }),
            Value::Expr(_) => Value::Done(self.unsupported(whole.span, "an `inout` argument through `dyn`")),
            done => done,
        }
    }
}

enum Value {
    /// The value is ready as an operand.
    Done(Operand),
    /// The value is an expression still to be stored or evaluated.
    Expr(Expr),
}

fn is_unit(ty: &Ty) -> bool {
    matches!(ty, Ty::Unit)
}

fn binop(op: ast::BinOp) -> BinOp {
    match op {
        ast::BinOp::Add => BinOp::Add,
        ast::BinOp::Sub => BinOp::Sub,
        ast::BinOp::Mul => BinOp::Mul,
        ast::BinOp::Div => BinOp::TrueDiv,
        ast::BinOp::FloorDiv => BinOp::FloorDiv,
        ast::BinOp::Mod => BinOp::Mod,
        ast::BinOp::Pow => BinOp::Pow,
        ast::BinOp::LShift => BinOp::Shl,
        ast::BinOp::RShift => BinOp::Shr,
        ast::BinOp::BitOr => BinOp::BitOr,
        ast::BinOp::BitXor => BinOp::BitXor,
        ast::BinOp::BitAnd => BinOp::BitAnd,
    }
}

/// Every name an expression reads, with its span, lambdas' bodies included.
fn names_in(e: &ast::Expr, out: &mut Vec<(String, Span)>) {
    let mut each = |x: &ast::Expr| names_in(x, out);
    match &e.kind {
        ExprKind::Name(n) => out.push((n.clone(), e.span)),
        ExprKind::Tuple(items) | ExprKind::List(items) | ExprKind::Set(items) => items.iter().for_each(each),
        ExprKind::Dict(pairs) => {
            for (k, v) in pairs {
                names_in(k, out);
                names_in(v, out);
            }
        }
        ExprKind::ListComp { element, loops }
        | ExprKind::SetComp { element, loops }
        | ExprKind::Generator { element, loops } => {
            names_in(element, out);
            for l in loops {
                names_in(&l.iter, out);
                l.conditions.iter().for_each(|c| names_in(c, out));
            }
        }
        ExprKind::DictComp { key, value, loops } => {
            names_in(key, out);
            names_in(value, out);
            for l in loops {
                names_in(&l.iter, out);
                l.conditions.iter().for_each(|c| names_in(c, out));
            }
        }
        ExprKind::Unary { operand, .. } | ExprKind::Not(operand) | ExprKind::Try(operand) | ExprKind::Fail(operand) => {
            each(operand)
        }
        ExprKind::Binary { left, right, .. } => {
            names_in(left, out);
            names_in(right, out);
        }
        ExprKind::Compare { first, rest } => {
            names_in(first, out);
            rest.iter().for_each(|(_, x)| names_in(x, out));
        }
        ExprKind::Logical { operands, .. } => operands.iter().for_each(each),
        ExprKind::Coalesce { value, default } => {
            names_in(value, out);
            names_in(default, out);
        }
        ExprKind::IfExp { test, then, orelse } => {
            names_in(test, out);
            names_in(then, out);
            names_in(orelse, out);
        }
        ExprKind::Lambda { body, .. } => names_in(body, out),
        ExprKind::Call { func, args } => {
            names_in(func, out);
            args.iter().for_each(|a| names_in(a.expr(), out));
        }
        ExprKind::Index { object, index } => {
            names_in(object, out);
            names_in(index, out);
        }
        ExprKind::Slice { object, lower, upper, step } => {
            names_in(object, out);
            for b in [lower, upper, step].into_iter().flatten() {
                names_in(b, out);
            }
        }
        ExprKind::Attr { object, .. } => names_in(object, out),
        ExprKind::Str(literals) => {
            for l in literals {
                parts_names(&l.parts, out);
            }
        }
        _ => {}
    }
}

fn parts_names(parts: &[StrPart], out: &mut Vec<(String, Span)>) {
    for p in parts {
        if let StrPart::Expr { expr, spec, .. } = p {
            names_in(expr, out);
            parts_names(spec, out);
        }
    }
}

/// An integer literal's value: decimal, `0x`, `0o` or `0b`, with `_` between digits.
fn parse_int(text: &str) -> i128 {
    let clean = text.replace('_', "").to_ascii_lowercase();
    let (digits, radix) = match clean.get(..2) {
        Some("0x") => (&clean[2..], 16),
        Some("0o") => (&clean[2..], 8),
        Some("0b") => (&clean[2..], 2),
        _ => (clean.as_str(), 10),
    };
    i128::from_str_radix(digits, radix).unwrap_or(0)
}
