//! Options, as `processFragmentNow` and `Fragment.print` build them.
//!
//! Upstream collects the computed font size and colour and the `data-*` attributes into
//! a plain object, then `Fragment.print` normalizes it with `||`, `Number(..) ||`, and
//! `== 'true'`. Those coercions are reproduced here, so that for instance `data-sep="0"`
//! and `data-sep="abc"` both give `0.07`, `data-fontsize="0"` gives a font size of 0
//! (a non-empty string is truthy), and `data-dir="foo"` gives a direction that is
//! neither horizontal nor vertical.

use crate::jsnum::number_from_str;

/// The values `processFragmentNow` reads from an element: the computed font size in
/// pixels and colour, and the `data-*` attributes. An attribute that is absent or empty
/// is the empty string (upstream only copies an attribute when it is truthy).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RawOptions {
    /// `Number(style.getPropertyValue('font-size').replace('px', ''))`.
    pub font_px: f64,
    /// `style.getPropertyValue('color')`.
    pub sign_color: String,
    pub fontsize: String,
    pub signcolor: String,
    pub dir: String,
    pub linesize: String,
    pub sep: String,
    pub separated: String,
    pub type_: String,
    pub bracketcolor: String,
    pub shadepattern: String,
    pub align: String,
    pub border: String,
    pub standalone: String,
    pub log: String,
}

/// The attribute names, in the order `processFragmentNow` reads them (after `data-`).
pub const DATA_ATTRIBUTES: [&str; 13] = [
    "fontsize", "signcolor", "dir", "linesize", "sep", "separated", "type", "bracketcolor",
    "shadepattern", "align", "border", "standalone", "log",
];

impl RawOptions {
    /// Build from the two computed values and the thirteen attribute values in the order
    /// of [`DATA_ATTRIBUTES`]. Missing trailing values are empty.
    pub fn from_parts(font_px: f64, sign_color: &str, attrs: &[String]) -> RawOptions {
        let a = |i: usize| attrs.get(i).cloned().unwrap_or_default();
        RawOptions {
            font_px,
            sign_color: sign_color.to_string(),
            fontsize: a(0),
            signcolor: a(1),
            dir: a(2),
            linesize: a(3),
            sep: a(4),
            separated: a(5),
            type_: a(6),
            bracketcolor: a(7),
            shadepattern: a(8),
            align: a(9),
            border: a(10),
            standalone: a(11),
            log: a(12),
        }
    }
}

/// `options.dir`. Anything but the four known values is neither horizontal nor
/// vertical (`Group.h` and `Group.v` are both false), so layout takes its vertical
/// branches while `Group.rl` is false.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Hlr,
    Hrl,
    Vlr,
    Vrl,
    Other,
}

impl Dir {
    fn from_str(s: &str) -> Dir {
        match s {
            "hlr" => Dir::Hlr,
            "hrl" => Dir::Hrl,
            "vlr" => Dir::Vlr,
            "vrl" => Dir::Vrl,
            _ => Dir::Other,
        }
    }
}

/// `options.type`: anything but `canvas` and `svg` is the DOM printer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrintKind {
    Svg,
    Canvas,
    Dom,
}

/// The numbers and flags layout reads. `Copy`, so it can be passed down freely.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutOpts {
    pub dir: Dir,
    pub linesize: f64,
    pub fontsize: f64,
    pub sep: f64,
    pub separated: bool,
    /// `options.align == 'bottom'`
    pub align_bottom: bool,
}

impl LayoutOpts {
    /// `Group.h`
    pub fn h(&self) -> bool {
        matches!(self.dir, Dir::Hlr | Dir::Hrl)
    }
    /// `Group.v`
    pub fn v(&self) -> bool {
        matches!(self.dir, Dir::Vlr | Dir::Vrl)
    }
    /// `Group.rl`
    pub fn rl(&self) -> bool {
        matches!(self.dir, Dir::Hrl | Dir::Vrl)
    }
    /// `Group.measureOptions`: `{ dir, fontsize: 50, sep }`. Upstream's object has no
    /// `linesize`, `separated` or `align`; nothing below `Fragment` reads the first two, and
    /// `align` being absent is not `'bottom'`. An unknown `dir` measures as `vlr`.
    pub fn measure_options(&self) -> LayoutOpts {
        LayoutOpts {
            dir: if self.h() { Dir::Hlr } else { Dir::Vlr },
            linesize: f64::NAN,
            fontsize: 50.0,
            sep: self.sep,
            separated: false,
            align_bottom: false,
        }
    }
}

