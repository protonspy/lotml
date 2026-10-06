//! The prelude (R35) and the methods of built-in types: how each call types.

use crate::ty::{F64, INT, Infer, Ty};

pub const BUILTIN_TRAITS: &[&str] = &["Ord", "Eq", "Hash", "Show", "Num"];

pub const PRELUDE: &[&str] = &[
    "print",
    "len",
    "range",
    "enumerate",
    "zip",
    "sorted",
    "reversed",
    "sum",
    "min",
    "max",
    "abs",
    "any",
    "all",
    "round",
    "int",
    "float",
    "str",
    "bool",
    "ord",
    "chr",
    "set",
    "list",
    "dict",
    "divmod",
    "pow",
    "hash",
    "map",
    "filter",
    "Heap",
    "todo",
    "Ok",
    "Err",
    // A record: what a call into Python fails with (adr:0012).
    "PyError",
    "wrapping_add",
    "wrapping_sub",
    "wrapping_mul",
    "isqrt",
    "gcd",
];

pub fn is_prelude(name: &str) -> bool {
    PRELUDE.contains(&name)
}

/// The element type of something iterated: a list's items, a dict's keys, a string's letters.
pub fn element(ty: &Ty, infer: &mut Infer) -> Option<Ty> {
    match infer.resolve(ty) {
        Ty::List(t) | Ty::Set(t) | Ty::Heap(t) => Some(*t),
        Ty::Dict(k, _) => Some(*k),
        Ty::Str => Some(Ty::Str),
        Ty::Bytes => Some(INT),
        Ty::Tuple(items) if !items.is_empty() && items.iter().all(|t| *t == items[0]) => Some(items[0].clone()),
        Ty::Var(_) => {
            let item = infer.fresh();
            infer.unify(ty, &Ty::list(item.clone()));
            Some(item)
        }
        Ty::Error | Ty::Never => Some(Ty::Error),
        _ => None,
    }
}

fn numeric(ty: &Ty, infer: &Infer) -> bool {
    matches!(infer.resolve(ty), Ty::Int(_) | Ty::Float(_) | Ty::Var(_) | Ty::Error | Ty::Never | Ty::Param(_))
}

fn not_iterable(ty: &Ty, infer: &Infer) -> String {
    format!("`{}` cannot be iterated", infer.resolve(ty))
}

/// The type a lambda argument is checked against, when a built-in call has one.
pub fn lambda_expectation(
    name: &str,
    keyword: Option<&str>,
    position: usize,
    args: &[Ty],
    infer: &mut Infer,
) -> Option<Ty> {
    let first = args.first()?;
    match (name, keyword, position) {
        ("sorted" | "min" | "max", Some("key"), _) => {
            let item = element(first, infer)?;
            Some(Ty::Func(vec![item], Box::new(infer.fresh())))
        }
        ("map", None, 0) | ("filter", None, 0) => {
            let source = args.get(1)?;
            let item = element(source, infer)?;
            let out = if name == "filter" { Ty::Bool } else { infer.fresh() };
            Some(Ty::Func(vec![item], Box::new(out)))
        }
        _ => None,
    }
}

