//! The C signature of each function of the runtime, read from `lotml.h` itself, so the LLVM
//! backend declares and calls them with the types C gives them (specs/llvm-backend R2.7) and no
//! table kept by hand can drift from the header.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::FILES;

/// How a C type crosses a call, as the LLVM backend writes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CType {
    Void,
    /// `bool`: an `i1` extended to a byte, `zeroext` as clang declares it.
    Bool,
    /// An integer of `bits`, and whether it is signed; narrower than 32 bits it crosses extended.
    Int {
        bits: u32,
        signed: bool,
    },
    Double,
    Float,
    /// Any pointer, a function pointer included.
    Ptr,
    /// A struct passed or returned by value, which LLVM IR cannot pass the way C does.
    Struct,
}

impl CType {
    /// The LLVM type of the value, with the attribute the C ABI wants on it.
    pub fn llvm(self) -> &'static str {
        match self {
            CType::Void => "void",
            CType::Bool => "i1 zeroext",
            CType::Int { bits: 8, signed: true } => "i8 signext",
            CType::Int { bits: 8, signed: false } => "i8 zeroext",
            CType::Int { bits: 16, signed: true } => "i16 signext",
            CType::Int { bits: 16, signed: false } => "i16 zeroext",
            CType::Int { bits: 32, .. } => "i32",
            CType::Int { .. } => "i64",
            CType::Double => "double",
            CType::Float => "float",
            CType::Ptr | CType::Struct => "ptr",
        }
    }

    /// The LLVM type alone, without the attribute: what a value of it is in a register.
    pub fn bare(self) -> &'static str {
        self.llvm().split(' ').next().unwrap_or("void")
    }

    /// The type as a function returns it, in a declaration and a call: the attribute, if any,
    /// before the type, where LLVM wants a return attribute.
    pub fn returned(self) -> String {
        match self.llvm().split_once(' ') {
            Some((ty, attribute)) => format!("{attribute} {ty}"),
            None => self.llvm().to_string(),
        }
    }
}

/// A function of the runtime as C declares it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    pub ret: CType,
    pub params: Vec<CType>,
    pub variadic: bool,
    pub noreturn: bool,
}

impl Signature {
    /// The LLVM declaration of the function `name`.
    pub fn declaration(&self, name: &str) -> String {
        let mut params: Vec<&str> = self.params.iter().map(|p| p.llvm()).collect();
        if self.variadic {
            params.push("...");
        }
        let noreturn = if self.noreturn { " noreturn" } else { "" };
        format!("declare {} @{name}({}){noreturn}", self.ret.returned(), params.join(", "))
    }
}

/// The signature `lotml.h` declares for the runtime function `name`.
pub fn signature(name: &str) -> Option<&'static Signature> {
    static TABLE: OnceLock<HashMap<String, Signature>> = OnceLock::new();
    TABLE.get_or_init(|| parse(header())).get(name)
}

fn header() -> &'static str {
    FILES.iter().find(|(name, _)| *name == "lotml.h").map_or("", |(_, text)| text)
}

/// Every function declaration of a C header in this runtime's style: one declaration, or an
/// inline definition, per statement, at the start of a line.
fn parse(text: &str) -> HashMap<String, Signature> {
    let mut table = HashMap::new();
    let mut statement = String::new();
    let mut in_comment = false;
    let mut depth = 0usize;
    for line in text.lines() {
        let mut line = line.trim().to_string();
        if in_comment {
            match line.find("*/") {
                Some(end) => {
                    line = line[end + 2..].to_string();
                    in_comment = false;
                }
                None => continue,
            }
        }
        if let Some(start) = line.find("/*") {
            match line[start..].find("*/") {
                Some(end) => line = format!("{}{}", &line[..start], &line[start + end + 2..]),
                None => {
                    line.truncate(start);
                    in_comment = true;
                }
            }
        }
        if depth > 0 {
            depth = (depth + line.matches('{').count()).saturating_sub(line.matches('}').count());
            continue;
        }
        if line.starts_with('#') || line.starts_with("typedef") || line.starts_with("extern") || line.is_empty() {
            continue;
        }
        statement.push(' ');
        statement.push_str(&line);
        if let Some(open) = statement.find('{') {
            let declaration = statement[..open].to_string();
            depth = statement.matches('{').count().saturating_sub(statement.matches('}').count());
            statement.clear();
            if let Some((name, sig)) = declaration_of(&declaration) {
                table.insert(name, sig);
            }
        } else if statement.trim_end().ends_with(';') {
            if let Some((name, sig)) = declaration_of(statement.trim_end().trim_end_matches(';')) {
                table.insert(name, sig);
            }
            statement.clear();
        } else if !statement.contains('(') && !line.ends_with(',') {
            statement.clear();
        }
    }
    table
}

