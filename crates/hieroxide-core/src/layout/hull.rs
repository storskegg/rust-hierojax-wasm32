//! What `Shapes` computes from a group's pixels: scans for dark pixels, the orthogonal hull
//! with its buffer, the open rectangle around a point, distances between shapes, the
//! displacement of an insertion, and the small tables around insertion places.
//!
//! All numbers are `f64` and go through the same operations as upstream's. Coordinates are
//! JS numbers: they can be NaN, and then a loop over them does not run and a lookup with them
//! finds nothing, which the code below gets by keeping them as `f64` too.

use super::jsmath::{get_max, get_min, js_max, js_min};
use super::raster::{Bitmap, Plane};
use super::{LayoutError, Unsupported};
use crate::jsnum::js_round;
use crate::tables::{self, Adj};

/// `{ x, y }`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// Which `isDark` a plane has (`Shapes.plane`, `planeExtended`, `planeRestricted`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneKind {
    /// The index is computed without looking at the sides, so past the end of a row is the
    /// start of the next.
    Plain,
    /// Outside the canvas is not dark.
    Extended,
    /// Outside the canvas is dark.
    Restricted,
}

/// `{ w, h, isDark }`.
#[derive(Clone, Copy, Debug)]
pub struct ScanPlane<'a> {
    pub w: f64,
    pub h: f64,
    plane: Plane<'a>,
    kind: PlaneKind,
}

impl<'a> ScanPlane<'a> {
    pub fn new(bitmap: &'a Bitmap, kind: PlaneKind) -> ScanPlane<'a> {
        ScanPlane { w: f64::from(bitmap.w), h: f64::from(bitmap.h), plane: Plane::new(bitmap), kind }
    }

    fn is_dark(&self, x: f64, y: f64) -> bool {
        match self.kind {
            PlaneKind::Plain => self.plane.is_dark_at(x, y),
            PlaneKind::Extended => {
                0.0 <= x && x < self.w && 0.0 <= y && y < self.h && self.plane.is_dark_at(x, y)
            }
            PlaneKind::Restricted => {
                x < 0.0 || self.w <= x || y < 0.0 || self.h <= y || self.plane.is_dark_at(x, y)
            }
        }
    }

    /// `Shapes.topMostDark`
    fn top_most_dark(&self, x: f64, y_min: f64, y_max: f64) -> f64 {
        let mut y = y_min;
        while y <= y_max {
            if self.is_dark(x, y) {
                return y;
            }
            y += 1.0;
        }
        f64::NAN
    }

    /// `Shapes.bottomMostDark`
    fn bottom_most_dark(&self, x: f64, y_min: f64, y_max: f64) -> f64 {
        let mut y = y_max;
        while y >= y_min {
            if self.is_dark(x, y) {
                return y;
            }
            y -= 1.0;
        }
        f64::NAN
    }

    /// `Shapes.leftMostDark`
    fn left_most_dark(&self, x_min: f64, x_max: f64, y: f64) -> f64 {
        let mut x = x_min;
        while x <= x_max {
            if self.is_dark(x, y) {
                return x;
            }
            x += 1.0;
        }
        f64::NAN
    }

    /// `Shapes.rightMostDark`
    fn right_most_dark(&self, x_min: f64, x_max: f64, y: f64) -> f64 {
        let mut x = x_max;
        while x >= x_min {
            if self.is_dark(x, y) {
                return x;
            }
            x -= 1.0;
        }
        f64::NAN
    }
}

/// One of the four arrays of the hull. A JS array that is indexed from `-dist`: the negative
/// indices are plain properties, which `reduce` does not visit, and an index nothing was
/// assigned to is a hole, `undefined` when read (which is NaN to everything that reads it
/// here), and skipped by `reduce`.
#[derive(Clone, Debug, Default)]
pub struct JsArray {
    /// The index of `v[0]`.
    first: i64,
    v: Vec<Option<f64>>,
}

impl JsArray {
    fn get(&self, i: i64) -> f64 {
        let Some(p) = i.checked_sub(self.first).and_then(|d| usize::try_from(d).ok()) else {
            return f64::NAN;
        };
        self.v.get(p).copied().flatten().unwrap_or(f64::NAN)
    }

