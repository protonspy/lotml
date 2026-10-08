//! The per-user cache of the runtime's object (plans/build-and-check-speed.md 1.2): the runtime
//! compiled once per toolchain and flags rather than with every program, a separate unit as
//! adr:0025 has it.
//!
//! An entry is named by its key and by the digest of its own bytes, `<key>-<digest>.o`. It is
//! compiled into a temporary file and renamed into place, so no reader sees half of one, and its
//! bytes are checked against the digest in its name before it is linked. The digest finds a
//! damaged entry; it does not authenticate one, which is why the directory has to be this user's
//! alone: only they can write an entry into it.

use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::sha256;

/// The entries kept: past these, a new one removes the least recently used.
const KEPT: usize = 32;

/// How old a temporary file or directory is before it is taken for one a build abandoned.
const ABANDONED: Duration = Duration::from_secs(3600);

/// The hexadecimal digits of a key, and of the digest an entry's name carries.
const DIGITS: usize = 32;

/// The file that marks the directory as this cache's, in the form of the Cache Directory Tagging
/// Specification, so backup tools skip it too. A directory with files in it and no marker is
/// someone else's, and is not used.
const MARKER: &str = "CACHEDIR.TAG";
const MARKER_TEXT: &str =
    "Signature: 8a477f597d28d172789f06886806bc55\n# lotml's cache of the runtime's compiled objects.\n";

/// The prefix of the file that shows, on Unix, who owns the directory.
const PROBE: &str = ".owner.";

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

impl Object {
    /// An object compiled where the build asked, outside any cache: nothing to remove.
    pub fn kept(path: PathBuf) -> Object {
        Object { path, temporary: false }
    }
}

/// A directory of its own inside the cache, for one compile's sources, removed when dropped.
#[derive(Debug)]
pub struct Workspace {
    pub path: PathBuf,
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

impl Cache {
    /// This user's cache: under `LOTML_CACHE_DIR` when it is set, or else under the system's
    /// per-user cache directory. `None` when there is none, or it cannot be shown to be this
    /// user's own; the runtime is then compiled with each program.
    pub fn user() -> Option<Cache> {
        root(|name| std::env::var_os(name), cfg!(windows)).and_then(|root| Cache::at(&root))
    }

