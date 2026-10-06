//! Edits as an agent makes them. An edit addressed to a symbol — a function, a method, a type,
//! the body of either, or one `match` arm — takes its indentation from the target, so the agent
//! never reproduces the whitespace around it; a search-and-replace edit matches whole lines, up
//! to one indentation offset common to all of them. Either is refused when it would break the
//! syntax, with what was attempted and what was there.

use lotml_diag::Diagnostic;
use lotml_syntax::ast::*;
use lotml_syntax::lexer::TokenKind;
use lotml_syntax::parse;
use lotml_syntax::span::{Span, line_column};

/// What part of a symbol an edit replaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    /// The whole declaration: a function or method with its signature, a type, a trait.
    Definition,
    /// A function's or method's statements, its documentation string included.
    Body,
    /// The `match` arm of a function or method whose pattern is this — written in full,
    /// `Circle(r)`, or by its variant alone, `Circle`.
    Arm(String),
}

/// Why an edit was not made, said so the agent can correct it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused(pub String);

/// A file's text with an edit made in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Changed {
    pub text: String,
    /// What the edit wrote, in the new text.
    pub span: Span,
    /// The text it replaced.
    pub original: String,
}

/// Replace a part of the symbol named `symbol`: `area`, `Shape`, or `Counter.get`.
pub fn replace(text: &str, symbol: &str, part: &Part, new: &str) -> Result<Changed, Refused> {
    let module = parse(text).module;
    let found = find(&module, text, symbol)?;
    let span = match (part, &found) {
        (Part::Definition, Found::Item(item)) => Span { start: item.span().start, end: item.end() },
        (Part::Definition, Found::Function(f)) => Span { start: f.span.start, end: f.end() },
        (Part::Body, Found::Function(f)) => body(f, text, symbol)?,
        (Part::Arm(pattern), Found::Function(f)) => arm(f, text, symbol, pattern)?,
        (_, Found::Item(_)) => {
            return Err(Refused(format!("`{symbol}` is a type or trait: replace its whole definition")));
        }
    };
    let header = !matches!(part, Part::Body);
    splice(text, span, &reindent(new, &indentation(text, span.start), header, false))
}

/// Add a declaration after the one named `after` — a method after a method, inside its
/// `impl` — or at the end of the file.
pub fn add(text: &str, after: Option<&str>, new: &str) -> Result<Changed, Refused> {
    let (at, indent) = match after {
        None => (text.trim_end().len(), String::new()),
        Some(symbol) => {
            let module = parse(text).module;
            match find(&module, text, symbol)? {
                Found::Item(item) => (item.end() as usize, String::new()),
                Found::Function(f) => (f.end() as usize, indentation(text, f.span.start)),
            }
        }
    };
    let at = u32::try_from(at).unwrap_or(u32::MAX);
    let separator = if at == 0 { "" } else { "\n\n" };
    let written = format!("{separator}{}", reindent(new, &indent, true, true));
    let mut changed = splice(text, Span { start: at, end: at }, &written)?;
    // The file ends with one line break, wherever the declaration went.
    if !changed.text.ends_with('\n') {
        changed.text.push('\n');
    }
    changed.span.start += u32::try_from(separator.len()).unwrap_or(0);
    Ok(changed)
}

/// Remove the declaration named `symbol`, with the blank line that separated it.
pub fn remove(text: &str, symbol: &str) -> Result<Changed, Refused> {
    let module = parse(text).module;
    let (start, end) = match find(&module, text, symbol)? {
        Found::Item(item) => (item.span().start, item.end()),
        Found::Function(f) => (f.span.start, f.end()),
    };
    // Whole lines: from the line's start to the next declaration, blank lines between them.
    let start = text[..start as usize].rfind('\n').map_or(0, |i| i + 1);
    let mut end = text[end as usize..].find('\n').map_or(text.len(), |i| end as usize + i + 1);
    while text[end..].starts_with('\n') || text[end..].starts_with("\r\n") {
        end += if text[end..].starts_with('\n') { 1 } else { 2 };
    }
    if end == text.len() {
        // The last declaration: the blank lines before it go instead.
        let kept = text[..start].trim_end().len();
        let start = if kept == 0 { 0 } else { kept + 1 };
        return splice(text, Span::new(start, end), "");
    }
    splice(text, Span::new(start, end), "")
}

