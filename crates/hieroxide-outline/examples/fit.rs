//! The tool the constants of `Tuning::FROZEN` were fitted with. Two commands:
//!
//!   fit record <out.txt>    lay out the corpus and write every distinct single-glyph draw
//!   fit score <draws.txt> <truth.bin> [name=value;...]...
//!                           pixels of each draw against what a browser drew (see
//!                           tests/golden/replay.mjs), per kind of draw, for each tuning
//!
//! `tests/golden/README.md` has the whole procedure.

use std::collections::HashSet;
use std::io::Write;

use hieroxide_core::layout::{self, Bitmap, DrawCmd, Engine, NoIds, Raster, RasterError, RawOptions};
use hieroxide_outline::{OutlineRaster, Tuning};

#[path = "../../../tests/golden/common.rs"]
mod common;

struct Recorder<'a, 'f> {
    inner: OutlineRaster<'f>,
    seen: &'a mut HashSet<String>,
    out: &'a mut dyn Write,
}

impl Raster for Recorder<'_, '_> {
    fn advance(&mut self, ch: char, font_size: f64) -> Result<f64, RasterError> {
        self.inner.advance(ch, font_size)
    }
    fn draw(&mut self, w: f64, h: f64, cmds: &[DrawCmd]) -> Result<Bitmap, RasterError> {
        for c in cmds {
            let l = format!(
                "{} {} {} {} {} {} {} {} {} {} {}",
                w, h, c.ch as u32, c.font_size, c.tx, c.ty, c.sx, c.sy, c.rotate, c.x, c.y
            );
            if self.seen.insert(l.clone()) {
                writeln!(self.out, "{l}").expect("write");
            }
        }
        self.inner.draw(w, h, cmds)
    }
    fn pow(&mut self, b: f64, e: f64) -> f64 {
        self.inner.pow(b, e)
    }
    fn log(&mut self, x: f64) -> f64 {
        self.inner.log(x)
    }
}

fn record(out_path: &str) {
    let corpus = common::load_corpus(None);
    let font = common::font();
    let mut out = std::io::BufWriter::new(std::fs::File::create(out_path).expect("create"));
    let mut seen = HashSet::new();
    for chunk in corpus.chunks(200) {
        let mut raster = Recorder { inner: OutlineRaster::new(&font).expect("font"), seen: &mut seen, out: &mut out };
        let mut engine = Engine::new();
        for f in chunk {
            let raw = RawOptions::from_parts(f.font_px, &f.color, &f.attrs);
            if let Ok(tree) = hieroxide_core::parse(&f.text) {
                let _ = layout::render_svg(&mut engine, &tree, &raw, &mut raster, &mut NoIds, true);
            }
        }
    }
    eprintln!("{} distinct draws", seen.len());
}

pub fn parse_tuning(s: &str) -> Tuning {
    let mut t = Tuning::FROZEN;
    for kv in s.split(';').filter(|x| !x.is_empty()) {
        let (k, v) = kv.split_once('=').expect("name=value");
        let n: Vec<f64> = v.split(',').map(|x| x.parse().expect("number")).collect();
        t.set(k, &n);
    }
    t
}

#[derive(Clone)]
struct Draw {
    w: f64,
    h: f64,
    cmd: DrawCmd,
    kind: &'static str,
    /// bbox x, y, w, h and the packed dark bits in it, from the browser
    bbox: [u32; 4],
    bits: Vec<u8>,
}

fn kind(c: &DrawCmd) -> &'static str {
    let r = (c.rotate * 180.0 / core::f64::consts::PI).round() as i64;
    let rs = r.rem_euclid(360);
    let unit = c.sx.abs() == 1.0 && c.sy == 1.0;
    let meas = c.font_size == 150.0;
    match (rs, unit, meas) {
        (0, true, true) => {
            if c.sx < 0.0 { "m150 mirror" } else { "m150 plain" }
        }
        (0, true, false) => {
            if c.sx < 0.0 { "g mirror" } else { "g plain" }
        }
        (0, false, true) => "m150 scaled",
        (0, false, false) => "g scaled",
        (180, _, _) => "rot180",
        (90, _, true) => "m150 rot90",
        (270, _, true) => "m150 rot270",
        (90, _, false) => "g rot90",
        (270, _, false) => "g rot270",
        (_, _, _) => "rot other",
    }
}

