//! A project's Python dependencies (specs/python-dependencies,
//! adr:0033-lotml-run-installs-a-project-s-python-dependencies-from-its-uv-lock): its `uv.lock`,
//! checked to name PyPI alone, and the environment `lotml run` and `lotml test` make from it.

use std::io::Read;
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
/// dependency, or when its root is one another user could have put a lock in, which is ignored and
/// said; an error for a lock lotml does not install from, and for dependencies declared with no
/// lock to install them from.
pub fn locked(root: &Path) -> Result<Option<Locked>, String> {
    let lock_path = root.join("uv.lock");
    if !trusted(root, &lock_path) {
        eprintln!(
            "lotml: ignoring the project files in {}: another user may write that directory, or it is a drive's root",
            root.display()
        );
        return Ok(None);
    }
    let pyproject = read_bounded(&root.join("pyproject.toml"))?;
    let Some(lock) = read_bounded(&lock_path)? else {
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
    check_lock(&lock, project_name(&pyproject).as_deref())?;
    Ok(Some(Locked { lock, pyproject }))
}

/// Whether the project files at `root` are the user's to install from: `root` no drive's root and,
/// on Unix, a directory nobody else may write, its `uv.lock`, when there is one, owned by `root`'s
/// owner — so a lock another user left in a shared directory above a program is never installed.
fn trusted(root: &Path, lock: &Path) -> bool {
    let Ok(root) = root.canonicalize() else { return false };
    if root.parent().is_none() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let Ok(owner) = root.metadata() else { return false };
        if owner.mode() & 0o002 != 0 {
            return false;
        }
        if let Ok(made) = lock.metadata()
            && made.uid() != owner.uid()
        {
            return false;
        }
    }
    let _ = lock;
    true
}

/// The text of the regular file at `path`, read no further than [`LARGEST`]; `None` when there is
/// no file there.
fn read_bounded(path: &Path) -> Result<Option<String>, String> {
    read_regular(path, LARGEST)
}

/// The text of the regular file at `path`, read no further than `largest` bytes; `None` when there
/// is no file there. A link is followed, but what it leads to must be a regular file, so a pipe or
/// a device never blocks the read.
pub fn read_regular(path: &Path, largest: u64) -> Result<Option<String>, String> {
    let cannot = |e: std::io::Error| format!("cannot read {}: {e}", path.display());
    match std::fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(cannot(e)),
        Ok(meta) if !meta.is_file() => return Err(format!("{} is not a regular file", path.display())),
        Ok(_) => {}
    }
    let file = std::fs::File::open(path).map_err(cannot)?;
    if !file.metadata().map_err(cannot)?.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    let mut text = String::new();
    file.take(largest + 1).read_to_string(&mut text).map_err(cannot)?;
    if text.len() as u64 > largest {
        return Err(format!("{} is past the {largest} bytes lotml reads", path.display()));
    }
    Ok(Some(text))
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
/// to make it. An environment is made in a directory of its own and moved into place whole, so two
/// runs never make one in the same place and a stopped run leaves none half made in use.
pub fn environment(root: &Path, locked: &Locked, base: &str, make: Option<Make<'_>>) -> Result<PathBuf, String> {
    let name = key(locked, base);
    let dir = root.join(&name);
    let python = interpreter(&dir.join("environment"));
    if dir.join(COMPLETE).is_file() && python.is_file() {
        return Ok(python);
    }
    let Some(make) = make else {
        return Err(
            "the project's uv.lock has no environment made yet, and lotml installs nothing here; run `lotml run` or `lotml test` once without --offline"
                .into(),
        );
    };
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let building = root.join(format!("{name}.{}.{nanos}", std::process::id()));
    let made = build(&building, locked, make);
    if let Err(why) = made {
        let _ = std::fs::remove_dir_all(&building);
        return Err(why);
    }
    if std::fs::rename(&building, &dir).is_err() {
        if dir.join(COMPLETE).is_file() && python.is_file() {
            let _ = std::fs::remove_dir_all(&building);
            return Ok(python);
        }
        let failed = |e: std::io::Error| format!("cannot put the environment in {}: {e}", dir.display());
        std::fs::remove_dir_all(&dir).map_err(failed)?;
        std::fs::rename(&building, &dir).map_err(failed)?;
    }
    Ok(python)
}

/// The directory of the user's own that environments are made under, if lotml has one.
pub fn environments() -> Option<PathBuf> {
    lotml_llvm::cache::user_root()
        .map(|root| root.join("python-environments"))
        .filter(|root| lotml_llvm::cache::private_directory(root).is_ok())
}

