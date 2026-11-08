//! The printer of consistent mode: em geometry, the same numbers at every font size.
//!
//! Layout runs on a virtual font size of [`EM_UNITS`] (see [`Options::consistent`]), so nothing
//! in it can depend on the page. The markup then has
//!
//! * a `viewBox` in those units, with `width` and `height` in `em`, so the box scales with the
//!   CSS font size of its surroundings and has no `Math.ceil` in it;
//! * one rounding per printed number, in [`unit`], where upstream rounds each term to two
//!   decimals of a pixel and then adds them in the browser.
//!
//! **Why 1000 units and one decimal.** That is a grid of 1/10000 em: 0.004 px at 40 px, finer than
//! upstream's 0.01 px, and still under 1/64 px (the layout unit of Blink) up to 150 px. A
//! wall of signs at 1000 px is off by at most 0.05 px. It also matches the font's 1000 units per
//! em, so a coordinate reads as font units. Numbers stay short: `1234.5` at worst for a
//! group of a few em, shorter than upstream's `dx="12.87" dy="21.67"` once `x` and `dx` are merged.
//!
//! Scales of a transform have four decimals: a bracket's `xScale` times a width of an em or so
//! then stays on the same grid. Rotations are printed in full, as upstream does.

use std::collections::HashSet;

use super::options::{Options, RawOptions, EM_UNITS};
use super::plan::{Item, Plan, Rect, Unit};
use super::raster::Raster;
use super::shapes::{Engine, Mode, SignProps};
use super::svg::{esc_attr, esc_text, internal_css, SvgOutput};
use super::{Layouter, LayoutError, Rendered};
use crate::jsnum::{number_to_string, to_fixed};
use crate::tree::Tree;

/// One period of the hatching in units. Upstream's is 4 px at any font size, so it cannot be kept
/// as it is in em; this is 4 px at the default 20 px of an em, and it scales with the rest.
const HATCH_PERIOD: f64 = 200.0;
const HATCH_PATH: &str = "M-50,50 l100,-100 M0,200 l200,-200 M150,250 l100,-100";
const HATCH_STROKE: &str = "stroke-width:50";

/// `x` with at most `decimals` decimals, no trailing zeros, and no `-0`. Infinities and NaN set
/// `bad`, and the fragment comes back as an error.
fn fixed(x: f64, decimals: u32, bad: &mut bool, out: &mut String) {
    if !x.is_finite() {
        *bad = true;
        out.push('0');
        return;
    }
    let start = out.len();
    to_fixed(x, decimals, out);
    if out[start..].contains('.') {
        let t = out.trim_end_matches('0').trim_end_matches('.').len();
        out.truncate(t);
    }
    if &out[start..] == "-0" {
        out.truncate(start);
        out.push('0');
    }
}

/// A coordinate in units of the `viewBox`.
fn unit(x: f64, bad: &mut bool, out: &mut String) {
    fixed(x, 1, bad, out);
}

struct Printer<'a, Ra: Raster> {
    eng: &'a mut Engine,
    raster: &'a mut Ra,
    opts: &'a Options,
    plan: &'a Plan,
    css_font_px: Option<f64>,
    bad: bool,
    logs: Vec<String>,
}

/// What one `svg` collects before it is written.
#[derive(Default)]
struct Parts {
    front: Vec<String>,
    patterns: HashSet<u32>,
    shaded: bool,
    spans: String,
    signs: String,
}

