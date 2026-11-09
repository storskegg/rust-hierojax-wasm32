//! The command line: `cargo test -p hieroxide-cli --test cli`.
//!
//! Golden bytes (`tests/cli/golden/`), batch mode, and the errors. To accept a change of output,
//! which is a versioned one (`CHANGELOG.md`): `HIEROXIDE_CLI_UPDATE=1 cargo test -p hieroxide-cli --test cli`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn font() -> PathBuf {
    root().join("docs/NewGardiner.otf")
}

fn golden(name: &str) -> PathBuf {
    root().join("tests/cli/golden").join(name)
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

fn with_font<'a>(args: &[&'a str], font: &'a str) -> Vec<&'a str> {
    let mut v = vec!["render", "--font", font];
    v.extend_from_slice(args);
    v
}

fn check_golden(name: &str, got: &[u8]) {
    let path = golden(name);
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

/// `(file, arguments, the fragment)`
const CASES: &[(&str, &[&str], &str)] = &[
    ("line.svg", &[], LINE),
    ("line-svg-text.svg", &["--selectable", "svg"], LINE),
    ("line-no-copy.svg", &["--selectable", "none", "--no-data-text"], LINE),
    ("line-xmlns.svg", &["--selectable", "none", "--xmlns", "--label", "A \"line\" & more"], LINE),
    ("line-hrl.svg", &["--dir", "hrl"], LINE),
    ("line-vlr.svg", &["--dir", "vlr", "--sep", "0.1", "--sign-color", "#336"], LINE),
    ("line-separated.svg", &["--separated"], LINE),
    ("line-separated-vlr-border.svg", &["--separated", "--dir", "vlr", "--border", "--font-size", "30"], LINE),
    ("damaged-hatching.svg", &["--shading", "hatching", "--selectable", "none"], DAMAGED),
    ("brackets.svg", &["--bracket-color", "red"], BRACKETS),
    ("line-text.svg", &["--kind", "text"], LINE),
    ("line-contents.svg", &["--contents"], LINE),
    ("line-contents-svg-text.svg", &["--contents", "--selectable", "svg"], LINE),
    ("line-contents-font-size.svg", &["--contents", "--font-size", "30", "--border"], LINE),
    ("line-contents-vlr.svg", &["--contents", "--dir", "vlr", "--no-data-text"], LINE),
    ("line-contents-separated.svg", &["--contents", "--separated", "--font-size", "30"], LINE),
];

#[test]
fn bytes_are_golden() {
    let f = font();
    let f = f.to_str().unwrap();
    for (name, args, text) in CASES {
        let mut a = with_font(args, f);
        a.push(text);
        let out = run(&a, None);
        assert!(out.status.success(), "{name}: {}", String::from_utf8_lossy(&out.stderr));
        assert!(out.stderr.is_empty(), "{name}: stderr {:?}", String::from_utf8_lossy(&out.stderr));
        check_golden(name, &out.stdout);
        // The same bytes from stdin, with the newline that ends a line of input, and again: deterministic.
        let again = run(&with_font(args, f), Some(&format!("{text}\n")));
        assert_eq!(again.stdout, out.stdout, "{name}: stdin");
        assert_eq!(run(&a, None).stdout, out.stdout, "{name}: second run");
    }
}

#[test]
fn the_font_can_come_from_the_environment() {
    let out = Command::new(env!("CARGO_BIN_EXE_hieroxide"))
        .args(["render", LINE])
        .env("HIEROXIDE_FONT", font())
        .output()
        .expect("run");
    assert!(out.status.success());
    assert_eq!(out.stdout, std::fs::read(golden("line.svg")).expect("golden"), "same as --font");
}

/// Each line of the input is a fragment, with options of its own; each line out is its block or its error.
#[test]
fn batch_mode() {
    let f = font();
    let f = f.to_str().unwrap();
    let input = format!(
        "{{\"text\":\"{LINE}\",\"id\":\"a\"}}\n\
         {{\"text\":\"{LINE}\",\"dir\":\"hrl\",\"id\":7}}\n\
         {{\"text\":\"{LINE}\",\"kind\":\"text\",\"selectable\":\"none\"}}\n\
         {{\"text\":\"x\",\"id\":\"bad\"}}\n\
         {{\"text\":\"{LINE}\",\"dir\":\"sideways\"}}\n\
         {{\"text\":\"{LINE}\",\"nope\":1}}\n\
         not json\n\
         \n\
         {{\"text\":\"{LINE}\",\"label\":\"with \\\"quotes\\\" and \\u00e9\",\"separated\":true,\"fontSize\":30}}\n"
    );
    let out = run(&["render", "--font", f, "--batch"], Some(&input));
    assert_eq!(out.status.code(), Some(1), "some lines failed");
    assert!(String::from_utf8_lossy(&out.stderr).contains("5 line(s) could not be rendered"), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 9, "one line out for each line in");
    check_golden("batch.out", text.as_bytes());
    // Lines that went right are the single-block output, escaped.
    let single = String::from_utf8(run(&["render", "--font", f, LINE], None).stdout).unwrap();
    assert!(lines[0].starts_with("{\"id\":\"a\",\"ok\":true,\"svg\":\""));
    assert!(lines[0].len() > single.len() && lines[0].contains(&single.replace('"', "\\\"")), "line 0 is the block");
    assert!(lines[1].starts_with("{\"id\":7,\"ok\":true"));
    assert!(lines[3].starts_with("{\"id\":\"bad\",\"ok\":false,\"kind\":\"parse\",\"message\":\"parse error at code unit 0: unrecognized text\",\"position\":0}"), "{}", lines[3]);
    assert!(lines[4].contains("\"kind\":\"input\"") && lines[4].contains("dir is hlr"), "{}", lines[4]);
    assert!(lines[5].contains("\"kind\":\"input\"") && lines[5].contains("unknown key 'nope'"), "{}", lines[5]);
    assert!(lines[6].contains("\"kind\":\"input\"") && lines[6].contains("invalid JSON"), "{}", lines[6]);
    assert!(lines[7].contains("\"kind\":\"input\""), "an empty line is an error line: {}", lines[7]);
    // The label is HTML-escaped in the attribute, and the line JSON-escaped around that.
    assert!(lines[8].contains("with &quot;quotes&quot; and é"), "{}", lines[8]);
    // Each block is what `render` prints for the same fragment and options.
    let hrl = String::from_utf8(run(&["render", "--font", f, "--dir", "hrl", LINE], None).stdout).unwrap();
    assert!(lines[1].contains(&hrl.replace('"', "\\\"")));
    // All good: exit status 0.
    let ok = run(&["render", "--font", f, "--batch"], Some(&format!("{{\"text\":\"{LINE}\"}}\n")));
    assert_eq!(ok.status.code(), Some(0));
    assert!(ok.stderr.is_empty());
}

#[test]
fn errors_have_a_message_and_a_status() {
    let f = font();
    let f = f.to_str().unwrap();
    // A fragment that does not parse: status 1, the position, nothing on stdout.
    let out = run(&["render", "--font", f, "x"], None);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert_eq!(String::from_utf8_lossy(&out.stderr), "hieroxide: parse error at code unit 0: unrecognized text (kind: parse, position 0)\n");
    // The command line is wrong: status 2.
    for (args, msg) in [
        (vec!["render", LINE], "the font is required: --font FILE or HIEROXIDE_FONT"),
        (vec!["render", "--font", f, "--dir", "up", LINE], "dir is hlr, hrl, vlr or vrl, not 'up'"),
        (vec!["render", "--font", f, "--sep", "-1", LINE], "--sep needs a positive number"),
        (vec!["render", "--font", f, "--sep", "abc", LINE], "--sep needs a positive number, not 'abc'"),
        (vec!["render", "--font", f, "--bogus", LINE], "unknown option '--bogus'"),
        (vec!["render", "--font", f, "--label"], "--label needs a value"),
        (vec!["render", "--font", f, "a", "b"], "only one fragment can be given"),
        (vec!["render", "--font", f, "--kind", "png", LINE], "kind is paths or text, not 'png'"),
        (vec!["render", "--font", f, "--selectable", "maybe", LINE], "selectable is overlay, svg or none, not 'maybe'"),
        (vec!["frobnicate"], "unknown command 'frobnicate'"),
    ] {
        let out = run(&args, None);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.starts_with("hieroxide: ") && err.contains(msg), "{args:?}: {err}");
    }
    // The font is missing, or not a font.
    let out = run(&["render", "--font", "/no/such/font.otf", LINE], None);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("cannot read the font '/no/such/font.otf'"));
    let not_font = root().join("tests/cli/cli.rs");
    let out = run(&["render", "--font", not_font.to_str().unwrap(), LINE], None);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("is not a usable font"), "{}", String::from_utf8_lossy(&out.stderr));
    // Help is not an error; no command is.
    let h = run(&["render", "--help"], None);
    assert_eq!(h.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&h.stdout).contains("hieroxide render"));
    assert_eq!(run(&[], None).status.code(), Some(2));
}

