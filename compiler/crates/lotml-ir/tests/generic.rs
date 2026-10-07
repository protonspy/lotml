//! The generic IR and the pass that makes it monomorphic (specs/python-on-ir R4.1, R4.2): lowering
//! writes each generic function, method and lambda once, every use carrying its type arguments;
//! `mono` writes an instance per set of them, and leaves nothing generic for a native backend.

use lotml_check::Interfaces;
use lotml_check::ty::{IntKind, Ty};
use lotml_ir::ir::{Callee, Expr, Function, block_exprs};
use lotml_ir::lower::{Lowered, lower};
use lotml_ir::mono::mono;
use lotml_syntax::parse;

const PROGRAM: &str = "type Box[T](v: T)\n\n\
impl Box[T]:\n    fn get(self) -> T:\n        return self.v\n\n\
trait Show:\n    fn show(self) -> str\n\n\
type Dot(n: int)\n\n\
impl Show for Dot:\n    fn show(self) -> str:\n        return str(self.n)\n\n\
fn first[T](xs: [T]) -> T:\n    return xs[0]\n\n\
fn describe[T: Show](x: T) -> str:\n    return x.show()\n\n\
fn twice[T](xs: [T]) -> [[T]]:\n    pair = lambda x: [x, x]\n    return [pair(x) for x in xs]\n\n\
fn main():\n    print(first([1]), first([\"a\"]), Box(2.5).get(), describe(Dot(3)), twice([1]), twice([\"b\"]))\n";

fn lowered() -> Lowered {
    let parsed = parse(PROGRAM);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let checked = lotml_check::check_resolved_with(&parsed.module, PROGRAM, &Interfaces::new());
    let errors: Vec<_> = checked.diagnostics.iter().filter(|d| d.severity == lotml_diag::Severity::Error).collect();
    assert!(errors.is_empty(), "{:#?}", errors.iter().map(|d| &d.message).collect::<Vec<_>>());
    lower(&parsed.module, &checked, PROGRAM, false).unwrap_or_else(|d| panic!("{d:#?}"))
}

fn exprs(f: &Function) -> Vec<Expr> {
    let mut out = Vec::new();
    block_exprs(&f.body, &mut |e| out.push(e.clone()));
    out
}

fn named<'l>(lowered: &'l Lowered, name: &str) -> Vec<&'l Function> {
    lowered.functions.iter().filter(|f| f.source_name == name).collect()
}

#[test]
fn a_generic_function_method_and_lambda_are_lowered_once_and_stay_generic() {
    let lowered = lowered();
    for (name, params) in [("first", &["T"][..]), ("get", &["T"]), ("describe", &["T"]), ("twice", &["T"])] {
        let found = named(&lowered, name);
        assert_eq!(found.len(), 1, "{name} is lowered once:\n{}", lowered.text());
        assert_eq!(found[0].type_params, params, "{name}");
    }
    let first = named(&lowered, "first")[0];
    assert!(first.locals.iter().any(|l| l.ty == Ty::list(Ty::Param("T".into()))), "{}", lowered.function_text(first));
    let lambda = named(&lowered, "<lambda>");
    assert_eq!(lambda.len(), 1);
    assert_eq!(lambda[0].type_params, ["T"], "a lambda of a generic function is generic over its parameters");
}

#[test]
fn each_use_of_a_generic_carries_its_type_arguments() {
    let lowered = lowered();
    let main = named(&lowered, "main")[0];
    let callees: Vec<Callee> = exprs(main)
        .into_iter()
        .filter_map(|e| if let Expr::CallGeneric { callee, .. } = e { Some(callee) } else { None })
        .collect();
    let int = Ty::Int(IntKind::I64);
    for wanted in [
        Callee::Function { name: "first".into(), type_args: vec![int.clone()] },
        Callee::Function { name: "first".into(), type_args: vec![Ty::Str] },
        Callee::Method {
            owner: Ty::Adt("Box".into(), vec![Ty::Float(lotml_check::ty::FloatKind::F64)]),
            method: "get".into(),
            own: vec![],
        },
        Callee::Function { name: "describe".into(), type_args: vec![Ty::Adt("Dot".into(), vec![])] },
        Callee::Function { name: "twice".into(), type_args: vec![int] },
    ] {
        assert!(callees.contains(&wanted), "{wanted:?} in {callees:?}");
    }
    let describe = named(&lowered, "describe")[0];
    let through_bound = exprs(describe).into_iter().any(|e| {
        matches!(e, Expr::CallGeneric { callee: Callee::Method { owner: Ty::Param(ref p), ref method, .. }, .. } if p == "T" && method == "show")
    });
    assert!(through_bound, "x.show() on a T bound by Show:\n{}", lowered.function_text(describe));
}

#[test]
fn mono_writes_an_instance_per_set_of_type_arguments_and_nothing_generic() {
    let lowered = mono(lowered()).unwrap_or_else(|d| panic!("{d:#?}"));
    assert!(lowered.functions.iter().all(|f| f.type_params.is_empty()), "{}", lowered.text());
    for f in &lowered.functions {
        for e in exprs(f) {
            assert!(
                !matches!(e, Expr::CallGeneric { .. } | Expr::FnRefGeneric { .. } | Expr::ToDynOf { .. }),
                "{}",
                lowered.function_text(f)
            );
        }
    }
    assert_eq!(named(&lowered, "first").len(), 2, "first for int and for str");
    assert_eq!(named(&lowered, "twice").len(), 2);
    assert_eq!(named(&lowered, "get").len(), 1);
    let lambdas = named(&lowered, "<lambda>");
    assert_eq!(lambdas.len(), 2, "the lambda of twice, once per instance of twice");
    assert_eq!(lowered.lambdas.len(), 2);
    let elements: Vec<&Ty> = lowered.lambdas.iter().map(|(_, ty)| ty).collect();
    assert!(elements.iter().all(|ty| !format!("{ty}").contains('T')), "{elements:?}");
    let describe = &named(&lowered, "describe")[0];
    let calls: Vec<String> = exprs(describe)
        .into_iter()
        .filter_map(|e| if let Expr::Call(name, _) = e { Some(name) } else { None })
        .collect();
    assert_eq!(calls, [lotml_ir::symbol::method("Dot", "show")], "x.show() on a Dot calls Dot's show");
}
