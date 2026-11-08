//! Parser for hierojax fragments: a hand-written, left-factored recursive-descent
//! parser for the grammar of `src/syntax.jison`.
//!
//! # The grammar is the spec
//!
//! A string is accepted here exactly when jison's parser accepts it, and the tree is
//! the one jison's actions build. This is checked by the differential test in
//! `tests/parser/`. Nothing is more lenient or stricter than the grammar.
//!
//! # Why the grammar needs care
//!
//! The grammar is LR(1) but not LALR(1) (jison builds it with `-p lr`, 16,557 states).
//! The trouble is local: what a `BEGIN_SEGMENT` or a literal starts is only known after
//! some tokens. This parser reads the shared prefix and classifies by what follows:
//!
//! * Everything that can appear between horizontal separators is an *element*: a
//!   literal, a placeholder, an enclosure, a bracketed vertical group, or a basic group
//!   (a literal or an overlay, optionally with insertions). A *chunk* is an element
//!   with an optional opening bracket before it and an optional closing bracket after.
//!   A chunk list `chunk (HOR chunk)*` is what `horizontal_group`, `br_horizontal_group`
//!   and the contents of `br_vertical_group` all start with.
//! * After `BEGIN_SEGMENT`, the chunk list is read, then the next token decides:
//!   `VER` makes a bracketed vertical group (`br_vertical_group`), `END_SEGMENT` makes a
//!   flat horizontal group of an overlay (`br_flat_horizontal_group`, if the chunks are
//!   all plain literals and `OVERLAY` follows), or, inside an insertion only, a
//!   bracketed horizontal group or a bracketed insertion group.
//! * A chunk list is a `horizontal_group` if it has several chunks or any bracket, and a
//!   plain `ver_subgroup` otherwise.
//!
//! Lookahead is one token throughout. There is no backtracking and no table.
//!
//! # Differences from jison
//!
//! * Errors carry a position (in UTF-16 code units) and a kind, not jison's list of
//!   expected tokens.
//! * Nesting is limited to [`MAX_DEPTH`] levels of enclosures plus bracketed segments;
//!   deeper input is an error ([`ErrorKind::TooDeep`]). jison's parser keeps an explicit
//!   stack and has no such limit. Everything else is iterative: long flat input is
//!   parsed in linear time.

use crate::lexer::{Lexer, Tok, PLACES};
use crate::tree::{Insertion, InsertionId, Node, NodeId, Span, Tree};

/// The deepest nesting accepted: enclosures and `BEGIN_SEGMENT .. END_SEGMENT`
/// constructs inside each other. The parser recurses once per level (a few frames each),
/// and this keeps the stack use well below wasm's default 1 MiB stack.
pub const MAX_DEPTH: u32 = 256;

/// What went wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// No lexer rule matches at this position (including a lone surrogate).
    Lex,
    /// A token the grammar does not allow here.
    Unexpected,
    /// Nesting deeper than [`MAX_DEPTH`].
    TooDeep,
    /// More nodes than a `u32` can index.
    TooLarge,
}

/// A parse error. `pos` is an offset in UTF-16 code units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub pos: usize,
    pub kind: ErrorKind,
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let what = match self.kind {
            ErrorKind::Lex => "unrecognized text",
            ErrorKind::Unexpected => "unexpected token",
            ErrorKind::TooDeep => "nesting too deep",
            ErrorKind::TooLarge => "fragment too large",
        };
        write!(f, "parse error at code unit {}: {}", self.pos, what)
    }
}

impl std::error::Error for ParseError {}

/// Parse a fragment from UTF-16. A lone surrogate is an error, as in upstream.
pub fn parse_utf16(src: &[u16]) -> Result<Tree, ParseError> {
    let mut p = Parser {
        lex: Lexer::new(src),
        tok: Tok::Eof,
        tok_pos: 0,
        nodes: Vec::with_capacity(src.len() / 2 + 4),
        kids: Vec::with_capacity(src.len() / 2 + 4),
        insertions: Vec::new(),
        scratch: Vec::new(),
        depth: 0,
    };
    p.fragment()
}