fn load(draws: &str, truth: &str) -> Vec<Draw> {
    let text = std::fs::read_to_string(draws).expect("draws");
    let t = std::fs::read(truth).expect("truth");
    let mut p = 0usize;
    let u32at = |p: usize| u32::from_le_bytes([t[p], t[p + 1], t[p + 2], t[p + 3]]);
    let mut out = Vec::new();
    for l in text.lines() {
        let n: Vec<f64> = l.split(' ').map(|x| x.parse().expect("num")).collect();
        let bbox = [u32at(p), u32at(p + 4), u32at(p + 8), u32at(p + 12)];
        p += 16;
        let nbytes = ((bbox[2] as usize * bbox[3] as usize) + 7) / 8;
        let bits = t[p..p + nbytes].to_vec();
        p += nbytes;
        let cmd = DrawCmd {
            ch: char::from_u32(n[2] as u32).expect("char"),
            font_size: n[3],
            tx: n[4],
            ty: n[5],
            sx: n[6],
            sy: n[7],
            rotate: n[8],
            x: n[9],
            y: n[10],
        };
        out.push(Draw { w: n[0], h: n[1], cmd, kind: kind(&cmd), bbox, bits });
    }
    assert_eq!(p, t.len(), "truth does not fit the draws");
    out
}

fn score(draws: &[Draw], font: &[u8], tuning: Tuning, label: &str) {
    let mut r = OutlineRaster::with_tuning(font, tuning).expect("font").lenient();
    let mut stats: std::collections::BTreeMap<&str, [u64; 4]> = Default::default();
    for d in draws {
        let b = r.draw(d.w, d.h, std::slice::from_ref(&d.cmd)).expect("draw");
        let (w, h) = (b.w as usize, b.h as usize);
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
        for y in 0..h {
            for x in 0..w {
                if b.data[(y * w + x) * 4 + 3] != 0 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + 1);
                    y1 = y1.max(y + 1);
                }
            }
        }
        let ours = if x1 > x0 { [x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32] } else { [0, 0, 0, 0] };
        let same_box = ours == d.bbox;
        let mut same_mask = same_box;
        if same_box {
            let [bx, by, bw, bh] = d.bbox.map(|v| v as usize);
            'o: for yy in 0..bh {
                for xx in 0..bw {
                    let i = yy * bw + xx;
                    let truth = d.bits[i / 8] >> (i % 8) & 1 != 0;
                    if (b.data[((yy + by) * w + xx + bx) * 4 + 3] != 0) != truth {
                        same_mask = false;
                        break 'o;
                    }
                }
            }
        }
        for k in [d.kind, "ALL"] {
            let e = stats.entry(k).or_default();
            e[0] += 1;
            e[1] += u64::from(same_box);
            e[2] += u64::from(same_mask);
        }
    }
    println!("== {label}");
    for (k, s) in &stats {
        println!(
            "  {k:14} {:7} box {:7} {:6.2}%  mask {:7} {:6.2}%",
            s[0],
            s[1],
            100.0 * s[1] as f64 / s[0] as f64,
            s[2],
            100.0 * s[2] as f64 / s[0] as f64
        );
    }
}

/// Where the boxes differ: per kind and font-size band, the histogram of each edge's difference,
/// and how many pixels differ each way.
fn edges(draws: &[Draw], font: &[u8], tuning: Tuning) {
    let mut r = OutlineRaster::with_tuning(font, tuning).expect("font").lenient();
    #[derive(Default)]
    struct S {
        n: u64,
        e: [std::collections::BTreeMap<i64, u64>; 4],
        extra: u64,
        missing: u64,
        ink: u64,
    }
    let mut stats: std::collections::BTreeMap<String, S> = Default::default();
    for d in draws {
        let b = r.draw(d.w, d.h, std::slice::from_ref(&d.cmd)).expect("draw");
        let (w, h) = (b.w as usize, b.h as usize);
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
        let [bx, by, bw, bh] = d.bbox.map(|v| v as usize);
        let truth = |x: usize, y: usize| -> bool {
            if x < bx || y < by || x >= bx + bw || y >= by + bh {
                return false;
            }
            let i = (y - by) * bw + x - bx;
            d.bits[i / 8] >> (i % 8) & 1 != 0
        };
        let (mut extra, mut missing, mut ink) = (0, 0, 0);
        for y in 0..h {
            for x in 0..w {
                let o = b.data[(y * w + x) * 4 + 3] != 0;
                let t = truth(x, y);
                if o {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + 1);
                    y1 = y1.max(y + 1);
                }
                if t {
                    ink += 1;
                }
                if o && !t {
                    extra += 1;
                }
                if t && !o {
                    missing += 1;
                }
            }
        }
        if x1 <= x0 || bw == 0 {
            continue;
        }
        let size = d.cmd.font_size;
        let band = if size < 15.0 { "<15" } else if size < 25.0 { "<25" } else if size < 40.0 { "<40" } else if size <= 50.0 { "<=50" } else { ">50" };
        let s = stats.entry(format!("{} {}", d.kind, band)).or_default();
        s.n += 1;
        let de = [
            x0 as i64 - bx as i64,
            (x1 as i64) - (bx + bw) as i64,
            y0 as i64 - by as i64,
            (y1 as i64) - (by + bh) as i64,
        ];
        for (i, v) in de.iter().enumerate() {
            *s.e[i].entry(*v).or_default() += 1;
        }
        s.extra += extra;
        s.missing += missing;
        s.ink += ink;
    }
    for (k, s) in &stats {
        println!(
            "{k:22} n={:6} ink={:9} extra={:7} missing={:7}\n    L {:?}\n    R {:?}\n    T {:?}\n    B {:?}",
            s.n, s.ink, s.extra, s.missing, s.e[0], s.e[1], s.e[2], s.e[3]
        );
    }
}