/// How a call to a prelude function types, or why it does not.
pub fn call(name: &str, args: &[Ty], keywords: &[(String, Ty)], infer: &mut Infer) -> Result<Ty, String> {
    let arg = |i: usize| args.get(i).cloned();
    let count = |low: usize, high: usize| -> Result<(), String> {
        if args.len() < low || args.len() > high {
            return Err(if low == high {
                format!("`{name}` takes {low} argument{}", if low == 1 { "" } else { "s" })
            } else {
                format!("`{name}` takes {low} to {high} arguments")
            });
        }
        Ok(())
    };
    match name {
        "print" => Ok(Ty::Unit),
        "todo" => Ok(Ty::Never),
        "len" => {
            count(1, 1)?;
            match infer.resolve(&args[0]) {
                Ty::Str
                | Ty::Bytes
                | Ty::List(_)
                | Ty::Set(_)
                | Ty::Dict(..)
                | Ty::Heap(_)
                | Ty::Tuple(_)
                | Ty::Var(_)
                | Ty::Error
                | Ty::Never
                | Ty::Param(_) => Ok(INT),
                other => Err(format!("`len` needs a string or a collection, not `{other}`")),
            }
        }
        "range" => {
            count(1, 3)?;
            for a in args {
                if !infer.unify(a, &INT) {
                    return Err(format!("`range` takes integers, not `{}`", infer.resolve(a)));
                }
            }
            Ok(Ty::list(INT))
        }
        "enumerate" => {
            count(1, 2)?;
            let item = element(&args[0], infer).ok_or_else(|| not_iterable(&args[0], infer))?;
            Ok(Ty::list(Ty::Tuple(vec![INT, item])))
        }
        "zip" => {
            if args.is_empty() {
                return Err("`zip` takes at least one argument".into());
            }
            let mut items = Vec::new();
            for a in args {
                items.push(element(a, infer).ok_or_else(|| not_iterable(a, infer))?);
            }
            Ok(Ty::list(Ty::Tuple(items)))
        }
        "sorted" | "reversed" | "list" => {
            if name == "list" && args.is_empty() {
                return Ok(Ty::list(infer.fresh()));
            }
            count(1, 1)?;
            let item = element(&args[0], infer).ok_or_else(|| not_iterable(&args[0], infer))?;
            for (k, _) in keywords {
                if name != "sorted" || !matches!(k.as_str(), "key" | "reverse") {
                    return Err(format!("`{name}` has no argument `{k}`"));
                }
            }
            Ok(Ty::list(item))
        }
        "set" => {
            if args.is_empty() {
                return Ok(Ty::Set(Box::new(infer.fresh())));
            }
            count(1, 1)?;
            let item = element(&args[0], infer).ok_or_else(|| not_iterable(&args[0], infer))?;
            Ok(Ty::Set(Box::new(item)))
        }
        "dict" => {
            if args.is_empty() {
                return Ok(Ty::Dict(Box::new(infer.fresh()), Box::new(infer.fresh())));
            }
            count(1, 1)?;
            match element(&args[0], infer).map(|t| infer.resolve(&t)) {
                Some(Ty::Tuple(pair)) if pair.len() == 2 => {
                    Ok(Ty::Dict(Box::new(pair[0].clone()), Box::new(pair[1].clone())))
                }
                _ => Err("`dict` takes a list of (key, value) pairs".into()),
            }
        }
        "Heap" => {
            if args.is_empty() {
                return Ok(Ty::Heap(Box::new(infer.fresh())));
            }
            count(1, 1)?;
            let item = element(&args[0], infer).ok_or_else(|| not_iterable(&args[0], infer))?;
            Ok(Ty::Heap(Box::new(item)))
        }
        "sum" => {
            count(1, 2)?;
            let item = element(&args[0], infer).ok_or_else(|| not_iterable(&args[0], infer))?;
            if !numeric(&item, infer) {
                return Err(format!("`sum` adds numbers, not `{}`", infer.resolve(&item)));
            }
            Ok(item)
        }
        "min" | "max" => {
            if args.is_empty() {
                return Err(format!("`{name}` takes a collection or at least two values"));
            }
            if args.len() == 1 {
                return element(&args[0], infer).ok_or_else(|| not_iterable(&args[0], infer));
            }
            let first = args[0].clone();
            for a in &args[1..] {
                if !infer.unify(&first, a) {
                    return Err(format!(
                        "`{name}` compares values of one type, not `{}` and `{}`",
                        infer.resolve(&first),
                        infer.resolve(a)
                    ));
                }
            }
            Ok(first)
        }
        "abs" => {
            count(1, 1)?;
            if !numeric(&args[0], infer) {
                return Err(format!("`abs` takes a number, not `{}`", infer.resolve(&args[0])));
            }
            Ok(args[0].clone())
        }
        "round" => {
            count(1, 2)?;
            if !numeric(&args[0], infer) {
                return Err(format!("`round` takes a number, not `{}`", infer.resolve(&args[0])));
            }
            Ok(if args.len() == 2 { F64 } else { INT })
        }
        "any" | "all" => {
            count(1, 1)?;
            let item = element(&args[0], infer).ok_or_else(|| not_iterable(&args[0], infer))?;
            if !infer.unify(&item, &Ty::Bool) {
                return Err(format!(
                    "`{name}` takes `bool` values, not `{}`: write a comparison",
                    infer.resolve(&item)
                ));
            }
            Ok(Ty::Bool)
        }
        "int" | "float" => {
            count(1, 1)?;
            match infer.resolve(&args[0]) {
                Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Var(_) | Ty::Error | Ty::Never => {
                    Ok(if name == "int" { INT } else { F64 })
                }
                Ty::Str => Err(format!(
                    "`{name}` converts numbers; text converts with `.{}()`, which returns an optional",
                    if name == "int" { "to_int" } else { "to_float" }
                )),
                other => Err(format!("`{name}` cannot convert `{other}`")),
            }
        }
        "str" | "hash" => {
            count(1, 1)?;
            Ok(if name == "str" { Ty::Str } else { INT })
        }
        "bool" => {
            count(1, 1)?;
            Ok(Ty::Bool)
        }
        "ord" => {
            count(1, 1)?;
            if !infer.unify(&args[0], &Ty::Str) {
                return Err("`ord` takes a one-letter string".into());
            }
            Ok(INT)
        }
        "chr" => {
            count(1, 1)?;
            if !infer.unify(&args[0], &INT) {
                return Err("`chr` takes an int".into());
            }
            Ok(Ty::Str)
        }
        "divmod" => {
            count(2, 2)?;
            if !infer.unify(&args[0], &args[1]) || !numeric(&args[0], infer) {
                return Err("`divmod` takes two numbers of one type".into());
            }
            Ok(Ty::Tuple(vec![args[0].clone(), args[0].clone()]))
        }
        "pow" => {
            count(2, 3)?;
            if !args.iter().all(|a| numeric(a, infer)) {
                return Err("`pow` takes numbers".into());
            }
            Ok(args[0].clone())
        }
        "wrapping_add" | "wrapping_sub" | "wrapping_mul" | "gcd" => {
            count(2, 2)?;
            if !(infer.unify(&args[0], &INT) && infer.unify(&args[1], &INT)) {
                return Err(format!("`{name}` takes two ints"));
            }
            Ok(INT)
        }
        "isqrt" => {
            count(1, 1)?;
            if !infer.unify(&args[0], &INT) {
                return Err("`isqrt` takes an int".into());
            }
            Ok(INT)
        }
        "map" | "filter" => {
            count(2, 2)?;
            let item = element(&args[1], infer).ok_or_else(|| not_iterable(&args[1], infer))?;
            match infer.resolve(&args[0]) {
                Ty::Func(_, out) => Ok(Ty::list(if name == "map" { *out } else { item })),
                Ty::Error | Ty::Never | Ty::Var(_) => Ok(Ty::list(if name == "map" { infer.fresh() } else { item })),
                other => Err(format!("`{name}` takes a function first, not `{other}`")),
            }
        }
        "Ok" => {
            count(1, 1)?;
            Ok(Ty::Result(Box::new(arg(0).unwrap_or(Ty::Error)), Box::new(infer.fresh())))
        }
        "Err" => {
            count(1, 1)?;
            Ok(Ty::Result(Box::new(infer.fresh()), Box::new(arg(0).unwrap_or(Ty::Error))))
        }
        _ => Err(format!("`{name}` is not in the prelude")),
    }
}

