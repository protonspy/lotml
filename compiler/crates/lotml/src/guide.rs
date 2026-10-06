//! The harness guide: a small model that tells an agent where to change its lotml and what kind
//! of change it needs, offered as the MCP server's `guide` tool when this machine has one
//! configured (specs/guide-tool/).

mod answer;
mod client;
mod gate;

pub use answer::{Edit, ProjectFile, judge};
pub use client::ask;
pub use gate::withheld;

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};

/// The version of the messages [`render`] writes. Any change to their text bumps it: a guide
/// trained on records rendered one way and shown a prompt rendered another fails silently.
pub const RENDERER: u32 = 1;
/// The variable naming the configuration file, which must be an absolute path.
pub const VARIABLE: &str = "LOTML_HARNESS_GUIDE";
const FILE: &str = "harness-guide.toml";
/// Bytes of configuration read; the file is a few lines.
const CONFIG_LIMIT: u64 = 64 * 1024;

/// Where the guide server listens: plain HTTP to a loopback address on this machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    /// The host as written: `127.0.0.1`, `[::1]` or `localhost`.
    pub host: String,
    pub port: u16,
    /// The loopback address connected to, `localhost` resolved once.
    pub address: SocketAddr,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub url: Endpoint,
    pub model: String,
    /// The probability of the first location below which the guide says nothing.
    pub threshold: f64,
    /// The answer's `max_tokens`.
    pub answer: u32,
    /// Whether an edit for a failing test may be run to check it.
    pub run_candidates: bool,
    pub renderer: u32,
    /// The records the guide was trained from.
    pub records: PathBuf,
}

/// The configuration this machine has, if any: `Ok(None)` when there is none, `Err` with the
/// reason when there is one the tool cannot use.
pub fn configured(env: &dyn Fn(&str) -> Option<String>) -> Result<Option<Config>, String> {
    let Some(path) = locate(env)? else { return Ok(None) };
    let text = read(&path)?;
    let dir = path.parent().unwrap_or(Path::new("/"));
    parse(&text, dir).map(Some).map_err(|why| format!("{}: {why}", path.display()))
}

fn read(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| format!("{} cannot be read: {e}", path.display()))?;
    let mut text = String::new();
    file.take(CONFIG_LIMIT + 1)
        .read_to_string(&mut text)
        .map_err(|e| format!("{} cannot be read: {e}", path.display()))?;
    if text.len() as u64 > CONFIG_LIMIT {
        return Err(format!("{} is larger than {CONFIG_LIMIT} bytes", path.display()));
    }
    Ok(text)
}

/// The configuration file: where the variable points, or else `harness-guide.toml` in the user's
/// lotml directory. A relative location is refused: a harness starts the server inside the
/// project, so a relative one would let the project supply its own guide.
pub fn locate(env: &dyn Fn(&str) -> Option<String>) -> Result<Option<PathBuf>, String> {
    if let Some(named) = env(VARIABLE).filter(|v| !v.is_empty()) {
        let path = PathBuf::from(named);
        if !path.is_absolute() {
            return Err(format!("{VARIABLE} must be an absolute path"));
        }
        return Ok(Some(path));
    }
    let mut directories = Vec::new();
    if let Some(xdg) = env("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        let base = PathBuf::from(xdg);
        if !base.is_absolute() {
            return Err("XDG_CONFIG_HOME must be an absolute path".to_string());
        }
        directories.push(base.join("lotml"));
    }
    if let Some(home) = env("HOME").map(PathBuf::from).filter(|p| p.is_absolute()) {
        directories.push(home.join(".config").join("lotml"));
    }
    if let Some(appdata) = env("APPDATA").map(PathBuf::from).filter(|p| p.is_absolute()) {
        directories.push(appdata.join("lotml"));
    }
    Ok(directories.into_iter().map(|d| d.join(FILE)).find(|p| p.is_file()))
}