    /// The cache under `root`, in a `runtime` directory created readable by its owner alone; `None`
    /// when it cannot be created, is not this user's own, or holds files and no marker.
    pub fn at(root: &Path) -> Option<Cache> {
        let dir = root.join("runtime");
        owned_directory(&dir).ok()?;
        marked(&dir).ok()?;
        Some(Cache { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The entry for `key` whose bytes match the digest its name carries, marked as just used.
    /// An entry that does not match is removed.
    pub fn get(&self, key: &str) -> Option<PathBuf> {
        if !is_hex(key) {
            return None;
        }
        let read = std::fs::read_dir(&self.dir).ok()?;
        let mut names: Vec<String> = read
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| entry_key(n) == Some(key))
            .collect();
        names.sort();
        names.into_iter().find_map(|name| {
            let path = self.dir.join(&name);
            if verified(&path, &name) {
                let used = touching().open(&path);
                let _ = used.and_then(|file| file.set_modified(SystemTime::now()));
                return Some(path);
            }
            let _ = std::fs::remove_file(&path);
            None
        })
    }

    /// A new directory inside the cache for compiling the entry of `key` from sources nobody else
    /// can write.
    pub fn workspace(&self, key: &str) -> Result<Workspace, String> {
        if !is_hex(key) {
            return Err(format!("`{key}` is not a key of the runtime cache"));
        }
        let path = self.dir.join(format!("{key}.{}.{}.src", std::process::id(), nanos()));
        std::fs::create_dir(&path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        Ok(Workspace { path })
    }

    /// The entry for `key`, made by `compile` into the temporary file it is given and renamed into
    /// place. When the rename fails, as on Windows while another build links an entry of the same
    /// name, that entry is used if it is sound, and the temporary file otherwise.
    pub fn put(&self, key: &str, compile: impl FnOnce(&Path) -> Result<(), String>) -> Result<Object, String> {
        if !is_hex(key) {
            return Err(format!("`{key}` is not a key of the runtime cache"));
        }
        let temporary = self.dir.join(format!("{key}.{}.{}.tmp.o", std::process::id(), nanos()));
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
        let name = format!("{key}-{}.o", &sha256::hex_of(&bytes)[..DIGITS]);
        let entry = self.dir.join(&name);
        match std::fs::rename(&temporary, &entry) {
            Ok(()) => {
                self.prune();
                Ok(Object { path: entry, temporary: false })
            }
            Err(_) if verified(&entry, &name) => {
                let _ = std::fs::remove_file(&temporary);
                Ok(Object { path: entry, temporary: false })
            }
            Err(_) => Ok(Object { path: temporary, temporary: true }),
        }
    }

    /// Keep the [`KEPT`] most recently used entries, and remove the temporary files and
    /// directories a build abandoned. Nothing else in the directory is touched.
    fn prune(&self) {
        let Ok(read) = std::fs::read_dir(&self.dir) else { return };
        let now = SystemTime::now();
        let mut entries: Vec<(SystemTime, PathBuf)> = Vec::new();
        for item in read.flatten() {
            let Ok(modified) = item.metadata().and_then(|m| m.modified()) else { continue };
            let name = item.file_name().to_string_lossy().into_owned();
            let abandoned = now.duration_since(modified).is_ok_and(|age| age > ABANDONED);
            if entry_key(&name).is_some() {
                entries.push((modified, item.path()));
            } else if abandoned && is_temporary(&name, ".tmp.o") {
                let _ = std::fs::remove_file(item.path());
            } else if abandoned && is_temporary(&name, ".src") {
                let _ = std::fs::remove_dir_all(item.path());
            }
        }
        entries.sort_by_key(|e| std::cmp::Reverse(e.0));
        for (_, path) in entries.into_iter().skip(KEPT) {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// How an entry is opened to mark it used.
fn touching() -> std::fs::OpenOptions {
    let mut options = std::fs::File::options();
    options.write(true);
    options
}

fn nanos() -> u128 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_nanos())
}

fn is_hex(text: &str) -> bool {
    text.len() == DIGITS && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The key of an entry's name, `<key>-<digest>.o`; `None` for any other name.
fn entry_key(name: &str) -> Option<&str> {
    let (key, rest) = name.split_once('-')?;
    let digest = rest.strip_suffix(".o")?;
    (is_hex(key) && is_hex(digest)).then_some(key)
}

/// Whether `name` is a temporary file or directory of this cache: `<key>.<pid>.<nanos><suffix>`.
fn is_temporary(name: &str, suffix: &str) -> bool {
    let Some(rest) = name.strip_suffix(suffix) else { return false };
    let mut parts = rest.split('.');
    let key = parts.next().is_some_and(is_hex);
    let numbers: Vec<&str> = parts.collect();
    key && numbers.len() == 2 && numbers.iter().all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// Whether the bytes at `path` have the digest the entry's `name` carries.
fn verified(path: &Path, name: &str) -> bool {
    let Some(digest) = name.strip_suffix(".o").and_then(|n| n.split_once('-')).map(|(_, d)| d) else { return false };
    std::fs::read(path).is_ok_and(|bytes| sha256::hex_of(&bytes)[..DIGITS] == *digest)
}

/// The cache's root from the environment `var` reads: `LOTML_CACHE_DIR`, or the per-user cache
/// directory of the system. A root must be absolute and without `..`; on Windows, where only the
/// user profile's own permissions keep it private, `LOTML_CACHE_DIR` must lie inside the profile.
fn root(var: impl Fn(&str) -> Option<std::ffi::OsString>, windows: bool) -> Option<PathBuf> {
    let absolute = |name: &str| {
        var(name).map(PathBuf::from).filter(|p| p.is_absolute() && !p.components().any(|c| c == Component::ParentDir))
    };
    if var("LOTML_CACHE_DIR").is_some_and(|v| !v.is_empty()) {
        let given = absolute("LOTML_CACHE_DIR")?;
        if windows {
            let profile = [absolute("USERPROFILE"), absolute("LOCALAPPDATA")];
            if !profile.iter().flatten().any(|p| given.starts_with(p)) {
                return None;
            }
        }
        return Some(given);
    }
    let base = if windows {
        absolute("LOCALAPPDATA")
    } else if cfg!(target_os = "macos") {
        absolute("HOME").map(|home| home.join("Library").join("Caches"))
    } else {
        absolute("XDG_CACHE_HOME").or_else(|| absolute("HOME").map(|home| home.join(".cache")))
    };
    Some(base?.join("lotml"))
}

/// `dir`, created if need be, a directory and not a link. On Unix it must also be owned by this
/// user, which a file just created in it shows, be readable by its owner alone, and sit in a
/// directory nobody else may rename it out of.
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
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let probe = dir.join(format!("{PROBE}{}.{}", std::process::id(), nanos()));
        let file = std::fs::File::options().write(true).create_new(true).open(&probe)?;
        let me = file.metadata()?.uid();
        drop(file);
        let _ = std::fs::remove_file(&probe);
        if std::fs::symlink_metadata(dir)?.uid() != me {
            return Err(std::io::Error::other("owned by another user"));
        }
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        if let Some(parent) = dir.parent() {
            let mode = std::fs::metadata(parent)?.mode();
            if mode & 0o002 != 0 && mode & 0o1000 == 0 {
                return Err(std::io::Error::other("in a directory anyone may write"));
            }
        }
    }
    Ok(())
}

/// Mark `dir` as this cache's if it is empty; refuse it if it holds files and no marker.
fn marked(dir: &Path) -> std::io::Result<()> {
    let marker = dir.join(MARKER);
    if marker.is_file() {
        return Ok(());
    }
    let others = std::fs::read_dir(dir)?.flatten().any(|e| !e.file_name().to_string_lossy().starts_with(PROBE));
    if others {
        return Err(std::io::Error::other("a directory this cache did not make"));
    }
    std::fs::write(marker, MARKER_TEXT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("lotml-llvm-cache").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn key(k: usize) -> String {
        format!("{k:032x}")
    }

    fn write(text: &'static str) -> impl FnOnce(&Path) -> Result<(), String> {
        move |path| std::fs::write(path, text).map_err(|e| e.to_string())
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> =
            std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        names.sort();
        names
    }

    #[test]
    fn an_entry_put_is_got_by_its_key_alone() {
        let cache = Cache::at(&scratch("put")).unwrap();
        assert_eq!(cache.get(&key(1)), None);
        let object = cache.put(&key(1), write("object")).unwrap();
        assert!(object.path.starts_with(cache.dir()));
        assert_eq!(cache.get(&key(1)), Some(object.path.clone()));
        assert_eq!(cache.get(&key(2)), None);
        let entry = format!("{}-{}.o", key(1), &sha256::hex_of(b"object")[..DIGITS]);
        assert_eq!(names(cache.dir()), [entry, MARKER.to_string()], "no temporary file is left");
    }

    #[test]
    fn a_key_that_is_not_a_digest_is_refused() {
        let cache = Cache::at(&scratch("keys")).unwrap();
        for bad in ["k".to_string(), "../x".to_string(), format!("{}/..", key(1)), key(1).to_uppercase() + "A"] {
            assert!(cache.put(&bad, write("object")).is_err(), "{bad}");
            assert!(cache.workspace(&bad).is_err(), "{bad}");
            assert_eq!(cache.get(&bad), None, "{bad}");
        }
    }

    #[test]
    fn an_entry_whose_bytes_changed_is_not_got_and_is_removed() {
        let cache = Cache::at(&scratch("changed")).unwrap();
        let path = cache.put(&key(1), write("object")).unwrap().path.clone();
        std::fs::write(&path, "tampered").unwrap();
        assert_eq!(cache.get(&key(1)), None);
        assert!(!path.exists());
    }

    #[test]
    fn a_failed_compile_leaves_nothing_behind() {
        let cache = Cache::at(&scratch("failed")).unwrap();
        let error = cache
            .put(&key(1), |path| {
                std::fs::write(path, "half").unwrap();
                Err("clang failed".to_string())
            })
            .unwrap_err();
        assert_eq!(error, "clang failed");
        assert_eq!(names(cache.dir()), [MARKER]);
    }

    #[test]
    fn a_workspace_is_removed_when_done_with() {
        let cache = Cache::at(&scratch("workspace")).unwrap();
        let workspace = cache.workspace(&key(1)).unwrap();
        std::fs::write(workspace.path.join("lotml.c"), "").unwrap();
        let path = workspace.path.clone();
        drop(workspace);
        assert!(!path.exists());
    }

    #[test]
    fn a_directory_holding_files_and_no_marker_is_not_taken_for_the_cache() {
        let root = scratch("foreign");
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        std::fs::write(root.join("runtime").join("theirs.o"), "").unwrap();
        assert!(Cache::at(&root).is_none());
        assert!(root.join("runtime").join("theirs.o").exists());
    }

    fn age(path: &Path, when: SystemTime) {
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_modified(when).unwrap();
    }

    #[test]
    fn only_the_most_recently_used_entries_are_kept_and_abandoned_temporary_files_go() {
        let cache = Cache::at(&scratch("prune")).unwrap();
        let old = cache.dir().join(format!("{}.1.1.tmp.o", key(0)));
        std::fs::write(&old, "").unwrap();
        age(&old, SystemTime::now() - 2 * ABANDONED);
        let fresh = cache.dir().join(format!("{}.2.2.tmp.o", key(0)));
        std::fs::write(&fresh, "").unwrap();
        let unrelated = cache.dir().join("notes.o");
        std::fs::write(&unrelated, "").unwrap();
        age(&unrelated, SystemTime::now() - 2 * ABANDONED);
        let first = cache.put(&key(0), write("0")).unwrap().path.clone();
        age(&first, SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000));
        assert!(cache.get(&key(0)).is_some(), "used again, so no longer the oldest");
        for k in 1..KEPT + 3 {
            let path = cache.put(&key(k), move |p| std::fs::write(p, format!("{k}")).map_err(|e| e.to_string()));
            age(&path.unwrap().path, SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000 + k as u64));
        }
        assert!(!old.exists(), "an abandoned temporary file is removed");
        assert!(fresh.exists(), "a recent one may still be written");
        assert!(unrelated.exists(), "a file the cache did not name is never removed");
        let entries = names(cache.dir()).into_iter().filter(|n| entry_key(n).is_some()).count();
        assert_eq!(entries, KEPT);
        assert!(cache.get(&key(0)).is_some(), "the entry used most recently is kept");
        assert_eq!(cache.get(&key(1)), None, "the least recently used went first");
    }

    #[test]
    fn the_root_is_lotml_cache_dir_when_set_and_absolute_else_the_system_s() {
        let absolute = std::env::temp_dir().join("lotml-root");
        let given = absolute.clone();
        let var = move |name: &str| (name == "LOTML_CACHE_DIR").then(|| given.clone().into_os_string());
        assert_eq!(root(var, false), Some(absolute.clone()));
        let relative = |name: &str| (name == "LOTML_CACHE_DIR").then(|| "cache".into());
        assert_eq!(root(relative, false), None, "a relative LOTML_CACHE_DIR is refused, not resolved");
        let climbing = absolute.join("..").join("elsewhere");
        let up = move |name: &str| (name == "LOTML_CACHE_DIR").then(|| climbing.clone().into_os_string());
        assert_eq!(root(up, false), None, "a root with `..` in it is refused");
        let home = std::env::temp_dir().join("home");
        let system = home.clone();
        let var = move |name: &str| {
            matches!(name, "LOCALAPPDATA" | "HOME" | "XDG_CACHE_HOME").then(|| system.clone().into_os_string())
        };
        let found = root(var, cfg!(windows)).unwrap();
        assert!(found.starts_with(&home) && found.ends_with("lotml"), "{}", found.display());
        assert_eq!(root(|_| None, false), None);
    }

    #[test]
    fn on_windows_lotml_cache_dir_must_lie_inside_the_user_profile() {
        let profile = std::env::temp_dir().join("profile");
        let inside = profile.join("caches");
        let outside = std::env::temp_dir().join("shared");
        let env = |cache: PathBuf| {
            let profile = profile.clone();
            move |name: &str| match name {
                "LOTML_CACHE_DIR" => Some(cache.clone().into_os_string()),
                "USERPROFILE" => Some(profile.clone().into_os_string()),
                _ => None,
            }
        };
        assert_eq!(root(env(inside.clone()), true), Some(inside));
        assert_eq!(root(env(outside.clone()), true), None);
        assert_eq!(root(env(outside.clone()), false), Some(outside), "elsewhere, ownership is checked instead");
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

    #[cfg(unix)]
    #[test]
    fn a_directory_anyone_may_write_without_the_sticky_bit_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let root = scratch("open");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(Cache::at(&root).is_none());
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o1777)).unwrap();
        assert!(Cache::at(&root).is_some(), "the sticky bit keeps others from renaming it away");
    }

    #[cfg(windows)]
    #[test]
    fn marking_an_entry_used_leaves_a_linker_free_to_open_it() {
        use std::os::windows::fs::OpenOptionsExt;
        /// `FILE_SHARE_READ`: what `link.exe` lets others do with a file it reads.
        const SHARE_READ: u32 = 1;
        let dir = scratch("touch");
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("entry.o");
        std::fs::write(&entry, b"object").unwrap();
        let marking = touching().open(&entry).expect("the entry opens to be marked");
        let linking = std::fs::File::options().read(true).share_mode(SHARE_READ).open(&entry);
        assert!(linking.is_ok(), "a build linking the entry while another marks it: {linking:?}");
        marking.set_modified(SystemTime::now()).expect("the mark is set");
    }
}
