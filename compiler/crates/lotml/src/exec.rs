//! `lotml build`, `lotml run` and `lotml test`: programs compiled to Python and run by it, or
//! compiled to C and built by a C compiler (`--target c`, specs/c-backend).

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use lotml_check::Interfaces;
use lotml_diag::Report;
use serde_json::{Value, json};

use crate::{Failure, Target, files};

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

/// Python, started for one of the compiler's scripts. `-P` keeps the working directory out
/// of the front of `sys.path`, where a `json.py` left there would be imported in place of the
/// library's; a script that runs a program puts the directory back at the end, after the
/// library, so the project's own Python modules are still found.
fn interpreter(python: &[String], script: &str) -> Command {
    let mut command = Command::new(&python[0]);
    command
        .args(&python[1..])
        .args(["-P", "-c", script])
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("PYTHONIOENCODING", "utf-8");
    command
}

/// The lines a script that runs a program starts with: the scratch directory first, for the
/// runtime and the compiled modules, and the working directory last.
fn search_path(scratch: &Path) -> String {
    format!(
        "import os, sys\nsys.path.insert(0, {})\nsys.path.append(os.getcwd())\n",
        Value::String(scratch.display().to_string())
    )
}

/// A directory of its own for one run, removed when dropped. It is created anew — never one
/// that was already there, which another user could have put in place — and on Unix only its
/// owner may enter it.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Scratch, Failure> {
        let base = std::env::temp_dir();
        for attempt in 0..64 {
            let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
            let path = base.join(format!("lotml-{}-{nanos}-{attempt}", std::process::id()));
            let builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            let builder = {
                use std::os::unix::fs::DirBuilderExt;
                let mut owned = builder;
                owned.mode(0o700);
                owned
            };
            match builder.create(&path) {
                Ok(()) => return Ok(Scratch(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(Failure(format!("cannot create a directory in {}: {e}", base.display()))),
            }
        }
        Err(Failure(format!("cannot create a directory of its own in {}", base.display())))
    }
}

/// How long a run of the tests may take and how much of its output is kept: the MCP server
/// runs them for an agent, and a loop in a test must not hold the server or its memory.
pub struct Limits {
    pub seconds: u64,
    pub output: usize,
}

/// A child's exit status and the end of its output, at most `limits.output` bytes of each
/// stream; killed when it runs past `limits.seconds`.
fn wait_limited(mut child: std::process::Child, limits: &Limits) -> Result<std::process::Output, Failure> {
    fn tail(mut stream: impl std::io::Read + Send + 'static, keep: usize) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut kept: Vec<u8> = Vec::new();
            let mut buffer = [0u8; 8192];
            while let Ok(n) = stream.read(&mut buffer) {
                if n == 0 {
                    break;
                }
                kept.extend_from_slice(&buffer[..n]);
                if kept.len() > 2 * keep {
                    kept.drain(..kept.len() - keep);
                }
            }
            if kept.len() > keep {
                kept.drain(..kept.len() - keep);
            }
            kept
        })
    }
    let out = child.stdout.take().map(|s| tail(s, limits.output));
    let err = child.stderr.take().map(|s| tail(s, limits.output));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(limits.seconds);
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| Failure(format!("cannot run Python: {e}")))? {
            break status;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Failure(format!("the tests did not finish within {} s", limits.seconds)));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let joined =
        |handle: Option<std::thread::JoinHandle<Vec<u8>>>| handle.and_then(|h| h.join().ok()).unwrap_or_default();
    Ok(std::process::Output { status, stdout: joined(out), stderr: joined(err) })
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `lotml build`: each file as `<name>_lotml.py`, next to the runtime, in `out`; for the C target,
/// as `<name>.c` and the executable built from it.
pub fn build(paths: &[PathBuf], out: &Path, target: Target) -> Result<u8, Failure> {
    if target == Target::C {
        return build_c(paths, out);
    }
    let Some(modules) = compile_or_report(paths, out)? else { return Ok(1) };
    for module in modules {
        println!("{} -> {}", module.shown, out.join(format!("{}.py", module.name)).display());
    }
    Ok(0)
}

