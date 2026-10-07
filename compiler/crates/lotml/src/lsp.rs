//! `lotml lsp`: the language server. It keeps the workspace's files in the incremental engine,
//! loads and checks every `.lotml` file under the workspace's folders when the client connects —
//! so the first question is answered from a warm index — and answers from memory for any file
//! that did not change since.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

use lotml_diag::{Applicability, Diagnostic, Severity};
use lotml_ide::lines::{Encoding, Lines};
use lotml_ide::{Kind as OutlineKind, Outline, Refused, Workspace};
use lotml_syntax::span::Span;
use serde_json::{Value, json};

use crate::rpc::{self, Framing, Incoming, Kind};
use crate::{Failure, files};

/// The server's own error codes, from the protocol.
const SERVER_NOT_INITIALIZED: i64 = -32002;
const REQUEST_FAILED: i64 = -32803;

/// Serve on standard input and output until the client says `exit`: 0 after a `shutdown`
/// request, 1 without one, as the protocol asks.
pub fn serve() -> Result<u8, Failure> {
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let mut server = Server::default();
    let fail = |e: io::Error| Failure(format!("the client's stream failed: {e}"));
    while let Some(incoming) = rpc::read(&mut input, Framing::Headers).map_err(fail)? {
        let replies = match incoming {
            Incoming::Message(message) => server.handle(&message),
            Incoming::Malformed(why) => vec![rpc::error(&Value::Null, rpc::PARSE_ERROR, why)],
        };
        for reply in replies {
            rpc::write(&mut output, Framing::Headers, &reply).map_err(fail)?;
        }
        if server.exited {
            break;
        }
    }
    Ok(u8::from(!server.shut_down))
}

#[derive(Default)]
pub struct Server {
    workspace: Workspace,
    encoding: Option<Encoding>,
    /// The URI the client named each file by, so replies use its own spelling.
    uris: HashMap<PathBuf, String>,
    /// The files the client has open, whose text is the client's rather than the disk's.
    open: HashSet<PathBuf>,
    shut_down: bool,
    exited: bool,
}

/// Why a request could not be answered: an error the client is told.
struct Refusal(i64, String);

fn invalid(what: &str) -> Refusal {
    Refusal(rpc::INVALID_PARAMS, format!("the request needs {what}"))
}

impl Server {
    /// Everything to send back for one message: its response, and the diagnostics it changed.
    pub fn handle(&mut self, message: &Value) -> Vec<Value> {
        match rpc::kind(message) {
            Kind::Request { id, method, params } => {
                let result = if self.shut_down {
                    Err(Refusal(rpc::INVALID_REQUEST, "the server is shutting down".into()))
                } else if self.encoding.is_none() && method != "initialize" {
                    Err(Refusal(SERVER_NOT_INITIALIZED, "the client has not sent `initialize`".into()))
                } else {
                    self.request(method, params)
                };
                vec![match result {
                    Ok(result) => rpc::response(id, result),
                    Err(Refusal(code, message)) => rpc::error(id, code, message),
                }]
            }
            Kind::Notification { method, params } => {
                if self.encoding.is_none() && method != "exit" {
                    return Vec::new();
                }
                self.notification(method, params)
            }
            Kind::Other => Vec::new(),
        }
    }

    fn encoding(&self) -> Encoding {
        self.encoding.unwrap_or(Encoding::Utf16)
    }

