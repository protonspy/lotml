//! The Python backend: a checked lotml program as a Python module.

use std::path::Path;

use lotml_diag::Diagnostic;

/// The runtime every generated module imports, shipped next to it as `lotml_rt.py`.
pub const RUNTIME: &str = include_str!("../runtime/lotml_rt.py");

/// The command that runs Python: `LOTML_PYTHON`, else the first of `python3`, `python` and the
/// Windows launcher `py -3` that answers `--version` — a name that only opens an app store,
/// as Windows ships `python.exe`, does not.
pub fn python() -> Option<Vec<String>> {
    if let Ok(path) = std::env::var("LOTML_PYTHON") {
        return Some(vec![path]);
    }
    let candidates: [&[&str]; 3] = [&["python3"], &["python"], &["py", "-3"]];
    candidates.into_iter().map(|c| c.iter().map(ToString::to_string).collect::<Vec<_>>()).find(|command| {
        std::process::Command::new(&command[0])
            .args(&command[1..])
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success() && !o.stdout.is_empty())
    })
}

/// The Python module for `source`, which was read from `path`; or the errors that stop it.
pub fn compile(_source: &str, _path: &Path) -> Result<String, Vec<Diagnostic>> {
    unimplemented!("the Python backend")
}
