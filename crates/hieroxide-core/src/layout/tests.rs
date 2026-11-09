//! Native checks of the layout on a few fragments, with a stub raster (every glyph is a
//! solid box half as wide as it is high). The numbers are worked out by hand from
//! `src/formatting.js`; the parity harness is the real test against upstream.

use super::*;
use crate::parse;

const MAN: &str = "\u{13000}"; // the reference glyph
const HOR: &str = "\u{13431}";
const VER: &str = "\u{13430}";

fn raw() -> RawOptions {
    RawOptions { font_px: 22.0, sign_color: "rgb(0, 0, 0)".into(), ..Default::default() }
}

fn render(text: &str, raw: &RawOptions) -> Result<Rendered, LayoutError> {
    let tree = parse(text).unwrap();
    let mut engine = Engine::new();
    render_svg(&mut engine, &tree, raw, &mut BoxRaster::default(), &mut NoIds, true)
}

#[test]
fn one_sign() {
    let r = render(MAN, &raw()).unwrap();
    // Size 0.5 by 1 em; plus sep 0.07 in each direction: ceil(22 * 0.57) and ceil(22 * 1.07).
    assert!(r.svg.markup.starts_with("<svg width=\"13\" height=\"24\"><text x=\"0\" y=\"0\">"));
    // x = 22 * 0.035, widthScaled = 11, y = 22 * 0.035 - 11 + 22
    assert!(r.svg.markup.contains(" dx=\"6.27\" dy=\"11.77\">\u{13000}</tspan>"), "{}", r.svg.markup);
    assert!(r.svg.markup.contains("x=\"-5.50\" y=\"11.00\""));
    assert!(r.svg.markup.contains("<tspan class=\"hierojax-svg-sign\""));
    assert!(r.svg.markup.ends_with("</text></svg>"));
    assert_eq!(r.container, Container::default());
}

#[test]
fn two_signs_are_separated_by_sep() {
    let r = render(&format!("{MAN}{HOR}{MAN}"), &raw()).unwrap();
    // width 0.5 + 0.07 + 0.5 and sep again, ceil(22 * 1.14); hidden separator between the signs.
    assert!(r.svg.markup.starts_with("<svg width=\"26\" height=\"24\">"));
    assert!(r.svg.markup.contains("\u{13431}</tspan>"));
}

#[test]
fn reversed_direction_mirrors_the_text() {
    let mut o = raw();
    o.dir = "hrl".into();
    let r = render(MAN, &o).unwrap();
    assert!(r.svg.markup.contains("<text x=\"0\" y=\"0\" transform=\"scale(-1,1)\">"));
    // reverse(): w - (x + rect.w) = 0.57 - (0.035 + 0.5) = 0.035; dx = -xTrans
    assert!(r.svg.markup.contains(" dx=\"-6.27\" "), "{}", r.svg.markup);
}

#[test]
fn rotation_and_mirror_make_a_text_element() {
    // U+13012 allows a 45 degree rotation (adjusted by -15): variation selector 4 is U+FE03.
    let r = render("\u{13012}\u{FE03}", &raw()).unwrap();
    assert!(r.svg.markup.contains("rotate(30)"), "{}", r.svg.markup);
    assert!(r.svg.markup.contains("class=\"hierojax-svg-visual\""));
    let r = render("\u{13000}\u{13440}", &raw()).unwrap();
    assert!(r.svg.markup.contains("scale(-1 1.00)"), "{}", r.svg.markup);
    // The hidden text holds the sign, then the mirror mark.
    assert!(r.svg.markup.contains("<tspan class=\"hierojax-svg-hidden\">\u{13000}</tspan><tspan class=\"hierojax-svg-hidden\">\u{13440}</tspan>"));
}

#[test]
fn damage_adds_a_shade_before_everything_else() {
    let r = render("\u{13000}\u{13447}", &raw()).unwrap();
    assert!(r.svg.markup.starts_with("<svg width=\"13\" height=\"24\"><rect class=\"hierojax-svg-uniform\" "));
    let mut o = raw();
    o.shadepattern = "hatching".into();
    let r = render("\u{13000}\u{13447}", &o).unwrap();
    assert!(r.svg.markup.contains("<defs><pattern id=\"hierojax-svg-id0\""));
    assert!(r.svg.markup.contains("fill=\"url(#hierojax-svg-id0)\""));
    // The rect was inserted last, so it comes first.
    assert!(r.svg.markup.find("<rect").unwrap() < r.svg.markup.find("<defs>").unwrap());
}

