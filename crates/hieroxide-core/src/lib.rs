//! Parser and layout core of Hieroxide, a Rust port of hierojax.
//!
//! This crate is based on hierojax by Mark-Jan Nederhof, which is released
//! under the GPL-3.0.

pub mod lexer;
pub mod parser;
pub mod tree;

pub use parser::{parse, parse_utf16, ErrorKind, ParseError, MAX_DEPTH};
pub use tree::{Node, NodeId, Tree};
