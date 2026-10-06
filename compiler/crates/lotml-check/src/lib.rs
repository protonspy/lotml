//! The type checker: signatures first, so a body is checked against the declarations of the
//! whole file and a function may be used before it is declared; then each body, with
//! bidirectional inference of its locals.

mod body;
mod builtins;
mod interface;
mod prefix;
mod program;
pub mod ty;

pub use interface::{Interface, Interfaces, c_interface, interface, interface_of, is_c_library};
pub use prefix::{PrefixCheck, Verdict, check_prefix, check_prefix_with};
pub use program::{FieldSig, FnSig, ParamSig, TypeDef, VariantSig};

/// The names every program sees without an import (R35).
pub const PRELUDE: &[&str] = builtins::PRELUDE;

use std::collections::{BTreeMap, HashMap};

use lotml_diag::{Applicability, Diagnostic};
use lotml_syntax::ast::{FnDef, Item, Module, TypeKind};
use lotml_syntax::span::Span;
use lotml_syntax::{SyntaxError, parse};

use crate::body::Body;
use crate::program::Program;
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
    /// Each name that resolved to a local, parameter or binding, with the span of the name
    /// that declared it; a declaration refers to itself. One span may be listed more than once.
    pub locals: Vec<(Span, Span)>,
    /// The signature of each top-level function.
    pub functions: BTreeMap<String, FnSig>,
    /// The records and sum types the module declares, with their fields.
    pub declared: BTreeMap<String, TypeDef>,
    /// The Python modules imported through interfaces, each with its functions.
    pub foreign: BTreeMap<String, BTreeMap<String, FnSig>>,
    /// How many type nodes `types` holds, up to [`MODULE_TYPES`].
    stored: usize,
}

/// The most type nodes a module keeps for its expressions, in all: a long-lived editor or MCP
/// server holds them for every file it has open.
const MODULE_TYPES: usize = 1 << 21;

impl Checked {
    fn absorb(&mut self, mut body: Body) {
        for (span, ty) in body.types() {
            let before = self.stored;
            self.stored = self.stored.saturating_add(ty.size());
            if self.stored <= MODULE_TYPES {
                self.types.insert(span, ty);
                continue;
            }
            if before <= MODULE_TYPES {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E0222",
                        span,
                        format!("the types of this module have more than {MODULE_TYPES} parts in all"),
                    )
                    .note("name the shape with a record type, or keep the values in a list"),
                );
            }
            self.types.insert(span, Ty::Error);
        }
        self.locals.extend_from_slice(body.locals());
        self.diagnostics.extend(body.diagnostics);
    }
}

/// Type-check a parsed module, keeping the type of every expression and what each local's name
/// refers to, for a backend or an editor.
pub fn check_resolved(module: &Module, text: &str) -> Checked {
    check_resolved_with(module, text, &Interfaces::new())
}

/// [`check_resolved`] for a module that may import the Python modules in `interfaces`.
pub fn check_resolved_with(module: &Module, text: &str, interfaces: &Interfaces) -> Checked {
    let mut program = Program::collect(module, &interface::functions(interfaces));
    let mut checked = Checked { diagnostics: std::mem::take(&mut program.diagnostics), ..Checked::default() };
    for item in &module.items {
        match item {
            Item::Fn(f) => {
                let sig = quiet_signature(&program, f, &[], None);
                function(&program, text, f, &sig, &[], &mut checked);
            }
            Item::Impl(imp) => {
                let TypeKind::Named { name, .. } = &imp.target.kind else { continue };
                let Some(def) = program.types.get(&name.name) else { continue };
                let params = def.params().to_vec();
                let self_ty = Ty::Adt(name.name.clone(), params.iter().map(|p| Ty::Param(p.clone())).collect());
                let outer: Vec<(String, Option<String>)> = params.iter().map(|p| (p.clone(), None)).collect();
                for method in &imp.methods {
                    let sig = quiet_signature(&program, method, &params, Some(&self_ty));
                    function(&program, text, method, &sig, &outer, &mut checked);
                }
            }
            Item::Trait(t) => {
                let outer = vec![("Self".to_string(), Some(t.name.name.clone()))];
                for method in &t.methods {
                    let sig = quiet_signature(&program, method, &[], Some(&Ty::Param("Self".into())));
                    function(&program, text, method, &sig, &outer, &mut checked);
                }
            }
            Item::Record(r) => {
                let mut body = Body::new(&program, text, None, &[], false);
                if let Some(program::TypeDef::Record { fields, .. }) = program.types.get(&r.name.name) {
                    for (field, sig) in r.fields.iter().zip(fields) {
                        if let Some(default) = &field.default {
                            let found = body.expr(default, Some(&sig.ty));
                            body.coerce(&found, &sig.ty, default.span);
                        }
                    }
                }
                checked.absorb(body);
            }
            Item::Test(t) => {
                let mut body = Body::new(&program, text, None, &[], true);
                body.block(&t.body);
                checked.absorb(body);
            }
            Item::Sum(_) | Item::Import(_) | Item::Error(_) => {}
        }
    }
    let declared_here = |name: &String| {
        module.items.iter().any(|item| match item {
            Item::Fn(f) => f.name.name == *name,
            Item::Record(r) => r.name.name == *name,
            Item::Sum(s) => s.name.name == *name,
            _ => false,
        })
    };
    checked.functions =
        program.functions.iter().filter(|(n, _)| declared_here(n)).map(|(n, s)| (n.clone(), s.clone())).collect();
    checked.declared =
        program.types.iter().filter(|(n, _)| declared_here(n)).map(|(n, t)| (n.clone(), t.clone())).collect();
    checked.foreign = program.foreign;
    checked
}

/// A signature already reported on when the program was collected, lowered again without
/// reporting: a duplicate declaration keeps its own signature.
fn quiet_signature(program: &Program, f: &FnDef, outer: &[String], self_ty: Option<&Ty>) -> FnSig {
    let mut scratch = Program { types: program.types.clone(), traits: program.traits.clone(), ..Program::default() };
    scratch.signature(f, outer, self_ty)
}

fn function(
    program: &Program,
    text: &str,
    f: &FnDef,
    sig: &FnSig,
    outer: &[(String, Option<String>)],
    checked: &mut Checked,
) {
    let mut body = Body::new(program, text, Some(sig), outer, false);
    for (param, param_sig) in f.params.iter().zip(&sig.params) {
        if let Some(default) = &param.default {
            let found = body.expr(default, Some(&param_sig.ty));
            body.coerce(&found, &param_sig.ty, default.span);
        }
    }
    if let Some(block) = &f.body {
        body.function_body(block, sig);
    }
    checked.absorb(body);
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
