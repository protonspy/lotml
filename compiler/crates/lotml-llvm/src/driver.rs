//! `clang`, found and run on the LLVM IR the backend writes and on the runtime (specs/llvm-backend
//! R1.2–R1.5): looked for where adr:0021 says, never in the working directory, and given its
//! arguments as a vector, never through a shell.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use crate::cache::{Cache, Object};
use crate::sha256::{self, Sha256};

/// The oldest `clang` the backend writes IR for: the first to read only opaque pointers.
pub const OLDEST: u32 = 17;

/// A `clang` found on this machine, and its major version.
#[derive(Clone, Debug)]
pub struct Clang {
    pub program: PathBuf,
    pub version: u32,
    /// All that `clang --version` printed: its version, vendor and target, which a cached
    /// runtime object is keyed by.
    identity: String,
}

/// How much `clang` optimises: `lotml run` compiles for a quick start, `lotml build` for speed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Debug,
    Release,
}

/// Where `clang` is looked for, in order, as a message names them.
pub fn searched() -> String {
    let mut places = vec!["LOTML_CLANG".to_string(), "clang on PATH".to_string()];
    if cfg!(windows) {
        places.push(installer_clang().display().to_string());
    }
    places.join(", then ")
}

/// The `clang` of [`searched`], checked to be [`OLDEST`] or newer: looked for and run once per
/// process.
pub fn find() -> Result<Clang, String> {
    static FOUND: OnceLock<Result<Clang, String>> = OnceLock::new();
    FOUND.get_or_init(probe).clone()
}

fn probe() -> Result<Clang, String> {
    let installer = cfg!(windows).then(installer_clang);
    let program = locate(std::env::var_os("LOTML_CLANG").as_deref(), std::env::var_os("PATH").as_deref(), installer)
        .ok_or_else(|| missing("no clang was found"))?;
    let out = Command::new(&program)
        .arg("--version")
        .output()
        .map_err(|e| missing(&format!("{} could not run: {e}", program.display())))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let version = major_version(&text).ok_or_else(|| missing(&format!("{} gave no version", program.display())))?;
    if version < OLDEST {
        return Err(missing(&format!("{} is clang {version}", program.display())));
    }
    Ok(Clang { program, version, identity: text.into_owned() })
}

/// The message for a `clang` that is missing or too old (R1.3).
fn missing(what: &str) -> String {
    format!(
        "{what}: `--target llvm` needs clang {OLDEST} or newer; looked in {}. Set LOTML_CLANG to one, or build with \
         `--target python`",
        searched()
    )
}

/// Where the LLVM installer for Windows puts `clang`, without adding it to `PATH`.
fn installer_clang() -> PathBuf {
    let base = std::env::var_os("ProgramFiles").map_or_else(|| PathBuf::from("C:\\Program Files"), PathBuf::from);
    base.join("LLVM").join("bin").join("clang.exe")
}

/// The first of `variable`, `clang` in a directory of `path`, and `installer` that is a file. A
/// bare name in `variable` is looked up on `path`, the working directory never searched; a path
/// in it must be absolute.
fn locate(variable: Option<&OsStr>, path: Option<&OsStr>, installer: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(given) = variable.filter(|v| !v.is_empty()) {
        let given = Path::new(given);
        if given.components().count() > 1 {
            return (given.is_absolute() && given.is_file()).then(|| given.to_path_buf());
        }
        return on_path(&given.to_string_lossy(), path);
    }
    on_path("clang", path).or_else(|| installer.filter(|p| p.is_file()))
}

fn on_path(name: &str, path: Option<&OsStr>) -> Option<PathBuf> {
    let suffixes: &[&str] = if cfg!(windows) { &[".exe", ""] } else { &[""] };
    std::env::split_paths(path?)
        .filter(|dir| dir.is_absolute())
        .find_map(|dir| suffixes.iter().map(|s| dir.join(format!("{name}{s}"))).find(|c| c.is_file()))
}

/// The major version `clang --version` names: `clang version 23.1.3 (...)`, possibly after a vendor.
fn major_version(text: &str) -> Option<u32> {
    let rest = &text[text.find("clang version ")? + "clang version ".len()..];
    rest.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
}

