//! What the native reference test and the `fragments` example share.
#![allow(dead_code)]

use std::io::Read;
use std::path::{Path, PathBuf};

pub struct Fragment {
    pub id: String,
    pub group: String,
    pub class: String,
    pub font_px: f64,
    pub color: String,
    pub attrs: Vec<String>,
    pub text: String,
}

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The unmodified font, from upstream's `docs/`.
pub fn font() -> Vec<u8> {
    std::fs::read(root().join("docs/NewGardiner.otf")).expect("docs/NewGardiner.otf")
}

fn unesc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(o) => out.push(o),
            None => {}
        }
    }
    out
}

pub fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n").replace('\r', "\\r")
}

pub fn load_corpus(path: Option<&str>) -> Vec<Fragment> {
    let p = path.map_or_else(|| root().join("tests/golden/corpus.tsv.gz"), PathBuf::from);
    let raw = std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    let mut text = String::new();
    flate2::read::GzDecoder::new(&raw[..]).read_to_string(&mut text).expect("gunzip");
    let mut out = Vec::new();
    for line in text.lines() {
        let c: Vec<&str> = line.split('\t').collect();
        assert_eq!(c.len(), 19, "{line}");
        out.push(Fragment {
            id: c[0].to_string(),
            group: c[1].to_string(),
            class: c[2].to_string(),
            font_px: c[3].parse().expect("font size"),
            color: c[4].to_string(),
            attrs: c[5..18].iter().map(|s| unesc(s)).collect(),
            text: unesc(c[18]),
        });
    }
    out
}
