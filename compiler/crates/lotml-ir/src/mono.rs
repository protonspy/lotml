//! Monomorphization (specs/python-on-ir R4.2): the program `lower` writes, its generic functions
//! and methods generic, made into the one a native backend compiles — an instance of each generic
//! function per set of type arguments a call gives it, each lambda of a generic function per
//! instance of that function, each table a `dyn` value calls through. A generic function whose
//! recursion asks for instances without end is refused (specs/shared-ir R3.3).

use std::collections::{BTreeSet, HashMap};

use lotml_check::ty::Ty;
use lotml_diag::Diagnostic;
use lotml_syntax::span::Span;

use crate::ir::{Arg, Block, Callee, Expr, Function, Operand, StmtKind};
use crate::lower::{Lowered, VSlot, VTable};
use crate::symbol;

/// The most instances one generic function or method is given.
const INSTANCE_LIMIT: usize = 64;

/// The most nodes the type arguments of one instance may have between them.
const INSTANCE_TYPE_LIMIT: usize = 256;

/// Whether `ty` has more than `*budget` nodes, spending the budget on those it counts: it stops at
/// the first node past it, so a type too large to walk is never walked.
fn exceeds(ty: &Ty, budget: &mut usize) -> bool {
    if *budget == 0 {
        return true;
    }
    *budget -= 1;
    match ty {
        Ty::List(t) | Ty::Set(t) | Ty::Heap(t) | Ty::Optional(t) => exceeds(t, budget),
        Ty::Dict(a, b) | Ty::Result(a, b) => exceeds(a, budget) || exceeds(b, budget),
        Ty::Tuple(items) | Ty::Adt(_, items) => items.iter().any(|t| exceeds(t, budget)),
        Ty::Func(params, ret) => params.iter().any(|t| exceeds(t, budget)) || exceeds(ret, budget),
        _ => false,
    }
}

/// Type parameters and the types an instance gives them.
#[derive(Clone, Default)]
struct Subst {
    names: Vec<String>,
    types: Vec<Ty>,
}

impl Subst {
    fn apply(&self, ty: &mut Ty) {
        if !self.names.is_empty() {
            *ty = ty.substitute(&self.names, &self.types);
        }
    }
}

/// A function waiting to have its generic calls resolved: an original of the program or a new
/// instance, with the type arguments its lambdas take.
struct Work {
    function: Function,
    subst: Subst,
}

struct Mono {
    /// Each generic function and lambda of the program, by its name.
    generic: HashMap<String, Function>,
    /// Each function's parameters after the first, and result, by name: what a table's slot calls.
    signatures: HashMap<String, (Vec<Ty>, Ty)>,
    /// What each lambda of the program captures, and its type, by its index.
    lambdas_in: Vec<(Vec<Ty>, Ty)>,
    /// The lambdas of the monomorphic program, in order.
    lambdas: Vec<(Vec<Ty>, Ty)>,
    /// The index each lambda of no generic function keeps in the monomorphic program.
    kept: HashMap<usize, usize>,
    /// Each instance made, by the generic function's name and the type arguments.
    instances: HashMap<(String, Vec<Ty>), String>,
    /// Each lambda instance made, by the lambda's index and its function's type arguments.
    lambda_instances: HashMap<(usize, Vec<Ty>), usize>,
    /// How many instances each generic function has been given.
    counts: HashMap<String, usize>,
    work: Vec<Work>,
    dyn_methods: std::collections::BTreeMap<String, Vec<Option<String>>>,
    vtables: Vec<VTable>,
    vtable_ids: HashMap<(String, Ty), usize>,
    fn_refs: BTreeSet<String>,
    diagnostics: Vec<Diagnostic>,
}

