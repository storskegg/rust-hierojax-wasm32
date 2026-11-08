//! `OutlineRaster`: glyph pixels computed from the font's outlines, without a browser.
//!
//! It implements the core's [`Raster`] with exact-area coverage over the outlines of the
//! unmodified font file. Nothing in it comes from the host: the arithmetic is IEEE 754 `+`,
//! `-`, `*`, `/`, `floor` and `ceil`, and `sin`, `cos`, `pow` and `log` come from the pure-Rust
//! `libm` crate, so wasm and native builds agree to the bit ([`determinism_digest`]).
//!
//! The constants in [`Tuning::FROZEN`] make its pixels match upstream's canvas in Chromium on
//! macOS as closely as a fixed model can. They are part of the output's definition: changing
//! one changes the reference files in `tests/golden`, and that is a versioned change. Against
//! upstream on any other OS or browser it is only as close as those are to the Mac's Chromium.

use std::collections::HashMap;
use std::rc::Rc;

use hieroxide_core::jsnum::js_round;
use hieroxide_core::layout::{canvas_dim, Bitmap, DrawCmd, Raster, RasterError};
use ttf_parser::{Face, GlyphId, OutlineBuilder};

/// The fitted constants of the model. All lengths are in device pixels.
///
/// Every value below was fitted against Chromium 158's canvas on an Apple-silicon Mac (macOS
/// 26), with `examples/fit.rs`: 54,483 distinct draws of the corpus replayed on real canvases,
/// and then the identical share of all fragments. `tests/golden/README.md` has the procedure. They
/// describe what Skia and CoreGraphics do there, as far as a fixed model can (the explanations are
/// inferred from how the numbers behave). Each is part of the output's definition.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuning {
    /// Ink added left, right, above and below a draw that is not rotated by other than a quarter
    /// turn, at `full_ppem` pixels per em or more: macOS font smoothing, as a fixed outset. It stays
    /// in device space when the text is mirrored or quarter-turned.
    pub dil: [f64; 4],
    /// Below this many pixels per em, the outset shrinks in proportion. Pixels per em are the font
    /// size times the geometric mean of the two scales.
    pub full_ppem: f64,
    /// A glyph this many pixels wide or high is too large for Skia's glyph masks, and it fills the
    /// outline itself: no outset, as with `rot` (`kSkSideTooBigForAtlas` is 256).
    pub path_side: f64,
    /// Coverage at or below this leaves a pixel empty (upstream's dark test is alpha > 0).
    pub eps: f32,
    /// The same for a draw at another rotation than a quarter turn. There, there is no font smoothing
    /// and hence no outset.
    pub eps_rot: f32,
    /// Origin snap along the baseline, in steps per pixel: Skia's subpixel positions are quarters.
    pub snap_along: f64,
    /// Origin snap across the baseline, in steps per pixel: Skia rounds that axis. A quarter turn
    /// swaps the two axes.
    pub snap_across: f64,
    /// Origin snap on both axes at any other rotation, in steps per pixel.
    pub snap_rot: f64,
    /// How far a cubic's segments may be from the curve. Finer is not closer to the browser: it
    /// flattens too, and these fit its pixels best.
    pub tol: f64,
    /// The same for a draw at another rotation than a quarter turn.
    pub tol_rot: f64,
}

impl Tuning {
    /// The frozen model.
    pub const FROZEN: Tuning = Tuning {
        dil: [0.3, 0.3, 0.6, 0.0],
        full_ppem: 52.0,
        path_side: 256.0,
        eps: 0.003,
        eps_rot: 0.005,
        snap_along: 4.0,
        snap_across: 1.0,
        snap_rot: 4.0,
        tol: 0.25,
        tol_rot: 0.12,
    };

    /// Without the fit: exact outlines, no outset or snap. For measuring ink.
    pub const EXACT: Tuning = Tuning {
        dil: [0.0; 4],
        full_ppem: 1.0,
        path_side: 0.0,
        eps: 0.5,
        eps_rot: 0.5,
        snap_along: 0.0,
        snap_across: 0.0,
        snap_rot: 0.0,
        tol: 0.05,
        tol_rot: 0.05,
    };

