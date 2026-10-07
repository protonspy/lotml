//! The IR's verifier (specs/shared-ir R1.6): functions written by hand, each breaking one rule.

use lotml_check::ty::{IntKind, Ty};
use lotml_ir::ir::{Const, Expr, Function, LocalInfo, Operand, Stmt, StmtKind};
use lotml_ir::verify::verify;
use lotml_syntax::span::Span;

const INT: Ty = Ty::Int(IntKind::I64);

fn stmt(at: usize, kind: StmtKind) -> Stmt {
    Stmt { span: Span::new(at, at + 1), kind }
}

/// A function of `params` and `locals` (the parameters first) returning `int`.
fn function(params: usize, locals: Vec<Ty>, body: Vec<Stmt>) -> Function {
    Function {
        name: "lf_f".to_string(),
        source_name: "f".to_string(),
        type_params: Vec::new(),
        params: (0..params).collect(),
        ret: INT,
        locals: locals.into_iter().map(|ty| LocalInfo { ty, name: None, by_ref: false }).collect(),
        body,
        span: Span::new(0, 1),
    }
}

fn local(l: usize) -> Operand {
    Operand::Local(l)
}

fn one() -> Operand {
    Operand::Const(Const::Int(1, IntKind::I64))
}

#[test]
fn a_local_read_before_it_is_set_is_reported_with_the_pass_and_the_statement() {
    let f = function(0, vec![INT], vec![stmt(7, StmtKind::Return(Some(local(0))))]);
    let findings = verify(&f, "counting");
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].contains("after counting"), "{}", findings[0]);
    assert!(findings[0].contains("bytes 7..8"), "{}", findings[0]);
    assert!(findings[0].contains("%0 is read before it is set"), "{}", findings[0]);
}

#[test]
fn a_local_set_on_one_branch_only_is_not_set_after_the_if() {
    let body = |otherwise: Vec<Stmt>| {
        vec![
            stmt(0, StmtKind::If(local(0), vec![stmt(1, StmtKind::Let(1, Expr::Use(one())))], otherwise)),
            stmt(2, StmtKind::Return(Some(local(1)))),
        ]
    };
    let one_branch = function(1, vec![Ty::Bool, INT], body(Vec::new()));
    assert_eq!(verify(&one_branch, "lowering").len(), 1);
    let both = function(1, vec![Ty::Bool, INT], body(vec![stmt(3, StmtKind::Let(1, Expr::Use(one())))]));
    assert_eq!(verify(&both, "lowering"), Vec::<String>::new());
    let returns = function(1, vec![Ty::Bool, INT], body(vec![stmt(3, StmtKind::Return(Some(one())))]));
    assert_eq!(verify(&returns, "lowering"), Vec::<String>::new(), "a branch that returns sets nothing after");
}

#[test]
fn what_every_break_of_a_loop_has_set_is_set_after_it() {
    let f = function(
        0,
        vec![INT],
        vec![
            stmt(0, StmtKind::Loop(vec![stmt(1, StmtKind::Let(0, Expr::Use(one()))), stmt(2, StmtKind::Break)])),
            stmt(3, StmtKind::Return(Some(local(0)))),
        ],
    );
    assert_eq!(verify(&f, "lowering"), Vec::<String>::new());
    let early = function(
        0,
        vec![INT],
        vec![
            stmt(0, StmtKind::Loop(vec![stmt(1, StmtKind::Break), stmt(2, StmtKind::Let(0, Expr::Use(one())))])),
            stmt(3, StmtKind::Return(Some(local(0)))),
        ],
    );
    assert_eq!(verify(&early, "lowering").len(), 1, "the break comes before the local is set");
}

#[test]
fn a_local_given_a_value_of_another_type_is_reported() {
    let f = function(
        0,
        vec![INT],
        vec![
            stmt(0, StmtKind::Let(0, Expr::Use(Operand::Const(Const::Bool(true))))),
            stmt(1, StmtKind::Return(Some(local(0)))),
        ],
    );
    let findings = verify(&f, "reuse");
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].contains("%0, of type int, is given a value of type bool"), "{}", findings[0]);
}

#[test]
fn a_condition_that_is_not_a_bool_and_a_returned_value_of_the_wrong_type_are_reported() {
    let f = function(
        0,
        vec![],
        vec![
            stmt(0, StmtKind::If(one(), Vec::new(), Vec::new())),
            stmt(1, StmtKind::Return(Some(Operand::Const(Const::Str("no".into()))))),
        ],
    );
    let findings = verify(&f, "hoisting");
    assert_eq!(findings.len(), 2, "{findings:?}");
    assert!(findings[0].contains("the condition is int where bool is required"), "{}", findings[0]);
    assert!(findings[1].contains("the value returned is str where int is required"), "{}", findings[1]);
}

#[test]
fn an_operand_of_an_arithmetic_operation_of_another_type_is_reported() {
    use lotml_ir::ir::BinOp;
    let f = function(
        1,
        vec![Ty::Str, INT],
        vec![
            stmt(0, StmtKind::Let(1, Expr::Binary(BinOp::Add, local(0), one(), INT))),
            stmt(1, StmtKind::Return(Some(local(1)))),
        ],
    );
    let findings = verify(&f, "lowering");
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].contains("the left operand is str where int is required"), "{}", findings[0]);
}
