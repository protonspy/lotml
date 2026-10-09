//! The parser: hand-written recursive descent over the lexer's tokens, tolerant of errors.
//!
//! A statement or item that does not parse is reported once and skipped to the end of its
//! line — and of the block it opens — so the rest of the file still parses and every error
//! after it is a real one rather than a cascade.

use crate::ast::*;
use crate::lexer::{self, Token, TokenKind as T};
use crate::span::Span;
use crate::strings;

/// A syntax error: what was expected, where, and a fix when one is certain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxError {
    pub span: Span,
    pub message: String,
    pub expected: Vec<&'static str>,
    pub fix: Option<(Span, String)>,
    /// Stable code: `E0001` and up; see `lotml explain`.
    pub code: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parsed {
    pub module: Module,
    pub errors: Vec<SyntaxError>,
    pub comments: Vec<Span>,
}

pub fn parse(text: &str) -> Parsed {
    parse_file(text, false)
}

/// Parse an interface: a file of declarations whose functions are signatures with no body, as
/// the bindings `lotml bind` generates for a Python module are (adr:0012).
pub fn parse_interface(text: &str) -> Parsed {
    parse_file(text, true)
}

fn parse_file(text: &str, interface: bool) -> Parsed {
    let lexed = lexer::lex(text);
    let mut parser = Parser::new(text, lexed.tokens, 0);
    parser.interface = interface;
    for error in lexed.errors {
        let fix = error.fix.map(|f| (error.span, f));
        let code = if fix.is_some() { "E0002" } else { "E0001" };
        parser.errors.push(SyntaxError { span: error.span, message: error.message, expected: vec![], fix, code });
    }
    let module = parser.module();
    let mut errors = parser.errors;
    errors.sort_by_key(|e| e.span.start);
    Parsed { module, errors, comments: lexed.comments }
}

/// Parse one expression from `text`, whose spans start at `base` in the enclosing file, inside
/// an expression already `depth` deep and `chain` long, so a field nested in a field is bounded
/// with the rest.
fn parse_expression(text: &str, base: u32, depth: u32, chain: u32) -> (Expr, Vec<SyntaxError>) {
    let lexed = lexer::lex(text);
    let mut parser = Parser::new(text, lexed.tokens, base);
    parser.depth = depth;
    parser.chain = chain;
    let expr = parser.test();
    if !matches!(parser.peek(), T::Newline | T::Eof) {
        parser.error_here("expected the end of the f-string field");
    }
    (expr, parser.errors)
}

struct Parser<'a> {
    text: &'a str,
    tokens: Vec<Token>,
    pos: usize,
    base: u32,
    errors: Vec<SyntaxError>,
    /// Set by the first error of a statement, so its cascade is not reported.
    reported: bool,
    /// How deep the recursive descent is, so pathological nesting is refused, not a stack crash.
    depth: u32,
    chain: u32,
    /// Reading an interface, whose functions are signatures without bodies.
    interface: bool,
}

/// The deepest nesting of expressions, types or patterns the parser recurses into. Far beyond
/// anything a program is written with, and well under the stack it would take to overflow.
const MAX_DEPTH: u32 = 256;

/// The longest chain of operators — `a + b + …`, `- - x`, `x.a.b…` — on any path through an
/// expression. A chain builds a tree as deep as it is long, which every later pass recurses
/// over, so it is bounded like nesting; cheaper to descend, so the bound is higher.
const MAX_CHAIN: u32 = 1000;

impl<'a> Parser<'a> {
    fn new(text: &'a str, tokens: Vec<Token>, base: u32) -> Parser<'a> {
        Parser { text, tokens, pos: 0, base, errors: Vec::new(), reported: false, depth: 0, chain: 0, interface: false }
    }

    /// Whether the descent has gone too deep; when it has, report once and let the caller bail
    /// with an error node, so a deeply nested input is a diagnostic rather than a crash.
    fn too_deep(&mut self) -> bool {
        if self.depth >= MAX_DEPTH {
            self.error_here("this nests too deeply to read");
            return true;
        }
        false
    }

    /// Whether one more link would make the chain too long; when it would, report once and let
    /// the caller stop the chain where it is.
    fn too_long(&mut self) -> bool {
        if self.chain >= MAX_CHAIN {
            self.error_here("this expression is too long to read; split it with names");
            return true;
        }
        false
    }

    // Tokens ------------------------------------------------------------------------

    fn token(&self) -> Token {
        self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek(&self) -> T {
        self.token().kind
    }

    fn peek_at(&self, offset: usize) -> T {
        self.tokens[(self.pos + offset).min(self.tokens.len() - 1)].kind
    }

    fn span(&self) -> Span {
        self.shifted(self.token().span)
    }

    fn shifted(&self, span: Span) -> Span {
        Span { start: span.start + self.base, end: span.end + self.base }
    }

    fn previous_end(&self) -> u32 {
        if self.pos == 0 {
            return self.base;
        }
        self.tokens[self.pos - 1].span.end + self.base
    }

    fn since(&self, start: Span) -> Span {
        Span { start: start.start, end: self.previous_end().max(start.start) }
    }

    fn text_of_span(&self, span: Span) -> &'a str {
        let start = span.start.saturating_sub(self.base) as usize;
        let end = span.end.saturating_sub(self.base) as usize;
        self.text.get(start..end).unwrap_or("")
    }

    /// A token's text; empty for the tokens the lexer makes up, such as the last line's end.
    fn text_of(&self, token: Token) -> &'a str {
        self.text.get(token.span.range()).unwrap_or("")
    }

    fn bump(&mut self) -> Token {
        let token = self.token();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        token
    }

    fn at(&self, kind: T) -> bool {
        self.peek() == kind
    }

    fn eat(&mut self, kind: T) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    // Errors ------------------------------------------------------------------------

    fn report(
        &mut self,
        span: Span,
        message: String,
        expected: Vec<&'static str>,
        fix: Option<(Span, String)>,
        code: &'static str,
    ) {
        if self.reported {
            return;
        }
        self.reported = true;
        self.errors.push(SyntaxError { span, message, expected, fix, code });
    }

    fn found(&self) -> String {
        let token = self.token();
        match token.kind {
            T::Name | T::Int | T::Float | T::Str | T::Error => format!("`{}`", self.text_of(token)),
            kind => kind.describe().to_string(),
        }
    }

    fn error_here(&mut self, message: &str) {
        let found = self.found();
        let span = self.span();
        self.report(span, format!("{message}, found {found}"), vec![], None, "E0003");
    }

    /// Consume `kind` or report it missing, naming what it was expected after.
    fn expect(&mut self, kind: T, after: &str) -> bool {
        if self.eat(kind) {
            return true;
        }
        let found = self.found();
        let span = self.span();
        let fix = matches!(kind, T::Colon | T::RParen | T::RBracket | T::RBrace).then(|| {
            (Span { start: self.previous_end(), end: self.previous_end() }, kind.text().trim_matches('`').to_string())
        });
        self.report(
            span,
            format!("expected {} {after}, found {found}", kind.describe()),
            vec![kind.text()],
            fix,
            "E0003",
        );
        false
    }

    fn name(&mut self, what: &str) -> Ident {
        if self.at(T::Name) || self.peek().is_keyword() {
            let token = self.bump();
            return Ident { name: self.text_of(token).to_string(), span: self.shifted(token.span) };
        }
        let span = self.span();
        let found = self.found();
        self.report(span, format!("expected {what}, found {found}"), vec!["a name"], None, "E0003");
        Ident { name: String::new(), span }
    }

