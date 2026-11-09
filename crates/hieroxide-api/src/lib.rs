//! Hieroxide: typeset Egyptian hieroglyphs from a font's outlines.
//!
//! ```no_run
//! use hieroxide_api::{parse, Engine, Font, Options, SvgKind};
//!
//! # fn main() -> Result<(), hieroxide_api::Error> {
//! let font = Font::from_bytes(std::fs::read("NewGardiner.otf").unwrap())?;
//! let mut engine = Engine::new(font, Options::default());
//! let doc = parse("\u{13000}\u{13431}\u{13001}")?;
//! let layout = engine.layout(&doc)?;
//! let svg = layout.to_svg(SvgKind::Paths)?; // one self-contained block: a <span> holding the <svg> (see Selectable)
//! for g in layout.glyphs() {
//!     let _ = (g.glyph_id, g.transform); // for your own output
//! }
//! # let _ = svg;
//! # Ok(())
//! # }
//! ```
//!
//! **Consistent.** Layout is computed from the font's outlines, not from a browser's canvas, so it
//! is the same on every platform, at every font size, in any order. That is the one mode of this
//! API. (The drop-in that does what upstream hierojax does, in a browser, is a separate build.)
//!
//! **Units.** Everything a [`Layout`] reports is in em, with y downward and the origin at the top
//! left of the unit's box. A glyph's `transform` maps the font's own units (y up) into that space.
//!
//! **Errors are values.** Nothing here panics on bad input; a bad font, a fragment that does not
//! parse, a layout that comes to no finite size, and a kind of output that was not compiled in are
//! each an [`Error`] with a [`ErrorKind`].
//!
//! **Semver.** 0.x: this is a first version, and anything may change in a minor release until 1.0.

use std::fmt;
use std::sync::Arc;

use hieroxide_core::layout as lay;
use hieroxide_core::tree::Tree;
use hieroxide_outline::{Cache, OutlineRaster};

/// What went wrong, for a caller that wants to decide what to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The bytes are not a font this can use: an OpenType font with CFF outlines.
    Font,
    /// The text is not a hieroglyph fragment.
    Parse,
    /// The fragment parsed, but could not be laid out or printed (a number that is not finite).
    Layout,
    /// What was asked for is not in this build (`SvgKind::Paths` without the feature `paths`).
    Unsupported,
}

/// An error: a kind to branch on and a message to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
    /// For a [`ErrorKind::Parse`] error: where, in UTF-16 code units from the start of the text
    /// (the index a JavaScript string would have).
    pub position: Option<usize>,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

fn error(kind: ErrorKind, message: impl Into<String>) -> Error {
    Error { kind, message: message.into(), position: None }
}

impl From<lay::LayoutError> for Error {
    fn from(e: lay::LayoutError) -> Error {
        error(ErrorKind::Layout, e.to_string())
    }
}

/// A font, checked: the unmodified bytes of an OpenType font with CFF outlines (NewGardiner).
/// Cloning shares the bytes.
#[derive(Clone)]
pub struct Font {
    bytes: Arc<[u8]>,
    units_per_em: u16,
}

impl Font {
    /// Check a font's bytes. The font is not bundled: it is a file you ship.
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Result<Font, Error> {
        let bytes: Vec<u8> = bytes.into();
        let raster = OutlineRaster::new(&bytes).map_err(|e| error(ErrorKind::Font, e.to_string()))?;
        let units_per_em = raster.units_per_em() as u16;
        Ok(Font { bytes: bytes.into(), units_per_em })
    }

    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }
}

impl fmt::Debug for Font {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Font({} bytes, {} units per em)", self.bytes.len(), self.units_per_em)
    }
}

/// The direction of writing, as `data-dir`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    /// Horizontal, left to right.
    #[default]
    Hlr,
    /// Horizontal, right to left: signs face right.
    Hrl,
    /// Vertical, left to right.
    Vlr,
    /// Vertical, right to left.
    Vrl,
}

/// Where a line's signs sit on it, as `data-align`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Middle,
    Bottom,
}

/// How a shaded (damaged) area is drawn, as `data-shadepattern`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shading {
    #[default]
    Uniform,
    Hatching,
}

