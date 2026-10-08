//! A module as a library C calls (specs/c-abi-export): the functions chosen by their signatures,
//! each one left out warned, the header read by C11 and C++, and a C program calling the library
//! from two threads, stopping as a LotML program stops on a panic or a string that is not UTF-8.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::{clang, scratch};
use lotml_check::Interfaces;
use lotml_llvm::driver::Clang;
use lotml_llvm::export::library_file;

const GEO: &str = "type P(x: int)\n\n\
fn add(a: int, b: int) -> int:\n    return a + b\n\n\
fn half(x: f32) -> f32:\n    return x / 2.0\n\n\
fn big(n: u8) -> bool:\n    return n > u8(9)\n\n\
fn small(n: i16) -> i8:\n    return i8(n // i16(2))\n\n\
fn shout(s: str) -> int:\n    print(s.upper())\n    return len(s)\n\n\
fn pick(i: int) -> int:\n    xs = [10, 20, 30]\n    return xs[i]\n\n\
fn hello():\n    print(\"hello from LotML\")\n\n\
fn total(xs: [int]) -> int:\n    return sum(xs)\n\n\
fn first[T](xs: [T]) -> T:\n    return xs[0]\n\n\
fn label(n: int) -> str:\n    return str(n)\n\n\
fn parse(s: str) -> int ! str:\n    return len(s)\n\n\
fn bump(inout n: int):\n    n += 1\n\n\
fn origin() -> P:\n    return P(0)\n\n\
fn main():\n    print(add(1, 2))\n";

/// The C program the library is linked into: `calls` calls every exported function, two threads
/// adding at once; `panic` indexes out of range; `utf8` passes bytes that are not UTF-8.
const PROGRAM: &str = r#"#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include "geo.h"
#ifdef _WIN32
#include <windows.h>
#else
#include <pthread.h>
#endif

static int64_t sums[2];

static void work(int k) {
    int64_t total = 0;
    for (int64_t i = 0; i < 100000; i++) total = geo_add(total, i % 7);
    sums[k] = total;
}

#ifdef _WIN32
static DWORD WINAPI run(LPVOID arg) { work((int)(intptr_t)arg); return 0; }
#else
static void *run(void *arg) { work((int)(intptr_t)arg); return NULL; }
#endif

int main(int argc, char **argv) {
    const char *mode = argc > 1 ? argv[1] : "calls";
    if (strcmp(mode, "panic") == 0) {
        printf("%lld\n", (long long)geo_pick(3));
        return 0;
    }
    if (strcmp(mode, "utf8") == 0) {
        printf("%lld\n", (long long)geo_shout("\xff\xfe"));
        return 0;
    }
    printf("%lld %.2f %d %d %d\n", (long long)geo_add(2, 40), (double)geo_half(5.0f), geo_big(10), geo_big(3),
           geo_small(-9));
    fflush(stdout);
    int64_t n = geo_shout("h\xc3\xa9llo");
    printf("%lld\n", (long long)n);
    fflush(stdout);
    geo_hello();
    printf("%lld\n", (long long)geo_pick(-1));
    fflush(stdout);
#ifdef _WIN32
    HANDLE threads[2];
    for (int k = 0; k < 2; k++) threads[k] = CreateThread(NULL, 0, run, (LPVOID)(intptr_t)k, 0, NULL);
    WaitForMultipleObjects(2, threads, TRUE, INFINITE);
#else
    pthread_t threads[2];
    for (int k = 0; k < 2; k++) pthread_create(&threads[k], NULL, run, (void *)(intptr_t)k);
    for (int k = 0; k < 2; k++) pthread_join(threads[k], NULL);
#endif
    printf("%lld %lld\n", (long long)sums[0], (long long)sums[1]);
    return 0;
}
"#;

fn compiled(name: &str, source: &str) -> (PathBuf, lotml_llvm::Library) {
    let dir = scratch("library", name);
    let path = dir.join("geo.lot");
    std::fs::write(&path, source).unwrap();
    let library = lotml_llvm::compile_library(source, &path, &Interfaces::new())
        .unwrap_or_else(|d| panic!("{:#?}", d.iter().map(|d| &d.message).collect::<Vec<_>>()));
    (dir, library)
}

#[test]
fn the_functions_c_can_call_are_exported_and_each_one_left_out_is_warned() {
    let (_, library) = compiled("chosen", GEO);
    for symbol in ["geo_add", "geo_half", "geo_big", "geo_small", "geo_shout", "geo_pick", "geo_hello"] {
        assert!(library.header.contains(&format!(" {symbol}(")), "{symbol} is declared:\n{}", library.header);
    }
    assert!(!library.header.contains("geo_main"), "main is a program's, not the library's");
    let warned: Vec<(&str, &str)> = library
        .warnings
        .iter()
        .map(|d| {
            assert_eq!((d.code, d.severity), ("E0403", lotml_diag::Severity::Warning));
            let name = d.message.split('`').nth(1).unwrap_or("");
            (name, d.message.as_str())
        })
        .collect();
    let names: Vec<&str> = warned.iter().map(|(n, _)| *n).collect();
    assert_eq!(names, ["total", "first", "label", "parse", "bump", "origin"]);
    for (name, why) in [
        ("total", "`[int]`"),
        ("first", "generic"),
        ("label", "`str`"),
        ("parse", "`int ! str`"),
        ("bump", "`inout`"),
        ("origin", "`P`"),
    ] {
        let message = warned.iter().find(|(n, _)| *n == name).map(|(_, m)| *m).unwrap_or("");
        assert!(message.contains(why), "{name}: {message}");
    }
}

