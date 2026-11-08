//! The parse tree.
//!
//! One node type per class of upstream's `formatting.js` (`Fragment`, `Vertical`,
//! `Horizontal`, `Enclosure`, `Basic`, `Overlay`, `Literal`, `Singleton`, `Blank`,
//! `Lost`, `BracketOpen`, `BracketClose`), holding only what the constructors are
//! given. Layout fields come with the layout code.
//!
//! # Representation
//!
//! The tree is an arena: all nodes live in one `Vec<Node>` and refer to each other by
//! [`NodeId`] (a `NonZeroU32` index, so `Option<NodeId>` is 4 bytes). Child lists are
//! [`Span`]s into one shared `Vec<NodeId>`, and the seven insertion slots of a `Basic`
//! sit in a side table. The reasons, in order of weight:
//!
//! * a parse costs a handful of allocations in total, not one per node and list, which
//!   matters under wasm's allocator;
//! * `Node` is `Copy` and 24 bytes, so the layout pass can keep its own side tables
//!   indexed by `NodeId` (positions, scales) in plain `Vec`s;
//! * dropping the tree is not recursive, so a deeply nested fragment cannot overflow
//!   the stack on drop (and `write_string` below is iterative too).
//!
//! Children are allocated before their parent, so a child's id is always smaller than
//! its parent's and the root is the last node.

use crate::lexer::PLACES;

/// Index of a node in a [`Tree`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(core::num::NonZeroU32);

impl NodeId {
    pub(crate) fn from_index(i: usize) -> Option<NodeId> {
        u32::try_from(i + 1).ok().and_then(core::num::NonZeroU32::new).map(NodeId)
    }

    /// The position of the node in the arena, usable to index side tables.
    pub fn index(self) -> usize {
        (self.0.get() - 1) as usize
    }
}

/// A range of the tree's child-id array.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

impl Span {
    pub fn len(self) -> usize {
        self.len as usize
    }
    pub fn is_empty(self) -> bool {
        self.len == 0
    }
}

/// Index into the table of insertions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InsertionId(pub(crate) u32);

/// The contents of one insertion: `ts, bs, te, be, m, t, b`, in the order of
/// `Group.INSERTION_PLACES` upstream. `None` where the place is absent.
pub type Insertion = [Option<NodeId>; PLACES];

/// Insertion place names, in slot order.
pub const PLACE_NAMES: [&str; PLACES] = ["ts", "bs", "te", "be", "m", "t", "b"];

/// A node. The comment on each variant gives the upstream class and which classes its
/// children can be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Node {
    /// `Fragment`: Vertical, Horizontal, Enclosure, Basic (as Overlay or Basic),
    /// Literal, Singleton, Blank, Lost.
    Fragment { groups: Span },
    /// `Vertical`: at least two of Horizontal, Enclosure, Basic, Overlay, Literal,
    /// Blank, Lost.
    Vertical { groups: Span },
    /// `Horizontal`: Vertical, Enclosure, Basic, Overlay, Literal, Blank, Lost,
    /// BracketOpen, BracketClose.
    Horizontal { groups: Span },
    /// `Enclosure`. `walled` is upstream's `type == 'walled'`. Delimiters are the
    /// characters before and after the enclosure signs (`None` for null), damage 0..=15.
    Enclosure {
        walled: bool,
        groups: Span,
        delim_open: Option<char>,
        damage_open: u8,
        delim_close: Option<char>,
        damage_close: u8,
    },
    /// `Basic`: `core` is an Overlay or a Literal; the insertions are in the table.
    Basic { core: NodeId, insertion: InsertionId },
    /// `Overlay`: `lits1` and `lits2` are non-empty lists of Literal.
    Overlay { lits1: Span, lits2: Span },
    /// `Literal`: `vs` is 0..=7, `damage` 0..=15.
    Literal { ch: char, vs: u8, mirror: bool, damage: u8 },
    /// `Singleton`: a delimiter with damage 0..=15.
    Singleton { ch: char, damage: u8 },
    /// `Blank`: `full` is `dim == 1`, otherwise 0.5.
    Blank { full: bool },
    /// `Lost`: `full_width` is `width == 1` (else 0.5), likewise `full_height`.
    /// `expand` is the raw variation number 0..=7 (upstream tests it for truthiness).
    Lost { full_width: bool, full_height: bool, expand: u8 },
    /// `BracketOpen`
    BracketOpen { ch: char },
    /// `BracketClose`
    BracketClose { ch: char },
}

