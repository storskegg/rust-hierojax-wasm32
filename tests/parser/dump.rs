//! Rust side of the differential test: reads the input file (see oracle.mjs for the
//! format), parses every line and writes the same output format as the oracle.
//!
//! Usage: cargo run --release -p hieroxide-core --example parser_dump -- INPUT OUTPUT

#[path = "common.rs"]
mod common;

use std::io::Write as _;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input = std::fs::read_to_string(&args[1]).expect("read input");
    let mut lines: Vec<&str> = input.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    let mut out = String::new();
    for l in lines {
        common::run_line(l, &mut out);
        out.push('\n');
    }
    std::fs::File::create(&args[2]).and_then(|mut f| f.write_all(out.as_bytes())).expect("write output");
}
