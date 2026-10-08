//! `lotml lsp` and `lotml mcp` as a client drives them: messages in on standard input, replies
//! out on standard output.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

const SHAPES: &str = "\
type Shape = Circle(r: f64) | Empty

fn area(s: Shape) -> f64:
    match s:
        case Circle(r):
            return 3.0 * r * r
        case Empty:
            return 0.0

fn twice(s: Shape) -> f64:
    return area(s) + area(s)
";

/// A fresh directory for one test, holding `files`.
fn scratch(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    for (file, text) in files {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).expect("a scratch file");
    }
    dir
}

struct Client {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    headers: bool,
}

impl Client {
    fn start(args: &[&str], headers: bool) -> Client {
        Client::start_with(args, headers, |_| {})
    }

    /// The server started with `setup` applied to its command first.
    fn start_with(args: &[&str], headers: bool, setup: impl FnOnce(&mut Command)) -> Client {
        // A guide this machine has configured would add a tool; these tests serve without one.
        let no_guide = Path::new(env!("CARGO_TARGET_TMPDIR")).join("no-harness-guide.toml");
        let mut command = Command::new(env!("CARGO_BIN_EXE_lotml"));
        setup(&mut command);
        let mut child = command
            .args(args)
            .env("LOTML_HARNESS_GUIDE", no_guide)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("the server starts");
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        Client { child, input, output, headers }
    }

    fn send(&mut self, message: &Value) {
        let text = message.to_string();
        if self.headers {
            write!(self.input, "Content-Length: {}\r\n\r\n{text}", text.len()).unwrap();
        } else {
            writeln!(self.input, "{text}").unwrap();
        }
        self.input.flush().unwrap();
    }

    fn receive(&mut self) -> Value {
        if self.headers {
            let mut length = 0;
            loop {
                let mut line = String::new();
                self.output.read_line(&mut line).unwrap();
                let line = line.trim_end();
                if line.is_empty() {
                    break;
                }
                if let Some(n) = line.strip_prefix("Content-Length: ") {
                    length = n.parse().unwrap();
                }
            }
            let mut body = vec![0; length];
            self.output.read_exact(&mut body).unwrap();
            serde_json::from_slice(&body).unwrap()
        } else {
            let mut line = String::new();
            self.output.read_line(&mut line).unwrap();
            serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON ({e}): {line:?}"))
        }
    }

    /// The response to request `id`, and the notifications that came before it.
    fn request(&mut self, id: u64, method: &str, params: Value) -> (Value, Vec<Value>) {
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let mut before = Vec::new();
        loop {
            let message = self.receive();
            if message["id"] == id {
                return (message, before);
            }
            before.push(message);
        }
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(&json!({"jsonrpc": "2.0", "method": method, "params": params}));
    }
}

fn uri(path: &Path) -> String {
    let text = path.display().to_string().replace('\\', "/");
    let text = if text.starts_with('/') { text } else { format!("/{text}") };
    format!("file://{}", text.replace(' ', "%20"))
}

/// The position (from 0) of the `n`th occurrence of `word`, in UTF-16 units as the protocol counts.
fn position(text: &str, word: &str, n: usize) -> Value {
    let offset = text.match_indices(word).nth(n).unwrap().0;
    let line = text[..offset].matches('\n').count();
    let start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    json!({"line": line, "character": text[start..offset].encode_utf16().count()})
}

