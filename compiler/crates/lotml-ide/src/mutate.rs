//! Mutants of a program: each a single span of its text replaced, made the ways agents break
//! programs, to seed failures a guide learns to locate (specs/seeded-failures/).
//!
//! Four families break checking — a name or field misspelt, an annotation swapped for a
//! neighbour, an argument dropped, added or its `?` dropped, a `var` dropped — and one, `meaning`,
//! keeps the program checking while changing what it does. Test blocks are never mutated: the
//! program's own tests are what judge a mutant.

use lotml_check::Types;
use lotml_check::ty::Ty;
use lotml_syntax::ast::{
    Arg, BinOp, Block, BoolOp, CmpOp, Comprehension, Expr, ExprKind, FnDef, Ident, Item, StmtKind, StrPart, TypeExpr,
    TypeKind,
};
use lotml_syntax::span::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mutant {
    pub operator: &'static str,
    /// `names`, `types`, `calls`, `mutability` or `meaning`.
    pub family: &'static str,
    /// The innermost declaration the span falls in: `name`, `Type.method`, or a type's name.
    pub declaration: String,
    pub span: Span,
    pub replacement: String,
}

impl Mutant {
    /// `text` with this mutant's span replaced.
    pub fn apply(&self, text: &str) -> String {
        let range = self.span.range();
        format!("{}{}{}", &text[..range.start], self.replacement, &text[range.end..])
    }
}

/// Every mutant of `text`, in source order, each span and replacement once.
pub fn mutants(text: &str) -> Vec<Mutant> {
    let parsed = lotml_syntax::parse(text);
    let types = lotml_check::check_resolved(&parsed.module, text).types;
    let mut walk = Walk { text, types: &types, declaration: String::new(), found: Vec::new() };
    for item in &parsed.module.items {
        match item {
            Item::Fn(function) => walk.function(function.name.name.clone(), function),
            Item::Impl(imp) => {
                let owner = match &imp.target.kind {
                    TypeKind::Named { name, .. } => name.name.clone(),
                    _ => walk.source(imp.target.span).to_string(),
                };
                for method in &imp.methods {
                    walk.function(format!("{owner}.{}", method.name.name), method);
                }
            }
            Item::Record(record) => {
                walk.declaration.clone_from(&record.name.name);
                for field in &record.fields {
                    walk.annotation(&field.ty, Place::Field);
                }
            }
            Item::Sum(sum) => {
                walk.declaration.clone_from(&sum.name.name);
                for field in sum.variants.iter().filter_map(|v| v.fields.as_ref()).flatten() {
                    walk.annotation(&field.ty, Place::Field);
                }
            }
            Item::Trait(_) | Item::Import(_) | Item::Test(_) | Item::Error(_) => {}
        }
    }
    let mut found = walk.found;
    found.sort_by(|a, b| {
        (a.span.start, a.span.end, &a.replacement, a.operator).cmp(&(
            b.span.start,
            b.span.end,
            &b.replacement,
            b.operator,
        ))
    });
    found.dedup_by(|a, b| a.span == b.span && a.replacement == b.replacement);
    found
}

/// Where an annotation stands, which decides the swaps that break it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Place {
    Param,
    Return,
    Local,
    Field,
}

struct Walk<'t> {
    text: &'t str,
    /// The checker's type of each expression, by span: two arguments swap only when theirs match.
    types: &'t Types,
    declaration: String,
    found: Vec<Mutant>,
}

