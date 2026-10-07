//! `clang`, found and run on the LLVM IR the backend writes and on the runtime (specs/llvm-backend
//! R1.2–R1.5): looked for where adr:0021 says, never in the working directory, and given its
//! arguments as a vector, never through a shell.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The oldest `clang` the backend writes IR for: the first to read only opaque pointers.
pub const OLDEST: u32 = 17;

/// A `clang` found on this machine, and its major version.
#[derive(Clone, Debug)]
pub struct Clang {
    pub program: PathBuf,
    pub version: u32,
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

/// The `clang` of [`searched`], checked to be [`OLDEST`] or newer.
pub fn find() -> Result<Clang, String> {
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
    Ok(Clang { program, version })
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
        let Build { level, counting, shared } = build;
        let mut command = Command::new(&self.program);
        command.arg(if level == Level::Release { "-O2" } else { "-O0" });
        if level == Level::Debug {
            command.arg("-g");
        }
        if counting {
            command.arg("-DLT_COUNT_CELLS");
        }
        if shared {
            command.arg("-shared");
            if !cfg!(windows) {
                command.args(["-fPIC", "-fvisibility=hidden"]);
            }
        }
        if !shared && std::env::var_os("LOTML_SANITIZE").is_some() {
            command.args(["-fsanitize=address,undefined", "-fno-sanitize-recover=undefined"]);
        }
        command.arg("-w").arg("-o").arg(exe).arg(ll).arg(runtime_dir.join("lotml.c"));
        command.arg("-I").arg(runtime_dir);
        if !cfg!(windows) {
            command.args(["-lm", "-pthread"]);
        }
        command.args(libraries.iter().map(|l| format!("-l{l}")));
        let out = command.output().map_err(|e| format!("could not run {}: {e}", self.program.display()))?;
        if out.status.success() {
            return Ok(());
        }
        let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        Err(rejected(&text, ll))
    }
}

/// How a build compiles and links: its level, whether it counts cells, whether it is a library.
#[derive(Clone, Copy)]
struct Build {
    level: Level,
    counting: bool,
    shared: bool,
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
fn rejected(output: &str, ll: &Path) -> String {
    let first = output.lines().find(|l| l.contains("error:")).unwrap_or_else(|| output.lines().next().unwrap_or(""));
    let in_ir = output
        .lines()
        .any(|l| l.contains("error:") && l.contains(&*ll.file_name().unwrap_or_default().to_string_lossy()));
    if in_ir {
        format!(
            "a bug in the LotML compiler: clang rejected the LLVM IR it wrote, kept in {}:\n{}",
            ll.display(),
            first.trim()
        )
    } else {
        format!("clang failed:\n{}", output.trim_end())
    }
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
        let message = rejected(output, Path::new("C:\\b\\prog.ll"));
        assert!(message.starts_with("a bug in the LotML compiler"), "{message}");
        assert!(message.contains("prog.ll:3:7: error: expected value token"), "{message}");
        assert!(message.contains("kept in C:\\b\\prog.ll"), "{message}");
        let link = rejected("lld-link: error: undefined symbol: foo\n", Path::new("prog.ll"));
        assert!(link.starts_with("clang failed"), "{link}");
    }
}
