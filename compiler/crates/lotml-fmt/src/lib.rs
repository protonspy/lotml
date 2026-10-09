//! The canonical formatter: one way to write a lotml program, with no options (R08).
//!
//! The program is printed from its syntax tree, so the layout of the input does not survive:
//! four spaces per level, one space around binary operators and after commas, one blank line
//! between items, at most one between statements, parentheses only where precedence needs
//! them, tuples parenthesized except as assignment and loop targets. Comments are kept, at the
//! end of the line they followed or on lines of their own; literals are kept as written.

use lotml_syntax::ast::*;
use lotml_syntax::span::Span;
use lotml_syntax::{SyntaxError, parse};

/// The canonical form of `source`, or the syntax errors that stop it being formatted.
pub fn format(source: &str) -> Result<String, Vec<SyntaxError>> {
    let parsed = parse(source);
    if !parsed.errors.is_empty() {
        return Err(parsed.errors);
    }
    let mut f = Formatter { source, comments: parsed.comments, next: 0, out: String::new(), indent: 0, last_end: 0 };
    f.module(&parsed.module);
    Ok(f.out)
}

/// A function's header as the formatter writes it, without the colon: `fn f(x: int) -> int`.
pub fn signature(source: &str, f: &FnDef) -> String {
    let printer = Formatter { source, comments: Vec::new(), next: 0, out: String::new(), indent: 0, last_end: 0 };
    printer.header(f)
}

/// An item in canonical form, without its comments.
pub fn item(source: &str, item: &Item) -> String {
    let mut printer = Formatter { source, comments: Vec::new(), next: 0, out: String::new(), indent: 0, last_end: 0 };
    printer.item(item);
    printer.out
}

const INDENT: &str = "    ";

// Precedence, loosest first: what an operand must reach to need no parentheses.
const LAMBDA: u8 = 0;
const IF_EXP: u8 = 1;
const COALESCE: u8 = 2;
const OR: u8 = 3;
const AND: u8 = 4;
const NOT: u8 = 5;
const COMPARE: u8 = 6;
const BIT_OR: u8 = 7;
const BIT_XOR: u8 = 8;
const BIT_AND: u8 = 9;
const SHIFT: u8 = 10;
const ARITH: u8 = 11;
const TERM: u8 = 12;
const UNARY: u8 = 13;
const POWER: u8 = 14;
const POSTFIX: u8 = 15;
const ATOM: u8 = 16;

struct Formatter<'a> {
    source: &'a str,
    comments: Vec<Span>,
    /// The first comment not printed yet.
    next: usize,
    out: String,
    indent: usize,
    /// Where in the source the last printed thing ended, to see the blank lines after it.
    last_end: u32,
}

