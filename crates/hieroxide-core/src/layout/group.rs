//! The group classes of `src/formatting.js`: `Group` (the shared `fit`, `resize` and
//! helpers), `Fragment`, `Vertical`, `Horizontal`, `Literal`, `Singleton`, `Blank`, `Lost`,
//! `BracketOpen` and `BracketClose` here, and `Enclosure`, `Basic` and `Overlay` (with the
//! ligatures) in their own modules.
//!
//! Upstream keeps layout state on the objects, and `fit` and `format` mutate it, so the
//! same tree can be formatted again with other options and what an earlier pass left is
//! still there. The state lives here in a side table indexed by [`NodeId`] ([`St`]) and
//! persists in the same way for as long as the [`Layouter`] does.
//!
//! The methods follow upstream's names and order of operations. Each carries the name of
//! the JavaScript method it ports; `PORT_MAP.md` lists them all. Where upstream's code
//! looks odd it is kept as it is.
//!
//! Group pixels make that persistence matter. `Basic.fit` and `Enclosure.fit` print groups
//! on measuring canvases, with their own options (`Group.measureOptions`, font size 50):
//! `fit` multiplies into `scale` however often it runs, and `format` overwrites `x`, `y`, `w`,
//! `h`, `areas`, `xScale` and the rest for those options, until the real `format` overwrites
//! them again. An enclosure's `kernOpen` and `kernClose` and a `Basic`'s `extension` are from
//! the most recent `fit`, and `rect` on an inserted group from the most recent `fit` of its
//! `Basic`. Nothing here saves or restores any of it, because upstream does not.

use super::basic::{Extension, InsRect};
use super::canvas::MeasCanvas;
use super::hull::{mirror_adjustment, mirror_place, N_PLACES};
use super::jsmath::{get_max, get_sum, js_max, js_min};
use super::options::{Dir, LayoutOpts, Options, PrintKind};
use super::plan::{Item, Plan, Rect, Unit};
use super::raster::Raster;
use super::shapes::{Engine, SignProps};
use super::{LayoutError, Unsupported};
use crate::jsnum::{number_to_string, to_fixed};
use crate::tables::{self, Adj, Alt, LigKind};
use crate::tree::{damage_char, variation_char, Node, NodeId, Span, Tree};

// Group.VER and friends.
const VER: char = '\u{13430}';
const HOR: char = '\u{13431}';
const BEGIN_SEGMENT: char = '\u{13437}';
const END_SEGMENT: char = '\u{13438}';
const MIRROR: char = '\u{13440}';
const FULL_BLANK: char = '\u{13441}';
const HALF_BLANK: char = '\u{13442}';
const FULL_LOST: char = '\u{13443}';
const HALF_LOST: char = '\u{13444}';
const TALL_LOST: char = '\u{13445}';
const WIDE_LOST: char = '\u{13446}';
/// `Shapes.PLACEHOLDER`
const PLACEHOLDER: char = '\u{FFFD}';

/// `Shapes.rotatedChars`
const ROTATED_CHARS: [(char, char); 17] = [
    ('\u{E45C}', '\u{E462}'),
    ('\u{E45D}', '\u{E463}'),
    ('\u{13258}', '\u{E464}'),
    ('\u{13259}', '\u{E465}'),
    ('\u{1325A}', '\u{E466}'),
    ('\u{1325B}', '\u{E467}'),
    ('\u{1325C}', '\u{E468}'),
    ('\u{1325D}', '\u{E469}'),
    ('\u{13282}', '\u{E46A}'),
    ('\u{13286}', '\u{E46B}'),
    ('\u{13287}', '\u{E46C}'),
    ('\u{13288}', '\u{E46D}'),
    ('\u{13289}', '\u{E46E}'),
    ('\u{13379}', '\u{E46F}'),
    ('\u{1337A}', '\u{E470}'),
    ('\u{1337B}', '\u{E471}'),
    ('\u{1342F}', '\u{E472}'),
];

/// `Shapes.rotatedChars[ch]`
pub(super) fn rotated_char(ch: char) -> Option<char> {
    ROTATED_CHARS.iter().find(|e| e.0 == ch).map(|e| e.1)
}

/// `{ w, h }` as `size` returns it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    pub w: f64,
    pub h: f64,
}

/// The eight coordinates `format` receives after the options.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub x0: f64,
    pub x1: f64,
    pub x2: f64,
    pub x3: f64,
    pub y0: f64,
    pub y1: f64,
    pub y2: f64,
    pub y3: f64,
}

