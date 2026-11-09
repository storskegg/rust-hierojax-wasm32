//! The path data of a glyph is its outline: for every glyph of the font, `d` read back is the
//! outline's points, each to a tenth of a font unit. `cargo test -p hieroxide-api --test outlines`

use hieroxide_core::layout::{outline_d, Seg};
use hieroxide_outline::OutlineRaster;

#[path = "../golden/common.rs"]
mod common;

/// A reader for the commands the printer writes: M m L l H h V v C c Z z, with the implicit repeats
/// and the separators of the path grammar. Gives the absolute points of each segment.
fn read(d: &str) -> Vec<Seg> {
    let b = d.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    let (mut cx, mut cy, mut sx, mut sy) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let mut cmd = b' ';
    let number = |i: &mut usize| -> f64 {
        while *i < b.len() && (b[*i] == b' ' || b[*i] == b',') {
            *i += 1;
        }
        let start = *i;
        if *i < b.len() && b[*i] == b'-' {
            *i += 1;
        }
        let mut dot = false;
        while *i < b.len() && (b[*i].is_ascii_digit() || (b[*i] == b'.' && !dot)) {
            dot |= b[*i] == b'.';
            *i += 1;
        }
        d[start..*i].parse::<f64>().unwrap_or_else(|_| panic!("number at {start} in {d:?}"))
    };
    loop {
        while i < b.len() && (b[i] == b' ' || b[i] == b',') {
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        if b[i].is_ascii_alphabetic() {
            cmd = b[i];
            i += 1;
            if cmd == b'z' || cmd == b'Z' {
                out.push(Seg::Close);
                cx = sx;
                cy = sy;
                continue;
            }
        } else if cmd == b'M' {
            cmd = b'L';
        } else if cmd == b'm' {
            cmd = b'l';
        }
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { (cx, cy) } else { (0.0, 0.0) };
        match cmd.to_ascii_lowercase() {
            b'm' => {
                let (x, y) = (ox + number(&mut i), oy + number(&mut i));
                out.push(Seg::Move(x as f32, y as f32));
                (cx, cy, sx, sy) = (x, y, x, y);
            }
            b'l' => {
                let (x, y) = (ox + number(&mut i), oy + number(&mut i));
                out.push(Seg::Line(x as f32, y as f32));
                (cx, cy) = (x, y);
            }
            b'h' => {
                let x = ox + number(&mut i);
                out.push(Seg::Line(x as f32, cy as f32));
                cx = x;
            }
            b'v' => {
                let y = oy + number(&mut i);
                out.push(Seg::Line(cx as f32, y as f32));
                cy = y;
            }
            b'c' => {
                let v: Vec<f64> = (0..6).map(|_| number(&mut i)).collect();
                out.push(Seg::Cubic((ox + v[0]) as f32, (oy + v[1]) as f32, (ox + v[2]) as f32, (oy + v[3]) as f32, (ox + v[4]) as f32, (oy + v[5]) as f32));
                (cx, cy) = (ox + v[4], oy + v[5]);
            }
            other => panic!("command {}", other as char),
        }
    }
    out
}

fn points(s: &Seg) -> Vec<f64> {
    match *s {
        Seg::Move(x, y) | Seg::Line(x, y) => vec![x.into(), y.into()],
        Seg::Cubic(a, b, c, d, e, f) => vec![a.into(), b.into(), c.into(), d.into(), e.into(), f.into()],
        Seg::Close => vec![],
    }
}

#[test]
fn path_data_is_the_outline() {
    let font = common::font();
    let mut raster = OutlineRaster::new(&font).expect("font");
    let (mut glyphs, mut segments, mut longest, mut non_integer) = (0usize, 0usize, 0usize, 0usize);
    for cp in (0x13000u32..=0x1345F).chain(0x14400..=0x14646).chain([0x27E6, 0x27E7, 0x27E8, 0x27E9, 0x2E22, 0x2E23, 0x5B, 0x5D, 0x7B, 0x7D]) {
        let Some(ch) = char::from_u32(cp) else { continue };
        let Some(g) = raster.outline(ch) else { continue };
        if g.path.is_empty() {
            continue;
        }
        let d = outline_d(&g.path);
        let back = read(&d);
        assert_eq!(back.len(), g.path.len(), "U+{cp:X}: {} segments, read back {}", g.path.len(), back.len());
        for (k, (want, got)) in g.path.iter().zip(&back).enumerate() {
            assert_eq!(std::mem::discriminant(want), std::mem::discriminant(got), "U+{cp:X} segment {k}: {want:?} is {got:?}");
            for (a, b) in points(want).iter().zip(points(got)) {
                assert!((a - b).abs() <= 0.051, "U+{cp:X} segment {k}: {want:?} read back as {got:?}");
                non_integer += usize::from(a.fract() != 0.0);
            }
        }
        glyphs += 1;
        segments += g.path.len();
        longest = longest.max(d.len());
    }
    eprintln!("{glyphs} glyphs, {segments} segments read back; longest path data {longest} bytes; {non_integer} coordinates are not whole font units");
    assert!(glyphs > 1000);
}

#[test]
fn small_cases() {
    let segs = [
        Seg::Move(10.0, 20.0),
        Seg::Line(30.0, 20.0),
        Seg::Line(30.0, 40.5),
        Seg::Line(30.5, 41.0),
        Seg::Cubic(31.0, 42.0, 32.0, 43.0, 35.0, 45.0),
        Seg::Close,
        Seg::Move(-5.0, -5.0),
        Seg::Line(-5.0, 0.0),
        Seg::Line(0.25, 0.0),
        Seg::Close,
    ];
    let d = outline_d(&segs);
    assert_eq!(d, "m10 20h20v20.5l.5.5c.5 1 1.5 2 4.5 4zm-15-25v5h5.3z", "{d}");
}
