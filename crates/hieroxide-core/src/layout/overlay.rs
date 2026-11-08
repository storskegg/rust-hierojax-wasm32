//! `Overlay` of `src/formatting.js`, and the ligature lookups and `formatLigature` of
//! `Vertical`, `Horizontal` and `Overlay`.

use super::group::{damage_areas, Frame, Layouter, Sink, Size, R};
use super::jsmath::{get_max, get_sum, js_max};
use super::options::LayoutOpts;
use super::plan::{Item, Rect};
use super::raster::Raster;
use super::shapes::SignProps;
use super::{JsFault, LayoutError};
use crate::tables::{self, LigKind, Ligature, Sign};
use crate::tree::{NodeId, Span};

const VER: char = '\u{13430}';
const HOR: char = '\u{13431}';
const OVERLAY: char = '\u{13436}';
const BEGIN_SEGMENT: char = '\u{13437}';
const END_SEGMENT: char = '\u{13438}';

/// A literal's `ch`, `mirror` and `vs`, the fields a ligature looks at, and its `damage`.
#[derive(Clone, Copy)]
pub(super) struct LitInfo {
    pub ch: char,
    pub mirror: bool,
    pub vs: u8,
    pub damage: u8,
}

/// `lit.ch != sign.ch || lit.mirror != !!sign.mirror || lit.vs` over two lists.
fn signs_match(signs: &[Sign], lits: &[LitInfo]) -> bool {
    signs.len() == lits.len()
        && signs.iter().zip(lits).all(|(sign, lit)| lit.ch == sign.ch && lit.mirror == sign.mirror && lit.vs == 0)
}

/// `Overlay.matchLigature`
fn match_overlay(lig: &Ligature, lits1: &[LitInfo], lits2: &[LitInfo]) -> bool {
    signs_match(lig.horizontal, lits1) && signs_match(lig.vertical, lits2)
}

