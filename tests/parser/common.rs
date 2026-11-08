//! Shared by the dump example and the fixtures test: one input line in, one output line
//! out, in the format that oracle.mjs documents.

#![allow(dead_code)] // each user of this file needs a different part of it

use std::fmt::Write as _;

use hieroxide_core::parse_utf16;

/// Decode a line of concatenated 4-digit hex code units.
pub fn unhex(line: &str) -> Vec<u16> {
    line.as_bytes()
        .chunks(4)
        .map(|c| u16::from_str_radix(std::str::from_utf8(c).unwrap_or("0"), 16).unwrap_or(0))
        .collect()
}

/// One output line (without the newline): `R`, or `A <TAB> toString <TAB> dump`.
pub fn run_line(line: &str, out: &mut String) {
    match parse_utf16(&unhex(line)) {
        Err(_) => out.push('R'),
        Ok(tree) => {
            out.push_str("A\t");
            for u in tree.to_string_value().encode_utf16() {
                let _ = write!(out, "{u:04x}");
            }
            out.push('\t');
            tree.dump(out);
        }
    }
}
