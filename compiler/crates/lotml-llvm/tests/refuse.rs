//! What the LLVM backend does not compile yet is refused at the construct, naming the target that
//! does (specs/llvm-backend R2.6), and a Python import is refused at the import (adr:0025).

use std::path::Path;

fn refusal(source: &str) -> Vec<lotml_diag::Diagnostic> {
    match lotml_llvm::compile(source, Path::new("prog.lot")) {
        Ok(ll) => panic!("compiled to LLVM:\n{source}\n{ll}"),
        Err(errors) => errors,
    }
}

#[test]
fn a_construct_outside_the_first_increment_is_refused_where_it_is_written() {
    for (source, construct, what) in [
        ("fn main():\n    xs = [1, 2, 3]\n    print(len(xs))\n", "xs = [1, 2, 3]", "list"),
        ("fn main():\n    name = \"ana\"\n    print(name)\n", "name = \"ana\"", "str"),
        ("type P(x: int)\n\nfn main():\n    p = P(1)\n    print(p.x)\n", "p = P(1)", "record"),
        ("fn main():\n    n = 3\n    print(f\"{n}\")\n", "print(f\"{n}\")", "f-string"),
    ] {
        let errors = refusal(source);
        let first = &errors[0];
        assert_eq!(first.code, "E0402", "{source}: {:?}", first.message);
        assert!(first.message.contains("`--target llvm` does not compile"), "{}", first.message);
        assert!(first.message.contains(what), "{source}: {}", first.message);
        assert!(first.message.contains("`--target python`"), "{}", first.message);
        let at = &source[first.span.start as usize..first.span.end as usize];
        assert!(at.starts_with(construct), "{source}: the refusal points at {at:?}");
    }
}

#[test]
fn a_python_import_is_refused_at_the_import_pointing_at_run() {
    let mut interfaces = lotml_check::Interfaces::new();
    let (found, problems) = lotml_check::interface_of("textwrap", "fn dedent(text: str) -> str ! PyError\n");
    assert!(problems.is_empty());
    interfaces.insert("textwrap".to_string(), found);
    let source = "from textwrap import dedent\n\nfn main():\n    print(1)\n";
    let Err(errors) = lotml_llvm::compile_program(source, Path::new("prog.lot"), &interfaces, false) else {
        panic!("a Python import compiled to LLVM");
    };
    assert_eq!(errors[0].code, "E0401");
    assert_eq!(errors[0].span.start, 0, "the import is what is refused");
    assert!(errors[0].message.contains("LLVM target runs without Python"), "{}", errors[0].message);
}

#[test]
fn test_blocks_are_refused_until_the_parity_increment() {
    let source = "fn f() -> int:\n    return 1\n\ntest \"one\":\n    assert f() == 1\n";
    let Err(errors) = lotml_llvm::compile_program(source, Path::new("prog.lot"), &lotml_check::Interfaces::new(), true)
    else {
        panic!("test blocks compiled to LLVM");
    };
    assert!(errors[0].message.contains("`test` blocks"), "{}", errors[0].message);
}