    /// Whether the last token ended a line, so a failed statement left nothing of it behind.
    fn at_line_start(&self) -> bool {
        self.pos > 0 && matches!(self.tokens[self.pos - 1].kind, T::Newline | T::Dedent)
    }

    /// Whether the next two tokens are `kind` twice with nothing between them: `&&`, `||`, `++`.
    fn doubled(&self, kind: T) -> bool {
        let at = self.pos.min(self.tokens.len() - 1);
        self.peek() == kind
            && self.peek_at(1) == kind
            && self.tokens.get(at + 1).is_some_and(|next| next.span.start == self.tokens[at].span.end)
    }

    /// `word` as a replacement for the tokens in `span`, keeping it apart from its neighbours.
    fn spaced(&self, span: Span, word: &str) -> String {
        let (start, end) = (span.start.saturating_sub(self.base) as usize, span.end.saturating_sub(self.base) as usize);
        let before =
            self.text.get(..start).and_then(|t| t.chars().next_back()).is_some_and(|c| !c.is_whitespace() && c != '(');
        let after = self.text.get(end..).and_then(|t| t.chars().next()).is_some_and(|c| !c.is_whitespace() && c != ')');
        format!("{}{word}{}", if before { " " } else { "" }, if after { " " } else { "" })
    }

    /// Report an operator from another language and read it as lotml's.
    fn foreign_operator(&mut self, kind: T, word: &str) -> Span {
        let first = self.span();
        self.bump();
        let mut span = first;
        if self.at(kind) && kind != T::Bang {
            span = Span { start: first.start, end: self.span().end };
            self.bump();
        }
        let written = self.text_of_span(span);
        let replacement = self.spaced(span, word);
        // Not `report`: the operator is understood, so the rest of the statement is still checked.
        self.errors.push(SyntaxError {
            span,
            message: format!("lotml writes `{word}`, not `{written}`"),
            expected: vec![],
            fix: Some((span, replacement)),
            code: "E0111",
        });
        span
    }

    /// Skip the rest of a logical line and any block it opens; used after an error.
    fn skip_line(&mut self) {
        let mut depth = 0usize;
        loop {
            match self.peek() {
                T::Eof => return,
                T::Indent => depth += 1,
                T::Dedent => {
                    if depth == 0 {
                        return;
                    }
                    depth -= 1;
                    if depth == 0 {
                        self.bump();
                        return;
                    }
                }
                T::Newline if depth == 0 => {
                    self.bump();
                    if self.at(T::Indent) {
                        continue;
                    }
                    return;
                }
                _ => {}
            }
            self.bump();
        }
    }

    /// After one pass of a loop over items, statements, methods or arms that began at `before`:
    /// if the pass read nothing, report it and read the token it stopped at, so the next pass
    /// starts further on and the loop always ends. The loops over lists end on their own, each
    /// pass reading a separator or stopping.
    fn past(&mut self, before: usize) {
        if self.pos == before && !self.at(T::Eof) {
            self.error_here("expected something lotml can read here");
            self.bump();
        }
    }

    // Items -------------------------------------------------------------------------

    fn module(&mut self) -> Module {
        let mut items = Vec::new();
        loop {
            self.reported = false;
            match self.peek() {
                T::Eof => break,
                T::Newline | T::Dedent => {
                    self.bump();
                }
                T::Indent => {
                    let span = self.span();
                    self.report(
                        span,
                        "a top-level item must start at column 1".into(),
                        vec![],
                        Some((span, String::new())),
                        "E0002",
                    );
                    self.bump();
                }
                _ => {
                    let before = self.pos;
                    items.push(self.item());
                    self.past(before);
                }
            }
        }
        Module { items }
    }

    fn item(&mut self) -> Item {
        let start = self.span();
        let before = self.pos;
        if self.at_keyword_habit("async") {
            return self.item();
        }
        let item = match self.peek() {
            T::Fn => Item::Fn(self.fn_def(!self.interface)),
            T::Type => self.type_def(),
            T::Impl => Item::Impl(self.impl_def()),
            T::Trait => Item::Trait(self.trait_def()),
            T::From | T::Import => Item::Import(self.import()),
            T::Test => Item::Test(self.test_def()),
            T::Name if self.interface && self.text_of(self.token()) == "class" => Item::Class(self.class_def()),
            _ => {
                self.unknown_item();
                Item::Error(self.since(start))
            }
        };
        if self.reported && (self.pos == before || !self.at_line_start()) {
            self.skip_line();
        }
        item
    }

    fn unknown_item(&mut self) {
        let token = self.token();
        let span = self.span();
        let text = self.text_of(token);
        let (message, fix, code) = match text {
            "def" => {
                ("lotml declares functions with `fn`, not `def`".to_string(), Some((span, "fn".to_string())), "E0101")
            }
            "class" => (
                "lotml has no classes: declare a record with `type` and its methods in `impl`".to_string(),
                None,
                "E0102",
            ),
            _ => (
                format!(
                    "expected an item — `fn`, `type`, `impl`, `trait`, `test`, `from` or `import` — found {}",
                    self.found()
                ),
                None,
                "E0003",
            ),
        };
        self.report(
            span,
            message,
            vec!["`fn`", "`type`", "`impl`", "`trait`", "`test`", "`from`", "`import`"],
            fix,
            code,
        );
        self.skip_line();
    }

    fn type_params(&mut self) -> Vec<TypeParam> {
        let mut params = Vec::new();
        if !self.eat(T::LBracket) {
            return params;
        }
        loop {
            let name = self.name("a type parameter");
            let bound = if self.eat(T::Colon) { Some(self.name("a trait bound")) } else { None };
            params.push(TypeParam { name, bound });
            if !self.eat(T::Comma) || self.at(T::RBracket) {
                break;
            }
        }
        self.expect(T::RBracket, "after the type parameters");
        params
    }

    fn fn_def(&mut self, needs_body: bool) -> FnDef {
        let start = self.span();
        self.bump(); // fn
        let name = self.name("the function's name");
        let type_params = self.type_params();
        let mut params = Vec::new();
        if self.expect(T::LParen, "after the function's name") {
            while !matches!(self.peek(), T::RParen | T::Newline | T::Eof | T::Colon) {
                params.push(self.param());
                if !self.eat(T::Comma) {
                    break;
                }
            }
            self.expect(T::RParen, "to close the parameters");
        }
        let (mut returns, mut error) = (None, None);
        if self.eat(T::Arrow) {
            returns = Some(self.type_expr());
            if self.eat(T::Bang) {
                error = Some(self.type_expr());
            }
        }
        let has_colon = self.at(T::Colon);
        let body = if has_colon || needs_body {
            if self.at(T::LBrace) {
                let span = self.span();
                self.report(
                    span,
                    "a lotml block starts with `:` and an indented line, not `{`".into(),
                    vec!["`:`"],
                    Some((span, ":".into())),
                    "E0103",
                );
                None
            } else if self.expect(T::Colon, "after the function's signature") {
                Some(self.block())
            } else {
                None
            }
        } else {
            self.eat(T::Newline);
            None
        };
        let doc = body.as_ref().and_then(|b| b.stmts.first()).and_then(|s| match &s.kind {
            StmtKind::Expr(Expr { kind: ExprKind::Str(parts), .. }) => Some(
                parts
                    .iter()
                    .flat_map(|l| &l.parts)
                    .map(|p| match p {
                        StrPart::Text(t) => t.as_str(),
                        StrPart::Expr { .. } => "",
                    })
                    .collect(),
            ),
            _ => None,
        });
        FnDef { span: self.since(start), name, type_params, params, returns, error, body, doc }
    }

