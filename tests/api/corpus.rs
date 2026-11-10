//! The public API over the whole corpus (the 19,756 fragments of `tests/golden`), natively:
//! `cargo test --release -p hieroxide --test corpus` (about a minute; the profile of tests is
//! optimized in this workspace).
//!
//! * The text printer through the API is still the golden bytes (`tests/golden/hashes.txt.gz`).
//! * The path block places every sign by the numbers of the text block: the same `translate`,
//!   `scale` and `rotate`, the same `x`, `y` and font size, and the box (`width`, `height`,
//!   `viewBox`) the same. The comparison reads the printed markup of both, not the data behind it.
//! * The copyable text, in each way a block carries it, is the source, character for character.
//! * Nothing in a path block has an `id`, a reference, a stylesheet, a script or a font.
//! * Output does not depend on what was laid out before.

use std::io::Read;

use hieroxide::{parse, Engine, Font, Options, Selectable, Shading, SvgKind};
use hieroxide_core::jsnum::number_from_str;

#[path = "../golden/common.rs"]
mod common;

fn fnv64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// The options a corpus fragment's `data-*` attributes stand for, in the order of
/// `DATA_ATTRIBUTES`: fontsize, signcolor, dir, linesize, sep, separated, type, bracketcolor,
/// shadepattern, align, border, standalone, log.
fn options_of(a: &[String]) -> Options {
    let num = |s: &str| if s.is_empty() { None } else { Some(number_from_str(s)) };
    let mut o = Options::default();
    o.font_size = num(&a[0]);
    o.sign_color = Some(a[1].clone()).filter(|s| !s.is_empty());
    o.dir = match a[2].as_str() {
        "hrl" => hieroxide::Direction::Hrl,
        "vlr" => hieroxide::Direction::Vlr,
        "vrl" => hieroxide::Direction::Vrl,
        _ => hieroxide::Direction::Hlr,
    };
    o.line_size = num(&a[3]);
    o.sep = num(&a[4]);
    o.separated = a[5] == "true";
    o.bracket_color = Some(a[7].clone()).filter(|s| !s.is_empty());
    o.shading = if a[8].is_empty() || a[8] == "uniform" { Shading::Uniform } else { Shading::Hatching };
    o.align = if a[9] == "bottom" { hieroxide::Align::Bottom } else { hieroxide::Align::Middle };
    o.border = a[10] == "true";
    o.standalone_font = if a[11] == "true" { Some("NewGardiner.otf".to_string()) } else { None };
    o.log = a[12] == "true";
    o
}

fn engine() -> Engine {
    Engine::new(Font::from_bytes(common::font()).expect("font"), Options::default())
}

// ---- reading markup ----

/// `(name, attributes, offset after the tag)` of each tag, in order.
fn tags(s: &str) -> Vec<(String, Vec<(String, String)>, usize, usize)> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let start = i;
        let end = s[i..].find('>').map(|e| i + e).expect("unterminated tag");
        let inner = &s[i + 1..end];
        i = end + 1;
        let closing = inner.starts_with('/');
        let inner = inner.trim_start_matches('/').trim_end_matches('/');
        let name: String = inner.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
        let mut attrs = Vec::new();
        let mut rest = &inner[name.len()..];
        while let Some(eq) = rest.find("=\"") {
            let key = rest[..eq].trim().to_string();
            let after = &rest[eq + 2..];
            let close = after.find('"').expect("unterminated attribute");
            attrs.push((key, after[..close].to_string()));
            rest = &after[close + 1..];
        }
        out.push((if closing { format!("/{name}") } else { name }, attrs, start, end + 1));
    }
    out
}

fn attr<'a>(attrs: &'a [(String, String)], k: &str) -> Option<&'a str> {
    attrs.iter().find(|a| a.0 == k).map(|a| a.1.as_str())
}

fn nums(s: &str) -> Vec<f64> {
    s.split([' ', ',']).filter(|t| !t.is_empty()).map(|t| t.parse::<f64>().expect(t)).collect()
}

/// A transform attribute as `[(function, args)]`.
fn transform(s: &str) -> Vec<(String, Vec<f64>)> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(open) = rest.find('(') {
        let name = rest[..open].trim().to_string();
        let close = rest.find(')').expect("transform");
        out.push((name, nums(&rest[open + 1..close])));
        rest = &rest[close + 1..];
    }
    out
}

