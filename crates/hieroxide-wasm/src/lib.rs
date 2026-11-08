//! WebAssembly entry points of Hieroxide: the canvas raster backend and the functions the
//! drop-in script calls.
//!
//! There is no `web-sys` and no `js-sys`: the browser side is a small JS host object,
//! `hieroxideHost`, with three functions (`advance`, `draw` and `hasId`) that the shim in
//! `js/compat/` defines. Everything else is the core crate.

use hieroxide_core::layout::{
    self, Bitmap, Container, DocIds, DrawCmd, Engine, LayoutError, Layouter, Options, RawOptions, Raster, RasterError,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_namespace = hieroxideHost)]
extern "C" {
    /// `ctx.measureText(String.fromCodePoint(ch)).width` on a fresh canvas whose font is
    /// `fontSize + 'px Hieroglyphic'`.
    #[wasm_bindgen(catch, js_name = advance)]
    fn host_advance(ch: u32, font_size: f64) -> Result<f64, JsValue>;

    /// Creates a canvas of `w` by `h`, runs the draw commands (eight numbers each in
    /// `params`: font size, translate x and y, scale x and y, rotation, x and y of the
    /// text; the character of each in `chars`), and returns `getImageData(...).data`.
    #[wasm_bindgen(catch, js_name = draw)]
    fn host_draw(w: u32, h: u32, params: &[f64], chars: &[u32]) -> Result<Vec<u8>, JsValue>;

    /// `performance.now()`, for the benchmarks' split of where the time goes.
    #[wasm_bindgen(js_name = now)]
    fn host_now() -> f64;

    /// `document.getElementById(id) !== null`
    #[wasm_bindgen(catch, js_name = hasId)]
    fn host_has_id(id: &str) -> Result<bool, JsValue>;
}

fn host_err(e: JsValue) -> RasterError {
    RasterError(e.as_string().unwrap_or_else(|| "exception in the host".to_string()))
}

/// The canvas backend: advances and bitmaps from the page's canvas, through the host.
#[derive(Default)]
pub struct CanvasRaster {
    params: Vec<f64>,
    chars: Vec<u32>,
}

impl Raster for CanvasRaster {
    fn advance(&mut self, ch: char, font_size: f64) -> Result<f64, RasterError> {
        host_advance(ch as u32, font_size).map_err(host_err)
    }

    fn draw(&mut self, w: u32, h: u32, cmds: &[DrawCmd]) -> Result<Bitmap, RasterError> {
        self.params.clear();
        self.chars.clear();
        for c in cmds {
            self.chars.push(c.ch as u32);
            self.params.extend_from_slice(&[c.font_size, c.tx, c.ty, c.sx, c.sy, c.rotate, c.x, c.y]);
        }
        let data = host_draw(w, h, &self.params, &self.chars).map_err(host_err)?;
        let want = (w as usize).checked_mul(h as usize).and_then(|n| n.checked_mul(4));
        if want != Some(data.len()) {
            return Err(RasterError("the host returned a bitmap of the wrong size".to_string()));
        }
        Ok(Bitmap { w, h, data })
    }
}

struct HostIds;

impl DocIds for HostIds {
    fn has_id(&mut self, id: &str) -> Result<bool, RasterError> {
        host_has_id(id).map_err(host_err)
    }
}

/// Result of `Compat::process`.
const OK: u32 = 0;
const PARSE_ERROR: u32 = 1;
const UNSUPPORTED: u32 = 2;
const FAILED: u32 = 3;

/// Parse a fragment and give back what its `toString()` returns, or throw the error text.
/// For the drop-in's `syntax.parse(...).toString()` (the test suite page calls it).
#[wasm_bindgen]
pub fn parse_round_trip(text: &[u16]) -> Result<String, JsValue> {
    match hieroxide_core::parse_utf16(text) {
        Ok(tree) => Ok(tree.to_string_value()),
        Err(e) => Err(JsValue::from_str(&e.to_string())),
    }
}

/// The drop-in script's engine: one memo, and the result of the last fragment.
#[wasm_bindgen]
pub struct Compat {
    engine: Engine,
    raster: CanvasRaster,
    rendered: Option<layout::Rendered>,
    message: String,
    /// Time the phases of `process` (for the benchmarks); off by default.
    profile: bool,
    /// Milliseconds spent in parse, format and fit, and print, for the last fragment.
    timings: [f64; 3],
}

#[wasm_bindgen]
impl Compat {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Compat {
        Compat {
            engine: Engine::new(),
            raster: CanvasRaster::default(),
            rendered: None,
            message: String::new(),
            profile: false,
            timings: [0.0; 3],
        }
    }

