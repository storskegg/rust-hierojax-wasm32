//! The reference test of consistent mode, natively: `cargo test -p hieroxide-outline --test golden`.
//! No browser and no Node. `tests/golden/README.md` says what the files are.
//!
//! To accept a change of output (a versioned, changelogged change, see the README):
//! `HIEROXIDE_GOLDEN_UPDATE=1 cargo test -p hieroxide-outline --test golden`

use std::io::{Read, Write};

use hieroxide_core::layout::{self, Engine, LayoutError, Mode, RawOptions, Rendered};
use hieroxide_outline::{determinism_digest, OutlineRaster};

#[path = "common.rs"]
mod common;
use common::{root, Fragment};

/// FNV-1a, 64 bits; `tests/golden/engines.mjs` has the same.
fn fnv64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// What is compared for one fragment: the status and all the output, one field a line. The same
/// lines are built from the module's getters in `engines.mjs`.
fn record(r: Result<Rendered, LayoutError>, parse_failed: bool) -> String {
    if parse_failed {
        return "parse".to_string();
    }
    match r {
        Err(_) => "failed".to_string(),
        Ok(r) => {
            let c = r.container;
            format!(
                "ok\n{}\n{}\n{}\n{}\n{}\n{}",
                c.class_name.unwrap_or(""),
                c.style_width.unwrap_or_default(),
                c.style_height.unwrap_or_default(),
                c.add_border,
                r.svg.logs.join("\u{1}"),
                r.svg.markup
            )
        }
    }
}

struct Run<'f> {
    engine: Engine,
    raster: OutlineRaster<'f>,
}