/// Parse a fragment from a string. Positions in errors are in UTF-16 code units.
pub fn parse(src: &str) -> Result<Tree, ParseError> {
    let units: Vec<u16> = src.encode_utf16().collect();
    parse_utf16(&units)
}

type R<T> = Result<T, ParseError>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ctx {
    /// A `BEGIN_SEGMENT` where an element is expected (inside a horizontal or
    /// vertical group, or at the top).
    Elem,
    /// A `BEGIN_SEGMENT` where an `in_subgroup` is expected (after an insertion sign).
    InSub,
}

/// A chunk list that has been read onto `Parser::scratch`, from index `mark`.
struct ChunkList {
    mark: usize,
    chunks: usize,
    /// Some chunk has an opening or closing bracket.
    bracketed: bool,
    /// Every chunk is a plain literal (no brackets, no insertions, no overlay).
    all_lits: bool,
}

struct Parser<'a> {
    lex: Lexer<'a>,
    tok: Tok,
    tok_pos: usize,
    nodes: Vec<Node>,
    kids: Vec<NodeId>,
    insertions: Vec<Insertion>,
    /// Stack on which child lists are collected before being moved to `kids`.
    scratch: Vec<NodeId>,
    depth: u32,
}

impl Parser<'_> {
    fn advance(&mut self) -> R<()> {
        let pos = self.lex.pos();
        match self.lex.next_token() {
            Ok(t) => {
                self.tok = t;
                self.tok_pos = pos;
                Ok(())
            }
            Err(p) => Err(ParseError { pos: p, kind: ErrorKind::Lex }),
        }
    }

    fn unexpected<T>(&self) -> R<T> {
        Err(ParseError { pos: self.tok_pos, kind: ErrorKind::Unexpected })
    }

    fn enter(&mut self) -> R<()> {
        if self.depth >= MAX_DEPTH {
            return Err(ParseError { pos: self.tok_pos, kind: ErrorKind::TooDeep });
        }
        self.depth += 1;
        Ok(())
    }

    fn too_large(&self) -> ParseError {
        ParseError { pos: self.tok_pos, kind: ErrorKind::TooLarge }
    }

    fn push_node(&mut self, n: Node) -> R<NodeId> {
        let id = NodeId::from_index(self.nodes.len()).ok_or_else(|| self.too_large())?;
        self.nodes.push(n);
        Ok(id)
    }

    /// Move the child ids collected on `scratch` since `mark` into `kids`.
    fn take_span(&mut self, mark: usize) -> R<Span> {
        let start = u32::try_from(self.kids.len()).map_err(|_| self.too_large())?;
        let len = u32::try_from(self.scratch.len() - mark).map_err(|_| self.too_large())?;
        self.kids.extend(self.scratch.drain(mark..));
        Ok(Span { start, len })
    }

    fn is_node(&self, id: NodeId, f: fn(&Node) -> bool) -> bool {
        f(&self.nodes[id.index()])
    }

    // fragment : top_groups EOF
    // top_groups : | group top_groups | singleton_group top_groups
    fn fragment(&mut self) -> R<Tree> {
        self.advance()?;
        let mark = self.scratch.len();
        loop {
            match self.tok {
                Tok::Eof => break,
                Tok::Delimiter(ch) => {
                    // singleton_group : delimiter opt_damaged
                    self.advance()?;
                    let damage = self.opt_damaged()?;
                    let n = self.push_node(Node::Singleton { ch, damage })?;
                    self.scratch.push(n);
                }
                t if starts_group(t) => {
                    let g = self.group()?;
                    self.scratch.push(g);
                }
                _ => return self.unexpected(),
            }
        }
        let groups = self.take_span(mark)?;
        let root = self.push_node(Node::Fragment { groups })?;
        Ok(Tree {
            nodes: core::mem::take(&mut self.nodes),
            kids: core::mem::take(&mut self.kids),
            insertions: core::mem::take(&mut self.insertions),
            root: Some(root),
        })
    }

    fn opt_damaged(&mut self) -> R<u8> {
        if let Tok::Damaged(d) = self.tok {
            self.advance()?;
            Ok(d)
        } else {
            Ok(0)
        }
    }

    fn opt_vs(&mut self) -> R<u8> {
        if let Tok::Vs(v) = self.tok {
            self.advance()?;
            Ok(v)
        } else {
            Ok(0)
        }
    }

    // group : vertical_group | horizontal_group | basic_group | literal
    // vertical_group : ver_subgroup (VER ver_subgroup)+
    fn group(&mut self) -> R<NodeId> {
        let first = self.ver_subgroup()?;
        if self.tok != Tok::Ver {
            return Ok(first);
        }
        let mark = self.scratch.len();
        self.scratch.push(first);
        while self.tok == Tok::Ver {
            self.advance()?;
            let v = self.ver_subgroup()?;
            self.scratch.push(v);
        }
        let groups = self.take_span(mark)?;
        self.push_node(Node::Vertical { groups })
    }

    // ver_subgroup : horizontal_group | basic_group | literal
    fn ver_subgroup(&mut self) -> R<NodeId> {
        let list = self.chunk_list()?;
        self.collapse(list)
    }

    /// Turn a chunk list into a `horizontal_group`, or, if it is one plain element,
    /// into that element (which must not be a bracketed vertical group, since a
    /// `ver_subgroup` has no such alternative).
    fn collapse(&mut self, list: ChunkList) -> R<NodeId> {
        if list.chunks == 1 && !list.bracketed {
            let Some(e) = self.scratch.pop() else { return self.unexpected() };
            if self.is_node(e, |n| matches!(n, Node::Vertical { .. })) {
                return self.unexpected();
            }
            return Ok(e);
        }
        let groups = self.take_span(list.mark)?;
        self.push_node(Node::Horizontal { groups })
    }

    // chunk : bracket_open? element bracket_close?
    // chunk_list : chunk (HOR chunk)*
    //
    // Collects the flattened nodes (bracket, element, bracket, ...) on `scratch`.
    fn chunk_list(&mut self) -> R<ChunkList> {
        let mark = self.scratch.len();
        let mut chunks = 0;
        let mut bracketed = false;
        let mut all_lits = true;
        loop {
            chunks += 1;
            let mut plain = true;
            if let Tok::BracketOpen(ch) = self.tok {
                let n = self.push_node(Node::BracketOpen { ch })?;
                self.scratch.push(n);
                self.advance()?;
                plain = false;
            }
            let e = self.element()?;
            if !self.is_node(e, |n| matches!(n, Node::Literal { .. })) {
                all_lits = false;
            }
            self.scratch.push(e);
            if let Tok::BracketClose(ch) = self.tok {
                let n = self.push_node(Node::BracketClose { ch })?;
                self.scratch.push(n);
                self.advance()?;
                plain = false;
            }
            if !plain {
                bracketed = true;
                all_lits = false;
            }
            if self.tok == Tok::Hor {
                self.advance()?;
            } else {
                break;
            }
        }
        Ok(ChunkList { mark, chunks, bracketed, all_lits })
    }

    // element : hor_subgroup | literal
    // hor_subgroup : br_vertical_group | basic_group
    // basic_group : core_group | insertion_group | placeholder | enclosure
    fn element(&mut self) -> R<NodeId> {
        match self.tok {
            Tok::Sign(_) => {
                let l = self.literal()?;
                let head = if self.tok == Tok::Overlay { self.overlay_from_literal(l)? } else { l };
                self.with_insertion(head)
            }
            Tok::BeginSegment => {
                self.enter()?;
                self.advance()?;
                let n = self.segment(Ctx::Elem)?;
                self.depth -= 1;
                if self.is_node(n, |n| matches!(n, Node::Overlay { .. })) {
                    self.with_insertion(n)
                } else {
                    Ok(n)
                }
            }
            Tok::FullBlank
            | Tok::HalfBlank
            | Tok::FullLost
            | Tok::HalfLost
            | Tok::TallLost
            | Tok::WideLost => self.placeholder(),
            Tok::EnclosureOpening { .. } => self.enclosure(),
            _ => self.unexpected(),
        }
    }

    /// `core insertion` if an insertion follows, else `core` itself.
    fn with_insertion(&mut self, core: NodeId) -> R<NodeId> {
        if matches!(self.tok, Tok::Insert(_)) {
            let insertion = self.insertion()?;
            self.push_node(Node::Basic { core, insertion })
        } else {
            Ok(core)
        }
    }

    /// The part after a consumed `BEGIN_SEGMENT`, up to and including `END_SEGMENT`.
    /// The possibilities, told apart by the token after the chunk list:
    ///
    /// * `VER`: `br_vertical_group` (both contexts);
    /// * `END_SEGMENT`, all chunks plain literals, `OVERLAY` next: the flat horizontal
    ///   side of a `core_group` (both contexts);
    /// * `END_SEGMENT` otherwise, in an insertion only: `br_horizontal_group`, or
    ///   `br_insertion_group` when the list is one basic group with insertions.
    fn segment(&mut self, ctx: Ctx) -> R<NodeId> {
        let list = self.chunk_list()?;
        match self.tok {
            Tok::Ver => {
                let first = self.collapse(list)?;
                let mark = self.scratch.len();
                self.scratch.push(first);
                while self.tok == Tok::Ver {
                    self.advance()?;
                    let l = self.chunk_list()?;
                    let v = self.collapse(l)?;
                    self.scratch.push(v);
                }
                if self.tok != Tok::EndSegment {
                    return self.unexpected();
                }
                self.advance()?;
                let groups = self.take_span(mark)?;
                self.push_node(Node::Vertical { groups })
            }
            Tok::EndSegment => {
                if list.all_lits && list.chunks >= 2 {
                    self.advance()?;
                    if self.tok == Tok::Overlay {
                        let lits1 = self.take_span(list.mark)?;
                        return self.overlay_rest(lits1);
                    }
                    if ctx == Ctx::InSub {
                        let groups = self.take_span(list.mark)?;
                        return self.push_node(Node::Horizontal { groups });
                    }
                    return self.unexpected();
                }
                if ctx == Ctx::Elem {
                    return self.unexpected();
                }
                if list.chunks == 1 && !list.bracketed {
                    // br_insertion_group
                    let Some(&e) = self.scratch.last() else { return self.unexpected() };
                    if !self.is_node(e, |n| matches!(n, Node::Basic { .. })) {
                        return self.unexpected();
                    }
                    self.scratch.pop();
                    self.advance()?;
                    return Ok(e);
                }
                // br_horizontal_group
                let groups = self.take_span(list.mark)?;
                self.advance()?;
                self.push_node(Node::Horizontal { groups })
            }
            _ => self.unexpected(),
        }
    }

    // in_subgroup : br_vertical_group | br_horizontal_group | br_insertion_group
    //             | core_group | literal | placeholder | enclosure
    fn in_subgroup(&mut self) -> R<NodeId> {
        match self.tok {
            Tok::BeginSegment => {
                self.enter()?;
                self.advance()?;
                let n = self.segment(Ctx::InSub)?;
                self.depth -= 1;
                Ok(n)
            }
            Tok::Sign(_) => {
                let l = self.literal()?;
                if self.tok == Tok::Overlay {
                    self.overlay_from_literal(l)
                } else {
                    Ok(l)
                }
            }
            Tok::FullBlank
            | Tok::HalfBlank
            | Tok::FullLost
            | Tok::HalfLost
            | Tok::TallLost
            | Tok::WideLost => self.placeholder(),
            Tok::EnclosureOpening { .. } => self.enclosure(),
            _ => self.unexpected(),
        }
    }

    // insertion : INSERT_x in_subgroup, then the later places in order, each optional.
    fn insertion(&mut self) -> R<InsertionId> {
        let mut slots: Insertion = [None; PLACES];
        let mut last: i32 = -1;
        while let Tok::Insert(i) = self.tok {
            if i32::from(i) <= last {
                break;
            }
            self.advance()?;
            let n = self.in_subgroup()?;
            slots[usize::from(i)] = Some(n);
            last = i32::from(i);
        }
        let id = u32::try_from(self.insertions.len()).map_err(|_| self.too_large())?;
        self.insertions.push(slots);
        Ok(InsertionId(id))
    }

    // literal : SIGN opt_vs opt_mirror opt_damaged
    fn literal(&mut self) -> R<NodeId> {
        let Tok::Sign(ch) = self.tok else { return self.unexpected() };
        self.advance()?;
        let vs = self.opt_vs()?;
        let mirror = self.tok == Tok::Mirror;
        if mirror {
            self.advance()?;
        }
        let damage = self.opt_damaged()?;
        self.push_node(Node::Literal { ch, vs, mirror, damage })
    }

    /// `lit` is read and the current token is `OVERLAY`.
    fn overlay_from_literal(&mut self, lit: NodeId) -> R<NodeId> {
        let mark = self.scratch.len();
        self.scratch.push(lit);
        let lits1 = self.take_span(mark)?;
        self.overlay_rest(lits1)
    }

    /// The current token is `OVERLAY`; read it and the flat vertical group after it.
    fn overlay_rest(&mut self, lits1: Span) -> R<NodeId> {
        self.advance()?;
        let mark = self.scratch.len();
        match self.tok {
            Tok::BeginSegment => {
                // br_flat_vertical_group : BEGIN_SEGMENT literal (VER literal)+ END_SEGMENT
                self.advance()?;
                let l = self.literal()?;
                self.scratch.push(l);
                if self.tok != Tok::Ver {
                    return self.unexpected();
                }
                while self.tok == Tok::Ver {
                    self.advance()?;
                    let l = self.literal()?;
                    self.scratch.push(l);
                }
                if self.tok != Tok::EndSegment {
                    return self.unexpected();
                }
                self.advance()?;
            }
            Tok::Sign(_) => {
                let l = self.literal()?;
                self.scratch.push(l);
            }
            _ => return self.unexpected(),
        }
        let lits2 = self.take_span(mark)?;
        self.push_node(Node::Overlay { lits1, lits2 })
    }

    // placeholder : FULL_BLANK | HALF_BLANK | FULL_LOST opt_vs | ...
    fn placeholder(&mut self) -> R<NodeId> {
        let t = self.tok;
        self.advance()?;
        let node = match t {
            Tok::FullBlank => Node::Blank { full: true },
            Tok::HalfBlank => Node::Blank { full: false },
            Tok::FullLost => Node::Lost { full_width: true, full_height: true, expand: self.opt_vs()? },
            Tok::HalfLost => Node::Lost { full_width: false, full_height: false, expand: self.opt_vs()? },
            Tok::TallLost => Node::Lost { full_width: false, full_height: true, expand: self.opt_vs()? },
            Tok::WideLost => Node::Lost { full_width: true, full_height: false, expand: self.opt_vs()? },
            _ => return self.unexpected(),
        };
        self.push_node(node)
    }

    // enclosure : opening groups closing
    fn enclosure(&mut self) -> R<NodeId> {
        let Tok::EnclosureOpening { delim: delim_open, damage: damage_open, walled } = self.tok else {
            return self.unexpected();
        };
        self.enter()?;
        self.advance()?;
        let mark = self.scratch.len();
        let (delim_close, damage_close) = loop {
            match self.tok {
                Tok::EnclosureClosing { delim, damage } => break (delim, damage),
                t if starts_group(t) => {
                    let g = self.group()?;
                    self.scratch.push(g);
                }
                _ => return self.unexpected(),
            }
        };
        self.advance()?;
        self.depth -= 1;
        let groups = self.take_span(mark)?;
        self.push_node(Node::Enclosure { walled, groups, delim_open, damage_open, delim_close, damage_close })
    }
}

