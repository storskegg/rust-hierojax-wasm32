// SPDX-License-Identifier: GPL-3.0-only
//! The Android vector drawable (`--kind vd`): `cargo test -p hieroxide-cli --test vd`.
//!
//! It must draw what `--kind paths` draws. The tests check the XML is well formed, read the root,
//! the groups and the colours, compare path data with the path SVG's, and (`points`) draw both to
//! a set of points through their transforms and compare those. Golden bytes are in
//! `tests/cli/golden/`; to accept a change of output (a versioned one, see `CHANGELOG.md`):
//! `HIEROXIDE_CLI_UPDATE=1 cargo test -p hieroxide-cli --test vd`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn font() -> PathBuf {
    root().join("docs/NewGardiner.otf")
}

fn run(args: &[&str], stdin: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hieroxide"));
    cmd.args(args).env_remove("HIEROXIDE_FONT").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn");
    if let Some(s) = stdin {
        child.stdin.take().unwrap().write_all(s.as_bytes()).unwrap();
    } else {
        drop(child.stdin.take());
    }
    child.wait_with_output().expect("wait")
}

fn check_golden(name: &str, got: &[u8]) {
    let path = root().join("tests/cli/golden").join(name);
    if std::env::var_os("HIEROXIDE_CLI_UPDATE").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, got).unwrap();
        return;
    }
    let want = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert!(want == got, "{name}: the output differs from {} ({} bytes against {})", path.display(), got.len(), want.len());
}

/// Three groups: a sign, two joined, and a bracketed one.
const LINE: &str = "\u{13000}\u{13430}\u{13001}\u{13431}\u{13002}";
const DAMAGED: &str = "\u{13000}\u{13451}\u{13431}\u{13001}";
const BRACKETS: &str = "\u{27E6}\u{13000}\u{13431}\u{13001}\u{27E7}";
/// A sign turned 180 degrees (a variation selector) in a group: written right to left, its path SVG
/// has `scale(-1 1) rotate(180)`.
const ROTATED: &str = "\u{13000}\u{FE01}\u{13431}\u{13000}";
/// A sign turned 270 degrees, and shrunk to fit.
const ROTATED_SCALED: &str = "\u{13000}\u{FE02}\u{13430}\u{13000}";

fn vd(args: &[&str], text: &str) -> String {
    let f = font();
    let mut a = vec!["render", "--font", f.to_str().unwrap(), "--kind", "vd"];
    a.extend_from_slice(args);
    a.push(text);
    let out = run(&a, None);
    assert!(out.status.success(), "{args:?} {text:?}: {}", String::from_utf8_lossy(&out.stderr));
    assert!(out.stderr.is_empty());
    String::from_utf8(out.stdout).unwrap()
}

/// The path SVG of the same fragment, with nothing but the drawing around it.
fn paths(args: &[&str], text: &str) -> String {
    let f = font();
    let mut a = vec!["render", "--font", f.to_str().unwrap(), "--selectable", "none"];
    a.extend_from_slice(args);
    a.push(text);
    let out = run(&a, None);
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap()
}

type Tag = (String, Vec<(String, String)>);