/// How a path block carries the text that a selection of the page copies (invisible: nothing is
/// drawn from it, so no font is needed).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Selectable {
    /// In HTML beside the `svg`, in a `span` that wraps both, as one run of text over the signs of a line (a box over each sign of a column). Copies
    /// exactly the text in Chromium, Firefox and WebKit. The block is a `span` and not a bare `svg`.
    #[default]
    Overlay,
    /// As SVG text inside the `svg`, which stays bare. Chromium and WebKit put a line break before
    /// and after the text in a copy that takes in the whole block.
    Svg,
    /// None.
    Off,
}

/// The default `aria-label` of a path block.
pub const DEFAULT_LABEL: &str = "Egyptian hieroglyphs";

/// What `layout` is told, mostly the fragment options of hierojax (`data-*`). A value that cannot be
/// used (a size that is not a finite positive number) is the default.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub dir: Direction,
    /// The CSS font size of the block in px (`data-fontsize`); `None` follows the page's. It does
    /// not affect layout, which is in em.
    pub font_size: Option<f64>,
    /// The space between lines or columns, in em (`data-linesize`); default 1.
    pub line_size: Option<f64>,
    /// The space between groups, in em (`data-sep`); default 0.07.
    pub sep: Option<f64>,
    /// One `svg` per group (`data-separated`), so that a browser can break the line between them.
    pub separated: bool,
    pub align: Align,
    /// A CSS colour for signs (`data-signcolor`); default the page's text colour.
    pub sign_color: Option<String>,
    /// A CSS colour for brackets (`data-bracketcolor`); default black.
    pub bracket_color: Option<String>,
    pub shading: Shading,
    /// A border of 1px (`data-border`).
    pub border: bool,
    /// Collect upstream's messages into [`Layout::logs`] (`data-log`).
    pub log: bool,
    /// Paths: the `aria-label` of the block. Default [`DEFAULT_LABEL`].
    pub label: Option<String>,
    /// Paths: how the block carries the text that a selection of the page copies. Default
    /// [`Selectable::Overlay`].
    pub selectable: Selectable,
    /// Paths: write the text on the block as `data-text`, for a script to read. Default on.
    pub data_text: bool,
    /// Paths: `xmlns` on each `svg`, so that the output is also a valid SVG file. Default off.
    pub xmlns: bool,
    /// Paths: the `style` attribute that aligns the block on the line and carries `font_size` and
    /// `border`. Default on. A page whose CSP forbids inline styles sets it off and writes that in
    /// its own CSS.
    pub inline_style: bool,
    /// Paths: only what goes *inside* an element the page already has, such as its own
    /// `<span class="hierojax">`, which keeps its class and attributes (so its CSS still applies)
    /// and its place on the line. No wrapper `span`, no `vertical-align`; `role`, `aria-label` and
    /// `data-text` are on the `svg`; the invisible text (`Selectable::Overlay`) comes before the
    /// `svg`, sized in em like it, and takes the position the `svg` takes in whatever box the page
    /// gives. [`Options::font_size`] is set on the `svg` and the text, and [`Options::border`] has no
    /// effect (the page's element can have a border). With `separated`, one `span` with no class
    /// stays inside the page's element, to lay the `svg`s out (it has the border, if asked for).
    /// Default off: a block that stands alone.
    pub contents: bool,
    /// Text: the URL of the font for a standalone SVG's `@font-face`; with it, the `svg` carries
    /// its own style (`data-standalone`).
    pub standalone_font: Option<String>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            dir: Direction::Hlr,
            font_size: None,
            line_size: None,
            sep: None,
            separated: false,
            align: Align::Middle,
            sign_color: None,
            bracket_color: None,
            shading: Shading::Uniform,
            border: false,
            log: false,
            label: None,
            selectable: Selectable::Overlay,
            data_text: true,
            xmlns: false,
            inline_style: true,
            contents: false,
            standalone_font: None,
        }
    }
}

fn number(x: Option<f64>) -> String {
    match x {
        Some(v) if v.is_finite() && v > 0.0 => {
            let mut s = String::new();
            hieroxide_core::jsnum::number_to_string(v, &mut s);
            s
        }
        _ => String::new(),
    }
}

