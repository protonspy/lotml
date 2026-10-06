//! The harness guide's tool as a harness meets it (specs/guide-tool/): listed and named only when
//! this machine has a usable configuration, and what `lotml guide render` writes.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};

const CONFIG: &str = "\
url = \"http://127.0.0.1:8088\"
model = \"lotml-guide\"
threshold = 0.5
answer = 512
run_candidates = false
renderer = 1
records = \"records\"
";

fn scratch(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("guide-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (file, text) in files {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

/// `lotml` with only `vars` among the variables a guide is found through, so this machine's own
/// configuration never reaches the test.
fn lotml(args: &[&str], dir: &Path, vars: &[(&str, &Path)]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lotml"));
    command.args(args).current_dir(dir);
    for name in ["LOTML_HARNESS_GUIDE", "XDG_CONFIG_HOME", "HOME", "APPDATA", "USERPROFILE"] {
        command.env_remove(name);
    }
    for (name, value) in vars {
        command.env(name, value);
    }
    command
}

/// The server's answers to `initialize` and `tools/list`, and what it said on standard error.
fn serve(name: &str, vars: &[(&str, &Path)]) -> (Value, Value, String) {
    let project = scratch(&format!("project-{name}"), &[("a.lotml", "fn f() -> int:\n    return 1\n")]);
    let mut child = lotml(&["mcp", "--root", "."], &project, vars)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let initialize = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}});
    let list = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}});
    writeln!(input, "{initialize}\n{list}").unwrap();
    drop(input);
    let Output { stdout, stderr, .. } = child.wait_with_output().unwrap();
    let replies: Vec<Value> =
        String::from_utf8_lossy(&stdout).lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    (replies[0].clone(), replies[1].clone(), String::from_utf8_lossy(&stderr).into_owned())
}

fn names(list: &Value) -> Vec<String> {
    list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect()
}

