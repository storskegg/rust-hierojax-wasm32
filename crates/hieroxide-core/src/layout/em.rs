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

use super::options::{Options, RawOptions, EM_UNITS};
use super::plan::{Container, Item, Plan, Rect, Unit};
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
pub(super) fn fixed(x: f64, decimals: u32, bad: &mut bool, out: &mut String) {
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
pub(super) fn unit(x: f64, bad: &mut bool, out: &mut String) {
    fixed(x, 1, bad, out);
}

/// One sign, measured and placed: what every printer of consistent mode needs of it. Units are
/// those of the `viewBox` ([`EM_UNITS`] to the em).
///
/// The sign's outline is placed by `translate(x_trans y_trans) scale(sx sy) rotate(rotate)`, and
/// the glyph's origin is at `(-width / 2, height / 2)` of that frame, `font_size` its size.
/// `sx` is `-x_scale` if `mirror_any`.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedSign {
    pub ch: char,
    pub props: SignProps,
    pub font_size: f64,
    pub x_scale: f64,
    pub y_scale: f64,
    pub rotate: f64,
    /// The sign's own mirror.
    pub mirror: bool,
    /// Mirrored on the page: its own mirror, or a right-to-left unit's.
    pub mirror_any: bool,
    pub x_trans: f64,
    pub y_trans: f64,
    /// The measured width and height of the glyph's ink.
    pub width: f64,
    pub height: f64,
    /// The sign's place in its unit, the box layout gave it, in units; mirrored in a right-to-left unit.
    pub rect: Rect,
}

impl PlacedSign {
    /// Whether the sign has a transform of its own (a `text` of its own in the text printer; the
    /// others are a `tspan` of the unit's one).
    pub fn transformed(&self) -> bool {
        (self.rotate != 0.0 && !self.rotate.is_nan()) || self.mirror || self.x_scale != 1.0 || self.y_scale != 1.0
    }
    /// The horizontal scale as printed: negative when mirrored on the page.
    pub fn sx(&self) -> f64 {
        if self.mirror_any { -self.x_scale } else { self.x_scale }
    }
}

/// A plan with every sign measured: `signs` are in the order of the plan's `Item::Sign`s.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub plan: Plan,
    pub signs: Vec<PlacedSign>,
}

/// Measure one `Item::Sign` of `u`: its place on the page, in units.
fn place_one<Ra: Raster>(eng: &mut Engine, raster: &mut Ra, u: &Unit, item: &Item) -> Result<PlacedSign, LayoutError> {
    let Item::Sign { ch, scale, x_scale, y_scale, rotate, mirror, rect, props } = item else {
        return Err(LayoutError::Internal("not a sign"));
    };
    let x = if u.reversed { u.w - (rect.x + rect.w) } else { rect.x };
    let mirror_any = u.reversed != *mirror;
    let font_size = EM_UNITS * scale;
    let x_u = EM_UNITS * x;
    let y_u = EM_UNITS * rect.y;
    let meas = eng.corrected_measurement(raster, *ch, font_size, *x_scale, *y_scale, *rotate, mirror_any, props)?;
    Ok(PlacedSign {
        ch: *ch,
        props: *props,
        font_size,
        x_scale: *x_scale,
        y_scale: *y_scale,
        rotate: *rotate,
        mirror: *mirror,
        mirror_any,
        x_trans: x_u + meas.width_scaled / 2.0 - meas.x,
        y_trans: y_u - meas.height_scaled / 2.0 + meas.h - meas.y,
        width: meas.width,
        height: meas.height,
        rect: Rect { x: x_u, y: y_u, w: EM_UNITS * rect.w, h: EM_UNITS * rect.h },
    })
}

/// Measure every sign of a plan, in its order (the engine's memo is filled in that order), and keep
/// the result, for printing more than once or in more than one way.
pub fn place<Ra: Raster>(eng: &mut Engine, raster: &mut Ra, plan: Plan) -> Result<Placed, LayoutError> {
    let mut signs = Vec::new();
    for u in &plan.units {
        for item in &u.items {
            if matches!(item, Item::Sign { .. }) {
                signs.push(place_one(eng, raster, u, item)?);
            }
        }
    }
    Ok(Placed { plan, signs })
}

/// Where the printer gets each sign's place: by measuring it as it goes, or from a [`Placed`].
trait SignSource {
    fn next(&mut self, u: &Unit, item: &Item) -> Result<PlacedSign, LayoutError>;
}

struct Live<'a, Ra: Raster> {
    eng: &'a mut Engine,
    raster: &'a mut Ra,
}

