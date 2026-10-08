//! The database's report for a file, assembled from its items, is the whole-file check's: the
//! same diagnostics in the same order and the same `Checked` (specs/incremental-check/R2.1, R2.2,
//! R2.3). Over every program of the corpus, from an empty database, after an edit inside each
//! body, and cut short at each line, where the parser is reading half-written code.

use lotml_check::{Interfaces, check_resolved_with, check_source_with, parts};
use lotml_db::{Database, SourceFile, checked, diagnostics};
use lotml_syntax::parse;
use salsa::Setter;

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

/// Whether the database's report for `file`, whose text is `text`, is the whole-file check's.
fn agrees(db: &Database, file: SourceFile, text: &str, place: &str) {
    let interfaces = Interfaces::new();
    assert_eq!(diagnostics(db, file), &check_source_with(text, &interfaces), "{place}: the diagnostics");
    let whole = check_resolved_with(&parse(text).module, text, &interfaces);
    let assembled = checked(db, file);
    assert_eq!(assembled.diagnostics, whole.diagnostics, "{place}: the type errors");
    assert_eq!(assembled.locals, whole.locals, "{place}: the locals");
    assert_eq!(assembled.types, whole.types, "{place}: the types");
    assert_eq!(assembled, &whole, "{place}: the declarations");
}

/// `text` with a statement added at the start of each body, one body at a time.
fn each_body_edited(text: &str) -> Vec<String> {
    let parsed = parse(text);
    parts(&parsed.module)
        .iter()
        .filter_map(|part| {
            let block = match part {
                lotml_check::Part::Fn(f)
                | lotml_check::Part::Method { f, .. }
                | lotml_check::Part::TraitMethod { f, .. } => f.body.as_ref()?,
                lotml_check::Part::Test(t) => &t.body,
                lotml_check::Part::Record(_) => return None,
            };
            let at = block.stmts.first()?.span.start as usize;
            let line = text[..at].rfind('\n').map_or(0, |i| i + 1);
            let indent = &text[line..at];
            indent.chars().all(|c| c == ' ').then(|| format!("{}pass\n{indent}{}", &text[..at], &text[at..]))
        })
        .collect()
}

#[test]
fn every_corpus_program_is_reported_as_the_whole_file_check_reports_it() {
    let corpus = corpus();
    assert!(corpus.len() > 500, "the corpus holds its programs");
    for (task, text) in &corpus {
        let mut db = Database::default();
        let file = SourceFile::create(&db, format!("{task}.lot"), text.clone(), vec![]);
        agrees(&db, file, text, &format!("{task}, from an empty database"));
        for (k, edited) in each_body_edited(text).into_iter().enumerate() {
            file.set_text(&mut db).to(edited.clone());
            agrees(&db, file, &edited, &format!("{task}, body {k} edited"));
        }
    }
}

#[test]
fn every_corpus_program_cut_short_is_reported_as_the_whole_file_check_reports_it() {
    for (task, text) in corpus() {
        let mut db = Database::default();
        let file = SourceFile::create(&db, format!("{task}.lot"), String::new(), vec![]);
        let ends: Vec<usize> = text.match_indices('\n').map(|(i, _)| i + 1).collect();
        for end in ends.into_iter().step_by(3) {
            let cut = &text[..end];
            file.set_text(&mut db).to(cut.to_string());
            agrees(&db, file, cut, &format!("{task}, cut at byte {end}"));
        }
    }
}

/// `t0 = 1` to `t9 = (t8, t8)` in a function's body: `t9` has 1,023 nodes.
fn large_types(mentions: usize) -> String {
    let mut lines = String::from("    t0 = 1\n");
    for i in 1..=9 {
        lines += &format!("    t{i} = (t{}, t{})\n", i - 1, i - 1);
    }
    lines + &"    print(t9)\n".repeat(mentions)
}

#[test]
fn the_module_type_limit_is_counted_item_by_item_in_source_order() {
    let module: String = (0..6).map(|k| format!("fn f{k}():\n{}\n", large_types(400))).collect();
    let mut db = Database::default();
    let file = SourceFile::create(&db, "large.lot".into(), module.clone(), vec![]);
    agrees(&db, file, &module, "from an empty database");
    let codes: Vec<&str> = diagnostics(&db, file).iter().map(|d| d.code).collect();
    assert_eq!(codes, vec!["E0222"], "reported once");
    let edited = module.replacen("print(t9)", "print(t8)", 1);
    file.set_text(&mut db).to(edited.clone());
    agrees(&db, file, &edited, "after an edit inside the first body");
}
