//! The compiler as queries over source files: each file is a Salsa input, and parsing and
//! checking are tracked functions, so a query whose inputs did not change answers from memory.

use lotml_check::{Assembly, Checked, Declarations, Part, PartChecked, PartKind};
use lotml_diag::Diagnostic;
use lotml_syntax::Parsed;
use lotml_syntax::ast::{self, FnDef, RecordDef, TestDef};
use lotml_syntax::shift::Shift;
use salsa::Durability;

/// One source file: its path, for reports, its text, and the interfaces of the Python modules
/// it may import — each module's name with the text of its `.lotmli` (adr:0012). Made with
/// [`SourceFile::create`], which keeps the interfaces durable.
#[salsa::input(debug)]
pub struct SourceFile {
    #[returns(ref)]
    pub path: String,
    #[returns(ref)]
    pub text: String,
    #[returns(ref)]
    pub interfaces: Vec<(String, String)>,
}

impl SourceFile {
    /// A file whose interfaces change far less often than its text: they are a durable input, so
    /// an edit to the text revalidates nothing that only read them.
    pub fn create(
        db: &dyn salsa::Database,
        path: String,
        text: String,
        interfaces: Vec<(String, String)>,
    ) -> SourceFile {
        SourceFile::builder(path, text, interfaces).interfaces_durability(Durability::HIGH).new(db)
    }
}

/// The interfaces a file may import from, read once for as long as they stay the same.
#[salsa::tracked(returns(ref))]
pub fn interfaces(db: &dyn salsa::Database, file: SourceFile) -> lotml_check::Interfaces {
    file.interfaces(db)
        .iter()
        .map(|(module, text)| (module.clone(), lotml_check::interface_of(module, text).0))
        .collect()
}

/// The syntax tree of a file, with its syntax errors.
#[salsa::tracked(returns(ref))]
pub fn parse(db: &dyn salsa::Database, file: SourceFile) -> Parsed {
    lotml_syntax::parse(file.text(db))
}

/// One part of a file that is checked on its own (specs/incremental-check/): a function, a
/// method, a record's field defaults or a test. Told from the file's other parts by its kind and
/// name; its syntax and its text have every span relative to where its text starts, so moving it
/// changes only its start.
#[salsa::tracked(debug)]
pub struct Item<'db> {
    #[returns(copy)]
    pub file: SourceFile,
    #[returns(copy)]
    pub kind: PartKind,
    #[returns(ref)]
    pub name: String,
    #[tracked]
    #[returns(ref)]
    pub syntax: Syntax,
    /// The text from the start of the part's first line to the next part, which its spans index.
    #[tracked]
    #[returns(ref)]
    pub text: String,
    /// Where the text starts in the file.
    #[tracked]
    #[returns(copy)]
    pub start: u32,
}

/// The syntax of an [`Item`]: the part's own, with the name of the type or trait a method
/// belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Syntax {
    Fn(FnDef),
    Method(String, FnDef),
    TraitMethod(String, FnDef),
    Record(RecordDef),
    Test(TestDef),
}

impl Syntax {
    fn of(part: Part<'_>) -> Syntax {
        match part {
            Part::Fn(f) => Syntax::Fn(f.clone()),
            Part::Method { owner, f } => Syntax::Method(owner.to_string(), f.clone()),
            Part::TraitMethod { owner, f } => Syntax::TraitMethod(owner.to_string(), f.clone()),
            Part::Record(r) => Syntax::Record(r.clone()),
            Part::Test(t) => Syntax::Test(t.clone()),
        }
    }

    pub fn part(&self) -> Part<'_> {
        match self {
            Syntax::Fn(f) => Part::Fn(f),
            Syntax::Method(owner, f) => Part::Method { owner, f },
            Syntax::TraitMethod(owner, f) => Part::TraitMethod { owner, f },
            Syntax::Record(r) => Part::Record(r),
            Syntax::Test(t) => Part::Test(t),
        }
    }
}

impl Shift for Syntax {
    fn shift(&mut self, from: u32, to: u32) {
        match self {
            Syntax::Fn(f) | Syntax::Method(_, f) | Syntax::TraitMethod(_, f) => f.shift(from, to),
            Syntax::Record(r) => r.shift(from, to),
            Syntax::Test(t) => t.shift(from, to),
        }
    }
}

