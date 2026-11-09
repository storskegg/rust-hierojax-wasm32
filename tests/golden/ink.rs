//! Ink per sign, for `collisions.mjs`: reads one line per `svg` on stdin,
//!
//!   key  width  height  cp fontsize tx ty sx sy rotate x y  [the same again for each sign]
//!
//! rasterizes each sign alone with the exact outlines, at a device scale of 1, a pixel being ink when
//! it is at least half covered, and writes `key  pairs  overlap_px  clipped_px  ink_px`: pairs of
//! signs sharing ink, the pixels they share, and ink outside the box.

use std::io::{BufRead, Write};

use hieroxide_core::layout::{DrawCmd, Raster};
use hieroxide_outline::{OutlineRaster, Tuning};

#[path = "common.rs"]
mod common;

fn main() {
    let font = common::font();
    let mut raster = OutlineRaster::with_tuning(&font, Tuning::EXACT).expect("font");
    let stdin = std::io::stdin();
    let mut out = std::io::BufWriter::new(std::io::stdout());
    const MARGIN: f64 = 60.0;
    for line in stdin.lock().lines() {
        let line = line.expect("read");
        let c: Vec<&str> = line.split('\t').collect();
        let (w, h): (f64, f64) = (c[1].parse().expect("w"), c[2].parse().expect("h"));
        let (cw, ch) = ((w + 2.0 * MARGIN).ceil(), (h + 2.0 * MARGIN).ceil());
        // Each sign's ink, as a sorted list of pixel indices.
        let mut inks: Vec<Vec<u32>> = Vec::new();
        let mut clipped = 0u64;
        let mut total = 0u64;
        for s in c[3..].chunks_exact(9) {
            let n: Vec<f64> = s.iter().map(|v| v.parse().expect("number")).collect();
            let Some(chr) = char::from_u32(n[0] as u32) else { continue };
            let cmd = DrawCmd {
                ch: chr,
                font_size: n[1],
                tx: n[2] + MARGIN,
                ty: n[3] + MARGIN,
                sx: n[4],
                sy: n[5],
                rotate: n[6] * core::f64::consts::PI / 180.0,
                x: n[7],
                y: n[8],
            };
            let b = match raster.draw(cw, ch, std::slice::from_ref(&cmd)) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let bw = b.w as usize;
            let mut ink = Vec::new();
            for (i, px) in b.data.chunks_exact(4).enumerate() {
                if px[3] != 0 {
                    ink.push(i as u32);
                    let (x, y) = ((i % bw) as f64 - MARGIN + 0.5, (i / bw) as f64 - MARGIN + 0.5);
                    if x < 0.0 || y < 0.0 || x > w || y > h {
                        clipped += 1;
                    }
                }
            }
            total += ink.len() as u64;
            inks.push(ink);
        }
        let (mut pairs, mut overlap) = (0u64, 0u64);
        for i in 0..inks.len() {
            for j in i + 1..inks.len() {
                let (a, b) = (&inks[i], &inks[j]);
                let (mut p, mut q, mut n) = (0, 0, 0u64);
                while p < a.len() && q < b.len() {
                    match a[p].cmp(&b[q]) {
                        std::cmp::Ordering::Less => p += 1,
                        std::cmp::Ordering::Greater => q += 1,
                        std::cmp::Ordering::Equal => {
                            n += 1;
                            p += 1;
                            q += 1;
                        }
                    }
                }
                if n > 0 {
                    pairs += 1;
                    overlap += n;
                }
            }
        }
        writeln!(out, "{}\t{pairs}\t{overlap}\t{clipped}\t{total}", c[0]).expect("write");
    }
}
