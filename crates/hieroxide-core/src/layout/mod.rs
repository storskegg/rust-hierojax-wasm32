//! Layout: a port of `src/formatting.js` of hierojax, for fragments that need no pixels
//! beyond measuring single glyphs.
//!
//! * [`Raster`] is the one seam to a glyph rasterizer (advance widths and bitmaps).
//! * [`Engine`] owns upstream's global measurement memo.
//! * [`Layouter`] is `size`, `fit`, `format` and `print` of the groups; it records a
//!   [`Plan`], the flat list of what upstream's `print` asks a printer to do.
//! * [`print_svg`] turns a plan into the markup of upstream's `PrintedSVG`.
//!
//! Nothing here depends on a browser, and there is no global state.
//!
//! Not yet ported (milestone M3): insertions (`Basic`), enclosures, overlays, ligatures,
//! and everything that scans pixels beyond a glyph's margins. A fragment that reaches
//! one of those gets [`LayoutError::Unsupported`], so a test harness can report it as not
//! covered instead of wrong. The canvas and DOM printers are not ported either.

mod group;
mod jsmath;
mod options;
mod plan;
mod raster;
mod shapes;
mod svg;
#[cfg(test)]
mod tests;

pub use group::{Layouter, Size};
pub use jsmath::{get_max, get_min, get_sum, js_max, js_min};
pub use options::{Dir, LayoutOpts, Options, PrintKind, RawOptions, DATA_ATTRIBUTES};
pub use plan::{Container, Item, Plan, Rect, Unit};
pub use raster::{Bitmap, BoxRaster, DrawCmd, Margins, Plane, Raster, RasterError};
pub use shapes::{Engine, EmSize, Meas, SignProps, MEASURE_SIZE, REFERENCE_GLYPH};
pub use svg::{print_svg, DocIds, NoIds, SvgOutput};

use crate::tree::Tree;

/// What is not ported yet, as the harness reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unsupported {
    /// An insertion (`Basic`): needs hulls and displacement.
    Insertion,
    /// An enclosure: needs kerning against its delimiters.
    Enclosure,
    /// An overlay.
    Overlay,
    /// A vertical or horizontal group that forms a ligature.
    Ligature,
    /// `type=canvas`
    CanvasPrinter,
    /// `type=dom` (and any type that is not `svg` or `canvas`)
    DomPrinter,
    /// A measuring canvas of an impossible size, where upstream would have thrown.
    CanvasSize,
}

impl Unsupported {
    pub fn as_str(self) -> &'static str {
        match self {
            Unsupported::Insertion => "insertion",
            Unsupported::Enclosure => "enclosure",
            Unsupported::Overlay => "overlay",
            Unsupported::Ligature => "ligature",
            Unsupported::CanvasPrinter => "canvas printer",
            Unsupported::DomPrinter => "dom printer",
            Unsupported::CanvasSize => "canvas size",
        }
    }
}

/// Why a fragment could not be laid out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutError {
    /// Not ported yet, see [`Unsupported`].
    Unsupported(Unsupported),
    /// The raster backend failed (upstream would have thrown).
    Raster(RasterError),
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
            LayoutError::Internal(m) => write!(f, "internal error: {m}"),
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
