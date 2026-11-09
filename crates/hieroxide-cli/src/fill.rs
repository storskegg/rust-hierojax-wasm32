//! `hieroxide fill`: the scanner that finds the `<span class="hierojax">` elements of an HTML file.
//!
//! It is not an HTML parser, and does not try to be one. It follows the tokenizer of the HTML
//! standard as far as it matters for finding a `span` and where it ends: comments, the raw text of
//! `script`, `style` and the like, quoted attribute values that hold a `>`, and depth. Whatever it
//! cannot be sure of is an [`Error`] with the line, and nothing is written: a wrong guess would
//! damage a page, and an error costs the author one edit.
//!
//! What it promises about the bytes: the text outside the contents of the spans it finds is never
//! looked at again after the scan, so it is the input's bytes, including line endings.

use std::ops::Range;

/// A problem, and the line (counting from 1) it is on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub line: usize,
    pub message: String,
}

type Res<T> = Result<T, Error>;

fn err<T>(line: usize, message: impl Into<String>) -> Res<T> {
    Err(Error { line, message: message.into() })
}

/// The `data-*` attributes (without `data-`) that `processFragmentNow` of upstream reads.
pub const DATA: [&str; 13] = [
    "fontsize", "signcolor", "dir", "linesize", "sep", "separated", "type", "bracketcolor", "shadepattern", "align", "border",
    "standalone", "log",
];

/// Elements with no end tag (and no contents) in HTML.
const VOID: [&str; 16] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "keygen", "link", "meta", "param", "source", "track", "wbr", "basefont",
];
/// Elements whose contents are text up to their end tag, in HTML content.
const RAW: [&str; 9] = ["script", "style", "textarea", "title", "xmp", "iframe", "noembed", "noframes", "plaintext"];

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0c | b'\r')
}

fn trim_ws(s: &str) -> &str {
    s.trim_matches(|c| matches!(c, ' ' | '\t' | '\n' | '\u{c}' | '\r'))
}

#[derive(Debug)]
struct Attr {
    name: String,
    /// The value as written (character references not decoded); empty if there is none.
    value: Range<usize>,
}

#[derive(Debug)]
struct Tag {
    name: String,
    attrs: Vec<Attr>,
    /// Whether it ends with `/>`.
    slash: bool,
    /// Whether it has no contents: `/>` in SVG and MathML, or a void element of HTML.
    empty: bool,
    start: usize,
    end: usize,
}

enum Tok {
    Text(Range<usize>),
    Comment,
    Start(Tag),
    End { name: String, start: usize, end: usize },
    /// The text inside `script`, `style` and the other elements that hold text.
    Raw,
}

struct Lexer<'a> {
    s: &'a str,
    b: &'a [u8],
    pos: usize,
    /// How many `svg` and `math` elements are open: in them `/>` closes a tag and `style` and
    /// `script` are ordinary elements.
    foreign: usize,
    /// After the start tag of a raw text element: its name and where the tag was.
    raw: Option<(String, usize)>,
    /// Where each line starts.
    lines: Vec<usize>,
}

