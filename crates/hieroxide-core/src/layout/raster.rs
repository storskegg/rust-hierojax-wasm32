//! The seam between layout and whatever draws glyphs.
//!
//! Upstream measures with a canvas: `ctx.measureText(ch).width` for an advance, and
//! `fillText` onto a fresh canvas followed by `getImageData` for pixels. Everything
//! above those two operations (margins, and later hulls, distances, kerning and
//! insertion fitting) is plain computation, so it lives in this crate; only these two
//! operations are behind the trait.

use core::fmt;

/// A failure of the raster backend (upstream would have thrown).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RasterError(pub String);

impl fmt::Display for RasterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RasterError {}

/// One `fillText`, with the state it is drawn in. The drawing calls upstream makes for
/// one glyph on a measuring canvas are, in this order:
///
/// ```text
/// ctx.font = font_size + 'px Hieroglyphic'; ctx.fillStyle = 'black'; ctx.textBaseline = 'alphabetic';
/// ctx.translate(tx, ty); ctx.scale(sx, sy); ctx.rotate(rotate); ctx.fillText(ch, x, y);
/// ```
///
/// on a canvas of the size given to [`Raster::draw`], freshly created and resized. A
/// canvas backend replays exactly this; the colour is always black on these canvases
/// (`PrintedCanvasWithoutExtras` and `measureGlyph` both draw in black), which is why
/// it is not a field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawCmd {
    pub ch: char,
    pub font_size: f64,
    pub tx: f64,
    pub ty: f64,
    pub sx: f64,
    pub sy: f64,
    /// The argument of `ctx.rotate`, in radians, as computed (`rotate * Math.PI / 180`).
    pub rotate: f64,
    pub x: f64,
    pub y: f64,
}

/// Pixels as `getImageData(...).data` returns them: RGBA, 8 bits, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bitmap {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

/// What layout needs from a glyph rasterizer.
pub trait Raster {
    /// `ctx.measureText(ch).width` on a canvas whose font is `font_size + 'px Hieroglyphic'`.
    fn advance(&mut self, ch: char, font_size: f64) -> Result<f64, RasterError>;

    /// Create a `w` by `h` canvas, run the commands on it in order, and read it back.
    /// The result has `w * h * 4` bytes.
    fn draw(&mut self, w: u32, h: u32, cmds: &[DrawCmd]) -> Result<Bitmap, RasterError>;
}

/// A read-only view of a bitmap with upstream's dark test.
#[derive(Clone, Copy, Debug)]
pub struct Plane<'a> {
    pub w: u32,
    pub h: u32,
    data: &'a [u8],
}

impl<'a> Plane<'a> {
    pub fn new(bitmap: &'a Bitmap) -> Plane<'a> {
        Plane { w: bitmap.w, h: bitmap.h, data: &bitmap.data }
    }

    /// `Shapes.isDark(data, w, x, y)`: any of the four channels is nonzero. The index is
    /// computed as upstream does, `y * w * 4 + x * 4`, on a typed array: an index outside
    /// the array reads as `undefined`, which makes the sum `NaN` and the test false.
    pub fn is_dark(&self, x: i64, y: i64) -> bool {
        let idx = y
            .wrapping_mul(i64::from(self.w))
            .wrapping_mul(4)
            .wrapping_add(x.wrapping_mul(4));
        if idx < 0 {
            return false;
        }
        let Ok(i) = usize::try_from(idx) else { return false };
        match self.data.get(i..i.saturating_add(4)) {
            Some(px) => px.iter().any(|&c| c != 0),
            None => false,
        }
    }

    /// `Shapes.margins`: the number of empty rows above and below and empty columns left
    /// and right of the dark pixels (all four equal to the full extent when nothing is
    /// dark, as upstream's loops give).
    pub fn margins(&self) -> Margins {
        let (w, h) = (i64::from(self.w), i64::from(self.h));
        let mut t = 0;
        'top: for y in 0..h {
            for x in 0..w {
                if self.is_dark(x, y) {
                    break 'top;
                }
            }
            t += 1;
        }
        let mut b = 0;
        'bottom: for y in (0..h).rev() {
            for x in 0..w {
                if self.is_dark(x, y) {
                    break 'bottom;
                }
            }
            b += 1;
        }
        let mut l = 0;
        'left: for x in 0..w {
            for y in 0..h {
                if self.is_dark(x, y) {
                    break 'left;
                }
            }
            l += 1;
        }
        let mut r = 0;
        'right: for x in (0..w).rev() {
            for y in 0..h {
                if self.is_dark(x, y) {
                    break 'right;
                }
            }
            r += 1;
        }
        Margins { t, b, l, r }
    }
}

