//! Lowering: the checked syntax tree to the intermediate form, every call resolved and every
//! intermediate value named (specs/c-backend/design.md).

use std::collections::HashMap;

use lotml_check::Checked;
use lotml_check::ty::{IntKind, Ty};
use lotml_diag::Diagnostic;
use lotml_syntax::ast::{self, Arg, BoolOp, ExprKind, FnDef, Item, Module, StmtKind as Ast, StrPart};
use lotml_syntax::span::Span;

use crate::mir::{
    BinOp, Block, CmpOp, Const, Expr, FormatPart, Function, Local, LocalInfo, Operand, Panic, Stmt, StmtKind, UnOp,
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

impl<'a> Builder<'_, 'a> {
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

    fn block(&mut self, f: impl FnOnce(&mut Self)) -> Block {
        self.blocks.push(Vec::new());
        f(self);
        self.blocks.pop().expect("the block pushed")
    }

    fn ty(&self, e: &ast::Expr) -> Ty {
        self.cx.checked.types.get(&e.span).cloned().unwrap_or(Ty::Error)
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
                let ty = self.cx.checked.types.get(&value.span).cloned().unwrap_or(Ty::Error);
                let declared = self.var_type(name.span).unwrap_or(ty);
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
            Ast::Assign { target, value } => self.assign(target, value),
            Ast::AugAssign { target, op, value } => {
                let ExprKind::Name(_) = &target.kind else {
                    self.unsupported(target.span, "this assignment target");
                    return;
                };
                let Some(local) = self.local_of(target.span) else {
                    self.unsupported(target.span, "this assignment target");
                    return;
                };
                let ty = self.function.locals[local].ty.clone();
                let right = self.value(value);
                let op = binop(*op);
                self.push(StmtKind::Let(local, Expr::Binary(op, Operand::Local(local), right, ty)));
            }
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
            Ast::For { target, iter, body } => self.for_loop(target, iter, body),
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

    fn assign(&mut self, target: &ast::Expr, value: &ast::Expr) {
        let ExprKind::Name(name) = &target.kind else {
            self.unsupported(target.span, "this assignment target");
            return;
        };
        let value_operand = self.value(value);
        let local = match self.local_of(target.span) {
            Some(local) => local,
            None => {
                let decl = self.cx.resolved.get(&target.span).copied().unwrap_or(target.span);
                let ty = self.ty(value);
                self.declare(decl, name, ty)
            }
        };
        self.push(StmtKind::Let(local, Expr::Use(value_operand)));
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

    fn for_loop(&mut self, target: &ast::Target, iter: &ast::Expr, body: &ast::Block) {
        let ast::Target::Name(name) = target else {
            self.unsupported(target.span(), "this loop target");
            return;
        };
        let ExprKind::Call { func, args } = &iter.kind else {
            self.unsupported(iter.span, "a loop over this value");
            return;
        };
        if !matches!(&func.kind, ExprKind::Name(n) if n == "range") || self.local_of(func.span).is_some() {
            self.unsupported(iter.span, "a loop over this value");
            return;
        }
        let values: Vec<Operand> = args.iter().map(|a| self.value(a.expr())).collect();
        let int = |v: i128| Operand::Const(Const::Int(v, IntKind::I64));
        let (start, stop, step) = match values.as_slice() {
            [stop] => (int(0), stop.clone(), int(1)),
            [start, stop] => (start.clone(), stop.clone(), int(1)),
            [start, stop, step] => (start.clone(), stop.clone(), step.clone()),
            _ => {
                self.unsupported(iter.span, "this `range`");
                return;
            }
        };
        let var = self.declare(name.span, &name.name, Ty::Int(IntKind::I64));
        let line = self.line;
        let body = self.block(|b| b.statements(&body.stmts));
        self.line = line;
        self.push(StmtKind::ForRange { var, start, stop, step, body });
    }

    // Expressions -----------------------------------------------------------------------

    /// Evaluate `e` for its effect.
    fn effect(&mut self, e: &ast::Expr) {
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
                let t = self.temp(ty);
                self.push(StmtKind::Let(t, expr));
                Operand::Local(t)
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
            ExprKind::Binary { op, left, right } => {
                let ty = self.ty(left);
                let right_ty = self.ty(right);
                let a = self.value(left);
                let b = self.value(right);
                match (op, &ty, &right_ty) {
                    (ast::BinOp::Add, Ty::Str, _) => Value::Expr(rt("lt_str_concat", vec![a, b], false)),
                    (ast::BinOp::Mul, Ty::Str, _) => Value::Expr(rt("lt_str_repeat", vec![a, b], true)),
                    (ast::BinOp::Mul, _, Ty::Str) => Value::Expr(rt("lt_str_repeat", vec![b, a], true)),
                    _ => Value::Expr(Expr::Binary(binop(*op), a, b, ty)),
                }
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
                let o = self.value(object);
                let i = self.value(index);
                match ty {
                    Ty::Str => Value::Expr(rt("lt_str_index", vec![o, i], true)),
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
                            args.push(Operand::Const(Const::Bool(true)));
                            args.push(v);
                        }
                        None => {
                            args.push(Operand::Const(Const::Bool(false)));
                            args.push(Operand::Const(Const::Int(0, IntKind::I64)));
                        }
                    }
                }
                match ty {
                    Ty::Str => Value::Expr(rt("lt_str_slice", args, true)),
                    _ => Value::Done(self.unsupported(e.span, "slicing this value")),
                }
            }
            _ => Value::Done(self.unsupported(e.span, "this expression")),
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
                    let t = self.temp(Ty::Bool);
                    self.push(StmtKind::Let(t, contains));
                    Expr::Unary(UnOp::Not, Operand::Local(t), Ty::Bool)
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

    fn call(&mut self, whole: &ast::Expr, func: &ast::Expr, args: &[Arg]) -> Value {
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
            args.iter().filter_map(|a| if let Arg::Positional(e) = a { Some(e) } else { None }).collect();
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
                    if let Arg::Keyword(key, e) = a {
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
            ("min" | "max", [a, b]) if self.ty(a).is_numeric() => {
                let ty = self.ty(whole);
                let a = self.value(a);
                let b = self.value(b);
                Value::Expr(Expr::MinMax { max: name == "max", a, b, ty })
            }
            ("len", [x]) => {
                let ty = self.ty(x);
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
                let to = self.ty(whole);
                let v = self.value(x);
                Value::Expr(Expr::Convert(v, from, to))
            }
            _ => Value::Done(self.unsupported(whole.span, &format!("`{name}` here"))),
        }
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

    /// `object.name(args)`: a method of a built-in type.
    fn method(&mut self, whole: &ast::Expr, object: &ast::Expr, name: &str, args: &[Arg]) -> Value {
        let ty = self.ty(object);
        if ty != Ty::Str {
            return Value::Done(self.unsupported(whole.span, &format!("the method `{name}` here")));
        }
        let receiver = self.value(object);
        let mut values = Vec::new();
        for a in args {
            match a {
                Arg::Positional(e) => values.push(self.value(e)),
                _ => return Value::Done(self.unsupported(whole.span, &format!("these arguments of `{name}`"))),
            }
        }
        let null = Operand::Const(Const::Null);
        let flag = |b: bool| Operand::Const(Const::Bool(b));
        let int = |v: i128| Operand::Const(Const::Int(v, IntKind::I64));
        let mut all = vec![receiver];
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
            _ => return Value::Done(self.unsupported(whole.span, &format!("the method `{name}` here"))),
        };
        Value::Expr(call)
    }

    fn call_function(&mut self, name: &str, f: &'a FnDef, args: &[Arg]) -> Value {
        let mut slots: Vec<Option<&ast::Expr>> = vec![None; f.params.len()];
        let mut next = 0;
        for a in args {
            match a {
                Arg::Positional(e) => {
                    if next < slots.len() {
                        slots[next] = Some(e);
                    }
                    next += 1;
                }
                Arg::Keyword(key, e) => {
                    if let Some(i) = f.params.iter().position(|p| p.name.name == key.name) {
                        slots[i] = Some(e);
                    }
                }
                Arg::Inout(e, _) => return Value::Done(self.unsupported(e.span, "an `inout` argument")),
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

fn rt(name: &'static str, args: Vec<Operand>, at: bool) -> Expr {
    Expr::Rt { name, args, at }
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
