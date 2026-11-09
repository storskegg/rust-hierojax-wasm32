//! The path printer: the same placed plan as consistent mode's text, drawn as `<path>`s from the
//! font's outlines. Nothing in the markup needs the font, a stylesheet or a script.
//!
//! **Placement.** Each sign is one `path` whose `d` is the glyph's outline in *font units* (y up,
//! relative commands, integers in NewGardiner), and whose `transform` is the chain consistent mode
//! prints, then the glyph's own scale:
//!
//! ```text
//! translate(x_trans y_trans) scale(sx sy) rotate(r) translate(-w/2 h/2) scale(k -k)
//! ```
//!
//! Every number but `k` (the font size over the units per em) is one the text printer prints, in
//! the same words (`<text x y transform>`), and a sign with no transform of its own is
//! `translate(x y) scale(k -k)` with the `x` and `y` of its `tspan`. So the two printers place
//! signs by the same numbers (`tests/paths.rs` compares them for the whole corpus), and a glyph's
//! `d` is the same text wherever and however large it is placed, which a compressor finds.
//!
//! **Shading** is geometry: a uniform shading is a `rect`, and hatching is the bands of upstream's
//! pattern (`x + y + key` a multiple of 200, 50 wide) clipped to the shaded rectangle by
//! calculation. No `defs`, no `id`, no `url(#...)`.
//!
//! **Copyable text.** See [`PathParams`]. The invisible text of a line of signs is one run in a tiny font whose
//! `letter-spacing` puts each sign's text where the sign is drawn (`text_run`): the browser sees one line of
//! text, so a double or triple click and a drag across or from beside the block select as they do on upstream's
//! text, and its highlight is one band. The `svg` is `user-select:none`, so a drag that starts beside the block ends
//! in the text and not in the drawing. A column or a block of several lines keeps a box per sign.
//!
//! **Contents.** A page that has its own `<span class="hierojax">` (with its own CSS) wants the
//! block inside that element: `PathParams::contents`. What is written is then the `svg`, with
//! `role`, `aria-label` and `data-text`, and before it, with `Selectable::Overlay`, the invisible
//! text. The text's box is not `100%` of the page's element (which may be a fixed box that centres
//! the `svg`): it has the `svg`'s own size in em and no `left` or `top`, so it takes its *static
//! position*, which is where the `svg` goes whatever the element's display (a block, an inline
//! block, a flex box that centres) and direction. It comes first, so that in an inline block it
//! starts where the `svg` starts. It sets a generic `font-family`, so that a page that names a web
//! font for the element does not make the browser fetch it for text nobody sees.

use super::em::{fixed, unit, Placed, PlacedSign};
use super::options::{Options, EM_UNITS};
use super::plan::{Item, Rect, Unit};
use super::svg::{esc_attr, esc_text, SvgOutput};
use super::LayoutError;
use crate::jsnum::{number_from_str, number_to_string};

/// One path command of an outline, in font units with y up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    Move(f32, f32),
    Line(f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

/// Where the path printer reads outlines from.
pub trait Outlines {
    fn units_per_em(&self) -> f64;
    /// The outline of `ch`, or `None` for a sign the font does not have.
    fn outline(&self, ch: char) -> Option<&[Seg]>;
}

/// What the path printer needs besides the options of layout.
#[derive(Clone, Debug)]
pub struct PathParams<'a> {
    /// `aria-label` of the block.
    pub label: &'a str,
    /// What a selection of the page copies, and how.
    pub selectable: Selectable,
    /// Whether to write the `data-text` attribute (on the outer element of the block).
    pub data_text: bool,
    /// The fragment as the source has it: what is copied. The printer takes each character of the
    /// copied text from it (the tree writes an enclosure's end by its opening, whichever was
    /// written), and when the lengths differ, which no parsed fragment does, writes all of it as one
    /// run in the first unit.
    pub source: &'a str,
    /// `xmlns` on each `svg`, so that the markup is also an SVG file.
    pub xmlns: bool,
    /// The `style` attribute (`vertical-align`, the font size, a border). Without it, the block
    /// does not sit on the line as consistent mode's does, and `data-fontsize` and `data-border`
    /// have no effect: a page whose CSP forbids inline styles has to say these in its own CSS.
    pub inline_style: bool,
    /// The CSS font size in px that `data-fontsize` asked for.
    pub css_font_px: Option<f64>,
    /// Only what goes inside an element the page already has (see the module's **Contents**): no
    /// wrapper, nothing about the block's place on the line, and a font size is set on the `svg`
    /// and the invisible text, which have no wrapper to carry it. `border` has no effect (the
    /// page's element can have one). With `separated`, one `span` with no class and no
    /// `vertical-align` stays, which holds the `svg`s.
    pub contents: bool,
}