#[test]
fn a_valid_configuration_lists_and_names_the_guide_tool() {
    let dir = scratch("valid", &[("guide.toml", CONFIG)]);
    let (init, list, _) = serve("valid", &[("LOTML_HARNESS_GUIDE", &dir.join("guide.toml"))]);
    assert!(names(&list).contains(&"guide".to_string()));
    assert!(init["result"]["instructions"].as_str().unwrap().contains("`guide`"));
    let tool = list["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == "guide").unwrap();
    assert_eq!(tool["inputSchema"]["properties"]["task"]["maxLength"], 2000);
}

#[test]
fn the_user_s_lotml_directory_is_found_without_the_variable() {
    let dir = scratch("xdg", &[("lotml/harness-guide.toml", CONFIG)]);
    let (_, list, _) = serve("xdg", &[("XDG_CONFIG_HOME", &dir)]);
    assert!(names(&list).contains(&"guide".to_string()));
}

#[test]
fn no_configuration_lists_no_guide_and_every_other_tool_as_before() {
    let empty = scratch("none", &[]);
    let (init, list, stderr) = serve("none", &[("XDG_CONFIG_HOME", &empty)]);
    assert!(!names(&list).contains(&"guide".to_string()));
    assert!(names(&list).contains(&"check".to_string()));
    assert!(!init["result"]["instructions"].as_str().unwrap().contains("`guide`"));
    assert!(stderr.is_empty(), "{stderr}");
}

#[test]
fn an_unusable_configuration_lists_no_guide_and_says_why() {
    let dir = scratch("unknown", &[("guide.toml", &format!("{CONFIG}temperature = 0.2\n"))]);
    let (_, list, stderr) = serve("unknown", &[("LOTML_HARNESS_GUIDE", &dir.join("guide.toml"))]);
    assert!(!names(&list).contains(&"guide".to_string()));
    assert!(stderr.contains("temperature"), "{stderr}");
    let remote = scratch("remote", &[("guide.toml", &CONFIG.replace("127.0.0.1", "10.0.0.1"))]);
    let (_, list, stderr) = serve("remote", &[("LOTML_HARNESS_GUIDE", &remote.join("guide.toml"))]);
    assert!(!names(&list).contains(&"guide".to_string()));
    assert!(stderr.contains("10.0.0.1"), "{stderr}");
    let (_, list, stderr) = serve("relative", &[("LOTML_HARNESS_GUIDE", Path::new("guide.toml"))]);
    assert!(!names(&list).contains(&"guide".to_string()));
    assert!(stderr.contains("absolute"), "{stderr}");
}

#[test]
fn guide_render_prints_the_messages_for_a_state_given_as_json() {
    let state = json!({
        "task": "Sum it.",
        "path": "a.lotml",
        "text": "fn f() -> int:\n    return x\n",
        "diagnostics": [{"code": "E0201", "severity": "error", "message": "`x` is not defined", "location": {"line": 2, "column": 12}}],
        "failing": null
    });
    let dir = scratch("render", &[("state.json", &state.to_string())]);
    let out = lotml(&["guide", "render", "state.json"], &dir, &[]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let rendered: Value = serde_json::from_slice(&out.stdout).unwrap();
    let messages = rendered["messages"].as_array().unwrap();
    assert_eq!((messages[0]["role"].as_str(), messages[1]["role"].as_str()), (Some("system"), Some("user")));
    let user = messages[1]["content"].as_str().unwrap();
    assert!(user.starts_with("Task: Sum it.\n\nFile a.lotml:\n  1 | fn f() -> int:\n  2 |     return x\n"), "{user}");
    assert!(user.ends_with("error E0201 at 2:12: `x` is not defined\n"), "{user}");
    let both =
        scratch("render-both", &[("state.json", &json!({"path": "a", "text": "", "failing": null}).to_string())]);
    let refused = lotml(&["guide", "render", "state.json"], &both, &[]).output().unwrap();
    assert_eq!(refused.status.code(), Some(2));
}

const FAILING: &str = "fn add(a: int, b: int) -> int:\n    return a - b\n\ntest \"adds\":\n    assert add(1, 2) == 3\n";

/// A guide server answering every request with `answer`, its one token at `logprob`.
fn guide_server(answer: Value, logprob: f64) -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            let mut length = 0;
            loop {
                let mut line = String::new();
                std::io::BufRead::read_line(&mut reader, &mut line).unwrap();
                if let Some(n) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = n.trim().parse().unwrap();
                }
                if line == "\r\n" {
                    break;
                }
            }
            let mut body = vec![0; length];
            std::io::Read::read_exact(&mut reader, &mut body).unwrap();
            let content = answer.to_string();
            let response = json!({"choices": [{"message": {"role": "assistant", "content": content},
                "logprobs": {"content": [{"token": content, "logprob": logprob}]}}]})
            .to_string();
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{response}", response.len());
        }
    });
    port
}

fn fixing_answer(path: &str) -> Value {
    json!({"locations": [{"path": path, "symbol": "add", "lines": [1, 2]}], "kind": "body",
        "edit": {"tool": "replace", "arguments": {"symbol": "add", "part": "body", "text": "return a + b", "path": path}}})
}

/// `lotml guide ask` over a project holding `FAILING`, with a guide that answers `answer`.
fn ask(name: &str, answer: Value, logprob: f64, run_candidates: bool) -> (Value, String) {
    let port = guide_server(answer, logprob);
    let config = CONFIG
        .replace("8088", &port.to_string())
        .replace("run_candidates = false", &format!("run_candidates = {run_candidates}"));
    let dir = scratch(name, &[("guide.toml", &config), ("project/a.lotml", FAILING)]);
    let out = lotml(
        &["guide", "ask", "--task", "Add two numbers."],
        &dir.join("project"),
        &[("LOTML_HARNESS_GUIDE", &dir.join("guide.toml"))],
    )
    .output()
    .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let file = std::fs::read_to_string(dir.join("project/a.lotml")).unwrap();
    (serde_json::from_slice(&out.stdout).unwrap(), file)
}