/// How many of `draws` have the browser's box, and the browser's pixels.
fn matches(draws: &[&Draw], font: &[u8], tuning: Tuning) -> (u64, u64) {
    let mut r = OutlineRaster::with_tuning(font, tuning).expect("font").lenient();
    let (mut nb, mut nm) = (0, 0);
    for d in draws {
        let b = r.draw(d.w, d.h, std::slice::from_ref(&d.cmd)).expect("draw");
        let (w, h) = (b.w as usize, b.h as usize);
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
        for y in 0..h {
            for x in 0..w {
                if b.data[(y * w + x) * 4 + 3] != 0 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + 1);
                    y1 = y1.max(y + 1);
                }
            }
        }
        let ours = if x1 > x0 { [x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32] } else { [0, 0, 0, 0] };
        if ours != d.bbox {
            continue;
        }
        nb += 1;
        let [bx, by, bw, bh] = d.bbox.map(|v| v as usize);
        let mut same = true;
        'o: for yy in 0..bh {
            for xx in 0..bw {
                let i = yy * bw + xx;
                if (b.data[((yy + by) * w + xx + bx) * 4 + 3] != 0) != (d.bits[i / 8] >> (i % 8) & 1 != 0) {
                    same = false;
                    break 'o;
                }
            }
        }
        nm += u64::from(same);
    }
    (nb, nm)
}

/// For each font size, the dilation that gives the most exact boxes, among the draws of `kind`
/// (a comma-separated list of kinds).
fn gridsize(draws: &[Draw], font: &[u8], kinds: &str, base: Tuning) {
    let kinds: Vec<&str> = kinds.split(',').collect();
    let mut sizes: Vec<i64> = draws.iter().filter(|d| kinds.contains(&d.kind)).map(|d| d.cmd.font_size as i64).collect();
    sizes.sort();
    sizes.dedup();
    for s in sizes {
        let sub: Vec<&Draw> = draws.iter().filter(|d| kinds.contains(&d.kind) && d.cmd.font_size as i64 == s).take(250).collect();
        let mut best = (0, 0.0, 0.0, 0.0);
        for dx10 in 0..=16 {
            let dx = f64::from(dx10) * 0.025;
            for dt10 in 0..=16 {
                let dt = f64::from(dt10) * 0.05;
                for db10 in 0..=2 {
                    let db = f64::from(db10) * 0.1;
                    let mut t = base;
                    t.set("dil", &[dx, dx, dt, db]);
                    let (nb, _) = matches(&sub, font, t);
                    if nb > best.0 {
                        best = (nb, dx, dt, db);
                    }
                }
            }
        }
        let mut t = base;
        t.set("dil", &[0.3, 0.3, 0.6, 0.0]);
        let (now, _) = matches(&sub, font, t);
        println!("size {s:4} n={:4} best {:4} dx={:.3} top={:.2} bottom={:.1}   at .3/.6/0: {now}", sub.len(), best.0, best.1, best.2, best.3);
    }
}

/// `fit grid <draws> <truth> <kinds> <tuning per line, in a file>`: the score of each tuning on
/// the draws of those kinds, best first.
fn grid(draws: &[Draw], font: &[u8], kinds: &str, list: &str) {
    let kinds: Vec<&str> = kinds.split(',').collect();
    let every: usize = std::env::var("HX_EVERY").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    let sub: Vec<&Draw> = draws.iter().filter(|d| kinds.contains(&d.kind)).step_by(every).collect();
    let tunings: Vec<String> = std::fs::read_to_string(list).expect("list").lines().map(String::from).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let res = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|sc| {
        for _ in 0..8 {
            sc.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let Some(t) = tunings.get(i) else { break };
                let (b, m) = matches(&sub, font, parse_tuning(t));
                res.lock().expect("lock").push((b, m, t.clone()));
            });
        }
    });
    let mut res = res.into_inner().expect("lock");
    res.sort_by_key(|r| std::cmp::Reverse((r.1, r.0)));
    println!("{} draws, best by exact pixels", sub.len());
    for r in res.iter().take(12) {
        println!("box {:6} {:6.2}%  mask {:6} {:6.2}%   {}", r.0, 100.0 * r.0 as f64 / sub.len() as f64, r.1, 100.0 * r.1 as f64 / sub.len() as f64, r.2);
    }
}

