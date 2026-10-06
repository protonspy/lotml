//! `lotml mcp`: the compiler as MCP tools for an agent — check, digest, show, references,
//! definition, hover, explain and test — over the same incremental engine as the language server.
//!
//! Every `.lotml` file under the root is loaded and checked at start, so the first call is
//! answered from a warm index; before each call the files whose modification time changed are
//! read again, and only those are checked again. References carry the two lines around each,
//! since a bare location makes an agent read the file back to see the site.
//!
//! The server speaks both eras of the protocol: the handshake of `initialize` (2025-11-25 and
//! earlier) and per-request metadata with `server/discover` (2026-07-28).

use std::collections::HashMap;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use lotml_diag::{DEFAULT_LIMIT, Report};
use lotml_ide::lines::{Encoding, Lines};
use lotml_ide::{Symbol, Workspace};
use lotml_syntax::span::{Span, line_column};
use serde_json::{Value, json};

use crate::rpc::{self, Framing, Incoming, Kind};
use crate::{Failure, exec, files, index};

const MODERN: &[&str] = &["2026-07-28"];
const LEGACY: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const UNSUPPORTED_VERSION: i64 = -32022;
/// How many lines around each reference a result shows.
const AROUND: usize = 2;

const INSTRUCTIONS: &str = "lotml's compiler. Run `check` after every edit: diagnostics come root cause first, with the alternatives in scope and fixes. `digest` is the project's index of signatures; `show` gives a symbol's body; `references` lists every use with the lines around it; `explain` gives an error code's page; `test` runs the test blocks.";

/// Serve on standard input and output until the input ends.
pub fn serve(root: &Path) -> Result<u8, Failure> {
    let mut server = Server::new(root)?;
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let fail = |e: io::Error| Failure(format!("the client's stream failed: {e}"));
    while let Some(incoming) = rpc::read(&mut input, Framing::Lines).map_err(fail)? {
        let reply = match incoming {
            Incoming::Message(message) => server.handle(&message),
            Incoming::Malformed(why) => Some(rpc::error(&Value::Null, rpc::PARSE_ERROR, why)),
        };
        if let Some(reply) = reply {
            rpc::write(&mut output, Framing::Lines, &reply).map_err(fail)?;
        }
    }
    Ok(0)
}

pub struct Server {
    root: PathBuf,
    workspace: Workspace,
    /// When each file was last read, so an unchanged one is not read again.
    modified: HashMap<PathBuf, Option<SystemTime>>,
}

/// A tool's failure, told to the agent as a result so it can correct the call.
struct ToolError(String);

impl Server {
    /// A server over every `.lotml` file under `root`, loaded and checked.
    pub fn new(root: &Path) -> Result<Server, Failure> {
        let root = std::path::absolute(root).map_err(|e| Failure(format!("{}: {e}", root.display())))?;
        if !root.is_dir() {
            return Err(Failure(format!("{} is not a directory", root.display())));
        }
        let mut server = Server { root, workspace: Workspace::new(), modified: HashMap::new() };
        server.refresh();
        server.workspace.warm();
        Ok(server)
    }

    /// Read again the files that changed on the disk, add new ones and forget deleted ones.
    fn refresh(&mut self) {
        let found = files::expand(std::slice::from_ref(&self.root)).unwrap_or_default();
        let gone: Vec<PathBuf> = self.modified.keys().filter(|p| !found.contains(p)).cloned().collect();
        for path in gone {
            self.modified.remove(&path);
            self.workspace.remove(&path);
        }
        for path in found {
            let stamp = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            if self.modified.get(&path).is_some_and(|known| *known == stamp && stamp.is_some()) {
                continue;
            }
            if let Ok(text) = files::read(&path) {
                self.workspace.set(&path, text);
                self.modified.insert(path, stamp);
            }
        }
    }

