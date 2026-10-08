//! `lotml.lock`: for each `py.` module a project's programs import and the compiler binds on
//! import, the source of its stub, the stub's SHA-256 and the interface's (specs/bind-on-import/
//! R2, adr:0032-a-python-module-is-bound-at-check-time-from-stubs-read-in-rust). It records what
//! was bound, so two machines bind alike; it is no root of trust.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::files;

/// The lock's file name, at the project's root.
pub const NAME: &str = "lotml.lock";

/// What one module was bound from.
#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub source: String,
    pub stub: String,
    pub interface: String,
}

/// `sha256:` and the digest of `text`, as the lock writes a hash.
pub fn hash(text: &str) -> String {
    format!("sha256:{}", lotml_llvm::sha256::hex_of(text.as_bytes()))
}

/// Past this, a lock is not read.
const LARGEST: u64 = 16 << 20;

/// The lock at `root`, read as data: `None` when there is none, an error for one lotml cannot read
/// or that names a module by anything but a dotted identifier after `py.`.
pub fn read(root: &Path) -> Result<Option<Vec<Entry>>, String> {
    use std::io::Read;
    let path = root.join(NAME);
    let Ok(file) = std::fs::File::open(&path) else { return Ok(None) };
    let mut text = String::new();
    file.take(LARGEST + 1).read_to_string(&mut text).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if text.len() as u64 > LARGEST {
        return Err(format!("{} is past the {LARGEST} bytes lotml reads", path.display()));
    }
    let table: toml::Table =
        text.parse().map_err(|e: toml::de::Error| format!("{} is not TOML: {}", path.display(), e.message()))?;
    if table.get("version").and_then(toml::Value::as_integer) != Some(1) {
        return Err(format!("{} is not a lock of version 1", path.display()));
    }
    let mut entries = Vec::new();
    for module in table.get("module").and_then(toml::Value::as_array).map(Vec::as_slice).unwrap_or_default() {
        let field = |key: &str| module.get(key).and_then(toml::Value::as_str).map(str::to_string);
        let (Some(name), Some(source), Some(stub), Some(interface)) =
            (field("name"), field("source"), field("stub"), field("interface"))
        else {
            return Err(format!("{} has a module without its name, source, stub and interface", path.display()));
        };
        let module = name
            .strip_prefix("py.")
            .ok_or_else(|| format!("{} names `{}`, not a `py.` module", path.display(), name.escape_debug()))?;
        crate::stubs::name(module).map_err(|why| format!("{}: {why}", path.display()))?;
        entries.push(Entry { name, source, stub, interface });
    }
    Ok(Some(entries))
}

/// The entry the lock at `root` has for `name` (`py.<module>`), if it can be read and has one.
pub fn entry(root: &Path, name: &str) -> Option<Entry> {
    read(root).ok().flatten()?.into_iter().find(|e| e.name == name)
}

/// What `lotml check --locked` fails on for the files at `paths` (specs/bind-on-import/ R2.3): a
/// project without a lock, a `py.` import bound on import that its lock does not record, and a
/// stub that differs from the one it records. Empty when the locks hold.
pub fn verify(paths: &[PathBuf]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut imported: std::collections::BTreeMap<PathBuf, BTreeSet<String>> = std::collections::BTreeMap::new();
    for path in paths {
        let Some(root) = crate::exec::project_of(path) else {
            problems.push(format!("{}: no project to hold lotml.lock", path.display()));
            continue;
        };
        let Ok(text) = files::read(path) else { continue };
        let covered: Vec<String> = files::bindings_for(path).into_iter().map(|b| b.module).collect();
        imported
            .entry(root)
            .or_default()
            .extend(files::python_imports(&text).into_iter().filter(|m| !covered.contains(m)));
    }
    for (root, names) in imported {
        let lock = match read(&root) {
            Ok(Some(lock)) => lock,
            Ok(None) => {
                problems.push(format!("{} has no {NAME}; `lotml bind --lock` writes it", root.display()));
                continue;
            }
            Err(why) => {
                problems.push(why);
                continue;
            }
        };
        for name in names {
            let Some(entry) = lock.iter().find(|e| e.name == name) else {
                problems.push(format!("`{name}` is imported, and {NAME} does not record it; `lotml bind --lock` does"));
                continue;
            };
            let module = name.strip_prefix("py.").unwrap_or(&name);
            match crate::stubs::find(module, || crate::stubs::environment(Some(&root))) {
                Ok(stub) if hash(&stub.text) != entry.stub => {
                    problems.push(format!("the stub of `{name}` differs from the one {NAME} records"));
                }
                Ok(_) => {}
                Err(why) => problems.push(format!("`{name}`: {why}")),
            }
        }
    }
    problems
}