    fn request(&mut self, method: &str, params: &Value) -> Result<Value, Refusal> {
        match method {
            "initialize" => Ok(self.initialize(params)),
            "shutdown" => {
                self.shut_down = true;
                Ok(Value::Null)
            }
            "textDocument/definition" => {
                let (path, offset) = self.position(params)?;
                let Some(found) = self.workspace.at(&path, offset) else { return Ok(Value::Null) };
                let spans = self.workspace.declarations(&path, &found.symbol);
                Ok(Value::Array(spans.into_iter().map(|s| self.location(&path, s)).collect()))
            }
            "textDocument/references" => {
                let (path, offset) = self.position(params)?;
                let Some(found) = self.workspace.at(&path, offset) else { return Ok(Value::Null) };
                let declarations = params["context"]["includeDeclaration"].as_bool().unwrap_or(true);
                let spans = self.workspace.references(&path, &found.symbol, declarations);
                Ok(Value::Array(spans.into_iter().map(|s| self.location(&path, s)).collect()))
            }
            "textDocument/hover" => {
                let (path, offset) = self.position(params)?;
                let Some((span, shown)) = self.workspace.hover(&path, offset) else { return Ok(Value::Null) };
                Ok(json!({
                    "contents": {"kind": "markdown", "value": format!("```lotml\n{shown}\n```")},
                    "range": self.range(&path, span),
                }))
            }
            "textDocument/documentSymbol" => {
                let path = self.document(params)?;
                let outline = self.workspace.outline(&path);
                Ok(Value::Array(outline.iter().map(|o| self.document_symbol(&path, o)).collect()))
            }
            "workspace/symbol" => Ok(self.workspace_symbols(params["query"].as_str().unwrap_or(""))),
            "textDocument/formatting" => {
                let path = self.document(params)?;
                let text = self.workspace.text(&path).unwrap_or_default();
                match lotml_fmt::format(text) {
                    Ok(formatted) if formatted != text => Ok(json!([{
                        "range": self.range(&path, Span::new(0, text.len())),
                        "newText": formatted,
                    }])),
                    _ => Ok(json!([])),
                }
            }
            "textDocument/codeAction" => {
                let path = self.document(params)?;
                let range = self.span_of(&path, &params["range"]).ok_or_else(|| invalid("a range"))?;
                Ok(self.code_actions(&path, range))
            }
            "textDocument/prepareRename" => {
                let (path, offset) = self.position(params)?;
                let Some(found) = self.workspace.at(&path, offset) else { return Ok(Value::Null) };
                let text = self.workspace.text(&path).unwrap_or_default();
                Ok(json!({"range": self.range(&path, found.span), "placeholder": &text[found.span.range()]}))
            }
            "textDocument/rename" => {
                let (path, offset) = self.position(params)?;
                let new_name = params["newName"].as_str().ok_or_else(|| invalid("a newName"))?;
                let found = self
                    .workspace
                    .at(&path, offset)
                    .ok_or_else(|| Refusal(REQUEST_FAILED, "there is no name here to rename".into()))?;
                let sites = self.workspace.references(&path, &found.symbol, true);
                self.workspace
                    .rename(&path, &found.symbol, new_name)
                    .map_err(|Refused(why)| Refusal(REQUEST_FAILED, why))?;
                let edits: Vec<Value> =
                    sites.into_iter().map(|s| json!({"range": self.range(&path, s), "newText": new_name})).collect();
                Ok(json!({"changes": {self.uri(&path): edits}}))
            }
            _ => Err(Refusal(rpc::METHOD_NOT_FOUND, format!("`{method}` is not supported"))),
        }
    }

    fn notification(&mut self, method: &str, params: &Value) -> Vec<Value> {
        match method {
            "initialized" => {
                let paths: Vec<PathBuf> = self.workspace.paths().map(Path::to_path_buf).collect();
                paths.iter().map(|p| self.publish(p)).collect()
            }
            "exit" => {
                self.exited = true;
                Vec::new()
            }
            "textDocument/didOpen" => {
                let document = &params["textDocument"];
                let (Some(uri), Some(text)) = (document["uri"].as_str(), document["text"].as_str()) else {
                    return Vec::new();
                };
                let Some(path) = self.remember(uri) else { return Vec::new() };
                self.open.insert(path.clone());
                self.store(&path, text.to_string(), &mut files::InterfaceCache::default());
                vec![self.publish(&path)]
            }
            "textDocument/didChange" => {
                let Some(path) = params["textDocument"]["uri"].as_str().and_then(path_of) else { return Vec::new() };
                let mut text = self.workspace.text(&path).unwrap_or_default().to_string();
                for change in params["contentChanges"].as_array().into_iter().flatten() {
                    let Some(new) = change["text"].as_str() else { continue };
                    match self.span_in(&text, &change["range"]) {
                        Some(span) => text.replace_range(span.range(), new),
                        None => text = new.to_string(),
                    }
                }
                self.workspace.set(&path, text);
                vec![self.publish(&path)]
            }
            "textDocument/didClose" => {
                let Some(path) = params["textDocument"]["uri"].as_str().and_then(path_of) else { return Vec::new() };
                self.open.remove(&path);
                self.reload(&path)
            }
            "workspace/didChangeWatchedFiles" => {
                let mut out = Vec::new();
                let mut interfaces_changed = false;
                for change in params["changes"].as_array().into_iter().flatten() {
                    let Some(uri) = change["uri"].as_str() else { continue };
                    let Some(path) = path_of(uri) else { continue };
                    if path.extension().is_some_and(|e| e == "lotmli") {
                        interfaces_changed = true;
                        continue;
                    }
                    self.remember(uri);
                    if !self.open.contains(&path) {
                        out.extend(self.reload(&path));
                    }
                }
                if interfaces_changed {
                    // A binding regenerated changes what every file importing it checks to.
                    let mut cache = files::InterfaceCache::default();
                    let paths: Vec<PathBuf> = self.workspace.paths().map(Path::to_path_buf).collect();
                    for path in paths {
                        if self.workspace.set_interfaces(&path, cache.get(&path)) {
                            out.push(self.publish(&path));
                        }
                    }
                }
                out
            }
            _ => Vec::new(),
        }
    }

