//! What `lotml bind` reads a module from when typeshed has no stub for it (plans/bind-sources.md
//! 1.1, adr:0032): the packages of the project's environment, in PEP 561's order — a stub-only
//! `<package>-stubs`, then a `.pyi` the package ships, then its own `.py` when it carries
//! `py.typed`. Each is read as a file; no module is imported.

use std::path::{Path, PathBuf};

/// Past this, a file is no stub lotml reads.
pub const LARGEST: u64 = 8 << 20;

/// A module's source in an environment, and what it is, as the interface says it.
#[derive(Debug, PartialEq, Eq)]
pub struct Source {
    pub path: PathBuf,
    pub said: String,
}

/// The `site-packages` directories of the virtual environment `venv`: `Lib/site-packages` on
/// Windows, `lib/python3.*/site-packages` elsewhere; each canonical, and only those inside `venv`
/// once links are followed.
pub fn site_packages(venv: &Path) -> Vec<PathBuf> {
    let Ok(home) = venv.canonicalize() else { return Vec::new() };
    candidates(&home).into_iter().filter_map(|p| p.canonicalize().ok()).filter(|p| p.starts_with(&home)).collect()
}

/// Where a virtual environment keeps `site-packages`, before links are followed.
fn candidates(venv: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let windows = venv.join("Lib").join("site-packages");
    if cfg!(windows) && windows.is_dir() {
        found.push(windows);
    }
    if let Ok(entries) = std::fs::read_dir(venv.join("lib")) {
        let mut versions: Vec<PathBuf> = entries
            .flatten()
            .filter(|e| e.file_name().to_str().is_some_and(|n| n.starts_with("python")))
            .map(|e| e.path().join("site-packages"))
            .filter(|p| p.is_dir())
            .collect();
        versions.sort();
        found.extend(versions);
    }
    found
}

/// The first source of `module`, a dotted name of identifiers, under `roots` in PEP 561's order;
/// a file whose links lead out of its root is passed over, and one past [`LARGEST`] is an error.
/// A source is named by the path built from the module's identifiers, never by what a link leads
/// to, so no file name reaches the interface.
pub fn find(module: &str, roots: &[PathBuf]) -> Result<Option<Source>, String> {
    let parts: Vec<&str> = module.split('.').collect();
    let (top, rest) = parts.split_first().ok_or("no module named")?;
    let stubs = format!("{top}-stubs");
    for root in roots {
        let Ok(root) = root.canonicalize() else { continue };
        let package = root.join(top);
        let candidates = [
            (module_file(&root.join(&stubs), rest, "pyi"), format!("the stub-only package `{stubs}`")),
            (module_file(&package, rest, "pyi"), format!("the stub the package `{top}` ships")),
            (lone_file(&root, top, rest, "pyi"), format!("the stub the module `{top}` ships")),
        ];
        let typed = package.join("py.typed").is_file();
        let annotated = typed.then(|| {
            (
                module_file(&package, rest, "py"),
                format!("the package `{top}`'s own annotated source (it ships py.typed)"),
            )
        });
        for (candidates, said) in candidates.into_iter().chain(annotated) {
            for candidate in candidates {
                if let Some(path) = inside(&root, &candidate)? {
                    let shown =
                        candidate.strip_prefix(&root).unwrap_or(&candidate).display().to_string().replace('\\', "/");
                    return Ok(Some(Source { path, said: format!("{shown}, {said}") }));
                }
            }
        }
    }
    Ok(None)
}

/// `dir/<rest>.<ext>` and `dir/<rest>/__init__.<ext>`: a module, or a package, inside `dir`.
fn module_file(dir: &Path, rest: &[&str], ext: &str) -> Vec<PathBuf> {
    let init = rest.iter().fold(dir.to_path_buf(), |path, part| path.join(part)).join(format!("__init__.{ext}"));
    match rest.split_last() {
        Some((last, inner)) => {
            let module =
                inner.iter().fold(dir.to_path_buf(), |path, part| path.join(part)).join(format!("{last}.{ext}"));
            vec![module, init]
        }
        None => vec![init],
    }
}

/// A module that is one file at the root, `<top>.<ext>`, which only a top-level name can be.
fn lone_file(root: &Path, top: &str, rest: &[&str], ext: &str) -> Vec<PathBuf> {
    if rest.is_empty() { vec![root.join(format!("{top}.{ext}"))] } else { Vec::new() }
}