impl Clang {
    /// Compile the LLVM IR `ll` with the runtime in `runtime_dir` into the executable `exe`,
    /// linking `libraries` by name.
    pub fn build(
        &self,
        ll: &Path,
        runtime_dir: &Path,
        exe: &Path,
        level: Level,
        libraries: &[String],
    ) -> Result<(), String> {
        self.build_with(ll, runtime_dir, exe, level, libraries, false)
    }

    /// `build`, the runtime reporting at exit, when `counting`, the cells still live and the cells
    /// allocated: what the leak tests read (specs/llvm-parity R3.1). With `LOTML_SANITIZE` set, as
    /// CI sets it, the program, not a library, is built under AddressSanitizer and
    /// UndefinedBehaviorSanitizer.
    pub fn build_with(
        &self,
        ll: &Path,
        runtime_dir: &Path,
        exe: &Path,
        level: Level,
        libraries: &[String],
        counting: bool,
    ) -> Result<(), String> {
        self.link(ll, runtime_dir, exe, libraries, Build { level, counting, shared: false })
    }

    /// Compile the LLVM IR `ll` of a library with the runtime in `runtime_dir` into the shared
    /// library `library` at `-O2`, linking `libraries`; on Windows, its import library beside it
    /// (specs/c-abi-export R1.1). Only the functions the IR exports are exported: elsewhere the
    /// runtime's are hidden.
    pub fn build_shared(
        &self,
        ll: &Path,
        runtime_dir: &Path,
        library: &Path,
        libraries: &[String],
    ) -> Result<(), String> {
        self.link(ll, runtime_dir, library, libraries, Build { level: Level::Release, counting: false, shared: true })
    }

    fn link(
        &self,
        ll: &Path,
        runtime_dir: &Path,
        exe: &Path,
        libraries: &[String],
        build: Build,
    ) -> Result<(), String> {
        self.link_with(ll, runtime_dir, exe, libraries, build, Cache::user().as_ref()).map(|_| ())
    }

    /// Compile the LLVM IR `ll` and link it with the runtime of `runtime_dir` into `out`, as
    /// `build` asks. The runtime's object is taken from `cache` when it holds one for this `clang`,
    /// these flags and these sources, and compiled into it otherwise; with no cache, it is compiled
    /// beside `out`. Whether the runtime was compiled is returned.
    pub fn link_with(
        &self,
        ll: &Path,
        runtime_dir: &Path,
        out: &Path,
        libraries: &[String],
        build: Build,
        cache: Option<&Cache>,
    ) -> Result<Runtime, String> {
        let (object, runtime) = self.runtime(runtime_dir, build, cache)?;
        let mut command = Command::new(&self.program);
        command.args(build.flags(false)).arg("-o").arg(out).arg(ll).arg(&object.path);
        if !cfg!(windows) {
            command.arg("-lm");
        }
        command.args(libraries.iter().map(|l| format!("-l{l}")));
        let output = command.output().map_err(|e| format!("could not run {}: {e}", self.program.display()))?;
        if output.status.success() {
            return Ok(runtime);
        }
        let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        Err(rejected(&text, Some(ll)))
    }

    /// The runtime of `runtime_dir` as an object for `build`, and whether it was compiled for it.
    fn runtime(&self, runtime_dir: &Path, build: Build, cache: Option<&Cache>) -> Result<(Object, Runtime), String> {
        let compile = |object: &Path| {
            let mut command = Command::new(&self.program);
            command.args(build.flags(true)).arg("-c").arg(runtime_dir.join("lotml.c"));
            command.arg("-I").arg(runtime_dir).arg("-o").arg(object);
            let output = command.output().map_err(|e| format!("could not run {}: {e}", self.program.display()))?;
            if output.status.success() {
                return Ok(());
            }
            let text =
                format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
            Err(rejected(&text, None))
        };
        let Some(cache) = cache else {
            let object = runtime_dir.join(if cfg!(windows) { "lotml.obj" } else { "lotml.o" });
            compile(&object)?;
            return Ok((Object::kept(object), Runtime::Compiled));
        };
        let key = self.runtime_key(runtime_dir, build)?;
        if let Some(entry) = cache.get(&key) {
            return Ok((Object::kept(entry), Runtime::Cached));
        }
        Ok((cache.put(&key, compile)?, Runtime::Compiled))
    }

