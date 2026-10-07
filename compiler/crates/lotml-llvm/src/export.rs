//! A module as a C library (specs/c-abi-export, adr:0024-c-abi-exports-chosen-by-signature-without-new-syntax):
//! which of its functions C can call, chosen from their signatures, and the header declaring them.

use std::fmt::Write;

use lotml_check::Checked;
use lotml_check::ty::{FloatKind, IntKind, Ty};
use lotml_diag::Diagnostic;
use lotml_syntax::ast::{Convention, Item, Module};
use lotml_syntax::span::Span;

/// A function the library exports: its LotML name, its C symbol, its parameters and result.
pub struct Export {
    pub name: String,
    pub symbol: String,
    pub params: Vec<(String, Ty)>,
    pub ret: Ty,
    pub span: Span,
}

/// The name a library of the file `stem` takes in C: every character a C identifier cannot hold
/// there replaced by `_`.
pub fn module_name(stem: &str) -> String {
    stem.char_indices()
        .map(|(i, c)| if c.is_ascii_alphabetic() || c == '_' || (i > 0 && c.is_ascii_digit()) { c } else { '_' })
        .collect()
}

/// The file the shared library `name` is written as: `name.dll` on Windows, beside its import
/// library `name.lib`; `libname.dylib` on macOS; `libname.so` elsewhere.
pub fn library_file(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.dll")
    } else if cfg!(target_os = "macos") {
        format!("lib{name}.dylib")
    } else {
        format!("lib{name}.so")
    }
}

/// Whether a value of `ty` cannot cross into C as a parameter (adr:0013).
fn param_refused(ty: &Ty) -> bool {
    !matches!(ty, Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Str)
}

fn ret_refused(ty: &Ty) -> bool {
    !matches!(ty, Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Unit)
}

/// The top-level functions of `module`, other than `main`, that C can call (R1.2), and a warning
/// for each one left out naming what excluded it (R1.4).
pub fn exports(name: &str, module: &Module, checked: &Checked) -> (Vec<Export>, Vec<Diagnostic>) {
    let mut chosen = Vec::new();
    let mut warnings = Vec::new();
    for item in &module.items {
        let Item::Fn(def) = item else { continue };
        let lotml = &def.name.name;
        if lotml == "main" {
            continue;
        }
        let Some(sig) = checked.functions.get(lotml) else { continue };
        let left_out = |why: String| {
            Diagnostic::warning("E0403", def.name.span, format!("`{lotml}` is left out of the C library: {why}"))
        };
        let why = if !sig.type_params.is_empty() {
            Some("it is generic, and C calls one function, not one per type".to_string())
        } else if let Some(error) = &sig.error {
            Some(format!("it returns `{} ! {error}`, a result C cannot read", sig.ret))
        } else if let Some(p) = sig.params.iter().find(|p| p.convention == Convention::Inout) {
            Some(format!("its parameter `{}` is `inout`, a slot C cannot lend", p.name))
        } else if let Some(p) = sig.params.iter().find(|p| param_refused(&p.ty)) {
            Some(format!("its parameter `{}` is a `{}`, which C cannot pass", p.name, p.ty))
        } else if ret_refused(&sig.ret) {
            Some(format!("it returns a `{}`, which C cannot take back", sig.ret))
        } else {
            None
        };
        match why {
            Some(why) => warnings.push(left_out(why)),
            None => chosen.push(Export {
                name: lotml.clone(),
                symbol: format!("{name}_{lotml}"),
                params: sig.params.iter().map(|p| (p.name.clone(), p.ty.clone())).collect(),
                ret: sig.ret.clone(),
                span: def.name.span,
            }),
        }
    }
    (chosen, warnings)
}

