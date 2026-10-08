//! Packs the vendored typeshed `stdlib/` into one deflated blob the crate embeds (specs/rust-binder
//! 1.2): for each file in path order, the length of its path, the path, the length of its text and
//! the text, every length four bytes little-endian.

use std::path::{Path, PathBuf};

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("typeshed").join("stdlib");
    println!("cargo::rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();
    let mut blob = Vec::new();
    for file in &files {
        let relative = file.strip_prefix(&root).expect("under the root");
        let name =
            relative.components().map(|c| c.as_os_str().to_str().expect("a UTF-8 name")).collect::<Vec<_>>().join("/");
        let text = std::fs::read(file).expect("a vendored stub");
        for part in [name.as_bytes(), &text] {
            blob.extend_from_slice(&u32::try_from(part.len()).expect("under 4 GiB").to_le_bytes());
            blob.extend_from_slice(part);
        }
    }
    let packed = miniz_oxide::deflate::compress_to_vec(&blob, 9);
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR")).join("typeshed.deflate");
    std::fs::write(out, packed).expect("the packed stubs");
}

fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("the vendored typeshed").flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, files);
        } else {
            files.push(path);
        }
    }
}
