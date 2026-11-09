//! The output of layout: a flat list of what upstream's `print` methods ask a `Printed*`
//! object to do, in the order it asks it. The printers read this list; layout does not
//! know which printer follows.

use super::shapes::SignProps;

/// `{ x, y, w, h }` in em.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// One call on a printed object.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// `addSign(ch, scale, xScale, yScale, rotate, mirror, rect, properties)`
    Sign {
        ch: char,
        scale: f64,
        x_scale: f64,
        y_scale: f64,
        rotate: f64,
        mirror: bool,
        rect: Rect,
        props: SignProps,
    },
    /// `addHidden(s)`; the text is `Plan::text[start..start + len]`.
    Hidden { start: u32, len: u32 },
    /// `addShading(x, y, w, h)`
    Shade(Rect),
    /// `addLog(message)`, only recorded when `options.log` is set.
    Log { start: u32, len: u32 },
}

/// One printed object (`new PrintedSVG(...)` and the calls made on it until the next
/// one): a whole fragment, or one group in separated mode.
#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    /// The `w` and `h` given to the constructor, in em.
    pub w: f64,
    pub h: f64,
    pub w_accum: f64,
    pub h_accum: f64,
    pub reversed: bool,
    pub items: Vec<Item>,
}

/// What `Fragment.print` does to the element itself, in separated mode.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Container {
    /// `element.className = ...`
    pub class_name: Option<&'static str>,
    /// `element.style['width']` and `['height']`, as the strings assigned.
    pub style_width: Option<String>,
    pub style_height: Option<String>,
    /// `element.classList.add('hierojax-border')`
    pub add_border: bool,
    /// `element.style.setProperty('width', ...)` and `('height', ...)`, which `PrintedDOM`
    /// does to the element itself when the fragment is not separated.
    pub prop_width: Option<String>,
    pub prop_height: Option<String>,
}

/// Everything `Fragment.print` does after formatting.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub container: Container,
    pub units: Vec<Unit>,
    /// Backing store for `Item::Hidden` and `Item::Log`.
    pub text: String,
}

impl Plan {
    pub fn str(&self, start: u32, len: u32) -> &str {
        let s = start as usize;
        self.text.get(s..s + len as usize).unwrap_or("")
    }
}
