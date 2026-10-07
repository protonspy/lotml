//! The symbol of each function of the IR, the same whichever native backend writes it: the C and
//! LLVM targets name a function alike, and so does the wrapper a C ABI export calls it through
//! (specs/shared-ir/design.md, "Data").

/// A function of the module.
pub fn function(name: &str) -> String {
    format!("lf_{name}")
}

/// The method `method` of the type `owner`; the owner's length keeps `a_b.c` apart from `a.b_c`.
pub fn method(owner: &str, method: &str) -> String {
    format!("lm{}_{owner}_{method}", owner.len())
}

/// The `index`th instance of the generic function or method whose symbol is `base`.
pub fn instance(base: &str, index: usize) -> String {
    format!("li{index}_{base}")
}

/// The `index`th lambda of the module, under a prefix no function's symbol starts with, so a
/// function named `lambda0` keeps its own.
pub fn lambda(index: usize) -> String {
    format!("ll{index}")
}

/// The `index`th default of a parameter or a field, as a function of no parameters.
pub fn default(index: usize) -> String {
    format!("ld{index}")
}

/// The `k`th `test` block of the module.
pub fn test(k: usize) -> String {
    format!("lt_test{k}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_two_functions_share_a_symbol() {
        let all = [
            function("f"),
            function("lambda0"),
            function("test0"),
            method("f", "f"),
            method("a_b", "c"),
            method("a", "b_c"),
            instance(&function("f"), 0),
            instance(&method("f", "f"), 0),
            lambda(0),
            test(0),
        ];
        for (i, a) in all.iter().enumerate() {
            assert!(!all[i + 1..].contains(a), "{a} names two functions");
        }
    }

    #[test]
    fn a_symbol_is_an_identifier_c_and_llvm_both_take() {
        for s in [function("median"), method("Stack", "push"), instance("lm_Stack_push", 2), lambda(3), test(4)] {
            assert!(s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'), "{s}");
            assert!(!s.starts_with(|c: char| c.is_ascii_digit()), "{s}");
        }
    }
}
