//! A project's Python dependencies (specs/python-dependencies,
//! adr:0033-lotml-run-installs-a-project-s-python-dependencies-from-its-uv-lock): its `uv.lock`,
//! checked to name PyPI alone, and the environment `lotml run` and `lotml test` make from it.

use std::path::{Path, PathBuf};

use toml::{Table, Value};

/// Past this, a `uv.lock` or `pyproject.toml` is not read.
const LARGEST: u64 = 64 << 20;

/// The file whose presence says an environment was made whole.
const COMPLETE: &str = "lotml-complete";

/// A project's `uv.lock` and `pyproject.toml`, the lock checked to name PyPI alone.
pub struct Locked {
    pub lock: String,
    pub pyproject: String,
}

/// The lock of the project at `root`, checked: `None` when it has no `uv.lock` and declares no
/// dependency; an error for a lock lotml does not install from, and for dependencies declared
/// with no lock to install them from.
pub fn locked(root: &Path) -> Result<Option<Locked>, String> {
    let read = |name: &str| -> Result<Option<String>, String> {
        let path = root.join(name);
        match std::fs::metadata(&path) {
            Err(_) => Ok(None),
            Ok(meta) if meta.len() > LARGEST => {
                Err(format!("{} is past the {LARGEST} bytes lotml reads", path.display()))
            }
            Ok(_) => {
                std::fs::read_to_string(&path).map(Some).map_err(|e| format!("cannot read {}: {e}", path.display()))
            }
        }
    };
    let pyproject = read("pyproject.toml")?;
    let Some(lock) = read("uv.lock")? else {
        if pyproject.as_deref().is_some_and(declares_dependencies) {
            return Err(format!(
                "{} declares dependencies and holds no uv.lock to install them from; write it with `uv lock`",
                root.join("pyproject.toml").display()
            ));
        }
        return Ok(None);
    };
    let Some(pyproject) = pyproject else {
        return Err(format!("{} holds a uv.lock and no pyproject.toml", root.display()));
    };
    check_lock(&lock)?;
    Ok(Some(Locked { lock, pyproject }))
}

/// The environment's name: the SHA-256 of the lock, the manifest and the interpreter it is made
/// over, each closed by a zero byte.
pub fn key(locked: &Locked, base: &str) -> String {
    let mut bytes = Vec::new();
    for part in [locked.lock.as_str(), locked.pyproject.as_str(), base] {
        bytes.extend_from_slice(part.as_bytes());
        bytes.push(0);
    }
    lotml_llvm::sha256::hex_of(&bytes)[..32].to_string()
}

/// How the environment is made: `uv sync` of the project copied to the first directory into the
/// second.
pub type Make<'a> = &'a dyn Fn(&Path, &Path) -> Result<(), String>;

/// The interpreter of the environment of `locked` over `base`, under `root`: the one already made
/// whole there, else one `make` makes from copies of the two files; without `make` — offline, or
/// for the MCP server, the grader and the harness — a missing environment is an error saying how
/// to make it. A directory `make` left unfinished is removed and made again.
pub fn environment(root: &Path, locked: &Locked, base: &str, make: Option<Make<'_>>) -> Result<PathBuf, String> {
    let dir = root.join(key(locked, base));
    let environment = dir.join("environment");
    let python = interpreter(&environment);
    if dir.join(COMPLETE).is_file() && python.is_file() {
        return Ok(python);
    }
    let Some(make) = make else {
        return Err(
            "the project's uv.lock has no environment made yet, and lotml installs nothing here; run `lotml run` or `lotml test` once without --offline"
                .into(),
        );
    };
    let failed = |e: std::io::Error| format!("cannot make {}: {e}", dir.display());
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(failed)?;
    }
    lotml_llvm::cache::private_directory(&dir).map_err(failed)?;
    let project = dir.join("project");
    std::fs::create_dir(&project).map_err(failed)?;
    std::fs::write(project.join("pyproject.toml"), &locked.pyproject).map_err(failed)?;
    std::fs::write(project.join("uv.lock"), &locked.lock).map_err(failed)?;
    make(&project, &environment)?;
    if !python.is_file() {
        return Err(format!("uv made no interpreter in {}", environment.display()));
    }
    std::fs::write(dir.join(COMPLETE), "").map_err(failed)?;
    Ok(python)
}

/// The interpreter a virtual environment holds.
fn interpreter(environment: &Path) -> PathBuf {
    if cfg!(windows) { environment.join("Scripts").join("python.exe") } else { environment.join("bin").join("python") }
}

/// The one index a lock may install from.
pub const REGISTRY: &str = "https://pypi.org/simple";

/// Where PyPI serves the files its index lists.
pub const FILES: &str = "https://files.pythonhosted.org/";