/// Replace the whole lines of `search` with `replace`. The search must occur once; when it
/// occurs only with every line moved by one indentation offset, the replacement is moved by
/// the same offset.
pub fn search_replace(text: &str, search: &str, replace: &str) -> Result<Changed, Refused> {
    if search.trim().is_empty() {
        return Err(Refused("the search text is empty".into()));
    }
    let exact: Vec<usize> =
        text.match_indices(search).map(|(i, _)| i).filter(|&i| i == 0 || text.as_bytes()[i - 1] == b'\n').collect();
    match exact.as_slice() {
        [at] => return splice(text, Span::new(*at, at + search.len()), replace),
        [_, _, ..] => return Err(ambiguous(text, &exact)),
        [] => {}
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let wanted: Vec<&str> = search.trim_end_matches('\n').split('\n').collect();
    let matches = relative(&lines, &wanted);
    let starts: Vec<usize> = matches.iter().map(|(line, _)| offset_of_line(&lines, *line)).collect();
    match matches.as_slice() {
        [(line, offset)] => {
            let start = starts[0];
            let end = offset_of_line(&lines, line + wanted.len()).min(text.len());
            let end = if line + wanted.len() < lines.len() { end - 1 } else { end };
            let moved: Vec<String> = replace.trim_end_matches('\n').split('\n').map(|l| shifted(l, *offset)).collect();
            let moved = if replace.is_empty() { String::new() } else { moved.join("\n") };
            splice(text, Span::new(start, end), &moved)
        }
        [_, _, ..] => Err(ambiguous(text, &starts)),
        [] => {
            let loose = occurs(
                &lines.iter().map(|l| l.trim()).collect::<Vec<_>>(),
                &wanted.iter().map(|l| l.trim()).collect::<Vec<_>>(),
            );
            Err(Refused(if loose.is_empty() {
                "the search text is not in the file: copy its lines exactly".into()
            } else {
                "the search text is in the file only with different indentation inside it: copy its lines exactly"
                    .into()
            }))
        }
    }
}

fn ambiguous(text: &str, starts: &[usize]) -> Refused {
    let lines: Vec<String> =
        starts.iter().map(|&s| line_column(text, u32::try_from(s).unwrap_or(u32::MAX)).0.to_string()).collect();
    Refused(format!(
        "the search text occurs {} times, at lines {}: add lines to make it unique",
        starts.len(),
        lines.join(", ")
    ))
}

fn offset_of_line(lines: &[&str], line: usize) -> usize {
    lines.iter().take(line).map(|l| l.len() + 1).sum()
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn shifted(line: &str, offset: isize) -> String {
    if line.trim().is_empty() {
        return line.to_string();
    }
    let width = (indent_of(line).cast_signed() + offset).max(0).cast_unsigned();
    format!("{}{}", " ".repeat(width), line.trim_start_matches(' '))
}

fn occurs(haystack: &[&str], needle: &[&str]) -> Vec<usize> {
    if needle.len() > haystack.len() {
        return Vec::new();
    }
    (0..=haystack.len() - needle.len()).filter(|&i| haystack[i..i + needle.len()] == *needle).collect()
}

/// Where `search` matches with every line moved by one common offset: the line, and the offset.
fn relative(lines: &[&str], search: &[&str]) -> Vec<(usize, isize)> {
    let Some(anchor) = search.iter().position(|l| !l.trim().is_empty()) else { return Vec::new() };
    let trimmed: Vec<&str> = lines.iter().map(|l| l.trim()).collect();
    let wanted: Vec<&str> = search.iter().map(|l| l.trim()).collect();
    occurs(&trimmed, &wanted)
        .into_iter()
        .filter_map(|start| {
            let offset = indent_of(lines[start + anchor]).cast_signed() - indent_of(search[anchor]).cast_signed();
            search
                .iter()
                .enumerate()
                .all(|(j, s)| s.trim().is_empty() || shifted(s, offset) == lines[start + j].trim_end_matches('\r'))
                .then_some((start, offset))
        })
        .collect()
}

/// A declaration an address names.
enum Found<'m> {
    Item(&'m Item),
    Function(&'m FnDef),
}

/// The declaration named `name` — a function, type or trait — or `Type.method`, a method of an
/// `impl` of that type or of that trait.
fn find<'m>(module: &'m Module, text: &str, name: &str) -> Result<Found<'m>, Refused> {
    let mut known: Vec<String> = Vec::new();
    let mut found: Vec<Found<'m>> = Vec::new();
    for item in &module.items {
        match item {
            Item::Fn(f) => {
                known.push(f.name.name.clone());
                if f.name.name == name {
                    found.push(Found::Function(f));
                }
            }
            Item::Record(RecordDef { name: n, .. }) | Item::Sum(SumDef { name: n, .. }) => {
                known.push(n.name.clone());
                if n.name == name {
                    found.push(Found::Item(item));
                }
            }
            Item::Trait(t) => {
                known.push(t.name.name.clone());
                if t.name.name == name {
                    found.push(Found::Item(item));
                }
                for m in &t.methods {
                    known.push(format!("{}.{}", t.name.name, m.name.name));
                    if format!("{}.{}", t.name.name, m.name.name) == name {
                        found.push(Found::Function(m));
                    }
                }
            }
            Item::Impl(imp) => {
                let Some(owner) = crate::symbols::named(&imp.target) else { continue };
                for m in &imp.methods {
                    let full = format!("{}.{}", owner.name, m.name.name);
                    if full == name {
                        found.push(Found::Function(m));
                    }
                    known.push(full);
                }
            }
            Item::Import(_) | Item::Test(_) | Item::Error(_) => {}
        }
    }
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => {
            known.sort();
            known.dedup();
            Err(Refused(format!("nothing is called `{name}`; the file declares {}", known.join(", "))))
        }
        _ => {
            let lines: Vec<String> = found
                .iter()
                .map(|f| match f {
                    Found::Item(i) => i.span().start,
                    Found::Function(f) => f.span.start,
                })
                .map(|s| line_column(text, s).0.to_string())
                .collect();
            Err(Refused(format!("`{name}` is declared {} times, at lines {}", found.len(), lines.join(", "))))
        }
    }
}