/// One placed sign, as numbers.
#[derive(Debug, PartialEq)]
struct Placement {
    /// `translate(a b)`
    t: [f64; 2],
    /// `scale(c d)`, 1 1 if absent
    s: [f64; 2],
    /// `rotate(r)`, 0 if absent
    r: f64,
    /// where the glyph's origin is in that frame
    origin: [f64; 2],
    /// font size
    size: f64,
}

#[derive(Debug, PartialEq)]
struct UnitInfo {
    w: String,
    h: String,
    view_box: String,
    plain: Vec<Placement>,
    transformed: Vec<Placement>,
}

/// What the text printer places, per `svg`.
fn read_text(markup: &str, reversed: bool) -> Vec<UnitInfo> {
    let mut units = Vec::new();
    let mut cur: Option<UnitInfo> = None;
    for (name, a, _, _) in tags(markup) {
        match name.as_str() {
            "svg" => {
                cur = Some(UnitInfo {
                    w: attr(&a, "width").unwrap_or("").to_string(),
                    h: attr(&a, "height").unwrap_or("").to_string(),
                    view_box: attr(&a, "viewBox").unwrap_or("").to_string(),
                    plain: Vec::new(),
                    transformed: Vec::new(),
                })
            }
            "/svg" => units.extend(cur.take()),
            "tspan" if attr(&a, "x").is_some() => {
                let x: f64 = attr(&a, "x").unwrap().parse().unwrap();
                let y: f64 = attr(&a, "y").unwrap().parse().unwrap();
                let size: f64 = attr(&a, "font-size").unwrap().parse().unwrap();
                // In a reversed unit the row is mirrored by `scale(-1,1)`.
                cur.as_mut().unwrap().plain.push(Placement {
                    t: [if reversed { -x } else { x }, y],
                    s: [if reversed { -1.0 } else { 1.0 }, 1.0],
                    r: 0.0,
                    origin: [0.0, 0.0],
                    size,
                });
            }
            "text" if attr(&a, "class") == Some("hierojax-svg-visual") => {
                let tf = transform(attr(&a, "transform").unwrap());
                let get = |n: &str| tf.iter().find(|t| t.0 == n).map(|t| t.1.clone());
                let t = get("translate").unwrap();
                let s = get("scale").unwrap();
                cur.as_mut().unwrap().transformed.push(Placement {
                    t: [t[0], t[1]],
                    s: [s[0], s[1]],
                    r: get("rotate").map_or(0.0, |r| r[0]),
                    origin: [attr(&a, "x").unwrap().parse().unwrap(), attr(&a, "y").unwrap().parse().unwrap()],
                    size: attr(&a, "font-size").unwrap().parse().unwrap(),
                });
            }
            _ => {}
        }
    }
    units
}

/// What the path printer places, per `svg`: `translate(a b) [scale] [rotate] translate(x y)
/// scale(k -k)` for a sign with a transform of its own, `translate(x y) scale(k -k)` for the others.
fn read_paths(markup: &str, upem: f64, reversed: bool) -> Vec<UnitInfo> {
    let mut units = Vec::new();
    let mut cur: Option<UnitInfo> = None;
    for (name, a, _, _) in tags(markup) {
        match name.as_str() {
            "svg" => {
                cur = Some(UnitInfo {
                    w: attr(&a, "width").unwrap_or("").to_string(),
                    h: attr(&a, "height").unwrap_or("").to_string(),
                    view_box: attr(&a, "viewBox").unwrap_or("").to_string(),
                    plain: Vec::new(),
                    transformed: Vec::new(),
                })
            }
            "/svg" => units.extend(cur.take()),
            "path" if attr(&a, "transform").is_some() => {
                let tf = transform(attr(&a, "transform").unwrap());
                let (last, first) = (tf.last().unwrap(), &tf[0]);
                assert_eq!(last.0, "scale");
                assert_eq!(last.1[0].abs() * upem, last.1[1].abs() * upem);
                let size = last.1[1].abs() * upem;
                let size = (size * 1e6).round() / 1e6;
                if tf.len() == 2 {
                    // plain: `translate(x y) scale(k -k)`; in a reversed unit `translate(-x y) scale(-k -k)`
                    assert_eq!((last.1[0] < 0.0), reversed, "scale sign");
                    cur.as_mut().unwrap().plain.push(Placement {
                        t: [first.1[0], first.1[1]],
                        s: [if reversed { -1.0 } else { 1.0 }, 1.0],
                        r: 0.0,
                        origin: [0.0, 0.0],
                        size,
                    });
                } else {
                    let get = |n: &str| tf.iter().find(|t| t.0 == n).map(|t| t.1.clone());
                    let ts: Vec<&(String, Vec<f64>)> = tf.iter().filter(|t| t.0 == "translate").collect();
                    assert_eq!(ts.len(), 2, "{:?}", tf);
                    let s = get("scale");
                    // The scale in the middle: absent when it is 1 1; the last `scale` is the glyph's.
                    let mid = tf[..tf.len() - 1].iter().find(|t| t.0 == "scale").map(|t| t.1.clone());
                    let _ = s;
                    let mid = mid.unwrap_or(vec![1.0, 1.0]);
                    cur.as_mut().unwrap().transformed.push(Placement {
                        t: [ts[0].1[0], ts[0].1[1]],
                        s: [mid[0], mid[1]],
                        r: get("rotate").map_or(0.0, |r| r[0]),
                        origin: [ts[1].1[0], ts[1].1[1]],
                        size,
                    });
                }
            }
            _ => {}
        }
    }
    units
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&amp;", "&")
}