/// Well-formedness, enough for what we write: one root, tags nest and match, attributes are
/// quoted and not repeated, a comment has no `--`, and text between tags is white space. Returns
/// the start tags in order.
fn check_xml(xml: &str) -> Vec<Tag> {
    let b = xml.as_bytes();
    let (mut i, mut stack, mut tags, mut roots) = (0, Vec::<String>::new(), Vec::new(), 0);
    if xml.starts_with("<?xml") {
        i = xml.find("?>").expect("declaration ends") + 2;
    }
    while i < b.len() {
        if b[i] != b'<' {
            assert!(b[i].is_ascii_whitespace(), "text outside a tag at {i}");
            i += 1;
        } else if xml[i..].starts_with("<!--") {
            let end = xml[i + 4..].find("-->").expect("comment ends") + i + 4;
            let body = &xml[i + 4..end];
            assert!(!body.contains("--") && !body.ends_with('-'), "a comment with --: {body:?}");
            i = end + 3;
        } else if xml[i..].starts_with("</") {
            let end = xml[i..].find('>').unwrap() + i;
            assert_eq!(stack.pop().as_deref(), Some(&xml[i + 2..end]), "end tag at {i}");
            i = end + 1;
        } else {
            let mut j = i + 1;
            let name_end = xml[j..].find(|c: char| c.is_whitespace() || c == '>' || c == '/').unwrap() + j;
            let name = xml[j..name_end].to_string();
            j = name_end;
            let mut attrs: Vec<(String, String)> = Vec::new();
            loop {
                while b[j].is_ascii_whitespace() {
                    j += 1;
                }
                if b[j] == b'>' || b[j] == b'/' {
                    break;
                }
                let eq = xml[j..].find('=').unwrap() + j;
                let key = xml[j..eq].to_string();
                assert!(!key.contains(char::is_whitespace) && !key.is_empty(), "attribute name {key:?}");
                assert_eq!(b[eq + 1], b'"', "attribute {key} is quoted");
                let close = xml[eq + 2..].find('"').unwrap() + eq + 2;
                let val = xml[eq + 2..close].to_string();
                assert!(!val.contains('<'), "< in an attribute");
                assert!(!attrs.iter().any(|a| a.0 == key), "attribute {key} twice");
                attrs.push((key, val));
                j = close + 1;
            }
            let self_closing = b[j] == b'/';
            if self_closing {
                j += 1;
            }
            assert_eq!(b[j], b'>');
            if stack.is_empty() {
                roots += 1;
            }
            assert!(roots == 1, "a second root");
            tags.push((name.clone(), attrs));
            if !self_closing {
                stack.push(name);
            }
            i = j + 1;
        }
    }
    assert!(stack.is_empty(), "unclosed {stack:?}");
    assert_eq!(roots, 1);
    tags
}

fn attr<'a>(tag: &'a Tag, name: &str) -> Option<&'a str> {
    tag.1.iter().find(|a| a.0 == name).map(|a| a.1.as_str())
}

#[test]
fn the_root_has_the_size_the_svg_gives() {
    let x = vd(&[], LINE);
    let svg = paths(&[], LINE);
    let tags = check_xml(&x);
    let root = &tags[0];
    assert_eq!(root.0, "vector");
    assert_eq!(attr(root, "xmlns:android"), Some("http://schemas.android.com/apk/res/android"));
    // The SVG's viewBox is the viewport; its em are 24dp each, written to four decimals as the SVG writes them.
    let vb = svg.split("viewBox=\"0 0 ").nth(1).unwrap().split('"').next().unwrap();
    let (vw, vh) = vb.split_once(' ').unwrap();
    assert_eq!(attr(root, "android:viewportWidth"), Some(vw));
    assert_eq!(attr(root, "android:viewportHeight"), Some(vh));
    let em = |name: &str| -> f64 { svg.split(&format!(" {name}=\"")).nth(1).unwrap().split("em\"").next().unwrap().parse().unwrap() };
    let dp = |name: &str| -> f64 { attr(root, name).unwrap().strip_suffix("dp").unwrap().parse().unwrap() };
    assert!((dp("android:width") - 24.0 * em("width")).abs() < 1e-3, "{x}");
    assert!((dp("android:height") - 24.0 * em("height")).abs() < 1e-3);
    assert!(x.starts_with("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!-- hieroxide: "), "{x}");
    let paths: Vec<_> = tags.iter().filter(|t| t.0 == "path").collect();
    assert_eq!(paths.len(), 3);
    for t in paths {
        assert_eq!(attr(t, "android:fillColor"), Some("#FF000000"));
    }
    assert!(x.ends_with("</vector>\n"));
}

#[test]
fn the_comment_has_the_text_and_the_label_and_cannot_break_the_xml() {
    let x = vd(&["--label", "a -- b --- c-"], LINE);
    check_xml(&x);
    let comment = x.split("<!--").nth(1).unwrap().split("-->").next().unwrap();
    assert!(comment.contains(LINE), "{comment}");
    assert!(comment.contains("label: a - - b - - - c- "), "{comment:?}");
}

/// The points a drawing has, in its viewport: the end and control points of each path, through its
/// transforms. Two drawings that place the same outlines the same way have the same points.
mod points {
    pub type P = (f64, f64);
    /// `[a b c d e f]`: `x' = a x + c y + e`.
    pub type M = [f64; 6];
    pub const ID: M = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