#[derive(Debug, PartialEq)]
enum Value {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

/// The flat `key = value` lines the file holds: strings, integers, floats and booleans, `#`
/// comments. Nested tables and arrays have no place in it and are refused.
fn values(text: &str) -> Result<BTreeMap<String, Value>, String> {
    let mut found = BTreeMap::new();
    for (number, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = || format!("line {}", number + 1);
        let (key, rest) = line.split_once('=').ok_or_else(|| format!("{}: not `key = value`", at()))?;
        let key = key.trim();
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!("{}: `{key}` is not a key", at()));
        }
        let value = value(rest.trim()).map_err(|why| format!("{}: {why}", at()))?;
        if found.insert(key.to_string(), value).is_some() {
            return Err(format!("`{key}` is set twice"));
        }
    }
    Ok(found)
}

fn value(text: &str) -> Result<Value, String> {
    if let Some(body) = text.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = body.chars();
        loop {
            match chars.next() {
                None => return Err("a string is not closed".to_string()),
                Some('"') => break,
                Some('\\') => match chars.next() {
                    Some('\\') => out.push('\\'),
                    Some('"') => out.push('"'),
                    _ => return Err("only `\\\\` and `\\\"` may be escaped".to_string()),
                },
                Some(c) if c.is_control() => return Err("a string holds a control character".to_string()),
                Some(c) => out.push(c),
            }
        }
        let after = chars.as_str().trim();
        if !after.is_empty() && !after.starts_with('#') {
            return Err(format!("`{after}` follows the string"));
        }
        return Ok(Value::Str(out));
    }
    let token = text.split('#').next().unwrap_or("").trim();
    match token {
        "true" => Ok(Value::Bool(true)),
        "false" => Ok(Value::Bool(false)),
        _ if !token.is_empty() && token.trim_start_matches('-').chars().all(|c| c.is_ascii_digit()) => {
            token.parse().map(Value::Int).map_err(|_| format!("`{token}` is out of range"))
        }
        _ if token.contains('.') && token.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-') => {
            token.parse().map(Value::Float).map_err(|_| format!("`{token}` is not a number"))
        }
        _ => Err(format!("`{token}` is not a value")),
    }
}

const KEYS: [&str; 7] = ["url", "model", "threshold", "answer", "run_candidates", "renderer", "records"];

/// The configuration in `text`, the file found in `dir`: every key of [`KEYS`] once, typed, and
/// no other.
pub fn parse(text: &str, dir: &Path) -> Result<Config, String> {
    let mut found = values(text)?;
    if let Some(unknown) = found.keys().find(|k| !KEYS.contains(&k.as_str())) {
        return Err(format!("unknown key `{unknown}`"));
    }
    let mut take = |key: &str| found.remove(key).ok_or_else(|| format!("`{key}` is missing"));
    let string = |key: &str, v: Value| match v {
        Value::Str(s) => Ok(s),
        _ => Err(format!("`{key}` must be a string")),
    };
    let url = endpoint(&string("url", take("url")?)?)?;
    let model = string("model", take("model")?)?;
    let threshold = match take("threshold")? {
        Value::Float(f) => f,
        Value::Int(i) => i as f64,
        _ => return Err("`threshold` must be a number".to_string()),
    };
    if !(0.0..=1.0).contains(&threshold) {
        return Err("`threshold` must lie between 0 and 1".to_string());
    }
    let answer = match take("answer")? {
        Value::Int(i) if i > 0 => u32::try_from(i).map_err(|_| "`answer` is too large".to_string())?,
        _ => return Err("`answer` must be a positive integer".to_string()),
    };
    let Value::Bool(run_candidates) = take("run_candidates")? else {
        return Err("`run_candidates` must be true or false".to_string());
    };
    let renderer = match take("renderer")? {
        Value::Int(i) => u32::try_from(i).map_err(|_| "`renderer` is out of range".to_string())?,
        _ => return Err("`renderer` must be an integer".to_string()),
    };
    if renderer != RENDERER {
        return Err(format!(
            "the guide was trained on renderer {renderer}, and this compiler renders version {RENDERER}"
        ));
    }
    let records = PathBuf::from(string("records", take("records")?)?);
    let records = if records.is_absolute() { records } else { dir.join(records) };
    Ok(Config { url, model, threshold, answer, run_candidates, renderer, records })
}