    /// What a runtime object is keyed by: this `clang` and all it says of itself, the flags it is
    /// compiled with, and each of the runtime's files as `runtime_dir` holds them.
    fn runtime_key(&self, runtime_dir: &Path, build: Build) -> Result<String, String> {
        let mut hash = Sha256::new();
        let mut field = |bytes: &[u8]| {
            hash.update(&(bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        };
        field(b"lotml runtime object 1");
        field(self.program.as_os_str().as_encoded_bytes());
        field(self.identity.as_bytes());
        for flag in build.flags(true) {
            field(flag.as_bytes());
        }
        for (name, _) in lotml_runtime::FILES {
            let path = runtime_dir.join(name);
            let text =
                std::fs::read(&path).map_err(|e| format!("cannot read the runtime's {}: {e}", path.display()))?;
            field(name.as_bytes());
            field(&text);
        }
        Ok(sha256::hex(&hash.finish())[..32].to_string())
    }
}

/// Whether a build compiled the runtime, or took its object from the cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Runtime {
    Compiled,
    Cached,
}

/// How a build compiles and links: its level, whether it counts cells, whether it is a library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Build {
    pub level: Level,
    pub counting: bool,
    pub shared: bool,
}

impl Build {
    /// The flags of `clang` for the runtime's compile, with `runtime`, or for compiling the
    /// program and linking. With `LOTML_SANITIZE` set, as CI sets it, a program, not a library, is
    /// built under AddressSanitizer and UndefinedBehaviorSanitizer.
    fn flags(self, runtime: bool) -> Vec<&'static str> {
        let mut flags = vec![if self.level == Level::Release { "-O2" } else { "-O0" }];
        if self.level == Level::Debug {
            flags.push("-g");
        }
        if runtime && self.counting {
            flags.push("-DLT_COUNT_CELLS");
        }
        if self.shared {
            if !runtime {
                flags.push("-shared");
            }
            if !cfg!(windows) {
                flags.extend(["-fPIC", "-fvisibility=hidden"]);
            }
        }
        if !self.shared && std::env::var_os("LOTML_SANITIZE").is_some() {
            flags.extend(["-fsanitize=address,undefined", "-fno-sanitize-recover=undefined"]);
        }
        if !cfg!(windows) {
            flags.push("-pthread");
        }
        flags.push("-w");
        flags
    }
}

impl Clang {
    /// Compile the C file `source`, on its own, into the executable `exe` at `-O2`: the hand-written
    /// programs the benchmarks time LotML against.
    pub fn build_c(&self, source: &Path, exe: &Path) -> Result<(), String> {
        let mut command = Command::new(&self.program);
        command.args(["-std=c11", "-O2", "-w", "-o"]).arg(exe).arg(source);
        if !cfg!(windows) {
            command.arg("-lm");
        }
        let out = command.output().map_err(|e| format!("could not run {}: {e}", self.program.display()))?;
        if out.status.success() {
            return Ok(());
        }
        Err(format!("clang failed on {}:\n{}", source.display(), String::from_utf8_lossy(&out.stderr).trim_end()))
    }
}

/// What a failed build says: an error `clang` found in the IR is a bug in the compiler that wrote
/// it, and the `.ll` file is kept to report it with (R1.5); any other is `clang`'s own.
fn rejected(output: &str, ll: Option<&Path>) -> String {
    if let Some(missing) = missing_tool(output, cfg!(windows)) {
        return missing;
    }
    let first = output.lines().find(|l| l.contains("error:")).unwrap_or_else(|| output.lines().next().unwrap_or(""));
    let in_ir = ll
        .and_then(Path::file_name)
        .is_some_and(|name| output.lines().any(|l| l.contains("error:") && l.contains(&*name.to_string_lossy())));
    if let Some(ll) = ll.filter(|_| in_ir) {
        format!(
            "a bug in the LotML compiler: clang rejected the LLVM IR it wrote, kept in {}:\n{}",
            ll.display(),
            first.trim()
        )
    } else {
        format!("clang failed:\n{}", output.trim_end())
    }
}