/// The parts of a file, in source order. The file is parsed whole, and each part's syntax moved
/// from the tree: a part parsed again on its own could recover from a syntax error differently.
#[salsa::tracked(returns(ref))]
pub fn items(db: &dyn salsa::Database, file: SourceFile) -> Vec<Item<'_>> {
    let text = file.text(db);
    let module = &parse(db, file).module;
    let mut starts: Vec<u32> = module
        .items
        .iter()
        .flat_map(|item| {
            let methods: &[FnDef] = match item {
                ast::Item::Impl(imp) => &imp.methods,
                ast::Item::Trait(t) => &t.methods,
                _ => &[],
            };
            std::iter::once(item.span().start).chain(methods.iter().map(|m| m.span.start))
        })
        .collect();
    starts.sort_unstable();
    lotml_check::parts(module)
        .into_iter()
        .map(|part| {
            let span = part.span();
            let before = &text[..(span.start as usize).min(text.len())];
            let at = before.rfind('\n').map_or(0, |i| i + 1);
            let next = starts.iter().find(|&&s| s > span.start).map_or(text.len(), |&s| s as usize);
            let end = next.max(span.end as usize).min(text.len()).max(at);
            let at = u32::try_from(at).unwrap_or(u32::MAX);
            let mut syntax = Syntax::of(part);
            syntax.shift(at, 0);
            Item::new(db, file, part.kind(), part.name(), syntax, text[at as usize..end].to_string(), at)
        })
        .collect()
}

/// What a file declares, with the diagnostics found collecting it at their places in the file.
#[salsa::tracked(returns(ref))]
pub fn declarations(db: &dyn salsa::Database, file: SourceFile) -> Declarations {
    lotml_check::declarations(&parse(db, file).module, interfaces(db, file))
}

/// What each part of a file is checked against: its declarations, each signature relative to the
/// function that declares it, so an edit inside a body leaves them equal.
#[salsa::tracked(returns(ref))]
pub fn signatures(db: &dyn salsa::Database, file: SourceFile) -> Declarations {
    declarations(db, file).signatures()
}

/// What checking one item found, every span relative to the start of its text. It reads the
/// item's syntax and text and the file's signatures, and no other item.
#[salsa::tracked(returns(ref))]
pub fn check_item<'db>(db: &'db dyn salsa::Database, item: Item<'db>) -> PartChecked {
    lotml_check::check_part(signatures(db, item.file(db)), item.syntax(db).part(), item.text(db))
}

/// What the checker found in a file: its type errors, the type of every expression, and what
/// each local's name refers to.
#[salsa::tracked(returns(ref))]
pub fn checked(db: &dyn salsa::Database, file: SourceFile) -> Checked {
    lotml_check::check_resolved_with(&parse(db, file).module, file.text(db), interfaces(db, file))
}

/// Every diagnostic of a file, in source order: syntax errors, then type errors, each item's
/// moved to where the item is.
#[salsa::tracked(returns(ref))]
pub fn diagnostics(db: &dyn salsa::Database, file: SourceFile) -> Vec<Diagnostic> {
    let mut found: Vec<Diagnostic> = parse(db, file).errors.iter().map(lotml_check::syntax).collect();
    let mut assembly = Assembly::new(declarations(db, file), false);
    for &item in items(db, file) {
        assembly.absorb(check_item(db, item), item.start(db));
    }
    found.extend(assembly.diagnostics());
    found.sort_by_key(|d| d.span.start);
    found
}

#[salsa::db]
#[derive(Clone, Default)]
pub struct Database {
    storage: salsa::Storage<Self>,
}

