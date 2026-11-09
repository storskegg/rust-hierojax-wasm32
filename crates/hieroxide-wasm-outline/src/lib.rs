//! WebAssembly entry points with the outline raster. No imports from the page in the default
//! build: everything the module computes is its own, so it gives the same bytes everywhere.
//!
//! With the feature `compat-api`, it also has `Compat`, the interface of `hieroxide-wasm`'s
//! class of that name over outlines: see `tools/build-consistent.mjs`.

use hieroxide_core::layout::{self, Engine, Mode, RawOptions, Rendered};
use hieroxide_outline::OutlineRaster;
use wasm_bindgen::prelude::*;

/// Result of [`Hieroxide::render`].
const OK: u32 = 0;
const PARSE_ERROR: u32 = 1;
const FAILED: u32 = 3;
/// What was asked for is not in this build: path output, in the build without the feature `paths`.
#[cfg(not(feature = "paths"))]
const UNSUPPORTED: u32 = 4;

/// The extra values `render` takes after the thirteen of `DATA_ATTRIBUTES`: the `aria-label` of a path
/// block, the kind of output (`paths`, or empty for text; `parse` stops after parsing), how the block carries its copyable text
/// (`overlay`, `svg` or `none`), and `false` to leave `data-text` out and `true` to have `xmlns`.
#[cfg(feature = "paths")]
const EXTRA_LABEL: usize = 13;
const EXTRA_KIND: usize = 14;
#[cfg(feature = "paths")]
const EXTRA_SELECTABLE: usize = 15;
#[cfg(feature = "paths")]
const EXTRA_DATA_TEXT: usize = 16;
#[cfg(feature = "paths")]
const EXTRA_XMLNS: usize = 17;

/// The font for as long as the module lives: a page has one, and giving the raster a borrowed
/// slice spares it all the rest.
fn keep_font(font: &[u8]) -> &'static [u8] {
    Box::leak(font.to_vec().into_boxed_slice())
}

/// One engine in consistent mode with its font. Errors are results: nothing here throws but the
/// constructor, for a font that cannot be used.
#[wasm_bindgen]
pub struct Hieroxide {
    engine: Engine,
    raster: OutlineRaster<'static>,
    rendered: Option<Rendered>,
    message: String,
    /// Where a parse error is, in UTF-16 code units.
    position: u32,
}

#[wasm_bindgen]
impl Hieroxide {
    /// From the bytes of the unmodified font.
    #[wasm_bindgen(constructor)]
    pub fn new(font: &[u8]) -> Result<Hieroxide, JsValue> {
        let raster = OutlineRaster::new(keep_font(font)).map_err(|e| JsValue::from_str(&e.to_string()))?.lenient();
        Ok(Hieroxide {
            engine: Engine::with_mode(Mode::Consistent),
            raster,
            rendered: None,
            message: String::new(),
            position: 0,
        })
    }

    /// The URL in the `@font-face` of a standalone SVG.
    pub fn set_external_font(&mut self, url: &str) {
        self.engine.external_font = url.to_string();
    }

    /// Parse and lay out one fragment: the text as UTF-16 and the option values in the order of
    /// `DATA_ATTRIBUTES`, then the extra ones (empty when absent). 0 is success; otherwise `message`
    /// says what went wrong (1, the text does not parse, and `position` where; 3, it could not be
    /// laid out; 4, the kind of output is not in this build).
    pub fn render(&mut self, text: &[u16], attrs: Vec<String>) -> u32 {
        self.rendered = None;
        self.message.clear();
        let tree = match hieroxide_core::parse_utf16(text) {
            Ok(t) => t,
            Err(e) => {
                self.message = e.to_string();
                self.position = e.pos as u32;
                return PARSE_ERROR;
            }
        };
        let kind = attrs.get(EXTRA_KIND).map_or("", String::as_str);
        if kind == "parse" {
            return OK;
        }
        let raw = RawOptions::from_parts(f64::NAN, "", &attrs);
        let paths = kind == "paths";
        let done = if paths {
            #[cfg(feature = "paths")]
            {
                self.render_paths(&tree, &raw, &attrs, text)
            }
            #[cfg(not(feature = "paths"))]
            {
                self.message = "this build has no path output".to_string();
                return UNSUPPORTED;
            }
        } else {
            layout::render_consistent(&mut self.engine, &tree, &raw, &mut self.raster)
        };
        match done {
            Ok(r) => {
                self.rendered = Some(r);
                OK
            }
            Err(e) => {
                self.message = e.to_string();
                FAILED
            }
        }
    }

    /// The error of the last `render`.
    pub fn message(&self) -> String {
        self.message.clone()
    }