/// The contents of an element the page has: the `svg` and, first, the text; no wrapper and no place on the line.
#[test]
fn contents_have_no_wrapper() {
    let f = font();
    let f = f.to_str().unwrap();
    let out = String::from_utf8(run(&["render", "--font", f, "--contents", LINE], None).stdout).unwrap();
    assert!(out.starts_with("<span aria-hidden=\"true\" style=\"position:absolute;width:"), "{out}");
    assert!(out.contains("font-family:sans-serif"));
    let svg = out.find("<svg").expect("svg");
    assert!(out[..svg].ends_with("</span>"), "the text comes before the svg");
    assert!(out.ends_with("</svg>"), "the svg is last");
    for banned in ["vertical-align", "position:relative", "display:inline-block", "left:0;top:0", "100%;height:100%", "class="] {
        assert!(!out.contains(banned), "{banned}");
    }
    // role, label and the text to copy are on the svg.
    let tag = &out[svg..svg + out[svg..].find('>').unwrap()];
    assert!(tag.contains("role=\"img\"") && tag.contains("aria-label=\"Egyptian hieroglyphs\"") && tag.contains("data-text=\""), "{tag}");
    // The text has the svg's size in em, and nothing else about its place.
    let w = tag.split("width=\"").nth(1).unwrap().split('"').next().unwrap();
    assert!(out.contains(&format!("width:{w};height:")), "the text is as wide as the svg ({w})");
    // A font size is on the svg and the text; a border is not written.
    let fs = String::from_utf8(run(&["render", "--font", f, "--contents", "--font-size", "30", "--border", LINE], None).stdout).unwrap();
    assert_eq!(fs.matches("font-size:30px").count(), 2, "{fs}");
    assert!(!fs.contains("border"));
    // Without a text, the block is the bare svg.
    for sel in ["svg", "none"] {
        let b = String::from_utf8(run(&["render", "--font", f, "--contents", "--selectable", sel, LINE], None).stdout).unwrap();
        assert!(b.starts_with("<svg ") && b.ends_with("</svg>") && !b.contains("aria-hidden"), "{sel}");
    }
    // Separated: one span with no class and no place on the line stays.
    let sp = String::from_utf8(run(&["render", "--font", f, "--contents", "--separated", LINE], None).stdout).unwrap();
    assert!(sp.starts_with("<span style=\"display:inline-block\" role=\"img\""), "{sp}");
    assert!(!sp.contains("vertical-align") && !sp.contains("class="));
    // The batch key.
    let b = run(&["render", "--font", f, "--batch"], Some(&format!("{{\"text\":\"{LINE}\",\"contents\":true}}\n")));
    let line = String::from_utf8(b.stdout).unwrap();
    assert!(line.contains("\\\"position:absolute;width:") && !line.contains("position:relative"), "{line}");
}