impl Walk<'_> {
    fn source(&self, span: Span) -> &str {
        &self.text[span.range()]
    }

    /// The `meaning` family's operator swaps: `written`, alone between two operands but for
    /// spaces, replaced. A gap holding anything else — a comment, a line break — is left alone.
    fn operator(&mut self, gap: Span, written: &str, replacement: &str, operator: &'static str) {
        let between = self.source(gap);
        if between.trim() != written {
            return;
        }
        let Some(offset) = between.find(written) else { return };
        let start = gap.start + u32::try_from(offset).unwrap_or(0);
        let span = Span { start, end: start + u32::try_from(written.len()).unwrap_or(0) };
        self.push("meaning", operator, span, replacement.to_string());
    }

    fn numeric(&self, span: Span) -> bool {
        matches!(self.types.get(&span), Some(Ty::Int(_) | Ty::Float(_)))
    }

    fn push(&mut self, family: &'static str, operator: &'static str, span: Span, replacement: String) {
        if self.source(span) != replacement {
            self.found.push(Mutant { operator, family, declaration: self.declaration.clone(), span, replacement });
        }
    }

    fn function(&mut self, declaration: String, function: &FnDef) {
        self.declaration = declaration;
        for param in &function.params {
            if let Some(ty) = &param.ty {
                self.annotation(ty, Place::Param);
            }
            if let Some(default) = &param.default {
                self.expr(default);
            }
        }
        if let Some(returns) = &function.returns {
            self.annotation(returns, Place::Return);
        }
        if let Some(body) = &function.body {
            self.block(body);
        }
    }

    /// The `types` family: `int` and `f64` swapped anywhere in the annotation; a list unwrapped
    /// or wrapped, and an optional dropped; a parameter or local made optional.
    fn annotation(&mut self, ty: &TypeExpr, place: Place) {
        self.numbers(ty);
        match &ty.kind {
            TypeKind::List(item) => self.push("types", "unwrap-list", ty.span, self.source(item.span).to_string()),
            TypeKind::Optional(inner) => {
                self.push("types", "drop-optional", ty.span, self.source(inner.span).to_string());
            }
            _ => {}
        }
        self.push("types", "wrap-list", ty.span, format!("[{}]", self.source(ty.span)));
        if matches!(place, Place::Param | Place::Local) && !matches!(ty.kind, TypeKind::Optional(_)) {
            self.push("types", "add-optional", ty.span, format!("{}?", self.source(ty.span)));
        }
    }

    fn numbers(&mut self, ty: &TypeExpr) {
        match &ty.kind {
            TypeKind::Named { name, args } if args.is_empty() && (name.name == "int" || name.name == "f64") => {
                let other = if name.name == "int" { "f64" } else { "int" };
                self.push("types", "int-f64", ty.span, other.to_string());
            }
            TypeKind::Named { args, .. } => args.iter().for_each(|a| self.numbers(a)),
            TypeKind::List(item) | TypeKind::Set(item) | TypeKind::Optional(item) => self.numbers(item),
            TypeKind::Dict(key, value) => {
                self.numbers(key);
                self.numbers(value);
            }
            TypeKind::Tuple(items) => items.iter().for_each(|i| self.numbers(i)),
            TypeKind::Dyn(_) | TypeKind::Unit | TypeKind::Error => {}
        }
    }

    fn block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            let dropped = match &stmt.kind {
                StmtKind::Expr(_) | StmtKind::AugAssign { .. } => true,
                StmtKind::Assign { target, .. } => !matches!(target.kind, ExprKind::Name(_)),
                _ => false,
            };
            if dropped {
                let span = Span { start: stmt.span.start, end: stmt.end() };
                self.push("meaning", "drop-statement", span, "pass".to_string());
            }
            match &stmt.kind {
                StmtKind::Expr(e) => self.expr(e),
                StmtKind::Var { name, ty, value } => {
                    let keyword = Span { start: stmt.span.start, end: name.span.start };
                    self.push("mutability", "drop-var", keyword, String::new());
                    if let Some(ty) = ty {
                        self.annotation(ty, Place::Local);
                    }
                    self.expr(value);
                }
                StmtKind::Assign { target, value } => {
                    self.target(target);
                    self.expr(value);
                }
                StmtKind::Annotated { ty, value, .. } => {
                    self.annotation(ty, Place::Local);
                    self.expr(value);
                }
                StmtKind::AugAssign { target, value, .. } => {
                    self.expr(target);
                    self.expr(value);
                }
                StmtKind::Return(value) => value.iter().for_each(|v| self.expr(v)),
                StmtKind::Assert { test, message } => {
                    self.expr(test);
                    message.iter().for_each(|m| self.expr(m));
                }
                StmtKind::If { branches, orelse } => {
                    for (condition, body) in branches {
                        self.negate(condition);
                        self.expr(condition);
                        self.block(body);
                    }
                    orelse.iter().for_each(|b| self.block(b));
                }
                StmtKind::While { test, body } => {
                    self.negate(test);
                    self.expr(test);
                    self.block(body);
                }
                StmtKind::For { iter, body, .. } => {
                    self.expr(iter);
                    self.block(body);
                }
                StmtKind::Match { subject, arms } => {
                    self.expr(subject);
                    arms.iter().for_each(|arm| self.block(&arm.body));
                }
                StmtKind::Pass | StmtKind::Break | StmtKind::Continue | StmtKind::Error => {}
            }
        }
    }

    fn negate(&mut self, condition: &Expr) {
        let negated = format!("not ({})", self.source(condition.span));
        self.push("meaning", "negate-condition", condition.span, negated);
    }

    /// An assignment's target: a bare name declares or rebinds and is left as written; the
    /// names inside a field or element target are uses.
    fn target(&mut self, target: &Expr) {
        match &target.kind {
            ExprKind::Name(_) => {}
            ExprKind::Tuple(items) => items.iter().for_each(|i| self.target(i)),
            _ => self.expr(target),
        }
    }

    fn expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Name(name) => {
                if name != "self" {
                    self.push("names", "misspell-name", expr.span, misspelt(name));
                }
            }
            ExprKind::Str(literals) => {
                for part in literals.iter().flat_map(|l| &l.parts) {
                    self.part(part);
                }
            }
            ExprKind::Tuple(items) | ExprKind::List(items) | ExprKind::Set(items) => {
                items.iter().for_each(|i| self.expr(i));
            }
            ExprKind::Dict(pairs) => {
                for (key, value) in pairs {
                    self.expr(key);
                    self.expr(value);
                }
            }
            ExprKind::ListComp { element, loops }
            | ExprKind::SetComp { element, loops }
            | ExprKind::Generator { element, loops } => {
                self.expr(element);
                self.loops(loops);
            }
            ExprKind::DictComp { key, value, loops } => {
                self.expr(key);
                self.expr(value);
                self.loops(loops);
            }
            ExprKind::Unary { operand, .. } => self.expr(operand),
            ExprKind::Binary { op, left, right } => {
                let gap = Span { start: left.span.end, end: right.span.start };
                match op {
                    BinOp::Add if self.numeric(expr.span) => self.operator(gap, "+", "-", "swap-arithmetic"),
                    BinOp::Sub if self.numeric(expr.span) => self.operator(gap, "-", "+", "swap-arithmetic"),
                    _ => {}
                }
                self.expr(left);
                self.expr(right);
            }
            ExprKind::Compare { first, rest } => {
                let mut previous = first.span;
                for (op, operand) in rest {
                    let swapped = match op {
                        CmpOp::Lt => Some(CmpOp::Le),
                        CmpOp::Le => Some(CmpOp::Lt),
                        CmpOp::Gt => Some(CmpOp::Ge),
                        CmpOp::Ge => Some(CmpOp::Gt),
                        CmpOp::Eq => Some(CmpOp::NotEq),
                        CmpOp::NotEq => Some(CmpOp::Eq),
                        _ => None,
                    };
                    if let Some(swapped) = swapped {
                        let gap = Span { start: previous.end, end: operand.span.start };
                        self.operator(gap, op.text(), swapped.text(), "swap-comparison");
                    }
                    previous = operand.span;
                }
                self.expr(first);
                rest.iter().for_each(|(_, e)| self.expr(e));
            }
            ExprKind::Logical { op, operands } => {
                let (written, other) = match op {
                    BoolOp::And => ("and", "or"),
                    BoolOp::Or => ("or", "and"),
                };
                for pair in operands.windows(2) {
                    let gap = Span { start: pair[0].span.end, end: pair[1].span.start };
                    self.operator(gap, written, other, "swap-logical");
                }
                operands.iter().for_each(|o| self.expr(o));
            }
            ExprKind::Not(inner) | ExprKind::Fail(inner) => self.expr(inner),
            ExprKind::Coalesce { value, default } => {
                self.expr(value);
                self.expr(default);
            }
            ExprKind::IfExp { test, then, orelse } => {
                self.expr(test);
                self.expr(then);
                self.expr(orelse);
            }
            ExprKind::Lambda { body, .. } => self.expr(body),
            ExprKind::Call { func, args } => {
                self.arguments(args);
                self.swaps(args);
                if matches!(&func.kind, ExprKind::Name(name) if name == "range") {
                    self.bound(args);
                }
                self.expr(func);
                args.iter().for_each(|a| self.expr(a.expr()));
            }
            ExprKind::Index { object, index } => {
                self.expr(object);
                self.expr(index);
            }
            ExprKind::Slice { object, lower, upper, step } => {
                self.expr(object);
                for bound in [lower, upper, step].into_iter().flatten() {
                    self.expr(bound);
                }
            }
            ExprKind::Attr { object, name } => {
                self.field(name);
                self.expr(object);
            }
            ExprKind::Try(inner) => {
                let mark = Span { start: inner.span.end, end: expr.span.end };
                if self.source(mark).trim() == "?" {
                    self.push("calls", "drop-try", mark, String::new());
                }
                self.expr(inner);
            }
            ExprKind::Int(written) => {
                let digits = written.replace('_', "");
                if let Some(next) = digits.parse::<i64>().ok().and_then(|n| n.checked_add(1)) {
                    self.push("meaning", "change-constant", expr.span, next.to_string());
                }
            }
            ExprKind::Bool(value) => {
                let other = if *value { "False" } else { "True" };
                self.push("meaning", "change-constant", expr.span, other.to_string());
            }
            ExprKind::Float(_) | ExprKind::None | ExprKind::Unit | ExprKind::Error => {}
        }
    }

    fn part(&mut self, part: &StrPart) {
        if let StrPart::Expr { expr, spec, .. } = part {
            self.expr(expr);
            spec.iter().for_each(|p| self.part(p));
        }
    }

    fn loops(&mut self, loops: &[Comprehension]) {
        for comprehension in loops {
            self.expr(&comprehension.iter);
            comprehension.conditions.iter().for_each(|c| self.expr(c));
        }
    }

    fn field(&mut self, name: &Ident) {
        self.push("names", "misspell-field", name.span, misspelt(&name.name));
    }

    /// The `calls` family: each argument dropped with its comma, and the last one passed twice.
    fn arguments(&mut self, args: &[Arg]) {
        let spans: Vec<Span> = args.iter().map(argument_span).collect();
        for (i, &span) in spans.iter().enumerate() {
            let dropped = if spans.len() == 1 {
                span
            } else if i + 1 < spans.len() {
                Span { start: span.start, end: spans[i + 1].start }
            } else {
                Span { start: spans[i - 1].end, end: span.end }
            };
            self.push("calls", "drop-argument", dropped, String::new());
        }
        if let Some(&last) = spans.last() {
            let after = Span { start: last.end, end: last.end };
            self.push("calls", "duplicate-argument", after, format!(", {}", self.source(last)));
        }
    }
}

