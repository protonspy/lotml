//! Finding uv, which provisions and runs the CPython the Python target needs
//! (adr:0026-lotml-ships-uv-and-runs-python-3-14-by-default). By absolute path only: a name
//! looked up relative to the working directory would let a project's own `uv` run in its place.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// uv's executable name on this platform.
pub const EXECUTABLE: &str = if cfg!(windows) { "uv.exe" } else { "uv" };

/// Where lotml looks for uv, given what it reads of the world: an environment variable by name,
/// the directory of lotml's own executable, and the working directory, which the path search
/// skips.
pub struct Places<'a> {
    pub var: &'a dyn Fn(&str) -> Option<OsString>,
    pub beside: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
}

impl Places<'_> {
    /// The places of this process.
    pub fn here() -> Places<'static> {
        Places {
            var: &|name| std::env::var_os(name),
            beside: std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)),
            cwd: std::env::current_dir().ok(),
        }
    }
}

/// uv, in the order adr:0026 sets: `LOTML_UV`, which must name an existing file by an absolute
/// path; the uv beside lotml's own executable; then the path, each entry an absolute directory that
/// is not the working one. `Ok(None)` when there is none; an error when `LOTML_UV` is set wrong.
pub fn find(places: &Places<'_>) -> Result<Option<PathBuf>, String> {
    if let Some(given) = (places.var)("LOTML_UV") {
        let path = PathBuf::from(&given);
        if !path.is_absolute() || !path.is_file() {
            return Err(format!("LOTML_UV is {}: it must name an existing file by an absolute path", path.display()));
        }
        return Ok(Some(path));
    }
    if let Some(beside) = &places.beside {
        let path = beside.join(EXECUTABLE);
        if path.is_file() {
            return Ok(Some(path));
        }
    }
    let Some(search) = (places.var)("PATH") else { return Ok(None) };
    let cwd = places.cwd.as_deref().and_then(|c| c.canonicalize().ok());
    for dir in std::env::split_paths(&search) {
        if dir.as_os_str().is_empty() || !dir.is_absolute() {
            continue;
        }
        if cwd.is_some() && dir.canonicalize().ok() == cwd {
            continue;
        }
        let path = dir.join(EXECUTABLE);
        if path.is_file() {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

/// What `uv --version` says, `uv 0.11.29 (…)`, or `None` when it does not run.
pub fn version(uv: &Path) -> Option<String> {
    let out = Command::new(uv).arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && text.starts_with("uv ")).then_some(text)
}

/// The line lotml logs for the uv it uses: its path and its version.
pub fn describe(uv: &Path) -> String {
    match version(uv) {
        Some(said) => format!("lotml: {said} at {}", uv.display()),
        None => format!("lotml: uv at {}, whose version it does not say", uv.display()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("lotml-uv-find").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn with_uv(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let uv = dir.join(EXECUTABLE);
        std::fs::write(&uv, b"").unwrap();
        uv
    }

    fn found(
        vars: &HashMap<&str, OsString>,
        beside: Option<PathBuf>,
        cwd: Option<PathBuf>,
    ) -> Result<Option<PathBuf>, String> {
        let var = |name: &str| vars.get(name).cloned();
        find(&Places { var: &var, beside, cwd })
    }

    #[test]
    fn lotml_uv_wins_and_must_be_an_existing_absolute_file() {
        let root = scratch("given");
        let given = with_uv(&root.join("given"));
        let beside = with_uv(&root.join("beside"));
        let vars = HashMap::from([("LOTML_UV", given.clone().into_os_string())]);
        assert_eq!(found(&vars, Some(beside.parent().unwrap().into()), None), Ok(Some(given)));
        let relative = HashMap::from([("LOTML_UV", OsString::from(EXECUTABLE))]);
        assert!(found(&relative, None, None).unwrap_err().contains("absolute path"));
        let missing = HashMap::from([("LOTML_UV", root.join("nowhere").join(EXECUTABLE).into_os_string())]);
        assert!(found(&missing, None, None).is_err());
    }

    #[test]
    fn the_uv_beside_lotml_comes_before_the_path() {
        let root = scratch("beside");
        let beside = with_uv(&root.join("beside"));
        let on_path = with_uv(&root.join("bin"));
        let vars = HashMap::from([("PATH", on_path.parent().unwrap().as_os_str().to_owned())]);
        assert_eq!(found(&vars, Some(beside.parent().unwrap().into()), None), Ok(Some(beside)));
        assert_eq!(found(&vars, Some(root.join("empty")), None), Ok(Some(on_path)));
    }

    #[test]
    fn the_path_search_skips_relative_and_empty_entries_and_the_working_directory() {
        let root = scratch("path");
        let project = with_uv(&root.join("project"));
        let real = with_uv(&root.join("tools"));
        let entries = [
            OsString::new(),
            OsString::from("."),
            OsString::from("bin"),
            project.parent().unwrap().as_os_str().to_owned(),
            real.parent().unwrap().as_os_str().to_owned(),
        ];
        let joined = std::env::join_paths(entries.iter().filter(|e| !e.is_empty())).unwrap();
        let vars = HashMap::from([("PATH", joined)]);
        let cwd = Some(project.parent().unwrap().to_path_buf());
        assert_eq!(found(&vars, None, cwd), Ok(Some(real)), "the project's own uv is never the one");
        assert_eq!(found(&HashMap::new(), None, None), Ok(None));
    }
}

#[cfg(test)]
mod version_tests {
    use super::*;

    #[test]
    fn the_uv_found_says_its_version_in_the_line_lotml_logs() {
        let Ok(Some(uv)) = find(&Places::here()) else { return };
        let said = version(&uv).expect("uv says its version");
        assert!(said.starts_with("uv 0."), "{said}");
        assert!(describe(&uv).contains(&uv.display().to_string()));
    }
}
