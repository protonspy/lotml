//! Which CPython runs the Python target (adr:0026-lotml-ships-uv-and-runs-python-3-14-by-default,
//! plans/python-via-uv.md 1.5), in a fixed order: `LOTML_PYTHON`; the project's virtual
//! environment, for what runs the project's code; a managed CPython 3.14 uv already has; a download
//! of one through uv, said before it starts; and, with no uv, the interpreters on the path.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::uv::Uv;

/// The CPython lotml runs by default once the parity suite passes on it (plans/python-via-uv.md 1.1).
pub const VERSION: &str = "3.14";

/// What the interpreter is for: `run` and `test` run the project's code, so its virtual environment
/// adds no trust; `bind` reads a stub and never uses it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Use {
    Run,
    Bind,
}

/// What a resolution may do beyond looking.
pub struct Options<'a> {
    pub uses: Use,
    /// The project's root, where a `.venv/` is looked for.
    pub project: Option<&'a Path>,
    /// Whether a missing CPython may be downloaded: never offline, never by the MCP server, the
    /// grader or the harness.
    pub downloads: bool,
}

/// The managed interpreters uv gives: what [`resolve`] asks of it.
pub trait Managed {
    /// The managed `version` uv already has, its executable.
    fn find(&self, version: &str) -> Option<PathBuf>;
    /// Download `version`, having said what and from where.
    fn install(&self, version: &str) -> Result<(), String>;
}

/// The command of the interpreter to run, or why there is none.
pub fn resolve(
    options: &Options<'_>,
    var: &dyn Fn(&str) -> Option<OsString>,
    managed: Option<&dyn Managed>,
    on_path: &dyn Fn() -> Option<Vec<String>>,
) -> Result<Vec<String>, String> {
    if let Some(given) = var("LOTML_PYTHON").filter(|v| !v.is_empty()) {
        return Ok(vec![given.to_string_lossy().into_owned()]);
    }
    if options.uses == Use::Run
        && let Some(venv) = virtual_environment(var, options.project)
    {
        return Ok(vec![venv.display().to_string()]);
    }
    let Some(uv) = managed else {
        return on_path().ok_or_else(|| {
            format!(
                "no Python found: lotml runs CPython {VERSION} through uv, and neither uv nor a Python on the path was found; put the uv lotml ships beside it, set LOTML_UV, or set LOTML_PYTHON"
            )
        });
    };
    if let Some(found) = uv.find(VERSION) {
        return Ok(vec![found.display().to_string()]);
    }
    if !options.downloads {
        return Err(format!(
            "no Python found: CPython {VERSION} is not installed through uv, and lotml downloads nothing here; install it with `uv python install {VERSION}`, or set LOTML_PYTHON"
        ));
    }
    uv.install(VERSION)?;
    uv.find(VERSION)
        .map(|found| vec![found.display().to_string()])
        .ok_or_else(|| format!("uv installed CPython {VERSION}, and then could not find it"))
}

/// The interpreter of the virtual environment `VIRTUAL_ENV` names, or of the project's `.venv/`:
/// one holding `pyvenv.cfg`, inside the project once its links are followed, in a project root
/// that is no drive's root and that nobody else may write to, and owned by whoever owns the root —
/// so a `.venv` another user left above the project is never run.
fn virtual_environment(var: &dyn Fn(&str) -> Option<OsString>, project: Option<&Path>) -> Option<PathBuf> {
    let interpreter = |venv: &Path| {
        let python =
            if cfg!(windows) { venv.join("Scripts").join("python.exe") } else { venv.join("bin").join("python") };
        python.is_file().then_some(python)
    };
    if let Some(active) = var("VIRTUAL_ENV").map(PathBuf::from).filter(|p| p.is_absolute())
        && let Some(python) = interpreter(&active)
    {
        return Some(python);
    }
    let root = project?.canonicalize().ok()?;
    let venv = root.join(".venv").canonicalize().ok()?;
    if !venv.starts_with(&root) || root.parent().is_none() || !venv.join("pyvenv.cfg").is_file() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let (owner, made) = (root.metadata().ok()?, venv.metadata().ok()?);
        if owner.mode() & 0o002 != 0 || made.uid() != owner.uid() {
            return None;
        }
    }
    interpreter(&venv)
}

/// uv's managed interpreters, from its confined calls (adr:0026), saying what it downloads.
pub struct ThroughUv<'a> {
    pub uv: Uv,
    pub var: &'a dyn Fn(&str) -> Option<OsString>,
}

