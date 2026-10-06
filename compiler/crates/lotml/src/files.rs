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
        let linked = entry.file_type().is_ok_and(|t| t.is_symlink());
        if path.is_dir() && !hidden && !linked && entry.file_name() != "target" {
            walk(&path, found)?;
        } else if path.extension().is_some_and(|e| e == "lotml") {
            found.push(path);
        }
    }
    Ok(())
}

/// The interface of a Python module a file may import (adr:0012).
pub struct Binding {
    pub module: String,
    pub path: PathBuf,
    pub text: String,
}

/// The interfaces a file may import from: each `bindings/<module>.lotmli` in the file's
/// directory or one above it, the nearest one for each module — up to the repository's root,
/// the directory holding `.git`, so a `bindings/` outside the project never applies.
pub fn interfaces_for(path: &Path) -> Vec<Binding> {
    let mut found: Vec<Binding> = Vec::new();
    let mut dir = std::path::absolute(path).ok().and_then(|p| p.parent().map(Path::to_path_buf));
    while let Some(here) = dir {
        if let Ok(entries) = std::fs::read_dir(here.join("bindings")) {
            let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
            entries.sort();
            for file in entries.into_iter().filter(|p| p.extension().is_some_and(|e| e == "lotmli")) {
                let Some(module) = file.file_stem().map(|s| s.to_string_lossy().into_owned()) else { continue };
                if found.iter().any(|b| b.module == module) {
                    continue;
                }
                if let Ok(text) = std::fs::read_to_string(&file) {
                    found.push(Binding { module, path: file, text });
                }
            }
        }
        if here.join(".git").exists() {
            break;
        }
        dir = here.parent().map(Path::to_path_buf);
    }
    found
}

/// [`interfaces_for`] as the incremental engine keeps them, each module's name with its text,
/// read once per directory however many files it holds.
#[derive(Default)]
pub struct InterfaceCache(std::collections::HashMap<PathBuf, Vec<(String, String)>>);

impl InterfaceCache {
    pub fn get(&mut self, path: &Path) -> Vec<(String, String)> {
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        self.0
            .entry(dir)
            .or_insert_with(|| interfaces_for(path).into_iter().map(|b| (b.module, b.text)).collect())
            .clone()
    }
}

pub fn read(path: &Path) -> Result<String, Failure> {
    std::fs::read_to_string(path).map_err(|e| Failure(format!("cannot read {}: {e}", path.display())))
}

/// The variables that point git at a repository other than the one it finds from its
/// directory. A git hook sets `GIT_DIR`, and one inherited sends `git -C dir` to the hook's
/// repository instead of the file's.
const GIT_LOCATION: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
];

/// `git -C dir`, for the repository holding `dir` whatever this process inherited.
fn git(dir: &Path) -> Command {
    let mut command = Command::new("git");
    for variable in GIT_LOCATION {
        command.env_remove(variable);
    }
    command.arg("-C").arg(dir);
    command
}

/// The text of `path` at git revision `rev`: empty when the file did not exist then.
pub fn at_revision(rev: &str, path: &Path) -> Result<String, Failure> {
    // A revision is never an option: `--output=…` would make `git show` write a file.
    if rev.starts_with('-') || rev.is_empty() {
        return Err(Failure(format!("`{rev}` is not a revision")));
    }
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let name = path.file_name().ok_or_else(|| Failure(format!("{} is not a file", path.display())))?;
    // `rev:./name` is resolved against the directory git runs in.
    let output = git(dir)
        .arg("show")
        .arg(format!("{rev}:./{}", name.to_string_lossy()))
        .output()
        .map_err(|e| Failure(format!("cannot run git: {e}")))?;
    if output.status.success() {
        return String::from_utf8(output.stdout)
            .map_err(|_| Failure(format!("{} at {rev} is not UTF-8", path.display())));
    }
    let verify = git(dir)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interfaces_are_looked_for_up_to_the_repository_s_root() {
        let base = std::env::temp_dir().join(format!("lotml-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let project = base.join("repo").join("src");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(base.join("repo").join(".git")).unwrap();
        std::fs::create_dir_all(base.join("repo").join("bindings")).unwrap();
        std::fs::create_dir_all(base.join("bindings")).unwrap();
        std::fs::write(base.join("repo").join("bindings").join("inside.lotmli"), "").unwrap();
        std::fs::write(base.join("bindings").join("outside.lotmli"), "").unwrap();
        let found: Vec<String> = interfaces_for(&project.join("a.lotml")).into_iter().map(|b| b.module).collect();
        let _ = std::fs::remove_dir_all(&base);
        assert_eq!(found, vec!["inside"], "a bindings/ above the repository never applies");
    }
}
