//! `lotml build`, `lotml run` and `lotml test`: programs compiled to Python and run by it.

use std::path::{Path, PathBuf};
use std::process::Command;

use lotml_check::Interfaces;
use lotml_diag::Report;
use serde_json::{Value, json};

use crate::{Failure, files};

/// A compiled module: the name it is imported by, its source's absolute path, which tracebacks
/// name, and the path as the user wrote it.
struct Module {
    name: String,
    source: PathBuf,
    shown: String,
}

/// The Python modules compiled from `paths`, written into `dir` with the runtime, each with the
/// `.pyi` that types it for Python. When a file does not compile, none is written and the
/// diagnostics come back as text.
fn compile(paths: &[PathBuf], dir: &Path) -> Result<Result<Vec<Module>, String>, Failure> {
    let mut compiled = Vec::new();
    let mut reports = Vec::new();
    let mut texts = Vec::new();
    for path in files::expand(paths)? {
        let text = files::read(&path)?;
        let absolute = std::path::absolute(&path).map_err(|e| Failure(format!("{}: {e}", path.display())))?;
        let interfaces: Interfaces = files::interfaces_for(&path)
            .into_iter()
            .map(|b| {
                let read = lotml_check::interface_of(&b.module, &b.text).0;
                (b.module, read)
            })
            .collect();
        let result = lotml_py::compile_with(&text, &absolute, &interfaces);
        texts.push((path.display().to_string(), text, result, absolute));
    }
    for (name, text, result, absolute) in &texts {
        match result {
            Ok(module) => compiled.push((module, absolute.clone(), name.clone())),
            Err(diagnostics) => reports.push(Report { file: name, text, diagnostics: diagnostics.clone() }),
        }
    }
    if !reports.is_empty() {
        return Ok(Err(lotml_diag::text(&reports, Some(lotml_diag::DEFAULT_LIMIT))));
    }
    std::fs::create_dir_all(dir).map_err(|e| Failure(format!("cannot create {}: {e}", dir.display())))?;
    write(&dir.join("lotml_rt.py"), lotml_py::RUNTIME)?;
    let mut modules = Vec::new();
    for (module, source, shown) in compiled {
        let stem = source
            .file_stem()
            .map_or("program".into(), |s| s.to_string_lossy().replace(|c: char| !c.is_alphanumeric(), "_"));
        let name = format!("{stem}_lotml");
        write(&dir.join(format!("{name}.py")), &module.module)?;
        write(&dir.join(format!("{name}.pyi")), &lotml_py::stub(&shown, &name, &module.checked))?;
        modules.push(Module { name, source, shown });
    }
    Ok(Ok(modules))
}

/// The compiled modules, or the diagnostics printed and `None` when a file does not compile.
fn compile_or_report(paths: &[PathBuf], dir: &Path) -> Result<Option<Vec<Module>>, Failure> {
    match compile(paths, dir)? {
        Ok(modules) => Ok(Some(modules)),
        Err(report) => {
            print!("{report}");
            Ok(None)
        }
    }
}

fn write(path: &Path, text: &str) -> Result<(), Failure> {
    std::fs::write(path, text).map_err(|e| Failure(format!("cannot write {}: {e}", path.display())))
}

fn python() -> Result<Vec<String>, Failure> {
    lotml_py::python()
        .ok_or_else(|| Failure("no Python found: install Python 3.11 or later, or set LOTML_PYTHON".into()))
}

/// A directory of its own for one run, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Scratch {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        Scratch(std::env::temp_dir().join(format!("lotml-{}-{nanos}", std::process::id())))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `lotml build`: each file as `<name>_lotml.py`, next to the runtime, in `out`.
pub fn build(paths: &[PathBuf], out: &Path) -> Result<u8, Failure> {
    let Some(modules) = compile_or_report(paths, out)? else { return Ok(1) };
    for module in modules {
        println!("{} -> {}", module.shown, out.join(format!("{}.py", module.name)).display());
    }
    Ok(0)
}

/// `lotml run`: the program's `fn main()`, with Python's exit status: 1 when it returned an
/// error, 101 when it panicked.
pub fn run(path: &Path) -> Result<u8, Failure> {
    let scratch = Scratch::new();
    let Some(modules) = compile_or_report(&[path.to_path_buf()], &scratch.0)? else { return Ok(1) };
    let name = &modules[0].name;
    let python = python()?;
    let script = format!(
        "import sys\nsys.path.insert(0, {dir})\nimport lotml_rt\nsys.exit(lotml_rt.main({name}))",
        dir = Value::String(scratch.0.display().to_string()),
        name = Value::String(name.clone()),
    );
    let status = Command::new(&python[0])
        .args(&python[1..])
        .arg("-c")
        .arg(script)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .status()
        .map_err(|e| Failure(format!("cannot run Python: {e}")))?;
    Ok(status.code().map_or(101, |c| u8::try_from(c).unwrap_or(1)))
}

/// `lotml test`: every `test` block, with the values a failed comparison saw.
pub fn test(paths: &[PathBuf], as_json: bool) -> Result<u8, Failure> {
    let (status, report) = test_report(paths, as_json)?;
    print!("{report}");
    Ok(status)
}