/// The monomorphic program of `lowered`, or why it cannot be one.
pub fn mono(mut lowered: Lowered) -> Result<Lowered, Vec<Diagnostic>> {
    let lambda_names: HashMap<String, usize> = (0..lowered.lambdas.len()).map(|k| (symbol::lambda(k), k)).collect();
    let mut m = Mono {
        generic: HashMap::new(),
        signatures: HashMap::new(),
        lambdas_in: std::mem::take(&mut lowered.lambdas),
        lambdas: Vec::new(),
        kept: HashMap::new(),
        instances: HashMap::new(),
        lambda_instances: HashMap::new(),
        counts: HashMap::new(),
        work: Vec::new(),
        dyn_methods: std::mem::take(&mut lowered.dyn_methods),
        vtables: Vec::new(),
        vtable_ids: HashMap::new(),
        fn_refs: std::mem::take(&mut lowered.fn_refs),
        diagnostics: Vec::new(),
    };
    let mut originals = Vec::new();
    for mut f in std::mem::take(&mut lowered.functions) {
        if !f.type_params.is_empty() {
            m.generic.insert(f.name.clone(), f);
            continue;
        }
        if let Some(&k) = lambda_names.get(&f.name) {
            let index = m.lambdas.len();
            m.lambdas.push(m.lambdas_in[k].clone());
            m.kept.insert(k, index);
            f.name = symbol::lambda(index);
        }
        m.signatures.insert(f.name.clone(), signature(&f));
        originals.push(f);
    }
    let mut done = Vec::new();
    for f in originals {
        m.work.push(Work { function: f, subst: Subst::default() });
        while let Some(mut work) = m.work.pop() {
            m.block(&mut work.function.body, &work.subst);
            done.push(work.function);
        }
    }
    if !m.diagnostics.is_empty() {
        return Err(m.diagnostics);
    }
    lowered.functions = done;
    lowered.lambdas = m.lambdas;
    lowered.vtables = m.vtables;
    lowered.fn_refs = m.fn_refs;
    lowered.dyn_methods = m.dyn_methods;
    for f in &lowered.functions {
        crate::verify::assert_valid(f, "mono");
    }
    Ok(lowered)
}

/// A function's parameters after the first, and its result.
fn signature(f: &Function) -> (Vec<Ty>, Ty) {
    (f.params.iter().skip(1).map(|&p| f.locals[p].ty.clone()).collect(), f.ret.clone())
}

impl Mono {
    fn refuse(&mut self, span: Span, what: &str) {
        self.diagnostics.push(Diagnostic::error(
            "E0402",
            span,
            format!("`--target llvm` does not compile {what} yet: run it with `--target python`"),
        ));
    }

    /// The name of the instance of the generic function `base` for `type_args`, made the first time
    /// it is asked for; `base` itself when it is not generic.
    fn instance(&mut self, base: String, type_args: Vec<Ty>) -> String {
        let Some(generic) = self.generic.get(&base) else { return base };
        let key = (base, type_args);
        if let Some(name) = self.instances.get(&key) {
            return name.clone();
        }
        // Recursion that grows its type argument asks for instances without end: `f([x])` inside
        // `f[T]` adds a node per instance, and `f((x, x))` doubles the type, so a bound on the
        // count alone still builds types of 2^64 nodes. Past either bound the function is refused,
        // once, and no instance it refused is made to grow a larger type.
        let mut budget = INSTANCE_TYPE_LIMIT;
        let too_large = key.1.iter().any(|ty| exceeds(ty, &mut budget));
        let span = generic.span;
        let count = self.counts.entry(key.0.clone()).or_default();
        if too_large || *count >= INSTANCE_LIMIT {
            if *count <= INSTANCE_LIMIT {
                *count = INSTANCE_LIMIT + 1;
                let what = if too_large {
                    "a generic function instantiated for a type this large"
                } else {
                    "a generic function instantiated for this many types"
                };
                self.refuse(span, what);
            }
            return key.0;
        }
        *count += 1;
        let name = symbol::instance(&key.0, self.instances.len());
        let mut function = generic.clone();
        let subst = Subst { names: std::mem::take(&mut function.type_params), types: key.1.clone() };
        function.types_mut(&mut |ty| subst.apply(ty));
        function.name = name.clone();
        self.signatures.insert(name.clone(), signature(&function));
        self.instances.insert(key, name.clone());
        self.work.push(Work { function, subst });
        name
    }