#[salsa::db]
impl salsa::Database for Database {}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use salsa::Setter;
    use salsa::plumbing::AsId;

    use super::*;

    /// A database that counts the item checks it runs.
    #[salsa::db]
    #[derive(Clone)]
    struct Counting {
        storage: salsa::Storage<Self>,
        executed: Arc<Mutex<Vec<String>>>,
    }

    impl Default for Counting {
        fn default() -> Counting {
            let executed = Arc::new(Mutex::new(Vec::new()));
            let log = Arc::clone(&executed);
            let storage = salsa::Storage::new(Some(Box::new(move |event: salsa::Event| {
                if let salsa::EventKind::WillExecute { database_key } = event.kind {
                    log.lock().expect("the log").push(format!("{database_key:?}"));
                }
            })));
            Counting { storage, executed }
        }
    }

    #[salsa::db]
    impl salsa::Database for Counting {}

    impl Counting {
        /// How many item checks ran since this was last asked.
        fn item_checks(&self) -> usize {
            let mut executed = self.executed.lock().expect("the log");
            let checks = executed.iter().filter(|e| e.starts_with("check_item(")).count();
            executed.clear();
            checks
        }
    }

    #[test]
    fn an_edit_inside_one_body_checks_that_item_again_and_an_edit_to_a_signature_checks_all() {
        let mut db = Counting::default();
        let file = SourceFile::create(&db, "f.lot".into(), PARTS.into(), vec![]);
        let all = items(&db, file).len();
        assert!(diagnostics(&db, file).is_empty());
        assert_eq!(db.item_checks(), all, "from an empty database, every item is checked");
        let body = PARTS.replace("        return \"named\"\n", "        var n = \"named\"\n        return n\n");
        file.set_text(&mut db).to(body.clone());
        assert!(diagnostics(&db, file).is_empty());
        assert_eq!(db.item_checks(), 1, "the edited body, and none of the items it moved");
        file.set_text(&mut db).to(body.replace("    return 0\n", "    return 1\n"));
        assert!(diagnostics(&db, file).is_empty());
        assert_eq!(db.item_checks(), 1, "the last body");
        let signature = PARTS.replace("fn main() -> int:", "fn main(n: int) -> int:");
        file.set_text(&mut db).to(signature);
        assert!(diagnostics(&db, file).is_empty());
        assert_eq!(db.item_checks(), all, "a signature changed, and every item is checked against it");
        file.set_interfaces(&mut db).to(vec![("textwrap".into(), "fn dedent(text: str) -> str ! PyError\n".into())]);
        assert!(diagnostics(&db, file).is_empty());
        assert_eq!(db.item_checks(), all, "the interfaces changed");
    }

    #[test]
    fn diagnostics_follow_the_text() {
        let mut db = Database::default();
        let file = SourceFile::create(&db, "f.lotml".into(), "fn f() -> int:\n    return \"a\"\n".into(), vec![]);
        assert_eq!(diagnostics(&db, file).iter().map(|d| d.code).collect::<Vec<_>>(), vec!["E0204"]);
        file.set_text(&mut db).to("fn f() -> int:\n    return 1\n".into());
        assert!(diagnostics(&db, file).is_empty());
    }

    #[test]
    fn diagnostics_follow_the_interfaces() {
        let mut db = Database::default();
        let text = "from textwrap import dedent\n\nfn f(s: str) -> str ! PyError:\n    return dedent(s)?\n";
        let file = SourceFile::create(&db, "f.lotml".into(), text.into(), vec![]);
        assert_eq!(diagnostics(&db, file)[0].code, "E0216", "no module to import");
        let textwrap = ("textwrap".to_string(), "fn dedent(text: str) -> str ! PyError\n".to_string());
        file.set_interfaces(&mut db).to(vec![textwrap]);
        assert!(diagnostics(&db, file).is_empty());
    }

    #[test]
    fn an_edit_to_the_text_reads_the_interfaces_no_more() {
        let mut db = Database::default();
        let text = "from textwrap import dedent

fn f(s: str) -> str ! PyError:
    return dedent(s)?
";
        let textwrap = (
            "textwrap".to_string(),
            "fn dedent(text: str) -> str ! PyError
"
            .to_string(),
        );
        let file = SourceFile::create(&db, "f.lotml".into(), text.into(), vec![textwrap]);
        assert!(diagnostics(&db, file).is_empty());
        let read: *const lotml_check::Interfaces = interfaces(&db, file);
        file.set_text(&mut db).to(text.replace("dedent(s)?", "dedent(s + \"x\")?"));
        assert!(diagnostics(&db, file).is_empty(), "the edited text checked against the same interfaces");
        let again: *const lotml_check::Interfaces = interfaces(&db, file);
        assert_eq!(read, again, "the interfaces read before the edit are the ones still in use");
        file.set_interfaces(&mut db).to(vec![]);
        assert_eq!(diagnostics(&db, file)[0].code, "E0216", "a change to the interfaces is still seen");
    }

    const PARTS: &str = "from math import sqrt

