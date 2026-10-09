//! Moving a node of the syntax tree to another place in the text: every span it holds moves by
//! the same distance, so an item read at one position equals the same item read at another once
//! both are moved to one start (specs/incremental-check/R1.2).

use crate::ast::*;
use crate::span::Span;

/// A node whose every span can be moved from one position to another.
pub trait Shift {
    /// Move every span by `to - from`: a span at `from` ends up at `to`. Moving back with the
    /// arguments swapped gives the node it was.
    fn shift(&mut self, from: u32, to: u32);
}

impl Span {
    /// This span moved by `to - from`.
    pub fn shifted(self, from: u32, to: u32) -> Span {
        let at = |n: u32| n.wrapping_sub(from).wrapping_add(to);
        Span { start: at(self.start), end: at(self.end) }
    }
}

impl Shift for Span {
    fn shift(&mut self, from: u32, to: u32) {
        *self = self.shifted(from, to);
    }
}

impl<T: Shift> Shift for Vec<T> {
    fn shift(&mut self, from: u32, to: u32) {
        for node in self {
            node.shift(from, to);
        }
    }
}

impl<T: Shift> Shift for Option<T> {
    fn shift(&mut self, from: u32, to: u32) {
        if let Some(node) = self {
            node.shift(from, to);
        }
    }
}

impl<T: Shift> Shift for Box<T> {
    fn shift(&mut self, from: u32, to: u32) {
        (**self).shift(from, to);
    }
}

impl<A: Shift, B: Shift> Shift for (A, B) {
    fn shift(&mut self, from: u32, to: u32) {
        self.0.shift(from, to);
        self.1.shift(from, to);
    }
}

impl Shift for Module {
    fn shift(&mut self, from: u32, to: u32) {
        let Module { items } = self;
        items.shift(from, to);
    }
}

impl Shift for Ident {
    fn shift(&mut self, from: u32, to: u32) {
        let Ident { name: _, span } = self;
        span.shift(from, to);
    }
}

impl Shift for Item {
    fn shift(&mut self, from: u32, to: u32) {
        match self {
            Item::Fn(f) => f.shift(from, to),
            Item::Record(r) => r.shift(from, to),
            Item::Sum(s) => s.shift(from, to),
            Item::Impl(i) => i.shift(from, to),
            Item::Trait(t) => t.shift(from, to),
            Item::Import(i) => i.shift(from, to),
            Item::Test(t) => t.shift(from, to),
            Item::Class(c) => c.shift(from, to),
            Item::Error(span) => span.shift(from, to),
        }
    }
}

impl Shift for FnDef {
    fn shift(&mut self, from: u32, to: u32) {
        let FnDef { span, name, type_params, params, returns, error, body, doc: _ } = self;
        span.shift(from, to);
        name.shift(from, to);
        type_params.shift(from, to);
        params.shift(from, to);
        returns.shift(from, to);
        error.shift(from, to);
        body.shift(from, to);
    }
}

impl Shift for TypeParam {
    fn shift(&mut self, from: u32, to: u32) {
        let TypeParam { name, bound } = self;
        name.shift(from, to);
        bound.shift(from, to);
    }
}

impl Shift for Param {
    fn shift(&mut self, from: u32, to: u32) {
        let Param { span, convention: _, name, ty, default } = self;
        span.shift(from, to);
        name.shift(from, to);
        ty.shift(from, to);
        default.shift(from, to);
    }
}

impl Shift for RecordDef {
    fn shift(&mut self, from: u32, to: u32) {
        let RecordDef { span, name, type_params, fields } = self;
        span.shift(from, to);
        name.shift(from, to);
        type_params.shift(from, to);
        fields.shift(from, to);
    }
}

impl Shift for Field {
    fn shift(&mut self, from: u32, to: u32) {
        let Field { span, name, ty, default } = self;
        span.shift(from, to);
        name.shift(from, to);
        ty.shift(from, to);
        default.shift(from, to);
    }
}

impl Shift for SumDef {
    fn shift(&mut self, from: u32, to: u32) {
        let SumDef { span, name, type_params, variants } = self;
        span.shift(from, to);
        name.shift(from, to);
        type_params.shift(from, to);
        variants.shift(from, to);
    }
}