    /// Whether `process` times its phases, readable with `timings`.
    pub fn set_profile(&mut self, on: bool) {
        self.profile = on;
    }

    /// Milliseconds the last `process` spent in parse, in format and fit, and in print
    /// (laying out the calls and building the markup); zero unless profiling is on.
    pub fn timings(&self) -> Vec<f64> {
        self.timings.to_vec()
    }

    /// `PrintedSVG.EXTERNAL_FONT`
    pub fn set_external_font(&mut self, url: &str) {
        self.engine.external_font = url.to_string();
    }

    /// Parse and lay out one fragment: the text as UTF-16 (so that a lone surrogate is an
    /// error, as in upstream), the computed font size in pixels and colour, the thirteen
    /// `data-*` values in the order of `DATA_ATTRIBUTES` (empty when absent), and whether
    /// the element is in the document. Returns a status; the other methods read the result.
    pub fn process(
        &mut self,
        text: &[u16],
        font_px: f64,
        sign_color: &str,
        attrs: Vec<String>,
        attached: bool,
    ) -> u32 {
        self.rendered = None;
        self.message.clear();
        self.timings = [0.0; 3];
        let t0 = self.now();
        let tree = match hieroxide_core::parse_utf16(text) {
            Ok(t) => t,
            Err(e) => {
                self.message = e.to_string();
                return PARSE_ERROR;
            }
        };
        let raw = RawOptions::from_parts(font_px, sign_color, &attrs);
        let t1 = self.now();
        self.timings[0] = t1 - t0;
        match self.render(&tree, &raw, attached, t1) {
            Ok(r) => {
                self.rendered = Some(r);
                OK
            }
            Err(LayoutError::Unsupported(u)) => {
                self.message = u.as_str().to_string();
                UNSUPPORTED
            }
            Err(e) => {
                self.message = e.to_string();
                FAILED
            }
        }
    }

    /// The parse error, the reason a fragment is unsupported, or the failure.
    pub fn message(&self) -> String {
        self.message.clone()
    }

    /// The markup to append to the element.
    pub fn markup(&self) -> String {
        self.rendered.as_ref().map(|r| r.svg.markup.clone()).unwrap_or_default()
    }

    /// The messages for `console.error`.
    pub fn logs(&self) -> Vec<String> {
        self.rendered.as_ref().map(|r| r.svg.logs.clone()).unwrap_or_default()
    }

    /// `element.className = ...`, or empty for no change.
    pub fn container_class(&self) -> String {
        self.container().and_then(|c| c.class_name).unwrap_or("").to_string()
    }

    /// `element.style['width']`, or empty.
    pub fn container_width(&self) -> String {
        self.container().and_then(|c| c.style_width.clone()).unwrap_or_default()
    }

    /// `element.style['height']`, or empty.
    pub fn container_height(&self) -> String {
        self.container().and_then(|c| c.style_height.clone()).unwrap_or_default()
    }

    /// Whether to add the class `hierojax-border` to the element.
    pub fn container_border(&self) -> bool {
        self.container().is_some_and(|c| c.add_border)
    }

    /// How many glyph measurements the memo holds (for tests).
    pub fn memo_len(&self) -> usize {
        self.engine.memo_len()
    }
}

impl Compat {
    fn now(&self) -> f64 {
        if self.profile { host_now() } else { 0.0 }
    }

    /// `layout::render_svg`, in its two halves so that each can be timed.
    fn render(
        &mut self,
        tree: &hieroxide_core::Tree,
        raw: &RawOptions,
        attached: bool,
        t1: f64,
    ) -> Result<layout::Rendered, LayoutError> {
        let opts = Options::from_raw(raw);
        let profile = self.profile;
        let (plan, t2) = {
            let mut layouter = Layouter::new(tree, &mut self.engine, &mut self.raster);
            layouter.format_for(&opts)?;
            let t2 = if profile { host_now() } else { 0.0 };
            (layouter.print_plan(&opts)?, t2)
        };
        let svg = layout::print_svg(&mut self.engine, &mut self.raster, &mut HostIds, &plan, &opts, attached)?;
        if self.profile {
            let t3 = host_now();
            self.timings[1] = t2 - t1;
            self.timings[2] = t3 - t2;
        }
        Ok(layout::Rendered { container: plan.container, svg })
    }

    fn container(&self) -> Option<&Container> {
        self.rendered.as_ref().map(|r| &r.container)
    }
}

impl Default for Compat {
    fn default() -> Self {
        Compat::new()
    }
}