/// The C type a value of `ty` crosses as (adr:0013).
pub fn c_type(ty: &Ty) -> &'static str {
    match ty {
        Ty::Int(IntKind::I8) => "int8_t",
        Ty::Int(IntKind::I16) => "int16_t",
        Ty::Int(IntKind::I32) => "int32_t",
        Ty::Int(IntKind::I64) => "int64_t",
        Ty::Int(IntKind::U8) => "uint8_t",
        Ty::Int(IntKind::U16) => "uint16_t",
        Ty::Int(IntKind::U32) => "uint32_t",
        Ty::Int(IntKind::U64) => "uint64_t",
        Ty::Float(FloatKind::F32) => "float",
        Ty::Float(_) => "double",
        Ty::Bool => "bool",
        Ty::Str => "const char *",
        _ => "void",
    }
}

/// The words C and C++ reserve, which a parameter cannot be named in the header.
const RESERVED: [&str; 5] = [
    "alignas alignof and asm auto bool break case catch char class const constexpr continue default",
    "delete do double else enum explicit export extern false float for friend goto if inline int long",
    "mutable namespace new noexcept not nullptr operator or private protected public register",
    "restrict return short signed sizeof static struct switch template this throw true try typedef",
    "typeid typename union unsigned using virtual void volatile while xor",
];

/// A parameter's name as the header writes it: its LotML name, unless C cannot read it as one.
fn c_name(name: &str, index: usize) -> String {
    let plain =
        name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !name.starts_with(|c: char| c.is_ascii_digit());
    if plain && !RESERVED.iter().flat_map(|r| r.split(' ')).any(|w| w == name) && !name.starts_with("__") {
        name.to_string()
    } else {
        format!("p{index}")
    }
}

/// The header of the library `name`, built from `file`, declaring `exports` (R1.3): read by C11 and
/// C++ compilers alike, each function `dllimport` on Windows unless the library is being built.
pub fn header(name: &str, file: &str, exports: &[Export], line: impl Fn(Span) -> u32) -> String {
    let upper = name.to_ascii_uppercase();
    let guard = format!("LOTML_{upper}_H");
    let api = format!("{upper}_API");
    let mut out = format!(
        "/* {name}.h: the C interface of {file}, written by `lotml build --shared`. A function may stop the\n \
         * process with status 101, as a LotML program stops, when it meets a broken invariant. */\n\
         #ifndef {guard}\n#define {guard}\n\n#include <stdbool.h>\n#include <stdint.h>\n\n\
         #if defined(_WIN32) && !defined({upper}_BUILDING)\n#define {api} __declspec(dllimport)\n#else\n#define {api}\n#endif\n\n\
         #ifdef __cplusplus\nextern \"C\" {{\n#endif\n\n"
    );
    for e in exports {
        let params: Vec<String> = e
            .params
            .iter()
            .enumerate()
            .map(|(i, (n, t))| {
                let ty = c_type(t);
                let space = if ty.ends_with('*') { "" } else { " " };
                format!("{ty}{space}{}", c_name(n, i))
            })
            .collect();
        let params = if params.is_empty() { "void".to_string() } else { params.join(", ") };
        let shown: Vec<String> = e.params.iter().map(|(n, t)| format!("{n}: {t}")).collect();
        let _ = writeln!(
            out,
            "/* `{}({})` -> {}, line {} */\n{api} {} {}({params});\n",
            e.name,
            shown.join(", "),
            e.ret,
            line(e.span),
            c_type(&e.ret),
            e.symbol
        );
    }
    let _ = write!(out, "#ifdef __cplusplus\n}}\n#endif\n\n#endif\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_module_name_is_a_c_identifier() {
        assert_eq!(module_name("stats"), "stats");
        assert_eq!(module_name("my-lib.v2"), "my_lib_v2");
        assert_eq!(module_name("2d"), "_d");
        assert_eq!(module_name("café"), "caf_");
    }

    #[test]
    fn a_parameter_c_cannot_name_takes_its_position() {
        assert_eq!(c_name("count", 0), "count");
        assert_eq!(c_name("int", 1), "p1");
        assert_eq!(c_name("é", 2), "p2");
        assert_eq!(c_name("__x", 3), "p3");
    }
}
