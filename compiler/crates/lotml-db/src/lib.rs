//! The compiler as queries over source files: each file is a Salsa input, and parsing and
//! checking are tracked functions, so a query whose inputs did not change answers from memory.

use lotml_diag::Diagnostic;
use lotml_syntax::Parsed;

/// One source file: its path, for reports, and its text.
#[salsa::input]
pub struct SourceFile {
    #[returns(ref)]
    pub path: String,
    #[returns(ref)]
    pub text: String,
}

/// The syntax tree of a file, with its syntax errors.
#[salsa::tracked(returns(ref))]
pub fn parse(db: &dyn salsa::Database, file: SourceFile) -> Parsed {
    lotml_syntax::parse(file.text(db))
}

/// Every diagnostic of a file, in source order: syntax errors, then type errors.
#[salsa::tracked(returns(ref))]
pub fn diagnostics(db: &dyn salsa::Database, file: SourceFile) -> Vec<Diagnostic> {
    let parsed = parse(db, file);
    let mut found: Vec<Diagnostic> = parsed.errors.iter().map(lotml_check::syntax).collect();
    found.extend(lotml_check::check(&parsed.module, file.text(db)));
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
        let file = SourceFile::new(&db, "f.lotml".into(), "fn f() -> int:\n    return \"a\"\n".into());
        assert_eq!(diagnostics(&db, file).iter().map(|d| d.code).collect::<Vec<_>>(), vec!["E0204"]);
        file.set_text(&mut db).to("fn f() -> int:\n    return 1\n".into());
        assert!(diagnostics(&db, file).is_empty());
    }

    #[test]
    fn an_unchanged_file_is_parsed_once() {
        let db = Database::default();
        let file = SourceFile::new(&db, "f.lotml".into(), "fn f() -> int:\n    return 1\n".into());
        let first: *const Parsed = parse(&db, file);
        let second: *const Parsed = parse(&db, file);
        assert_eq!(first, second, "the second query is answered from memory");
    }
}
