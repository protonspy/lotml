//! A project's Python dependencies (specs/python-dependencies,
//! adr:0033-lotml-run-installs-a-project-s-python-dependencies-from-its-uv-lock): its `uv.lock`,
//! checked to name PyPI alone, and the environment `lotml run` and `lotml test` make from it.

use toml::{Table, Value};

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

    #[test]
    fn dependencies_declared_need_a_lock() {
        assert!(declares_dependencies("[project]\nname = \"app\"\ndependencies = [\"six\"]\n"));
        assert!(!declares_dependencies("[project]\nname = \"app\"\ndependencies = []\n"));
        assert!(!declares_dependencies("[project]\nname = \"app\"\n"));
        assert!(!declares_dependencies("not toml ["));
    }
}