/// The libraries of the Windows SDK and of Visual Studio's C runtime a program links.
const WINDOWS_SDK: &[&str] =
    &["kernel32", "user32", "advapi32", "shell32", "ole32", "uuid", "ws2_32", "ucrt", "ucrtd", "libucrt", "libucrtd"];
const MSVC_RUNTIME: &[&str] = &[
    "libcmt",
    "libcmtd",
    "msvcrt",
    "msvcrtd",
    "oldnames",
    "vcruntime",
    "vcruntimed",
    "libvcruntime",
    "libvcruntimed",
    "msvcprt",
    "libcpmt",
];
/// The C library's own parts, which its development files hold.
const C_LIBRARY: &[&str] = &["c", "m", "pthread", "dl", "rt"];

/// What a failed build lacks, named for whoever has to install it, with the line of `clang`'s
/// output that shows it: a linker, the Windows SDK, Visual Studio's C runtime, the C library's
/// headers or development files, or a C library the program links. `None` for any other failure.
fn missing_tool(output: &str, windows: bool) -> Option<String> {
    const OTHERWISE: &str = "or build with `--target python`, which needs no clang";
    const WORKLOAD: &str = "Visual Studio or its Build Tools with the \"Desktop development with C++\" workload";
    let named = |line: &str, what: String| Some(format!("{what}\n  clang said: {}", line.trim()));
    for line in output.lines() {
        let lower = line.to_ascii_lowercase();
        if let Some(file) = windows_library(&lower) {
            let stem = file.trim_end_matches(".lib");
            if WINDOWS_SDK.contains(&stem) {
                return named(
                    line,
                    format!(
                        "the Windows SDK was not found (`{file}` is missing): clang links with its libraries; install it \
                         with {WORKLOAD}, {OTHERWISE}"
                    ),
                );
            }
            if MSVC_RUNTIME.contains(&stem) {
                return named(
                    line,
                    format!(
                        "Visual Studio's C runtime was not found (`{file}` is missing): install {WORKLOAD}, {OTHERWISE}"
                    ),
                );
            }
            return named(line, user_library(stem, windows));
        }
        if let Some(name) = unix_library(&lower) {
            if C_LIBRARY.contains(&name) {
                return named(line, c_library(&format!("-l{name}")));
            }
            return named(line, user_library(name, windows));
        }
        if let Some(file) =
            ["crt1.o", "crti.o", "crtn.o", "scrt1.o"].into_iter().find(|f| lower.contains(&format!("cannot find {f}")))
        {
            return named(line, c_library(file));
        }
        if let Some(header) = missing_header(line) {
            let install = if windows {
                format!("on Windows they come with the Windows SDK; install it with {WORKLOAD}")
            } else {
                "install the C library's development files (`libc6-dev` on Debian and Ubuntu, `glibc-devel` on Fedora)"
                    .to_string()
            };
            return named(
                line,
                format!("the C library's headers were not found (`{header}` is missing): {install}, {OTHERWISE}"),
            );
        }
        let no_linker =
            ["unable to execute command", "invalid linker name", "unable to find a visual studio installation"]
                .iter()
                .any(|p| lower.contains(p))
                || lower.starts_with("xcrun: error");
        if no_linker {
            let linker = if windows {
                format!("clang links with Visual Studio's `link.exe`, which comes with {WORKLOAD}")
            } else {
                "clang links with the system's `ld`; install the platform's build tools (binutils, or the Xcode command \
                 line tools on macOS)"
                    .to_string()
            };
            return named(line, format!("no linker was found: {linker}; {OTHERWISE}"));
        }
    }
    None
}

/// The `.lib` file a Windows linker could not open: `could not open 'kernel32.lib'` (lld-link) or
/// `cannot open input file 'kernel32.lib'` (`link.exe`), read from a lowercased line.
fn windows_library(lower: &str) -> Option<&str> {
    if !lower.contains("could not open") && !lower.contains("cannot open") {
        return None;
    }
    let rest = &lower[lower.find('\'')? + 1..];
    let file = &rest[..rest.find('\'')?];
    file.ends_with(".lib").then_some(file)
}

