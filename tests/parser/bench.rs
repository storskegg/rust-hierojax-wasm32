//! Native parse throughput over the fragments extracted from docs/ (out/docs.txt, made
//! by gen.mjs). Parse only: the input is decoded to UTF-16 beforehand, and the tree is
//! dropped inside the timing, as a caller would.
//!
//! Usage: node tests/parser/gen.mjs && cargo run --release -p hieroxide-core --example parser_bench

#[path = "common.rs"]
mod common;

use std::hint::black_box;
use std::time::Instant;

use hieroxide_core::parse_utf16;

fn best_of(passes: usize, mut f: impl FnMut()) -> f64 {
    let mut best = f64::MAX;
    for _ in 0..passes {
        let t = Instant::now();
        f();
        best = best.min(t.elapsed().as_secs_f64());
    }
    best
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "tests/parser/out/docs.txt".to_string());
    let text = std::fs::read_to_string(&path).expect("run gen.mjs first");
    let frags: Vec<Vec<u16>> = text.lines().map(common::unhex).collect();
    let units: usize = frags.iter().map(Vec::len).sum();
    let utf8: usize = frags
        .iter()
        .map(|f| String::from_utf16_lossy(f).len())
        .sum();
    let ok = frags.iter().filter(|f| parse_utf16(f).is_ok()).count();

    let t = best_of(200, || {
        for f in &frags {
            black_box(parse_utf16(black_box(f)).is_ok());
        }
    });
    println!("fragments {} ({} parse ok), {} UTF-16 units, {} UTF-8 bytes", frags.len(), ok, units, utf8);
    println!(
        "per fragment: {:.2} us; {:.1} MB/s of UTF-8 ({:.1} MB/s of UTF-16); {:.1} M units/s; one pass {:.3} ms",
        t * 1e6 / frags.len() as f64,
        utf8 as f64 / t / 1e6,
        units as f64 * 2.0 / t / 1e6,
        units as f64 / t / 1e6,
        t * 1e3
    );

    // The same text as one long fragment (every fragment that parses, concatenated).
    let all: Vec<u16> = frags.iter().filter(|f| parse_utf16(f).is_ok()).flatten().copied().collect();
    let t1 = best_of(100, || {
        black_box(parse_utf16(black_box(&all)).is_ok());
    });
    println!(
        "one fragment of {} units: {:.2} ms, {:.1} MB/s of UTF-16",
        all.len(),
        t1 * 1e3,
        all.len() as f64 * 2.0 / t1 / 1e6
    );
}