    fn param(&mut self) -> Param {
        let start = self.span();
        let convention = match self.peek() {
            T::Inout => {
                self.bump();
                Convention::Inout
            }
            T::Sink => {
                self.bump();
                Convention::Sink
            }
            T::Var => {
                self.bump();
                Convention::Var
            }
            _ => Convention::Default,
        };
        let name = self.name("a parameter name");
        let ty = if self.eat(T::Colon) { Some(self.type_expr()) } else { None };
        let default = if self.eat(T::Eq) { Some(self.test()) } else { None };
        Param { span: self.since(start), convention, name, ty, default }
    }

    fn type_def(&mut self) -> Item {
        let start = self.span();
        self.bump(); // type
        let name = self.name("the type's name");
        let type_params = self.type_params();
        if self.eat(T::LParen) {
            let fields = self.fields(true);
            self.expect(T::RParen, "to close the record's fields");
            self.end_of_line("after the record");
            return Item::Record(RecordDef { span: self.since(start), name, type_params, fields });
        }
        if !self.expect(T::Eq, "after the type's name: `type T(fields)` or `type T = A | B`") {
            return Item::Error(self.since(start));
        }
        let mut variants = Vec::new();
        loop {
            let variant_start = self.span();
            let variant_name = self.name("a variant name");
            let fields = if self.eat(T::LParen) {
                let fields = self.fields(false);
                self.expect(T::RParen, "to close the variant's fields");
                Some(fields)
            } else {
                None
            };
            variants.push(Variant { span: self.since(variant_start), name: variant_name, fields });
            if !self.eat(T::Pipe) {
                break;
            }
        }
        self.end_of_line("after the sum type");
        Item::Sum(SumDef { span: self.since(start), name, type_params, variants })
    }

    /// Record fields are always named; a variant's may be positional types: `Num(int)`.
    fn fields(&mut self, named: bool) -> Vec<Field> {
        let mut fields = Vec::new();
        while !matches!(self.peek(), T::RParen | T::Newline | T::Eof) {
            let start = self.span();
            let field_name = if named || (self.at(T::Name) && self.peek_at(1) == T::Colon) {
                let name = self.name("a field name");
                self.expect(T::Colon, "after the field's name");
                Some(name)
            } else {
                None
            };
            let ty = self.type_expr();
            let default = if self.eat(T::Eq) { Some(self.test()) } else { None };
            fields.push(Field { span: self.since(start), name: field_name, ty, default });
            if !self.eat(T::Comma) {
                break;
            }
        }
        fields
    }

    fn end_of_line(&mut self, after: &str) {
        if !matches!(self.peek(), T::Eof | T::Dedent) {
            self.expect(T::Newline, after);
        }
    }

    fn impl_def(&mut self) -> ImplDef {
        let start = self.span();
        self.bump(); // impl
        let first = self.type_expr();
        let (trait_name, target) = if self.eat(T::For) { (Some(first), self.type_expr()) } else { (None, first) };
        let mut methods = Vec::new();
        if self.expect(T::Colon, "after the implemented type")
            && self.expect(T::Newline, "after `impl …:`")
            && self.expect(T::Indent, "for the methods")
        {
            while !matches!(self.peek(), T::Dedent | T::Eof) {
                self.reported = false;
                let before = self.pos;
                if self.at(T::Fn) {
                    methods.push(self.fn_def(true));
                    if self.reported {
                        self.skip_line();
                    }
                } else if self.eat(T::Newline) {
                } else {
                    self.error_here("expected a method (`fn`) in the `impl` block");
                    self.skip_line();
                }
                self.past(before);
            }
            self.eat(T::Dedent);
        }
        ImplDef { span: self.since(start), trait_name, target, methods }
    }

    /// `class C(B, …):` and a block of attribute lines (`name: T`) and bodyless `fn` signatures, or
    /// `class C(B, …)` alone for a class with no member: a Python class, read in an interface only
    /// (adr:0034).
    fn class_def(&mut self) -> ClassDef {
        let start = self.span();
        self.bump(); // class
        let name = self.name("the class's name");
        let mut bases = Vec::new();
        if self.eat(T::LParen) {
            while !matches!(self.peek(), T::RParen | T::Newline | T::Eof) {
                bases.push(self.name("a base class"));
                if !self.eat(T::Comma) {
                    break;
                }
            }
            self.expect(T::RParen, "to close the class's bases");
        }
        let mut attributes = Vec::new();
        let mut methods = Vec::new();
        // `class C` alone: a class with no member the interface declares.
        if !self.at(T::Colon) {
            self.end_of_line("after the class");
            return ClassDef { span: self.since(start), name, bases, attributes, methods };
        }
        if self.expect(T::Colon, "after the class's name")
            && self.expect(T::Newline, "after `class …:`")
            && self.expect(T::Indent, "for the class's attributes and methods")
        {
            while !matches!(self.peek(), T::Dedent | T::Eof) {
                self.reported = false;
                let before = self.pos;
                if self.at(T::Fn) {
                    methods.push(self.fn_def(false));
                    if self.reported {
                        self.skip_line();
                    }
                } else if self.at(T::Name) && self.peek_at(1) == T::Colon {
                    let field_start = self.span();
                    let field_name = self.name("an attribute's name");
                    self.bump(); // :
                    let ty = self.type_expr();
                    attributes.push(Field { span: self.since(field_start), name: Some(field_name), ty, default: None });
                    self.end_of_line("after the attribute");
                } else if self.eat(T::Newline) {
                } else {
                    self.error_here("expected an attribute (`name: T`) or a method signature (`fn`) in the class");
                    self.skip_line();
                }
                self.past(before);
            }
            self.eat(T::Dedent);
        }
        ClassDef { span: self.since(start), name, bases, attributes, methods }
    }

    fn trait_def(&mut self) -> TraitDef {
        let start = self.span();
        self.bump(); // trait
        let name = self.name("the trait's name");
        let type_params = self.type_params();
        let mut methods = Vec::new();
        if self.expect(T::Colon, "after the trait's name")
            && self.expect(T::Newline, "after `trait …:`")
            && self.expect(T::Indent, "for the trait's methods")
        {
            while !matches!(self.peek(), T::Dedent | T::Eof) {
                self.reported = false;
                let before = self.pos;
                if self.at(T::Fn) {
                    methods.push(self.fn_def(false));
                    if self.reported {
                        self.skip_line();
                    }
                } else if self.eat(T::Newline) {
                } else {
                    self.error_here("expected a method signature (`fn`) in the trait");
                    self.skip_line();
                }
                self.past(before);
            }
            self.eat(T::Dedent);
        }
        TraitDef { span: self.since(start), name, type_params, methods }
    }

    fn dotted(&mut self) -> Vec<Ident> {
        let mut path = vec![self.name("a module name")];
        while self.eat(T::Dot) {
            path.push(self.name("a module name"));
        }
        path
    }