/// `adjustments` of a `Literal` or an `Overlay`.
#[derive(Clone, Copy, Debug, Default)]
pub(super) enum Adjustments {
    /// `{ }`
    #[default]
    None,
    /// `alt`
    Alt(&'static Alt),
    /// `Shapes.mirrorAdjustments(alt)`
    Mirrored(&'static Alt),
}

/// One `{ x, y, w, h, scaleX, scaleY }` of `Enclosure.outlines`.
#[derive(Clone, Copy, Debug)]
pub(super) struct Outline {
    pub rect: Rect,
    pub scale_x: f64,
    pub scale_y: f64,
}

/// The fields upstream stores on a group object.
#[derive(Clone, Debug)]
pub(super) struct St {
    /// `Group.scale`
    pub scale: f64,
    /// `x`, `y`, `w`, `h` set by `format` (`undefined`, here NaN, before it ran)
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// `BracketOpen.xScale`, `BracketClose.xScale`
    pub x_scale: f64,
    /// `areas`
    pub areas: Vec<Rect>,
    /// `Literal.logRotation`: the rotation it names, if set
    pub log_rotation: Option<f64>,
    /// `alt`. On `Vertical`, `Horizontal` and `Overlay` the ligature glyph, `null` where
    /// `None`; on `Literal` the glyph that replaces `ch`, `ch` itself where `None`.
    pub alt: Option<char>,
    /// `Overlay.swap`
    pub swap: bool,
    /// `adjustments` of a `Literal` or an `Overlay`.
    pub adjustments: Adjustments,
    /// `Literal.logInsertion`
    pub log_insertion: Option<String>,
    /// `Enclosure.kernOpen`, `kernClose` (`undefined`, here NaN, before `fit`)
    pub kern_open: f64,
    pub kern_close: f64,
    /// `Enclosure.delimOpenRect`, `delimCloseRect`
    pub delim_open_rect: Rect,
    pub delim_close_rect: Rect,
    /// `Enclosure.outlines`
    pub outlines: Vec<Outline>,
    /// `Basic.extension`
    pub extension: Option<Extension>,
    /// `rect`, as `Basic.fit` leaves it on an inserted group.
    pub ins_rect: InsRect,
}

impl St {
    fn new() -> St {
        St {
            scale: 1.0,
            x: f64::NAN,
            y: f64::NAN,
            w: f64::NAN,
            h: f64::NAN,
            x_scale: f64::NAN,
            areas: Vec::new(),
            log_rotation: None,
            alt: None,
            swap: false,
            adjustments: Adjustments::None,
            log_insertion: None,
            kern_open: f64::NAN,
            kern_close: f64::NAN,
            delim_open_rect: Rect { x: f64::NAN, y: f64::NAN, w: f64::NAN, h: f64::NAN },
            delim_close_rect: Rect { x: f64::NAN, y: f64::NAN, w: f64::NAN, h: f64::NAN },
            outlines: Vec::new(),
            extension: None,
            ins_rect: InsRect { x0: f64::NAN, y0: f64::NAN, x1: f64::NAN, y1: f64::NAN },
        }
    }

    /// `this.core.adjustments[place]`, `None` where `place in this.core.adjustments` is
    /// false.
    pub fn adjustment(&self, place: usize) -> Option<Adj> {
        match self.adjustments {
            Adjustments::None => None,
            Adjustments::Alt(alt) => alt.place(place),
            // Shapes.mirrorAdjustments: what the place is mirrored from, mirrored.
            Adjustments::Mirrored(alt) => {
                alt.place(mirror_place(place)).map(mirror_adjustment)
            }
        }
    }
}

pub(super) type R<T> = Result<T, LayoutError>;

/// `Group.numToRotate`
fn num_to_rotate(n: u8) -> f64 {
    match n {
        1 => 90.0,
        2 => 180.0,
        3 => 270.0,
        4 => 45.0,
        5 => 135.0,
        6 => 225.0,
        7 => 315.0,
        _ => 0.0,
    }
}

/// `Group.damageAreas`
pub(super) fn damage_areas(damage: u8, x0: f64, x1: f64, x2: f64, y0: f64, y1: f64, y2: f64) -> Vec<Rect> {
    if damage == 15 {
        return vec![Rect { x: x0, y: y0, w: x2 - x0, h: y2 - y0 }];
    }
    let mut areas = Vec::new();
    if damage & 1 != 0 {
        areas.push(Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 });
    }
    if damage & 2 != 0 {
        areas.push(Rect { x: x0, y: y1, w: x1 - x0, h: y2 - y1 });
    }
    if damage & 4 != 0 {
        areas.push(Rect { x: x1, y: y0, w: x2 - x1, h: y1 - y0 });
    }
    if damage & 8 != 0 {
        areas.push(Rect { x: x1, y: y1, w: x2 - x1, h: y2 - y1 });
    }
    areas
}

/// Collects the calls of one printed object.
pub(super) struct Sink<'p> {
    plan_text: &'p mut String,
    pub items: Vec<Item>,
    log: bool,
    /// Printing on a measuring canvas: `addHidden` does nothing there, so its text is not
    /// kept.
    measure: bool,
    /// Consistent mode: the text of a lost sign is what was written, and nothing is hidden when
    /// there is nothing to hide.
    exact: bool,
}

impl Sink<'_> {
    pub fn hidden_str(&mut self, s: &str) {
        if self.measure || (self.exact && s.is_empty()) {
            return;
        }
        let start = self.plan_text.len() as u32;
        self.plan_text.push_str(s);
        self.items.push(Item::Hidden { start, len: s.len() as u32 });
    }
    pub fn hidden_char(&mut self, c: char) {
        let mut b = [0u8; 4];
        self.hidden_str(c.encode_utf8(&mut b));
    }
    /// `addHidden(this.toString())`
    pub fn hidden_node(&mut self, tree: &Tree, id: NodeId) {
        if self.measure {
            return;
        }
        let start = self.plan_text.len() as u32;
        if self.exact {
            tree.write_node_exact(id, self.plan_text);
        } else {
            tree.write_node(id, self.plan_text);
        }
        let len = self.plan_text.len() as u32 - start;
        self.items.push(Item::Hidden { start, len });
    }
    /// `addLog`: nothing unless `options.log`.
    fn log(&mut self, s: &str) {
        if self.log {
            let start = self.plan_text.len() as u32;
            self.plan_text.push_str(s);
            self.items.push(Item::Log { start, len: s.len() as u32 });
        }
    }
}

/// Lays out one tree. Holds the side table of per-node state.
pub struct Layouter<'a, Ra: Raster> {
    pub(super) tree: &'a Tree,
    pub(super) eng: &'a mut Engine,
    pub(super) raster: &'a mut Ra,
    st: Vec<St>,
}