/// The guide server's address: exactly `http://`, then `127.0.0.1`, `[::1]` or a `localhost`
/// that resolves to loopback alone, then `:` and a decimal port, and at most a trailing `/`.
pub fn endpoint(url: &str) -> Result<Endpoint, String> {
    let refused = || format!("`{url}` is not http:// to 127.0.0.1, [::1] or localhost with a port");
    let rest = url.strip_prefix("http://").ok_or_else(refused)?;
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    for host in ["127.0.0.1", "[::1]", "localhost"] {
        let Some(port) = rest.strip_prefix(host).and_then(|r| r.strip_prefix(':')) else { continue };
        if port.is_empty() || port.len() > 5 || !port.bytes().all(|b| b.is_ascii_digit()) {
            return Err(refused());
        }
        let port: u16 = port.parse().map_err(|_| refused())?;
        if port == 0 {
            return Err(refused());
        }
        let address = match host {
            "127.0.0.1" => SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
            "[::1]" => SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port),
            _ => {
                let found: Vec<SocketAddr> = ("localhost", port)
                    .to_socket_addrs()
                    .map_err(|e| format!("`localhost` does not resolve: {e}"))?
                    .collect();
                if found.is_empty() || !found.iter().all(|a| a.ip().is_loopback()) {
                    return Err("`localhost` resolves to an address that is not loopback".to_string());
                }
                found[0]
            }
        };
        return Ok(Endpoint { host: host.to_string(), port, address });
    }
    Err(refused())
}

/// What the guide is shown: the task when one is known, one file, and either the diagnostics
/// `check` gave it, root cause first, or the one test block that failed with the values each side
/// of its comparison had.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub task: Option<String>,
    pub path: String,
    pub text: String,
    /// As `lotml check --json` lists them.
    pub diagnostics: Vec<serde_json::Value>,
    /// One row of `lotml test --json`.
    pub failing: Option<serde_json::Value>,
}

/// The longest task the tool takes.
pub const TASK_LIMIT: usize = 2000;

impl State {
    /// The state `lotml guide render` reads: `task`, `path`, `text`, and exactly one of a non-empty
    /// `diagnostics` and a `failing` row.
    pub fn from_json(value: &serde_json::Value) -> Result<State, String> {
        let text_field = |key: &str| {
            value[key].as_str().map(str::to_string).ok_or_else(|| format!("the state needs a string `{key}`"))
        };
        let task = match &value["task"] {
            serde_json::Value::Null => None,
            serde_json::Value::String(task) if task.chars().count() <= TASK_LIMIT => Some(task.clone()),
            serde_json::Value::String(_) => return Err(format!("`task` is longer than {TASK_LIMIT} characters")),
            _ => return Err("`task` is a string or null".to_string()),
        };
        let diagnostics = match &value["diagnostics"] {
            serde_json::Value::Null => Vec::new(),
            serde_json::Value::Array(found) => found.clone(),
            _ => return Err("`diagnostics` is a list or null".to_string()),
        };
        let failing = match &value["failing"] {
            serde_json::Value::Null => None,
            row @ serde_json::Value::Object(_) => Some(row.clone()),
            _ => return Err("`failing` is an object or null".to_string()),
        };
        if diagnostics.is_empty() == failing.is_none() {
            return Err("a state holds either diagnostics or a failing block, and not both".to_string());
        }
        Ok(State { task, path: text_field("path")?, text: text_field("text")?, diagnostics, failing })
    }
}

/// `lotml guide render`: the messages for the state in `path`, and the renderer's version.
pub fn render_file(path: &Path) -> Result<bool, crate::Failure> {
    let text = read(path).map_err(crate::Failure)?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| crate::Failure(format!("{}: not JSON: {e}", path.display())))?;
    let state = State::from_json(&value).map_err(|why| crate::Failure(format!("{}: {why}", path.display())))?;
    let mut rendered = render(&state);
    rendered["renderer"] = serde_json::json!(RENDERER);
    println!("{rendered}");
    Ok(true)
}

