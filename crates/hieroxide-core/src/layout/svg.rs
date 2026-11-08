//! `PrintedSVG`: turns a [`Plan`] into the markup of the SVG upstream builds with DOM
//! calls.
//!
//! Upstream creates elements with `createElementNS` and `setAttribute`; this emits one
//! string for the host to parse with `insertAdjacentHTML` (the HTML parser puts `<svg>`
//! and its children in the SVG namespace). What the string parses to must be what the DOM
//! calls build, so:
//!
//! * every attribute value is the string `setAttribute` would receive (numbers as JS
//!   prints them);
//! * `style` declarations carry the strings given to `style.setProperty`, which the
//!   browser's CSS parser normalizes the same way in both routes;
//! * children are in upstream's order: shadings and `defs` are inserted before the first
//!   child, so the most recent comes first; then the optional `style`; then the `text`
//!   holding the `tspan`s; then the `text` elements of transformed signs, in call order;
//! * text goes through the HTML parser, so `&`, `<` and CR are written as references. The
//!   embedded stylesheet is the exception: upstream sets it with `innerHTML`, which parses
//!   it as markup too, so it is written raw.
//!
//! Measurement (`correctedMeasurement`) happens here, in the order of the plan, because
//! that is where upstream does it.

use std::collections::HashSet;

use super::options::Options;
use super::plan::{Item, Plan, Rect, Unit};
use super::raster::{Raster, RasterError};
use super::shapes::{Engine, SignProps};
use super::LayoutError;
use crate::jsnum::{number_from_str, number_to_string, js_round, to_fixed};

/// `document.getElementById(id)`, for the shade patterns.
pub trait DocIds {
    fn has_id(&mut self, id: &str) -> Result<bool, RasterError>;
}

/// No element has any id: for tests and for printing outside a document.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoIds;

impl DocIds for NoIds {
    fn has_id(&mut self, _id: &str) -> Result<bool, RasterError> {
        Ok(false)
    }
}

/// The markup, and the messages to pass to `console.error` (for `options.log`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SvgOutput {
    pub markup: String,
    pub logs: Vec<String>,
}

pub(super) fn esc_attr(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\r' => out.push_str("&#13;"),
            _ => out.push(c),
        }
    }
}

pub(super) fn esc_text(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '\r' => out.push_str("&#13;"),
            _ => out.push(c),
        }
    }
}

/// `-s` for a numeric string `s`, as a string (`'' + -s`).
fn neg_str(s: &str) -> String {
    let mut out = String::new();
    number_to_string(-number_from_str(s), &mut out);
    out
}

fn fixed2(x: f64) -> String {
    let mut s = String::new();
    to_fixed(x, 2, &mut s);
    s
}

fn num(x: f64) -> String {
    let mut s = String::new();
    number_to_string(x, &mut s);
    s
}

/// ` style="..."` for `style.setProperty('font-size', s + 'px')` and the same for
/// `line-height`. Those calls ignore a value the CSS parser refuses, and a negative length
/// or `NaNpx` is one, and then the element has no `style` attribute at all; written into
/// markup, the attribute would be there, empty once parsed.
fn push_font_style(out: &mut String, size: &str) {
    let n = number_from_str(size);
    if n.is_finite() && n >= 0.0 {
        out.push_str(" style=\"font-size: ");
        out.push_str(size);
        out.push_str("px; line-height: ");
        out.push_str(size);
        out.push_str("px\"");
    }
}

/// `PrintedSVG.internalCSS()`.
pub(super) fn internal_css(font: &str) -> String {
    let mut s = String::from("\n@font-face {\n\tfont-family: 'NewGardiner';\n\tsrc: url('");
    s.push_str(font);
    s.push_str(
        "') format('opentype');\n}\n\
.hierojax-svg-sign {\n\tfont-family: NewGardiner;\n}\n\
.hierojax-svg-visual {\n\tfont-family: NewGardiner;\n\tuser-select: none;\n\t-webkit-user-select: none;\n\t-moz-user-select: none;\n\t-ms-user-select: none;\n}\n\
.hierojax-svg-hatching {\n\tstroke: gray;\n\tstroke-width: 1;\n}\n\
.hierojax-svg-uniform {\n\tfill: darkgray;\n}\n\
.hierojax-svg-hidden {\n\tfill-opacity: 0;\n\tstroke-opacity: 0;\n\tcolor: transparent;\n\tfont-size: 0;\n\tuser-select: all;\n}",
    );
    s
}

struct Printer<'a, Ra: Raster, D: DocIds> {
    eng: &'a mut Engine,
    raster: &'a mut Ra,
    doc: &'a mut D,
    opts: &'a Options,
    plan: &'a Plan,
    /// Whether the element is in the document, so that the `svg` it appends is found by
    /// `document.getElementById`.
    attached: bool,
    /// Pattern ids this fragment has created.
    created: HashSet<String>,
    logs: Vec<String>,
}