/// The library `-l<name>` a Unix linker could not find: `cannot find -lm` (`ld`), `unable to find
/// library -lm` (`ld.lld`) or `library not found for -lm` (macOS), read from a lowercased line.
fn unix_library(lower: &str) -> Option<&str> {
    let at = ["cannot find -l", "unable to find library -l", "library not found for -l"]
        .iter()
        .find_map(|p| lower.find(p).map(|k| k + p.len()))?;
    let rest = &lower[at..];
    let name = &rest[..rest.find(|c: char| c.is_whitespace() || c == ':' || c == '\'').unwrap_or(rest.len())];
    (!name.is_empty()).then_some(name)
}

/// The system header `clang` could not find, `fatal error: 'stdio.h' file not found`; never the
/// runtime's own, whose absence is the compiler's fault.
fn missing_header(line: &str) -> Option<&str> {
    let rest = &line[line.find("fatal error: '")? + "fatal error: '".len()..];
    let header = &rest[..rest.find("' file not found")?];
    (!header.starts_with("lotml")).then_some(header)
}

fn c_library(file: &str) -> String {
    format!(
        "the C library's development files were not found (`{file}` is missing): install them (`libc6-dev` on Debian \
         and Ubuntu, `glibc-devel` on Fedora), or build with `--target python`, which needs no clang"
    )
}

