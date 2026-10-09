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

/// A module's stub: its text, what the interface says of it, and its source as `lotml.lock`
/// records it — typeshed's commit, or the distribution and version that installed it.
#[derive(Debug)]
pub struct Stub {
    pub text: String,
    pub said: String,
    pub source: String,
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
    let cache = lotml_llvm::cache::user_root().map(|root| root.join("interfaces"));
    interface_in(module, project, cache.as_deref())
}

/// [`interface`], kept in `cache` when given: a directory of the user's own, one file per stub
/// and lotml version, written whole (specs/bind-on-import/ R3.1). Where the project's
/// `lotml.lock` names the module, a stub that differs from the lock's is bound all the same and
/// the interface marked so ([`lotml_check::unlocked`], R2.2).
fn interface_in(module: &str, project: Option<&Path>, cache: Option<&Path>) -> Result<String, String> {
    let locked = project.and_then(|root| crate::lockfile::entry(root, &format!("py.{module}")));
    let (stub, text) = generate(module, project, cache, locked.as_ref())?;
    let differs = locked.is_some_and(|entry| entry.stub != crate::lockfile::hash(&stub.text));
    Ok(if differs { lotml_check::unlocked(&text) } else { text })
}

/// The stub of `module` and the interface generated from it, kept in the user's cache: what
/// `lotml.lock` records (specs/bind-on-import/ R2.1), whatever a lock already says.
pub fn bound_with_stub(module: &str, project: Option<&Path>) -> Result<(Stub, String), String> {
    let cache = lotml_llvm::cache::user_root().map(|root| root.join("interfaces"));
    generate(module, project, cache.as_deref(), None)
}

/// The stub of `module` and its interface, read from `cache` when an entry is kept there for the
/// stub — and, where `locked` names the module, only when the entry's hash is the lock's.
fn generate(
    module: &str,
    project: Option<&Path>,
    cache: Option<&Path>,
    locked: Option<&crate::lockfile::Entry>,
) -> Result<(Stub, String), String> {
    name(module)?;
    let stub = find(module, || environment(project))?;
    let cache = cache.filter(|dir| lotml_llvm::cache::private_directory(dir).is_ok());
    let entry = cache.map(|dir| dir.join(format!("{}.lotmli", key(&stub))));
    let trusted = |text: &String| locked.is_none_or(|lock| lock.interface == crate::lockfile::hash(text));
    if let Some(text) = entry.as_deref().and_then(|e| std::fs::read_to_string(e).ok()).filter(trusted) {
        return Ok((stub, text));
    }
    let text = bound(module, &stub)?;
    if let Some(entry) = entry {
        keep(&entry, &text);
    }
    Ok((stub, text))
}

/// The name a stub's interface is kept under: the SHA-256 of the stub and of where it was read,
/// which the interface's first line names, lotml's version, and the binder that wrote it, by a
/// hash of its source, so a changed binder never reads what an earlier one kept.
fn key(stub: &Stub) -> String {
    static BINDER: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let binder =
        BINDER.get_or_init(|| lotml_llvm::sha256::hex_of(lotml_bind::binder::SOURCE.as_bytes())[..16].to_string());
    let mut bytes = stub.text.as_bytes().to_vec();
    bytes.push(0);
    bytes.extend_from_slice(stub.said.as_bytes());
    format!("{}-{}-{binder}", lotml_llvm::sha256::hex_of(&bytes), env!("CARGO_PKG_VERSION"))
}

/// `text` written to `entry` whole: to a name of its own first, then renamed, so a reader never
/// sees half of it; a cache that cannot be written is passed over.
fn keep(entry: &Path, text: &str) {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let partial = entry.with_extension(format!("{}.{nanos}", std::process::id()));
    if std::fs::write(&partial, text).is_ok() && std::fs::rename(&partial, entry).is_err() {
        let _ = std::fs::remove_file(&partial);
    }
}

