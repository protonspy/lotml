//! `lotml init`: set a project up for coding agents. A lotml block in `AGENTS.md` teaching the
//! compiler-driven edit loop, `lotml.guide.lotml` teaching the language as code that checks, and
//! the compiler's MCP server registered in each assistant harness the project uses.
//!
//! Both texts are compiled in, so they always describe the compiler that wrote them; running
//! `init` again after an upgrade refreshes them and leaves everything else as it was.

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::Path;

use clap::ValueEnum;
use serde_json::Value;

use crate::Failure;

pub const GUIDE: &str = include_str!("init/lotml.guide.lotml");
pub const AGENTS: &str = include_str!("init/AGENTS.md");
const GUIDE_FILE: &str = "lotml.guide.lotml";
const BEGIN: &str = "<!-- lotml:begin -->";
const END: &str = "<!-- lotml:end -->";

/// A harness `--harness` can name; `none` names no harness.
#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
pub enum Named {
    Claude,
    Codex,
    Cursor,
    None,
}

/// An assistant harness: how it is recognised in a project and where it reads MCP servers from.
struct Harness {
    name: &'static str,
    named: Named,
    marker: &'static str,
    config: &'static str,
}

const HARNESSES: [Harness; 3] = [
    Harness { name: "Claude Code", named: Named::Claude, marker: ".claude", config: ".mcp.json" },
    Harness { name: "Codex", named: Named::Codex, marker: ".codex", config: ".codex/config.toml" },
    Harness { name: "Cursor", named: Named::Cursor, marker: ".cursor", config: ".cursor/mcp.json" },
];

/// What happened to a file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Created,
    Updated,
    Unchanged,
}

impl Outcome {
    fn word(self) -> &'static str {
        match self {
            Outcome::Created => "created",
            Outcome::Updated => "updated",
            Outcome::Unchanged => "unchanged",
        }
    }
}

/// Set `dir` up: the guide, the `AGENTS.md` block, and the harnesses chosen — by `--harness`, or
/// on a terminal by the checklist, or else the ones detected. Status 2 when a configuration was
/// left unread.
pub fn run(dir: &Path, named: Option<&[Named]>, yes: bool) -> Result<u8, Failure> {
    if !dir.is_dir() {
        return Err(Failure(format!("{} is not a directory", dir.display())));
    }
    let detected = HARNESSES.map(|h| dir.join(h.marker).is_dir());
    let chosen = match named {
        Some(named) => HARNESSES.map(|h| named.contains(&h.named)),
        None if yes || !io::stdin().is_terminal() => detected,
        None => checklist(detected, &mut io::stdin().lock(), &mut io::stdout().lock())
            .map_err(|e| Failure(format!("cannot read the checklist's answer: {e}")))?,
    };
    let mut status = 0;
    report(GUIDE_FILE, put(&dir.join(GUIDE_FILE), GUIDE)?);
    report("AGENTS.md", put_block(&dir.join("AGENTS.md"), AGENTS)?);
    for (harness, _) in HARNESSES.iter().zip(chosen).filter(|(_, on)| *on) {
        let path = dir.join(harness.config);
        let existing = read(&path)?;
        let registered = match (harness.named, existing.as_deref()) {
            (Named::Codex, text) => Ok(register_toml(text.unwrap_or(""))),
            (_, None) => Ok(Some(NEW_JSON.to_string())),
            (_, Some(text)) => register_json(text),
        };
        match registered {
            Ok(Some(text)) => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| Failure(format!("cannot create {}: {e}", parent.display())))?;
                }
                report(harness.config, put(&path, &text)?);
            }
            Ok(None) => report(harness.config, Outcome::Unchanged),
            Err(why) => {
                println!("{}: {why}; left as it was, add the lotml server by hand", harness.config);
                status = 2;
            }
        }
        if harness.named == Named::Claude {
            report("CLAUDE.md", put_block(&dir.join("CLAUDE.md"), "@AGENTS.md\n")?);
        }
    }
    let set_up: Vec<&str> = HARNESSES.iter().zip(chosen).filter(|(_, on)| *on).map(|(h, _)| h.name).collect();
    println!("harnesses: {}", if set_up.is_empty() { "none".to_string() } else { set_up.join(", ") });
    Ok(status)
}

fn report(name: &str, outcome: Outcome) {
    println!("{name}: {}", outcome.word());
}

