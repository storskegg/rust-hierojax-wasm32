//! What the raster reads of an OpenType font with CFF outlines: the `cmap` (formats 4 and 12),
//! advance widths, units per em, and the Type 2 charstrings. It reads only what NewGardiner needs,
//! at a fraction of the size of a general parser: the test `same_as_ttf_parser` holds it to
//! `ttf-parser`, glyph by glyph, over the whole font. Every read is checked, so bad data gives
//! `None` or an empty outline, never a panic.

/// One path command, in font units with y upward.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    Move(f32, f32),
    Line(f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

fn be16(d: &[u8], at: usize) -> Option<u16> {
    let b = d.get(at..at.checked_add(2)?)?;
    Some(u16::from_be_bytes([b[0], b[1]]))
}

fn be32(d: &[u8], at: usize) -> Option<u32> {
    let b = d.get(at..at.checked_add(4)?)?;
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// A CFF INDEX: `count` objects, whose data follows the offsets.
#[derive(Clone, Copy, Default)]
struct Index<'a> {
    data: &'a [u8],
    count: usize,
    off_size: usize,
    /// Where the offset array starts.
    offsets: usize,
    /// One before the data, as CFF's offsets count from there.
    base: usize,
    /// The end of the INDEX.
    end: usize,
}

impl<'a> Index<'a> {
    fn parse(data: &'a [u8], at: usize) -> Option<Index<'a>> {
        let count = usize::from(be16(data, at)?);
        if count == 0 {
            return Some(Index { data, end: at + 2, ..Index::default() });
        }
        let off_size = usize::from(*data.get(at + 2)?);
        if !(1..=4).contains(&off_size) {
            return None;
        }
        let offsets = at + 3;
        let base = offsets.checked_add((count + 1).checked_mul(off_size)?)?.checked_sub(1)?;
        let mut idx = Index { data, count, off_size, offsets, base, end: 0 };
        idx.end = base.checked_add(idx.offset(count)?)?;
        Some(idx)
    }

    fn offset(&self, i: usize) -> Option<usize> {
        let at = self.offsets.checked_add(i.checked_mul(self.off_size)?)?;
        let b = self.data.get(at..at.checked_add(self.off_size)?)?;
        Some(b.iter().fold(0usize, |v, x| (v << 8) | usize::from(*x)))
    }

    fn get(&self, i: usize) -> Option<&'a [u8]> {
        if i >= self.count {
            return None;
        }
        let (a, b) = (self.base.checked_add(self.offset(i)?)?, self.base.checked_add(self.offset(i + 1)?)?);
        self.data.get(a..b)
    }
}

/// The operand of `op` in a DICT, as integers; only the integer operators are of use here.
fn dict_ints(d: &[u8], want: u16) -> Option<Vec<i32>> {
    let mut args: Vec<i32> = Vec::new();
    let mut i = 0;
    while i < d.len() {
        let b = d[i];
        match b {
            0..=21 => {
                let op = if b == 12 {
                    i += 1;
                    0x0c00 | u16::from(*d.get(i)?)
                } else {
                    u16::from(b)
                };
                i += 1;
                if op == want {
                    return Some(args);
                }
                args.clear();
            }
            28 => {
                args.push(i32::from(be16(d, i + 1)? as i16));
                i += 3;
            }
            29 => {
                args.push(be32(d, i + 1)? as i32);
                i += 5;
            }
            30 => {
                // A real number: packed decimal, which no operator read here takes.
                i += 1;
                loop {
                    let n = *d.get(i)?;
                    i += 1;
                    if n & 0x0f == 0x0f || n >> 4 == 0x0f {
                        break;
                    }
                }
                args.push(0);
            }
            32..=246 => {
                args.push(i32::from(b) - 139);
                i += 1;
            }
            247..=250 => {
                args.push((i32::from(b) - 247) * 256 + i32::from(*d.get(i + 1)?) + 108);
                i += 2;
            }
            251..=254 => {
                args.push(-(i32::from(b) - 251) * 256 - i32::from(*d.get(i + 1)?) - 108);
                i += 2;
            }
            _ => return None,
        }
    }
    None
}

/// The tables the raster reads.
pub struct Font<'a> {
    pub units_per_em: u16,
    cmap: &'a [u8],
    /// Where each `cmap` subtable worth reading starts, best first.
    subtables: Vec<usize>,
    hmtx: &'a [u8],
    n_hmetrics: usize,
    charstrings: Index<'a>,
    global_subrs: Index<'a>,
    local_subrs: Index<'a>,
}