/// The guide's system message: what it is shown and what it answers, the kinds named. Changing it
/// changes what [`RENDERER`] stands for.
pub const SYSTEM: &str = "You are lotml's guide. A coding agent's lotml file does not check, or one of its test blocks fails. Say where to change it and what kind of change it needs. Answer with JSON only: {\"locations\": [{\"path\": the file, \"symbol\": the declaration to change, or null before the first one, \"lines\": [first, last]}], \"kind\": \"arm\", \"body\", \"definition\", \"add\", \"remove\", \"lines\" or \"several\", \"edit\": null or {\"tool\": \"replace\", \"add\", \"remove\" or \"edit\", \"arguments\": {...}}}. Give one to three locations, the likeliest first. The line a diagnostic names is often not the one to change.";

/// The messages the guide is asked with: its system message, then the task, the file with its
/// lines numbered, and the diagnostics or the failing block. The one renderer the tool, `lotml
/// guide render` and the records the guide is trained on all use.
pub fn render(state: &State) -> serde_json::Value {
    serde_json::json!({"messages": [
        {"role": "system", "content": SYSTEM},
        {"role": "user", "content": user(state)},
    ]})
}

fn user(state: &State) -> String {
    let mut out = String::new();
    if let Some(task) = state.task.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        out += &format!("Task: {task}\n\n");
    }
    out += &format!("File {}:\n", state.path);
    let lines: Vec<&str> = state.text.lines().collect();
    let width = lines.len().to_string().len().max(3);
    for (number, line) in lines.iter().enumerate() {
        out += &format!("{:>width$} | {line}\n", number + 1);
    }
    out.push('\n');
    if let Some(row) = &state.failing {
        out += &failing(row);
    } else {
        out += "`check` reports:\n";
        for diagnostic in &state.diagnostics {
            out += &diagnosed(diagnostic);
        }
    }
    out
}

fn diagnosed(diagnostic: &serde_json::Value) -> String {
    let field = |key: &str| diagnostic[key].as_str().unwrap_or("");
    let location = &diagnostic["location"];
    let at = match (location["line"].as_u64(), location["column"].as_u64()) {
        (Some(line), Some(column)) => format!(" at {line}:{column}"),
        _ => String::new(),
    };
    let mut out = format!("{} {}{at}: {}\n", field("severity"), field("code"), field("message"));
    for note in diagnostic["notes"].as_array().into_iter().flatten().filter_map(|n| n.as_str()) {
        out += &format!("  note: {note}\n");
    }
    let alternatives: Vec<&str> =
        diagnostic["alternatives"].as_array().into_iter().flatten().filter_map(|a| a.as_str()).collect();
    if !alternatives.is_empty() {
        out += &format!("  in scope: {}\n", alternatives.join(", "));
    }
    out
}