impl<'a> Lexer<'a> {
    fn new(s: &'a str) -> Lexer<'a> {
        let b = s.as_bytes();
        let mut lines = vec![0];
        for (i, &c) in b.iter().enumerate() {
            // `\r\n`, `\n` and a lone `\r` each end a line, as the HTML standard has it.
            if c == b'\n' || (c == b'\r' && b.get(i + 1) != Some(&b'\n')) {
                lines.push(i + 1);
            }
        }
        Lexer { s, b, pos: 0, foreign: 0, raw: None, lines }
    }

    fn line(&self, pos: usize) -> usize {
        self.lines.partition_point(|&l| l <= pos)
    }

    fn find(&self, from: usize, pat: &str) -> Option<usize> {
        self.s.get(from..)?.find(pat).map(|i| i + from)
    }

    fn next(&mut self) -> Res<Option<Tok>> {
        if let Some((name, at)) = self.raw.take() {
            return self.raw_text(&name, at);
        }
        let n = self.b.len();
        if self.pos >= n {
            return Ok(None);
        }
        let start = self.pos;
        // The text up to the next `<` that begins markup (another `<` is text).
        let mut i = start;
        while let Some(j) = self.s.get(i..).and_then(|t| t.find('<')).map(|k| k + i) {
            if self.begins_markup(j) {
                if j > start {
                    self.pos = j;
                    return Ok(Some(Tok::Text(start..j)));
                }
                return self.markup(j).map(Some);
            }
            i = j + 1;
        }
        self.pos = n;
        Ok(Some(Tok::Text(start..n)))
    }

    fn begins_markup(&self, j: usize) -> bool {
        match self.b.get(j + 1) {
            Some(c) if c.is_ascii_alphabetic() => true,
            Some(b'!') | Some(b'?') => true,
            // `</` followed by nothing is text; by anything else, an end tag or a bogus comment.
            Some(b'/') => j + 2 < self.b.len(),
            _ => false,
        }
    }

    fn markup(&mut self, j: usize) -> Res<Tok> {
        let line = self.line(j);
        let c = self.b[j + 1];
        if c == b'!' {
            if self.s[j..].starts_with("<!--") {
                let a = j + 4;
                if self.b.get(a) == Some(&b'>') {
                    self.pos = a + 1;
                    return Ok(Tok::Comment);
                }
                if self.s[a..].starts_with("->") {
                    self.pos = a + 2;
                    return Ok(Tok::Comment);
                }
                let one = self.find(a, "-->").map(|k| k + 3);
                let two = self.find(a, "--!>").map(|k| k + 4);
                return match (one, two) {
                    (None, None) => err(line, "a comment is never closed"),
                    (x, y) => {
                        self.pos = x.into_iter().chain(y).min().unwrap_or(a);
                        Ok(Tok::Comment)
                    }
                };
            }
            if self.s[j..].starts_with("<![CDATA[") {
                if self.foreign > 0 {
                    return match self.find(j, "]]>") {
                        Some(k) => {
                            self.pos = k + 3;
                            Ok(Tok::Comment)
                        }
                        None => err(line, "a CDATA section is never closed"),
                    };
                }
                // In HTML content it is a comment that ends at the first `>`.
            }
            return self.bogus(j, line);
        }
        if c == b'?' {
            return self.bogus(j, line);
        }
        if c == b'/' {
            return match self.b.get(j + 2) {
                Some(d) if d.is_ascii_alphabetic() => {
                    let tag = self.tag(j + 1, line)?;
                    if (tag.name == "svg" || tag.name == "math") && self.foreign > 0 {
                        self.foreign -= 1;
                    }
                    Ok(Tok::End { name: tag.name, start: j, end: tag.end })
                }
                // `</>` is nothing at all.
                Some(b'>') => {
                    self.pos = j + 3;
                    Ok(Tok::Comment)
                }
                _ => self.bogus(j, line),
            };
        }
        // A start tag.
        let mut tag = self.tag(j, line)?;
        tag.start = j;
        let foreign_el = tag.name == "svg" || tag.name == "math";
        tag.empty = if self.foreign > 0 || foreign_el { tag.slash } else { VOID.contains(&tag.name.as_str()) };
        if foreign_el && !tag.slash {
            self.foreign += 1;
        }
        if self.foreign == 0 && RAW.contains(&tag.name.as_str()) {
            self.raw = Some((tag.name.clone(), j));
        }
        Ok(Tok::Start(tag))
    }

    /// `<!...>`, `<?...>` and `</...>` that are not tags: a comment up to the first `>`.
    fn bogus(&mut self, j: usize, line: usize) -> Res<Tok> {
        match self.find(j, ">") {
            Some(k) => {
                self.pos = k + 1;
                Ok(Tok::Comment)
            }
            None => err(line, "markup that starts here is never closed"),
        }
    }

    /// The tag whose `<` (or `</`'s `/`) is at `at - 1`... `at` is the first byte of the name.
    fn tag(&mut self, lt: usize, line: usize) -> Res<Tag> {
        let b = self.b;
        let n = b.len();
        let mut i = lt + 1;
        let name_start = i;
        while i < n && !is_ws(b[i]) && b[i] != b'/' && b[i] != b'>' {
            i += 1;
        }
        let name = self.s[name_start..i].to_ascii_lowercase();
        let mut attrs = Vec::new();
        let mut slash = false;
        loop {
            while i < n && is_ws(b[i]) {
                i += 1;
            }
            if i >= n {
                return err(line, "a tag is never closed");
            }
            match b[i] {
                b'>' => {
                    i += 1;
                    break;
                }
                b'/' => {
                    if b.get(i + 1) == Some(&b'>') {
                        slash = true;
                        i += 2;
                        break;
                    }
                    i += 1;
                    continue;
                }
                _ => {}
            }
            let a0 = i;
            i += 1;
            while i < n && !is_ws(b[i]) && !matches!(b[i], b'/' | b'>' | b'=') {
                i += 1;
            }
            let aname = self.s[a0..i].to_ascii_lowercase();
            while i < n && is_ws(b[i]) {
                i += 1;
            }
            let mut value = i..i;
            if i < n && b[i] == b'=' {
                i += 1;
                while i < n && is_ws(b[i]) {
                    i += 1;
                }
                if i >= n {
                    return err(line, "a tag is never closed");
                }
                match b[i] {
                    q @ (b'"' | b'\'') => {
                        let Some(k) = self.s[i + 1..].bytes().position(|c| c == q) else {
                            return err(self.line(i), "a quoted attribute value is never closed");
                        };
                        value = i + 1..i + 1 + k;
                        i += k + 2;
                        if i < n && !is_ws(b[i]) && b[i] != b'/' && b[i] != b'>' {
                            return err(self.line(i), "attributes with no space between them");
                        }
                    }
                    b'>' => {}
                    _ => {
                        let v0 = i;
                        while i < n && !is_ws(b[i]) && b[i] != b'>' {
                            i += 1;
                        }
                        value = v0..i;
                    }
                }
            }
            attrs.push(Attr { name: aname, value });
        }
        self.pos = i;
        Ok(Tag { name, attrs, slash, empty: false, start: lt, end: i })
    }

    /// The text of `script` and the like, up to its end tag (which the next call reads).
    fn raw_text(&mut self, name: &str, at: usize) -> Res<Option<Tok>> {
        let line = self.line(at);
        let from = self.pos;
        if name == "plaintext" {
            self.pos = self.b.len();
            return Ok(Some(Tok::Raw));
        }
        let lower = self.s[from..].to_ascii_lowercase();
        let pat = format!("</{name}");
        let mut k = 0;
        let end = loop {
            match lower[k..].find(&pat) {
                None => return err(line, format!("<{name}> is never closed")),
                Some(p) => {
                    let e = k + p;
                    match lower.as_bytes().get(e + pat.len()) {
                        Some(c) if is_ws(*c) || *c == b'/' || *c == b'>' => break e,
                        _ => k = e + pat.len(),
                    }
                }
            }
        };
        let body = &lower[..end];
        if name == "script" && body.contains("<!--") && body.contains("<script") {
            return err(line, "a script with both <!-- and <script in it: whether </script> ends it depends on rules this tool does not follow");
        }
        self.pos = from + end;
        Ok(Some(Tok::Raw))
    }
}

/// Decode the character references of `s`: numeric ones, and the five that XML has with `nbsp`.
/// Anything else is an error, with the byte offset in `s`: the HTML standard has 2,231 names, and
/// guessing at one would change the fragment.
fn decode(s: &str) -> Result<String, (usize, String)> {
    let s = if s.contains('\r') { s.replace("\r\n", "\n").replace('\r', "\n") } else { s.to_string() };
    if !s.contains('&') {
        return Ok(s);
    }
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'&' {
            let next = s[i..].find('&').map_or(s.len(), |k| i + k);
            out.push_str(&s[i..next]);
            i = next;
            continue;
        }
        let at = i;
        i += 1;
        if b.get(i) == Some(&b'#') {
            i += 1;
            let hex = matches!(b.get(i), Some(b'x' | b'X'));
            if hex {
                i += 1;
            }
            let d0 = i;
            while i < b.len() && (if hex { b[i].is_ascii_hexdigit() } else { b[i].is_ascii_digit() }) {
                i += 1;
            }
            if i == d0 {
                return Err((at, "a character reference with no digits".into()));
            }
            let v = u32::from_str_radix(&s[d0..i], if hex { 16 } else { 10 }).unwrap_or(u32::MAX);
            if b.get(i) == Some(&b';') {
                i += 1;
            }
            match char::from_u32(v) {
                // NUL, the C1 controls (which HTML maps to other characters) and the surrogates
                // and beyond Unicode (which it replaces) are not what an author means.
                Some(c) if v != 0 && !(0x80..=0x9f).contains(&v) => out.push(c),
                _ => return Err((at, format!("&#{}; is not a character that HTML takes as written; write the character itself", &s[d0..i].trim_end_matches(';')))),
            }
            continue;
        }
        let n0 = i;
        while i < b.len() && b[i].is_ascii_alphanumeric() {
            i += 1;
        }
        if i == n0 {
            out.push('&');
            continue;
        }
        let name = &s[n0..i];
        if b.get(i) != Some(&b';') {
            return Err((at, format!("&{name} has no semicolon; write &amp; for an ampersand")));
        }
        i += 1;
        out.push(match name {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            "nbsp" => '\u{a0}',
            _ => return Err((at, format!("&{name}; is a named character reference this tool does not know; write it as &#N;"))),
        });
    }
    Ok(out)
}