#[test]
fn separated_mode_has_one_svg_per_group_and_changes_the_container() {
    let mut o = raw();
    o.separated = "true".into();
    let r = render(&format!("{MAN}{MAN}"), &o).unwrap();
    assert_eq!(r.container.class_name, Some("hierojax-hlr-container"));
    assert_eq!(r.svg.markup.matches("<svg ").count(), 2);
    assert!(r.container.style_width.is_none());
    o.dir = "vlr".into();
    o.border = "true".into();
    let r = render(&format!("{MAN}{MAN}"), &o).unwrap();
    assert_eq!(r.container.class_name, Some("hierojax-v-container"));
    assert!(r.container.add_border);
    assert_eq!(r.container.style_width.as_deref(), Some("24.00px"));
}

#[test]
fn rotation_without_an_entry_is_logged() {
    let mut o = raw();
    o.log = "true".into();
    // U+13000 has no 90 degree entry: variation selector 1 is U+FE00.
    let r = render("\u{13000}\u{FE00}", &o).unwrap();
    assert_eq!(r.svg.logs, vec!["Unknown rotation 90 degrees for \u{13000}".to_string()]);
    o.log = String::new();
    assert!(render("\u{13000}\u{FE00}", &o).unwrap().svg.logs.is_empty());
}

#[test]
fn bracket_is_squeezed_into_the_space_beside_the_sign() {
    // The room for the opening bracket is sep / 2, far narrower than the glyph's ratio,
    // so xScale bottoms out at 0.5; that makes it a transformed text element.
    let r = render(&format!("[{MAN}"), &raw()).unwrap();
    assert!(r.svg.markup.contains("scale(0.50 1.00)"), "{}", r.svg.markup);
    assert!(r.svg.markup.contains("hierojax-svg-hidden\">["));
}

/// `A VER A HOR [ ... ]` nested `depth` times: a Vertical in a Horizontal in a Vertical,
/// as deep as the parser allows.
fn nested(depth: usize) -> String {
    let mut s = format!("{MAN}{VER}{MAN}");
    for _ in 0..depth {
        s = format!("{MAN}{VER}{MAN}{HOR}\u{13437}{s}\u{13438}");
    }
    s
}

#[test]
fn nesting_to_the_parser_limit_lays_out() {
    // Debug builds have big frames; the wasm stack use is measured separately.
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(|| {
            let r = render(&nested(crate::MAX_DEPTH as usize), &raw()).unwrap();
            assert!(r.svg.markup.starts_with("<svg "));
            assert!(parse(&nested(crate::MAX_DEPTH as usize + 1)).is_err());
        })
        .unwrap()
        .join()
        .unwrap();
}

// ---- group pixels ----

/// Records what reaches the raster, and answers as [`BoxRaster`].
#[derive(Default)]
struct Recording {
    inner: BoxRaster,
    draws: Vec<(f64, f64, Vec<DrawCmd>)>,
}

impl Raster for Recording {
    fn advance(&mut self, ch: char, font_size: f64) -> Result<f64, RasterError> {
        self.inner.advance(ch, font_size)
    }
    fn draw(&mut self, w: f64, h: f64, cmds: &[DrawCmd]) -> Result<Bitmap, RasterError> {
        self.draws.push((w, h, cmds.to_vec()));
        self.inner.draw(w, h, cmds)
    }
}

const TS: &str = "\u{13432}";
const OPEN: &str = "\u{13258}";
const CLOSE: &str = "\u{1325B}";
const BENC: &str = "\u{1343C}";
const EENC: &str = "\u{1343D}";

#[test]
fn an_insertion_is_printed_after_its_core_with_a_control_between() {
    let r = render(&format!("{MAN}{TS}{MAN}"), &raw()).unwrap();
    let m = &r.svg.markup;
    assert!(m.contains(">\u{13432}</tspan>"), "{m}");
    assert_eq!(m.matches(">\u{13000}</tspan>").count(), 2);
}

