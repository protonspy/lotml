//! The lexer: source text to tokens, with indentation made into `Indent` and `Dedent` tokens
//! the way Python's tokenizer does, and line breaks inside brackets ignored.
//!
//! Positions are UTF-8 byte offsets. Comments are not tokens; they are collected separately
//! so the formatter can put them back.

use crate::span::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TokenKind {
    Name,
    Int,
    Float,
    Str,
    // Keywords.
    Fn,
    Type,
    Impl,
    Trait,
    For,
    Test,
    From,
    Import,
    Var,
    Inout,
    Sink,
    Return,
    If,
    Elif,
    Else,
    While,
    In,
    Match,
    Case,
    Lambda,
    And,
    Or,
    Not,
    Is,
    None,
    True,
    False,
    Fail,
    Pass,
    Break,
    Continue,
    Assert,
    Dyn,
    // Punctuation.
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Dot,
    Arrow,
    Question,
    QuestionQuestion,
    Bang,
    Amp,
    Pipe,
    Caret,
    Tilde,
    Plus,
    Minus,
    Star,
    Slash,
    SlashSlash,
    Percent,
    StarStar,
    LShift,
    RShift,
    Lt,
    Gt,
    Le,
    Ge,
    EqEq,
    NotEq,
    Eq,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    SlashSlashEq,
    PercentEq,
    StarStarEq,
    AmpEq,
    PipeEq,
    CaretEq,
    LShiftEq,
    RShiftEq,
    // Layout.
    Newline,
    Indent,
    Dedent,
    Eof,
    /// Text no token matches; the lexer reports it and moves on.
    Error,
}

impl TokenKind {
    /// A keyword is soft: where the grammar expects no keyword, it is a name, as in
    /// `fn histogram(test: str)` — the same language the published grammar's lexer reads.
    pub fn is_keyword(self) -> bool {
        use TokenKind::*;
        matches!(
            self,
            Fn | Type
                | Impl
                | Trait
                | For
                | Test
                | From
                | Import
                | Var
                | Inout
                | Sink
                | Return
                | If
                | Elif
                | Else
                | While
                | In
                | Match
                | Case
                | Lambda
                | And
                | Or
                | Not
                | Is
                | None
                | True
                | False
                | Fail
                | Pass
                | Break
                | Continue
                | Assert
                | Dyn
        )
    }

    pub fn keyword(text: &str) -> Option<TokenKind> {
        use TokenKind::*;
        Some(match text {
            "fn" => Fn,
            "type" => Type,
            "impl" => Impl,
            "trait" => Trait,
            "for" => For,
            "test" => Test,
            "from" => From,
            "import" => Import,
            "var" => Var,
            "inout" => Inout,
            "sink" => Sink,
            "return" => Return,
            "if" => If,
            "elif" => Elif,
            "else" => Else,
            "while" => While,
            "in" => In,
            "match" => Match,
            "case" => Case,
            "lambda" => Lambda,
            "and" => And,
            "or" => Or,
            "not" => Not,
            "is" => Is,
            "None" => None,
            "True" => True,
            "False" => False,
            "fail" => Fail,
            "pass" => Pass,
            "break" => Break,
            "continue" => Continue,
            "assert" => Assert,
            "dyn" => Dyn,
            _ => return Option::None,
        })
    }