/// The outline raster, except that the draws of some kinds come from the browser's pixels: how much
/// would fragments gain from a perfect model of those draws?
struct TruthRaster<'a, 'f> {
    inner: OutlineRaster<'f>,
    truth: &'a std::collections::HashMap<String, &'a Draw>,
    kinds: &'a [&'a str],
}

impl Raster for TruthRaster<'_, '_> {
    fn advance(&mut self, ch: char, font_size: f64) -> Result<f64, RasterError> {
        self.inner.advance(ch, font_size)
    }
    fn draw(&mut self, w: f64, h: f64, cmds: &[DrawCmd]) -> Result<Bitmap, RasterError> {
        let mut out = self.inner.draw(w, h, &[])?;
        for c in cmds {
            let k = format!("{} {} {} {} {} {} {} {} {} {} {}", w, h, c.ch as u32, c.font_size, c.tx, c.ty, c.sx, c.sy, c.rotate, c.x, c.y);
            let mut done = false;
            if let Some(d) = self.truth.get(&k)
                && (self.kinds.contains(&d.kind) || self.kinds.contains(&"ALL"))
            {
                let [bx, by, bw, bh] = d.bbox.map(|v| v as usize);
                for yy in 0..bh {
                    for xx in 0..bw {
                        let i = yy * bw + xx;
                        if d.bits[i / 8] >> (i % 8) & 1 != 0 {
                            out.data[((yy + by) * out.w as usize + xx + bx) * 4 + 3] = 255;
                        }
                    }
                }
                done = true;
            }
            if !done {
                let one = self.inner.draw(w, h, std::slice::from_ref(c))?;
                for (a, b) in out.data.iter_mut().zip(one.data.iter()) {
                    *a |= *b;
                }
            }
        }
        Ok(out)
    }
    fn pow(&mut self, b: f64, e: f64) -> f64 {
        self.inner.pow(b, e)
    }
    fn log(&mut self, x: f64) -> f64 {
        self.inner.log(x)
    }
}

fn truthrun(draws: &[Draw], font: &[u8], kinds: &str, out_path: &str) {
    let kinds: Vec<&str> = kinds.split(',').collect();
    let truth: std::collections::HashMap<String, &Draw> = draws
        .iter()
        .map(|d| {
            let c = &d.cmd;
            (format!("{} {} {} {} {} {} {} {} {} {} {}", d.w, d.h, c.ch as u32, c.font_size, c.tx, c.ty, c.sx, c.sy, c.rotate, c.x, c.y), d)
        })
        .collect();
    let corpus = common::load_corpus(None);
    let mut out = std::io::BufWriter::new(std::fs::File::create(out_path).expect("create"));
    for chunk in corpus.chunks(200) {
        let mut raster = TruthRaster { inner: OutlineRaster::new(font).expect("font"), truth: &truth, kinds: &kinds };
        let mut engine = Engine::new();
        for f in chunk {
            let raw = RawOptions::from_parts(f.font_px, &f.color, &f.attrs);
            let (status, markup) = match hieroxide_core::parse(&f.text) {
                Err(_) => ("parse", String::new()),
                Ok(tree) => match layout::render_svg(&mut engine, &tree, &raw, &mut raster, &mut NoIds, true) {
                    Ok(r) => ("ok", r.svg.markup),
                    Err(layout::LayoutError::Unsupported(_)) => ("unsupported", String::new()),
                    Err(e) => ("err", e.to_string()),
                },
            };
            writeln!(out, "{}\t{}\t{}", f.id, status, common::esc(&markup)).expect("write");
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("truthrun") => {
            let draws = load(&args[2], &args[3]);
            truthrun(&draws, &common::font(), &args[4], &args[5]);
        }
        Some("record") => record(&args[2]),
        Some("grid") => {
            let draws = load(&args[2], &args[3]);
            grid(&draws, &common::font(), &args[4], &args[5]);
        }
        Some("gridsize") => {
            let draws = load(&args[2], &args[3]);
            gridsize(&draws, &common::font(), &args[4], parse_tuning(args.get(5).map_or("", String::as_str)));
        }
        Some("edges") => {
            let draws = load(&args[2], &args[3]);
            edges(&draws, &common::font(), parse_tuning(args.get(4).map_or("", String::as_str)));
        }
        Some("score") => {
            let draws = load(&args[2], &args[3]);
            let font = common::font();
            let tunings: Vec<&str> = if args.len() > 4 { args[4..].iter().map(String::as_str).collect() } else { vec![""] };
            for s in tunings {
                score(&draws, &font, parse_tuning(s), if s.is_empty() { "frozen" } else { s });
            }
        }
        _ => eprintln!("fit record <out> | fit score <draws> <truth> [tuning]..."),
    }
}