    fn import(&mut self) -> Import {
        let start = self.span();
        if self.eat(T::Import) {
            let module = self.dotted();
            self.end_of_line("after the import");
            return Import { span: self.since(start), module, names: vec![] };
        }
        self.bump(); // from
        let module = self.dotted();
        let mut names = Vec::new();
        if self.expect(T::Import, "after the module's name") {
            if self.at(T::Star) {
                let span = self.span();
                self.report(
                    span,
                    "lotml has no `import *`: name what you import".into(),
                    vec!["a name"],
                    None,
                    "E0104",
                );
            } else {
                loop {
                    names.push(self.name("a name to import"));
                    if !self.eat(T::Comma) {
                        break;
                    }
                }
            }
        }
        self.end_of_line("after the import");
        Import { span: self.since(start), module, names }
    }

    fn test_def(&mut self) -> TestDef {
        let start = self.span();
        self.bump(); // test
        let (name, name_span) = if self.at(T::Str) {
            let token = self.bump();
            let literal = strings::split(self.text_of(token));
            let raw = &self.text_of(token)[literal.body.clone()];
            (strings::decode(raw, literal.raw).unwrap_or_default(), self.shifted(token.span))
        } else {
            let span = self.span();
            self.error_here("expected the test's name as a string");
            (String::new(), span)
        };
        let body = if self.expect(T::Colon, "after the test's name") {
            self.block()
        } else {
            Block { span: self.span(), stmts: vec![] }
        };
        TestDef { span: self.since(start), name, name_span, body }
    }

    // Statements --------------------------------------------------------------------

    /// After `:`: an indented block, or one simple statement on the same line.
    fn block(&mut self) -> Block {
        let start = self.span();
        if self.too_deep() {
            self.skip_line();
            return Block { span: self.since(start), stmts: vec![] };
        }
        self.depth += 1;
        let block = self.block_inner(start);
        self.depth -= 1;
        block
    }

    fn block_inner(&mut self, start: Span) -> Block {
        if !self.at(T::Newline) {
            let stmt = self.simple_stmt();
            return Block { span: self.since(start), stmts: vec![stmt] };
        }
        self.bump();
        if !self.at(T::Indent) {
            let span = self.span();
            let found = self.found();
            self.report(
                span,
                format!("expected an indented block, found {found}"),
                vec!["an indented block"],
                Some((Span { start: span.start, end: span.start }, "    ".into())),
                "E0002",
            );
            return Block { span: self.since(start), stmts: vec![] };
        }
        self.bump();
        let mut stmts = Vec::new();
        while !matches!(self.peek(), T::Dedent | T::Eof) {
            if self.eat(T::Newline) {
                continue;
            }
            let outer = self.reported;
            self.reported = false;
            let before = self.pos;
            stmts.push(self.stmt());
            if self.reported && (self.pos == before || !self.at_line_start()) {
                self.skip_line();
            }
            self.past(before);
            self.reported = outer && self.reported;
        }
        self.eat(T::Dedent);
        Block { span: self.since(start), stmts }
    }

    fn stmt(&mut self) -> Stmt {
        match self.peek() {
            T::If => self.if_stmt(),
            T::While => {
                let start = self.span();
                self.bump();
                let test = self.test();
                let body = self.colon_block("after the condition");
                Stmt { span: self.since(start), kind: StmtKind::While { test, body } }
            }
            T::For => {
                let start = self.span();
                self.bump();
                let target = self.targets();
                self.expect(T::In, "after the loop variables");
                let iter = self.testlist();
                let body = self.colon_block("after the loop header");
                Stmt { span: self.since(start), kind: StmtKind::For { target, iter, body } }
            }
            T::Match => self.match_stmt(),
            T::Indent => {
                let span = self.span();
                self.report(
                    span,
                    "this line is indented deeper than its block".into(),
                    vec![],
                    Some((span, String::new())),
                    "E0002",
                );
                self.bump();
                Stmt { span, kind: StmtKind::Error }
            }
            _ => self.simple_stmt(),
        }
    }

    fn colon_block(&mut self, after: &str) -> Block {
        if self.at(T::LBrace) {
            let span = self.span();
            self.report(
                span,
                "a lotml block starts with `:` and an indented line, not `{`".into(),
                vec!["`:`"],
                Some((span, ":".into())),
                "E0103",
            );
            return Block { span, stmts: vec![] };
        }
        if self.expect(T::Colon, after) { self.block() } else { Block { span: self.span(), stmts: vec![] } }
    }

    fn if_stmt(&mut self) -> Stmt {
        let start = self.span();
        self.bump(); // if
        let mut branches = Vec::new();
        let test = self.test();
        let body = self.colon_block("after the condition");
        branches.push((test, body));
        let mut orelse = None;
        loop {
            let another = self.at(T::Elif) || (self.at(T::Else) && self.peek_at(1) == T::If);
            if another && branches.len() == MAX_DEPTH as usize {
                // Each `elif` nests the rest of the chain one level deeper in what every target
                // runs: the Python target stops a few hundred levels down.
                let span = self.span();
                self.report(
                    span,
                    format!(
                        "this `if` has more than {MAX_DEPTH} branches, more than lotml nests; look the value up in a \
                         dict, or split the chain between functions"
                    ),
                    vec![],
                    None,
                    "E0003",
                );
            }
            if self.at(T::Elif) {
                self.bump();
                let test = self.test();
                let body = self.colon_block("after the condition");
                branches.push((test, body));
            } else if self.at(T::Else) {
                if self.peek_at(1) == T::If {
                    let span = Span { start: self.span().start, end: self.shifted(self.tokens[self.pos + 1].span).end };
                    self.report(
                        span,
                        "lotml writes `elif`, not `else if`".into(),
                        vec!["`elif`", "`:`"],
                        Some((span, "elif".into())),
                        "E0105",
                    );
                    self.bump();
                    self.bump();
                    let test = self.test();
                    let body = self.colon_block("after the condition");
                    branches.push((test, body));
                    continue;
                }
                self.bump();
                orelse = Some(self.colon_block("after `else`"));
                break;
            } else {
                break;
            }
        }
        Stmt { span: self.since(start), kind: StmtKind::If { branches, orelse } }
    }

    fn match_stmt(&mut self) -> Stmt {
        let start = self.span();
        self.bump(); // match
        let subject = self.test();
        let mut arms = Vec::new();
        if self.expect(T::Colon, "after the matched value")
            && self.expect(T::Newline, "after `match …:`")
            && self.expect(T::Indent, "for the `case` arms")
        {
            while !matches!(self.peek(), T::Dedent | T::Eof) {
                if self.eat(T::Newline) {
                    continue;
                }
                let outer = self.reported;
                self.reported = false;
                let before = self.pos;
                let arm_start = self.span();
                if !self.eat(T::Case) {
                    let span = self.span();
                    let found = self.found();
                    self.report(
                        span,
                        format!("expected `case` to start a match arm, found {found}"),
                        vec!["`case`"],
                        Some((Span { start: span.start, end: span.start }, "case ".into())),
                        "E0106",
                    );
                }
                let pattern = self.pattern();
                let body = self.colon_block("after the pattern");
                arms.push(Arm { span: self.since(arm_start), pattern, body });
                if self.reported {
                    self.skip_line();
                }
                self.past(before);
                self.reported = outer && self.reported;
            }
            self.eat(T::Dedent);
        }
        Stmt { span: self.since(start), kind: StmtKind::Match { subject, arms } }
    }

