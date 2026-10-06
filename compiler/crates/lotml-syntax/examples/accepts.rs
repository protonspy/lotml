//! Read file paths on stdin; print `ok` or the first error for each.
use std::io::BufRead;

fn main() {
    for line in std::io::stdin().lock().lines() {
        let path = line.unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let parsed = lotml_syntax::parse(&text);
        match parsed.errors.first() {
            None => println!("ok\t{path}"),
            Some(e) => {
                let (l, c) = lotml_syntax::span::line_column(&text, e.span.start);
                println!("error\t{path}\t{l}:{c} {}", e.message);
            }
        }
    }
}