    /// Reading with a JS number as the index.
    pub fn at(&self, i: f64) -> f64 {
        if i.is_finite() && i.abs() < 9e15 && i.fract() == 0.0 {
            self.get(i as i64)
        } else {
            f64::NAN
        }
    }

    fn set(&mut self, i: i64, x: f64) {
        if self.v.is_empty() {
            self.first = i;
        }
        if i < self.first {
            let grow = (self.first - i) as usize;
            self.v.splice(0..0, core::iter::repeat_n(None, grow));
            self.first = i;
        }
        let p = (i - self.first) as usize;
        if p >= self.v.len() {
            self.v.resize(p + 1, None);
        }
        if let Some(e) = self.v.get_mut(p) {
            *e = Some(x);
        }
    }

    /// What `reduce` visits: the assigned indices from 0 up.
    fn indexed(&self) -> impl Iterator<Item = f64> + '_ {
        let skip = usize::try_from(-self.first).unwrap_or(0);
        self.v.iter().skip(skip).filter_map(|e| *e)
    }
}

/// The result of `Shapes.orthogonalHullWithBuffer`.
#[derive(Clone, Debug)]
pub struct Hull {
    pub w: f64,
    pub h: f64,
    pub x_mins: JsArray,
    pub x_maxs: JsArray,
    pub y_mins: JsArray,
    pub y_maxs: JsArray,
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub dist: f64,
}

/// More than this many em of buffer is not attempted (see [`Unsupported::SpanTooLarge`]).
const MAX_SPAN: f64 = (1u32 << 24) as f64;

struct Buffered {
    x_mins: JsArray,
    x_maxs: JsArray,
    y_mins: JsArray,
    y_maxs: JsArray,
    dist: f64,
    slant: f64,
}

impl Buffered {
    /// `addBufferedVertical`
    fn vertical(&mut self, x: f64, y_min: f64, y_max: f64) {
        let (dist, slant) = (self.dist, self.slant);
        let s = slant as i64;
        if y_min.is_nan() || y_max.is_nan() {
            return;
        }
        let (lo, hi) = (y_min as i64, y_max as i64);
        for y in lo..=hi {
            let m = &mut self.x_mins;
            m.set(y, js_min(m.get(y), x - dist));
            m.set(y - s, js_min(m.get(y - s), x - slant));
            m.set(y + s, js_min(m.get(y + s), x - slant));
            let m = &mut self.x_maxs;
            m.set(y, js_max(m.get(y), x + dist));
            m.set(y - s, js_max(m.get(y - s), x + slant));
            m.set(y + s, js_max(m.get(y + s), x + slant));
        }
    }

    /// `addBufferedHorizontal`
    fn horizontal(&mut self, x_min: f64, x_max: f64, y: f64) {
        let (dist, slant) = (self.dist, self.slant);
        let s = slant as i64;
        if x_min.is_nan() || x_max.is_nan() {
            return;
        }
        let (lo, hi) = (x_min as i64, x_max as i64);
        for x in lo..=hi {
            let m = &mut self.y_mins;
            m.set(x, js_min(m.get(x), y - dist));
            m.set(x - s, js_min(m.get(x - s), y - slant));
            m.set(x + s, js_min(m.get(x + s), y - slant));
            let m = &mut self.y_maxs;
            m.set(x, js_max(m.get(x), y + dist));
            m.set(x - s, js_max(m.get(x - s), y + slant));
            m.set(x + s, js_max(m.get(x + s), y + slant));
        }
    }
}

