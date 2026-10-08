//! Where a Python module's stub is read from, for `lotml bind` and for the compiler binding an
//! import (specs/bind-on-import/ R1.2): the typeshed lotml carries for the standard library, else
//! the packages of the environment made from the project's `uv.lock`, and no other environment
//! (adr:0032, adr:0033). A stub is read as a file; nothing is imported or run.

use std::path::{Path, PathBuf};

/// The environment a module outside the standard library may come from.
pub enum Environment {
    /// The project has no `uv.lock`.
    NoLock,
    /// The project's `uv.lock` has no environment made yet.
    NotMade,
    /// The environment made from the project's `uv.lock`, by its directory.
    Made(PathBuf),
}

/// A module's stub: its text, and what the interface says of it.
#[derive(Debug)]
pub struct Stub {
    pub text: String,
    pub said: String,
}

/// Whether `module`, without its `py.` origin, may name a module: identifiers and dots only, so as
/// a file name it cannot leave the directory it is joined to, and no part a device's name on
/// Windows, which no file may have.
pub fn name(module: &str) -> Result<(), String> {
    let valid = module.split('.').all(|part| {
        part.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    });
    if !valid {
        return Err(format!("`{module}` is not a Python module name"));
    }
    let device = |part: &str| {
        let lower = part.to_ascii_lowercase();
        matches!(lower.as_str(), "con" | "prn" | "aux" | "nul")
            || ((lower.starts_with("com") || lower.starts_with("lpt"))
                && lower.len() == 4
                && lower.as_bytes()[3].is_ascii_digit())
    };
    if module.split('.').any(device) {
        return Err(format!("`{module}` is a device's name on Windows, which no file may have"));
    }
    Ok(())
}

/// The interface of the Python module `module`, without its `py.` origin, generated from its stub
/// as `lotml bind` writes it, for the project at `project`; or why there is none.
pub fn interface(module: &str, project: Option<&Path>) -> Result<String, String> {
    name(module)?;
    let stub = find(module, || environment(project))?;
    let text = lotml_bind::binder::interface(module, &stub.text, &stub.said)
        .map_err(|lotml_bind::binder::Refused(why)| format!("cannot bind `{module}`: {why}"))?;
    if let Some(problem) = lotml_check::interface(&text).1.first() {
        return Err(format!("the binding of `{module}` does not check: {} {}", problem.code, problem.message));
    }
    Ok(text)
}

/// The environment the project at `project` binds its packages from: the one made from its
/// `uv.lock`, found without running Python; an error for a lock lotml does not install from.
pub fn environment(project: Option<&Path>) -> Result<Environment, String> {
    let Some(locked) = project.map(crate::dependencies::locked).transpose()?.flatten() else {
        return Ok(Environment::NoLock);
    };
    let made = crate::dependencies::environments().and_then(|root| crate::dependencies::made(&root, &locked));
    Ok(made.map_or(Environment::NotMade, Environment::Made))
}

/// The stub of `module`, a dotted name of identifiers without its `py.` origin, or why there is
/// none. A standard-library name is never taken from the project's packages, and `environment` is
/// asked for only when the module is none of the standard library's.
pub fn find(module: &str, environment: impl FnOnce() -> Result<Environment, String>) -> Result<Stub, String> {
    match lotml_bind::typeshed::find(module) {
        lotml_bind::typeshed::Found::Stub { path, text } => {
            let commit = lotml_bind::typeshed::COMMIT.trim();
            let said = format!("typeshed's stdlib/{path}, at commit {}", &commit[..commit.len().min(12)]);
            Ok(Stub { text: text.to_string(), said })
        }
        lotml_bind::typeshed::Found::Absent { range } => {
            let (major, minor) = lotml_bind::typeshed::PYTHON;
            Err(format!("`{module}` is not in CPython {major}.{minor}'s standard library: typeshed gives it {range}"))
        }
        lotml_bind::typeshed::Found::Missing if lotml_bind::typeshed::is_standard_library(module) => Err(format!(
            "typeshed has no stub for the standard library's `{module}`, and a package of the project never stands in for one; give one with --stub <file.pyi>"
        )),
        lotml_bind::typeshed::Found::Missing => {
            let made = match environment()? {
                Environment::Made(dir) => dir,
                Environment::NotMade => {
                    return Err(format!(
                        "no stub for `{module}`: typeshed has none, and the project's uv.lock has no environment made yet to look in; run `lotml run` or `lotml test` once, or give one with --stub <file.pyi>"
                    ));
                }
                Environment::NoLock => {
                    return Err(format!(
                        "no stub for `{module}`: typeshed has none, and the project has no uv.lock whose environment could hold it; give one with --stub <file.pyi>"
                    ));
                }
            };
            let roots = lotml_py::sources::site_packages(&made);
            let Some(found) = lotml_py::sources::find(module, &roots)? else {
                return Err(format!(
                    "no stub for `{module}`: typeshed has none, nor does any package in the environment made from the project's uv.lock ({}); give one with --stub <file.pyi>",
                    made.display()
                ));
            };
            let text = read(&found.path)?;
            Ok(Stub { text, said: found.said })
        }
    }
}

