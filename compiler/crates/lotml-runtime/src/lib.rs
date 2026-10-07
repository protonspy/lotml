//! The runtime native programs run on, apart from the compiler that writes them: C, compiled into
//! every program as one translation unit (adr:0016, plans/ir-architecture.md).

pub mod abi;

use std::path::Path;

use lotml_ir::ir::Builtin;

/// The runtime's files: `lotml.h` declares it, `lotml.c` includes the rest, and the program
/// includes both, so it is one translation unit.
pub const FILES: &[(&str, &str)] = &[
    ("lotml.h", include_str!("../c/lotml.h")),
    ("lotml.c", include_str!("../c/lotml.c")),
    ("lotml_text.c", include_str!("../c/lotml_text.c")),
    ("lotml_list.c", include_str!("../c/lotml_list.c")),
    ("lotml_dict.c", include_str!("../c/lotml_dict.c")),
];

/// Write the runtime into `dir`, where a compiled program includes it from.
pub fn write(dir: &Path) -> std::io::Result<()> {
    for (name, text) in FILES {
        std::fs::write(dir.join(name), text)?;
    }
    Ok(())
}

/// The runtime function that performs `op`, as both native backends call it (specs/shared-ir R1.3).
pub fn function(op: Builtin) -> &'static str {
    match op {
        Builtin::AssertCompared => "lt_assert_compared",
        Builtin::Comb => "lt_comb",
        Builtin::DictClear => "lt_dict_clear",
        Builtin::DictContains => "lt_dict_contains",
        Builtin::DictCopy => "lt_dict_copy",
        Builtin::DictFromPairs => "lt_dict_from_pairs",
        Builtin::DictGet => "lt_dict_get",
        Builtin::DictGetOptional => "lt_dict_get_optional",
        Builtin::DictGetOr => "lt_dict_get_or",
        Builtin::DictItems => "lt_dict_items",
        Builtin::DictKeys => "lt_dict_keys",
        Builtin::DictPop => "lt_dict_pop",
        Builtin::DictSet => "lt_dict_set",
        Builtin::DictValues => "lt_dict_values",
        Builtin::Factorial => "lt_factorial",
        Builtin::Gcd => "lt_gcd",
        Builtin::HashValue => "lt_hash_value",
        Builtin::HeapPeek => "lt_heap_peek",
        Builtin::HeapPop => "lt_heap_pop",
        Builtin::HeapPush => "lt_heap_push",
        Builtin::Heapify => "lt_heapify",
        Builtin::Isqrt => "lt_isqrt",
        Builtin::ListAll => "lt_list_all",
        Builtin::ListAny => "lt_list_any",
        Builtin::ListClear => "lt_list_clear",
        Builtin::ListConcat => "lt_list_concat",
        Builtin::ListContains => "lt_list_contains",
        Builtin::ListCopy => "lt_list_copy",
        Builtin::ListCount => "lt_list_count",
        Builtin::ListExtend => "lt_list_extend",
        Builtin::ListExtreme => "lt_list_extreme",
        Builtin::ListIndex => "lt_list_index",
        Builtin::ListInsert => "lt_list_insert",
        Builtin::ListLast => "lt_list_last",
        Builtin::ListPop => "lt_list_pop",
        Builtin::ListPush => "lt_list_push",
        Builtin::ListRemove => "lt_list_remove",
        Builtin::ListRepeat => "lt_list_repeat",
        Builtin::ListReverse => "lt_list_reverse",
        Builtin::ListReversed => "lt_list_reversed",
        Builtin::ListSlice => "lt_list_slice",
        Builtin::ListSort => "lt_list_sort",
        Builtin::ListSortByKeys => "lt_list_sort_by_keys",
        Builtin::ListSorted => "lt_list_sorted",
        Builtin::ListUnique => "lt_list_unique",
        Builtin::ListUnpack => "lt_list_unpack",
        Builtin::MathAtan => "lt_math_atan",
        Builtin::MathAtan2 => "lt_math_atan2",
        Builtin::MathCeil => "lt_math_ceil",
        Builtin::MathCos => "lt_math_cos",
        Builtin::MathExp => "lt_math_exp",
        Builtin::MathFabs => "lt_math_fabs",
        Builtin::MathFloor => "lt_math_floor",
        Builtin::MathHypot => "lt_math_hypot",
        Builtin::MathLn => "lt_math_ln",
        Builtin::MathLog10 => "lt_math_log10",
        Builtin::MathLog2 => "lt_math_log2",
        Builtin::MathPow => "lt_math_pow",
        Builtin::MathSin => "lt_math_sin",
        Builtin::MathSqrt => "lt_math_sqrt",
        Builtin::MathTan => "lt_math_tan",
        Builtin::MathTrunc => "lt_math_trunc",
        Builtin::Perm => "lt_perm",
        Builtin::PowMod => "lt_pow_mod",
        Builtin::RangeList => "lt_range_list",
        Builtin::RoundF64 => "lt_round_f64",
        Builtin::RoundI64 => "lt_round_i64",
        Builtin::SetAdd => "lt_set_add",
        Builtin::SetContains => "lt_set_contains",
        Builtin::SetCopy => "lt_set_copy",
        Builtin::SetDifference => "lt_set_difference",
        Builtin::SetDiscard => "lt_set_discard",
        Builtin::SetFromList => "lt_set_from_list",
        Builtin::SetIntersection => "lt_set_intersection",
        Builtin::SetIssubset => "lt_set_issubset",
        Builtin::SetList => "lt_set_list",
        Builtin::SetPop => "lt_set_pop",
        Builtin::SetRemove => "lt_set_remove",
        Builtin::SetUnion => "lt_set_union",
        Builtin::StrCapitalize => "lt_str_capitalize",
        Builtin::StrChars => "lt_str_chars",
        Builtin::StrChr => "lt_str_chr",
        Builtin::StrConcat => "lt_str_concat",
        Builtin::StrCount => "lt_str_count",
        Builtin::StrEndswith => "lt_str_endswith",
        Builtin::StrFind => "lt_str_find",
        Builtin::StrFloat => "lt_str_float",
        Builtin::StrIndex => "lt_str_index",
        Builtin::StrInt => "lt_str_int",
        Builtin::StrIsalnum => "lt_str_isalnum",
        Builtin::StrIsalpha => "lt_str_isalpha",
        Builtin::StrIsdigit => "lt_str_isdigit",
        Builtin::StrIslower => "lt_str_islower",
        Builtin::StrIsspace => "lt_str_isspace",
        Builtin::StrIsupper => "lt_str_isupper",
        Builtin::StrJoin => "lt_str_join",
        Builtin::StrLower => "lt_str_lower",
        Builtin::StrOrd => "lt_str_ord",
        Builtin::StrPad => "lt_str_pad",
        Builtin::StrPartitionPart => "lt_str_partition_part",
        Builtin::StrRepeat => "lt_str_repeat",
        Builtin::StrReplace => "lt_str_replace",
        Builtin::StrSlice => "lt_str_slice",
        Builtin::StrSplit => "lt_str_split",
        Builtin::StrSplitOnce => "lt_str_split_once",
        Builtin::StrSplitlines => "lt_str_splitlines",
        Builtin::StrStartswith => "lt_str_startswith",
        Builtin::StrStrip => "lt_str_strip",
        Builtin::StrSwapcase => "lt_str_swapcase",
        Builtin::StrTitle => "lt_str_title",
        Builtin::StrToFloat => "lt_str_to_float",
        Builtin::StrToInt => "lt_str_to_int",
        Builtin::StrUpper => "lt_str_upper",
        Builtin::StrZfill => "lt_str_zfill",
        Builtin::SumF64 => "lt_sum_f64",
        Builtin::SumI64 => "lt_sum_i64",
        Builtin::SumU64 => "lt_sum_u64",
        Builtin::TestError => "lt_test_error",
        Builtin::WrappingAdd => "lt_wrapping_add",
        Builtin::WrappingMul => "lt_wrapping_mul",
        Builtin::WrappingSub => "lt_wrapping_sub",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_operation_is_a_function_the_runtime_declares() {
        let header = FILES.iter().find(|(name, _)| *name == "lotml.h").map(|(_, text)| *text).unwrap();
        for &op in Builtin::ALL {
            let name = function(op);
            let declared = header.match_indices(name).any(|(i, _)| {
                let after = &header[i + name.len()..];
                let before = header[..i].chars().next_back();
                after.trim_start().starts_with('(') && !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
            });
            assert!(declared, "{op:?} names {name}, which lotml.h does not declare");
        }
    }

    #[test]
    fn no_two_operations_share_a_function() {
        let mut names: Vec<&str> = Builtin::ALL.iter().map(|&op| function(op)).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count);
    }
}