/// `Shapes.orthogonalHullWithBuffer`, on the canvas's pixels (`Shapes.plane(canvas)`).
pub fn orthogonal_hull_with_buffer(bitmap: &Bitmap, dist: f64) -> Result<Hull, LayoutError> {
    if !(dist.abs() <= MAX_SPAN) {
        return Err(LayoutError::Unsupported(Unsupported::SpanTooLarge));
    }
    let plane = ScanPlane::new(bitmap, PlaneKind::Plain);
    let (w, h) = (plane.w, plane.h);
    let m = Plane::new(bitmap).margins();
    let (mt, mb, ml, mr) = (m.t as f64, m.b as f64, m.l as f64, m.r as f64);
    let slant = js_round(dist / 2f64.sqrt());
    let mut b = Buffered {
        x_mins: JsArray::default(),
        x_maxs: JsArray::default(),
        y_mins: JsArray::default(),
        y_maxs: JsArray::default(),
        dist,
        slant,
    };
    let mut x = -dist;
    while x < w + dist {
        b.y_mins.set(x as i64, h - 1.0);
        b.y_maxs.set(x as i64, 0.0);
        x += 1.0;
    }
    let mut y = -dist;
    while y < h + dist {
        b.x_mins.set(y as i64, w - 1.0);
        b.x_maxs.set(y as i64, 0.0);
        y += 1.0;
    }
    if mt == h {
        b.horizontal(0.0, w - 1.0, 0.0);
        b.horizontal(0.0, w - 1.0, h - 1.0);
        b.vertical(0.0, 0.0, h - 1.0);
        b.vertical(w - 1.0, 0.0, h - 1.0);
        let x_min = -dist;
        let x_max = w + dist - 1.0;
        let y_min = -dist;
        let y_max = h + dist - 1.0;
        return Ok(Hull {
            w,
            h,
            x_mins: b.x_mins,
            x_maxs: b.x_maxs,
            y_mins: b.y_mins,
            y_maxs: b.y_maxs,
            x_min,
            x_max,
            y_min,
            y_max,
            dist,
        });
    }
    let mut x = ml;
    let mut y = plane.top_most_dark(x, mt, h - 1.0 - mb);
    let mut x_old = x;
    while y > mt {
        x += 1.0;
        let y_new = plane.top_most_dark(x, mt, y - 1.0);
        if y_new >= 0.0 {
            b.horizontal(x_old, x, y);
            b.vertical(x, y_new, y);
            y = y_new;
            x_old = x;
        }
    }
    x = plane.right_most_dark(x, w - 1.0 - mr, y);
    b.horizontal(x_old, x, y);
    let mut y_old = y;
    while x < w - 1.0 - mr {
        y += 1.0;
        let x_new = plane.right_most_dark(x + 1.0, w - 1.0 - mr, y);
        if x_new >= 0.0 {
            b.vertical(x, y_old, y);
            b.horizontal(x, x_new, y);
            x = x_new;
            y_old = y;
        }
    }
    y = plane.bottom_most_dark(x, y, h - 1.0 - mb);
    b.vertical(x, y_old, y);
    x_old = x;
    while y < h - 1.0 - mb {
        x -= 1.0;
        let y_new = plane.bottom_most_dark(x, y + 1.0, h - 1.0 - mb);
        if y_new >= 0.0 {
            b.horizontal(x, x_old, y);
            b.vertical(x, y, y_new);
            y = y_new;
            x_old = x;
        }
    }
    x = plane.left_most_dark(ml, x, y);
    b.horizontal(x, x_old, y);
    y_old = y;
    while x > ml {
        y -= 1.0;
        let x_new = plane.left_most_dark(ml, x - 1.0, y);
        if x_new >= 0.0 {
            b.vertical(x, y, y_old);
            b.horizontal(x_new, x, y);
            x = x_new;
            y_old = y;
        }
    }
    y = plane.top_most_dark(x, mt, y);
    b.vertical(x, y, y_old);
    // Upstream quirk: `reduce` passes over the negative indices, so a buffer that reaches
    // above or left of the canvas does not count here.
    let x_min = get_min(b.x_mins.indexed());
    let x_max = get_max(b.x_maxs.indexed());
    let y_min = get_min(b.y_mins.indexed());
    let y_max = get_max(b.y_maxs.indexed());
    Ok(Hull {
        w,
        h,
        x_mins: b.x_mins,
        x_maxs: b.x_maxs,
        y_mins: b.y_mins,
        y_maxs: b.y_maxs,
        x_min,
        x_max,
        y_min,
        y_max,
        dist,
    })
}

