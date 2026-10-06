//! Where a change falls and which one edit makes it (specs/guide-records/ R1.1-R1.4).

use lotml_ide::diff::{Declaration, changed_lines, diff};

const STATS: &str = "\
type Stats(count: int, total: int)

impl Stats:
    fn mean(self) -> f64:
        return float(self.total) / float(self.count)

fn median(xs: [f64]) -> f64?:
    if len(xs) == 0:
        return None
    var sorted = xs
    sorted.sort()
    return sorted[len(sorted) // 2]

fn describe(s: Stats) -> str:
    match s.count:
        case 0:
            return \"empty\"
        case _:
            return \"some\"

test \"median\":
    assert median([1.0, 2.0, 3.0]) == 2.0
";

fn declaration(symbol: &str, kind: &'static str, lines: [u32; 2]) -> Declaration {
    Declaration { symbol: Some(symbol.into()), kind, lines }
}

#[test]
fn changed_lines_are_the_first_version_s_lines_a_change_removed_or_replaced() {
    let before = "a\nb\nc\nd\n";
    assert_eq!(changed_lines(before, "a\nB\nc\nd\n"), [2]);
    assert_eq!(changed_lines(before, "a\nc\nd\n"), [2]);
    assert_eq!(changed_lines(before, "a\nb\nc\nd\ne\n"), [4], "an insertion names the line it follows");
    assert_eq!(changed_lines(before, "z\na\nb\nc\nd\n"), [0], "at the file's start, line 0");
    assert_eq!(changed_lines(before, "a\nB\nc\nD\n"), [2, 4], "two changes apart stay apart");
    assert!(changed_lines(before, before).is_empty());
}

#[test]
fn a_change_inside_a_method_names_the_method_with_its_lines() {
    let after = STATS.replace("float(self.total) / float(self.count)", "float(self.total)");
    assert_eq!(diff(STATS, &after, "stats.lotml").declarations, [declaration("Stats.mean", "method", [4, 5])]);
}

#[test]
fn a_change_inside_a_function_s_match_names_the_function() {
    let after = STATS.replace("return \"empty\"", "return \"none\"");
    assert_eq!(diff(STATS, &after, "stats.lotml").declarations, [declaration("describe", "function", [14, 19])]);
}

#[test]
fn changes_in_two_declarations_name_both_innermost_first() {
    let after = STATS
        .replace("type Stats(count: int, total: int)", "type Stats(count: int, total: f64)")
        .replace("    if len(xs) == 0:\n        return None\n    var sorted = xs\n", "    var sorted = xs\n");
    let found = diff(STATS, &after, "stats.lotml").declarations;
    assert_eq!(found, [declaration("Stats", "record", [1, 1]), declaration("median", "function", [7, 12])]);
}

#[test]
fn a_declaration_only_the_second_version_has_names_the_one_it_follows_or_the_file_s_start() {
    let added = STATS.replace("fn describe", "fn double(x: int) -> int:\n    return 2 * x\n\nfn describe");
    assert_eq!(diff(STATS, &added, "stats.lotml").declarations, [declaration("median", "function", [7, 12])]);
    let first = format!("fn first() -> int:\n    return 1\n\n{STATS}");
    assert_eq!(
        diff(STATS, &first, "stats.lotml").declarations,
        [Declaration { symbol: None, kind: "file", lines: [1, 1] }]
    );
}

#[test]
fn a_test_block_is_named_as_test_and_its_name() {
    let after = STATS.replace("== 2.0", "== 3.0");
    assert_eq!(diff(STATS, &after, "stats.lotml").declarations, [declaration("test \"median\"", "test", [21, 22])]);
}

/// `before` with `edit` applied through the edit functions the MCP tools apply.
fn apply(before: &str, edit: &lotml_ide::diff::Edit) -> String {
    use lotml_ide::edit::{self, Part};
    let arg = |key: &str| edit.arguments.iter().find(|(k, _)| *k == key).map(|(_, v)| v.as_str());
    assert_eq!(arg("path"), Some("stats.lotml"), "every edit names its file");
    let changed = match edit.tool {
        "replace" => {
            let part = match arg("part") {
                Some("body") => Part::Body,
                Some("arm") => Part::Arm(arg("arm").unwrap().to_string()),
                _ => Part::Definition,
            };
            edit::replace(before, arg("symbol").unwrap(), &part, arg("text").unwrap())
        }
        "add" => edit::add(before, arg("after"), arg("text").unwrap()),
        "remove" => edit::remove(before, arg("symbol").unwrap()),
        "edit" => edit::search_replace(before, arg("search").unwrap(), arg("replace").unwrap()),
        other => panic!("no tool {other}"),
    };
    changed.unwrap().text
}

fn reproduced(after: &str) -> (&'static str, Vec<(&'static str, String)>) {
    let found = diff(STATS, after, "stats.lotml").edit.expect("an edit");
    assert_eq!(apply(STATS, &found), after, "the edit reproduces the change byte for byte");
    (found.kind(), found.arguments)
}

