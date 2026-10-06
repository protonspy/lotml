//! Where a change to a file falls and which one edit makes it (specs/guide-records/): the
//! declarations of the first version a change touches, innermost first, and the smallest
//! symbol-addressed edit that turns the first version into the second, found by applying it
//! through the same edit functions the MCP tools apply, so every edit reported is one the tools
//! reproduce byte for byte.

use lotml_syntax::span::{Span, line_column};

use crate::edit::{self, Part};
use crate::{Kind, Outline, Workspace};

/// A declaration a change touches, in the first version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    /// As `show` and `replace` take it; None for the file's start, before its first declaration.
    pub symbol: Option<String>,
    /// `function`, `method`, `record`, `sum`, `trait`, `test`, or `file` for the file's start.
    pub kind: &'static str,
    /// The first and last line of its text, from 1.
    pub lines: [u32; 2],
}

/// The one edit that reproduces a change, as the arguments of the MCP tool that applies it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    /// `replace`, `add`, `remove` or `edit`.
    pub tool: &'static str,
    /// The tool's arguments, the file's path among them.
    pub arguments: Vec<(&'static str, String)>,
}

impl Edit {
    /// The kind of change the edit is: `arm`, `body`, `definition`, `add`, `remove` or `lines`.
    pub fn kind(&self) -> &'static str {
        match self.tool {
            "replace" => match self.arguments.iter().find(|(k, _)| *k == "part").map(|(_, v)| v.as_str()) {
                Some("arm") => "arm",
                Some("body") => "body",
                _ => "definition",
            },
            "add" => "add",
            "remove" => "remove",
            _ => "lines",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diff {
    pub declarations: Vec<Declaration>,
    /// None when no single edit reproduces the change.
    pub edit: Option<Edit>,
}

/// Where the change from `before` to `after` falls, and the edit that makes it, its arguments
/// naming the file `path`.
pub fn diff(before: &str, after: &str, path: &str) -> Diff {
    let changed = changed_lines(before, after);
    let declarations = touched(before, &changed);
    let edit = if before == after { None } else { reproduce(before, after, path, &declarations, &changed) };
    Diff { declarations, edit }
}

/// The lines of `before`, from 1, a change removed or replaced; for a pure insertion, the line
/// it follows, 0 at the file's start. A longest common subsequence of lines, after the common
/// prefix and suffix are set aside.
pub fn changed_lines(before: &str, after: &str) -> Vec<u32> {
    let old: Vec<&str> = before.split_inclusive('\n').collect();
    let new: Vec<&str> = after.split_inclusive('\n').collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..].iter().rev().zip(new[prefix..].iter().rev()).take_while(|(a, b)| a == b).count();
    let (a, b) = (&old[prefix..old.len() - suffix], &new[prefix..new.len() - suffix]);
    let mut found = Vec::new();
    if a.len() * b.len() > 4_000_000 {
        found.extend((0..a.len()).map(|i| line(prefix + i + 1)));
        if a.is_empty() {
            found.push(line(prefix));
        }
        return found;
    }
    // lengths[i][j]: the longest common subsequence of a[i..] and b[j..].
    let mut lengths = vec![vec![0u32; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lengths[i][j] =
                if a[i] == b[j] { lengths[i + 1][j + 1] + 1 } else { lengths[i + 1][j].max(lengths[i][j + 1]) };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut inserted = Vec::new();
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            i += 1;
            j += 1;
        } else if j < b.len() && (i == a.len() || lengths[i][j + 1] >= lengths[i + 1][j]) {
            inserted.push(line(prefix + i));
            j += 1;
        } else {
            found.push(line(prefix + i + 1));
            i += 1;
        }
    }
    // A line written where another was removed replaces it: only a pure insertion names the line
    // it follows.
    let removed = found.clone();
    found.extend(inserted.into_iter().filter(|&at| !removed.contains(&at) && !removed.contains(&(at + 1))));
    found.sort_unstable();
    found.dedup();
    found
}

fn line(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// A file's symbol-addressed declarations: their symbol, kind and lines.
fn declared(text: &str) -> Vec<Declaration> {
    let mut workspace = Workspace::new();
    let path = std::path::Path::new("diff.lotml");
    workspace.set(path, text.to_string());
    let lines = |span: Span| {
        let start = line(line_column(text, span.start).0);
        let end = line(line_column(text, span.end.max(span.start + 1) - 1).0);
        [start, end.max(start)]
    };
    let mut found = Vec::new();
    for entry in workspace.outline(path) {
        let mut add = |symbol: String, kind: &'static str, span: Span| {
            found.push(Declaration { symbol: Some(symbol), kind, lines: lines(span) });
        };
        match entry.kind {
            Kind::Function => add(entry.name.clone(), "function", entry.span),
            Kind::Record => add(entry.name.clone(), "record", entry.span),
            Kind::Sum => add(entry.name.clone(), "sum", entry.span),
            Kind::Test => add(entry.name.clone(), "test", entry.span),
            Kind::Trait => {
                add(entry.name.clone(), "trait", entry.span);
                for method in &entry.children {
                    add(format!("{}.{}", entry.name, method.name), "method", method.span);
                }
            }
            Kind::Impl => {
                for method in &entry.children {
                    add(format!("{}.{}", owner(&entry), method.name), "method", method.span);
                }
            }
            _ => {}
        }
    }
    found
}

/// The type an `impl` block's methods are named under: `Counter` for `impl Counter`, `impl
/// Stack[T]` and `impl Show for Counter`.
fn owner(entry: &Outline) -> String {
    let target = entry.name.rsplit(" for ").next().unwrap_or("").trim_start_matches("impl ");
    target.split('[').next().unwrap_or(target).trim().to_string()
}

/// The declarations holding a changed line, innermost first; a line no declaration holds is the
/// one before it — the declaration it follows — or the file's start.
fn touched(before: &str, changed: &[u32]) -> Vec<Declaration> {
    let declared = declared(before);
    let mut found: Vec<Declaration> = Vec::new();
    let start = Declaration { symbol: None, kind: "file", lines: [1, 1] };
    for &changed_line in changed {
        let mut holding: Vec<&Declaration> =
            declared.iter().filter(|d| d.lines[0] <= changed_line && changed_line <= d.lines[1]).collect();
        if holding.is_empty() {
            let preceding = declared.iter().filter(|d| d.lines[1] < changed_line).max_by_key(|d| d.lines[1]);
            holding.extend(preceding);
        }
        holding.sort_by_key(|d| d.lines[1] - d.lines[0]);
        if holding.is_empty() && !found.contains(&start) {
            found.push(start.clone());
        }
        for declaration in holding {
            if !found.contains(declaration) {
                found.push(declaration.clone());
            }
        }
    }
    found.sort_by_key(|d| d.lines[1] - d.lines[0]);
    found
}

/// The smallest edit that turns `before` into `after`, tried in order — an arm, the body, the
/// definition of the innermost changed declaration; a declaration added or removed; whole lines —
/// each reported only when applying it gives `after` exactly. None when none does.
fn reproduce(before: &str, after: &str, path: &str, touched: &[Declaration], changed: &[u32]) -> Option<Edit> {
    let made = |result: Result<edit::Changed, edit::Refused>| result.is_ok_and(|c| c.text == after);
    let edit = |tool: &'static str, mut arguments: Vec<(&'static str, String)>| {
        arguments.push(("path", path.to_string()));
        Edit { tool, arguments }
    };
    if let Some(inner) = touched.first()
        && let Some(symbol) = &inner.symbol
        && inner.kind != "test"
        && changed.iter().all(|&l| inner.lines[0] <= l && l <= inner.lines[1])
    {
        let mut parts = Vec::new();
        if matches!(inner.kind, "function" | "method") {
            parts.extend(edit::arms(before, symbol).into_iter().map(Part::Arm));
            parts.push(Part::Body);
        }
        parts.push(Part::Definition);
        for part in parts {
            let Ok(span) = edit::region(after, symbol, &part) else { continue };
            let text = &after[line_start(after, span.start)..span.end as usize];
            if made(edit::replace(before, symbol, &part, text)) {
                let mut arguments = vec![("symbol", symbol.clone())];
                match &part {
                    Part::Arm(pattern) => arguments.extend([("part", "arm".to_string()), ("arm", pattern.clone())]),
                    Part::Body => arguments.push(("part", "body".to_string())),
                    Part::Definition => arguments.push(("part", "definition".to_string())),
                }
                arguments.push(("text", text.to_string()));
                return Some(edit("replace", arguments));
            }
        }
    }
    let (old, new) = (declared(before), declared(after));
    let named = |found: &[Declaration], symbol: &Option<String>| found.iter().any(|d| d.symbol == *symbol);
    let added: Vec<&Declaration> = new.iter().filter(|d| !named(&old, &d.symbol)).collect();
    let removed: Vec<&Declaration> = old.iter().filter(|d| !named(&new, &d.symbol)).collect();
    if let ([one], []) = (added.as_slice(), removed.as_slice())
        && let Some(symbol) = &one.symbol
        && let Ok(span) = edit::region(after, symbol, &Part::Definition)
    {
        let text = &after[span.range()];
        let after_symbol = preceding(&new, one).filter(|p| named(&old, &Some(p.clone())));
        if made(edit::add(before, after_symbol.as_deref(), text)) {
            let mut arguments = vec![("text", text.to_string())];
            arguments.extend(after_symbol.map(|s| ("after", s)));
            return Some(edit("add", arguments));
        }
    }
    if let ([], [one]) = (added.as_slice(), removed.as_slice())
        && let Some(symbol) = &one.symbol
        && made(edit::remove(before, symbol))
    {
        return Some(edit("remove", vec![("symbol", symbol.clone())]));
    }
    lines(before, after).map(|(search, replace)| edit("edit", vec![("search", search), ("replace", replace)]))
}

