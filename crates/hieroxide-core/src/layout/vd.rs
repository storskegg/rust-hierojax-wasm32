// SPDX-License-Identifier: GPL-3.0-only
//! The Android vector drawable printer: what the path printer draws (`paths.rs`), as the XML of a
//! `<vector>`. It reads the same placing numbers and the same `d` as the path SVG, and does not
//! lay anything out.
//!
//! **Size.** `viewportWidth` and `viewportHeight` are the `viewBox` of the SVG (1000 to the em);
//! `width` and `height` are the SVG's em at 24dp to the em.
//!
//! **Placing.** A sign's SVG transform is `translate(t) [scale(s)] [rotate(r)] [translate(o)]
//! scale(k -k)`. A `<group>` applies, outermost first, its translation, its rotation, then its
//! scale (about the pivot, which is 0 0 here), so a chain that goes translate, scale, rotate, is
//! cut into the fewest groups in which each group's operations come in that order. A matrix is
//! never needed: every transform the path printer writes is such a chain, so nothing is baked into
//! coordinates and no transform is refused.
//!
//! **Colour.** A path is filled `#FF000000` unless a colour was asked for (the app tints the
//! drawable, which keeps the alpha and drops the colour). A shading is black with `fillAlpha`, so
//! that a tint does not flatten it, of the share of black that the SVG's gray (hatching) or
//! darkgray (uniform) has on white. It is antialiased: a vector drawable has no `crispEdges`.

use super::em::{fixed, unit, Placed};
use super::options::{Options, EM_UNITS};
use super::paths::{plain_printer, Outlines, PathParams, Shape, Xf};
use super::plan::Item;
use super::svg::{esc_attr, SvgOutput};
use super::LayoutError;

/// The size of an em in dp.
const DP_PER_EM: f64 = 24.0;
/// The share of black that the SVG's `gray` and `darkgray` have on white (255 - 128, 255 - 169, of 255).
const HATCH_ALPHA: &str = "0.498";
const UNIFORM_ALPHA: &str = "0.337";
const BLACK: &str = "#FF000000";

/// What the printer needs besides the options of layout.
#[derive(Clone, Debug)]
pub struct VdParams<'a> {
    /// The label and the source, for the comment at the top.
    pub label: &'a str,
    pub source: &'a str,
    /// The colours of signs and of brackets, as Android writes them (`#AARRGGBB`).
    pub sign_color: &'a str,
    pub bracket_color: &'a str,
}