/// What the contents of a span say its fragment is.
#[derive(Debug, PartialEq, Eq)]
pub enum Source {
    /// The text, with the white space at its ends removed.
    Text(String),
    /// Contents hieroxide wrote: the `data-text` of the first element that has one.
    Filled(String),
    /// Nothing: the span is left as it is.
    Empty,
}

/// A `<span class="hierojax">` of the file.
#[derive(Debug)]
pub struct Found {
    /// The line of its opening tag.
    pub line: usize,
    /// Where its contents are: from the `>` of the opening tag to the `<` of the closing one.
    pub inner: Range<usize>,
    /// The `data-*` attributes of [`DATA`] that it has, decoded, with their values (an empty value is not given).
    pub data: Vec<(&'static str, String)>,
    pub source: Source,
}

#[derive(Debug, Default)]
pub struct Scan {
    pub spans: Vec<Found>,
    /// Things that were skipped, with the line.
    pub warnings: Vec<Error>,
}

fn class_has(l: &Lexer, t: &Tag, token: &str) -> Res<bool> {
    let Some(a) = t.attrs.iter().find(|a| a.name == "class") else { return Ok(false) };
    let v = decode(&l.s[a.value.clone()]).map_err(|(_, m)| Error { line: l.line(a.value.start), message: m })?;
    Ok(v.split(|c: char| matches!(c, ' ' | '\t' | '\n' | '\u{c}' | '\r')).any(|c| c == token))
}

/// Find the `span`s with the class `hierojax`, and what each says its fragment is.
pub fn scan(html: &str) -> Res<Scan> {
    let mut lx = Lexer::new(html);
    let mut out = Scan::default();
    while let Some(tok) = lx.next()? {
        let Tok::Start(tag) = tok else { continue };
        if !class_has(&lx, &tag, "hierojax")? {
            continue;
        }
        let line = lx.line(tag.start);
        if tag.name != "span" {
            out.warnings.push(Error { line, message: format!("<{}> has the class hierojax, and only spans are filled: left as it is", tag.name) });
            continue;
        }
        if tag.slash {
            return err(line, "<span ... /> is an open tag in HTML (the slash is ignored): write <span ...></span>");
        }
        out.spans.push(read_span(&mut lx, &tag, line)?);
    }
    Ok(out)
}

fn read_span(lx: &mut Lexer, open: &Tag, line: usize) -> Res<Found> {
    let mut data = Vec::new();
    for name in DATA {
        let full = format!("data-{name}");
        // The first attribute of a name is the one that counts, as in the DOM.
        if let Some(a) = open.attrs.iter().find(|a| a.name == full) {
            let v = decode(&lx.s[a.value.clone()]).map_err(|(o, m)| Error { line: lx.line(a.value.start + o), message: m })?;
            if !v.is_empty() {
                data.push((name, v));
            }
        }
    }
    let mut stack: Vec<String> = Vec::new();
    let mut text: Vec<Range<usize>> = Vec::new();
    let mut markup = false;
    let mut data_text: Option<String> = None;
    let close = loop {
        let Some(tok) = lx.next()? else { return err(line, "this span is never closed") };
        match tok {
            Tok::Text(r) => {
                if stack.is_empty() {
                    text.push(r);
                }
            }
            Tok::Comment | Tok::Raw => {}
            Tok::Start(t) => {
                let tl = lx.line(t.start);
                if t.name == "span" && class_has(lx, &t, "hierojax")? {
                    return err(tl, format!("a span with the class hierojax inside the one on line {line}"));
                }
                if lx.foreign == 0 && RAW.contains(&t.name.as_str()) {
                    return err(tl, format!("<{}> inside the span on line {line}: it is not a fragment", t.name));
                }
                markup = true;
                if data_text.is_none() {
                    if let Some(a) = t.attrs.iter().find(|a| a.name == "data-text") {
                        data_text = Some(decode(&lx.s[a.value.clone()]).map_err(|(o, m)| Error { line: lx.line(a.value.start + o), message: m })?);
                    }
                }
                if !t.empty {
                    stack.push(t.name);
                }
            }
            Tok::End { name, start, end } => match stack.last() {
                Some(top) if *top == name => {
                    stack.pop();
                }
                Some(top) => {
                    return err(lx.line(start), format!("</{name}> where </{top}> was expected, inside the span on line {line} (this tool does not guess at implied end tags)"));
                }
                None if name == "span" => break (start, end),
                None => return err(lx.line(start), format!("</{name}> with nothing open inside the span on line {line}")),
            },
        }
    };
    let inner = open.end..close.0;
    let source = if markup {
        match data_text {
            Some(t) if t.is_empty() => Source::Empty,
            Some(t) => Source::Filled(t),
            None => {
                return err(line, "the span has markup in it and no data-text, so what its fragment is cannot be told (it was not filled by hieroxide, or with --no-data-text)");
            }
        }
    } else {
        let mut t = String::new();
        for r in text {
            t.push_str(&decode(&lx.s[r.clone()]).map_err(|(o, m)| Error { line: lx.line(r.start + o), message: m })?);
        }
        let t = trim_ws(&t);
        if t.is_empty() { Source::Empty } else { Source::Text(t.to_string()) }
    };
    Ok(Found { line, inner, data, source })
}

/// The file with the contents of each span replaced by what `render` gives for it. Spans that are
/// [`Source::Empty`] are left as they are, with a warning. Errors from `render` are collected, one
/// per span, and nothing is returned in that case.
pub fn fill(html: &str, mut render: impl FnMut(&Found, &str) -> Result<String, String>) -> Result<(String, Vec<Error>), Vec<Error>> {
    let scan = scan(html).map_err(|e| vec![e])?;
    let mut warnings = scan.warnings;
    let mut errors = Vec::new();
    let mut out = String::with_capacity(html.len() + scan.spans.len() * 4096);
    let mut at = 0;
    for f in &scan.spans {
        let src = match &f.source {
            Source::Text(t) | Source::Filled(t) => t,
            Source::Empty => {
                warnings.push(Error { line: f.line, message: "the span has no text: left as it is".into() });
                continue;
            }
        };
        match render(f, src) {
            Ok(contents) => {
                out.push_str(&html[at..f.inner.start]);
                out.push_str(&contents);
                at = f.inner.end;
            }
            Err(m) => errors.push(Error { line: f.line, message: m }),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    out.push_str(&html[at..]);
    Ok((out, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(html: &str) -> Vec<(usize, Source)> {
        scan(html).unwrap().spans.into_iter().map(|f| (f.line, f.source)).collect()
    }

    fn text(t: &str) -> Source {
        Source::Text(t.to_string())
    }

    #[test]
    fn finds_a_span_by_its_class_token() {
        assert_eq!(spans(r#"<p><span class="hierojax">ab</span></p>"#), [(1, text("ab"))]);
        assert_eq!(spans("<span class='x  hierojax\n y'>ab</span>"), [(1, text("ab"))]);
        assert_eq!(spans("<SPAN CLASS=hierojax>ab</SPAN>"), [(1, text("ab"))]);
        // Not the token, not a span.
        assert!(spans(r#"<span class="hierojax-x">ab</span><span class="xhierojax">a</span><span data-class="hierojax">"#).is_empty());
        let s = scan(r#"<div class="hierojax">ab</div>"#).unwrap();
        assert!(s.spans.is_empty() && s.warnings.len() == 1);
    }

    #[test]
    fn comments_and_raw_text_are_skipped() {
        let html = concat!(
            "<!-- <span class=\"hierojax\">a</span> -->",
            "<!--> <!---> <!-- x --!> <!---->",
            "<script>var s = '<span class=\"hierojax\">a</span>'; if (a < b) {}</script>",
            "<style>/* <span class=\"hierojax\"> */ a > b {}</style>",
            "<textarea><span class=\"hierojax\">a</span></TEXTAREA >",
            "<title><span class=\"hierojax\">a</span></title>",
            "<span class=\"hierojax\">b</span>"
        );
        assert_eq!(spans(html), [(1, text("b"))]);
        // After `<!-->`, the comment is over.
        assert_eq!(spans("<!--><span class=\"hierojax\">b</span>"), [(1, text("b"))]);
    }

    #[test]
    fn quoted_attribute_values_may_hold_a_greater_than() {
        let html = "<a title=\"<span class='hierojax'>\" href='a>b'>x</a><span class=\"hierojax\" title=\"a>b\" data-dir='h>rl'>ab</span>";
        let s = scan(html).unwrap();
        assert_eq!(s.spans.len(), 1);
        assert_eq!(s.spans[0].data, [("dir", "h>rl".to_string())]);
        assert_eq!(&html[s.spans[0].inner.clone()], "ab");
    }

    #[test]
    fn nesting_is_by_depth() {
        let html = "<span class=\"hierojax\"><span aria-hidden=\"true\"><span>a</span></span><svg data-text=\"xy\"><path d=\"\"/><g></g></svg></span>after</span>";
        let s = scan(html).unwrap();
        assert_eq!(s.spans[0].source, Source::Filled("xy".into()));
        assert_eq!(&html[s.spans[0].inner.end..], "</span>after</span>");
        // Void elements and self-closing foreign elements do not open anything.
        let s = scan("<span class=\"hierojax\"><br><img src=x><svg data-text=\"q\"><circle/></svg></span>").unwrap();
        assert_eq!(s.spans[0].source, Source::Filled("q".into()));
    }

    #[test]
    fn markup_without_data_text_is_an_error() {
        let e = scan("\n<span class=\"hierojax\"><b>a</b></span>").unwrap_err();
        assert_eq!(e.line, 2);
        assert!(e.message.contains("data-text"));
    }

    #[test]
    fn unbalanced_markup_is_an_error_with_the_line() {
        let e = scan("a\n<span class=\"hierojax\">\n<svg data-text=\"x\"><g></svg></span>").unwrap_err();
        assert_eq!(e.line, 3);
        let e = scan("<p>\n<span class=\"hierojax\">a</p>").unwrap_err();
        assert_eq!(e.line, 2);
        let e = scan("<span class=\"hierojax\">a\n\n").unwrap_err();
        assert_eq!((e.line, e.message.as_str()), (1, "this span is never closed"));
    }

    #[test]
    fn nested_hierojax_and_self_closing_spans_are_errors() {
        assert!(scan("<span class=\"hierojax\"><span class=\"hierojax\">a</span></span>").is_err());
        assert!(scan("<span class=\"hierojax\"/>a</span>").unwrap_err().message.contains("open tag"));
        assert!(scan("<span class=\"hierojax\"><script>a</script></span>").is_err());
    }

    #[test]
    fn character_references() {
        assert_eq!(spans("<span class=\"hierojax\">&#x13000;&#77824;&#X13431;&amp;&lt;&quot;</span>"), [(1, text("\u{13000}\u{13000}\u{13431}&<\""))]);
        for bad in ["&bull;", "&amp", "&#;", "&#x;", "&#0;", "&#x80;", "&#xD800;", "&#x110000;", "&#99999999999999;"] {
            let e = scan(&format!("\n\n<span class=\"hierojax\">a{bad}</span>")).unwrap_err();
            assert_eq!(e.line, 3, "{bad}");
        }
        // A bare ampersand is one.
        assert_eq!(spans("<span class=\"hierojax\">a & b</span>"), [(1, text("a & b"))]);
        // The data-text of a filled span is decoded too.
        let s = scan("<span class=\"hierojax\"><svg data-text=\"&#x13000;&amp;&quot;\"></svg></span>").unwrap();
        assert_eq!(s.spans[0].source, Source::Filled("\u{13000}&\"".into()));
    }

    #[test]
    fn white_space_at_the_ends_is_not_part_of_the_text() {
        assert_eq!(spans("<span class=\"hierojax\">\r\n  ab\t\n</span>"), [(1, text("ab"))]);
        assert_eq!(spans("<span class=\"hierojax\"> a<!-- x -->b </span>"), [(1, text("ab"))]);
        assert_eq!(spans("<span class=\"hierojax\">  </span>"), [(1, Source::Empty)]);
    }

    #[test]
    fn lines_count_each_kind_of_line_ending() {
        let s = scan("a\r\nb\rc\n<span class=\"hierojax\">x</span>").unwrap();
        assert_eq!(s.spans[0].line, 4);
    }

    #[test]
    fn what_cannot_be_scanned_is_an_error() {
        for (html, line) in [
            ("a\n<!-- never", 2),
            ("a\n<span class=\"hierojax", 2),
            ("a\n<p title=\"x>", 2),
            ("a\n<script>x", 2),
            ("a\n<a b=\"c\"d>", 2),
            ("a\n<![CDATA[ x", 2),
            ("a\n<!doctype", 2),
            ("a\n<script><!-- <script> </script> -->", 2),
        ] {
            let e = scan(html).unwrap_err();
            assert_eq!(e.line, line, "{html:?}: {}", e.message);
        }
    }

    #[test]
    fn fill_replaces_only_the_contents() {
        let html = "<p>\r\n<span class=\"hierojax\" data-x=\"1\">\u{13000}</span>\r\n<span  class=\"hierojax\"\n>\u{13001}</span> </p>\t";
        let (out, w) = fill(html, |f, src| Ok(format!("[{}:{src}]", f.line))).unwrap();
        assert!(w.is_empty());
        assert_eq!(out, "<p>\r\n<span class=\"hierojax\" data-x=\"1\">[2:\u{13000}]</span>\r\n<span  class=\"hierojax\"\n>[3:\u{13001}]</span> </p>\t");
    }

    #[test]
    fn fill_collects_the_errors_and_writes_nothing() {
        let html = "<span class=\"hierojax\">a</span>\n<span class=\"hierojax\">b</span>\n<span class=\"hierojax\">c</span>";
        let e = fill(html, |_, s| if s == "a" { Ok("x".into()) } else { Err(format!("no {s}")) }).unwrap_err();
        assert_eq!(e.iter().map(|e| e.line).collect::<Vec<_>>(), [2, 3]);
    }
}