    fn pattern(&mut self) -> Pattern {
        let start = self.span();
        if self.too_deep() {
            return Pattern { span: self.span(), kind: PatternKind::Error };
        }
        self.depth += 1;
        let kind = match self.peek() {
            T::Name => {
                let name = self.name("a pattern");
                if name.name == "_" {
                    PatternKind::Wildcard
                } else if self.eat(T::LParen) {
                    let mut args = Vec::new();
                    while !matches!(self.peek(), T::RParen | T::Newline | T::Eof) {
                        args.push(self.pattern());
                        if !self.eat(T::Comma) {
                            break;
                        }
                    }
                    self.expect(T::RParen, "to close the pattern");
                    PatternKind::Variant { name, args }
                } else {
                    PatternKind::Name(name)
                }
            }
            T::LParen => {
                self.bump();
                let mut items = Vec::new();
                while !matches!(self.peek(), T::RParen | T::Newline | T::Eof) {
                    items.push(self.pattern());
                    if !self.eat(T::Comma) {
                        break;
                    }
                }
                self.expect(T::RParen, "to close the tuple pattern");
                PatternKind::Tuple(items)
            }
            T::Int | T::Float | T::Str | T::True | T::False | T::None | T::Minus => PatternKind::Literal(self.factor()),
            _ => {
                self.error_here("expected a pattern");
                PatternKind::Error
            }
        };
        self.depth -= 1;
        Pattern { span: self.since(start), kind }
    }

    fn simple_stmt(&mut self) -> Stmt {
        let start = self.span();
        let kind = match self.peek() {
            T::Var => {
                self.bump();
                let name = self.name("the variable's name");
                let ty = if self.eat(T::Colon) { Some(self.type_expr()) } else { None };
                self.expect(T::Eq, "in `var name = value`");
                let value = self.testlist();
                StmtKind::Var { name, ty, value }
            }
            T::Return => {
                self.bump();
                let value =
                    if matches!(self.peek(), T::Newline | T::Eof | T::Dedent) { None } else { Some(self.testlist()) };
                StmtKind::Return(value)
            }
            T::Assert => {
                self.bump();
                let test = self.test();
                let message = if self.eat(T::Comma) { Some(self.test()) } else { None };
                StmtKind::Assert { test, message }
            }
            T::Pass => {
                self.bump();
                StmtKind::Pass
            }
            T::Break => {
                self.bump();
                StmtKind::Break
            }
            T::Continue => {
                self.bump();
                StmtKind::Continue
            }
            T::Name
                if matches!(self.text_of(self.token()), "let" | "const")
                    && self.peek_at(1) == T::Name
                    && matches!(self.peek_at(2), T::Eq | T::Colon | T::Name) =>
            {
                self.declaration_keyword()
            }
            T::Name
                if matches!(
                    self.text_of(self.token()),
                    "raise" | "try" | "except" | "finally" | "with" | "def" | "class" | "global" | "nonlocal" | "yield"
                ) =>
            {
                self.python_statement();
                StmtKind::Error
            }
            _ => self.expression_stmt(),
        };
        let stmt = Stmt { span: self.since(start), kind };
        if !matches!(self.peek(), T::Eof | T::Dedent) && !self.reported {
            self.expect(T::Newline, "at the end of the statement");
        } else {
            self.eat(T::Newline);
        }
        stmt
    }

    /// `let x = …` and `const x = …` are `x = …`; `let mut x = …` is `var x = …`.
    fn declaration_keyword(&mut self) -> StmtKind {
        let first = self.span();
        let word = self.text_of(self.token());
        self.bump();
        let mutable = word == "let" && self.text_of(self.token()) == "mut" && self.peek_at(1) == T::Name;
        if mutable {
            self.bump();
        }
        let span = Span { start: first.start, end: self.span().start };
        let (message, replacement) = if mutable {
            ("lotml declares a variable that changes with `var`, not `let mut`".to_string(), "var ".to_string())
        } else {
            (
                format!("lotml declares a local by assigning it: drop `{word}`, and write `var` if it changes"),
                String::new(),
            )
        };
        self.errors.push(SyntaxError {
            span,
            message,
            expected: vec![],
            fix: Some((span, replacement)),
            code: "E0110",
        });
        if mutable {
            let name = self.name("the variable's name");
            let ty = if self.eat(T::Colon) { Some(self.type_expr()) } else { None };
            self.expect(T::Eq, "in `var name = value`");
            let value = self.testlist();
            return StmtKind::Var { name, ty, value };
        }
        self.expression_stmt()
    }

    fn python_statement(&mut self) {
        let span = self.span();
        let word = self.text_of(self.token());
        let message = match word {
            "raise" => "lotml has no exceptions: a function that can fail returns `T ! E` and uses `fail error`",
            "try" | "except" | "finally" => {
                "lotml has no exceptions: match on `Ok(v)` and `Err(e)`, or pass the error on with `?`"
            }
            "with" => "lotml has no `with` statement",
            "def" => "lotml declares functions with `fn`, and only at the top level or in `impl`",
            "class" => "lotml has no classes: declare a record with `type` and its methods in `impl`",
            "yield" => "lotml has no generators: build and return a list",
            _ => "lotml has no global mutable state",
        };
        let (fix, code) = match word {
            "raise" => (Some((span, "fail".to_string())), "E0107"),
            "def" => (Some((span, "fn".to_string())), "E0101"),
            "try" | "except" | "finally" => (None, "E0108"),
            "class" => (None, "E0102"),
            _ => (None, "E0109"),
        };
        self.report(span, message.into(), vec![], fix, code);
    }

    fn expression_stmt(&mut self) -> StmtKind {
        let target = self.testlist();
        if self.at(T::Colon) && matches!(target.kind, ExprKind::Name(_)) {
            self.bump();
            let ty = self.type_expr();
            self.expect(T::Eq, "in `name: type = value`");
            let value = self.test();
            let ExprKind::Name(name) = &target.kind else { unreachable!() };
            return StmtKind::Annotated { target: Ident { name: name.clone(), span: target.span }, ty, value };
        }
        if self.eat(T::Eq) {
            let value = self.testlist();
            return StmtKind::Assign { target, value };
        }
        for (kind, op, word) in [(T::Plus, BinOp::Add, "+= 1"), (T::Minus, BinOp::Sub, "-= 1")] {
            if self.doubled(kind) && matches!(self.peek_at(2), T::Newline | T::Eof | T::Dedent) {
                let first = self.span();
                self.bump();
                let end = self.span().end;
                self.bump();
                let span = Span { start: first.start, end };
                self.errors.push(SyntaxError {
                    span,
                    message: format!("lotml has no `{}`: write `{word}`", self.text_of_span(span)),
                    expected: vec![],
                    fix: Some((span, format!(" {word}"))),
                    code: "E0111",
                });
                let value = Expr { span, kind: ExprKind::Int("1".into()) };
                return StmtKind::AugAssign { target, op, value };
            }
        }
        let op = match self.peek() {
            T::PlusEq => Some(BinOp::Add),
            T::MinusEq => Some(BinOp::Sub),
            T::StarEq => Some(BinOp::Mul),
            T::SlashEq => Some(BinOp::Div),
            T::SlashSlashEq => Some(BinOp::FloorDiv),
            T::PercentEq => Some(BinOp::Mod),
            T::StarStarEq => Some(BinOp::Pow),
            T::AmpEq => Some(BinOp::BitAnd),
            T::PipeEq => Some(BinOp::BitOr),
            T::CaretEq => Some(BinOp::BitXor),
            T::LShiftEq => Some(BinOp::LShift),
            T::RShiftEq => Some(BinOp::RShift),
            _ => None,
        };
        if let Some(op) = op {
            self.bump();
            let value = self.test();
            return StmtKind::AugAssign { target, op, value };
        }
        StmtKind::Expr(target)
    }