    /// Set a constant by name, for the fitting tool (`examples/fit.rs`).
    pub fn set(&mut self, name: &str, v: &[f64]) {
        match name {
            "dil" => self.dil = [v[0], v[1], v[2], v[3]],
            "full" => self.full_ppem = v[0],
            "pathside" => self.path_side = v[0],
            "eps" => self.eps = v[0] as f32,
            "epsr" => self.eps_rot = v[0] as f32,
            "snap" => {
                self.snap_along = v[0];
                self.snap_across = v[1];
            }
            "snapr" => self.snap_rot = v[0],
            "tol" => self.tol = v[0],
            "tolr" => self.tol_rot = v[0],
            _ => panic!("unknown tuning {name}"),
        }
    }
}

impl Default for Tuning {
    fn default() -> Self {
        Tuning::FROZEN
    }
}

/// A font that cannot be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontError(pub String);

impl core::fmt::Display for FontError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FontError {}

/// One path command, in font units with y upward.
#[derive(Clone, Copy, Debug)]
enum Seg {
    Move(f32, f32),
    Line(f32, f32),
    Quad(f32, f32, f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

#[derive(Debug, Default)]
struct Path(Vec<Seg>);

impl OutlineBuilder for Path {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push(Seg::Move(x, y));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.push(Seg::Line(x, y));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.push(Seg::Quad(x1, y1, x, y));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.push(Seg::Cubic(x1, y1, x2, y2, x, y));
    }
    fn close(&mut self) {
        self.0.push(Seg::Close);
    }
}

/// A glyph's advance (font units) and outline.
#[derive(Debug)]
struct Glyph {
    advance: f64,
    path: Vec<Seg>,
}

/// The largest canvas it will make, in pixels: past this a browser fails too.
const MAX_PIXELS: usize = 1 << 28;

/// A [`Raster`] over the outlines of one font.
pub struct OutlineRaster<'f> {
    face: Face<'f>,
    upem: f64,
    tuning: Tuning,
    glyphs: HashMap<char, Rc<Glyph>>,
    /// Scratch: the accumulation buffer of the area coverage.
    acc: Vec<f32>,
    /// Whether a canvas with a side of 0 is an empty bitmap instead of the browser's exception.
    lenient: bool,
}

