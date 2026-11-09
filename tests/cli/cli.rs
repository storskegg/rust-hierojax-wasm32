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