/// `Shapes.hDistance`, after both planes have been read.
pub fn h_distance(p1: &Bitmap, p2: &Bitmap, y_min: f64, y_max: f64) -> f64 {
    let y_min = js_round(y_min);
    let y_max = js_round(y_max);
    let plane1 = ScanPlane::new(p1, PlaneKind::Plain);
    let plane2 = ScanPlane::new(p2, PlaneKind::Plain);
    let mut dist_min = plane1.w + plane2.w;
    let mut y = y_min;
    while y < y_max {
        let r_most = plane1.right_most_dark(0.0, plane1.w - 1.0, y);
        let l_most = plane2.left_most_dark(0.0, plane2.w - 1.0, y);
        if !r_most.is_nan() && !l_most.is_nan() {
            dist_min = js_min(dist_min, plane1.w - 1.0 - r_most + l_most);
        }
        y += 1.0;
    }
    if dist_min < plane1.w + plane2.w { dist_min } else { 0.0 }
}

/// `Shapes.vDistance`, after both planes have been read.
pub fn v_distance(p1: &Bitmap, p2: &Bitmap, x_min: f64, x_max: f64) -> f64 {
    let x_min = js_round(x_min);
    let x_max = js_round(x_max);
    let plane1 = ScanPlane::new(p1, PlaneKind::Plain);
    let plane2 = ScanPlane::new(p2, PlaneKind::Plain);
    let mut dist_min = plane1.h + plane2.h;
    let mut x = x_min;
    while x < x_max {
        let b_most = plane1.bottom_most_dark(x, 0.0, plane1.h - 1.0);
        let t_most = plane2.top_most_dark(x, 0.0, plane2.h - 1.0);
        if !b_most.is_nan() && !t_most.is_nan() {
            dist_min = js_min(dist_min, plane1.h - 1.0 - b_most + t_most);
        }
        x += 1.0;
    }
    if dist_min < plane1.h + plane2.h { dist_min } else { 0.0 }
}

/// `{ x, y, w, h }` of `openRect` and the insertion code.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PxRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// `Shapes.openRect`, on `Shapes.planeExtended(canvas)`.
pub fn open_rect(bitmap: &Bitmap, x: f64, y: f64) -> PxRect {
    let plane = ScanPlane::new(bitmap, PlaneKind::Extended);
    let (cw, ch) = (plane.w, plane.h);
    let mut rect = PxRect { x, y, w: 1.0, h: 1.0 };
    let mut extended = true;
    // Upstream quirk: `rect.w < canvas.width & rect.h < canvas.height` has `&` for `&&`.
    // Neither operand has a side effect, so the result is the same.
    while extended && ((rect.w < cw) & (rect.h < ch)) {
        extended = false;
        if rect.x > 0.0 && plane.top_most_dark(rect.x - 1.0, rect.y, rect.y + rect.h - 1.0).is_nan() {
            rect.x -= 1.0;
            rect.w += 1.0;
            extended = true;
        }
        if rect.x + rect.w < cw && plane.top_most_dark(rect.x + rect.w, rect.y, rect.y + rect.h - 1.0).is_nan() {
            rect.w += 1.0;
            extended = true;
        }
        if rect.y > 0.0 && plane.left_most_dark(rect.x, rect.x + rect.w - 1.0, rect.y - 1.0).is_nan() {
            rect.y -= 1.0;
            rect.h += 1.0;
            extended = true;
        }
        if rect.y + rect.h < ch && plane.left_most_dark(rect.x, rect.x + rect.w - 1.0, rect.y + rect.h).is_nan() {
            rect.h += 1.0;
            extended = true;
        }
    }
    rect
}