#[test]
fn the_language_server_answers_from_the_workspace_it_loaded() {
    let dir = scratch("lsp", &[("shapes.lotml", SHAPES), ("sub/broken.lotml", "fn f() -> int:\n    return y\n")]);
    let file = uri(&dir.join("shapes.lotml"));
    let mut lsp = Client::start(&["lsp"], true);

    let (init, _) = lsp.request(1, "initialize", json!({"rootUri": uri(&dir), "capabilities": {}}));
    assert_eq!(init["result"]["capabilities"]["positionEncoding"], "utf-16");
    assert_eq!(init["result"]["capabilities"]["referencesProvider"], true);

    // Every file found under the root is checked and reported once the client is ready.
    lsp.notify("initialized", json!({}));
    let (_, published) = lsp.request(2, "workspace/symbol", json!({"query": "area"}));
    let reports: Vec<&Value> = published.iter().filter(|m| m["method"] == "textDocument/publishDiagnostics").collect();
    assert_eq!(reports.len(), 2);
    let broken = reports.iter().find(|r| r["params"]["uri"].as_str().unwrap().ends_with("broken.lotml")).unwrap();
    assert_eq!(broken["params"]["diagnostics"][0]["code"], "E0201");

    let at_call = json!({"textDocument": {"uri": file}, "position": position(SHAPES, "area", 1)});
    let (definition, _) = lsp.request(3, "textDocument/definition", at_call.clone());
    assert_eq!(definition["result"][0]["range"]["start"], json!({"line": 2, "character": 3}));

    let mut refs = at_call.clone();
    refs["context"] = json!({"includeDeclaration": false});
    let (references, _) = lsp.request(4, "textDocument/references", refs);
    let lines: Vec<u64> = references["result"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["range"]["start"]["line"].as_u64().unwrap())
        .collect();
    assert_eq!(lines, vec![10, 10]);

    let (hover, _) = lsp.request(5, "textDocument/hover", at_call);
    assert!(hover["result"]["contents"]["value"].as_str().unwrap().contains("fn area(s: Shape) -> f64"));

    // An edit the client has not saved is checked as typed.
    let edited = SHAPES.replace("area(s) + area(s)", "area(s) + aera(s)");
    lsp.notify(
        "textDocument/didOpen",
        json!({"textDocument": {"uri": file, "languageId": "lotml", "version": 1, "text": SHAPES}}),
    );
    lsp.notify(
        "textDocument/didChange",
        json!({"textDocument": {"uri": file, "version": 2}, "contentChanges": [{"text": edited}]}),
    );
    let (symbols, published) = lsp.request(6, "textDocument/documentSymbol", json!({"textDocument": {"uri": file}}));
    let last = published.iter().rev().find(|m| m["method"] == "textDocument/publishDiagnostics").unwrap();
    assert_eq!(last["params"]["uri"], file);
    let message = last["params"]["diagnostics"][0]["message"].as_str().unwrap();
    assert!(message.contains("`aera` is not defined") && message.contains("alternatives: area"), "{message}");
    let names: Vec<&str> = symbols["result"].as_array().unwrap().iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["Shape", "area", "twice"]);

    let (unknown, _) = lsp.request(7, "textDocument/nothing", json!({}));
    assert_eq!(unknown["error"]["code"], -32601);

    let (down, _) = lsp.request(8, "shutdown", Value::Null);
    assert_eq!(down["result"], Value::Null);
    lsp.notify("exit", Value::Null);
    assert_eq!(lsp.child.wait().unwrap().code(), Some(0));
}

#[test]
fn the_language_server_offers_fixes_and_the_canonical_form() {
    let dir = scratch("lsp-fix", &[]);
    let file = uri(&dir.join("habit.lotml"));
    let text = "fn f() -> bool:\n    return true\n";
    let mut lsp = Client::start(&["lsp"], true);
    lsp.request(
        1,
        "initialize",
        json!({"rootUri": uri(&dir), "capabilities": {"general": {"positionEncodings": ["utf-8", "utf-16"]}}}),
    );
    lsp.notify("initialized", json!({}));
    lsp.notify(
        "textDocument/didOpen",
        json!({"textDocument": {"uri": file, "languageId": "lotml", "version": 1, "text": text}}),
    );

    let whole = json!({"start": {"line": 0, "character": 0}, "end": {"line": 2, "character": 0}});
    let (actions, _) = lsp.request(
        2,
        "textDocument/codeAction",
        json!({"textDocument": {"uri": file}, "range": whole, "context": {"diagnostics": []}}),
    );
    let action = &actions["result"][0];
    assert_eq!(action["kind"], "quickfix");
    assert_eq!(action["isPreferred"], true, "a machine-applicable fix");
    assert_eq!(action["edit"]["changes"][file.as_str()][0]["newText"], "True");

    let messy = "fn f()->int:\n    return 1+2\n";
    lsp.notify(
        "textDocument/didChange",
        json!({"textDocument": {"uri": file, "version": 2}, "contentChanges": [{"text": messy}]}),
    );
    let (edits, _) = lsp.request(
        3,
        "textDocument/formatting",
        json!({"textDocument": {"uri": file}, "options": {"tabSize": 4, "insertSpaces": true}}),
    );
    assert_eq!(edits["result"][0]["newText"], "fn f() -> int:\n    return 1 + 2\n");
}

