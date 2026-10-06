//! lotml's syntax: the lexer, the syntax tree and the tolerant recursive-descent parser.

pub mod ast;
pub mod lexer;
pub mod parser;
pub mod span;
pub mod strings;

pub use parser::{Parsed, SyntaxError, parse, parse_interface};
