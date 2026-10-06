//! C libraries (R17, adr:0013): an interface named `c.<library>` declares C functions over the
//! types C passes by value, none of which can fail; and the sized integers they take are written
//! as literals or converted to.

use lotml_check::{Interfaces, c_interface, check_resolved_with, check_source, interface_of, is_c_library};
use lotml_syntax::parse;

fn codes(text: &str) -> Vec<&'static str> {
    c_interface(text).1.iter().map(|d| d.code).collect()
}

#[test]
fn a_c_interface_takes_scalars_and_strings_and_returns_scalars() {
    let (read, problems) =
        c_interface("fn cos(x: f64) -> f64\nfn strlen(s: str) -> u64\nfn Sleep(ms: u32)\nfn isalpha(c: i32) -> i32\n");
    assert!(problems.is_empty(), "{:?}", problems.iter().map(|d| &d.message).collect::<Vec<_>>());
    assert_eq!(read.names().collect::<Vec<_>>(), vec!["Sleep", "cos", "isalpha", "strlen"]);
}

#[test]
fn a_c_function_cannot_fail_or_take_what_c_does_not_pass_by_value() {
    assert_eq!(codes("fn f(x: int) -> int ! PyError\n"), vec!["E0221"], "C reports no failure");
    assert_eq!(codes("fn f(xs: [int]) -> int\n"), vec!["E0221"], "a list");
    assert_eq!(codes("fn f() -> str\n"), vec!["E0221"], "a string to free");
    assert_eq!(codes("fn f(x: int = 1) -> int\n"), vec!["E0221"], "a default");
    assert_eq!(codes("fn f(inout x: int)\n"), vec!["E0221"], "a place");
}

#[test]
fn the_name_says_which_world_an_interface_binds() {
    assert!(is_c_library("c.m") && is_c_library("c.kernel32"));
    assert!(!is_c_library("c") && !is_c_library("textwrap") && !is_c_library("cmath"));
    assert!(interface_of("c.m", "fn cos(x: f64) -> f64\n").1.is_empty());
    assert_eq!(interface_of("textwrap", "fn cos(x: f64) -> f64\n").1[0].code, "E0221", "a Python one needs PyError");
}

#[test]
fn a_c_function_is_called_and_returns_its_value() {
    let interfaces =
        Interfaces::from([("c.m".to_string(), c_interface("fn cos(x: f64) -> f64\nfn abs(n: i32) -> i32\n").0)]);
    let source = "from c.m import cos, abs\n\nfn f(x: f64) -> f64:\n    return cos(x) + float(abs(-3))\n";
    let found = check_resolved_with(&parse(source).module, source, &interfaces).diagnostics;
    assert!(found.is_empty(), "{:?}", found.iter().map(|d| (d.code, &d.message)).collect::<Vec<_>>());
}

#[test]
fn an_integer_literal_takes_the_sized_type_it_is_given_to() {
    let clean = |source: &str| {
        let found = check_source(source);
        assert!(found.is_empty(), "{source}\n{:?}", found.iter().map(|d| &d.message).collect::<Vec<_>>());
    };
    clean("fn f() -> u8:\n    x: u8 = 200\n    return x\n");
    clean("fn g(n: i32) -> i32:\n    return n\n\nfn f() -> i32:\n    return g(70_000)\n");
    clean("fn f() -> u16?:\n    return 0xFFFF\n");
    clean("fn f() -> i8:\n    return -128\n");
    let codes = |source: &str| check_source(source).iter().map(|d| d.code).collect::<Vec<_>>();
    assert_eq!(codes("fn f() -> u8:\n    return 256\n"), vec!["E0204"]);
    assert_eq!(codes("fn f() -> i8:\n    x: i8 = 0x80\n    return x\n"), vec!["E0204"]);
    assert_eq!(codes("fn f() -> i8:\n    return -129\n"), vec!["E0204"]);
}

#[test]
fn a_number_converts_to_a_sized_type() {
    let found = check_source("fn f(n: int) -> u32:\n    return u32(n)\n\nfn g(x: f64) -> f32:\n    return f32(x)\n");
    assert!(found.is_empty(), "{:?}", found.iter().map(|d| &d.message).collect::<Vec<_>>());
    assert_eq!(check_source("fn f(s: str) -> u8:\n    return u8(s)\n")[0].code, "E0204", "text is not a number");
}