impl<'a> Font<'a> {
    /// `None` when something the raster needs is missing or malformed.
    pub fn parse(data: &'a [u8]) -> Option<Font<'a>> {
        if data.get(0..4)? != b"OTTO" {
            return None;
        }
        let n = usize::from(be16(data, 4)?);
        let table = |tag: &[u8; 4]| -> Option<&'a [u8]> {
            for i in 0..n {
                let rec = 12 + i * 16;
                if data.get(rec..rec + 4)? == tag {
                    let off = be32(data, rec + 8)? as usize;
                    let len = be32(data, rec + 12)? as usize;
                    return data.get(off..off.checked_add(len)?);
                }
            }
            None
        };
        let units_per_em = be16(table(b"head")?, 18).filter(|u| *u > 0)?;
        let n_hmetrics = usize::from(be16(table(b"hhea")?, 34)?);
        let hmtx = table(b"hmtx")?;
        let cmap = table(b"cmap")?;
        let mut subtables: Vec<(u8, usize)> = Vec::new();
        for i in 0..usize::from(be16(cmap, 2)?) {
            let rec = 4 + i * 8;
            let (platform, encoding) = (be16(cmap, rec)?, be16(cmap, rec + 2)?);
            let off = be32(cmap, rec + 4)? as usize;
            let format = be16(cmap, off)?;
            let unicode = matches!((platform, encoding), (0, _) | (3, 1) | (3, 10));
            if unicode && (format == 4 || format == 12) {
                // Format 12 reaches the signs, which are outside the BMP.
                subtables.push((u8::from(format != 12), off));
            }
        }
        subtables.sort();
        let subtables = subtables.into_iter().map(|s| s.1).collect();

        let cff = table(b"CFF ")?;
        let name = Index::parse(cff, usize::from(*cff.get(2)?))?;
        let top_index = Index::parse(cff, name.end)?;
        let top = top_index.get(0)?;
        let strings = Index::parse(cff, top_index.end)?;
        let global_subrs = Index::parse(cff, strings.end)?;
        // CID-keyed fonts would need FDSelect; this font is not one, and none other is promised.
        if dict_ints(top, 0x0c1e).is_some() {
            return None;
        }
        let charstrings = Index::parse(cff, usize::try_from(*dict_ints(top, 17)?.first()?).ok()?)?;
        let mut local_subrs = Index::default();
        if let Some(p) = dict_ints(top, 18)
            && let [size, off] = p[..]
        {
            let (size, off) = (usize::try_from(size).ok()?, usize::try_from(off).ok()?);
            let private = cff.get(off..off.checked_add(size)?)?;
            if let Some(s) = dict_ints(private, 19)
                && let Some(rel) = s.first()
            {
                local_subrs = Index::parse(cff, off.checked_add(usize::try_from(*rel).ok()?)?)?;
            }
        }
        Some(Font { units_per_em, cmap, subtables, hmtx, n_hmetrics, charstrings, global_subrs, local_subrs })
    }

    /// The glyph of a character, or `None` when the font has none.
    pub fn glyph_index(&self, ch: char) -> Option<u16> {
        let cp = ch as u32;
        for &off in &self.subtables {
            let found = match be16(self.cmap, off)? {
                12 => self.cmap12(off, cp),
                4 => self.cmap4(off, cp),
                _ => None,
            };
            if let Some(g) = found.filter(|g| *g != 0) {
                return Some(g);
            }
        }
        None
    }

    fn cmap12(&self, off: usize, cp: u32) -> Option<u16> {
        let n = be32(self.cmap, off + 12)? as usize;
        let (mut lo, mut hi) = (0usize, n);
        while lo < hi {
            let mid = (lo + hi) / 2;
            let g = off + 16 + mid * 12;
            let (start, end) = (be32(self.cmap, g)?, be32(self.cmap, g + 4)?);
            if cp < start {
                hi = mid;
            } else if cp > end {
                lo = mid + 1;
            } else {
                return u16::try_from(be32(self.cmap, g + 8)?.checked_add(cp - start)?).ok();
            }
        }
        None
    }

    fn cmap4(&self, off: usize, cp: u32) -> Option<u16> {
        let cp = u16::try_from(cp).ok()?;
        let seg_x2 = usize::from(be16(self.cmap, off + 6)?);
        let (ends, starts) = (off + 14, off + 16 + seg_x2);
        let (deltas, ranges) = (starts + seg_x2, starts + 2 * seg_x2);
        for i in (0..seg_x2).step_by(2) {
            if cp > be16(self.cmap, ends + i)? {
                continue;
            }
            let start = be16(self.cmap, starts + i)?;
            if cp < start {
                return None;
            }
            let delta = be16(self.cmap, deltas + i)?;
            let range = usize::from(be16(self.cmap, ranges + i)?);
            if range == 0 {
                return Some(cp.wrapping_add(delta));
            }
            let g = be16(self.cmap, ranges + i + range + usize::from(cp - start) * 2)?;
            return if g == 0 { None } else { Some(g.wrapping_add(delta)) };
        }
        None
    }