#[test]
fn the_language_server_refuses_requests_before_initialize() {
    let mut lsp = Client::start(&["lsp"], true);
    let (early, _) = lsp.request(1, "textDocument/hover", json!({}));
    assert_eq!(early["error"]["code"], -32002);
    lsp.notify("exit", Value::Null);
    assert_eq!(lsp.child.wait().unwrap().code(), Some(1), "an exit without shutdown");
}

#[test]
fn the_language_server_binds_a_python_module_on_import_and_on_an_edit_that_imports_one() {
    let dir = scratch("lsp-bind-on-import", &[(".git", "")]);
    let file = uri(&dir.join("main.lotml"));
    let text = "from py.textwrap import dedent\n\nfn f() -> str ! PyError:\n    return dedent(\"  x\")?\n";
    let mut lsp = Client::start(&["lsp"], true);
    lsp.request(1, "initialize", json!({"rootUri": uri(&dir), "capabilities": {}}));
    lsp.notify("initialized", json!({}));
    lsp.notify(
        "textDocument/didOpen",
        json!({"textDocument": {"uri": file, "languageId": "lotml", "version": 1, "text": text}}),
    );
    let edited = format!("{text}\nfn g() -> str ! PyError:\n    return py.shlex.quote(\"a b\")?\n").replacen(
        "from py.textwrap import dedent\n",
        "from py.textwrap import dedent\nimport py.shlex\n",
        1,
    );
    lsp.notify(
        "textDocument/didChange",
        json!({"textDocument": {"uri": file, "version": 2}, "contentChanges": [{"text": edited}]}),
    );
    let (_, published) = lsp.request(2, "textDocument/documentSymbol", json!({"textDocument": {"uri": file}}));
    let reports: Vec<&Value> = published.iter().filter(|m| m["method"] == "textDocument/publishDiagnostics").collect();
    assert!(reports.len() >= 2, "{published:?}");
    for report in reports {
        assert_eq!(report["params"]["diagnostics"], json!([]), "{report}");
    }
    assert!(!dir.join("bindings").exists());
}

#[test]
fn the_mcp_server_binds_a_python_module_on_import() {
    let text = "from py.textwrap import dedent\n\nfn f() -> str ! PyError:\n    return dedent(\"  x\")?\n";
    let dir = scratch("mcp-bind-on-import", &[(".git", ""), ("main.lotml", text)]);
    let mut mcp = Client::start(&["mcp", "--root", dir.to_str().unwrap()], false);
    mcp.request(
        1,
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}),
    );
    mcp.notify("notifications/initialized", json!({}));
    let (said, failed) = call(&mut mcp, 2, "check", json!({}));
    assert!(!failed && !said.contains("E0"), "{said}");
}

fn call(mcp: &mut Client, id: u64, tool: &str, arguments: Value) -> (String, bool) {
    let (reply, _) = mcp.request(id, "tools/call", json!({"name": tool, "arguments": arguments}));
    let result = &reply["result"];
    (result["content"][0]["text"].as_str().unwrap_or_default().to_string(), result["isError"] == true)
}

