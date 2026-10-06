//! The gate an edit passes before an agent is shown it: applied in memory through the compiler's
//! own edit functions, it leaves the text of every test block as it was and the file checking
//! clean, and for a failing block makes that block pass in a private copy of the project
//! (specs/guide-tool/ R2.9-R2.11). The project's files are only read.

use lotml_ide::edit::{self, Part};
use lotml_syntax::ast::Item;

use super::answer::Edit;

/// For a failing block: whether a candidate may run, and the run, which says whether the block
/// passes with the edited text in place of the file — None when it could not run.
pub type Candidate<'r> = (bool, &'r dyn Fn(&str) -> Option<bool>);

/// `text` with `edit` made in it, as the MCP tool of the same name would make it.
pub fn applied(edit: &Edit, text: &str) -> Result<String, &'static str> {
    let field = |key: &str| edit.arguments[key].as_str().unwrap_or_default();
    let changed = match edit.tool.as_str() {
        "replace" => {
            let part = match field("part") {
                "body" => Part::Body,
                "arm" => Part::Arm(field("arm").to_string()),
                _ => Part::Definition,
            };
            edit::replace(text, field("symbol"), &part, field("text"))
        }
        "add" => edit::add(text, edit.arguments["after"].as_str(), field("text")),
        "remove" => edit::remove(text, field("symbol")),
        "edit" => edit::search_replace(text, field("search"), field("replace")),
        _ => return Err("edit-fails-check"),
    };
    changed.map(|c| c.text).map_err(|_| "edit-fails-check")
}

/// Each test block's own text, in order.
fn tests(text: &str) -> Vec<String> {
    lotml_syntax::parse(text)
        .module
        .items
        .iter()
        .filter(|item| matches!(item, Item::Test(_)))
        .map(|item| text.get(item.span().start as usize..item.end() as usize).unwrap_or_default().to_string())
        .collect()
}

/// Why `edit` is not shown, or None when it may be: refused by the edit functions or leaving an
/// error (`edit-fails-check`), changing a test block's text (`edit-changes-tests`), and for a
/// failing block not allowed to run (`not-run`) or not making it pass (`edit-fails-test`).
pub fn withheld(
    edit: &Edit,
    text: &str,
    clean: &dyn Fn(&str) -> bool,
    failing: Option<Candidate<'_>>,
) -> Option<&'static str> {
    let new = match applied(edit, text) {
        Ok(new) => new,
        Err(reason) => return Some(reason),
    };
    if tests(&new) != tests(text) {
        return Some("edit-changes-tests");
    }
    if !clean(&new) {
        return Some("edit-fails-check");
    }
    match failing {
        None => None,
        Some((false, _)) => Some("not-run"),
        Some((true, run)) if run(&new) == Some(true) => None,
        Some(_) => Some("edit-fails-test"),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::guide::answer::Edit;

    const FILE: &str = "\
fn add(a: int, b: int) -> int:
    return a - b

test \"adds\":
    assert add(1, 2) == 3
";

    fn edit(tool: &str, arguments: serde_json::Value) -> Edit {
        Edit { tool: tool.into(), path: "a.lotml".into(), arguments }
    }

    fn clean(text: &str) -> bool {
        lotml_check::check_source(text).iter().all(|d| d.severity != lotml_diag::Severity::Error)
    }

    fn body(text: &str) -> Edit {
        edit("replace", json!({"symbol": "add", "part": "body", "text": text, "path": "a.lotml"}))
    }

    #[test]
    fn an_edit_that_checks_and_keeps_the_tests_is_shown_when_nothing_failed_at_run_time() {
        assert_eq!(withheld(&body("return a + b"), FILE, &clean, None), None);
    }

    #[test]
    fn an_edit_the_edit_functions_refuse_or_that_leaves_an_error_fails_check() {
        let unknown = edit("replace", json!({"symbol": "nowhere", "text": "x", "path": "a.lotml"}));
        assert_eq!(withheld(&unknown, FILE, &clean, None), Some("edit-fails-check"));
        assert_eq!(withheld(&body("return missing"), FILE, &clean, None), Some("edit-fails-check"));
        let removed = edit("remove", json!({"symbol": "add", "path": "a.lotml"}));
        assert_eq!(withheld(&removed, FILE, &clean, None), Some("edit-fails-check"));
    }

    #[test]
    fn an_edit_that_rewrites_a_test_block_is_withheld() {
        let rewritten = edit(
            "edit",
            json!({"path": "a.lotml", "search": "    assert add(1, 2) == 3", "replace": "    assert add(1, 2) == -1"}),
        );
        assert_eq!(withheld(&rewritten, FILE, &clean, None), Some("edit-changes-tests"));
    }

    #[test]
    fn every_edit_tool_is_applied_through_the_compiler_s_edit_functions() {
        let added = edit(
            "add",
            json!({"text": "fn double(a: int) -> int:\n    return 2 * a", "after": "add", "path": "a.lotml"}),
        );
        assert_eq!(applied(&added, FILE).unwrap().matches("fn double").count(), 1);
        let lines =
            edit("edit", json!({"path": "a.lotml", "search": "    return a - b", "replace": "    return a + b"}));
        assert!(applied(&lines, FILE).unwrap().contains("return a + b"));
        assert!(applied(&edit("rename", json!({})), FILE).is_err());
    }

    #[test]
    fn for_a_failing_block_the_candidate_runs_only_when_it_may() {
        let fixed = body("return a + b");
        let never = |_: &str| -> Option<bool> { panic!("the candidate ran") };
        assert_eq!(withheld(&fixed, FILE, &clean, Some((false, &never))), Some("not-run"));
        assert_eq!(withheld(&fixed, FILE, &clean, Some((true, &|_: &str| Some(true)))), None);
        assert_eq!(withheld(&fixed, FILE, &clean, Some((true, &|_: &str| Some(false)))), Some("edit-fails-test"));
        assert_eq!(withheld(&fixed, FILE, &clean, Some((true, &|_: &str| None))), Some("edit-fails-test"));
        let ran_with = std::cell::RefCell::new(String::new());
        let record = |text: &str| {
            ran_with.replace(text.to_string());
            Some(true)
        };
        withheld(&fixed, FILE, &clean, Some((true, &record)));
        assert!(ran_with.borrow().contains("return a + b"), "the candidate is the edited text");
    }
}