    /// The advance width in font units.
    pub fn advance(&self, gid: u16) -> u16 {
        if self.n_hmetrics == 0 {
            return 0;
        }
        let i = usize::from(gid).min(self.n_hmetrics - 1);
        be16(self.hmtx, i * 4).unwrap_or(0)
    }

    /// The outline of a glyph; empty for one without ink or with a malformed charstring.
    pub fn outline(&self, gid: u16) -> Vec<Seg> {
        let mut c = Charstring { font: self, out: Vec::new(), stack: [0.0; 48], sp: 0, x: 0.0, y: 0.0, open: false, stems: 0, width_done: false };
        let done = self.charstrings.get(usize::from(gid)).and_then(|cs| c.run(cs, 0));
        if done.is_none() {
            return Vec::new();
        }
        c.close();
        c.out
    }
}

/// Nesting of subroutines the Type 2 format allows.
const MAX_DEPTH: u8 = 10;

struct Charstring<'f, 'a> {
    font: &'f Font<'a>,
    out: Vec<Seg>,
    stack: [f32; 48],
    sp: usize,
    x: f32,
    y: f32,
    open: bool,
    stems: usize,
    width_done: bool,
}

fn subr_bias(count: usize) -> i32 {
    if count < 1240 {
        107
    } else if count < 33900 {
        1131
    } else {
        32768
    }
}

impl Charstring<'_, '_> {
    fn close(&mut self) {
        if self.open {
            self.out.push(Seg::Close);
            self.open = false;
        }
    }

    fn move_to(&mut self, dx: f32, dy: f32) {
        self.close();
        self.x += dx;
        self.y += dy;
        self.out.push(Seg::Move(self.x, self.y));
        self.open = true;
    }

    fn line(&mut self, dx: f32, dy: f32) {
        self.x += dx;
        self.y += dy;
        self.out.push(Seg::Line(self.x, self.y));
    }

    fn curve(&mut self, d: [f32; 6]) {
        let (x1, y1) = (self.x + d[0], self.y + d[1]);
        let (x2, y2) = (x1 + d[2], y1 + d[3]);
        self.x = x2 + d[4];
        self.y = y2 + d[5];
        self.out.push(Seg::Cubic(x1, y1, x2, y2, self.x, self.y));
    }

    /// The optional width comes first, before the first stem or move: drop it. `even`: the operator
    /// takes an even number of arguments; otherwise it takes `n`.
    fn take_width(&mut self, even: bool, n: usize) {
        if !self.width_done {
            self.width_done = true;
            let has = if even { self.sp % 2 == 1 } else { self.sp > n };
            if has {
                self.stack.copy_within(1..self.sp, 0);
                self.sp -= 1;
            }
        }
    }