/// `contents` has no element to put a border on, so `fill` says so for each span that asked for one.
#[test]
fn fill_warns_when_a_border_cannot_be_drawn() {
    let html = format!(
        "<span class=\"hierojax\">{LINE}</span>\n<span class=\"hierojax\" data-border=\"true\">{LINE}</span>\n\
         <span class=\"hierojax\" data-border=\"true\" data-separated=\"true\">{LINE}</span>\n<span class=\"hierojax\" data-border=\"false\">{LINE}</span>"
    );
    let out = fill(&[], html.as_bytes());
    assert_eq!(out.status.code(), Some(0));
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(err.matches("warning: the border (data-border) is not drawn").count(), 1, "{err}");
    assert!(err.contains("in.html:2: warning: the border"), "{err}");
    // The output is not changed by the warning: no border in the second span, one in the separated third.
    let got = String::from_utf8(out.stdout).unwrap();
    assert_eq!(got.matches("border:solid 1px black").count(), 1);
    // The command line's --border is the same for every span that is not separated, unless the span says false.
    let all = fill(&["--border"], html.as_bytes());
    assert_eq!(String::from_utf8_lossy(&all.stderr).matches("not drawn").count(), 2);
    // --help says so.
    let h = run(&["fill", "--help"], None);
    assert!(String::from_utf8_lossy(&h.stdout).contains("is ignored\n                         unless --separated"));
}