    /// The reply to one message; a notification has none.
    pub fn handle(&mut self, message: &Value) -> Option<Value> {
        let Kind::Request { id, method, params } = rpc::kind(message) else { return None };
        let modern = params["_meta"].get("io.modelcontextprotocol/protocolVersion");
        if let Some(version) = modern {
            if !version.as_str().is_some_and(|v| MODERN.contains(&v)) {
                let mut error = rpc::error(id, UNSUPPORTED_VERSION, "Unsupported protocol version");
                error["error"]["data"] = json!({"supported": supported(), "requested": version});
                return Some(error);
            }
            if params["_meta"].get("io.modelcontextprotocol/clientCapabilities").is_none()
                && method != "server/discover"
            {
                return Some(rpc::error(id, rpc::INVALID_PARAMS, "a request needs its clientCapabilities in _meta"));
            }
        }
        let result = match method {
            "initialize" => Ok(self.initialize(params)),
            "server/discover" => Ok(json!({
                "supportedVersions": supported(),
                "capabilities": {"tools": {}},
                "instructions": INSTRUCTIONS,
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools": tools()})),
            "tools/call" => self.call(params),
            _ => Err((rpc::METHOD_NOT_FOUND, format!("`{method}` is not supported"))),
        };
        Some(match result {
            Ok(mut result) => {
                if modern.is_some() || method == "server/discover" {
                    result["resultType"] = json!("complete");
                    result["_meta"] = json!({"io.modelcontextprotocol/serverInfo": server_info()});
                }
                rpc::response(id, result)
            }
            Err((code, message)) => rpc::error(id, code, message),
        })
    }

    fn initialize(&self, params: &Value) -> Value {
        let requested = params["protocolVersion"].as_str().unwrap_or("");
        let version = if LEGACY.contains(&requested) { requested } else { LEGACY[0] };
        json!({
            "protocolVersion": version,
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": server_info(),
            "instructions": INSTRUCTIONS,
        })
    }

    fn call(&mut self, params: &Value) -> Result<Value, (i64, String)> {
        let name = params["name"].as_str().ok_or((rpc::INVALID_PARAMS, "a call needs the tool's name".to_string()))?;
        if !tools().iter().any(|t| t["name"] == name) {
            return Err((rpc::INVALID_PARAMS, format!("Unknown tool: {name}")));
        }
        static EMPTY: Value = Value::Null;
        let args = params.get("arguments").unwrap_or(&EMPTY);
        self.refresh();
        let outcome = match name {
            "check" => self.check(args),
            "digest" => self.sources(args).map(|s| index::digest(&s)),
            "show" => self.show(args),
            "references" => self.references(args),
            "definition" => self.definition(args),
            "hover" => self.hover(args),
            "explain" => {
                let code = args["code"].as_str().unwrap_or("");
                crate::explanation(code).map_err(|Failure(why)| ToolError(why))
            }
            _ => self.test(args),
        };
        let (text, failed) = match outcome {
            Ok(text) => (text, false),
            Err(ToolError(why)) => (why, true),
        };
        Ok(json!({"content": [{"type": "text", "text": text}], "isError": failed}))
    }

    /// A path an agent named, which must be inside the root.
    fn inside(&self, named: &str) -> Result<PathBuf, ToolError> {
        let path = Path::new(named);
        if path.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(ToolError(format!("`{named}` leaves the project: name a path inside it")));
        }
        let joined = if path.is_absolute() { path.to_path_buf() } else { self.root.join(path) };
        if !joined.starts_with(&self.root) {
            return Err(ToolError(format!("`{named}` is outside the project at {}", self.root.display())));
        }
        Ok(joined)
    }

    /// The workspace's files named by `paths` (files or directories), or all of them.
    fn selected(&self, args: &Value) -> Result<Vec<PathBuf>, ToolError> {
        let Some(named) = args["paths"].as_array() else {
            return Ok(self.workspace.paths().map(Path::to_path_buf).collect());
        };
        let mut out = Vec::new();
        for name in named {
            let name = name.as_str().ok_or_else(|| ToolError("`paths` holds strings".into()))?;
            let wanted = self.inside(name)?;
            let before = out.len();
            out.extend(self.workspace.paths().filter(|p| p.starts_with(&wanted)).map(Path::to_path_buf));
            if out.len() == before {
                return Err(ToolError(format!("no .lotml file at `{name}`")));
            }
        }
        Ok(out)
    }

    /// How a path is shown: relative to the root.
    fn shown(&self, path: &Path) -> String {
        path.strip_prefix(&self.root).unwrap_or(path).display().to_string().replace('\\', "/")
    }

    fn sources(&self, args: &Value) -> Result<Vec<index::Source>, ToolError> {
        Ok(self
            .selected(args)?
            .into_iter()
            .map(|p| index::Source {
                name: self.shown(&p),
                text: self.workspace.text(&p).unwrap_or_default().to_string(),
            })
            .collect())
    }

    fn check(&self, args: &Value) -> Result<String, ToolError> {
        let paths = self.selected(args)?;
        let names: Vec<String> = paths.iter().map(|p| self.shown(p)).collect();
        let reports: Vec<Report> = paths
            .iter()
            .zip(&names)
            .map(|(p, name)| Report {
                file: name,
                text: self.workspace.text(p).unwrap_or_default(),
                diagnostics: self.workspace.diagnostics(p).to_vec(),
            })
            .collect();
        let limit = if args["all"].as_bool() == Some(true) { None } else { Some(DEFAULT_LIMIT) };
        Ok(lotml_diag::json(&reports, limit).to_string())
    }

    fn show(&self, args: &Value) -> Result<String, ToolError> {
        let symbol = args["symbol"].as_str().ok_or_else(|| ToolError("`show` needs a `symbol`".into()))?;
        index::show(symbol, &self.sources(args)?).map_err(|close| {
            let hint = if close.is_empty() { String::new() } else { format!("; the closest are {}", close.join(", ")) };
            ToolError(format!("nothing is called `{symbol}`{hint}"))
        })
    }

    /// The symbol an agent means: one named, `area` or `Counter.get`, or the one at a line and
    /// column (both from 1) of a file.
    fn target(&self, args: &Value) -> Result<Vec<(PathBuf, Symbol)>, ToolError> {
        if let Some(name) = args["symbol"].as_str() {
            let found = self.workspace.find(name);
            if found.is_empty() {
                return Err(ToolError(format!(
                    "nothing in the project declares `{name}`; name a function, type, variant or `Type.member`, or give a path, line and column"
                )));
            }
            return Ok(found);
        }
        let (path, offset) = self.position(args)?;
        let found = self
            .workspace
            .at(&path, offset)
            .ok_or_else(|| ToolError(format!("there is no name at {}:{}", self.shown(&path), args["line"])))?;
        Ok(vec![(path, found.symbol.clone())])
    }

    fn position(&self, args: &Value) -> Result<(PathBuf, u32), ToolError> {
        let usage = "give `path`, `line` and `column` (from 1), or a `symbol`";
        let name = args["path"].as_str().ok_or_else(|| ToolError(usage.into()))?;
        let path = self.inside(name)?;
        let text = self.workspace.text(&path).ok_or_else(|| ToolError(format!("no .lotml file at `{name}`")))?;
        let line = args["line"].as_u64().filter(|&l| l >= 1).ok_or_else(|| ToolError(usage.into()))?;
        let column = args["column"].as_u64().filter(|&c| c >= 1).ok_or_else(|| ToolError(usage.into()))?;
        let lines = Lines::new(text);
        // Columns count characters, as diagnostics do.
        let start = lines.offset(u32::try_from(line - 1).unwrap_or(u32::MAX), 0, Encoding::Utf8) as usize;
        let within = lines.line((line - 1) as usize);
        let at = within.char_indices().nth((column - 1) as usize).map_or(within.len(), |(i, _)| i);
        Ok((path, u32::try_from(start + at).unwrap_or(u32::MAX)))
    }

    fn references(&self, args: &Value) -> Result<String, ToolError> {
        let mut out = String::new();
        let mut count = 0;
        for (path, symbol) in self.target(args)? {
            for span in self.workspace.references(&path, &symbol, true) {
                let declared = self.workspace.declarations(&path, &symbol).contains(&span);
                out += &self.site(&path, span, if declared { "declaration" } else { "" });
                count += 1;
            }
        }
        Ok(format!("{count} reference{}\n\n{out}", if count == 1 { "" } else { "s" }))
    }

    fn definition(&self, args: &Value) -> Result<String, ToolError> {
        let mut out = String::new();
        for (path, symbol) in self.target(args)? {
            for span in self.workspace.declarations(&path, &symbol) {
                out += &self.site(&path, span, "declaration");
            }
        }
        if out.is_empty() {
            return Err(ToolError(
                "this name is not declared in the project: it is the prelude's or a module's".into(),
            ));
        }
        Ok(out)
    }

    fn hover(&self, args: &Value) -> Result<String, ToolError> {
        let (path, offset) = self.position(args)?;
        self.workspace
            .hover(&path, offset)
            .map(|(_, shown)| shown)
            .ok_or_else(|| ToolError("there is no name declared in the project at that position".into()))
    }

    /// A place in a file, with the lines around it and its own line marked.
    fn site(&self, path: &Path, span: Span, what: &str) -> String {
        let text = self.workspace.text(path).unwrap_or_default();
        let (line, column) = line_column(text, span.start);
        let lines = Lines::new(text);
        let first = line.saturating_sub(AROUND).max(1);
        let last = (line + AROUND).min(lines.count());
        let width = last.to_string().len();
        let mut out = format!("{}:{line}:{column}", self.shown(path));
        if !what.is_empty() {
            out += &format!("  {what}");
        }
        out.push('\n');
        for n in first..=last {
            let mark = if n == line { '>' } else { ' ' };
            out += format!("{mark} {n:>width$} | {}", lines.line(n - 1)).trim_end();
            out.push('\n');
        }
        out.push('\n');
        out
    }

    fn test(&self, args: &Value) -> Result<String, ToolError> {
        let paths = self.selected(args)?;
        let (_, report) = exec::test_report(&paths, true).map_err(|Failure(why)| ToolError(why))?;
        Ok(report)
    }
}

fn supported() -> Vec<&'static str> {
    MODERN.iter().chain(LEGACY).copied().collect()
}

fn server_info() -> Value {
    json!({"name": "lotml", "version": env!("CARGO_PKG_VERSION")})
}

/// The tools, each with the schema of its arguments.
fn tools() -> Vec<Value> {
    let paths = json!({
        "type": "array",
        "items": {"type": "string"},
        "description": "Files or directories inside the project; all of its .lotml files when absent."
    });
    let at = |what: &str| {
        json!({
            "type": "object",
            "properties": {
                "symbol": {"type": "string", "description": "A function, type, trait or variant, or `Type.member` for a field or method."},
                "path": {"type": "string", "description": "A .lotml file inside the project."},
                "line": {"type": "integer", "minimum": 1},
                "column": {"type": "integer", "minimum": 1, "description": "In characters, from 1."}
            },
            "description": what
        })
    };
    vec![
        json!({
            "name": "check",
            "description": "Check files for syntax, type and mutability errors. Returns versioned JSON: diagnostics root cause first, each with its code, location, the alternatives in scope and fixes; five at most unless `all`.",
            "inputSchema": {"type": "object", "properties": {"paths": paths, "all": {"type": "boolean"}}}
        }),
        json!({
            "name": "digest",
            "description": "The project's index: every type in full and every function's signature with its documentation, without bodies.",
            "inputSchema": {"type": "object", "properties": {"paths": paths}}
        }),
        json!({
            "name": "show",
            "description": "A symbol as written, `name` or `Type.method`, then the functions and types it uses.",
            "inputSchema": {"type": "object", "properties": {"symbol": {"type": "string"}, "paths": paths}, "required": ["symbol"]}
        }),
        json!({
            "name": "references",
            "description": "Every place a symbol is named, its declaration marked, each with the two lines around it.",
            "inputSchema": at("Name the symbol, or point at it with path, line and column.")
        }),
        json!({
            "name": "definition",
            "description": "Where a symbol is declared, with the two lines around it.",
            "inputSchema": at("Name the symbol, or point at it with path, line and column.")
        }),
        json!({
            "name": "hover",
            "description": "The type of the local, or the signature of the declaration, at a position.",
            "inputSchema": {
                "type": "object",
                "properties": {"path": {"type": "string"}, "line": {"type": "integer", "minimum": 1}, "column": {"type": "integer", "minimum": 1}},
                "required": ["path", "line", "column"]
            }
        }),
        json!({
            "name": "explain",
            "description": "The page explaining an error code, such as E0204.",
            "inputSchema": {"type": "object", "properties": {"code": {"type": "string"}}, "required": ["code"]}
        }),
        json!({
            "name": "test",
            "description": "Run the `test` blocks and report each as JSON: pass, or fail with the values each side of the comparison had.",
            "inputSchema": {"type": "object", "properties": {"paths": paths}}
        }),
    ]
}
