//! The format spec of an f-string field, `{value:spec}`, read as Python reads it — the order and
//! the errors of CPython's `format()`, which RustPython's `FormatSpec::parse` reproduces — so a
//! spec the program would fail on when it runs is an error when it is checked
//! (plans/frontend-robustness.md 2.2).

use crate::ty::Ty;

/// The widest width or precision either target lays out: the native runtime stops past it
/// (`LT_SPEC_LIMIT` in `lotml_text.c`), so the Python target is held to it too.
const LIMIT: u64 = 10_000;

/// A spec read into its parts: `[[fill]align][sign][z][#][0][width][grouping][.precision][type]`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Spec {
    pub fill: Option<char>,
    pub align: Option<char>,
    pub sign: Option<char>,
    pub no_negative_zero: bool,
    pub alternate: bool,
    pub zero: bool,
    pub width: Option<u64>,
    pub grouping: Option<char>,
    pub precision: Option<u64>,
    pub kind: Option<char>,
}

/// What a value is formatted as, which decides the spec it takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Str,
    Int,
    Float,
    /// A value with no format of its own: Python's `object.__format__` takes only an empty spec.
    Other(&'static str),
}

/// The kind a value of `ty` is formatted as, after a conversion (`!r`, `!s`, `!a`) if any; `None`
/// when it cannot be known while checking — a type parameter, an optional, a type not yet inferred
/// — and the spec is then left to the run.
pub fn kind(ty: &Ty, conversion: Option<char>) -> Option<Kind> {
    if conversion.is_some() {
        return Some(Kind::Str);
    }
    Some(match ty {
        Ty::Str => Kind::Str,
        Ty::Int(_) | Ty::Bool => Kind::Int,
        Ty::Float(_) => Kind::Float,
        Ty::Bytes => Kind::Other("bytes"),
        Ty::Unit => Kind::Other("NoneType"),
        Ty::List(_) => Kind::Other("list"),
        Ty::Set(_) => Kind::Other("set"),
        Ty::Dict(..) => Kind::Other("dict"),
        Ty::Tuple(_) => Kind::Other("tuple"),
        Ty::Func(..) => Kind::Other("function"),
        Ty::Adt(..) => Kind::Other("a record or sum type"),
        _ => return None,
    })
}

fn is_align(c: char) -> bool {
    matches!(c, '<' | '>' | '^' | '=')
}

/// The digits at the start of `rest`, as a number, and what follows them.
fn number(rest: &str) -> Result<(Option<u64>, &str), String> {
    let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 {
        return Ok((None, rest));
    }
    let value = rest[..digits].parse::<u64>().ok().filter(|v| *v <= LIMIT);
    let Some(value) = value else {
        return Err(format!("a width or precision is at most {LIMIT}"));
    };
    Ok((Some(value), &rest[digits..]))
}

/// `text` read as a spec, or why it is not one; `kind` names the value in the message.
pub fn parse(text: &str, kind: &str) -> Result<Spec, String> {
    if text.contains(['{', '}']) {
        return Err("a format spec cannot hold a field: lotml reads the spec as written, so compute the \
                    width into the text, or pad with `ljust`, `rjust` or `center`"
            .into());
    }
    let mut spec = Spec::default();
    let mut chars = text.chars();
    let first = chars.next();
    let second = chars.next();
    let mut rest = text;
    if let (Some(fill), Some(align)) = (first, second)
        && is_align(align)
    {
        spec.fill = Some(fill);
        spec.align = Some(align);
        rest = &text[fill.len_utf8() + 1..];
    } else if let Some(align) = first.filter(|c| is_align(*c)) {
        spec.align = Some(align);
        rest = &text[1..];
    }
    if let Some(sign) = rest.chars().next().filter(|c| matches!(c, '+' | '-' | ' ')) {
        spec.sign = Some(sign);
        rest = &rest[1..];
    }
    if let Some(after) = rest.strip_prefix('z') {
        spec.no_negative_zero = true;
        rest = after;
    }
    if let Some(after) = rest.strip_prefix('#') {
        spec.alternate = true;
        rest = after;
    }
    if let Some(after) = rest.strip_prefix('0') {
        spec.zero = true;
        rest = after;
    }
    (spec.width, rest) = number(rest)?;
    if let Some(grouping) = rest.chars().next().filter(|c| matches!(c, ',' | '_')) {
        spec.grouping = Some(grouping);
        rest = &rest[1..];
    }
    if let Some(after) = rest.strip_prefix('.') {
        (spec.precision, rest) = number(after)?;
        if spec.precision.is_none() {
            return Err("Format specifier missing precision".into());
        }
    }
    let mut left = rest.chars();
    spec.kind = left.next();
    if left.next().is_some() {
        return Err(format!("Invalid format specifier '{text}' for object of type '{kind}'"));
    }
    Ok(spec)
}

