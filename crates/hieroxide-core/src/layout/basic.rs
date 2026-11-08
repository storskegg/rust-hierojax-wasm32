//! `Basic` of `src/formatting.js`: a sign with others inserted into its empty places. Each
//! insertion is scaled and moved by looking at the pixels of the core, and of the insertions
//! before it, on one measuring canvas.

use super::canvas::MeasCanvas;
use super::group::{Frame, Layouter, Sink, Size, R};
use super::hull::{
    displacement, insertion_position, open_rect, orthogonal_hull_with_buffer, Hull, PlaneKind, Point, PxRect,
    ScanPlane, N_PLACES, PLACE_B, PLACE_M, PLACE_T,
};
use super::jsmath::{js_max, js_min};
use super::options::LayoutOpts;
use super::raster::Raster;
use super::{JsFault, LayoutError};
use crate::jsnum::js_round;
use crate::tables;
use crate::tree::{InsertionId, Node, NodeId};

/// `Group.INSERTION_CHARS`, in the order of the places.
const INSERTION_CHARS: [char; N_PLACES] =
    ['\u{13432}', '\u{13433}', '\u{13434}', '\u{13435}', '\u{13439}', '\u{1343A}', '\u{1343B}'];
const BEGIN_SEGMENT: char = '\u{13437}';
const END_SEGMENT: char = '\u{13438}';

/// `{ l, r, t, b, w, h }`: `Basic.extension`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Extension {
    pub l: f64,
    pub r: f64,
    pub t: f64,
    pub b: f64,
    pub w: f64,
    pub h: f64,
}

/// `{ x0, y0, x1, y1 }`: `group.rect`, which `Basic.fit` leaves on an inserted group.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct InsRect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

/// `Basic.fitInsertedHull`, after the two canvases: the hull, `rectInit`, `insW`, `insH`.
fn fit_inserted_hull(
    meas: &LayoutOpts,
    core: &super::raster::Bitmap,
    ins: &super::raster::Bitmap,
    position: Point,
) -> R<(Hull, PxRect, f64, f64)> {
    let core_w = f64::from(core.w);
    let core_h = f64::from(core.h);
    let x_init = js_min(core_w - 1.0, js_round(position.x * core_w));
    let y_init = js_min(core_h - 1.0, js_round(position.y * core_h));
    let rect_init = open_rect(core, x_init, y_init);
    let margin = js_round(meas.fontsize * meas.sep);
    let hull = orthogonal_hull_with_buffer(ins, margin)?;
    let margin_l = if position.x == 0.0 { 0.0 } else { -hull.x_min };
    let margin_r = if position.x == 1.0 { 0.0 } else { hull.x_max - (hull.w - 1.0) };
    let margin_t = if position.y == 0.0 { 0.0 } else { -hull.y_min };
    let margin_b = if position.y == 1.0 { 0.0 } else { hull.y_max - (hull.h - 1.0) };
    let ins_w = hull.w + margin_l + margin_r;
    let ins_h = hull.h + margin_t + margin_b;
    Ok((hull, rect_init, ins_w, ins_h))
}

/// `Basic.fitInsertedPosition`
fn fit_inserted_position(
    position: Point,
    hull: &Hull,
    rect_init: PxRect,
    ins_w: f64,
    ins_h: f64,
    scale: f64,
) -> (PxRect, Point, Point) {
    let remainder_w = rect_init.w - scale * ins_w;
    let remainder_h = rect_init.h - scale * ins_h;
    let remainder_l = if position.x == 0.0 {
        0.0
    } else if position.x == 1.0 {
        remainder_w
    } else {
        remainder_w / 2.0
    };
    let remainder_t = if position.y == 0.0 {
        0.0
    } else if position.y == 1.0 {
        remainder_h
    } else {
        remainder_h / 2.0
    };
    let rect = PxRect {
        x: rect_init.x + remainder_l,
        y: rect_init.y + remainder_t,
        w: rect_init.w - remainder_w,
        h: rect_init.h - remainder_h,
    };
    let (px, hx) = if position.x == 0.0 {
        (rect.x, 0.0)
    } else if position.x == 1.0 {
        (rect.x + rect.w, hull.w - 1.0)
    } else {
        (rect.x + rect.w / 2.0, hull.w / 2.0)
    };
    let (py, hy) = if position.y == 0.0 {
        (rect.y, 0.0)
    } else if position.y == 1.0 {
        (rect.y + rect.h, hull.h - 1.0)
    } else {
        (rect.y + rect.h / 2.0, hull.h / 2.0)
    };
    (rect, Point { x: px, y: py }, Point { x: hx, y: hy })
}