    pub fn mul(m: M, n: M) -> M {
        [
            m[0] * n[0] + m[2] * n[1],
            m[1] * n[0] + m[3] * n[1],
            m[0] * n[2] + m[2] * n[3],
            m[1] * n[2] + m[3] * n[3],
            m[0] * n[4] + m[2] * n[5] + m[4],
            m[1] * n[4] + m[3] * n[5] + m[5],
        ]
    }
    fn translate(x: f64, y: f64) -> M {
        [1.0, 0.0, 0.0, 1.0, x, y]
    }
    fn scale(x: f64, y: f64) -> M {
        [x, 0.0, 0.0, y, 0.0, 0.0]
    }
    fn rotate(deg: f64) -> M {
        let (s, c) = deg.to_radians().sin_cos();
        [c, s, -s, c, 0.0, 0.0]
    }
    fn apply(m: M, p: P) -> P {
        (m[0] * p.0 + m[2] * p.1 + m[4], m[1] * p.0 + m[3] * p.1 + m[5])
    }
    fn fnum(s: &str) -> f64 {
        s.trim().parse().unwrap_or_else(|_| panic!("number {s:?}"))
    }

    /// The points of a path's `d`: relative `m l h v c z`, as the writer emits, or absolute `M L z`.
    pub fn of_d(d: &str) -> Vec<P> {
        let mut out: Vec<P> = Vec::new();
        let b: Vec<char> = d.chars().collect();
        let (mut i, mut cmd) = (0, ' ');
        let (mut cur, mut start) = ((0.0, 0.0), (0.0, 0.0));
        let number = |i: &mut usize| -> f64 {
            while b[*i] == ' ' || b[*i] == ',' {
                *i += 1;
            }
            let s = *i;
            if b[*i] == '-' {
                *i += 1;
            }
            let mut dot = false;
            while *i < b.len() && (b[*i].is_ascii_digit() || (b[*i] == '.' && !dot)) {
                dot |= b[*i] == '.';
                *i += 1;
            }
            b[s..*i].iter().collect::<String>().parse().unwrap()
        };
        loop {
            while i < b.len() && (b[i] == ' ' || b[i] == ',') {
                i += 1;
            }
            if i >= b.len() {
                break;
            }
            if b[i].is_ascii_alphabetic() {
                cmd = b[i];
                i += 1;
                if cmd == 'z' || cmd == 'Z' {
                    cur = start;
                    continue;
                }
            }
            let rel = cmd.is_ascii_lowercase();
            let at = |cur: P, x: f64, y: f64| if rel { (cur.0 + x, cur.1 + y) } else { (x, y) };
            match cmd.to_ascii_lowercase() {
                'm' | 'l' => {
                    let (x, y) = (number(&mut i), number(&mut i));
                    cur = at(cur, x, y);
                    if cmd.eq_ignore_ascii_case(&'m') {
                        start = cur;
                        // What follows a moveto's pair is a lineto of the same kind.
                        cmd = if rel { 'l' } else { 'L' };
                    }
                    out.push(cur);
                }
                'h' => {
                    let x = number(&mut i);
                    cur = at(cur, x, 0.0);
                    if !rel {
                        cur.1 = out.last().map_or(0.0, |p| p.1);
                    }
                    out.push(cur);
                }
                'v' => {
                    let y = number(&mut i);
                    cur = at(cur, 0.0, y);
                    if !rel {
                        cur.0 = out.last().map_or(0.0, |p| p.0);
                    }
                    out.push(cur);
                }
                'c' => {
                    let from = cur;
                    for _ in 0..3 {
                        let (x, y) = (number(&mut i), number(&mut i));
                        cur = if rel { (from.0 + x, from.1 + y) } else { (x, y) };
                        out.push(cur);
                    }
                }
                other => panic!("path command {other}"),
            }
        }
        out
    }

    /// The matrix of an SVG `transform`: `translate(x y)`, `scale(x y)`, `rotate(r)`, outermost first.
    pub fn svg_matrix(t: &str) -> M {
        let mut m = ID;
        let mut rest = t.trim();
        while !rest.is_empty() {
            let open = rest.find('(').unwrap();
            let close = rest.find(')').unwrap();
            let args: Vec<f64> = rest[open + 1..close].split_whitespace().map(fnum).collect();
            m = mul(
                m,
                match &rest[..open] {
                    "translate" => translate(args[0], args[1]),
                    "scale" => scale(args[0], args[1]),
                    "rotate" => rotate(args[0]),
                    other => panic!("transform {other}"),
                },
            );
            rest = rest[close + 1..].trim_start();
        }
        m
    }

    /// A vector drawable group as Android applies it: translate, then rotate, then scale, those two
    /// about the pivot.
    pub fn group_matrix(attrs: &[(String, String)]) -> M {
        let get = |n: &str, d: f64| attrs.iter().find(|a| a.0 == format!("android:{n}")).map_or(d, |a| fnum(&a.1));
        let (px, py) = (get("pivotX", 0.0), get("pivotY", 0.0));
        let m = mul(translate(get("translateX", 0.0), get("translateY", 0.0)), translate(px, py));
        let m = mul(m, rotate(get("rotation", 0.0)));
        let m = mul(m, scale(get("scaleX", 1.0), get("scaleY", 1.0)));
        mul(m, translate(-px, -py))
    }