type Point(x: f64 = 0.0, y: f64 = 0.0)

type Shape = Dot | Line(Point, Point)

trait Named:
    fn name(self) -> str:
        return \"named\"

impl Point:
    fn norm(self) -> f64:
        return sqrt(self.x * self.x + self.y * self.y)

    fn origin() -> Point:
        return Point(x=0.0, y=0.0)

fn main() -> int:
    return 0

test \"the origin\":
    assert Point.origin().norm() == 0.0
";

    #[test]
    fn a_file_has_an_item_for_each_function_method_record_and_test() {
        let db = Database::default();
        let file = SourceFile::create(&db, "f.lot".into(), PARTS.into(), vec![]);
        assert_eq!(diagnostics(&db, file), &[], "a file that checks");
        let found: Vec<(PartKind, String)> =
            items(&db, file).iter().map(|i| (i.kind(&db), i.name(&db).clone())).collect();
        let named = |kind, name: &str| (kind, name.to_string());
        assert_eq!(
            found,
            vec![
                named(PartKind::Record, "Point"),
                named(PartKind::TraitMethod, "Named.name"),
                named(PartKind::Method, "Point.norm"),
                named(PartKind::Method, "Point.origin"),
                named(PartKind::Fn, "main"),
                named(PartKind::Test, "the origin"),
            ],
            "the sum type and the import have no body to check"
        );
    }

    #[test]
    fn an_item_starts_its_text_at_its_first_line_and_its_spans_index_that_text() {
        let db = Database::default();
        let file = SourceFile::create(&db, "f.lot".into(), PARTS.into(), vec![]);
        for item in items(&db, file) {
            let (start, text) = (item.start(&db) as usize, item.text(&db));
            assert!(start == 0 || PARTS.as_bytes()[start - 1] == b'\n', "{}: starts a line", item.name(&db));
            assert!(PARTS[start..].starts_with(text.as_str()), "{}: its text is the file's from there", item.name(&db));
            let span = item.syntax(&db).part().span();
            let own = &text[span.range()];
            assert!(
                own.trim_start().starts_with(['f', 'r', 't']),
                "{}: its span indexes its text: {own:?}",
                item.name(&db)
            );
        }
        let norm = items(&db, file).iter().find(|i| i.name(&db) == "Point.norm").expect("the method");
        assert!(norm.text(&db).starts_with("    fn norm"), "a method's text starts at its line, indentation and all");
    }

    #[test]
    fn an_item_moved_without_a_change_keeps_its_syntax_and_text() {
        let mut db = Database::default();
        let file = SourceFile::create(&db, "f.lot".into(), PARTS.into(), vec![]);
        let before: Vec<_> = items(&db, file)
            .iter()
            .map(|i| (i.as_id(), i.syntax(&db).clone(), i.text(&db).clone(), i.start(&db)))
            .collect();
        let added = "        var n = \"named\"\n";
        let edited = PARTS.replace("        return \"named\"\n", &format!("{added}        return n\n"));
        let by_edit = (edited.len() - PARTS.len()) as u32;
        file.set_text(&mut db).to(edited);
        let after = items(&db, file);
        assert_eq!(after.len(), before.len());
        let mut edited_seen = false;
        for ((id, syntax, text, start), moved) in before.iter().zip(after) {
            let name = moved.name(&db);
            assert_eq!(*id, moved.as_id(), "{name}: the same item, told by its kind and name");
            if name == "Named.name" {
                assert_ne!(moved.syntax(&db), syntax, "the edited method changed");
                edited_seen = true;
                continue;
            }
            assert_eq!(moved.syntax(&db), syntax, "{name}: the same syntax");
            assert_eq!(moved.text(&db), text, "{name}: the same text");
            let by = if edited_seen { by_edit } else { 0 };
            assert_eq!(moved.start(&db), start + by, "{name}: moved down past the edit");
        }
        assert!(edited_seen);
    }

    #[test]
    fn two_items_of_one_kind_and_name_are_two_items() {
        let db = Database::default();
        let text = "fn f() -> int:\n    return 1\n\nfn f() -> int:\n    return 2\n";
        let file = SourceFile::create(&db, "f.lot".into(), text.into(), vec![]);
        let found = items(&db, file);
        assert_eq!(found.len(), 2);
        assert_ne!(found[0], found[1]);
        assert_eq!(found[1].start(&db), text.find("\nfn f() -> int:\n    return 2").expect("the second") as u32 + 1);
    }

    /// The signatures of `text` before and after it becomes `edited`, with the interfaces given.
    fn signatures_around(text: &str, edited: &str, interfaces: Vec<(String, String)>) -> (Declarations, Declarations) {
        let mut db = Database::default();
        let file = SourceFile::create(&db, "f.lot".into(), text.into(), interfaces);
        let before = signatures(&db, file).clone();
        file.set_text(&mut db).to(edited.into());
        (before, signatures(&db, file).clone())
    }

    #[test]
    fn the_declarations_keep_their_diagnostics_where_they_are_in_the_file() {
        let db = Database::default();
        let text = "fn f() -> int:\n    return 1\n\nfn f() -> int:\n    return 2\n";
        let file = SourceFile::create(&db, "f.lot".into(), text.into(), vec![]);
        let found = declarations(&db, file).diagnostics();
        assert_eq!(
            found.iter().map(|d| (d.code, d.span.start)).collect::<Vec<_>>(),
            vec![("E0210", text.rfind("f()").expect("the second f") as u32)]
        );
        assert_eq!(&text[found[0].span.range()], "f");
        assert!(signatures(&db, file).diagnostics().is_empty(), "the signatures are what a body reads, not a report");
    }

    #[test]
    fn the_signatures_stay_equal_across_an_edit_inside_a_body() {
        let edited = PARTS.replace("        return \"named\"\n", "        var n = \"named\"\n        return n\n");
        let (before, after) = signatures_around(PARTS, &edited, vec![]);
        assert_eq!(before, after, "every function after the edit moved, and its signature did not change");
        let (before, after) = signatures_around(PARTS, &PARTS.replace("    return 0\n", "    return 1 + 2\n"), vec![]);
        assert_eq!(before, after, "the last function's body grew");
    }

    #[test]
    fn the_signatures_change_with_a_declaration_an_import_or_an_interface() {
        let declared = PARTS.replace("fn norm(self) -> f64", "fn norm(self, scale: f64) -> f64");
        let (before, after) = signatures_around(PARTS, &declared, vec![]);
        assert_ne!(before, after, "a method's parameters changed");
        let (before, after) = signatures_around(PARTS, &PARTS.replace("type Point(", "type Point(z: int, "), vec![]);
        assert_ne!(before, after, "a record's fields changed");
        let (before, after) = signatures_around(PARTS, &PARTS.replace("from math import sqrt\n", ""), vec![]);
        assert_ne!(before, after, "an import was removed");
        let mut db = Database::default();
        let text = "from textwrap import dedent\n\nfn f(s: str) -> str ! PyError:\n    return dedent(s)?\n";
        let file = SourceFile::create(&db, "f.lot".into(), text.into(), vec![]);
        let before = signatures(&db, file).clone();
        file.set_interfaces(&mut db).to(vec![("textwrap".into(), "fn dedent(text: str) -> str ! PyError\n".into())]);
        assert_ne!(&before, signatures(&db, file), "the interfaces changed");
    }

    #[test]
    fn an_unchanged_file_is_parsed_once() {
        let db = Database::default();
        let file = SourceFile::create(&db, "f.lotml".into(), "fn f() -> int:\n    return 1\n".into(), vec![]);
        let first: *const Parsed = parse(&db, file);
        let second: *const Parsed = parse(&db, file);
        assert_eq!(first, second, "the second query is answered from memory");
    }
}