fn tmp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("hieroxide-fill-{}-{name}", std::process::id()))
}

/// `hieroxide render --contents` for a fragment.
fn contents(args: &[&str], text: &str) -> String {
    let f = font();
    let mut a = vec!["render", "--font", f.to_str().unwrap(), "--contents"];
    a.extend_from_slice(args);
    a.push(text);
    String::from_utf8(run(&a, None).stdout).unwrap()
}

/// `hieroxide fill` on a file with these bytes (named `in.html`).
fn fill(args: &[&str], html: &[u8]) -> Output {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = tmp(&format!("dir{}", N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("in.html");
    std::fs::write(&path, html).unwrap();
    let f = font();
    let mut a = vec!["fill", "--font", f.to_str().unwrap()];
    a.extend_from_slice(args);
    // The path as the command line has it: the file's name only, so that messages are the same everywhere.
    a.push("in.html");
    let out = Command::new(env!("CARGO_BIN_EXE_hieroxide"))
        .args(&a)
        .current_dir(&dir)
        .env_remove("HIEROXIDE_FONT")
        .output()
        .expect("run");
    std::fs::remove_dir_all(dir).unwrap();
    out
}

const SRC_A: &str = "\u{13000}\u{13430}\u{13001}\u{13431}\u{13002}";

/// A page with each thing the scanner has to get past, and three spans in it.
fn page() -> String {
    format!(
        "<!DOCTYPE html>\r\n<html>\r\n<head><title><span class=\"hierojax\">{LINE}</span></title>\r\n<style>.a > .b {{ }} /* <span class=\"hierojax\"> */</style>\r\n\
         <script>var s = '<span class=\"hierojax\">{LINE}</span>'; if (a < b) {{ }}</script></head>\r\n\
         <body>\t<!-- <span class=\"hierojax\">{LINE}</span> -->\n\
         <textarea><span class=\"hierojax\">{LINE}</span></textarea>\n\
         <p title=\"a>b\">before <span class=\"x hierojax\" title=\"a>b\" data-dir='hrl' data-fontsize=\"30\" data-type=\"svg\" data-standalone=\"true\" >&#x13000;&#x13430;&#77825;&#x13431;\u{13002}</span> after</p>\r\n\
         <div class=\"hierojax\">{LINE}</div>\n\
         <span class=\"hierojax\">\n  {LINE}\n</span><span  class=\"hierojax\"\n data-separated=\"true\">{LINE}</span>\n</body>\n</html>\n"
    )
}

#[test]
fn fill_replaces_only_what_is_inside_the_spans() {
    let html = page();
    let out = fill(&[], html.as_bytes());
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    // Only the <div> is left, with a warning that says which line.
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("in.html:9: warning: <div> has the class hierojax"), "{err}");
    let got = String::from_utf8(out.stdout).unwrap();
    // The page with the contents of the three spans replaced, and nothing else changed.
    let a = "&#x13000;&#x13430;&#77825;&#x13431;\u{13002}";
    let want = html
        .replacen(a, &contents(&["--dir", "hrl", "--font-size", "30"], SRC_A), 1)
        .replacen(&format!("\n  {LINE}\n</span>"), &format!("{}</span>", contents(&[], LINE)), 1)
        .replacen(&format!("data-separated=\"true\">{LINE}</span>"), &format!("data-separated=\"true\">{}</span>", contents(&["--separated"], LINE)), 1);
    assert!(want != html);
    assert!(got == want, "the output is the page with three contents replaced");
    check_golden("fill.html", got.as_bytes());
    // The opening tags, the line endings and everything around them are the input's bytes.
    for keep in [
        "<!DOCTYPE html>\r\n<html>\r\n",
        "<body>\t<!-- <span class=\"hierojax\">",
        "<span class=\"x hierojax\" title=\"a>b\" data-dir='hrl' data-fontsize=\"30\" data-type=\"svg\" data-standalone=\"true\" >",
        "<span  class=\"hierojax\"\n data-separated=\"true\">",
        "</span> after</p>\r\n",
        "</span>\n</body>\n</html>\n",
    ] {
        assert!(got.contains(keep), "{keep:?}");
    }
    // `type` and `standalone` are ignored: the same as without them.
    let tag = |extra: &str| format!("<span class=\"hierojax\" data-dir=\"hrl\" data-fontsize=\"30\"{extra}>{SRC_A}</span>");
    let plain = fill(&[], tag("").as_bytes());
    let with = fill(&[], tag(" data-type=\"canvas\" data-standalone=\"true\"").as_bytes());
    let inner = |o: Output| {
        let s = String::from_utf8(o.stdout).unwrap();
        s[s.find('>').unwrap() + 1..].to_string()
    };
    assert_eq!(inner(plain), inner(with));
}

#[test]
fn filling_twice_is_the_same_bytes() {
    let once = fill(&[], page().as_bytes());
    assert_eq!(once.status.code(), Some(0));
    let twice = fill(&[], &once.stdout);
    assert_eq!(twice.status.code(), Some(0), "{}", String::from_utf8_lossy(&twice.stderr));
    assert!(twice.stdout == once.stdout, "a second run changes the file");
    // With other options on the command line the second run renders what the span says now, from data-text.
    let hrl = fill(&["--dir", "hrl"], &once.stdout);
    assert_eq!(hrl.status.code(), Some(0));
    assert!(hrl.stdout != once.stdout);
    // Options that the span names win over the command line's.
    let a = fill(&["--sign-color", "red"], "<span class=\"hierojax\" data-signcolor=\"blue\">\u{13000}</span>".as_bytes());
    let s = String::from_utf8_lossy(&a.stdout);
    assert!(s.contains("fill=\"blue\"") && !s.contains("red"), "{s}");
}

#[test]
fn a_span_that_cannot_be_filled_again_is_an_error_with_its_line() {
    let src = "a\n<span class=\"hierojax\">\u{13000}\u{13431}\u{13001}</span>\n";
    // Without data-text, a second run has nothing to read.
    let once = fill(&["--no-data-text"], src.as_bytes());
    assert_eq!(once.status.code(), Some(0), "{}", String::from_utf8_lossy(&once.stderr));
    assert!(!String::from_utf8_lossy(&once.stdout).contains("data-text"));
    let again = fill(&[], &once.stdout);
    assert_eq!(again.status.code(), Some(1));
    assert!(again.stdout.is_empty());
    let err = String::from_utf8_lossy(&again.stderr);
    assert!(err.contains("in.html:2: the span has markup in it and no data-text"), "{err}");
    // Markup that was never ours.
    let own = fill(&[], b"\n\n<span class=\"hierojax\"><b>x</b></span>");
    assert_eq!(own.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&own.stderr).contains("in.html:3: the span has markup"), "{}", String::from_utf8_lossy(&own.stderr));
}