/// Whether the text of `uv.lock` names only what lotml installs: every package from PyPI's
/// registry, save the project's own entry, and every artifact a file on PyPI's host with a hash;
/// else why not, naming the package.
pub fn check_lock(text: &str) -> Result<(), String> {
    let lock: Table = text.parse().map_err(|e| format!("uv.lock does not parse: {e}"))?;
    let packages = lock.get("package").and_then(Value::as_array).ok_or("uv.lock lists no [[package]]")?;
    for package in packages {
        let name = package.get("name").and_then(Value::as_str).unwrap_or("a package without a name");
        let refuse = |why: String| Err(format!("uv.lock: {name:?} {why}; lotml installs from PyPI alone (adr:0033)"));
        let Some(source) = package.get("source").and_then(Value::as_table) else {
            return refuse("names no source".into());
        };
        let mut entries = source.iter();
        let (Some((kind, value)), None) = (entries.next(), entries.next()) else {
            return refuse("names a source lotml does not read".into());
        };
        match (kind.as_str(), value.as_str()) {
            ("registry", Some(REGISTRY)) => {}
            ("virtual" | "editable", Some(".")) => continue,
            (kind, _) => return refuse(format!("comes from the {kind} {value}")),
        }
        let sdist = package.get("sdist").into_iter();
        let wheels = package.get("wheels").and_then(Value::as_array).into_iter().flatten();
        for artifact in sdist.chain(wheels) {
            let url = artifact.get("url").and_then(Value::as_str);
            let hash = artifact.get("hash").and_then(Value::as_str).filter(|h| !h.is_empty());
            match (url, hash) {
                (Some(url), Some(_)) if url.starts_with(FILES) => {}
                (Some(url), Some(_)) => return refuse(format!("names a file at {url:?}, not on PyPI's host")),
                (Some(url), None) => return refuse(format!("names a file without a hash, {url:?}")),
                (None, _) => return refuse("names a file without a URL".into()),
            }
        }
    }
    Ok(())
}