impl Options {
    /// These options with the values of an element's `data-*` attributes applied over them, as
    /// upstream's `processFragmentNow` reads them: `attrs` are `(name, value)` with the name as it
    /// is after `data-` (`fontsize`, `signcolor`, `dir`, `linesize`, `sep`, `separated`,
    /// `bracketcolor`, `shadepattern`, `align`, `border`, `log`), and an empty value is as absent
    /// (a falsy attribute is not read). Values are read as upstream reads them (`Number(...)` for
    /// sizes, `"true"` for a flag, any `shadepattern` but `uniform` is hatching) and then as layout
    /// takes them (a size that is not a finite positive number, or an unknown `dir`, is the
    /// default). `type` and `standalone` are for other kinds of output, and names that are not
    /// options are ignored.
    pub fn with_data_attributes<'a>(mut self, attrs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Options {
        let size = |v: &str| {
            let n = hieroxide_core::jsnum::number_from_str(v);
            (n.is_finite() && n > 0.0).then_some(n)
        };
        for (name, v) in attrs {
            if v.is_empty() {
                continue;
            }
            match name {
                "fontsize" => self.font_size = size(v),
                "signcolor" => self.sign_color = Some(v.to_string()),
                "dir" => {
                    self.dir = match v {
                        "hrl" => Direction::Hrl,
                        "vlr" => Direction::Vlr,
                        "vrl" => Direction::Vrl,
                        _ => Direction::Hlr,
                    }
                }
                "linesize" => self.line_size = size(v),
                "sep" => self.sep = size(v),
                "separated" => self.separated = v == "true",
                "bracketcolor" => self.bracket_color = Some(v.to_string()),
                "shadepattern" => self.shading = if v == "uniform" { Shading::Uniform } else { Shading::Hatching },
                "align" => self.align = if v == "bottom" { Align::Bottom } else { Align::Middle },
                "border" => self.border = v == "true",
                "log" => self.log = v == "true",
                _ => {}
            }
        }
        self
    }

    fn raw(&self) -> lay::RawOptions {
        let flag = |b: bool| if b { "true" } else { "" }.to_string();
        lay::RawOptions {
            font_px: f64::NAN,
            sign_color: String::new(),
            fontsize: number(self.font_size),
            signcolor: self.sign_color.clone().unwrap_or_default(),
            dir: match self.dir {
                Direction::Hlr => "hlr",
                Direction::Hrl => "hrl",
                Direction::Vlr => "vlr",
                Direction::Vrl => "vrl",
            }
            .to_string(),
            linesize: number(self.line_size),
            sep: number(self.sep),
            separated: flag(self.separated),
            type_: String::new(),
            bracketcolor: self.bracket_color.clone().unwrap_or_default(),
            shadepattern: if self.shading == Shading::Hatching { "hatching" } else { "uniform" }.to_string(),
            align: if self.align == Align::Bottom { "bottom" } else { "middle" }.to_string(),
            border: flag(self.border),
            standalone: flag(self.standalone_font.is_some()),
            log: flag(self.log),
        }
    }
}

/// A parsed fragment: the text and its tree. Parsing does not need a font.
#[derive(Clone, Debug)]
pub struct Doc {
    source: String,
    tree: Tree,
}

/// Parse a fragment (Unicode hieroglyphs with format controls). The fragment is not trimmed:
/// whitespace is not part of the grammar.
pub fn parse(text: &str) -> Result<Doc, Error> {
    match hieroxide_core::parse(text) {
        Ok(tree) => Ok(Doc { source: text.to_string(), tree }),
        Err(e) => Err(Error { kind: ErrorKind::Parse, message: e.to_string(), position: Some(e.pos) }),
    }
}

impl Doc {
    /// The text as it was given: what a reader copies.
    pub fn source(&self) -> &str {
        &self.source
    }
}

/// What kind of SVG to print.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SvgKind {
    /// Signs as text in the font: needs the font on the page, and a stylesheet (`hierojax.css`) for
    /// the page's one `svg` of each fragment. Smallest.
    Text,
    /// Signs as `<path>`: needs nothing, and has no `id`s, so any number of blocks fit on a page.
    Paths,
}

