//! Lowering a checked program into the IR (specs/shared-ir).

use lotml_ir::ir::{Block, Function, Stmt, StmtKind};
use lotml_ir::lower::{Lowered, lower};
use lotml_syntax::parse;
use lotml_syntax::span::Span;

fn lowered(source: &str) -> Lowered {
    let parsed = parse(source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let checked = lotml_check::check_resolved(&parsed.module, source);
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