/// `{ l, r, t, b }` of `Shapes.distances`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Dists {
    l: f64,
    r: f64,
    t: f64,
    b: f64,
}

/// `incr > 0 ? x < last : x > last`
fn in_range(x: f64, last: f64, incr: f64) -> bool {
    if incr > 0.0 { x < last } else { x > last }
}

// Upstream quirk, in the four functions below: each computes `almostLast`, which only the
// commented-out `|| (incr > 0 ? x < almostLast : x > almostLast)` would read. Rounding it has
// no side effect, so it is left out here, and the condition stays as upstream leaves it.

/// `Shapes.distancesBottommost`
#[allow(clippy::too_many_arguments)]
fn distances_bottommost(
    plane: &ScanPlane<'_>,
    hull: &Hull,
    p_plane: Point,
    p_hull: Point,
    scale_prev: f64,
    scale: f64,
    mut t: f64,
    first: f64,
    last: f64,
    incr: f64,
) -> f64 {
    let mut x = first;
    while in_range(x, last, incr) {
        let x_plane = js_round(p_plane.x + (x - p_hull.x) * scale);
        let y_min = hull.y_mins.at(x);
        if y_min <= p_hull.y {
            let dist_prev = (p_hull.y - y_min) * scale_prev;
            let dist = (p_hull.y - y_min) * scale;
            let y_plane_min = js_round(p_plane.y - dist);
            let y_plane_max = js_round(p_plane.y - dist_prev);
            let bottom_most = plane.bottom_most_dark(x_plane, y_plane_min, js_round(p_plane.y));
            if !bottom_most.is_nan() {
                if bottom_most < y_plane_max + 1.0 {
                    t = js_max(t, bottom_most - y_plane_min + 1.0);
                } else {
                    break;
                }
            }
        }
        x += incr;
    }
    t
}

/// `Shapes.distancesTopmost`
#[allow(clippy::too_many_arguments)]
fn distances_topmost(
    plane: &ScanPlane<'_>,
    hull: &Hull,
    p_plane: Point,
    p_hull: Point,
    scale_prev: f64,
    scale: f64,
    mut b: f64,
    first: f64,
    last: f64,
    incr: f64,
) -> f64 {
    let mut x = first;
    while in_range(x, last, incr) {
        let x_plane = js_round(p_plane.x + (x - p_hull.x) * scale);
        let y_max = hull.y_maxs.at(x);
        if y_max >= p_hull.y {
            let dist_prev = (y_max - p_hull.y) * scale_prev;
            let dist = (y_max - p_hull.y) * scale;
            let y_plane_min = js_round(p_plane.y + dist_prev);
            let y_plane_max = js_round(p_plane.y + dist);
            let top_most = plane.top_most_dark(x_plane, js_round(p_plane.y), y_plane_max);
            if !top_most.is_nan() {
                if top_most > y_plane_min - 1.0 {
                    b = js_min(b, top_most - y_plane_max - 1.0);
                } else {
                    break;
                }
            }
        }
        x += incr;
    }
    b
}