/// Why a value of `kind` cannot be formatted with `spec`, in Python's words; `None` when it can.
pub fn refused(spec: &Spec, text: &str, kind: Kind) -> Option<String> {
    let code = |k: Option<char>| k.map_or("s".to_string(), String::from);
    match kind {
        Kind::Other(name) => {
            (!text.is_empty()).then(|| format!("unsupported format string passed to {name}.__format__"))
        }
        Kind::Str => {
            if let Some(k) = spec.kind.filter(|k| *k != 's') {
                return Some(format!("Unknown format code '{k}' for object of type 'str'"));
            }
            if spec.sign.is_some() {
                return Some("Sign not allowed in string format specifier".into());
            }
            if spec.no_negative_zero {
                return Some("Negative zero coercion (z) not allowed in string format specifier".into());
            }
            if spec.alternate {
                return Some("Alternate form (#) not allowed in string format specifier".into());
            }
            if spec.align == Some('=') {
                return Some("'=' alignment not allowed in string format specifier".into());
            }
            spec.grouping.map(|g| format!("Cannot specify '{g}' with 's'."))
        }
        Kind::Int => match spec.kind {
            Some('e' | 'E' | 'f' | 'F' | 'g' | 'G' | '%') => refused(spec, text, Kind::Float),
            Some(k) if !matches!(k, 'b' | 'c' | 'd' | 'o' | 'x' | 'X' | 'n') => {
                Some(format!("Unknown format code '{k}' for object of type 'int'"))
            }
            k => {
                if spec.precision.is_some() {
                    return Some("Precision not allowed in integer format specifier".into());
                }
                if spec.no_negative_zero {
                    return Some("Negative zero coercion (z) not allowed in integer format specifier".into());
                }
                if k == Some('c') && spec.sign.is_some() {
                    return Some("Sign not allowed with integer format specifier 'c'".into());
                }
                if k == Some('c') && spec.alternate {
                    return Some("Alternate form (#) not allowed with integer format specifier 'c'".into());
                }
                match (spec.grouping, k) {
                    (Some(g), Some('n' | 'c')) | (Some(g @ ','), Some('b' | 'o' | 'x' | 'X')) => {
                        Some(format!("Cannot specify '{g}' with '{}'.", code(k)))
                    }
                    _ => None,
                }
            }
        },
        Kind::Float => match spec.kind {
            Some(k) if !matches!(k, 'e' | 'E' | 'f' | 'F' | 'g' | 'G' | 'n' | '%') => {
                Some(format!("Unknown format code '{k}' for object of type 'float'"))
            }
            Some('n') => spec.grouping.map(|g| format!("Cannot specify '{g}' with 'n'.")),
            _ => None,
        },
    }
}

