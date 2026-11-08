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
//! Not yet ported (milestone M4a): the canvas and DOM printers. A fragment that needs one
//! gets [`LayoutError::Unsupported`], so a test harness can report it as not covered instead
//! of wrong.

mod basic;
mod canvas;
mod em;
mod enclosure;
mod group;
mod hull;
mod jsmath;
mod options;
mod overlay;
mod plan;
mod raster;
mod readback;
mod shapes;
mod svg;
#[cfg(test)]
mod tests;

pub use canvas::MeasCanvas;
pub use em::{print_em, render_consistent};
pub use group::{Layouter, Size};
pub use hull::{allowed_places, Hull, Point};
pub use jsmath::{get_max, get_min, get_sum, js_max, js_min};
pub use options::{Dir, LayoutOpts, Options, PrintKind, RawOptions, DATA_ATTRIBUTES, EM_UNITS};
pub use plan::{Container, Item, Plan, Rect, Unit};
pub use raster::{canvas_dim, Bitmap, BoxRaster, DrawCmd, Margins, Plane, Raster, RasterError};
pub use readback::{readback_is_sane, PROBE_RGBA, PROBE_SIDE, PROBE_X, PROBE_Y};
pub use shapes::{Engine, EmSize, Meas, Mode, SignProps, MEASURE_SIZE, REFERENCE_GLYPH};
pub use svg::{print_svg, print_svg_partial, DocIds, NoIds, SvgOutput};

use crate::tree::Tree;

/// What is not ported yet, as the harness reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unsupported {
    /// `type=canvas`
    CanvasPrinter,
    /// `type=dom` (and any type that is not `svg` or `canvas`)
    DomPrinter,
    /// A `data-sep` so large (millions of em) that upstream would spend minutes building
    /// arrays with hundreds of millions of entries, or never finish.
    SpanTooLarge,
}

impl Unsupported {
    pub fn as_str(self) -> &'static str {
        match self {
            Unsupported::CanvasPrinter => "canvas printer",
            Unsupported::DomPrinter => "dom printer",
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
    /// Consistent mode: the layout came to a number that cannot be printed (an infinity or NaN).
    NotFinite,
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
            // Not `{:?}`: printing a float in Debug form links a good deal of core's float code.
            LayoutError::Js(JsFault::InvalidArrayLength(_)) => f.write_str("upstream throws: invalid array length"),
            LayoutError::Js(JsFault::LigatureUndefined) => f.write_str("upstream throws: no ligature entry"),
            LayoutError::Internal(m) => write!(f, "internal error: {m}"),
            LayoutError::NotFinite => f.write_str("the layout has no finite size"),
        }
    }
}

impl std::error::Error for LayoutError {}

/// The result of laying out and printing one fragment as SVG.
#[derive(Clone, Debug, PartialEq)]
pub struct Rendered {
    /// What `Fragment.print` does to the element itself.
    pub container: Container,
    /// The markup to append to the element, and the messages for `console.error`.
    pub svg: SvgOutput,
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
    let opts = Options::from_raw(raw);
    let plan = {
        let mut layouter = Layouter::new(tree, engine, raster);
        layouter.plan(&opts)?
    };
    let svg = print_svg(engine, raster, doc, &plan, &opts, attached)?;
    Ok(Rendered { container: plan.container, svg })
}