/// Whether a `pyproject.toml` declares dependencies, which need a `uv.lock` to be installed.
pub fn declares_dependencies(pyproject: &str) -> bool {
    pyproject
        .parse::<Table>()
        .ok()
        .and_then(|t| t.get("project")?.get("dependencies")?.as_array().map(|d| !d.is_empty()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROJECT: &str = "[[package]]\nname = \"app\"\nversion = \"0.1.0\"\nsource = { virtual = \".\" }\n";

    fn package(source: &str, artifacts: &str) -> String {
        format!(
            "version = 1\n{PROJECT}\n[[package]]\nname = \"six\"\nversion = \"1.17.0\"\nsource = {source}\n{artifacts}\n"
        )
    }

    const WHEEL: &str = "wheels = [\n    { url = \"https://files.pythonhosted.org/packages/six-1.17.0-py2.py3-none-any.whl\", hash = \"sha256:4721f391ed90541fddacab5acf947aa0d3dc7d27b2e1e8eda2be8970586c3274\" },\n]";

    #[test]
    fn a_lock_of_pypi_packages_and_the_project_itself_is_installed() {
        assert_eq!(check_lock(&package(&format!("{{ registry = \"{REGISTRY}\" }}"), WHEEL)), Ok(()));
        let editable = PROJECT.replace("virtual", "editable");
        assert_eq!(check_lock(&format!("version = 1\n{editable}")), Ok(()));
    }

    #[test]
    fn a_package_from_anywhere_but_pypi_s_registry_is_refused_by_name() {
        for source in [
            "{ registry = \"https://evil.example/simple\" }",
            "{ git = \"https://github.com/x/six?rev=1#abc\" }",
            "{ url = \"https://evil.example/six.whl\" }",
            "{ path = \"../six\" }",
            "{ directory = \"vendor/six\" }",
            "{ editable = \"vendor/six\" }",
            "{ virtual = \"../elsewhere\" }",
        ] {
            let why = check_lock(&package(source, WHEEL)).unwrap_err();
            assert!(why.contains("\"six\" comes from the"), "{source}: {why}");
        }
        let two = package(&format!("{{ registry = \"{REGISTRY}\", git = \"x\" }}"), WHEEL);
        assert!(check_lock(&two).unwrap_err().contains("source lotml does not read"));
    }

    #[test]
    fn a_file_off_pypi_s_host_or_without_a_hash_is_refused() {
        let registry = format!("{{ registry = \"{REGISTRY}\" }}");
        let elsewhere = WHEEL.replace("https://files.pythonhosted.org/", "https://evil.example/");
        assert!(check_lock(&package(&registry, &elsewhere)).unwrap_err().contains("not on PyPI's host"));
        let unhashed = "sdist = { url = \"https://files.pythonhosted.org/packages/six-1.17.0.tar.gz\" }";
        assert!(check_lock(&package(&registry, unhashed)).unwrap_err().contains("without a hash"));
    }

    #[test]
    fn a_lock_that_is_not_one_is_refused() {
        assert!(check_lock("version = [").unwrap_err().contains("does not parse"));
        assert!(check_lock("version = 1\n").unwrap_err().contains("no [[package]]"));
        let sourceless = "version = 1\n[[package]]\nname = \"six\"\n";
        assert!(check_lock(sourceless).unwrap_err().contains("names no source"));
    }

    #[test]
    fn a_name_with_a_control_character_is_shown_escaped() {
        let lock = "version = 1\n[[package]]\nname = \"six\\u001b[2J\"\nsource = { git = \"x\" }\n";
        let why = check_lock(lock).unwrap_err();
        assert!(!why.contains('\u{1b}'), "{why:?}");
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("lotml-dependencies").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn lock_of_six() -> String {
        package(&format!("{{ registry = \"{REGISTRY}\" }}"), WHEEL)
    }

    #[test]
    fn a_project_without_a_lock_has_none_unless_it_declares_dependencies() {
        let root = scratch("no-lock");
        assert!(locked(&root).unwrap().is_none());
        std::fs::write(root.join("pyproject.toml"), "[project]\nname = \"app\"\ndependencies = [\"six\"]\n").unwrap();
        assert!(locked(&root).err().unwrap().contains("`uv lock`"));
        std::fs::write(root.join("uv.lock"), lock_of_six()).unwrap();
        assert!(locked(&root).unwrap().is_some());
        std::fs::write(root.join("uv.lock"), package("{ git = \"x\" }", WHEEL)).unwrap();
        assert!(locked(&root).err().unwrap().contains("\"six\" comes from the git"), "the lock is checked first");
    }

    #[test]
    fn the_key_changes_with_the_lock_the_manifest_and_the_interpreter() {
        let one = Locked { lock: "a".into(), pyproject: "b".into() };
        let base = key(&one, "/py");
        assert_eq!(base.len(), 32);
        assert_ne!(base, key(&Locked { lock: "a2".into(), pyproject: "b".into() }, "/py"));
        assert_ne!(base, key(&Locked { lock: "a".into(), pyproject: "b2".into() }, "/py"));
        assert_ne!(base, key(&one, "/other"));
        assert_ne!(
            key(&Locked { lock: "ab".into(), pyproject: "".into() }, ""),
            key(&Locked { lock: "a".into(), pyproject: "b".into() }, "")
        );
    }

    /// A `make` that writes the interpreter where uv would, counting its calls.
    fn fake_make(calls: &std::cell::Cell<u32>) -> impl Fn(&Path, &Path) -> Result<(), String> + '_ {
        move |project: &Path, environment: &Path| {
            calls.set(calls.get() + 1);
            assert!(project.join("uv.lock").is_file() && project.join("pyproject.toml").is_file());
            let python = interpreter(environment);
            std::fs::create_dir_all(python.parent().unwrap()).unwrap();
            std::fs::write(python, "").unwrap();
            Ok(())
        }
    }

    #[test]
    fn an_environment_is_made_once_and_then_used_as_it_is_offline_too() {
        let root = scratch("made");
        let locked = Locked { lock: lock_of_six(), pyproject: "[project]\nname = \"app\"\n".into() };
        let calls = std::cell::Cell::new(0);
        let make = fake_make(&calls);
        let python = environment(&root, &locked, "/py", Some(&make)).unwrap();
        assert!(python.is_file());
        assert_eq!(environment(&root, &locked, "/py", Some(&make)).unwrap(), python);
        assert_eq!(calls.get(), 1, "a complete environment is not made again");
        assert_eq!(environment(&root, &locked, "/py", None).unwrap(), python, "offline uses it");
    }

    #[test]
    fn without_make_a_missing_environment_says_how_to_make_it() {
        let root = scratch("offline");
        let locked = Locked { lock: lock_of_six(), pyproject: String::new() };
        let why = environment(&root, &locked, "/py", None).unwrap_err();
        assert!(why.contains("installs nothing here") && why.contains("without --offline"), "{why}");
    }

    #[test]
    fn an_environment_left_unfinished_is_made_again_and_a_failed_make_is_not_marked() {
        let root = scratch("unfinished");
        let locked = Locked { lock: lock_of_six(), pyproject: String::new() };
        let failing = |_: &Path, environment: &Path| -> Result<(), String> {
            std::fs::create_dir_all(environment).unwrap();
            Err("uv could not install".into())
        };
        assert!(environment(&root, &locked, "/py", Some(&failing)).is_err());
        assert!(environment(&root, &locked, "/py", None).is_err(), "a failed make is not complete");
        let calls = std::cell::Cell::new(0);
        assert!(environment(&root, &locked, "/py", Some(&fake_make(&calls))).is_ok());
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn dependencies_declared_need_a_lock() {
        assert!(declares_dependencies("[project]\nname = \"app\"\ndependencies = [\"six\"]\n"));
        assert!(!declares_dependencies("[project]\nname = \"app\"\ndependencies = []\n"));
        assert!(!declares_dependencies("[project]\nname = \"app\"\n"));
        assert!(!declares_dependencies("not toml ["));
    }
}
