//! An item moved by its distance from one place equals the same item read at another
//! (specs/incremental-check/R1.2), over every program of the corpus.

use lotml_syntax::ast::Item;
use lotml_syntax::parse;
use lotml_syntax::shift::Shift;
use lotml_syntax::span::Span;

/// The programs of the corpus (`harness/results/corpus/corpus.jsonl`).
fn corpus() -> Vec<(String, String)> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../harness/results/corpus/corpus.jsonl");
    let text = std::fs::read_to_string(path).expect("the corpus");
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let row: serde_json::Value = serde_json::from_str(line).expect("a JSON row");
            (row["task"].as_str().unwrap_or("").to_string(), row["lotml"].as_str().unwrap_or("").to_string())
        })
        .collect()
}

/// Each item of `text` moved to start at zero.
fn at_zero(text: &str) -> Vec<Item> {
    let mut items = parse(text).module.items;
    for item in &mut items {
        let start = item.span().start;
        item.shift(start, 0);
    }
    items
}

#[test]
fn an_item_moved_to_zero_is_the_same_wherever_it_was_read() {
    let corpus = corpus();
    assert!(corpus.len() > 50, "the corpus is there");
    for (task, text) in &corpus {
        let padded = format!("\n\n# moved down\n\n{text}");
        assert_eq!(at_zero(text), at_zero(&padded), "{task}: the items read lower down, moved to zero");
    }
}

#[test]
fn an_item_read_further_down_moves_onto_the_same_item_read_higher_up() {
    let pad = "\n# two lines\n";
    for (task, text) in corpus() {
        let higher = parse(&text).module;
        let mut lower = parse(&format!("{pad}{text}")).module;
        lower.shift(pad.len() as u32, 0);
        assert_eq!(higher, lower, "{task}");
    }
}

#[test]
fn moving_back_gives_the_node_it_was() {
    for (task, text) in corpus() {
        let module = parse(&text).module;
        let mut moved = module.clone();
        moved.shift(7, 1_000);
        assert_ne!(moved, module, "{task}: every item has a span, and it moved");
        moved.shift(1_000, 7);
        assert_eq!(moved, module, "{task}");
    }
}

#[test]
fn a_span_moves_by_the_distance_between_two_places() {
    assert_eq!(Span::new(10, 14).shifted(10, 0), Span::new(0, 4));
    assert_eq!(Span::new(3, 5).shifted(0, 20), Span::new(23, 25));
    assert_eq!(Span::new(2, 4).shifted(5, 0).shifted(0, 5), Span::new(2, 4), "a span before the start comes back");
}