/// The checklist: the harnesses with the detected ones checked, toggled by number until an empty
/// line accepts. The end of the input accepts too.
pub fn checklist(detected: [bool; 3], input: &mut impl BufRead, output: &mut impl Write) -> io::Result<[bool; 3]> {
    let mut chosen = detected;
    loop {
        writeln!(output, "Harnesses to set up for lotml (found in this project: checked):")?;
        for (i, (harness, (on, found))) in HARNESSES.iter().zip(chosen.into_iter().zip(detected)).enumerate() {
            let mark = if on { "x" } else { " " };
            let found = if found { format!("  ({}/ found)", harness.marker) } else { String::new() };
            writeln!(output, "  [{mark}] {} {}{found}", i + 1, harness.name)?;
        }
        write!(output, "Toggle by number (e.g. `2 3`), Enter to accept: ")?;
        output.flush()?;
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 || line.trim().is_empty() {
            return Ok(chosen);
        }
        for word in line.split(|c: char| c.is_whitespace() || c == ',').filter(|w| !w.is_empty()) {
            match word.parse::<usize>() {
                Ok(n) if (1..=HARNESSES.len()).contains(&n) => chosen[n - 1] = !chosen[n - 1],
                _ => writeln!(output, "`{word}` is not 1, 2 or 3")?,
            }
        }
    }
}

fn read(path: &Path) -> Result<Option<String>, Failure> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Failure(format!("cannot read {}: {e}", path.display()))),
    }
}

/// Write `text` to `path` unless it already holds exactly that.
fn put(path: &Path, text: &str) -> Result<Outcome, Failure> {
    let existing = read(path)?;
    if existing.as_deref() == Some(text) {
        return Ok(Outcome::Unchanged);
    }
    std::fs::write(path, text).map_err(|e| Failure(format!("cannot write {}: {e}", path.display())))?;
    Ok(if existing.is_some() { Outcome::Updated } else { Outcome::Created })
}

fn put_block(path: &Path, body: &str) -> Result<Outcome, Failure> {
    let existing = read(path)?.unwrap_or_default();
    put(path, &with_block(&existing, body))
}

/// `text` with the lotml block holding `body`: the block replaced where there is one, appended
/// after a blank line where there is not. Written in the file's own line endings.
pub fn with_block(text: &str, body: &str) -> String {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let block = format!("{BEGIN}\n{}\n{END}", body.trim_end()).replace('\n', newline);
    if let Some(start) = text.find(BEGIN)
        && let Some(end) = text[start..].find(END).map(|e| start + e + END.len())
    {
        return format!("{}{block}{}", &text[..start], &text[end..]);
    }
    if text.trim().is_empty() {
        return format!("{block}{newline}");
    }
    let separator = if text.ends_with('\n') { newline } else { &format!("{newline}{newline}") };
    format!("{text}{separator}{block}{newline}")
}

const NEW_JSON: &str = "{\n  \"mcpServers\": {\n    \"lotml\": {\n      \"command\": \"lotml\",\n      \"args\": [\"mcp\", \"--root\", \".\"]\n    }\n  }\n}\n";

/// `text`, a JSON MCP configuration, with the lotml server added to `mcpServers`: `None` when it
/// is already there, an error when the file is not such a configuration. The server is inserted
/// as text, so the file keeps its other entries, their order and its layout.
pub fn register_json(text: &str) -> Result<Option<String>, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| format!("not valid JSON ({e})"))?;
    let Some(top) = value.as_object() else { return Err("not a JSON object".to_string()) };
    let servers = match top.get("mcpServers") {
        Some(Value::Object(servers)) if servers.contains_key("lotml") => return Ok(None),
        Some(Value::Object(servers)) => Some(servers),
        None => None,
        Some(_) => return Err("`mcpServers` is not an object".to_string()),
    };
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let unit = indent_unit(text);
    let indent = |depth: usize| format!("{newline}{}", unit.repeat(depth));
    let server = |depth: usize| {
        let inner = indent(depth + 1);
        format!(
            "\"lotml\": {{{inner}\"command\": \"lotml\",{inner}\"args\": [\"mcp\", \"--root\", \".\"]{}}}",
            indent(depth)
        )
    };
    let (brace, member, empty, depth) = match servers {
        Some(servers) => {
            let brace = servers_brace(text).ok_or("`mcpServers` could not be found in the text")?;
            (brace, server(2), servers.is_empty(), 2)
        }
        None => {
            let brace = text.find('{').ok_or("not a JSON object")?;
            (brace, format!("\"mcpServers\": {{{}{}{}}}", indent(2), server(2), indent(1)), top.is_empty(), 1)
        }
    };
    let after = brace + 1;
    let (rest, close) = if empty {
        (after + text[after..].len() - text[after..].trim_start().len(), indent(depth - 1))
    } else {
        (after, ",".to_string())
    };
    let out = format!("{}{}{member}{close}{}", &text[..after], indent(depth), &text[rest..]);
    // The insertion must have added the server and nothing else: anything the scan misread, such
    // as an escaped key, shows up here rather than in the user's file.
    let mut check: Value = serde_json::from_str(&out).map_err(|_| "the server could not be inserted".to_string())?;
    let added = check.get_mut("mcpServers").and_then(Value::as_object_mut).and_then(|s| s.remove("lotml"));
    if added.is_none() {
        return Err("the server could not be inserted".to_string());
    }
    if servers.is_none() {
        check.as_object_mut().map(|o| o.remove("mcpServers"));
    }
    if check != value {
        return Err("the server could not be inserted without changing other entries".to_string());
    }
    Ok(Some(out))
}