#[test]
fn the_measuring_canvas_gets_upstreams_calls() {
    let tree = parse(&format!("{MAN}{TS}{MAN}")).unwrap();
    let mut engine = Engine::new();
    let mut rec = Recording::default();
    render_svg(&mut engine, &tree, &raw(), &mut rec, &mut NoIds, true).unwrap();
    // First the reference glyph, measured by measureGlyph at size 150: 75 + 6 by 150 + 6.
    let (w, h, cmds) = &rec.draws[0];
    assert_eq!((*w, *h), (81.0, 156.0));
    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0].font_size, 150.0);
    // Then the core at size 50: Math.ceil(50 * 0.5) by Math.ceil(50 * 1), with a font of
    // Math.floor(50 * 1), translated by xPx + widthScaled / 2 - x and so on.
    let (w, h, cmds) = &rec.draws[1];
    assert_eq!((*w, *h), (25.0, 50.0));
    assert_eq!(cmds.len(), 1);
    let c = cmds[0];
    assert_eq!((c.font_size, c.sx, c.sy, c.rotate), (50.0, 1.0, 1.0, 0.0));
    assert_eq!((c.tx, c.ty), (12.5, 25.0));
    assert_eq!((c.x, c.y), (-12.5, 25.0));
    // Then the inserted sign on its own canvas, and no canvas is read with the insertion on
    // it before the core has been read without.
    let (w, h, cmds) = &rec.draws[2];
    assert_eq!((*w, *h), (25.0, 50.0));
    assert_eq!(cmds.len(), 1);
}

#[test]
fn an_empty_insertion_canvas_throws_where_the_browser_would() {
    // Size 0 by 0.5 or so: canvas.width = 0, and getImageData throws on it.
    let r = render(&format!("{MAN}{TS}{BENC}{EENC}"), &raw());
    assert_eq!(r.unwrap_err(), LayoutError::Raster(RasterError("IndexSizeError".into())));
}

#[test]
fn an_enclosure_has_its_outlines_and_empty_hidden_elements() {
    let r = render(&format!("{OPEN}{BENC}{MAN}{EENC}{CLOSE}"), &raw()).unwrap();
    let m = &r.svg.markup;
    // Upstream quirk: addHidden(numToDamage(0)) adds an empty element after each delimiter.
    assert!(m.contains("<tspan class=\"hierojax-svg-hidden\"></tspan><tspan class=\"hierojax-svg-hidden\">\u{1343C}</tspan>"), "{m}");
    assert!(m.contains("\u{E45C}"), "{m}");
}

#[test]
fn a_horizontal_ligature_is_one_sign_with_the_text_hidden() {
    let lig = crate::tables::ligatures()
        .iter()
        .find(|l| l.kind == crate::tables::LigKind::Horizontal && !l.alt && l.groups.iter().all(|g| !g.mirror))
        .unwrap();
    let chars: Vec<String> = lig.groups.iter().map(|g| g.ch.to_string()).collect();
    let text = chars.join(HOR);
    let r = render(&text, &raw()).unwrap();
    let m = &r.svg.markup;
    let sign = format!(">{}</tspan>", lig.ch);
    assert!(m.contains(&sign), "{m}");
    assert!(m.contains("hierojax-svg-visual"), "{m}");
    assert!(m.contains(&format!("hidden\">{text}</tspan>")), "{m}");
    // A variation selector on a sign blocks it.
    let r = render(&format!("{}\u{FE00}{HOR}{}", chars[0], chars[1]), &raw()).unwrap();
    assert!(!r.svg.markup.contains(&sign));
}

#[test]
fn an_insertion_alternative_without_a_ligature_entry_is_upstreams_type_error() {
    // Shapes.insertions[U+13353] has an alternative with a glyph that Shapes.ligatures lacks.
    let lig = crate::tables::ligature('\u{13353}').unwrap();
    let alt = crate::tables::insertions('\u{13353}').unwrap().iter().find(|a| a.glyph.is_some()).unwrap();
    let side = |signs: &[crate::tables::Sign], sep: &str| {
        let s: Vec<String> = signs.iter().map(|s| s.ch.to_string()).collect();
        if s.len() > 1 { format!("\u{13437}{}\u{13438}", s.join(sep)) } else { s.join("") }
    };
    let mut text = format!("{}\u{13436}{}", side(lig.horizontal, HOR), side(lig.vertical, VER));
    const CONTROLS: [char; 7] = ['\u{13432}', '\u{13433}', '\u{13434}', '\u{13435}', '\u{13439}', '\u{1343A}', '\u{1343B}'];
    for p in 0..CONTROLS.len() {
        if alt.place(p).is_some() {
            text.push(CONTROLS[p]);
            text.push_str(MAN);
        }
    }
    assert_eq!(render(&text, &raw()).unwrap_err(), LayoutError::Js(JsFault::LigatureUndefined));
}

#[test]
fn unknown_insertions_are_logged_before_an_unknown_rotation() {
    let mut o = raw();
    o.log = "true".into();
    // U+13001 has no entry in Shapes.insertions.
    let r = render(&format!("\u{13001}\u{FE00}{TS}{MAN}\u{13433}{MAN}"), &o).unwrap();
    assert_eq!(
        r.svg.logs,
        vec![
            "Unknown insertions ts,bs for \u{13001}".to_string(),
            "Unknown rotation 90 degrees for \u{13001}".to_string()
        ]
    );
}