/// Why `text` is no spec for a value of `kind`; `None` when it is one.
pub fn check(text: &str, kind: Kind) -> Option<String> {
    let name = match kind {
        Kind::Str => "str",
        Kind::Int => "int",
        Kind::Float => "float",
        Kind::Other(name) => name,
    };
    if let Kind::Other(_) = kind {
        return refused(&Spec::default(), text, kind);
    }
    match parse(text, name) {
        Ok(spec) => refused(&spec, text, kind),
        Err(why) => Some(why),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Specs CPython 3.13 accepts, with the kind of value each formats.
    const ACCEPTED: &[(&str, Kind)] = &[
        ("", Kind::Int),
        (">5", Kind::Int),
        ("*^10", Kind::Str),
        ("05", Kind::Str),
        ("+08.3f", Kind::Float),
        ("_x", Kind::Int),
        ("#o", Kind::Int),
        (",d", Kind::Int),
        ("e", Kind::Int),
        ("%", Kind::Int),
        (".2%", Kind::Float),
        ("z.1f", Kind::Float),
        ("n", Kind::Float),
        ("c", Kind::Int),
        ("s", Kind::Str),
        ("é<4", Kind::Str),
        ("", Kind::Other("list")),
    ];

    /// Specs CPython 3.13 refuses, with the message `format()` raises.
    const REFUSED: &[(&str, Kind, &str)] = &[
        ("d", Kind::Str, "Unknown format code 'd' for object of type 'str'"),
        (">5", Kind::Other("list"), "unsupported format string passed to list.__format__"),
        ("s", Kind::Int, "Unknown format code 's' for object of type 'int'"),
        (".2", Kind::Int, "Precision not allowed in integer format specifier"),
        (",x", Kind::Int, "Cannot specify ',' with 'x'."),
        ("=5", Kind::Str, "'=' alignment not allowed in string format specifier"),
        ("+", Kind::Str, "Sign not allowed in string format specifier"),
        ("#", Kind::Str, "Alternate form (#) not allowed in string format specifier"),
        (",s", Kind::Str, "Cannot specify ',' with 's'."),
        (",", Kind::Str, "Cannot specify ',' with 's'."),
        ("z", Kind::Str, "Negative zero coercion (z) not allowed in string format specifier"),
        ("zd", Kind::Int, "Negative zero coercion (z) not allowed in integer format specifier"),
        ("c", Kind::Float, "Unknown format code 'c' for object of type 'float'"),
        ("+c", Kind::Int, "Sign not allowed with integer format specifier 'c'"),
        ("#c", Kind::Int, "Alternate form (#) not allowed with integer format specifier 'c'"),
        (",n", Kind::Float, "Cannot specify ',' with 'n'."),
        (".", Kind::Int, "Format specifier missing precision"),
        ("5.", Kind::Float, "Format specifier missing precision"),
        ("x", Kind::Float, "Unknown format code 'x' for object of type 'float'"),
        ("5dd", Kind::Int, "Invalid format specifier '5dd' for object of type 'int'"),
        ("10001", Kind::Int, "a width or precision is at most 10000"),
    ];

    #[test]
    fn the_specs_python_accepts_are_accepted() {
        for (text, kind) in ACCEPTED {
            assert_eq!(check(text, *kind), None, "{text:?} for {kind:?}");
        }
    }

    #[test]
    fn the_specs_python_refuses_are_refused_in_its_words() {
        for (text, kind, message) in REFUSED {
            assert_eq!(check(text, *kind).as_deref(), Some(*message), "{text:?} for {kind:?}");
        }
    }

    #[test]
    fn a_field_inside_a_spec_is_refused_rather_than_read_as_text() {
        assert!(check(">{width}", Kind::Int).is_some_and(|m| m.contains("cannot hold a field")));
    }

    #[test]
    fn a_spec_is_read_into_its_parts_in_python_s_order() {
        let spec = parse("*>+z#012,.3f", "float").unwrap();
        assert_eq!(
            spec,
            Spec {
                fill: Some('*'),
                align: Some('>'),
                sign: Some('+'),
                no_negative_zero: true,
                alternate: true,
                zero: true,
                width: Some(12),
                grouping: Some(','),
                precision: Some(3),
                kind: Some('f'),
            }
        );
    }

    #[test]
    fn a_conversion_formats_the_value_as_text_and_an_unknown_type_is_left_to_the_run() {
        assert_eq!(kind(&Ty::Int(crate::ty::IntKind::I64), Some('r')), Some(Kind::Str));
        assert_eq!(kind(&Ty::Bool, None), Some(Kind::Int));
        assert_eq!(kind(&Ty::Param("T".into()), None), None);
        assert_eq!(kind(&Ty::optional(Ty::Str), None), None);
    }
}