/// A parsed fragment. The root is a `Node::Fragment`.
#[derive(Clone, Debug, Default)]
pub struct Tree {
    pub(crate) nodes: Vec<Node>,
    pub(crate) kids: Vec<NodeId>,
    pub(crate) insertions: Vec<Insertion>,
    pub(crate) root: Option<NodeId>,
}

// Upstream's `Group` constants.
const VER: char = '\u{13430}';
const HOR: char = '\u{13431}';
const INSERTION_CHARS: [char; PLACES] =
    ['\u{13432}', '\u{13433}', '\u{13434}', '\u{13435}', '\u{13439}', '\u{1343A}', '\u{1343B}'];
const OVERLAY: char = '\u{13436}';
const BEGIN_SEGMENT: char = '\u{13437}';
const END_SEGMENT: char = '\u{13438}';
const BEGIN_ENCLOSURE: char = '\u{1343C}';
const END_ENCLOSURE: char = '\u{1343D}';
const BEGIN_WALLED_ENCLOSURE: char = '\u{1343E}';
const END_WALLED_ENCLOSURE: char = '\u{1343F}';
const MIRROR: char = '\u{13440}';
const FULL_BLANK: char = '\u{13441}';
const HALF_BLANK: char = '\u{13442}';
const FULL_LOST: char = '\u{13443}';
const HALF_LOST: char = '\u{13444}';
const TALL_LOST: char = '\u{13445}';
const WIDE_LOST: char = '\u{13446}';
const DAMAGE_BASE: u32 = 0x13446;
const VARIATION_BASE: u32 = 0xFDFF;

enum Work {
    Node(NodeId),
    Char(char),
}