impl<Ra: Raster> Printer<'_, Ra> {
    fn unit(&mut self, u: &Unit, out: &mut String) -> Result<(), LayoutError> {
        let mut parts = Parts::default();
        for item in &u.items {
            match item {
                Item::Sign { ch, scale, x_scale, y_scale, rotate, mirror, rect, props } => {
                    self.sign(u, &mut parts, *ch, *scale, *x_scale, *y_scale, *rotate, *mirror, rect, props)?;
                }
                Item::Hidden { start, len } => {
                    let s = self.plan.str(*start, *len);
                    if !s.is_empty() {
                        parts.spans.push_str("<tspan class=\"hierojax-svg-hidden\">");
                        esc_text(s, &mut parts.spans);
                        parts.spans.push_str("</tspan>");
                    }
                }
                Item::Shade(r) => self.shade(u, &mut parts, r),
                Item::Log { start, len } => self.logs.push(self.plan.str(*start, *len).to_string()),
            }
        }
        let (w, h) = (EM_UNITS * u.w, EM_UNITS * u.h);
        out.push_str("<svg width=\"");
        fixed(u.w, 4, &mut self.bad, out);
        out.push_str("em\" height=\"");
        fixed(u.h, 4, &mut self.bad, out);
        out.push_str("em\" viewBox=\"0 0 ");
        unit(w, &mut self.bad, out);
        out.push(' ');
        unit(h, &mut self.bad, out);
        out.push_str("\" fill=\"currentColor\"");
        if parts.shaded {
            out.push_str(" shape-rendering=\"crispEdges\"");
        }
        if self.opts.border && !self.opts.layout.separated {
            out.push_str(" class=\"hierojax-border\"");
        }
        if let Some(px) = self.css_font_px {
            out.push_str(" style=\"font-size:");
            number_to_string(px, out);
            out.push_str("px\"");
        }
        out.push('>');
        for f in parts.front.iter().rev() {
            out.push_str(f);
        }
        if self.opts.standalone {
            out.push_str("<style type=\"text/css\">");
            out.push_str(&internal_css(&self.eng.external_font));
            out.push_str("</style>");
        }
        out.push_str("<text");
        if u.reversed {
            out.push_str(" transform=\"scale(-1,1)\"");
        }
        out.push('>');
        out.push_str(&parts.spans);
        out.push_str("</text>");
        out.push_str(&parts.signs);
        out.push_str("</svg>");
        Ok(())
    }

    /// `PrintedSVG.addSign`, on the same terms: a sign with a transform gets its own `text`, the
    /// others are a `tspan`, so the order of selectable text is upstream's.
    #[allow(clippy::too_many_arguments)]
    fn sign(
        &mut self,
        u: &Unit,
        parts: &mut Parts,
        ch: char,
        scale: f64,
        x_scale: f64,
        y_scale: f64,
        rotate: f64,
        mirror: bool,
        rect: &Rect,
        props: &SignProps,
    ) -> Result<(), LayoutError> {
        let bad = &mut self.bad;
        let x = if u.reversed { u.w - (rect.x + rect.w) } else { rect.x };
        let mirror_any = u.reversed != mirror;
        let font_size = EM_UNITS * scale;
        let x_u = EM_UNITS * x;
        let y_u = EM_UNITS * rect.y;
        let meas = self.eng.corrected_measurement(self.raster, ch, font_size, x_scale, y_scale, rotate, mirror_any, props)?;
        let x_trans = x_u + meas.width_scaled / 2.0 - meas.x;
        let y_trans = y_u - meas.height_scaled / 2.0 + meas.h - meas.y;
        let color = if props.bracket { self.opts.bracketcolor.as_str() } else { self.opts.signcolor.as_str() };
        let rotate_truthy = rotate != 0.0 && !rotate.is_nan();
        let mut chb = [0u8; 4];
        let chs: &str = ch.encode_utf8(&mut chb);
        if rotate_truthy || mirror || x_scale != 1.0 || y_scale != 1.0 {
            let s = &mut parts.signs;
            s.push_str("<text class=\"hierojax-svg-visual\" x=\"");
            unit(-meas.width / 2.0, bad, s);
            s.push_str("\" y=\"");
            unit(meas.height / 2.0, bad, s);
            s.push_str("\" font-size=\"");
            unit(font_size, bad, s);
            s.push('"');
            push_fill(s, color);
            s.push_str(" transform=\"translate(");
            unit(x_trans, bad, s);
            s.push(' ');
            unit(y_trans, bad, s);
            s.push_str(") scale(");
            fixed(if mirror_any { -x_scale } else { x_scale }, 4, bad, s);
            s.push(' ');
            fixed(y_scale, 4, bad, s);
            s.push(')');
            if rotate_truthy {
                s.push_str(" rotate(");
                number_to_string(rotate, s);
                s.push(')');
            }
            s.push_str("\">");
            esc_text(chs, s);
            s.push_str("</text>");
            if !props.unselectable {
                parts.spans.push_str("<tspan class=\"hierojax-svg-hidden\">");
                esc_text(chs, &mut parts.spans);
                parts.spans.push_str("</tspan>");
            }
        } else {
            let s = &mut parts.spans;
            s.push_str("<tspan class=\"");
            s.push_str(if props.unselectable { "hierojax-svg-visual" } else { "hierojax-svg-sign" });
            s.push_str("\" x=\"");
            // Upstream's `x` and `dx`, added here rather than each rounded and added by the browser.
            let a = -meas.width / 2.0;
            unit(if u.reversed { a - x_trans } else { a + x_trans }, bad, s);
            s.push_str("\" y=\"");
            unit(meas.height / 2.0 + y_trans, bad, s);
            s.push_str("\" font-size=\"");
            unit(font_size, bad, s);
            s.push('"');
            push_fill(s, color);
            s.push('>');
            esc_text(chs, s);
            s.push_str("</tspan>");
        }
        Ok(())
    }

    /// `PrintedSVG.addShading`, without rounding to pixels. So that two shadings that touch have
    /// no seam at a fractional boundary, the browser snaps their edges (`crispEdges`). A shading
    /// with no area is left out (upstream prints a negative width, which SVG refuses).
    fn shade(&mut self, u: &Unit, parts: &mut Parts, r: &Rect) {
        let bad = &mut self.bad;
        let x = if u.reversed { u.w - (r.x + r.w) } else { r.x };
        if !(r.w > 0.0 && r.h > 0.0) {
            if r.w.is_nan() || r.h.is_nan() {
                *bad = true;
            }
            return;
        }
        parts.shaded = true;
        let mut s = String::from("<rect");
        if self.opts.uniform_shade {
            s.push_str(" class=\"hierojax-svg-uniform\"");
        } else {
            let offset = (u.w_accum + u.h_accum) % HATCH_PERIOD;
            let key = offset as u32;
            s.push_str(" fill=\"url(#hierojax-em-hatch");
            number_to_string(f64::from(key), &mut s);
            s.push_str(")\"");
            if parts.patterns.insert(key) {
                let mut d = String::from("<defs><pattern id=\"hierojax-em-hatch");
                number_to_string(f64::from(key), &mut d);
                d.push_str("\" patternUnits=\"userSpaceOnUse\" width=\"");
                number_to_string(HATCH_PERIOD, &mut d);
                d.push_str("\" height=\"");
                number_to_string(HATCH_PERIOD, &mut d);
                d.push_str("\" patternTransform=\"translate(");
                number_to_string(-f64::from(key), &mut d);
                d.push_str(" 0)\"><path d=\"");
                d.push_str(HATCH_PATH);
                d.push_str("\" class=\"hierojax-svg-hatching\" style=\"");
                d.push_str(HATCH_STROKE);
                d.push_str("\"></path></pattern></defs>");
                parts.front.push(d);
            }
        }
        s.push_str(" x=\"");
        unit(EM_UNITS * x, bad, &mut s);
        s.push_str("\" y=\"");
        unit(EM_UNITS * r.y, bad, &mut s);
        s.push_str("\" width=\"");
        unit(EM_UNITS * r.w, bad, &mut s);
        s.push_str("\" height=\"");
        unit(EM_UNITS * r.h, bad, &mut s);
        s.push_str("\"></rect>");
        parts.front.push(s);
    }
}