/// One `PrintedSVG`.
struct Svg {
    font: f64,
    /// `this.w`, in em (for `reverse`)
    w: f64,
    reversed: bool,
    w_accum: f64,
    h_accum: f64,
    /// Shadings and defs, in the order they were inserted before the first child.
    front: Vec<String>,
    /// The `tspan`s of `this.spans`.
    spans: String,
    /// Signs appended to the `svg`.
    signs: String,
}

impl<Ra: Raster, D: DocIds> Printer<'_, Ra, D> {
    fn unit(&mut self, unit: &Unit, out: &mut String) -> Result<(), LayoutError> {
        let font = self.opts.layout.fontsize;
        let mut svg = Svg {
            font,
            w: unit.w,
            reversed: unit.reversed,
            w_accum: unit.w_accum,
            h_accum: unit.h_accum,
            front: Vec::new(),
            spans: String::new(),
            signs: String::new(),
        };
        // An exception stops the calls where they are, and what they had built stays in the
        // element: the `svg` was appended by the constructor, before any sign.
        let mut failure = None;
        for item in &unit.items {
            let r = match item {
                Item::Sign { ch, scale, x_scale, y_scale, rotate, mirror, rect, props } => {
                    self.add_sign(&mut svg, *ch, *scale, *x_scale, *y_scale, *rotate, *mirror, rect, props)
                }
                Item::Hidden { start, len } => {
                    let s = self.plan.str(*start, *len);
                    add_hidden(&mut svg, s);
                    Ok(())
                }
                Item::Shade(r) => self.add_shading(&mut svg, r),
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
        // The constructor: svg with width and height (Math.ceil of the pixel size).
        out.push_str("<svg width=\"");
        out.push_str(&num((font * unit.w).ceil()));
        out.push_str("\" height=\"");
        out.push_str(&num((font * unit.h).ceil()));
        out.push('"');
        if self.opts.border && !self.opts.layout.separated {
            out.push_str(" class=\"hierojax-border\"");
        }
        out.push('>');
        for f in svg.front.iter().rev() {
            out.push_str(f);
        }
        if self.opts.standalone {
            out.push_str("<style type=\"text/css\">");
            out.push_str(&internal_css(&self.eng.external_font));
            out.push_str("</style>");
        }
        out.push_str("<text x=\"0\" y=\"0\"");
        if unit.reversed {
            out.push_str(" transform=\"scale(-1,1)\"");
        }
        out.push('>');
        out.push_str(&svg.spans);
        out.push_str("</text>");
        out.push_str(&svg.signs);
        out.push_str("</svg>");
        match failure {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// `PrintedSVG.addSign`
    #[allow(clippy::too_many_arguments)]
    fn add_sign(
        &mut self,
        svg: &mut Svg,
        ch: char,
        scale: f64,
        x_scale: f64,
        y_scale: f64,
        rotate: f64,
        mirror: bool,
        rect: &Rect,
        props: &SignProps,
    ) -> Result<(), LayoutError> {
        // reverse(rect.x, rect.w)
        let x = if svg.reversed { svg.w - (rect.x + rect.w) } else { rect.x };
        let mirror_any = svg.reversed != mirror; // this.reversed ^ mirror
        let font_size = svg.font * scale;
        let font_size_str = fixed2(font_size);
        let x_px = svg.font * x;
        let y_px = svg.font * rect.y;
        let x_scale_str = fixed2(x_scale);
        let y_scale_str = fixed2(y_scale);
        let meas = self.eng.corrected_measurement(
            self.raster, ch, font_size, x_scale, y_scale, rotate, mirror_any, props,
        )?;
        let x_trans = fixed2(x_px + meas.width_scaled / 2.0 - meas.x);
        let y_trans = fixed2(y_px - meas.height_scaled / 2.0 + meas.h - meas.y);
        let color = if props.bracket { &self.opts.bracketcolor } else { &self.opts.signcolor };
        let rotate_truthy = rotate != 0.0 && !rotate.is_nan();
        // Upstream quirk: this tests the raw `mirror`, not `mirror_any` (reversed ^ mirror).
        if rotate_truthy || mirror || x_scale != 1.0 || y_scale != 1.0 {
            let mut t = String::new();
            t.push_str("translate(");
            t.push_str(&x_trans);
            t.push(' ');
            t.push_str(&y_trans);
            t.push_str(") scale(");
            // (mirror_any ? -xScaleStr : xScaleStr): a number for -, the string otherwise.
            if mirror_any {
                t.push_str(&neg_str(&x_scale_str));
            } else {
                t.push_str(&x_scale_str);
            }
            t.push(' ');
            t.push_str(&y_scale_str);
            t.push_str(") ");
            if rotate_truthy {
                t.push_str("rotate(");
                number_to_string(rotate, &mut t);
                t.push_str(") ");
            }
            let s = &mut svg.signs;
            s.push_str("<text class=\"hierojax-svg-visual\" x=\"");
            s.push_str(&fixed2(-meas.width / 2.0));
            s.push_str("\" y=\"");
            s.push_str(&fixed2(meas.height / 2.0));
            s.push('"');
            push_font_style(s, &font_size_str);
            s.push_str(" fill=\"");
            esc_attr(color, s);
            s.push_str("\" transform=\"");
            s.push_str(&t);
            s.push_str("\">");
            esc_text(ch.encode_utf8(&mut [0; 4]), s);
            s.push_str("</text>");
            if !props.unselectable {
                add_hidden(svg, ch.encode_utf8(&mut [0; 4]));
            }
        } else {
            let s = &mut svg.spans;
            s.push_str("<tspan class=\"");
            s.push_str(if props.unselectable { "hierojax-svg-visual" } else { "hierojax-svg-sign" });
            s.push_str("\" x=\"");
            s.push_str(&fixed2(-meas.width / 2.0));
            s.push_str("\" y=\"");
            s.push_str(&fixed2(meas.height / 2.0));
            s.push('"');
            push_font_style(s, &font_size_str);
            s.push_str(" fill=\"");
            esc_attr(color, s);
            s.push_str("\" dx=\"");
            if svg.reversed {
                s.push_str(&neg_str(&x_trans));
            } else {
                s.push_str(&x_trans);
            }
            s.push_str("\" dy=\"");
            s.push_str(&y_trans);
            s.push_str("\">");
            esc_text(ch.encode_utf8(&mut [0; 4]), s);
            s.push_str("</tspan>");
        }
        Ok(())
    }

    /// `PrintedSVG.addShading`
    fn add_shading(&mut self, svg: &mut Svg, r: &Rect) -> Result<(), LayoutError> {
        let x = if svg.reversed { svg.w - (r.x + r.w) } else { r.x };
        let x_min = js_round(svg.font * x);
        let y_min = js_round(svg.font * r.y);
        let x_max = js_round(svg.font * (x + r.w));
        let y_max = js_round(svg.font * (r.y + r.h));
        let width = x_max - x_min;
        let height = y_max - y_min;
        let mut shade = String::from("<rect");
        if self.opts.uniform_shade {
            shade.push_str(" class=\"hierojax-svg-uniform\"");
        } else {
            let offset = (svg.w_accum + svg.h_accum) % 4.0;
            let id = self.shade_pattern(svg, offset)?;
            shade.push_str(" fill=\"url(#");
            esc_attr(&id, &mut shade);
            shade.push_str(")\"");
        }
        shade.push_str(" x=\"");
        shade.push_str(&num(x_min));
        shade.push_str("\" y=\"");
        shade.push_str(&num(y_min));
        shade.push_str("\" width=\"");
        shade.push_str(&num(width));
        shade.push_str("\" height=\"");
        shade.push_str(&num(height));
        shade.push_str("\"></rect>");
        svg.front.push(shade);
        Ok(())
    }

    /// `PrintedSVG.shadePattern`: `document.getElementById(id)` is document-global, so a
    /// pattern an earlier fragment (or an earlier part of this one) left in the document
    /// is reused.
    fn shade_pattern(&mut self, svg: &mut Svg, offset: f64) -> Result<String, LayoutError> {
        let mut id = String::from("hierojax-svg-id");
        number_to_string(offset, &mut id);
        if self.attached && self.created.contains(&id) {
            return Ok(id);
        }
        if self.doc.has_id(&id)? {
            return Ok(id);
        }
        let mut d = String::from("<defs><pattern id=\"");
        esc_attr(&id, &mut d);
        d.push_str("\" patternUnits=\"userSpaceOnUse\" width=\"4\" height=\"4\" patternTransform=\"translate(");
        number_to_string(-offset, &mut d);
        d.push_str(" 0)\"><path d=\"M-1,1 l2,-2 M0,4 l4,-4 M3,5 l2,-2\" class=\"hierojax-svg-hatching\"></path></pattern></defs>");
        svg.front.push(d);
        self.created.insert(id.clone());
        Ok(id)
    }
}

/// `PrintedSVG.addHidden`
fn add_hidden(svg: &mut Svg, s: &str) {
    svg.spans.push_str("<tspan class=\"hierojax-svg-hidden\">");
    esc_text(s, &mut svg.spans);
    svg.spans.push_str("</tspan>");
}

/// Print a plan as SVG. `attached` says whether the element the markup goes into is in
/// the document (`element.getRootNode() === document`).
pub fn print_svg<Ra: Raster, D: DocIds>(
    eng: &mut Engine,
    raster: &mut Ra,
    doc: &mut D,
    plan: &Plan,
    opts: &Options,
    attached: bool,
) -> Result<SvgOutput, LayoutError> {
    let (out, failure) = print_svg_partial(eng, raster, doc, plan, opts, attached);
    match failure {
        Some(e) => Err(e),
        None => Ok(out),
    }
}

/// [`print_svg`], which on an exception also gives what upstream would have left in the
/// element by then.
pub fn print_svg_partial<Ra: Raster, D: DocIds>(
    eng: &mut Engine,
    raster: &mut Ra,
    doc: &mut D,
    plan: &Plan,
    opts: &Options,
    attached: bool,
) -> (SvgOutput, Option<LayoutError>) {
    let mut p = Printer { eng, raster, doc, opts, plan, attached, created: HashSet::new(), logs: Vec::new() };
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
