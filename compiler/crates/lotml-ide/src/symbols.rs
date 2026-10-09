//! What each name in a file refers to. Functions, types, traits and variants are known by name,
//! fields and methods by the type that declares them, and locals by where they were declared, as
//! the checker resolved them. Definitions, references and renames are all questions about these
//! occurrences.

use std::collections::{HashMap, HashSet};

use lotml_check::Checked;
use lotml_check::ty::Ty;
use lotml_syntax::ast::*;
use lotml_syntax::span::Span;

/// Something a name can refer to.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Symbol {
    /// A function, type or trait declared at the top of the file.
    Item(String),
    /// A variant of a sum type.
    Variant(String),
    /// A field or method of a type or variant, or a method of a trait: `Counter.get`.
    Member(String, String),
    /// A local, parameter or binding, by the span of the name that declared it.
    Local(Span),
}

/// One name in the text, and what it refers to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Occurrence {
    pub span: Span,
    pub symbol: Symbol,
    /// Whether this is where the symbol is declared.
    pub declaration: bool,
}

/// Every name in a module that refers to something it declares, in source order. Names of the
/// prelude, of imported modules and of builtin methods declare nothing here and are left out.
pub fn occurrences(module: &Module, checked: &Checked) -> Vec<Occurrence> {
    let mut walk = Walk {
        checked,
        locals: checked.locals.iter().map(|(at, _)| *at).collect(),
        declared: declare(module),
        type_params: Vec::new(),
        trait_self: None,
        out: Vec::new(),
    };
    for (at, to) in &checked.locals {
        walk.out.push(Occurrence { span: *at, symbol: Symbol::Local(*to), declaration: at == to });
    }
    for item in &module.items {
        walk.item(item);
    }
    let mut out = walk.out;
    out.sort_by(|a, b| (a.span.start, a.span.end, &a.symbol).cmp(&(b.span.start, b.span.end, &b.symbol)));
    out.dedup();
    out
}

/// What a module declares, collected before any body is walked, since a name may be used above
/// its declaration.
#[derive(Default)]
struct Declared {
    /// Functions, types and traits.
    items: HashSet<String>,
    variants: HashSet<String>,
    /// Fields and methods, each with the type, variant or trait declaring it.
    members: HashSet<(String, String)>,
    /// The parameters of each function and method, with the span of each one's name.
    params: HashMap<Symbol, Vec<(String, Span)>>,
}

fn declare(module: &Module) -> Declared {
    let mut d = Declared::default();
    let params = |f: &FnDef| f.params.iter().map(|p| (p.name.name.clone(), p.name.span)).collect::<Vec<_>>();
    for item in &module.items {
        match item {
            Item::Fn(f) => {
                d.items.insert(f.name.name.clone());
                d.params.insert(Symbol::Item(f.name.name.clone()), params(f));
            }
            Item::Record(r) => {
                d.items.insert(r.name.name.clone());
                for name in r.fields.iter().filter_map(|f| f.name.as_ref()) {
                    d.members.insert((r.name.name.clone(), name.name.clone()));
                }
            }
            Item::Sum(s) => {
                d.items.insert(s.name.name.clone());
                for v in &s.variants {
                    d.variants.insert(v.name.name.clone());
                    for name in v.fields.iter().flatten().filter_map(|f| f.name.as_ref()) {
                        d.members.insert((v.name.name.clone(), name.name.clone()));
                    }
                }
            }
            Item::Trait(t) => {
                d.items.insert(t.name.name.clone());
                for m in &t.methods {
                    d.members.insert((t.name.name.clone(), m.name.name.clone()));
                    d.params.insert(Symbol::Member(t.name.name.clone(), m.name.name.clone()), params(m));
                }
            }
            Item::Impl(imp) => {
                let Some(owner) = named(&imp.target) else { continue };
                for m in &imp.methods {
                    d.members.insert((owner.name.clone(), m.name.name.clone()));
                    d.params.insert(Symbol::Member(owner.name.clone(), m.name.name.clone()), params(m));
                }
            }
            Item::Import(_) | Item::Test(_) | Item::Class(_) | Item::Error(_) => {}
        }
    }
    d
}