    fn initialize(&mut self, params: &Value) -> Value {
        let offered = params["capabilities"]["general"]["positionEncodings"].as_array();
        let utf8 = offered.is_some_and(|all| all.iter().any(|e| e == "utf-8"));
        self.encoding = Some(if utf8 { Encoding::Utf8 } else { Encoding::Utf16 });
        let mut roots: Vec<&str> = params["workspaceFolders"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|folder| folder["uri"].as_str())
            .collect();
        if roots.is_empty() {
            roots.extend(params["rootUri"].as_str());
        }
        let mut folders: Vec<PathBuf> = roots.into_iter().filter_map(path_of).collect();
        if folders.is_empty()
            && let Some(root) = params["rootPath"].as_str()
        {
            folders.push(normalize(PathBuf::from(root)));
        }
        let mut cache = files::InterfaceCache::default();
        for folder in folders.iter().filter(|f| f.is_dir()) {
            // A folder that cannot be read leaves the workspace with what could.
            for path in files::sources(std::slice::from_ref(folder)).unwrap_or_default() {
                if let Ok(text) = files::read(&path) {
                    self.store(&normalize(path), text, &mut cache);
                }
            }
        }
        self.workspace.warm();
        json!({
            "capabilities": {
                "positionEncoding": if utf8 { "utf-8" } else { "utf-16" },
                "textDocumentSync": {"openClose": true, "change": 1, "save": {"includeText": false}},
                "definitionProvider": true,
                "referencesProvider": true,
                "hoverProvider": true,
                "documentSymbolProvider": true,
                "workspaceSymbolProvider": true,
                "documentFormattingProvider": true,
                "codeActionProvider": {"codeActionKinds": ["quickfix"]},
                "renameProvider": {"prepareProvider": true},
            },
            "serverInfo": {"name": "lotml", "version": env!("CARGO_PKG_VERSION")},
        })
    }

    /// Give the workspace a file's text and the interfaces of the Python modules it may import.
    fn store(&mut self, path: &Path, text: String, interfaces: &mut files::InterfaceCache) {
        self.workspace.set(path, text);
        self.workspace.set_interfaces(path, interfaces.get(path));
    }

    /// The path a URI names, remembering how the client spelled it.
    fn remember(&mut self, uri: &str) -> Option<PathBuf> {
        let path = path_of(uri)?;
        self.uris.insert(path.clone(), uri.to_string());
        Some(path)
    }

    /// Read a closed or changed file from the disk again, or forget it when it is gone. Only a
    /// `.lot` or `.lotml` file is read: a URI naming anything else is not the server's to open.
    fn reload(&mut self, path: &Path) -> Vec<Value> {
        let source = files::is_source(path);
        match source.then(|| std::fs::read_to_string(path).ok()).flatten() {
            Some(text) => {
                self.store(path, text, &mut files::InterfaceCache::default());
                vec![self.publish(path)]
            }
            None => {
                if self.workspace.remove(path) {
                    vec![rpc::notification(
                        "textDocument/publishDiagnostics",
                        json!({"uri": self.uri(path), "diagnostics": []}),
                    )]
                } else {
                    Vec::new()
                }
            }
        }
    }

