//! The type checker: signatures first, so a body is checked against the declarations of the
//! whole file and a function may be used before it is declared; then each body, with
//! bidirectional inference of its locals.

mod body;
mod builtins;
mod format;
mod interface;
mod parts;
mod prefix;
mod program;
pub mod ty;

pub use interface::{
    Interface, Interfaces, OVERLOADS, PyClass, c_interface, interface, interface_of, is_c_library, py_constructor,
    shadowing, unlocked,
};
pub use parts::{Assembly, Declarations, Part, PartChecked, PartKind, check_part, declarations, parts};
pub use prefix::{PrefixCheck, Verdict, check_prefix, check_prefix_with};
pub use program::{FieldSig, FnSig, MODULES, Method, ParamSig, TypeDef, VariantSig};

/// The names every program sees without an import (R35).
pub const PRELUDE: &[&str] = builtins::PRELUDE;

use std::collections::{BTreeMap, HashMap};

use lotml_diag::{Applicability, Diagnostic};
use lotml_syntax::ast::Module;
use lotml_syntax::span::Span;
use lotml_syntax::{SyntaxError, parse};

use crate::ty::Ty;

/// Parse and check one file, returning every diagnostic in source order.
pub fn check_source(source: &str) -> Vec<Diagnostic> {
    check_source_with(source, &Interfaces::new())
}

/// [`check_source`] for a file that may import the Python modules in `interfaces`.
pub fn check_source_with(source: &str, interfaces: &Interfaces) -> Vec<Diagnostic> {
    let parsed = parse(source);
    let mut diagnostics: Vec<Diagnostic> = parsed.errors.iter().map(syntax).collect();
    diagnostics.extend(check_resolved_with(&parsed.module, source, interfaces).diagnostics);
    diagnostics.sort_by_key(|d| d.span.start);
    diagnostics
}

/// A syntax error as a diagnostic: what was expected, and the fix when there is one.
pub fn syntax(error: &SyntaxError) -> Diagnostic {
    let mut d = Diagnostic::error(error.code, error.span, error.message.clone());
    if !error.expected.is_empty() {
        d = d.alternatives(error.expected.iter().map(ToString::to_string));
    }
    if let Some((span, replacement)) = &error.fix {
        let message = if replacement.trim().is_empty() {
            "remove it".to_string()
        } else {
            format!("write `{}`", replacement.trim())
        };
        d = d.fix(message, Applicability::MachineApplicable, vec![(*span, replacement.clone())]);
    }
    d
}

/// Type-check a parsed module.
pub fn check(module: &Module, text: &str) -> Vec<Diagnostic> {
    check_typed(module, text).0
}

/// The type of each expression of a module, by its span.
pub type Types = HashMap<Span, Ty>;

/// Type-check a parsed module, keeping the type of every expression for a backend.
pub fn check_typed(module: &Module, text: &str) -> (Vec<Diagnostic>, Types) {
    let checked = check_resolved(module, text);
    (checked.diagnostics, checked.types)
}

/// What checking a module found: its diagnostics, the type of every expression, and what each
/// name of a local refers to.
#[derive(Debug, Default, PartialEq)]
pub struct Checked {
    pub diagnostics: Vec<Diagnostic>,
    pub types: Types,
    /// How many type nodes `types` holds, up to [`MODULE_TYPES`].
    stored: usize,
    /// Each name that resolved to a local, parameter or binding, with the span of the name
    /// that declared it; a declaration refers to itself. One span may be listed more than once.
    pub locals: Vec<(Span, Span)>,
    /// The signature of each top-level function.
    pub functions: BTreeMap<String, FnSig>,
    /// The records and sum types the module declares, with their fields.
    pub declared: BTreeMap<String, TypeDef>,
    /// The Python modules imported through interfaces, each with its functions.
    pub foreign: BTreeMap<String, BTreeMap<String, FnSig>>,
    /// The methods of each type the module declares, by type and by name.
    pub methods: BTreeMap<String, BTreeMap<String, Method>>,
    /// The methods of each trait the module declares, by trait and by name; `self` is of the
    /// type `Self`.
    pub traits: BTreeMap<String, BTreeMap<String, Method>>,
    /// The Python classes imported by name, each with its full name (adr:0034): `date` for
    /// `py.datetime.date`.
    pub py_classes: BTreeMap<String, String>,
    /// The methods and static methods each Python class an imported module declares has of its
    /// own, by its full name; [`Checked::py_method`] looks through its bases too.
    pub py_methods: BTreeMap<String, BTreeMap<String, Method>>,
    /// The bases of each Python class an imported module declares, by full names.
    pub py_bases: BTreeMap<String, Vec<String>>,
    /// The constructor each of those classes has of its own; [`Checked::py_constructor`] finds an
    /// inherited one.
    pub py_constructors: BTreeMap<String, FnSig>,
    /// Each call of a Python function, constructor or member, by the call's span: the signature
    /// it was checked against, overload chosen and type arguments inferred (adr:0035, adr:0036).
    pub py_calls: BTreeMap<Span, PyCall>,
}

