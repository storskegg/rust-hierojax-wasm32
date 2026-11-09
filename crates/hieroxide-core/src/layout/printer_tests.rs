//! Native checks of the canvas and DOM printers, with the stub raster of `tests.rs`. The
//! parity harness is the real test against upstream.

use super::*;
use crate::parse;

const MAN: &str = "\u{13000}"; // the reference glyph

fn raw() -> RawOptions {
    RawOptions { font_px: 22.0, sign_color: "rgb(0, 0, 0)".into(), ..Default::default() }
}

fn render_with(text: &str, raw: &RawOptions, linux: bool) -> Result<Rendered, LayoutError> {
    let tree = parse(text).unwrap();
    let mut engine = Engine::new();
    render(&mut engine, &tree, raw, &mut BoxRaster::default(), &mut NoIds, true, linux)
}

fn dom(text: &str, raw: &mut RawOptions, linux: bool) -> Rendered {
    raw.type_ = "dom".into();
    render_with(text, raw, linux).unwrap()
}

#[test]
fn dom_one_sign() {
    let mut o = raw();
    let r = dom(MAN, &mut o, false);
    // The element itself gets the size: 22 * 0.57 and 22 * 1.07.
    assert_eq!(r.container.prop_width.as_deref(), Some("12.54px"));
    assert_eq!(r.container.prop_height.as_deref(), Some("23.54px"));
    assert!(r.canvas.is_none());
    let m = &r.svg.markup;
    assert!(
        m.starts_with(
            "<div class=\"hierojax-dom-sign\" style=\"font-size: 22.00px; height: 22.00px; \
             color: rgb(0, 0, 0); left: -5.50px; bottom: 12.54px; transform: translate("
        ),
        "{m}"
    );
    assert!(m.ends_with("scale(1.00,1.00) \">\u{13000}</div>"), "{m}");
    // On Linux and Android it is line-height that carries the size.
    let l = dom(MAN, &mut o, true).svg.markup;
    assert!(l.contains("line-height: 22.00px") && !l.contains(" height: 22.00px"), "{l}");
}

#[test]
fn dom_leaves_out_what_the_css_parser_would_refuse() {
    let mut o = raw();
    o.fontsize = "-4".into();
    let m = dom(MAN, &mut o, false).svg.markup;
    assert!(!m.contains("font-size") && m.contains("left:"), "{m}");
    // A colour that could end the declaration is set by the host.
    o.fontsize = String::new();
    o.signcolor = "red; width: 1px".into();
    let m = dom(MAN, &mut o, false).svg.markup;
    assert!(!m.contains("red") && m.contains("data-hx-color=\"0\""), "{m}");
}

#[test]
fn dom_separated_has_a_span_per_group() {
    let mut o = raw();
    o.separated = "true".into();
    let r = dom(&format!("{MAN}{MAN}{MAN}"), &mut o, false);
    assert!(r.container.prop_width.is_none());
    assert_eq!(r.svg.markup.matches("<span class=\"hierojax\" style=\"width: ").count(), 3);
}

#[test]
fn dom_border_goes_on_the_element() {
    let mut o = raw();
    o.border = "true".into();
    assert!(dom(MAN, &mut o, false).container.add_border);
}

#[test]
fn canvas_ops_for_one_sign() {
    let mut o = raw();
    o.type_ = "canvas".into();
    let r = render_with(MAN, &o, false).unwrap();
    let c = r.canvas.unwrap();
    assert!(r.svg.markup.is_empty());
    // Math.ceil(22 * 0.57), Math.ceil(22 * 1.07); not separated, so the hidden text follows.
    assert_eq!(&c.ops[..3], &[13.0, 24.0, 4.0]);
    assert_eq!(c.ops[3], OP_SIGN);
    assert_eq!(c.ops[c.ops.len() - 2], f64::from(MAN.chars().next().unwrap() as u32));
    assert_eq!(*c.ops.last().unwrap(), OP_END);
    assert_eq!(c.text, MAN);
    o.border = "true".into();
    assert_eq!(render_with(MAN, &o, false).unwrap().canvas.unwrap().ops[2], 6.0);
    o.separated = "true".into();
    let c = render_with(&format!("{MAN}{MAN}{MAN}"), &o, false).unwrap().canvas.unwrap();
    // Three canvases of 13 by 24, each starting with its size and the flag for separated.
    assert_eq!(&c.ops[..3], &[13.0, 24.0, 1.0]);
    let n = c.ops.windows(3).filter(|w| w == &[13.0, 24.0, 1.0]).count();
    assert_eq!(n, 3);
    assert!(c.text.is_empty());
}
