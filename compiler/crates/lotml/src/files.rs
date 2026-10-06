//! Finding the files a command works on, and reading them as they were at a git revision.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::Failure;

/// The `.lotml` files named, or found under the directories named, in a stable order.
pub fn expand(paths: &[PathBuf]) -> Result<Vec<PathBuf>, Failure> {
    let mut found = Vec::new();
    for path in paths {
        if path.is_dir() {
            walk(path, &mut found).map_err(|e| Failure(format!("cannot read {}: {e}", path.display())))?;
        } else if path.is_file() {
            found.push(path.clone());
        } else {
            return Err(Failure(format!("{} does not exist", path.display())));
        }
    }
    Ok(found)
}

fn walk(dir: &Path, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        // A file found under `.` is named as the user would write it, without `./`.
        let path = if dir == Path::new(".") { PathBuf::from(entry.file_name()) } else { entry.path() };
        let hidden = entry.file_name().to_string_lossy().starts_with('.');
        if path.is_dir() && !hidden && entry.file_name() != "target" {
            walk(&path, found)?;
        } else if path.extension().is_some_and(|e| e == "lotml") {
            found.push(path);
        }
    }
    Ok(())
}

pub fn read(path: &Path) -> Result<String, Failure> {
    std::fs::read_to_string(path).map_err(|e| Failure(format!("cannot read {}: {e}", path.display())))
}

/// The text of `path` at git revision `rev`: empty when the file did not exist then.
pub fn at_revision(rev: &str, path: &Path) -> Result<String, Failure> {
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let name = path.file_name().ok_or_else(|| Failure(format!("{} is not a file", path.display())))?;
    // `rev:./name` is resolved against the directory git runs in.
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .arg("show")
        .arg(format!("{rev}:./{}", name.to_string_lossy()))
        .output()
        .map_err(|e| Failure(format!("cannot run git: {e}")))?;
    if output.status.success() {
        return String::from_utf8(output.stdout)
            .map_err(|_| Failure(format!("{} at {rev} is not UTF-8", path.display())));
    }
    let verify = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--verify", "--quiet"])
        .arg(format!("{rev}^{{commit}}"))
        .output()
        .map_err(|e| Failure(format!("cannot run git: {e}")))?;
    if verify.status.success() {
        Ok(String::new())
    } else {
        Err(Failure(format!("`{rev}` is not a revision of the repository holding {}", path.display())))
    }
}