/// `lotml run`: the program's `fn main()`, with Python's exit status: 1 when it returned an
/// error, 101 when it panicked; the C target's program exits as Python's does.
pub fn run(path: &Path, target: Target) -> Result<u8, Failure> {
    let scratch = Scratch::new()?;
    if target == Target::C {
        let Some(exe) = c_executable(path, &scratch.0, false)? else { return Ok(1) };
        let status = Command::new(&exe).status().map_err(|e| Failure(format!("cannot run {}: {e}", exe.display())))?;
        return Ok(status.code().map_or(101, |c| u8::try_from(c).unwrap_or(1)));
    }
    let Some(modules) = compile_or_report(&[path.to_path_buf()], &scratch.0)? else { return Ok(1) };
    let name = &modules[0].name;
    let python = python()?;
    let script = format!(
        "{}import lotml_rt\nsys.exit(lotml_rt.main({name}))",
        search_path(&scratch.0),
        name = Value::String(name.clone()),
    );
    let status = interpreter(&python, &script)
        .env_remove("PYTHONIOENCODING")
        .status()
        .map_err(|e| Failure(format!("cannot run Python: {e}")))?;
    Ok(status.code().map_or(101, |c| u8::try_from(c).unwrap_or(1)))
}

/// `lotml test`: every `test` block, with the values a failed comparison saw.
pub fn test(paths: &[PathBuf], as_json: bool, target: Target) -> Result<u8, Failure> {
    let (status, report) = match target {
        Target::Python => test_report(paths, as_json, None)?,
        Target::C => c_test_report(paths, as_json)?,
    };
    print!("{report}");
    Ok(status)
}

/// The C program of `path`, or of its `test` blocks, built into `dir`: the executable, or `None`
/// once the diagnostics that stop it are printed.
fn c_executable(path: &Path, dir: &Path, tests: bool) -> Result<Option<PathBuf>, Failure> {
    let text = files::read(path)?;
    let absolute = std::path::absolute(path).map_err(|e| Failure(format!("{}: {e}", path.display())))?;
    let interfaces: Interfaces = files::interfaces_for(path)
        .into_iter()
        .map(|b| {
            let read = lotml_check::interface_of(&b.module, &b.text).0;
            (b.module, read)
        })
        .collect();
    let program = match lotml_c::compile_program(&text, &absolute, &interfaces, tests) {
        Ok(program) => program,
        Err(diagnostics) => {
            let shown = path.display().to_string();
            let report = Report { file: &shown, text: &text, diagnostics };
            print!("{}", lotml_diag::text(&[report], Some(lotml_diag::DEFAULT_LIMIT)));
            return Ok(None);
        }
    };
    let stem =
        path.file_stem().map_or("program".into(), |s| s.to_string_lossy().replace(|c: char| !c.is_alphanumeric(), "_"));
    std::fs::create_dir_all(dir).map_err(|e| Failure(format!("cannot create {}: {e}", dir.display())))?;
    let source = dir.join(format!("{stem}.c"));
    write(&source, &program.c)?;
    lotml_c::write_runtime(dir).map_err(|e| Failure(format!("cannot write the runtime in {}: {e}", dir.display())))?;
    let exe = dir.join(if cfg!(windows) { format!("{stem}.exe") } else { stem });
    let compiler = lotml_c::driver::find().map_err(Failure)?;
    compiler.build(&source, &exe, &program.libraries).map_err(Failure)?;
    Ok(Some(exe))
}

/// `lotml build --target c`: each file built into `out`, its C beside it.
fn build_c(paths: &[PathBuf], out: &Path) -> Result<u8, Failure> {
    let mut status = 0;
    for path in files::expand(paths)? {
        match c_executable(&path, out, false)? {
            Some(exe) => println!("{} -> {}", path.display(), exe.display()),
            None => status = 1,
        }
    }
    Ok(status)
}

/// What `lotml test --target c` prints, and its exit status: each file's tests built and run,
/// reported as the Python target reports them.
fn c_test_report(paths: &[PathBuf], as_json: bool) -> Result<(u8, String), Failure> {
    let scratch = Scratch::new()?;
    let mut rows = Vec::new();
    for (k, path) in files::expand(paths)?.into_iter().enumerate() {
        let dir = scratch.0.join(k.to_string());
        let Some(exe) = c_executable(&path, &dir, true)? else { return Ok((1, String::new())) };
        let output = Command::new(&exe)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| Failure(format!("cannot run {}: {e}", exe.display())))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let tests: Vec<Value> = serde_json::from_str(stdout.lines().last().unwrap_or("")).map_err(|_| {
            Failure(format!("the tests did not report: {}", String::from_utf8_lossy(&output.stderr).trim()))
        })?;
        let shown = path.display().to_string();
        for t in tests {
            let mut row = t;
            row["file"] = shown.clone().into();
            rows.push(row);
        }
    }
    Ok(report_rows(rows, as_json))
}

