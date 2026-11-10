//! `hieroxide render`: hieroglyph fragments to SVG, for build steps.
//!
//! ```text
//! hieroxide render --font NewGardiner.otf '𓀀𓏏'            one block on stdout
//! echo '𓀀𓏏' | hieroxide render --font NewGardiner.otf     the fragment from stdin
//! hieroxide render --font NewGardiner.otf --batch          JSON lines in, JSON lines out
//! hieroxide fill --font NewGardiner.otf page.html -o out.html   fill the spans of a page
//! ```
//!
//! No dependencies beyond the library: the arguments and the JSON are read by hand, which keeps the
//! binary small.

mod fill;
mod json;

use std::io::{self, BufRead, Read, Write};
use std::process::ExitCode;

use hieroxide::{parse, Align, Direction, Engine, Error, ErrorKind, Font, Options, Selectable, Shading, SvgKind};

const HELP: &str = "\
hieroxide render: typeset Egyptian hieroglyphs as SVG

USAGE
  hieroxide render [OPTIONS] [TEXT]     one block on stdout (TEXT, or stdin if absent)
  hieroxide render [OPTIONS] --batch    one JSON object per input line, one JSON object per output line
  hieroxide fill [OPTIONS] FILE [-o OUT]  fill the <span class=\"hierojax\"> elements of an HTML file (see FILL)