#[test]
fn an_edit_that_makes_the_failing_block_pass_in_a_private_copy_is_shown_and_the_project_is_untouched() {
    let (answer, file) = ask("shown", fixing_answer("a.lotml"), -0.01, true);
    assert_eq!(answer["locations"], json!([{"path": "a.lotml", "symbol": "add", "lines": [1, 2]}]));
    assert_eq!(answer["kind"], "body");
    assert_eq!(answer["edit"]["tool"], "replace");
    assert_eq!(answer["edit"]["arguments"]["text"], "return a + b");
    assert_eq!(answer["withheld"], Value::Null);
    assert!(answer["confidence"].as_f64().unwrap() > 0.98);
    assert_eq!(file, FAILING, "the guide never writes the project");
}

#[test]
fn where_candidates_may_not_run_the_edit_for_a_failing_block_is_withheld() {
    let (answer, _) = ask("not-run", fixing_answer("a.lotml"), -0.01, false);
    assert_eq!(answer["edit"], Value::Null);
    assert_eq!(answer["withheld"], "not-run");
    assert_eq!(answer["locations"][0]["symbol"], "add", "the locations still stand");
}

#[test]
fn an_edit_that_does_not_make_the_block_pass_is_withheld() {
    let mut answer = fixing_answer("a.lotml");
    answer["edit"]["arguments"]["text"] = json!("return a * b");
    let (answer, _) = ask("fails-test", answer, -0.01, true);
    assert_eq!(answer["withheld"], "edit-fails-test");
}

#[test]
fn a_location_outside_the_project_or_an_unconfident_answer_is_a_silence() {
    let (answer, _) = ask("outside", fixing_answer("../a.lotml"), -0.01, true);
    assert_eq!(answer, json!({"guidance": null, "reason": "no-location"}));
    let (answer, _) = ask("unsure", fixing_answer("a.lotml"), -3.0, true);
    assert_eq!(answer, json!({"guidance": null, "reason": "not-confident"}));
}

#[test]
fn a_green_project_has_nothing_to_guide_and_an_absent_server_is_an_error() {
    let dir = scratch("green", &[("guide.toml", CONFIG), ("project/a.lotml", "fn f() -> int:\n    return 1\n")]);
    let guide = [("LOTML_HARNESS_GUIDE", dir.join("guide.toml"))];
    let vars: Vec<(&str, &Path)> = guide.iter().map(|(k, v)| (*k, v.as_path())).collect();
    let out = lotml(&["guide", "ask"], &dir.join("project"), &vars).output().unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
        json!({"guidance": null, "reason": "nothing-to-guide"})
    );
    std::fs::write(dir.join("project/a.lotml"), FAILING).unwrap();
    let out = lotml(&["guide", "ask"], &dir.join("project"), &vars).output().unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
        json!({"guidance": null, "reason": "server-error"})
    );
    let unconfigured =
        lotml(&["guide", "ask"], &dir.join("project"), &[("XDG_CONFIG_HOME", &dir.join("none"))]).output().unwrap();
    assert_eq!(unconfigured.status.code(), Some(1));
}

#[test]
fn the_mcp_tool_answers_the_same_call() {
    let port = guide_server(fixing_answer("a.lotml"), -0.01);
    let config = CONFIG.replace("8088", &port.to_string()).replace("run_candidates = false", "run_candidates = true");
    let dir = scratch("tool", &[("guide.toml", &config), ("project/a.lotml", FAILING)]);
    let mut child =
        lotml(&["mcp", "--root", "."], &dir.join("project"), &[("LOTML_HARNESS_GUIDE", &dir.join("guide.toml"))])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
    let mut input = child.stdin.take().unwrap();
    let initialize = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}});
    let call = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "guide", "arguments": {"task": "Add."}}});
    writeln!(input, "{initialize}\n{call}").unwrap();
    drop(input);
    let out = child.wait_with_output().unwrap();
    let replies: Vec<Value> =
        String::from_utf8_lossy(&out.stdout).lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let result = &replies[1]["result"];
    assert_eq!(result["isError"], false);
    let answer: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(answer["edit"]["tool"], "replace");
    assert_eq!(answer["withheld"], Value::Null);
}
