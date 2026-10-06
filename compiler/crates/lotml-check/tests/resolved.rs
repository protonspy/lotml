//! What each name of a local refers to: the declaration an editor jumps to and a rename
//! changes together with every use.

use lotml_check::check_resolved;
use lotml_syntax::parse;

/// The byte offset of the `n`th (from 0) whole-word occurrence of `word` in `source`.
fn at(source: &str, word: &str, n: usize) -> u32 {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let found = source
        .match_indices(word)
        .filter(|(i, _)| {
            let before = source[..*i].chars().next_back();
            let after = source[i + word.len()..].chars().next();
            !before.is_some_and(is_word) && !after.is_some_and(is_word)
        })
        .nth(n)
        .unwrap_or_else(|| panic!("no occurrence {n} of `{word}`"));
    u32::try_from(found.0).expect("a small source")
}

/// Where the name starting at `offset` was declared, if it names a local.
fn declaration(source: &str, offset: u32) -> Option<u32> {
    let checked = check_resolved(&parse(source).module, source);
    assert!(
        checked.diagnostics.is_empty(),
        "the example should check clean: {:?}",
        checked.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    let found: Vec<u32> =
        checked.locals.iter().filter(|(at, _)| at.start == offset).map(|(_, declared)| declared.start).collect();
    assert!(found.windows(2).all(|w| w[0] == w[1]), "one name resolved two ways: {found:?}");
    found.first().copied()
}

#[test]
fn a_use_refers_to_its_declaration() {
    let source = "fn f() -> int:\n    total = 1\n    return total + total\n";
    let declared = at(source, "total", 0);
    assert_eq!(declaration(source, declared), Some(declared), "a declaration refers to itself");
    assert_eq!(declaration(source, at(source, "total", 1)), Some(declared));
    assert_eq!(declaration(source, at(source, "total", 2)), Some(declared));
}

#[test]
fn a_parameter_is_a_declaration() {
    let source = "fn double(n: int) -> int:\n    return n * 2\n";
    assert_eq!(declaration(source, at(source, "n", 1)), Some(at(source, "n", 0)));
}

#[test]
fn an_assignment_to_a_var_refers_to_the_declaration() {
    let source = "fn f() -> int:\n    var count = 0\n    count = count + 1\n    count += 1\n    return count\n";
    let declared = at(source, "count", 0);
    for n in 1..=4 {
        assert_eq!(declaration(source, at(source, "count", n)), Some(declared), "occurrence {n}");
    }
}

#[test]
fn a_local_declared_in_every_branch_is_one_local() {
    let source = "fn f(c: bool) -> int:\n    if c:\n        x = 1\n    else:\n        x = 2\n    return x\n";
    let first = at(source, "x", 0);
    assert_eq!(declaration(source, at(source, "x", 1)), Some(first), "the second branch's declaration");
    assert_eq!(declaration(source, at(source, "x", 2)), Some(first), "the use after the branches");
}

#[test]
fn a_lambda_parameter_shadows_a_local() {
    let source = "fn f(xs: [int]) -> [int]:\n    x = 10\n    return sorted([x + 1 for x in xs], key=lambda x: x * x)\n";
    // `x` of the comprehension and of the lambda are their own locals, not the `x = 10` above.
    let comprehension = at(source, "x", 2);
    assert_eq!(declaration(source, at(source, "x", 1)), Some(comprehension));
    let lambda = at(source, "x", 3);
    assert_eq!(declaration(source, at(source, "x", 4)), Some(lambda));
    assert_eq!(declaration(source, at(source, "x", 5)), Some(lambda));
}

#[test]
fn a_lambda_sees_the_local_it_captures() {
    let source = "fn f() -> int:\n    var base = 3\n    add = lambda y: y + base\n    return add(1)\n";
    assert_eq!(declaration(source, at(source, "base", 1)), Some(at(source, "base", 0)));
}

#[test]
fn for_and_case_bind_locals() {
    let source = "fn f(xs: [int], o: int?) -> int:\n    var t = 0\n    for item in xs:\n        t += item\n    match o:\n        case None:\n            return t\n        case v:\n            return v + t\n";
    assert_eq!(declaration(source, at(source, "item", 1)), Some(at(source, "item", 0)));
    assert_eq!(declaration(source, at(source, "v", 1)), Some(at(source, "v", 0)));
}

#[test]
fn a_narrowed_name_still_refers_to_its_declaration() {
    let source = "fn f(o: int?) -> int:\n    if o is not None:\n        return o\n    return 0\n";
    let declared = at(source, "o", 0);
    assert_eq!(declaration(source, at(source, "o", 2)), Some(declared));
}

#[test]
fn a_function_or_a_type_is_not_a_local() {
    let source = "type P(x: int)\n\nfn g() -> int:\n    return 1\n\nfn f() -> int:\n    p = P(x=g())\n    return p.x\n";
    assert_eq!(declaration(source, at(source, "g", 1)), None);
    assert_eq!(declaration(source, at(source, "P", 1)), None);
}