/// A CSS colour as Android's `#AARRGGBB`: `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()` and `rgba()`
/// with numbers or percentages, `transparent`, `currentColor` (which is black: the app tints), and
/// a short list of names. An empty string is black too. Anything else is an error.
pub fn android_color(css: &str) -> Result<String, String> {
    let t = css.trim().to_ascii_lowercase();
    let argb = |a: u8, r: u8, g: u8, b: u8| format!("#{a:02X}{r:02X}{g:02X}{b:02X}");
    let bad = || Err(format!("the colour '{css}' has no Android form here: use #rgb, #rrggbb, #rrggbbaa, rgb(), rgba() or a basic colour name"));
    if t.is_empty() || t == "currentcolor" {
        return Ok(BLACK.to_string());
    }
    if let Some(h) = t.strip_prefix('#') {
        if !h.bytes().all(|b| b.is_ascii_hexdigit()) {
            return bad();
        }
        let v = |i: usize, n: usize| u8::from_str_radix(&h[i..i + n], 16).unwrap_or(0);
        let dup = |i: usize| v(i, 1) * 17;
        return match h.len() {
            3 => Ok(argb(255, dup(0), dup(1), dup(2))),
            4 => Ok(argb(dup(3), dup(0), dup(1), dup(2))),
            6 => Ok(argb(255, v(0, 2), v(2, 2), v(4, 2))),
            8 => Ok(argb(v(6, 2), v(0, 2), v(2, 2), v(4, 2))),
            _ => bad(),
        };
    }
    if let Some(args) = t.strip_prefix("rgba(").or_else(|| t.strip_prefix("rgb(")).and_then(|r| r.strip_suffix(')')) {
        let parts: Vec<&str> = args.split([',', ' ', '/']).filter(|p| !p.is_empty()).collect();
        let channel = |p: &str, full: f64| -> Option<u8> {
            let (n, percent) = match p.strip_suffix('%') {
                Some(n) => (n, true),
                None => (p, false),
            };
            let x: f64 = n.parse().ok().filter(|x: &f64| x.is_finite())?;
            let v = if percent { x * full / 100.0 } else { x };
            Some(v.clamp(0.0, full).round() as u8)
        };
        if parts.len() == 3 || parts.len() == 4 {
            let rgb: Option<Vec<u8>> = parts[..3].iter().map(|p| channel(p, 255.0)).collect();
            let a = match parts.get(3) {
                None => Some(255),
                Some(p) => match p.strip_suffix('%') {
                    Some(_) => channel(p, 255.0),
                    None => p.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8),
                },
            };
            if let (Some(rgb), Some(a)) = (rgb, a) {
                return Ok(argb(a, rgb[0], rgb[1], rgb[2]));
            }
        }
        return bad();
    }
    let named: Option<(u8, u8, u8)> = match t.as_str() {
        "black" => Some((0, 0, 0)),
        "silver" => Some((192, 192, 192)),
        "gray" | "grey" => Some((128, 128, 128)),
        "darkgray" | "darkgrey" => Some((169, 169, 169)),
        "lightgray" | "lightgrey" => Some((211, 211, 211)),
        "white" => Some((255, 255, 255)),
        "maroon" => Some((128, 0, 0)),
        "red" => Some((255, 0, 0)),
        "purple" => Some((128, 0, 128)),
        "fuchsia" | "magenta" => Some((255, 0, 255)),
        "green" => Some((0, 128, 0)),
        "lime" => Some((0, 255, 0)),
        "olive" => Some((128, 128, 0)),
        "yellow" => Some((255, 255, 0)),
        "navy" => Some((0, 0, 128)),
        "blue" => Some((0, 0, 255)),
        "teal" => Some((0, 128, 128)),
        "aqua" | "cyan" => Some((0, 255, 255)),
        "orange" => Some((255, 165, 0)),
        "brown" => Some((165, 42, 42)),
        _ => None,
    };
    if t == "transparent" {
        return Ok(argb(0, 0, 0, 0));
    }
    match named {
        Some((r, g, b)) => Ok(argb(255, r, g, b)),
        None => bad(),
    }
}

