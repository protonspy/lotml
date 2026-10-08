//! A Python import is refused at the import: a program built for the LLVM target runs without
//! Python (adr:0025).

use std::path::Path;

#[test]
fn a_python_import_is_refused_at_the_import_pointing_at_run() {
    let mut interfaces = lotml_check::Interfaces::new();
    let (found, problems) = lotml_check::interface_of("py.textwrap", "fn dedent(text: str) -> str ! PyError\n");
    assert!(problems.is_empty());
    interfaces.insert("py.textwrap".to_string(), found);
    let source = "from py.textwrap import dedent\n\nfn main():\n    print(1)\n";
    let Err(errors) = lotml_llvm::compile_program(source, Path::new("prog.lot"), &interfaces, false, false) else {
        panic!("a Python import compiled to LLVM");
    };
    assert_eq!(errors[0].code, "E0401");
    assert_eq!(errors[0].span.start, 0, "the import is what is refused");
    assert!(errors[0].message.contains("LLVM target runs without Python"), "{}", errors[0].message);
}

#[test]
fn a_python_object_is_refused_naming_the_python_target() {
    let source = "fn keep(o: PyObject) -> PyObject:\n    return o\n\nfn main():\n    print(1)\n";
    let Err(errors) =
        lotml_llvm::compile_program(source, Path::new("prog.lot"), &lotml_check::Interfaces::new(), false, false)
    else {
        panic!("a PyObject compiled to LLVM");
    };
    assert_eq!(errors[0].code, "E0402", "{:?}", errors.iter().map(|d| &d.message).collect::<Vec<_>>());
    assert!(errors[0].message.contains("PyObject"), "{}", errors[0].message);
    assert!(errors[0].notes.iter().any(|n| n.contains("Python target")), "{:?}", errors[0].notes);
}