/// The result of [`Plane::margins`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Margins {
    pub t: i64,
    pub b: i64,
    pub l: i64,
    pub r: i64,
}

/// A raster for native tests: every glyph advances half its font size and draws as a
/// solid box of that advance by the font size, from the baseline up, through the
/// translate and scale of the command (rotation is ignored).
#[derive(Clone, Copy, Debug, Default)]
pub struct BoxRaster {
    pub advances: u32,
    pub draws: u32,
}

impl Raster for BoxRaster {
    fn advance(&mut self, _ch: char, font_size: f64) -> Result<f64, RasterError> {
        self.advances += 1;
        Ok(font_size * 0.5)
    }

    fn draw(&mut self, w: u32, h: u32, cmds: &[DrawCmd]) -> Result<Bitmap, RasterError> {
        self.draws += 1;
        let len = (w as usize)
            .checked_mul(h as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(|| RasterError("canvas too large".into()))?;
        let mut data = vec![0u8; len];
        for c in cmds {
            let adv = c.font_size * 0.5;
            let x0 = c.tx + c.sx * c.x;
            let x1 = c.tx + c.sx * (c.x + adv);
            let y0 = c.ty + c.sy * (c.y - c.font_size);
            let y1 = c.ty + c.sy * c.y;
            let (xa, xb) = (x0.min(x1).round().max(0.0) as i64, x0.max(x1).round().min(f64::from(w)) as i64);
            let (ya, yb) = (y0.min(y1).round().max(0.0) as i64, y0.max(y1).round().min(f64::from(h)) as i64);
            for y in ya..yb {
                for x in xa..xb {
                    let i = ((y * i64::from(w) + x) * 4) as usize;
                    if let Some(px) = data.get_mut(i..i + 4) {
                        px.copy_from_slice(&[0, 0, 0, 255]);
                    }
                }
            }
        }
        Ok(Bitmap { w, h, data })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bitmap(w: u32, h: u32, dark: &[(u32, u32)]) -> Bitmap {
        let mut data = vec![0u8; (w * h * 4) as usize];
        for &(x, y) in dark {
            data[((y * w + x) * 4 + 3) as usize] = 255;
        }
        Bitmap { w, h, data }
    }

    #[test]
    fn margins_of_a_box() {
        let b = bitmap(10, 8, &[(3, 2), (5, 4)]);
        let m = Plane::new(&b).margins();
        assert_eq!(m, Margins { t: 2, b: 3, l: 3, r: 4 });
    }

    #[test]
    fn margins_of_nothing_are_the_full_extent() {
        let b = bitmap(10, 8, &[]);
        let m = Plane::new(&b).margins();
        assert_eq!(m, Margins { t: 8, b: 8, l: 10, r: 10 });
    }

    #[test]
    fn dark_is_any_channel() {
        let mut b = bitmap(2, 1, &[]);
        b.data[0] = 1; // red only
        let p = Plane::new(&b);
        assert!(p.is_dark(0, 0));
        assert!(!p.is_dark(1, 0));
        assert!(!p.is_dark(-1, 0));
        assert!(!p.is_dark(2, 0));
        assert!(!p.is_dark(0, 1));
    }
}
