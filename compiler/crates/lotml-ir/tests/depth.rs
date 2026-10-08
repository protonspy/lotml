//! The functions whose calls count toward the recursion limit (specs/recursion-depth R1.1, R1.4,
//! R2.3): those in a cycle of the call graph, those used as a value and the methods called
//! through `dyn`, and no other.

use std::collections::BTreeSet;

use lotml_check::Interfaces;
use lotml_ir::depth::recursive;
use lotml_ir::lower::{Lowered, lower};
use lotml_ir::symbol;
use lotml_syntax::parse;

fn lowered(source: &str) -> Lowered {
    let parsed = parse(source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let checked = lotml_check::check_resolved_with(&parsed.module, source, &Interfaces::new());
    let errors: Vec<_> = checked.diagnostics.iter().filter(|d| d.severity == lotml_diag::Severity::Error).collect();
    assert!(errors.is_empty(), "{:#?}", errors.iter().map(|d| &d.message).collect::<Vec<_>>());
    lower(&parsed.module, &checked, source, true).unwrap_or_else(|d| panic!("{d:#?}"))
}

/// The functions of `source` that count, by their symbols.
fn counted(source: &str) -> BTreeSet<String> {
    recursive(&lowered(source))
}

fn functions(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|n| symbol::function(n)).collect()
}

#[test]
fn a_function_that_calls_itself_counts_and_its_callers_do_not() {
    let source = "fn fact(n: int) -> int:\n    if n <= 1:\n        return 1\n    return n * fact(n - 1)\n\n\
fn main():\n    print(fact(5))\n";
    assert_eq!(counted(source), functions(&["fact"]));
}

#[test]
fn functions_calling_each_other_count_and_what_they_call_does_not() {
    let source = "fn half(n: int) -> int:\n    return n - 1\n\n\
fn even(n: int) -> bool:\n    if n == 0:\n        return True\n    return odd(half(n))\n\n\
fn odd(n: int) -> bool:\n    if n == 0:\n        return False\n    return even(half(n))\n\n\
fn main():\n    print(even(10))\n";
    assert_eq!(counted(source), functions(&["even", "odd"]));
}

#[test]
fn a_program_without_recursion_counts_nothing() {
    let source = "fn double(n: int) -> int:\n    return n * 2\n\nfn main():\n    print(double(double(3)))\n";
    assert_eq!(counted(source), BTreeSet::new());
}

#[test]
fn a_function_used_as_a_value_and_every_lambda_count() {
    let source = "fn shout(w: str) -> str:\n    return w.upper()\n\nfn quiet(w: str) -> str:\n    return w.lower()\n\n\
fn main():\n    loud = shout\n    print(loud(\"a\"), list(map(quiet, [\"X\"])))\n    add = lambda x: x + 1\n    print(add(1))\n";
    let mut expected = functions(&["shout"]);
    expected.insert(symbol::lambda(0));
    assert_eq!(counted(source), expected);
}

const SHOW: &str = "trait Show:\n    fn show(self) -> str\n\ntype Dog(name: str)\ntype Car(model: str)\n\n\
impl Show for Dog:\n    fn show(self) -> str:\n        return self.name\n\n\
impl Show for Car:\n    fn show(self) -> str:\n        return self.model\n\n\
impl Dog:\n    fn bark(self) -> str:\n        return \"woof\"\n\n";

#[test]
fn a_method_of_a_type_made_a_dyn_value_counts() {
    let source = format!(
        "{SHOW}fn mixed(items: [dyn Show]) -> str:\n    return \", \".join([i.show() for i in items])\n\n\
fn main():\n    print(mixed([Dog(\"a\")]), Car(\"b\").show(), Dog(\"c\").bark())\n"
    );
    let expected: BTreeSet<String> = [symbol::method("Dog", "show")].into();
    let found = counted(&source);
    assert!(found.contains(&symbol::method("Dog", "show")), "{found:?}");
    assert!(!found.contains(&symbol::method("Dog", "bark")), "not a method of the trait: {found:?}");
    assert!(!found.contains(&symbol::function("mixed")), "{found:?}");
    assert!(found.is_subset(&expected.union(&[symbol::method("Car", "show")].into()).cloned().collect()), "{found:?}");
}

#[test]
fn a_recursive_method_and_a_recursive_generic_function_count() {
    let source = "type Node(value: int, rest: [Node])\n\n\
impl Node:\n    fn size(self) -> int:\n        var n = 1\n        for r in self.rest:\n            n += r.size()\n        return n\n\n\
fn depth[T](xs: [T], n: int) -> int:\n    if n == 0:\n        return len(xs)\n    return depth(xs, n - 1)\n\n\
fn main():\n    print(Node(1, []).size(), depth([1, 2], 3))\n";
    let mut expected = functions(&["depth"]);
    expected.insert(symbol::method("Node", "size"));
    assert_eq!(counted(source), expected);
}

#[test]
fn a_cycle_through_a_bound_method_of_a_type_parameter_counts() {
    let source = "trait Walk:\n    fn walk(self, n: int) -> int\n\ntype A(x: int)\n\n\
impl Walk for A:\n    fn walk(self, n: int) -> int:\n        return go(self, n - 1)\n\n\
fn go[T: Walk](x: T, n: int) -> int:\n    if n <= 0:\n        return 0\n    return x.walk(n)\n\n\
fn main():\n    print(go(A(1), 3))\n";
    let mut expected = functions(&["go"]);
    expected.insert(symbol::method("A", "walk"));
    assert_eq!(counted(source), expected);
}