#[test]
fn the_mcp_server_serves_the_compilers_tools() {
    let dir = scratch("mcp", &[("shapes.lotml", SHAPES)]);
    let mut mcp = Client::start(&["mcp", "--root", dir.to_str().unwrap()], false);

    let (init, _) = mcp.request(
        1,
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}),
    );
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "lotml");
    mcp.notify("notifications/initialized", json!({}));

    let (list, _) = mcp.request(2, "tools/list", json!({}));
    let names: Vec<&str> =
        list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        vec![
            "check",
            "digest",
            "show",
            "references",
            "definition",
            "hover",
            "explain",
            "test",
            "replace",
            "add",
            "remove",
            "edit",
            "rename"
        ]
    );

    let (clean, failed) = call(&mut mcp, 3, "check", json!({}));
    assert!(!failed);
    assert_eq!(serde_json::from_str::<Value>(&clean).unwrap()["summary"]["clean"], true);

    // Each reference carries the two lines around it, its own marked.
    let (refs, _) = call(&mut mcp, 4, "references", json!({"symbol": "area"}));
    assert!(refs.starts_with("3 references\n"), "{refs}");
    assert!(refs.contains("shapes.lotml:3:4  declaration\n  1 | type Shape = Circle(r: f64) | Empty\n  2 |\n> 3 | fn area(s: Shape) -> f64:\n  4 |     match s:\n  5 |         case Circle(r):\n"), "{refs}");
    let (by_position, _) = call(&mut mcp, 5, "references", json!({"path": "shapes.lotml", "line": 11, "column": 23}));
    assert_eq!(by_position, refs);

    let (hover, _) = call(&mut mcp, 6, "hover", json!({"path": "shapes.lotml", "line": 6, "column": 26}));
    assert_eq!(hover, "r: f64");

    // A change on the disk is seen by the next call.
    std::fs::write(dir.join("shapes.lotml"), SHAPES.replace("return 0.0", "return zero")).unwrap();
    let (broken, _) = call(&mut mcp, 7, "check", json!({"paths": ["shapes.lotml"]}));
    assert_eq!(serde_json::from_str::<Value>(&broken).unwrap()["diagnostics"][0]["code"], "E0201");

    let (explained, _) = call(&mut mcp, 8, "explain", json!({"code": "E0201"}));
    assert!(explained.starts_with("E0201: "));

    let (outside, failed) = call(&mut mcp, 9, "check", json!({"paths": ["../mcp"]}));
    assert!(failed && outside.contains("leaves the project"), "{outside}");
    let (missing, failed) = call(&mut mcp, 10, "references", json!({"symbol": "nothing"}));
    assert!(failed && missing.contains("declares `nothing`"));

    let (unknown, _) = mcp.request(11, "tools/call", json!({"name": "rm", "arguments": {}}));
    assert_eq!(unknown["error"]["code"], -32602);
}

#[test]
fn the_mcp_server_speaks_the_stateless_protocol_too() {
    let dir = scratch("mcp-modern", &[("shapes.lotml", SHAPES)]);
    let mut mcp = Client::start(&["mcp", "--root", dir.to_str().unwrap()], false);
    let meta = json!({"io.modelcontextprotocol/protocolVersion": "2026-07-28", "io.modelcontextprotocol/clientCapabilities": {}});

    let (discover, _) = mcp.request(1, "server/discover", json!({"_meta": meta}));
    assert_eq!(discover["result"]["resultType"], "complete");
    assert!(discover["result"]["supportedVersions"].as_array().unwrap().contains(&json!("2026-07-28")));

    let (digest, _) = mcp.request(2, "tools/call", json!({"_meta": meta, "name": "digest", "arguments": {}}));
    assert_eq!(digest["result"]["resultType"], "complete");
    assert!(digest["result"]["content"][0]["text"].as_str().unwrap().contains("fn twice(s: Shape) -> f64"));

    let (old, _) =
        mcp.request(3, "tools/list", json!({"_meta": {"io.modelcontextprotocol/protocolVersion": "1900-01-01"}}));
    assert_eq!(old["error"]["code"], -32022);
    assert_eq!(old["error"]["data"]["requested"], "1900-01-01");

    let (bare, _) =
        mcp.request(4, "tools/list", json!({"_meta": {"io.modelcontextprotocol/protocolVersion": "2026-07-28"}}));
    assert_eq!(bare["error"]["code"], -32602, "a modern request names its client's capabilities");
}