// ---- no panics ----

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.below(xs.len())]
    }
}

/// Glyphs with arbitrary shapes: advances of all sizes (zero too), and bitmaps of noise, of
/// nothing, of everything; what the layout makes of them is nonsense, but it must not panic.
struct NoiseRaster(Rng);

impl Raster for NoiseRaster {
    fn advance(&mut self, _ch: char, font_size: f64) -> Result<f64, RasterError> {
        Ok(match self.0.below(6) {
            0 => 0.0,
            1 => font_size * 3.0,
            _ => font_size * self.0.below(100) as f64 / 100.0,
        })
    }
    fn draw(&mut self, w: f64, h: f64, _cmds: &[DrawCmd]) -> Result<Bitmap, RasterError> {
        let (w, h) = (canvas_dim(w, 300), canvas_dim(h, 150));
        if w == 0 || h == 0 {
            return Err(RasterError("IndexSizeError".into()));
        }
        let n = w as usize * h as usize;
        if n > 1 << 22 {
            return Err(RasterError("too large for a test".into()));
        }
        let mode = self.0.below(5);
        let mut data = vec![0u8; n * 4];
        for px in data.chunks_mut(4) {
            let dark = match mode {
                0 => false,
                1 => true,
                2 => self.0.below(2) == 0,
                _ => self.0.below(40) == 0,
            };
            px[3] = if dark { 255 } else { 0 };
        }
        Ok(Bitmap { w, h, data })
    }
}

/// Text from the grammar's alphabet, with a shape that the parser mostly takes.
fn random_group(r: &mut Rng, depth: u32) -> String {
    const SIGNS: [&str; 16] = [
        "\u{13000}", "\u{13012}", "\u{13153}", "\u{13093}", "\u{13098}", "\u{130C0}", "\u{1309D}", "\u{13353}",
        "\u{13381}", "\u{132DE}", "\u{132DF}", "\u{E506}", "\u{1336F}", "\u{133CF}", "\u{FFFD}", "\u{13001}",
    ];
    const SUFFIX: [&str; 8] = ["", "", "", "\u{13440}", "\u{FE00}", "\u{FE03}", "\u{13447}", "\u{13455}"];
    let sign = |r: &mut Rng| format!("{}{}", r.pick(&SIGNS), r.pick(&SUFFIX));
    if depth == 0 {
        return sign(r);
    }
    match r.below(12) {
        0 | 1 => sign(r),
        2 => format!("{}{}{}", random_group(r, depth - 1), HOR, random_group(r, depth - 1)),
        3 => format!("{}{}{}", random_group(r, depth - 1), VER, random_group(r, depth - 1)),
        4 | 5 => {
            let controls = ["\u{13432}", "\u{13433}", "\u{13434}", "\u{13435}", "\u{13439}", "\u{1343A}", "\u{1343B}"];
            let mut s = sign(r);
            for _ in 0..1 + r.below(3) {
                s.push_str(r.pick(&controls));
                s.push_str("\u{13437}");
                s.push_str(&random_group(r, depth - 1));
                s.push_str("\u{13438}");
            }
            s
        }
        6 => format!("{}\u{13436}{}", sign(r), sign(r)),
        7 => format!("{OPEN}{BENC}{}{EENC}{CLOSE}", random_group(r, depth - 1)),
        8 => format!("{BENC}{}{EENC}", random_group(r, depth - 1)),
        9 => format!("\u{1343E}{}\u{1343F}", random_group(r, depth - 1)),
        10 => r.pick(&["\u{13441}", "\u{13442}", "\u{13443}", "\u{13444}", "\u{13445}", "\u{13446}", OPEN, CLOSE, "\u{1343C}\u{1343D}"]).to_string(),
        _ => format!("[{}]{}{}", sign(r), HOR, sign(r)),
    }
}

fn odd_options(r: &mut Rng) -> RawOptions {
    let mut o = raw();
    o.sep = r.pick(&["", "", "0.2", "1", "3", "-0.01", "-0.1", "-0.5", "-3", "1e-9"]).to_string();
    o.fontsize = r.pick(&["", "", "33.6", "0", "x", "-5", "0.5", "150"]).to_string();
    o.linesize = r.pick(&["", "", "2", "0.01", "-1", "100"]).to_string();
    o.dir = r.pick(&["", "hrl", "vlr", "vrl", "foo"]).to_string();
    o.separated = r.pick(&["", "true"]).to_string();
    o.align = r.pick(&["", "bottom"]).to_string();
    o.shadepattern = r.pick(&["", "hatching"]).to_string();
    o.log = r.pick(&["", "true"]).to_string();
    o
}