/// `Array(n)` is a `RangeError` unless `n` is an unsigned 32-bit integer.
fn valid_array_length(n: f64) -> bool {
    n >= 0.0 && n <= 4294967295.0 && n.fract() == 0.0
}

/// `Basic.fitGrow`
fn fit_grow<Ra: Raster>(
    raster: &mut Ra,
    hull: &Hull,
    scale: f64,
    rect: PxRect,
    p_plane: Point,
    p_hull: Point,
    core_plane: &ScanPlane<'_>,
) -> R<(f64, PxRect)> {
    let mut scale = scale;
    let mut rect = rect;
    let mut p_plane = p_plane;
    let incr_factor = 1.1; /* increase in scale between steps */
    let min_scale = 0.15;
    let n_steps = (-raster.log(scale) / raster.log(incr_factor)).ceil();
    if !valid_array_length(n_steps) {
        return Err(LayoutError::Js(JsFault::InvalidArrayLength(n_steps)));
    }
    let mut scalings = Vec::with_capacity(n_steps as usize);
    let mut i = 1.0;
    while i <= n_steps {
        // scale * (1/scale) ** (i/nSteps)
        scalings.push(scale * raster.pow(1.0 / scale, i / n_steps));
        i += 1.0;
    }
    for scale_new in scalings {
        let disp = displacement(core_plane, hull, p_plane, p_hull, scale, scale_new);
        if let Some(d) = disp {
            p_plane = Point { x: p_plane.x + d.x, y: p_plane.y + d.y };
        } else if scale > min_scale {
            break;
        }
        let rect_new = PxRect {
            x: p_plane.x - p_hull.x * scale_new,
            y: p_plane.y - p_hull.y * scale_new,
            w: hull.w * scale_new,
            h: hull.h * scale_new,
        };
        if scale >= 0.55
            && (rect_new.x <= -1.0
                || rect_new.y <= -1.0
                || rect_new.x + rect_new.w >= core_plane.w + 1.0
                || rect_new.y + rect_new.h >= core_plane.h + 1.0)
        {
            break;
        }
        scale = scale_new;
        rect = rect_new;
    }
    Ok((scale, rect))
}

/// `Basic.removeMargins`
fn remove_margins(position: Point, hull: &Hull, scale: f64, rect: PxRect) -> PxRect {
    let w = hull.w * scale;
    let h = hull.h * scale;
    let remain_w = rect.w - w;
    let remain_h = rect.h - h;
    let x = if position.x == 0.0 {
        rect.x
    } else if position.x == 1.0 {
        rect.x + remain_w
    } else {
        rect.x + remain_w / 2.0
    };
    let y = if position.y == 0.0 {
        rect.y
    } else if position.y == 1.0 {
        rect.y + remain_h
    } else {
        rect.y + remain_h / 2.0
    };
    PxRect { x, y, w, h }
}