/// Where the line holding `offset` starts, when only indentation precedes it there: a part taken
/// with its indentation is re-indented as a whole.
fn line_start(text: &str, offset: u32) -> usize {
    let offset = offset as usize;
    let start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    if text[start..offset].trim().is_empty() { start } else { offset }
}

/// The declaration `added` follows in its file: for a method, the method before it in the same
/// type; otherwise the top-level declaration before it.
fn preceding(declared: &[Declaration], added: &Declaration) -> Option<String> {
    let symbol = added.symbol.as_deref()?;
    let owner = (added.kind == "method").then(|| symbol.split('.').next().unwrap_or(""));
    declared
        .iter()
        .filter(|d| d.lines[1] < added.lines[0])
        .filter(|d| match owner {
            Some(owner) => {
                d.kind == "method" && d.symbol.as_deref().is_some_and(|s| s.starts_with(&format!("{owner}.")))
            }
            None => d.kind != "method",
        })
        .max_by_key(|d| d.lines[1])
        .and_then(|d| d.symbol.clone())
}

/// A whole-lines edit making the change: the changed lines of `before`, widened a line at a time
/// until they occur once, and the lines of `after` that take their place.
fn lines(before: &str, after: &str) -> Option<(String, String)> {
    let old: Vec<&str> = before.split_inclusive('\n').collect();
    let new: Vec<&str> = after.split_inclusive('\n').collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..].iter().rev().zip(new[prefix..].iter().rev()).take_while(|(a, b)| a == b).count();
    let (mut start, mut end) = (prefix, old.len() - suffix);
    let mut above = true;
    loop {
        if start < end {
            let search = old[start..end].concat();
            let replace = new[start..new.len() - (old.len() - end)].concat();
            let (search, replace) = (search.trim_end_matches('\n'), replace.trim_end_matches('\n'));
            if edit::search_replace(before, search, replace).is_ok_and(|c| c.text == after) {
                return Some((search.to_string(), replace.to_string()));
            }
        }
        match (start > 0, end < old.len()) {
            (false, false) => return None,
            (true, false) => start -= 1,
            (false, true) => end += 1,
            (true, true) if above => start -= 1,
            (true, true) => end += 1,
        }
        above = !above;
    }
}
