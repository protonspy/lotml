//! `lotml build`, `lotml run` and `lotml test`: programs compiled to Python and run by CPython, or
//! compiled to LLVM IR and built by `clang` (`--target llvm`, specs/llvm-backend), the target
//! `build` takes unless told otherwise (adr:0022).

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use lotml_check::Interfaces;
use lotml_diag::Report;
use serde_json::{Value, json};

use crate::{Failure, Target, files};
use lotml_llvm::driver::Level;
use lotml_py::resolve::{Managed, Options, ThroughUv, Use, resolve};
use lotml_py::uv::Uv;

/// A compiled module: the name it is imported by, its source's absolute path, which tracebacks
/// name, and the path as the user wrote it.
struct Module {
    name: String,
    source: PathBuf,
    shown: String,
    text: String,
    /// The functions left out of what Python sees (specs/python-object R3.2).
    warnings: Vec<lotml_diag::Diagnostic>,
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
            Ok(module) => compiled.push((module, absolute.clone(), name.clone(), text.clone())),
            Err(diagnostics) => reports.push(Report { file: name, text, diagnostics: diagnostics.clone() }),
        }
    }
    if !reports.is_empty() {
        return Ok(Err(lotml_diag::text(&reports, Some(lotml_diag::DEFAULT_LIMIT))));
    }
    std::fs::create_dir_all(dir).map_err(|e| Failure(format!("cannot create {}: {e}", dir.display())))?;
    write(&dir.join("lotml_rt.py"), lotml_py::RUNTIME)?;
    let mut modules = Vec::new();
    for (module, source, shown, text) in compiled {
        let stem = source
            .file_stem()
            .map_or("program".into(), |s| s.to_string_lossy().replace(|c: char| !c.is_alphanumeric(), "_"));
        let name = format!("{stem}_lotml");
        write(&dir.join(format!("{name}.py")), &module.module)?;
        write(&dir.join(format!("{name}.pyi")), &lotml_py::stub(&shown, &name, &module.checked))?;
        modules.push(Module { name, source, shown, text, warnings: module.warnings.clone() });
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

/// The CPython to run, resolved in adr:0026's order (lotml_py::resolve): `uses` says whether the
/// project's virtual environment may run it, `project` where that is, and `downloads` whether a
/// missing CPython may be fetched through uv.
fn python(uses: Use, project: Option<&Path>, downloads: bool) -> Result<Vec<String>, Failure> {
    let var = |name: &str| std::env::var_os(name);
    let uv = lotml_py::uv::find(&lotml_py::uv::Places::here()).map_err(Failure)?;
    let home = lotml_llvm::cache::user_root()
        .map(|root| root.join("uv"))
        .filter(|home| lotml_llvm::cache::private_directory(home).is_ok());
    let managed = match (uv, home) {
        (Some(path), Some(home)) => Some(ThroughUv { uv: Uv { path, home }, var: &var }),
        _ => None,
    };
    let options = Options { uses, project, downloads };
    resolve(&options, &var, managed.as_ref().map(|m| m as &dyn Managed), &lotml_py::python).map_err(Failure)
}

/// The project a file or directory belongs to: the nearest directory holding `.git` or a
/// `pyproject.toml`, from a directory itself or from a file's own; none without one.
fn project_of(path: &Path) -> Option<PathBuf> {
    let path = std::path::absolute(path).ok()?;
    let dir = if path.is_dir() { path } else { path.parent()?.to_path_buf() };
    dir.ancestors().find(|d| d.join(".git").exists() || d.join("pyproject.toml").is_file()).map(Path::to_path_buf)
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
pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new() -> Result<Scratch, Failure> {
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
/// stream; killed when it runs past `limits.seconds`, which the error says of `what`.
fn wait_limited(mut child: std::process::Child, limits: &Limits, what: &str) -> Result<std::process::Output, Failure> {
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
            return Err(Failure(format!("{what} did not finish within {} s", limits.seconds)));
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

/// `lotml build`: for the LLVM target, each file as `<name>.ll` and the executable built from it,
/// or with `shared` the library and its header, in `out`; for the Python target, as
/// `<name>_lotml.py`, next to the runtime.
pub fn build(paths: &[PathBuf], out: &Path, target: Target, shared: bool) -> Result<u8, Failure> {
    match (target, shared) {
        (Target::Llvm, true) => return build_shared(paths, out),
        (Target::Llvm, false) => return build_llvm(paths, out),
        (Target::Python, true) => {
            return Err(Failure("`--shared` builds a native library: build it with `--target llvm`".into()));
        }
        (Target::Python, false) => {}
    }
    let Some(modules) = compile_or_report(paths, out)? else { return Ok(1) };
    for module in modules {
        if !module.warnings.is_empty() {
            let report = Report { file: &module.shown, text: &module.text, diagnostics: module.warnings.clone() };
            print!("{}", lotml_diag::text(&[report], Some(lotml_diag::DEFAULT_LIMIT)));
        }
        println!("{} -> {}", module.shown, out.join(format!("{}.py", module.name)).display());
    }
    Ok(0)
}

/// Whether a command is offline: by its flag, or by `LOTML_OFFLINE` set to anything but empty, which
/// can only turn offline on (adr:0026).
pub fn offline(flag: bool) -> bool {
    flag || std::env::var_os("LOTML_OFFLINE").is_some_and(|v| !v.is_empty())
}

/// The CPython a command ran on, as its JSON records it (adr:0026): the executable and exact version
/// the interpreter itself reports, and the version of the uv lotml finds, null where there is none.
fn interpreter_record(python: &[String]) -> Value {
    let said = interpreter(python, "import platform, sys\nprint(sys.executable)\nprint(platform.python_version())")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect::<Vec<_>>())
        .unwrap_or_default();
    let uv = lotml_py::uv::find(&lotml_py::uv::Places::here())
        .ok()
        .flatten()
        .and_then(|uv| lotml_py::uv::version(&uv))
        .and_then(|said| said.split_whitespace().nth(1).map(str::to_string));
    json!({"path": said.first(), "version": said.get(1), "uv": uv})
}

/// `lotml run`: the program's `fn main()`, with Python's exit status: 1 when it returned an
/// error, 101 when it panicked; the LLVM target's program exits as Python's does. With `json`, a
/// program that ran is followed by one line: its status and the CPython it ran on.
pub fn run(path: &Path, target: Target, offline: bool, as_json: bool) -> Result<u8, Failure> {
    let scratch = Scratch::new()?;
    if target == Target::Llvm {
        let Some(exe) = llvm_executable(path, &scratch.0, false, Level::Debug)? else { return Ok(1) };
        let status = Command::new(&exe).status().map_err(|e| Failure(format!("cannot run {}: {e}", exe.display())))?;
        let status = exit_status(status);
        if as_json {
            println!("{}", json!({"version": 1, "status": status, "python": null}));
        }
        return Ok(status);
    }
    let Some(modules) = compile_or_report(&[path.to_path_buf()], &scratch.0)? else { return Ok(1) };
    let name = &modules[0].name;
    let python = python(Use::Run, project_of(path).as_deref(), !offline)?;
    let script = format!(
        "{}import lotml_rt\nsys.exit(lotml_rt.main({name}))",
        search_path(&scratch.0),
        name = Value::String(name.clone()),
    );
    let status = interpreter(&python, &script)
        .env_remove("PYTHONIOENCODING")
        .status()
        .map_err(|e| Failure(format!("cannot run Python: {e}")))?;
    let status = status.code().map_or(101, |c| u8::try_from(c).unwrap_or(1));
    if as_json {
        println!("{}", json!({"version": 1, "status": status, "python": interpreter_record(&python)}));
    }
    Ok(status)
}

/// A native program's exit status as `lotml run` returns it. A program the system stopped — a
/// signal, or an exception on Windows — says so and counts as a panic, so a crash is never
/// mistaken for an ordinary exit.
fn exit_status(status: std::process::ExitStatus) -> u8 {
    if let Some(code) = status.code().and_then(|c| u8::try_from(c).ok()) {
        return code;
    }
    match status.code() {
        Some(code) => eprintln!("lotml: the program was stopped by the system (exception {code:#x})"),
        None => eprintln!("lotml: the program was stopped by a signal ({status})"),
    }
    101
}

/// `lotml test`: every `test` block, with the values a failed comparison saw.
pub fn test(paths: &[PathBuf], as_json: bool, target: Target, offline: bool) -> Result<u8, Failure> {
    let (status, report) = match target {
        Target::Python => test_report(paths, as_json, None, !offline, as_json)?,
        Target::Llvm => native_test_report(paths, as_json)?,
    };
    print!("{report}");
    Ok(status)
}

/// The native program of `path` through LLVM, built into `dir` at `level`: the executable, or
/// `None` once the diagnostics that stop it are printed (specs/llvm-backend R1.1).
fn llvm_executable(path: &Path, dir: &Path, tests: bool, level: Level) -> Result<Option<PathBuf>, Failure> {
    let text = files::read(path)?;
    let absolute = std::path::absolute(path).map_err(|e| Failure(format!("{}: {e}", path.display())))?;
    let interfaces: Interfaces = files::interfaces_for(path)
        .into_iter()
        .map(|b| {
            let read = lotml_check::interface_of(&b.module, &b.text).0;
            (b.module, read)
        })
        .collect();
    let program = match lotml_llvm::compile_program(&text, &absolute, &interfaces, tests, level == Level::Debug) {
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
    let ll = dir.join(format!("{stem}.ll"));
    write(&ll, &program.ll)?;
    lotml_runtime::write(dir).map_err(|e| Failure(format!("cannot write the runtime in {}: {e}", dir.display())))?;
    let exe = dir.join(if cfg!(windows) { format!("{stem}.exe") } else { stem });
    let clang = lotml_llvm::driver::find().map_err(Failure)?;
    clang.build(&ll, dir, &exe, level, &program.libraries).map_err(Failure)?;
    Ok(Some(exe))
}

/// `lotml build --target llvm`: each file built into `out` at `-O2`, its LLVM IR beside it.
fn build_llvm(paths: &[PathBuf], out: &Path) -> Result<u8, Failure> {
    let mut status = 0;
    for path in files::expand(paths)? {
        match llvm_executable(&path, out, false, Level::Release)? {
            Some(exe) => println!("{} -> {}", path.display(), exe.display()),
            None => status = 1,
        }
    }
    Ok(status)
}

/// `lotml build --shared`: each file as the shared library C calls, its header and its LLVM IR, in
/// `out` (specs/c-abi-export R1.1); the warnings for the functions left out written first.
fn build_shared(paths: &[PathBuf], out: &Path) -> Result<u8, Failure> {
    let scratch = Scratch::new()?;
    lotml_runtime::write(&scratch.0)
        .map_err(|e| Failure(format!("cannot write the runtime in {}: {e}", scratch.0.display())))?;
    std::fs::create_dir_all(out).map_err(|e| Failure(format!("cannot create {}: {e}", out.display())))?;
    let mut status = 0;
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
        let shown = path.display().to_string();
        let report = |diagnostics: Vec<lotml_diag::Diagnostic>| {
            let report = Report { file: &shown, text: &text, diagnostics };
            print!("{}", lotml_diag::text(&[report], Some(lotml_diag::DEFAULT_LIMIT)));
        };
        let library = match lotml_llvm::compile_library(&text, &absolute, &interfaces) {
            Ok(library) => library,
            Err(diagnostics) => {
                report(diagnostics);
                status = 1;
                continue;
            }
        };
        if !library.warnings.is_empty() {
            report(library.warnings.clone());
        }
        let stem = path.file_stem().map_or("module".into(), |s| s.to_string_lossy().into_owned());
        let name = lotml_llvm::export::module_name(&stem);
        let ll = out.join(format!("{name}.ll"));
        write(&ll, &library.ll)?;
        write(&out.join(format!("{name}.h")), &library.header)?;
        let file = out.join(lotml_llvm::export::library_file(&name));
        let clang = lotml_llvm::driver::find().map_err(Failure)?;
        clang.build_shared(&ll, &scratch.0, &file, &library.libraries).map_err(Failure)?;
        println!("{shown} -> {}", file.display());
    }
    Ok(status)
}

/// What `lotml test --target llvm` prints, and its exit status: each file's tests built and run,
/// reported as the Python target reports them.
fn native_test_report(paths: &[PathBuf], as_json: bool) -> Result<(u8, String), Failure> {
    let scratch = Scratch::new()?;
    let mut rows = Vec::new();
    for (k, path) in files::expand(paths)?.into_iter().enumerate() {
        let dir = scratch.0.join(k.to_string());
        let Some(exe) = llvm_executable(&path, &dir, true, Level::Debug)? else { return Ok((1, String::new())) };
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
    Ok(report_rows(rows, as_json, None))
}

/// What `lotml test` prints, and its exit status; within `limits` when given.
/// `downloads` says whether a missing CPython may be fetched: never for the MCP server or the grader;
/// `record`, whether the JSON names the CPython the tests ran on.
pub fn test_report(
    paths: &[PathBuf],
    as_json: bool,
    limits: Option<&Limits>,
    downloads: bool,
    record: bool,
) -> Result<(u8, String), Failure> {
    let scratch = Scratch::new()?;
    let modules = match compile(paths, &scratch.0)? {
        Ok(modules) => modules,
        Err(report) => return Ok((1, report)),
    };
    let project = paths.first().and_then(|p| project_of(p));
    let python = python(Use::Run, project.as_deref(), downloads)?;
    let listed: Vec<Value> = modules.iter().map(|m| json!([m.name, m.source.display().to_string()])).collect();
    // The modules go in on standard input: a command line holding hundreds of paths passes
    // Windows' limit of 32,767 characters.
    let script = format!(
        "{}import json, lotml_rt\nout = lotml_rt.test_modules(json.loads(sys.stdin.readline()))\nsys.stdout.write('\\n' + json.dumps(out))",
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
        Some(limits) => wait_limited(child, limits, "the tests")?,
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
    Ok(report_rows(rows, as_json, record.then(|| interpreter_record(&python))))
}

/// The report of the tests' `rows` and its exit status: 1 when any did not pass.
fn report_rows(rows: Vec<Value>, as_json: bool, python: Option<Value>) -> (u8, String) {
    let count = |outcome: &str| rows.iter().filter(|r| r["outcome"] == outcome).count();
    let summary =
        json!({"passed": count("pass"), "failed": count("fail"), "errors": count("error"), "panics": count("panic")});
    let report = if as_json {
        let mut report = json!({"version": 1, "tests": rows, "summary": summary});
        if let Some(python) = python {
            report["python"] = python;
        }
        format!("{report}\n")
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

/// `lotml bind`: the interface of a Python module, read from its stub by the binder lotml carries,
/// which runs no Python (specs/rust-binder), and written to `out/py.<module>.lotmli`, the name a
/// program imports it by (adr:0012, adr:0029); the module may be given with its `py.` or without.
/// With no stub given, a standard-library module's is typeshed's, embedded in lotml, and any other
/// module's is found in the project's packages, in PEP 561's order (plans/bind-sources.md 1.1).
pub fn bind(module: &str, stub: Option<&Path>, out: &Path) -> Result<bool, Failure> {
    let module = module.strip_prefix("py.").unwrap_or(module);
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
    let (source, said) = match stub {
        Some(given) => {
            let name = given.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            (read_stub(given)?, format!("{name}, the stub given"))
        }
        None => match lotml_bind::typeshed::find(module) {
            lotml_bind::typeshed::Found::Stub { path, text } => {
                let commit = lotml_bind::typeshed::COMMIT.trim();
                (text.to_string(), format!("typeshed's stdlib/{path}, at commit {}", &commit[..commit.len().min(12)]))
            }
            lotml_bind::typeshed::Found::Absent { range } => {
                let (major, minor) = lotml_bind::typeshed::PYTHON;
                return Err(Failure(format!(
                    "`{module}` is not in CPython {major}.{minor}'s standard library: typeshed gives it {range}"
                )));
            }
            lotml_bind::typeshed::Found::Missing if lotml_bind::typeshed::is_standard_library(module) => {
                return Err(Failure(format!(
                    "typeshed has no stub for the standard library's `{module}`, and a package of the project never stands in for one; give one with --stub <file.pyi>"
                )));
            }
            lotml_bind::typeshed::Found::Missing => {
                let var = |name: &str| std::env::var_os(name);
                let here = std::env::current_dir().ok();
                let venv = lotml_py::resolve::environment(&var, here.as_deref().and_then(project_of).as_deref());
                let roots = venv.as_deref().map(lotml_py::sources::site_packages).unwrap_or_default();
                let Some(found) = lotml_py::sources::find(module, &roots).map_err(Failure)? else {
                    let looked = match &venv {
                        Some(venv) => format!("nor does any package in {}", venv.display()),
                        None => "and no virtual environment of the project was found to look in".into(),
                    };
                    return Err(Failure(format!(
                        "no stub for `{module}`: typeshed has none, {looked}; give one with --stub <file.pyi>"
                    )));
                };
                (read_stub(&found.path)?, found.said)
            }
        },
    };
    let text = lotml_bind::binder::interface(module, &source, &said)
        .map_err(|lotml_bind::binder::Refused(why)| Failure(format!("cannot bind `{module}`: {why}")))?;
    let (interface, problems) = lotml_check::interface(&text);
    if let Some(problem) = problems.first() {
        return Err(Failure(format!("the binding of `{module}` does not check: {} {}", problem.code, problem.message)));
    }
    std::fs::create_dir_all(out).map_err(|e| Failure(format!("cannot create {}: {e}", out.display())))?;
    let path = out.join(format!("py.{module}.lotmli"));
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

/// A stub's text, read no further than one byte past what the binder reads, so a file that is
/// larger, grows, or never ends is refused without being held whole.
fn read_stub(path: &Path) -> Result<String, Failure> {
    use std::io::Read;
    let largest = lotml_bind::binder::LARGEST;
    let cannot = |e: std::io::Error| Failure(format!("cannot read {}: {e}", path.display()));
    let file = std::fs::File::open(path).map_err(cannot)?;
    let mut text = String::new();
    file.take(largest as u64 + 1).read_to_string(&mut text).map_err(cannot)?;
    if text.len() > largest {
        return Err(Failure(format!("cannot bind {}: it is past the {largest} bytes lotml reads", path.display())));
    }
    Ok(text)
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
    fn a_project_is_found_from_a_directory_itself_and_never_made_up() {
        let place = scratch();
        let project = place.0.join("project");
        std::fs::create_dir_all(project.join("src")).unwrap();
        std::fs::write(project.join("pyproject.toml"), "").unwrap();
        std::fs::write(project.join("src").join("p.lot"), "").unwrap();
        assert_eq!(project_of(&project), Some(project.clone()), "a directory given is searched first");
        assert_eq!(project_of(&project.join("src").join("p.lot")), Some(project.clone()));
        let loose = place.0.join("loose.lot");
        std::fs::write(&loose, "").unwrap();
        let found = project_of(&loose);
        assert!(found.as_deref() != Some(place.0.as_path()), "a file's own directory is no project by itself");
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
        let Err(Failure(why)) = test_report(&[source], true, Some(&limits), false, false) else {
            panic!("it should not finish")
        };
        assert!(why.contains("did not finish within 2 s"), "{why}");
        assert!(started.elapsed() < std::time::Duration::from_secs(30));
    }
}