    pub fn through(m: M, d: &str) -> Vec<P> {
        of_d(d).into_iter().map(|p| apply(m, p)).collect()
    }

    pub fn sorted(mut v: Vec<P>) -> Vec<P> {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v
    }
}

/// The points of the sign paths of a path SVG.
fn svg_points(svg: &str) -> Vec<points::P> {
    let mut out = Vec::new();
    for p in svg.split("<path ").skip(1) {
        let (Some(t), Some(d)) = (p.split("transform=\"").nth(1), p.split(" d=\"").nth(1)) else { continue };
        out.extend(points::through(points::svg_matrix(t.split('"').next().unwrap()), d.split('"').next().unwrap()));
    }
    points::sorted(out)
}

/// The points of the sign paths of a vector drawable, through its groups. Shading (a path with an alpha) is left out.
fn vd_points(xml: &str) -> Vec<points::P> {
    let tags = check_xml(xml);
    let mut out = Vec::new();
    let mut stack = vec![points::ID];
    let (mut next, mut at) = (1, 0);
    while let Some(i) = xml[at..].find('<') {
        let rest = &xml[at + i..];
        if rest.starts_with("</group") {
            stack.pop();
        } else if rest.starts_with("<group") {
            stack.push(points::mul(*stack.last().unwrap(), points::group_matrix(&tags[next].1)));
            next += 1;
        } else if rest.starts_with("<path") {
            let t = &tags[next];
            next += 1;
            if attr(t, "android:fillAlpha").is_none() {
                out.extend(points::through(*stack.last().unwrap(), attr(t, "android:pathData").unwrap()));
            }
        }
        at += i + 1;
    }
    assert_eq!(next, tags.len());
    points::sorted(out)
}

#[test]
fn a_drawable_has_the_points_of_the_path_svg() {
    let cases: &[(&[&str], &str)] = &[
        (&[], "\u{13000}\u{13250}\u{1313F}"),
        (&[], LINE),
        (&["--dir", "vrl"], LINE),
        (&["--dir", "vlr"], LINE),
        (&["--dir", "hrl"], LINE),
        (&[], ROTATED),
        (&["--dir", "vrl"], ROTATED),
        (&["--dir", "vrl"], ROTATED_SCALED),
        (&[], ROTATED_SCALED),
        (&[], BRACKETS),
        (&["--dir", "vrl"], BRACKETS),
        (&[], "\u{13000}\u{FE04}\u{13430}\u{13001}"),
    ];
    for (args, text) in cases {
        let (a, b) = (vd_points(&vd(args, text)), svg_points(&paths(args, text)));
        let what = format!("{args:?} {text:?}");
        assert_eq!(a.len(), b.len(), "{what}: the number of points");
        assert!(!a.is_empty(), "{what}: no points");
        for (p, q) in a.iter().zip(&b) {
            assert!((p.0 - q.0).abs() < 1e-6 && (p.1 - q.1).abs() < 1e-6, "{what}: {p:?} against {q:?}");
        }
    }
}