/// A body's statements, with the comment lines right above the first, which belong to it.
fn body(f: &FnDef, text: &str, symbol: &str) -> Result<Span, Refused> {
    let block = f.body.as_ref().filter(|b| !b.stmts.is_empty());
    let block = block.ok_or_else(|| Refused(format!("`{symbol}` has no body: replace its definition")))?;
    let first = block.stmts[0].span.start as usize;
    let indent = indentation(text, block.stmts[0].span.start);
    let mut line_start = text[..first].rfind('\n').map_or(0, |i| i + 1);
    while line_start > 0 {
        let above = text[..line_start - 1].rfind('\n').map_or(0, |i| i + 1);
        let line = &text[above..line_start - 1];
        if !line.strip_prefix(indent.as_str()).is_some_and(|rest| rest.starts_with('#')) {
            break;
        }
        line_start = above;
    }
    let start = if line_start + indent.len() < first { line_start + indent.len() } else { first };
    Ok(Span { start: u32::try_from(start).unwrap_or(u32::MAX), end: block.end() })
}

/// The arm of a `match` in `f` whose pattern is `wanted`.
fn arm(f: &FnDef, text: &str, symbol: &str, wanted: &str) -> Result<Span, Refused> {
    let mut arms = Vec::new();
    if let Some(body) = &f.body {
        collect_arms(body, &mut arms);
    }
    let pattern_text = |a: &Arm| text[a.pattern.span.range()].to_string();
    let head = |a: &Arm| match &a.pattern.kind {
        PatternKind::Variant { name, .. } | PatternKind::Name(name) => Some(name.name.clone()),
        _ => None,
    };
    let wanted = wanted.trim().trim_start_matches("case ").trim_end_matches(':').trim();
    let exact: Vec<&&Arm> = arms.iter().filter(|a| pattern_text(a) == wanted).collect();
    let chosen: Vec<&&Arm> =
        if exact.is_empty() { arms.iter().filter(|a| head(a).as_deref() == Some(wanted)).collect() } else { exact };
    match chosen.as_slice() {
        [a] => Ok(Span { start: a.span.start, end: a.body.end() }),
        [] => {
            let all: Vec<String> = arms.iter().map(|a| pattern_text(a)).collect();
            Err(Refused(if all.is_empty() {
                format!("`{symbol}` has no `match`")
            } else {
                format!("no arm of `{symbol}` matches `{wanted}`; its arms are {}", all.join(", "))
            }))
        }
        many => {
            let lines: Vec<String> = many.iter().map(|a| line_column(text, a.span.start).0.to_string()).collect();
            Err(Refused(format!(
                "{} arms of `{symbol}` match `{wanted}`, at lines {}: write the pattern in full",
                many.len(),
                lines.join(", ")
            )))
        }
    }
}

fn collect_arms<'b>(block: &'b Block, out: &mut Vec<&'b Arm>) {
    for stmt in &block.stmts {
        match &stmt.kind {
            StmtKind::Match { arms, .. } => {
                for a in arms {
                    out.push(a);
                    collect_arms(&a.body, out);
                }
            }
            StmtKind::If { branches, orelse } => {
                for (_, b) in branches {
                    collect_arms(b, out);
                }
                if let Some(b) = orelse {
                    collect_arms(b, out);
                }
            }
            StmtKind::While { body, .. } | StmtKind::For { body, .. } => collect_arms(body, out),
            _ => {}
        }
    }
}