#[test]
fn a_module_with_nothing_c_can_call_is_an_error() {
    let source = "fn total(xs: [int]) -> int:\n    return sum(xs)\n\nfn main():\n    print(total([1]))\n";
    let Err(errors) = lotml_llvm::compile_library(source, Path::new("none.lot"), &Interfaces::new()) else {
        panic!("a library with nothing to export");
    };
    let codes: Vec<&str> = errors.iter().map(|d| d.code).collect();
    assert_eq!(codes, ["E0404", "E0403"], "{:?}", errors.iter().map(|d| &d.message).collect::<Vec<_>>());
    assert!(errors[0].message.contains("none"), "{}", errors[0].message);
}

#[test]
fn the_header_is_read_by_c11_and_cpp() {
    let Some(clang) = clang() else { return };
    let (dir, library) = compiled("header", GEO);
    std::fs::write(dir.join("geo.h"), &library.header).unwrap();
    std::fs::write(dir.join("use.c"), "#include \"geo.h\"\n").unwrap();
    for args in [&["-std=c11", "-pedantic"][..], &["-x", "c++", "-std=c++17", "-pedantic"]] {
        let out = Command::new(&clang.program)
            .args(["-fsyntax-only", "-Wall", "-Wextra", "-Werror"])
            .args(args)
            .arg("-I")
            .arg(&dir)
            .arg(dir.join("use.c"))
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}: {}\n{}", String::from_utf8_lossy(&out.stderr), library.header);
    }
}

/// The library built from `GEO` and the C program linked against it, in a directory of its own.
fn built(clang: &Clang, name: &str) -> PathBuf {
    let (dir, library) = compiled(name, GEO);
    lotml_runtime::write(&dir).unwrap();
    std::fs::write(dir.join("geo.h"), &library.header).unwrap();
    let ll = dir.join("geo.ll");
    std::fs::write(&ll, &library.ll).unwrap();
    let lib = dir.join(library_file("geo"));
    clang.build_shared(&ll, &dir, &lib, &library.libraries).unwrap_or_else(|e| panic!("{e}\n{}", library.ll));
    std::fs::write(dir.join("prog.c"), PROGRAM).unwrap();
    let exe = dir.join(if cfg!(windows) { "prog.exe" } else { "prog" });
    let mut command = Command::new(&clang.program);
    command.args(["-std=c11", "-w", "-o"]).arg(&exe).arg(dir.join("prog.c")).arg("-I").arg(&dir);
    if cfg!(windows) {
        command.arg(dir.join("geo.lib"));
    } else {
        command.arg(&lib).arg("-pthread");
    }
    let out = command.output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    exe
}

fn run(exe: &Path, mode: &str) -> Output {
    Command::new(exe).arg(mode).current_dir(exe.parent().unwrap()).output().expect("the program runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace("\r\n", "\n")
}

#[test]
fn a_c_program_calls_the_library_from_two_threads_as_lotml_calls_it() {
    let Some(clang) = clang() else { return };
    let exe = built(&clang, "calls");
    let out = run(&exe, "calls");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let sum: i64 = (0..100_000).map(|i| i % 7).sum();
    assert_eq!(text(&out.stdout), format!("42 2.50 1 0 -5\nHÉLLO\n5\nhello from LotML\n30\n{sum} {sum}\n"));
}

#[test]
fn a_panic_in_the_library_stops_the_process_naming_where() {
    let Some(clang) = clang() else { return };
    let exe = built(&clang, "panic");
    let out = run(&exe, "panic");
    assert_eq!(out.status.code(), Some(101), "{}", text(&out.stderr));
    let said = text(&out.stderr);
    assert!(said.contains("IndexError"), "{said}");
    assert!(said.contains("geo.lot\", line 21, in pick"), "{said}");
}

#[test]
fn a_string_that_is_not_utf8_stops_the_process_naming_the_function_and_parameter() {
    let Some(clang) = clang() else { return };
    let exe = built(&clang, "utf8");
    let out = run(&exe, "utf8");
    assert_eq!(out.status.code(), Some(101), "{}", text(&out.stderr));
    let said = text(&out.stderr);
    assert!(said.contains("the argument `s` is not valid UTF-8"), "{said}");
    assert!(said.contains("in shout"), "{said}");
}

#[test]
fn a_library_named_like_the_runtime_is_refused() {
    let Err(errors) =
        lotml_llvm::compile_library("fn init() -> int:\n    return 1\n", Path::new("lt.lot"), &Interfaces::new())
    else {
        panic!("a library exporting `lt_init`");
    };
    assert_eq!(errors[0].code, "E0405", "{:?}", errors.iter().map(|d| &d.message).collect::<Vec<_>>());
    assert!(errors[0].message.contains("lt_init"), "{}", errors[0].message);
}

/// The functions the library exports, read from its bytes (plans/target-parity-assurance.md 3.2);
/// on ELF, without the names starting `_` that the C runtime's start files define. `None` on a
/// platform whose format the reader does not read, Mach-O.
fn exported(library: &Path) -> Option<Vec<String>> {
    let bytes = std::fs::read(library).expect("the library");
    match common::objects::linkage(&bytes) {
        Ok(linkage) => Some(linkage.exports.into_iter().filter(|n| !n.starts_with('_')).collect()),
        Err(e) if cfg!(target_os = "macos") => {
            eprintln!("skipped: {e}");
            None
        }
        Err(e) => panic!("{}: {e}", library.display()),
    }
}

#[test]
fn a_library_exports_its_functions_and_nothing_of_the_runtime() {
    let Some(clang) = clang() else { return };
    let exe = built(&clang, "symbols");
    let library = exe.parent().unwrap().join(library_file("geo"));
    let Some(mut names) = exported(&library) else { return };
    names.sort();
    let wanted = ["geo_add", "geo_big", "geo_half", "geo_hello", "geo_pick", "geo_shout", "geo_small"];
    assert_eq!(names, wanted);
}