/// How a block carries the text that a selection of the page copies. It is invisible, and needs no
/// font: nothing is drawn from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selectable {
    /// Beside the `svg`, in HTML, inside a `span` that wraps both: the signs as one run of text in a tiny
    /// font (a line of signs), or a box over each sign (a column, several lines). Copies
    /// exactly the text in Chromium, Firefox and WebKit.
    Overlay,
    /// Inside the `svg`, as SVG text: the block is a bare `svg`. Chromium and WebKit put a line
    /// break before and after the text when a selection takes in the whole block.
    Svg,
    /// None: the block is a picture.
    Off,
}

/// What the `svg` of a block with an overlay says about selection.
const NO_SELECT: &str = "user-select:none;-webkit-user-select:none";

/// The font size of the invisible text run, as a share of the block's: see `text_run`.
const RUN_EPS: f64 = 0.01;

/// The attribute that holds the copyable text, on the outer element of the block.
pub const DATA_TEXT: &str = "data-text";

/// What `hierojax.css` says of the element that holds the `svg`: the baseline.
const ALIGN: &str = "vertical-align:-.1em";
const BORDER: &str = "border:solid 1px black";

/// The upstream hatching: period and the band's half width along `x + y`.
const HATCH_PERIOD: f64 = 200.0;
/// A stroke of 50 across a line at 45 degrees is 50 * sqrt(2) wide along `x + y`.
const HATCH_HALF: f64 = 25.0 * core::f64::consts::SQRT_2;
const HATCH_COLOR: &str = "gray";
const UNIFORM_COLOR: &str = "darkgray";

/// Coordinates of a glyph's `d` are on a grid of a tenth of a font unit (an integer in the font
/// that this is made for).
const D_SCALE: f64 = 10.0;

struct Printer<'a, O: Outlines> {
    opts: &'a Options,
    placed: &'a Placed,
    outlines: &'a O,
    params: &'a PathParams<'a>,
    bad: bool,
    logs: Vec<String>,
    next_sign: usize,
    /// The characters of the copyable text, and how many have been written.
    copy: Vec<char>,
    copied: usize,
    /// Whether the copyable text is written piece by piece, or (when it cannot be) whole.
    whole: bool,
}

/// `v` tenths as a number with at most one decimal, leading zero left out (`.5`, `-.5`).
fn tenths(v: i64, out: &mut String, prev_dot: &mut bool, first: bool) {
    let neg = v < 0;
    let a = v.unsigned_abs();
    let (int, frac) = (a / 10, a % 10);
    let mut s = String::new();
    if neg {
        s.push('-');
    }
    if int > 0 || frac == 0 {
        number_to_string(int as f64, &mut s);
    }
    if frac != 0 {
        s.push('.');
        s.push((b'0' + frac as u8) as char);
    }
    // A number needs a separator from the one before unless it starts with `-`, or with `.` when
    // the one before has its dot already.
    if !first {
        let starts = s.as_bytes()[0];
        let need = !(starts == b'-' || (starts == b'.' && *prev_dot));
        if need {
            out.push(' ');
        }
    }
    *prev_dot = s.contains('.');
    out.push_str(&s);
}

/// The outline as the `d` of a path, in font units to a tenth: see [`write_d`].
pub fn outline_d(segs: &[Seg]) -> String {
    let mut d = String::new();
    write_d(segs, &mut d);
    d
}