/// What `lotml test` prints, and its exit status; within `limits` when given.
pub fn test_report(paths: &[PathBuf], as_json: bool, limits: Option<&Limits>) -> Result<(u8, String), Failure> {
    let scratch = Scratch::new()?;
    let modules = match compile(paths, &scratch.0)? {
        Ok(modules) => modules,
        Err(report) => return Ok((1, report)),
    };
    let python = python()?;
    let listed: Vec<Value> = modules.iter().map(|m| json!([m.name, m.source.display().to_string()])).collect();
    // The modules go in on standard input: a command line holding hundreds of paths passes
    // Windows' limit of 32,767 characters.
    let script = format!(
        "{}import importlib, json, lotml_rt\nout = []\nfor name, path in json.loads(sys.stdin.readline()):\n    try:\n        module = importlib.import_module(name)\n        out.append({{'file': path, 'tests': lotml_rt.run_tests(vars(module), path)}})\n    except Exception as error:\n        out.append({{'file': path, 'load': type(error).__name__ + ': ' + str(error)}})\nsys.stdout.write('\\n' + json.dumps(out))",
        search_path(&scratch.0),
    );
    let mut child = interpreter(&python, &script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Failure(format!("cannot run Python: {e}")))?;
    if let Some(mut input) = child.stdin.take() {
        writeln!(input, "{}", Value::Array(listed)).map_err(|e| Failure(format!("cannot reach Python: {e}")))?;
    }
    let output = match limits {
        Some(limits) => wait_limited(child, limits)?,
        None => child.wait_with_output().map_err(|e| Failure(format!("cannot run Python: {e}")))?,
    };
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
    Ok(report_rows(rows, as_json))
}

/// The report of the tests' `rows` and its exit status: 1 when any did not pass.
fn report_rows(rows: Vec<Value>, as_json: bool) -> (u8, String) {
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
    (u8::from(rows.iter().any(|r| r["outcome"] != "pass")), report)
}

/// `lotml bind`: the interface of a Python module, read from its stub by Python's own parser
/// and written to `out/<module>.lotmli` (adr:0012).
pub fn bind(module: &str, stub: Option<&Path>, out: &Path) -> Result<bool, Failure> {
    // The name becomes a file name: identifiers and dots only, so it cannot leave `out`.
    let valid = module.split('.').all(|part| {
        part.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    });
    if !valid {
        return Err(Failure(format!("`{module}` is not a Python module name")));
    }
    let device = |part: &str| {
        let lower = part.to_ascii_lowercase();
        matches!(lower.as_str(), "con" | "prn" | "aux" | "nul")
            || ((lower.starts_with("com") || lower.starts_with("lpt"))
                && lower.len() == 4
                && lower.as_bytes()[3].is_ascii_digit())
    };
    if module.split('.').any(device) {
        return Err(Failure(format!("`{module}` is a device's name on Windows, which no file may have")));
    }
    if lotml_check::is_c_library(module) {
        return Err(Failure(format!(
            "`{module}` names a C library, whose interface is written by hand: bindings/{module}.lotmli (adr:0013)"
        )));
    }
    let python = python()?;
    let output = interpreter(&python, lotml_py::BIND)
        .arg(module)
        .arg(stub.map(|s| s.as_os_str().to_owned()).unwrap_or_default())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> Scratch {
        Scratch::new().unwrap_or_else(|Failure(why)| panic!("{why}"))
    }

    #[test]
    fn a_scratch_directory_is_new_each_time_and_removed_after() {
        let (first, second) = (scratch(), scratch());
        assert_ne!(first.0, second.0);
        assert!(first.0.is_dir() && second.0.is_dir());
        let kept = first.0.clone();
        drop(first);
        assert!(!kept.exists());
    }

    #[test]
    fn tests_that_run_past_their_limit_are_stopped() {
        let dir = scratch();
        let source = dir.0.join("forever.lotml");
        std::fs::write(&source, "test \"forever\":\n    var n = 0\n    while True:\n        n = 1\n").unwrap();
        let limits = Limits { seconds: 2, output: 1024 };
        let started = std::time::Instant::now();
        let Err(Failure(why)) = test_report(&[source], true, Some(&limits)) else { panic!("it should not finish") };
        assert!(why.contains("did not finish within 2 s"), "{why}");
        assert!(started.elapsed() < std::time::Duration::from_secs(30));
    }
}