/// The text that a selection of the page copies from a path block, by kind.
fn copyable(markup: &str, kind: Selectable) -> String {
    let mut out = String::new();
    let tg = tags(markup);
    match kind {
        Selectable::Svg => {
            // Between `<text ...>` and `</text>`, without the tags of the tspans.
            for w in tg.windows(1) {
                let (name, _, _, end) = &w[0];
                if name == "text" {
                    let close = markup[*end..].find("</text>").unwrap() + *end;
                    let mut inner = String::new();
                    let mut in_tag = false;
                    for c in markup[*end..close].chars() {
                        match c {
                            '<' => in_tag = true,
                            '>' => in_tag = false,
                            c if !in_tag => inner.push(c),
                            _ => {}
                        }
                    }
                    out.push_str(&unescape(&inner));
                }
            }
        }
        Selectable::Overlay => {
            // The text of every `span aria-hidden`, which holds the boxes.
            for (name, a, _, end) in &tg {
                if name == "span" && attr(a, "aria-hidden") == Some("true") {
                    let mut depth = 1;
                    let mut inner = String::new();
                    let mut in_tag = false;
                    let mut j = *end;
                    let bytes = markup.as_bytes();
                    while depth > 0 {
                        let c = markup[j..].chars().next().unwrap();
                        if c == '<' {
                            in_tag = true;
                            if bytes[j + 1] == b'/' {
                                depth -= 1;
                            } else {
                                depth += 1;
                            }
                        } else if c == '>' {
                            in_tag = false;
                        } else if !in_tag && depth > 0 {
                            inner.push(c);
                        }
                        j += c.len_utf8();
                    }
                    out.push_str(&unescape(&inner));
                }
            }
        }
        _ => {}
    }
    out
}

fn assert_self_contained(m: &str, id: &str) {
    for bad in [" id=", "<defs", "<use", "url(", "<style", "<script", "href", "<image", "<foreignObject", "<?xml", "@font-face", "class="] {
        assert!(!m.contains(bad), "{id}: path markup has {bad:?}");
    }
}

#[test]
fn text_through_the_api_is_the_golden_bytes() {
    let corpus = common::load_corpus(None);
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(&std::fs::read(common::root().join("tests/golden/hashes.txt.gz")).expect("hashes")[..])
        .read_to_end(&mut raw)
        .expect("gunzip");
    let hashes: Vec<&str> = std::str::from_utf8(&raw).unwrap().lines().collect();
    assert_eq!(hashes.len(), corpus.len());
    let mut eng = engine();
    let mut bad = Vec::new();
    for (i, f) in corpus.iter().enumerate() {
        let rec = match parse(&f.text) {
            Err(_) => "parse".to_string(),
            Ok(doc) => match eng.layout_with(&doc, &options_of(&f.attrs)).and_then(|l| l.to_svg(SvgKind::Text).map(|svg| (l, svg))) {
                Err(_) => "failed".to_string(),
                Ok((l, svg)) => {
                    let c = l.container();
                    format!(
                        "ok\n{}\n{}\n{}\n{}\n{}\n{}",
                        c.class.clone().unwrap_or_default(),
                        c.width.clone().unwrap_or_default(),
                        c.height.clone().unwrap_or_default(),
                        c.border,
                        l.logs().join("\u{1}"),
                        svg
                    )
                }
            },
        };
        if format!("{:016x}", fnv64(rec.as_bytes())) != hashes[i] {
            bad.push(f.id.clone());
        }
    }
    assert!(bad.is_empty(), "{} fragments differ from the golden bytes: {:?}", bad.len(), &bad[..bad.len().min(8)]);
}