/// `Shapes.distancesRightmost`
#[allow(clippy::too_many_arguments)]
fn distances_rightmost(
    plane: &ScanPlane<'_>,
    hull: &Hull,
    p_plane: Point,
    p_hull: Point,
    scale_prev: f64,
    scale: f64,
    mut l: f64,
    first: f64,
    last: f64,
    incr: f64,
) -> f64 {
    let mut y = first;
    while in_range(y, last, incr) {
        let y_plane = js_round(p_plane.y + (y - p_hull.y) * scale);
        let x_min = hull.x_mins.at(y);
        if x_min <= p_hull.x {
            let dist_prev = (p_hull.x - x_min) * scale_prev;
            let dist = (p_hull.x - x_min) * scale;
            let x_plane_min = js_round(p_plane.x - dist);
            let x_plane_max = js_round(p_plane.x - dist_prev);
            let right_most = plane.right_most_dark(x_plane_min, js_round(p_plane.x), y_plane);
            if !right_most.is_nan() {
                if right_most < x_plane_max + 1.0 {
                    l = js_max(l, right_most - x_plane_min + 1.0);
                } else {
                    break;
                }
            }
        }
        y += incr;
    }
    l
}

/// `Shapes.distancesLeftmost`
#[allow(clippy::too_many_arguments)]
fn distances_leftmost(
    plane: &ScanPlane<'_>,
    hull: &Hull,
    p_plane: Point,
    p_hull: Point,
    scale_prev: f64,
    scale: f64,
    mut r: f64,
    first: f64,
    last: f64,
    incr: f64,
) -> f64 {
    let mut y = first;
    while in_range(y, last, incr) {
        let y_plane = js_round(p_plane.y + (y - p_hull.y) * scale);
        let x_max = hull.x_maxs.at(y);
        if x_max >= p_hull.x {
            let dist_prev = (x_max - p_hull.x) * scale_prev;
            let dist = (x_max - p_hull.x) * scale;
            let x_plane_min = js_round(p_plane.x + dist_prev);
            let x_plane_max = js_round(p_plane.x + dist);
            let left_most = plane.left_most_dark(js_round(p_plane.x), x_plane_max, y_plane);
            if !left_most.is_nan() {
                if left_most > x_plane_min - 1.0 {
                    r = js_min(r, left_most - x_plane_max - 1.0);
                } else {
                    break;
                }
            }
        }
        y += incr;
    }
    r
}

/// `Shapes.distances`
fn distances(plane: &ScanPlane<'_>, hull: &Hull, p_plane: Point, p_hull: Point, scale_prev: f64, scale: f64) -> Dists {
    let mut l = -hull.dist;
    let mut r = hull.dist;
    let mut t = -hull.dist;
    let mut b = hull.dist;
    let (px, py) = (js_round(p_hull.x), js_round(p_hull.y));
    t = distances_bottommost(plane, hull, p_plane, p_hull, scale_prev, scale, t, px, hull.w + hull.dist, 1.0);
    t = distances_bottommost(plane, hull, p_plane, p_hull, scale_prev, scale, t, px - 1.0, -hull.dist - 1.0, -1.0);
    b = distances_topmost(plane, hull, p_plane, p_hull, scale_prev, scale, b, px, hull.w + hull.dist, 1.0);
    b = distances_topmost(plane, hull, p_plane, p_hull, scale_prev, scale, b, px - 1.0, -hull.dist - 1.0, -1.0);
    // Upstream quirk: the last argument of these two backward scans is `-hull.dist` where
    // the two above have `-hull.dist-1`, so the row at `-dist` is not visited.
    l = distances_rightmost(plane, hull, p_plane, p_hull, scale_prev, scale, l, py, hull.h + hull.dist, 1.0);
    l = distances_rightmost(plane, hull, p_plane, p_hull, scale_prev, scale, l, py - 1.0, -hull.dist, -1.0);
    r = distances_leftmost(plane, hull, p_plane, p_hull, scale_prev, scale, r, py, hull.h + hull.dist, 1.0);
    r = distances_leftmost(plane, hull, p_plane, p_hull, scale_prev, scale, r, py - 1.0, -hull.dist, -1.0);
    Dists { l, r, t, b }
}

