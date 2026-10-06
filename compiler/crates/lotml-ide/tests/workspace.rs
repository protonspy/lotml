//! The questions an editor and an agent ask of a workspace: where a name is declared, every
//! place it is used, what it is, and what a file declares.

use std::path::Path;

use lotml_ide::{Kind, Symbol, Workspace};
use lotml_syntax::span::Span;

const SHAPES: &str = "\
type Shape = Circle(r: f64) | Rect(w: f64, h: f64) | Empty

type Counter(count: int)

impl Counter:
    fn new() -> Counter:
        return Counter(count=0)

    fn get(self) -> int:
        \"\"\"The count so far.\"\"\"
        return self.count

    fn bump(inout self):
        self.count += 1

fn area(s: Shape) -> f64:
    match s:
        case Circle(r):
            return 3.14159 * r * r
        case Rect(w, h):
            return w * h
        case Empty:
            return 0.0

fn scaled(s: Shape, factor: f64 = 1.0) -> f64:
    return area(s) * factor

fn main():
    var c = Counter.new()
    c.bump()
    total = scaled(Circle(r=2.0), factor=2.0) + area(Empty)
    print(f\"{c.get()} {total}\")
";

fn workspace(text: &str) -> Workspace {
    let mut ws = Workspace::new();
    ws.set(Path::new("shapes.lotml"), text.to_string());
    assert!(
        ws.diagnostics(Path::new("shapes.lotml")).is_empty(),
        "the example should check clean: {:?}",
        ws.diagnostics(Path::new("shapes.lotml")).iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    ws
}

/// The byte offset of the `n`th (from 0) whole-word occurrence of `word`.
fn at(text: &str, word: &str, n: usize) -> u32 {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let found = text
        .match_indices(word)
        .filter(|(i, _)| {
            !text[..*i].chars().next_back().is_some_and(is_word)
                && !text[i + word.len()..].chars().next().is_some_and(is_word)
        })
        .nth(n)
        .unwrap_or_else(|| panic!("no occurrence {n} of `{word}`"));
    u32::try_from(found.0).unwrap()
}

/// The text each span covers, with the line it is on (from 1).
fn shown(text: &str, spans: &[Span]) -> Vec<(usize, String)> {
    spans.iter().map(|s| (text[..s.start as usize].matches('\n').count() + 1, text[s.range()].to_string())).collect()
}

fn symbol_at(ws: &Workspace, offset: u32) -> Symbol {
    ws.at(Path::new("shapes.lotml"), offset).expect("a name here").symbol.clone()
}

#[test]
fn a_function_is_found_where_it_is_declared_and_every_place_it_is_called() {
    let ws = workspace(SHAPES);
    let path = Path::new("shapes.lotml");
    let symbol = symbol_at(&ws, at(SHAPES, "area", 2));
    assert_eq!(symbol, Symbol::Item("area".into()));
    assert_eq!(
        ws.declarations(path, &symbol),
        vec![Span { start: at(SHAPES, "area", 0), end: at(SHAPES, "area", 0) + 4 }]
    );
    assert_eq!(shown(SHAPES, &ws.references(path, &symbol, false)), vec![(26, "area".into()), (31, "area".into())]);
    assert_eq!(ws.references(path, &symbol, true).len(), 3, "with its declaration");
}

#[test]
fn a_method_is_resolved_through_the_type_of_its_receiver() {
    let ws = workspace(SHAPES);
    let path = Path::new("shapes.lotml");
    let get = symbol_at(&ws, at(SHAPES, "get", 1));
    assert_eq!(get, Symbol::Member("Counter".into(), "get".into()));
    assert_eq!(shown(SHAPES, &ws.declarations(path, &get)), vec![(9, "get".into())]);
    // `Counter.new()` names the method through the type itself.
    let new = symbol_at(&ws, at(SHAPES, "new", 1));
    assert_eq!(new, Symbol::Member("Counter".into(), "new".into()));
    assert_eq!(shown(SHAPES, &ws.references(path, &new, false)), vec![(29, "new".into())]);
}

#[test]
fn a_field_is_used_by_reads_writes_and_keyword_arguments() {
    let ws = workspace(SHAPES);
    let path = Path::new("shapes.lotml");
    let count = Symbol::Member("Counter".into(), "count".into());
    assert_eq!(symbol_at(&ws, at(SHAPES, "count", 0)), count);
    let lines: Vec<usize> = shown(SHAPES, &ws.references(path, &count, true)).into_iter().map(|(l, _)| l).collect();
    assert_eq!(lines, vec![3, 7, 11, 14]);
}

#[test]
fn a_keyword_argument_names_the_parameter_or_the_field() {
    let ws = workspace(SHAPES);
    let path = Path::new("shapes.lotml");
    let factor = symbol_at(&ws, at(SHAPES, "factor", 2));
    assert_eq!(factor, Symbol::Local(Span { start: at(SHAPES, "factor", 0), end: at(SHAPES, "factor", 0) + 6 }));
    assert_eq!(ws.references(path, &factor, true).len(), 3, "the parameter, its use, and the keyword");
    let r = symbol_at(&ws, at(SHAPES, "r", 4));
    assert_eq!(r, Symbol::Member("Circle".into(), "r".into()), "a variant's field built by name");
}

#[test]
fn variants_and_types_are_found_in_patterns_and_signatures() {
    let ws = workspace(SHAPES);
    let path = Path::new("shapes.lotml");
    let circle = Symbol::Variant("Circle".into());
    assert_eq!(shown(SHAPES, &ws.references(path, &circle, false)), vec![(18, "Circle".into()), (31, "Circle".into())]);
    let shape = Symbol::Item("Shape".into());
    assert_eq!(ws.references(path, &shape, false).len(), 2, "both signatures");
    // `r` bound by the pattern is a local, not the field.
    assert!(matches!(symbol_at(&ws, at(SHAPES, "r", 1)), Symbol::Local(_)));
}

#[test]
fn hover_shows_a_locals_type_or_a_declarations_signature() {
    let ws = workspace(SHAPES);
    let path = Path::new("shapes.lotml");
    let (_, total) = ws.hover(path, at(SHAPES, "total", 1)).unwrap();
    assert_eq!(total, "total: f64");
    let (_, get) = ws.hover(path, at(SHAPES, "get", 1)).unwrap();
    assert_eq!(get, "Counter\nfn get(self) -> int\n\nThe count so far.");
    let (_, factor) = ws.hover(path, at(SHAPES, "factor", 0)).unwrap();
    assert_eq!(factor, "factor: f64");
    let (_, circle) = ws.hover(path, at(SHAPES, "Circle", 1)).unwrap();
    assert_eq!(circle, "Shape\nCircle(r: f64)");
    let (_, count) = ws.hover(path, at(SHAPES, "count", 3)).unwrap();
    assert_eq!(count, "Counter\ncount: int");
}

#[test]
fn a_cursor_just_after_a_name_points_at_it() {
    let ws = workspace(SHAPES);
    let end = at(SHAPES, "area", 1) + 4;
    assert_eq!(symbol_at(&ws, end), Symbol::Item("area".into()));
}

#[test]
fn the_outline_lists_declarations_and_what_they_hold() {
    let ws = workspace(SHAPES);
    let outline = ws.outline(Path::new("shapes.lotml"));
    let top: Vec<(&str, Kind)> = outline.iter().map(|o| (o.name.as_str(), o.kind)).collect();
    assert_eq!(
        top,
        vec![
            ("Shape", Kind::Sum),
            ("Counter", Kind::Record),
            ("impl Counter", Kind::Impl),
            ("area", Kind::Function),
            ("scaled", Kind::Function),
            ("main", Kind::Function)
        ]
    );
    let methods: Vec<&str> = outline[2].children.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(methods, vec!["new", "get", "bump"]);
    assert_eq!(outline[0].children[1].children.len(), 2, "Rect's two fields");
}

#[test]
fn find_names_a_symbol_across_the_workspace() {
    let mut ws = workspace(SHAPES);
    ws.set(Path::new("other.lotml"), "fn area() -> int:\n    return 1\n".into());
    let found = ws.find("area");
    assert_eq!(found.len(), 2, "each file declares its own `area`");
    assert_eq!(
        ws.find("Counter.bump"),
        vec![(Path::new("shapes.lotml").to_path_buf(), Symbol::Member("Counter".into(), "bump".into()))]
    );
    assert!(ws.find("nothing").is_empty());
}

#[test]
fn a_changed_file_is_answered_from_its_new_text() {
    let mut ws = workspace(SHAPES);
    let path = Path::new("shapes.lotml");
    assert!(!ws.set(path, SHAPES.to_string()), "the same text is no change");
    let broken = SHAPES.replace("return w * h", "return w * missing");
    assert!(ws.set(path, broken));
    assert_eq!(ws.diagnostics(path).iter().map(|d| d.code).collect::<Vec<_>>(), vec!["E0201"]);
    assert!(ws.remove(path));
    assert!(ws.diagnostics(path).is_empty() && !ws.contains(path));
}
