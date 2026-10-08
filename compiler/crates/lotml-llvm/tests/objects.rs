//! What a native program loads, read from its bytes by the test's own PE and ELF reader
//! (plans/target-parity-assurance.md 3.2): a program built for the LLVM target imports no
//! libpython (adr:0025), and a file cut short is an error, never a panic.

mod common;

use std::path::{Path, PathBuf};

use common::objects::{Linkage, linkage};
use common::{clang, scratch};
use lotml_llvm::driver::Level;

/// The hello program built at `level`, and its bytes.
fn built(name: &str, level: Level) -> Option<(PathBuf, Vec<u8>)> {
    let clang = clang()?;
    let dir = scratch("objects", name);
    let source = "fn main():\n    xs = [1, 2, 3]\n    print(\"hello\", sum(xs))\n";
    let path = dir.join("prog.lot");
    let ll = lotml_llvm::compile(source, &path).unwrap_or_else(|d| panic!("{:?}", d.first().map(|d| &d.message)));
    std::fs::write(dir.join("prog.ll"), ll).unwrap();
    lotml_runtime::write(&dir).unwrap();
    let exe = dir.join(if cfg!(windows) { "prog.exe" } else { "prog" });
    clang.build(&dir.join("prog.ll"), &dir, &exe, level, &[]).unwrap_or_else(|e| panic!("{e}"));
    let bytes = std::fs::read(&exe).unwrap();
    Some((exe, bytes))
}

fn read(exe: &Path, bytes: &[u8]) -> Option<Linkage> {
    match linkage(bytes) {
        Ok(linkage) => Some(linkage),
        Err(e) if cfg!(target_os = "macos") => {
            eprintln!("skipped: {e}");
            None
        }
        Err(e) => panic!("{}: {e}", exe.display()),
    }
}

#[test]
fn a_native_program_loads_no_python() {
    for level in [Level::Debug, Level::Release] {
        let Some((exe, bytes)) = built(&format!("no-python-{level:?}"), level) else { return };
        let Some(linkage) = read(&exe, &bytes) else { return };
        assert!(!linkage.imports.is_empty(), "a program loads the C library at least");
        let python: Vec<&String> = linkage.imports.iter().filter(|l| l.to_lowercase().contains("python")).collect();
        assert!(python.is_empty(), "{level:?} loads {python:?} among {:?}", linkage.imports);
    }
}

#[test]
fn a_file_cut_short_is_an_error_and_never_a_panic() {
    let Some((exe, bytes)) = built("cut", Level::Release) else { return };
    if read(&exe, &bytes).is_none() {
        return;
    }
    for end in [0, 1, 3, 4, 16, 63, 64, 100, 200, 300, 400, 512] {
        assert!(linkage(&bytes[..end.min(bytes.len())]).is_err(), "cut at {end}");
    }
    // Wherever it is cut, the reader answers: an error, or what the part it kept holds.
    let step = (bytes.len() / 997).max(1);
    for end in (0..bytes.len()).step_by(step) {
        let _ = linkage(&bytes[..end]);
    }
}

#[test]
fn counts_that_lie_are_bounded_rather_than_followed() {
    let Some((exe, bytes)) = built("lies", Level::Release) else { return };
    if read(&exe, &bytes).is_none() {
        return;
    }
    let mut broken = bytes.clone();
    if broken.starts_with(b"MZ") {
        // The PE header's number of sections, set as large as it goes.
        let header = u32::from_le_bytes(broken[0x3c..0x40].try_into().unwrap()) as usize;
        broken[header + 6..header + 8].copy_from_slice(&u16::MAX.to_le_bytes());
    } else {
        // The ELF section headers' offset, pointed past the file.
        let wide = broken[4] == 2;
        let at = if wide { 0x28 } else { 0x20 };
        let past = (broken.len() as u64 + 1000).to_le_bytes();
        let size = if wide { 8 } else { 4 };
        broken[at..at + size].copy_from_slice(&past[..size]);
    }
    assert!(linkage(&broken).is_err(), "a header pointing past the file is an error");
}

#[test]
fn neither_format_is_an_error() {
    assert!(linkage(b"").is_err());
    assert!(linkage(b"#!/bin/sh\necho hi\n").is_err());
}
