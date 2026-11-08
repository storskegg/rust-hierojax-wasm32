//! Lexer for hierojax fragments.
//!
//! This follows the rules of `src/syntax.jison` (as generated into `src/syntax.js`)
//! one for one, in the same order, but by code point instead of by UTF-16 code unit.
//! The jison rules are regular expressions over surrogate pairs without the `u`
//! flag, and the generated lexer is not `flex`, so the first rule that matches wins.
//! The rule order is:
//!
//! 1. `BRACKET_OPEN`, `BRACKET_CLOSE`, `VS`
//! 2. `SIGN`
//! 3. the single control characters U+13430..U+13446 (except the enclosure
//!    controls U+1343C..U+1343F)
//! 4. `ENCLOSURE_OPENING`, `ENCLOSURE_CLOSING`, `DELIMITER`, `DAMAGED`
//!
//! The ranges are disjoint, with one exception that is longer than one character:
//! an opening character, optionally followed by a damage character, followed by a
//! begin-enclosure character matches both `ENCLOSURE_OPENING` and `DELIMITER` (the
//! opening character alone). `ENCLOSURE_OPENING` comes first in the rule list and
//! wins. Everything else, including a lone surrogate, matches no rule and is an
//! error, as in jison.

/// Number of insertion places, in the order of `Group.INSERTION_PLACES` upstream:
/// `ts`, `bs`, `te`, `be`, `m`, `t`, `b`.
pub const PLACES: usize = 7;

/// A token. The payload is what the grammar's actions read from `yytext`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tok {
    BracketOpen(char),
    BracketClose(char),
    /// Variation selector, 1..=7 (U+FE00..U+FE06).
    Vs(u8),
    Sign(char),
    Ver,
    Hor,
    /// One of the seven insertion controls; the number is the index into
    /// `ts, bs, te, be, m, t, b`.
    Insert(u8),
    Overlay,
    BeginSegment,
    EndSegment,
    Mirror,
    FullBlank,
    HalfBlank,
    FullLost,
    HalfLost,
    TallLost,
    WideLost,
    /// `[opening [damage]] begin-enclosure`.
    EnclosureOpening { delim: Option<char>, damage: u8, walled: bool },
    /// `end-enclosure [closing [damage]]`.
    EnclosureClosing { delim: Option<char>, damage: u8 },
    Delimiter(char),
    /// Damage, 1..=15 (U+13447..U+13455).
    Damaged(u8),
    Eof,
}

/// Lexer over UTF-16 code units.
pub struct Lexer<'a> {
    src: &'a [u16],
    pos: usize,
}

const DAMAGE_BASE: u32 = 0x13446;
const VARIATION_BASE: u32 = 0xFDFF;

fn is_opening(cp: u32) -> bool {
    matches!(cp, 0x13258 | 0x13259 | 0x1325A | 0x13286 | 0x13288 | 0x13379 | 0x1342F)
}

fn is_closing(cp: u32) -> bool {
    matches!(cp, 0x1325B | 0x1325C | 0x1325D | 0x13282 | 0x13287 | 0x13289 | 0x1337A | 0x1337B)
}

fn is_damaged(cp: u32) -> bool {
    (0x13447..=0x13455).contains(&cp)
}

fn is_begin_enclosure(cp: u32) -> bool {
    cp == 0x1343C || cp == 0x1343E
}