/// The interface `stub` gives `module`, or why it gives none: it does not check, or it binds no
/// function, which counts as no stub (specs/bind-on-import/ R1.4).
fn bound(module: &str, stub: &Stub) -> Result<String, String> {
    let text = lotml_bind::binder::interface(module, &stub.text, &stub.said)
        .map_err(|lotml_bind::binder::Refused(why)| format!("cannot bind `{module}`: {why}"))?;
    let (interface, problems) = lotml_check::interface(&text);
    if let Some(problem) = problems.first() {
        return Err(format!("the binding of `{module}` does not check: {} {}", problem.code, problem.message));
    }
    if interface.names().next().is_none() && interface.classes().next().is_none() {
        return Err(format!(
            "the stub of `{module}` binds no function or class lotml can type; the interface's comments say why"
        ));
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
            let commit = &commit[..commit.len().min(12)];
            let said = format!("typeshed's stdlib/{path}, at commit {commit}");
            Ok(Stub { text: text.to_string(), said, source: format!("typeshed {commit}") })
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
            Ok(Stub { text, said: found.said, source: distribution(&found.path, &roots) })
        }
    }
}

/// Past this, a `RECORD` is not read.
const RECORD_LARGEST: u64 = 8 << 20;

/// The distribution that installed `path`, a file under one of `roots`, as `<name> <version>`:
/// the `.dist-info` beside it whose `RECORD` lists the file's top-level package or module.
fn distribution(path: &Path, roots: &[PathBuf]) -> String {
    let Some((root, top)) = roots.iter().find_map(|root| {
        let root = root.canonicalize().ok()?;
        let top = path.strip_prefix(&root).ok()?.components().next()?.as_os_str().to_string_lossy().into_owned();
        Some((root, top))
    }) else {
        return "an unknown distribution".into();
    };
    let mut infos: Vec<PathBuf> = std::fs::read_dir(&root)
        .map(|entries| {
            entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "dist-info")).collect()
        })
        .unwrap_or_default();
    infos.sort();
    let lists = |record: &str| {
        record
            .lines()
            .filter_map(|line| line.split(',').next())
            .any(|file| file.split('/').next() == Some(top.as_str()))
    };
    for info in infos {
        let own = std::fs::symlink_metadata(&info).is_ok_and(|m| m.is_dir());
        let record =
            own.then(|| crate::dependencies::read_regular(&info.join("RECORD"), RECORD_LARGEST).ok().flatten());
        if record.flatten().is_some_and(|record| lists(&record)) {
            let stem = info.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            if let Some((name, version)) = stem.rsplit_once('-') {
                return format!("{name} {version}");
            }
        }
    }
    format!("{top}, which no distribution's RECORD lists")
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
        assert!(text.contains("fn dedent(text: str) -> str ! PyError\n"), "{text}");
        assert!(interface("con", None).unwrap_err().contains("device"));
        assert!(interface("distutils", None).unwrap_err().contains("not in CPython"));
    }

    /// A project whose `lotml.lock` names `py.textwrap` with `stub` and `interface` hashes.
    fn locked_project(name: &str, stub: &str, interface: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lotml-stubs-locked-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let entry = crate::lockfile::Entry {
            name: "py.textwrap".into(),
            source: "typeshed".into(),
            stub: stub.into(),
            interface: interface.into(),
        };
        std::fs::write(dir.join(crate::lockfile::NAME), crate::lockfile::text(&[entry])).unwrap();
        dir
    }

    #[test]
    fn a_stub_that_differs_from_the_lock_is_bound_and_marked_so() {
        let stub = find("textwrap", || panic!("not asked")).unwrap();
        let fresh = bound("textwrap", &stub).unwrap();
        let same = locked_project("same", &crate::lockfile::hash(&stub.text), &crate::lockfile::hash(&fresh));
        assert_eq!(interface_in("textwrap", Some(&same), None).unwrap(), fresh, "the lock's stub: no mark");
        let other = locked_project("other", "sha256:00", &crate::lockfile::hash(&fresh));
        let marked = interface_in("textwrap", Some(&other), None).unwrap();
        assert_eq!(marked, lotml_check::unlocked(&fresh), "bound from the stub found, and marked");
    }

    #[test]
    fn a_kept_interface_the_lock_does_not_record_is_bound_again() {
        let cache = std::env::temp_dir().join(format!("lotml-stubs-locked-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&cache);
        std::fs::create_dir_all(&cache).unwrap();
        let stub = find("textwrap", || panic!("not asked")).unwrap();
        let fresh = bound("textwrap", &stub).unwrap();
        std::fs::write(cache.join(format!("{}.lotmli", key(&stub))), "# tampered\n").unwrap();
        let project = locked_project("cache", &crate::lockfile::hash(&stub.text), &crate::lockfile::hash(&fresh));
        assert_eq!(interface_in("textwrap", Some(&project), Some(&cache)).unwrap(), fresh);
        assert_eq!(interface_in("textwrap", None, Some(&cache)).unwrap(), fresh, "and kept again whole");
        let _ = std::fs::remove_dir_all(&cache);
    }

    #[test]
    fn a_generated_interface_is_kept_by_its_stub_and_read_back() {
        let cache = std::env::temp_dir().join(format!("lotml-stubs-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&cache);
        let first = interface_in("textwrap", None, Some(&cache)).unwrap();
        let kept: Vec<PathBuf> = std::fs::read_dir(&cache).unwrap().flatten().map(|e| e.path()).collect();
        assert_eq!(kept.len(), 1, "one entry, no partial file left: {kept:?}");
        let name = kept[0].file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.contains(&format!("-{}-", env!("CARGO_PKG_VERSION"))) && name.ends_with(".lotmli"), "{name}");
        let binder = &lotml_llvm::sha256::hex_of(lotml_bind::binder::SOURCE.as_bytes())[..16];
        assert!(name.ends_with(&format!("-{binder}.lotmli")), "keyed by the binder that wrote it: {name}");
        assert_eq!(std::fs::read_to_string(&kept[0]).unwrap(), first);
        std::fs::write(&kept[0], "# read from the cache\n").unwrap();
        assert_eq!(interface_in("textwrap", None, Some(&cache)).unwrap(), "# read from the cache\n");
        let _ = std::fs::remove_dir_all(&cache);
    }

    #[test]
    fn the_key_changes_with_the_stub_and_where_it_was_read() {
        let stub = |text: &str, said: &str| Stub { text: text.into(), said: said.into(), source: String::new() };
        let base = key(&stub("def f() -> int: ...\n", "a.pyi"));
        assert_ne!(base, key(&stub("def g() -> int: ...\n", "a.pyi")));
        assert_ne!(base, key(&stub("def f() -> int: ...\n", "b.pyi")));
    }

    #[test]
    fn a_package_s_source_is_the_distribution_whose_record_lists_it() {
        let made = environment(
            "distribution",
            &[
                ("greet-stubs/__init__.pyi", "def hello(name: str) -> str: ...\n"),
                ("greet_stubs-1.2.0.dist-info/RECORD", "greet-stubs/__init__.pyi,sha256=x,10\n"),
                ("other-0.1.dist-info/RECORD", "other/__init__.py,,\n"),
                ("loose/__init__.pyi", "def f() -> int: ...\n"),
                ("odd/__init__.pyi", "def f() -> int: ...\n"),
                ("odd-1.0.dist-info/RECORD/odd/__init__.pyi", ""),
            ],
        );
        let stub = find("greet", || Ok(Environment::Made(made.clone()))).unwrap();
        assert_eq!(stub.source, "greet_stubs 1.2.0");
        let stub = find("odd", || Ok(Environment::Made(made.clone()))).unwrap();
        assert_eq!(stub.source, "odd, which no distribution's RECORD lists", "a RECORD that is no file is not read");
        let stub = find("loose", || Ok(Environment::Made(made))).unwrap();
        assert_eq!(stub.source, "loose, which no distribution's RECORD lists");
        let typeshed = find("textwrap", || panic!("not asked")).unwrap();
        assert!(typeshed.source.starts_with("typeshed ") && typeshed.source.len() == "typeshed ".len() + 12);
    }

    #[test]
    fn a_stub_that_binds_no_function_or_class_counts_as_none() {
        let stub = |text: &str| Stub { text: text.into(), said: "box.pyi".into(), source: String::new() };
        let hidden = stub("class _Box:\n    def size(self) -> int: ...\n@overload\ndef f(x: int) -> int: ...\n");
        assert!(bound("box", &hidden).unwrap_err().contains("binds no function or class"));
        assert!(bound("box", &stub("def size() -> int: ...\n")).unwrap().contains("fn size() -> int ! PyError"));
        let classes = bound("box", &stub("class Box:\n    def size(self) -> int: ...\n")).unwrap();
        assert!(classes.contains("class Box:"), "a class alone is bound (specs/python-classes): {classes}");
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
