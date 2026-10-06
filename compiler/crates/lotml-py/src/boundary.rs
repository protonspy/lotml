//! The boundary with Python (adr:0012). A type crossing it is written as a descriptor, which the
//! runtime checks a value against. A compiled module's exports say which of its functions and
//! types Python sees, and its `.pyi` stub says what types Python's checkers see.

use std::fmt::Write as _;

use lotml_check::ty::Ty;
use lotml_check::{Checked, FieldSig, FnSig, TypeDef};
use lotml_syntax::ast::Convention;
use serde_json::{Map, Value, json};

use crate::emit::range_of;

/// A type as the runtime's `accept` reads it: a list naming the kind, then its parts.
pub fn descriptor(ty: &Ty) -> Value {
    match ty {
        Ty::Int(kind) => {
            let (low, high) = range_of(*kind);
            json!(["int", number(low), number(high), ty.to_string()])
        }
        Ty::Float(_) => json!(["float"]),
        Ty::Bool => json!(["bool"]),
        Ty::Str => json!(["str"]),
        Ty::Bytes => json!(["bytes"]),
        Ty::Unit => json!(["none"]),
        Ty::List(t) => json!(["list", descriptor(t)]),
        Ty::Set(t) => json!(["set", descriptor(t)]),
        Ty::Dict(k, v) => json!(["dict", descriptor(k), descriptor(v)]),
        Ty::Tuple(items) => Value::Array(std::iter::once(json!("tuple")).chain(items.iter().map(descriptor)).collect()),
        Ty::Optional(t) => json!(["optional", descriptor(t)]),
        Ty::Adt(name, _) => json!(["adt", name]),
        _ => json!(["any"]),
    }
}

fn number(n: i128) -> Value {
    i64::try_from(n).map_or_else(|_| Value::from(u64::try_from(n).unwrap_or(u64::MAX)), Value::from)
}

/// A record's or variant's fields: each name — `_0`, `_1` for positional ones, as the backend
/// names them — with its type.
fn fields(fields: &[FieldSig]) -> Value {
    fields
        .iter()
        .enumerate()
        .map(|(i, f)| json!([f.name.clone().unwrap_or_else(|| format!("_{i}")), descriptor(&f.ty)]))
        .collect()
}

/// What Python sees of a compiled module: each function's signature as descriptors, the shape
/// of each record and sum type, and the names of the types and variants it can build.
pub fn exports(checked: &Checked) -> Value {
    let mut functions = Map::new();
    for (name, sig) in &checked.functions {
        functions.insert(
            name.clone(),
            json!({
                "params": sig.params.iter().map(|p| json!([p.name, descriptor(&p.ty), p.has_default])).collect::<Vec<_>>(),
                "returns": descriptor(&sig.ret),
                "error": sig.error.as_ref().map(descriptor),
                "inout": sig.params.iter().any(|p| p.convention == Convention::Inout),
            }),
        );
    }
    let mut types = Map::new();
    let mut names = Vec::new();
    for (name, def) in &checked.declared {
        match def {
            TypeDef::Record { fields: fs, .. } => {
                types.insert(name.clone(), json!(["record", fields(fs)]));
                names.push(name.clone());
            }
            TypeDef::Sum { variants, .. } => {
                let cases: Vec<Value> =
                    variants.iter().map(|v| json!([v.name, v.fields.as_deref().map(fields)])).collect();
                types.insert(name.clone(), json!(["sum", cases]));
                names.extend(variants.iter().map(|v| v.name.clone()));
            }
        }
    }
    json!({"functions": functions, "types": types, "names": names})
}

/// The Python annotation for a lotml type, as a `.pyi` writes it.
fn annotation(ty: &Ty) -> String {
    match ty {
        Ty::Int(_) => "int".into(),
        Ty::Float(_) => "float".into(),
        Ty::Bool => "bool".into(),
        Ty::Str => "str".into(),
        Ty::Bytes => "bytes".into(),
        Ty::Unit => "None".into(),
        Ty::List(t) => format!("list[{}]", annotation(t)),
        Ty::Set(t) => format!("set[{}]", annotation(t)),
        Ty::Dict(k, v) => format!("dict[{}, {}]", annotation(k), annotation(v)),
        Ty::Tuple(items) if items.is_empty() => "tuple[()]".into(),
        Ty::Tuple(items) => format!("tuple[{}]", items.iter().map(annotation).collect::<Vec<_>>().join(", ")),
        Ty::Optional(t) => format!("{} | None", annotation(t)),
        Ty::Result(t, _) => annotation(t),
        Ty::Adt(name, _) => name.clone(),
        Ty::Heap(t) => format!("lotml_rt.Heap[{}]", annotation(t)),
        Ty::Func(params, ret) => {
            format!("Callable[[{}], {}]", params.iter().map(annotation).collect::<Vec<_>>().join(", "), annotation(ret))
        }
        _ => "Any".into(),
    }
}

fn signature(name: &str, sig: &FnSig) -> String {
    let params: Vec<String> = sig
        .params
        .iter()
        .map(|p| format!("{}: {}{}", p.name, annotation(&p.ty), if p.has_default { " = ..." } else { "" }))
        .collect();
    format!("def {name}({}) -> {}: ...\n", params.join(", "), annotation(&sig.ret))
}

/// The `.pyi` for a compiled module, so Python's type checkers see its types: records as
/// classes, a sum type as the union of its variants, and each function's signature — one that
/// can fail raising `lotml_rt.LotmlError`, as its comment says.
pub fn stub(source: &str, module: &str, checked: &Checked) -> String {
    let mut out = format!(
        "# Generated by lotml from {source}: the types Python sees in `{module}`. Do not edit.\n\
         from collections.abc import Callable\nfrom typing import Any, Final, TypeAlias\n\nimport lotml_rt\n"
    );
    let class = |out: &mut String, name: &str, fs: &[FieldSig]| {
        let _ = write!(out, "\nclass {name}:\n");
        let mut params = Vec::new();
        for (i, f) in fs.iter().enumerate() {
            let field = f.name.clone().unwrap_or_else(|| format!("_{i}"));
            let _ = writeln!(out, "    {field}: {}", annotation(&f.ty));
            params.push(format!("{field}: {}{}", annotation(&f.ty), if f.has_default { " = ..." } else { "" }));
        }
        let _ = writeln!(out, "    def __init__(self, {}) -> None: ...", params.join(", "));
    };
    for (name, def) in &checked.declared {
        match def {
            TypeDef::Record { fields: fs, .. } => class(&mut out, name, fs),
            TypeDef::Sum { variants, .. } => {
                let mut cases = Vec::new();
                for v in variants {
                    match &v.fields {
                        Some(fs) => {
                            class(&mut out, &v.name, fs);
                            cases.push(v.name.clone());
                        }
                        None => {
                            let _ = write!(out, "\n{}: Final[lotml_rt.Unit]\n", v.name);
                            if !cases.contains(&"lotml_rt.Unit".to_string()) {
                                cases.push("lotml_rt.Unit".into());
                            }
                        }
                    }
                }
                let _ = write!(out, "\n{name}: TypeAlias = {}\n", cases.join(" | "));
            }
        }
    }
    for (name, sig) in &checked.functions {
        out.push('\n');
        if let Some(error) = &sig.error {
            let _ =
                writeln!(out, "# Raises lotml_rt.LotmlError when it fails; its `error` is a {}.", annotation(error));
        }
        if sig.params.iter().any(|p| p.convention == Convention::Inout) {
            out += "# Changes an argument in place (`inout`): call it from lotml.\n";
        }
        out += &signature(name, sig);
    }
    out
}