impl Walk<'_> {
    /// Two neighbouring positional arguments of one type, swapped: DeepBugs' swapped arguments,
    /// which still check and pass the wrong value.
    fn swaps(&mut self, args: &[Arg]) {
        for pair in args.windows(2) {
            let (Arg::Positional(a), Arg::Positional(b)) = (&pair[0], &pair[1]) else { continue };
            let same = matches!((self.types.get(&a.span), self.types.get(&b.span)), (Some(x), Some(y)) if x == y);
            if same {
                let between = self.source(Span { start: a.span.end, end: b.span.start }).to_string();
                let swapped = format!("{}{between}{}", self.source(b.span), self.source(a.span));
                self.push("meaning", "swap-arguments", a.span.to(b.span), swapped);
            }
        }
    }

    /// A `range`'s stop moved by one either way.
    fn bound(&mut self, args: &[Arg]) {
        let stop = match args {
            [Arg::Positional(stop)] | [Arg::Positional(_), Arg::Positional(stop), ..] => stop,
            _ => return,
        };
        let written = self.source(stop.span);
        let atom = matches!(
            stop.kind,
            ExprKind::Name(_)
                | ExprKind::Int(_)
                | ExprKind::Call { .. }
                | ExprKind::Attr { .. }
                | ExprKind::Index { .. }
        );
        let shown = if atom { written.to_string() } else { format!("({written})") };
        self.push("meaning", "move-bound", stop.span, format!("{shown} + 1"));
        self.push("meaning", "move-bound", stop.span, format!("{shown} - 1"));
    }
}

fn argument_span(arg: &Arg) -> Span {
    match arg {
        Arg::Positional(e) => e.span,
        Arg::Keyword(name, e) => name.span.to(e.span),
        Arg::Inout(e, amp) => amp.to(e.span),
    }
}

/// A name one edit away, the way an agent confuses one with its plural: `xs` and `x`,
/// `count` and `counts`.
fn misspelt(name: &str) -> String {
    match name.strip_suffix('s') {
        Some(stem) if !stem.is_empty() => stem.to_string(),
        _ => format!("{name}s"),
    }
}