/// `ret name(params)` as a name and a signature, when it is a function declaration.
fn declaration_of(text: &str) -> Option<(String, Signature)> {
    let text = text.trim();
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    let head = text[..open].trim();
    let name = head.rsplit(|c: char| c.is_whitespace() || c == '*').next()?.to_string();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let ret_text = head[..head.len() - name.len()].trim();
    let noreturn = ret_text.contains("LT_NORETURN");
    let ret = ctype(ret_text)?;
    let mut params = Vec::new();
    let mut variadic = false;
    for param in split_params(&text[open + 1..close]) {
        let param = param.trim();
        if param == "..." {
            variadic = true;
        } else if param == "void" || param.is_empty() {
            continue;
        } else {
            params.push(ctype(param)?);
        }
    }
    Some((name, Signature { ret, params, variadic, noreturn }))
}

/// The parameters of a declaration, split at the commas outside parentheses.
fn split_params(text: &str) -> Vec<String> {
    let (mut out, mut current, mut depth) = (Vec::new(), String::new(), 0usize);
    for c in text.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}

/// The C type a return type or a parameter (with its name) is.
fn ctype(text: &str) -> Option<CType> {
    if text.contains('(') || text.contains('*') || text.contains('[') {
        return Some(CType::Ptr);
    }
    let words: Vec<&str> = text
        .split_whitespace()
        .filter(|w| !matches!(*w, "const" | "static" | "LT_NORETURN" | "LT_INLINE" | "inline" | "struct" | "LT_UNUSED"))
        .collect();
    let base = *words.first()?;
    Some(match base {
        "void" => CType::Void,
        "bool" => CType::Bool,
        "char" | "int8_t" => CType::Int { bits: 8, signed: true },
        "uint8_t" => CType::Int { bits: 8, signed: false },
        "int16_t" => CType::Int { bits: 16, signed: true },
        "uint16_t" => CType::Int { bits: 16, signed: false },
        "int" | "int32_t" => CType::Int { bits: 32, signed: true },
        "unsigned" | "uint32_t" => CType::Int { bits: 32, signed: false },
        "int64_t" | "long" => CType::Int { bits: 64, signed: true },
        "uint64_t" | "size_t" => CType::Int { bits: 64, signed: false },
        "double" => CType::Double,
        "float" => CType::Float,
        _ if base.starts_with("lt_") => CType::Struct,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::function;
    use lotml_ir::ir::Builtin;

    #[test]
    fn the_header_s_declarations_are_read_with_their_types() {
        let index = signature("lt_index").expect("lt_index");
        assert_eq!(index.ret, CType::Int { bits: 64, signed: true });
        assert_eq!(
            index.params,
            vec![CType::Int { bits: 64, signed: true }, CType::Int { bits: 64, signed: true }, CType::Ptr]
        );
        let panic = signature("lt_panicf").expect("lt_panicf");
        assert!(panic.noreturn && panic.variadic);
        assert_eq!(signature("lt_dec").unwrap().ret, CType::Bool);
        let math = signature("lt_math_1").expect("lt_math_1");
        assert_eq!(math.params[0], CType::Ptr, "a function pointer is a pointer");
        assert_eq!(
            signature("lt_list_push").unwrap().declaration("lt_list_push"),
            "declare void @lt_list_push(ptr, ptr)"
        );
    }

    #[test]
    fn every_built_in_operation_has_a_signature_that_passes_no_struct_by_value() {
        for &op in Builtin::ALL {
            let name = function(op);
            let sig = signature(name).unwrap_or_else(|| panic!("{op:?}: no signature for {name}"));
            assert!(
                sig.ret != CType::Struct && !sig.params.contains(&CType::Struct),
                "{op:?}: {name} passes a struct by value: {sig:?}"
            );
        }
    }
}