fn failing(row: &serde_json::Value) -> String {
    let field = |key: &str| row[key].as_str().unwrap_or("");
    let name = if row["name"].is_string() { format!("test \"{}\"", field("name")) } else { "loading".to_string() };
    let at = row["line"].as_u64().map(|l| format!(" at line {l}")).unwrap_or_default();
    match field("outcome") {
        "fail" => {
            let mut out = format!("{name} fails{at}:\n  assert {}\n", field("expression"));
            if row["left"].is_string() && row["right"].is_string() {
                out += &format!("  left:  {}\n  right: {}\n", field("left"), field("right"));
            }
            if row["message"].is_string() {
                out += &format!("  message: {}\n", field("message"));
            }
            out
        }
        "error" => format!("{name} fails{at}: `?` passed on {}\n", field("error")),
        _ => format!("{name} stops{at}: {}: {}\n", field("kind"), field("message")),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    use super::*;

    const VALID: &str = "\
url = \"http://127.0.0.1:8088\"   # the guide server
model = \"lotml-guide-0.5b-q4\"
threshold = 0.62
answer = 512
run_candidates = false
renderer = 1
records = \"records/2026-10-06\"
";

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect();
        move |name| map.get(name).cloned()
    }

    fn scratch(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lotml-guide-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (file, text) in files {
            let path = dir.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn diagnostic() -> serde_json::Value {
        serde_json::json!({"code": "E0201", "severity": "error", "message": "`totals` is not defined",
            "location": {"line": 3, "column": 12}, "notes": ["a name is in scope after the statement that declares it"],
            "alternatives": ["total"]})
    }

    #[test]
    fn the_user_turn_is_the_task_the_numbered_file_and_the_diagnostics() {
        let state = State {
            task: Some("Sum the list.".into()),
            path: "a.lotml".into(),
            text: "fn f(xs: [int]) -> int:\n    var total = 0\n    return totals\n".into(),
            diagnostics: vec![diagnostic()],
            failing: None,
        };
        let messages = render(&state)["messages"].clone();
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], SYSTEM);
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(
            messages[1]["content"],
            "Task: Sum the list.\n\nFile a.lotml:\n  1 | fn f(xs: [int]) -> int:\n  2 |     var total = 0\n  3 |     return totals\n\n`check` reports:\nerror E0201 at 3:12: `totals` is not defined\n  note: a name is in scope after the statement that declares it\n  in scope: total\n"
        );
        let untasked = State { task: None, ..state };
        assert!(user(&untasked).starts_with("File a.lotml:\n"));
    }

    #[test]
    fn a_failing_block_is_shown_with_the_values_each_side_had() {
        let row = serde_json::json!({"name": "hidden: 1", "outcome": "fail", "expression": "add(1, 2) == 3",
            "op": "==", "left": "-1", "right": "3", "line": 5, "file": "a.lotml"});
        let state =
            State { task: None, path: "a.lotml".into(), text: "x\n".into(), diagnostics: vec![], failing: Some(row) };
        assert!(
            user(&state)
                .ends_with("\ntest \"hidden: 1\" fails at line 5:\n  assert add(1, 2) == 3\n  left:  -1\n  right: 3\n")
        );
        let panic = serde_json::json!({"name": "t", "outcome": "panic", "kind": "IndexError", "message": "list index out of range", "line": 2});
        let stopped = State { failing: Some(panic), ..state };
        assert!(user(&stopped).ends_with("test \"t\" stops at line 2: IndexError: list index out of range\n"));
    }

    #[test]
    fn a_state_holds_diagnostics_or_a_failing_block_never_both_nor_neither() {
        let base = serde_json::json!({"task": null, "path": "a.lotml", "text": "x\n"});
        let mut diagnosed = base.clone();
        diagnosed["diagnostics"] = serde_json::json!([diagnostic()]);
        assert_eq!(State::from_json(&diagnosed).unwrap().diagnostics.len(), 1);
        let mut both = diagnosed.clone();
        both["failing"] = serde_json::json!({"name": "t"});
        assert!(State::from_json(&both).is_err());
        assert!(State::from_json(&base).is_err());
        let mut long = diagnosed.clone();
        long["task"] = serde_json::json!("x".repeat(TASK_LIMIT + 1));
        assert!(State::from_json(&long).is_err());
        let mut nameless = diagnosed;
        nameless["path"] = serde_json::Value::Null;
        assert!(State::from_json(&nameless).is_err());
    }

    #[test]
    fn the_variable_names_the_file_and_must_be_absolute() {
        let dir = scratch("variable", &[("g.toml", VALID)]);
        let file = dir.join("g.toml");
        let found = locate(&env(&[("LOTML_HARNESS_GUIDE", file.to_str().unwrap())])).unwrap();
        assert_eq!(found, Some(file));
        let relative = locate(&env(&[("LOTML_HARNESS_GUIDE", "g.toml")]));
        assert!(relative.unwrap_err().contains("absolute"));
    }

    #[test]
    fn without_the_variable_the_user_s_lotml_directory_is_searched() {
        let dir = scratch("user", &[("lotml/harness-guide.toml", VALID)]);
        let base = dir.to_str().unwrap();
        let expected = Some(dir.join("lotml").join("harness-guide.toml"));
        assert_eq!(locate(&env(&[("XDG_CONFIG_HOME", base)])).unwrap(), expected);
        assert_eq!(locate(&env(&[("APPDATA", base)])).unwrap(), expected);
        let home = scratch("home", &[(".config/lotml/harness-guide.toml", VALID)]);
        let at_home = Some(home.join(".config").join("lotml").join("harness-guide.toml"));
        assert_eq!(locate(&env(&[("HOME", home.to_str().unwrap())])).unwrap(), at_home);
        assert!(locate(&env(&[("XDG_CONFIG_HOME", "relative")])).unwrap_err().contains("absolute"));
        assert_eq!(locate(&env(&[("XDG_CONFIG_HOME", scratch("empty", &[]).to_str().unwrap())])).unwrap(), None);
        assert_eq!(locate(&env(&[])).unwrap(), None);
    }

    #[test]
    fn a_valid_file_gives_every_key() {
        let config = parse(VALID, Path::new("/guides")).unwrap();
        assert_eq!(config.url.port, 8088);
        assert!(config.url.address.ip().is_loopback());
        assert_eq!(config.model, "lotml-guide-0.5b-q4");
        assert!((config.threshold - 0.62).abs() < 1e-12);
        assert_eq!(config.answer, 512);
        assert!(!config.run_candidates);
        assert_eq!(config.renderer, RENDERER);
        assert_eq!(config.records, Path::new("/guides").join("records/2026-10-06"));
    }

    #[test]
    fn an_unknown_missing_mistyped_or_repeated_key_is_refused() {
        let unknown = format!("{VALID}temperature = 0.5\n");
        assert!(parse(&unknown, Path::new("/")).unwrap_err().contains("temperature"));
        let missing = VALID.replace("answer = 512\n", "");
        assert!(parse(&missing, Path::new("/")).unwrap_err().contains("answer"));
        let mistyped = VALID.replace("threshold = 0.62", "threshold = \"high\"");
        assert!(parse(&mistyped, Path::new("/")).unwrap_err().contains("threshold"));
        let repeated = format!("{VALID}model = \"other\"\n");
        assert!(parse(&repeated, Path::new("/")).unwrap_err().contains("model"));
        assert!(parse("not toml at all", Path::new("/")).is_err());
    }

    #[test]
    fn the_url_must_be_plain_http_to_a_loopback_address_with_a_decimal_port() {
        for accepted in ["http://127.0.0.1:8088", "http://127.0.0.1:8088/", "http://[::1]:1", "http://localhost:65535"]
        {
            let url = endpoint(accepted).unwrap_or_else(|e| panic!("{accepted}: {e}"));
            assert!(url.address.ip().is_loopback(), "{accepted}");
        }
        for refused in [
            "https://127.0.0.1:8088",
            "http://127.0.0.1",
            "http://127.0.0.1:0",
            "http://127.0.0.1:65536",
            "http://127.0.0.1:+80",
            "http://127.0.0.1:0x50",
            "http://127.0.0.1:8088/v1",
            "http://127.0.0.1:8088?x",
            "http://127.0.0.1:8088#x",
            "http://127.0.0.1:8088//",
            "http://user@127.0.0.1:8088",
            "http://127.0.0.1@evil:8088",
            "http://localhost.evil:8088",
            "http://127.1:8088",
            "http://0x7f.0.0.1:8088",
            "http://[::ffff:127.0.0.1]:8088",
            "http://[::1%1]:8088",
            "http://10.0.0.1:8088",
            "http://example.com:8088",
            "http://127.0.0.1:80 88",
            "http://127.0.0.1:8088\n",
            "http:\\\\127.0.0.1:8088",
            "http://127.0.0.1%2e:8088",
            " http://127.0.0.1:8088",
        ] {
            assert!(endpoint(refused).is_err(), "{refused:?} was accepted");
        }
    }

    #[test]
    fn another_renderer_version_is_refused() {
        let other = VALID.replace("renderer = 1", &format!("renderer = {}", RENDERER + 1));
        assert!(parse(&other, Path::new("/")).unwrap_err().contains("renderer"));
    }

    #[test]
    fn a_relative_records_path_is_read_beside_the_file_and_an_absolute_one_as_is() {
        let absolute = std::env::temp_dir().join("records");
        let text = VALID.replace("records/2026-10-06", &absolute.to_string_lossy().replace('\\', "/"));
        let config = parse(&text, Path::new("/guides")).unwrap();
        assert_eq!(config.records, PathBuf::from(absolute.to_string_lossy().replace('\\', "/")));
    }
}