    fn uri(&self, path: &Path) -> String {
        self.uris.get(path).cloned().unwrap_or_else(|| uri_of(path))
    }

    fn document(&self, params: &Value) -> Result<PathBuf, Refusal> {
        let path = params["textDocument"]["uri"].as_str().and_then(path_of).ok_or_else(|| invalid("a file: URI"))?;
        if self.workspace.contains(&path) {
            Ok(path)
        } else {
            Err(Refusal(rpc::INVALID_PARAMS, format!("{} is not open or in the workspace", path.display())))
        }
    }

    fn position(&self, params: &Value) -> Result<(PathBuf, u32), Refusal> {
        let path = self.document(params)?;
        let text = self.workspace.text(&path).unwrap_or_default();
        let offset = self.offset_in(text, &params["position"]).ok_or_else(|| invalid("a position"))?;
        Ok((path, offset))
    }

    fn offset_in(&self, text: &str, position: &Value) -> Option<u32> {
        let line = u32::try_from(position["line"].as_u64()?).ok()?;
        let character = u32::try_from(position["character"].as_u64()?).ok()?;
        Some(Lines::new(text).offset(line, character, self.encoding()))
    }

    fn span_in(&self, text: &str, range: &Value) -> Option<Span> {
        let start = self.offset_in(text, &range["start"])?;
        let end = self.offset_in(text, &range["end"])?;
        (start <= end).then_some(Span { start, end })
    }

    fn span_of(&self, path: &Path, range: &Value) -> Option<Span> {
        self.span_in(self.workspace.text(path)?, range)
    }

    fn range(&self, path: &Path, span: Span) -> Value {
        let lines = Lines::new(self.workspace.text(path).unwrap_or_default());
        let (line, character) = lines.position(span.start, self.encoding());
        let (end_line, end_character) = lines.position(span.end, self.encoding());
        json!({"start": {"line": line, "character": character}, "end": {"line": end_line, "character": end_character}})
    }

    fn location(&self, path: &Path, span: Span) -> Value {
        json!({"uri": self.uri(path), "range": self.range(path, span)})
    }

    fn diagnostic(&self, path: &Path, d: &Diagnostic) -> Value {
        let mut message = d.message.clone();
        for note in &d.notes {
            message += &format!("\nnote: {note}");
        }
        if !d.alternatives.is_empty() {
            message += &format!("\nalternatives: {}", d.alternatives.join(", "));
        }
        let related: Vec<Value> =
            d.labels.iter().map(|l| json!({"location": self.location(path, l.span), "message": l.message})).collect();
        json!({
            "range": self.range(path, d.span),
            "severity": if d.severity == Severity::Error { 1 } else { 2 },
            "code": d.code,
            "source": "lotml",
            "message": message,
            "relatedInformation": related,
        })
    }

    fn publish(&self, path: &Path) -> Value {
        let diagnostics: Vec<Value> =
            self.workspace.diagnostics(path).iter().map(|d| self.diagnostic(path, d)).collect();
        rpc::notification("textDocument/publishDiagnostics", json!({"uri": self.uri(path), "diagnostics": diagnostics}))
    }

    /// The fixes of the diagnostics touching `range`, the safe ones preferred.
    fn code_actions(&self, path: &Path, range: Span) -> Value {
        let touches = |s: Span| s.start <= range.end && range.start <= s.end;
        let mut actions = Vec::new();
        for d in self.workspace.diagnostics(path).iter().filter(|d| touches(d.span)) {
            for fix in &d.fixes {
                let edits: Vec<Value> = fix
                    .edits
                    .iter()
                    .map(|e| json!({"range": self.range(path, e.span), "newText": e.replacement}))
                    .collect();
                actions.push(json!({
                    "title": fix.message,
                    "kind": "quickfix",
                    "diagnostics": [self.diagnostic(path, d)],
                    "isPreferred": fix.applicability == Applicability::MachineApplicable,
                    "edit": {"changes": {self.uri(path): edits}},
                }));
            }
        }
        Value::Array(actions)
    }

