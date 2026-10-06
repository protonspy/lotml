//! Print the canonical form of the files named.

fn main() {
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path).expect("a readable file");
        match lotml_fmt::format(&text) {
            Ok(formatted) => print!("{formatted}"),
            Err(errors) => eprintln!("{path}: {} syntax errors", errors.len()),
        }
    }
}
