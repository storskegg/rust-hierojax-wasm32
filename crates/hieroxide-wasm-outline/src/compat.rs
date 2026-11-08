//! `Compat` over outlines: the same methods as the class in `hieroxide-wasm`, so the drop-in shim
//! runs on it unchanged, with upstream's layout and printer. Its numbers come from outlines
//! instead of the page's canvas. Two uses: the fallback for a browser whose canvas readback is
//! corrupt, and measuring the raster against upstream (`tests/golden/README.md`).

use std::cell::Cell;

use hieroxide_core::layout::{
    self, Bitmap, Container, DocIds, DrawCmd, Engine, JsFault, LayoutError, Layouter, Options, Raster, RasterError, RawOptions,
};
use hieroxide_outline::OutlineRaster;
use wasm_bindgen::prelude::*;

use super::keep_font;

#[wasm_bindgen(js_namespace = hieroxideHost)]
extern "C" {
    /// See `hieroxide-wasm`. Called only with a side of 0, for the page's own exception.
    #[wasm_bindgen(catch, js_name = draw)]
    fn host_draw(w: f64, h: f64, params: &[f64], chars: &[u32]) -> Result<Vec<u8>, JsValue>;

    /// `document.getElementById(id) !== null`
    #[wasm_bindgen(catch, js_name = hasId)]
    fn host_has_id(id: &str) -> Result<bool, JsValue>;
}

thread_local! {
    static FONT: Cell<Option<&'static [u8]>> = const { Cell::new(None) };
}

/// The font's bytes: the shim's `new Compat()` takes no arguments, so they come first.
#[wasm_bindgen]
pub fn set_font(font: &[u8]) {
    FONT.with(|f| f.set(Some(keep_font(font))));
}

struct Ras {
    inner: OutlineRaster<'static>,
    exception: Option<JsValue>,
}

impl Raster for Ras {
    fn advance(&mut self, ch: char, font_size: f64) -> Result<f64, RasterError> {
        self.inner.advance(ch, font_size)
    }
    fn draw(&mut self, w: f64, h: f64, cmds: &[DrawCmd]) -> Result<Bitmap, RasterError> {
        match self.inner.draw(w, h, cmds) {
            // The canvas of a real page throws here: let it, for its exception in its wording.
            Err(e) => {
                if let Err(x) = host_draw(w, h, &[], &[]) {
                    self.exception = Some(x);
                }
                Err(e)
            }
            ok => ok,
        }
    }
    fn pow(&mut self, b: f64, e: f64) -> f64 {
        self.inner.pow(b, e)
    }
    fn log(&mut self, x: f64) -> f64 {
        self.inner.log(x)
    }
}

struct HostIds;

impl DocIds for HostIds {
    fn has_id(&mut self, id: &str) -> Result<bool, RasterError> {
        host_has_id(id).map_err(|_| RasterError("exception in the host".to_string()))
    }
}

const OK: u32 = 0;
const PARSE_ERROR: u32 = 1;
const UNSUPPORTED: u32 = 2;
const FAILED: u32 = 3;
const THREW: u32 = 4;
const FAULT: u32 = 5;

#[wasm_bindgen]
pub struct Compat {
    engine: Engine,
    raster: Ras,
    rendered: Option<layout::Rendered>,
    message: String,
    fault: Option<JsFault>,
}

#[wasm_bindgen]
impl Compat {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<Compat, JsValue> {
        let font = FONT.with(Cell::get).ok_or_else(|| JsValue::from_str("set_font first"))?;
        let inner = OutlineRaster::new(font).map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(Compat {
            engine: Engine::new(),
            raster: Ras { inner, exception: None },
            rendered: None,
            message: String::new(),
            fault: None,
        })
    }

    pub fn set_profile(&mut self, _on: bool) {}

    pub fn timings(&self) -> Vec<f64> {
        vec![0.0; 3]
    }

    pub fn set_external_font(&mut self, url: &str) {
        self.engine.external_font = url.to_string();
    }

    pub fn process(&mut self, text: &[u16], font_px: f64, sign_color: &str, attrs: Vec<String>, attached: bool) -> u32 {
        self.rendered = None;
        self.message.clear();
        self.fault = None;
        self.raster.exception = None;
        let tree = match hieroxide_core::parse_utf16(text) {
            Ok(t) => t,
            Err(e) => {
                self.message = e.to_string();
                return PARSE_ERROR;
            }
        };
        let raw = RawOptions::from_parts(font_px, sign_color, &attrs);
        match self.render(&tree, &raw, attached) {
            Ok(r) => {
                self.rendered = Some(r);
                OK
            }
            Err(LayoutError::Unsupported(u)) => {
                self.message = u.as_str().to_string();
                UNSUPPORTED
            }
            Err(LayoutError::Js(f)) => {
                self.fault = Some(f);
                FAULT
            }
            Err(e) => {
                self.message = e.to_string();
                if self.raster.exception.is_some() { THREW } else { FAILED }
            }
        }
    }

    pub fn take_exception(&mut self) -> JsValue {
        self.raster.exception.take().unwrap_or(JsValue::UNDEFINED)
    }

    pub fn fault_kind(&self) -> u32 {
        match self.fault {
            Some(JsFault::InvalidArrayLength(_)) => 1,
            Some(JsFault::LigatureUndefined) => 2,
            None => 0,
        }
    }

    pub fn fault_value(&self) -> f64 {
        match self.fault {
            Some(JsFault::InvalidArrayLength(n)) => n,
            _ => f64::NAN,
        }
    }

    pub fn message(&self) -> String {
        self.message.clone()
    }

    pub fn markup(&self) -> String {
        self.rendered.as_ref().map(|r| r.svg.markup.clone()).unwrap_or_default()
    }

    pub fn logs(&self) -> Vec<String> {
        self.rendered.as_ref().map(|r| r.svg.logs.clone()).unwrap_or_default()
    }

    pub fn container_class(&self) -> String {
        self.container().and_then(|c| c.class_name).unwrap_or("").to_string()
    }

    pub fn container_width(&self) -> String {
        self.container().and_then(|c| c.style_width.clone()).unwrap_or_default()
    }

    pub fn container_height(&self) -> String {
        self.container().and_then(|c| c.style_height.clone()).unwrap_or_default()
    }

    pub fn container_border(&self) -> bool {
        self.container().is_some_and(|c| c.add_border)
    }

    pub fn memo_len(&self) -> usize {
        self.engine.memo_len()
    }
}

impl Compat {
    fn render(&mut self, tree: &hieroxide_core::Tree, raw: &RawOptions, attached: bool) -> Result<layout::Rendered, LayoutError> {
        let opts = Options::from_raw(raw);
        let plan = {
            let mut layouter = Layouter::new(tree, &mut self.engine, &mut self.raster);
            layouter.plan(&opts)?
        };
        let (svg, failure) = layout::print_svg_partial(&mut self.engine, &mut self.raster, &mut HostIds, &plan, &opts, attached);
        if let Some(e) = failure {
            self.rendered = Some(layout::Rendered { container: plan.container, svg });
            return Err(e);
        }
        Ok(layout::Rendered { container: plan.container, svg })
    }

    fn container(&self) -> Option<&Container> {
        self.rendered.as_ref().map(|r| &r.container)
    }
}