    /// The markup of the last `render`.
    pub fn markup(&self) -> String {
        self.rendered.as_ref().map(|r| r.svg.markup.clone()).unwrap_or_default()
    }

    /// Where the parse error of the last `render` is, in UTF-16 code units.
    pub fn position(&self) -> u32 {
        self.position
    }

    /// What `log` asked for.
    pub fn logs(&self) -> Vec<String> {
        self.rendered.as_ref().map(|r| r.svg.logs.clone()).unwrap_or_default()
    }

    /// The class of the element, or empty.
    pub fn container_class(&self) -> String {
        self.rendered.as_ref().and_then(|r| r.container.class_name).unwrap_or("").to_string()
    }

    /// The width of the element, or empty.
    pub fn container_width(&self) -> String {
        self.rendered.as_ref().and_then(|r| r.container.style_width.clone()).unwrap_or_default()
    }

    /// The height of the element, or empty.
    pub fn container_height(&self) -> String {
        self.rendered.as_ref().and_then(|r| r.container.style_height.clone()).unwrap_or_default()
    }

    /// Whether the element gets the class `hierojax-border`.
    pub fn container_border(&self) -> bool {
        self.rendered.as_ref().is_some_and(|r| r.container.add_border)
    }

    /// How many glyph measurements the engine holds.
    pub fn memo_len(&self) -> usize {
        self.engine.memo_len()
    }
}

#[cfg(feature = "paths")]
struct Outlines {
    upem: f64,
    glyphs: Vec<(char, hieroxide_outline::Shared<hieroxide_outline::Glyph>)>,
}

#[cfg(feature = "paths")]
impl layout::Outlines for Outlines {
    fn units_per_em(&self) -> f64 {
        self.upem
    }
    fn outline(&self, ch: char) -> Option<&[layout::Seg]> {
        let i = self.glyphs.binary_search_by_key(&ch, |g| g.0).ok()?;
        Some(&self.glyphs[i].1.path)
    }
}

#[cfg(feature = "paths")]
impl Hieroxide {
    /// A block of paths. The element the page gives has nothing to say about it: it is a whole
    /// block with its own `svg`, so the container comes back empty.
    fn render_paths(&mut self, tree: &hieroxide_core::Tree, raw: &RawOptions, attrs: &[String], text: &[u16]) -> Result<Rendered, layout::LayoutError> {
        let (opts, css_font_px) = layout::Options::consistent(raw);
        let plan = {
            let mut layouter = layout::Layouter::new(tree, &mut self.engine, &mut self.raster);
            layouter.plan(&opts)?
        };
        let placed = layout::place(&mut self.engine, &mut self.raster, plan)?;
        let mut chars: Vec<char> = placed.signs.iter().map(|p| p.ch).collect();
        chars.sort_unstable();
        chars.dedup();
        let outlines = Outlines {
            upem: self.raster.units_per_em(),
            glyphs: chars.into_iter().filter_map(|c| self.raster.outline(c).map(|g| (c, g))).collect(),
        };
        let at = |i: usize| attrs.get(i).map_or("", String::as_str);
        let source = String::from_utf16(text).map_err(|_| layout::LayoutError::Internal("text is not UTF-16"))?;
        let params = layout::PathParams {
            label: if at(EXTRA_LABEL).is_empty() { "Egyptian hieroglyphs" } else { at(EXTRA_LABEL) },
            selectable: match at(EXTRA_SELECTABLE) {
                "svg" => layout::Selectable::Svg,
                "none" => layout::Selectable::Off,
                _ => layout::Selectable::Overlay,
            },
            data_text: at(EXTRA_DATA_TEXT) != "false",
            source: &source,
            xmlns: at(EXTRA_XMLNS) == "true",
            inline_style: true,
            css_font_px,
        };
        let svg = layout::write_paths(&placed, &opts, &params, &outlines)?;
        Ok(Rendered { container: layout::Container::default(), svg, canvas: None })
    }
}

/// [`hieroxide_outline::determinism_digest`], in hex: the same on every engine and natively means
/// `pow`, `log`, `sin` and `cos` agree to the bit.
#[wasm_bindgen]
pub fn determinism_digest_hex() -> String {
    let d = hieroxide_outline::determinism_digest();
    let mut s = String::with_capacity(16);
    for i in (0..16).rev() {
        s.push(char::from_digit(((d >> (i * 4)) & 15) as u32, 16).unwrap_or('0'));
    }
    s
}

#[cfg(feature = "compat-api")]
mod compat;
#[cfg(feature = "compat-api")]
pub use compat::*;
