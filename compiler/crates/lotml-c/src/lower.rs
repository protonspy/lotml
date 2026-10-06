//! Lowering: the checked syntax tree to the intermediate form, every call resolved and every
//! intermediate value named (specs/c-backend/design.md).

use std::collections::HashMap;

use lotml_check::Checked;
use lotml_check::ty::{INT, IntKind, Ty};
use lotml_diag::Diagnostic;
use lotml_syntax::ast::{self, Arg as AstArg, BoolOp, ExprKind, FnDef, Item, Module, StmtKind as Ast, StrPart};
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
        diagnostics: Vec::new(),
    };
    for item in &module.items {
        if let Item::Fn(f) = item {
            cx.fns.insert(f.name.name.clone(), f);
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
            Item::Test(_) | Item::Error(_) => {}
            other => cx.unsupported(other.span(), "this declaration"),
        }
    }
    if !cx.diagnostics.is_empty() {
        return Err(cx.diagnostics);
    }
    Ok(Lowered { main: cx.fns.contains_key("main"), functions })
}

struct Context<'a> {
    text: &'a str,
    checked: &'a Checked,
    line_starts: Vec<u32>,
    /// Each name that refers to a local, with the span of its declaration.
    resolved: HashMap<Span, Span>,
    fns: HashMap<String, &'a FnDef>,
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
        let line = self.line(f.span.start);
        let mut b = Builder {
            cx: self,
            function: Function {
                name: function_name(&f.name.name),
                source_name: f.name.name.clone(),
                params: Vec::new(),
                ret: sig.ret.clone(),
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
            b.function.params.push(local);
        }
        if let Some(body) = &f.body {
            b.statements(&body.stmts);
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
        self.function.locals.push(LocalInfo { ty, name: name.map(ToString::to_string) });
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
                let local = self.declare(name.span, &name.name, declared);
                self.push(StmtKind::Let(local, Expr::Use(value)));
            }
            Ast::Annotated { target, value, .. } => {
                let ty = self.var_type(target.span).unwrap_or_else(|| self.ty(value));
                let value = self.value(value);
                let local = self.declare(target.span, &target.name, ty);
                self.push(StmtKind::Let(local, Expr::Use(value)));
            }
            Ast::Assign { target, value } => {
                let ty = self.ty(value);
                let v = self.value(value);
                self.assign_to(target, v, ty);
            }
            Ast::AugAssign { target, op, value } => self.aug_assign(target, *op, value),
            Ast::Return(value) => {
                let value = value.as_ref().map(|v| self.value(v)).filter(|_| !is_unit(&self.function.ret));
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
            Ast::Match { .. } => {
                self.unsupported(stmt.span, "`match`");
            }
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
            ExprKind::Index { .. } => match self.place(target) {
                Some(place) => self.push(StmtKind::Store(place, value)),
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
            _ => None,
        }
    }

    /// The value at `place`, read.
    fn read_place(&mut self, place: &Place) -> Operand {
        let mut value = Operand::Local(place.local);
        let mut ty = self.function.locals[place.local].ty.clone();
        for proj in &place.proj {
            let Proj::Index(i) = proj;
            let elem = element(&ty);
            value = self.hold(elem.clone(), Expr::ListGet { list: value, index: i.clone(), elem: elem.clone() });
            ty = elem;
        }
        value
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

    /// `e` as a list: a list as it is, a string as its characters.
    fn materialize(&mut self, e: &ast::Expr) -> Operand {
        let ty = self.ty(e);
        let v = self.value(e);
        if ty == Ty::Str {
            return self.hold(Ty::list(Ty::Str), rt("lt_str_chars", vec![v], false));
        }
        v
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
        });
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
            ExprKind::None | ExprKind::Unit => Value::Done(Operand::Const(Const::Unit)),
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
                Some(local) => Value::Done(Operand::Local(local)),
                None => Value::Done(self.unsupported(e.span, &format!("the value `{name}`"))),
            },
            ExprKind::List(items) => {
                let elem = element(&self.ty(e));
                let mut values = Vec::new();
                for item in items {
                    values.push(self.value(item));
                }
                Value::Expr(Expr::ListNew { elem, items: values })
            }
            ExprKind::Tuple(items) => {
                let ty = self.ty(e);
                let mut values = Vec::new();
                for item in items {
                    values.push(self.value(item));
                }
                Value::Expr(Expr::TupleNew { ty, items: values })
            }
            ExprKind::ListComp { element, loops } | ExprKind::Generator { element, loops } => {
                Value::Done(self.comprehension(e, element, loops))
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
        let compared = match op {
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
        let ExprKind::Name(name) = &func.kind else {
            return Value::Done(self.unsupported(func.span, "a call of this value"));
        };
        if self.local_of(func.span).is_some() {
            return Value::Done(self.unsupported(func.span, "a call of a local"));
        }
        if let Some(f) = self.cx.fns.get(name.as_str()).copied() {
            return self.call_function(name, f, args);
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
                if keyword("key").is_some() {
                    return Value::Done(self.unsupported(whole.span, "`sorted` with a `key`"));
                }
                let reverse = match keyword("reverse") {
                    Some(r) => self.value(r),
                    None => flag(false),
                };
                let list = self.materialize(x);
                Value::Expr(rt("lt_list_sorted", vec![list, reverse], true))
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
            ("list", [x]) => match &x.kind {
                ExprKind::Call { .. } | ExprKind::Generator { .. } => {
                    let elem = element(&result_ty);
                    Value::Done(self.collect(x, &elem))
                }
                _ => {
                    let ty = self.ty(x);
                    let v = self.value(x);
                    match ty {
                        Ty::Str => Value::Expr(rt("lt_str_chars", vec![v], false)),
                        _ => Value::Expr(rt("lt_list_copy", vec![v], false)),
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

    /// `object.name(args)`: a method of a built-in type.
    fn method(&mut self, whole: &ast::Expr, object: &ast::Expr, name: &str, args: &[AstArg]) -> Value {
        let ty = self.ty(object);
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
        let mutating = matches!(name, "append" | "extend" | "insert" | "remove" | "clear" | "sort" | "reverse");
        if !mutating {
            let list = self.value(object);
            let mut values = Vec::new();
            for e in positional {
                values.push(self.value(e));
            }
            return match (name, values.as_slice()) {
                ("count" | "contains", [x]) => Value::Expr(rt_args(
                    if name == "count" { "lt_list_count" } else { "lt_list_contains" },
                    vec![Arg::Value(list), Arg::Address(x.clone(), elem.clone())],
                    false,
                )),
                ("copy", []) => Value::Expr(rt("lt_list_copy", vec![list], false)),
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
        let mutate = |name: &'static str, args: Vec<Arg>, at: bool| StmtKind::Mutate { name, place, args, at };
        let stmt = match (name, values.as_slice()) {
            ("append", [x]) => mutate("lt_list_push", vec![Arg::Address(x.clone(), elem.clone())], false),
            ("extend", [other]) => mutate("lt_list_extend", vec![Arg::Value(other.clone())], false),
            ("insert", [i, x]) => {
                mutate("lt_list_insert", vec![Arg::Value(i.clone()), Arg::Address(x.clone(), elem.clone())], false)
            }
            ("remove", [x]) => mutate("lt_list_remove", vec![Arg::Address(x.clone(), elem.clone())], true),
            ("clear", []) => mutate("lt_list_clear", Vec::new(), false),
            ("reverse", []) => mutate("lt_list_reverse", Vec::new(), false),
            ("sort", []) => {
                if keywords.iter().any(|(k, _)| *k == "key") {
                    return Value::Done(self.unsupported(whole.span, "`sort` with a `key`"));
                }
                let reverse = match keywords.iter().find(|(k, _)| *k == "reverse") {
                    Some((_, r)) => self.value(r),
                    None => flag(false),
                };
                mutate("lt_list_sort", vec![Arg::Value(reverse)], true)
            }
            _ => return Value::Done(self.unsupported(whole.span, &format!("the method `{name}` here"))),
        };
        self.push(stmt);
        Value::Done(Operand::Const(Const::Unit))
    }

    fn call_function(&mut self, name: &str, f: &'a FnDef, args: &[AstArg]) -> Value {
        let mut slots: Vec<Option<&ast::Expr>> = vec![None; f.params.len()];
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
                    if let Some(i) = f.params.iter().position(|p| p.name.name == key.name) {
                        slots[i] = Some(e);
                    }
                }
                AstArg::Inout(e, _) => return Value::Done(self.unsupported(e.span, "an `inout` argument")),
            }
        }
        let mut operands = Vec::new();
        for (slot, param) in slots.iter().zip(&f.params) {
            match slot.or(param.default.as_ref()) {
                Some(e) => {
                    let v = self.value(e);
                    operands.push(v);
                }
                None => return Value::Done(Operand::Const(Const::Unit)),
            }
        }
        Value::Expr(Expr::Call(function_name(name), operands))
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