impl<Ra: Raster> Layouter<'_, Ra> {
    /// The literals of a span, or `None` if some group is not a literal.
    pub(super) fn lit_infos(&self, span: Span) -> R<Option<Vec<LitInfo>>> {
        let kids = self.kids(span)?;
        let mut lits = Vec::with_capacity(kids.len());
        for k in kids {
            match self.node(k)? {
                crate::tree::Node::Literal { ch, vs, mirror, damage } => lits.push(LitInfo { ch, mirror, vs, damage }),
                _ => return Ok(None),
            }
        }
        Ok(Some(lits))
    }

    /// `Vertical.chooseAltGlyph`, `Horizontal.chooseAltGlyph`: `memoVerticalLigatures` and
    /// `memoHorizontalLigatures` hold the non-`alt` ligatures of a kind by the characters of
    /// their groups, in table order, and the first that `matchLigature` takes wins. A scan of
    /// the table in its order finds the same one.
    pub(super) fn choose_alt_glyph(&self, groups: Span, kind: LigKind) -> R<Option<char>> {
        let Some(lits) = self.lit_infos(groups)? else { return Ok(None) };
        Ok(tables::ligatures().iter().find(|l| l.kind == kind && !l.alt && signs_match(l.groups, &lits)).map(|l| l.ch))
    }

    /// `Overlay.findLigature`: `[ch, swap]`, candidates by the first character of each side,
    /// `lits1` first, each in table order.
    pub(super) fn find_overlay_ligature(&self, lits1: Span, lits2: Span) -> R<(Option<char>, bool)> {
        let (Some(a), Some(b)) = (self.lit_infos(lits1)?, self.lit_infos(lits2)?) else {
            return Err(LayoutError::Internal("overlay of something other than literals"));
        };
        let (Some(fa), Some(fb)) = (a.first(), b.first()) else {
            return Err(LayoutError::Internal("empty overlay"));
        };
        for first in [fa.ch, fb.ch] {
            let candidates = tables::ligatures().iter().filter(|l| {
                l.kind == LigKind::Overlay && !l.alt && l.horizontal.first().is_some_and(|s| s.ch == first)
            });
            for lig in candidates {
                if match_overlay(lig, &a, &b) {
                    return Ok((Some(lig.ch), false));
                } else if a.len() == 1 && b.len() == 1 && match_overlay(lig, &b, &a) {
                    return Ok((Some(lig.ch), true));
                }
            }
        }
        Ok((None, false))
    }

    /// `sizeLigature` of `Vertical`, `Horizontal` and `Overlay`.
    pub(super) fn size_ligature(&mut self, id: NodeId, alt: char, o: &LayoutOpts) -> R<Size> {
        let size = self.eng.em_size_of(self.raster, alt, o.fontsize, 1.0, 1.0, 0.0, false)?;
        let scale = self.s(id)?.scale;
        Ok(Size { w: size.w * scale, h: size.h * scale })
    }

    /// `Overlay.size`
    pub(super) fn size_overlay(&mut self, id: NodeId, lits1: Span, lits2: Span, o: &LayoutOpts) -> R<Size> {
        if let Some(alt) = self.s(id)?.alt {
            return self.size_ligature(id, alt, o);
        }
        let mut sizes1 = Vec::with_capacity(lits1.len());
        for g in self.kids(lits1)? {
            sizes1.push(self.size(g, o)?);
        }
        let mut sizes2 = Vec::with_capacity(lits2.len());
        for g in self.kids(lits2)? {
            sizes2.push(self.size(g, o)?);
        }
        let w = js_max(get_sum(sizes1.iter().map(|s| s.w)), get_max(sizes2.iter().map(|s| s.w)));
        let h = js_max(get_max(sizes1.iter().map(|s| s.h)), get_sum(sizes2.iter().map(|s| s.h)));
        Ok(Size { w, h })
    }

    /// `Overlay.format`
    pub(super) fn format_overlay(&mut self, id: NodeId, lits1: Span, lits2: Span, o: &LayoutOpts, f: Frame) -> R<()> {
        if let Some(alt) = self.s(id)?.alt {
            return self.format_ligature_overlay(id, alt, lits1, lits2, o, f);
        }
        let Frame { x0, x1, x2, x3, y0, y1, y2, y3 } = f;
        let kids1 = self.kids(lits1)?;
        let kids2 = self.kids(lits2)?;
        let mut v = Vec::with_capacity(kids1.len());
        for &g in &kids1 {
            v.push(self.size(g, o)?.w);
        }
        let width1 = get_sum(v);
        let mut v = Vec::with_capacity(kids2.len());
        for &g in &kids2 {
            v.push(self.size(g, o)?.h);
        }
        let height2 = get_sum(v);
        let buf_x = ((x2 - x1) - width1) / 2.0;
        let buf_y = ((y2 - y1) - height2) / 2.0;
        let mut x4 = x0;
        let mut x = x1 + buf_x;
        for (i, &group) in kids1.iter().enumerate() {
            let (x5, x6);
            if i < kids1.len() - 1 {
                x5 = x + self.size(group, o)?.w;
                x6 = x5;
            } else {
                x5 = x2 - buf_x;
                x6 = x3;
            }
            self.format(group, o, Frame { x0: x4, x1: x, x2: x5, x3: x6, y0, y1, y2, y3 })?;
            x = x5;
            x4 = x6;
        }
        let mut y4 = y0;
        let mut y = y1 + buf_y;
        for (i, &group) in kids2.iter().enumerate() {
            let (y5, y6);
            if i < kids2.len() - 1 {
                y5 = y + self.size(group, o)?.h;
                y6 = y5;
            } else {
                y5 = y2 - buf_y;
                y6 = y3;
            }
            self.format(group, o, Frame { x0, x1, x2, x3, y0: y4, y1: y, y2: y5, y3: y6 })?;
            y = y5;
            y4 = y6;
        }
        Ok(())
    }

    /// The first lines of every `formatLigature`: sets `x`, `y`, `w`, `h` and returns the rect.
    fn place_ligature(&mut self, id: NodeId, size: Size, o: &LayoutOpts, f: &Frame) -> R<Rect> {
        let buf_x = ((f.x2 - f.x1) - size.w) / 2.0;
        let buf_y = ((f.y2 - f.y1) - size.h) / 2.0;
        let x = f.x1 + buf_x;
        let y = f.y1 + if o.align_bottom { (f.y2 - f.y1) - size.h } else { buf_y };
        let st = self.s(id)?;
        st.x = x;
        st.y = y;
        st.w = size.w;
        st.h = size.h;
        Ok(Rect { x, y, w: size.w, h: size.h })
    }

    /// `Vertical.formatLigature`
    pub(super) fn format_ligature_vertical(
        &mut self,
        id: NodeId,
        alt: char,
        groups: Span,
        o: &LayoutOpts,
        f: Frame,
    ) -> R<()> {
        let lig = tables::ligature(alt).ok_or(LayoutError::Internal("vertical ligature without an entry"))?;
        let size = self.size_ligature(id, alt, o)?;
        let r = self.place_ligature(id, size, o, &f)?;
        let lits = self.lit_infos(groups)?.ok_or(LayoutError::Internal("ligature of other than literals"))?;
        let mut areas = Vec::new();
        let n = lig.groups.len();
        for (i, s) in lig.groups.iter().enumerate() {
            let damage = lits.get(i).ok_or(LayoutError::Internal("ligature group"))?.damage;
            let x_mid = r.x + (s.x + s.w / 2.0) * r.w;
            let y_min = if i == 0 { f.y0 } else { r.y + s.y * r.h };
            let y_mid = r.y + (s.y + s.h / 2.0) * r.h;
            let y_max = if i == n - 1 { f.y3 } else { r.y + (s.y + s.h) * r.h };
            areas.extend(damage_areas(damage, f.x0, x_mid, f.x3, y_min, y_mid, y_max));
        }
        self.s(id)?.areas = areas;
        Ok(())
    }

    /// `Horizontal.formatLigature`
    pub(super) fn format_ligature_horizontal(
        &mut self,
        id: NodeId,
        alt: char,
        groups: Span,
        o: &LayoutOpts,
        f: Frame,
    ) -> R<()> {
        let lig = tables::ligature(alt).ok_or(LayoutError::Internal("horizontal ligature without an entry"))?;
        let size = self.size_ligature(id, alt, o)?;
        let r = self.place_ligature(id, size, o, &f)?;
        let lits = self.lit_infos(groups)?.ok_or(LayoutError::Internal("ligature of other than literals"))?;
        let mut areas = Vec::new();
        let n = lig.groups.len();
        for (i, s) in lig.groups.iter().enumerate() {
            let damage = lits.get(i).ok_or(LayoutError::Internal("ligature group"))?.damage;
            let x_min = if i == 0 { f.x0 } else { r.x + s.x * r.w };
            let x_mid = r.x + (s.x + s.w / 2.0) * r.w;
            let x_max = if i == n - 1 { f.x3 } else { r.x + (s.x + s.w) * r.w };
            let y_mid = r.y + (s.y + s.h / 2.0) * r.h;
            areas.extend(damage_areas(damage, x_min, x_mid, x_max, f.y0, y_mid, f.y3));
        }
        self.s(id)?.areas = areas;
        Ok(())
    }

    /// `Overlay.formatLigature`
    fn format_ligature_overlay(
        &mut self,
        id: NodeId,
        alt: char,
        lits1: Span,
        lits2: Span,
        o: &LayoutOpts,
        f: Frame,
    ) -> R<()> {
        // `this.alt` can be a glyph from `Shapes.insertions` with no entry in `Shapes.ligatures`.
        let lig = tables::ligature(alt);
        let size = self.size_ligature(id, alt, o)?;
        let r = self.place_ligature(id, size, o, &f)?;
        // Upstream quirk: `this.areas = []` comes before the `lig.horizontal` that throws.
        self.s(id)?.areas = Vec::new();
        let Some(lig) = lig else {
            return Err(LayoutError::Js(JsFault::LigatureUndefined));
        };
        let a = self.lit_infos(lits1)?.ok_or(LayoutError::Internal("overlay"))?;
        let b = self.lit_infos(lits2)?.ok_or(LayoutError::Internal("overlay"))?;
        let swap = self.s(id)?.swap;
        let mut areas = Vec::new();
        let n = lig.horizontal.len();
        for (i, s) in lig.horizontal.iter().enumerate() {
            let side = if swap { &b } else { &a };
            let damage = side.get(i).ok_or(LayoutError::Internal("overlay ligature group"))?.damage;
            let x_min = if i == 0 { f.x0 } else { r.x + s.x * r.w };
            let x_mid = r.x + (s.x + s.w / 2.0) * r.w;
            let x_max = if i == n - 1 { f.x3 } else { r.x + (s.x + s.w) * r.w };
            let y_mid = r.y + (s.y + s.h / 2.0) * r.h;
            areas.extend(damage_areas(damage, x_min, x_mid, x_max, f.y0, y_mid, f.y3));
        }
        let n = lig.vertical.len();
        for (i, s) in lig.vertical.iter().enumerate() {
            let side = if swap { &a } else { &b };
            let damage = side.get(i).ok_or(LayoutError::Internal("overlay ligature group"))?.damage;
            let x_mid = r.x + (s.x + s.w / 2.0) * r.w;
            let y_min = if i == 0 { f.y0 } else { r.y + s.y * r.h };
            let y_mid = r.y + (s.y + s.h / 2.0) * r.h;
            let y_max = if i == n - 1 { f.y3 } else { r.y + (s.y + s.h) * r.h };
            areas.extend(damage_areas(damage, f.x0, x_mid, f.x3, y_min, y_mid, y_max));
        }
        self.s(id)?.areas = areas;
        Ok(())
    }

    /// `printLigature` of `Vertical`, `Horizontal` and `Overlay`.
    pub(super) fn print_ligature(&mut self, id: NodeId, alt: char, sink: &mut Sink<'_>) -> R<()> {
        sink.hidden_node(self.tree, id);
        let st = self.s(id)?;
        let rect = Rect { x: st.x, y: st.y, w: st.w, h: st.h };
        sink.items.push(Item::Sign {
            ch: alt,
            scale: st.scale,
            x_scale: 1.0,
            y_scale: 1.0,
            rotate: 0.0,
            mirror: false,
            rect,
            props: SignProps { unselectable: true, ..SignProps::default() },
        });
        for a in &st.areas {
            sink.items.push(Item::Shade(*a));
        }
        Ok(())
    }

    /// `Overlay.print`
    pub(super) fn print_overlay(&mut self, id: NodeId, lits1: Span, lits2: Span, o: &LayoutOpts, sink: &mut Sink<'_>) -> R<()> {
        if let Some(alt) = self.s(id)?.alt {
            return self.print_ligature(id, alt, sink);
        }
        self.print_overlay_side(lits1, HOR, o, sink)?;
        sink.hidden_char(OVERLAY);
        self.print_overlay_side(lits2, VER, o, sink)
    }

    fn print_overlay_side(&mut self, lits: Span, sep: char, o: &LayoutOpts, sink: &mut Sink<'_>) -> R<()> {
        let kids = self.kids(lits)?;
        if kids.len() > 1 {
            sink.hidden_char(BEGIN_SEGMENT);
            for (i, &g) in kids.iter().enumerate() {
                self.print(g, o, sink)?;
                if i < kids.len() - 1 {
                    sink.hidden_char(sep);
                }
            }
            sink.hidden_char(END_SEGMENT);
        } else if let Some(&g) = kids.first() {
            self.print(g, o, sink)?;
        }
        Ok(())
    }
}