/// A call into Python as the checker typed it.
#[derive(Clone, Debug, PartialEq)]
pub struct PyCall {
    /// The overload given: 0 the first signature, `i` its `overloads[i - 1]`.
    pub overload: usize,
    /// The signature as instantiated for the call, its types resolved.
    pub sig: FnSig,
}

impl Checked {
    /// The signature the call into Python at `call` was checked against, its overload chosen and
    /// its type arguments inferred (adr:0035, adr:0036); `sig` for a call the checker recorded none.
    pub fn py_call(&self, call: Span, sig: &FnSig) -> FnSig {
        self.py_calls.get(&call).map_or_else(|| sig.clone(), |recorded| recorded.sig.clone())
    }

    /// The method or static method `name` of the Python class `class`, or of the nearest base it
    /// declares that has one (adr:0034).
    pub fn py_method(&self, class: &str, name: &str) -> Option<&Method> {
        interface::py_lineage(class, |c| self.py_bases.get(c).map(Vec::as_slice))
            .iter()
            .find_map(|c| self.py_methods.get(c).and_then(|m| m.get(name)))
    }

    /// The constructor of the Python class `class`: its own, else a base's.
    pub fn py_constructor(&self, class: &str) -> Option<FnSig> {
        interface::py_constructor(class, |c| self.py_constructors.get(c), |c| self.py_bases.get(c).map(Vec::as_slice))
    }
}

/// The most type nodes a module keeps for its expressions, in all: a long-lived editor or MCP
/// server holds them for every file it has open.
const MODULE_TYPES: usize = 1 << 21;

/// Type-check a parsed module, keeping the type of every expression and what each local's name
/// refers to, for a backend or an editor.
pub fn check_resolved(module: &Module, text: &str) -> Checked {
    check_resolved_with(module, text, &Interfaces::new())
}

/// [`check_resolved`] for a module that may import the Python modules in `interfaces`.
pub fn check_resolved_with(module: &Module, text: &str, interfaces: &Interfaces) -> Checked {
    let declarations = parts::declarations(module, interfaces);
    let mut assembly = Assembly::new(&declarations, true);
    for part in parts::parts(module) {
        assembly.absorb(&check_part(&declarations, part, text), 0);
    }
    assembly.finish(module, &declarations)
}

/// Whether a name belongs to the compiler: everything the backend generates starts with `__`,
/// so a program may neither declare nor shadow such a name.
pub(crate) fn reserved(name: &str) -> bool {
    name.starts_with("__")
}

/// Report a declared name that is reserved for the compiler; true when it was.
pub(crate) fn report_reserved(diagnostics: &mut Vec<Diagnostic>, name: &lotml_syntax::ast::Ident) -> bool {
    if reserved(&name.name) {
        diagnostics.push(lotml_diag::Diagnostic::error(
            "E0220",
            name.span,
            format!("`{}` starts with `__`, which is reserved for the compiler", name.name),
        ));
        return true;
    }
    false
}

/// The names closest to `name`, for "did you mean": by edit distance with transpositions,
/// and names one contains the other of, nearest first.
pub(crate) fn closest(name: &str, candidates: &[String]) -> Vec<String> {
    let limit = (name.chars().count() / 3).max(1);
    let mut scored: Vec<(usize, &String)> = candidates
        .iter()
        .filter(|c| c.as_str() != name && !c.is_empty())
        .filter_map(|c| {
            let d = distance(&name.to_lowercase(), &c.to_lowercase());
            let contained = c.len() >= 3 && name.len() >= 3 && (name.contains(c.as_str()) || c.contains(name));
            (d <= limit || contained).then_some((d, c))
        })
        .collect();
    scored.sort();
    scored.dedup_by(|a, b| a.1 == b.1);
    scored.into_iter().take(5).map(|(_, c)| c.clone()).collect()
}

/// Optimal string alignment distance: insertions, deletions, substitutions and adjacent
/// transpositions each cost one.
fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_counts_a_transposition_once() {
        assert_eq!(distance("cuont", "count"), 1);
        assert_eq!(distance("abc", "abc"), 0);
        assert_eq!(distance("", "ab"), 2);
    }

    #[test]
    fn closest_finds_near_and_containing_names() {
        let names: Vec<String> = ["count", "counter", "upper", "zzz"].iter().map(ToString::to_string).collect();
        assert_eq!(closest("cuont", &names), vec!["count".to_string()]);
        assert!(closest("uppercase", &names).contains(&"upper".to_string()));
        assert!(closest("qqqqqq", &names).is_empty());
    }
}