#[test]
fn transforms_become_groups() {
    // The `scale(1 -1)` every path has: one group, with the offset the SVG's translate has.
    let x = vd(&[], "\u{13000}");
    let s = paths(&[], "\u{13000}");
    let tags = check_xml(&x);
    let groups: Vec<_> = tags.iter().filter(|t| t.0 == "group").collect();
    assert_eq!(groups.len(), 1, "{x}");
    let t = s.split("transform=\"translate(").nth(1).unwrap();
    let (tx, ty) = t.split(')').next().unwrap().split_once(' ').unwrap();
    assert!(s.contains(") scale(1 -1)\""), "{s}");
    assert_eq!(attr(groups[0], "android:translateX"), Some(tx));
    assert_eq!(attr(groups[0], "android:translateY"), Some(ty));
    assert_eq!(attr(groups[0], "android:scaleY"), Some("-1"));
    assert_eq!(attr(groups[0], "android:scaleX"), None, "a scale of 1 is the default");
    assert_eq!(attr(groups[0], "android:rotation"), None);

    // A sign turned 180 degrees: a group that moves and rotates, and inside it the one that puts the glyph at its origin and flips it.
    let x = vd(&[], "\u{13000}\u{FE01}");
    let s = paths(&[], "\u{13000}\u{FE01}");
    assert!(s.contains(" rotate(180) "), "{s}");
    let groups: Vec<_> = check_xml(&x).into_iter().filter(|t| t.0 == "group").collect();
    assert_eq!(groups.len(), 2, "{x}");
    assert_eq!(attr(&groups[0], "android:rotation"), Some("180"));
    assert!(attr(&groups[0], "android:translateX").is_some() && attr(&groups[0], "android:scaleY").is_none());
    assert_eq!(attr(&groups[1], "android:rotation"), None);
    assert_eq!(attr(&groups[1], "android:scaleY"), Some("-1"));
    assert!(x.contains("\n    <group ") && x.contains("\n      <path "), "the path is inside both groups: {x}");

    // A mirrored, rotated sign: the SVG scales, then rotates, and a group rotates before it scales, so
    // the scale is a group of its own, outside the rotation.
    let x = vd(&["--dir", "vrl"], ROTATED);
    let s = paths(&["--dir", "vrl"], ROTATED);
    assert!(s.contains("scale(-1 1) rotate(180)"), "{s}");
    let tags = check_xml(&x);
    let r = tags.iter().position(|t| attr(t, "android:rotation").is_some()).unwrap();
    assert_eq!(attr(&tags[r - 1], "android:scaleX"), Some("-1"), "{x}");
    assert_eq!(attr(&tags[r], "android:scaleX"), None);
    assert_eq!(tags[r + 1].0, "group");
    assert!(attr(&tags[r + 1], "android:scaleY").is_some());
    assert!(!x.contains("matrix") && !x.contains("pivot"), "every operation is about the origin");
}

#[test]
fn the_path_data_is_the_path_svgs() {
    for (args, text) in [(&[][..], LINE), (&["--dir", "vrl"][..], LINE), (&[][..], BRACKETS), (&["--dir", "vlr"][..], ROTATED)] {
        let (x, s) = (vd(args, text), paths(args, text));
        let ds = |t: &str, key: &str| -> Vec<String> { t.split(key).skip(1).map(|p| p.split('"').next().unwrap().to_string()).collect() };
        let (vd_d, svg_d) = (ds(&x, "android:pathData=\""), ds(&s, " d=\""));
        assert!(!vd_d.is_empty());
        assert_eq!(vd_d, svg_d, "{args:?} {text:?}");
    }
}

#[test]
fn colours_become_fill_colours_and_shading_becomes_alpha() {
    let x = vd(&["--sign-color", "#336", "--bracket-color", "red"], BRACKETS);
    let fills: Vec<String> = check_xml(&x).iter().filter(|t| t.0 == "path").map(|t| attr(t, "android:fillColor").unwrap().to_string()).collect();
    assert_eq!(fills, ["#FFFF0000", "#FF333366", "#FF333366", "#FFFF0000"], "{x}");
    for (css, want) in [
        ("#f00", "#FFFF0000"),
        ("#11223344", "#44112233"),
        ("rgb(1, 2, 3)", "#FF010203"),
        ("rgba(255 0 0 / 50%)", "#80FF0000"),
        ("rgba(0,0,255,0.5)", "#800000FF"),
        ("currentColor", "#FF000000"),
        ("Gray", "#FF808080"),
    ] {
        let x = vd(&["--sign-color", css], "\u{13000}");
        assert!(x.contains(&format!("android:fillColor=\"{want}\"")), "{css}: {x}");
    }
    // Shading is black with an alpha, so that a tint keeps it a shade; hatching is a path of bands.
    let u = vd(&[], DAMAGED);
    assert!(u.contains("android:fillColor=\"#FF000000\" android:fillAlpha=\"0.337\" android:pathData=\"M"), "{u}");
    let h = vd(&["--shading", "hatching"], DAMAGED);
    assert!(h.contains("android:fillAlpha=\"0.498\""), "{h}");
    check_xml(&u);
    check_xml(&h);
    // The shade is the SVG's: the same bands, the same rectangle.
    let s = paths(&["--shading", "hatching"], DAMAGED);
    let bands = s.split("<path d=\"").nth(1).unwrap().split('"').next().unwrap();
    assert!(h.contains(&format!("android:pathData=\"{bands}\"")), "{h}\n{s}");
    let s = paths(&[], DAMAGED);
    let mut rects = String::new();
    for r in s.split("<rect").skip(1) {
        let n = |a: &str| r.split(&format!(" {a}=\"")).nth(1).unwrap().split('"').next().unwrap().to_string();
        rects.push_str(&format!("M{} {}h{}v{}h-{}z", n("x"), n("y"), n("width"), n("height"), n("width")));
    }
    assert!(rects.len() > 20);
    let rect = format!("android:pathData=\"{rects}\"");
    assert!(u.contains(&rect), "{u}\n{s}");
}