/// The `py.` modules the programs under `root` import that no bindings file covers, each bound
/// now; an error naming every one no stub binds.
pub fn entries(root: &Path) -> Result<Vec<Entry>, String> {
    let mut names = BTreeSet::new();
    for path in files::sources(&[root.to_path_buf()]).map_err(|crate::Failure(why)| why)? {
        let text = files::read(&path).map_err(|crate::Failure(why)| why)?;
        let covered: Vec<String> = files::bindings_for(&path).into_iter().map(|b| b.module).collect();
        names.extend(files::python_imports(&text).into_iter().filter(|m| !covered.contains(m)));
    }
    let mut entries = Vec::new();
    let mut unbound = Vec::new();
    for name in names {
        let module = name.strip_prefix("py.").unwrap_or(&name);
        match crate::stubs::bound_with_stub(module, Some(root)) {
            Ok((stub, interface)) => {
                entries.push(Entry { source: stub.source, stub: hash(&stub.text), interface: hash(&interface), name })
            }
            Err(why) => unbound.push(format!("{name}: {why}")),
        }
    }
    if !unbound.is_empty() {
        return Err(format!("no lock written; no stub binds\n  {}", unbound.join("\n  ")));
    }
    Ok(entries)
}

/// The text of a lock holding `entries`, in the order given.
pub fn text(entries: &[Entry]) -> String {
    let quoted = |s: &str| toml::Value::String(s.to_string()).to_string();
    let mut out =
        String::from("# Written by `lotml bind --lock`: what each `py.` module was bound from.\nversion = 1\n");
    for e in entries {
        out += &format!(
            "\n[[module]]\nname = {}\nsource = {}\nstub = {}\ninterface = {}\n",
            quoted(&e.name),
            quoted(&e.source),
            quoted(&e.stub),
            quoted(&e.interface)
        );
    }
    out
}

/// `lotml bind --lock`: the lock of the project at `root` written whole, at its root.
pub fn write(root: &Path) -> Result<(PathBuf, usize), String> {
    let entries = entries(root)?;
    let path = root.join(NAME);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let partial = root.join(format!(".{NAME}.{}.{nanos}", std::process::id()));
    std::fs::write(&partial, text(&entries)).map_err(|e| format!("cannot write {}: {e}", partial.display()))?;
    std::fs::rename(&partial, &path).map_err(|e| {
        let _ = std::fs::remove_file(&partial);
        format!("cannot write {}: {e}", path.display())
    })?;
    Ok((path, entries.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lock_is_toml_with_each_value_quoted() {
        let entries = [Entry {
            name: "py.textwrap".into(),
            source: "typeshed 0d9b1926fc75".into(),
            stub: hash("def f() -> int: ...\n"),
            interface: hash("fn f() -> int ! PyError\n"),
        }];
        let written = text(&entries);
        let read: toml::Table = written.parse().unwrap();
        assert_eq!(read["version"].as_integer(), Some(1));
        let module = &read["module"].as_array().unwrap()[0];
        assert_eq!(module["name"].as_str(), Some("py.textwrap"));
        assert_eq!(module["source"].as_str(), Some("typeshed 0d9b1926fc75"));
        assert!(module["stub"].as_str().unwrap().starts_with("sha256:"));
        let odd = text(&[Entry {
            name: "py.x".into(),
            source: "a \"b\"\nc".into(),
            stub: String::new(),
            interface: String::new(),
        }]);
        let read: toml::Table = odd.parse().unwrap();
        assert_eq!(read["module"][0]["source"].as_str(), Some("a \"b\"\nc"), "a source cannot add a line");
    }

    fn root(name: &str, lock: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lotml-lockfile-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(NAME), lock).unwrap();
        dir
    }

    #[test]
    fn a_lock_written_is_read_back_and_a_bad_one_is_refused() {
        let entries = vec![Entry {
            name: "py.os.path".into(),
            source: "typeshed 0d9b1926fc75".into(),
            stub: hash("a"),
            interface: hash("b"),
        }];
        let dir = root("read", &text(&entries));
        assert_eq!(read(&dir).unwrap(), Some(entries));
        assert_eq!(entry(&dir, "py.os.path").unwrap().stub, hash("a"));
        assert!(entry(&dir, "py.os").is_none());
        assert_eq!(read(&dir.join("none")).unwrap(), None);
        let field = "[[module]]\nsource = \"s\"\nstub = \"s\"\ninterface = \"i\"\n";
        for (bad, says) in [
            ("version = 2\n", "version 1"),
            ("version = 1\n[[module]\n", "not TOML"),
            (&*format!("version = 1\n{field}name = \"py.../x\"\n"), "not a Python module name"),
            (&*format!("version = 1\n{field}name = \"textwrap\"\n"), "not a `py.` module"),
            ("version = 1\n[[module]]\nname = \"py.x\"\n", "without its name"),
        ] {
            let why = read(&root("bad", bad)).unwrap_err();
            assert!(why.contains(says), "{bad}: {why}");
        }
    }
}
