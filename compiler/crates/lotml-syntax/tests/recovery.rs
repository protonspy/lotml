//! A file an editor or an agent is halfway through writing still yields its outline: an unclosed
//! bracket costs the statement it is in, never the definitions after it (plans/frontend-robustness.md
//! 1.1).

use lotml_syntax::ast::Item;
use lotml_syntax::parse;

fn names(source: &str) -> Vec<String> {
    parse(source)
        .module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Fn(f) => Some(f.name.name.clone()),
            Item::Record(r) => Some(r.name.name.clone()),
            Item::Sum(s) => Some(s.name.name.clone()),
            Item::Test(t) => Some(t.name.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn an_unclosed_parenthesis_does_not_hide_the_functions_after_it() {
    let source = "fn first() -> int:\n    x = total(1, 2,\n    return x\n\nfn second() -> int:\n    return 2\n\nfn third() -> int:\n    return 3\n";
    assert_eq!(names(source), ["first", "second", "third"]);
}

#[test]
fn every_kind_of_definition_at_column_zero_ends_the_unclosed_bracket() {
    for (opener, definition, name) in [
        ("(", "type Point(x: int, y: int)\n", "Point"),
        ("[", "fn later() -> int:\n    return 1\n", "later"),
        ("{", "test \"later\":\n    assert True\n", "later"),
    ] {
        let source = format!("fn first() -> int:\n    x = f({opener}1,\n\n{definition}");
        assert!(names(&source).contains(&name.to_string()), "{opener} hid {name}: {:?}", names(&source));
    }
}

#[test]
fn the_unclosed_bracket_is_reported_where_it_opens() {
    let source = "fn first() -> int:\n    x = total(1, 2,\n\nfn second() -> int:\n    return 2\n";
    let parsed = parse(source);
    let at = source.find("total(").unwrap() + "total".len();
    let error = parsed.errors.iter().find(|e| e.message.contains("never closed")).expect("the bracket is reported");
    assert_eq!(error.span.start as usize, at, "{error:?}");
}

#[test]
fn a_bracket_continued_at_column_zero_without_a_definition_is_not_cut() {
    let source = "fn first() -> int:\n    return total(\n1, 2)\n\nfn second() -> int:\n    return 2\n";
    let parsed = parse(source);
    assert!(parsed.errors.iter().all(|e| !e.message.contains("never closed")), "{:?}", parsed.errors);
    assert_eq!(names(source), ["first", "second"]);
}

#[test]
fn a_name_that_starts_like_a_keyword_does_not_end_the_bracket() {
    let source = "fn first() -> int:\n    return total(\nfrom_value, types)\n";
    let parsed = parse(source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
}

#[test]
fn recovery_reads_a_file_with_windows_line_endings() {
    let source = "fn first() -> int:\n    x = total(1, 2,\n\nfn second() -> int:\n    return 2\n".replace('\n', "\r\n");
    assert_eq!(names(&source), ["first", "second"]);
}