fn is_sign(cp: u32) -> bool {
    matches!(cp,
        0x13000..=0x13257
        | 0x1325E..=0x13281
        | 0x13283..=0x13285
        | 0x1328A..=0x13378
        | 0x1337C..=0x1342E
        | 0x13460..=0x143FA
        | 0xFFFD)
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a [u16]) -> Self {
        Lexer { src, pos: 0 }
    }

    /// Offset, in code units, of the next unread unit.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// The code point starting at unit `i` and its length in units. A lone
    /// surrogate (or a position at or beyond the end) gives `None`.
    fn cp_at(&self, i: usize) -> Option<(u32, usize)> {
        let u = u32::from(*self.src.get(i)?);
        if (0xD800..0xDC00).contains(&u) {
            let lo = u32::from(*self.src.get(i + 1)?);
            if (0xDC00..0xE000).contains(&lo) {
                Some((0x10000 + ((u - 0xD800) << 10) + (lo - 0xDC00), 2))
            } else {
                None
            }
        } else if (0xDC00..0xE000).contains(&u) {
            None
        } else {
            Some((u, 1))
        }
    }

    /// Read the next token. On failure returns the offset of the unit at which no
    /// rule matches.
    pub fn next_token(&mut self) -> Result<Tok, usize> {
        let start = self.pos;
        if start >= self.src.len() {
            return Ok(Tok::Eof);
        }
        let (cp, n) = self.cp_at(start).ok_or(start)?;
        let ch = char::from_u32(cp).ok_or(start)?;
        let mut len = n;
        let tok = match cp {
            0x5B | 0x7B | 0x27E6 | 0x27E8 | 0x2E22 => Tok::BracketOpen(ch),
            0x5D | 0x7D | 0x27E7 | 0x27E9 | 0x2E23 => Tok::BracketClose(ch),
            0xFE00..=0xFE06 => Tok::Vs((cp - VARIATION_BASE) as u8),
            _ if is_sign(cp) => Tok::Sign(ch),
            0x13430 => Tok::Ver,
            0x13431 => Tok::Hor,
            0x13432 => Tok::Insert(0),
            0x13433 => Tok::Insert(1),
            0x13434 => Tok::Insert(2),
            0x13435 => Tok::Insert(3),
            0x13436 => Tok::Overlay,
            0x13437 => Tok::BeginSegment,
            0x13438 => Tok::EndSegment,
            0x13439 => Tok::Insert(4),
            0x1343A => Tok::Insert(5),
            0x1343B => Tok::Insert(6),
            0x13440 => Tok::Mirror,
            0x13441 => Tok::FullBlank,
            0x13442 => Tok::HalfBlank,
            0x13443 => Tok::FullLost,
            0x13444 => Tok::HalfLost,
            0x13445 => Tok::TallLost,
            0x13446 => Tok::WideLost,
            _ if is_begin_enclosure(cp) => {
                Tok::EnclosureOpening { delim: None, damage: 0, walled: cp == 0x1343E }
            }
            0x1343D | 0x1343F => {
                // END_ENCLOSURE_CHAR ((CLOSING_CHAR)(DAMAGED_CHAR)?)?
                let mut delim = None;
                let mut damage = 0;
                if let Some((c1, n1)) = self.cp_at(start + len)
                    && is_closing(c1) {
                        delim = char::from_u32(c1);
                        len += n1;
                        if let Some((c2, n2)) = self.cp_at(start + len)
                            && is_damaged(c2) {
                                damage = (c2 - DAMAGE_BASE) as u8;
                                len += n2;
                            }
                    }
                Tok::EnclosureClosing { delim, damage }
            }
            _ if is_opening(cp) => {
                // (OPENING (DAMAGED)?)? BEGIN_ENCLOSURE, else the opening character alone.
                let mut tok = Tok::Delimiter(ch);
                if let Some((c1, n1)) = self.cp_at(start + len) {
                    if is_begin_enclosure(c1) {
                        tok = Tok::EnclosureOpening {
                            delim: Some(ch),
                            damage: 0,
                            walled: c1 == 0x1343E,
                        };
                        len += n1;
                    } else if is_damaged(c1)
                        && let Some((c2, n2)) = self.cp_at(start + len + n1)
                            && is_begin_enclosure(c2) {
                                tok = Tok::EnclosureOpening {
                                    delim: Some(ch),
                                    damage: (c1 - DAMAGE_BASE) as u8,
                                    walled: c2 == 0x1343E,
                                };
                                len += n1 + n2;
                            }
                }
                tok
            }
            _ if is_closing(cp) => Tok::Delimiter(ch),
            _ if is_damaged(cp) => Tok::Damaged((cp - DAMAGE_BASE) as u8),
            _ => return Err(start),
        };
        self.pos = start + len;
        Ok(tok)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(s: &str) -> Result<Vec<Tok>, usize> {
        let u: Vec<u16> = s.encode_utf16().collect();
        lex16(&u)
    }

    fn lex16(u: &[u16]) -> Result<Vec<Tok>, usize> {
        let mut l = Lexer::new(u);
        let mut v = Vec::new();
        loop {
            let t = l.next_token()?;
            if t == Tok::Eof {
                return Ok(v);
            }
            v.push(t);
        }
    }

    #[test]
    fn signs_and_controls() {
        assert_eq!(lex("\u{13000}\u{13430}\u{13431}").unwrap(),
            vec![Tok::Sign('\u{13000}'), Tok::Ver, Tok::Hor]);
        assert_eq!(lex("\u{FFFD}").unwrap(), vec![Tok::Sign('\u{FFFD}')]);
        assert_eq!(lex("").unwrap(), vec![]);
    }

    #[test]
    fn sign_range_edges() {
        for cp in [0x13257u32, 0x1325E, 0x13281, 0x13283, 0x13285, 0x1328A, 0x13378,
            0x1337C, 0x1342E, 0x13460, 0x143FA] {
            let c = char::from_u32(cp).unwrap();
            assert_eq!(lex(&c.to_string()).unwrap(), vec![Tok::Sign(c)], "{cp:x}");
        }
        for cp in [0x13258u32, 0x1325D, 0x13282, 0x13286, 0x13289, 0x13379, 0x1337B,
            0x1342F, 0x13456, 0x1345F, 0x143FB, 0x14400, 0x41, 0x1F600] {
            let c = char::from_u32(cp).unwrap();
            assert!(!matches!(lex(&c.to_string()), Ok(ref v) if v == &[Tok::Sign(c)]), "{cp:x}");
        }
    }

    #[test]
    fn lone_surrogates_and_junk() {
        assert_eq!(lex16(&[0xD80C]), Err(0));
        assert_eq!(lex16(&[0xDC00]), Err(0));
        assert_eq!(lex16(&[0xD80C, 0xD80C, 0xDC00]), Err(0));
        assert_eq!(lex16(&[0xD80C, 0xDC00, 0xDC00]), Err(2));
        assert_eq!(lex("a"), Err(0));
        assert_eq!(lex("\u{13000}a"), Err(2));
        // A valid pair that no rule covers.
        assert_eq!(lex("\u{1F600}"), Err(0));
    }

    #[test]
    fn variation_selectors_and_brackets() {
        assert_eq!(lex("\u{FE00}\u{FE06}").unwrap(), vec![Tok::Vs(1), Tok::Vs(7)]);
        assert!(lex("\u{FE07}").is_err());
        assert_eq!(lex("[{\u{27E6}\u{27E8}\u{2E22}").unwrap().len(), 5);
        assert_eq!(lex("]}\u{27E7}\u{27E9}\u{2E23}").unwrap().len(), 5);
        assert!(lex("(").is_err());
    }

    #[test]
    fn enclosure_tokens() {
        assert_eq!(lex("\u{1343C}").unwrap(),
            vec![Tok::EnclosureOpening { delim: None, damage: 0, walled: false }]);
        assert_eq!(lex("\u{13258}\u{1343E}").unwrap(),
            vec![Tok::EnclosureOpening { delim: Some('\u{13258}'), damage: 0, walled: true }]);
        assert_eq!(lex("\u{13258}\u{13449}\u{1343C}").unwrap(),
            vec![Tok::EnclosureOpening { delim: Some('\u{13258}'), damage: 3, walled: false }]);
        // Opening + damage, then something that is not begin-enclosure: two tokens.
        assert_eq!(lex("\u{13258}\u{13449}\u{13000}").unwrap(),
            vec![Tok::Delimiter('\u{13258}'), Tok::Damaged(3), Tok::Sign('\u{13000}')]);
        // Opening alone.
        assert_eq!(lex("\u{1342F}").unwrap(), vec![Tok::Delimiter('\u{1342F}')]);
        // Closing side.
        assert_eq!(lex("\u{1343D}").unwrap(),
            vec![Tok::EnclosureClosing { delim: None, damage: 0 }]);
        assert_eq!(lex("\u{1343F}\u{1325B}\u{13455}").unwrap(),
            vec![Tok::EnclosureClosing { delim: Some('\u{1325B}'), damage: 15 }]);
        // A damage char after end-enclosure without a closing char is a token of its own.
        assert_eq!(lex("\u{1343D}\u{13447}").unwrap(),
            vec![Tok::EnclosureClosing { delim: None, damage: 0 }, Tok::Damaged(1)]);
        // Closing char alone is a delimiter.
        assert_eq!(lex("\u{1325B}").unwrap(), vec![Tok::Delimiter('\u{1325B}')]);
        // Opening char before an end-enclosure is just a delimiter.
        assert_eq!(lex("\u{13258}\u{1343D}").unwrap().len(), 2);
    }

    #[test]
    fn unassigned_controls_are_errors() {
        for cp in 0x13456u32..=0x1345F {
            assert!(lex(&char::from_u32(cp).unwrap().to_string()).is_err());
        }
    }
}
