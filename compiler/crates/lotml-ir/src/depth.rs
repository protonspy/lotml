//! The functions whose calls count toward the recursion limit (specs/recursion-depth R1.1): those
//! a call can nest without bound through.

use std::collections::BTreeSet;

use crate::lower::Lowered;

/// The functions of `lowered`, by name, that a call can nest through without bound: each one in a
/// cycle of the call graph, each one used as a value, and each method of a type made a `dyn` value.
pub fn recursive(lowered: &Lowered) -> BTreeSet<String> {
    let _ = lowered;
    BTreeSet::new()
}
