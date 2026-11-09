// The native side of the wall benchmark (tests/bench/wall.mjs): lays out the fragments of a file, one per
// line, with the API, and prints one line of JSON. Peak memory is not measured here: the caller runs it
// under `/usr/bin/time -l` (macOS) or `-v` (Linux).
//
//   cargo run --release -p hieroxide-api --example wall -- <font.otf> <fragments.txt> [layout|text|paths]
//
// `layout` lays out and counts the placed glyphs; `text` and `paths` also print the fragment as that
// kind of SVG and count its bytes. Each run is one pass over the file with one engine, as a page would
// do, so what the engine learns from the first fragments (outlines, measurements) is counted.
use std::time::Instant;

use hieroxide_api::{parse, Engine, Font, Options, SvgKind};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: wall <font.otf> <fragments.txt> [layout|text|paths]");
        std::process::exit(2);
    }
    let mode = args.get(3).map(String::as_str).unwrap_or("layout");
    let bytes = std::fs::read(&args[1]).expect("font");
    let text = std::fs::read_to_string(&args[2]).expect("fragments");
    let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();

    let t0 = Instant::now();
    let font = Font::from_bytes(bytes).expect("font");
    let mut engine = Engine::new(font, Options::default());
    let init_ms = t0.elapsed().as_secs_f64() * 1e3;

    let mut signs = 0usize;
    let mut glyphs = 0usize;
    let mut out_bytes = 0usize;
    let mut failed = 0usize;
    let mut parse_ms = 0.0;
    let mut layout_ms = 0.0;
    let mut print_ms = 0.0;
    let t_all = Instant::now();
    for line in &lines {
        signs += line.chars().filter(|c| ('\u{13000}'..='\u{1345F}').contains(c) && !('\u{13430}'..='\u{1345F}').contains(c)).count();
        let t = Instant::now();
        let doc = match parse(line) {
            Ok(d) => d,
            Err(_) => { failed += 1; continue; }
        };
        parse_ms += t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let layout = match engine.layout(&doc) {
            Ok(l) => l,
            Err(_) => { failed += 1; continue; }
        };
        layout_ms += t.elapsed().as_secs_f64() * 1e3;
        glyphs += layout.glyphs().len();
        let kind = match mode {
            "text" => Some(SvgKind::Text),
            "paths" => Some(SvgKind::Paths),
            _ => None,
        };
        if let Some(kind) = kind {
            let t = Instant::now();
            if let Ok(svg) = layout.to_svg(kind) {
                out_bytes += svg.len();
            }
            print_ms += t.elapsed().as_secs_f64() * 1e3;
        }
    }
    let total_ms = t_all.elapsed().as_secs_f64() * 1e3;
    println!(
        "{{\"mode\":\"{mode}\",\"fragments\":{},\"signs\":{signs},\"glyphs\":{glyphs},\"failed\":{failed},\"initMs\":{init_ms:.3},\"parseMs\":{parse_ms:.3},\"layoutMs\":{layout_ms:.3},\"printMs\":{print_ms:.3},\"totalMs\":{total_ms:.3},\"outBytes\":{out_bytes}}}",
        lines.len()
    );
}