/// What `lotml test` prints, and its exit status.
pub fn test_report(paths: &[PathBuf], as_json: bool) -> Result<(u8, String), Failure> {
    let scratch = Scratch::new();
    let modules = match compile(paths, &scratch.0)? {
        Ok(modules) => modules,
        Err(report) => return Ok((1, report)),
    };
    let python = python()?;
    let listed: Vec<Value> = modules.iter().map(|m| json!([m.name, m.source.display().to_string()])).collect();
    let script = format!(
        "import importlib, json, sys\nsys.path.insert(0, {dir})\nimport lotml_rt\nout = []\nfor name, path in {modules}:\n    try:\n        module = importlib.import_module(name)\n        out.append({{'file': path, 'tests': lotml_rt.run_tests(vars(module), path)}})\n    except Exception as error:\n        out.append({{'file': path, 'load': type(error).__name__ + ': ' + str(error)}})\nsys.stdout.write(json.dumps(out))",
        dir = Value::String(scratch.0.display().to_string()),
        modules = Value::Array(listed),
    );
    let output = Command::new(&python[0])
        .args(&python[1..])
        .arg("-c")
        .arg(script)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .output()
        .map_err(|e| Failure(format!("cannot run Python: {e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let files: Vec<Value> = serde_json::from_str(stdout.lines().last().unwrap_or("")).map_err(|_| {
        Failure(format!("the tests did not report: {}", String::from_utf8_lossy(&output.stderr).trim()))
    })?;
    let mut rows = Vec::new();
    for (file, module) in files.iter().zip(&modules) {
        let path = module.shown.as_str();
        if let Some(load) = file["load"].as_str() {
            rows.push(json!({"file": path, "name": null, "outcome": "panic", "message": load}));
            continue;
        }
        for t in file["tests"].as_array().into_iter().flatten() {
            let mut row = t.clone();
            row["file"] = path.into();
            rows.push(row);
        }
    }
    let count = |outcome: &str| rows.iter().filter(|r| r["outcome"] == outcome).count();
    let summary =
        json!({"passed": count("pass"), "failed": count("fail"), "errors": count("error"), "panics": count("panic")});
    let report = if as_json {
        format!("{}\n", json!({"version": 1, "tests": rows, "summary": summary}))
    } else {
        format!(
            "{}{} passed, {} failed, {} errors, {} panics\n",
            text(&rows),
            summary["passed"],
            summary["failed"],
            summary["errors"],
            summary["panics"]
        )
    };
    Ok((u8::from(rows.iter().any(|r| r["outcome"] != "pass")), report))
}

/// `lotml bind`: the interface of a Python module, read from its stub by Python's own parser
/// and written to `out/<module>.lotmli` (adr:0012).
pub fn bind(module: &str, stub: Option<&Path>, out: &Path) -> Result<bool, Failure> {
    // The name becomes a file name: identifiers and dots only, so it cannot leave `out`.
    let valid = module.split('.').all(|part| {
        part.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
            && part.chars().all(|c| c.is_alphanumeric() || c == '_')
    });
    if !valid {
        return Err(Failure(format!("`{module}` is not a Python module name")));
    }
    if lotml_check::is_c_library(module) {
        return Err(Failure(format!(
            "`{module}` names a C library, whose interface is written by hand: bindings/{module}.lotmli (adr:0013)"
        )));
    }
    let python = python()?;
    let output = Command::new(&python[0])
        .args(&python[1..])
        .arg("-c")
        .arg(lotml_py::BIND)
        .arg(module)
        .arg(stub.map(|s| s.as_os_str().to_owned()).unwrap_or_default())
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .output()
        .map_err(|e| Failure(format!("cannot run Python: {e}")))?;
    if !output.status.success() {
        return Err(Failure(String::from_utf8_lossy(&output.stderr).trim().to_string()));
    }
    // Python's text-mode stdout ends lines with `\r\n` on Windows; the file is written one way.
    let text =
        String::from_utf8(output.stdout).map_err(|_| Failure("the binding is not UTF-8".into()))?.replace("\r\n", "\n");
    let (interface, problems) = lotml_check::interface(&text);
    if let Some(problem) = problems.first() {
        return Err(Failure(format!("the binding of `{module}` does not check: {} {}", problem.code, problem.message)));
    }
    std::fs::create_dir_all(out).map_err(|e| Failure(format!("cannot create {}: {e}", out.display())))?;
    let path = out.join(format!("{module}.lotmli"));
    write(&path, &text)?;
    let bound = interface.names().count();
    let skipped = text.lines().filter(|l| l.starts_with("#   ")).count();
    println!(
        "{}: {bound} function{} bound{}",
        path.display(),
        if bound == 1 { "" } else { "s" },
        if skipped == 0 { String::new() } else { format!(", {skipped} not (the file's comments say why)") }
    );
    Ok(true)
}

fn text(rows: &[Value]) -> String {
    let mut out = String::new();
    for row in rows {
        let name = row["name"].as_str().unwrap_or("(loading)");
        let at =
            row["line"].as_u64().map(|l| format!("{}:{l}", row["file"].as_str().unwrap_or(""))).unwrap_or_default();
        match row["outcome"].as_str() {
            Some("pass") => out += &format!("ok     {name}\n"),
            Some("fail") => {
                out += &format!("FAIL   {name}  {at}\n       assert {}\n", row["expression"].as_str().unwrap_or(""));
                if let (Some(left), Some(right)) = (row["left"].as_str(), row["right"].as_str()) {
                    out += &format!("       left:  {left}\n       right: {right}\n");
                }
                if let Some(message) = row["message"].as_str() {
                    out += &format!("       message: {message}\n");
                }
            }
            Some("error") => {
                out += &format!("ERROR  {name}  {at}\n       `?` passed on {}\n", row["error"].as_str().unwrap_or(""))
            }
            _ => {
                let kind = row["kind"].as_str().unwrap_or("");
                out += &format!("PANIC  {name}  {at}\n       {kind}: {}\n", row["message"].as_str().unwrap_or(""));
            }
        }
    }
    out
}