/// Text for the inside of an XML comment: no `--` (a space between dashes), nothing XML cannot hold.
fn comment_text(s: &str, out: &mut String) {
    let mut last = ' ';
    for c in s.chars() {
        let c = match c {
            '\t' | '\n' | '\r' => c,
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => '\u{FFFD}',
            c => c,
        };
        if c == '-' && last == '-' {
            out.push(' ');
        }
        out.push(c);
        last = c;
    }
    if last == '-' {
        out.push(' ');
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    Translate,
    Rotate,
    Scale,
}

/// One group's attributes, in the order a group applies them.
#[derive(Default)]
struct Group {
    last: Option<Rank>,
    attrs: Vec<(&'static str, String)>,
}

/// The groups a sign's placing needs, outermost first.
fn groups(xf: &Xf) -> Vec<Group> {
    let t = |x: &'static str, y: &'static str, p: &'_ (String, String)| -> Vec<(&'static str, String)> {
        let mut v = Vec::new();
        if p.0 != "0" {
            v.push((x, p.0.clone()));
        }
        if p.1 != "0" {
            v.push((y, p.1.clone()));
        }
        v
    };
    let mut seq: Vec<(Rank, Vec<(&'static str, String)>)> = Vec::new();
    seq.push((Rank::Translate, t("android:translateX", "android:translateY", &xf.translate)));
    if let Some(s) = &xf.scale {
        let mut v = Vec::new();
        if s.0 != "1" {
            v.push(("android:scaleX", s.0.clone()));
        }
        if s.1 != "1" {
            v.push(("android:scaleY", s.1.clone()));
        }
        seq.push((Rank::Scale, v));
    }
    if let Some(r) = &xf.rotate {
        seq.push((Rank::Rotate, vec![("android:rotation", r.clone())]));
    }
    if let Some(o) = &xf.origin {
        seq.push((Rank::Translate, t("android:translateX", "android:translateY", o)));
    }
    let mut k = Vec::new();
    if xf.font_scale.0 != "1" {
        k.push(("android:scaleX", xf.font_scale.0.clone()));
    }
    if xf.font_scale.1 != "1" {
        k.push(("android:scaleY", xf.font_scale.1.clone()));
    }
    seq.push((Rank::Scale, k));
    let mut out: Vec<Group> = Vec::new();
    for (rank, attrs) in seq {
        if attrs.is_empty() {
            continue;
        }
        match out.last_mut() {
            Some(g) if g.last.is_some_and(|l| l < rank) => {
                g.last = Some(rank);
                g.attrs.extend(attrs);
            }
            _ => out.push(Group { last: Some(rank), attrs }),
        }
    }
    out
}

fn indent(n: usize, out: &mut String) {
    for _ in 0..n {
        out.push_str("  ");
    }
}

fn path(parts: (Xf, String), color: &str, out: &mut String) {
    let (xf, d) = parts;
    let gs = groups(&xf);
    for (i, g) in gs.iter().enumerate() {
        indent(1 + i, out);
        out.push_str("<group");
        for (name, v) in &g.attrs {
            out.push(' ');
            out.push_str(name);
            out.push_str("=\"");
            out.push_str(v);
            out.push('"');
        }
        out.push_str(">\n");
    }
    indent(1 + gs.len(), out);
    out.push_str("<path android:fillColor=\"");
    esc_attr(color, out);
    out.push_str("\" android:pathData=\"");
    out.push_str(&d);
    out.push_str("\"/>\n");
    for i in (0..gs.len()).rev() {
        indent(1 + i, out);
        out.push_str("</group>\n");
    }
}

/// Print a placed plan as a vector drawable. The plan must be one unit (not `separated`), and the
/// caller has refused a border: a drawable has no CSS pixel to draw it in.
pub fn write_vector_drawable<O: Outlines>(
    placed: &Placed,
    opts: &Options,
    params: &VdParams<'_>,
    outlines: &O,
) -> Result<SvgOutput, LayoutError> {
    let [u] = placed.plan.units.as_slice() else {
        return Err(LayoutError::Internal("a vector drawable is one unit"));
    };
    let unused = PathParams {
        label: params.label,
        selectable: super::paths::Selectable::Off,
        data_text: false,
        source: params.source,
        xmlns: false,
        inline_style: false,
        css_font_px: None,
        contents: false,
    };
    let mut pr = plain_printer(placed, opts, &unused, outlines);
    let mut shades = String::new();
    let mut shade_alpha = HATCH_ALPHA;
    let mut body = String::new();
    for item in &u.items {
        match item {
            Item::Sign { .. } => {
                let p = placed.signs.get(pr.next_sign).ok_or(LayoutError::Internal("a sign was not placed"))?;
                pr.next_sign += 1;
                if let Some(parts) = pr.sign_parts(u, p) {
                    let color = if p.props.bracket { params.bracket_color } else { params.sign_color };
                    path(parts, color, &mut body);
                }
            }
            Item::Shade(r) => {
                // As in the path SVG, the latest first.
                let mut one = String::new();
                match pr.shade_geometry(u, r) {
                    Some(Shape::Rect(x, y, w, h)) => {
                        shade_alpha = UNIFORM_ALPHA;
                        one = format!("M{x} {y}h{w}v{h}h-{w}z");
                    }
                    Some(Shape::Bands(d)) => one = d,
                    None => {}
                }
                shades.insert_str(0, &one);
            }
            Item::Log { start, len } => pr.logs.push(placed.plan.str(*start, *len).to_string()),
            Item::Hidden { .. } => {}
        }
    }
    let bad = &mut pr.bad;
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!-- hieroxide: ");
    comment_text(params.source, &mut out);
    out.push_str("; label: ");
    comment_text(params.label, &mut out);
    out.push_str(" -->\n<vector xmlns:android=\"http://schemas.android.com/apk/res/android\"\n    android:width=\"");
    fixed(DP_PER_EM * u.w, 4, bad, &mut out);
    out.push_str("dp\"\n    android:height=\"");
    fixed(DP_PER_EM * u.h, 4, bad, &mut out);
    out.push_str("dp\"\n    android:viewportWidth=\"");
    unit(EM_UNITS * u.w, bad, &mut out);
    out.push_str("\"\n    android:viewportHeight=\"");
    unit(EM_UNITS * u.h, bad, &mut out);
    out.push_str("\">\n");
    if !shades.is_empty() {
        out.push_str("  <path android:fillColor=\"");
        out.push_str(BLACK);
        out.push_str("\" android:fillAlpha=\"");
        out.push_str(shade_alpha);
        out.push_str("\" android:pathData=\"");
        out.push_str(&shades);
        out.push_str("\"/>\n");
    }
    out.push_str(&body);
    out.push_str("</vector>\n");
    if pr.bad {
        return Err(LayoutError::NotFinite);
    }
    Ok(SvgOutput { markup: out, logs: pr.logs })
}