/// Lay out a text; anything but a panic is fine, but an `Internal` error means a state the
/// port thinks cannot occur.
fn lay_out_any(text: &str, o: &RawOptions, raster: &mut impl Raster) -> Result<bool, String> {
    let Ok(tree) = parse(text) else { return Ok(false) };
    let mut engine = Engine::new();
    match render_svg(&mut engine, &tree, o, raster, &mut NoIds, true) {
        Err(LayoutError::Internal(m)) => Err(format!("{m}: {text:?} with {o:?}")),
        _ => Ok(true),
    }
}

/// Upstream's constructors run for a text that parses; so do these, whatever follows.
#[test]
fn nothing_panics_on_random_fragments() {
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(|| {
            let mut r = Rng(0x9E3779B97F4A7C15);
            let mut accepted = 0;
            for _ in 0..3000 {
                let depth = r.below(4) as u32;
                let mut text = random_group(&mut r, depth);
                if r.below(3) == 0 {
                    text.push_str(&random_group(&mut r, 1));
                }
                let o = odd_options(&mut r);
                let seed = r.next() | 1;
                accepted += u32::from(lay_out_any(&text, &raw(), &mut BoxRaster::default()).unwrap());
                lay_out_any(&text, &o, &mut BoxRaster::default()).unwrap();
                lay_out_any(&text, &o, &mut NoiseRaster(Rng(seed))).unwrap();
            }
            assert!(accepted > 1000, "the generator only gave {accepted} fragments the parser takes");
        })
        .unwrap()
        .join()
        .unwrap();
}

/// The texts of the items of the parity corpus in `group`, if it has been built.
fn corpus_texts(group: &str) -> Option<Vec<String>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/corpus/corpus.json");
    let corpus = std::fs::read_to_string(path).ok()?;
    let needle = format!("\"group\":\"{group}\"");
    let mut out = Vec::new();
    for item in corpus.split("{\"id\":\"").skip(1) {
        if !item.contains(&needle) {
            continue;
        }
        let Some((_, rest)) = item.split_once("\"text\":\"") else { continue };
        // The texts hold no quotes or backslashes but as JSON escapes; the generated group's
        // are plain.
        let Some((text, _)) = rest.split_once('"') else { continue };
        if !text.contains('\\') {
            out.push(text.to_string());
        }
    }
    Some(out)
}

/// The generated fragments of the parity corpus (where it has been built, with
/// `node tests/parity/corpus/build.mjs`), with sensible and with odd options.
#[test]
fn nothing_panics_on_the_generated_corpus() {
    let Some(texts) = corpus_texts("generated") else {
        eprintln!("skipped: the parity corpus has not been built");
        return;
    };
    assert!(texts.len() > 4000);
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let mut r = Rng(0xDEADBEEFCAFEF00D);
            for text in &texts {
                lay_out_any(text, &raw(), &mut BoxRaster::default()).unwrap();
                let o = odd_options(&mut r);
                let seed = r.next() | 1;
                lay_out_any(text, &o, &mut BoxRaster::default()).unwrap();
                lay_out_any(text, &raw(), &mut NoiseRaster(Rng(seed))).unwrap();
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

/// Nested as deep as the parser allows, in the shapes that recurse the most.
#[test]
fn deep_insertions_and_enclosures_lay_out() {
    std::thread::Builder::new()
        .stack_size(512 << 20)
        .spawn(|| {
            let d = crate::MAX_DEPTH as usize;
            let mut encl = MAN.to_string();
            let mut levels = 0;
            for _ in 0..d {
                let next = format!("{OPEN}{BENC}{encl}{EENC}{CLOSE}");
                if parse(&next).is_err() {
                    break;
                }
                encl = next;
                levels += 1;
            }
            assert!(levels > 100, "only {levels} levels of enclosure parse");
            assert!(render(&encl, &raw()).unwrap().svg.markup.starts_with("<svg "));
            let mut ins = format!("{MAN}{TS}{MAN}");
            let mut levels = 0;
            for _ in 0..d {
                let next = format!("{MAN}{TS}\u{13437}{ins}\u{13438}");
                if parse(&next).is_err() {
                    break;
                }
                ins = next;
                levels += 1;
            }
            assert!(levels > 100, "only {levels} levels of insertion parse");
            assert!(render(&ins, &raw()).unwrap().svg.markup.starts_with("<svg "));
        })
        .unwrap()
        .join()
        .unwrap();
}
