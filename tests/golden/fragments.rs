//! Lays out the corpus natively and writes one line per fragment: `id<TAB>status<TAB>markup`.
//!
//!   cargo run --release -p hieroxide-outline --example fragments -- <mode> <out.tsv> [corpus.tsv.gz]
//!
//! Modes: `raster`, the outline raster under compat's layout and printer (what the drift from
//! upstream is measured on); `raster-exact`, the same with consistent mode's fixes to layout;
//! `consistent`, the mode itself. `HX_TUNING` overrides constants for
//! fitting, as `name=value;name=v,v,v,v`.

use std::io::Write;
use std::time::Instant;

use hieroxide_core::layout::{self, Engine, LayoutError, Mode, NoIds, RawOptions};
use hieroxide_outline::{OutlineRaster, Tuning};

#[path = "common.rs"]
mod common;

fn tuning() -> Tuning {
    let mut t = Tuning::FROZEN;
    let Ok(s) = std::env::var("HX_TUNING") else { return t };
    for kv in s.split(';').filter(|x| !x.is_empty()) {
        let (k, v) = kv.split_once('=').expect("name=value");
        let nums: Vec<f64> = v.split(',').map(|x| x.parse().expect("number")).collect();
        t.set(k, &nums);
    }
    t
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).expect("mode").clone();
    let out_path = args.get(2).expect("out").clone();
    let corpus = common::load_corpus(args.get(3).map(String::as_str));
    let font = common::font();
    let t = tuning();
    let mut out = std::io::BufWriter::new(std::fs::File::create(&out_path).expect("create"));
    let t0 = Instant::now();
    // As the parity runner does: fresh state per 200 fragments, in corpus order.
    for chunk in corpus.chunks(200) {
        let mut raster = OutlineRaster::with_tuning(&font, t).expect("font");
        if mode == "consistent" {
            raster = raster.lenient();
        }
        let mut engine = Engine::with_mode(if mode == "raster" { Mode::Compat } else { Mode::Consistent });
        for f in chunk {
            let raw = RawOptions::from_parts(f.font_px, &f.color, &f.attrs);
            let (status, markup) = match hieroxide_core::parse(&f.text) {
                Err(_) => ("parse", String::new()),
                Ok(tree) => match mode.as_str() {
                    "consistent" => match layout::render_consistent(&mut engine, &tree, &raw, &mut raster, &f.text) {
                        Ok(r) => ("ok", r.svg.markup),
                        Err(e) => ("err", e.to_string()),
                    },
                    "raster" | "raster-exact" => match layout::render_svg(&mut engine, &tree, &raw, &mut raster, &mut NoIds, true) {
                        Ok(r) => ("ok", r.svg.markup),
                        Err(LayoutError::Unsupported(_)) => ("unsupported", String::new()),
                        Err(e) => ("err", e.to_string()),
                    },
                    _ => panic!("unknown mode"),
                },
            };
            writeln!(out, "{}\t{}\t{}", f.id, status, common::esc(&markup)).expect("write");
        }
    }
    eprintln!("{} fragments in {:.2} s", corpus.len(), t0.elapsed().as_secs_f64());
}