#[test]
fn fill_errors_name_the_line_and_write_nothing() {
    let ok = "<span class=\"hierojax\">\u{13000}</span>";
    for (html, line, msg) in [
        (format!("{ok}\n\n<span class=\"hierojax\">x</span>"), 3, "the fragment does not parse: parse error at code unit 0"),
        (format!("{ok}\n<span class=\"hierojax\">\u{13000}x</span>"), 2, "position 2, in UTF-16 units"),
        ("\n<span class=\"hierojax\">\u{13000}&bull;</span>".to_string(), 2, "&bull; is a named character reference this tool does not know"),
        (format!("{ok}\r\n<!-- never closed\r\n"), 2, "a comment is never closed"),
        (format!("{ok}\r<span class=\"hierojax\"/>\u{13000}</span>"), 2, "an open tag in HTML"),
        (format!("{ok}\n<p>\n<span class=\"hierojax\">\u{13000}</p>"), 3, "</p> with nothing open"),
        (format!("{ok}\n\n\n<span class=\"hierojax\">\u{13000}"), 4, "this span is never closed"),
        ("<span class=\"hierojax\"><span class=\"hierojax\">\u{13000}</span></span>".to_string(), 1, "a span with the class hierojax inside"),
        ("a\n<span class=\"hierojax\" title=\"x>\u{13000}</span>".to_string(), 2, "a quoted attribute value is never closed"),
        ("a\n<script>\u{13000}".to_string(), 2, "<script> is never closed"),
    ] {
        let out = fill(&[], html.as_bytes());
        assert_eq!(out.status.code(), Some(1), "{html:?}");
        assert!(out.stdout.is_empty(), "nothing is written: {html:?}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains(&format!("in.html:{line}: ")) && err.contains(msg), "{html:?}: {err}");
    }
    // All the spans that cannot be rendered are named, not only the first.
    let out = fill(&[], b"<span class=\"hierojax\">x</span>\n<span class=\"hierojax\">y</span>");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("in.html:1: ") && err.contains("in.html:2: ") && err.contains("2 problem(s); nothing was written"), "{err}");
    // A file that is not UTF-8.
    let out = fill(&[], b"a\nb\xff<span class=\"hierojax\">x</span>");
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("in.html:2: not UTF-8"));
}