const STR_METHODS: &[&str] = &[
    "split",
    "strip",
    "lstrip",
    "rstrip",
    "lower",
    "upper",
    "title",
    "capitalize",
    "swapcase",
    "startswith",
    "endswith",
    "replace",
    "join",
    "count",
    "isalpha",
    "isdigit",
    "isspace",
    "isupper",
    "islower",
    "isalnum",
    "isnumeric",
    "find",
    "rfind",
    "split_once",
    "to_int",
    "to_float",
    "format",
    "zfill",
    "center",
    "ljust",
    "rjust",
    "splitlines",
    "partition",
];
const LIST_METHODS: &[&str] = &[
    "append", "extend", "insert", "pop", "remove", "clear", "sort", "reverse", "index", "count", "last", "find",
    "contains", "copy",
];
const DICT_METHODS: &[&str] =
    &["get", "keys", "values", "items", "pop", "setdefault", "update", "clear", "copy", "contains"];
const SET_METHODS: &[&str] = &[
    "add",
    "remove",
    "discard",
    "clear",
    "pop",
    "union",
    "intersection",
    "difference",
    "issubset",
    "issuperset",
    "copy",
    "update",
];
const HEAP_METHODS: &[&str] = &["push", "pop_min", "peek"];

/// The methods of a built-in type, for "did you mean" lists.
pub fn methods_of(ty: &Ty) -> &'static [&'static str] {
    match ty {
        Ty::Str => STR_METHODS,
        Ty::List(_) => LIST_METHODS,
        Ty::Dict(..) => DICT_METHODS,
        Ty::Set(_) => SET_METHODS,
        Ty::Heap(_) => HEAP_METHODS,
        _ => &[],
    }
}

/// Whether a built-in method changes its receiver, so the receiver must be a `var`.
pub fn mutates(ty: &Ty, method: &str) -> bool {
    match ty {
        Ty::List(_) => {
            matches!(method, "append" | "extend" | "insert" | "pop" | "remove" | "clear" | "sort" | "reverse")
        }
        Ty::Dict(..) => matches!(method, "pop" | "setdefault" | "update" | "clear"),
        Ty::Set(_) => matches!(method, "add" | "remove" | "discard" | "clear" | "pop" | "update"),
        Ty::Heap(_) => matches!(method, "push" | "pop_min"),
        _ => false,
    }
}