/// The outline as the `d` of a path: relative commands, each endpoint rounded once, so that no
/// error adds up along a contour.
fn write_d(segs: &[Seg], out: &mut String) {
    let r = |v: f32| (f64::from(v) * D_SCALE).round() as i64;
    let (mut cx, mut cy) = (0i64, 0i64);
    let (mut mx, mut my) = (0i64, 0i64);
    let mut last = ' ';
    let mut dot = false;
    let mut first = true;
    let cmd = |c: char, out: &mut String, last: &mut char, first: &mut bool| {
        if *last != c || c == 'z' || c == 'm' {
            out.push(c);
            *last = c;
            *first = true;
        }
    };
    for s in segs {
        match *s {
            Seg::Move(x, y) => {
                let (nx, ny) = (r(x), r(y));
                // The first `m` is read as absolute (the pen starts at 0 0), and what follows a pair of
                // an `m` is relative: an `M` here would make every implied lineto absolute.
                cmd('m', out, &mut last, &mut first);
                tenths(nx - cx, out, &mut dot, true);
                tenths(ny - cy, out, &mut dot, false);
                cx = nx;
                cy = ny;
                mx = nx;
                my = ny;
                // What follows a moveto's pair is a lineto.
                last = 'l';
                first = false;
            }
            Seg::Line(x, y) => {
                let (nx, ny) = (r(x), r(y));
                if ny == cy && nx != cx {
                    cmd('h', out, &mut last, &mut first);
                    tenths(nx - cx, out, &mut dot, first);
                } else if nx == cx && ny != cy {
                    cmd('v', out, &mut last, &mut first);
                    tenths(ny - cy, out, &mut dot, first);
                } else {
                    cmd('l', out, &mut last, &mut first);
                    tenths(nx - cx, out, &mut dot, first);
                    first = false;
                    tenths(ny - cy, out, &mut dot, first);
                }
                first = false;
                cx = nx;
                cy = ny;
            }
            Seg::Cubic(a, b, c, d, x, y) => {
                cmd('c', out, &mut last, &mut first);
                let pts = [r(a) - cx, r(b) - cy, r(c) - cx, r(d) - cy, r(x) - cx, r(y) - cy];
                for v in pts {
                    tenths(v, out, &mut dot, first);
                    first = false;
                }
                cx = r(x);
                cy = r(y);
            }
            Seg::Close => {
                cmd('z', out, &mut last, &mut first);
                cx = mx;
                cy = my;
                last = 'z';
            }
        }
    }
}

/// Clip a convex polygon to `a * x + b * y <= c`.
fn clip(poly: &[(f64, f64)], a: f64, b: f64, c: f64) -> Vec<(f64, f64)> {
    let mut out = Vec::with_capacity(poly.len() + 2);
    for i in 0..poly.len() {
        let p = poly[i];
        let q = poly[(i + 1) % poly.len()];
        let (dp, dq) = (a * p.0 + b * p.1 - c, a * q.0 + b * q.1 - c);
        if dp <= 0.0 {
            out.push(p);
        }
        if (dp < 0.0 && dq > 0.0) || (dp > 0.0 && dq < 0.0) {
            let t = dp / (dp - dq);
            out.push((p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t));
        }
    }
    out
}

impl<'a, O: Outlines> Printer<'a, O> {
    /// The text of one `translate(...)`-style transform for a sign, and nothing if it has none.
    fn transform(&mut self, u: &Unit, p: &PlacedSign, k: f64, out: &mut String) {
        let bad = &mut self.bad;
        out.push_str(" transform=\"");
        if p.transformed() {
            out.push_str("translate(");
            unit(p.x_trans, bad, out);
            out.push(' ');
            unit(p.y_trans, bad, out);
            out.push(')');
            if p.sx() != 1.0 || p.y_scale != 1.0 {
                out.push_str(" scale(");
                fixed(p.sx(), 4, bad, out);
                out.push(' ');
                fixed(p.y_scale, 4, bad, out);
                out.push(')');
            }
            if p.rotate != 0.0 && !p.rotate.is_nan() {
                out.push_str(" rotate(");
                number_to_string(p.rotate, out);
                out.push(')');
            }
            out.push_str(" translate(");
            unit(-p.width / 2.0, bad, out);
            out.push(' ');
            unit(p.height / 2.0, bad, out);
            out.push(')');
        } else {
            // The `x` and `y` of the `tspan`; in a unit that is reversed, the text printer's
            // `scale(-1,1)` mirrors the whole row, so the glyph is at `-x` with its `x` mirrored.
            let a = -p.width / 2.0;
            let x = if u.reversed { a - p.x_trans } else { a + p.x_trans };
            out.push_str("translate(");
            unit(if u.reversed { -x } else { x }, bad, out);
            out.push(' ');
            unit(p.height / 2.0 + p.y_trans, bad, out);
            out.push(')');
        }
        out.push_str(" scale(");
        let sk = if p.transformed() { k } else if u.reversed { -k } else { k };
        fixed(sk, 6, bad, out);
        out.push(' ');
        fixed(-k, 6, bad, out);
        out.push_str(")\"");
    }

