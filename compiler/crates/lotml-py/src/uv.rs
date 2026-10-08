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

/// The variables lotml passes to uv, and no others: where the user's directories, the system and
/// the temporary directory are, a proxy and a certificate store the user set, and where the user
/// keeps uv's cache and its interpreters. Nothing that configures uv for a project.
pub const PASSED: &[&str] = &[
    "PATH",
    "SYSTEMROOT",
    "SYSTEMDRIVE",
    "WINDIR",
    "COMSPEC",
    "TEMP",
    "TMP",
    "TMPDIR",
    "HOME",
    "USERPROFILE",
    "LOCALAPPDATA",
    "APPDATA",
    "XDG_CACHE_HOME",
    "XDG_DATA_HOME",
    "HTTPS_PROXY",
    "HTTP_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "UV_CACHE_DIR",
    "UV_PYTHON_INSTALL_DIR",
];

/// The variables under which lotml refuses to download: each disables verification or replaces
/// where uv's downloads and their metadata come from.
pub const REFUSED: &[&str] =
    &["UV_INSECURE_HOST", "UV_PYTHON_INSTALL_MIRROR", "UV_PYPY_INSTALL_MIRROR", "UV_PYTHON_DOWNLOADS_JSON_URL"];

/// uv as lotml runs it (adr:0026): from `home`, a directory of lotml's own cache, never the
/// project's, with the environment it reads limited to [`PASSED`].
pub struct Uv {
    pub path: PathBuf,
    pub home: PathBuf,
}

impl Uv {
    /// A uv command, every rule of adr:0026 applied: run from `home` with uv's configuration files
    /// ignored, Python downloads forbidden, only uv's managed interpreters asked for, no shim on the
    /// user's path and no Windows registry entry.
    pub fn command(&self, var: &dyn Fn(&str) -> Option<OsString>) -> Command {
        let mut command = Command::new(&self.path);
        command.current_dir(&self.home).env_clear();
        for name in PASSED {
            if let Some(value) = var(name) {
                command.env(name, value);
            }
        }
        command
            .env("UV_NO_CONFIG", "1")
            .env("UV_PYTHON_DOWNLOADS", "never")
            .env("UV_MANAGED_PYTHON", "1")
            .env("UV_PYTHON_INSTALL_BIN", "0")
            .env("UV_PYTHON_INSTALL_REGISTRY", "0")
            .env("UV_NO_PROGRESS", "1");
        command
    }

    /// `uv python find` of the managed `version`, which downloads nothing.
    pub fn find_python(&self, version: &str, var: &dyn Fn(&str) -> Option<OsString>) -> Command {
        let mut command = self.command(var);
        command.args(["python", "find", "--managed-python", "--no-python-downloads", "--no-project", version]);
        command
    }

    /// `uv python install` of `version`: the one step that downloads, refused while a variable of
    /// [`REFUSED`] is set, which it names.
    pub fn install_python(&self, version: &str, var: &dyn Fn(&str) -> Option<OsString>) -> Result<Command, String> {
        if let Some(name) = REFUSED.iter().find(|name| var(name).is_some()) {
            return Err(format!(
                "{name} is set, which changes where uv downloads from or whether it verifies: lotml downloads nothing while it is"
            ));
        }
        let mut command = self.command(var);
        command.env("UV_PYTHON_DOWNLOADS", "manual").args([
            "python",
            "install",
            "--managed-python",
            "--no-bin",
            "--no-registry",
            version,
        ]);
        Ok(command)
    }
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

#[cfg(test)]
mod confined_tests {
    use std::collections::HashMap;

    use super::*;

    fn env_of(command: &Command) -> HashMap<String, Option<String>> {
        command
            .get_envs()
            .map(|(k, v)| (k.to_string_lossy().into_owned(), v.map(|v| v.to_string_lossy().into_owned())))
            .collect()
    }

    fn hostile(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("lotml-uv-confined").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("uv.toml"),
            "python-preference = \"only-system\"\npython-install-mirror = \"https://example.invalid/evil\"\n",
        )
        .unwrap();
        std::fs::write(dir.join(".python-version"), "3.9\n").unwrap();
        dir
    }

    #[test]
    fn a_uv_command_runs_from_lotml_s_directory_with_every_rule_set() {
        let uv = Uv { path: PathBuf::from("uv"), home: PathBuf::from("cache-home") };
        let vars = HashMap::from([
            ("PATH", OsString::from("p")),
            ("UV_PYTHON_PREFERENCE", OsString::from("only-system")),
            ("PYTHONPATH", OsString::from("evil")),
        ]);
        let var = |name: &str| vars.get(name).cloned();
        let command = uv.find_python("3.14", &var);
        assert_eq!(command.get_current_dir(), Some(Path::new("cache-home")), "never the project's directory");
        let env = env_of(&command);
        for (name, value) in [
            ("UV_NO_CONFIG", "1"),
            ("UV_PYTHON_DOWNLOADS", "never"),
            ("UV_MANAGED_PYTHON", "1"),
            ("UV_PYTHON_INSTALL_BIN", "0"),
            ("UV_PYTHON_INSTALL_REGISTRY", "0"),
            ("PATH", "p"),
        ] {
            assert_eq!(env.get(name), Some(&Some(value.to_string())), "{name}");
        }
        assert!(!env.contains_key("UV_PYTHON_PREFERENCE") && !env.contains_key("PYTHONPATH"), "only the listed pass");
        let args: Vec<_> = command.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(args, ["python", "find", "--managed-python", "--no-python-downloads", "--no-project", "3.14"]);
    }

    #[test]
    fn the_install_step_alone_downloads_with_no_shim_and_no_registry_entry() {
        let uv = Uv { path: PathBuf::from("uv"), home: PathBuf::from("cache-home") };
        let none = |_: &str| None;
        let command = uv.install_python("3.14", &none).expect("an install");
        assert_eq!(env_of(&command).get("UV_PYTHON_DOWNLOADS"), Some(&Some("manual".to_string())));
        let args: Vec<_> = command.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert!(args.contains(&"--no-bin".to_string()) && args.contains(&"--no-registry".to_string()), "{args:?}");
    }

    #[test]
    fn a_variable_that_changes_where_downloads_come_from_refuses_the_download() {
        let uv = Uv { path: PathBuf::from("uv"), home: PathBuf::from("cache-home") };
        for name in REFUSED {
            let var = |n: &str| (n == *name).then(|| OsString::from("https://example.invalid"));
            let Err(refused) = uv.install_python("3.14", &var) else { panic!("{name} refused nothing") };
            assert!(refused.starts_with(name), "{refused}");
        }
    }

    #[test]
    fn a_hostile_uv_toml_and_python_version_change_nothing() {
        let Ok(Some(path)) = find(&Places::here()) else { return };
        let home = hostile("home");
        let uv = Uv { path, home: home.clone() };
        let var = |name: &str| std::env::var_os(name);
        let Ok(out) = uv.find_python("3.11", &var).output() else { return };
        if !out.status.success() {
            // No managed 3.11 on this machine: nothing to compare against.
            return;
        }
        let found = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
        assert!(found.is_file(), "confined, uv finds its managed 3.11 despite the uv.toml: {}", found.display());
        // The same directory, unconfined: the uv.toml asks for system interpreters only, and the
        // .python-version names one that is not there.
        let unconfined = Command::new(&uv.path)
            .current_dir(&home)
            .args(["python", "find", "--no-python-downloads", "3.11"])
            .env_remove("UV_MANAGED_PYTHON")
            .output()
            .unwrap();
        assert!(!unconfined.status.success(), "the hostile uv.toml is what the confinement ignores");
    }
}
