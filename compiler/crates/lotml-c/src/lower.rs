//! Lowering: the checked syntax tree to the intermediate form, every call resolved and every
//! intermediate value named (specs/c-backend/design.md).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use lotml_check::ty::{INT, IntKind, Ty};
use lotml_check::{Checked, FieldSig, FnSig, TypeDef};
use lotml_diag::Diagnostic;
use lotml_syntax::ast::{
    self, Arg as AstArg, BoolOp, ExprKind, FnDef, Item, Module, PatternKind, StmtKind as Ast, StrPart,
};
use lotml_syntax::span::Span;

use crate::mir::{
    Arg, BinOp, Block, CmpOp, Const, Expr, FormatPart, Function, Local, LocalInfo, Operand, Panic, Place, Proj, Stmt,
    StmtKind, UnOp, counted,
};

/// The program as functions of the intermediate form.
pub struct Lowered {
    pub functions: Vec<Function>,
    /// Whether the module declares `fn main()`.
    pub main: bool,
    /// The records and sum types the module declares.
    pub declared: BTreeMap<String, TypeDef>,
    /// Each lambda: the types of what it captures, and its function type; its code is the
    /// function `lambda_name(index)`.
    pub lambdas: Vec<(Vec<Ty>, Ty)>,
    /// The module's functions used as values.
    pub fn_refs: BTreeSet<String>,
}

/// The C name of the method `method` of the type `owner`.
pub fn method_name(owner: &str, method: &str) -> String {
    format!("lm_{owner}_{method}")
}

/// The C name of the function of the lambda `index`.
pub fn lambda_name(index: usize) -> String {
    format!("lf_lambda{index}")
}

/// The C name of the lotml function `name`.
pub fn function_name(name: &str) -> String {
    format!("lf_{name}")
}