impl<Ra: Raster> Layouter<'_, Ra> {
    /// The groups in the places of a `Basic` (`this[place]`), in the order of the places.
    pub(super) fn places(&self, insertion: InsertionId) -> R<[Option<NodeId>; N_PLACES]> {
        self.tree
            .insertions
            .get(insertion.0 as usize)
            .copied()
            .ok_or(LayoutError::Internal("insertion out of range"))
    }

    /// `Basic.size`. `extension` is set by `fit`, and everything is fitted before it is
    /// sized (upstream would throw if not).
    pub(super) fn size_basic(&mut self, id: NodeId, core: NodeId, o: &LayoutOpts) -> R<Size> {
        let size = self.size(core, o)?;
        let ext = self.s(id)?.extension.ok_or(LayoutError::Internal("size before fit"))?;
        Ok(Size { w: size.w * ext.w, h: size.h * ext.h })
    }

    /// `Basic.measPrinted`
    fn meas_printed(&mut self, meas: &LayoutOpts, group: NodeId) -> R<MeasCanvas> {
        let meas_size = meas.fontsize;
        self.fit(group, meas, f64::INFINITY, f64::INFINITY)?;
        let size = self.size(group, meas)?;
        self.format(
            group,
            meas,
            Frame { x0: 0.0, x1: 0.0, x2: size.w, x3: size.w, y0: 0.0, y1: 0.0, y2: size.h, y3: size.h },
        )?;
        let mut printed = MeasCanvas::new(meas_size, size.w, size.h);
        self.print_on_canvas(group, meas, &mut printed)?;
        Ok(printed)
    }

    /// `Basic.addInserted`
    fn add_inserted(&mut self, printed: &mut MeasCanvas, meas: &LayoutOpts, core: NodeId, group: NodeId) -> R<()> {
        let size = self.size(core, meas)?;
        let rect = self.s(group)?.ins_rect;
        let x0 = rect.x0 * size.w;
        let x1 = rect.x1 * size.w;
        let y0 = rect.y0 * size.h;
        let y1 = rect.y1 * size.h;
        self.format(group, meas, Frame { x0, x1: x0, x2: x1, x3: x1, y0, y1: y0, y2: y1, y3: y1 })?;
        self.print_on_canvas(group, meas, printed)
    }

    /// `Basic.modifyRect`
    fn modify_rect(&self, rect: PxRect, core: NodeId, place: usize) -> R<PxRect> {
        let dist = js_round(0.2 * rect.h);
        if let Node::Literal { ch, .. } = self.node(core)? {
            if place == PLACE_T && tables::is_topup(ch) {
                return Ok(PxRect { x: rect.x, y: rect.y - dist, w: rect.w, h: rect.h + dist });
            }
            if place == PLACE_B && tables::is_bottomdown(ch) {
                return Ok(PxRect { x: rect.x, y: rect.y + dist, w: rect.w, h: rect.h + dist });
            }
        }
        Ok(rect)
    }

    /// `Basic.fit`
    pub(super) fn fit_basic(
        &mut self,
        id: NodeId,
        core: NodeId,
        insertion: InsertionId,
        o: &LayoutOpts,
        w: f64,
        h: f64,
    ) -> R<()> {
        let meas = o.measure_options();
        let mut printed = self.meas_printed(&meas, core)?;
        let core_w = f64::from(printed.width);
        let core_h = f64::from(printed.height);
        let places = self.places(insertion)?;
        for (place, slot) in places.iter().enumerate() {
            let Some(group) = *slot else { continue };
            let position = insertion_position(place, self.s(core)?.adjustment(place));
            let mut ins_printed = self.meas_printed(&meas, group)?;
            // `fitInsertedHull` reads the core's pixels for `openRect` before the insertion's
            // for the hull, so that is the order an exception finds them in.
            let core_px = printed.pixels(self.raster)?;
            let ins_px = ins_printed.pixels(self.raster)?;
            let (hull, rect_init, ins_w, ins_h) = fit_inserted_hull(&meas, core_px, ins_px, position)?;
            let rect_init = self.modify_rect(rect_init, core, place)?;
            // Math.min(1, rectInit.w / insW, rectInit.h / insH)
            let scale = js_min(js_min(1.0, rect_init.w / ins_w), rect_init.h / ins_h);
            let (rect, p_plane, p_hull) = fit_inserted_position(position, &hull, rect_init, ins_w, ins_h, scale);
            let kind = if place == PLACE_M { PlaneKind::Restricted } else { PlaneKind::Extended };
            let core_plane = ScanPlane::new(core_px, kind);
            let (scale, rect) = fit_grow(self.raster, &hull, scale, rect, p_plane, p_hull, &core_plane)?;
            self.resize(group, scale)?;
            let ins_rect = remove_margins(position, &hull, scale, rect);
            self.s(group)?.ins_rect = InsRect {
                x0: ins_rect.x / core_w,
                y0: ins_rect.y / core_h,
                x1: (ins_rect.x + ins_rect.w) / core_w,
                y1: (ins_rect.y + ins_rect.h) / core_h,
            };
            self.add_inserted(&mut printed, &meas, core, group)?;
        }
        let mut x0 = 0.0;
        let mut x1 = 1.0;
        let mut y0 = 0.0;
        let mut y1 = 1.0;
        for group in places.iter().flatten() {
            let r = self.s(*group)?.ins_rect;
            x0 = js_min(x0, r.x0);
            x1 = js_max(x1, r.x1);
            y0 = js_min(y0, r.y0);
            y1 = js_max(y1, r.y1);
        }
        self.s(id)?.extension =
            Some(Extension { l: -x0, r: x1 - 1.0, t: -y0, b: y1 - 1.0, w: x1 - x0, h: y1 - y0 });
        self.fit_group(id, o, w, h)
    }

    /// `Basic.format`
    pub(super) fn format_basic(
        &mut self,
        id: NodeId,
        core: NodeId,
        insertion: InsertionId,
        o: &LayoutOpts,
        f: Frame,
    ) -> R<()> {
        let Frame { x0, x1, x2, x3, y0, y1, y2, y3 } = f;
        let size = self.size_basic(id, core, o)?;
        let buf_x = ((x2 - x1) - size.w) / 2.0;
        let buf_y = ((y2 - y1) - size.h) / 2.0;
        let x1_net = x1 + buf_x;
        let y1_net = y1 + buf_y;
        let st = self.s(id)?;
        let scale = st.scale;
        let ext = st.extension.ok_or(LayoutError::Internal("format before fit"))?;
        let scale_w = size.w / ext.w;
        let scale_h = size.h / ext.h;
        let x1_core = x1_net + ext.l * scale_w;
        let x2_core = x1_core + scale_w;
        let y1_core = y1_net + ext.t * scale_h;
        let y2_core = y1_core + scale_h;
        self.format(core, o, Frame { x0, x1: x1_core, x2: x2_core, x3, y0, y1: y1_core, y2: y2_core, y3 })?;
        for group in self.places(insertion)?.iter().flatten() {
            let rect = self.s(*group)?.ins_rect;
            let x1_insert = x1_net + (rect.x0 + ext.l) * scale_w;
            let x2_insert = x1_net + (rect.x1 + ext.l) * scale_w;
            let x0_insert = x1_insert - scale * o.sep / 2.0;
            let x3_insert = x2_insert + scale * o.sep / 2.0;
            let y1_insert = y1_net + (rect.y0 + ext.t) * scale_h;
            let y2_insert = y1_net + (rect.y1 + ext.t) * scale_h;
            let y0_insert = y1_insert - scale * o.sep / 2.0;
            let y3_insert = y2_insert + scale * o.sep / 2.0;
            self.format(
                *group,
                o,
                Frame {
                    x0: x0_insert,
                    x1: x1_insert,
                    x2: x2_insert,
                    x3: x3_insert,
                    y0: y0_insert,
                    y1: y1_insert,
                    y2: y2_insert,
                    y3: y3_insert,
                },
            )?;
        }
        Ok(())
    }

    /// `Basic.print`, with `printInsertedExpression`.
    pub(super) fn print_basic(
        &mut self,
        core: NodeId,
        insertion: InsertionId,
        o: &LayoutOpts,
        sink: &mut Sink<'_>,
    ) -> R<()> {
        self.print(core, o, sink)?;
        for (place, slot) in self.places(insertion)?.iter().enumerate() {
            let Some(g) = *slot else { continue };
            if let Some(&control) = INSERTION_CHARS.get(place) {
                sink.hidden_char(control);
            }
            // Basic.isBracketedInsert
            if matches!(self.node(g)?, Node::Vertical { .. } | Node::Horizontal { .. } | Node::Basic { .. }) {
                sink.hidden_char(BEGIN_SEGMENT);
                self.print(g, o, sink)?;
                sink.hidden_char(END_SEGMENT);
            } else {
                self.print(g, o, sink)?;
            }
        }
        Ok(())
    }
}
