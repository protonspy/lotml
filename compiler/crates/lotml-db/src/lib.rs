//! The compiler as queries over source files: each file is a Salsa input, and parsing and
//! checking are tracked functions, so a query whose inputs did not change answers from memory.

use lotml_check::Checked;
use lotml_diag::Diagnostic;
use lotml_syntax::Parsed;
use salsa::Durability;

/// One source file: its path, for reports, its text, and the interfaces of the Python modules
/// it may import — each module's name with the text of its `.lotmli` (adr:0012). Made with
/// [`SourceFile::create`], which keeps the interfaces durable.
#[salsa::input]
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

/// What the checker found in a file: its type errors, the type of every expression, and what
/// each local's name refers to.
#[salsa::tracked(returns(ref))]
pub fn checked(db: &dyn salsa::Database, file: SourceFile) -> Checked {
    lotml_check::check_resolved_with(&parse(db, file).module, file.text(db), interfaces(db, file))
}

/// Every diagnostic of a file, in source order: syntax errors, then type errors.
#[salsa::tracked(returns(ref))]
pub fn diagnostics(db: &dyn salsa::Database, file: SourceFile) -> Vec<Diagnostic> {
    let mut found: Vec<Diagnostic> = parse(db, file).errors.iter().map(lotml_check::syntax).collect();
    found.extend(checked(db, file).diagnostics.iter().cloned());
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
    use salsa::Setter;

    use super::*;

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

    #[test]
    fn an_unchanged_file_is_parsed_once() {
        let db = Database::default();
        let file = SourceFile::create(&db, "f.lotml".into(), "fn f() -> int:\n    return 1\n".into(), vec![]);
        let first: *const Parsed = parse(&db, file);
        let second: *const Parsed = parse(&db, file);
        assert_eq!(first, second, "the second query is answered from memory");
    }
}