pub fn lower(module: &Module, checked: &Checked, text: &str) -> Result<Lowered, Vec<Diagnostic>> {
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
                        cx.methods.insert((name.name.clone(), m.name.name.clone()), m);
                    }
                }
            }
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
    let mut functions = Vec::new();
    for item in &module.items {
        match item {
            Item::Fn(f) => {
                if let Some(function) = cx.function(f) {
                    functions.push(function);
                }
            }
            Item::Impl(imp) => {
                let ast::TypeKind::Named { name, .. } = &imp.target.kind else {
                    cx.unsupported(imp.span, "this impl");
                    continue;
                };
                for m in &imp.methods {
                    let sig =
                        cx.checked.methods.get(&name.name).and_then(|ms| ms.get(&m.name.name)).map(|m| m.sig.clone());
                    let Some(sig) = sig else { continue };
                    if !sig.type_params.is_empty()
                        || matches!(cx.checked.declared.get(&name.name), Some(d) if !d.params().is_empty())
                    {
                        cx.unsupported(m.name.span, "a method of a generic type");
                        continue;
                    }
                    if let Some(function) = cx.lower_function(m, &sig, method_name(&name.name, &m.name.name)) {
                        functions.push(function);
                    }
                }
            }
            Item::Test(_) | Item::Error(_) | Item::Record(_) | Item::Sum(_) => {}
            other => cx.unsupported(other.span(), "this declaration"),
        }
    }
    if !cx.diagnostics.is_empty() {
        return Err(cx.diagnostics);
    }
    let mut lambdas = Vec::new();
    for (function, captures, ty) in std::mem::take(&mut cx.lambdas) {
        functions.push(function);
        lambdas.push((captures, ty));
    }
    Ok(Lowered {
        main: cx.fns.contains_key("main"),
        functions,
        declared: checked.declared.clone(),
        lambdas,
        fn_refs: std::mem::take(&mut cx.fn_refs),
    })
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
    /// The lambdas lowered so far: each one's function, the types it captures and its type.
    lambdas: Vec<(Function, Vec<Ty>, Ty)>,
    /// Each method of an `impl`, by its type and name.
    methods: HashMap<(String, String), &'a FnDef>,
    fn_refs: BTreeSet<String>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Context<'a> {
    fn line(&self, offset: u32) -> u32 {
        self.line_starts.partition_point(|&start| start <= offset) as u32
    }

    fn unsupported(&mut self, span: Span, what: &str) {
        self.diagnostics.push(Diagnostic::error("E0402", span, format!("the C backend does not compile {what} yet")));
    }

    fn source(&self, span: Span) -> &'a str {
        self.text.get(span.range()).unwrap_or("")
    }

    fn function(&mut self, f: &'a FnDef) -> Option<Function> {
        let sig = self.checked.functions.get(&f.name.name)?.clone();
        if !sig.type_params.is_empty() {
            self.unsupported(f.name.span, "a generic function");
            return None;
        }
        self.lower_function(f, &sig, function_name(&f.name.name))
    }

    /// The function `f`, of signature `sig`, as the C function `name`: an `inout` parameter is a
    /// pointer to the caller's slot.
    fn lower_function(&mut self, f: &'a FnDef, sig: &FnSig, name: String) -> Option<Function> {
        let line = self.line(f.span.start);
        let ret = match &sig.error {
            Some(error) => Ty::Result(Box::new(sig.ret.clone()), Box::new(error.clone())),
            None => sig.ret.clone(),
        };
        let mut b = Builder {
            cx: self,
            function: Function {
                name,
                source_name: f.name.name.clone(),
                params: Vec::new(),
                ret,
                locals: Vec::new(),
                body: Vec::new(),
                line,
            },
            vars: HashMap::new(),
            blocks: vec![Vec::new()],
            line,
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
                b.line = b.cx.line(body.end());
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
    line: u32,
}

/// What a loop gives its body: the element, and its type.
type Each<'f, 'c, 'a> = &'f mut dyn FnMut(&mut Builder<'c, 'a>, Operand, Ty);

fn int(v: i128) -> Operand {
    Operand::Const(Const::Int(v, IntKind::I64))
}

fn flag(b: bool) -> Operand {
    Operand::Const(Const::Bool(b))
}

fn rt(name: &'static str, args: Vec<Operand>, at: bool) -> Expr {
    Expr::Rt { name, args: args.into_iter().map(Arg::Value).collect(), at }
}

fn rt_args(name: &'static str, args: Vec<Arg>, at: bool) -> Expr {
    Expr::Rt { name, args, at }
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
        let line = self.line;
        self.blocks.last_mut().expect("a block").push(Stmt { line, kind });
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
        self.cx.checked.types.get(&e.span).cloned().unwrap_or(Ty::Error)
    }

    fn local_ty(&self, operand: &Operand) -> Ty {
        match operand {
            Operand::Local(l) => self.function.locals[*l].ty.clone(),
            Operand::Const(Const::Int(_, k)) => Ty::Int(*k),
            Operand::Const(Const::Float(_)) => Ty::Float(lotml_check::ty::FloatKind::F64),
            Operand::Const(Const::Bool(_)) => Ty::Bool,
            Operand::Const(Const::Str(_)) => Ty::Str,
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
        self.line = self.cx.line(stmt.span.start);
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
            Ast::Assert { test, .. } => {
                let ok = self.condition(test);
                let expression = self.cx.source(test.span).to_string();
                let fail = self.block(|b| b.push(StmtKind::Panic(Panic::Assert(expression))));
                self.push(StmtKind::If(ok, Vec::new(), fail));
            }
            Ast::Pass => {}
            Ast::Break => self.push(StmtKind::Break),
            Ast::Continue => self.push(StmtKind::Continue),
            Ast::If { branches, orelse } => self.if_chain(branches, orelse.as_ref()),
            Ast::While { test, body } => {
                let line = self.line;
                let body = self.block(|b| {
                    let ok = b.condition(test);
                    let stop = b.block(|b| b.push(StmtKind::Break));
                    b.line = line;
                    b.push(StmtKind::If(ok, Vec::new(), stop));
                    b.statements(&body.stmts);
                });
                self.line = line;
                self.push(StmtKind::Loop(body));
            }
            Ast::For { target, iter, body } => {
                let line = self.line;
                self.each(iter, &mut |b, element, ty| {
                    b.bind(target, element, &ty);
                    b.statements(&body.stmts);
                    b.line = line;
                });
            }
            Ast::Match { subject, arms } => self.match_stmt(subject, arms),
            Ast::Error => {}
        }
    }

    /// The type a `var` or annotated local was declared with, as the checker resolved it.
    fn var_type(&self, span: Span) -> Option<Ty> {
        self.cx.checked.types.get(&span).cloned()
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
                let Ty::Tuple(types) = ty else {
                    self.unsupported(target.span, "unpacking this value");
                    return;
                };
                for (i, (item, t)) in items.iter().zip(types).enumerate() {
                    let part = self.hold(t.clone(), Expr::TupleGet { tuple: value.clone(), index: i });
                    self.assign_to(item, part, t);
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
                    name: "lt_dict_set",
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
                Proj::Index(_) => element(&ty),
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
                Proj::Index(i) => {
                    let elem = element(&ty);
                    value =
                        self.hold(elem.clone(), Expr::ListGet { list: value, index: i.clone(), elem: elem.clone() });
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
        match (self.cx.checked.declared.get(name), variant) {
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
        match (self.cx.checked.declared.get(name), variant) {
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
            self.push(StmtKind::Panic(Panic::Assert("an error outside a function that can fail".into())));
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
            let line = self.line;
            self.variant_chain(&variants, &value, &ty, &tag, line);
            return;
        }
        let narrow =
            arms.iter().any(|a| matches!(&a.pattern.kind, PatternKind::Literal(e) if matches!(e.kind, ExprKind::None)));
        let done = self.temp(Ty::Bool);
        self.push(StmtKind::Let(done, Expr::Use(flag(false))));
        let line = self.line;
        for arm in arms {
            let body = self.block(|b| {
                b.pattern(&arm.pattern, value.clone(), &ty, narrow, &mut |b| {
                    b.push(StmtKind::Let(done, Expr::Use(flag(true))));
                    b.statements(&arm.body.stmts);
                });
            });
            self.line = line;
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
        line: u32,
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
                let otherwise = self.block(|b| b.variant_chain(rest, value, ty, tag, line));
                self.line = line;
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
        let line = self.line;
        let ok = self.condition(test);
        let then = self.block(|b| b.statements(&body.stmts));
        let otherwise = self.block(|b| b.if_chain(rest, orelse));
        self.line = line;
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
                let Ty::Tuple(types) = ty else {
                    self.unsupported(*span, "unpacking this value");
                    return;
                };
                for (i, (item, t)) in items.iter().zip(types).enumerate() {
                    let part = self.hold(t.clone(), Expr::TupleGet { tuple: value.clone(), index: i });
                    self.bind(item, part, t);
                }
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
                let line = self.line;
                let inner = self.block(|b| body(b, Operand::Local(var), INT));
                self.line = line;
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
                        let list = self.materialize(a);
                        let elem = element(&self.local_ty(&list));
                        (list, elem)
                    })
                    .collect();
                self.indexed(&lists, false, body);
                return;
            }
            if self.is_prelude(func, "reversed") && positional.len() == 1 {
                let list = self.materialize(positional[0]);
                let elem = element(&self.local_ty(&list));
                self.indexed(&[(list, elem)], true, body);
                return;
            }
        }
        let ty = self.ty(iter);
        match ty {
            Ty::List(_) => {
                let over = self.value(iter);
                let snapshot = self.hold(ty.clone(), Expr::Use(over));
                let elem = element(&ty);
                self.indexed(&[(snapshot, elem)], false, body);
            }
            Ty::Dict(..) | Ty::Set(_) => {
                let list = self.materialize(iter);
                let elem = element(&ty);
                self.indexed(&[(list, elem)], false, body);
            }
            Ty::Str => {
                let over = self.value(iter);
                let snapshot = self.hold(Ty::Str, Expr::Use(over));
                let var = self.temp(Ty::Str);
                let line = self.line;
                let inner = self.block(|b| body(b, Operand::Local(var), Ty::Str));
                self.line = line;
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
        let line = self.line;
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
                    let get = Expr::ListGet { list: list.clone(), index: Operand::Local(k), elem: elem.clone() };
                    (b.hold(elem.clone(), get), elem.clone())
                })
                .collect();
            let step = if reverse { BinOp::Sub } else { BinOp::Add };
            b.push(StmtKind::Let(k, Expr::Binary(step, Operand::Local(k), int(1), INT)));
            if let [(element, ty)] = elements.as_slice() {
                body(b, element.clone(), ty.clone());
            } else {
                let tuple_ty = Ty::Tuple(elements.iter().map(|(_, t)| t.clone()).collect());
                let items = elements.into_iter().map(|(e, _)| e).collect();
                let tuple = b.hold(tuple_ty.clone(), Expr::TupleNew { ty: tuple_ty.clone(), items });
                body(b, tuple, tuple_ty);
            }
        });
        self.line = line;
        self.push(StmtKind::Loop(inner));
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
            Ty::Str => self.hold(Ty::list(Ty::Str), rt("lt_str_chars", vec![v], false)),
            Ty::Dict(k, _) => self.hold(Ty::list((**k).clone()), rt("lt_dict_keys", vec![v], false)),
            Ty::Set(t) => self.hold(Ty::list((**t).clone()), rt("lt_set_list", vec![v], false)),
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
            name: "lt_list_push",
            place: Place { local: list, proj: Vec::new() },
            args: vec![Arg::Address(value, elem.clone())],
            at: false,
            result: None,
        });
    }

    /// An optional `ty` from a runtime call that returns whether it found a value and sets it in
    /// an output: `args` and then the output.
    fn optional_from(&mut self, ty: &Ty, name: &'static str, mut args: Vec<Arg>, at: bool) -> Value {
        let inner = match ty {
            Ty::Optional(t) => (**t).clone(),
            other => other.clone(),
        };
        let found = self.temp(inner.clone());
        args.push(Arg::Out(found, inner));
        let ok = self.hold(Ty::Bool, Expr::Rt { name, args, at });
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
                if is_unit(&ty) || matches!(ty, Ty::Never) {
                    self.push(StmtKind::Do(expr));
                    return Operand::Const(Const::Unit);
                }
                self.hold(ty, expr)
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
                    None if self.cx.fns.contains_key(name.as_str()) => {
                        self.cx.fn_refs.insert(name.clone());
                        Value::Expr(Expr::FnRef(name.clone()))
                    }
                    None => Value::Done(self.unsupported(e.span, &format!("the value `{name}`"))),
                },
            },
            ExprKind::Attr { object, name } => {
                let ty = self.ty(object);
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
                        name: "lt_dict_set",
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
                        name: "lt_set_add",
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
                let t = self.temp(ty);
                let a = self.block(|b| {
                    let v = b.value(then);
                    b.push(StmtKind::Let(t, Expr::Use(v)));
                });
                let c = self.block(|b| {
                    let v = b.value(orelse);
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
                    Ty::Tuple(_) => {
                        let ExprKind::Int(text) = &index.kind else {
                            return Value::Done(self.unsupported(index.span, "a tuple index that is not a literal"));
                        };
                        let o = self.value(object);
                        Value::Expr(Expr::TupleGet { tuple: o, index: parse_int(text) as usize })
                    }
                    Ty::Str => {
                        let o = self.value(object);
                        let i = self.value(index);
                        Value::Expr(rt("lt_str_index", vec![o, i], true))
                    }
                    Ty::List(elem) => {
                        let elem = (**elem).clone();
                        let o = self.value(object);
                        let i = self.value(index);
                        Value::Expr(Expr::ListGet { list: o, index: i, elem })
                    }
                    Ty::Dict(k, v) => {
                        let (k, v) = ((**k).clone(), (**v).clone());
                        let o = self.value(object);
                        let i = self.value(index);
                        let i = self.coerce(i, &k);
                        Value::Expr(Expr::RtValue {
                            name: "lt_dict_get",
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
                    Ty::Str => Value::Expr(rt("lt_str_slice", args, true)),
                    Ty::List(_) => Value::Expr(rt("lt_list_slice", args, true)),
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
            (ast::BinOp::Add, Ty::Str, _) => rt("lt_str_concat", vec![a, b], false),
            (ast::BinOp::Mul, Ty::Str, _) => rt("lt_str_repeat", vec![a, b], true),
            (ast::BinOp::Mul, _, Ty::Str) => rt("lt_str_repeat", vec![b, a], true),
            (ast::BinOp::Add, Ty::List(_), _) => rt("lt_list_concat", vec![a, b], false),
            (ast::BinOp::BitOr, Ty::Set(_), _) => rt("lt_set_union", vec![a, b], false),
            (ast::BinOp::BitAnd, Ty::Set(_), _) => rt("lt_set_intersection", vec![a, b], false),
            (ast::BinOp::Sub, Ty::Set(_), _) => rt("lt_set_difference", vec![a, b], false),
            (ast::BinOp::Mul, Ty::List(_), _) => rt("lt_list_repeat", vec![a, b], true),
            (ast::BinOp::Mul, _, Ty::List(_)) => rt("lt_list_repeat", vec![b, a], true),
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

    fn compare_rest(&mut self, result: Local, left: &mut Operand, left_ty: &mut Ty, rest: &[(ast::CmpOp, ast::Expr)]) {
        let Some(((op, right), more)) = rest.split_first() else { return };
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
            ast::CmpOp::In | ast::CmpOp::NotIn => {
                let contains =
                    Expr::Contains { container: right_value.clone(), item: left.clone(), ty: right_ty.clone() };
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
                Expr::Compare(op, left.clone(), right_value.clone(), left_ty.clone())
            }
        };
        self.push(StmtKind::Let(result, compared));
        if more.is_empty() {
            return;
        }
        *left = right_value;
        *left_ty = right_ty;
        let next = self.block(|b| b.compare_rest(result, left, left_ty, more));
        self.push(StmtKind::If(Operand::Local(result), next, Vec::new()));
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
            return self.call_function(name, f, args);
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
                    name: "lt_list_extreme",
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
                    name: "lt_list_extreme",
                    args: vec![Arg::Value(list), Arg::Value(flag(name == "max"))],
                    at: true,
                    ty,
                })
            }
            ("sum", [x, rest @ ..]) if rest.len() <= 1 => {
                let elem = result_ty;
                let list = self.materialize(x);
                let total = match &elem {
                    Ty::Float(_) => rt("lt_sum_f64", vec![list], false),
                    Ty::Int(IntKind::U64) => rt("lt_sum_u64", vec![list], true),
                    _ => rt("lt_sum_i64", vec![list], true),
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
                Value::Expr(rt(if any { "lt_list_any" } else { "lt_list_all" }, vec![list], false))
            }
            ("sorted", [x]) => {
                let reverse = match keyword("reverse") {
                    Some(r) => self.value(r),
                    None => flag(false),
                };
                let list = self.materialize(x);
                let Some(key) = keyword("key") else {
                    return Value::Expr(rt("lt_list_sorted", vec![list, reverse], true));
                };
                let ty = self.local_ty(&list);
                let Operand::Local(copy) = self.hold(ty, rt("lt_list_copy", vec![list], false)) else {
                    unreachable!("a held value")
                };
                let place = Place { local: copy, proj: Vec::new() };
                self.sort_by_key(place, key, reverse);
                Value::Done(Operand::Local(copy))
            }
            ("reversed", [x]) => {
                let list = self.materialize(x);
                Value::Expr(rt("lt_list_reversed", vec![list], false))
            }
            ("range", values) => {
                let values: Vec<Operand> = values.iter().map(|v| self.value(v)).collect();
                let (start, stop, step) = match values.as_slice() {
                    [stop] => (int(0), stop.clone(), int(1)),
                    [start, stop] => (start.clone(), stop.clone(), int(1)),
                    [start, stop, step] => (start.clone(), stop.clone(), step.clone()),
                    _ => return Value::Done(self.unsupported(whole.span, "this `range`")),
                };
                Value::Expr(rt("lt_range_list", vec![start, stop, step], true))
            }
            ("list", []) => Value::Expr(Expr::ListNew { elem: element(&result_ty), items: Vec::new() }),
            ("set", []) => Value::Expr(Expr::SetNew { elem: element(&result_ty), items: Vec::new() }),
            ("set", [x]) => {
                if let Ty::Set(_) = self.ty(x) {
                    let v = self.value(x);
                    return Value::Expr(rt("lt_set_copy", vec![v], false));
                }
                let elem = element(&result_ty);
                let list = match &x.kind {
                    ExprKind::Call { .. } | ExprKind::Generator { .. } => self.collect(x, &elem),
                    _ => self.materialize(x),
                };
                Value::Expr(rt_args("lt_set_from_list", vec![Arg::Desc(elem), Arg::Value(list)], false))
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
                    "lt_dict_from_pairs",
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
                        Ty::List(_) => Value::Expr(rt("lt_list_copy", vec![v], false)),
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
                Value::Expr(rt("lt_str_ord", vec![v], true))
            }
            ("chr", [x]) => {
                let v = self.value(x);
                Value::Expr(rt("lt_str_chr", vec![v], true))
            }
            ("int" | "float", [x]) if self.ty(x) == Ty::Str => {
                let v = self.value(x);
                Value::Expr(rt(if name == "int" { "lt_str_int" } else { "lt_str_float" }, vec![v], true))
            }
            ("int" | "float" | "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "f32", [x])
                if self.ty(x).is_numeric() || self.ty(x) == Ty::Bool =>
            {
                let from = self.ty(x);
                let v = self.value(x);
                Value::Expr(Expr::Convert(v, from, result_ty))
            }
            _ => Value::Done(self.unsupported(whole.span, &format!("`{name}` here"))),
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
        let capture_types: Vec<Ty> = captures.iter().map(|(_, _, l)| self.function.locals[*l].ty.clone()).collect();
        let line = self.line;
        let mut b = Builder {
            cx: &mut *self.cx,
            function: Function {
                name: lambda_name(index),
                source_name: "<lambda>".to_string(),
                params: Vec::new(),
                ret: ret.clone(),
                locals: Vec::new(),
                body: Vec::new(),
                line,
            },
            vars: HashMap::new(),
            blocks: vec![Vec::new()],
            line,
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
        self.cx.lambdas.push((function, capture_types, ty.clone()));
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
                    let operands = args.into_iter().zip(&sig.params).map(|((a, _), p)| self.coerce(a, &p.ty)).collect();
                    let ty = sig.ret.clone();
                    return (self.hold(ty.clone(), Expr::Call(function_name(name), operands)), ty);
                }
                let Some((a, t)) = args.into_iter().next() else {
                    return (self.unsupported(func.span, "this function"), Ty::Error);
                };
                let (expr, ty) = match name.as_str() {
                    "len" => (Expr::Len(a, t), INT),
                    "str" => (Expr::ToStr(a, t), Ty::Str),
                    "int" if t == Ty::Str => (rt("lt_str_int", vec![a], true), INT),
                    "float" if t == Ty::Str => {
                        (rt("lt_str_float", vec![a], true), Ty::Float(lotml_check::ty::FloatKind::F64))
                    }
                    "int" => (Expr::Convert(a, t, INT), INT),
                    "float" => (
                        Expr::Convert(a, t, Ty::Float(lotml_check::ty::FloatKind::F64)),
                        Ty::Float(lotml_check::ty::FloatKind::F64),
                    ),
                    "abs" => (Expr::Unary(UnOp::Abs, a, t.clone()), t),
                    "ord" => (rt("lt_str_ord", vec![a], true), INT),
                    "chr" => (rt("lt_str_chr", vec![a], true), Ty::Str),
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
            name: "lt_list_sort_by_keys",
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
                    return sig.ret.clone();
                }
                match name.as_str() {
                    "len" | "ord" | "int" => INT,
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
        let first = self.hold(elem.clone(), Expr::ListGet { list: list.clone(), index: int(0), elem: elem.clone() });
        self.push(StmtKind::Let(best, Expr::Use(first.clone())));
        let (k0, _) = self.apply(key, vec![(first, elem.clone())]);
        let best_key = self.temp(key_ty.clone());
        self.push(StmtKind::Let(best_key, Expr::Use(k0)));
        let i = self.temp(INT);
        self.push(StmtKind::Let(i, Expr::Use(int(1))));
        let line = self.line;
        let body = self.block(|b| {
            let more = b.hold(Ty::Bool, Expr::Compare(CmpOp::Lt, Operand::Local(i), n.clone(), INT));
            let done = b.block(|b| b.push(StmtKind::Break));
            b.push(StmtKind::If(more, Vec::new(), done));
            let item = b
                .hold(elem.clone(), Expr::ListGet { list: list.clone(), index: Operand::Local(i), elem: elem.clone() });
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
        self.line = line;
        self.push(StmtKind::Loop(body));
        Operand::Local(best)
    }

    /// `object.name(args)`: a method of a declared type, or of a built-in one.
    fn method(&mut self, whole: &ast::Expr, object: &ast::Expr, name: &str, args: &[AstArg]) -> Value {
        let ty = self.ty(object);
        let owner = match &ty {
            Ty::Adt(owner, _) | Ty::TypeName(owner) => Some(owner.clone()),
            _ => None,
        };
        if let Some(owner) = owner
            && let Some(value) = self.user_method(object, &owner, name, args)
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
                    name: "lt_dict_pop",
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
                self.push(StmtKind::Mutate { name: "lt_dict_clear", place, args: Vec::new(), at: false, result: None });
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
                            "lt_dict_get_optional",
                            vec![Arg::Value(d), Arg::Address(key, k.clone())],
                            false,
                        )
                    }
                    ("get", [key, default]) => {
                        let key = self.coerce(key.clone(), k);
                        let default = self.coerce(default.clone(), v);
                        Value::Expr(Expr::RtValue {
                            name: "lt_dict_get_or",
                            args: vec![Arg::Value(d), Arg::Address(key, k.clone()), Arg::Address(default, v.clone())],
                            at: false,
                            ty: v.clone(),
                        })
                    }
                    ("keys", []) => Value::Expr(rt("lt_dict_keys", vec![d], false)),
                    ("values", []) => Value::Expr(rt("lt_dict_values", vec![d], false)),
                    ("items", []) => {
                        let pair = Ty::Tuple(vec![k.clone(), v.clone()]);
                        Value::Expr(rt_args(
                            "lt_dict_items",
                            vec![Arg::Value(d), Arg::Desc(pair.clone()), Arg::Offset(pair)],
                            false,
                        ))
                    }
                    ("contains", [key]) => {
                        let key = self.coerce(key.clone(), k);
                        Value::Expr(rt_args(
                            "lt_dict_contains",
                            vec![Arg::Value(d), Arg::Address(key, k.clone())],
                            false,
                        ))
                    }
                    ("copy", []) => Value::Expr(rt("lt_dict_copy", vec![d], false)),
                    _ => Value::Done(self.unsupported(whole.span, &format!("the method `{name}` here"))),
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
            let mutate = |name: &'static str, args: Vec<Arg>, at: bool| StmtKind::Mutate {
                name,
                place: place.clone(),
                args,
                at,
                result: None,
            };
            let stmt = match (name, values.as_slice()) {
                ("add", [x]) => mutate("lt_set_add", vec![Arg::Address(x.clone(), t.clone())], false),
                ("remove", [x]) => mutate("lt_set_remove", vec![Arg::Address(x.clone(), t.clone())], true),
                ("discard", [x]) => mutate("lt_set_discard", vec![Arg::Address(x.clone(), t.clone())], false),
                ("pop", []) => {
                    let found = self.temp(t.clone());
                    let ok = self.temp(Ty::Bool);
                    self.push(StmtKind::Mutate {
                        name: "lt_set_pop",
                        place: place.clone(),
                        args: vec![Arg::Out(found, t.clone())],
                        at: false,
                        result: Some(ok),
                    });
                    return Value::Expr(Expr::OptIf { ty, cond: Operand::Local(ok), value: Operand::Local(found) });
                }
                _ => return Value::Done(self.unsupported(whole.span, &format!("the method `{name}` here"))),
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
                Value::Expr(rt_args("lt_set_contains", vec![Arg::Value(s), Arg::Address(x, t.clone())], false))
            }
            ("issubset", [other]) => Value::Expr(rt("lt_set_issubset", vec![s, other.clone()], false)),
            ("issuperset", [other]) => Value::Expr(rt("lt_set_issubset", vec![other.clone(), s], false)),
            ("union", [other]) => Value::Expr(rt("lt_set_union", vec![s, other.clone()], false)),
            ("intersection", [other]) => Value::Expr(rt("lt_set_intersection", vec![s, other.clone()], false)),
            ("difference", [other]) => Value::Expr(rt("lt_set_difference", vec![s, other.clone()], false)),
            ("copy", []) => Value::Expr(rt("lt_set_copy", vec![s], false)),
            _ => Value::Done(self.unsupported(whole.span, &format!("the method `{name}` here"))),
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
        if !keywords.is_empty() {
            return Value::Done(self.unsupported(whole.span, &format!("keyword arguments of `{name}`")));
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
                    "lower" => "lt_str_lower",
                    "upper" => "lt_str_upper",
                    "swapcase" => "lt_str_swapcase",
                    "title" => "lt_str_title",
                    "capitalize" => "lt_str_capitalize",
                    "isalpha" => "lt_str_isalpha",
                    "isdigit" => "lt_str_isdigit",
                    "isspace" => "lt_str_isspace",
                    "isalnum" => "lt_str_isalnum",
                    "isupper" => "lt_str_isupper",
                    _ => "lt_str_islower",
                };
                rt(rt_name, all, false)
            }
            ("strip" | "lstrip" | "rstrip", rest @ ([] | [_])) => {
                all.push(rest.first().cloned().unwrap_or(null));
                all.push(flag(name != "rstrip"));
                all.push(flag(name != "lstrip"));
                rt("lt_str_strip", all, false)
            }
            ("startswith" | "endswith" | "count", [x]) => {
                all.push(x.clone());
                let rt_name = match name {
                    "startswith" => "lt_str_startswith",
                    "endswith" => "lt_str_endswith",
                    _ => "lt_str_count",
                };
                rt(rt_name, all, false)
            }
            ("replace", [old, with, rest @ ..]) if rest.len() <= 1 => {
                all.push(old.clone());
                all.push(with.clone());
                all.push(rest.first().cloned().unwrap_or(int(-1)));
                rt("lt_str_replace", all, false)
            }
            ("zfill", [width]) => {
                all.push(width.clone());
                rt("lt_str_zfill", all, false)
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
                rt("lt_str_pad", all, true)
            }
            ("split", rest) if rest.len() <= 2 => {
                all.push(rest.first().cloned().unwrap_or(null));
                all.push(rest.get(1).cloned().unwrap_or(int(-1)));
                rt("lt_str_split", all, true)
            }
            ("splitlines", []) => rt("lt_str_splitlines", all, false),
            ("join", [parts]) => {
                all.push(parts.clone());
                rt("lt_str_join", all, false)
            }
            ("to_int", []) => {
                return self.optional_from(&self.ty(whole), "lt_str_to_int", vec![Arg::Value(receiver)], true);
            }
            ("to_float", []) => {
                return self.optional_from(&self.ty(whole), "lt_str_to_float", vec![Arg::Value(receiver)], false);
            }
            ("find" | "rfind", [sub]) => {
                let args = vec![Arg::Value(receiver), Arg::Value(sub.clone()), Arg::Value(flag(name == "rfind"))];
                return self.optional_from(&self.ty(whole), "lt_str_find", args, false);
            }
            ("split_once", [sep]) => {
                let (head, tail) = (self.temp(Ty::Str), self.temp(Ty::Str));
                let args = vec![
                    Arg::Value(receiver),
                    Arg::Value(sep.clone()),
                    Arg::Out(head, Ty::Str),
                    Arg::Out(tail, Ty::Str),
                ];
                let ok = self.hold(Ty::Bool, Expr::Rt { name: "lt_str_split_once", args, at: true });
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
                        rt("lt_str_partition_part", vec![receiver.clone(), sep.clone(), int(which)], true),
                    );
                    parts.push(part);
                }
                return Value::Expr(Expr::TupleNew { ty: self.ty(whole), items: parts });
            }
            _ => return Value::Done(self.unsupported(whole.span, &format!("the method `{name}` here"))),
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
                    if name == "count" { "lt_list_count" } else { "lt_list_contains" },
                    vec![Arg::Value(list), Arg::Address(x.clone(), elem.clone())],
                    false,
                )),
                ("copy", []) => Value::Expr(rt("lt_list_copy", vec![list], false)),
                ("index", [x]) => {
                    let args = vec![Arg::Value(list), Arg::Address(x.clone(), elem.clone())];
                    self.optional_from(&self.ty(whole), "lt_list_index", args, false)
                }
                ("last", []) => self.optional_from(&self.ty(whole), "lt_list_last", vec![Arg::Value(list)], false),
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
                _ => Value::Done(self.unsupported(whole.span, &format!("the method `{name}` here"))),
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
        let mutate = |name: &'static str, args: Vec<Arg>, at: bool| StmtKind::Mutate {
            name,
            place: place.clone(),
            args,
            at,
            result: None,
        };
        let stmt = match (name, values.as_slice()) {
            ("append", [x]) => mutate("lt_list_push", vec![Arg::Address(x.clone(), elem.clone())], false),
            ("extend", [other]) => mutate("lt_list_extend", vec![Arg::Value(other.clone())], false),
            ("insert", [i, x]) => {
                mutate("lt_list_insert", vec![Arg::Value(i.clone()), Arg::Address(x.clone(), elem.clone())], false)
            }
            ("remove", [x]) => mutate("lt_list_remove", vec![Arg::Address(x.clone(), elem.clone())], true),
            ("clear", []) => mutate("lt_list_clear", Vec::new(), false),
            ("pop", rest @ ([] | [_])) => {
                let ty = self.ty(whole);
                let found = self.temp(elem.clone());
                let ok = self.temp(Ty::Bool);
                let (has, index) = match rest.first() {
                    Some(i) => (flag(true), i.clone()),
                    None => (flag(false), int(0)),
                };
                self.push(StmtKind::Mutate {
                    name: "lt_list_pop",
                    place: place.clone(),
                    args: vec![Arg::Value(has), Arg::Value(index), Arg::Out(found, elem.clone())],
                    at: true,
                    result: Some(ok),
                });
                return Value::Expr(Expr::OptIf { ty, cond: Operand::Local(ok), value: Operand::Local(found) });
            }
            ("reverse", []) => mutate("lt_list_reverse", Vec::new(), false),
            ("sort", []) => {
                let reverse = match keywords.iter().find(|(k, _)| *k == "reverse") {
                    Some((_, r)) => self.value(r),
                    None => flag(false),
                };
                if let Some((_, key)) = keywords.iter().find(|(k, _)| *k == "key") {
                    self.sort_by_key(place.clone(), key, reverse);
                    return Value::Done(Operand::Const(Const::Unit));
                }
                mutate("lt_list_sort", vec![Arg::Value(reverse)], true)
            }
            _ => return Value::Done(self.unsupported(whole.span, &format!("the method `{name}` here"))),
        };
        self.push(stmt);
        Value::Done(Operand::Const(Const::Unit))
    }

    fn call_function(&mut self, name: &str, f: &'a FnDef, args: &[AstArg]) -> Value {
        let Some(sig) = self.cx.checked.functions.get(name).cloned() else {
            return Value::Done(self.unsupported(f.name.span, "this function"));
        };
        self.call_with(function_name(name), &f.params, &sig.params, None, args)
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
    /// type with that method.
    fn user_method(&mut self, object: &ast::Expr, owner: &str, name: &str, args: &[AstArg]) -> Option<Value> {
        let method = self.cx.checked.methods.get(owner)?.get(name)?.clone();
        let f = *self.cx.methods.get(&(owner.to_string(), name.to_string()))?;
        let c_name = method_name(owner, name);
        let (params, sigs, receiver) = match method.receiver {
            None => (&f.params[..], &method.sig.params[..], None),
            Some(convention) => {
                let receiver = if convention == ast::Convention::Inout {
                    match self.place(object) {
                        Some(place) => Arg::Slot(place),
                        None => return Some(Value::Done(self.unsupported(object.span, "this receiver"))),
                    }
                } else {
                    Arg::Value(self.value(object))
                };
                (&f.params[1..], &method.sig.params[1..], Some(receiver))
            }
        };
        Some(self.call_with(c_name, params, sigs, receiver, args))
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
