//! JSON-RPC 2.0 over standard input and output, in the two framings the servers speak: the
//! language server's `Content-Length` header before each message, and MCP's one message per line.

use std::io::{self, BufRead, Read, Write};

use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Framing {
    /// `Content-Length: N`, a blank line, then N bytes: the Language Server Protocol.
    Headers,
    /// One message per line: MCP's stdio transport.
    Lines,
}

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;

/// The largest message read: a client may not make the server allocate without bound.
const LIMIT: usize = 64 * 1024 * 1024;

/// One message as it arrived.
#[derive(Debug, PartialEq)]
pub enum Incoming {
    Message(Value),
    /// Text that is not a JSON-RPC message, with why.
    Malformed(String),
}

/// The next message, or `None` at the end of the input.
pub fn read(input: &mut impl BufRead, framing: Framing) -> io::Result<Option<Incoming>> {
    let body = match framing {
        Framing::Headers => {
            let mut length = None;
            loop {
                let mut line = String::new();
                if input.by_ref().take(LIMIT as u64).read_line(&mut line)? == 0 {
                    return Ok(None);
                }
                let line = line.trim_end();
                if line.is_empty() {
                    break;
                }
                if let Some((name, value)) = line.split_once(':')
                    && name.trim().eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse::<usize>().ok();
                }
            }
            let Some(length) = length.filter(|&n| n <= LIMIT) else {
                return Ok(Some(Incoming::Malformed("a message needs a Content-Length of at most 64 MiB".into())));
            };
            let mut body = vec![0; length];
            input.read_exact(&mut body)?;
            body
        }
        Framing::Lines => loop {
            let mut line = Vec::new();
            if input.by_ref().take(LIMIT as u64).read_until(b'\n', &mut line)? == 0 {
                return Ok(None);
            }
            if line.last() != Some(&b'\n') && line.len() >= LIMIT {
                input.skip_until(b'\n')?;
                return Ok(Some(Incoming::Malformed("a message may be at most 64 MiB".into())));
            }
            if !line.iter().all(u8::is_ascii_whitespace) {
                break line;
            }
        },
    };
    Ok(Some(match serde_json::from_slice::<Value>(&body) {
        Ok(message) if message.is_object() => Incoming::Message(message),
        Ok(_) => Incoming::Malformed("a message is a JSON object".into()),
        Err(e) => Incoming::Malformed(format!("not JSON: {e}")),
    }))
}

/// Write one message and flush it, so the client sees it at once.
pub fn write(out: &mut impl Write, framing: Framing, message: &Value) -> io::Result<()> {
    // serde_json escapes line breaks inside strings, so the text holds no raw newline.
    let text = message.to_string();
    match framing {
        Framing::Headers => write!(out, "Content-Length: {}\r\n\r\n{text}", text.len())?,
        Framing::Lines => writeln!(out, "{text}")?,
    }
    out.flush()
}

pub fn response(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

pub fn error(id: &Value, code: i64, message: impl Into<String>) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message.into()}})
}

pub fn notification(method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "method": method, "params": params})
}

/// What a message asks, by its shape: a request has an id and a method, a notification only a
/// method, and anything else is a response to the server or not a message at all.
pub enum Kind<'a> {
    Request { id: &'a Value, method: &'a str, params: &'a Value },
    Notification { method: &'a str, params: &'a Value },
    Other,
}

pub fn kind(message: &Value) -> Kind<'_> {
    static NULL: Value = Value::Null;
    let params = message.get("params").unwrap_or(&NULL);
    match (message.get("id"), message.get("method").and_then(Value::as_str)) {
        (Some(id), Some(method)) => Kind::Request { id, method, params },
        (None, Some(method)) => Kind::Notification { method, params },
        _ => Kind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(input: &str, framing: Framing) -> Vec<Incoming> {
        let mut reader = io::BufReader::new(input.as_bytes());
        std::iter::from_fn(|| read(&mut reader, framing).unwrap()).collect()
    }

    #[test]
    fn headers_frame_each_message_by_its_length() {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"é"}"#;
        let input =
            format!("Content-Length: {}\r\nContent-Type: x\r\n\r\n{body}Content-Length: 2\r\n\r\n{{}}", body.len());
        let found = all(&input, Framing::Headers);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0], Incoming::Message(json!({"jsonrpc": "2.0", "id": 1, "method": "é"})));
        assert_eq!(found[1], Incoming::Message(json!({})));
    }

    #[test]
    fn a_header_without_a_usable_length_is_malformed() {
        assert!(matches!(all("Content-Length: lots\r\n\r\n", Framing::Headers)[0], Incoming::Malformed(_)));
        let huge = format!("Content-Length: {}\r\n\r\n", LIMIT + 1);
        assert!(matches!(all(&huge, Framing::Headers)[0], Incoming::Malformed(_)));
    }

    #[test]
    fn lines_frame_one_message_each_and_skip_blank_ones() {
        let found = all("{\"id\":1}\n\n  \n{\"id\":2}\nnot json\n[1]\n", Framing::Lines);
        assert_eq!(found[0], Incoming::Message(json!({"id": 1})));
        assert_eq!(found[1], Incoming::Message(json!({"id": 2})));
        assert!(matches!(found[2], Incoming::Malformed(_)));
        assert!(matches!(found[3], Incoming::Malformed(_)), "an array is not a message");
        assert_eq!(found.len(), 4);
    }

    #[test]
    fn written_messages_read_back() {
        let message = json!({"jsonrpc": "2.0", "id": 7, "result": "two\nlines"});
        for framing in [Framing::Headers, Framing::Lines] {
            let mut out = Vec::new();
            write(&mut out, framing, &message).unwrap();
            let text = String::from_utf8(out).unwrap();
            assert_eq!(all(&text, framing), vec![Incoming::Message(message.clone())]);
            if framing == Framing::Lines {
                assert_eq!(text.matches('\n').count(), 1, "one line per message");
            }
        }
    }

    #[test]
    fn kind_tells_requests_from_notifications() {
        assert!(matches!(kind(&json!({"id": 1, "method": "m"})), Kind::Request { method: "m", .. }));
        assert!(matches!(kind(&json!({"method": "n"})), Kind::Notification { method: "n", .. }));
        assert!(matches!(kind(&json!({"id": 1, "result": 2})), Kind::Other));
    }
}