/// `Shapes.displacement`; `None` is `null`.
pub fn displacement(
    plane: &ScanPlane<'_>,
    hull: &Hull,
    p_plane: Point,
    p_hull: Point,
    scale_prev: f64,
    scale: f64,
) -> Option<Point> {
    let dist = distances(plane, hull, p_plane, p_hull, scale_prev, scale);
    let Dists { l, r, t, b } = dist;
    let mut x = 0.0;
    let mut y = 0.0;
    if l > 0.0 {
        x = if r >= l { l } else { f64::NAN };
    } else if r < 0.0 {
        x = if l <= r { r } else { f64::NAN };
    }
    if t > 0.0 {
        y = if b >= t { t } else { f64::NAN };
    } else if b < 0.0 {
        y = if t <= b { b } else { f64::NAN };
    }
    if x.is_nan() {
        if y.is_nan() {
            return None;
        }
        let y_incr = if y > 0.0 { y + 2.0 } else { y - 2.0 };
        let p_plane2 = Point { x: p_plane.x, y: p_plane.y + y_incr };
        let d = distances(plane, hull, p_plane2, p_hull, scale_prev, scale);
        if d.l < 0.0 && d.r > 0.0 && d.t < 0.0 && d.b > 0.0 {
            x = 0.0;
            y = y_incr;
        } else {
            return None;
        }
    } else if y.is_nan() {
        let x_incr = if x > 0.0 { x + 2.0 } else { x - 2.0 };
        let p_plane2 = Point { x: p_plane.x + x_incr, y: p_plane.y };
        let d = distances(plane, hull, p_plane2, p_hull, scale_prev, scale);
        if d.l < 0.0 && d.r > 0.0 && d.t < 0.0 && d.b > 0.0 {
            x = x_incr;
            y = 0.0;
        } else {
            return None;
        }
    }
    Some(Point { x, y })
}

// ---- insertion places ----
//
// A place is its index in `Group.INSERTION_PLACES`: ts, bs, te, be, m, t, b.

/// `Group.INSERTION_PLACES.length`
pub const N_PLACES: usize = 7;
/// `'m'`
pub const PLACE_M: usize = 4;
/// `'t'`
pub const PLACE_T: usize = 5;
/// `'b'`
pub const PLACE_B: usize = 6;

/// `Shapes.insertionPosition`
pub fn insertion_position(place: usize, adj: Option<Adj>) -> Point {
    let (mut x, mut y) = match place {
        0 => (0.0, 0.0),
        1 => (0.0, 1.0),
        2 => (1.0, 0.0),
        3 => (1.0, 1.0),
        4 => (0.5, 0.5),
        5 => (0.5, 0.0),
        _ => (0.5, 1.0),
    };
    if let Some(a) = adj {
        if let Some(v) = a.x {
            x = v;
        }
        if let Some(v) = a.y {
            y = v;
        }
    }
    Point { x, y }
}

/// `Shapes.mirrorPlace`
pub fn mirror_place(place: usize) -> usize {
    match place {
        0 => 2,
        1 => 3,
        2 => 0,
        3 => 1,
        p => p,
    }
}

/// `Shapes.mirrorAdjustment`
pub fn mirror_adjustment(a: Adj) -> Adj {
    Adj { x: a.x.map(|x| 1.0 - x), y: a.y }
}

