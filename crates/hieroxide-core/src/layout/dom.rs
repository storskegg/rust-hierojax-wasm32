//! `PrintedDOM`: turns a [`Plan`] into the markup of the `div`s upstream builds with DOM
//! calls.
//!
//! As for the SVG (see `svg.rs`), the markup is parsed by the host in one go, so what it
//! parses to must be what `createElement` and `style.setProperty` build:
//!
//! * `style.setProperty` ignores a value the CSS parser refuses, and then the element has no
//!   `style` attribute at all (an `NaNpx`, a negative size). Written into markup, a refused
//!   declaration is left out for the same reason, and an element with none gets no attribute.
//!   Which values are refused is decided here by the kind of property: a size is refused if
//!   it is not a finite, non-negative number (`-0` is fine); `left`, `bottom`, `top` and
//!   `background-position` take any finite number; `transform` is refused when any number in
//!   it is not finite.
//! * `color` takes whatever the page gave in `data-signcolor`. A value of plain characters
//!   goes into the style as it is (the parser treats it the same either way); any other (a
//!   semicolon, a quote, a comment) is not put in the markup. The element carries a marker
//!   attribute instead, [`COLOR_MARK`], and the host sets the property itself and removes
//!   the marker.
//! * `isLinux()` is a parameter: `line-height` on Linux and Android, `height` elsewhere.
//!
//! The text of a sign or of the hidden text goes through the HTML parser, as `innerHTML`
//! does, so `&`, `<` and CR are written as references.

use super::options::Options;
use super::plan::{Item, Plan, Rect, Unit};
use super::raster::Raster;
use super::shapes::{Engine, SignProps};
use super::svg::SvgOutput;
use super::LayoutError;
use crate::jsnum::{number_from_str, number_to_string, to_fixed};

/// The attribute the host looks for: `0` is the sign colour, `1` the bracket colour.
pub const COLOR_MARK: &str = "data-hx-color";

fn esc_text(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '\r' => out.push_str("&#13;"),
            _ => out.push(c),
        }
    }
}

fn fixed(x: f64, digits: u32) -> String {
    let mut s = String::new();
    to_fixed(x, digits, &mut s);
    s
}

fn num(x: f64) -> String {
    let mut s = String::new();
    number_to_string(x, &mut s);
    s
}

/// `-s` for a numeric string, as a string.
fn neg_str(s: &str) -> String {
    num(-number_from_str(s))
}

/// A number given as text is finite.
fn finite(s: &str) -> bool {
    number_from_str(s).is_finite()
}

/// A length for a property that refuses negative ones.
fn size_ok(s: &str) -> bool {
    let n = number_from_str(s);
    n.is_finite() && n >= 0.0
}

/// Characters that mean the same in a `color` declaration inside a `style` attribute as in
/// a value given to `setProperty`.
fn plain_color(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '#' | '(' | ')' | ',' | '.' | '%' | '+' | '-'))
}

/// The `style` attribute from declarations, left out when there are none.
struct Style(String);

impl Style {
    fn new() -> Style {
        Style(String::new())
    }
    fn set(&mut self, prop: &str, value: &str) {
        if !self.0.is_empty() {
            self.0.push_str("; ");
        }
        self.0.push_str(prop);
        self.0.push_str(": ");
        self.0.push_str(value);
    }
    fn write(&self, out: &mut String) {
        if !self.0.is_empty() {
            out.push_str(" style=\"");
            out.push_str(&self.0);
            out.push('"');
        }
    }
}

struct Printer<'a, Ra: Raster> {
    eng: &'a mut Engine,
    raster: &'a mut Ra,
    opts: &'a Options,
    plan: &'a Plan,
    linux: bool,
    logs: Vec<String>,
}