    fn document_symbol(&self, path: &Path, o: &Outline) -> Value {
        json!({
            "name": o.name,
            "kind": symbol_kind(o.kind),
            "range": self.range(path, o.span),
            "selectionRange": self.range(path, o.name_span),
            "children": o.children.iter().map(|c| self.document_symbol(path, c)).collect::<Vec<_>>(),
        })
    }

    /// The declarations whose name holds the query, ignoring case, across the workspace.
    fn workspace_symbols(&self, query: &str) -> Value {
        let query = query.to_lowercase();
        let mut found = Vec::new();
        for path in self.workspace.paths() {
            let outline = self.workspace.outline(path);
            let mut stack: Vec<(&Outline, Option<&str>)> = outline.iter().map(|o| (o, None)).collect();
            while let Some((o, container)) = stack.pop() {
                if o.name.to_lowercase().contains(&query) {
                    found.push(json!({
                        "name": o.name,
                        "kind": symbol_kind(o.kind),
                        "location": self.location(path, o.name_span),
                        "containerName": container,
                    }));
                }
                stack.extend(o.children.iter().map(|c| (c, Some(o.name.as_str()))));
            }
        }
        Value::Array(found)
    }
}

/// The protocol's number for each kind of declaration.
fn symbol_kind(kind: OutlineKind) -> u8 {
    match kind {
        OutlineKind::Function | OutlineKind::Test => 12,
        OutlineKind::Record => 23,
        OutlineKind::Sum => 10,
        OutlineKind::Variant => 22,
        OutlineKind::Field => 8,
        OutlineKind::Trait => 11,
        OutlineKind::Impl => 19,
        OutlineKind::Method => 6,
    }
}

/// The path a `file:` URI names, in the form the workspace keys files by.
pub fn path_of(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    // `file:///path`, or `file://localhost/path`; another host is not a local file.
    let path = match rest.find('/') {
        Some(0) => rest,
        Some(i) if rest[..i].eq_ignore_ascii_case("localhost") => &rest[i..],
        _ => return None,
    };
    let decoded = percent_decode(path)?;
    // On Windows `/C:/x` names `C:/x`.
    let bytes = decoded.as_bytes();
    let local = if cfg!(windows) && bytes.len() >= 3 && bytes[2] == b':' && bytes[1].is_ascii_alphabetic() {
        &decoded[1..]
    } else {
        &decoded
    };
    Some(normalize(PathBuf::from(local)))
}

/// One spelling per file: on Windows, backslashes and an upper-case drive letter, as a path
/// found on the disk and one sent by a client may differ in either.
pub fn normalize(path: PathBuf) -> PathBuf {
    if !cfg!(windows) {
        return path;
    }
    let mut text = path.to_string_lossy().replace('/', "\\");
    if text.as_bytes().get(1) == Some(&b':') {
        text[..1].make_ascii_uppercase();
    }
    PathBuf::from(text)
}

/// A `file:` URI for a path, its unsafe characters escaped.
pub fn uri_of(path: &Path) -> String {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut text = absolute.to_string_lossy().replace('\\', "/");
    if !text.starts_with('/') {
        text.insert(0, '/');
    }
    let mut uri = String::from("file://");
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/:".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            uri += &format!("%{byte:02X}");
        }
    }
    uri
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_and_paths_round_trip() {
        let path = normalize(std::env::temp_dir().join("a dir").join("ç.lotml"));
        let uri = uri_of(&path);
        assert!(uri.starts_with("file:///") && uri.contains("a%20dir") && uri.contains("%C3%A7"));
        assert_eq!(path_of(&uri), Some(path));
    }

    #[test]
    fn a_uri_naming_another_host_or_scheme_is_not_a_file() {
        assert_eq!(path_of("http://x/a.lotml"), None);
        assert_eq!(path_of("file://server/share/a.lotml"), None);
        assert_eq!(path_of("file:///a%zz"), None, "a broken escape");
    }

    #[cfg(windows)]
    #[test]
    fn a_drive_letter_is_spelled_one_way() {
        assert_eq!(path_of("file:///c%3A/Users/x.lotml"), Some(PathBuf::from("C:\\Users\\x.lotml")));
        assert_eq!(path_of("file:///C:/Users/x.lotml"), Some(PathBuf::from("C:\\Users\\x.lotml")));
    }
}
