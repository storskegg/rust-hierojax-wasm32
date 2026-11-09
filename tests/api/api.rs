//! The API's behaviour on its own terms: errors are values, and the data it reports is right.
//! `cargo test -p hieroxide-api --test api`

use hieroxide_api::{parse, Direction, Engine, ErrorKind, Font, GlyphRole, Options, Selectable, SvgKind};
use hieroxide_outline::OutlineRaster;

#[path = "../golden/common.rs"]
mod common;

fn engine() -> Engine {
    Engine::new(Font::from_bytes(common::font()).expect("font"), Options::default())
}

#[test]
fn a_bad_font_is_an_error_not_a_panic() {
    let e = Font::from_bytes(b"not a font".to_vec()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Font);
    assert_eq!(Font::from_bytes(Vec::new()).unwrap_err().kind, ErrorKind::Font);
    // Every prefix of a real font, and a font with bytes changed: an error or a font, never a panic.
    let font = common::font();
    for n in (0..font.len()).step_by(font.len() / 97 + 1) {
        let _ = Font::from_bytes(font[..n].to_vec());
    }
    let mut seed: u64 = 7;
    for _ in 0..300 {
        let mut f = font.clone();
        for _ in 0..8 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let at = (seed >> 24) as usize % f.len().min(1 << 16);
            f[at] ^= (seed >> 8) as u8 | 1;
        }
        if let Ok(font) = Font::from_bytes(f) {
            // A font that is accepted must also lay out without a panic.
            let mut eng = Engine::new(font, Options::default());
            let _ = eng.render("\u{13000}\u{13431}\u{13001}", SvgKind::Paths);
        }
    }
}

#[test]
fn bad_text_is_a_parse_error_with_a_position() {
    for (text, pos) in [("a", 0), ("\u{13000}x", 2), ("\u{13430}", 0), (" \u{13000}", 0)] {
        let e = parse(text).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Parse, "{text:?}");
        assert_eq!(e.position, Some(pos), "{text:?}: {e}");
    }
    // A lone surrogate cannot be in a Rust string; an astral sign counts two units.
    assert_eq!(parse("\u{13000}\u{13000}x").unwrap_err().position, Some(4));
    // A joiner with nothing after it: the error is at the end.
    assert_eq!(parse("\u{13000}\u{13000}\u{13430}").unwrap_err().position, Some(6));
    // Nothing panics on any text, however deep or long.
    let deep = format!("{}\u{13000}{}", "\u{13258}\u{1343C}".repeat(5000), "\u{1343D}\u{13259}".repeat(5000));
    let _ = parse(&deep);
    let mut eng = engine();
    assert_eq!(eng.render("zzz", SvgKind::Paths).unwrap_err().kind, ErrorKind::Parse);
    assert!(parse("").is_ok() || parse("").is_err());
}

#[test]
fn numbers_that_cannot_be_used_are_the_defaults() {
    let mut eng = engine();
    let doc = parse("\u{13000}\u{13431}\u{13001}").unwrap();
    let base = eng.layout(&doc).unwrap().to_svg(SvgKind::Paths).unwrap();
    for bad in [f64::NAN, f64::INFINITY, -1.0, 0.0] {
        let o = Options { sep: Some(bad), line_size: Some(bad), font_size: Some(bad), ..Options::default() };
        assert_eq!(eng.layout_with(&doc, &o).unwrap().to_svg(SvgKind::Paths).unwrap(), base, "{bad}");
    }
}