impl<'f> Run<'f> {
    fn new(font: &'f [u8]) -> Run<'f> {
        Run { engine: Engine::with_mode(Mode::Consistent), raster: OutlineRaster::new(font).expect("font").lenient() }
    }
    fn fragment(&mut self, f: &Fragment) -> String {
        // The font size and colour of the page are not given: they must not matter.
        let raw = RawOptions::from_parts(f64::NAN, "", &f.attrs);
        match hieroxide_core::parse(&f.text) {
            Err(_) => record(Err(LayoutError::Internal("parse")), true),
            Ok(tree) => record(layout::render_consistent(&mut self.engine, &tree, &raw, &mut self.raster), false),
        }
    }
}

fn read_gz(name: &str) -> String {
    let raw = std::fs::read(root().join("tests/golden").join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
    let mut s = String::new();
    flate2::read::GzDecoder::new(&raw[..]).read_to_string(&mut s).expect("gunzip");
    s
}

fn write_gz(name: &str, text: &str) {
    let f = std::fs::File::create(root().join("tests/golden").join(name)).expect("create");
    let mut e = flate2::GzBuilder::new().mtime(0).write(f, flate2::Compression::best());
    e.write_all(text.as_bytes()).expect("write");
    e.finish().expect("finish");
}

/// The fragments kept in full: every 40th of each class, and all of the site's lines.
fn in_sample(f: &Fragment, index_in_class: usize) -> bool {
    f.group == "site" || index_in_class % 40 == 0
}

fn hash_line(h: u64) -> String {
    format!("{h:016x}")
}

fn updating() -> bool {
    std::env::var_os("HIEROXIDE_GOLDEN_UPDATE").is_some()
}

#[test]
fn reference_hashes_and_sample() {
    let corpus = common::load_corpus(None);
    let font = common::font();
    let mut run = Run::new(&font);
    let mut hashes = String::new();
    let mut sample = String::new();
    let mut seen: std::collections::HashMap<&str, usize> = Default::default();
    let mut recs = Vec::with_capacity(corpus.len());
    for f in &corpus {
        let rec = run.fragment(f);
        hashes.push_str(&hash_line(fnv64(rec.as_bytes())));
        hashes.push('\n');
        let n = seen.entry(f.class.as_str()).or_insert(0);
        if in_sample(f, *n) {
            sample.push_str(&f.id);
            sample.push('\t');
            sample.push_str(&common::esc(&rec));
            sample.push('\n');
        }
        *n += 1;
        recs.push(rec);
    }
    let digest = format!("{:016x}", determinism_digest());
    let summary = format!("digest {digest}\nfragments {}\nall {:016x}\n", corpus.len(), fnv64(hashes.as_bytes()));
    if updating() {
        write_gz("hashes.txt.gz", &hashes);
        write_gz("sample.tsv.gz", &sample);
        std::fs::write(root().join("tests/golden/manifest.txt"), &summary).expect("write");
        return;
    }
    let want = std::fs::read_to_string(root().join("tests/golden/manifest.txt")).expect("manifest.txt");
    assert_eq!(want.lines().next(), summary.lines().next(), "pow, log, sin or cos changed");
    let want_hashes = read_gz("hashes.txt.gz");
    let mut bad = 0;
    for (i, (a, b)) in want_hashes.lines().zip(hashes.lines()).enumerate() {
        if a != b {
            bad += 1;
            if bad <= 5 {
                eprintln!("{}: reference {a}, now {b}\n  {}", corpus[i].id, recs[i].chars().take(400).collect::<String>());
            }
        }
    }
    assert_eq!(bad, 0, "{bad} fragments differ from the reference");
    assert_eq!(want, summary);
    assert_eq!(read_gz("sample.tsv.gz"), sample, "the sample differs");
}

/// In a shuffled order, with one engine throughout, every fragment gives the same bytes.
#[test]
fn shuffled_is_identical() {
    let corpus = common::load_corpus(None);
    let font = common::font();
    let mut run = Run::new(&font);
    let in_order: Vec<String> = corpus.iter().map(|f| run.fragment(f)).collect();
    // A fixed permutation (the same generator as `shuffled` in tests/parity/lib/runner.mjs is not
    // needed: any order will do).
    let mut idx: Vec<usize> = (0..corpus.len()).collect();
    let mut s: u64 = 7;
    for i in (1..idx.len()).rev() {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        idx.swap(i, (s >> 33) as usize % (i + 1));
    }
    for seedless in [true, false] {
        // Fresh state; and in the second pass fresh state per 200 fragments.
        let mut run = Run::new(&font);
        for (n, &i) in idx.iter().enumerate() {
            if !seedless && n % 200 == 0 {
                run = Run::new(&font);
            }
            assert_eq!(in_order[i], run.fragment(&corpus[i]), "{} differs when shuffled", corpus[i].id);
        }
    }
}

/// The markup does not depend on the page's font size or colour, or on `data-type`.
#[test]
fn independent_of_the_page() {
    let font = common::font();
    let tree = hieroxide_core::parse("\u{13000}\u{13430}\u{13001}").expect("parse");
    let mut outs = Vec::new();
    for (px, color, kind) in [(22.0, "red", ""), (40.0, "blue", "canvas"), (33.6, "", "dom"), (f64::NAN, "", "svg")] {
        let mut run = Run::new(&font);
        let mut raw = RawOptions::from_parts(px, color, &[]);
        raw.type_ = kind.to_string();
        let r = layout::render_consistent(&mut run.engine, &tree, &raw, &mut run.raster).expect("render");
        outs.push(r.svg.markup);
    }
    assert!(outs.windows(2).all(|w| w[0] == w[1]));
    assert!(outs[0].contains("em\" viewBox=\"0 0 "));
    // A mode that is not consistent is refused, not silently px-rounded.
    let mut compat = Engine::new();
    let mut run = Run::new(&font);
    let raw = RawOptions::from_parts(22.0, "", &[]);
    assert!(layout::render_consistent(&mut compat, &tree, &raw, &mut run.raster).is_err());
}

/// Upstream's accidents, as consistent mode has them: each is in `docs` of the mode.
#[test]
fn upstream_faults_are_gone() {
    let font = common::font();
    let mut run = Run::new(&font);
    let raw = RawOptions::from_parts(22.0, "", &[]);
    // An insertion into an empty enclosure: upstream throws IndexSizeError from getImageData.
    let empty = "\u{13258}\u{1343C}\u{13259}\u{13434}\u{13000}";
    if let Ok(tree) = hieroxide_core::parse(empty) {
        let r = layout::render_consistent(&mut run.engine, &tree, &raw, &mut run.raster);
        assert!(r.is_ok(), "{r:?}");
    }
    // Lost signs keep their variation selector in the text (upstream writes U+FE00 for any).
    let tree = hieroxide_core::parse("\u{13443}\u{FE03}").expect("parse");
    assert_eq!(tree.to_string_exact(), "\u{13443}\u{FE03}");
    assert_eq!(tree.to_string_value(), "\u{13443}\u{FE00}");
    let r = layout::render_consistent(&mut run.engine, &tree, &raw, &mut run.raster).expect("render");
    assert!(r.svg.markup.contains('\u{FE03}'), "{}", r.svg.markup);
    // Out-of-range options fall back to their defaults.
    let tree = hieroxide_core::parse("\u{13000}\u{13258}\u{13000}\u{13259}").expect("parse");
    let mut odd = raw.clone();
    odd.sep = "-0.5".into();
    odd.linesize = "Infinity".into();
    odd.dir = "sideways".into();
    let a = layout::render_consistent(&mut run.engine, &tree, &odd, &mut run.raster).expect("render");
    let b = layout::render_consistent(&mut run.engine, &tree, &raw, &mut run.raster).expect("render");
    assert_eq!(a.svg.markup, b.svg.markup);
}