/// `candidate` as a file read from its canonical path, which must lie under `root`.
fn inside(root: &Path, candidate: &Path) -> Result<Option<PathBuf>, String> {
    let Ok(path) = candidate.canonicalize() else { return Ok(None) };
    if !path.starts_with(root) || !path.is_file() {
        return Ok(None);
    }
    let size = path.metadata().map(|m| m.len()).unwrap_or(0);
    if size > LARGEST {
        return Err(format!("{} is {size} bytes, past the {LARGEST} a stub lotml reads may be", path.display()));
    }
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str, files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join("lotml-sources").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        for file in files {
            let path = dir.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }
        std::fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    fn found(module: &str, root: &Path) -> Option<String> {
        find(module, &[root.to_path_buf()]).unwrap().map(|s| s.said)
    }

    #[test]
    fn a_stub_only_package_comes_before_the_stub_the_package_ships() {
        let root = scratch("order", &["pkg-stubs/__init__.pyi", "pkg/__init__.pyi", "pkg/py.typed", "pkg/__init__.py"]);
        assert_eq!(found("pkg", &root).as_deref(), Some("pkg-stubs/__init__.pyi, the stub-only package `pkg-stubs`"));
        std::fs::remove_dir_all(root.join("pkg-stubs")).unwrap();
        assert_eq!(found("pkg", &root).as_deref(), Some("pkg/__init__.pyi, the stub the package `pkg` ships"));
        std::fs::remove_file(root.join("pkg").join("__init__.pyi")).unwrap();
        assert_eq!(
            found("pkg", &root).as_deref(),
            Some("pkg/__init__.py, the package `pkg`'s own annotated source (it ships py.typed)")
        );
    }

    #[test]
    fn a_submodule_is_a_file_or_a_package_inside_its_package() {
        let root = scratch("inner", &["pkg/inner/mod.pyi", "pkg/inner/sub/__init__.pyi"]);
        assert_eq!(
            found("pkg.inner.mod", &root).as_deref(),
            Some("pkg/inner/mod.pyi, the stub the package `pkg` ships")
        );
        assert_eq!(
            found("pkg.inner.sub", &root).as_deref(),
            Some("pkg/inner/sub/__init__.pyi, the stub the package `pkg` ships")
        );
        let lone = scratch("lone", &["six.pyi", "six.py"]);
        assert_eq!(found("six", &lone).as_deref(), Some("six.pyi, the stub the module `six` ships"));
    }

    #[test]
    fn source_without_py_typed_is_not_read() {
        let root = scratch("untyped", &["pkg/__init__.py", "solo.py"]);
        assert_eq!(found("pkg", &root), None, "an untyped package's annotations are no promise");
        assert_eq!(found("solo", &root), None);
    }

    #[test]
    fn the_roots_are_searched_in_order_and_a_missing_one_is_passed_over() {
        let first = scratch("first", &["pkg/__init__.pyi"]);
        let second = scratch("second", &["pkg-stubs/__init__.pyi"]);
        let source = find("pkg", &[first.join("missing"), first.clone(), second]).unwrap().unwrap();
        assert_eq!(source.path, first.join("pkg").join("__init__.pyi"));
    }

    #[test]
    fn a_file_past_the_limit_is_an_error_not_a_binding() {
        let root = scratch("large", &[]);
        std::fs::create_dir_all(root.join("big")).unwrap();
        let file = std::fs::File::create(root.join("big").join("__init__.pyi")).unwrap();
        file.set_len(LARGEST + 1).unwrap();
        assert!(find("big", &[root]).unwrap_err().contains("past the"));
    }

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_environment_is_passed_over() {
        let outside = scratch("outside", &["secret.pyi"]);
        let root = scratch("linked", &[]);
        std::fs::create_dir_all(root.join("pkg")).unwrap();
        std::os::unix::fs::symlink(outside.join("secret.pyi"), root.join("pkg").join("__init__.pyi")).unwrap();
        assert_eq!(found("pkg", &root), None);
    }

    #[test]
    fn site_packages_are_found_where_a_virtual_environment_keeps_them() {
        let venv = scratch("venv", &[]);
        let posix = venv.join("lib").join("python3.14").join("site-packages");
        std::fs::create_dir_all(&posix).unwrap();
        let mut expected = vec![posix];
        if cfg!(windows) {
            let windows = venv.join("Lib").join("site-packages");
            std::fs::create_dir_all(&windows).unwrap();
            expected.insert(0, windows);
        }
        let expected: Vec<PathBuf> = expected.iter().map(|p| p.canonicalize().unwrap()).collect();
        assert_eq!(site_packages(&venv), expected);
    }

    #[cfg(unix)]
    #[test]
    fn site_packages_a_link_leads_out_of_the_environment_are_not_read() {
        let outside = scratch("elsewhere", &["pkg/__init__.pyi"]);
        let venv = scratch("venv-linked", &[]);
        std::fs::create_dir_all(venv.join("lib").join("python3.14")).unwrap();
        std::os::unix::fs::symlink(&outside, venv.join("lib").join("python3.14").join("site-packages")).unwrap();
        assert_eq!(site_packages(&venv), Vec::<PathBuf>::new());
    }
}
