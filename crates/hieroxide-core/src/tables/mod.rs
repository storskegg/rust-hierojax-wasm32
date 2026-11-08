//! Static tables generated from upstream's data files by `tools/gen-tables.mjs`:
//! `Shapes.insertions`, `Shapes.ligatures`, `Shapes.rotations`, `Shapes.topupChars` and
//! `Shapes.bottomdownChars`. The generated files hold plain sorted arrays; the lookups
//! are binary searches.

mod insertions;
mod ligatures;
mod variants;

use crate::lexer::PLACES;

/// An insertion adjustment: the `{ x, y }` object of a place, either key optional.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Adj {
    pub x: Option<f64>,
    pub y: Option<f64>,
}

/// One alternative of `Shapes.insertions[ch]`. `places` is in the order of
/// `Group.INSERTION_PLACES` (`ts, bs, te, be, m, t, b`), `None` where the key is absent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Alt {
    /// `glyph`, the replacement glyph, if any.
    pub glyph: Option<char>,
    /// `rot`, 0 where absent (upstream reads `alt.rot ? alt.rot : 0`).
    pub rot: u16,
    /// `out`; carried for completeness, upstream's layout code never reads it.
    pub out: Option<&'static str>,
    pub places: [Option<Adj>; PLACES],
}

/// A sign of a ligature: `{ ch, x, y, w, h, mirror }`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sign {
    pub ch: char,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub mirror: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LigKind {
    Overlay,
    Vertical,
    Horizontal,
}

/// A ligature: `groups` for the vertical and horizontal kinds, `horizontal` and
/// `vertical` for overlays; the lists that do not apply are empty.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ligature {
    pub ch: char,
    pub kind: LigKind,
    pub alt: bool,
    pub groups: &'static [Sign],
    pub horizontal: &'static [Sign],
    pub vertical: &'static [Sign],
}

/// `Shapes.insertions[ch]`, or `None` where `ch in Shapes.insertions` is false.
pub fn insertions(ch: char) -> Option<&'static [Alt]> {
    let t = insertions::INSERTIONS;
    t.binary_search_by(|e| e.0.cmp(&ch)).ok().and_then(|i| t.get(i)).map(|e| e.1)
}

/// `Shapes.topupChars.has(ch)`.
pub fn is_topup(ch: char) -> bool {
    insertions::TOPUP_CHARS.binary_search(&ch).is_ok()
}

/// `Shapes.bottomdownChars.has(ch)`.
pub fn is_bottomdown(ch: char) -> bool {
    insertions::BOTTOMDOWN_CHARS.binary_search(&ch).is_ok()
}

/// All ligatures in the order of upstream's `for (const ch in Shapes.ligatures)`.
pub fn ligatures() -> &'static [Ligature] {
    ligatures::LIGATURES
}

/// `Shapes.ligatures[ch]`.
pub fn ligature(ch: char) -> Option<&'static Ligature> {
    let t = ligatures::LIGATURE_INDEX;
    let i = t.binary_search_by(|e| e.0.cmp(&ch)).ok()?;
    let pos = t.get(i)?.1;
    ligatures::LIGATURES.get(usize::from(pos))
}

/// `Shapes.rotations[ch]`: (coarse rotation, adjustment) pairs in `Object.keys` order.
pub fn rotations(ch: char) -> Option<&'static [(u16, i16)]> {
    let t = variants::ROTATIONS;
    t.binary_search_by(|e| e.0.cmp(&ch)).ok().and_then(|i| t.get(i)).map(|e| e.1)
}

/// Every character of `Shapes.rotations`, for the freshness test.
pub fn rotation_chars() -> impl Iterator<Item = char> {
    variants::ROTATIONS.iter().map(|e| e.0)
}

/// Every character of `Shapes.insertions`.
pub fn insertion_chars() -> impl Iterator<Item = char> {
    insertions::INSERTIONS.iter().map(|e| e.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookups() {
        assert_eq!(rotations('\u{13012}'), Some(&[(45u16, -15i16)][..]));
        assert_eq!(rotations('\u{13000}'), None);
        assert!(insertions('\u{13000}').is_some());
        assert!(insertions('\u{13001}').is_none());
        assert!(is_topup('\u{13093}'));
        assert!(!is_topup('\u{13098}'));
        assert!(is_bottomdown('\u{13098}'));
        let lig = ligature('\u{130C1}');
        assert_eq!(lig.map(|l| l.kind), Some(LigKind::Overlay));
        assert!(ligature('\u{13000}').is_none());
    }

    /// The committed tables are what `tools/gen-tables.mjs` makes of upstream's data files.
    /// Needs node (`NODE` names it); `cargo test -- --ignored tables_are_current`.
    #[test]
    #[ignore = "needs node"]
    fn tables_are_current() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let node = std::env::var("NODE").unwrap_or_else(|_| "node".to_string());
        let out = std::process::Command::new(node)
            .arg("tools/gen-tables.mjs")
            .arg("--check")
            .current_dir(root)
            .output()
            .expect("run node");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    }

    #[test]
    fn sorted_for_binary_search() {
        let mut prev = None;
        for c in rotation_chars() {
            assert!(prev < Some(c));
            prev = Some(c);
        }
        let mut prev = None;
        for c in insertion_chars() {
            assert!(prev < Some(c));
            prev = Some(c);
        }
        for l in ligatures() {
            assert_eq!(ligature(l.ch).map(|x| x.ch), Some(l.ch));
        }
    }
}
