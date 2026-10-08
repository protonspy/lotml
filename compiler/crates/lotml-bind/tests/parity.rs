//! The Rust binder writes the interface the Python binder wrote, save R1.3's escapes
//! (specs/rust-binder R1.2, task 2.3): every module of the vendored typeshed `stdlib`, and every
//! PyPI module of the binding coverage corpus whose stub the harness environment holds, bound by
//! both. The Python binder runs as the oracle while it exists; it needs `LOTML_PYTHON`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn manifest() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Each module of the vendored `stdlib`, with its stub.
fn standard_library() -> Vec<(String, PathBuf)> {
    fn walk(dir: &Path, root: &Path, found: &mut Vec<(String, PathBuf)>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, found);
            } else if path.extension().is_some_and(|e| e == "pyi") {
                let relative = path.strip_prefix(root).unwrap().with_extension("");
                let mut parts: Vec<String> =
                    relative.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
                if parts.last().is_some_and(|p| p == "__init__") {
                    parts.pop();
                }
                found.push((parts.join("."), path));
            }
        }
    }
    let root = manifest().join("typeshed").join("stdlib");
    let mut found = Vec::new();
    walk(&root, &root, &mut found);
    found.sort();
    found
}

/// Each PyPI module of the coverage corpus, with the stub the harness environment holds for it.
fn corpus_stubs() -> Vec<(String, PathBuf)> {
    let repository = manifest().join("../../..");
    let corpus: Value =
        serde_json::from_str(&std::fs::read_to_string(repository.join("harness/binding-corpus.json")).unwrap())
            .unwrap();
    let roots = lotml_py::sources::site_packages(&repository.join("harness").join(".venv"));
    corpus["pypi"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| {
            let module = entry["module"].as_str()?;
            let source = lotml_py::sources::find(module, &roots).ok()??;
            Some((module.to_string(), source.path))
        })
        .collect()
}

/// What the Python binder writes for each `(module, stub)`, by one run of it: the interface, or
/// `None` where it could not read the stub.
fn oracle(python: &str, stubs: &[(String, PathBuf)]) -> Vec<Option<String>> {
    let binder = manifest().join("../lotml-py/runtime/lotml_bind.py");
    let script = format!(
        "import importlib.util, json, sys\n\
         spec = importlib.util.spec_from_file_location('oracle', {binder:?})\n\
         b = importlib.util.module_from_spec(spec); spec.loader.exec_module(b)\n\
         out = []\n\
         for module, path in json.loads(sys.stdin.read()):\n\
         \x20   try:\n\
         \x20       out.append(b.interface(module, b.Path(path), 'x'))\n\
         \x20   except Exception:\n\
         \x20       out.append(None)\n\
         sys.stdout.write(json.dumps(out))\n",
        binder = binder.canonicalize().unwrap().display().to_string()
    );
    let mut child = Command::new(python)
        .args(["-c", &script])
        .env("PYTHONIOENCODING", "utf-8")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("the oracle runs");
    let listed: Vec<Value> = stubs.iter().map(|(m, p)| json!([m, p.display().to_string()])).collect();
    child.stdin.take().unwrap().write_all(Value::Array(listed).to_string().as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "the oracle failed");
    serde_json::from_slice::<Vec<Option<String>>>(&output.stdout).expect("the oracle's JSON")
}

/// A line of the Python binder's interface with R1.3's escapes applied: the control characters it
/// left raw in a string default, written as the Rust binder writes them.
fn escaped(line: &str) -> String {
    line.chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') {
                lotml_bind::binder::escape(&c.to_string())
            } else {
                c.to_string()
            }
        })
        .collect()
}

#[test]
fn the_rust_binder_writes_what_the_python_binder_wrote() {
    let Some(python) = std::env::var("LOTML_PYTHON").ok().filter(|p| !p.is_empty()) else {
        eprintln!("LOTML_PYTHON is not set: the oracle cannot run");
        return;
    };
    let mut stubs = standard_library();
    let pypi = corpus_stubs();
    eprintln!("{} standard-library modules, {} PyPI modules", stubs.len(), pypi.len());
    stubs.extend(pypi);
    let expected = oracle(&python, &stubs);
    let mut differences = Vec::new();
    for ((module, path), expected) in stubs.iter().zip(expected) {
        let text = std::fs::read_to_string(path).unwrap();
        let found = lotml_bind::binder::interface(module, &text, "x").ok();
        match (expected, found) {
            (None, None) => {}
            (Some(expected), Some(found)) => {
                let expected: Vec<String> = expected.split('\n').map(escaped).collect();
                let found: Vec<&str> = found.split('\n').collect();
                if expected != found {
                    let first = expected.iter().zip(&found).position(|(e, f)| e != f).unwrap_or(0);
                    differences.push(format!(
                        "{module}: python {:?}\n{}rust   {:?}",
                        expected.get(first),
                        " ".repeat(module.len() + 2),
                        found.get(first)
                    ));
                }
            }
            (expected, found) => differences.push(format!(
                "{module}: python {}, rust {}",
                if expected.is_some() { "bound it" } else { "refused it" },
                if found.is_some() { "bound it" } else { "refused it" }
            )),
        }
    }
    assert!(
        differences.is_empty(),
        "{} of {} modules differ:\n{}",
        differences.len(),
        stubs.len(),
        differences.join("\n")
    );
}