/// How a glyph takes part in the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum GlyphRole {
    /// A sign of the text, drawn as it was written.
    Sign,
    /// A bracket (`[` `]` and the like): sized to its content.
    Bracket,
    /// Drawn, but not the text: an alternative form that replaces the sign written (the text then
    /// holds the sign), a ligature, or the outline of an enclosure.
    Variant,
}

/// A rectangle in em.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// A glyph, placed.
#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    /// The index of the unit it is in ([`Layout::units`]).
    pub unit: usize,
    pub ch: char,
    /// The font's glyph id, `None` if the font has no glyph for `ch` (then nothing is drawn).
    pub glyph_id: Option<u16>,
    pub role: GlyphRole,
    /// `[a, b, c, d, e, f]`: a point `(x, y)` in font units, y up, goes to
    /// `(a x + c y + e, b x + d y + f)` in em in the unit's box, y down.
    pub transform: [f64; 6],
    /// The box layout gave the sign, in em.
    pub rect: Rect,
}

/// A shaded area, in em.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shade {
    pub unit: usize,
    pub rect: Rect,
    /// Where the hatching starts, in em along x, so that the hatching of neighbouring units joins.
    pub hatch_phase: f64,
}

/// One `svg` of a fragment: the whole fragment, or with `separated`, one of its groups.
#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    pub width: f64,
    pub height: f64,
    /// Right to left: signs face right and the unit's contents are mirrored.
    pub reversed: bool,
}

/// What an element that holds a separated fragment's `svg`s needs (text output; path output
/// carries its own `span`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Container {
    pub class: Option<String>,
    pub width: Option<String>,
    pub height: Option<String>,
    pub border: bool,
}

/// A fragment laid out: data, owning everything it holds. `Send`, so it can be made on one thread
/// and printed on another.
#[derive(Clone, Debug)]
#[cfg_attr(not(feature = "paths"), allow(dead_code))] // what only path output reads
pub struct Layout {
    source: String,
    placed: lay::Placed,
    core_opts: lay::Options,
    css_font_px: Option<f64>,
    font_url: (String, String),
    label: String,
    selectable: Selectable,
    data_text: bool,
    xmlns: bool,
    inline_style: bool,
    contents: bool,
    units: Vec<Unit>,
    glyphs: Vec<Glyph>,
    shades: Vec<Shade>,
    logs: Vec<String>,
    container: Container,
    outlines: Outlines,
}

#[derive(Clone, Debug)]
struct Outlines {
    upem: f64,
    glyphs: Vec<(char, hieroxide_outline::Shared<hieroxide_outline::Glyph>)>,
}

impl lay::Outlines for Outlines {
    fn units_per_em(&self) -> f64 {
        self.upem
    }
    fn outline(&self, ch: char) -> Option<&[lay::Seg]> {
        let i = self.glyphs.binary_search_by_key(&ch, |g| g.0).ok()?;
        Some(&self.glyphs[i].1.path)
    }
}

impl Layout {
    /// The fragment as it was written: what is copied from a block.
    pub fn text(&self) -> &str {
        &self.source
    }

    /// The `svg`s: one, or with `separated` one per group.
    pub fn units(&self) -> &[Unit] {
        &self.units
    }

    /// Every drawn glyph, in the order of the text.
    pub fn glyphs(&self) -> &[Glyph] {
        &self.glyphs
    }

    pub fn shades(&self) -> &[Shade] {
        &self.shades
    }

    /// The messages `Options::log` asked for.
    pub fn logs(&self) -> &[String] {
        &self.logs
    }

    /// What a host element of a separated fragment's `svg`s gets.
    pub fn container(&self) -> &Container {
        &self.container
    }

    /// Print as SVG. `Paths` is a self-contained block: by default a `span` that holds the `svg` and
    /// the invisible text (`Selectable::Overlay`), a bare `svg` with `Selectable::Svg` or `Off`; with
    /// [`Options::contents`], only what goes inside an element the page has.
    /// The bytes depend on the fragment, the options and the font, on nothing else.
    pub fn to_svg(&self, kind: SvgKind) -> Result<String, Error> {
        match kind {
            SvgKind::Text => {
                let out = lay::write_text(&self.placed, &self.core_opts, self.css_font_px, (&self.font_url.0, &self.font_url.1), &self.source)?;
                Ok(out.markup)
            }
            SvgKind::Paths => self.paths(),
        }
    }