/// A stub's text, read no further than one byte past what the binder reads, so a file that is
/// larger, grows, or never ends is refused without being held whole.
pub fn read(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let largest = lotml_bind::binder::LARGEST;
    let cannot = |e: std::io::Error| format!("cannot read {}: {e}", path.display());
    let file = std::fs::File::open(path).map_err(cannot)?;
    let mut text = String::new();
    file.take(largest as u64 + 1).read_to_string(&mut text).map_err(cannot)?;
    if text.len() > largest {
        return Err(format!("cannot bind {}: it is past the {largest} bytes lotml reads", path.display()));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made environment in a directory of its own, holding `packages` under its `site-packages`.
    fn environment(name: &str, packages: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lotml-stubs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let site = if cfg!(windows) {
            dir.join("Lib").join("site-packages")
        } else {
            dir.join("lib").join("python3.14").join("site-packages")
        };
        for (path, text) in packages {
            let file = site.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
        std::fs::create_dir_all(&site).unwrap();
        dir
    }

    #[test]
    fn a_name_is_identifiers_and_dots_and_never_a_device() {
        assert!(name("os.path").is_ok());
        assert!(name("../etc").unwrap_err().contains("not a Python module name"));
        assert!(name("a..b").is_err() && name("1a").is_err() && name("").is_err());
        assert!(name("x.COM1").unwrap_err().contains("device"));
    }

    #[test]
    fn an_interface_is_generated_from_typeshed_as_bind_writes_it() {
        let text = interface("textwrap", None).unwrap();
        assert!(
            text.contains(
                "fn dedent(text: str) -> str ! PyError
"
            ),
            "{text}"
        );
        assert!(interface("con", None).unwrap_err().contains("device"));
        assert!(interface("distutils", None).unwrap_err().contains("not in CPython"));
    }

    #[test]
    fn the_standard_library_comes_from_the_typeshed_lotml_carries() {
        let stub = find("textwrap", || panic!("the standard library needs no environment")).unwrap();
        assert!(stub.said.starts_with("typeshed's stdlib/textwrap.pyi, at commit "), "{}", stub.said);
        assert!(stub.text.contains("def dedent"));
    }

    #[test]
    fn a_standard_library_name_is_never_taken_from_the_environment() {
        let made = environment("shadow", &[("distutils/__init__.pyi", "def f() -> int: ...\n")]);
        let why = find("distutils", || Ok(Environment::Made(made))).unwrap_err();
        assert!(why.contains("not in CPython 3.14's standard library"), "{why}");
    }

    #[test]
    fn a_package_comes_from_the_lock_s_environment_alone() {
        let made = environment("package", &[("greet-stubs/__init__.pyi", "def hello(name: str) -> str: ...\n")]);
        let stub = find("greet", || Ok(Environment::Made(made))).unwrap();
        assert!(stub.said.contains("the stub-only package `greet-stubs`"), "{}", stub.said);
        assert_eq!(stub.text, "def hello(name: str) -> str: ...\n");
    }

    #[test]
    fn with_no_environment_made_or_no_lock_a_package_is_not_bound_and_the_reason_says_what_to_do() {
        let not_made = find("greet", || Ok(Environment::NotMade)).unwrap_err();
        assert!(not_made.contains("no environment made yet") && not_made.contains("lotml run"), "{not_made}");
        let no_lock = find("greet", || Ok(Environment::NoLock)).unwrap_err();
        assert!(no_lock.contains("no uv.lock") && no_lock.contains("--stub"), "{no_lock}");
    }

    #[test]
    fn a_package_the_environment_lacks_names_the_environment_looked_in() {
        let made = environment("empty", &[]);
        let why = find("greet", || Ok(Environment::Made(made.clone()))).unwrap_err();
        assert!(why.contains("typeshed has none") && why.contains(&made.display().to_string()), "{why}");
    }
}
