//! Checking a file that is still being written. Completeness comes first: a prefix that some
//! continuation turns into a correct program is never rejected. A diagnostic later code could
//! resolve — a name or type declared further down, a method an `impl` below may add — is held,
//! and so is anything in the unfinished tail: the last logical line, and the last item, whose
//! body may still grow.

use lotml_diag::{Diagnostic, Severity};
use lotml_syntax::lexer::{TokenKind, lex};
use lotml_syntax::parse;

use crate::check_source;

/// What a prefix check can say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// No diagnostic at all: the prefix is a correct program so far.
    Completable,
    /// A diagnostic no continuation can remove.
    Error,
    /// Only diagnostics later code may still resolve: cannot tell yet.
    Unknown,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Completable => "completable",
            Verdict::Error => "error",
            Verdict::Unknown => "unknown",
        }
    }
}

pub struct PrefixCheck {
    pub verdict: Verdict,
    /// The errors that stand whatever comes next.
    pub errors: Vec<Diagnostic>,
    /// The errors held until the file is complete.
    pub held: Vec<Diagnostic>,
}

/// Codes that a declaration further down can make go away.
const LATER: &[&str] = &["E0201", "E0202", "E0205", "E0213"];

/// Codes about a body that is still being written: a missing return, a `match` without all its
/// arms yet, a result whose `?` has not been typed.
const UNFINISHED: &[&str] = &["E0206", "E0209", "E0219"];

pub fn check_prefix(source: &str) -> PrefixCheck {
    let tail = open_tail(source);
    let last_item = parse(source).module.items.last().map_or(source.len() as u32, |item| item.span().start);
    // The tail can change what came before it — a signature still being typed changes every
    // call above — so an error stands only if it is there without the tail too.
    let without_tail = if (tail as usize) < source.len() { check_source(&source[..tail as usize]) } else { Vec::new() };
    let stands = |d: &Diagnostic| {
        (tail as usize) >= source.len()
            || without_tail.iter().any(|w| w.code == d.code && w.span == d.span && w.message == d.message)
    };
    let (mut errors, mut held) = (Vec::new(), Vec::new());
    for d in check_source(source) {
        if d.severity != Severity::Error {
            continue;
        }
        let in_tail = d.span.start >= tail;
        let later = LATER.contains(&d.code);
        let unfinished = UNFINISHED.contains(&d.code) && d.span.start >= last_item;
        if in_tail || later || unfinished || !stands(&d) { held.push(d) } else { errors.push(d) }
    }
    let verdict = if !errors.is_empty() {
        Verdict::Error
    } else if !held.is_empty() {
        Verdict::Unknown
    } else {
        Verdict::Completable
    };
    PrefixCheck { verdict, errors, held }
}

/// Where the unfinished tail starts: the first token of the last logical line, unless that
/// line ended with a newline outside every bracket.
fn open_tail(source: &str) -> u32 {
    let tokens = lex(source).tokens;
    let mut depth = 0usize;
    let mut start: Option<u32> = None;
    let mut line_open = false;
    for token in &tokens {
        match token.kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => depth += 1,
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => depth = depth.saturating_sub(1),
            _ => {}
        }
        match token.kind {
            // A newline the lexer made up at the end of the text does not end the line.
            TokenKind::Newline if depth == 0 && !token.span.is_empty() => line_open = false,
            TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent | TokenKind::Eof => {}
            _ if !line_open => {
                line_open = true;
                start = Some(token.span.start);
            }
            _ => {}
        }
    }
    match start {
        Some(start) if line_open || depth > 0 => start,
        _ => source.len() as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tail_is_the_last_unterminated_line() {
        assert_eq!(open_tail("x = 1\ny = 2"), 6);
        assert_eq!(open_tail("x = 1\n"), 6);
        assert_eq!(open_tail("x = f(1,\n  2"), 0);
        assert_eq!(open_tail(""), 0);
    }
}