/// The type a lambda argument of a built-in method is checked against.
pub fn method_lambda(receiver: &Ty, method: &str, keyword: Option<&str>) -> Option<Ty> {
    match (receiver, method, keyword) {
        (Ty::List(t), "sort", Some("key")) => Some(Ty::Func(vec![(**t).clone()], Box::new(Ty::Error))),
        (Ty::List(t), "find", None) => Some(Ty::Func(vec![(**t).clone()], Box::new(Ty::Bool))),
        _ => None,
    }
}

/// How a built-in method call types: None when the type has no such method.
pub fn method(receiver: &Ty, name: &str, args: &[Ty], infer: &mut Infer) -> Option<Result<Ty, String>> {
    let expect = |infer: &mut Infer, got: &Ty, want: &Ty| -> Result<(), String> {
        if infer.unify(got, want) {
            Ok(())
        } else {
            Err(format!("`{name}` takes `{}` here, not `{}`", infer.resolve(want), infer.resolve(got)))
        }
    };
    let arity = |low: usize, high: usize| -> Result<(), String> {
        if args.len() < low || args.len() > high {
            Err(format!(
                "`{name}` takes {} argument{}",
                if low == high { low.to_string() } else { format!("{low} to {high}") },
                if high == 1 { "" } else { "s" }
            ))
        } else {
            Ok(())
        }
    };
    let result = match receiver {
        Ty::Str => {
            if !STR_METHODS.contains(&name) {
                return None;
            }
            (|| -> Result<Ty, String> {
                match name {
                    "split" => {
                        arity(0, 2)?;
                        if let Some(separator) = args.first() {
                            expect(infer, separator, &Ty::Str)?;
                        }
                        if let Some(most) = args.get(1) {
                            expect(infer, most, &INT)?;
                        }
                        Ok(Ty::list(Ty::Str))
                    }
                    "splitlines" => {
                        arity(0, 0)?;
                        Ok(Ty::list(Ty::Str))
                    }
                    "strip" | "lstrip" | "rstrip" => {
                        arity(0, 1)?;
                        if let Some(characters) = args.first() {
                            expect(infer, characters, &Ty::Str)?;
                        }
                        Ok(Ty::Str)
                    }
                    "lower" | "upper" | "title" | "capitalize" | "swapcase" => {
                        arity(0, 0)?;
                        Ok(Ty::Str)
                    }
                    "startswith" | "endswith" => {
                        arity(1, 1)?;
                        expect(infer, &args[0], &Ty::Str)?;
                        Ok(Ty::Bool)
                    }
                    "replace" => {
                        arity(2, 3)?;
                        expect(infer, &args[0], &Ty::Str)?;
                        expect(infer, &args[1], &Ty::Str)?;
                        Ok(Ty::Str)
                    }
                    "join" => {
                        arity(1, 1)?;
                        let item = element(&args[0], infer).ok_or_else(|| not_iterable(&args[0], infer))?;
                        expect(infer, &item, &Ty::Str)?;
                        Ok(Ty::Str)
                    }
                    "count" => {
                        arity(1, 1)?;
                        Ok(INT)
                    }
                    "find" | "rfind" => {
                        arity(1, 1)?;
                        Ok(Ty::optional(INT))
                    }
                    "split_once" => {
                        arity(1, 1)?;
                        Ok(Ty::optional(Ty::Tuple(vec![Ty::Str, Ty::Str])))
                    }
                    "partition" => Ok(Ty::Tuple(vec![Ty::Str, Ty::Str, Ty::Str])),
                    "to_int" => Ok(Ty::optional(INT)),
                    "to_float" => Ok(Ty::optional(F64)),
                    "format" | "zfill" | "center" | "ljust" | "rjust" => Ok(Ty::Str),
                    _ => Ok(Ty::Bool),
                }
            })()
        }
        Ty::List(t) => {
            if !LIST_METHODS.contains(&name) {
                return None;
            }
            let t = (**t).clone();
            (|| -> Result<Ty, String> {
                match name {
                    "append" => {
                        arity(1, 1)?;
                        expect(infer, &args[0], &t)?;
                        Ok(Ty::Unit)
                    }
                    "extend" => {
                        arity(1, 1)?;
                        let item = element(&args[0], infer).ok_or_else(|| not_iterable(&args[0], infer))?;
                        expect(infer, &item, &t)?;
                        Ok(Ty::Unit)
                    }
                    "insert" => {
                        arity(2, 2)?;
                        expect(infer, &args[0], &INT)?;
                        expect(infer, &args[1], &t)?;
                        Ok(Ty::Unit)
                    }
                    "pop" => {
                        arity(0, 1)?;
                        Ok(Ty::optional(t))
                    }
                    "remove" | "contains" | "index" | "count" => {
                        arity(1, 1)?;
                        expect(infer, &args[0], &t)?;
                        Ok(match name {
                            "remove" => Ty::Unit,
                            "contains" => Ty::Bool,
                            "index" => Ty::optional(INT),
                            _ => INT,
                        })
                    }
                    "clear" | "reverse" | "sort" => Ok(Ty::Unit),
                    "last" => Ok(Ty::optional(t)),
                    "find" => {
                        arity(1, 1)?;
                        Ok(Ty::optional(t))
                    }
                    _ => Ok(Ty::list(t)),
                }
            })()
        }
        Ty::Dict(k, v) => {
            if !DICT_METHODS.contains(&name) {
                return None;
            }
            let (k, v) = ((**k).clone(), (**v).clone());
            (|| -> Result<Ty, String> {
                match name {
                    "get" | "pop" => {
                        arity(1, 2)?;
                        expect(infer, &args[0], &k)?;
                        if args.len() == 2 {
                            expect(infer, &args[1], &v)?;
                            Ok(v)
                        } else {
                            Ok(Ty::optional(v))
                        }
                    }
                    "setdefault" => {
                        arity(2, 2)?;
                        expect(infer, &args[0], &k)?;
                        expect(infer, &args[1], &v)?;
                        Ok(v)
                    }
                    "keys" => Ok(Ty::list(k)),
                    "values" => Ok(Ty::list(v)),
                    "items" => Ok(Ty::list(Ty::Tuple(vec![k, v]))),
                    "contains" => Ok(Ty::Bool),
                    "copy" => Ok(Ty::Dict(Box::new(k), Box::new(v))),
                    _ => Ok(Ty::Unit),
                }
            })()
        }
        Ty::Set(t) => {
            if !SET_METHODS.contains(&name) {
                return None;
            }
            let t = (**t).clone();
            (|| -> Result<Ty, String> {
                match name {
                    "add" | "remove" | "discard" => {
                        arity(1, 1)?;
                        expect(infer, &args[0], &t)?;
                        Ok(Ty::Unit)
                    }
                    "pop" => Ok(Ty::optional(t)),
                    "issubset" | "issuperset" => Ok(Ty::Bool),
                    "union" | "intersection" | "difference" | "copy" => Ok(Ty::Set(Box::new(t))),
                    _ => Ok(Ty::Unit),
                }
            })()
        }
        Ty::Heap(t) => {
            if !HEAP_METHODS.contains(&name) {
                return None;
            }
            let t = (**t).clone();
            (|| -> Result<Ty, String> {
                match name {
                    "push" => {
                        arity(1, 1)?;
                        expect(infer, &args[0], &t)?;
                        Ok(Ty::Unit)
                    }
                    _ => Ok(Ty::optional(t)),
                }
            })()
        }
        _ => return None,
    };
    Some(result)
}