impl<'a, Ra: Raster> Layouter<'a, Ra> {
    pub fn new(tree: &'a Tree, eng: &'a mut Engine, raster: &'a mut Ra) -> Layouter<'a, Ra> {
        let n = tree.len();
        Layouter { tree, eng, raster, st: vec![St::new(); n] }
    }

    pub(super) fn node(&self, id: NodeId) -> R<Node> {
        self.tree.nodes.get(id.index()).copied().ok_or(LayoutError::Internal("node id out of range"))
    }

    pub(super) fn s(&mut self, id: NodeId) -> R<&mut St> {
        self.st.get_mut(id.index()).ok_or(LayoutError::Internal("node id out of range"))
    }

    pub(super) fn kids(&self, span: Span) -> R<Vec<NodeId>> {
        let t = self.tree;
        t.kids
            .get(span.start as usize..span.start as usize + span.len())
            .map(<[NodeId]>::to_vec)
            .ok_or(LayoutError::Internal("child span out of range"))
    }

    /// What upstream's constructors decide, in the order they run (a child's id is smaller
    /// than its parent's): `chooseAltGlyph` of `Vertical` and `Horizontal`, `findLigature` of
    /// `Overlay`, and `Basic`'s call of `chooseAltGlyph` on its core.
    pub fn prepare(&mut self) -> R<()> {
        for i in 0..self.tree.len() {
            let Some(&node) = self.tree.nodes.get(i) else { continue };
            let (alt, swap) = match node {
                Node::Vertical { groups } => (self.choose_alt_glyph(groups, LigKind::Vertical)?, false),
                Node::Horizontal { groups } => (self.choose_alt_glyph(groups, LigKind::Horizontal)?, false),
                Node::Overlay { lits1, lits2 } => self.find_overlay_ligature(lits1, lits2)?,
                Node::Basic { core, insertion } => {
                    self.choose_alt_of_core(core, &self.places(insertion)?)?;
                    continue;
                }
                _ => continue,
            };
            if let Some(st) = self.st.get_mut(i) {
                st.alt = alt;
                st.swap = swap;
            }
        }
        Ok(())
    }

    /// `this.core.chooseAltGlyph(this.places())`
    fn choose_alt_of_core(&mut self, core: NodeId, places: &[Option<NodeId>; N_PLACES]) -> R<()> {
        let present = |p: usize| places.get(p).is_some_and(Option::is_some);
        match self.node(core)? {
            Node::Overlay { .. } => {
                // Overlay.chooseAltGlyph: this.lig is what alt is until now.
                let Some(lig) = self.s(core)?.alt else { return Ok(()) };
                if let Some(alts) = tables::insertions(lig) {
                    for alt in alts {
                        if (0..N_PLACES).all(|p| !present(p) || alt.place(p).is_some()) {
                            let st = self.s(core)?;
                            if alt.glyph.is_some() {
                                st.alt = alt.glyph;
                            }
                            st.adjustments = Adjustments::Alt(alt);
                            return Ok(());
                        }
                    }
                }
            }
            Node::Literal { ch, vs, mirror, .. } => {
                // Literal.chooseAltGlyph
                if let Some(alts) = tables::insertions(ch) {
                    let rot = num_to_rotate(vs);
                    for alt in alts {
                        let fits = (0..N_PLACES).all(|p| {
                            let m = if mirror { mirror_place(p) } else { p };
                            !present(p) || alt.place(m).is_some()
                        });
                        if fits && rot == f64::from(alt.rot) {
                            let st = self.s(core)?;
                            if alt.glyph.is_some() {
                                st.alt = alt.glyph;
                            }
                            st.adjustments = if mirror { Adjustments::Mirrored(alt) } else { Adjustments::Alt(alt) };
                            return Ok(());
                        }
                    }
                }
                if ch != PLACEHOLDER {
                    // 'Unknown insertions ' + [...places] + ' for ' + this.ch
                    let mut m = String::from("Unknown insertions ");
                    let mut first = true;
                    for (p, name) in crate::tree::PLACE_NAMES.iter().enumerate() {
                        if present(p) {
                            if !first {
                                m.push(',');
                            }
                            m.push_str(name);
                            first = false;
                        }
                    }
                    m.push_str(" for ");
                    m.push(ch);
                    self.s(core)?.log_insertion = Some(m);
                }
            }
            _ => return Err(LayoutError::Internal("the core of an insertion")),
        }
        Ok(())
    }

    // ---- Group ----

    /// `Group.resize`, with the recursion of `Vertical.resize` and `Horizontal.resize`.
    pub(super) fn resize(&mut self, id: NodeId, f: f64) -> R<()> {
        self.s(id)?.scale *= f;
        match self.node(id)? {
            Node::Vertical { groups } | Node::Horizontal { groups } | Node::Enclosure { groups, .. } => {
                for g in self.kids(groups)? {
                    self.resize(g, f)?;
                }
            }
            Node::Basic { core, insertion } => {
                self.resize(core, f)?;
                for g in self.places(insertion)?.iter().flatten() {
                    self.resize(*g, f)?;
                }
            }
            Node::Overlay { lits1, lits2 } => {
                for g in self.kids(lits1)? {
                    self.resize(g, f)?;
                }
                for g in self.kids(lits2)? {
                    self.resize(g, f)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// `Group.fit`
    pub(super) fn fit_group(&mut self, id: NodeId, o: &LayoutOpts, w: f64, h: f64) -> R<()> {
        let size = self.size(id, o)?;
        let mut f = 1.0;
        if w < f64::INFINITY && size.w > 0.0 {
            f = js_min(f, w / size.w);
        }
        if h < f64::INFINITY && size.h > 0.0 {
            f = js_min(f, h / size.h);
        }
        self.resize(id, f)
    }

    /// `fit` of any group: the subclasses' overrides, else `Group.fit`.
    pub(super) fn fit(&mut self, id: NodeId, o: &LayoutOpts, w: f64, h: f64) -> R<()> {
        let node = self.node(id)?;
        if let Some(e) = Self::encl_of(node) {
            return self.fit_enclosure(id, e, o, w, h);
        }
        match node {
            Node::Vertical { groups } => {
                // Vertical.fit
                for g in self.kids(groups)? {
                    self.fit_proper_groups(g, o, 1.0, f64::INFINITY)?;
                }
                self.fit_group(id, o, w, h)
            }
            Node::Horizontal { groups } => {
                // Horizontal.fit
                let height = if self.proper_groups(groups)?.len() == 1 { f64::INFINITY } else { 1.0 };
                for g in self.kids(groups)? {
                    self.fit(g, o, f64::INFINITY, height)?;
                }
                self.fit_group(id, o, w, h)
            }
            Node::Basic { core, insertion } => self.fit_basic(id, core, insertion, o, w, h),
            _ => self.fit_group(id, o, w, h),
        }
    }

    // ---- Fragment ----

    /// `Fragment.size`
    fn size_fragment(&mut self, groups: Span, o: &LayoutOpts) -> R<Size> {
        let kids = self.kids(groups)?;
        let mut sizes = Vec::with_capacity(kids.len());
        for &g in &kids {
            sizes.push(self.size(g, o)?);
        }
        let n = kids.len() as f64;
        if o.h() {
            let w = get_sum(sizes.iter().map(|s| s.w)) + o.sep * js_max(n - 1.0, 0.0);
            let h = js_max(get_max(sizes.iter().map(|s| s.h)), o.linesize);
            Ok(Size { w, h })
        } else {
            let w = js_max(get_max(sizes.iter().map(|s| s.w)), o.linesize);
            let h = get_sum(sizes.iter().map(|s| s.h)) + o.sep * js_max(n - 1.0, 0.0);
            Ok(Size { w, h })
        }
    }

    /// `Fragment.format`
    fn format_fragment(&mut self, id: NodeId, groups: Span, o: &LayoutOpts) -> R<()> {
        let kids = self.kids(groups)?;
        let n = kids.len();
        if o.h() {
            for &g in &kids {
                self.fit(g, o, f64::INFINITY, o.linesize)?;
            }
            let mut x0 = 0.0;
            let mut x = o.sep / 2.0;
            let y0 = 0.0;
            let y1 = o.sep / 2.0;
            let y2 = y1 + o.linesize;
            let y3 = o.linesize + o.sep;
            for (i, &group) in kids.iter().enumerate() {
                let x1 = x + self.size(group, o)?.w;
                let x2 = if i < n - 1 || o.separated {
                    x1 + o.sep / 2.0
                } else {
                    // Upstream quirk: sizes the whole fragment again for the last group.
                    self.size(id, o)?.w + o.sep
                };
                self.format(group, o, Frame { x0, x1: x, x2: x1, x3: x2, y0, y1, y2, y3 })?;
                if !o.separated {
                    x0 = x2;
                    x = x1 + o.sep;
                }
            }
        } else {
            for &g in &kids {
                self.fit(g, o, o.linesize, f64::INFINITY)?;
            }
            let x0 = 0.0;
            let x1 = o.sep / 2.0;
            let x2 = x1 + o.linesize;
            let x3 = o.linesize + o.sep;
            let mut y0 = 0.0;
            let mut y = o.sep / 2.0;
            for (i, &group) in kids.iter().enumerate() {
                let y1 = y + self.size(group, o)?.h;
                let y2 = if i < n - 1 || o.separated {
                    y1 + o.sep / 2.0
                } else {
                    // Upstream quirk: sizes the whole fragment again for the last group.
                    self.size(id, o)?.h + o.sep
                };
                self.format(group, o, Frame { x0, x1, x2, x3, y0, y1: y, y2: y1, y3: y2 })?;
                if !o.separated {
                    y0 = y2;
                    y = y1 + o.sep;
                }
            }
        }
        Ok(())
    }

    // ---- Vertical ----

    /// `Vertical.format`
    fn format_vertical(&mut self, id: NodeId, groups: Span, o: &LayoutOpts, f: Frame) -> R<()> {
        if let Some(alt) = self.s(id)?.alt {
            return self.format_ligature_vertical(id, alt, groups, o, f);
        }
        let Frame { x0, x1, x2, x3, mut y0, mut y1, y2, y3 } = f;
        let kids = self.kids(groups)?;
        let net_height = self.net_height_vertical(groups, o)?;
        let buf = ((y2 - y1) - net_height) / (kids.len() as f64 - 1.0 + self.nested_vertical_spaces(groups)?);
        for (i, &group) in kids.iter().enumerate() {
            let (y4, y5);
            if i < kids.len() - 1 {
                let h = self.net_height_of(group, o)?;
                y4 = y1 + h + self.nested_vertical_spaces_of(group)? * buf;
                y5 = y4 + buf / 2.0;
            } else {
                y4 = y2;
                y5 = y3;
            }
            self.format(group, o, Frame { x0, x1, x2, x3, y0, y1, y2: y4, y3: y5 })?;
            y0 = y5;
            y1 = y4 + buf;
        }
        Ok(())
    }

    /// `Vertical.netHeight`
    fn net_height_vertical(&mut self, groups: Span, o: &LayoutOpts) -> R<f64> {
        let mut v = Vec::new();
        for g in self.kids(groups)? {
            v.push(self.net_height_of(g, o)?);
        }
        Ok(get_sum(v))
    }

    /// `Vertical.nestedVerticalSpaces`
    fn nested_vertical_spaces(&self, groups: Span) -> R<f64> {
        let mut v = Vec::new();
        for g in self.kids(groups)? {
            v.push(self.nested_vertical_spaces_of(g)?);
        }
        Ok(get_sum(v))
    }

    /// The only proper group of a Horizontal if it is a Vertical:
    /// `group instanceof Horizontal && properGroups.length == 1 && properGroup instanceof Vertical`.
    fn single_vertical_in_horizontal(&self, group: NodeId) -> R<Option<NodeId>> {
        if let Node::Horizontal { groups } = self.node(group)? {
            let proper = self.proper_groups(groups)?;
            if proper.len() == 1
                && let Some(&p) = proper.first()
                && matches!(self.node(p)?, Node::Vertical { .. })
            {
                return Ok(Some(p));
            }
        }
        Ok(None)
    }

    /// `Vertical.fitProperGroups`
    fn fit_proper_groups(&mut self, group: NodeId, o: &LayoutOpts, w: f64, h: f64) -> R<()> {
        if let Some(p) = self.single_vertical_in_horizontal(group)? {
            // Vertical.fitProperGroups(properGroup, ...); a Vertical is not a Horizontal,
            // so that call ends in `properGroup.fit`.
            return self.fit_proper_groups(p, o, w, h);
        }
        self.fit(group, o, w, h)
    }

    /// `Vertical.nestedVerticalSpacesOf`
    fn nested_vertical_spaces_of(&self, group: NodeId) -> R<f64> {
        if let Some(p) = self.single_vertical_in_horizontal(group)?
            && let Node::Vertical { groups } = self.node(p)?
        {
            return Ok(groups.len() as f64 - 1.0);
        }
        Ok(0.0)
    }

    /// `Vertical.netHeightOf`
    fn net_height_of(&mut self, group: NodeId, o: &LayoutOpts) -> R<f64> {
        match self.node(group)? {
            Node::Horizontal { .. } => {
                if let Some(p) = self.single_vertical_in_horizontal(group)? {
                    return self.net_height_of(p, o);
                }
            }
            Node::Vertical { groups } => {
                let mut v = Vec::new();
                for g in self.kids(groups)? {
                    v.push(self.net_height_of(g, o)?);
                }
                return Ok(get_sum(v));
            }
            _ => {}
        }
        Ok(self.size(group, o)?.h)
    }

    // ---- Horizontal ----

    /// `Horizontal.properGroups`
    fn proper_groups(&self, groups: Span) -> R<Vec<NodeId>> {
        let mut v = Vec::new();
        for g in self.kids(groups)? {
            if !matches!(self.node(g)?, Node::BracketOpen { .. } | Node::BracketClose { .. }) {
                v.push(g);
            }
        }
        Ok(v)
    }

    /// `Horizontal.netWidth`
    fn net_width(&mut self, groups: Span, o: &LayoutOpts) -> R<f64> {
        let mut v = Vec::new();
        for g in self.proper_groups(groups)? {
            v.push(self.size(g, o)?.w);
        }
        Ok(get_sum(v))
    }

    /// `Horizontal.format`
    fn format_horizontal(&mut self, id: NodeId, groups: Span, o: &LayoutOpts, f: Frame) -> R<()> {
        if let Some(alt) = self.s(id)?.alt {
            return self.format_ligature_horizontal(id, alt, groups, o, f);
        }
        let Frame { mut x0, mut x1, x2, mut x3, y0, y1, y2, y3 } = f;
        let proper = self.proper_groups(groups)?;
        let kids = self.kids(groups)?;
        if proper.len() == 1 {
            let (Some(&first), Some(&last), Some(&only)) = (kids.first(), kids.last(), proper.first()) else {
                return Err(LayoutError::Internal("empty horizontal group"));
            };
            if matches!(self.node(first)?, Node::BracketOpen { .. }) {
                self.format_bracket(first, o, x0, y0, x1 - x0, y3 - y0, true)?;
                x0 = x1;
            }
            if matches!(self.node(last)?, Node::BracketClose { .. }) {
                self.format_bracket(last, o, x2, y0, x3 - x2, y3 - y0, false)?;
                x3 = x2;
            }
            self.format(only, o, Frame { x0, x1, x2, x3, y0, y1, y2, y3 })?;
        } else {
            let net_width = self.net_width(groups, o)?;
            let buf = ((x2 - x1) - net_width) / (proper.len() as f64 - 1.0);
            for (i, &group) in kids.iter().enumerate() {
                match self.node(group)? {
                    Node::BracketOpen { .. } => {
                        self.format_bracket(group, o, x0, y0, x1 - x0, y3 - y0, true)?;
                        x0 = x1;
                    }
                    Node::BracketClose { .. } => {}
                    _ => {
                        let (x4, x5, x6);
                        if i < kids.len() - 1 {
                            x4 = x1 + self.size(group, o)?.w;
                            let next = kids.get(i + 1).copied().ok_or(LayoutError::Internal("next group"))?;
                            if matches!(self.node(next)?, Node::BracketClose { .. }) {
                                self.format_bracket(next, o, x4, y0, buf / 2.0, y3 - y0, false)?;
                                x5 = x4;
                                x6 = x5 + buf / 2.0;
                            } else {
                                x5 = x4 + buf / 2.0;
                                x6 = x5;
                            }
                        } else {
                            x4 = x2;
                            x5 = x3;
                            x6 = x5;
                        }
                        self.format(group, o, Frame { x0, x1, x2: x4, x3: x5, y0, y1, y2, y3 })?;
                        x0 = x6;
                        x1 = x4 + buf;
                    }
                }
            }
        }
        Ok(())
    }

    // ---- Literal, Singleton, Blank, Lost, brackets ----

    /// `Literal.rotation`
    fn literal_rotation(&mut self, id: NodeId, ch: char, vs: u8) -> R<f64> {
        let rot = num_to_rotate(vs);
        if let Some(table) = tables::rotations(ch)
            && let Some(&(_, adj)) = table.iter().find(|e| f64::from(e.0) == rot)
        {
            return Ok(rot + f64::from(adj));
        }
        if rot != 0.0 {
            // Upstream quirk: the log message is a side effect of computing the rotation.
            self.s(id)?.log_rotation = Some(rot);
        }
        Ok(rot)
    }

    /// `Singleton.chRotated`
    fn singleton_ch_rotated(&self, ch: char, o: &LayoutOpts) -> R<char> {
        if o.h() {
            Ok(ch)
        } else {
            // Upstream would pass `undefined` on; every delimiter the parser yields has an entry.
            rotated_char(ch).ok_or(LayoutError::Internal("no rotated form"))
        }
    }

    /// `size` of any group.
    pub fn size(&mut self, id: NodeId, o: &LayoutOpts) -> R<Size> {
        let node = self.node(id)?;
        if let Some(e) = Self::encl_of(node) {
            return self.size_enclosure(id, e, o);
        }
        match node {
            Node::Fragment { groups } => self.size_fragment(groups, o),
            Node::Vertical { groups } => {
                if let Some(alt) = self.s(id)?.alt {
                    return self.size_ligature(id, alt, o);
                }
                let mut sizes = Vec::new();
                for g in self.kids(groups)? {
                    sizes.push(self.size(g, o)?);
                }
                let w = get_max(sizes.iter().map(|s| s.w));
                let scale = self.s(id)?.scale;
                let h = get_sum(sizes.iter().map(|s| s.h)) + scale * o.sep * (groups.len() as f64 - 1.0);
                Ok(Size { w, h })
            }
            Node::Horizontal { groups } => {
                if let Some(alt) = self.s(id)?.alt {
                    return self.size_ligature(id, alt, o);
                }
                let mut sizes = Vec::new();
                for g in self.proper_groups(groups)? {
                    sizes.push(self.size(g, o)?);
                }
                let scale = self.s(id)?.scale;
                let w = get_sum(sizes.iter().map(|s| s.w)) + scale * o.sep * (sizes.len() as f64 - 1.0);
                let h = get_max(sizes.iter().map(|s| s.h));
                Ok(Size { w, h })
            }
            Node::Literal { ch, vs, mirror, .. } => {
                // this.rotation() is evaluated with the arguments, before emSizeOf.
                let rot = self.literal_rotation(id, ch, vs)?;
                let alt = self.s(id)?.alt.unwrap_or(ch);
                let size = self.eng.em_size_of(self.raster, alt, o.fontsize, 1.0, 1.0, rot, mirror)?;
                let scale = self.s(id)?.scale;
                Ok(Size { w: size.w * scale, h: size.h * scale })
            }
            Node::Singleton { ch, .. } => {
                let ch_rotated = self.singleton_ch_rotated(ch, o)?;
                let size = self.eng.em_size_of(self.raster, ch_rotated, o.fontsize, 1.0, 1.0, 0.0, false)?;
                let scale = self.s(id)?.scale;
                Ok(Size { w: size.w * scale, h: size.h * scale })
            }
            Node::Blank { full } => {
                let dim = if full { 1.0 } else { 0.5 };
                let scale = self.s(id)?.scale;
                Ok(Size { w: dim * scale, h: dim * scale })
            }
            Node::Lost { full_width, full_height, .. } => {
                let width = if full_width { 1.0 } else { 0.5 };
                let height = if full_height { 1.0 } else { 0.5 };
                let scale = self.s(id)?.scale;
                Ok(Size { w: width * scale, h: height * scale })
            }
            Node::BracketOpen { .. } | Node::BracketClose { .. } => Ok(Size { w: 0.0, h: 0.0 }),
            Node::Enclosure { .. } => Err(LayoutError::Internal("enclosure")),
            Node::Basic { core, .. } => self.size_basic(id, core, o),
            Node::Overlay { lits1, lits2 } => self.size_overlay(id, lits1, lits2, o),
        }
    }

    /// `BracketOpen.format` (`open`) and `BracketClose.format`.
    fn format_bracket(&mut self, id: NodeId, o: &LayoutOpts, x: f64, y: f64, w: f64, h: f64, open: bool) -> R<()> {
        let ch = match self.node(id)? {
            Node::BracketOpen { ch } | Node::BracketClose { ch } => ch,
            _ => return Err(LayoutError::Internal("not a bracket")),
        };
        let size = self.eng.em_size_of(self.raster, ch, o.fontsize, 1.0, 1.0, 0.0, false)?;
        let natural_ratio = size.w;
        let available_ratio = w / h;
        // Upstream quirk: a NaN ratio (0 / 0) fails `<` and takes the else branch, xScale 1.
        let x_scale = if available_ratio < natural_ratio {
            js_max(0.5, available_ratio / natural_ratio)
        } else {
            1.0
        };
        let w_adjusted = natural_ratio * h * x_scale;
        let st = self.s(id)?;
        st.x_scale = x_scale;
        // BracketOpen: Math.max(x, x+w - wAdjusted); BracketClose: Math.min(...)
        st.x = if open { js_max(x, x + w - w_adjusted) } else { js_min(x, x + w - w_adjusted) };
        st.y = y;
        st.w = w_adjusted;
        st.h = h;
        Ok(())
    }

    /// `format` of any group (the nine-argument form; brackets use `format_bracket`).
    pub(super) fn format(&mut self, id: NodeId, o: &LayoutOpts, f: Frame) -> R<()> {
        let Frame { x0, x1, x2, x3, y0, y1, y2, y3 } = f;
        let node = self.node(id)?;
        if let Some(e) = Self::encl_of(node) {
            return self.format_enclosure(id, e, o, f);
        }
        match node {
            Node::Fragment { .. } => Err(LayoutError::Internal("nested fragment")),
            Node::Vertical { groups } => self.format_vertical(id, groups, o, f),
            Node::Horizontal { groups } => self.format_horizontal(id, groups, o, f),
            Node::Literal { damage, .. } => {
                // Literal.format
                let size = self.size(id, o)?;
                let buf_x = ((x2 - x1) - size.w) / 2.0;
                let buf_y = ((y2 - y1) - size.h) / 2.0;
                let x = x1 + buf_x;
                let y = if o.align_bottom { y2 - size.h } else { y1 + buf_y };
                let (w, h) = (size.w, size.h);
                let x_shade = x + w / 2.0;
                let y_shade = y + h / 2.0;
                let areas = damage_areas(damage, x0, x_shade, x3, y0, y_shade, y3);
                let st = self.s(id)?;
                st.x = x;
                st.y = y;
                st.w = w;
                st.h = h;
                st.areas = areas;
                Ok(())
            }
            Node::Singleton { damage, .. } => {
                // Singleton.format
                let size = self.size(id, o)?;
                let buf_x = ((x2 - x1) - size.w) / 2.0;
                let buf_y = ((y2 - y1) - size.h) / 2.0;
                let x = x1 + buf_x;
                let y = y1 + buf_y;
                let (w, h) = (size.w, size.h);
                let x_shade = x + w / 2.0;
                let y_shade = y + h / 2.0;
                let areas = damage_areas(damage, x0, x_shade, x3, y0, y_shade, y3);
                let st = self.s(id)?;
                st.x = x;
                st.y = y;
                st.w = w;
                st.h = h;
                st.areas = areas;
                Ok(())
            }
            Node::Blank { .. } => Ok(()), // Blank.format is empty
            Node::Lost { expand, .. } => {
                // Lost.format
                let areas = if expand != 0 {
                    vec![Rect { x: x0, w: x3 - x0, y: y0, h: y3 - y0 }]
                } else {
                    let size = self.size(id, o)?;
                    let buf_x = ((x2 - x1) - size.w) / 2.0;
                    let buf_y = ((y2 - y1) - size.h) / 2.0;
                    vec![Rect { x: x1 + buf_x, w: size.w, y: y1 + buf_y, h: size.h }]
                };
                self.s(id)?.areas = areas;
                Ok(())
            }
            Node::BracketOpen { .. } | Node::BracketClose { .. } => {
                Err(LayoutError::Internal("brackets are formatted by their Horizontal"))
            }
            Node::Enclosure { .. } => Err(LayoutError::Internal("enclosure")),
            Node::Basic { core, insertion } => self.format_basic(id, core, insertion, o, f),
            Node::Overlay { lits1, lits2 } => self.format_overlay(id, lits1, lits2, o, f),
        }
    }

    // ---- print ----

    /// `group.print(measOptions, printed)` on a measuring canvas. The printing walk has no
    /// measuring in it, so the signs can go to the canvas after it, in the same order.
    pub(super) fn print_on_canvas(&mut self, id: NodeId, o: &LayoutOpts, canvas: &mut MeasCanvas) -> R<()> {
        let mut text = String::new();
        let mut sink = Sink { plan_text: &mut text, items: Vec::new(), log: false, measure: true, exact: false };
        self.print(id, o, &mut sink)?;
        for item in &sink.items {
            if let Item::Sign { ch, scale, x_scale, y_scale, rotate, mirror, rect, props } = item {
                canvas.add_sign(self.eng, self.raster, *ch, *scale, *x_scale, *y_scale, *rotate, *mirror, rect, props)?;
            }
        }
        Ok(())
    }

    fn rect_of(&mut self, id: NodeId) -> R<Rect> {
        let st = self.s(id)?;
        Ok(Rect { x: st.x, y: st.y, w: st.w, h: st.h })
    }

    fn print_shades(&mut self, id: NodeId, sink: &mut Sink<'_>) -> R<()> {
        for a in &self.s(id)?.areas {
            sink.items.push(Item::Shade(*a));
        }
        Ok(())
    }

    /// `print` of any group.
    pub(super) fn print(&mut self, id: NodeId, o: &LayoutOpts, sink: &mut Sink<'_>) -> R<()> {
        let node = self.node(id)?;
        if let Some(e) = Self::encl_of(node) {
            return self.print_enclosure(id, e, o, sink);
        }
        match node {
            Node::Fragment { .. } => Err(LayoutError::Internal("nested fragment")),
            Node::Vertical { groups } => {
                if let Some(alt) = self.s(id)?.alt {
                    return self.print_ligature(id, alt, sink);
                }
                for (i, g) in self.kids(groups)?.into_iter().enumerate() {
                    if i > 0 {
                        sink.hidden_char(VER);
                    }
                    self.print(g, o, sink)?;
                }
                Ok(())
            }
            Node::Horizontal { groups } => {
                if let Some(alt) = self.s(id)?.alt {
                    return self.print_ligature(id, alt, sink);
                }
                let kids = self.kids(groups)?;
                for (i, &group) in kids.iter().enumerate() {
                    let prev_is_open = match i.checked_sub(1).and_then(|p| kids.get(p)) {
                        Some(&p) => matches!(self.node(p)?, Node::BracketOpen { .. }),
                        None => false,
                    };
                    let this_is_close = matches!(self.node(group)?, Node::BracketClose { .. });
                    if i > 0 && !prev_is_open && !this_is_close {
                        sink.hidden_char(HOR);
                    }
                    if matches!(self.node(group)?, Node::Vertical { .. }) {
                        sink.hidden_char(BEGIN_SEGMENT);
                        self.print(group, o, sink)?;
                        sink.hidden_char(END_SEGMENT);
                    } else {
                        self.print(group, o, sink)?;
                    }
                }
                Ok(())
            }
            Node::Literal { ch, vs, mirror, damage } => {
                // Literal.print
                let rotate = self.literal_rotation(id, ch, vs)?;
                let st = self.s(id)?;
                let scale = st.scale;
                let alt = st.alt.unwrap_or(ch);
                let rect = self.rect_of(id)?;
                sink.items.push(Item::Sign {
                    ch: alt,
                    scale,
                    x_scale: 1.0,
                    y_scale: 1.0,
                    rotate,
                    mirror,
                    rect,
                    props: if alt == ch {
                        SignProps::default()
                    } else {
                        SignProps { unselectable: true, ..SignProps::default() }
                    },
                });
                if alt != ch {
                    sink.hidden_char(ch);
                }
                self.print_shades(id, sink)?;
                if vs != 0 {
                    sink.hidden_char(variation_char(vs));
                }
                if mirror {
                    sink.hidden_char(MIRROR);
                }
                if damage != 0 {
                    sink.hidden_char(damage_char(damage));
                }
                if sink.log
                    && let Some(m) = self.s(id)?.log_insertion.clone()
                {
                    sink.log(&m);
                }
                if let Some(rot) = self.s(id)?.log_rotation {
                    // 'Unknown rotation ' + rot + ' degrees for ' + this.ch
                    let mut m = String::from("Unknown rotation ");
                    number_to_string(rot, &mut m);
                    m.push_str(" degrees for ");
                    m.push(ch);
                    sink.log(&m);
                }
                Ok(())
            }
            Node::Singleton { ch, damage } => {
                let ch_rotated = self.singleton_ch_rotated(ch, o)?;
                let scale = self.s(id)?.scale;
                let rect = self.rect_of(id)?;
                sink.items.push(Item::Sign {
                    ch: ch_rotated,
                    scale,
                    x_scale: 1.0,
                    y_scale: 1.0,
                    rotate: 0.0,
                    mirror: false,
                    rect,
                    props: SignProps::default(),
                });
                self.print_shades(id, sink)?;
                if damage != 0 {
                    sink.hidden_char(damage_char(damage));
                }
                Ok(())
            }
            Node::Blank { full } => {
                sink.hidden_char(if full { FULL_BLANK } else { HALF_BLANK });
                Ok(())
            }
            Node::Lost { full_width, full_height, expand } => {
                // Lost.toString
                let c = match (full_width, full_height) {
                    (false, false) => HALF_LOST,
                    (false, true) => TALL_LOST,
                    (true, false) => WIDE_LOST,
                    (true, true) => FULL_LOST,
                };
                let mut s = String::new();
                s.push(c);
                if expand != 0 {
                    s.push(variation_char(if sink.exact { expand } else { 1 }));
                }
                sink.hidden_str(&s);
                self.print_shades(id, sink)
            }
            Node::BracketOpen { ch } | Node::BracketClose { ch } => {
                let st = self.s(id)?;
                let (h, x_scale) = (st.h, st.x_scale);
                let rect = self.rect_of(id)?;
                sink.items.push(Item::Sign {
                    ch,
                    scale: h,
                    x_scale,
                    y_scale: 1.0,
                    rotate: 0.0,
                    mirror: false,
                    rect,
                    props: SignProps { extra: true, bracket: true, ..SignProps::default() },
                });
                Ok(())
            }
            Node::Enclosure { .. } => Err(LayoutError::Internal("enclosure")),
            Node::Basic { core, insertion } => self.print_basic(core, insertion, o, sink),
            Node::Overlay { lits1, lits2 } => self.print_overlay(id, lits1, lits2, o, sink),
        }
    }

    // ---- Fragment.print ----

    /// `Fragment.print` after the options have been normalized: formats, then walks the
    /// groups the way the printers are driven and records the calls as a [`Plan`].
    pub fn plan(&mut self, opts: &Options) -> R<Plan> {
        self.format_for(opts)?;
        self.print_plan(opts)
    }

    /// The first half of `Fragment.print`: `this.format(options)`.
    pub fn format_for(&mut self, opts: &Options) -> R<()> {
        let o = opts.layout;
        let root = self.tree.root().ok_or(LayoutError::Internal("empty tree"))?;
        let Node::Fragment { groups } = self.node(root)? else {
            return Err(LayoutError::Internal("root is not a fragment"));
        };
        match opts.kind {
            PrintKind::Svg => {}
            PrintKind::Canvas => return Err(LayoutError::Unsupported(Unsupported::CanvasPrinter)),
            PrintKind::Dom => return Err(LayoutError::Unsupported(Unsupported::DomPrinter)),
        }
        self.prepare()?;
        self.format_fragment(root, groups, &o)
    }

    /// The rest of `Fragment.print`, after [`Layouter::format_for`].
    pub fn print_plan(&mut self, opts: &Options) -> R<Plan> {
        let o = opts.layout;
        let root = self.tree.root().ok_or(LayoutError::Internal("empty tree"))?;
        let Node::Fragment { groups } = self.node(root)? else {
            return Err(LayoutError::Internal("root is not a fragment"));
        };
        let mut w_accum = 0.0;
        let mut h_accum = 0.0;
        let size = self.size(root, &o)?;
        let width_sep = size.w + o.sep;
        let height_sep = size.h + o.sep;
        let mut plan = Plan::default();
        let kids = self.kids(groups)?;
        if o.separated {
            if o.dir == Dir::Hrl {
                plan.container.class_name = Some("hierojax-hrl-container");
            } else if o.dir == Dir::Hlr {
                plan.container.class_name = Some("hierojax-hlr-container");
            } else {
                plan.container.class_name = Some("hierojax-v-container");
            }
            if o.v() {
                plan.container.size_em = Some((width_sep, height_sep));
                let mut s = String::new();
                to_fixed((o.fontsize * width_sep).ceil(), 2, &mut s);
                s.push_str("px");
                plan.container.style_width = Some(s);
                let mut s = String::new();
                to_fixed((o.fontsize * height_sep).ceil(), 2, &mut s);
                s.push_str("px");
                plan.container.style_height = Some(s);
            }
            if opts.border {
                plan.container.add_border = true;
            }
            for &g in &kids {
                let size_sub = self.size(g, &o)?;
                let width = if o.v() { js_max(o.linesize, size_sub.w) } else { size_sub.w };
                let height = if o.h() { js_max(o.linesize, size_sub.h) } else { size_sub.h };
                let width_sub_sep = width + o.sep;
                let height_sub_sep = height + o.sep;
                let mut sink = Sink { plan_text: &mut plan.text, items: Vec::new(), log: opts.log, measure: false, exact: self.eng.is_consistent() };
                self.print(g, &o, &mut sink)?;
                let items = sink.items;
                plan.units.push(Unit {
                    w: width_sub_sep,
                    h: height_sub_sep,
                    w_accum,
                    h_accum,
                    reversed: o.rl(),
                    items,
                });
                // printed.width() and printed.height() of PrintedSVG
                if o.h() {
                    w_accum += (o.fontsize * width_sub_sep).ceil();
                } else {
                    h_accum += (o.fontsize * height_sub_sep).ceil();
                }
            }
        } else {
            let mut sink = Sink { plan_text: &mut plan.text, items: Vec::new(), log: opts.log, measure: false, exact: self.eng.is_consistent() };
            for &g in &kids {
                self.print(g, &o, &mut sink)?;
            }
            let items = sink.items;
            plan.units.push(Unit { w: width_sep, h: height_sep, w_accum, h_accum, reversed: o.rl(), items });
        }
        Ok(plan)
    }
}