/// The environment already made from `locked` under `root`, over whichever interpreter, found
/// without running Python (specs/bind-on-import/ R1.2): a whole one, named by a key, whose copies
/// of the lock and the manifest are the project's, the newest first.
pub fn made(root: &Path, locked: &Locked) -> Option<PathBuf> {
    let same = |path: PathBuf, text: &str| std::fs::read_to_string(path).is_ok_and(|read| read == text);
    std::fs::read_dir(root)
        .ok()?
        .flatten()
        .filter(|entry| {
            entry.file_name().to_str().is_some_and(|n| n.len() == 32 && n.bytes().all(|b| b.is_ascii_hexdigit()))
        })
        .map(|entry| entry.path())
        .filter(|dir| interpreter(&dir.join("environment")).is_file())
        .filter(|dir| same(dir.join("project").join("uv.lock"), &locked.lock))
        .filter(|dir| same(dir.join("project").join("pyproject.toml"), &locked.pyproject))
        .filter_map(|dir| Some((std::fs::metadata(dir.join(COMPLETE)).ok()?.modified().ok()?, dir.join("environment"))))
        .max_by_key(|(when, _)| *when)
        .map(|(_, environment)| environment)
}

/// An environment made whole in `dir`: the two files copied, `make` run, the mark written last.
fn build(dir: &Path, locked: &Locked, make: Make<'_>) -> Result<(), String> {
    let failed = |e: std::io::Error| format!("cannot make {}: {e}", dir.display());
    lotml_llvm::cache::private_directory(dir).map_err(failed)?;
    let project = dir.join("project");
    std::fs::create_dir(&project).map_err(failed)?;
    std::fs::write(project.join("pyproject.toml"), &locked.pyproject).map_err(failed)?;
    std::fs::write(project.join("uv.lock"), &locked.lock).map_err(failed)?;
    let environment = dir.join("environment");
    make(&project, &environment)?;
    if !interpreter(&environment).is_file() {
        return Err(format!("uv made no interpreter in {}", environment.display()));
    }
    std::fs::write(dir.join(COMPLETE), "").map_err(failed)
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
/// registry, every artifact a file on PyPI's host with a hash, and the project itself, named
/// `project`, as its own source with no file of its own; else why not, naming the package.
pub fn check_lock(text: &str, project: Option<&str>) -> Result<(), String> {
    let lock: Table = text.parse().map_err(|e: toml::de::Error| {
        let line = e.span().map_or(0, |span| text[..span.start.min(text.len())].matches('\n').count() + 1);
        format!("uv.lock does not parse, at line {line}")
    })?;
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
            ("virtual" | "editable", Some(".")) => {
                let own = project.is_some_and(|p| normalized(p) == normalized(name));
                let files = ["sdist", "wheels", "url", "path"].iter().any(|k| package.get(k).is_some());
                if !own || files {
                    return refuse("comes from the project's own directory and is not the project".into());
                }
                continue;
            }
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

/// A distribution's name as PEP 503 compares it: lower case, each run of `-`, `_` and `.` one `-`.
fn normalized(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if matches!(c, '-' | '_' | '.') {
            if !out.ends_with('-') {
                out.push('-');
            }
        } else {
            out.extend(c.to_lowercase());
        }
    }
    out
}

/// The `[project] name` of a `pyproject.toml`.
fn project_name(pyproject: &str) -> Option<String> {
    pyproject.parse::<Table>().ok()?.get("project")?.get("name")?.as_str().map(str::to_string)
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

    fn check(text: &str) -> Result<(), String> {
        check_lock(text, Some("app"))
    }

    const PROJECT: &str = "[[package]]\nname = \"app\"\nversion = \"0.1.0\"\nsource = { virtual = \".\" }\n";

    fn package(source: &str, artifacts: &str) -> String {
        format!(
            "version = 1\n{PROJECT}\n[[package]]\nname = \"six\"\nversion = \"1.17.0\"\nsource = {source}\n{artifacts}\n"
        )
    }

    const WHEEL: &str = "wheels = [\n    { url = \"https://files.pythonhosted.org/packages/six-1.17.0-py2.py3-none-any.whl\", hash = \"sha256:4721f391ed90541fddacab5acf947aa0d3dc7d27b2e1e8eda2be8970586c3274\" },\n]";

    #[test]
    fn a_lock_of_pypi_packages_and_the_project_itself_is_installed() {
        assert_eq!(check(&package(&format!("{{ registry = \"{REGISTRY}\" }}"), WHEEL)), Ok(()));
        let editable = PROJECT.replace("virtual", "editable");
        assert_eq!(check(&format!("version = 1\n{editable}")), Ok(()));
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
            let why = check(&package(source, WHEEL)).unwrap_err();
            assert!(why.contains("\"six\" comes from the"), "{source}: {why}");
        }
        let two = package(&format!("{{ registry = \"{REGISTRY}\", git = \"x\" }}"), WHEEL);
        assert!(check(&two).unwrap_err().contains("source lotml does not read"));
    }

    #[test]
    fn a_file_off_pypi_s_host_or_without_a_hash_is_refused() {
        let registry = format!("{{ registry = \"{REGISTRY}\" }}");
        let elsewhere = WHEEL.replace("https://files.pythonhosted.org/", "https://evil.example/");
        assert!(check(&package(&registry, &elsewhere)).unwrap_err().contains("not on PyPI's host"));
        let unhashed = "sdist = { url = \"https://files.pythonhosted.org/packages/six-1.17.0.tar.gz\" }";
        assert!(check(&package(&registry, unhashed)).unwrap_err().contains("without a hash"));
    }

    #[test]
    fn a_lock_that_is_not_one_is_refused() {
        assert!(check("version = [").unwrap_err().contains("does not parse"));
        assert!(check("version = 1\n").unwrap_err().contains("no [[package]]"));
        let sourceless = "version = 1\n[[package]]\nname = \"six\"\n";
        assert!(check(sourceless).unwrap_err().contains("names no source"));
    }

    #[test]
    fn a_name_with_a_control_character_is_shown_escaped() {
        let lock = "version = 1\n[[package]]\nname = \"six\\u001b[2J\"\nsource = { git = \"x\" }\n";
        let why = check(lock).unwrap_err();
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

    #[test]
    fn a_made_environment_is_found_by_its_lock_and_manifest_without_running_python() {
        let root = scratch("found-made");
        let one = Locked { lock: "lock one".into(), pyproject: "manifest".into() };
        let other = Locked { lock: "lock two".into(), pyproject: "manifest".into() };
        assert!(made(&root, &one).is_none(), "nothing made yet");
        let calls = std::cell::Cell::new(0);
        environment(&root, &one, "/py/3.13", Some(&fake_make(&calls))).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        environment(&root, &one, "/py/3.14", Some(&fake_make(&calls))).unwrap();
        environment(&root, &other, "/py/3.14", Some(&fake_make(&calls))).unwrap();
        let found = made(&root, &one).unwrap();
        assert_eq!(found, root.join(key(&one, "/py/3.14")).join("environment"), "the newest of the lock's");
        assert!(made(&root, &Locked { lock: "lock one".into(), pyproject: "changed".into() }).is_none());
        std::fs::remove_file(root.join(key(&one, "/py/3.14")).join(COMPLETE)).unwrap();
        let fallback = made(&root, &one).unwrap();
        assert_eq!(fallback, root.join(key(&one, "/py/3.13")).join("environment"), "a half-made one is passed over");
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
    fn only_the_project_itself_may_come_from_its_own_directory_and_with_no_file() {
        let other = package("{ editable = \".\" }", "");
        assert!(check(&other).unwrap_err().contains("\"six\" comes from the project's own directory"));
        let with_files = format!("version = 1\n{PROJECT}{WHEEL}\n");
        assert!(check(&with_files).unwrap_err().contains("\"app\" comes from the project's own directory"));
        let renamed = "version = 1\n[[package]]\nname = \"my-app\"\nsource = { virtual = \".\" }\n";
        assert_eq!(check_lock(renamed, Some("My_App")), Ok(()), "names compare as PEP 503 has them");
        assert!(check_lock(renamed, None).is_err(), "no project name, no project entry");
    }

    #[test]
    fn a_lock_that_does_not_parse_says_where_and_quotes_none_of_it() {
        let why = check("version = 1\nsecret = \"sk-abcdef\n").unwrap_err();
        assert!(why.contains("line 2") && !why.contains("sk-abcdef"), "{why}");
    }

    #[test]
    fn a_lock_that_is_not_a_regular_file_is_not_read() {
        let root = scratch("not-a-file");
        std::fs::write(root.join("pyproject.toml"), "[project]\nname = \"app\"\n").unwrap();
        std::fs::create_dir(root.join("uv.lock")).unwrap();
        assert!(locked(&root).err().unwrap().contains("not a regular file"));
    }

    #[cfg(unix)]
    #[test]
    fn a_lock_in_a_directory_anyone_may_write_is_ignored() {
        use std::os::unix::fs::PermissionsExt;
        let root = scratch("shared");
        std::fs::write(root.join("pyproject.toml"), "[project]\nname = \"app\"\n").unwrap();
        std::fs::write(root.join("uv.lock"), lock_of_six()).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o1777)).unwrap();
        assert!(locked(&root).unwrap().is_none());
    }

    #[test]
    fn an_environment_is_put_in_place_whole_and_a_failed_one_leaves_nothing() {
        let root = scratch("whole");
        let locked = Locked { lock: lock_of_six(), pyproject: String::new() };
        let failing = |_: &Path, _: &Path| -> Result<(), String> { Err("uv could not install".into()) };
        assert!(environment(&root, &locked, "/py", Some(&failing)).is_err());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0, "nothing half made is left");
        let calls = std::cell::Cell::new(0);
        let python = environment(&root, &locked, "/py", Some(&fake_make(&calls))).unwrap();
        assert!(python.starts_with(root.join(key(&locked, "/py"))));
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    }

    #[test]
    fn dependencies_declared_need_a_lock() {
        assert!(declares_dependencies("[project]\nname = \"app\"\ndependencies = [\"six\"]\n"));
        assert!(!declares_dependencies("[project]\nname = \"app\"\ndependencies = []\n"));
        assert!(!declares_dependencies("[project]\nname = \"app\"\n"));
        assert!(!declares_dependencies("not toml ["));
    }
}
