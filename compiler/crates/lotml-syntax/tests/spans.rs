//! Every node of the syntax tree spans text in order, inside its parent's span, after the sibling
//! before it: what an editor, an agent's edit and every diagnostic rely on to find the text a
//! node came from (plans/frontend-robustness.md 1.2). Checked over every program of the corpus,
//! whole and cut short at each line, where the parser is reading half-written code.

use lotml_syntax::ast::*;
use lotml_syntax::parse;
use lotml_syntax::span::Span;

/// The programs of the corpus (`harness/results/corpus/corpus.jsonl`).
fn corpus() -> Vec<(String, String)> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../harness/results/corpus/corpus.jsonl");
    let text = std::fs::read_to_string(path).expect("the corpus");
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let row: serde_json::Value = serde_json::from_str(line).expect("a JSON row");
            (row["task"].as_str().unwrap_or("").to_string(), row["lotml"].as_str().unwrap_or("").to_string())
        })
        .collect()
}

/// The places a tree breaks the rule, each named by its path from the module.
struct Walk<'a> {
    text: &'a str,
    broken: Vec<String>,
    path: Vec<&'static str>,
}

impl Walk<'_> {
    /// `span`, a node called `what` inside `parent`, starting at or after `after`.
    fn node(&mut self, what: &'static str, span: Span, parent: Span, after: &mut u32) {
        let place = || format!("{}/{what} {}..{}", self.path.join("/"), span.start, span.end);
        if span.start > span.end {
            self.broken.push(format!("{}: ends before it starts", place()));
        } else if span.start < parent.start || span.end > parent.end {
            self.broken.push(format!("{}: outside its parent {}..{}", place(), parent.start, parent.end));
        } else if span.start < *after {
            self.broken.push(format!("{}: starts before its sibling ends its start at {after}", place()));
        } else if span.end as usize > self.text.len() {
            self.broken.push(format!("{}: past the end of the text", place()));
        }
        *after = (*after).max(span.start);
    }

    fn within(
        &mut self,
        what: &'static str,
        span: Span,
        parent: Span,
        after: &mut u32,
        inside: impl FnOnce(&mut Self, Span),
    ) {
        self.node(what, span, parent, after);
        self.path.push(what);
        inside(self, span);
        self.path.pop();
    }

    fn module(&mut self, module: &Module) {
        let whole = Span::new(0, self.text.len());
        let mut after = 0;
        for item in &module.items {
            self.item(item, whole, &mut after);
        }
    }

    fn item(&mut self, item: &Item, parent: Span, after: &mut u32) {
        match item {
            Item::Fn(f) => self.function(f, parent, after),
            Item::Record(r) => self.within("record", r.span, parent, after, |w, s| {
                let mut at = s.start;
                w.ident(&r.name, s, &mut at);
                w.type_params(&r.type_params, s, &mut at);
                w.fields(&r.fields, s, &mut at);
            }),
            Item::Sum(sum) => self.within("sum", sum.span, parent, after, |w, s| {
                let mut at = s.start;
                w.ident(&sum.name, s, &mut at);
                w.type_params(&sum.type_params, s, &mut at);
                for v in &sum.variants {
                    w.within("variant", v.span, s, &mut at, |w, vs| {
                        let mut at = vs.start;
                        w.ident(&v.name, vs, &mut at);
                        w.fields(v.fields.as_deref().unwrap_or_default(), vs, &mut at);
                    });
                }
            }),
            Item::Impl(i) => self.within("impl", i.span, parent, after, |w, s| {
                let mut at = s.start;
                if let Some(t) = &i.trait_name {
                    w.ty(t, s, &mut at);
                }
                w.ty(&i.target, s, &mut at);
                for m in &i.methods {
                    w.function(m, s, &mut at);
                }
            }),
            Item::Trait(t) => self.within("trait", t.span, parent, after, |w, s| {
                let mut at = s.start;
                w.ident(&t.name, s, &mut at);
                w.type_params(&t.type_params, s, &mut at);
                for m in &t.methods {
                    w.function(m, s, &mut at);
                }
            }),
            Item::Import(i) => self.within("import", i.span, parent, after, |w, s| {
                let mut at = s.start;
                for name in i.module.iter().chain(&i.names) {
                    w.ident(name, s, &mut at);
                }
            }),
            Item::Test(t) => self.within("test", t.span, parent, after, |w, s| {
                let mut at = s.start;
                w.node("name", t.name_span, s, &mut at);
                w.block(&t.body, s, &mut at);
            }),
            Item::Class(c) => self.within("class", c.span, parent, after, |w, s| {
                let mut at = s.start;
                w.ident(&c.name, s, &mut at);
                w.type_params(&c.type_params, s, &mut at);
                for base in &c.bases {
                    w.ident(base, s, &mut at);
                }
                w.fields(&c.attributes, s, &mut at);
                for m in &c.methods {
                    w.function(m, s, &mut at);
                }
            }),
            Item::Error(span) => self.node("error", *span, parent, after),
        }
    }

    fn function(&mut self, f: &FnDef, parent: Span, after: &mut u32) {
        self.within("fn", f.span, parent, after, |w, s| {
            let mut at = s.start;
            w.ident(&f.name, s, &mut at);
            w.type_params(&f.type_params, s, &mut at);
            for p in &f.params {
                w.within("param", p.span, s, &mut at, |w, ps| {
                    let mut at = ps.start;
                    w.ident(&p.name, ps, &mut at);
                    if let Some(t) = &p.ty {
                        w.ty(t, ps, &mut at);
                    }
                    if let Some(d) = &p.default {
                        w.expr(d, ps, &mut at);
                    }
                });
            }
            for t in f.returns.iter().chain(&f.error) {
                w.ty(t, s, &mut at);
            }
            if let Some(body) = &f.body {
                w.block(body, s, &mut at);
            }
        });
    }

    fn ident(&mut self, ident: &Ident, parent: Span, after: &mut u32) {
        self.node("name", ident.span, parent, after);
    }

    fn type_params(&mut self, params: &[TypeParam], parent: Span, after: &mut u32) {
        for p in params {
            self.ident(&p.name, parent, after);
            if let Some(bound) = &p.bound {
                self.ident(bound, parent, after);
            }
        }
    }

    fn fields(&mut self, fields: &[Field], parent: Span, after: &mut u32) {
        for f in fields {
            self.within("field", f.span, parent, after, |w, s| {
                let mut at = s.start;
                if let Some(name) = &f.name {
                    w.ident(name, s, &mut at);
                }
                w.ty(&f.ty, s, &mut at);
                if let Some(d) = &f.default {
                    w.expr(d, s, &mut at);
                }
            });
        }
    }

    fn ty(&mut self, ty: &TypeExpr, parent: Span, after: &mut u32) {
        self.within("type", ty.span, parent, after, |w, s| {
            let mut at = s.start;
            match &ty.kind {
                TypeKind::Named { name, args } => {
                    w.ident(name, s, &mut at);
                    for a in args {
                        w.ty(a, s, &mut at);
                    }
                }
                TypeKind::List(t) | TypeKind::Set(t) | TypeKind::Optional(t) => w.ty(t, s, &mut at),
                TypeKind::Dict(k, v) => {
                    w.ty(k, s, &mut at);
                    w.ty(v, s, &mut at);
                }
                TypeKind::Tuple(items) => {
                    for t in items {
                        w.ty(t, s, &mut at);
                    }
                }
                TypeKind::Dyn(name) => w.ident(name, s, &mut at),
                TypeKind::Unit | TypeKind::Error => {}
            }
        });
    }

    fn block(&mut self, block: &Block, parent: Span, after: &mut u32) {
        self.within("block", block.span, parent, after, |w, s| {
            let mut at = s.start;
            for stmt in &block.stmts {
                w.stmt(stmt, s, &mut at);
            }
        });
    }

    fn target(&mut self, target: &Target, parent: Span, after: &mut u32) {
        match target {
            Target::Name(ident) => self.ident(ident, parent, after),
            Target::Tuple(items, span) => self.within("targets", *span, parent, after, |w, s| {
                let mut at = s.start;
                for t in items {
                    w.target(t, s, &mut at);
                }
            }),
        }
    }

    fn stmt(&mut self, stmt: &Stmt, parent: Span, after: &mut u32) {
        self.within("stmt", stmt.span, parent, after, |w, s| {
            let mut at = s.start;
            match &stmt.kind {
                StmtKind::Expr(e) | StmtKind::Return(Some(e)) => w.expr(e, s, &mut at),
                StmtKind::Var { name, ty, value } => {
                    w.ident(name, s, &mut at);
                    if let Some(t) = ty {
                        w.ty(t, s, &mut at);
                    }
                    w.expr(value, s, &mut at);
                }
                StmtKind::Assign { target, value } | StmtKind::AugAssign { target, value, .. } => {
                    w.expr(target, s, &mut at);
                    w.expr(value, s, &mut at);
                }
                StmtKind::Annotated { target, ty, value } => {
                    w.ident(target, s, &mut at);
                    w.ty(ty, s, &mut at);
                    w.expr(value, s, &mut at);
                }
                StmtKind::Assert { test, message } => {
                    w.expr(test, s, &mut at);
                    if let Some(m) = message {
                        w.expr(m, s, &mut at);
                    }
                }
                StmtKind::If { branches, orelse } => {
                    for (test, body) in branches {
                        w.expr(test, s, &mut at);
                        w.block(body, s, &mut at);
                    }
                    if let Some(body) = orelse {
                        w.block(body, s, &mut at);
                    }
                }
                StmtKind::While { test, body } => {
                    w.expr(test, s, &mut at);
                    w.block(body, s, &mut at);
                }
                StmtKind::For { target, iter, body } => {
                    w.target(target, s, &mut at);
                    w.expr(iter, s, &mut at);
                    w.block(body, s, &mut at);
                }
                StmtKind::Match { subject, arms } => {
                    w.expr(subject, s, &mut at);
                    for arm in arms {
                        w.within("arm", arm.span, s, &mut at, |w, a| {
                            let mut at = a.start;
                            w.pattern(&arm.pattern, a, &mut at);
                            w.block(&arm.body, a, &mut at);
                        });
                    }
                }
                StmtKind::Return(None) | StmtKind::Pass | StmtKind::Break | StmtKind::Continue | StmtKind::Error => {}
            }
        });
    }

    fn pattern(&mut self, pattern: &Pattern, parent: Span, after: &mut u32) {
        self.within("pattern", pattern.span, parent, after, |w, s| {
            let mut at = s.start;
            match &pattern.kind {
                PatternKind::Name(name) => w.ident(name, s, &mut at),
                PatternKind::Variant { name, args } => {
                    w.ident(name, s, &mut at);
                    for p in args {
                        w.pattern(p, s, &mut at);
                    }
                }
                PatternKind::Tuple(items) => {
                    for p in items {
                        w.pattern(p, s, &mut at);
                    }
                }
                PatternKind::Literal(e) => w.expr(e, s, &mut at),
                PatternKind::Wildcard | PatternKind::Error => {}
            }
        });
    }

    fn loops(&mut self, loops: &[Comprehension], parent: Span, after: &mut u32) {
        for l in loops {
            self.target(&l.target, parent, after);
            self.expr(&l.iter, parent, after);
            for c in &l.conditions {
                self.expr(c, parent, after);
            }
        }
    }

    fn expr(&mut self, expr: &Expr, parent: Span, after: &mut u32) {
        self.within("expr", expr.span, parent, after, |w, s| {
            let mut at = s.start;
            match &expr.kind {
                ExprKind::Str(literals) => {
                    for part in literals.iter().flat_map(|l| &l.parts) {
                        w.str_part(part, s, &mut at);
                    }
                }
                ExprKind::Tuple(items) | ExprKind::List(items) | ExprKind::Set(items) => {
                    for e in items {
                        w.expr(e, s, &mut at);
                    }
                }
                ExprKind::Dict(pairs) => {
                    for (k, v) in pairs {
                        w.expr(k, s, &mut at);
                        w.expr(v, s, &mut at);
                    }
                }
                ExprKind::ListComp { element, loops }
                | ExprKind::SetComp { element, loops }
                | ExprKind::Generator { element, loops } => {
                    w.expr(element, s, &mut at);
                    w.loops(loops, s, &mut at);
                }
                ExprKind::DictComp { key, value, loops } => {
                    w.expr(key, s, &mut at);
                    w.expr(value, s, &mut at);
                    w.loops(loops, s, &mut at);
                }
                ExprKind::Unary { operand, .. } | ExprKind::Not(operand) => w.expr(operand, s, &mut at),
                ExprKind::Try(inner) | ExprKind::Fail(inner) => w.expr(inner, s, &mut at),
                ExprKind::Binary { left, right, .. } => {
                    w.expr(left, s, &mut at);
                    w.expr(right, s, &mut at);
                }
                ExprKind::Compare { first, rest } => {
                    w.expr(first, s, &mut at);
                    for (_, e) in rest {
                        w.expr(e, s, &mut at);
                    }
                }
                ExprKind::Logical { operands, .. } => {
                    for e in operands {
                        w.expr(e, s, &mut at);
                    }
                }
                ExprKind::Coalesce { value, default } => {
                    w.expr(value, s, &mut at);
                    w.expr(default, s, &mut at);
                }
                ExprKind::IfExp { test, then, orelse } => {
                    // Written `then if test else orelse`: the value comes first.
                    w.expr(then, s, &mut at);
                    w.expr(test, s, &mut at);
                    w.expr(orelse, s, &mut at);
                }
                ExprKind::Lambda { params, body } => {
                    for p in params {
                        w.ident(p, s, &mut at);
                    }
                    w.expr(body, s, &mut at);
                }
                ExprKind::Call { func, args } => {
                    w.expr(func, s, &mut at);
                    for arg in args {
                        match arg {
                            Arg::Positional(e) => w.expr(e, s, &mut at),
                            Arg::Keyword(name, e) => {
                                w.ident(name, s, &mut at);
                                w.expr(e, s, &mut at);
                            }
                            Arg::Inout(e, ampersand) => {
                                // The span is the `&`'s alone, and the place follows it.
                                w.node("ampersand", *ampersand, s, &mut at);
                                w.expr(e, s, &mut at);
                            }
                        }
                    }
                }
                ExprKind::Index { object, index } => {
                    w.expr(object, s, &mut at);
                    w.expr(index, s, &mut at);
                }
                ExprKind::Slice { object, lower, upper, step } => {
                    w.expr(object, s, &mut at);
                    for e in [lower, upper, step].into_iter().flatten() {
                        w.expr(e, s, &mut at);
                    }
                }
                ExprKind::Attr { object, name } => {
                    w.expr(object, s, &mut at);
                    w.ident(name, s, &mut at);
                }
                ExprKind::Name(_)
                | ExprKind::Int(_)
                | ExprKind::Float(_)
                | ExprKind::Bool(_)
                | ExprKind::None
                | ExprKind::Unit
                | ExprKind::Error => {}
            }
        });
    }

    fn str_part(&mut self, part: &StrPart, parent: Span, after: &mut u32) {
        if let StrPart::Expr { expr, spec, .. } = part {
            self.expr(expr, parent, after);
            for p in spec {
                self.str_part(p, parent, after);
            }
        }
    }
}

/// What breaks the rule in the tree of `text`.
fn broken(text: &str) -> Vec<String> {
    let parsed = parse(text);
    let mut walk = Walk { text, broken: Vec::new(), path: Vec::new() };
    walk.module(&parsed.module);
    walk.broken
}

#[test]
fn every_span_of_every_corpus_program_is_in_order_and_inside_its_parent() {
    let programs = corpus();
    assert!(programs.len() > 500, "the corpus holds its programs");
    let mut failures = Vec::new();
    for (task, text) in &programs {
        for b in broken(text) {
            failures.push(format!("{task}: {b}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} broken spans, the first:\n{}",
        failures.len(),
        failures[..failures.len().min(20)].join("\n")
    );
}

#[test]
fn the_spans_hold_on_every_program_cut_short_at_each_line() {
    let mut failures = Vec::new();
    for (task, text) in corpus() {
        let ends: Vec<usize> = text.match_indices('\n').map(|(k, _)| k).collect();
        for end in ends {
            for b in broken(&text[..end]) {
                failures.push(format!("{task} cut at {end}: {b}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} broken spans, the first:\n{}",
        failures.len(),
        failures[..failures.len().min(20)].join("\n")
    );
}