/// The options after `Fragment.print`'s normalization.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub layout: LayoutOpts,
    pub kind: PrintKind,
    pub signcolor: String,
    pub bracketcolor: String,
    /// `options.shadepattern == 'uniform'`
    pub uniform_shade: bool,
    pub border: bool,
    pub standalone: bool,
    pub log: bool,
}

fn or<'a>(s: &'a str, default: &'a str) -> &'a str {
    if s.is_empty() { default } else { s }
}

/// `Number(s) || default` for a string that may be empty (upstream: `undefined` or a
/// truthy string; `Number(undefined)` is NaN).
fn number_or(s: &str, default: f64) -> f64 {
    if s.is_empty() {
        return default;
    }
    let n = number_from_str(s);
    if n == 0.0 || n.is_nan() { default } else { n }
}

impl Options {
    /// `processFragmentNow`'s option object followed by the normalization at the top of
    /// `Fragment.print`.
    pub fn from_raw(raw: &RawOptions) -> Options {
        // options.fontsize = fontSize (a number); a data-fontsize attribute replaces it
        // with a string, and `options.fontsize || 22` then only replaces a falsy number.
        let fontsize = if raw.fontsize.is_empty() {
            if raw.font_px == 0.0 || raw.font_px.is_nan() { 22.0 } else { raw.font_px }
        } else {
            number_from_str(&raw.fontsize)
        };
        let signcolor = if raw.signcolor.is_empty() { raw.sign_color.as_str() } else { raw.signcolor.as_str() };
        let kind = match or(&raw.type_, "svg") {
            "canvas" => PrintKind::Canvas,
            "svg" => PrintKind::Svg,
            _ => PrintKind::Dom,
        };
        Options {
            layout: LayoutOpts {
                dir: Dir::from_str(or(&raw.dir, "hlr")),
                linesize: number_or(&raw.linesize, 1.0),
                fontsize,
                sep: number_or(&raw.sep, 0.07),
                separated: raw.separated == "true",
                align_bottom: or(&raw.align, "middle") == "bottom",
            },
            kind,
            signcolor: or(signcolor, "black").to_string(),
            bracketcolor: or(&raw.bracketcolor, "black").to_string(),
            uniform_shade: or(&raw.shadepattern, "uniform") == "uniform",
            border: raw.border == "true",
            standalone: raw.standalone == "true",
            log: raw.log == "true",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw() -> RawOptions {
        RawOptions { font_px: 22.0, sign_color: "rgb(0, 0, 0)".into(), ..Default::default() }
    }

    #[test]
    fn defaults() {
        let o = Options::from_raw(&raw());
        assert_eq!(o.layout.dir, Dir::Hlr);
        assert_eq!(o.layout.linesize, 1.0);
        assert_eq!(o.layout.sep, 0.07);
        assert_eq!(o.layout.fontsize, 22.0);
        assert!(!o.layout.separated && o.uniform_shade && !o.layout.align_bottom);
        assert_eq!(o.kind, PrintKind::Svg);
        assert_eq!(o.signcolor, "rgb(0, 0, 0)");
        assert_eq!(o.bracketcolor, "black");
    }

    #[test]
    fn falsy_numbers_fall_back() {
        let mut r = raw();
        r.sep = "0".into();
        r.linesize = "abc".into();
        let o = Options::from_raw(&r);
        assert_eq!((o.layout.sep, o.layout.linesize), (0.07, 1.0));
        r.sep = "-0.5".into();
        r.linesize = "Infinity".into();
        let o = Options::from_raw(&r);
        assert_eq!((o.layout.sep, o.layout.linesize), (-0.5, f64::INFINITY));
    }

    #[test]
    fn fontsize_string_and_number() {
        let mut r = raw();
        r.font_px = 0.0;
        assert_eq!(Options::from_raw(&r).layout.fontsize, 22.0);
        r.fontsize = "0".into(); // a non-empty string is truthy
        assert_eq!(Options::from_raw(&r).layout.fontsize, 0.0);
        r.fontsize = "40".into();
        assert_eq!(Options::from_raw(&r).layout.fontsize, 40.0);
        r.fontsize = "x".into();
        assert!(Options::from_raw(&r).layout.fontsize.is_nan());
    }

    #[test]
    fn unknown_dir_is_neither_h_nor_v() {
        let mut r = raw();
        r.dir = "foo".into();
        let l = Options::from_raw(&r).layout;
        assert!(!l.h() && !l.v() && !l.rl());
        r.dir = "vrl".into();
        let l = Options::from_raw(&r).layout;
        assert!(!l.h() && l.v() && l.rl());
    }
}