    fn targets(&mut self) -> Target {
        let start = self.span();
        let mut items = vec![self.target()];
        while self.eat(T::Comma) {
            if self.at(T::In) {
                break;
            }
            items.push(self.target());
        }
        if items.len() == 1 {
            items.pop().unwrap_or(Target::Name(Ident { name: String::new(), span: start }))
        } else {
            Target::Tuple(items, self.since(start))
        }
    }

    fn target(&mut self) -> Target {
        let start = self.span();
        if self.eat(T::LParen) {
            let inner = self.targets();
            self.expect(T::RParen, "to close the loop variables");
            return match inner {
                Target::Tuple(items, _) => Target::Tuple(items, self.since(start)),
                single => single,
            };
        }
        Target::Name(self.name("a loop variable"))
    }

    // Expressions -------------------------------------------------------------------

    fn testlist(&mut self) -> Expr {
        let start = self.span();
        let first = self.test();
        if !self.at(T::Comma) {
            return first;
        }
        let mut items = vec![first];
        while self.eat(T::Comma) {
            if matches!(self.peek(), T::Newline | T::Eof | T::Eq | T::RParen | T::Colon) {
                break;
            }
            items.push(self.test());
        }
        Expr { span: self.since(start), kind: ExprKind::Tuple(items) }
    }

    fn test(&mut self) -> Expr {
        // Every sub-expression — a paren's content, a call argument, a subscript, an operand —
        // comes through here, so the depth guard sits here and bounds the whole recursion.
        if self.too_deep() {
            return Expr { span: self.span(), kind: ExprKind::Error };
        }
        self.depth += 1;
        let expr = self.test_inner();
        self.depth -= 1;
        expr
    }

    fn test_inner(&mut self) -> Expr {
        if self.at(T::Lambda) {
            return self.lambda();
        }
        let start = self.span();
        let value = self.coalesce();
        if self.at(T::If) && !self.reported {
            self.bump();
            let test = self.coalesce();
            self.expect(T::Else, "in `a if condition else b`");
            let orelse = self.test();
            return Expr {
                span: self.since(start),
                kind: ExprKind::IfExp { test: Box::new(test), then: Box::new(value), orelse: Box::new(orelse) },
            };
        }
        value
    }

    fn lambda(&mut self) -> Expr {
        let start = self.span();
        self.bump();
        let mut params = Vec::new();
        while self.at(T::Name) {
            params.push(self.name("a parameter"));
            if !self.eat(T::Comma) {
                break;
            }
        }
        self.expect(T::Colon, "after the lambda's parameters");
        let body = self.test();
        Expr { span: self.since(start), kind: ExprKind::Lambda { params, body: Box::new(body) } }
    }

    fn coalesce(&mut self) -> Expr {
        let start = self.span();
        let base = self.chain;
        let mut value = self.or_test();
        while self.at(T::QuestionQuestion) && !self.too_long() {
            self.bump();
            self.chain += 1;
            let default = self.or_test();
            value = Expr {
                span: self.since(start),
                kind: ExprKind::Coalesce { value: Box::new(value), default: Box::new(default) },
            };
        }
        self.chain = base;
        value
    }

    fn boolean(&mut self, op: BoolOp, token: T, next: fn(&mut Self) -> Expr) -> Expr {
        let start = self.span();
        let first = next(self);
        let (foreign, word) = if op == BoolOp::And { (T::Amp, "and") } else { (T::Pipe, "or") };
        if !self.at(token) && !self.doubled(foreign) {
            return first;
        }
        let mut operands = vec![first];
        loop {
            if self.doubled(foreign) {
                self.foreign_operator(foreign, word);
            } else if !self.eat(token) {
                break;
            }
            operands.push(next(self));
        }
        Expr { span: self.since(start), kind: ExprKind::Logical { op, operands } }
    }

    fn or_test(&mut self) -> Expr {
        self.boolean(BoolOp::Or, T::Or, Self::and_test)
    }

    fn and_test(&mut self) -> Expr {
        self.boolean(BoolOp::And, T::And, Self::not_test)
    }

    fn not_test(&mut self) -> Expr {
        let start = self.span();
        if self.at(T::Bang) {
            self.foreign_operator(T::Bang, "not");
            return self.negation(start);
        }
        if self.eat(T::Not) {
            return self.negation(start);
        }
        self.comparison()
    }

    fn negation(&mut self, start: Span) -> Expr {
        if self.too_long() {
            return Expr { span: self.span(), kind: ExprKind::Error };
        }
        self.chain += 1;
        let operand = self.not_test();
        self.chain -= 1;
        Expr { span: self.since(start), kind: ExprKind::Not(Box::new(operand)) }
    }

    fn comparison(&mut self) -> Expr {
        let start = self.span();
        let first = self.bit_or();
        let mut rest = Vec::new();
        loop {
            let op = match (self.peek(), self.peek_at(1)) {
                (T::Lt, _) => CmpOp::Lt,
                (T::Gt, _) => CmpOp::Gt,
                (T::Le, _) => CmpOp::Le,
                (T::Ge, _) => CmpOp::Ge,
                (T::EqEq, _) => CmpOp::Eq,
                (T::NotEq, _) => CmpOp::NotEq,
                (T::In, _) => CmpOp::In,
                (T::Not, T::In) => {
                    self.bump();
                    CmpOp::NotIn
                }
                (T::Is, T::Not) => {
                    self.bump();
                    CmpOp::IsNot
                }
                (T::Is, _) => CmpOp::Is,
                _ => break,
            };
            self.bump();
            rest.push((op, self.bit_or()));
        }
        if rest.is_empty() {
            return first;
        }
        Expr { span: self.since(start), kind: ExprKind::Compare { first: Box::new(first), rest } }
    }

    fn binary(&mut self, operators: &[(T, BinOp)], next: fn(&mut Self) -> Expr) -> Expr {
        let base = self.chain;
        let expr = self.folds(operators, next);
        self.chain = base;
        expr
    }