    #[cfg(feature = "paths")]
    fn paths(&self) -> Result<String, Error> {
        let params = lay::PathParams {
            label: &self.label,
            selectable: match self.selectable { Selectable::Overlay => lay::Selectable::Overlay, Selectable::Svg => lay::Selectable::Svg, Selectable::Off => lay::Selectable::Off },
            data_text: self.data_text,
            source: &self.source,
            xmlns: self.xmlns,
            inline_style: self.inline_style,
            css_font_px: self.css_font_px,
            contents: self.contents,
        };
        Ok(lay::write_paths(&self.placed, &self.core_opts, &params, &self.outlines)?.markup)
    }

    #[cfg(not(feature = "paths"))]
    fn paths(&self) -> Result<String, Error> {
        Err(error(ErrorKind::Unsupported, "this build has no path output"))
    }
}

/// Lays fragments out. It owns the font and what it learns from it (outlines and measurements), so
/// the second fragment of a text costs less than the first. Not shared between threads while
/// laying out (`&mut self`); make one per thread.
pub struct Engine {
    font: Font,
    options: Options,
    core: lay::Engine,
    cache: Option<Cache>,
}

impl Engine {
    /// `options` are what [`Engine::layout`] uses.
    pub fn new(font: Font, options: Options) -> Engine {
        Engine { font, options, core: lay::Engine::with_mode(lay::Mode::Consistent), cache: Some(Cache::default()) }
    }

    pub fn options(&self) -> &Options {
        &self.options
    }

    pub fn set_options(&mut self, options: Options) {
        self.options = options;
    }

    /// Lay a fragment out with the engine's options.
    pub fn layout(&mut self, doc: &Doc) -> Result<Layout, Error> {
        let options = self.options.clone();
        self.layout_with(doc, &options)
    }

    /// Lay a fragment out with other options; what the engine has learned is kept. The result does
    /// not depend on what was laid out before.
    pub fn layout_with(&mut self, doc: &Doc, options: &Options) -> Result<Layout, Error> {
        let (core_opts, css_font_px) = lay::Options::consistent(&options.raw());
        let mut raster = OutlineRaster::with_cache(&self.font.bytes, self.cache.take().unwrap_or_default())
            .map_err(|e| error(ErrorKind::Font, e.to_string()))?
            .lenient();
        self.core.external_font = options.standalone_font.clone().unwrap_or_else(|| "NewGardiner.otf".to_string());
        let planned = {
            let mut layouter = lay::Layouter::new(&doc.tree, &mut self.core, &mut raster);
            layouter.plan(&core_opts)
        };
        let result = planned.map_err(Error::from).and_then(|plan| {
            let placed = lay::place(&mut self.core, &mut raster, plan)?;
            // The same checks the text printer makes: a layout that is not finite is an error.
            lay::write_text(&placed, &core_opts, css_font_px, ("", ""), &doc.source)?;
            Ok(placed)
        });
        let result = result.map(|placed| {
            let ids: Vec<Option<u16>> = placed.signs.iter().map(|p| raster.glyph_id(p.ch)).collect();
            let mut outlines = Outlines { upem: raster.units_per_em(), glyphs: Vec::new() };
            if cfg!(feature = "paths") {
                let mut chars: Vec<char> = placed.signs.iter().map(|p| p.ch).collect();
                chars.sort_unstable();
                chars.dedup();
                outlines.glyphs = chars.into_iter().filter_map(|c| raster.outline(c).map(|g| (c, g))).collect();
            }
            (placed, ids, outlines)
        });
        self.cache = Some(raster.into_cache());
        let (placed, ids, outlines) = result?;
        let upem = outlines.upem;

        let container = lay::sized_container(placed.plan.container.clone(), css_font_px)?;
        let mut layout = Layout {
            source: doc.source.clone(),
            core_opts,
            css_font_px,
            font_url: (self.core.external_font.clone(), self.core.external_font_format.clone()),
            label: options.label.clone().unwrap_or_else(|| DEFAULT_LABEL.to_string()),
            selectable: options.selectable,
            data_text: options.data_text,
            xmlns: options.xmlns,
            inline_style: options.inline_style,
            contents: options.contents,
            units: Vec::new(),
            glyphs: Vec::new(),
            shades: Vec::new(),
            logs: Vec::new(),
            container: Container {
                class: container.class_name.map(str::to_string),
                width: container.style_width,
                height: container.style_height,
                border: container.add_border,
            },
            outlines,
            placed: lay::Placed { plan: lay::Plan::default(), signs: Vec::new() },
        };
        layout.fill(&placed, &ids, upem);
        layout.placed = placed;
        Ok(layout)
    }

