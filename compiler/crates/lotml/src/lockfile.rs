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
}
