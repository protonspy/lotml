//! One bounded HTTP exchange with the guide server: a POST to its OpenAI-compatible chat
//! endpoint on a loopback address, under the call's one deadline, the response's headers and body
//! capped, a chunked body decoded within the same cap, and any status but 200 a silence
//! (specs/guide-tool/ R2.4-R2.6).

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::Endpoint;

/// Bytes of rendered messages sent at most; a state larger than this is not sent.
pub const PROMPT_LIMIT: usize = 256 * 1024;
/// The response's headers, in bytes and in lines.
pub const HEADER_BYTES: usize = 16 * 1024;
pub const HEADER_LINES: usize = 64;
/// The response's body, whether its length is given or it comes in chunks.
pub const BODY_LIMIT: usize = 1024 * 1024;
/// Bytes of a refusal's body read to tell a prompt too long from another failure.
const REFUSAL_LIMIT: usize = 64 * 1024;

/// Why the guide server gave no answer the tool can use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Silence {
    /// The prompt is longer than the server's context, or than the tool sends.
    TooLong,
    /// No connection, a status other than 200, or a response past its caps or not JSON.
    ServerError,
    /// The call's deadline passed.
    Deadline,
}

impl Silence {
    pub fn reason(self) -> &'static str {
        match self {
            Silence::TooLong => "too-long",
            Silence::ServerError => "server-error",
            Silence::Deadline => "deadline",
        }
    }
}

/// The JSON schema the guide's answer is held to: one to three ranked locations, the kind of
/// change, and at most one edit as an edit tool's call. Text rather than a [`Value`]: serde_json
/// sorts an object's keys, and a runtime that compiles the schema to a grammar — llama.cpp's does —
/// writes the properties in the order they come, so this order is the order the guide was trained
/// to answer in: `locations` before `kind` before `edit`, `path` before `symbol` before `lines`.
const ANSWER_SCHEMA: &str = r#"{
    "type": "object",
    "additionalProperties": false,
    "required": ["locations", "kind", "edit"],
    "properties": {
        "locations": {
            "type": "array",
            "minItems": 1,
            "maxItems": 3,
            "items": {
                "type": "object",
                "additionalProperties": false,
                "required": ["path", "symbol", "lines"],
                "properties": {
                    "path": {"type": "string", "maxLength": 512},
                    "symbol": {"type": ["string", "null"], "maxLength": 256},
                    "lines": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "integer", "minimum": 1}}
                }
            }
        },
        "kind": {"enum": ["arm", "body", "definition", "add", "remove", "lines", "several"]},
        "edit": {"anyOf": [
            {"type": "null"},
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["tool", "arguments"],
                "properties": {
                    "tool": {"enum": ["replace", "add", "remove", "edit"]},
                    "arguments": {"type": "object"}
                }
            }
        ]}
    }
}"#;

/// The guide server's response to `messages`, asked at temperature zero within `answer` tokens,
/// held to the answer schema, with the log-probabilities of its tokens; everything before
/// `deadline`.
pub fn ask(
    endpoint: &Endpoint,
    model: &str,
    messages: &Value,
    answer: u32,
    deadline: Instant,
) -> Result<Value, Silence> {
    if messages.to_string().len() >= PROMPT_LIMIT {
        return Err(Silence::TooLong);
    }
    let body = format!(
        r#"{{"model":{},"messages":{messages},"temperature":0,"max_tokens":{answer},"response_format":{{"type":"json_schema","json_schema":{{"name":"guide","strict":true,"schema":{ANSWER_SCHEMA}}}}},"logprobs":true,"top_logprobs":1}}"#,
        Value::from(model),
    );
    let request = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nHost: {}:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nAccept-Encoding: identity\r\n\r\n{body}",
        endpoint.host,
        endpoint.port,
        body.len()
    );
    let stream = TcpStream::connect_timeout(&endpoint.address, left(deadline)?).map_err(silence)?;
    let mut timed = Timed { stream, deadline };
    timed.write_all(request.as_bytes()).map_err(silence)?;
    let mut reader = BufReader::new(timed);
    let (status, headers) = head(&mut reader)?;
    if status != 200 {
        let mut refusal = Vec::new();
        let _ = (&mut reader).take(REFUSAL_LIMIT as u64).read_to_end(&mut refusal);
        let text = String::from_utf8_lossy(&refusal).to_ascii_lowercase();
        return Err(if status == 400 && text.contains("context") { Silence::TooLong } else { Silence::ServerError });
    }
    let body = if header(&headers, "transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked")) {
        chunked(&mut reader)?
    } else if let Some(length) = header(&headers, "content-length") {
        let length: usize = length.trim().parse().map_err(|_| Silence::ServerError)?;
        if length > BODY_LIMIT {
            return Err(Silence::ServerError);
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body).map_err(silence)?;
        body
    } else {
        let mut body = Vec::new();
        (&mut reader).take(BODY_LIMIT as u64 + 1).read_to_end(&mut body).map_err(silence)?;
        if body.len() > BODY_LIMIT {
            return Err(Silence::ServerError);
        }
        body
    };
    serde_json::from_slice(&body).map_err(|_| Silence::ServerError)
}