/// The whitespace before `offset` on its line.
fn indentation(text: &str, offset: u32) -> String {
    let start = text[..offset as usize].rfind('\n').map_or(0, |i| i + 1);
    text[start..offset as usize].chars().take_while(|c| *c == ' ').collect()
}

/// `new` moved to `indent`: its common indentation removed, then every line but the first —
/// which goes where the old text started, already indented — prefixed with `indent`. A
/// declaration copied from deeper in a file, its first line flush and the rest still indented,
/// keeps its body one level in.
fn reindent(new: &str, indent: &str, header: bool, first_too: bool) -> String {
    let lines: Vec<&str> = new.trim_matches('\n').trim_end().split('\n').map(|l| l.trim_end_matches('\r')).collect();
    let widths = |ls: &[&str]| ls.iter().filter(|l| !l.trim().is_empty()).map(|l| indent_of(l)).min();
    let first = lines.first().map_or(0, |l| indent_of(l));
    let rest = widths(&lines[1.min(lines.len())..]);
    let common = rest.map_or(first, |r| r.min(first));
    // A header written flush whose body sits more than one level in.
    let extra = match rest {
        Some(r) if header && first == common && r > common + 4 => r - common - 4,
        _ => 0,
    };
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if line.trim().is_empty() {
            continue;
        }
        let drop = if i == 0 { common } else { common + extra };
        let kept = &line[drop.min(indent_of(line))..];
        if i > 0 || first_too {
            out += indent;
        }
        out += kept;
    }
    out
}

/// `text` with `span` replaced, refused when the result has a syntax error the original did not.
fn splice(text: &str, span: Span, new: &str) -> Result<Changed, Refused> {
    let mut result = String::with_capacity(text.len() + new.len());
    result += &text[..span.start as usize];
    result += new;
    result += &text[span.end as usize..];
    let original = text[span.range()].to_string();
    let written = Span::new(span.start as usize, span.start as usize + new.len());
    let broken = introduced_syntax(text, &result);
    if broken.is_empty() {
        return Ok(Changed { text: result, span: written, original });
    }
    let errors: Vec<String> = broken
        .iter()
        .map(|d| {
            let (line, column) = line_column(&result, d.span.start);
            format!("  {line}:{column}: {} {}", d.code, d.message)
        })
        .collect();
    Err(Refused(format!(
        "the edit was not made: it breaks the syntax\n{}\n\nattempted:\n{new}\n\noriginal:\n{original}",
        errors.join("\n")
    )))
}

/// The syntax errors `after` has that `before` did not.
fn introduced_syntax(before: &str, after: &str) -> Vec<Diagnostic> {
    let errors = |text: &str| parse(text).errors.iter().map(lotml_check::syntax).collect::<Vec<_>>();
    lotml_diag::introduced(errors(after), after, &errors(before), before)
}

/// Whether `name` can name a declaration: an identifier that is not a keyword and not reserved
/// for the compiler.
pub fn valid_name(name: &str) -> Result<(), Refused> {
    let mut chars = name.chars();
    let starts = chars.next().is_some_and(|c| c.is_alphabetic() || c == '_');
    if !starts || !chars.all(|c| c.is_alphanumeric() || c == '_') {
        return Err(Refused(format!("`{name}` is not a name: letters, digits and `_`, not starting with a digit")));
    }
    if TokenKind::keyword(name).is_some() {
        return Err(Refused(format!("`{name}` is a keyword")));
    }
    if name.starts_with("__") {
        return Err(Refused(format!("`{name}` starts with `__`, which is reserved for the compiler")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reindent_moves_a_block_to_the_targets_indentation() {
        assert_eq!(
            reindent("x = 1\nif x > 0:\n    y = 2", "    ", false, false),
            "x = 1\n    if x > 0:\n        y = 2"
        );
        assert_eq!(reindent("        a\n        b", "    ", false, true), "    a\n    b", "a copy from deeper in");
        assert_eq!(reindent("a\n\n    \nb", "  ", false, false), "a\n\n\n  b", "blank lines stay blank");
    }

    #[test]
    fn reindent_keeps_a_flush_headers_body_one_level_in() {
        let copied = "fn get(self) -> int:\n        return self.count";
        assert_eq!(reindent(copied, "    ", true, false), "fn get(self) -> int:\n        return self.count");
        assert_eq!(reindent(copied, "", true, false), "fn get(self) -> int:\n    return self.count");
    }

    #[test]
    fn names_must_be_identifiers_and_not_keywords() {
        assert!(valid_name("total_2").is_ok());
        assert!(valid_name("2nd").is_err());
        assert!(valid_name("a-b").is_err());
        assert!(valid_name("match").is_err());
        assert!(valid_name("__x").is_err());
        assert!(valid_name("").is_err());
    }
}
