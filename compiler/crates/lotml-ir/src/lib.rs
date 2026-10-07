//! The IR between the checker and every backend (adr:0020, specs/shared-ir): the checked program
//! lowered once into typed, structured functions over named locals, and the passes from IR to IR
//! that make it ready for a native backend.

pub mod ir;
pub mod lower;
pub mod mono;
pub mod symbol;
pub mod text;
pub mod verify;

mod hoist;
mod own;
mod reuse;

/// The program as a native backend reads it: counts inserted, then reuse, then uniqueness
/// hoisted out of loops, in every function (R3.1, R3.2).
pub fn native(lowered: &mut lower::Lowered) {
    for f in &mut lowered.functions {
        own::insert_counts(f);
        verify::assert_valid(f, "counting");
        reuse::insert_reuse(f);
        verify::assert_valid(f, "reuse");
        hoist::hoist_uniqueness(f);
        verify::assert_valid(f, "hoisting");
    }
}