impl<Ra: Raster> Printer<'_, Ra> {
    fn unit(&mut self, unit: &Unit, out: &mut String) -> Result<(), LayoutError> {
        let font = self.opts.layout.fontsize;
        let w_px = font * unit.w;
        let h_px = font * unit.h;
        let separated = self.opts.layout.separated;
        // The constructor: a span of its own in separated mode, with the size.
        if separated {
            let (w, h) = (fixed(w_px, 2), fixed(h_px, 2));
            let mut st = Style::new();
            if size_ok(&w) {
                st.set("width", &format!("{w}px"));
            }
            if size_ok(&h) {
                st.set("height", &format!("{h}px"));
            }
            out.push_str("<span class=\"hierojax\"");
            st.write(out);
            out.push('>');
        }
        let mut failure = None;
        for item in &unit.items {
            let r = match item {
                Item::Sign { ch, scale, x_scale, y_scale, rotate, mirror, rect, props } => {
                    self.add_sign(unit, h_px, *ch, *scale, *x_scale, *y_scale, *rotate, *mirror, rect, props, out)
                }
                Item::Hidden { start, len } => {
                    out.push_str("<div class=\"hierojax-dom-hidden\">");
                    esc_text(self.plan.str(*start, *len), out);
                    out.push_str("</div>");
                    Ok(())
                }
                Item::Shade(r) => {
                    self.add_shading(unit, w_px, h_px, r, out);
                    Ok(())
                }
                Item::Log { start, len } => {
                    self.logs.push(self.plan.str(*start, *len).to_string());
                    Ok(())
                }
            };
            if let Err(e) = r {
                failure = Some(e);
                break;
            }
        }
        if separated {
            out.push_str("</span>");
        }
        match failure {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// `PrintedDOM.addSign`
    #[allow(clippy::too_many_arguments)]
    fn add_sign(
        &mut self,
        unit: &Unit,
        h_px: f64,
        ch: char,
        scale: f64,
        x_scale: f64,
        y_scale: f64,
        rotate: f64,
        mirror: bool,
        rect: &Rect,
        props: &SignProps,
        out: &mut String,
    ) -> Result<(), LayoutError> {
        let font = self.opts.layout.fontsize;
        let x = if unit.reversed { unit.w - (rect.x + rect.w) } else { rect.x };
        let mirror = unit.reversed != mirror;
        let font_size = font * scale;
        let font_size_str = fixed(font_size, 2);
        let x_px = font * x;
        let y_px = font * rect.y;
        let x_scale_str = fixed(x_scale, 2);
        let y_scale_str = fixed(y_scale, 2);
        let meas = self
            .eng
            .corrected_measurement(self.raster, ch, font_size, x_scale, y_scale, rotate, mirror, props)?;
        let mut st = Style::new();
        if size_ok(&font_size_str) {
            st.set("font-size", &format!("{font_size_str}px"));
            // line-height and height refuse what font-size refuses: same string.
            st.set(if self.linux { "line-height" } else { "height" }, &format!("{font_size_str}px"));
        }
        let color = if props.bracket { &self.opts.bracketcolor } else { &self.opts.signcolor };
        let plain = plain_color(color);
        if plain {
            st.set("color", color);
        }
        let tx = fixed(x_px + meas.width_scaled / 2.0 - meas.x, 2);
        let ty = fixed(y_px - meas.height_scaled / 2.0 + meas.h - meas.y, 2);
        let sx = if mirror { neg_str(&x_scale_str) } else { x_scale_str };
        let rotate_truthy = rotate != 0.0 && !rotate.is_nan();
        let rot = if rotate_truthy { Some(num(rotate)) } else { None };
        let left = fixed(-meas.width / 2.0, 2);
        let bottom = fixed(h_px - meas.height / 2.0, 2);
        if finite(&left) {
            st.set("left", &format!("{left}px"));
        }
        if finite(&bottom) {
            st.set("bottom", &format!("{bottom}px"));
        }
        if finite(&tx) && finite(&ty) && finite(&sx) && finite(&y_scale_str) && rot.as_deref().is_none_or(finite) {
            let mut t = format!("translate({tx}px,{ty}px) scale({sx},{y_scale_str}) ");
            if let Some(r) = &rot {
                t.push_str(&format!("rotate({r}deg) "));
            }
            st.set("transform", &t);
        }
        out.push_str("<div class=\"");
        out.push_str(if props.unselectable { "hierojax-dom-visual" } else { "hierojax-dom-sign" });
        out.push('"');
        st.write(out);
        if !plain {
            // The value is set by the host.
            out.push_str(&format!(" {COLOR_MARK}=\"{}\"", if props.bracket { 1 } else { 0 }));
        }
        out.push('>');
        esc_text(ch.encode_utf8(&mut [0; 4]), out);
        out.push_str("</div>");
        Ok(())
    }

    /// `PrintedDOM.addShading`, with its `hPx + hPx < this.hPx-1` (it compares twice the
    /// height of the shade, not its bottom edge, with the height of the unit).
    fn add_shading(&mut self, unit: &Unit, this_w_px: f64, this_h_px: f64, r: &Rect, out: &mut String) {
        let font = self.opts.layout.fontsize;
        let x = if unit.reversed { unit.w - (r.x + r.w) } else { r.x };
        let x_px = font * x;
        let y_px = font * r.y;
        let mut w_px = font * r.w;
        let mut h_px = font * r.h;
        let shading_diagonal_period = 6.0;
        let dx = (x_px + y_px + unit.w_accum + unit.h_accum) % (shading_diagonal_period * 2f64.sqrt());
        let mut w_back = w_px + dx;
        if x_px + w_px < this_w_px - 1.0 {
            w_px += 0.01;
            w_back += 0.01;
        }
        if h_px + h_px < this_h_px - 1.0 {
            h_px += 0.1;
        }
        let uniform = self.opts.uniform_shade;
        let (wb, hs) = (fixed(w_back, 3), fixed(h_px, 3));
        let (pos, ws) = (fixed(-dx, 3), fixed(w_px, 3));
        let (left, top) = (fixed(x_px, 3), fixed(y_px, 3));
        let mut st = Style::new();
        if size_ok(&wb) && size_ok(&hs) {
            st.set("background-size", &format!("{wb}px {hs}px"));
        }
        if finite(&pos) {
            st.set("background-position", &format!("{pos}px 0px"));
        }
        if size_ok(&ws) {
            st.set("width", &format!("{ws}px"));
        }
        if size_ok(&hs) {
            st.set("height", &format!("{hs}px"));
        }
        if finite(&left) {
            st.set("left", &format!("{left}px"));
        }
        if finite(&top) {
            st.set("top", &format!("{top}px"));
        }
        out.push_str(if uniform {
            "<div class=\"hierojax-dom-uniform\""
        } else {
            "<div class=\"hierojax-dom-hatching\""
        });
        st.write(out);
        out.push_str("></div>");
    }
}

/// Print a plan as DOM markup. `linux` is `isLinux()`. On an exception it also gives what
/// upstream would have left in the element by then.
pub fn print_dom_partial<Ra: Raster>(
    eng: &mut Engine,
    raster: &mut Ra,
    plan: &Plan,
    opts: &Options,
    linux: bool,
) -> (SvgOutput, Option<LayoutError>) {
    let mut p = Printer { eng, raster, opts, plan, linux, logs: Vec::new() };
    let mut markup = String::new();
    let mut failure = None;
    for unit in &plan.units {
        if let Err(e) = p.unit(unit, &mut markup) {
            failure = Some(e);
            break;
        }
    }
    (SvgOutput { markup, logs: p.logs }, failure)
}