fn argument<'a>(arguments: &'a [(&'static str, String)], key: &str) -> Option<&'a str> {
    arguments.iter().find(|(k, _)| *k == key).map(|(_, v)| v.as_str())
}

#[test]
fn a_change_inside_one_arm_is_that_arm() {
    let (kind, arguments) = reproduced(&STATS.replace("return \"empty\"", "return \"none\""));
    assert_eq!(kind, "arm");
    assert_eq!((argument(&arguments, "symbol"), argument(&arguments, "arm")), (Some("describe"), Some("0")));
}

#[test]
fn a_change_inside_a_body_is_the_body() {
    let (kind, arguments) = reproduced(&STATS.replace("sorted[len(sorted) // 2]", "sorted[len(sorted) // 2 - 1]"));
    assert_eq!(kind, "body");
    assert_eq!(argument(&arguments, "symbol"), Some("median"));
}

#[test]
fn a_changed_signature_is_the_definition_and_so_is_a_changed_type() {
    let (kind, _) = reproduced(&STATS.replace("fn median(xs: [f64]) -> f64?:", "fn median(xs: [int]) -> f64?:"));
    assert_eq!(kind, "definition");
    let (kind, arguments) = reproduced(&STATS.replace("total: int)", "total: f64)"));
    assert_eq!((kind, argument(&arguments, "symbol")), ("definition", Some("Stats")));
}

#[test]
fn a_declaration_only_the_second_version_has_is_added_after_the_one_it_follows() {
    let (kind, arguments) =
        reproduced(&STATS.replace("fn describe", "fn double(x: int) -> int:\n    return 2 * x\n\nfn describe"));
    assert_eq!((kind, argument(&arguments, "after")), ("add", Some("median")));
    let (kind, arguments) = reproduced(
        &STATS.replace("        return float(self.total) / float(self.count)\n", "        return float(self.total) / float(self.count)\n\n    fn size(self) -> int:\n        return self.count\n"),
    );
    assert_eq!((kind, argument(&arguments, "after")), ("add", Some("Stats.mean")));
}

#[test]
fn a_declaration_only_the_first_version_has_is_removed() {
    let after = STATS.replace("fn describe(s: Stats) -> str:\n    match s.count:\n        case 0:\n            return \"empty\"\n        case _:\n            return \"some\"\n\n", "");
    let (kind, arguments) = reproduced(&after);
    assert_eq!((kind, argument(&arguments, "symbol")), ("remove", Some("describe")));
}

#[test]
fn a_change_no_symbol_addresses_is_a_lines_edit() {
    let (kind, arguments) = reproduced(&STATS.replace("== 2.0", "== 3.0"));
    assert_eq!(kind, "lines");
    assert_eq!(argument(&arguments, "replace"), Some("    assert median([1.0, 2.0, 3.0]) == 3.0"));
    let both = STATS.replace("total: int)", "total: f64)").replace("== 2.0", "== 3.0");
    let (kind, _) = reproduced(&both);
    assert_eq!(kind, "lines", "changes in two declarations are one edit of the lines between them");
}

#[test]
fn no_change_is_no_edit() {
    let found = diff(STATS, STATS, "stats.lotml");
    assert!(found.declarations.is_empty());
    assert!(found.edit.is_none());
}
