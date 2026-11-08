//! Plain `cargo test` check of the parser against a sample of the differential test.
//!
//! `fixtures.txt` has one case per line: the input (UTF-16 code units as 4 hex digits
//! each, concatenated), a tab, and what the jison parser gave (see oracle.mjs for the
//! format). It is written by `mkfixtures.mjs` from a run of the full differential test,
//! so this needs no node.

#[path = "common.rs"]
mod common;

use hieroxide_core::{parse, parse_utf16, ErrorKind, MAX_DEPTH};

#[test]
fn sample_of_differential_cases() {
    let fixtures = include_str!("fixtures.txt");
    let (mut n, mut accepted) = (0, 0);
    for (i, line) in fixtures.lines().enumerate() {
        let (input, expected) = line.split_once('\t').unwrap_or((line, ""));
        let mut got = String::new();
        common::run_line(input, &mut got);
        assert_eq!(got, expected, "fixture line {} (input {})", i + 1, input);
        n += 1;
        accepted += usize::from(expected.starts_with('A'));
    }
    assert!(n > 500, "only {n} fixtures");
    assert!(accepted > 200 && n - accepted > 100, "unbalanced fixtures: {accepted} of {n} accepted");
}

fn enclosures(n: usize) -> String {
    format!("{}\u{13000}{}", "\u{1343C}".repeat(n), "\u{1343D}".repeat(n))
}

#[test]
fn deep_nesting_does_not_overflow_the_stack() {
    // At the limit: parses, drops and prints. Run on a thread with a small stack, as
    // wasm has (its default is 1 MiB; this is a quarter of that in a release build; an
    // unoptimized build has much larger frames, so it gets more). PARSER_TEST_STACK_KB
    // overrides this, to measure.
    let kb = std::env::var("PARSER_TEST_STACK_KB")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(if cfg!(debug_assertions) { 4096 } else { 256 });
    std::thread::Builder::new()
        .stack_size(kb * 1024)
        .spawn(|| {
            let s = enclosures(MAX_DEPTH as usize);
            let t = parse(&s).expect("at the limit");
            assert_eq!(t.to_string_value(), s);
            drop(t);
            // Beyond the limit: an error value, not a crash.
            for n in [MAX_DEPTH as usize + 1, 10_000, 1_000_000] {
                let e = parse(&enclosures(n)).unwrap_err();
                assert_eq!(e.kind, ErrorKind::TooDeep);
            }
            // The same through nested segments in insertions.
            let nested = |n: u32| {
                let mut s = String::from("\u{13000}\u{13436}\u{13000}");
                for _ in 0..n {
                    s = format!("\u{13437}\u{13000}\u{13432}{s}\u{13438}");
                }
                format!("\u{13000}\u{13432}{s}")
            };
            let ok = nested(MAX_DEPTH);
            assert_eq!(parse(&ok).unwrap().to_string_value(), ok);
            assert_eq!(parse(&nested(MAX_DEPTH + 1)).unwrap_err().kind, ErrorKind::TooDeep);
        })
        .expect("spawn")
        .join()
        .expect("no overflow");
}

#[test]
fn long_flat_input_is_linear() {
    // 400,000 groups in a row, and one horizontal group of 400,000 elements.
    let a = "\u{13000}";
    let flat = a.repeat(400_000);
    assert_eq!(parse(&flat).unwrap().to_string_value(), flat);
    let hor = vec![a; 400_000].join("\u{13431}");
    assert_eq!(parse(&hor).unwrap().to_string_value(), hor);
    let ver = vec![a; 400_000].join("\u{13430}");
    assert_eq!(parse(&ver).unwrap().to_string_value(), ver);
}

#[test]
fn lone_surrogates_are_rejected_but_replacement_char_is_a_sign() {
    assert!(parse_utf16(&[0xD80C]).is_err());
    assert!(parse_utf16(&[0xDC00]).is_err());
    assert!(parse_utf16(&[0xD80C, 0xD80C, 0xDC00]).is_err());
    assert!(parse("\u{FFFD}").is_ok());
}