const MATH: &[&str] = &[
    "sqrt",
    "floor",
    "ceil",
    "pow",
    "log",
    "log2",
    "log10",
    "exp",
    "sin",
    "cos",
    "tan",
    "atan",
    "atan2",
    "hypot",
    "fabs",
    "pi",
    "e",
    "inf",
    "gcd",
    "isqrt",
    "factorial",
    "comb",
    "perm",
    "trunc",
];

pub fn module_members(module: &str) -> &'static [&'static str] {
    if module == "math" { MATH } else { &[] }
}

/// A module member's type: a constant, or a function over numbers.
pub fn module_member(module: &str, name: &str) -> Option<Ty> {
    if module != "math" || !MATH.contains(&name) {
        return None;
    }
    Some(match name {
        "pi" | "e" | "inf" => F64,
        "floor" | "ceil" | "trunc" => Ty::Func(vec![Ty::Param("Num".into())], Box::new(INT)),
        "gcd" | "comb" | "perm" => Ty::Func(vec![INT, INT], Box::new(INT)),
        "isqrt" | "factorial" => Ty::Func(vec![INT], Box::new(INT)),
        "pow" | "atan2" | "hypot" => Ty::Func(vec![Ty::Param("Num".into()), Ty::Param("Num".into())], Box::new(F64)),
        "log" => Ty::Func(vec![Ty::Param("Num".into())], Box::new(F64)),
        _ => Ty::Func(vec![Ty::Param("Num".into())], Box::new(F64)),
    })
}
