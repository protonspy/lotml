//! The per-user cache of the runtime's object (plans/build-and-check-speed.md 1.2): the runtime
//! compiled once per toolchain and flags rather than with every program, a separate unit as
//! adr:0025 has it.
//!
//! An entry is named by its key and by the digest of its own bytes, `<key>-<digest>.o`. It is
//! compiled into a temporary file and renamed into place, so no reader sees half of one, and its
//! bytes are checked against the digest in its name before it is linked.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::sha256;

/// The entries kept: past these, a new one removes the oldest.
const KEPT: usize = 32;

/// How old a temporary file is before it is taken for one a build abandoned.
const ABANDONED: Duration = Duration::from_secs(3600);

/// The hexadecimal digits of a digest an entry's name carries.
const DIGITS: usize = 32;

/// A directory of runtime objects that belongs to this user alone.
#[derive(Clone, Debug)]
pub struct Cache {
    dir: PathBuf,
}

/// An object to link: an entry of the cache, or, when one could not be put in place, the
/// temporary file it was compiled into, removed when this is dropped.
#[derive(Debug)]
pub struct Object {
    pub path: PathBuf,
    temporary: bool,
}

impl Drop for Object {
    fn drop(&mut self) {
        if self.temporary {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

impl Cache {
    /// This user's cache: under `LOTML_CACHE_DIR` when it is set, which must then be absolute, or
    /// else under the system's per-user cache directory. `None` when there is none, or it cannot
    /// be made this user's own; the runtime is then compiled with each program.
    pub fn user() -> Option<Cache> {
        root(|name| std::env::var_os(name)).and_then(|root| Cache::at(&root))
    }

    /// The cache under `root`, in a `runtime` directory created readable by its owner alone; `None`
    /// when it cannot be created, or is not this user's own.
    pub fn at(root: &Path) -> Option<Cache> {
        let dir = root.join("runtime");
        owned_directory(&dir).ok()?;
        Some(Cache { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The entry for `key` whose bytes match the digest its name carries. An entry that does not
    /// match is removed.
    pub fn get(&self, key: &str) -> Option<PathBuf> {
        let prefix = format!("{key}-");
        let read = std::fs::read_dir(&self.dir).ok()?;
        let mut names: Vec<String> = read
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.starts_with(&prefix) && !n.ends_with(".tmp.o"))
            .collect();
        names.sort();
        names.into_iter().find_map(|name| {
            let path = self.dir.join(&name);
            if verified(&path, &name[prefix.len()..]) {
                return Some(path);
            }
            let _ = std::fs::remove_file(&path);
            None
        })
    }

    /// The entry for `key`, made by `compile` into the temporary file it is given and renamed into
    /// place. When the rename fails, as on Windows while another build links an entry of the same
    /// name, that entry is used if it is sound, and the temporary file otherwise.
    pub fn put(&self, key: &str, compile: impl FnOnce(&Path) -> Result<(), String>) -> Result<Object, String> {
        let nanos = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let temporary = self.dir.join(format!("{key}.{}.{nanos}.tmp.o", std::process::id()));
        let made = compile(&temporary);
        let bytes = made
            .and_then(|()| std::fs::read(&temporary).map_err(|e| format!("cannot read {}: {e}", temporary.display())));
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(e) => {
                let _ = std::fs::remove_file(&temporary);
                return Err(e);
            }
        };
        let digest = &sha256::hex_of(&bytes)[..DIGITS];
        let name = format!("{key}-{digest}.o");
        let entry = self.dir.join(&name);
        match std::fs::rename(&temporary, &entry) {
            Ok(()) => {
                self.prune();
                Ok(Object { path: entry, temporary: false })
            }
            Err(_) if verified(&entry, &name[key.len() + 1..]) => {
                let _ = std::fs::remove_file(&temporary);
                Ok(Object { path: entry, temporary: false })
            }
            Err(_) => Ok(Object { path: temporary, temporary: true }),
        }
    }

    /// Keep the newest [`KEPT`] entries, and remove the temporary files a build abandoned.
    fn prune(&self) {
        let Ok(read) = std::fs::read_dir(&self.dir) else { return };
        let now = SystemTime::now();
        let mut entries: Vec<(SystemTime, PathBuf)> = Vec::new();
        for item in read.flatten() {
            let Ok(modified) = item.metadata().and_then(|m| m.modified()) else { continue };
            let name = item.file_name().to_string_lossy().into_owned();
            if name.ends_with(".tmp.o") {
                if now.duration_since(modified).is_ok_and(|age| age > ABANDONED) {
                    let _ = std::fs::remove_file(item.path());
                }
            } else if name.ends_with(".o") {
                entries.push((modified, item.path()));
            }
        }
        entries.sort_by_key(|e| std::cmp::Reverse(e.0));
        for (_, path) in entries.into_iter().skip(KEPT) {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl Object {
    /// An object compiled where the build asked, outside any cache: nothing to remove.
    pub fn kept(path: PathBuf) -> Object {
        Object { path, temporary: false }
    }
}

/// Whether the bytes at `path` have the digest `rest` begins with: `<digest>.o`.
fn verified(path: &Path, rest: &str) -> bool {
    let Some(digest) = rest.strip_suffix(".o").filter(|d| d.len() == DIGITS) else { return false };
    std::fs::read(path).is_ok_and(|bytes| sha256::hex_of(&bytes)[..DIGITS] == *digest)
}

/// The cache's root from the environment `var` reads: `LOTML_CACHE_DIR`, or the per-user cache
/// directory of the system, each only when absolute.
fn root(var: impl Fn(&str) -> Option<std::ffi::OsString>) -> Option<PathBuf> {
    let absolute = |name: &str| var(name).map(PathBuf::from).filter(|p| p.is_absolute());
    if var("LOTML_CACHE_DIR").is_some_and(|v| !v.is_empty()) {
        return absolute("LOTML_CACHE_DIR");
    }
    let base = if cfg!(windows) {
        absolute("LOCALAPPDATA")
    } else if cfg!(target_os = "macos") {
        absolute("HOME").map(|home| home.join("Library").join("Caches"))
    } else {
        absolute("XDG_CACHE_HOME").or_else(|| absolute("HOME").map(|home| home.join(".cache")))
    };
    Some(base?.join("lotml"))
}

/// `dir`, created if need be, a directory and not a link, and on Unix readable by its owner alone.
/// Only a directory's owner may change its mode, so setting it is also what shows it to be ours.
fn owned_directory(dir: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)?;
    if !std::fs::symlink_metadata(dir)?.is_dir() {
        return Err(std::io::Error::other("not a directory"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("lotml-llvm-cache").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn write(text: &'static str) -> impl FnOnce(&Path) -> Result<(), String> {
        move |path| std::fs::write(path, text).map_err(|e| e.to_string())
    }

    #[test]
    fn an_entry_put_is_got_by_its_key_alone() {
        let cache = Cache::at(&scratch("put")).unwrap();
        assert_eq!(cache.get("k1"), None);
        let object = cache.put("k1", write("object")).unwrap();
        assert!(object.path.starts_with(cache.dir()));
        assert_eq!(cache.get("k1"), Some(object.path.clone()));
        assert_eq!(cache.get("k2"), None);
        let names: Vec<String> =
            std::fs::read_dir(cache.dir()).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        assert_eq!(names, [format!("k1-{}.o", &sha256::hex_of(b"object")[..DIGITS])], "no temporary file is left");
    }

    #[test]
    fn an_entry_whose_bytes_changed_is_not_got_and_is_removed() {
        let cache = Cache::at(&scratch("changed")).unwrap();
        let path = cache.put("k", write("object")).unwrap().path.clone();
        std::fs::write(&path, "tampered").unwrap();
        assert_eq!(cache.get("k"), None);
        assert!(!path.exists());
    }

    #[test]
    fn a_failed_compile_leaves_nothing_behind() {
        let cache = Cache::at(&scratch("failed")).unwrap();
        let error = cache
            .put("k", |path| {
                std::fs::write(path, "half").unwrap();
                Err("clang failed".to_string())
            })
            .unwrap_err();
        assert_eq!(error, "clang failed");
        assert_eq!(std::fs::read_dir(cache.dir()).unwrap().count(), 0);
    }

    #[test]
    fn only_the_newest_entries_are_kept_and_abandoned_temporary_files_go() {
        let cache = Cache::at(&scratch("prune")).unwrap();
        let old = cache.dir().join("k.1.1.tmp.o");
        std::fs::write(&old, "").unwrap();
        let file = std::fs::File::options().write(true).open(&old).unwrap();
        file.set_modified(SystemTime::now() - 2 * ABANDONED).unwrap();
        drop(file);
        let fresh = cache.dir().join("k.2.2.tmp.o");
        std::fs::write(&fresh, "").unwrap();
        for k in 0..KEPT + 3 {
            let path =
                cache.put(&format!("k{k}"), move |p| std::fs::write(p, format!("{k}")).map_err(|e| e.to_string()));
            let path = path.unwrap().path.clone();
            let file = std::fs::File::options().write(true).open(&path).unwrap();
            file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000 + k as u64)).unwrap();
        }
        assert!(!old.exists(), "an abandoned temporary file is removed");
        assert!(fresh.exists(), "a recent one may still be written");
        let entries =
            std::fs::read_dir(cache.dir()).unwrap().filter(|e| !e.as_ref().unwrap().path().ends_with("k.2.2.tmp.o"));
        assert_eq!(entries.count(), KEPT);
        assert_eq!(cache.get("k0"), None, "the oldest went first");
        assert!(cache.get(&format!("k{}", KEPT + 2)).is_some());
    }

    #[test]
    fn the_root_is_lotml_cache_dir_when_set_and_absolute_else_the_system_s() {
        let absolute = std::env::temp_dir().join("lotml-root");
        let given = absolute.clone();
        let var = move |name: &str| (name == "LOTML_CACHE_DIR").then(|| given.clone().into_os_string());
        assert_eq!(root(var), Some(absolute));
        let relative = |name: &str| (name == "LOTML_CACHE_DIR").then(|| "cache".into());
        assert_eq!(root(relative), None, "a relative LOTML_CACHE_DIR is refused, not resolved");
        let home = std::env::temp_dir().join("home");
        let system = home.clone();
        let var = move |name: &str| {
            matches!(name, "LOCALAPPDATA" | "HOME" | "XDG_CACHE_HOME").then(|| system.clone().into_os_string())
        };
        let found = root(var).unwrap();
        assert!(found.starts_with(&home) && found.ends_with("lotml"), "{}", found.display());
        assert_eq!(root(|_| None), None);
    }

    #[cfg(unix)]
    #[test]
    fn the_directory_is_readable_by_its_owner_alone() {
        use std::os::unix::fs::PermissionsExt;
        let cache = Cache::at(&scratch("mode")).unwrap();
        let mode = std::fs::metadata(cache.dir()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_in_place_of_the_directory_is_refused() {
        let root = scratch("link");
        let elsewhere = scratch("link-target");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, root.join("runtime")).unwrap();
        assert!(Cache::at(&root).is_none());
    }
}
