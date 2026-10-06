//! The platform's C compiler, found and run on what the backend writes (R5.1, R5.2).

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

/// How a compiler takes its options: gcc and clang, or Visual Studio's `cl` and `clang-cl`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavor {
    Gnu,
    Msvc,
}

/// A C compiler found on this machine, with the arguments and environment it is run with.
#[derive(Clone, Debug)]
pub struct CCompiler {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(OsString, OsString)>,
    pub flavor: Flavor,
}

/// Where a compiler is looked for, in order: what the message says when none is found.
pub const SEARCHED: &str = "LOTML_CC, CC, then cc, gcc and clang on PATH, then Visual Studio's cl";

/// The first C compiler of [`SEARCHED`] that this machine has.
pub fn find() -> Result<CCompiler, String> {
    find_with(&|name| std::env::var(name).ok(), std::env::var_os("PATH").as_deref(), cfg!(windows))
}

fn find_with(
    var: &dyn Fn(&str) -> Option<String>,
    path: Option<&OsStr>,
    visual_studio: bool,
) -> Result<CCompiler, String> {
    for name in ["LOTML_CC", "CC"] {
        let Some(value) = var(name) else { continue };
        let mut words = value.split_whitespace().map(String::from);
        let Some(program) = words.next() else { continue };
        let resolved = on_path(&program, path).unwrap_or_else(|| PathBuf::from(&program));
        return Ok(CCompiler::new(resolved, words.collect(), Vec::new()));
    }
    for name in ["cc", "gcc", "clang"] {
        if let Some(found) = on_path(name, path) {
            return Ok(CCompiler::new(found, Vec::new(), Vec::new()));
        }
    }
    if visual_studio && let Some(found) = visual_studio_cl() {
        return Ok(found);
    }
    Err(format!("no C compiler found: looked for {SEARCHED}; set LOTML_CC to one"))
}

/// `name` in a directory of `path`, as the shell would run it: with `.exe` on Windows.
fn on_path(name: &str, path: Option<&OsStr>) -> Option<PathBuf> {
    let given = Path::new(name);
    if given.components().count() > 1 {
        return given.is_file().then(|| given.to_path_buf());
    }
    let suffixes: &[&str] = if cfg!(windows) { &[".exe", ""] } else { &[""] };
    std::env::split_paths(path?)
        .find_map(|dir| suffixes.iter().map(|s| dir.join(format!("{name}{s}"))).find(|candidate| candidate.is_file()))
}

#[cfg(windows)]
fn visual_studio_cl() -> Option<CCompiler> {
    let tool = find_msvc_tools::find_tool("x86_64", "cl.exe")?;
    let env = tool.env().into_iter().cloned().collect();
    Some(CCompiler::new(tool.path().to_path_buf(), Vec::new(), env))
}

#[cfg(not(windows))]
fn visual_studio_cl() -> Option<CCompiler> {
    None
}

impl CCompiler {
    fn new(program: PathBuf, args: Vec<String>, env: Vec<(OsString, OsString)>) -> CCompiler {
        let stem = program.file_stem().map(|s| s.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let flavor = if stem == "cl" || stem == "clang-cl" { Flavor::Msvc } else { Flavor::Gnu };
        CCompiler { program, args, env, flavor }
    }

    /// Build `source`, one C file, into the executable `exe`, linking `libraries` by name; the
    /// compiler's own output when it fails.
    pub fn build(&self, source: &Path, exe: &Path, libraries: &[String]) -> Result<(), String> {
        self.build_with(source, exe, libraries, false)
    }

    /// [`CCompiler::build`], with AddressSanitizer and UndefinedBehaviorSanitizer when
    /// `sanitize` is set and the compiler is gcc or clang: a use after free, a double free or
    /// undefined behaviour then stops the program with a report.
    pub fn build_with(&self, source: &Path, exe: &Path, libraries: &[String], sanitize: bool) -> Result<(), String> {
        let mut command = Command::new(&self.program);
        command.args(&self.args).envs(self.env.iter().map(|(k, v)| (k, v)));
        match self.flavor {
            Flavor::Gnu => {
                if sanitize {
                    command.args(["-fsanitize=address,undefined", "-fno-sanitize-recover=undefined", "-g"]);
                }
                command.args(["-std=c11", "-O2", "-o"]).arg(exe).arg(source).arg("-lm");
                if !cfg!(windows) {
                    command.arg("-pthread");
                }
                command.args(libraries.iter().map(|l| format!("-l{l}")));
            }
            Flavor::Msvc => {
                let objects = exe.parent().unwrap_or(Path::new("."));
                command.args(["/nologo", "/std:c11", "/O2", "/utf-8"]).arg(source);
                command.arg(format!("/Fe{}", exe.display())).arg(format!("/Fo{}\\", objects.display()));
                command.args(libraries.iter().map(|l| format!("{l}.lib")));
            }
        }
        let out = command.output().map_err(|e| format!("could not run {}: {e}", self.program.display()))?;
        if out.status.success() {
            return Ok(());
        }
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        Err(format!("{} failed on {}:\n{}", self.program.display(), source.display(), text.trim_end()))
    }
}

/// Whether every program is linked with the C library `name` already: the C runtime and, on
/// Windows, the system's own; linking its import library a second time would mix runtimes.
pub fn linked_always(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    matches!(name.as_str(), "c" | "m")
        || (cfg!(windows) && matches!(name.as_str(), "msvcrt" | "ucrt" | "ucrtbase" | "vcruntime" | "kernel32"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_compiler_named_by_lotml_cc_comes_first_with_its_arguments() {
        let var = |name: &str| match name {
            "LOTML_CC" => Some("zig cc -target x86_64".to_string()),
            "CC" => Some("gcc".to_string()),
            _ => None,
        };
        let found = find_with(&var, None, false).unwrap();
        assert_eq!(found.program, PathBuf::from("zig"));
        assert_eq!(found.args, vec!["cc", "-target", "x86_64"]);
        assert_eq!(found.flavor, Flavor::Gnu);
    }

    #[test]
    fn cl_takes_visual_studio_s_options() {
        assert_eq!(CCompiler::new(PathBuf::from(r"C:\VS\bin\cl.exe"), vec![], vec![]).flavor, Flavor::Msvc);
        assert_eq!(CCompiler::new(PathBuf::from("/usr/bin/clang"), vec![], vec![]).flavor, Flavor::Gnu);
    }

    #[test]
    fn no_compiler_found_says_where_it_looked() {
        let error = find_with(&|_| None, Some(OsStr::new("")), false).unwrap_err();
        assert!(error.contains(SEARCHED), "{error}");
        assert!(error.contains("LOTML_CC"), "{error}");
    }
}