#[test]
fn a_test_block_counts_only_when_something_calls_it() {
    let source = "fn fact(n: int) -> int:\n    if n <= 1:\n        return 1\n    return n * fact(n - 1)\n\n\
test \"fact\":\n    assert fact(3) == 6\n";
    assert_eq!(counted(source), functions(&["fact"]));
}

/// Every statement of `block` and of the blocks nested in it, in order.
fn statements(block: &lotml_ir::ir::Block) -> Vec<&lotml_ir::ir::StmtKind> {
    use lotml_ir::ir::StmtKind;
    let mut out = Vec::new();
    for stmt in block {
        out.push(&stmt.kind);
        match &stmt.kind {
            StmtKind::If(_, then, otherwise) => {
                out.extend(statements(then));
                out.extend(statements(otherwise));
            }
            StmtKind::Loop(body) => out.extend(statements(body)),
            StmtKind::ForRange { body, exit, .. } | StmtKind::ForStr { body, exit, .. } => {
                out.extend(statements(body));
                out.extend(statements(exit));
            }
            _ => {}
        }
    }
    out
}

fn function<'l>(lowered: &'l Lowered, name: &str) -> &'l lotml_ir::ir::Function {
    lowered.functions.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("no function {name}"))
}

/// Whether every `return` of `block`, nested ones included, comes straight after a `Leave`.
fn leaves_before_each_return(block: &lotml_ir::ir::Block) -> bool {
    use lotml_ir::ir::StmtKind;
    block.iter().enumerate().all(|(i, stmt)| match &stmt.kind {
        StmtKind::Return(_) => i > 0 && matches!(block[i - 1].kind, StmtKind::Leave),
        StmtKind::If(_, then, otherwise) => leaves_before_each_return(then) && leaves_before_each_return(otherwise),
        StmtKind::Loop(body) => leaves_before_each_return(body),
        StmtKind::ForRange { body, exit, .. } | StmtKind::ForStr { body, exit, .. } => {
            leaves_before_each_return(body) && leaves_before_each_return(exit)
        }
        _ => true,
    })
}

#[test]
fn a_counted_function_enters_first_and_leaves_on_every_return() {
    use lotml_ir::ir::StmtKind;
    let source = "fn fact(n: int) -> int:\n    if n <= 1:\n        return 1\n    for i in range(3):\n        if i == n:\n            return i\n    return n * fact(n - 1)\n\n\
fn main():\n    print(fact(5))\n";
    let lowered = lowered(source);
    let fact = function(&lowered, &symbol::function("fact"));
    assert!(matches!(fact.body[0].kind, StmtKind::Enter), "the body begins with Enter");
    let all = statements(&fact.body);
    let count = |pick: fn(&StmtKind) -> bool| all.iter().filter(|k| pick(k)).count();
    assert_eq!(count(|k| matches!(k, StmtKind::Enter)), 1);
    assert_eq!(count(|k| matches!(k, StmtKind::Return(_))), 3);
    assert_eq!(count(|k| matches!(k, StmtKind::Leave)), 3, "one for each return");
    assert!(leaves_before_each_return(&fact.body));
    let main = function(&lowered, &symbol::function("main"));
    assert!(
        !statements(&main.body).iter().any(|k| matches!(k, StmtKind::Enter | StmtKind::Leave)),
        "main is not counted"
    );
}

#[test]
fn a_counted_function_that_ends_without_a_return_leaves_at_its_end() {
    use lotml_ir::ir::StmtKind;
    let source = "fn down(n: int):\n    if n > 0:\n        down(n - 1)\n\nfn main():\n    down(3)\n";
    let lowered = lowered(source);
    let down = function(&lowered, &symbol::function("down"));
    assert!(matches!(down.body.first().map(|s| &s.kind), Some(StmtKind::Enter)));
    assert!(matches!(down.body.last().map(|s| &s.kind), Some(StmtKind::Leave)), "{:#?}", down.body.last());
    assert!(leaves_before_each_return(&down.body));
}

#[test]
fn a_counted_function_that_fails_leaves_before_it_returns_the_error() {
    use lotml_ir::ir::StmtKind;
    let source = "fn parse(n: int) -> int ! str:\n    if n < 0:\n        fail \"negative\"\n    if n == 0:\n        return 0\n    return parse(n - 1)? + 1\n\n\
fn main():\n    print(parse(3))\n";
    let lowered = lowered(source);
    let parse = function(&lowered, &symbol::function("parse"));
    let all = statements(&parse.body);
    let returns = all.iter().filter(|k| matches!(k, StmtKind::Return(_))).count();
    assert!(returns >= 3, "the fail, the `?` and the two returns all return: {returns}");
    assert_eq!(all.iter().filter(|k| matches!(k, StmtKind::Leave)).count(), returns);
    assert!(leaves_before_each_return(&parse.body));
}

#[test]
fn an_instance_of_a_counted_generic_function_counts_too() {
    use lotml_ir::ir::StmtKind;
    let source = "fn depth[T](xs: [T], n: int) -> int:\n    if n == 0:\n        return len(xs)\n    return depth(xs, n - 1)\n\n\
fn main():\n    print(depth([1, 2], 3), depth([\"a\"], 1))\n";
    let monomorphic = lotml_ir::mono::mono(lowered(source)).unwrap_or_else(|d| panic!("{d:#?}"));
    let instances: Vec<_> =
        monomorphic.functions.iter().filter(|f| f.name.ends_with(&symbol::function("depth"))).collect();
    assert_eq!(instances.len(), 2);
    for f in instances {
        assert!(matches!(f.body[0].kind, StmtKind::Enter), "{}", f.name);
        assert!(leaves_before_each_return(&f.body), "{}", f.name);
    }
}