    /// The index, in the monomorphic program, of the lambda `k` written in a function whose type
    /// arguments are `subst`.
    fn lambda(&mut self, k: usize, subst: &Subst) -> usize {
        if let Some(&index) = self.kept.get(&k) {
            return index;
        }
        let key = (k, subst.types.clone());
        if let Some(&index) = self.lambda_instances.get(&key) {
            return index;
        }
        let Some(mut function) = self.generic.get(&symbol::lambda(k)).cloned() else { return k };
        let index = self.lambdas.len();
        let (mut captures, mut ty) = self.lambdas_in[k].clone();
        captures.iter_mut().for_each(|t| subst.apply(t));
        subst.apply(&mut ty);
        self.lambdas.push((captures, ty));
        self.lambda_instances.insert(key, index);
        function.type_params.clear();
        function.types_mut(&mut |t| subst.apply(t));
        function.name = symbol::lambda(index);
        self.signatures.insert(function.name.clone(), signature(&function));
        self.work.push(Work { function, subst: subst.clone() });
        index
    }

    /// The function a generic call calls, in an instance whose types are all known.
    fn callee(&mut self, callee: Callee, span: Span) -> String {
        match callee {
            Callee::Function { name, type_args } => self.instance(symbol::function(&name), type_args),
            Callee::Method { owner: Ty::Adt(owner, owner_args), method, own } => {
                let base = symbol::method(&owner, &method);
                let mut type_args = owner_args;
                type_args.extend(own);
                if self.generic.contains_key(&base) { self.instance(base, type_args) } else { base }
            }
            Callee::Method { owner, method, .. } => {
                self.refuse(span, &format!("the method `{method}` of `{owner}`"));
                String::new()
            }
        }
    }

    /// The table of `ty`'s methods for the trait `trait_name`, made the first time it is needed.
    fn vtable(&mut self, trait_name: &str, ty: &Ty, span: Span) -> Option<usize> {
        let key = (trait_name.to_string(), ty.clone());
        if let Some(&index) = self.vtable_ids.get(&key) {
            return Some(index);
        }
        let Ty::Adt(..) = ty else { return None };
        let methods = self.dyn_methods.get(trait_name)?.clone();
        let mut slots = Vec::new();
        for method in methods {
            let slot = method.map(|method| {
                let callee = Callee::Method { owner: ty.clone(), method, own: Vec::new() };
                let function = self.callee(callee, span);
                let (params, ret) = self.signatures.get(&function).cloned().unwrap_or((Vec::new(), Ty::Unit));
                VSlot { function, params, ret }
            });
            slots.push(slot);
        }
        let index = self.vtables.len();
        self.vtables.push(VTable { trait_name: trait_name.to_string(), ty: ty.clone(), slots });
        self.vtable_ids.insert(key, index);
        Some(index)
    }

    fn block(&mut self, block: &mut Block, subst: &Subst) {
        for stmt in block {
            let span = stmt.span;
            match &mut stmt.kind {
                StmtKind::Let(_, e) | StmtKind::Do(e) => self.expr(e, subst, span),
                StmtKind::If(_, then, otherwise) => {
                    self.block(then, subst);
                    self.block(otherwise, subst);
                }
                StmtKind::Loop(body) => self.block(body, subst),
                StmtKind::ForRange { body, exit, .. } | StmtKind::ForStr { body, exit, .. } => {
                    self.block(body, subst);
                    self.block(exit, subst);
                }
                _ => {}
            }
        }
    }

    fn expr(&mut self, e: &mut Expr, subst: &Subst, span: Span) {
        match e {
            Expr::CallGeneric { callee, args } => {
                let name = self.callee(callee.clone(), span);
                let args = std::mem::take(args);
                *e = if args.iter().all(|a| matches!(a, Arg::Value(_))) {
                    let operands = args.into_iter().filter_map(|a| if let Arg::Value(o) = a { Some(o) } else { None });
                    Expr::Call(name, operands.collect())
                } else {
                    Expr::CallSlots(name, args)
                };
            }
            Expr::FnRefGeneric { name, type_args } => {
                let name = self.instance(symbol::function(name), std::mem::take(type_args));
                self.fn_refs.insert(name.clone());
                *e = Expr::FnRef(name);
            }
            Expr::ToDynOf { value, ty, from } => {
                let Ty::Dyn(trait_name) = ty.clone() else { return };
                let value = std::mem::replace(value, Operand::Local(0));
                *e = match self.vtable(&trait_name, from, span) {
                    Some(vtable) => Expr::ToDyn { value, ty: ty.clone(), vtable },
                    None => Expr::Use(value),
                };
            }
            Expr::Closure { lambda, .. } | Expr::Capture { lambda, .. } => *lambda = self.lambda(*lambda, subst),
            _ => {}
        }
    }
}
