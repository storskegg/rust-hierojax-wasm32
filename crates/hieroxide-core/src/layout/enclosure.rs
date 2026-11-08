//! `Enclosure` of `src/formatting.js`: a row of groups inside an outline, between optional
//! delimiters, kerned against them by looking at the pixels of both on measuring canvases.

use super::canvas::MeasCanvas;
use super::group::{damage_areas, rotated_char, Frame, Layouter, Outline, Sink, Size, R};
use super::hull::{h_distance, v_distance};
use super::jsmath::{get_max, get_sum, js_max};
use super::options::LayoutOpts;
use super::plan::{Item, Rect};
use super::raster::Raster;
use super::shapes::SignProps;
use super::{LayoutError, Unsupported};
use crate::tree::{damage_char, Node, NodeId, Span};

/// `Shapes.OUTLINE`
const OUTLINE: char = '\u{E45C}';
/// `Shapes.WALLED_OUTLINE`
const WALLED_OUTLINE: char = '\u{E45D}';
/// `Shapes.ENCLOSURE_THICKNESS`
const ENCLOSURE_THICKNESS: f64 = 0.11;
/// `Shapes.WALLED_ENCLOSURE_THICKNESS`
const WALLED_ENCLOSURE_THICKNESS: f64 = 0.13;

const BEGIN_ENCLOSURE: char = '\u{1343C}';
const END_ENCLOSURE: char = '\u{1343D}';
const BEGIN_WALLED_ENCLOSURE: char = '\u{1343E}';
const END_WALLED_ENCLOSURE: char = '\u{1343F}';

/// More outlines than this are not attempted (see [`Unsupported::SpanTooLarge`]).
const MAX_OUTLINES: f64 = (1u32 << 24) as f64;

/// The fields of the constructor.
#[derive(Clone, Copy)]
pub(super) struct Encl {
    pub walled: bool,
    pub groups: Span,
    pub delim_open: Option<char>,
    pub damage_open: u8,
    pub delim_close: Option<char>,
    pub damage_close: u8,
}

/// `delimOpenRotated` and `delimCloseRotated`: the delimiter, or its rotated form when
/// not horizontal. Every delimiter the lexer yields has a rotated form.
fn delim_rotated(delim: Option<char>, o: &LayoutOpts) -> R<Option<char>> {
    match delim {
        None => Ok(None),
        Some(d) if o.h() => Ok(Some(d)),
        Some(d) => rotated_char(d).map(Some).ok_or(LayoutError::Internal("no rotated form")),
    }
}

/// `Enclosure.outlineCh`
fn outline_ch(walled: bool, o: &LayoutOpts) -> R<char> {
    let ch = if walled { WALLED_OUTLINE } else { OUTLINE };
    if o.h() { Ok(ch) } else { rotated_char(ch).ok_or(LayoutError::Internal("no rotated form")) }
}

