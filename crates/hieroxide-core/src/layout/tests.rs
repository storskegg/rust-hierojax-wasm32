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
fn unsupported_things_say_so() {
    let enclosure = "\u{1343C}\u{13000}\u{1343D}";
    assert_eq!(render(enclosure, &raw()).unwrap_err(), LayoutError::Unsupported(Unsupported::Enclosure));
    let overlay = "\u{13000}\u{13436}\u{13000}";
    assert_eq!(render(overlay, &raw()).unwrap_err(), LayoutError::Unsupported(Unsupported::Overlay));
    let insertion = "\u{13000}\u{13432}\u{13000}";
    assert_eq!(render(insertion, &raw()).unwrap_err(), LayoutError::Unsupported(Unsupported::Insertion));
    let mut o = raw();
    o.type_ = "canvas".into();
    assert_eq!(render(MAN, &o).unwrap_err(), LayoutError::Unsupported(Unsupported::CanvasPrinter));
    o.type_ = "dom".into();
    assert_eq!(render(MAN, &o).unwrap_err(), LayoutError::Unsupported(Unsupported::DomPrinter));
}

#[test]
fn ligature_candidates_are_unsupported_but_vs_blocks_them() {
    // 13000 has no ligature with itself; find a real one in the table.
    let lig = crate::tables::ligatures()
        .iter()
        .find(|l| l.kind == crate::tables::LigKind::Horizontal && !l.alt && l.groups.iter().all(|g| !g.mirror))
        .unwrap();
    let chars: Vec<String> = lig.groups.iter().map(|g| g.ch.to_string()).collect();
    let text = chars.join(HOR);
    assert_eq!(render(&text, &raw()).unwrap_err(), LayoutError::Unsupported(Unsupported::Ligature));
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