    fn folds(&mut self, operators: &[(T, BinOp)], next: fn(&mut Self) -> Expr) -> Expr {
        let start = self.span();
        let mut left = next(self);
        'outer: loop {
            let increment = (self.doubled(T::Plus) || self.doubled(T::Minus))
                && matches!(self.peek_at(2), T::Newline | T::Eof | T::Dedent);
            if self.doubled(T::Amp) || self.doubled(T::Pipe) || increment {
                return left;
            }
            for (token, op) in operators {
                if self.at(*token) {
                    if self.too_long() {
                        return left;
                    }
                    self.bump();
                    self.chain += 1;
                    let right = next(self);
                    left = Expr {
                        span: self.since(start),
                        kind: ExprKind::Binary { op: *op, left: Box::new(left), right: Box::new(right) },
                    };
                    continue 'outer;
                }
            }
            return left;
        }
    }

    fn bit_or(&mut self) -> Expr {
        self.binary(&[(T::Pipe, BinOp::BitOr)], Self::bit_xor)
    }

    fn bit_xor(&mut self) -> Expr {
        self.binary(&[(T::Caret, BinOp::BitXor)], Self::bit_and)
    }

    fn bit_and(&mut self) -> Expr {
        self.binary(&[(T::Amp, BinOp::BitAnd)], Self::shift)
    }

    fn shift(&mut self) -> Expr {
        self.binary(&[(T::LShift, BinOp::LShift), (T::RShift, BinOp::RShift)], Self::arith)
    }

    fn arith(&mut self) -> Expr {
        self.binary(&[(T::Plus, BinOp::Add), (T::Minus, BinOp::Sub)], Self::term)
    }

    fn term(&mut self) -> Expr {
        self.binary(
            &[
                (T::Star, BinOp::Mul),
                (T::Slash, BinOp::Div),
                (T::SlashSlash, BinOp::FloorDiv),
                (T::Percent, BinOp::Mod),
            ],
            Self::factor,
        )
    }

    /// Python's `async` before a function, or `await` before an expression: reported with the
    /// keyword removed, and stepped over so what follows is read as lotml.
    fn at_keyword_habit(&mut self, keyword: &str) -> bool {
        let follows = matches!(self.peek_at(1), T::Fn | T::Name | T::LParen | T::LBracket);
        if self.peek() != T::Name || self.text_of(self.token()) != keyword || !follows {
            return false;
        }
        let span = self.span();
        let next = self.shifted(self.tokens[(self.pos + 1).min(self.tokens.len() - 1)].span);
        self.report(
            span,
            format!("lotml has no `{keyword}`: no function is marked or awaited, and `parallel` runs tasks at once"),
            vec![],
            Some((Span { start: span.start, end: next.start }, String::new())),
            "E0112",
        );
        self.bump();
        true
    }

    fn factor(&mut self) -> Expr {
        if self.at_keyword_habit("await") {
            return self.factor();
        }
        let start = self.span();
        let op = match self.peek() {
            T::Minus => UnaryOp::Neg,
            T::Plus => UnaryOp::Pos,
            T::Tilde => UnaryOp::Invert,
            _ => return self.power(),
        };
        self.bump();
        if self.too_long() {
            return Expr { span: self.span(), kind: ExprKind::Error };
        }
        self.chain += 1;
        let operand = self.factor();
        self.chain -= 1;
        Expr { span: self.since(start), kind: ExprKind::Unary { op, operand: Box::new(operand) } }
    }

    fn power(&mut self) -> Expr {
        let start = self.span();
        let base = self.postfix();
        if self.at(T::StarStar) && !self.too_long() {
            self.bump();
            self.chain += 1;
            let exponent = self.factor();
            self.chain -= 1;
            return Expr {
                span: self.since(start),
                kind: ExprKind::Binary { op: BinOp::Pow, left: Box::new(base), right: Box::new(exponent) },
            };
        }
        base
    }

    fn postfix(&mut self) -> Expr {
        let base = self.chain;
        let expr = self.postfixes();
        self.chain = base;
        expr
    }

    fn postfixes(&mut self) -> Expr {
        let start = self.span();
        let mut expr = self.atom();
        loop {
            if matches!(self.peek(), T::LParen | T::LBracket | T::Dot | T::Question) {
                if self.too_long() {
                    return expr;
                }
                self.chain += 1;
            }
            match self.peek() {
                T::LParen => {
                    self.bump();
                    let args = self.arguments();
                    self.expect(T::RParen, "to close the call's arguments");
                    expr = Expr { span: self.since(start), kind: ExprKind::Call { func: Box::new(expr), args } };
                }
                T::LBracket => {
                    self.bump();
                    expr = self.subscript(expr, start);
                }
                T::Dot => {
                    self.bump();
                    let name = self.name("an attribute or method name");
                    expr = Expr { span: self.since(start), kind: ExprKind::Attr { object: Box::new(expr), name } };
                }
                T::Question => {
                    self.bump();
                    expr = Expr { span: self.since(start), kind: ExprKind::Try(Box::new(expr)) };
                }
                _ => return expr,
            }
        }
    }

    fn subscript(&mut self, object: Expr, start: Span) -> Expr {
        let lower = if matches!(self.peek(), T::Colon) { None } else { Some(Box::new(self.test())) };
        if !self.at(T::Colon) {
            self.expect(T::RBracket, "to close the index");
            let index = lower.unwrap_or_else(|| Box::new(Expr { span: self.span(), kind: ExprKind::Error }));
            return Expr { span: self.since(start), kind: ExprKind::Index { object: Box::new(object), index } };
        }
        self.bump();
        let upper = if matches!(self.peek(), T::Colon | T::RBracket) { None } else { Some(Box::new(self.test())) };
        let step = if self.eat(T::Colon) && !self.at(T::RBracket) { Some(Box::new(self.test())) } else { None };
        self.expect(T::RBracket, "to close the slice");
        Expr { span: self.since(start), kind: ExprKind::Slice { object: Box::new(object), lower, upper, step } }
    }

    fn arguments(&mut self) -> Vec<Arg> {
        let mut args = Vec::new();
        while !matches!(self.peek(), T::RParen | T::Newline | T::Eof) {
            if self.at(T::Name) && self.peek_at(1) == T::Eq {
                let name = self.name("an argument name");
                self.bump();
                args.push(Arg::Keyword(name, self.test()));
            } else if self.at(T::Amp) {
                let amp = self.span();
                self.bump();
                let place = self.postfix();
                args.push(Arg::Inout(place, amp));
            } else {
                let start = self.span();
                let value = self.test();
                if self.at(T::For) {
                    let loops = self.comprehensions();
                    args.push(Arg::Positional(Expr {
                        span: self.since(start),
                        kind: ExprKind::Generator { element: Box::new(value), loops },
                    }));
                } else {
                    args.push(Arg::Positional(value));
                }
            }
            if !self.eat(T::Comma) {
                break;
            }
        }
        args
    }

    fn comprehensions(&mut self) -> Vec<Comprehension> {
        let mut loops = Vec::new();
        while self.eat(T::For) {
            let target = self.targets();
            self.expect(T::In, "after the comprehension's variables");
            let iter = self.or_test();
            let mut conditions = Vec::new();
            while self.eat(T::If) {
                conditions.push(self.or_test());
            }
            loops.push(Comprehension { target, iter, conditions });
        }
        loops
    }

    fn sequence(&mut self, close: T) -> Vec<Expr> {
        let mut items = Vec::new();
        while !matches!(self.peek(), T::Eof) && !self.at(close) {
            items.push(self.test());
            if !self.eat(T::Comma) {
                break;
            }
        }
        items
    }

    fn atom(&mut self) -> Expr {
        let start = self.span();
        let token = self.token();
        let kind = match token.kind {
            kind if kind == T::Name
                || (kind.is_keyword()
                    && !matches!(kind, T::None | T::True | T::False | T::Not | T::Lambda | T::Fail)) =>
            {
                self.bump();
                ExprKind::Name(self.text_of(token).to_string())
            }
            T::Int => {
                self.bump();
                ExprKind::Int(self.text_of(token).replace('_', ""))
            }
            T::Float => {
                self.bump();
                ExprKind::Float(self.text_of(token).replace('_', ""))
            }
            T::Str => ExprKind::Str(self.strings()),
            T::True | T::False => {
                self.bump();
                ExprKind::Bool(token.kind == T::True)
            }
            T::None => {
                self.bump();
                ExprKind::None
            }
            T::Fail => {
                self.bump();
                let error = self.postfix();
                ExprKind::Fail(Box::new(error))
            }
            T::LParen => {
                self.bump();
                if self.eat(T::RParen) {
                    ExprKind::Unit
                } else {
                    let first = self.test();
                    if self.at(T::For) {
                        let loops = self.comprehensions();
                        self.expect(T::RParen, "to close the generator");
                        ExprKind::Generator { element: Box::new(first), loops }
                    } else if self.eat(T::Comma) {
                        let mut items = vec![first];
                        items.extend(self.sequence(T::RParen));
                        self.expect(T::RParen, "to close the tuple");
                        ExprKind::Tuple(items)
                    } else {
                        self.expect(T::RParen, "to close the parenthesis");
                        return Expr { span: self.since(start), kind: first.kind };
                    }
                }
            }
            T::LBracket => {
                self.bump();
                if self.eat(T::RBracket) {
                    ExprKind::List(vec![])
                } else {
                    let first = self.test();
                    if self.at(T::For) {
                        let loops = self.comprehensions();
                        self.expect(T::RBracket, "to close the list comprehension");
                        ExprKind::ListComp { element: Box::new(first), loops }
                    } else {
                        let mut items = vec![first];
                        if self.eat(T::Comma) {
                            items.extend(self.sequence(T::RBracket));
                        }
                        self.expect(T::RBracket, "to close the list");
                        ExprKind::List(items)
                    }
                }
            }
            T::LBrace => self.brace(),
            _ => {
                self.error_here("expected an expression");
                if !matches!(token.kind, T::Newline | T::Eof | T::Dedent | T::Indent) {
                    self.bump();
                }
                ExprKind::Error
            }
        };
        Expr { span: self.since(start), kind }
    }

    fn brace(&mut self) -> ExprKind {
        self.bump();
        if self.eat(T::RBrace) {
            return ExprKind::Dict(vec![]);
        }
        let first = self.test();
        if self.eat(T::Colon) {
            let value = self.test();
            if self.at(T::For) {
                let loops = self.comprehensions();
                self.expect(T::RBrace, "to close the dict comprehension");
                return ExprKind::DictComp { key: Box::new(first), value: Box::new(value), loops };
            }
            let mut pairs = vec![(first, value)];
            while self.eat(T::Comma) {
                if self.at(T::RBrace) {
                    break;
                }
                let key = self.test();
                self.expect(T::Colon, "between a key and its value");
                pairs.push((key, self.test()));
            }
            self.expect(T::RBrace, "to close the dict");
            return ExprKind::Dict(pairs);
        }
        if self.at(T::For) {
            let loops = self.comprehensions();
            self.expect(T::RBrace, "to close the set comprehension");
            return ExprKind::SetComp { element: Box::new(first), loops };
        }
        let mut items = vec![first];
        if self.eat(T::Comma) {
            items.extend(self.sequence(T::RBrace));
        }
        self.expect(T::RBrace, "to close the set");
        ExprKind::Set(items)
    }

    /// One or more adjacent string literals, concatenated as Python does.
    fn strings(&mut self) -> Vec<StrLit> {
        let mut literals = Vec::new();
        while self.at(T::Str) {
            let token = self.bump();
            literals.push(self.string(token));
        }
        literals
    }

    fn string(&mut self, token: Token) -> StrLit {
        let raw = self.text_of(token);
        let literal = strings::split(raw);
        let body = &raw[literal.body.clone()];
        let base = self.shifted(token.span).start + literal.body.start as u32;
        let mut parts = Vec::new();
        if literal.format {
            match strings::pieces(body) {
                Ok(pieces) => {
                    for piece in pieces {
                        match piece {
                            strings::Piece::Text(text) => {
                                parts.push(StrPart::Text(self.decoded(&text, literal.raw, base)))
                            }
                            strings::Piece::Field { expr, conversion, spec } => {
                                let (value, errors) = parse_expression(
                                    &body[expr.clone()],
                                    base + expr.start as u32,
                                    self.depth,
                                    self.chain,
                                );
                                self.errors.extend(errors);
                                let spec = spec.map(|s| vec![StrPart::Text(body[s].to_string())]).unwrap_or_default();
                                parts.push(StrPart::Expr { expr: Box::new(value), conversion, spec });
                            }
                        }
                    }
                }
                Err((offset, message)) => {
                    let at = base + offset as u32;
                    self.report(Span { start: at, end: at + 1 }, message, vec![], None, "E0004");
                }
            }
        } else {
            parts.push(StrPart::Text(self.decoded(body, literal.raw, base)));
        }
        StrLit { raw: raw.to_string(), bytes: literal.bytes, parts }
    }

    fn decoded(&mut self, text: &str, raw: bool, at: u32) -> String {
        match strings::decode(text, raw) {
            Ok(value) => value,
            Err(message) => {
                self.report(Span { start: at, end: at + text.len() as u32 }, message, vec![], None, "E0004");
                text.to_string()
            }
        }
    }

    // Types -------------------------------------------------------------------------

    fn type_expr(&mut self) -> TypeExpr {
        let start = self.span();
        if self.too_deep() {
            return TypeExpr { span: self.span(), kind: TypeKind::Error };
        }
        self.depth += 1;
        let mut ty = self.base_type();
        while self.eat(T::Question) {
            ty = TypeExpr { span: self.since(start), kind: TypeKind::Optional(Box::new(ty)) };
        }
        self.depth -= 1;
        ty
    }

    fn base_type(&mut self) -> TypeExpr {
        let start = self.span();
        let kind = match self.peek() {
            T::Name => {
                let name = self.name("a type");
                let mut args = Vec::new();
                if self.eat(T::LBracket) {
                    loop {
                        args.push(self.type_expr());
                        if !self.eat(T::Comma) {
                            break;
                        }
                    }
                    self.expect(T::RBracket, "to close the type arguments");
                }
                TypeKind::Named { name, args }
            }
            T::None => {
                self.bump();
                TypeKind::Unit
            }
            T::LBracket => {
                self.bump();
                let item = self.type_expr();
                self.expect(T::RBracket, "to close the list type");
                TypeKind::List(Box::new(item))
            }
            T::LBrace => {
                self.bump();
                let key = self.type_expr();
                if self.eat(T::Colon) {
                    let value = self.type_expr();
                    self.expect(T::RBrace, "to close the dict type");
                    TypeKind::Dict(Box::new(key), Box::new(value))
                } else {
                    self.expect(T::RBrace, "to close the set type");
                    TypeKind::Set(Box::new(key))
                }
            }
            T::LParen => {
                self.bump();
                if self.eat(T::RParen) {
                    TypeKind::Unit
                } else {
                    let mut items = vec![self.type_expr()];
                    while self.eat(T::Comma) {
                        if self.at(T::RParen) {
                            break;
                        }
                        items.push(self.type_expr());
                    }
                    self.expect(T::RParen, "to close the tuple type");
                    TypeKind::Tuple(items)
                }
            }
            T::Dyn => {
                self.bump();
                TypeKind::Dyn(self.name("a trait"))
            }
            _ => {
                self.error_here("expected a type");
                TypeKind::Error
            }
        };
        TypeExpr { span: self.since(start), kind }
    }
}