impl<Ra: Raster> SignSource for Live<'_, Ra> {
    fn next(&mut self, u: &Unit, item: &Item) -> Result<PlacedSign, LayoutError> {
        place_one(self.eng, self.raster, u, item)
    }
}

struct Stored<'a> {
    signs: &'a [PlacedSign],
    at: usize,
}

impl SignSource for Stored<'_> {
    fn next(&mut self, _: &Unit, _: &Item) -> Result<PlacedSign, LayoutError> {
        let sign = self.signs.get(self.at).ok_or(LayoutError::Internal("a sign was not placed"))?;
        self.at += 1;
        Ok(sign.clone())
    }
}

struct Printer<'a, S: SignSource> {
    opts: &'a Options,
    plan: &'a Plan,
    signs: S,
    font: (&'a str, &'a str),
    css_font_px: Option<f64>,
    bad: bool,
    logs: Vec<String>,
    /// The fragment as the source has it, as characters: what the hidden text copies.
    copy: Vec<char>,
    copied: usize,
    /// The plan's copied text and the source differ in length (no parsed fragment does that): the
    /// text is what the plan wrote.
    whole: bool,
}

impl<S: SignSource> Printer<'_, S> {
    /// The characters of the copied text that stand in place of `s`: as many characters of the source,
    /// from where the last call stopped. The tree writes an enclosure's end by its opening and a lost
    /// sign's selector as U+FE00, whichever the source had; what a selection copies is the source.
    fn take(&mut self, s: &str) -> String {
        if self.whole {
            return s.to_string();
        }
        let n = s.chars().count();
        let from = self.copied.min(self.copy.len());
        let to = (self.copied + n).min(self.copy.len());
        self.copied += n;
        self.copy[from..to].iter().collect()
    }
}

/// What one `svg` collects before it is written.
#[derive(Default)]
struct Parts {
    front: Vec<String>,
    patterns: Vec<u32>,
    shaded: bool,
    spans: String,
    signs: String,
}