fn push_fill(s: &mut String, color: &str) {
    if !color.is_empty() {
        s.push_str(" fill=\"");
        esc_attr(color, s);
        s.push('"');
    }
}

/// Print a plan in em. `css_font_px` is what [`Options::consistent`] gave.
pub fn print_em<Ra: Raster>(
    eng: &mut Engine,
    raster: &mut Ra,
    plan: &Plan,
    opts: &Options,
    css_font_px: Option<f64>,
) -> Result<SvgOutput, LayoutError> {
    let mut p = Printer { eng, raster, opts, plan, css_font_px, bad: false, logs: Vec::new() };
    let mut markup = String::new();
    for u in &plan.units {
        p.unit(u, &mut markup)?;
    }
    if p.bad {
        return Err(LayoutError::NotFinite);
    }
    Ok(SvgOutput { markup, logs: p.logs })
}

/// Lay out and print one fragment in consistent mode. The engine must be in [`Mode::Consistent`],
/// and the raster one whose numbers are the same everywhere; any other gives an error.
pub fn render_consistent<Ra: Raster>(
    engine: &mut Engine,
    tree: &Tree,
    raw: &RawOptions,
    raster: &mut Ra,
) -> Result<Rendered, LayoutError> {
    if engine.mode() != Mode::Consistent {
        return Err(LayoutError::Internal("not an engine in consistent mode"));
    }
    let (opts, css_font_px) = Options::consistent(raw);
    let plan = {
        let mut layouter = Layouter::new(tree, engine, raster);
        layouter.plan(&opts)?
    };
    let svg = print_em(engine, raster, &plan, &opts, css_font_px)?;
    let mut container = plan.container;
    // In separated mode the element gets the size, in the same units as its `svg`s.
    if let Some((w, h)) = container.size_em.take() {
        container.style_width = Some(em_length(w, css_font_px)?);
        container.style_height = Some(em_length(h, css_font_px)?);
    }
    Ok(Rendered { container, svg })
}

fn em_length(em: f64, css_font_px: Option<f64>) -> Result<String, LayoutError> {
    let mut s = String::new();
    let mut bad = false;
    match css_font_px {
        // `data-fontsize` is on the `svg`s, not on the element that takes this size.
        Some(px) => {
            fixed(em * px, 4, &mut bad, &mut s);
            s.push_str("px");
        }
        None => {
            fixed(em, 4, &mut bad, &mut s);
            s.push_str("em");
        }
    }
    if bad { Err(LayoutError::NotFinite) } else { Ok(s) }
}