    /// `None` on malformed data; `Some(true)` when the glyph ended.
    fn run(&mut self, cs: &[u8], depth: u8) -> Option<bool> {
        if depth > MAX_DEPTH {
            return None;
        }
        let mut i = 0;
        while i < cs.len() {
            let b = cs[i];
            i += 1;
            match b {
                1 | 3 | 18 | 23 => {
                    self.take_width(true, 0);
                    self.stems += self.sp / 2;
                    self.sp = 0;
                }
                19 | 20 => {
                    self.take_width(true, 0);
                    self.stems += self.sp / 2;
                    self.sp = 0;
                    i += self.stems.div_ceil(8);
                }
                21 => {
                    self.take_width(false, 2);
                    let (dx, dy) = (*self.stack.get(self.sp.checked_sub(2)?)?, *self.stack.get(self.sp - 1)?);
                    self.move_to(dx, dy);
                    self.sp = 0;
                }
                22 => {
                    self.take_width(false, 1);
                    let dx = *self.stack.get(self.sp.checked_sub(1)?)?;
                    self.move_to(dx, 0.0);
                    self.sp = 0;
                }
                4 => {
                    self.take_width(false, 1);
                    let dy = *self.stack.get(self.sp.checked_sub(1)?)?;
                    self.move_to(0.0, dy);
                    self.sp = 0;
                }
                5 => {
                    let s = self.stack;
                    for p in s[..self.sp].chunks_exact(2) {
                        self.line(p[0], p[1]);
                    }
                    self.sp = 0;
                }
                6 | 7 => {
                    let s = self.stack;
                    let mut horizontal = b == 6;
                    for v in &s[..self.sp] {
                        if horizontal {
                            self.line(*v, 0.0);
                        } else {
                            self.line(0.0, *v);
                        }
                        horizontal = !horizontal;
                    }
                    self.sp = 0;
                }
                8 => {
                    let s = self.stack;
                    for p in s[..self.sp].chunks_exact(6) {
                        self.curve([p[0], p[1], p[2], p[3], p[4], p[5]]);
                    }
                    self.sp = 0;
                }
                24 => {
                    // rcurveline
                    let s = self.stack;
                    let n = self.sp.checked_sub(2)?;
                    for p in s[..n].chunks_exact(6) {
                        self.curve([p[0], p[1], p[2], p[3], p[4], p[5]]);
                    }
                    self.line(s[n], s[n + 1]);
                    self.sp = 0;
                }
                25 => {
                    // rlinecurve
                    let s = self.stack;
                    let n = self.sp.checked_sub(6)?;
                    for p in s[..n].chunks_exact(2) {
                        self.line(p[0], p[1]);
                    }
                    let p = &s[n..n + 6];
                    self.curve([p[0], p[1], p[2], p[3], p[4], p[5]]);
                    self.sp = 0;
                }
                26 => {
                    // vvcurveto
                    let s = self.stack;
                    let mut a = &s[..self.sp];
                    let mut dx1 = 0.0;
                    if a.len() % 2 == 1 {
                        dx1 = a[0];
                        a = &a[1..];
                    }
                    for p in a.chunks_exact(4) {
                        self.curve([dx1, p[0], p[1], p[2], 0.0, p[3]]);
                        dx1 = 0.0;
                    }
                    self.sp = 0;
                }
                27 => {
                    // hhcurveto
                    let s = self.stack;
                    let mut a = &s[..self.sp];
                    let mut dy1 = 0.0;
                    if a.len() % 2 == 1 {
                        dy1 = a[0];
                        a = &a[1..];
                    }
                    for p in a.chunks_exact(4) {
                        self.curve([p[0], dy1, p[1], p[2], p[3], 0.0]);
                        dy1 = 0.0;
                    }
                    self.sp = 0;
                }
                30 | 31 => {
                    // vhcurveto, hvcurveto
                    let s = self.stack;
                    let mut a = &s[..self.sp];
                    let mut horizontal = b == 31;
                    while a.len() >= 4 {
                        let last = if a.len() == 5 { a[4] } else { 0.0 };
                        if horizontal {
                            self.curve([a[0], 0.0, a[1], a[2], last, a[3]]);
                        } else {
                            self.curve([0.0, a[0], a[1], a[2], a[3], last]);
                        }
                        horizontal = !horizontal;
                        a = &a[4..];
                    }
                    self.sp = 0;
                }
                10 | 29 => {
                    let index = if b == 10 { self.font.local_subrs } else { self.font.global_subrs };
                    self.sp = self.sp.checked_sub(1)?;
                    let n = self.stack[self.sp] as i32 + subr_bias(index.count);
                    let sub = index.get(usize::try_from(n).ok()?)?;
                    if self.run(sub, depth + 1)? {
                        return Some(true);
                    }
                }
                11 => return Some(false),
                14 => {
                    self.take_width(false, 0);
                    return Some(true);
                }
                12 => {
                    let op = *cs.get(i)?;
                    i += 1;
                    let s = self.stack;
                    match op {
                        34 if self.sp == 7 => {
                            // hflex
                            self.curve([s[0], 0.0, s[1], s[2], s[3], 0.0]);
                            self.curve([s[4], 0.0, s[5], -s[2], s[6], 0.0]);
                        }
                        35 if self.sp == 13 => {
                            // flex
                            self.curve([s[0], s[1], s[2], s[3], s[4], s[5]]);
                            self.curve([s[6], s[7], s[8], s[9], s[10], s[11]]);
                        }
                        36 if self.sp == 9 => {
                            // hflex1
                            self.curve([s[0], s[1], s[2], s[3], s[4], 0.0]);
                            self.curve([s[5], 0.0, s[6], s[7], s[8], -(s[1] + s[3] + s[7])]);
                        }
                        37 if self.sp == 11 => {
                            // flex1
                            let dx = s[0] + s[2] + s[4] + s[6] + s[8];
                            let dy = s[1] + s[3] + s[5] + s[7] + s[9];
                            let (dx6, dy6) = if dx.abs() > dy.abs() { (s[10], -dy) } else { (-dx, s[10]) };
                            self.curve([s[0], s[1], s[2], s[3], s[4], s[5]]);
                            self.curve([s[6], s[7], s[8], s[9], dx6, dy6]);
                        }
                        _ => return None,
                    }
                    self.sp = 0;
                }
                28 => {
                    self.push(f32::from(be16(cs, i)? as i16))?;
                    i += 2;
                }
                32..=246 => self.push(f32::from(i16::from(b) - 139))?,
                247..=250 => {
                    let v = (i16::from(b) - 247) * 256 + i16::from(*cs.get(i)?) + 108;
                    i += 1;
                    self.push(f32::from(v))?;
                }
                251..=254 => {
                    let v = -(i16::from(b) - 251) * 256 - i16::from(*cs.get(i)?) - 108;
                    i += 1;
                    self.push(f32::from(v))?;
                }
                255 => {
                    let v = be32(cs, i)? as i32;
                    i += 4;
                    self.push(v as f32 / 65536.0)?;
                }
                _ => return None,
            }
        }
        Some(false)
    }