fn user_library(name: &str, windows: bool) -> String {
    let variable = if windows { "LIB" } else { "LIBRARY_PATH" };
    format!("the C library `{name}` the program links was not found: install it, or name its directory in `{variable}`")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("lotml-llvm-driver").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn file(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, "").unwrap();
        p
    }

    #[test]
    fn lotml_clang_comes_first_then_path_then_the_installer() {
        let dir = scratch("order");
        let named = file(&dir, "my-clang");
        let on_path = dir.join("bin");
        std::fs::create_dir_all(&on_path).unwrap();
        let clang = file(&on_path, if cfg!(windows) { "clang.exe" } else { "clang" });
        let installer = file(&dir, "installer-clang");
        let path = std::env::join_paths([&on_path]).unwrap();
        assert_eq!(locate(Some(named.as_os_str()), Some(&path), Some(installer.clone())), Some(named));
        assert_eq!(locate(None, Some(&path), Some(installer.clone())), Some(clang));
        assert_eq!(locate(None, None, Some(installer.clone())), Some(installer));
        assert_eq!(locate(None, None, Some(dir.join("absent"))), None);
    }

    #[test]
    fn a_relative_directory_on_path_is_never_searched() {
        let path = std::env::join_paths([Path::new("."), Path::new("bin")]).unwrap();
        assert_eq!(locate(None, Some(&path), None), None);
    }

    #[test]
    fn a_relative_path_in_lotml_clang_is_not_run_from_the_working_directory() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp/relative-clang");
        std::fs::create_dir_all(&dir).unwrap();
        file(&dir, "my-clang");
        let relative = Path::new("../../target/tmp/relative-clang/my-clang");
        assert!(relative.is_file(), "the test runs in the crate's directory");
        assert_eq!(locate(Some(relative.as_os_str()), None, None), None);
    }

    #[test]
    fn the_major_version_is_read_after_any_vendor() {
        assert_eq!(major_version("clang version 23.1.3 (https://github.com/llvm/llvm-project 0d26)\n"), Some(23));
        assert_eq!(major_version("Ubuntu clang version 18.1.3 (1ubuntu1)\nTarget: x86_64"), Some(18));
        assert_eq!(major_version("Apple clang version 15.0.0"), Some(15));
        assert_eq!(major_version("gcc (GCC) 13.2"), None);
    }

    #[test]
    fn a_missing_clang_names_where_it_looked_the_version_it_needs_and_the_python_target() {
        let message = missing("no clang was found");
        assert!(message.contains("LOTML_CLANG"), "{message}");
        assert!(message.contains("clang on PATH"), "{message}");
        assert!(message.contains(&format!("clang {OLDEST} or newer")), "{message}");
        assert!(message.contains("--target python"), "{message}");
    }

    #[test]
    fn clang_rejecting_the_ir_is_reported_as_a_compiler_bug_naming_the_kept_file() {
        let output = "C:\\b\\prog.ll:3:7: error: expected value token\n  ret i64 %x\n      ^\n1 error generated.\n";
        let message = rejected(output, Some(Path::new("C:\\b\\prog.ll")));
        assert!(message.starts_with("a bug in the LotML compiler"), "{message}");
        assert!(message.contains("prog.ll:3:7: error: expected value token"), "{message}");
        assert!(message.contains("kept in C:\\b\\prog.ll"), "{message}");
        let link = rejected("lld-link: error: undefined symbol: foo\n", Some(Path::new("prog.ll")));
        assert!(link.starts_with("clang failed"), "{link}");
        let runtime = rejected("lotml.c:9:1: error: unknown type name 'x'\n", None);
        assert!(runtime.starts_with("clang failed"), "a failure in the runtime is not the IR's: {runtime}");
    }

    #[test]
    fn a_missing_windows_sdk_or_c_runtime_is_named_whichever_linker_says_so() {
        for output in [
            "lld-link: error: could not open 'kernel32.lib': no such file or directory\n",
            "LINK : fatal error LNK1181: cannot open input file 'kernel32.lib'\r\n",
            "lld-link: error: could not open 'libucrt.lib': no such file or directory\n",
        ] {
            let message = missing_tool(output, true).expect("named");
            assert!(message.starts_with("the Windows SDK was not found"), "{message}");
            assert!(message.contains("Desktop development with C++"), "{message}");
            assert!(message.contains("--target python"), "{message}");
            assert!(message.contains("clang said: "), "the decisive line is kept: {message}");
        }
        let runtime = missing_tool("LINK : fatal error LNK1104: cannot open file 'libcmt.lib'\n", true).unwrap();
        assert!(runtime.starts_with("Visual Studio's C runtime was not found (`libcmt.lib` is missing)"), "{runtime}");
    }

    #[test]
    fn a_missing_linker_is_named_for_the_system_it_runs_on() {
        let windows = "clang: error: unable to execute command: program not executable\n\
                       clang: error: linker command failed with exit code 1 (use -v to see invocation)\n";
        let message = missing_tool(windows, true).expect("named");
        assert!(message.starts_with("no linker was found"), "{message}");
        assert!(message.contains("`link.exe`"), "{message}");
        let unix = "clang: error: unable to execute command: Executable \"ld\" doesn't exist!\n";
        let message = missing_tool(unix, false).expect("named");
        assert!(message.contains("the system's `ld`"), "{message}");
        assert!(missing_tool("xcrun: error: invalid active developer path\n", false).is_some());
    }

    #[test]
    fn missing_c_library_files_are_told_apart_from_a_library_the_program_names() {
        let libc = missing_tool("/usr/bin/ld: cannot find crt1.o: No such file or directory\n", false).unwrap();
        assert!(libc.starts_with("the C library's development files were not found (`crt1.o`"), "{libc}");
        let m = missing_tool("ld.lld: error: unable to find library -lm\n", false).unwrap();
        assert!(m.contains("(`-lm` is missing)"), "{m}");
        let header = missing_tool("C:\\t\\lotml.c:3:10: fatal error: 'stdio.h' file not found\n", true).unwrap();
        assert!(header.starts_with("the C library's headers were not found (`stdio.h`"), "{header}");
        assert!(header.contains("Windows SDK"), "{header}");
        let own = "C:\\t\\lotml.c:3:10: fatal error: 'lotml.h' file not found\n";
        assert_eq!(missing_tool(own, true), None, "the runtime's own header missing is the compiler's fault");
        for (output, windows) in [
            ("/usr/bin/ld: cannot find -lsqlite3: No such file or directory\n", false),
            ("ld: library not found for -lsqlite3\n", false),
            ("lld-link: error: could not open 'sqlite3.lib': no such file or directory\n", true),
        ] {
            let message = missing_tool(output, windows).expect("named");
            assert!(message.starts_with("the C library `sqlite3` the program links was not found"), "{message}");
            assert!(message.contains(if windows { "`LIB`" } else { "`LIBRARY_PATH`" }), "{message}");
        }
        assert_eq!(missing_tool("lld-link: error: undefined symbol: foo\n", true), None);
    }
}