impl<Ra: Raster> Layouter<'_, Ra> {
    /// `Enclosure.size`
    pub(super) fn size_enclosure(&mut self, id: NodeId, e: Encl, o: &LayoutOpts) -> R<Size> {
        if o.h() {
            let w = self.open_size(id, e, o)?.w
                + self.kern_open_size(id)?
                + self.inner_size(id, e, o)?.w
                + self.kern_close_size(id)?
                + self.close_size(id, e, o)?.w;
            let h = self.outline_size(id, e, o)?.h;
            Ok(Size { w, h })
        } else {
            let w = self.outline_size(id, e, o)?.w;
            let h = self.open_size(id, e, o)?.h
                + self.kern_open_size(id)?
                + self.inner_size(id, e, o)?.h
                + self.kern_close_size(id)?
                + self.close_size(id, e, o)?.h;
            Ok(Size { w, h })
        }
    }

    /// `Enclosure.innerSize`
    fn inner_size(&mut self, id: NodeId, e: Encl, o: &LayoutOpts) -> R<Size> {
        let mut sizes = Vec::with_capacity(e.groups.len());
        for g in self.kids(e.groups)? {
            sizes.push(self.size(g, o)?);
        }
        let scale = self.s(id)?.scale;
        let n = sizes.len() as f64;
        if o.h() {
            let w = get_sum(sizes.iter().map(|s| s.w)) + scale * o.sep * n;
            let h = js_max(get_max(sizes.iter().map(|s| s.h)), 0.0);
            Ok(Size { w, h })
        } else {
            let w = js_max(get_max(sizes.iter().map(|s| s.w)), 0.0);
            let h = get_sum(sizes.iter().map(|s| s.h)) + scale * o.sep * n;
            Ok(Size { w, h })
        }
    }

    /// `Enclosure.openSize`
    fn open_size(&mut self, id: NodeId, e: Encl, o: &LayoutOpts) -> R<Size> {
        self.delim_size(id, delim_rotated(e.delim_open, o)?, o)
    }

    /// `Enclosure.closeSize`
    fn close_size(&mut self, id: NodeId, e: Encl, o: &LayoutOpts) -> R<Size> {
        self.delim_size(id, delim_rotated(e.delim_close, o)?, o)
    }

    fn delim_size(&mut self, id: NodeId, ch: Option<char>, o: &LayoutOpts) -> R<Size> {
        let (w, h) = match ch {
            Some(ch) => {
                let size = self.eng.em_size_of(self.raster, ch, o.fontsize, 1.0, 1.0, 0.0, false)?;
                (size.w, size.h)
            }
            None => (0.0, 0.0),
        };
        let scale = self.s(id)?.scale;
        Ok(Size { w: scale * w, h: scale * h })
    }

    /// `Enclosure.kernOpenSize`
    fn kern_open_size(&mut self, id: NodeId) -> R<f64> {
        let st = self.s(id)?;
        Ok(st.scale * st.kern_open)
    }

    /// `Enclosure.kernCloseSize`
    fn kern_close_size(&mut self, id: NodeId) -> R<f64> {
        let st = self.s(id)?;
        Ok(st.scale * st.kern_close)
    }

    /// `Enclosure.outlineSize`
    fn outline_size(&mut self, id: NodeId, e: Encl, o: &LayoutOpts) -> R<Size> {
        let ch = outline_ch(e.walled, o)?;
        let size = self.eng.em_size_of(self.raster, ch, o.fontsize, 1.0, 1.0, 0.0, false)?;
        let scale = self.s(id)?.scale;
        Ok(Size { w: scale * size.w, h: scale * size.h })
    }

    /// `Enclosure.thickness`
    fn thickness(&mut self, id: NodeId, e: Encl) -> R<f64> {
        let scale = self.s(id)?.scale;
        Ok(scale * if e.walled { WALLED_ENCLOSURE_THICKNESS } else { ENCLOSURE_THICKNESS })
    }

    /// `Enclosure.fit`
    pub(super) fn fit_enclosure(&mut self, id: NodeId, e: Encl, o: &LayoutOpts, w: f64, h: f64) -> R<()> {
        let inner_space = 1.0 - 2.0 * self.thickness(id, e)?;
        for g in self.kids(e.groups)? {
            if o.h() {
                self.fit(g, o, f64::INFINITY, inner_space)?;
            } else {
                self.fit(g, o, inner_space, f64::INFINITY)?;
            }
        }
        self.fit_open(id, e, o)?;
        self.fit_close(id, e, o)?;
        let kern_open = self.kern_open_size(id)?;
        let inner = self.inner_size(id, e, o)?;
        let kern_close = self.kern_close_size(id)?;
        let total = kern_open + if o.h() { inner.w } else { inner.h } + kern_close;
        if total < 0.0 {
            let st = self.s(id)?;
            st.kern_open = 0.0;
            st.kern_close = 0.0;
        }
        self.fit_group(id, o, w, h)
    }

    /// The canvas with the delimiter on it, and its size: the start of `fitOpen` and `fitClose`.
    fn delim_canvas(&mut self, id: NodeId, ch: Option<char>, size: Size) -> R<MeasCanvas> {
        let scale = self.s(id)?.scale;
        let rect = Rect { x: 0.0, y: 0.0, w: size.w, h: size.h };
        let mut printed = MeasCanvas::new(50.0, size.w, size.h);
        let ch = ch.ok_or(LayoutError::Internal("fitting to a missing delimiter"))?;
        printed.add_sign(self.eng, self.raster, ch, scale, 1.0, 1.0, 0.0, false, &rect, &SignProps::default())?;
        Ok(printed)
    }

    /// `Enclosure.fitOpen`
    fn fit_open(&mut self, id: NodeId, e: Encl, o: &LayoutOpts) -> R<()> {
        self.s(id)?.kern_open = 0.0;
        let first = self.kids(e.groups)?.first().copied();
        if let (true, Some(group)) = (e.delim_open.is_some(), first) {
            let meas = o.measure_options();
            let meas_size = meas.fontsize;
            let open_size = self.open_size(id, e, o)?;
            let mut open_printed = self.delim_canvas(id, delim_rotated(e.delim_open, o)?, open_size)?;
            let group_size = self.size(group, o)?;
            let thick = self.thickness(id, e)?;
            let kern = if o.h() {
                let f = Frame {
                    x0: 0.0,
                    x1: 0.0,
                    x2: group_size.w,
                    x3: group_size.w,
                    y0: 0.0,
                    y1: thick,
                    y2: open_size.h - thick,
                    y3: open_size.h,
                };
                self.format(group, &meas, f)?;
                let mut group_printed = MeasCanvas::new(meas_size, group_size.w, open_size.h);
                self.print_on_canvas(group, &meas, &mut group_printed)?;
                let a = open_printed.pixels(self.raster)?;
                let b = group_printed.pixels(self.raster)?;
                -h_distance(a, b, thick * meas_size, (open_size.h - thick) * meas_size) / meas_size
            } else {
                let f = Frame {
                    x0: 0.0,
                    x1: thick,
                    x2: open_size.w - thick,
                    x3: open_size.w,
                    y0: 0.0,
                    y1: 0.0,
                    y2: group_size.h,
                    y3: group_size.h,
                };
                self.format(group, &meas, f)?;
                let mut group_printed = MeasCanvas::new(meas_size, open_size.w, group_size.h);
                self.print_on_canvas(group, &meas, &mut group_printed)?;
                let a = open_printed.pixels(self.raster)?;
                let b = group_printed.pixels(self.raster)?;
                -v_distance(a, b, thick * meas_size, (open_size.w - thick) * meas_size) / meas_size
            };
            self.s(id)?.kern_open = kern;
        }
        Ok(())
    }

    /// `Enclosure.fitClose`
    fn fit_close(&mut self, id: NodeId, e: Encl, o: &LayoutOpts) -> R<()> {
        self.s(id)?.kern_close = 0.0;
        let last = self.kids(e.groups)?.last().copied();
        if let (true, Some(group)) = (e.delim_close.is_some(), last) {
            let meas = o.measure_options();
            let meas_size = meas.fontsize;
            let close_size = self.close_size(id, e, o)?;
            let mut close_printed = self.delim_canvas(id, delim_rotated(e.delim_close, o)?, close_size)?;
            let group_size = self.size(group, o)?;
            let thick = self.thickness(id, e)?;
            let kern = if o.h() {
                let f = Frame {
                    x0: 0.0,
                    x1: 0.0,
                    x2: group_size.w,
                    x3: group_size.w,
                    y0: 0.0,
                    y1: thick,
                    y2: close_size.h - thick,
                    y3: close_size.h,
                };
                self.format(group, &meas, f)?;
                let mut group_printed = MeasCanvas::new(meas_size, group_size.w, close_size.h);
                self.print_on_canvas(group, &meas, &mut group_printed)?;
                let a = group_printed.pixels(self.raster)?;
                let b = close_printed.pixels(self.raster)?;
                -h_distance(a, b, thick * meas_size, (close_size.h - thick) * meas_size) / meas_size
            } else {
                let f = Frame {
                    x0: 0.0,
                    x1: thick,
                    x2: close_size.w - thick,
                    x3: close_size.w,
                    y0: 0.0,
                    y1: 0.0,
                    y2: group_size.h,
                    y3: group_size.h,
                };
                self.format(group, &meas, f)?;
                let mut group_printed = MeasCanvas::new(meas_size, close_size.w, group_size.h);
                self.print_on_canvas(group, &meas, &mut group_printed)?;
                let a = group_printed.pixels(self.raster)?;
                let b = close_printed.pixels(self.raster)?;
                -v_distance(a, b, thick * meas_size, (close_size.w - thick) * meas_size) / meas_size
            };
            self.s(id)?.kern_close = kern;
        }
        Ok(())
    }

    /// `Enclosure.format`
    pub(super) fn format_enclosure(&mut self, id: NodeId, e: Encl, o: &LayoutOpts, f: Frame) -> R<()> {
        let Frame {
            x0: x0_encl,
            x1: x1_encl,
            x2: x2_encl,
            x3: x3_encl,
            y0: y0_encl,
            y1: y1_encl,
            y2: y2_encl,
            y3: y3_encl,
        } = f;
        let size = self.size_enclosure(id, e, o)?;
        let buf_x = ((x2_encl - x1_encl) - size.w) / 2.0;
        let buf_y = ((y2_encl - y1_encl) - size.h) / 2.0;
        let open = self.open_size(id, e, o)?;
        let close = self.close_size(id, e, o)?;
        let outline = self.outline_size(id, e, o)?;
        let scale = self.s(id)?.scale;
        let kern_open = self.kern_open_size(id)?;
        let kern_close = self.kern_close_size(id)?;
        let thick = self.thickness(id, e)?;
        let mut areas = Vec::new();
        let open_rect = Rect { x: x1_encl + buf_x, y: y1_encl + buf_y, w: open.w, h: open.h };
        let kids = self.kids(e.groups)?;
        let n = kids.len();
        let close_rect;
        if o.h() {
            close_rect = Rect { x: x2_encl - buf_x - close.w, y: y1_encl + buf_y, w: close.w, h: close.h };
            self.s(id)?.delim_open_rect = open_rect;
            self.s(id)?.delim_close_rect = close_rect;
            if e.damage_open != 0 {
                let shade_open_width = open_rect.w + kern_open;
                areas.extend(damage_areas(
                    e.damage_open,
                    x0_encl,
                    open_rect.x + shade_open_width / 2.0,
                    open_rect.x + shade_open_width,
                    y0_encl,
                    open_rect.y + open_rect.h / 2.0,
                    y3_encl,
                ));
            }
            if e.damage_close != 0 {
                let shade_close_width = close_rect.w + kern_close;
                areas.extend(damage_areas(
                    e.damage_close,
                    close_rect.x - kern_close,
                    close_rect.x + close_rect.w - shade_close_width / 2.0,
                    x3_encl,
                    y0_encl,
                    close_rect.y + close_rect.h / 2.0,
                    y3_encl,
                ));
            }
            self.s(id)?.areas = areas;
            let mut x0 = open_rect.x + open_rect.w + kern_open;
            let mut x1 = x0 + scale * o.sep / 2.0;
            if e.delim_open.is_none() {
                x0 = x0_encl;
            }
            for (i, &group) in kids.iter().enumerate() {
                let x2 = x1 + self.size(group, o)?.w;
                let x3 = if i < n - 1 {
                    x2 + scale * o.sep / 2.0
                } else if e.delim_close.is_none() {
                    x3_encl
                } else {
                    close_rect.x - kern_close
                };
                self.format(
                    group,
                    o,
                    Frame {
                        x0,
                        x1,
                        x2,
                        x3,
                        y0: y0_encl,
                        y1: y1_encl + buf_y + thick,
                        y2: y2_encl - buf_y - thick,
                        y3: y3_encl,
                    },
                )?;
                x0 = x3;
                x1 = x2 + scale * o.sep;
            }
        } else {
            close_rect = Rect { x: x1_encl + buf_x, y: y2_encl - buf_y - close.h, w: close.w, h: close.h };
            self.s(id)?.delim_open_rect = open_rect;
            self.s(id)?.delim_close_rect = close_rect;
            if e.damage_open != 0 {
                let shade_open_height = open_rect.h + kern_open;
                areas.extend(damage_areas(
                    e.damage_open,
                    x0_encl,
                    open_rect.x + open_rect.w / 2.0,
                    x3_encl,
                    y0_encl,
                    open_rect.y + shade_open_height / 2.0,
                    open_rect.y + shade_open_height,
                ));
            }
            if e.damage_close != 0 {
                let shade_close_height = close_rect.h + kern_close;
                areas.extend(damage_areas(
                    e.damage_close,
                    x0_encl,
                    close_rect.x + close_rect.w / 2.0,
                    x3_encl,
                    close_rect.y - kern_close,
                    close_rect.y + close_rect.h - shade_close_height / 2.0,
                    y3_encl,
                ));
            }
            self.s(id)?.areas = areas;
            let mut y0 = open_rect.y + open_rect.h + kern_open;
            let mut y1 = y0 + scale * o.sep / 2.0;
            if e.delim_open.is_none() {
                y0 = y0_encl;
            }
            for (i, &group) in kids.iter().enumerate() {
                let y2 = y1 + self.size(group, o)?.h;
                let y3 = if i < n - 1 {
                    y2 + scale * o.sep / 2.0
                } else if e.delim_close.is_none() {
                    y3_encl
                } else {
                    close_rect.y - kern_close
                };
                self.format(
                    group,
                    o,
                    Frame {
                        x0: x0_encl,
                        x1: x1_encl + buf_x + thick,
                        x2: x2_encl - buf_x - thick,
                        x3: x3_encl,
                        y0,
                        y1,
                        y2,
                        y3,
                    },
                )?;
                y0 = y3;
                y1 = y2 + scale * o.sep;
            }
        }
        let mut outlines = Vec::new();
        let overlap = 0.02;
        if !kids.is_empty() {
            if o.h() {
                let net_w = (1.0 - overlap) * outline.w;
                let inner_w = close_rect.x - (open_rect.x + open_rect.w - overlap * outline.w);
                if inner_w > 0.0 {
                    let count = js_max(1.0, (inner_w / net_w).floor());
                    if !(count <= MAX_OUTLINES) {
                        return Err(LayoutError::Unsupported(Unsupported::SpanTooLarge));
                    }
                    let scale_x = inner_w / (count * net_w);
                    let scale_y = 1.0;
                    let mut x = open_rect.x + open_rect.w - overlap * outline.w;
                    let y = open_rect.y;
                    let w = scale_x * outline.w;
                    let h = outline.h;
                    let mut i = 0.0;
                    while i < count {
                        outlines.push(Outline { rect: Rect { x, y, w, h }, scale_x, scale_y });
                        x += scale_x * net_w;
                        i += 1.0;
                    }
                }
            } else {
                let net_h = (1.0 - overlap) * outline.h;
                let inner_h = close_rect.y - (open_rect.y + open_rect.h - overlap * outline.h);
                if inner_h > 0.0 {
                    let count = js_max(1.0, (inner_h / net_h).floor());
                    if !(count <= MAX_OUTLINES) {
                        return Err(LayoutError::Unsupported(Unsupported::SpanTooLarge));
                    }
                    let scale_x = 1.0;
                    let scale_y = inner_h / (count * net_h);
                    let x = open_rect.x;
                    let mut y = open_rect.y + open_rect.h - overlap * outline.h;
                    let w = outline.w;
                    let h = scale_y * outline.h;
                    let mut i = 0.0;
                    while i < count {
                        outlines.push(Outline { rect: Rect { x, y, w, h }, scale_x, scale_y });
                        y += scale_y * net_h;
                        i += 1.0;
                    }
                }
            }
        }
        self.s(id)?.outlines = outlines;
        Ok(())
    }

    /// `Enclosure.print`
    pub(super) fn print_enclosure(&mut self, id: NodeId, e: Encl, o: &LayoutOpts, sink: &mut Sink<'_>) -> R<()> {
        let ch_open = delim_rotated(e.delim_open, o)?;
        let ch_close = delim_rotated(e.delim_close, o)?;
        let outline = outline_ch(e.walled, o)?;
        let properties = if o.h() {
            SignProps { y_as: Some(outline), ..SignProps::default() }
        } else {
            SignProps { x_as: Some(outline), unselectable: true, ..SignProps::default() }
        };
        let st = self.s(id)?;
        let (scale, open_rect, close_rect) = (st.scale, st.delim_open_rect, st.delim_close_rect);
        if let Some(ch) = ch_open {
            sink.items.push(Item::Sign {
                ch,
                scale,
                x_scale: 1.0,
                y_scale: 1.0,
                rotate: 0.0,
                mirror: false,
                rect: open_rect,
                props: properties,
            });
            if o.v()
                && let Some(d) = e.delim_open
            {
                sink.hidden_char(d);
            }
            // Upstream quirk: `addHidden(numToDamage(0))` adds an empty hidden element.
            if e.damage_open != 0 {
                sink.hidden_char(damage_char(e.damage_open));
            } else {
                sink.hidden_str("");
            }
        }
        sink.hidden_char(if e.walled { BEGIN_WALLED_ENCLOSURE } else { BEGIN_ENCLOSURE });
        for g in self.kids(e.groups)? {
            self.print(g, o, sink)?;
        }
        sink.hidden_char(if e.walled { END_WALLED_ENCLOSURE } else { END_ENCLOSURE });
        if let Some(ch) = ch_close {
            sink.items.push(Item::Sign {
                ch,
                scale,
                x_scale: 1.0,
                y_scale: 1.0,
                rotate: 0.0,
                mirror: false,
                rect: close_rect,
                props: properties,
            });
            if o.v()
                && let Some(d) = e.delim_close
            {
                sink.hidden_char(d);
            }
            if e.damage_close != 0 {
                sink.hidden_char(damage_char(e.damage_close));
            } else {
                sink.hidden_str("");
            }
        }
        let st = self.s(id)?;
        for out in &st.outlines {
            sink.items.push(Item::Sign {
                ch: outline,
                scale,
                x_scale: out.scale_x,
                y_scale: out.scale_y,
                rotate: 0.0,
                mirror: false,
                rect: out.rect,
                props: SignProps { unselectable: true, ..SignProps::default() },
            });
        }
        for a in &st.areas {
            sink.items.push(Item::Shade(*a));
        }
        Ok(())
    }

    /// `Encl` of a node.
    pub(super) fn encl_of(node: Node) -> Option<Encl> {
        match node {
            Node::Enclosure { walled, groups, delim_open, damage_open, delim_close, damage_close } => {
                Some(Encl { walled, groups, delim_open, damage_open, delim_close, damage_close })
            }
            _ => None,
        }
    }
}