#[test]
fn a_block_says_what_it_is() {
    let mut eng = engine();
    let doc = parse("\u{13000}\u{13431}\u{13001}").unwrap();
    let o = Options { label: Some("a \"quoted\" & <odd> label".to_string()), ..Options::default() };
    let svg = eng.layout_with(&doc, &o).unwrap().to_svg(SvgKind::Paths).unwrap();
    assert!(svg.contains(r#"role="img" aria-label="a &quot;quoted&quot; &amp; <odd> label""#), "{svg}");
    let svg = eng.layout(&doc).unwrap().to_svg(SvgKind::Paths).unwrap();
    assert!(svg.contains(r#"aria-label="Egyptian hieroglyphs""#));
    assert!(svg.starts_with("<span style=\"position:relative;display:inline-block;line-height:0;vertical-align:-.1em\"><svg width="), "{}", &svg[..120]);
    // Bare svg when the copyable text is inside it or off.
    for k in [Selectable::Svg, Selectable::Off] {
        let o = Options { selectable: k, ..Options::default() };
        let svg = eng.layout_with(&doc, &o).unwrap().to_svg(SvgKind::Paths).unwrap();
        assert!(svg.starts_with("<svg width="), "{k:?}");
        assert!(svg.ends_with("</svg>"));
    }
    // And it is a fragment: no prolog, no xmlns unless asked.
    assert!(!svg.contains("<?xml") && !svg.contains("xmlns"));
    let o = Options { xmlns: true, selectable: Selectable::Off, ..Options::default() };
    assert!(eng.layout_with(&doc, &o).unwrap().to_svg(SvgKind::Paths).unwrap().starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" width="));
}

/// A glyph's transform puts the outline's box where layout put the sign.
#[test]
fn glyph_transforms_put_the_signs_in_their_boxes() {
    let font = common::font();
    let mut raster = OutlineRaster::new(&font).expect("font");
    let mut eng = engine();
    let corpus = common::load_corpus(None);
    let (mut checked, mut off) = (0usize, 0usize);
    let mut worst = 0.0f64;
    let mut far: Vec<String> = Vec::new();
    for f in corpus.iter().filter(|f| f.class == "plain" && f.attrs.iter().all(|a| a.is_empty())).step_by(7).take(1500) {
        let Ok(doc) = parse(&f.text) else { continue };
        let Ok(layout) = eng.layout(&doc) else { continue };
        for g in layout.glyphs().iter().filter(|g| g.role == GlyphRole::Sign) {
            let Some(outline) = raster.outline(g.ch) else { continue };
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            let mut add = |x: f32, y: f32| {
                let t = g.transform;
                let (px, py) = (t[0] * f64::from(x) + t[2] * f64::from(y) + t[4], t[1] * f64::from(x) + t[3] * f64::from(y) + t[5]);
                x0 = x0.min(px);
                x1 = x1.max(px);
                y0 = y0.min(py);
                y1 = y1.max(py);
            };
            for s in &outline.path {
                match *s {
                    hieroxide_outline::Seg::Move(x, y) | hieroxide_outline::Seg::Line(x, y) => add(x, y),
                    hieroxide_outline::Seg::Cubic(a, b, c, d, x, y) => {
                        add(a, b);
                        add(c, d);
                        add(x, y);
                    }
                    hieroxide_outline::Seg::Close => {}
                }
            }
            if x0 > x1 {
                continue;
            }
            // The box layout gave the sign, and the ink in it: within a hundredth of an em of the box
            // (layout measures ink at 1/150 em).
            let r = g.rect;
            let miss = (x0 - r.x).abs().max((x1 - (r.x + r.w)).abs()).max((y0 - r.y).abs()).max((y1 - (r.y + r.h)).abs());
            worst = worst.max(miss);
            checked += 1;
            if miss > 0.02 {
                off += 1;
                far.push(format!("U+{:X} {:.3} (box {:.3}x{:.3}, ink {:.3}x{:.3})", g.ch as u32, miss, r.w, r.h, x1 - x0, y1 - y0));
            }
        }
    }
    eprintln!("{checked} signs: the ink is at most {worst:.4} em from the box layout gave; {off} further than 0.02 em");
    eprintln!("far: {far:?}");
    assert!(checked > 1000);
    assert!(off * 100 <= checked, "{off} of {checked}");
}

#[test]
fn the_data_says_what_is_in_the_block() {
    let mut eng = engine();
    // Left to right: a sign, a joiner, a sign; the first unit holds both.
    let l = eng.layout(&parse("\u{13000}\u{13431}\u{13001}").unwrap()).unwrap();
    assert_eq!(l.units().len(), 1);
    assert!(!l.units()[0].reversed && l.units()[0].width > 0.5 && l.units()[0].height > 0.5);
    assert_eq!(l.glyphs().iter().map(|g| g.ch).collect::<String>(), "\u{13000}\u{13001}");
    assert!(l.glyphs().iter().all(|g| g.glyph_id.is_some() && g.role == GlyphRole::Sign));
    assert_eq!(l.text(), "\u{13000}\u{13431}\u{13001}");
    // Right to left: mirrored, and the first sign of the text is at the right.
    let o = Options { dir: Direction::Hrl, ..Options::default() };
    let r = eng.layout_with(&parse("\u{13000}\u{13431}\u{13001}").unwrap(), &o).unwrap();
    assert!(r.units()[0].reversed);
    assert!(r.glyphs()[0].rect.x > r.glyphs()[1].rect.x);
    assert!(r.glyphs()[0].transform[0] < 0.0, "mirrored: x goes left");
    // Brackets are brackets; the sign a ligature stands for is a variant, not text.
    let b = eng.layout(&parse("\u{27E6}\u{13000}\u{27E7}").unwrap()).unwrap();
    assert_eq!(b.glyphs().iter().filter(|g| g.role == GlyphRole::Bracket).count(), 2);
    // Separated: a unit for each group, in a container.
    let o = Options { separated: true, ..Options::default() };
    let s = eng.layout_with(&parse("\u{13000}\u{13001}\u{13002}").unwrap(), &o).unwrap();
    assert_eq!(s.units().len(), 3);
    assert!(s.container().class.is_some());
    let svg = s.to_svg(SvgKind::Paths).unwrap();
    assert_eq!(svg.matches("<svg ").count(), 3);
    assert!(svg.starts_with("<span style=\"display:inline-block;vertical-align:-.1em\" role=\"img\""), "{}", &svg[..100]);
    assert_eq!(svg.matches("data-text=").count(), 1, "the copy attribute is on the one element that holds the block");
}

#[test]
fn a_layout_can_be_sent_to_another_thread() {
    fn send<T: Send + 'static>(_: &T) {}
    let mut eng = engine();
    let l = eng.layout(&parse("\u{13000}").unwrap()).unwrap();
    send(&l);
    send(&eng);
    let svg = std::thread::spawn(move || l.to_svg(SvgKind::Paths).unwrap()).join().unwrap();
    assert!(svg.contains("<path"));
}

#[test]
fn without_inline_style_a_block_is_a_bare_svg_with_no_style() {
    let mut eng = engine();
    let doc = parse("\u{13000}\u{13431}\u{13001}").unwrap();
    let o = Options { inline_style: false, font_size: Some(30.0), border: true, ..Options::default() };
    let svg = eng.layout_with(&doc, &o).unwrap().to_svg(SvgKind::Paths).unwrap();
    assert!(svg.starts_with("<svg width=") && !svg.contains("style="), "{}", &svg[..200]);
    // The copyable text is then inside the svg (the overlay needs styles).
    assert!(svg.contains("<text font-size=\"1000\""));
}

/// Hatching is geometry: the bands of upstream's pattern (a stroke of 50 on lines 200 apart along
/// x + y) cover 50 * sqrt(2) / 200 of a shaded rectangle, whatever its size.
#[test]
fn hatching_covers_a_third_of_a_shaded_area() {
    let mut eng = engine();
    let doc = parse("\u{13000}\u{13451}\u{13431}\u{13001}").unwrap();
    let o = Options { shading: hieroxide_api::Shading::Hatching, selectable: Selectable::Off, ..Options::default() };
    let layout = eng.layout_with(&doc, &o).unwrap();
    let shade = layout.shades()[0];
    let svg = layout.to_svg(SvgKind::Paths).unwrap();
    assert!(!svg.contains("<defs") && !svg.contains("<pattern") && !svg.contains("url("));
    let head = "<g shape-rendering=\"crispEdges\" fill=\"gray\"><path d=\"";
    let at = svg.find(head).expect("hatching") + head.len();
    let d = &svg[at..at + svg[at..].find('"').unwrap()];
    let mut area = 0.0;
    for poly in d.split('z').filter(|p| !p.is_empty()) {
        let pts: Vec<(f64, f64)> = poly
            .split(['M', 'L'])
            .filter(|p| !p.is_empty())
            .map(|p| {
                let (x, y) = p.split_once(' ').unwrap();
                (x.parse().unwrap(), y.parse().unwrap())
            })
            .collect();
        let mut a = 0.0;
        for i in 0..pts.len() {
            let (p, q) = (pts[i], pts[(i + 1) % pts.len()]);
            a += p.0 * q.1 - q.0 * p.1;
        }
        area += a.abs() / 2.0;
    }
    let rect: f64 = layout.shades().iter().map(|s| s.rect.w * 1000.0 * s.rect.h * 1000.0).sum();
    assert!(shade.rect.w > 0.0 && shade.rect.h > 0.0);
    let share = area / rect;
    assert!((share - 50.0 * 2f64.sqrt() / 200.0).abs() < 0.02, "{share}");
}

/// The copyable text is written whole, in the first unit, when the characters of the plan and of the
/// source are not as many: not reachable through `Engine` (the source is what was parsed), but
/// `write_paths` is public, and the fallback is what it does with a source that does not fit.
#[test]
fn a_source_of_another_length_is_written_whole() {
    use hieroxide_core::layout::{self, Mode, Outlines, Seg};
    struct None_;
    impl Outlines for None_ {
        fn units_per_em(&self) -> f64 {
            1000.0
        }
        fn outline(&self, _: char) -> Option<&[Seg]> {
            None
        }
    }
    let font = common::font();
    let mut raster = OutlineRaster::new(&font).expect("font").lenient();
    let mut core = layout::Engine::with_mode(Mode::Consistent);
    let tree = hieroxide_core::parse("\u{13000}\u{13431}\u{13001}").unwrap();
    let (opts, css) = layout::Options::consistent(&layout::RawOptions::default());
    let plan = layout::Layouter::new(&tree, &mut core, &mut raster).plan(&opts).unwrap();
    let placed = layout::place(&mut core, &mut raster, plan).unwrap();
    let write = |source: &str| {
        let params = layout::PathParams { label: "x", selectable: layout::Selectable::Svg, data_text: true, source, xmlns: false, inline_style: true, css_font_px: css };
        layout::write_paths(&placed, &opts, &params, &None_).unwrap().markup
    };
    // As many characters: piece by piece, a tspan at each sign.
    assert!(write("\u{13000}\u{13431}\u{13001}").contains("<tspan x="));
    // Another count: all of it, once, as one run, and no tspan.
    let whole = write("\u{13000}\u{13001}");
    assert!(whole.contains("\u{13000}\u{13001}</text>") && !whole.contains("<tspan"), "{whole}");
    assert_eq!(whole.matches("<text").count(), 1);
}