impl<'f> OutlineRaster<'f> {
    /// From the bytes of an unmodified OpenType font.
    pub fn new(font: &'f [u8]) -> Result<OutlineRaster<'f>, FontError> {
        OutlineRaster::with_tuning(font, Tuning::FROZEN)
    }

    /// With other constants, for fitting them. The output is only reproducible with the frozen ones.
    pub fn with_tuning(font: &'f [u8], tuning: Tuning) -> Result<OutlineRaster<'f>, FontError> {
        let face = Face::parse(font, 0).map_err(|e| FontError(format!("not a usable font: {e}")))?;
        let upem = f64::from(face.units_per_em());
        Ok(OutlineRaster { face, upem, tuning, glyphs: HashMap::new(), acc: Vec::new(), lenient: false })
    }

    /// A canvas with a side of 0 reads as empty pixels, where a browser's `getImageData` throws
    /// `IndexSizeError`. Consistent mode uses this.
    pub fn lenient(mut self) -> Self {
        self.lenient = true;
        self
    }

    fn glyph(&mut self, ch: char) -> Option<Rc<Glyph>> {
        if let Some(g) = self.glyphs.get(&ch) {
            return Some(g.clone());
        }
        let id: GlyphId = self.face.glyph_index(ch)?;
        let advance = f64::from(self.face.glyph_hor_advance(id).unwrap_or(0));
        let mut path = Path::default();
        let _ = self.face.outline_glyph(id, &mut path);
        let g = Rc::new(Glyph { advance, path: path.0 });
        self.glyphs.insert(ch, g.clone());
        Some(g)
    }

    /// `fillText` on the canvas `alpha` (one byte a pixel; nonzero is dark).
    fn fill(&mut self, cmd: &DrawCmd, w: usize, h: usize, alpha: &mut [u8]) {
        let bad = |v: f64| !v.is_finite();
        if bad(cmd.font_size)
            || cmd.font_size <= 0.0
            || bad(cmd.tx)
            || bad(cmd.ty)
            || bad(cmd.sx)
            || bad(cmd.sy)
            || bad(cmd.rotate)
            || bad(cmd.x)
            || bad(cmd.y)
        {
            return;
        }
        let Some(g) = self.glyph(cmd.ch) else { return };
        if g.path.is_empty() {
            return;
        }
        let t = self.tuning;
        let (sin, cos) = (libm::sin(cmd.rotate), libm::cos(cmd.rotate));
        let (a, b, c, d) = (cmd.sx * cos, cmd.sy * sin, -cmd.sx * sin, cmd.sy * cos);
        let (e, f) = (cmd.tx, cmd.ty);
        let k = cmd.font_size / self.upem;
        const NEAR_ZERO: f64 = 1e-9;
        let axis_aligned = b.abs() < NEAR_ZERO && c.abs() < NEAR_ZERO;
        let quarter = a.abs() < NEAR_ZERO && d.abs() < NEAR_ZERO;
        // In single precision, as Skia computes it: the noise of sin and cos at a quarter turn is
        // gone there, and a tie stays a tie.
        let ox = f64::from((a * cmd.x + c * cmd.y + e) as f32);
        let oy = f64::from((b * cmd.x + d * cmd.y + f) as f32);
        let snap = |v: f64, steps: f64| if steps > 0.0 { js_round(v * steps) / steps - v } else { 0.0 };
        let (sdx, sdy) = if axis_aligned {
            (snap(ox, t.snap_along), snap(oy, t.snap_across))
        } else if quarter {
            (snap(ox, t.snap_across), snap(oy, t.snap_along))
        } else {
            (snap(ox, t.snap_rot), snap(oy, t.snap_rot))
        };
        let tr = |ux: f64, uy: f64| -> (f64, f64) {
            let xx = cmd.x + ux * k;
            let yy = cmd.y - uy * k;
            (a * xx + c * yy + e + sdx, b * xx + d * yy + f + sdy)
        };

        // The glyph's box, from its control points, to bound the work to that window.
        let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        {
            let mut add = |ux: f32, uy: f32| {
                let (px, py) = tr(f64::from(ux), f64::from(uy));
                if px < x0 {
                    x0 = px;
                }
                if px > x1 {
                    x1 = px;
                }
                if py < y0 {
                    y0 = py;
                }
                if py > y1 {
                    y1 = py;
                }
            };
            for s in &g.path {
                match *s {
                    Seg::Move(x, y) | Seg::Line(x, y) => add(x, y),
                    Seg::Quad(p, q, x, y) => {
                        add(p, q);
                        add(x, y);
                    }
                    Seg::Cubic(p, q, r, s, x, y) => {
                        add(p, q);
                        add(r, s);
                        add(x, y);
                    }
                    Seg::Close => {}
                }
            }
        }
        if !(x0 <= x1 && y0 <= y1) {
            return;
        }

        let hard = axis_aligned || quarter;
        let too_big = t.path_side > 0.0 && (x1 - x0 >= t.path_side || y1 - y0 >= t.path_side);
        let ppem = cmd.font_size * libm::sqrt((cmd.sx * cmd.sy).abs());
        let f = if ppem < t.full_ppem { ppem / t.full_ppem } else { 1.0 };
        let dil = if hard && !too_big { [t.dil[0] * f, t.dil[1] * f, t.dil[2] * f, t.dil[3] * f] } else { [0.0; 4] };
        let shifts: &[(f64, f64)] = if dil == [0.0; 4] {
            &[(0.0, 0.0)]
        } else {
            &[(-dil[0], -dil[2]), (dil[1], -dil[2]), (-dil[0], dil[3]), (dil[1], dil[3])]
        };
        let margin = dil.iter().fold(0.0f64, |m, v| if *v > m { *v } else { m }) + 2.0;
        let wx0 = float_clamp((x0 - margin).floor(), w);
        let wx1 = float_clamp((x1 + margin).ceil(), w);
        let wy0 = float_clamp((y0 - margin).floor(), h);
        let wy1 = float_clamp((y1 + margin).ceil(), h);
        if wx1 <= wx0 || wy1 <= wy0 {
            return;
        }
        let eps = if hard { t.eps } else { t.eps_rot };
        let tol = if hard { t.tol } else { t.tol_rot };
        let (ww, wh) = (wx1 - wx0, wy1 - wy0);
        let stride = ww + 2;
        let need = stride * wh;
        if self.acc.len() < need {
            self.acc.resize(need, 0.0);
        }
        let (bx, by) = (wx0 as f64, wy0 as f64);
        let acc = &mut self.acc[..need];
        for &(sx, sy) in shifts {
            acc.fill(0.0);
            let (ofx, ofy) = (sx - bx, sy - by);
            let (mut cx, mut cy) = (0.0f64, 0.0f64);
            let (mut mx, mut my) = (0.0f64, 0.0f64);
            let mut open = false;
            let mut acc_line = |p: (f64, f64), q: (f64, f64)| {
                line(acc, ww, wh, p.0 + ofx, p.1 + ofy, q.0 + ofx, q.1 + ofy);
            };
            for s in &g.path {
                match *s {
                    Seg::Move(x, y) => {
                        if open && (cx != mx || cy != my) {
                            acc_line(tr(cx, cy), tr(mx, my));
                        }
                        cx = f64::from(x);
                        cy = f64::from(y);
                        mx = cx;
                        my = cy;
                        open = true;
                    }
                    Seg::Line(x, y) => {
                        let (x, y) = (f64::from(x), f64::from(y));
                        acc_line(tr(cx, cy), tr(x, y));
                        cx = x;
                        cy = y;
                    }
                    Seg::Quad(px, py, x, y) => {
                        // Degree-raised: this font has only cubics, other fonts may not.
                        let (px, py, x, y) = (f64::from(px), f64::from(py), f64::from(x), f64::from(y));
                        let c1 = (cx + 2.0 * (px - cx) / 3.0, cy + 2.0 * (py - cy) / 3.0);
                        let c2 = (x + 2.0 * (px - x) / 3.0, y + 2.0 * (py - y) / 3.0);
                        cubic(&mut acc_line, tr(cx, cy), tr(c1.0, c1.1), tr(c2.0, c2.1), tr(x, y), tol);
                        cx = x;
                        cy = y;
                    }
                    Seg::Cubic(p, q, r, u, x, y) => {
                        let (x, y) = (f64::from(x), f64::from(y));
                        cubic(
                            &mut acc_line,
                            tr(cx, cy),
                            tr(f64::from(p), f64::from(q)),
                            tr(f64::from(r), f64::from(u)),
                            tr(x, y),
                            tol,
                        );
                        cx = x;
                        cy = y;
                    }
                    Seg::Close => {
                        if cx != mx || cy != my {
                            acc_line(tr(cx, cy), tr(mx, my));
                        }
                        cx = mx;
                        cy = my;
                    }
                }
            }
            if open && (cx != mx || cy != my) {
                acc_line(tr(cx, cy), tr(mx, my));
            }
            // Integrate along each row. Non-zero winding of a well-formed glyph: |sum|, capped.
            for yy in 0..wh {
                let row = &acc[yy * stride..yy * stride + ww];
                let out = &mut alpha[(yy + wy0) * w + wx0..(yy + wy0) * w + wx0 + ww];
                let mut sum = 0.0f32;
                for (v, o) in row.iter().zip(out.iter_mut()) {
                    sum += *v;
                    if sum.abs() > eps {
                        *o = 255;
                    }
                }
            }
        }
    }
}

/// `v` as an index into `0..=max`: below 0 is 0, above `max` is `max`.
fn float_clamp(v: f64, max: usize) -> usize {
    if v <= 0.0 {
        0
    } else if v >= max as f64 {
        max
    } else {
        v as usize
    }
}

/// Flatten a cubic by uniform steps of its parameter, as many as keep it within `tol` of the curve
/// (the usual bound from its second differences), at most 64.
fn cubic(line: &mut impl FnMut((f64, f64), (f64, f64)), p0: (f64, f64), p1: (f64, f64), p2: (f64, f64), p3: (f64, f64), tol: f64) {
    let larger = |a: f64, b: f64| if a > b { a } else { b };
    let dd = larger((p0.0 - 2.0 * p1.0 + p2.0).abs(), (p1.0 - 2.0 * p2.0 + p3.0).abs())
        + larger((p0.1 - 2.0 * p1.1 + p2.1).abs(), (p1.1 - 2.0 * p2.1 + p3.1).abs());
    let n = libm::sqrt(0.75 * dd / tol).ceil();
    let n = if n < 1.0 {
        1
    } else if n > 64.0 {
        64
    } else {
        n as u32
    };
    let mut prev = p0;
    for i in 1..=n {
        let t = f64::from(i) / f64::from(n);
        let u = 1.0 - t;
        let (uu, tt) = (u * u, t * t);
        let (b0, b1, b2, b3) = (uu * u, 3.0 * uu * t, 3.0 * u * tt, tt * t);
        let next = (b0 * p0.0 + b1 * p1.0 + b2 * p2.0 + b3 * p3.0, b0 * p0.1 + b1 * p1.1 + b2 * p2.1 + b3 * p3.1);
        line(prev, next);
        prev = next;
    }
}

/// Accumulate the signed area of one line into a `w`-wide (plus two), `h`-high buffer: the
/// method of font-rs. `x` beyond the window is clamped to its edge, so a part of the glyph
/// left of the window still counts for the row.
fn line(a: &mut [f32], w: usize, h: usize, mut x0: f64, mut y0: f64, mut x1: f64, mut y1: f64) {
    if y0 == y1 {
        return;
    }
    let dir = if y0 > y1 {
        core::mem::swap(&mut x0, &mut x1);
        core::mem::swap(&mut y0, &mut y1);
        -1.0
    } else {
        1.0
    };
    let hf = h as f64;
    if y1 <= 0.0 || y0 >= hf {
        return;
    }
    let wf = w as f64;
    let dxdy = (x1 - x0) / (y1 - y0);
    let mut x = if y0 < 0.0 { x0 - y0 * dxdy } else { x0 };
    let y_start = if y0 <= 0.0 { 0 } else { y0.floor() as usize };
    let y_end = if y1 >= hf { h } else { y1.ceil() as usize };
    let stride = w + 2;
    for y in y_start..y_end {
        let ls = y * stride;
        let yf = y as f64;
        let top = if yf > y0 { yf } else { y0 };
        let bot = if yf + 1.0 < y1 { yf + 1.0 } else { y1 };
        let dy = bot - top;
        let xn = x + dxdy * dy;
        let d = dy * dir;
        let (xa, xb) = if x < xn { (x, xn) } else { (xn, x) };
        let xa = if xa < 0.0 { 0.0 } else if xa > wf { wf } else { xa };
        let xb = if xb < 0.0 { 0.0 } else if xb > wf { wf } else { xb };
        let x0f = xa.floor();
        let x0i = x0f as usize;
        let x1c = xb.ceil();
        let x1i = x1c as usize;
        if x1i <= x0i + 1 {
            let xmf = 0.5 * (xa + xb) - x0f;
            a[ls + x0i] += (d - d * xmf) as f32;
            a[ls + x0i + 1] += (d * xmf) as f32;
        } else {
            let s = 1.0 / (xb - xa);
            let x0fr = xa - x0f;
            let a0 = 0.5 * s * (1.0 - x0fr) * (1.0 - x0fr);
            let x1fr = xb - x1c + 1.0;
            let am = 0.5 * s * x1fr * x1fr;
            a[ls + x0i] += (d * a0) as f32;
            if x1i == x0i + 2 {
                a[ls + x0i + 1] += (d * (1.0 - a0 - am)) as f32;
            } else {
                let a1 = s * (1.5 - x0fr);
                a[ls + x0i + 1] += (d * (a1 - a0)) as f32;
                for xi in x0i + 2..x1i - 1 {
                    a[ls + xi] += (d * s) as f32;
                }
                let a2 = a1 + (x1i - x0i - 3) as f64 * s;
                a[ls + x1i - 1] += (d * (1.0 - a2 - am)) as f32;
            }
            a[ls + x1i] += (d * am) as f32;
        }
        x = xn;
    }
}

impl Raster for OutlineRaster<'_> {
    fn advance(&mut self, ch: char, font_size: f64) -> Result<f64, RasterError> {
        Ok(match self.glyph(ch) {
            Some(g) => g.advance * font_size / self.upem,
            None => 0.0,
        })
    }

    fn draw(&mut self, w: f64, h: f64, cmds: &[DrawCmd]) -> Result<Bitmap, RasterError> {
        let (w, h) = (canvas_dim(w, 300), canvas_dim(h, 150));
        if w == 0 || h == 0 {
            if self.lenient {
                return Ok(Bitmap { w, h, data: Vec::new() });
            }
            return Err(RasterError("IndexSizeError".to_string()));
        }
        let (wu, hu) = (w as usize, h as usize);
        let pixels = wu.checked_mul(hu).filter(|p| *p <= MAX_PIXELS).ok_or_else(|| RasterError("canvas too large".to_string()))?;
        let mut alpha = vec![0u8; pixels];
        for c in cmds {
            self.fill(c, wu, hu, &mut alpha);
        }
        let mut data = vec![0u8; pixels * 4];
        for (px, a) in data.chunks_exact_mut(4).zip(alpha.iter()) {
            px[3] = *a;
        }
        Ok(Bitmap { w, h, data })
    }

    fn pow(&mut self, base: f64, exp: f64) -> f64 {
        libm::pow(base, exp)
    }

    fn log(&mut self, x: f64) -> f64 {
        libm::log(x)
    }
}

/// A digest of `pow`, `log`, `sin` and `cos` over a fixed sweep of arguments, for comparing a
/// wasm build with a native one: the same value on both means the same bits. FNV-1a over the
/// IEEE bits of the results.
pub fn determinism_digest() -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    let mut mix = |v: f64| {
        for b in v.to_bits().to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x100000001b3);
        }
    };
    // A small multiplicative congruential sequence in (0, 1): no floats from anywhere else.
    let mut s: u64 = 1;
    for _ in 0..20_000 {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let u = ((s >> 11) as f64 + 0.5) / 9007199254740992.0;
        let v = 0.15 + u * 4.0;
        mix(libm::log(v));
        mix(libm::pow(1.0 / v, u));
        mix(libm::sin(u * 7.0));
        mix(libm::cos(u * 7.0));
    }
    for r in [0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0] {
        let a = r * core::f64::consts::PI / 180.0;
        mix(libm::sin(a));
        mix(libm::cos(a));
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> Vec<u8> {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/NewGardiner.otf");
        std::fs::read(p).expect("docs/NewGardiner.otf")
    }

    fn sitting_man(raster: &mut OutlineRaster<'_>) -> Bitmap {
        let cmd = DrawCmd { ch: '\u{13000}', font_size: 150.0, tx: 78.0, ty: 78.0, sx: 1.0, sy: 1.0, rotate: 0.0, x: -75.0, y: 75.0 };
        raster.draw(156.0, 156.0, &[cmd]).expect("draw")
    }

    #[test]
    fn draws_the_way_a_canvas_does() {
        let font = font();
        let mut r = OutlineRaster::new(&font).expect("font");
        let b = sitting_man(&mut r);
        assert_eq!((b.w, b.h), (156, 156));
        // Chromium on the Mac: measureGlyph(U+13000, 150, 1, 1, 0, false) gives w 106, h 151.
        let m = hieroxide_core::layout::Plane::new(&b).margins();
        assert_eq!((156 - m.l - m.r, 156 - m.t - m.b), (106, 151));
    }

    #[test]
    fn advance_comes_from_the_font() {
        let font = font();
        let mut r = OutlineRaster::new(&font).expect("font");
        assert!(r.advance('\u{13000}', 150.0).expect("advance") > 0.0);
        // Not in the font: nothing to draw, no advance, no error.
        assert_eq!(r.advance('a', 150.0).expect("advance"), 0.0);
    }

    #[test]
    fn a_canvas_with_no_pixels() {
        let font = font();
        let mut r = OutlineRaster::new(&font).expect("font");
        assert!(r.draw(0.0, 10.0, &[]).is_err());
        assert!(r.draw(f64::NAN, 10.0, &[]).is_err());
        let mut r = OutlineRaster::new(&font).expect("font").lenient();
        assert_eq!(r.draw(0.0, 10.0, &[]).expect("draw").data.len(), 0);
    }

    #[test]
    fn a_font_that_is_not_one_is_an_error() {
        assert!(OutlineRaster::new(b"not a font").is_err());
    }

    #[test]
    fn digest_is_stable_within_a_run() {
        assert_eq!(determinism_digest(), determinism_digest());
    }
}