impl<S: SignSource> Printer<'_, S> {
    fn unit(&mut self, u: &Unit, out: &mut String) -> Result<(), LayoutError> {
        let mut parts = Parts::default();
        for item in &u.items {
            match item {
                Item::Sign { .. } => {
                    let sign = self.signs.next(u, item)?;
                    self.sign(u, &mut parts, &sign);
                }
                Item::Hidden { start, len } => {
                    let plan = self.plan;
                    let s = plan.str(*start, *len);
                    if !s.is_empty() {
                        let t = self.take(s);
                        parts.spans.push_str("<tspan class=\"hierojax-svg-hidden\">");
                        esc_text(&t, &mut parts.spans);
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
            out.push_str(&internal_css(self.font.0, self.font.1));
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
    fn sign(&mut self, u: &Unit, parts: &mut Parts, p: &PlacedSign) {
        // The copied character of a selectable sign: the source's.
        let copied = if p.props.unselectable { String::new() } else { self.take(p.ch.encode_utf8(&mut [0u8; 4])) };
        let bad = &mut self.bad;
        let color = if p.props.bracket { self.opts.bracketcolor.as_str() } else { self.opts.signcolor.as_str() };
        let mut chb = [0u8; 4];
        let chs: &str = p.ch.encode_utf8(&mut chb);
        if p.transformed() {
            let s = &mut parts.signs;
            s.push_str("<text class=\"hierojax-svg-visual\" x=\"");
            unit(-p.width / 2.0, bad, s);
            s.push_str("\" y=\"");
            unit(p.height / 2.0, bad, s);
            s.push_str("\" font-size=\"");
            unit(p.font_size, bad, s);
            s.push('"');
            push_fill(s, color);
            s.push_str(" transform=\"translate(");
            unit(p.x_trans, bad, s);
            s.push(' ');
            unit(p.y_trans, bad, s);
            s.push_str(") scale(");
            fixed(p.sx(), 4, bad, s);
            s.push(' ');
            fixed(p.y_scale, 4, bad, s);
            s.push(')');
            if p.rotate != 0.0 && !p.rotate.is_nan() {
                s.push_str(" rotate(");
                number_to_string(p.rotate, s);
                s.push(')');
            }
            s.push_str("\">");
            esc_text(chs, s);
            s.push_str("</text>");
            if !p.props.unselectable {
                parts.spans.push_str("<tspan class=\"hierojax-svg-hidden\">");
                esc_text(&copied, &mut parts.spans);
                parts.spans.push_str("</tspan>");
            }
        } else {
            let s = &mut parts.spans;
            s.push_str("<tspan class=\"");
            s.push_str(if p.props.unselectable { "hierojax-svg-visual" } else { "hierojax-svg-sign" });
            s.push_str("\" x=\"");
            // Upstream's `x` and `dx`, added here rather than each rounded and added by the browser.
            let a = -p.width / 2.0;
            unit(if u.reversed { a - p.x_trans } else { a + p.x_trans }, bad, s);
            s.push_str("\" y=\"");
            unit(p.height / 2.0 + p.y_trans, bad, s);
            s.push_str("\" font-size=\"");
            unit(p.font_size, bad, s);
            s.push('"');
            push_fill(s, color);
            s.push('>');
            esc_text(chs, s);
            s.push_str("</tspan>");
        }
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
            if !parts.patterns.contains(&key) {
                parts.patterns.push(key);
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

/// Print a placed plan in em, as text. `css_font_px` is what [`Options::consistent`] gave; `font` is
/// the URL and format of the font for `Options::standalone`; `source` is the fragment as written, which
/// the hidden text copies character for character.
pub fn write_text(
    placed: &Placed,
    opts: &Options,
    css_font_px: Option<f64>,
    font: (&str, &str),
    source: &str,
) -> Result<SvgOutput, LayoutError> {
    print(&placed.plan, Stored { signs: &placed.signs, at: 0 }, opts, css_font_px, font, source)
}

/// Measure a plan as it prints it, and print it in em, as text.
pub fn print_em<Ra: Raster>(
    eng: &mut Engine,
    raster: &mut Ra,
    plan: &Plan,
    opts: &Options,
    css_font_px: Option<f64>,
    source: &str,
) -> Result<SvgOutput, LayoutError> {
    // Only a standalone `svg` says which font; the printer holds the engine otherwise.
    let font = if opts.standalone { (eng.external_font.clone(), eng.external_font_format.clone()) } else { Default::default() };
    print(plan, Live { eng, raster }, opts, css_font_px, (&font.0, &font.1), source)
}

fn print<S: SignSource>(
    plan: &Plan,
    signs: S,
    opts: &Options,
    css_font_px: Option<f64>,
    font: (&str, &str),
    source: &str,
) -> Result<SvgOutput, LayoutError> {
    let copy: Vec<char> = source.chars().collect();
    // The characters the plan would have copied: the hidden text and one for each selectable sign.
    let mut n = 0usize;
    for u in &plan.units {
        for item in &u.items {
            match item {
                Item::Hidden { start, len } => n += plan.str(*start, *len).chars().count(),
                Item::Sign { props, .. } if !props.unselectable => n += 1,
                _ => {}
            }
        }
    }
    let whole = n != copy.len();
    let mut p = Printer { opts, plan, signs, font, css_font_px, bad: false, logs: Vec::new(), copy, copied: 0, whole };
    let mut markup = String::new();
    for u in &plan.units {
        p.unit(u, &mut markup)?;
    }
    if p.bad {
        return Err(LayoutError::NotFinite);
    }
    Ok(SvgOutput { markup, logs: p.logs })
}

/// Lay out and print one fragment in consistent mode; `source` is the text the tree was parsed from. The engine must be in [`Mode::Consistent`],
/// and the raster one whose numbers are the same everywhere; any other gives an error.
pub fn render_consistent<Ra: Raster>(
    engine: &mut Engine,
    tree: &Tree,
    raw: &RawOptions,
    raster: &mut Ra,
    source: &str,
) -> Result<Rendered, LayoutError> {
    if engine.mode() != Mode::Consistent {
        return Err(LayoutError::Internal("not an engine in consistent mode"));
    }
    let (opts, css_font_px) = Options::consistent(raw);
    let plan = {
        let mut layouter = Layouter::new(tree, engine, raster);
        layouter.plan(&opts)?
    };
    let svg = print_em(engine, raster, &plan, &opts, css_font_px, source)?;
    let container = sized_container(plan.container, css_font_px)?;
    Ok(Rendered { container, svg, canvas: None })
}

/// What `Fragment.print` does to the element: in separated mode it gets the size, in the same
/// units as its `svg`s.
pub fn sized_container(mut container: Container, css_font_px: Option<f64>) -> Result<Container, LayoutError> {
    if let Some((w, h)) = container.size_em.take() {
        container.style_width = Some(em_length(w, css_font_px)?);
        container.style_height = Some(em_length(h, css_font_px)?);
    }
    Ok(container)
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