impl Tree {
    /// The root node, always a `Node::Fragment`; `None` only for an empty `Tree::default()`.
    pub fn root(&self) -> Option<NodeId> {
        self.root
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The children named by a span.
    pub fn children(&self, span: Span) -> &[NodeId] {
        &self.kids[span.start as usize..(span.start + span.len) as usize]
    }

    pub fn insertion(&self, id: InsertionId) -> &Insertion {
        &self.insertions[id.0 as usize]
    }

    /// Append what upstream's `toString` gives for the whole fragment to `out`.
    pub fn write_string(&self, out: &mut String) {
        if let Some(r) = self.root {
            self.write_node(r, out);
        }
    }

    /// Upstream's `toString`, in a new string.
    pub fn to_string_value(&self) -> String {
        let mut s = String::new();
        self.write_string(&mut s);
        s
    }

    /// Append `toString` of one node to `out`. This uses an explicit stack, so the
    /// depth of the tree does not matter.
    pub fn write_node(&self, id: NodeId, out: &mut String) {
        let mut stack = vec![Work::Node(id)];
        while let Some(w) = stack.pop() {
            let id = match w {
                Work::Char(c) => {
                    out.push(c);
                    continue;
                }
                Work::Node(id) => id,
            };
            // Children are pushed last-first so that they pop first-first.
            match *self.node(id) {
                Node::Fragment { groups } => {
                    for &k in self.children(groups).iter().rev() {
                        stack.push(Work::Node(k));
                    }
                }
                Node::Vertical { groups } => {
                    let ks = self.children(groups);
                    for (i, &k) in ks.iter().enumerate().rev() {
                        stack.push(Work::Node(k));
                        if i > 0 {
                            stack.push(Work::Char(VER));
                        }
                    }
                }
                Node::Horizontal { groups } => {
                    let ks = self.children(groups);
                    for (i, &k) in ks.iter().enumerate().rev() {
                        let is_vertical = matches!(self.node(k), Node::Vertical { .. });
                        if is_vertical {
                            stack.push(Work::Char(END_SEGMENT));
                        }
                        stack.push(Work::Node(k));
                        if is_vertical {
                            stack.push(Work::Char(BEGIN_SEGMENT));
                        }
                        if i > 0
                            && !matches!(self.node(ks[i - 1]), Node::BracketOpen { .. })
                            && !matches!(self.node(k), Node::BracketClose { .. })
                        {
                            stack.push(Work::Char(HOR));
                        }
                    }
                }
                Node::Enclosure { walled, groups, delim_open, damage_open, delim_close, damage_close } => {
                    // Pushed in reverse: close delimiter and damage, end sign, groups,
                    // begin sign, open delimiter and damage.
                    if let Some(d) = delim_close {
                        if damage_close > 0 {
                            stack.push(Work::Char(damage_char(damage_close)));
                        }
                        stack.push(Work::Char(d));
                    }
                    stack.push(Work::Char(if walled { END_WALLED_ENCLOSURE } else { END_ENCLOSURE }));
                    for &k in self.children(groups).iter().rev() {
                        stack.push(Work::Node(k));
                    }
                    stack.push(Work::Char(if walled { BEGIN_WALLED_ENCLOSURE } else { BEGIN_ENCLOSURE }));
                    if let Some(d) = delim_open {
                        if damage_open > 0 {
                            stack.push(Work::Char(damage_char(damage_open)));
                        }
                        stack.push(Work::Char(d));
                    }
                }
                Node::Basic { core, insertion } => {
                    let ins = self.insertion(insertion);
                    for (p, slot) in ins.iter().enumerate().rev() {
                        if let Some(k) = *slot {
                            if matches!(
                                self.node(k),
                                Node::Vertical { .. } | Node::Horizontal { .. } | Node::Basic { .. }
                            ) {
                                stack.push(Work::Char(END_SEGMENT));
                                stack.push(Work::Node(k));
                                stack.push(Work::Char(BEGIN_SEGMENT));
                            } else {
                                stack.push(Work::Node(k));
                            }
                            stack.push(Work::Char(INSERTION_CHARS[p]));
                        }
                    }
                    stack.push(Work::Node(core));
                }
                Node::Overlay { lits1, lits2 } => {
                    // lits1 OVERLAY lits2, pushed in reverse.
                    self.push_flat(&mut stack, lits2, VER);
                    stack.push(Work::Char(OVERLAY));
                    self.push_flat(&mut stack, lits1, HOR);
                }
                Node::Literal { ch, vs, mirror, damage } => {
                    out.push(ch);
                    if vs > 0 {
                        out.push(variation_char(vs));
                    }
                    if mirror {
                        out.push(MIRROR);
                    }
                    if damage > 0 {
                        out.push(damage_char(damage));
                    }
                }
                Node::Singleton { ch, damage } => {
                    out.push(ch);
                    if damage > 0 {
                        out.push(damage_char(damage));
                    }
                }
                Node::Blank { full } => out.push(if full { FULL_BLANK } else { HALF_BLANK }),
                Node::Lost { full_width, full_height, expand } => {
                    out.push(match (full_width, full_height) {
                        (false, false) => HALF_LOST,
                        (false, true) => TALL_LOST,
                        (true, false) => WIDE_LOST,
                        (true, true) => FULL_LOST,
                    });
                    // Upstream: numToVariation(this.expand ? 1 : 0)
                    if expand != 0 {
                        out.push(variation_char(1));
                    }
                }
                Node::BracketOpen { ch } | Node::BracketClose { ch } => out.push(ch),
            }
        }
    }

    /// Push one side of an overlay: a single literal as it is, several wrapped in
    /// begin/end segment and joined by `sep`.
    fn push_flat(&self, stack: &mut Vec<Work>, span: Span, sep: char) {
        let ls = self.children(span);
        if ls.len() > 1 {
            stack.push(Work::Char(END_SEGMENT));
            for (i, &k) in ls.iter().enumerate().rev() {
                stack.push(Work::Node(k));
                if i > 0 {
                    stack.push(Work::Char(sep));
                }
            }
            stack.push(Work::Char(BEGIN_SEGMENT));
        } else if let Some(&k) = ls.first() {
            stack.push(Work::Node(k));
        }
    }

    /// Canonical dump of the tree, the format that `tests/parser/oracle.mjs` documents.
    /// A test aid, and recursive (the parser's depth limit bounds the recursion).
    pub fn dump(&self, out: &mut String) {
        if let Some(r) = self.root {
            self.dump_node(r, out);
        }
    }

    fn dump_list(&self, span: Span, out: &mut String) {
        for (i, &k) in self.children(span).iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            self.dump_node(k, out);
        }
    }