/// `Shapes.allowedPlaces`, as a set of places. Upstream's layout does not call it (its editor
/// does, through `allowedPlaces` of `Literal` and `Overlay`).
pub fn allowed_places(ch: char, rot: f64, mirror: bool) -> [bool; N_PLACES] {
    let mut places = [false; N_PLACES];
    if let Some(alts) = tables::insertions(ch) {
        for alt in alts {
            if rot == f64::from(alt.rot) {
                for p in 0..N_PLACES {
                    if alt.place(p).is_some()
                        && let Some(e) = places.get_mut(if mirror { mirror_place(p) } else { p })
                    {
                        *e = true;
                    }
                }
            }
        }
    }
    places
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
    fn js_array_holes_and_negative_indices() {
        let mut a = JsArray::default();
        a.set(-2, 7.0);
        a.set(1, 3.0);
        a.set(3, 1.0);
        assert_eq!(a.get(-2), 7.0);
        assert!(a.get(0).is_nan());
        assert!(a.get(100).is_nan());
        assert!(a.at(f64::NAN).is_nan());
        // reduce visits 1 and 3 only.
        assert_eq!(get_min(a.indexed()), 1.0);
        assert_eq!(get_max(a.indexed()), 3.0);
    }

    #[test]
    fn hull_of_one_pixel() {
        let b = bitmap(5, 5, &[(2, 2)]);
        let h = orthogonal_hull_with_buffer(&b, 2.0).unwrap();
        assert_eq!((h.w, h.h, h.dist), (5.0, 5.0, 2.0));
        // Reached by the buffer: 2 away from the pixel in each direction.
        assert_eq!(h.x_mins.get(2), 0.0);
        assert_eq!(h.x_maxs.get(2), 4.0);
        assert_eq!(h.y_mins.get(2), 0.0);
        assert_eq!(h.y_maxs.get(2), 4.0);
        // Slant is Math.round(2 / sqrt 2) = 1.
        assert_eq!(h.x_mins.get(1), 1.0);
        assert_eq!((h.x_min, h.x_max, h.y_min, h.y_max), (0.0, 4.0, 0.0, 4.0));
    }

    #[test]
    fn hull_of_nothing_spans_the_buffer() {
        let b = bitmap(3, 2, &[]);
        let h = orthogonal_hull_with_buffer(&b, 1.0).unwrap();
        assert_eq!((h.x_min, h.x_max, h.y_min, h.y_max), (-1.0, 3.0, -1.0, 2.0));
    }

    #[test]
    fn hull_with_a_negative_buffer_has_no_panics() {
        let b = bitmap(6, 6, &[(1, 1), (4, 4)]);
        let h = orthogonal_hull_with_buffer(&b, -2.0).unwrap();
        assert_eq!(h.dist, -2.0);
    }

    #[test]
    fn distance_between_two_shapes() {
        let a = bitmap(4, 3, &[(2, 1)]);
        let b = bitmap(4, 3, &[(1, 1)]);
        // 4 - 1 - 2 + 1 = 2
        assert_eq!(h_distance(&a, &b, 0.0, 3.0), 2.0);
        // No row in common: 0.
        assert_eq!(h_distance(&a, &b, 2.0, 3.0), 0.0);
        let a = bitmap(3, 4, &[(1, 2)]);
        let b = bitmap(3, 4, &[(1, 1)]);
        assert_eq!(v_distance(&a, &b, 0.0, 3.0), 2.0);
    }

    #[test]
    fn open_rect_stops_at_either_side() {
        let b = bitmap(5, 3, &[]);
        let r = open_rect(&b, 2.0, 1.0);
        // Grows until its height reaches the canvas height; the `&` ends the loop there.
        assert_eq!(r, PxRect { x: 1.0, y: 0.0, w: 3.0, h: 3.0 });
    }

    #[test]
    fn places() {
        assert_eq!(mirror_place(0), 2);
        assert_eq!(mirror_place(PLACE_M), PLACE_M);
        let p = insertion_position(1, Some(Adj { x: None, y: Some(0.6) }));
        assert_eq!(p, Point { x: 0.0, y: 0.6 });
        assert_eq!(mirror_adjustment(Adj { x: Some(0.25), y: Some(0.5) }), Adj { x: Some(0.75), y: Some(0.5) });
        let a = allowed_places('\u{13000}', 0.0, false);
        assert!(a[0] && a[1] && !a[2]);
        let a = allowed_places('\u{13000}', 0.0, true);
        assert!(a[2] && a[3] && !a[0]);
    }
}
