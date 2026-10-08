//! The typeshed `stdlib` stubs lotml carries (specs/rust-binder R2.1, adr:0032): vendored at the
//! commit `typeshed/COMMIT` pins, deflated into the binary at build time, inflated on the first
//! lookup, and gated by typeshed's `VERSIONS` for the CPython lotml runs.

use std::collections::BTreeMap;
use std::sync::OnceLock;

/// The CPython whose standard library is bound: the default adr:0026 runs.
pub const PYTHON: (u32, u32) = (3, 14);

/// The typeshed commit the stubs were copied from.
pub const COMMIT: &str = include_str!("../typeshed/COMMIT");

/// typeshed's licence, which ships with the stubs.
pub const LICENSE: &str = include_str!("../typeshed/LICENSE");

static PACKED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/typeshed.deflate"));

/// Each vendored file under `stdlib/`, by its path with `/` between segments.
fn files() -> &'static BTreeMap<String, String> {
    static FILES: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    FILES.get_or_init(|| {
        let blob = miniz_oxide::inflate::decompress_to_vec(PACKED).expect("the stubs packed at build time inflate");
        let mut files = BTreeMap::new();
        let mut rest = blob.as_slice();
        while !rest.is_empty() {
            let (name, after) = field(rest);
            let (text, after) = field(after);
            files.insert(String::from_utf8_lossy(name).into_owned(), String::from_utf8_lossy(text).into_owned());
            rest = after;
        }
        files
    })
}

/// One length-prefixed field of the blob, and what follows it.
fn field(blob: &[u8]) -> (&[u8], &[u8]) {
    let (length, rest) = blob.split_at(4);
    let length = u32::from_le_bytes(length.try_into().expect("four bytes")) as usize;
    rest.split_at(length)
}

/// A standard-library module's stub, or why there is none.
#[derive(Debug, PartialEq, Eq)]
pub enum Found {
    /// The stub, by its path under `stdlib/`, and its text.
    Stub { path: String, text: &'static str },
    /// `VERSIONS` says the module is not in CPython [`PYTHON`]: the versions it lists.
    Absent { range: String },
    /// typeshed has no stub of that name.
    Missing,
}

/// The stub of `module`, a dotted name: `a/b.pyi`, else `a/b/__init__.pyi`, if `VERSIONS` places
/// the module, or the nearest package above it, in CPython [`PYTHON`].
pub fn find(module: &str) -> Found {
    if let Some((range, false)) = available(module) {
        return Found::Absent { range };
    }
    let base = module.replace('.', "/");
    for path in [format!("{base}.pyi"), format!("{base}/__init__.pyi")] {
        if let Some((path, text)) = files().get_key_value(&path) {
            return Found::Stub { path: path.clone(), text };
        }
    }
    Found::Missing
}

/// Whether `module`'s top-level name is a standard-library one: a module `VERSIONS` lists.
pub fn is_standard_library(module: &str) -> bool {
    let top = module.split('.').next().unwrap_or(module);
    versions().contains_key(top)
}

/// The range `VERSIONS` gives `module`, or the nearest package above it, and whether CPython
/// [`PYTHON`] falls in it; None when it names neither.
fn available(module: &str) -> Option<(String, bool)> {
    let mut name = module;
    loop {
        if let Some(&(from, until)) = versions().get(name) {
            let range = match until {
                Some((major, minor)) => format!("{}.{}-{major}.{minor}", from.0, from.1),
                None => format!("{}.{}-", from.0, from.1),
            };
            return Some((range, from <= PYTHON && until.is_none_or(|until| PYTHON <= until)));
        }
        name = name.rsplit_once('.')?.0;
    }
}

type Version = (u32, u32);

/// typeshed's `VERSIONS`: each module with the first and, when it was removed, the last version
/// that has it.
fn versions() -> &'static BTreeMap<String, (Version, Option<Version>)> {
    static VERSIONS: OnceLock<BTreeMap<String, (Version, Option<Version>)>> = OnceLock::new();
    VERSIONS.get_or_init(|| files().get("VERSIONS").map(|text| parse_versions(text)).unwrap_or_default())
}

fn parse_versions(text: &str) -> BTreeMap<String, (Version, Option<Version>)> {
    let version = |s: &str| {
        let (major, minor) = s.trim().split_once('.')?;
        Some((major.parse().ok()?, minor.parse().ok()?))
    };
    text.lines()
        .filter_map(|line| {
            let line = line.split('#').next()?.trim();
            let (module, range) = line.split_once(':')?;
            let (from, until) = range.split_once('-')?;
            let until = if until.trim().is_empty() { None } else { Some(version(until)?) };
            Some((module.trim().to_string(), (version(from)?, until)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_module_is_its_file_or_its_package() {
        let Found::Stub { path, text } = find("textwrap") else { panic!("textwrap is in typeshed") };
        assert_eq!(path, "textwrap.pyi");
        assert!(text.contains("def dedent("), "the vendored stub, byte for byte");
        assert!(matches!(find("json"), Found::Stub { path, .. } if path == "json/__init__.pyi"));
        assert!(matches!(find("os.path"), Found::Stub { path, .. } if path == "os/path.pyi"));
        assert_eq!(find("no_such_module"), Found::Missing);
    }

    #[test]
    fn a_module_cpython_3_14_no_longer_has_is_absent_with_its_versions() {
        let Found::Absent { range } = find("distutils") else { panic!("distutils left in 3.12") };
        assert_eq!(range, "3.0-3.11");
        assert!(matches!(find("asyncio.graph"), Found::Stub { .. }), "new in 3.14");
    }

    #[test]
    fn a_submodule_not_listed_lives_as_long_as_its_package() {
        assert_eq!(available("json.decoder"), Some(("3.0-".into(), true)));
        assert_eq!(available("nothing.here"), None);
    }

    #[test]
    fn the_standard_library_is_what_versions_lists() {
        assert!(is_standard_library("subprocess"));
        assert!(is_standard_library("os.path"));
        assert!(!is_standard_library("requests"));
    }

    #[test]
    fn versions_reads_ranges_and_skips_comments() {
        let parsed = parse_versions("# head\nfoo: 3.0-  # trailing\nbar: 3.4-3.11\n\nbad line\n");
        assert_eq!(parsed.get("foo"), Some(&((3, 0), None)));
        assert_eq!(parsed.get("bar"), Some(&((3, 4), Some((3, 11)))));
        assert_eq!(parsed.len(), 2);
    }

    #[test]
    fn the_pin_and_the_licence_ship_with_the_stubs() {
        assert_eq!(COMMIT.trim().len(), 40);
        assert!(LICENSE.contains("Apache"), "typeshed is Apache-2.0 and MIT");
    }
}
