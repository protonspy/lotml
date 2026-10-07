//! Lowering a checked program into the IR (specs/shared-ir).

use lotml_check::ty::Ty;
use lotml_check::{Interfaces, interface_of};
use lotml_ir::ir::{Block, Expr, Function, Stmt, StmtKind};
use lotml_ir::lower::{Lowered, lower};
use lotml_syntax::parse;
use lotml_syntax::span::Span;

fn lowered(source: &str) -> Lowered {
    lowered_with(source, &Interfaces::new())
}

fn lowered_with(source: &str, interfaces: &Interfaces) -> Lowered {
    let parsed = parse(source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let checked = lotml_check::check_resolved_with(&parsed.module, source, interfaces);
    let errors: Vec<_> = checked.diagnostics.iter().filter(|d| d.severity == lotml_diag::Severity::Error).collect();
    assert!(errors.is_empty(), "{:#?}", errors.iter().map(|d| &d.message).collect::<Vec<_>>());
    lower(&parsed.module, &checked, source, false).unwrap_or_else(|d| panic!("{d:#?}"))
}

fn function<'l>(lowered: &'l Lowered, name: &str) -> &'l Function {
    lowered.functions.iter().find(|f| f.source_name == name).unwrap_or_else(|| panic!("no function {name}"))
}

fn each_stmt<'b>(block: &'b Block, f: &mut impl FnMut(&'b Stmt)) {
    for stmt in block {
        f(stmt);
        match &stmt.kind {
            StmtKind::If(_, then, otherwise) => {
                each_stmt(then, f);
                each_stmt(otherwise, f);
            }
            StmtKind::Loop(body) => each_stmt(body, f),
            StmtKind::ForRange { body, exit, .. } | StmtKind::ForStr { body, exit, .. } => {
                each_stmt(body, f);
                each_stmt(exit, f);
            }
            _ => {}
        }
    }
}

const PROGRAM: &str = "fn f(x: int) -> int:\n    y = x + 1\n    if y > 2:\n        return y * 2\n    return y\n";

#[test]
fn every_statement_carries_the_span_of_the_source_statement_it_was_lowered_from() {
    let lowered = lowered(PROGRAM);
    let statements = ["y = x + 1", "if y > 2:", "return y * 2", "return y"];
    let mut lines = Vec::new();
    each_stmt(&function(&lowered, "f").body, &mut |stmt| {
        let text = &PROGRAM[stmt.span.start as usize..stmt.span.end as usize];
        assert!(statements.iter().any(|s| text.starts_with(s)), "{text:?} is no statement of f");
        lines.push(lowered.line(stmt.span));
    });
    lines.dedup();
    assert_eq!(lines, [2, 3, 4, 5]);
}

#[test]
fn a_function_carries_the_span_of_its_declaration() {
    let lowered = lowered(PROGRAM);
    let f = function(&lowered, "f");
    assert!(PROGRAM[f.span.start as usize..].starts_with("fn f("));
    assert_eq!(lowered.line(f.span), 1);
}

#[test]
fn a_span_names_the_line_it_starts_on_counting_from_one() {
    let lowered = lowered(PROGRAM);
    let at = |offset: usize| lowered.line(Span::new(offset, offset));
    assert_eq!(at(0), 1);
    assert_eq!(at(PROGRAM.find("y = x").unwrap()), 2);
    assert_eq!(at(PROGRAM.find("return y\n").unwrap()), 5);
    assert_eq!(at(PROGRAM.find('\n').unwrap()), 1, "a line's own newline is on it");
}

fn textwrap() -> Interfaces {
    let (found, problems) = interface_of("textwrap", "fn dedent(text: str) -> str ! PyError\n");
    assert!(problems.is_empty(), "{:?}", problems.iter().map(|d| &d.message).collect::<Vec<_>>());
    Interfaces::from([("textwrap".to_string(), found)])
}

fn python_calls(lowered: &Lowered) -> Vec<(String, String, Ty)> {
    let mut calls = Vec::new();
    for f in &lowered.functions {
        each_stmt(&f.body, &mut |stmt| {
            if let StmtKind::Let(_, Expr::CallPython { module, function, ret, .. }) = &stmt.kind {
                calls.push((module.clone(), function.clone(), ret.clone()));
            }
        });
    }
    calls
}

#[test]
fn a_call_into_a_python_module_is_a_python_call_whichever_way_it_was_imported() {
    let py_error = Ty::Adt("PyError".into(), vec![]);
    let returns = Ty::Result(Box::new(Ty::Str), Box::new(py_error));
    for source in [
        "from textwrap import dedent\n\nfn f(s: str) -> str ! PyError:\n    return dedent(s)?\n",
        "import textwrap\n\nfn f(s: str) -> str ! PyError:\n    return textwrap.dedent(s)?\n",
    ] {
        let lowered = lowered_with(source, &textwrap());
        assert_eq!(
            python_calls(&lowered),
            [("textwrap".to_string(), "dedent".to_string(), returns.clone())],
            "{source}"
        );
    }
}

#[test]
fn each_python_import_is_kept_with_its_span_for_a_target_without_python() {
    let source = "from math import sqrt\nfrom textwrap import dedent\n\nfn f(s: str) -> str ! PyError:\n    print(sqrt(2.0))\n    return dedent(s)?\n";
    let lowered = lowered_with(source, &textwrap());
    let [(module, span)] = lowered.python_imports.as_slice() else {
        panic!("only textwrap is Python, math is the language's: {:?}", lowered.python_imports)
    };
    assert_eq!(module, "textwrap");
    assert!(source[span.start as usize..span.end as usize].starts_with("from textwrap import dedent"));
}

#[test]
fn the_ir_prints_one_statement_per_line_with_where_it_came_from() {
    let lowered = lowered("fn add(a: f64, b: f64) -> f64:\n    return a + b\n");
    assert_eq!(
        lowered.text(),
        "fn lf_add(%0 a: f64, %1 b: f64) -> f64 @1:1\n  let %2: f64 = Binary(Add, %0, %1, Float(F64)) @2:5\n  return %2 @2:5\n"
    );
}

#[test]
fn nested_blocks_print_indented_under_the_statement_that_holds_them() {
    let lowered = lowered("fn f(x: int) -> int:\n    if x > 0:\n        return 1\n    return 0\n");
    let text = lowered.text();
    assert!(text.contains("\n  if %"), "{text}");
    assert!(text.contains("\n    return 1_i64 @3:9\n"), "{text}");
    assert!(text.contains("\n  return 0_i64 @4:5\n"), "{text}");
}