    fn push(&mut self, v: f32) -> Option<()> {
        *self.stack.get_mut(self.sp)? = v;
        self.sp += 1;
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct B(Vec<Seg>);
    impl ttf_parser::OutlineBuilder for B {
        fn move_to(&mut self, x: f32, y: f32) {
            self.0.push(Seg::Move(x, y));
        }
        fn line_to(&mut self, x: f32, y: f32) {
            self.0.push(Seg::Line(x, y));
        }
        fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {
            panic!("a quadratic in a CFF font");
        }
        fn curve_to(&mut self, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) {
            self.0.push(Seg::Cubic(a, b, c, d, e, f));
        }
        fn close(&mut self) {
            self.0.push(Seg::Close);
        }
    }

    /// Every glyph, every character of the font, every advance.
    #[test]
    fn same_as_ttf_parser() {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/NewGardiner.otf");
        let data = std::fs::read(p).expect("docs/NewGardiner.otf");
        let face = ttf_parser::Face::parse(&data, 0).expect("ttf-parser");
        let font = Font::parse(&data).expect("font");
        assert_eq!(font.units_per_em, face.units_per_em());
        let mut outlines = 0;
        for g in 0..face.number_of_glyphs() {
            let mut b = B(Vec::new());
            let _ = face.outline_glyph(ttf_parser::GlyphId(g), &mut b);
            assert_eq!(font.outline(g), b.0, "glyph {g}");
            assert_eq!(Some(font.advance(g)), face.glyph_hor_advance(ttf_parser::GlyphId(g)), "advance of {g}");
            outlines += usize::from(!b.0.is_empty());
        }
        assert!(outlines > 4000, "{outlines}");
        let mut mapped = 0;
        for cp in 0..0x11_0000u32 {
            if let Some(ch) = char::from_u32(cp) {
                let theirs = face.glyph_index(ch).map(|g| g.0).filter(|g| *g != 0);
                assert_eq!(font.glyph_index(ch), theirs, "U+{cp:X}");
                mapped += usize::from(theirs.is_some());
            }
        }
        assert!(mapped > 4000, "{mapped}");
    }

    #[test]
    fn bad_data_is_no_font() {
        assert!(Font::parse(b"").is_none());
        assert!(Font::parse(b"OTTO").is_none());
        assert!(Font::parse(&[0x4f, 0x54, 0x54, 0x4f, 0, 200, 0, 0, 0, 0, 0, 0]).is_none());
    }

    /// Cut and flipped copies of the font, and every glyph of each: no panic, whatever comes back.
    #[test]
    fn damaged_font_does_not_panic() {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/NewGardiner.otf");
        let data = std::fs::read(p).expect("docs/NewGardiner.otf");
        for cut in [100, 1000, data.len() / 2] {
            if let Some(f) = Font::parse(&data[..cut]) {
                let _ = f.outline(5);
            }
        }
        let mut s: u64 = 1;
        for _ in 0..8 {
            let mut d = data.clone();
            for _ in 0..200 {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let at = (s >> 33) as usize % d.len();
                d[at] = (s >> 20) as u8;
            }
            if let Some(f) = Font::parse(&d) {
                for g in (0..5000u16).step_by(37) {
                    let _ = (f.outline(g), f.advance(g));
                }
                let _ = f.glyph_index('\u{13000}');
            }
        }
    }
}