#[test]
fn what_a_drawable_cannot_draw_is_an_error() {
    let f = font();
    let f = f.to_str().unwrap();
    for (args, msg) in [
        (vec!["--border"], "border is not supported with vd"),
        (vec!["--separated"], "separated is not supported with vd"),
        (vec!["--sign-color", "chartreuse"], "the colour 'chartreuse' has no Android form"),
        (vec!["--bracket-color", "var(--x)"], "bracket color"),
        (vec!["--sign-color", "#12"], "has no Android form"),
    ] {
        let mut a = vec!["render", "--font", f, "--kind", "vd"];
        a.extend_from_slice(&args);
        a.push(LINE);
        let out = run(&a, None);
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        assert!(out.stdout.is_empty());
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains(msg) && err.contains("kind: unsupported"), "{args:?}: {err}");
    }
    // fill writes paths only.
    let out = run(&["fill", "--font", f, "--kind", "vd", "-"], Some(""));
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("fill writes paths"));
}

#[test]
fn options_for_the_page_around_a_block_do_not_change_a_drawable() {
    let plain = vd(&[], LINE);
    for args in [
        vec!["--selectable", "svg"],
        vec!["--selectable", "none"],
        vec!["--no-data-text"],
        vec!["--xmlns"],
        vec!["--no-inline-style"],
        vec!["--contents"],
        vec!["--font-size", "30"],
        vec!["--standalone-font", "x.otf"],
    ] {
        assert_eq!(vd(&args, LINE), plain, "{args:?}");
    }
}

#[test]
fn a_drawable_is_deterministic_and_batch_mode_answers_with_vd() {
    let f = font();
    let f = f.to_str().unwrap();
    assert_eq!(vd(&[], LINE), vd(&[], LINE));
    assert_eq!(vd(&["--dir", "vrl"], ROTATED), vd(&["--dir", "vrl"], ROTATED));
    let input = format!(
        "{{\"text\":\"{LINE}\",\"kind\":\"vd\",\"id\":\"a\"}}\n\
         {{\"text\":\"{LINE}\",\"id\":2}}\n\
         {{\"text\":\"{LINE}\",\"kind\":\"vd\",\"border\":true,\"id\":3}}\n\
         {{\"text\":\"x\",\"kind\":\"vd\"}}\n\
         {{\"text\":\"{LINE}\",\"kind\":\"xml\"}}\n"
    );
    let out = run(&["render", "--font", f, "--batch"], Some(&input));
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 5);
    let escaped = vd(&[], LINE).replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
    assert_eq!(lines[0], format!("{{\"id\":\"a\",\"ok\":true,\"vd\":\"{escaped}\"}}"));
    assert!(lines[1].starts_with("{\"id\":2,\"ok\":true,\"svg\":\"<span"), "a line without a kind has the command line's: {}", lines[1]);
    assert!(lines[2].starts_with("{\"id\":3,\"ok\":false,\"kind\":\"unsupported\",\"message\":\"border is not supported with vd"), "{}", lines[2]);
    assert!(lines[3].contains("\"kind\":\"parse\""));
    assert!(lines[4].contains("kind is paths, text or vd, not 'xml'"), "{}", lines[4]);
    // With the command line's kind vd, every line answers with a drawable.
    let all = run(&["render", "--font", f, "--kind", "vd", "--batch"], Some(&format!("{{\"text\":\"{LINE}\"}}\n")));
    assert!(String::from_utf8_lossy(&all.stdout).starts_with("{\"ok\":true,\"vd\":\"<?xml"));
}

#[test]
fn drawable_goldens() {
    for (name, args, text) in [
        ("line.xml", &[][..], LINE),
        ("line-vrl.xml", &["--dir", "vrl"][..], LINE),
        ("rotated-vrl.xml", &["--dir", "vrl"][..], ROTATED),
        ("damaged-hatching.xml", &["--shading", "hatching"][..], DAMAGED),
        ("brackets-colours.xml", &["--bracket-color", "red", "--sign-color", "#336"][..], BRACKETS),
    ] {
        check_golden(name, vd(args, text).as_bytes());
    }
}