impl Shift for Variant {
    fn shift(&mut self, from: u32, to: u32) {
        let Variant { span, name, fields } = self;
        span.shift(from, to);
        name.shift(from, to);
        fields.shift(from, to);
    }
}

impl Shift for ImplDef {
    fn shift(&mut self, from: u32, to: u32) {
        let ImplDef { span, trait_name, target, methods } = self;
        span.shift(from, to);
        trait_name.shift(from, to);
        target.shift(from, to);
        methods.shift(from, to);
    }
}

impl Shift for ClassDef {
    fn shift(&mut self, from: u32, to: u32) {
        let ClassDef { span, name, bases, attributes, methods } = self;
        span.shift(from, to);
        name.shift(from, to);
        bases.shift(from, to);
        attributes.shift(from, to);
        methods.shift(from, to);
    }
}

impl Shift for TraitDef {
    fn shift(&mut self, from: u32, to: u32) {
        let TraitDef { span, name, type_params, methods } = self;
        span.shift(from, to);
        name.shift(from, to);
        type_params.shift(from, to);
        methods.shift(from, to);
    }
}

impl Shift for Import {
    fn shift(&mut self, from: u32, to: u32) {
        let Import { span, module, names } = self;
        span.shift(from, to);
        module.shift(from, to);
        names.shift(from, to);
    }
}

impl Shift for TestDef {
    fn shift(&mut self, from: u32, to: u32) {
        let TestDef { span, name: _, name_span, body } = self;
        span.shift(from, to);
        name_span.shift(from, to);
        body.shift(from, to);
    }
}

impl Shift for Block {
    fn shift(&mut self, from: u32, to: u32) {
        let Block { span, stmts } = self;
        span.shift(from, to);
        stmts.shift(from, to);
    }
}

impl Shift for Stmt {
    fn shift(&mut self, from: u32, to: u32) {
        let Stmt { span, kind } = self;
        span.shift(from, to);
        match kind {
            StmtKind::Expr(e) | StmtKind::Return(Some(e)) => e.shift(from, to),
            StmtKind::Var { name, ty, value } => {
                name.shift(from, to);
                ty.shift(from, to);
                value.shift(from, to);
            }
            StmtKind::Assign { target, value } | StmtKind::AugAssign { target, op: _, value } => {
                target.shift(from, to);
                value.shift(from, to);
            }
            StmtKind::Annotated { target, ty, value } => {
                target.shift(from, to);
                ty.shift(from, to);
                value.shift(from, to);
            }
            StmtKind::Assert { test, message } => {
                test.shift(from, to);
                message.shift(from, to);
            }
            StmtKind::If { branches, orelse } => {
                branches.shift(from, to);
                orelse.shift(from, to);
            }
            StmtKind::While { test, body } => {
                test.shift(from, to);
                body.shift(from, to);
            }
            StmtKind::For { target, iter, body } => {
                target.shift(from, to);
                iter.shift(from, to);
                body.shift(from, to);
            }
            StmtKind::Match { subject, arms } => {
                subject.shift(from, to);
                arms.shift(from, to);
            }
            StmtKind::Return(None) | StmtKind::Pass | StmtKind::Break | StmtKind::Continue | StmtKind::Error => {}
        }
    }
}

impl Shift for Target {
    fn shift(&mut self, from: u32, to: u32) {
        match self {
            Target::Name(ident) => ident.shift(from, to),
            Target::Tuple(items, span) => {
                items.shift(from, to);
                span.shift(from, to);
            }
        }
    }
}

impl Shift for Arm {
    fn shift(&mut self, from: u32, to: u32) {
        let Arm { span, pattern, body } = self;
        span.shift(from, to);
        pattern.shift(from, to);
        body.shift(from, to);
    }
}

impl Shift for Pattern {
    fn shift(&mut self, from: u32, to: u32) {
        let Pattern { span, kind } = self;
        span.shift(from, to);
        match kind {
            PatternKind::Name(name) => name.shift(from, to),
            PatternKind::Variant { name, args } => {
                name.shift(from, to);
                args.shift(from, to);
            }
            PatternKind::Tuple(items) => items.shift(from, to),
            PatternKind::Literal(e) => e.shift(from, to),
            PatternKind::Wildcard | PatternKind::Error => {}
        }
    }
}