/// A stream whose every read and write waits only for what is left of the deadline, so a server
/// sending or taking a byte at a time still stops there.
struct Timed {
    stream: TcpStream,
    deadline: Instant,
}

impl Write for Timed {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let left = self.deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "the deadline passed"));
        }
        self.stream.set_write_timeout(Some(left))?;
        self.stream.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

impl Read for Timed {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = self.deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "the deadline passed"));
        }
        self.stream.set_read_timeout(Some(left))?;
        self.stream.read(buf)
    }
}

fn left(deadline: Instant) -> Result<Duration, Silence> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() { Err(Silence::Deadline) } else { Ok(left) }
}

fn silence(error: io::Error) -> Silence {
    match error.kind() {
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => Silence::Deadline,
        _ => Silence::ServerError,
    }
}

/// One line of at most `budget` bytes, its end included; an error when it runs past the budget
/// or the stream ends first.
fn line(reader: &mut BufReader<Timed>, budget: &mut usize) -> Result<Vec<u8>, Silence> {
    let mut found = Vec::new();
    reader.take(*budget as u64).read_until(b'\n', &mut found).map_err(silence)?;
    if !found.ends_with(b"\n") {
        return Err(Silence::ServerError);
    }
    *budget -= found.len();
    Ok(found)
}

/// The status and the headers, lower-cased by name, held to [`HEADER_BYTES`] and
/// [`HEADER_LINES`].
fn head(reader: &mut BufReader<Timed>) -> Result<(u16, Vec<(String, String)>), Silence> {
    let mut budget = HEADER_BYTES;
    let status_line = line(reader, &mut budget)?;
    let status_line = String::from_utf8_lossy(&status_line);
    let mut parts = status_line.split_whitespace();
    let status = match (parts.next(), parts.next()) {
        (Some(version), Some(code)) if version.starts_with("HTTP/1.") => {
            code.parse().map_err(|_| Silence::ServerError)?
        }
        _ => return Err(Silence::ServerError),
    };
    let mut headers = Vec::new();
    loop {
        let raw = line(reader, &mut budget)?;
        let text = String::from_utf8_lossy(&raw);
        let text = text.trim_end_matches(['\r', '\n']);
        if text.is_empty() {
            return Ok((status, headers));
        }
        if headers.len() == HEADER_LINES {
            return Err(Silence::ServerError);
        }
        let (name, value) = text.split_once(':').ok_or(Silence::ServerError)?;
        headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
    }
}

fn header<'h>(headers: &'h [(String, String)], name: &str) -> Option<&'h str> {
    headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
}