#[test]
fn the_mcp_server_edits_by_symbol_and_reports_what_an_edit_introduced() {
    let dir = scratch("mcp-edit", &[("shapes.lotml", SHAPES)]);
    let file = dir.join("shapes.lotml");
    let mut mcp = Client::start(&["mcp", "--root", dir.to_str().unwrap()], false);
    mcp.request(1, "initialize", json!({"protocolVersion": "2025-11-25", "capabilities": {}}));

    let (replaced, failed) =
        call(&mut mcp, 2, "replace", json!({"symbol": "twice", "part": "body", "text": "return 2.0 * area(s)"}));
    assert!(!failed, "{replaced}");
    assert_eq!(replaced, "replaced the body of `twice` in shapes.lotml, line 11\nno errors introduced");
    assert!(
        std::fs::read_to_string(&file).unwrap().ends_with("fn twice(s: Shape) -> f64:\n    return 2.0 * area(s)\n")
    );

    let (broken, failed) =
        call(&mut mcp, 3, "replace", json!({"symbol": "twice", "part": "body", "text": "return (2.0"}));
    assert!(failed && broken.contains("breaks the syntax"), "{broken}");
    assert!(std::fs::read_to_string(&file).unwrap().contains("return 2.0 * area(s)"), "a refused edit writes nothing");

    let (arm, _) = call(
        &mut mcp,
        4,
        "replace",
        json!({"symbol": "area", "part": "arm", "arm": "Empty", "text": "case Empty:\n    return \"none\""}),
    );
    assert!(arm.contains("the edit introduced:") && arm.contains("E0204"), "{arm}");

    let (added, failed) = call(
        &mut mcp,
        5,
        "add",
        json!({"after": "area", "text": "fn half(s: Shape) -> f64:\n    return area(s) / 2.0"}),
    );
    assert!(!failed && added.starts_with("added a declaration after `area` in shapes.lotml, lines"), "{added}");

    let (renamed, failed) = call(&mut mcp, 6, "rename", json!({"symbol": "area", "new_name": "surface"}));
    assert!(!failed, "{renamed}");
    assert!(renamed.starts_with("renamed `area` to `surface` at 3 places in shapes.lotml\n"), "{renamed}");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(!text.contains("area(") && text.contains("fn surface(s: Shape)"));

    let (edited, failed) = call(
        &mut mcp,
        7,
        "edit",
        json!({"path": "shapes.lotml", "search": "    return 2.0 * surface(s)\n", "replace": "    return surface(s) + surface(s)\n"}),
    );
    assert!(!failed, "{edited}");

    let (removed, _) = call(&mut mcp, 8, "remove", json!({"symbol": "half"}));
    assert!(removed.starts_with("removed `half` in shapes.lotml"), "{removed}");
    let (removed, _) = call(&mut mcp, 9, "remove", json!({"symbol": "surface"}));
    assert!(removed.contains("the edit introduced:") && removed.contains("E0201"), "callers lose it: {removed}");
    assert!(!dir.join("shapes.lotml.partial").exists());
}

#[test]
fn the_mcp_server_serves_and_edits_a_lot_file_as_a_lotml_one() {
    let dir = scratch("mcp-lot", &[("shapes.lot", SHAPES)]);
    let file = dir.join("shapes.lot");
    let mut mcp = Client::start(&["mcp", "--root", dir.to_str().unwrap()], false);
    mcp.request(1, "initialize", json!({"protocolVersion": "2025-11-25", "capabilities": {}}));

    let (replaced, failed) =
        call(&mut mcp, 2, "replace", json!({"symbol": "twice", "part": "body", "text": "return 2.0 * area(s)"}));
    assert!(!failed, "{replaced}");
    assert_eq!(replaced, "replaced the body of `twice` in shapes.lot, line 11\nno errors introduced");
    assert!(std::fs::read_to_string(&file).unwrap().ends_with("    return 2.0 * area(s)\n"));
    assert!(!dir.join("shapes.lot.partial").exists() && !dir.join("shapes.lotml.partial").exists());

    let (missing, failed) = call(&mut mcp, 3, "check", json!({"paths": ["other.lot"]}));
    assert!(failed, "{missing}");
    assert_eq!(missing, "no .lot or .lotml file at `other.lot`");
}