    fn sign(&mut self, u: &Unit, p: &PlacedSign, out: &mut String) {
        let Some(segs) = self.outlines.outline(p.ch) else { return };
        let mut d = String::new();
        write_d(segs, &mut d);
        if d.is_empty() {
            return;
        }
        // The font size as the text printer prints it, to a tenth of a unit.
        let mut size = String::new();
        unit(p.font_size, &mut self.bad, &mut size);
        let k = number_from_str(&size) / self.outlines.units_per_em();
        out.push_str("<path");
        self.transform(u, p, k, out);
        let color = if p.props.bracket { self.opts.bracketcolor.as_str() } else { self.opts.signcolor.as_str() };
        if !color.is_empty() {
            out.push_str(" fill=\"");
            esc_attr(color, out);
            out.push('"');
        }
        out.push_str(" d=\"");
        out.push_str(&d);
        out.push_str("\"/>");
    }

    /// One shading as markup: a `rect`, or the bands of the hatching.
    fn shade(&mut self, u: &Unit, r: &Rect, out: &mut String) -> bool {
        let x = if u.reversed { u.w - (r.x + r.w) } else { r.x };
        if !(r.w > 0.0 && r.h > 0.0) {
            if r.w.is_nan() || r.h.is_nan() {
                self.bad = true;
            }
            return false;
        }
        let bad = &mut self.bad;
        let (x0, y0, w, h) = (EM_UNITS * x, EM_UNITS * r.y, EM_UNITS * r.w, EM_UNITS * r.h);
        if self.opts.uniform_shade {
            out.push_str("<rect x=\"");
            unit(x0, bad, out);
            out.push_str("\" y=\"");
            unit(y0, bad, out);
            out.push_str("\" width=\"");
            unit(w, bad, out);
            out.push_str("\" height=\"");
            unit(h, bad, out);
            out.push_str("\"/>");
            return true;
        }
        // Upstream's pattern is anchored at the unit and shifted by what was printed before it,
        // so that a row of units has one hatching: `(x + key) + y` a multiple of the period.
        let key = ((u.w_accum + u.h_accum) % HATCH_PERIOD) as u32;
        let key = f64::from(key);
        let rect = [(x0, y0), (x0 + w, y0), (x0 + w, y0 + h), (x0, y0 + h)];
        let (lo, hi) = (x0 + y0 + key, x0 + w + y0 + h + key);
        let first = ((lo - HATCH_HALF) / HATCH_PERIOD).floor() as i64;
        let last = ((hi + HATCH_HALF) / HATCH_PERIOD).ceil() as i64;
        let mut any = false;
        for m in first..=last {
            let c = HATCH_PERIOD * m as f64 - key;
            let band = clip(&clip(&rect, 1.0, 1.0, c + HATCH_HALF), -1.0, -1.0, -(c - HATCH_HALF));
            if band.len() < 3 {
                continue;
            }
            any = true;
            for (i, (px, py)) in band.iter().enumerate() {
                out.push(if i == 0 { 'M' } else { 'L' });
                unit(*px, bad, out);
                out.push(' ');
                unit(*py, bad, out);
            }
            out.push('z');
        }
        any
    }

    /// The characters this unit contributes to the copied text. Returns the next char of the source.
    fn take(&mut self, s: &str) -> String {
        if self.whole {
            return String::new();
        }
        let n = s.chars().count();
        let from = self.copied.min(self.copy.len());
        let to = (self.copied + n).min(self.copy.len());
        self.copied += n;
        self.copy[from..to].iter().collect()
    }