impl Formatter<'_> {
    fn text(&self, span: Span) -> &str {
        self.source.get(span.range()).unwrap_or("")
    }

    fn line(&mut self, text: &str) {
        for _ in 0..self.indent {
            self.out.push_str(INDENT);
        }
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn blank(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with("\n\n") {
            self.out.push('\n');
        }
    }

    /// Whether the source has a blank line between the last printed thing and `start`.
    fn blank_before(&self, start: u32) -> bool {
        let gap = self.source.get(self.last_end as usize..start as usize).unwrap_or("");
        let lines: Vec<&str> = gap.split('\n').collect();
        lines.len() > 2 && lines[1..lines.len() - 1].iter().any(|l| l.trim().is_empty())
    }

    fn column(&self, offset: u32) -> usize {
        let before = &self.source[..(offset as usize).min(self.source.len())];
        before.len() - before.rfind('\n').map_or(0, |i| i + 1)
    }

    /// Whether code precedes the comment on its line.
    fn trailing(&self, comment: Span) -> bool {
        let start = comment.start as usize;
        let line_start = self.source[..start].rfind('\n').map_or(0, |i| i + 1);
        !self.source[line_start..start].trim().is_empty()
    }

    /// Print the comments before `before`: one that trailed code goes at the end of the last
    /// line printed, the others on lines of their own. With `column`, only comments indented at
    /// least that far: the rest belong to an enclosing block.
    fn comments_before(&mut self, before: u32, column: Option<usize>) {
        while let Some(&comment) = self.comments.get(self.next) {
            if comment.start >= before
                || column.is_some_and(|c| !self.trailing(comment) && self.column(comment.start) < c)
            {
                return;
            }
            self.next += 1;
            let text = self.text(comment).trim_end().to_string();
            if self.trailing(comment) && self.out.ends_with('\n') && !self.out.ends_with("\n\n") {
                self.out.pop();
                self.out.push_str("  ");
                self.out.push_str(&text);
                self.out.push('\n');
            } else {
                if self.blank_before(comment.start) {
                    self.blank();
                }
                self.line(&text);
            }
            self.last_end = comment.end;
        }
    }

    // Items --------------------------------------------------------------------------------

    fn module(&mut self, module: &Module) {
        for (i, item) in module.items.iter().enumerate() {
            let start = item.span().start;
            if i > 0 {
                // Exactly one blank line between items, before any comments heading the item;
                // none between imports, which read as one block.
                let imports = matches!((&module.items[i - 1], item), (Item::Import(_), Item::Import(_)));
                self.comments_before_item(start, !imports || self.blank_before(start));
            } else {
                self.comments_before(start, None);
                if !self.out.is_empty() && self.blank_before(start) {
                    self.blank();
                }
            }
            self.item(item);
            self.last_end = self.last_end.max(item.end());
        }
        self.comments_before(u32::MAX, None);
    }

    fn comments_before_item(&mut self, start: u32, separate: bool) {
        // A trailing comment of the previous item stays on its line.
        while let Some(&comment) = self.comments.get(self.next) {
            if comment.start < start && self.trailing(comment) {
                self.comments_before(comment.end, None);
            } else {
                break;
            }
        }
        if separate {
            self.blank();
        }
        let had = self.next;
        self.comments_before(start, None);
        if self.next > had && self.blank_before(start) {
            self.blank();
        }
    }

    fn item(&mut self, item: &Item) {
        match item {
            Item::Fn(f) => self.function(f),
            Item::Record(r) => {
                let fields = self.fields(&r.fields);
                let header = format!("type {}{}({fields})", r.name.name, self.type_params(&r.type_params));
                self.line(&header);
            }
            Item::Sum(s) => {
                let variants: Vec<String> = s
                    .variants
                    .iter()
                    .map(|v| match &v.fields {
                        Some(fields) => format!("{}({})", v.name.name, self.fields(fields)),
                        None => v.name.name.clone(),
                    })
                    .collect();
                let header =
                    format!("type {}{} = {}", s.name.name, self.type_params(&s.type_params), variants.join(" | "));
                self.line(&header);
            }
            Item::Impl(imp) => {
                let header = match &imp.trait_name {
                    Some(t) => format!("impl {} for {}:", self.ty(t), self.ty(&imp.target)),
                    None => format!("impl {}:", self.ty(&imp.target)),
                };
                self.line(&header);
                self.members(&imp.methods, imp.span.end, true);
            }
            Item::Trait(t) => {
                let header = format!("trait {}{}:", t.name.name, self.type_params(&t.type_params));
                self.line(&header);
                let spaced = t.methods.iter().any(|m| m.body.is_some());
                self.members(&t.methods, t.span.end, spaced);
            }
            Item::Import(import) => {
                let module: Vec<&str> = import.module.iter().map(|m| m.name.as_str()).collect();
                if import.names.is_empty() {
                    self.line(&format!("import {}", module.join(".")));
                } else {
                    let names: Vec<&str> = import.names.iter().map(|n| n.name.as_str()).collect();
                    self.line(&format!("from {} import {}", module.join("."), names.join(", ")));
                }
            }
            Item::Test(t) => {
                let name = self.text(t.name_span).to_string();
                self.line(&format!("test {name}:"));
                self.block(&t.body);
            }
            Item::Class(c) => {
                let bases: Vec<&str> = c.bases.iter().map(|b| b.name.as_str()).collect();
                let colon = if c.attributes.is_empty() && c.methods.is_empty() { "" } else { ":" };
                let params = self.type_params(&c.type_params);
                let header = if bases.is_empty() {
                    format!("class {}{params}{colon}", c.name.name)
                } else {
                    format!("class {}{params}({}){colon}", c.name.name, bases.join(", "))
                };
                self.line(&header);
                self.indent += 1;
                for attribute in &c.attributes {
                    if let Some(name) = &attribute.name {
                        let line = format!("{}: {}", name.name, self.ty(&attribute.ty));
                        self.line(&line);
                    }
                }
                self.indent -= 1;
                self.members(&c.methods, c.span.end, false);
            }
            Item::Error(_) => {}
        }
    }

    fn members(&mut self, methods: &[FnDef], end: u32, spaced: bool) {
        self.indent += 1;
        for (i, method) in methods.iter().enumerate() {
            let column = self.column(method.span.start);
            if i > 0 && spaced {
                while let Some(&comment) = self.comments.get(self.next) {
                    if comment.start < method.span.start && self.trailing(comment) {
                        self.comments_before(comment.end, None);
                    } else {
                        break;
                    }
                }
                self.blank();
            }
            self.comments_before(method.span.start, Some(column));
            self.function(method);
            self.last_end = self.last_end.max(method.end());
        }
        let column = methods.first().map(|m| self.column(m.span.start));
        self.comments_before(end, column);
        self.indent -= 1;
    }

    fn header(&self, f: &FnDef) -> String {
        let params: Vec<String> = f.params.iter().map(|p| self.param(p)).collect();
        let mut header = format!("fn {}{}({})", f.name.name, self.type_params(&f.type_params), params.join(", "));
        if let Some(ret) = &f.returns {
            header += &format!(" -> {}", self.ty(ret));
        }
        if let Some(error) = &f.error {
            if f.returns.is_none() {
                header += " -> None";
            }
            header += &format!(" ! {}", self.ty(error));
        }
        header
    }

    fn function(&mut self, f: &FnDef) {
        let header = self.header(f);
        match &f.body {
            Some(body) => {
                self.line(&format!("{header}:"));
                self.block(body);
            }
            None => self.line(&header),
        }
    }

    fn param(&self, p: &Param) -> String {
        let convention = match p.convention {
            Convention::Default => "",
            Convention::Inout => "inout ",
            Convention::Sink => "sink ",
            Convention::Var => "var ",
        };
        let mut out = format!("{convention}{}", p.name.name);
        if let Some(ty) = &p.ty {
            out += &format!(": {}", self.ty(ty));
        }
        if let Some(default) = &p.default {
            out += &format!(" = {}", self.expr(default, LAMBDA));
        }
        out
    }

    fn fields(&self, fields: &[Field]) -> String {
        let fields: Vec<String> = fields
            .iter()
            .map(|f| {
                let mut out = match &f.name {
                    Some(name) => format!("{}: {}", name.name, self.ty(&f.ty)),
                    None => self.ty(&f.ty),
                };
                if let Some(default) = &f.default {
                    out += &format!(" = {}", self.expr(default, LAMBDA));
                }
                out
            })
            .collect();
        fields.join(", ")
    }

    fn type_params(&self, params: &[TypeParam]) -> String {
        if params.is_empty() {
            return String::new();
        }
        let params: Vec<String> = params
            .iter()
            .map(|p| match &p.bound {
                Some(bound) => format!("{}: {}", p.name.name, bound.name),
                None => p.name.name.clone(),
            })
            .collect();
        format!("[{}]", params.join(", "))
    }

    fn ty(&self, t: &TypeExpr) -> String {
        match &t.kind {
            TypeKind::Named { name, args } if args.is_empty() => name.name.clone(),
            TypeKind::Named { name, args } => {
                let args: Vec<String> = args.iter().map(|a| self.ty(a)).collect();
                format!("{}[{}]", name.name, args.join(", "))
            }
            TypeKind::List(item) => format!("[{}]", self.ty(item)),
            TypeKind::Set(item) => format!("{{{}}}", self.ty(item)),
            TypeKind::Dict(k, v) => format!("{{{}: {}}}", self.ty(k), self.ty(v)),
            TypeKind::Tuple(items) if items.len() == 1 => format!("({},)", self.ty(&items[0])),
            TypeKind::Tuple(items) => {
                let items: Vec<String> = items.iter().map(|i| self.ty(i)).collect();
                format!("({})", items.join(", "))
            }
            TypeKind::Optional(inner) => format!("{}?", self.ty(inner)),
            TypeKind::Dyn(name) => format!("dyn {}", name.name),
            TypeKind::Unit => "None".into(),
            TypeKind::Error => self.text(t.span).to_string(),
        }
    }

    // Statements ---------------------------------------------------------------------------

    fn block(&mut self, block: &Block) {
        self.indent += 1;
        let column = block.stmts.first().map(|s| self.column(s.span.start));
        for (i, stmt) in block.stmts.iter().enumerate() {
            self.comments_before(stmt.span.start, None);
            if i > 0 && self.blank_before(stmt.span.start) {
                self.blank();
            }
            self.stmt(stmt);
            self.last_end = self.last_end.max(stmt.end());
        }
        self.comments_before(block.span.end, column);
        self.indent -= 1;
    }

    /// Mark the end of the header just printed, so comments after it count from there.
    fn header_end(&mut self, at: u32) {
        self.last_end = at;
    }

    fn stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Expr(e) => {
                let text = self.expr(e, LAMBDA);
                self.line(&text);
            }
            StmtKind::Var { name, ty, value } => {
                let ty = ty.as_ref().map(|t| format!(": {}", self.ty(t))).unwrap_or_default();
                let text = format!("var {}{ty} = {}", name.name, self.expr(value, LAMBDA));
                self.line(&text);
            }
            StmtKind::Assign { target, value } => {
                let text = format!("{} = {}", self.target_expr(target), self.expr(value, LAMBDA));
                self.line(&text);
            }
            StmtKind::Annotated { target, ty, value } => {
                let text = format!("{}: {} = {}", target.name, self.ty(ty), self.expr(value, LAMBDA));
                self.line(&text);
            }
            StmtKind::AugAssign { target, op, value } => {
                let text = format!("{} {}= {}", self.target_expr(target), op.text(), self.expr(value, LAMBDA));
                self.line(&text);
            }
            StmtKind::Return(None) => self.line("return"),
            StmtKind::Return(Some(value)) => {
                let text = format!("return {}", self.expr(value, LAMBDA));
                self.line(&text);
            }
            StmtKind::Assert { test, message } => {
                let mut text = format!("assert {}", self.expr(test, LAMBDA));
                if let Some(m) = message {
                    text += &format!(", {}", self.expr(m, LAMBDA));
                }
                self.line(&text);
            }
            StmtKind::Pass => self.line("pass"),
            StmtKind::Break => self.line("break"),
            StmtKind::Continue => self.line("continue"),
            StmtKind::If { branches, orelse } => {
                for (i, (test, body)) in branches.iter().enumerate() {
                    let keyword = if i == 0 { "if" } else { "elif" };
                    self.comments_before(test.span.start, None);
                    let text = format!("{keyword} {}:", self.expr(test, LAMBDA));
                    self.line(&text);
                    self.header_end(test.span.end);
                    self.block(body);
                }
                if let Some(body) = orelse {
                    self.line("else:");
                    self.block(body);
                }
            }
            StmtKind::While { test, body } => {
                let text = format!("while {}:", self.expr(test, LAMBDA));
                self.line(&text);
                self.header_end(test.span.end);
                self.block(body);
            }
            StmtKind::For { target, iter, body } => {
                let text = format!("for {} in {}:", self.target(target, false), self.expr(iter, LAMBDA));
                self.line(&text);
                self.header_end(iter.span.end);
                self.block(body);
            }
            StmtKind::Match { subject, arms } => {
                let text = format!("match {}:", self.expr(subject, LAMBDA));
                self.line(&text);
                self.header_end(subject.span.end);
                self.indent += 1;
                let column = arms.first().map(|a| self.column(a.span.start));
                for arm in arms {
                    self.comments_before(arm.span.start, None);
                    let text = format!("case {}:", self.pattern(&arm.pattern));
                    self.line(&text);
                    self.header_end(arm.pattern.span.end);
                    self.block(&arm.body);
                }
                self.comments_before(stmt.span.end, column);
                self.indent -= 1;
            }
            StmtKind::Error => {}
        }
    }

    fn target(&self, target: &Target, nested: bool) -> String {
        match target {
            Target::Name(name) => name.name.clone(),
            Target::Tuple(items, _) => {
                let items: Vec<String> = items.iter().map(|t| self.target(t, true)).collect();
                if nested { format!("({})", items.join(", ")) } else { items.join(", ") }
            }
        }
    }

    /// An assignment target: a tuple of places goes without parentheses.
    fn target_expr(&self, target: &Expr) -> String {
        match &target.kind {
            ExprKind::Tuple(items) if items.len() > 1 => {
                let items: Vec<String> = items.iter().map(|i| self.expr(i, BIT_OR)).collect();
                items.join(", ")
            }
            _ => self.expr(target, LAMBDA),
        }
    }

    fn pattern(&self, p: &Pattern) -> String {
        match &p.kind {
            PatternKind::Wildcard => "_".into(),
            PatternKind::Name(name) => name.name.clone(),
            PatternKind::Variant { name, args } => {
                let args: Vec<String> = args.iter().map(|a| self.pattern(a)).collect();
                format!("{}({})", name.name, args.join(", "))
            }
            PatternKind::Tuple(items) if items.len() == 1 => format!("({},)", self.pattern(&items[0])),
            PatternKind::Tuple(items) => {
                let items: Vec<String> = items.iter().map(|i| self.pattern(i)).collect();
                format!("({})", items.join(", "))
            }
            PatternKind::Literal(e) => self.expr(e, UNARY),
            PatternKind::Error => self.text(p.span).to_string(),
        }
    }

    // Expressions --------------------------------------------------------------------------

    /// `e`, parenthesized when its precedence is looser than `at_least`.
    fn expr(&self, e: &Expr, at_least: u8) -> String {
        let (text, precedence) = self.bare(e);
        if precedence < at_least { format!("({text})") } else { text }
    }

    fn list(&self, items: &[Expr]) -> String {
        let items: Vec<String> = items.iter().map(|i| self.expr(i, LAMBDA)).collect();
        items.join(", ")
    }

    fn loops(&self, loops: &[Comprehension]) -> String {
        let mut out = String::new();
        for l in loops {
            out += &format!(" for {} in {}", self.target(&l.target, false), self.expr(&l.iter, OR));
            for c in &l.conditions {
                out += &format!(" if {}", self.expr(c, OR));
            }
        }
        out
    }

    fn bare(&self, e: &Expr) -> (String, u8) {
        match &e.kind {
            ExprKind::Name(n) => (n.clone(), ATOM),
            ExprKind::Int(_) | ExprKind::Float(_) => (self.text(e.span).to_string(), ATOM),
            ExprKind::Str(literals) => {
                let parts: Vec<&str> = literals.iter().map(|l| l.raw.as_str()).collect();
                (parts.join(" "), ATOM)
            }
            ExprKind::Bool(true) => ("True".into(), ATOM),
            ExprKind::Bool(false) => ("False".into(), ATOM),
            ExprKind::None => ("None".into(), ATOM),
            ExprKind::Unit => ("()".into(), ATOM),
            ExprKind::Tuple(items) if items.len() == 1 => (format!("({},)", self.expr(&items[0], LAMBDA)), ATOM),
            ExprKind::Tuple(items) => (format!("({})", self.list(items)), ATOM),
            ExprKind::List(items) => (format!("[{}]", self.list(items)), ATOM),
            ExprKind::Set(items) => (format!("{{{}}}", self.list(items)), ATOM),
            ExprKind::Dict(pairs) => {
                let pairs: Vec<String> =
                    pairs.iter().map(|(k, v)| format!("{}: {}", self.expr(k, LAMBDA), self.expr(v, LAMBDA))).collect();
                (format!("{{{}}}", pairs.join(", ")), ATOM)
            }
            ExprKind::ListComp { element, loops } => {
                (format!("[{}{}]", self.expr(element, LAMBDA), self.loops(loops)), ATOM)
            }
            ExprKind::SetComp { element, loops } => {
                (format!("{{{}{}}}", self.expr(element, LAMBDA), self.loops(loops)), ATOM)
            }
            ExprKind::DictComp { key, value, loops } => {
                (format!("{{{}: {}{}}}", self.expr(key, LAMBDA), self.expr(value, LAMBDA), self.loops(loops)), ATOM)
            }
            ExprKind::Generator { element, loops } => {
                (format!("({}{})", self.expr(element, LAMBDA), self.loops(loops)), ATOM)
            }
            ExprKind::Unary { op, operand } => {
                let sign = match op {
                    UnaryOp::Neg => "-",
                    UnaryOp::Pos => "+",
                    UnaryOp::Invert => "~",
                };
                (format!("{sign}{}", self.expr(operand, UNARY)), UNARY)
            }
            ExprKind::Binary { op, left, right } => {
                let level = binary_level(*op);
                if *op == BinOp::Pow {
                    return (format!("{} ** {}", self.expr(left, POSTFIX), self.expr(right, UNARY)), POWER);
                }
                (format!("{} {} {}", self.expr(left, level), op.text(), self.expr(right, level + 1)), level)
            }
            ExprKind::Compare { first, rest } => {
                let mut out = self.expr(first, BIT_OR);
                for (op, right) in rest {
                    out += &format!(" {} {}", op.text(), self.expr(right, BIT_OR));
                }
                (out, COMPARE)
            }
            ExprKind::Logical { op, operands } => {
                let (word, level, operand) = match op {
                    BoolOp::And => ("and", AND, NOT),
                    BoolOp::Or => ("or", OR, AND),
                };
                let parts: Vec<String> = operands.iter().map(|o| self.expr(o, operand)).collect();
                (parts.join(&format!(" {word} ")), level)
            }
            // `not (a == b)` keeps its parentheses: `not a == b` reads as `(not a) == b`.
            ExprKind::Not(inner) => (format!("not {}", self.expr(inner, BIT_OR)), NOT),
            ExprKind::Coalesce { value, default } => {
                (format!("{} ?? {}", self.expr(value, COALESCE), self.expr(default, OR)), COALESCE)
            }
            ExprKind::IfExp { test, then, orelse } => (
                format!(
                    "{} if {} else {}",
                    self.expr(then, COALESCE),
                    self.expr(test, COALESCE),
                    self.expr(orelse, LAMBDA)
                ),
                IF_EXP,
            ),
            ExprKind::Lambda { params, body } => {
                let params: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
                let head =
                    if params.is_empty() { "lambda".to_string() } else { format!("lambda {}", params.join(", ")) };
                (format!("{head}: {}", self.expr(body, LAMBDA)), LAMBDA)
            }
            ExprKind::Call { func, args } => {
                let alone = args.len() == 1;
                let args: Vec<String> = args
                    .iter()
                    .map(|a| match a {
                        // A lone generator argument needs no parentheses of its own.
                        Arg::Positional(Expr { kind: ExprKind::Generator { element, loops }, .. }) if alone => {
                            format!("{}{}", self.expr(element, LAMBDA), self.loops(loops))
                        }
                        Arg::Positional(e) => self.expr(e, LAMBDA),
                        Arg::Keyword(name, e) => format!("{}={}", name.name, self.expr(e, LAMBDA)),
                        Arg::Inout(e, _) => format!("&{}", self.expr(e, POSTFIX)),
                    })
                    .collect();
                (format!("{}({})", self.expr(func, POSTFIX), args.join(", ")), POSTFIX)
            }
            ExprKind::Index { object, index } => {
                (format!("{}[{}]", self.expr(object, POSTFIX), self.expr(index, LAMBDA)), POSTFIX)
            }
            ExprKind::Slice { object, lower, upper, step } => {
                let part = |p: &Option<Box<Expr>>| p.as_ref().map(|e| self.expr(e, LAMBDA)).unwrap_or_default();
                let mut inner = format!("{}:{}", part(lower), part(upper));
                if step.is_some() {
                    inner += &format!(":{}", part(step));
                }
                (format!("{}[{inner}]", self.expr(object, POSTFIX)), POSTFIX)
            }
            ExprKind::Attr { object, name } => {
                // `1 .real` would need a space; lotml numbers have no fields, so no case arises.
                (format!("{}.{}", self.expr(object, POSTFIX), name.name), POSTFIX)
            }
            ExprKind::Try(inner) => (format!("{}?", self.expr(inner, POSTFIX)), POSTFIX),
            // An atom to the parser, but `fail e.x` would read the field as part of the error.
            ExprKind::Fail(error) => (format!("fail {}", self.expr(error, POSTFIX)), UNARY),
            ExprKind::Error => (self.text(e.span).to_string(), ATOM),
        }
    }
}

fn binary_level(op: BinOp) -> u8 {
    match op {
        BinOp::BitOr => BIT_OR,
        BinOp::BitXor => BIT_XOR,
        BinOp::BitAnd => BIT_AND,
        BinOp::LShift | BinOp::RShift => SHIFT,
        BinOp::Add | BinOp::Sub => ARITH,
        BinOp::Mul | BinOp::Div | BinOp::FloorDiv | BinOp::Mod => TERM,
        BinOp::Pow => POWER,
    }
}
