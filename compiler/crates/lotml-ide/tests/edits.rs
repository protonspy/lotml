//! Edits addressed to symbols, search and replace by whole lines, and atomic renames (R32): the
//! tool takes the indentation from the target, refuses an edit that breaks the syntax, and a
//! rename either changes every reference or nothing.

use std::path::Path;

use lotml_ide::edit::{Part, add, remove, replace, search_replace};
use lotml_ide::{Symbol, Workspace};

const COUNTER: &str = "\
type Shape = Circle(r: f64) | Empty

type Counter(count: int)

impl Counter:
    fn get(self) -> int:
        return self.count

    fn bump(inout self):
        self.count += 1

fn area(s: Shape) -> f64:
    # area is half of what twice returns
    match s:
        case Circle(r):
            return 3.0 * r * r
        case Empty:
            return 0.0
";

fn clean(text: &str) {
    let found = lotml_check::check_source(text);
    assert!(
        found.is_empty(),
        "expected a clean program:\n{text}\n{:?}",
        found.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn a_body_takes_the_indentation_of_the_one_it_replaces() {
    let changed = replace(
        COUNTER,
        "area",
        &Part::Body,
        "match s:\n    case Circle(r):\n        return 3.14 * r * r\n    case Empty:\n        return 0.0",
    )
    .unwrap();
    assert!(changed.text.contains(
        "fn area(s: Shape) -> f64:\n    match s:\n        case Circle(r):\n            return 3.14 * r * r\n"
    ));
    assert!(changed.original.starts_with("# area is half"), "the comment opening the body is part of it");
    clean(&changed.text);
}

#[test]
fn a_method_written_flush_lands_inside_its_impl() {
    let changed =
        replace(COUNTER, "Counter.get", &Part::Definition, "fn get(self) -> int:\n    return self.count * 1").unwrap();
    assert!(
        changed.text.contains("impl Counter:\n    fn get(self) -> int:\n        return self.count * 1\n\n    fn bump")
    );
    assert_eq!(changed.original, "fn get(self) -> int:\n        return self.count");
    clean(&changed.text);
}

#[test]
fn an_arm_is_found_by_its_variant_or_its_pattern() {
    let by_variant =
        replace(COUNTER, "area", &Part::Arm("Circle".into()), "case Circle(radius):\n    return radius * radius")
            .unwrap();
    assert!(
        by_variant
            .text
            .contains("        case Circle(radius):\n            return radius * radius\n        case Empty:")
    );
    clean(&by_variant.text);
    let by_pattern = replace(COUNTER, "area", &Part::Arm("case Empty:".into()), "case Empty:\n    return 1.0").unwrap();
    assert!(by_pattern.text.contains("        case Empty:\n            return 1.0\n"));
    let missing = replace(COUNTER, "area", &Part::Arm("Square".into()), "case Square:\n    return 0.0").unwrap_err();
    assert!(missing.0.contains("its arms are Circle(r), Empty"), "{}", missing.0);
}

#[test]
fn an_edit_that_breaks_the_syntax_is_refused_with_what_was_there() {
    let refused = replace(COUNTER, "Counter.get", &Part::Body, "return (self.count").unwrap_err();
    assert!(refused.0.starts_with("the edit was not made: it breaks the syntax"), "{}", refused.0);
    assert!(refused.0.contains("attempted:\nreturn (self.count\n\noriginal:\nreturn self.count"), "{}", refused.0);
}

#[test]
fn an_unknown_symbol_is_refused_with_what_the_file_declares() {
    let refused = replace(COUNTER, "Counter.reset", &Part::Body, "pass").unwrap_err();
    assert!(refused.0.contains("Counter.bump") && refused.0.contains("area"), "{}", refused.0);
    let a_type = replace(COUNTER, "Shape", &Part::Body, "pass").unwrap_err();
    assert!(a_type.0.contains("replace its whole definition"));
}

#[test]
fn a_declaration_is_added_after_another_or_at_the_end() {
    let method = add(COUNTER, Some("Counter.get"), "fn reset(inout self):\n    self.count = 0").unwrap();
    assert!(
        method
            .text
            .contains("        return self.count\n\n    fn reset(inout self):\n        self.count = 0\n\n    fn bump")
    );
    clean(&method.text);
    let function = add(COUNTER, None, "fn zero() -> int:\n    return 0\n").unwrap();
    assert!(function.text.ends_with("            return 0.0\n\nfn zero() -> int:\n    return 0\n"));
    clean(&function.text);
}

#[test]
fn a_removed_declaration_takes_its_blank_line_with_it() {
    let method = remove(COUNTER, "Counter.get").unwrap();
    assert!(method.text.contains("impl Counter:\n    fn bump(inout self):"));
    let last = remove(COUNTER, "area").unwrap();
    assert!(last.text.ends_with("        self.count += 1\n"), "{:?}", last.text);
    let first = remove(COUNTER, "Shape").unwrap();
    assert!(first.text.starts_with("type Counter(count: int)\n\nimpl Counter:"));
}

#[test]
fn search_and_replace_matches_whole_lines_up_to_one_offset() {
    let exact = search_replace(COUNTER, "        self.count += 1\n", "        self.count += 2\n").unwrap();
    assert!(exact.text.contains("self.count += 2"));
    // Written at the wrong depth, the lines still match, and the replacement moves with them.
    let shifted = search_replace(COUNTER, "case Empty:\n    return 0.0", "case Empty:\n    return -1.0").unwrap();
    assert!(shifted.text.contains("        case Empty:\n            return -1.0\n"));
    let ambiguous = search_replace(COUNTER, "    fn", "    fn").unwrap_err();
    assert!(ambiguous.0.contains("occurs 2 times"), "{}", ambiguous.0);
    let inside = search_replace(COUNTER, "match s:\ncase Empty:", "x").unwrap_err();
    assert!(inside.0.contains("not in the file"), "{}", inside.0);
    let skewed = search_replace(COUNTER, "    case Empty:\n    return 0.0", "x").unwrap_err();
    assert!(skewed.0.contains("different indentation"), "{}", skewed.0);
}

fn workspace() -> Workspace {
    let mut ws = Workspace::new();
    ws.set(Path::new("counter.lotml"), COUNTER.to_string());
    ws.set(Path::new("notes.lotml"), "# call area on each shape\nfn area() -> int:\n    return 1\n".into());
    ws
}

#[test]
fn a_rename_changes_every_reference_and_lists_textual_mentions() {
    let ws = workspace();
    let path = Path::new("counter.lotml");
    let renamed = ws.rename(path, &Symbol::Member("Counter".into(), "count".into()), "total").unwrap();
    assert_eq!(renamed.sites.len(), 3);
    assert!(renamed.text.contains("type Counter(total: int)") && renamed.text.contains("self.total += 1"));
    clean(&renamed.text);

    let area = ws.rename(path, &Symbol::Item("area".into()), "surface").unwrap();
    let mentioned: Vec<(String, String)> = area
        .mentions
        .iter()
        .map(|(p, s)| {
            let text = if p == path { area.text.clone() } else { ws.text(p).unwrap().to_string() };
            (p.display().to_string(), text[..s.start as usize].lines().last().unwrap_or("").to_string())
        })
        .collect();
    // The comment in this file, and the comment in the other one — not the other file's own `area`.
    assert_eq!(mentioned, vec![("counter.lotml".into(), "    # ".into()), ("notes.lotml".into(), "# call ".into())]);
}

#[test]
fn a_rename_that_would_capture_or_collide_is_refused() {
    let ws = workspace();
    let path = Path::new("counter.lotml");
    let get = ws.rename(path, &Symbol::Member("Counter".into(), "get".into()), "bump").unwrap_err();
    assert!(get.0.contains("would"), "{}", get.0);
    let area = ws.rename(path, &Symbol::Item("area".into()), "Shape").unwrap_err();
    assert!(area.0.contains("would"), "{}", area.0);
    let keyword = ws.rename(path, &Symbol::Item("area".into()), "match").unwrap_err();
    assert!(keyword.0.contains("keyword"));

    let mut shadowed = Workspace::new();
    let text = "fn f(a: int, b: int) -> int:\n    return a + b\n";
    shadowed.set(Path::new("s.lotml"), text.into());
    let a = shadowed.at(Path::new("s.lotml"), 5).unwrap().symbol.clone();
    let refused = shadowed.rename(Path::new("s.lotml"), &a, "b").unwrap_err();
    assert!(refused.0.contains("would"), "{}", refused.0);
}

#[test]
fn the_check_on_an_edit_reports_only_what_it_introduced() {
    let mut ws = Workspace::new();
    let path = Path::new("x.lotml");
    ws.set(path, "fn f() -> int:\n    return missing\n".into());
    let edited = "fn f() -> int:\n    return missing\n\nfn g() -> int:\n    return \"no\"\n";
    let introduced: Vec<&str> = ws.introduced(path, edited).iter().map(|d| d.code).collect();
    assert_eq!(introduced, vec!["E0204"], "the old error is not new");
}