/// A chunked body, decoded within [`BODY_LIMIT`] in all.
fn chunked(reader: &mut BufReader<Timed>) -> Result<Vec<u8>, Silence> {
    let mut body = Vec::new();
    loop {
        let mut budget = 256;
        let size_line = line(reader, &mut budget)?;
        let size_text = String::from_utf8_lossy(&size_line);
        let size_text = size_text.trim().split(';').next().unwrap_or("");
        let size = usize::from_str_radix(size_text, 16).map_err(|_| Silence::ServerError)?;
        if size == 0 {
            return Ok(body);
        }
        if size > BODY_LIMIT - body.len() {
            return Err(Silence::ServerError);
        }
        let start = body.len();
        body.resize(start + size, 0);
        reader.read_exact(&mut body[start..]).map_err(silence)?;
        let mut end = [0; 2];
        reader.read_exact(&mut end).map_err(silence)?;
        if &end != b"\r\n" {
            return Err(Silence::ServerError);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{SocketAddr, TcpListener};
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    use serde_json::{Value, json};

    use super::*;
    use crate::guide::Endpoint;

    /// A server that accepts one connection, sends what it read back on the channel, and answers
    /// with `respond`, which writes to the stream however the test wants.
    fn serve(respond: impl FnOnce(&mut std::net::TcpStream) + Send + 'static) -> (Endpoint, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address: SocketAddr = listener.local_addr().unwrap();
        let (sent, received) = mpsc::channel();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut head = String::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if let Some(n) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = n.trim().parse().unwrap();
                }
                head += &line;
                if line == "\r\n" {
                    break;
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let _ = sent.send(head + &String::from_utf8(body).unwrap());
            respond(&mut stream);
        });
        (Endpoint { host: "127.0.0.1".into(), port: address.port(), address }, received)
    }

    fn answer_schema() -> Value {
        serde_json::from_str(ANSWER_SCHEMA).expect("the answer schema is JSON")
    }

    fn messages() -> Value {
        json!([{"role": "system", "content": "s"}, {"role": "user", "content": "u"}])
    }

    fn soon() -> Instant {
        Instant::now() + Duration::from_secs(10)
    }

    const ANSWER: &str = r#"{"choices":[{"message":{"content":"{}"}}]}"#;

    #[test]
    fn the_request_is_one_post_with_a_schema_temperature_zero_and_log_probabilities() {
        let (endpoint, received) = serve(|stream| {
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{ANSWER}", ANSWER.len()).unwrap();
        });
        let answer = ask(&endpoint, "lotml-guide", &messages(), 512, soon()).unwrap();
        assert_eq!(answer["choices"][0]["message"]["content"], "{}");
        let request = received.recv().unwrap();
        let (head, body) = request.split_once("\r\n\r\n").unwrap();
        assert!(head.starts_with("POST /v1/chat/completions HTTP/1.1\r\n"), "{head}");
        for header in
            ["Host: 127.0.0.1:", "Connection: close", "Accept-Encoding: identity", "Content-Type: application/json"]
        {
            assert!(head.contains(header), "{header} missing from {head}");
        }
        let text = body;
        let body: Value = serde_json::from_str(body).unwrap();
        assert_eq!(body["model"], "lotml-guide");
        assert_eq!(body["temperature"], 0);
        assert_eq!(body["max_tokens"], 512);
        assert_eq!(body["logprobs"], true);
        assert_eq!(body["top_logprobs"], 1, "one alternative per token keeps a long answer under the body's cap");
        assert_eq!(body["messages"], messages());
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(body["response_format"]["json_schema"]["schema"], answer_schema());
        let schema = &text[text.find(r#""schema":"#).unwrap()..];
        let at = |key: &str| schema.find(&format!(r#""{key}":"#)).unwrap_or_else(|| panic!("{key} missing"));
        assert!(at("locations") < at("kind") && at("kind") < at("edit"), "the answer's keys in the order trained");
        assert!(at("path") < at("symbol") && at("symbol") < at("lines"), "a location's keys in the order trained");
        assert!(at("tool") < at("arguments"), "an edit's keys in the order trained");
    }

    #[test]
    fn a_chunked_body_is_decoded() {
        let (endpoint, _) = serve(|stream| {
            let (a, b) = ANSWER.split_at(10);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{a}\r\n{:x}\r\n{b}\r\n0\r\n\r\n",
                a.len(),
                b.len()
            )
            .unwrap();
        });
        let answer = ask(&endpoint, "m", &messages(), 64, soon()).unwrap();
        assert_eq!(answer["choices"][0]["message"]["content"], "{}");
    }

    #[test]
    fn any_status_but_200_is_a_silence_and_no_redirect_is_followed() {
        for status in ["500 Internal Server Error", "302 Found\r\nLocation: http://example.com/", "404 Not Found"] {
            let (endpoint, _) = serve(move |stream| {
                write!(stream, "HTTP/1.1 {status}\r\nContent-Length: 2\r\n\r\n{{}}").unwrap();
            });
            assert_eq!(ask(&endpoint, "m", &messages(), 64, soon()).unwrap_err(), Silence::ServerError);
        }
    }

    #[test]
    fn a_prompt_longer_than_the_context_is_too_long() {
        let (endpoint, _) = serve(|stream| {
            let body = r#"{"error":{"code":400,"message":"the request exceeds the available context size","type":"exceed_context_size_error"}}"#;
            write!(stream, "HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        });
        assert_eq!(ask(&endpoint, "m", &messages(), 64, soon()).unwrap_err(), Silence::TooLong);
        let huge = json!([{"role": "user", "content": "x".repeat(PROMPT_LIMIT)}]);
        let (endpoint, _) = serve(|_| {});
        assert_eq!(ask(&endpoint, "m", &huge, 64, soon()).unwrap_err(), Silence::TooLong);
    }

    #[test]
    fn a_body_or_headers_past_their_caps_are_refused() {
        let (endpoint, _) = serve(|stream| {
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", BODY_LIMIT + 1).unwrap();
            let _ = stream.write_all(&vec![b' '; BODY_LIMIT + 1]);
        });
        assert_eq!(ask(&endpoint, "m", &messages(), 64, soon()).unwrap_err(), Silence::ServerError);
        let (endpoint, _) = serve(|stream| {
            write!(stream, "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n").unwrap();
            for _ in 0..=BODY_LIMIT / 1000 {
                if write!(stream, "3e8\r\n{}\r\n", " ".repeat(1000)).is_err() {
                    break;
                }
            }
        });
        assert_eq!(ask(&endpoint, "m", &messages(), 64, soon()).unwrap_err(), Silence::ServerError);
        let (endpoint, _) = serve(|stream| {
            write!(stream, "HTTP/1.1 200 OK\r\n").unwrap();
            for i in 0..=HEADER_LINES {
                if write!(stream, "X-Filler-{i}: x\r\n").is_err() {
                    break;
                }
            }
        });
        assert_eq!(ask(&endpoint, "m", &messages(), 64, soon()).unwrap_err(), Silence::ServerError);
        let (endpoint, _) = serve(|stream| {
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nX-Long: {}\r\n", "x".repeat(HEADER_BYTES));
        });
        assert_eq!(ask(&endpoint, "m", &messages(), 64, soon()).unwrap_err(), Silence::ServerError);
    }

    #[test]
    fn a_chunk_size_that_would_overflow_the_cap_is_refused_not_a_crash() {
        let (endpoint, _) = serve(|stream| {
            let _ =
                write!(stream, "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n1\r\nA\r\nffffffffffffffff\r\n");
        });
        assert_eq!(ask(&endpoint, "m", &messages(), 64, soon()).unwrap_err(), Silence::ServerError);
    }

    #[test]
    fn a_server_that_never_reads_the_request_stops_at_the_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_secs(10));
        });
        let endpoint = Endpoint { host: "127.0.0.1".into(), port: address.port(), address };
        let large = json!([{"role": "user", "content": "x".repeat(PROMPT_LIMIT - 1000)}]);
        let started = Instant::now();
        let answer = ask(&endpoint, "m", &large, 64, started + Duration::from_millis(800));
        assert_eq!(answer.unwrap_err(), Silence::Deadline);
        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
    }

    #[test]
    fn a_server_sending_a_byte_at_a_time_stops_at_the_deadline() {
        let (endpoint, _) = serve(|stream| {
            for byte in b"HTTP/1.1 200 OK\r\nContent-Length: 100000\r\n\r\n".iter().cycle().take(10_000) {
                if stream.write_all(&[*byte]).is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
        });
        let started = Instant::now();
        let deadline = started + Duration::from_millis(800);
        assert_eq!(ask(&endpoint, "m", &messages(), 64, deadline).unwrap_err(), Silence::Deadline);
        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
    }

    #[test]
    fn nothing_listening_is_a_server_error() {
        let free = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = free.local_addr().unwrap();
        drop(free);
        let endpoint = Endpoint { host: "127.0.0.1".into(), port: address.port(), address };
        assert_eq!(ask(&endpoint, "m", &messages(), 64, soon()).unwrap_err(), Silence::ServerError);
    }
}