/// Tokens a `group` can start with.
fn starts_group(t: Tok) -> bool {
    matches!(
        t,
        Tok::Sign(_)
            | Tok::BracketOpen(_)
            | Tok::BeginSegment
            | Tok::FullBlank
            | Tok::HalfBlank
            | Tok::FullLost
            | Tok::HalfLost
            | Tok::TallLost
            | Tok::WideLost
            | Tok::EnclosureOpening { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: char = '\u{13000}';
    const B: char = '\u{13001}';
    const C: char = '\u{13002}';
    const VER: char = '\u{13430}';
    const HOR: char = '\u{13431}';
    const TS: char = '\u{13432}';
    const BS_: char = '\u{13433}';
    const OVL: char = '\u{13436}';
    const BEG: char = '\u{13437}';
    const END: char = '\u{13438}';

    fn rt(s: &str) -> Option<String> {
        parse(s).ok().map(|t| t.to_string_value())
    }

    fn dump(s: &str) -> String {
        let mut o = String::new();
        parse(s).unwrap().dump(&mut o);
        o
    }

    #[test]
    fn empty_and_simple() {
        assert_eq!(dump(""), "F[]");
        assert_eq!(dump(&A.to_string()), "F[L13000.0.0.0]");
        let s = format!("{A}{B}");
        assert_eq!(dump(&s), "F[L13000.0.0.0,L13001.0.0.0]");
        assert_eq!(rt(&s).unwrap(), s);
    }

    #[test]
    fn vertical_and_horizontal() {
        let s = format!("{A}{VER}{B}");
        assert_eq!(dump(&s), "F[V[L13000.0.0.0,L13001.0.0.0]]");
        let s = format!("{A}{HOR}{B}");
        assert_eq!(dump(&s), "F[H[L13000.0.0.0,L13001.0.0.0]]");
        // vertical of a horizontal and a literal
        let s = format!("{A}{HOR}{B}{VER}{C}");
        assert_eq!(dump(&s), "F[V[H[L13000.0.0.0,L13001.0.0.0],L13002.0.0.0]]");
        assert_eq!(rt(&s).unwrap(), s);
        // a horizontal group with a bracketed vertical needs the segment signs
        let s = format!("{A}{HOR}{BEG}{B}{VER}{C}{END}");
        assert_eq!(dump(&s), "F[H[L13000.0.0.0,V[L13001.0.0.0,L13002.0.0.0]]]");
        assert_eq!(rt(&s).unwrap(), s);
        // a bracketed vertical group alone is not a group
        assert!(parse(&format!("{BEG}{A}{VER}{B}{END}")).is_err());
    }

    #[test]
    fn overlay_and_insertion() {
        let s = format!("{A}{OVL}{B}");
        assert_eq!(dump(&s), "F[O[L13000.0.0.0][L13001.0.0.0]]");
        let s = format!("{BEG}{A}{HOR}{B}{END}{OVL}{C}");
        assert_eq!(dump(&s), "F[O[L13000.0.0.0,L13001.0.0.0][L13002.0.0.0]]");
        assert_eq!(rt(&s).unwrap(), s);
        let s = format!("{A}{TS}{B}{BS_}{C}");
        assert_eq!(dump(&s), "F[B[L13000.0.0.0|ts=L13001.0.0.0|bs=L13002.0.0.0]]");
        assert_eq!(rt(&s).unwrap(), s);
        // places must come in order
        assert!(parse(&format!("{A}{BS_}{B}{TS}{C}")).is_err());
        assert!(parse(&format!("{A}{TS}{B}{TS}{C}")).is_err());
        // a segment-wrapped insertion group inside an insertion
        let s = format!("{A}{TS}{BEG}{B}{TS}{C}{END}");
        assert_eq!(dump(&s), "F[B[L13000.0.0.0|ts=B[L13001.0.0.0|ts=L13002.0.0.0]]]");
        assert_eq!(rt(&s).unwrap(), s);
    }

    #[test]
    fn brackets_and_enclosures() {
        let s = format!("[{A}]");
        assert_eq!(dump(&s), "F[H[BO5b,L13000.0.0.0,BC5d]]");
        assert_eq!(rt(&s).unwrap(), s);
        let s = format!("{A}]");
        assert_eq!(dump(&s), "F[H[L13000.0.0.0,BC5d]]");
        let s = format!("\u{1343C}{A}\u{1343D}");
        assert_eq!(dump(&s), "F[Ep(-,0,-,0)[L13000.0.0.0]]");
        assert_eq!(rt(&s).unwrap(), s);
        // a walled opening closed by a plain closing is accepted and keeps the opening's type
        let s = format!("\u{1343E}{A}\u{1343D}");
        assert_eq!(dump(&s), "F[Ew(-,0,-,0)[L13000.0.0.0]]");
        assert_eq!(rt(&s).unwrap(), format!("\u{1343E}{A}\u{1343F}"));
    }

    #[test]
    fn rejects() {
        for s in [
            format!("{HOR}"),
            format!("{A}{HOR}"),
            format!("{BEG}{A}{END}"),
            format!("{BEG}{A}{HOR}{B}{END}"),
            format!("{A}{OVL}"),
            "\u{1343D}".to_string(),
            "\u{1343C}".to_string(),
            "\u{1343C}\u{13258}\u{1343D}".to_string(),
            "\u{13447}".to_string(),
            "x".to_string(),
        ] {
            assert!(parse(&s).is_err(), "{s:?}");
        }
        assert!(parse_utf16(&[0xD80C]).is_err());
        assert!(parse_utf16(&[0xDC00, 0xD80C]).is_err());
    }

    #[test]
    fn singleton() {
        let s = "\u{13258}\u{13447}";
        assert_eq!(dump(s), "F[S13258.1]");
        // singletons are only allowed at the top
        assert!(parse("\u{1343C}\u{13258}\u{1343D}").is_err());
    }

    fn nest_enclosures(n: usize) -> String {
        let mut s = String::new();
        for _ in 0..n {
            s.push('\u{1343C}');
        }
        s.push(A);
        for _ in 0..n {
            s.push('\u{1343D}');
        }
        s
    }

    #[test]
    fn depth_limit() {
        let ok = nest_enclosures(MAX_DEPTH as usize);
        let t = parse(&ok).unwrap();
        assert_eq!(t.to_string_value(), ok);
        let too_deep = nest_enclosures(MAX_DEPTH as usize + 1);
        assert_eq!(parse(&too_deep).unwrap_err().kind, ErrorKind::TooDeep);
        // Absurdly deep input fails cleanly.
        let huge = nest_enclosures(100_000);
        assert_eq!(parse(&huge).unwrap_err().kind, ErrorKind::TooDeep);
        // Nested segments count too.
        let mut s = String::new();
        for _ in 0..1000 {
            s.push(BEG);
        }
        assert_eq!(parse(&s).unwrap_err().kind, ErrorKind::TooDeep);
    }

    #[test]
    fn long_flat_input() {
        let one = format!("{A}{HOR}{B}{VER}{C}");
        let s = one.repeat(200_000);
        let t = parse(&s).unwrap();
        assert_eq!(t.to_string_value().len(), s.len());
        let many_hor = std::iter::repeat_n(A.to_string(), 200_000).collect::<Vec<_>>().join(&HOR.to_string());
        assert!(parse(&many_hor).is_ok());
    }
}