    /// The pieces of copyable text of one unit: each selectable sign with the controls that follow
    /// it, and whatever comes before the first sign.
    fn pieces(&mut self, ui: usize, u: &Unit) -> Vec<(Option<&'a PlacedSign>, String)> {
        let placed: &'a Placed = self.placed;
        let mut pieces: Vec<(Option<&'a PlacedSign>, String)> = Vec::new();
        let mut signs = placed.signs[self.next_sign..].iter();
        for item in &u.items {
            match item {
                Item::Sign { .. } => {
                    let Some(p) = signs.next() else { break };
                    if !p.props.unselectable {
                        let mut b = [0u8; 4];
                        let t = self.take(p.ch.encode_utf8(&mut b));
                        pieces.push((Some(p), t));
                    }
                }
                Item::Hidden { start, len } => {
                    let s = placed.plan.str(*start, *len);
                    let t = self.take(s);
                    match pieces.last_mut() {
                        Some(last) => last.1.push_str(&t),
                        None => pieces.push((None, t)),
                    }
                }
                _ => {}
            }
        }
        if self.whole {
            pieces.clear();
            if ui == 0 {
                pieces.push((None, self.params.source.to_string()));
            }
        }
        pieces.retain(|p| !p.1.is_empty());
        pieces
    }

    /// The invisible text inside the `svg`: one `text`, a `tspan` at each sign. Chromium and WebKit
    /// treat SVG text as a block, and a copy of a selection that takes in the whole block has a
    /// line break before and after it.
    fn text_inside(&mut self, pieces: &[(Option<&PlacedSign>, String)], out: &mut String) {
        out.push_str("<text font-size=\"");
        unit(EM_UNITS, &mut self.bad, out);
        out.push_str("\" fill-opacity=\"0\" text-anchor=\"middle\">");
        for (sign, t) in pieces {
            match sign {
                Some(p) => {
                    out.push_str("<tspan x=\"");
                    unit(p.rect.x + p.rect.w / 2.0, &mut self.bad, out);
                    out.push_str("\" y=\"");
                    unit(p.rect.y + p.rect.h / 2.0, &mut self.bad, out);
                    out.push('"');
                    if p.font_size != EM_UNITS {
                        out.push_str(" font-size=\"");
                        unit(p.font_size, &mut self.bad, out);
                        out.push('"');
                    }
                    out.push('>');
                    esc_text(t, out);
                    out.push_str("</tspan>");
                }
                None => esc_text(t, out),
            }
        }
        out.push_str("</text>");
    }

    /// The invisible text as one run on one line, for a line of signs (`hlr` or `hrl`): the browser
    /// sees one stretch of text, as it does upstream's, so a double or triple click, a drag from outside
    /// the block and a drag across it select as they do there. Each sign is a `span` whose
    /// `letter-spacing` is the sign's width on the drawing, so that the text of a sign lies where the
    /// sign is, and the highlight is one band. The font is tiny ([`RUN_EPS`] of the block's), so that no
    /// glyph the browser paints when it paints a selection is seen; `letter-spacing` is in em of that
    /// font, and the natural advance of a glyph (about one em of it) is taken off. Returns false, and
    /// writes nothing, when the signs do not run along the line (several lines, a column).
    fn text_run(&mut self, u: &Unit, pieces: &[(Option<&PlacedSign>, String)], bare: Option<&str>, generic: bool, out: &mut String) -> bool {
        if !self.opts.layout.h() {
            return false;
        }
        let rl = self.opts.layout.rl();
        let (w, h) = (EM_UNITS * u.w, EM_UNITS * u.h);
        let mut pre = String::new();
        // Start, width and text of each sign, from the side the text starts at.
        let mut signs: Vec<(f64, f64, &str)> = Vec::new();
        for (sign, t) in pieces {
            match sign {
                Some(p) if p.rect.w > 0.0 && p.rect.w.is_finite() && p.rect.x.is_finite() => {
                    let x = if rl { w - (p.rect.x + p.rect.w) } else { p.rect.x };
                    signs.push((x, p.rect.w, t));
                }
                Some(_) => return false,
                None if signs.is_empty() => pre.push_str(t),
                None => return false,
            }
        }
        if signs.is_empty() || !(w > 0.0 && h > 0.0) {
            return false;
        }
        // Signs that overlap (a stack) share their column's width, so that the run keeps going forward.
        let mut starts = vec![0.0f64; signs.len()];
        let mut i = 0;
        while i < signs.len() {
            let (mut a, mut b) = (signs[i].0, signs[i].0 + signs[i].1);
            let mut j = i + 1;
            while j < signs.len() {
                let (x, sw) = (signs[j].0, signs[j].1);
                let overlap = b.min(x + sw) - a.max(x);
                if overlap > 0.4 * (b - a).min(sw) {
                    a = a.min(x);
                    b = b.max(x + sw);
                    j += 1;
                } else {
                    break;
                }
            }
            let n = (j - i) as f64;
            for (k, s) in starts[i..j].iter_mut().enumerate() {
                *s = a + (b - a) * k as f64 / n;
            }
            i = j;
        }
        if starts.windows(2).any(|p| p[1] < p[0] - 1e-6) {
            return false;
        }
        out.push_str("<span aria-hidden=\"true\" style=\"position:absolute;");
        match bare {
            None => out.push_str("left:0;top:0;width:100%;height:100%"),
            Some(_) => {
                out.push_str("width:");
                fixed(u.w / RUN_EPS, 2, &mut self.bad, out);
                out.push_str("em;height:");
                fixed(u.h / RUN_EPS, 2, &mut self.bad, out);
                out.push_str("em");
            }
        }
        out.push_str(";color:transparent;white-space:nowrap;line-height:");
        fixed(u.h / RUN_EPS, 2, &mut self.bad, out);
        out.push_str("em;font-size:");
        match (bare.filter(|f| !f.is_empty()), self.params.css_font_px) {
            (Some(_), Some(px)) => {
                number_to_string(px * RUN_EPS, out);
                out.push_str("px");
            }
            _ => {
                fixed(RUN_EPS, 3, &mut self.bad, out);
                out.push_str("em");
            }
        }
        // The signs run from the side the drawing starts at, whatever the page's direction.
        out.push_str(if rl { ";direction:rtl;unicode-bidi:bidi-override" } else { ";direction:ltr" });
        if generic {
            out.push_str(";font-family:sans-serif");
        }
        out.push_str("\">");
        esc_text(&pre, out);
        for (i, (_, _, t)) in signs.iter().enumerate() {
            let next = starts.get(i + 1).copied().unwrap_or(w);
            let k = t.chars().count().max(1) as f64;
            let spacing = (next - starts[i]) / EM_UNITS / k / RUN_EPS - 1.0;
            out.push_str("<span style=\"");
            if i == 0 && starts[0] > 0.005 {
                out.push_str(if rl { "margin-right:" } else { "margin-left:" });
                fixed(100.0 * starts[0] / w, 2, &mut self.bad, out);
                out.push_str("%;");
            }
            out.push_str("letter-spacing:");
            fixed(spacing, 2, &mut self.bad, out);
            out.push_str("em\">");
            esc_text(t, out);
            out.push_str("</span>");
        }
        out.push_str("</span>");
        true
    }

    /// The invisible text beside the `svg`, in HTML, when it is not a run: a box over each sign, with its character in it.
    /// Text that is HTML copies as it is written in all three engines, with no line breaks around
    /// it, and its boxes (in percent of the block) are where the highlight of a selection falls.
    ///
    /// With `bare`, in contents mode with no wrapper: the box has the `svg`'s size in em and takes its
    /// static position (`bare` is the font size style the `svg` has, if the options gave one). With
    /// `generic`, a generic `font-family`.
    fn text_overlay(&mut self, u: &Unit, pieces: &[(Option<&PlacedSign>, String)], bare: Option<&str>, generic: bool, out: &mut String) {
        if self.text_run(u, pieces, bare, generic, out) {
            return;
        }
        let (w, h) = (EM_UNITS * u.w, EM_UNITS * u.h);
        // Out of the flow and as big as the block, so that it adds nothing to the line the block is on.
        out.push_str("<span aria-hidden=\"true\" style=\"position:absolute;");
        match bare {
            None => out.push_str("left:0;top:0;width:100%;height:100%"),
            Some(_) => {
                out.push_str("width:");
                fixed(u.w, 4, &mut self.bad, out);
                out.push_str("em;height:");
                fixed(u.h, 4, &mut self.bad, out);
                out.push_str("em");
            }
        }
        out.push_str(";color:transparent;white-space:nowrap;line-height:1");
        if generic {
            out.push_str(";font-family:sans-serif");
        }
        if let Some(font) = bare.filter(|f| !f.is_empty()) {
            out.push(';');
            out.push_str(font);
        }
        out.push_str("\">");
        for (sign, t) in pieces {
            match sign {
                Some(p) => {
                    let bad = &mut self.bad;
                    out.push_str("<span style=\"position:absolute;overflow:hidden;left:");
                    fixed(100.0 * p.rect.x / w, 2, bad, out);
                    out.push_str("%;top:");
                    fixed(100.0 * p.rect.y / h, 2, bad, out);
                    out.push_str("%;width:");
                    fixed(100.0 * p.rect.w / w, 2, bad, out);
                    out.push_str("%;height:");
                    fixed(100.0 * p.rect.h / h, 2, bad, out);
                    out.push_str("%;font-size:");
                    fixed(p.rect.h / EM_UNITS, 3, bad, out);
                    out.push_str("em\">");
                    esc_text(t, out);
                    out.push_str("</span>");
                }
                None => esc_text(t, out),
            }
        }
        out.push_str("</span>");
    }

    fn unit(&mut self, ui: usize, u: &Unit, wrapped: bool, out: &mut String) -> Result<(), LayoutError> {
        let mut shades = String::new();
        let mut body = String::new();
        let first_sign = self.next_sign;
        for item in &u.items {
            match item {
                Item::Sign { .. } => {
                    let p = self.placed.signs.get(self.next_sign).ok_or(LayoutError::Internal("a sign was not placed"))?;
                    self.next_sign += 1;
                    self.sign(u, p, &mut body);
                }
                Item::Shade(r) => {
                    // Shadings come before the signs, the latest first, as in the text printer.
                    let mut one = String::new();
                    if self.shade(u, r, &mut one) {
                        shades.insert_str(0, &one);
                    }
                }
                Item::Log { start, len } => self.logs.push(self.placed.plan.str(*start, *len).to_string()),
                Item::Hidden { .. } => {}
            }
        }
        // The copyable text of this unit's signs: the walk above has moved on, so go back.
        let after = self.next_sign;
        self.next_sign = first_sign;
        let kind = self.kind();
        let pieces = if kind == Selectable::Off { Vec::new() } else { self.pieces(ui, u) };
        self.next_sign = after;
        let overlay = kind == Selectable::Overlay && !pieces.is_empty() && u.w > 0.0 && u.h > 0.0;

        // Contents of an element the page has, with no wrapper of ours: the `svg` carries the font size.
        let bare = self.params.contents && !wrapped;
        // What the wrapper, or else the `svg`, says about the block's place on the line.
        let mut style = String::new();
        if self.params.inline_style {
            if !wrapped && !bare {
                style.push_str(ALIGN);
                if self.opts.border && !self.opts.layout.separated {
                    style.push(';');
                    style.push_str(BORDER);
                }
            }
            if let Some(px) = self.params.css_font_px {
                if !style.is_empty() {
                    style.push(';');
                }
                style.push_str("font-size:");
                number_to_string(px, &mut style);
                style.push_str("px");
            }
        }
        if overlay && bare {
            // First, so that in an inline block it starts where the `svg` does.
            self.text_overlay(u, &pieces, Some(&style), true, out);
        }
        if overlay && !bare {
            // The wrapper is `.hierojax`: an inline block that the `svg` and the boxes of the
            // text sit in. `line-height:0` so that it is no taller than the `svg`.
            out.push_str("<span style=\"position:relative;display:inline-block;line-height:0");
            if !style.is_empty() {
                out.push(';');
                out.push_str(&style);
            }
            out.push_str("\">");
        }
        out.push_str("<svg");
        if self.params.xmlns {
            out.push_str(" xmlns=\"http://www.w3.org/2000/svg\"");
        }
        out.push_str(" width=\"");
        fixed(u.w, 4, &mut self.bad, out);
        out.push_str("em\" height=\"");
        fixed(u.h, 4, &mut self.bad, out);
        out.push_str("em\" viewBox=\"0 0 ");
        unit(EM_UNITS * u.w, &mut self.bad, out);
        out.push(' ');
        unit(EM_UNITS * u.h, &mut self.bad, out);
        out.push_str("\" fill=\"currentColor\"");
        if !wrapped {
            self.block_attrs(out);
        } else if u.reversed {
            out.push_str(" direction=\"ltr\"");
        }
        let own_style = !style.is_empty() && (!overlay || bare);
        if own_style || overlay {
            out.push_str(" style=\"");
            if own_style {
                out.push_str(&style);
            }
            if overlay {
                // The drawing has no text, so nothing is lost; and a drag that starts beside the block
                // then ends in the invisible text, not inside the `svg` (Firefox puts the caret in the
                // nearest in-flow content, and the text is out of flow).
                if own_style {
                    out.push(';');
                }
                out.push_str(NO_SELECT);
            }
            out.push('"');
        }
        out.push('>');
        if !shades.is_empty() {
            let uniform = self.opts.uniform_shade;
            out.push_str("<g shape-rendering=\"crispEdges\" fill=\"");
            out.push_str(if uniform { UNIFORM_COLOR } else { HATCH_COLOR });
            out.push_str("\">");
            if uniform {
                out.push_str(&shades);
            } else {
                out.push_str("<path d=\"");
                out.push_str(&shades);
                out.push_str("\"/>");
            }
            out.push_str("</g>");
        }
        out.push_str(&body);
        if kind == Selectable::Svg && !pieces.is_empty() {
            self.text_inside(&pieces, out);
        }
        out.push_str("</svg>");
        if overlay && !bare {
            self.text_overlay(u, &pieces, None, self.params.contents, out);
            out.push_str("</span>");
        }
        Ok(())
    }

    /// The kind of copyable text this block gets: an overlay needs style attributes.
    fn kind(&self) -> Selectable {
        match self.params.selectable {
            Selectable::Overlay if !self.params.inline_style => Selectable::Svg,
            k => k,
        }
    }

    /// `role`, `aria-label` and the copy attribute of the outer element.
    fn block_attrs(&mut self, out: &mut String) {
        out.push_str(" role=\"img\" aria-label=\"");
        esc_attr(self.params.label, out);
        out.push('"');
        if self.params.data_text {
            out.push(' ');
            out.push_str(DATA_TEXT);
            out.push_str("=\"");
            esc_attr(self.params.source, out);
            out.push('"');
        }
    }
}

/// Print a placed plan as paths. One `svg` for the fragment; with `data-separated`, a `span` that
/// holds one `svg` per group (the element upstream's script is given is that `span`: it has the
/// class and size that `hierojax.css` and the text printer give it, as inline styles).
pub fn write_paths<O: Outlines>(
    placed: &Placed,
    opts: &Options,
    params: &PathParams<'_>,
    outlines: &O,
) -> Result<SvgOutput, LayoutError> {
    let copy: Vec<char> = params.source.chars().collect();
    // The characters that would be copied, counted as the printers walk the plan.
    let mut n = 0usize;
    for u in &placed.plan.units {
        for item in &u.items {
            if let Item::Hidden { start, len } = item {
                n += placed.plan.str(*start, *len).chars().count();
            }
        }
    }
    n += placed.signs.iter().filter(|p| !p.props.unselectable).count();
    let mut p = Printer {
        opts,
        placed,
        outlines,
        params,
        bad: false,
        logs: Vec::new(),
        next_sign: 0,
        whole: n != copy.len(),
        copy,
        copied: 0,
    };
    let mut markup = String::new();
    let wrapped = opts.layout.separated;
    if wrapped {
        let c = &placed.plan.container;
        markup.push_str("<span");
        if params.inline_style {
            markup.push_str(" style=\"");
            // In contents mode the page's element has the place on the line, so there is none here.
            let align = if params.contents { "" } else { ";vertical-align:-.1em" };
            debug_assert!(align.is_empty() || align[1..] == *ALIGN);
            match c.class_name {
                Some("hierojax-v-container") => {
                    markup.push_str("display:inline-flex;flex-direction:column");
                    if !params.contents {
                        markup.push_str(";vertical-align:top");
                    }
                }
                Some("hierojax-hrl-container") => {
                    markup.push_str("display:inline-block;direction:rtl");
                    markup.push_str(align);
                }
                _ => {
                    markup.push_str("display:inline-block");
                    markup.push_str(align);
                }
            }
            if let Some((w, h)) = c.size_em {
                for (name, v) in [("width", w), ("height", h)] {
                    markup.push(';');
                    markup.push_str(name);
                    markup.push(':');
                    match params.css_font_px {
                        Some(px) => {
                            fixed(v * px, 4, &mut p.bad, &mut markup);
                            markup.push_str("px");
                        }
                        None => {
                            fixed(v, 4, &mut p.bad, &mut markup);
                            markup.push_str("em");
                        }
                    }
                }
            }
            if c.add_border {
                markup.push(';');
                markup.push_str(BORDER);
            }
            markup.push('"');
        }
        p.block_attrs(&mut markup);
        markup.push('>');
    }
    for (i, u) in placed.plan.units.iter().enumerate() {
        p.unit(i, u, wrapped, &mut markup)?;
    }
    if wrapped {
        markup.push_str("</span>");
    }
    if p.bad {
        return Err(LayoutError::NotFinite);
    }
    Ok(SvgOutput { markup, logs: p.logs })
}