#[test]
fn paths_place_signs_as_the_text_does_and_copy_the_source() {
    let corpus = common::load_corpus(None);
    let mut eng = engine();
    let upem = f64::from(eng_units());
    let (mut checked, mut signs, mut overlay_bytes, mut text_bytes) = (0usize, 0usize, 0usize, 0usize);
    for f in &corpus {
        let Ok(doc) = parse(&f.text) else { continue };
        let base = options_of(&f.attrs);
        let Ok(layout) = eng.layout_with(&doc, &base) else { continue };
        let reversed = matches!(base.dir, hieroxide::Direction::Hrl | hieroxide::Direction::Vrl);
        let text = layout.to_svg(SvgKind::Text).expect("text");
        let want = read_text(&text, reversed);
        for kind in [Selectable::Overlay, Selectable::Svg, Selectable::Off] {
            let mut o = base.clone();
            o.selectable = kind;
            let layout = eng.layout_with(&doc, &o).expect("layout");
            let paths = layout.to_svg(SvgKind::Paths).expect("paths");
            assert_self_contained(&paths, &f.id);
            let got = read_paths(&paths, upem, reversed);
            assert_eq!(got, want, "{}: the path block places signs differently from the text block", f.id);
            let copied = copyable(&paths, kind);
            match kind {
                Selectable::Off => assert_eq!(copied, "", "{}", f.id),
                _ => assert_eq!(copied, f.text, "{}: {kind:?} copies something else than the source", f.id),
            }
            // The attribute on the block is the source too (attribute escapes undone).
            let at = paths.find("data-text=\"").expect("data-text") + 11;
            let data = paths[at..at + paths[at..].find('"').unwrap()].replace("&quot;", "\"").replace("&#13;", "\r").replace("&amp;", "&");
            assert_eq!(data, f.text, "{}: data-text", f.id);
            if kind == Selectable::Overlay {
                overlay_bytes += paths.len();
            }
            if kind == Selectable::Off {
                text_bytes += paths.len();
            }
        }
        signs += want.iter().map(|u| u.plain.len() + u.transformed.len()).sum::<usize>();
        checked += 1;
    }
    eprintln!("{checked} fragments, {signs} signs placed the same; path markup {text_bytes} bytes without copyable text, {overlay_bytes} with the overlay");
    assert!(checked > 19_000);
}

fn eng_units() -> u16 {
    Font::from_bytes(common::font()).expect("font").units_per_em()
}

#[test]
fn output_does_not_depend_on_the_order() {
    let corpus = common::load_corpus(None);
    let sample: Vec<&common::Fragment> = corpus.iter().step_by(23).collect();
    let run = |order: &[usize]| -> Vec<(usize, Option<String>)> {
        let mut eng = engine();
        order
            .iter()
            .map(|&i| {
                let f = sample[i];
                let out = parse(&f.text).ok().and_then(|d| eng.layout_with(&d, &options_of(&f.attrs)).ok()).and_then(|l| l.to_svg(SvgKind::Paths).ok());
                (i, out)
            })
            .collect()
    };
    let forward: Vec<usize> = (0..sample.len()).collect();
    let mut shuffled: Vec<usize> = forward.clone();
    let mut s: u64 = 12345;
    for i in (1..shuffled.len()).rev() {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        shuffled.swap(i, (s >> 33) as usize % (i + 1));
    }
    let a = run(&forward);
    let mut b = run(&shuffled);
    b.sort_by_key(|x| x.0);
    assert_eq!(a, b);
}
