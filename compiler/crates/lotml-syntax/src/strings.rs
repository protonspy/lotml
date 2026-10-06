//! String literals: their prefix, their quotes, and their value with escapes decoded, as
//! Python reads them. An f-string's `{…}` parts are returned as source ranges for the parser.

/// One literal split into what the parser needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Literal {
    pub raw: bool,
    pub bytes: bool,
    pub format: bool,
    /// Byte offsets of the body (between the quotes) within the literal's text.
    pub body: std::ops::Range<usize>,
}

pub fn split(text: &str) -> Literal {
    let prefix_end = text.find(['"', '\'']).unwrap_or(0);
    let prefix = text[..prefix_end].to_ascii_lowercase();
    let quote = &text[prefix_end..];
    let width = if quote.starts_with("\"\"\"") || quote.starts_with("'''") { 3 } else { 1 };
    let start = prefix_end + width;
    let end = text.len().saturating_sub(width).max(start);
    Literal { raw: prefix.contains('r'), bytes: prefix.contains('b'), format: prefix.contains('f'), body: start..end }
}

/// The value of a string's body, escapes decoded unless the literal is raw.
pub fn decode(body: &str, raw: bool) -> Result<String, String> {
    if raw {
        return Ok(body.to_string());
    }
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let Some(next) = chars.next() else {
            out.push('\\');
            break;
        };
        match next {
            '\n' => {}
            '\\' => out.push('\\'),
            '\'' => out.push('\''),
            '"' => out.push('"'),
            'a' => out.push('\u{7}'),
            'b' => out.push('\u{8}'),
            'f' => out.push('\u{c}'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'v' => out.push('\u{b}'),
            'x' | 'u' | 'U' => {
                let width = match next {
                    'x' => 2,
                    'u' => 4,
                    _ => 8,
                };
                let digits: String = (0..width).filter_map(|_| chars.next()).collect();
                let code = u32::from_str_radix(&digits, 16)
                    .map_err(|_| format!("`\\{next}{digits}` is not a valid escape"))?;
                out.push(char::from_u32(code).ok_or_else(|| format!("`\\{next}{digits}` is not a character"))?);
            }
            '0'..='7' => {
                let mut digits = next.to_string();
                while digits.len() < 3 {
                    match chars.peek() {
                        Some(d @ '0'..='7') => {
                            digits.push(*d);
                            chars.next();
                        }
                        _ => break,
                    }
                }
                let code = u32::from_str_radix(&digits, 8).unwrap_or(0);
                out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
            }
            other => {
                // Python keeps an unknown escape as written.
                out.push('\\');
                out.push(other);
            }
        }
    }
    Ok(out)
}

/// A piece of an f-string's body: literal text, or an expression field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    /// Text as written, `{{` and `}}` already halved.
    Text(String),
    /// Byte ranges within the body: the expression, then the conversion character and the
    /// format spec when present.
    Field { expr: std::ops::Range<usize>, conversion: Option<char>, spec: Option<std::ops::Range<usize>> },
}

/// The pieces of an f-string body, or the offset and reason of the first malformed field.
pub fn pieces(body: &str) -> Result<Vec<Piece>, (usize, String)> {
    let bytes = body.as_bytes();
    let mut out = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' if bytes.get(i + 1) == Some(&b'{') => {
                text.push('{');
                i += 2;
            }
            b'}' if bytes.get(i + 1) == Some(&b'}') => {
                text.push('}');
                i += 2;
            }
            b'}' => return Err((i, "a single `}` in an f-string must be written `}}`".into())),
            b'{' => {
                if !text.is_empty() {
                    out.push(Piece::Text(std::mem::take(&mut text)));
                }
                let start = i + 1;
                let (mut depth, mut j, mut quote) = (0usize, start, None::<u8>);
                let mut expr_end = None;
                let mut conversion = None;
                let mut spec_start = None;
                while j < bytes.len() {
                    let c = bytes[j];
                    if let Some(q) = quote {
                        if c == q {
                            quote = None;
                        }
                    } else {
                        match c {
                            b'\'' | b'"' => quote = Some(c),
                            b'(' | b'[' | b'{' => depth += 1,
                            b')' | b']' => depth = depth.saturating_sub(1),
                            b'}' if depth > 0 => depth -= 1,
                            b'}' => break,
                            b'!' if depth == 0 && expr_end.is_none() && bytes.get(j + 1) != Some(&b'=') => {
                                expr_end = Some(j);
                                conversion = bytes.get(j + 1).map(|&b| b as char);
                                j += 1;
                            }
                            b':' if depth == 0 && spec_start.is_none() => {
                                expr_end.get_or_insert(j);
                                spec_start = Some(j + 1);
                            }
                            _ => {}
                        }
                    }
                    j += 1;
                }
                if j >= bytes.len() {
                    return Err((i, "this f-string field is never closed".into()));
                }
                let end = expr_end.unwrap_or(j);
                if body[start..end].trim().is_empty() {
                    return Err((i, "an f-string field needs an expression".into()));
                }
                out.push(Piece::Field { expr: start..end, conversion, spec: spec_start.map(|s| s..j) });
                i = j + 1;
            }
            _ => {
                let width = body[i..].chars().next().map_or(1, char::len_utf8);
                text.push_str(&body[i..i + width]);
                i += width;
            }
        }
    }
    if !text.is_empty() {
        out.push(Piece::Text(text));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_and_quotes_are_found() {
        assert_eq!(split("rb'x'"), Literal { raw: true, bytes: true, format: false, body: 3..4 });
        assert_eq!(split("f\"\"\"ab\"\"\"").body, 4..6);
    }

    #[test]
    fn escapes_decode_as_python_reads_them() {
        assert_eq!(decode(r"a\n\t\\\x41é\101\q", false).unwrap(), "a\n\t\\Aé A\\q".replace(' ', ""));
        assert_eq!(decode(r"a\n", true).unwrap(), r"a\n");
        assert!(decode(r"\xZZ", false).is_err());
    }

    #[test]
    fn f_string_fields_split_with_conversion_and_spec() {
        let body = "{name}: {value!r:>{width}} {{x}}";
        let pieces = pieces(body).unwrap();
        assert_eq!(pieces.len(), 4);
        match &pieces[2] {
            Piece::Field { expr, conversion, spec } => {
                assert_eq!(&body[expr.clone()], "value");
                assert_eq!(*conversion, Some('r'));
                assert_eq!(&body[spec.clone().unwrap()], ">{width}");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(pieces[3], Piece::Text(" {x}".into()));
    }

    #[test]
    fn malformed_fields_are_errors() {
        assert!(pieces("{x").is_err());
        assert!(pieces("}").is_err());
        assert!(pieces("{}").is_err());
        assert!(pieces("{d['k']}").is_ok());
    }
}