#[test]
fn an_edit_never_writes_through_a_link_planted_at_its_partial_name() {
    let dir = scratch("mcp-planted", &[("shapes.lot", SHAPES)]);
    let victim = Path::new(env!("CARGO_TARGET_TMPDIR")).join("mcp-planted-victim.txt");
    std::fs::write(&victim, "untouched").unwrap();
    #[cfg(unix)]
    let planted = std::os::unix::fs::symlink(&victim, dir.join("shapes.lot.partial"));
    #[cfg(windows)]
    let planted = std::os::windows::fs::symlink_file(&victim, dir.join("shapes.lot.partial"));
    if planted.is_err() {
        return; // this machine may not create links without privileges
    }
    let mut mcp = Client::start(&["mcp", "--root", dir.to_str().unwrap()], false);
    mcp.request(1, "initialize", json!({"protocolVersion": "2025-11-25", "capabilities": {}}));
    let (replaced, failed) =
        call(&mut mcp, 2, "replace", json!({"symbol": "twice", "part": "body", "text": "return 2.0 * area(s)"}));
    assert!(!failed, "{replaced}");
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "untouched");
    assert!(std::fs::read_to_string(dir.join("shapes.lot")).unwrap().ends_with("    return 2.0 * area(s)\n"));
}

#[test]
fn the_language_server_renames_every_reference() {
    let dir = scratch("lsp-rename", &[("shapes.lotml", SHAPES)]);
    let file = uri(&dir.join("shapes.lotml"));
    let mut lsp = Client::start(&["lsp"], true);
    lsp.request(1, "initialize", json!({"rootUri": uri(&dir), "capabilities": {}}));
    let at = json!({"textDocument": {"uri": file}, "position": position(SHAPES, "area", 1)});

    let (prepared, _) = lsp.request(2, "textDocument/prepareRename", at.clone());
    assert_eq!(prepared["result"]["placeholder"], "area");

    let mut rename = at.clone();
    rename["newName"] = json!("surface");
    let (renamed, _) = lsp.request(3, "textDocument/rename", rename);
    assert_eq!(renamed["result"]["changes"][file.as_str()].as_array().unwrap().len(), 3);

    let mut keyword = at;
    keyword["newName"] = json!("match");
    let (refused, _) = lsp.request(4, "textDocument/rename", keyword);
    assert_eq!(refused["error"]["code"], -32803);
}

#[test]
fn the_mcp_test_tool_never_downloads_a_python() {
    let dir = scratch(
        "mcp-offline",
        &[
            ("pyproject.toml", ""),
            (
                "t.lotml",
                "test \"t\":
    assert 1 == 1
",
            ),
            ("uv.exe", "not a program"),
        ],
    );
    let uv = dir.join("uv.exe");
    let mut mcp = Client::start_with(&["mcp", "--root", dir.to_str().unwrap()], false, |command| {
        command.env("LOTML_UV", &uv).env_remove("LOTML_PYTHON").env_remove("LOTML_OFFLINE").env_remove("VIRTUAL_ENV");
    });
    mcp.request(
        1,
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}),
    );
    mcp.notify("notifications/initialized", json!({}));
    let (said, failed) = call(&mut mcp, 2, "test", json!({"paths": ["t.lotml"]}));
    assert!(failed, "no Python, no tests: {said}");
    assert!(said.contains("downloads nothing"), "the download step is never reached, offline or not: {said}");
}