    /// Parse and lay out `text`, and print it.
    pub fn render(&mut self, text: &str, kind: SvgKind) -> Result<String, Error> {
        let doc = parse(text)?;
        self.layout(&doc)?.to_svg(kind)
    }
}

/// `translate(tx ty) scale(sx sy) rotate(r) translate(x0 y0) scale(k -k)` as a matrix
/// `[a b c d e f]` (`x' = a x + c y + e`).
fn matrix(p: &lay::PlacedSign, upem: f64) -> [f64; 6] {
    type M = [f64; 6];
    let mul = |m: M, n: M| -> M {
        [
            m[0] * n[0] + m[2] * n[1],
            m[1] * n[0] + m[3] * n[1],
            m[0] * n[2] + m[2] * n[3],
            m[1] * n[2] + m[3] * n[3],
            m[0] * n[4] + m[2] * n[5] + m[4],
            m[1] * n[4] + m[3] * n[5] + m[5],
        ]
    };
    let rotate = if p.rotate != 0.0 && !p.rotate.is_nan() { p.rotate.to_radians() } else { 0.0 };
    let (s, c) = rotate.sin_cos();
    let k = p.font_size / upem;
    let m = [1.0, 0.0, 0.0, 1.0, p.x_trans, p.y_trans];
    let m = mul(m, [p.sx(), 0.0, 0.0, p.y_scale, 0.0, 0.0]);
    let m = mul(m, [c, s, -s, c, 0.0, 0.0]);
    let m = mul(m, [1.0, 0.0, 0.0, 1.0, -p.width / 2.0, p.height / 2.0]);
    let m = mul(m, [k, 0.0, 0.0, -k, 0.0, 0.0]);
    m.map(|v| v / lay::EM_UNITS)
}

impl Layout {
    fn fill(&mut self, placed: &lay::Placed, ids: &[Option<u16>], upem: f64) {
        let mut next = 0;
        for (ui, u) in placed.plan.units.iter().enumerate() {
            self.units.push(Unit { width: u.w, height: u.h, reversed: u.reversed });
            for item in &u.items {
                match item {
                    lay::Item::Sign { .. } => {
                        let Some(p) = placed.signs.get(next) else { break };
                        let id = ids.get(next).copied().flatten();
                        next += 1;
                        let role = if p.props.bracket {
                            GlyphRole::Bracket
                        } else if p.props.unselectable {
                            GlyphRole::Variant
                        } else {
                            GlyphRole::Sign
                        };
                        let r = p.rect;
                        self.glyphs.push(Glyph {
                            unit: ui,
                            ch: p.ch,
                            glyph_id: id,
                            role,
                            transform: matrix(p, upem),
                            rect: Rect { x: r.x / lay::EM_UNITS, y: r.y / lay::EM_UNITS, w: r.w / lay::EM_UNITS, h: r.h / lay::EM_UNITS },
                        });
                    }
                    lay::Item::Shade(r) => {
                        let x = if u.reversed { u.w - (r.x + r.w) } else { r.x };
                        self.shades.push(Shade {
                            unit: ui,
                            rect: Rect { x, y: r.y, w: r.w, h: r.h },
                            hatch_phase: ((u.w_accum + u.h_accum) % 200.0) / lay::EM_UNITS,
                        });
                    }
                    lay::Item::Log { start, len } => self.logs.push(placed.plan.str(*start, *len).to_string()),
                    lay::Item::Hidden { .. } => {}
                }
            }
        }
    }
}