    /// How the token is written, for messages: `` `:` ``, `a name`, `the end of the file`.
    pub fn describe(self) -> &'static str {
        use TokenKind::*;
        match self {
            Name => "a name",
            Int => "an integer",
            Float => "a float",
            Str => "a string",
            Newline => "the end of the line",
            Indent => "an indented block",
            Dedent => "the end of the block",
            Eof => "the end of the file",
            Error => "an unknown character",
            other => other.text(),
        }
    }

    /// The fixed text of a keyword or punctuation token.
    pub fn text(self) -> &'static str {
        use TokenKind::*;
        match self {
            Fn => "`fn`",
            Type => "`type`",
            Impl => "`impl`",
            Trait => "`trait`",
            For => "`for`",
            Test => "`test`",
            From => "`from`",
            Import => "`import`",
            Var => "`var`",
            Inout => "`inout`",
            Sink => "`sink`",
            Return => "`return`",
            If => "`if`",
            Elif => "`elif`",
            Else => "`else`",
            While => "`while`",
            In => "`in`",
            Match => "`match`",
            Case => "`case`",
            Lambda => "`lambda`",
            And => "`and`",
            Or => "`or`",
            Not => "`not`",
            Is => "`is`",
            None => "`None`",
            True => "`True`",
            False => "`False`",
            Fail => "`fail`",
            Pass => "`pass`",
            Break => "`break`",
            Continue => "`continue`",
            Assert => "`assert`",
            Dyn => "`dyn`",
            LParen => "`(`",
            RParen => "`)`",
            LBracket => "`[`",
            RBracket => "`]`",
            LBrace => "`{`",
            RBrace => "`}`",
            Comma => "`,`",
            Colon => "`:`",
            Dot => "`.`",
            Arrow => "`->`",
            Question => "`?`",
            QuestionQuestion => "`??`",
            Bang => "`!`",
            Amp => "`&`",
            Pipe => "`|`",
            Caret => "`^`",
            Tilde => "`~`",
            Plus => "`+`",
            Minus => "`-`",
            Star => "`*`",
            Slash => "`/`",
            SlashSlash => "`//`",
            Percent => "`%`",
            StarStar => "`**`",
            LShift => "`<<`",
            RShift => "`>>`",
            Lt => "`<`",
            Gt => "`>`",
            Le => "`<=`",
            Ge => "`>=`",
            EqEq => "`==`",
            NotEq => "`!=`",
            Eq => "`=`",
            PlusEq => "`+=`",
            MinusEq => "`-=`",
            StarEq => "`*=`",
            SlashEq => "`/=`",
            SlashSlashEq => "`//=`",
            PercentEq => "`%=`",
            StarStarEq => "`**=`",
            AmpEq => "`&=`",
            PipeEq => "`|=`",
            CaretEq => "`^=`",
            LShiftEq => "`<<=`",
            RShiftEq => "`>>=`",
            _ => "",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LexError {
    pub span: Span,
    pub message: String,
    /// A fix that is certainly right, as replacement text for `span`.
    pub fix: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lexed {
    pub tokens: Vec<Token>,
    pub comments: Vec<Span>,
    pub errors: Vec<LexError>,
}

/// Columns of indentation: a space is one, a tab moves to the next multiple of four.
pub const TAB: usize = 4;

const OPERATORS: &[(&str, TokenKind)] = &[
    ("**=", TokenKind::StarStarEq),
    ("//=", TokenKind::SlashSlashEq),
    ("<<=", TokenKind::LShiftEq),
    (">>=", TokenKind::RShiftEq),
    ("->", TokenKind::Arrow),
    ("??", TokenKind::QuestionQuestion),
    ("**", TokenKind::StarStar),
    ("//", TokenKind::SlashSlash),
    ("<<", TokenKind::LShift),
    (">>", TokenKind::RShift),
    ("<=", TokenKind::Le),
    (">=", TokenKind::Ge),
    ("==", TokenKind::EqEq),
    ("!=", TokenKind::NotEq),
    ("+=", TokenKind::PlusEq),
    ("-=", TokenKind::MinusEq),
    ("*=", TokenKind::StarEq),
    ("/=", TokenKind::SlashEq),
    ("%=", TokenKind::PercentEq),
    ("&=", TokenKind::AmpEq),
    ("|=", TokenKind::PipeEq),
    ("^=", TokenKind::CaretEq),
    ("(", TokenKind::LParen),
    (")", TokenKind::RParen),
    ("[", TokenKind::LBracket),
    ("]", TokenKind::RBracket),
    ("{", TokenKind::LBrace),
    ("}", TokenKind::RBrace),
    (",", TokenKind::Comma),
    (":", TokenKind::Colon),
    (".", TokenKind::Dot),
    ("?", TokenKind::Question),
    ("!", TokenKind::Bang),
    ("&", TokenKind::Amp),
    ("|", TokenKind::Pipe),
    ("^", TokenKind::Caret),
    ("~", TokenKind::Tilde),
    ("+", TokenKind::Plus),
    ("-", TokenKind::Minus),
    ("*", TokenKind::Star),
    ("/", TokenKind::Slash),
    ("%", TokenKind::Percent),
    ("<", TokenKind::Lt),
    (">", TokenKind::Gt),
    ("=", TokenKind::Eq),
];

/// The keywords a definition begins with, and the habits from Python the parser reports at the
/// same place: at column 0 they end any bracket left open.
const DEFINITIONS: &[&str] = &["fn", "type", "impl", "trait", "test", "from", "import", "async", "def", "class"];

struct Lexer<'a> {
    text: &'a str,
    bytes: &'a [u8],
    pos: usize,
    /// The brackets open at this point, each with where it opened: inside one, newlines and
    /// indentation are not tokens.
    open: Vec<(u8, usize)>,
    levels: Vec<usize>,
    out: Lexed,
    at_line_start: bool,
}

pub fn lex(text: &str) -> Lexed {
    let mut lexer = Lexer {
        text,
        bytes: text.as_bytes(),
        pos: 0,
        open: Vec::new(),
        levels: vec![0],
        out: Lexed::default(),
        at_line_start: true,
    };
    lexer.run();
    lexer.out
}

impl Lexer<'_> {
    fn push(&mut self, kind: TokenKind, start: usize, end: usize) {
        self.out.tokens.push(Token { kind, span: Span::new(start, end) });
    }

    fn error(&mut self, start: usize, end: usize, message: impl Into<String>, fix: Option<String>) {
        self.out.errors.push(LexError { span: Span::new(start, end), message: message.into(), fix });
    }

    fn peek(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    fn last_kind(&self) -> Option<TokenKind> {
        self.out.tokens.last().map(|t| t.kind)
    }

    fn run(&mut self) {
        while self.pos < self.bytes.len() {
            if self.at_line_start && self.open.is_empty() {
                self.indentation();
                if self.pos >= self.bytes.len() {
                    break;
                }
            }
            let c = self.bytes[self.pos];
            match c {
                b' ' | b'\t' | b'\x0c' => self.pos += 1,
                b'\r' if self.peek(1) == Some(b'\n') => self.pos += 1,
                b'\n' | b'\r' => {
                    if !self.open.is_empty() && self.definition_at(self.pos + 1) {
                        self.close_unclosed();
                    }
                    if self.open.is_empty() && !matches!(self.last_kind(), None | Some(TokenKind::Newline)) {
                        self.push(TokenKind::Newline, self.pos, self.pos + 1);
                    }
                    self.pos += 1;
                    self.at_line_start = true;
                }
                b'#' => self.comment(),
                b'\\' if matches!(self.peek(1), Some(b'\n' | b'\r')) => {
                    self.pos += 1;
                    if self.peek(0) == Some(b'\r') {
                        self.pos += 1;
                    }
                    if self.peek(0) == Some(b'\n') {
                        self.pos += 1;
                    }
                }
                b'"' | b'\'' => self.string(self.pos),
                b'0'..=b'9' => self.number(),
                b'.' if matches!(self.peek(1), Some(b'0'..=b'9')) => self.number(),
                c if c == b'_' || c.is_ascii_alphabetic() || c >= 0x80 => self.word(),
                _ => self.operator(),
            }
        }
        let end = self.bytes.len();
        if !matches!(self.last_kind(), None | Some(TokenKind::Newline | TokenKind::Dedent)) {
            self.push(TokenKind::Newline, end, end);
        }
        while self.levels.len() > 1 {
            self.levels.pop();
            self.push(TokenKind::Dedent, end, end);
        }
        self.push(TokenKind::Eof, end, end);
    }

    /// Whether a definition starts at `at`, at the start of a line: at column 0, a keyword an item
    /// begins with, then a space and the name or string the item goes on with. A bracket still
    /// open there was never closed, since no expression goes on that way.
    fn definition_at(&self, at: usize) -> bool {
        let rest = self.bytes.get(at..).unwrap_or_default();
        DEFINITIONS.iter().any(|word| {
            let Some(after) = rest.strip_prefix(word.as_bytes()) else { return false };
            let gap = after.iter().take_while(|c| matches!(c, b' ' | b'\t')).count();
            gap > 0
                && after
                    .get(gap)
                    .is_some_and(|&c| c == b'_' || c == b'"' || c == b'\'' || c.is_ascii_alphabetic() || c >= 0x80)
        })
    }

    /// Report each bracket still open as never closed, and close them, so the line ahead is read
    /// as the start of a definition rather than as more of the expression.
    fn close_unclosed(&mut self) {
        for (bracket, start) in std::mem::take(&mut self.open) {
            self.error(start, start + 1, format!("this `{}` is never closed", bracket as char), None);
        }
    }

    /// At the start of a logical line: measure its indentation and compare with the open levels.
    fn indentation(&mut self) {
        let start = self.pos;
        let mut column = 0;
        while let Some(c) = self.peek(0) {
            match c {
                b' ' => column += 1,
                b'\t' => column = (column / TAB + 1) * TAB,
                b'\x0c' => column = 0,
                _ => break,
            }
            self.pos += 1;
        }
        self.at_line_start = false;
        match self.peek(0) {
            None | Some(b'\n' | b'\r' | b'#') => return,
            _ => {}
        }
        let current = *self.levels.last().unwrap_or(&0);
        if column > current {
            self.levels.push(column);
            self.push(TokenKind::Indent, start, self.pos);
        } else if column < current {
            while column < *self.levels.last().unwrap_or(&0) {
                self.levels.pop();
                self.push(TokenKind::Dedent, self.pos, self.pos);
            }
            let level = *self.levels.last().unwrap_or(&0);
            if column != level {
                let fix = " ".repeat(level);
                self.error(
                    start,
                    self.pos,
                    format!(
                        "this line is indented {column} columns, which matches no enclosing \
                         block; the block around it starts at column {level}"
                    ),
                    Some(fix),
                );
            }
        }
    }

    fn comment(&mut self) {
        let start = self.pos;
        while let Some(c) = self.peek(0) {
            if c == b'\n' || c == b'\r' {
                break;
            }
            self.pos += 1;
        }
        self.out.comments.push(Span::new(start, self.pos));
    }

    fn word(&mut self) {
        let start = self.pos;
        while let Some(c) = self.peek(0) {
            if c == b'_' || c.is_ascii_alphanumeric() || c >= 0x80 {
                self.pos += 1;
            } else {
                break;
            }
        }
        // String prefixes: r"…", b'…', f"…", rb"…" and their capitals.
        if matches!(self.peek(0), Some(b'"' | b'\'')) {
            let prefix = &self.text[start..self.pos];
            if prefix.len() <= 2 && prefix.chars().all(|c| "rRbBfF".contains(c)) {
                self.string(start);
                return;
            }
        }
        let text = &self.text[start..self.pos];
        let kind = TokenKind::keyword(text).unwrap_or(TokenKind::Name);
        self.push(kind, start, self.pos);
    }

    fn string(&mut self, start: usize) {
        let quote = self.bytes[self.pos];
        let triple = self.peek(1) == Some(quote) && self.peek(2) == Some(quote);
        self.pos += if triple { 3 } else { 1 };
        loop {
            match self.peek(0) {
                None => {
                    self.error(start, self.pos, "this string is never closed", None);
                    self.push(TokenKind::Error, start, self.pos);
                    return;
                }
                Some(b'\\') => {
                    // The escaped character whole, so a string's span never ends inside one, nor
                    // past the text when the backslash is its last character.
                    self.pos += 1;
                    self.pos +=
                        self.text.get(self.pos..).and_then(|rest| rest.chars().next()).map_or(0, char::len_utf8);
                }
                Some(b'\n' | b'\r') if !triple => {
                    self.error(start, self.pos, "this string is never closed on its line", None);
                    self.push(TokenKind::Error, start, self.pos);
                    return;
                }
                Some(c) if c == quote => {
                    if !triple {
                        self.pos += 1;
                        break;
                    }
                    if self.peek(1) == Some(quote) && self.peek(2) == Some(quote) {
                        self.pos += 3;
                        break;
                    }
                    self.pos += 1;
                }
                Some(_) => self.pos += 1,
            }
        }
        self.pos = self.pos.min(self.bytes.len());
        self.push(TokenKind::Str, start, self.pos);
    }

    fn number(&mut self) {
        let start = self.pos;
        let digits = |lexer: &mut Self, valid: fn(u8) -> bool| {
            while let Some(c) = lexer.peek(0) {
                if valid(c) || c == b'_' {
                    lexer.pos += 1;
                } else {
                    break;
                }
            }
        };
        if self.peek(0) == Some(b'0') && matches!(self.peek(1), Some(b'x' | b'X' | b'b' | b'B' | b'o' | b'O')) {
            let base = self.peek(1).unwrap_or(b'x').to_ascii_lowercase();
            self.pos += 2;
            match base {
                b'x' => digits(self, |c| c.is_ascii_hexdigit()),
                b'b' => digits(self, |c| c == b'0' || c == b'1'),
                _ => digits(self, |c| (b'0'..=b'7').contains(&c)),
            }
            self.push(TokenKind::Int, start, self.pos);
            return;
        }
        let mut float = false;
        digits(self, |c| c.is_ascii_digit());
        if self.peek(0) == Some(b'.') && matches!(self.peek(1), Some(b'0'..=b'9')) {
            float = true;
            self.pos += 1;
            digits(self, |c| c.is_ascii_digit());
        }
        if matches!(self.peek(0), Some(b'e' | b'E'))
            && (matches!(self.peek(1), Some(b'0'..=b'9'))
                || (matches!(self.peek(1), Some(b'+' | b'-')) && matches!(self.peek(2), Some(b'0'..=b'9'))))
        {
            float = true;
            self.pos += if matches!(self.peek(1), Some(b'+' | b'-')) { 2 } else { 1 };
            digits(self, |c| c.is_ascii_digit());
        }
        self.push(if float { TokenKind::Float } else { TokenKind::Int }, start, self.pos);
    }

    fn operator(&mut self) {
        let rest = &self.text[self.pos..];
        for (text, kind) in OPERATORS {
            if rest.starts_with(text) {
                let start = self.pos;
                self.pos += text.len();
                match kind {
                    TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                        self.open.push((self.bytes[start], start));
                    }
                    TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                        self.open.pop();
                    }
                    _ => {}
                }
                self.push(*kind, start, self.pos);
                return;
            }
        }
        let start = self.pos;
        let width = rest.chars().next().map_or(1, char::len_utf8);
        self.pos += width;
        self.error(start, self.pos, format!("`{}` is not part of lotml", &rest[..width]), None);
        self.push(TokenKind::Error, start, self.pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use TokenKind::*;

    fn kinds(text: &str) -> Vec<TokenKind> {
        lex(text).tokens.iter().map(|t| t.kind).collect()
    }

    #[test]
    fn blocks_become_indent_and_dedent() {
        assert_eq!(
            kinds("fn f():\n    if x:\n        y\n    z\n"),
            vec![
                Fn, Name, LParen, RParen, Colon, Newline, Indent, If, Name, Colon, Newline, Indent, Name, Newline,
                Dedent, Name, Newline, Dedent, Eof
            ]
        );
    }

    #[test]
    fn brackets_join_lines_and_blank_lines_and_comments_are_skipped() {
        let lexed = lex("x = [1,\n  2]  # two\n\n# alone\ny\n");
        assert_eq!(
            lexed.tokens.iter().map(|t| t.kind).collect::<Vec<_>>(),
            vec![Name, Eq, LBracket, Int, Comma, Int, RBracket, Newline, Name, Newline, Eof]
        );
        assert_eq!(lexed.comments.len(), 2);
    }

    #[test]
    fn keywords_operators_and_literals() {
        assert_eq!(
            kinds("a ?? b ** 2 // 3 <<= x != None\n"),
            vec![
                Name,
                QuestionQuestion,
                Name,
                StarStar,
                Int,
                SlashSlash,
                Int,
                LShiftEq,
                Name,
                NotEq,
                None,
                Newline,
                Eof
            ]
        );
        assert_eq!(
            kinds("0x1F 1_000 1.5e-3 .5 f\"x\" rb'y' \"\"\"a\nb\"\"\"\n"),
            vec![Int, Int, Float, Float, Str, Str, Str, Newline, Eof]
        );
    }

    #[test]
    fn spans_are_utf8_byte_offsets() {
        let lexed = lex("s = \"é\" + t\n");
        let t = lexed.tokens[4];
        assert_eq!((t.kind, t.span.start, t.span.end), (Name, 11, 12));
    }

    #[test]
    fn a_dedent_to_no_level_is_reported_with_its_fix() {
        let lexed = lex("fn f():\n    if x:\n        y\n      z\n");
        assert_eq!(lexed.errors.len(), 1);
        assert_eq!(lexed.errors[0].fix.as_deref(), Some("    "));
    }

    #[test]
    fn unclosed_strings_and_unknown_characters_are_errors() {
        let lexed = lex("x = \"abc\ny = $\n");
        assert_eq!(lexed.errors.len(), 2);
        assert!(lexed.tokens.iter().any(|t| t.kind == Error));
    }

    #[test]
    fn an_escape_never_carries_a_string_s_span_past_the_text_or_into_a_character() {
        for text in ["x = 'abc\\", "x = \"\\", "x = \"\\é\"", "x = '\\é"] {
            let lexed = lex(text);
            for token in &lexed.tokens {
                let end = token.span.end as usize;
                assert!(end <= text.len() && text.is_char_boundary(end), "{text:?}: {token:?}");
            }
            for error in &lexed.errors {
                let end = error.span.end as usize;
                assert!(end <= text.len() && text.is_char_boundary(end), "{text:?}: {error:?}");
            }
        }
        let escaped = lex("x = \"\\é\"");
        assert!(escaped.errors.is_empty(), "{:?}", escaped.errors);
    }
}
