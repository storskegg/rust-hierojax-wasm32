//! Layout: a port of `src/formatting.js` of hierojax.
//!
//! * [`Raster`] is the one seam to a glyph rasterizer (advance widths and bitmaps).
//! * [`Engine`] owns upstream's global measurement memo.
//! * [`Layouter`] is `size`, `fit`, `format` and `print` of the groups; it records a
//!   [`Plan`], the flat list of what upstream's `print` asks a printer to do.
//! * [`print_svg`] turns a plan into the markup of upstream's `PrintedSVG`.
//!
//! Nothing here depends on a browser, and there is no global state.
//!
//! * [`MeasCanvas`] is `PrintedCanvasWithoutExtras`, and `hull` holds what upstream computes
//!   from its pixels.
//!
//! * [`print_canvas`] and [`print_dom`] are `PrintedCanvas` and `PrintedDOM`: the first gives
//!   a flat buffer of canvas operations for a host loop, the second markup.

mod basic;
mod canvas;
mod canvas_out;
mod dom;
mod enclosure;
mod group;
mod hull;
mod jsmath;
mod options;
mod overlay;
mod plan;
mod raster;
mod shapes;
mod svg;
#[cfg(test)]
mod printer_tests;
#[cfg(test)]
mod tests;

pub use canvas::MeasCanvas;
pub use canvas_out::{print_canvas_partial, CanvasOutput, OP_END, OP_HATCH, OP_SIGN, OP_UNIFORM};
pub use dom::print_dom_partial;
pub use group::{Layouter, Size};
pub use hull::{allowed_places, Hull, Point};
pub use jsmath::{get_max, get_min, get_sum, js_max, js_min};
pub use options::{Dir, LayoutOpts, Options, PrintKind, RawOptions, DATA_ATTRIBUTES};
pub use plan::{Container, Item, Plan, Rect, Unit};
pub use raster::{canvas_dim, Bitmap, BoxRaster, DrawCmd, Margins, Plane, Raster, RasterError};
pub use shapes::{Engine, EmSize, Meas, SignProps, MEASURE_SIZE, REFERENCE_GLYPH};
pub use svg::{print_svg, print_svg_partial, DocIds, NoIds, SvgOutput};

use crate::tree::Tree;

/// What is not ported yet, as the harness reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unsupported {
    /// A `data-sep` so large (millions of em) that upstream would spend minutes building
    /// arrays with hundreds of millions of entries, or never finish.
    SpanTooLarge,
}

impl Unsupported {
    pub fn as_str(self) -> &'static str {
        match self {
            Unsupported::SpanTooLarge => "span too large",
        }
    }
}

/// A place where upstream's own JavaScript throws, whose message each engine words in its
/// own way. The host makes the same mistake in the same words, so the exception is the
/// engine's own (see `js/compat/hierojax.js`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JsFault {
    /// `Array(nSteps)` in `Basic.fitGrow`, for a length that is not an unsigned 32-bit
    /// integer: a `RangeError`.
    InvalidArrayLength(f64),
    /// `lig.horizontal` in `Overlay.formatLigature`, where `Shapes.ligatures[this.alt]` is
    /// `undefined`: a `TypeError`. An insertion alternative of U+13353 names a glyph that has
    /// no ligature entry.
    LigatureUndefined,
}

/// Why a fragment could not be laid out.
#[derive(Clone, Debug, PartialEq)]
pub enum LayoutError {
    /// Not ported yet, see [`Unsupported`].
    Unsupported(Unsupported),
    /// The raster backend failed (upstream would have thrown).
    Raster(RasterError),
    /// Upstream's own code throws here; see [`JsFault`].
    Js(JsFault),
    /// An inconsistency in the tree or the layout state; cannot happen for a parsed tree.
    Internal(&'static str),
}

impl From<RasterError> for LayoutError {
    fn from(e: RasterError) -> Self {
        LayoutError::Raster(e)
    }
}

impl core::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LayoutError::Unsupported(u) => write!(f, "unsupported: {}", u.as_str()),
            LayoutError::Raster(e) => write!(f, "raster: {e}"),
            LayoutError::Js(j) => write!(f, "upstream throws: {j:?}"),
            LayoutError::Internal(m) => write!(f, "internal error: {m}"),
        }
    }
}

impl std::error::Error for LayoutError {}

/// The result of laying out and printing one fragment.
#[derive(Clone, Debug, PartialEq)]
pub struct Rendered {
    /// What `Fragment.print` does to the element itself.
    pub container: Container,
    /// The markup to append to the element (SVG, or the `div`s of `type=dom`), and the
    /// messages for `console.error`.
    pub svg: SvgOutput,
    /// For `type=canvas`: the operations to replay, instead of markup.
    pub canvas: Option<CanvasOutput>,
}

/// `Fragment.print` for `type=svg`: format, then print. `attached` says whether the
/// element is in the document (for the shade-pattern ids).
pub fn render_svg<Ra: Raster, D: DocIds>(
    engine: &mut Engine,
    tree: &Tree,
    raw: &RawOptions,
    raster: &mut Ra,
    doc: &mut D,
    attached: bool,
) -> Result<Rendered, LayoutError> {
    render(engine, tree, raw, raster, doc, attached, false)
}

/// `Fragment.print` for any `type`. `linux` is `isLinux()`, which `type=dom` asks.
pub fn render<Ra: Raster, D: DocIds>(
    engine: &mut Engine,
    tree: &Tree,
    raw: &RawOptions,
    raster: &mut Ra,
    doc: &mut D,
    attached: bool,
    linux: bool,
) -> Result<Rendered, LayoutError> {
    let opts = Options::from_raw(raw);
    let plan = {
        let mut layouter = Layouter::new(tree, engine, raster);
        layouter.plan(&opts)?
    };
    let (out, failure) = print_plan(engine, raster, doc, tree, &plan, &opts, attached, linux);
    match failure {
        Some(e) => Err(e),
        None => Ok(Rendered { container: plan.container, svg: out.0, canvas: out.1 }),
    }
}

/// The printing half of `Fragment.print`, by `type`. On an exception it also gives what
/// upstream would have left in the element by then.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn print_plan<Ra: Raster, D: DocIds>(
    engine: &mut Engine,
    raster: &mut Ra,
    doc: &mut D,
    tree: &Tree,
    plan: &Plan,
    opts: &Options,
    attached: bool,
    linux: bool,
) -> ((SvgOutput, Option<CanvasOutput>), Option<LayoutError>) {
    match opts.kind {
        PrintKind::Svg => {
            let (o, f) = print_svg_partial(engine, raster, doc, plan, opts, attached);
            ((o, None), f)
        }
        PrintKind::Dom => {
            let (o, f) = print_dom_partial(engine, raster, plan, opts, linux);
            ((o, None), f)
        }
        PrintKind::Canvas => {
            let text = tree.to_string_value();
            let (o, f) = print_canvas_partial(engine, raster, plan, opts, &text);
            let logs = o.logs.clone();
            ((SvgOutput { markup: String::new(), logs }, Some(o)), f)
        }
    }
}