impl Shift for StrPart {
    fn shift(&mut self, from: u32, to: u32) {
        match self {
            StrPart::Text(_) => {}
            StrPart::Expr { expr, conversion: _, spec } => {
                expr.shift(from, to);
                spec.shift(from, to);
            }
        }
    }
}

impl Shift for StrLit {
    fn shift(&mut self, from: u32, to: u32) {
        let StrLit { raw: _, bytes: _, parts } = self;
        parts.shift(from, to);
    }
}

impl Shift for Comprehension {
    fn shift(&mut self, from: u32, to: u32) {
        let Comprehension { target, iter, conditions } = self;
        target.shift(from, to);
        iter.shift(from, to);
        conditions.shift(from, to);
    }
}

impl Shift for Arg {
    fn shift(&mut self, from: u32, to: u32) {
        match self {
            Arg::Positional(e) => e.shift(from, to),
            Arg::Keyword(name, e) => {
                name.shift(from, to);
                e.shift(from, to);
            }
            Arg::Inout(e, span) => {
                e.shift(from, to);
                span.shift(from, to);
            }
        }
    }
}

impl Shift for CmpOp {
    fn shift(&mut self, _: u32, _: u32) {}
}

impl Shift for Expr {
    fn shift(&mut self, from: u32, to: u32) {
        let Expr { span, kind } = self;
        span.shift(from, to);
        match kind {
            ExprKind::Name(_)
            | ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::None
            | ExprKind::Unit
            | ExprKind::Error => {}
            ExprKind::Str(parts) => parts.shift(from, to),
            ExprKind::Tuple(items) | ExprKind::List(items) | ExprKind::Set(items) => items.shift(from, to),
            ExprKind::Dict(pairs) => pairs.shift(from, to),
            ExprKind::ListComp { element, loops }
            | ExprKind::SetComp { element, loops }
            | ExprKind::Generator { element, loops } => {
                element.shift(from, to);
                loops.shift(from, to);
            }
            ExprKind::DictComp { key, value, loops } => {
                key.shift(from, to);
                value.shift(from, to);
                loops.shift(from, to);
            }
            ExprKind::Unary { op: _, operand } => operand.shift(from, to),
            ExprKind::Binary { op: _, left, right } => {
                left.shift(from, to);
                right.shift(from, to);
            }
            ExprKind::Compare { first, rest } => {
                first.shift(from, to);
                rest.shift(from, to);
            }
            ExprKind::Logical { op: _, operands } => operands.shift(from, to),
            ExprKind::Not(e) | ExprKind::Try(e) | ExprKind::Fail(e) => e.shift(from, to),
            ExprKind::Coalesce { value, default } => {
                value.shift(from, to);
                default.shift(from, to);
            }
            ExprKind::IfExp { test, then, orelse } => {
                test.shift(from, to);
                then.shift(from, to);
                orelse.shift(from, to);
            }
            ExprKind::Lambda { params, body } => {
                params.shift(from, to);
                body.shift(from, to);
            }
            ExprKind::Call { func, args } => {
                func.shift(from, to);
                args.shift(from, to);
            }
            ExprKind::Index { object, index } => {
                object.shift(from, to);
                index.shift(from, to);
            }
            ExprKind::Slice { object, lower, upper, step } => {
                object.shift(from, to);
                lower.shift(from, to);
                upper.shift(from, to);
                step.shift(from, to);
            }
            ExprKind::Attr { object, name } => {
                object.shift(from, to);
                name.shift(from, to);
            }
        }
    }
}

impl Shift for TypeExpr {
    fn shift(&mut self, from: u32, to: u32) {
        let TypeExpr { span, kind } = self;
        span.shift(from, to);
        match kind {
            TypeKind::Named { name, args } => {
                name.shift(from, to);
                args.shift(from, to);
            }
            TypeKind::List(item) | TypeKind::Set(item) | TypeKind::Optional(item) => item.shift(from, to),
            TypeKind::Dict(k, v) => {
                k.shift(from, to);
                v.shift(from, to);
            }
            TypeKind::Tuple(items) => items.shift(from, to),
            TypeKind::Dyn(name) => name.shift(from, to),
            TypeKind::Unit | TypeKind::Error => {}
        }
    }
}