#[test]
fn fill_writes_where_it_is_told_and_takes_the_command_line_right() {
    let f = font();
    let f = f.to_str().unwrap();
    let input = tmp("o-in.html");
    let output = tmp("o-out.html");
    let (inp, outp) = (input.to_str().unwrap(), output.to_str().unwrap());
    std::fs::write(&input, format!("<p>\n<span class=\"hierojax\">{LINE}</span></p>")).unwrap();
    let out = run(&["fill", "--font", f, "-o", outp, inp], None);
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty(), "with -o, nothing on stdout");
    let written = std::fs::read_to_string(&output).unwrap();
    assert!(written.contains("<svg"));
    // Stdin as `-`, and the same bytes.
    let piped = run(&["fill", "--font", f, "-"], Some(&format!("<p>\n<span class=\"hierojax\">{LINE}</span></p>")));
    assert_eq!(String::from_utf8(piped.stdout).unwrap(), written);
    // Filling in place.
    let again = run(&["fill", "--font", f, "-o", inp, inp], None);
    assert_eq!(again.status.code(), Some(0));
    assert_eq!(std::fs::read_to_string(&input).unwrap(), written);
    // A file with no span is copied as it is.
    std::fs::write(&input, "<p>no spans</p>\r\n").unwrap();
    let out = run(&["fill", "--font", f, inp], None);
    assert_eq!(out.stdout, b"<p>no spans</p>\r\n");
    // An empty span is left as it is, with a warning.
    std::fs::write(&input, "<span class=\"hierojax\"> </span>").unwrap();
    let out = run(&["fill", "--font", f, inp], None);
    assert_eq!(out.stdout, b"<span class=\"hierojax\"> </span>");
    assert!(String::from_utf8_lossy(&out.stderr).contains(":1: warning: the span has no text"));
    std::fs::remove_file(&input).unwrap();
    std::fs::remove_file(&output).unwrap();
    for (args, msg) in [
        (vec!["fill", "--font", f], "fill needs a FILE"),
        (vec!["fill", "--font", f, "/no/such/file.html"], "cannot read '/no/such/file.html'"),
        (vec!["fill", "--font", f, "--kind", "text", "x.html"], "fill writes paths"),
        (vec!["fill", "--font", f, "--batch", "x.html"], "--batch is for render"),
        (vec!["fill", "--font", f, "a.html", "b.html"], "only one file can be given"),
        (vec!["render", "--font", f, "-o", "x", LINE], "--output is for fill"),
    ] {
        let out = run(&args, None);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains(msg), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }
}