/// The type an `impl` is for: `Stack` in `impl Stack[T]`.
pub(crate) fn named(t: &TypeExpr) -> Option<&Ident> {
    match &t.kind {
        TypeKind::Named { name, .. } => Some(name),
        _ => None,
    }
}

struct Walk<'a> {
    checked: &'a Checked,
    /// The names the checker resolved to locals.
    locals: HashSet<Span>,
    declared: Declared,
    /// The type parameters in scope, which shadow no declared type but are not one either.
    type_params: Vec<String>,
    /// Inside a trait, the trait that `Self` stands for.
    trait_self: Option<String>,
    out: Vec<Occurrence>,
}

impl Walk<'_> {
    fn push(&mut self, span: Span, symbol: Symbol, declaration: bool) {
        self.out.push(Occurrence { span, symbol, declaration });
    }

    fn item(&mut self, item: &Item) {
        match item {
            Item::Fn(f) => {
                self.push(f.name.span, Symbol::Item(f.name.name.clone()), true);
                self.function(f);
            }
            Item::Record(r) => {
                self.push(r.name.span, Symbol::Item(r.name.name.clone()), true);
                let saved = self.enter(&r.type_params);
                self.fields(&r.name.name, &r.fields);
                self.type_params.truncate(saved);
            }
            Item::Sum(s) => {
                self.push(s.name.span, Symbol::Item(s.name.name.clone()), true);
                let saved = self.enter(&s.type_params);
                for v in &s.variants {
                    self.push(v.name.span, Symbol::Variant(v.name.name.clone()), true);
                    self.fields(&v.name.name, v.fields.as_deref().unwrap_or_default());
                }
                self.type_params.truncate(saved);
            }
            Item::Trait(t) => {
                self.push(t.name.span, Symbol::Item(t.name.name.clone()), true);
                let saved = self.enter(&t.type_params);
                self.trait_self = Some(t.name.name.clone());
                for m in &t.methods {
                    self.push(m.name.span, Symbol::Member(t.name.name.clone(), m.name.name.clone()), true);
                    self.function(m);
                }
                self.trait_self = None;
                self.type_params.truncate(saved);
            }
            Item::Impl(imp) => {
                // The impl's type parameters are the bare names among its target's arguments.
                let saved = self.type_params.len();
                if let TypeKind::Named { args, .. } = &imp.target.kind {
                    for arg in args {
                        if let TypeKind::Named { name, args } = &arg.kind
                            && args.is_empty()
                            && !self.declared.items.contains(&name.name)
                        {
                            self.type_params.push(name.name.clone());
                        }
                    }
                }
                if let Some(t) = &imp.trait_name {
                    self.ty(t);
                }
                self.ty(&imp.target);
                let owner = named(&imp.target).map(|n| n.name.clone());
                for m in &imp.methods {
                    if let Some(owner) = &owner {
                        self.push(m.name.span, Symbol::Member(owner.clone(), m.name.name.clone()), true);
                    }
                    self.function(m);
                }
                self.type_params.truncate(saved);
            }
            Item::Test(t) => self.block(&t.body),
            Item::Import(_) | Item::Class(_) | Item::Error(_) => {}
        }
    }

    /// Bring type parameters into scope, returning what to truncate back to.
    fn enter(&mut self, params: &[TypeParam]) -> usize {
        let saved = self.type_params.len();
        for p in params {
            self.type_params.push(p.name.name.clone());
            if let Some(bound) = &p.bound {
                self.item_use(bound);
            }
        }
        saved
    }

    fn fields(&mut self, owner: &str, fields: &[Field]) {
        for field in fields {
            if let Some(name) = &field.name {
                self.push(name.span, Symbol::Member(owner.to_string(), name.name.clone()), true);
            }
            self.ty(&field.ty);
            if let Some(default) = &field.default {
                self.expr(default);
            }
        }
    }

    fn function(&mut self, f: &FnDef) {
        let saved = self.enter(&f.type_params);
        for p in &f.params {
            if let Some(t) = &p.ty {
                self.ty(t);
            }
            if let Some(default) = &p.default {
                self.expr(default);
            }
        }
        for t in f.returns.iter().chain(&f.error) {
            self.ty(t);
        }
        if let Some(body) = &f.body {
            self.block(body);
        }
        self.type_params.truncate(saved);
    }

    /// A name that refers to a function, type or trait, when this module declares one.
    fn item_use(&mut self, name: &Ident) {
        if self.declared.items.contains(&name.name) {
            self.push(name.span, Symbol::Item(name.name.clone()), false);
        }
    }

    fn ty(&mut self, t: &TypeExpr) {
        match &t.kind {
            TypeKind::Named { name, args } => {
                if !self.type_params.contains(&name.name) {
                    self.item_use(name);
                }
                for arg in args {
                    self.ty(arg);
                }
            }
            TypeKind::List(inner) | TypeKind::Set(inner) | TypeKind::Optional(inner) => self.ty(inner),
            TypeKind::Dict(key, value) => {
                self.ty(key);
                self.ty(value);
            }
            TypeKind::Tuple(items) => {
                for item in items {
                    self.ty(item);
                }
            }
            TypeKind::Dyn(name) => self.item_use(name),
            TypeKind::Unit | TypeKind::Error => {}
        }
    }

    fn block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.stmt(stmt);
        }
    }

    fn stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Expr(e) | StmtKind::Return(Some(e)) => self.expr(e),
            StmtKind::Var { ty, value, .. } => {
                if let Some(t) = ty {
                    self.ty(t);
                }
                self.expr(value);
            }
            StmtKind::Assign { target, value } | StmtKind::AugAssign { target, value, .. } => {
                self.expr(target);
                self.expr(value);
            }
            StmtKind::Annotated { ty, value, .. } => {
                self.ty(ty);
                self.expr(value);
            }
            StmtKind::Assert { test, message } => {
                self.expr(test);
                if let Some(m) = message {
                    self.expr(m);
                }
            }
            StmtKind::If { branches, orelse } => {
                for (test, body) in branches {
                    self.expr(test);
                    self.block(body);
                }
                if let Some(body) = orelse {
                    self.block(body);
                }
            }
            StmtKind::While { test, body } => {
                self.expr(test);
                self.block(body);
            }
            StmtKind::For { iter, body, .. } => {
                self.expr(iter);
                self.block(body);
            }
            StmtKind::Match { subject, arms } => {
                self.expr(subject);
                for arm in arms {
                    self.pattern(&arm.pattern);
                    self.block(&arm.body);
                }
            }
            StmtKind::Return(None) | StmtKind::Pass | StmtKind::Break | StmtKind::Continue | StmtKind::Error => {}
        }
    }

    fn pattern(&mut self, pattern: &Pattern) {
        match &pattern.kind {
            // A name that is not a variant binds a local, which the checker reported.
            PatternKind::Name(name) => {
                if self.declared.variants.contains(&name.name) {
                    self.push(name.span, Symbol::Variant(name.name.clone()), false);
                }
            }
            PatternKind::Variant { name, args } => {
                if self.declared.variants.contains(&name.name) {
                    self.push(name.span, Symbol::Variant(name.name.clone()), false);
                }
                for arg in args {
                    self.pattern(arg);
                }
            }
            PatternKind::Tuple(items) => {
                for item in items {
                    self.pattern(item);
                }
            }
            PatternKind::Literal(e) => self.expr(e),
            PatternKind::Wildcard | PatternKind::Error => {}
        }
    }

    fn exprs<'e>(&mut self, exprs: impl IntoIterator<Item = &'e Expr>) {
        for e in exprs {
            self.expr(e);
        }
    }

    fn loops(&mut self, loops: &[Comprehension]) {
        for l in loops {
            self.expr(&l.iter);
            self.exprs(&l.conditions);
        }
    }

    fn str_part(&mut self, part: &StrPart) {
        if let StrPart::Expr { expr, spec, .. } = part {
            self.expr(expr);
            for p in spec {
                self.str_part(p);
            }
        }
    }

    fn expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Name(name) => {
                if let Some(symbol) = self.global(name, expr.span) {
                    self.push(expr.span, symbol, false);
                }
            }
            ExprKind::Str(literals) => {
                for part in literals.iter().flat_map(|l| &l.parts) {
                    self.str_part(part);
                }
            }
            ExprKind::Tuple(items) | ExprKind::List(items) | ExprKind::Set(items) => self.exprs(items),
            ExprKind::Dict(pairs) => {
                for (k, v) in pairs {
                    self.expr(k);
                    self.expr(v);
                }
            }
            ExprKind::ListComp { element, loops }
            | ExprKind::SetComp { element, loops }
            | ExprKind::Generator { element, loops } => {
                self.loops(loops);
                self.expr(element);
            }
            ExprKind::DictComp { key, value, loops } => {
                self.loops(loops);
                self.expr(key);
                self.expr(value);
            }
            ExprKind::Unary { operand: inner, .. }
            | ExprKind::Not(inner)
            | ExprKind::Try(inner)
            | ExprKind::Fail(inner)
            | ExprKind::Lambda { body: inner, .. } => self.expr(inner),
            ExprKind::Binary { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            ExprKind::Compare { first, rest } => {
                self.expr(first);
                self.exprs(rest.iter().map(|(_, e)| e));
            }
            ExprKind::Logical { operands, .. } => self.exprs(operands),
            ExprKind::Coalesce { value, default } => {
                self.expr(value);
                self.expr(default);
            }
            ExprKind::IfExp { test, then, orelse } => {
                self.expr(test);
                self.expr(then);
                self.expr(orelse);
            }
            ExprKind::Call { func, args } => {
                self.expr(func);
                let callee = self.callee(func);
                for arg in args {
                    if let Arg::Keyword(name, _) = arg {
                        self.keyword(callee.as_ref(), name);
                    }
                    self.expr(arg.expr());
                }
            }
            ExprKind::Index { object, index } => {
                self.expr(object);
                self.expr(index);
            }
            ExprKind::Slice { object, lower, upper, step } => {
                self.expr(object);
                self.exprs([lower, upper, step].into_iter().flatten().map(|e| &**e));
            }
            ExprKind::Attr { object, name } => {
                self.expr(object);
                if let Some(owner) = self.owner(object)
                    && self.declared.members.contains(&(owner.clone(), name.name.clone()))
                {
                    self.push(name.span, Symbol::Member(owner, name.name.clone()), false);
                }
            }
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::None
            | ExprKind::Unit
            | ExprKind::Error => {}
        }
    }

    /// What a name that is not a local refers to.
    fn global(&self, name: &str, span: Span) -> Option<Symbol> {
        if self.locals.contains(&span) {
            None
        } else if self.declared.items.contains(name) {
            Some(Symbol::Item(name.to_string()))
        } else if self.declared.variants.contains(name) {
            Some(Symbol::Variant(name.to_string()))
        } else {
            None
        }
    }

    /// The type, variant or trait whose members `object` has, by its checked type.
    fn owner(&self, object: &Expr) -> Option<String> {
        match self.checked.types.get(&object.span)? {
            Ty::Adt(name, _) | Ty::TypeName(name) | Ty::Dyn(name) => Some(name.clone()),
            Ty::Param(p) if p == "Self" => self.trait_self.clone(),
            _ => None,
        }
    }

    /// What a call calls, when the module declares it.
    fn callee(&self, func: &Expr) -> Option<Symbol> {
        match &func.kind {
            ExprKind::Name(name) => self.global(name, func.span),
            ExprKind::Attr { object, name } => self.owner(object).map(|o| Symbol::Member(o, name.name.clone())),
            _ => None,
        }
    }

    /// A keyword argument: a parameter of the function called, or a field of the record or
    /// variant built.
    fn keyword(&mut self, callee: Option<&Symbol>, name: &Ident) {
        let Some(callee) = callee else { return };
        if let Some(params) = self.declared.params.get(callee) {
            if let Some((_, declared)) = params.iter().find(|(p, _)| *p == name.name) {
                self.push(name.span, Symbol::Local(*declared), false);
            }
            return;
        }
        let owner = match callee {
            Symbol::Item(owner) | Symbol::Variant(owner) => owner,
            Symbol::Member(..) | Symbol::Local(_) => return,
        };
        if self.declared.members.contains(&(owner.clone(), name.name.clone())) {
            self.push(name.span, Symbol::Member(owner.clone(), name.name.clone()), false);
        }
    }
}