impl Managed for ThroughUv<'_> {
    fn find(&self, version: &str) -> Option<PathBuf> {
        let out = self.uv.find_python(version, self.var).output().ok()?;
        let found = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
        (out.status.success() && found.is_file()).then_some(found)
    }

    fn install(&self, version: &str) -> Result<(), String> {
        let mut command = self.uv.install_python(version, self.var)?;
        eprintln!("{}", crate::uv::describe(&self.uv.path));
        eprintln!(
            "lotml: downloading CPython {version}, about 30 MB, from github.com/astral-sh/python-build-standalone through uv; it is kept for every later run"
        );
        let status = command.status().map_err(|e| format!("cannot run uv: {e}"))?;
        if !status.success() {
            return Err(format!("uv could not install CPython {version}"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;

    /// A uv whose managed interpreters are `has`, recording what it was asked to install.
    struct Fake {
        has: RefCell<Option<PathBuf>>,
        installs: RefCell<Vec<String>>,
        gives: Option<PathBuf>,
    }

    impl Managed for Fake {
        fn find(&self, _: &str) -> Option<PathBuf> {
            self.has.borrow().clone()
        }
        fn install(&self, version: &str) -> Result<(), String> {
            self.installs.borrow_mut().push(version.to_string());
            *self.has.borrow_mut() = self.gives.clone();
            Ok(())
        }
    }

    fn fake(has: Option<&str>, gives: Option<&str>) -> Fake {
        Fake {
            has: RefCell::new(has.map(PathBuf::from)),
            installs: RefCell::new(Vec::new()),
            gives: gives.map(PathBuf::from),
        }
    }

    fn options(uses: Use, project: Option<&Path>, downloads: bool) -> Options<'_> {
        Options { uses, project, downloads }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("lotml-resolve").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn venv_in(root: &Path) -> PathBuf {
        let python = if cfg!(windows) {
            root.join(".venv").join("Scripts").join("python.exe")
        } else {
            root.join(".venv").join("bin").join("python")
        };
        std::fs::create_dir_all(python.parent().unwrap()).unwrap();
        std::fs::write(&python, b"").unwrap();
        std::fs::write(root.join(".venv").join("pyvenv.cfg"), b"home = x\n").unwrap();
        python
    }

    #[test]
    fn lotml_python_comes_first() {
        let vars = HashMap::from([("LOTML_PYTHON", OsString::from("/my/python"))]);
        let var = |n: &str| vars.get(n).cloned();
        let uv = fake(Some("managed"), None);
        assert_eq!(resolve(&options(Use::Bind, None, true), &var, Some(&uv), &|| None), Ok(vec!["/my/python".into()]));
    }

    #[test]
    fn the_project_s_virtual_environment_runs_its_code_and_never_binds() {
        let root = scratch("venv");
        let python = venv_in(&root);
        let none = |_: &str| None;
        let uv = fake(Some("managed"), None);
        let ran = resolve(&options(Use::Run, Some(&root), true), &none, Some(&uv), &|| None).unwrap();
        assert_eq!(PathBuf::from(&ran[0]).canonicalize().unwrap(), python.canonicalize().unwrap());
        let bound = resolve(&options(Use::Bind, Some(&root), true), &none, Some(&uv), &|| None).unwrap();
        assert_eq!(bound, vec!["managed".to_string()], "bind never uses the project's environment");
    }

    #[test]
    fn a_directory_that_is_no_virtual_environment_is_never_run() {
        let root = scratch("no-pyvenv");
        venv_in(&root);
        std::fs::remove_file(root.join(".venv").join("pyvenv.cfg")).unwrap();
        let none = |_: &str| None;
        let uv = fake(Some("managed"), None);
        let ran = resolve(&options(Use::Run, Some(&root), true), &none, Some(&uv), &|| None);
        assert_eq!(ran, Ok(vec!["managed".into()]), "a `.venv` without `pyvenv.cfg` is not one");
    }

    #[cfg(unix)]
    #[test]
    fn a_project_root_anyone_may_write_runs_no_virtual_environment() {
        use std::os::unix::fs::PermissionsExt;
        let root = scratch("shared-root");
        venv_in(&root);
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o1777)).unwrap();
        let none = |_: &str| None;
        let uv = fake(Some("managed"), None);
        let ran = resolve(&options(Use::Run, Some(&root), true), &none, Some(&uv), &|| None);
        assert_eq!(ran, Ok(vec!["managed".into()]), "a `.venv` in a shared directory is anyone's");
    }

    #[test]
    fn a_managed_python_uv_has_comes_before_a_download() {
        let none = |_: &str| None;
        let uv = fake(Some("managed"), Some("downloaded"));
        assert_eq!(resolve(&options(Use::Run, None, true), &none, Some(&uv), &|| None), Ok(vec!["managed".into()]));
        assert!(uv.installs.borrow().is_empty());
    }

    #[test]
    fn a_missing_python_is_downloaded_only_where_downloads_are_allowed() {
        let none = |_: &str| None;
        let uv = fake(None, Some("downloaded"));
        let refused = resolve(&options(Use::Run, None, false), &none, Some(&uv), &|| Some(vec!["path".into()]));
        assert!(refused.unwrap_err().contains("downloads nothing"), "offline, the path is not a fallback for uv");
        assert!(uv.installs.borrow().is_empty());
        let got = resolve(&options(Use::Run, None, true), &none, Some(&uv), &|| None);
        assert_eq!(got, Ok(vec!["downloaded".into()]));
        assert_eq!(*uv.installs.borrow(), vec![VERSION.to_string()]);
    }

    #[test]
    fn without_uv_the_path_is_searched_as_before() {
        let none = |_: &str| None;
        let found = resolve(&options(Use::Run, None, true), &none, None, &|| Some(vec!["python3".into()]));
        assert_eq!(found, Ok(vec!["python3".into()]));
        let missing = resolve(&options(Use::Run, None, true), &none, None, &|| None).unwrap_err();
        assert!(missing.contains("LOTML_UV") && missing.contains("LOTML_PYTHON"), "{missing}");
        assert!(!missing.contains("3.11 or later"), "the old message is gone: {missing}");
    }
}
