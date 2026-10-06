//! Read `{"id", "code"}` lines and print `{"id", "module"}` with the compiled stub, or
//! `{"id", "errors"}` with the codes that stopped it.

use std::io::{BufRead, Write};
use std::path::Path;

fn main() {
    let mut out = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let line = line.expect("a line");
        let value: serde_json::Value = serde_json::from_str(&line).expect("a JSON line");
        let code = value["code"].as_str().unwrap_or("");
        let row = match lotml_py::compile(code, Path::new("program.lotml")) {
            Ok(module) => serde_json::json!({"id": value["id"], "module": module}),
            Err(errors) => {
                let codes: Vec<&str> = errors.iter().map(|d| d.code).collect();
                serde_json::json!({"id": value["id"], "errors": codes})
            }
        };
        writeln!(out, "{row}").expect("stdout");
    }
}