OPTIONS
  --font FILE            the font (NewGardiner.otf); or set HIEROXIDE_FONT
  --kind paths|text      the SVG to print (default paths: self-contained)
  --dir D                hlr (default), hrl, vlr or vrl
  --font-size PX         the CSS font size of the block
  --line-size EM         space between lines (default 1)
  --sep EM               space between groups (default 0.07)
  --separated            one svg per group, in a span
  --align A              middle (default) or bottom
  --sign-color C         a CSS colour for signs (default: the page's text colour)
  --bracket-color C      a CSS colour for brackets (default black)
  --shading S            uniform (default) or hatching
  --border               a border of 1px
  --label TEXT           the accessible name of the block (default \"Egyptian hieroglyphs\")
  --selectable S         paths: how a selection on the page copies the text: overlay (default: a span
                         wraps the svg), svg (the svg stays bare) or none
  --no-data-text         paths: no data-text attribute
  --xmlns                paths: xmlns on each svg, so the output is also an SVG file
  --no-inline-style      paths: no style attribute (the page's CSS places the block)
  --contents             paths: only what goes inside an element the page already has (its own span, with its
                         class and CSS): no wrapper and no vertical-align; role, aria-label and data-text on
                         the svg; the invisible text before the svg, sized in em, at the svg's position;
                         --font-size is set on the svg and that text, --border (and fill's data-border) is ignored
                         unless --separated (fill warns); with
                         --separated one span with no class stays, to lay the svgs out, and it has the border
  -o, --output FILE      fill: write here instead of stdout
  --standalone-font URL  text: put the style with this font URL in the svg
  -h, --help, -V, --version

BATCH
  Each input line is a JSON object: {\"text\": \"...\", ...}. Keys besides text are
  dir, fontSize, lineSize, sep, separated, align, signColor, bracketColor, shading, border, label,
  kind, selectable, dataText, xmlns, inlineStyle, contents, standaloneFont; they override the options of
  the command line for that line. \"id\" is copied to the output. Each output line is
  {\"ok\":true,\"svg\":\"...\"} or {\"ok\":false,\"kind\":\"parse\",\"message\":\"...\",\"position\":3}
  (kinds: input, font, parse, layout, unsupported). The exit status is 1 if any line failed.

FILL
  Finds each <span> of FILE whose class has the token hierojax, keeps its opening tag byte for byte,
  and replaces what is inside it with the --contents output for the text it had. Everything else in
  the file is copied as it is. The options of a span are its data-* attributes, as hierojax reads them
  (fontsize, signcolor, dir, linesize, sep, separated, bracketcolor, shadepattern, align, border, log;
  type and standalone are for the other kinds of output and are ignored), over the command line's.
  The text is the span's text with the white space at its ends removed, character references decoded
  (numeric ones, and amp, lt, gt, quot, apos, nbsp; another name is an error). A span that has
  been filled is filled again from the data-text of its svg, so a second run gives the same bytes;
  without data-text (--no-data-text, or not ours) that is an error, as is any other markup in a span.
  The scanner knows comments, the raw text of script, style, textarea and title, quoted attribute
  values and depth, and stops with the line number and exit status 1, writing nothing, at anything it
  cannot be sure of (a tag or comment never closed, a self-closing span, an end tag that does not match,
  a span in a span, a fragment that does not parse). Other elements with the class are left as they are
  with a warning on stderr. FILE can be - for stdin.

EXIT STATUS
  0 success, 1 a fragment could not be rendered or a file could not be filled, 2 the command line,
  the font or a file is wrong.
";

struct Settings {
    options: Options,
    kind: SvgKind,
}

fn usage(msg: &str) -> ExitCode {
    eprintln!("hieroxide: {msg}\nTry 'hieroxide render --help'.");
    ExitCode::from(2)
}

fn positive(name: &str, x: f64) -> Result<f64, String> {
    if x.is_finite() && x > 0.0 { Ok(x) } else { Err(format!("{name} needs a positive number")) }
}

fn parse_f64(name: &str, v: &str) -> Result<f64, String> {
    v.parse::<f64>().map_err(|_| format!("{name} needs a positive number, not '{v}'")).and_then(|x| positive(name, x))
}

fn kind_of(v: &str) -> Result<SvgKind, String> {
    match v {
        "paths" => Ok(SvgKind::Paths),
        "text" => Ok(SvgKind::Text),
        _ => Err(format!("kind is paths or text, not '{v}'")),
    }
}

fn dir_of(v: &str) -> Result<Direction, String> {
    match v {
        "hlr" => Ok(Direction::Hlr),
        "hrl" => Ok(Direction::Hrl),
        "vlr" => Ok(Direction::Vlr),
        "vrl" => Ok(Direction::Vrl),
        _ => Err(format!("dir is hlr, hrl, vlr or vrl, not '{v}'")),
    }
}

fn align_of(v: &str) -> Result<Align, String> {
    match v {
        "middle" => Ok(Align::Middle),
        "bottom" => Ok(Align::Bottom),
        _ => Err(format!("align is middle or bottom, not '{v}'")),
    }
}

fn selectable_of(v: &str) -> Result<Selectable, String> {
    match v {
        "overlay" => Ok(Selectable::Overlay),
        "svg" => Ok(Selectable::Svg),
        "none" => Ok(Selectable::Off),
        _ => Err(format!("selectable is overlay, svg or none, not '{v}'")),
    }
}

fn shading_of(v: &str) -> Result<Shading, String> {
    match v {
        "uniform" => Ok(Shading::Uniform),
        "hatching" => Ok(Shading::Hatching),
        _ => Err(format!("shading is uniform or hatching, not '{v}'")),
    }
}

/// Apply a JSON line's keys over the settings of the command line.
fn apply_json(base: &Settings, obj: &[(String, json::Value)]) -> Result<(Settings, String), String> {
    use json::Value::{Bool, Num, Str};
    let mut s = Settings { options: base.options.clone(), kind: base.kind };
    let mut text = None;
    for (k, v) in obj {
        let bad = || format!("the value of '{k}' has the wrong type");
        match (k.as_str(), v) {
            ("text", Str(t)) => text = Some(t.clone()),
            ("id", _) => {}
            ("kind", Str(x)) => s.kind = kind_of(x)?,
            ("dir", Str(x)) => s.options.dir = dir_of(x)?,
            ("align", Str(x)) => s.options.align = align_of(x)?,
            ("shading", Str(x)) => s.options.shading = shading_of(x)?,
            ("fontSize", Num(x)) => s.options.font_size = Some(positive(k, *x)?),
            ("lineSize", Num(x)) => s.options.line_size = Some(positive(k, *x)?),
            ("sep", Num(x)) => s.options.sep = Some(positive(k, *x)?),
            ("separated", Bool(x)) => s.options.separated = *x,
            ("border", Bool(x)) => s.options.border = *x,
            ("xmlns", Bool(x)) => s.options.xmlns = *x,
            ("selectable", Str(x)) => s.options.selectable = selectable_of(x)?,
            ("dataText", Bool(x)) => s.options.data_text = *x,
            ("inlineStyle", Bool(x)) => s.options.inline_style = *x,
            ("contents", Bool(x)) => s.options.contents = *x,
            ("signColor", Str(x)) => s.options.sign_color = Some(x.clone()),
            ("bracketColor", Str(x)) => s.options.bracket_color = Some(x.clone()),
            ("label", Str(x)) => s.options.label = Some(x.clone()),
            ("standaloneFont", Str(x)) => s.options.standalone_font = Some(x.clone()),
            (
                "text" | "kind" | "dir" | "align" | "shading" | "fontSize" | "lineSize" | "sep" | "separated" | "border" | "xmlns"
                | "selectable" | "dataText" | "inlineStyle" | "contents" | "signColor" | "bracketColor" | "label" | "standaloneFont",
                _,
            ) => return Err(bad()),
            _ => return Err(format!("unknown key '{k}'")),
        }
    }
    let text = text.ok_or_else(|| "no 'text'".to_string())?;
    Ok((s, text))
}

fn render(engine: &mut Engine, s: &Settings, text: &str) -> Result<String, Error> {
    let doc = parse(text)?;
    let layout = engine.layout_with(&doc, &s.options)?;
    layout.to_svg(s.kind)
}

fn kind_name(k: ErrorKind) -> &'static str {
    match k {
        ErrorKind::Font => "font",
        ErrorKind::Parse => "parse",
        ErrorKind::Layout => "layout",
        ErrorKind::Unsupported => "unsupported",
        _ => "error",
    }
}

fn batch(engine: &mut Engine, base: &Settings) -> ExitCode {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let mut failed = 0usize;
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("hieroxide: reading stdin: {e}");
                return ExitCode::from(2);
            }
        };
        let line = line.strip_suffix('\r').unwrap_or(&line);
        let mut o = String::from("{");
        let parsed = json::parse_object(line);
        let id = parsed.as_ref().ok().and_then(|obj| obj.iter().find(|(k, _)| k == "id").map(|(_, v)| v.clone()));
        if let Some(id) = &id {
            o.push_str("\"id\":");
            id.write(&mut o);
            o.push(',');
        }
        let result = parsed
            .map_err(|m| Error { kind: ErrorKind::Parse, message: m, position: None })
            .map_err(|e| (Some("input"), e))
            .and_then(|obj| apply_json(base, &obj).map_err(|m| (Some("input"), Error { kind: ErrorKind::Parse, message: m, position: None })))
            .and_then(|(s, text)| render(engine, &s, &text).map_err(|e| (None, e)));
        match result {
            Ok(svg) => {
                o.push_str("\"ok\":true,\"svg\":");
                json::write_string(&svg, &mut o);
            }
            Err((kind, e)) => {
                failed += 1;
                o.push_str("\"ok\":false,\"kind\":\"");
                o.push_str(kind.unwrap_or_else(|| kind_name(e.kind)));
                o.push_str("\",\"message\":");
                json::write_string(&e.message, &mut o);
                if let Some(p) = e.position.filter(|_| kind.is_none()) {
                    o.push_str(&format!(",\"position\":{p}"));
                }
            }
        }
        o.push_str("}\n");
        if out.write_all(o.as_bytes()).is_err() {
            return ExitCode::from(2);
        }
    }
    if out.flush().is_err() {
        return ExitCode::from(2);
    }
    if failed > 0 {
        eprintln!("hieroxide: {failed} line(s) could not be rendered");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

/// `hieroxide fill`: the file's spans, filled. Exit 0, 1 (the file cannot be filled) or 2.
fn fill_file(engine: &mut Engine, s: &Settings, file: &str, output: Option<&str>) -> ExitCode {
    let mut bytes = Vec::new();
    let read = if file == "-" { io::stdin().lock().read_to_end(&mut bytes).map(|_| ()) } else { std::fs::read(file).map(|b| bytes = b) };
    if let Err(e) = read {
        eprintln!("hieroxide: cannot read '{file}': {e}");
        return ExitCode::from(2);
    }
    let html = match String::from_utf8(bytes) {
        Ok(h) => h,
        Err(e) => {
            let at = e.utf8_error().valid_up_to();
            let line = 1 + e.as_bytes()[..at].iter().filter(|&&b| b == b'\n').count();
            eprintln!("hieroxide: {file}:{line}: not UTF-8 (byte {at})");
            return ExitCode::from(1);
        }
    };
    let mut border_lost = Vec::new();
    let result = fill::fill(&html, |f, text| {
        let mut o = s.options.clone().with_data_attributes(f.data.iter().map(|(k, v)| (*k, v.as_str())));
        o.contents = true;
        // Upstream would have put a class on the element; the contents have no element to put a border on.
        if o.border && !o.separated {
            border_lost.push(f.line);
        }
        let doc = parse(text).map_err(|e| match e.position {
            Some(p) => format!("the fragment does not parse: {} (position {p}, in UTF-16 units)", e.message),
            None => e.message,
        })?;
        engine.layout_with(&doc, &o).and_then(|l| l.to_svg(SvgKind::Paths)).map_err(|e| e.message)
    });
    match result {
        Ok((out, warnings)) => {
            for w in warnings {
                eprintln!("hieroxide: {file}:{}: warning: {}", w.line, w.message);
            }
            for line in border_lost {
                eprintln!("hieroxide: {file}:{line}: warning: the border (data-border) is not drawn: the span's own CSS has to draw it (only a separated span has one)");
            }
            let done = match output {
                Some(path) => std::fs::write(path, out.as_bytes()),
                None => {
                    let mut o = io::stdout().lock();
                    o.write_all(out.as_bytes()).and_then(|()| o.flush())
                }
            };
            match done {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("hieroxide: cannot write the output: {e}");
                    ExitCode::from(2)
                }
            }
        }
        Err(errors) => {
            for e in &errors {
                eprintln!("hieroxide: {file}:{}: {}", e.line, e.message);
            }
            eprintln!("hieroxide: {} problem(s); nothing was written", errors.len());
            ExitCode::from(1)
        }
    }
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut filling = false;
    match args.next().as_deref() {
        Some("render") => {}
        Some("fill") => filling = true,
        Some("-h" | "--help") | None => {
            print!("{HELP}");
            return if std::env::args().len() > 1 { ExitCode::SUCCESS } else { ExitCode::from(2) };
        }
        Some("-V" | "--version") => {
            println!("hieroxide {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Some(other) => return usage(&format!("unknown command '{other}'")),
    }
    let mut s = Settings { options: Options::default(), kind: SvgKind::Paths };
    let mut font: Option<String> = std::env::var("HIEROXIDE_FONT").ok().filter(|v| !v.is_empty());
    let mut batch_mode = false;
    let mut text: Option<String> = None;
    let mut output: Option<String> = None;
    let mut only_text = false;
    while let Some(a) = args.next() {
        if only_text || !a.starts_with('-') || a == "-" {
            if text.is_some() {
                return usage(if filling { "only one file can be given" } else { "only one fragment can be given; use --batch for many" });
            }
            text = Some(a);
            continue;
        }
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        let r: Result<(), String> = (|| {
            match a.as_str() {
                "--" => only_text = true,
                "-h" | "--help" => {
                    print!("{HELP}");
                    std::process::exit(0);
                }
                "-V" | "--version" => {
                    println!("hieroxide {}", env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }
                "--batch" if filling => return Err("--batch is for render".into()),
                "--batch" => batch_mode = true,
                "-o" | "--output" if !filling => return Err("--output is for fill".into()),
                "-o" | "--output" => output = Some(value("--output")?),
                "--font" => font = Some(value("--font")?),
                "--kind" => s.kind = kind_of(&value("--kind")?)?,
                "--dir" => s.options.dir = dir_of(&value("--dir")?)?,
                "--font-size" => s.options.font_size = Some(parse_f64("--font-size", &value("--font-size")?)?),
                "--line-size" => s.options.line_size = Some(parse_f64("--line-size", &value("--line-size")?)?),
                "--sep" => s.options.sep = Some(parse_f64("--sep", &value("--sep")?)?),
                "--separated" => s.options.separated = true,
                "--align" => s.options.align = align_of(&value("--align")?)?,
                "--sign-color" => s.options.sign_color = Some(value("--sign-color")?),
                "--bracket-color" => s.options.bracket_color = Some(value("--bracket-color")?),
                "--shading" => s.options.shading = shading_of(&value("--shading")?)?,
                "--border" => s.options.border = true,
                "--label" => s.options.label = Some(value("--label")?),
                "--selectable" => s.options.selectable = selectable_of(&value("--selectable")?)?,
                "--no-data-text" => s.options.data_text = false,
                "--xmlns" => s.options.xmlns = true,
                "--no-inline-style" => s.options.inline_style = false,
                "--contents" => s.options.contents = true,
                "--standalone-font" => s.options.standalone_font = Some(value("--standalone-font")?),
                _ => return Err(format!("unknown option '{a}'")),
            }
            Ok(())
        })();
        if let Err(m) = r {
            return usage(&m);
        }
    }
    let Some(font_path) = font else {
        return usage("the font is required: --font FILE or HIEROXIDE_FONT");
    };
    let bytes = match std::fs::read(&font_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("hieroxide: cannot read the font '{font_path}': {e}");
            return ExitCode::from(2);
        }
    };
    let font = match Font::from_bytes(bytes) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("hieroxide: '{font_path}' is not a usable font: {e}");
            return ExitCode::from(2);
        }
    };
    let mut engine = Engine::new(font, s.options.clone());
    if filling {
        if s.kind != SvgKind::Paths {
            return usage("fill writes paths; --kind text does not apply");
        }
        let Some(file) = text else { return usage("fill needs a FILE (or - for stdin)") };
        return fill_file(&mut engine, &s, &file, output.as_deref());
    }
    if batch_mode {
        if text.is_some() {
            return usage("--batch reads stdin; no fragment argument");
        }
        return batch(&mut engine, &s);
    }
    let text = match text {
        Some(t) if t != "-" => t,
        _ => {
            let mut t = String::new();
            if let Err(e) = io::stdin().read_to_string(&mut t) {
                eprintln!("hieroxide: reading stdin: {e}");
                return ExitCode::from(2);
            }
            // The newline that ends a line of input is not part of the fragment.
            if t.ends_with('\n') {
                t.pop();
                if t.ends_with('\r') {
                    t.pop();
                }
            }
            t
        }
    };
    match render(&mut engine, &s, &text) {
        Ok(svg) => {
            let mut out = io::stdout().lock();
            if out.write_all(svg.as_bytes()).and_then(|()| out.flush()).is_err() {
                return ExitCode::from(2);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            match e.position {
                Some(p) => eprintln!("hieroxide: {} (kind: parse, position {p})", e.message),
                None => eprintln!("hieroxide: {} (kind: {})", e.message, kind_name(e.kind)),
            }
            ExitCode::from(1)
        }
    }
}