/// The indentation of the first indented line, two spaces when no line is.
fn indent_unit(text: &str) -> &str {
    text.lines()
        .skip(1)
        .map(|line| &line[..line.len() - line.trim_start().len()])
        .find(|indent| !indent.is_empty())
        .unwrap_or("  ")
}

/// Where the top-level object's `"mcpServers"` value opens: the first key of that name at depth
/// one, strings skipped whole so a key quoted inside a value is never taken for one.
fn servers_brace(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0;
    let mut key: Option<&str> = None;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let start = i + 1;
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                key = (depth == 1).then(|| &text[start..i.min(bytes.len())]);
            }
            b':' if depth == 1 && key.take() == Some("mcpServers") => {
                let value = &text[i + 1..];
                return Some(i + 1 + value.len() - value.trim_start().len());
            }
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    None
}

/// `text`, Codex's `config.toml`, with a `[mcp_servers.lotml]` table appended; `None` when the
/// table is already there.
pub fn register_toml(text: &str) -> Option<String> {
    if text.lines().any(|line| line.trim() == "[mcp_servers.lotml]") {
        return None;
    }
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let table =
        "[mcp_servers.lotml]\ncommand = \"lotml\"\nargs = [\"mcp\", \"--root\", \".\"]\n".replace('\n', newline);
    Some(match text {
        "" => table,
        _ if text.ends_with('\n') => format!("{text}{newline}{table}"),
        _ => format!("{text}{newline}{newline}{table}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lotml_server(text: &str) -> Value {
        serde_json::from_str::<Value>(text).expect("valid JSON")["mcpServers"]["lotml"].clone()
    }

    #[test]
    fn a_new_configuration_declares_the_server() {
        assert_eq!(lotml_server(NEW_JSON)["command"], "lotml");
    }

    #[test]
    fn the_server_joins_the_existing_ones_in_their_order() {
        let text = "{\n    \"mcpServers\": {\n        \"zeta\": {\"command\": \"z\"},\n        \"alpha\": {\"command\": \"a\"}\n    },\n    \"other\": 1\n}\n";
        let out = register_json(text).unwrap().expect("a change");
        assert_eq!(lotml_server(&out)["args"], serde_json::json!(["mcp", "--root", "."]));
        assert!(out.find("\"zeta\"").unwrap() < out.find("\"alpha\"").unwrap(), "{out}");
        assert!(out.contains("\n        \"lotml\": {\n"), "indented as the file is: {out}");
        let mut value: Value = serde_json::from_str(&out).unwrap();
        value["mcpServers"].as_object_mut().unwrap().remove("lotml");
        assert_eq!(value, serde_json::from_str::<Value>(text).unwrap());
    }

    #[test]
    fn a_file_without_servers_gets_the_member() {
        let text = "{\n  \"theme\": \"dark\"\n}";
        let out = register_json(text).unwrap().expect("a change");
        assert_eq!(lotml_server(&out)["command"], "lotml");
        assert!(out.find("\"mcpServers\"").unwrap() < out.find("\"theme\"").unwrap(), "{out}");
        assert_eq!(serde_json::from_str::<Value>(&out).unwrap()["theme"], "dark");
    }

    #[test]
    fn empty_objects_take_the_server() {
        for text in ["{}", "{ }", "{\"mcpServers\": {}}", "{\n  \"mcpServers\": { }\n}\n"] {
            let out = register_json(text).unwrap().expect("a change");
            assert_eq!(lotml_server(&out)["command"], "lotml", "{text} gave {out}");
        }
    }

    #[test]
    fn a_key_inside_a_string_is_not_the_member() {
        let text = "{\"note\": \"\\\"mcpServers\\\": {\", \"mcpServers\": {\"a\": {}}}";
        let out = register_json(text).unwrap().expect("a change");
        assert_eq!(lotml_server(&out)["command"], "lotml");
        let value: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(value["note"], "\"mcpServers\": {");
        assert!(value["mcpServers"]["a"].is_object());
    }

    #[test]
    fn a_nested_mcp_servers_key_is_not_the_member() {
        let text = "{\"profile\": {\"mcpServers\": {}}}";
        let out = register_json(text).unwrap().expect("a change");
        assert_eq!(lotml_server(&out)["command"], "lotml");
        assert_eq!(serde_json::from_str::<Value>(&out).unwrap()["profile"]["mcpServers"], serde_json::json!({}));
    }

    #[test]
    fn a_registered_server_is_left_alone() {
        assert_eq!(register_json("{\"mcpServers\": {\"lotml\": {\"command\": \"/opt/lotml\"}}}"), Ok(None));
    }

    #[test]
    fn what_is_not_a_configuration_is_refused() {
        for text in ["{", "[]", "{\"mcpServers\": []}", "// comment\n{}"] {
            assert!(register_json(text).is_err(), "{text}");
        }
    }

    #[test]
    fn windows_line_endings_are_kept() {
        let out = register_json("{\r\n  \"mcpServers\": {\r\n    \"a\": {}\r\n  }\r\n}\r\n").unwrap().unwrap();
        assert!(!out.replace("\r\n", "").contains('\n'), "{out:?}");
    }

    #[test]
    fn the_block_is_appended_once_then_replaced() {
        let first = with_block("# Project\n\nRules.\n", "one\n");
        assert_eq!(first, "# Project\n\nRules.\n\n<!-- lotml:begin -->\none\n<!-- lotml:end -->\n");
        let second = with_block(&first, "two\n");
        assert_eq!(second, first.replace("one", "two"));
        assert_eq!(with_block(&second, "two\n"), second);
        assert_eq!(with_block("", "x"), "<!-- lotml:begin -->\nx\n<!-- lotml:end -->\n");
    }

    #[test]
    fn the_block_keeps_what_follows_it() {
        let text = "a\n<!-- lotml:begin -->\nold\n<!-- lotml:end -->\nb\n";
        assert_eq!(with_block(text, "new"), "a\n<!-- lotml:begin -->\nnew\n<!-- lotml:end -->\nb\n");
    }

    #[test]
    fn the_codex_table_is_appended_once() {
        let out = register_toml("model = \"o4\"\n").unwrap();
        assert_eq!(
            out,
            "model = \"o4\"\n\n[mcp_servers.lotml]\ncommand = \"lotml\"\nargs = [\"mcp\", \"--root\", \".\"]\n"
        );
        assert_eq!(register_toml(&out), None);
    }

    #[test]
    fn the_checklist_toggles_until_accepted() {
        let mut out = Vec::new();
        let chosen = checklist([true, false, false], &mut "2 3\n1\n\n".as_bytes(), &mut out).unwrap();
        assert_eq!(chosen, [false, true, true]);
        let shown = String::from_utf8(out).unwrap();
        assert!(shown.contains("[x] 1 Claude Code  (.claude/ found)"), "{shown}");
        assert!(shown.contains("[ ] 2 Codex"), "{shown}");
        assert!(shown.contains("[ ] 1 Claude Code  (.claude/ found)"), "found stays said once unchecked: {shown}");
    }

    #[test]
    fn the_checklist_ignores_what_is_not_a_number_and_accepts_at_the_end() {
        let mut out = Vec::new();
        let chosen = checklist([false, false, true], &mut "x 9\n".as_bytes(), &mut out).unwrap();
        assert_eq!(chosen, [false, false, true]);
        assert!(String::from_utf8(out).unwrap().contains("`x` is not 1, 2 or 3"));
    }

    #[test]
    fn the_agents_block_stays_short() {
        assert!(AGENTS.lines().count() <= 78, "{} lines", AGENTS.lines().count());
    }
}