    fn dump_node(&self, id: NodeId, out: &mut String) {
        use core::fmt::Write;
        let hex_or_dash = |o: &mut String, c: Option<char>| match c {
            Some(c) => {
                let _ = write!(o, "{:x}", c as u32);
            }
            None => o.push('-'),
        };
        match *self.node(id) {
            Node::Fragment { groups } => {
                out.push_str("F[");
                self.dump_list(groups, out);
                out.push(']');
            }
            Node::Vertical { groups } => {
                out.push_str("V[");
                self.dump_list(groups, out);
                out.push(']');
            }
            Node::Horizontal { groups } => {
                out.push_str("H[");
                self.dump_list(groups, out);
                out.push(']');
            }
            Node::Enclosure { walled, groups, delim_open, damage_open, delim_close, damage_close } => {
                out.push('E');
                out.push(if walled { 'w' } else { 'p' });
                out.push('(');
                hex_or_dash(out, delim_open);
                let _ = write!(out, ",{damage_open},");
                hex_or_dash(out, delim_close);
                let _ = write!(out, ",{damage_close})[");
                self.dump_list(groups, out);
                out.push(']');
            }
            Node::Basic { core, insertion } => {
                out.push_str("B[");
                self.dump_node(core, out);
                for (p, slot) in self.insertion(insertion).iter().enumerate() {
                    if let Some(k) = *slot {
                        out.push('|');
                        out.push_str(PLACE_NAMES[p]);
                        out.push('=');
                        self.dump_node(k, out);
                    }
                }
                out.push(']');
            }
            Node::Overlay { lits1, lits2 } => {
                out.push_str("O[");
                self.dump_list(lits1, out);
                out.push_str("][");
                self.dump_list(lits2, out);
                out.push(']');
            }
            Node::Literal { ch, vs, mirror, damage } => {
                let _ = write!(out, "L{:x}.{}.{}.{}", ch as u32, vs, u8::from(mirror), damage);
            }
            Node::Singleton { ch, damage } => {
                let _ = write!(out, "S{:x}.{}", ch as u32, damage);
            }
            Node::Blank { full } => out.push_str(if full { "Z1" } else { "Z0.5" }),
            Node::Lost { full_width, full_height, expand } => {
                let d = |full: bool| if full { "1" } else { "0.5" };
                let _ = write!(out, "X{},{},{}", d(full_width), d(full_height), expand);
            }
            Node::BracketOpen { ch } => {
                let _ = write!(out, "BO{:x}", ch as u32);
            }
            Node::BracketClose { ch } => {
                let _ = write!(out, "BC{:x}", ch as u32);
            }
        }
    }
}

fn damage_char(n: u8) -> char {
    char::from_u32(DAMAGE_BASE + u32::from(n)).unwrap_or('\u{FFFD}')
}

fn variation_char(n: u8) -> char {
    char::from_u32(VARIATION_BASE + u32::from(n)).unwrap_or('\u{FFFD}')
}
