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
    text: String,
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
            text: String::new(),
        })
    }

    /// The URL in the `@font-face` of a standalone SVG.
    pub fn set_external_font(&mut self, url: &str) {
        self.engine.external_font = url.to_string();
    }

    /// Parse and lay out one fragment: the text as UTF-16 and the thirteen option values in the
    /// order of `DATA_ATTRIBUTES` (empty when absent). 0 is success; otherwise `message` says what
    /// went wrong (1, the text does not parse; 3, it could not be laid out).
    pub fn render(&mut self, text: &[u16], attrs: Vec<String>) -> u32 {
        self.rendered = None;
        self.message.clear();
        self.text.clear();
        let tree = match hieroxide_core::parse_utf16(text) {
            Ok(t) => t,
            Err(e) => {
                self.message = e.to_string();
                return PARSE_ERROR;
            }
        };
        self.text = tree.to_string_exact();
        let raw = RawOptions::from_parts(f64::NAN, "", &attrs);
        match layout::render_consistent(&mut self.engine, &tree, &raw, &mut self.raster) {
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

    /// The fragment's own text, as parsed.
    pub fn text(&self) -> String {
        self.text.clone()
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
