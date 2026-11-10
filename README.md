# Hieroxide

Hieroxide typesets Egyptian hieroglyphs written in Unicode (with the format controls of Unicode 15) as SVG.
It is a Rust port, compiled to WebAssembly, of [hierojax](https://github.com/nederhof/hierojax) by
Dr Mark-Jan Nederhof, which does this in JavaScript in the browser. The layout rules, the tables and
the font are his. The site that explains the notation is
[nederhof.github.io/hierojax](https://nederhof.github.io/hierojax/).

It has two modes.

- **Compat** is a drop-in for upstream's `hierojax.js`. It takes the same `<span class="hierojax">`
  elements and `data-*` options, and writes the same output that upstream writes in the same browser,
  byte for byte. It measures glyphs with the page's own canvas, as upstream does, so its output is as
  platform-dependent as upstream's.
- **Consistent** is the new API's default. It takes the glyphs' sizes from the outlines in the font, not from a canvas, so
  the output is the same on every platform and at every font size. It is available as a JavaScript module,
  as a Rust crate, and as the command line `hieroxide`.

The font, `docs/NewGardiner.otf`, is not built into anything. You give the path or the URL.

Contents: [Quick starts](#quick-starts) | [Which one to use](#which-mode-to-use) | [Browsers](#browser-support) |
[Size and speed](#size-and-speed) | [Known limits](#known-limits) | [Getting it](#getting-it) |
[Licences](#licences) | [History](#history)

## Quick starts

Builds are made from this repository. You need Rust (the version in `rust-toolchain.toml`, with the
`wasm32-unknown-unknown` target) and Node 24. Install the build tool's one dependency first:

```sh
(cd tools && npm ci)
```

### A page: swap in the drop-in script

Build compat:

```sh
node tools/build-compat.mjs        # build/compat/hierojax.js and build/compat/hieroxide_wasm_bg.wasm
```

Serve `hierojax.js` where upstream's `docs/hierojax.js` was, with `hieroxide_wasm_bg.wasm` beside it (as
`application/wasm`), and keep upstream's `hierojax.css` and `NewGardiner.otf` (both in `docs/`).
The page needs nothing else changed:

```html
<link rel="stylesheet" href="hierojax.css">
<script src="hierojax.js"></script>
<script>
  window.addEventListener("DOMContentLoaded", () => { hierojax.processFragments(); });
</script>

<span class="hierojax" style="font-size: 30px" data-dir="hrl">𓂸𓐰𓏏𓂋</span>
```

`node tools/build-compat.mjs --embed --out build/embedded` makes one file with the module inside, for
a page that calls `syntax.parse` at start-up. It is about 25 KB larger (brotli) than the two files together.
[`js/compat/README.md`](js/compat/README.md) has the API, the site options (`window.HIEROJAX_CONFIG`),
what differs from upstream on purpose, and the load measurements.
A page with a Content-Security-Policy that has `script-src` needs `'wasm-unsafe-eval'`.

### JavaScript: the module

Build it:

```sh
node tools/build-consistent.mjs --only consistent         # build/consistent/: index.js and the module
node tools/build-consistent.mjs --only consistent-paths   # build/consistent-paths/: the same, with path output
```

Use it. The types are in [`js/hieroxide/index.d.ts`](js/hieroxide/index.d.ts); the source of the
entry point is [`js/hieroxide/index.js`](js/hieroxide/index.js).

```html
<span id="line"></span>
<script type="module">
  import { init, render } from './index.js';

  await init({ font: './NewGardiner.otf' });          // a URL, or the font's bytes
  const result = render(document.getElementById('line'), '𓂸𓐰𓏏𓂋', { dir: 'hrl' });
  if (!result.ok) console.log(result.kind, result.message);
</script>
```

- `init(options)` loads the module and the font. It is the only call that rejects. A bad font rejects with
  an `Error` whose `kind` is `'font'`.
- `render(element, text, options)` lays a fragment out and replaces the element's contents. On an error it leaves
  the element as it was and returns `{ ok: false, kind, message, position? }`.
- `layout(text, options)` does the same without touching a page (usable in a Worker). `parse(text)` only
  checks that a fragment parses.
- `options` are the names of the `data-*` attributes (`dir`, `sep`, `linesize`, `separated`, `shadepattern`,
  `align`, `border`, ...) and, for output, `svg` (`'text'`, the default, or `'paths'`), `label`, `selectable`,
  `datatext`, `xmlns` and `contents`.
- `svg: 'paths'` draws the signs as `<path>`s from the font, so the page needs no font, stylesheet or script. It
  works only in the build with paths (`build/consistent-paths`). Without it the result is a failure of kind
  `'unsupported'`.

### The command line

`hieroxide render` typesets a fragment as SVG. It is made for a build step that puts the result in static HTML:
a page needs no script, no font and no stylesheet to show it.

```sh
cargo build --release -p hieroxide-cli                    # target/release/hieroxide, about 650 kB
export HIEROXIDE_FONT=docs/NewGardiner.otf                # or --font FILE; the font is not bundled

target/release/hieroxide render '𓀀𓐱𓏏𓐰𓁐' > block.svg                # one block on stdout
echo '𓀀𓐱𓏏' | target/release/hieroxide render --dir hrl --label 'a scribe'
target/release/hieroxide render --batch < lines.jsonl > blocks.jsonl   # many lines, one process
```

Each line of a batch is `{"text": "...", "dir": "hrl", "label": "..."}`. Any option can be a key, and `"id"` is
copied. Each output line is `{"ok":true,"svg":"..."}` or `{"ok":false,"kind":"parse","message":"...","position":3}`.
`hieroxide render --help` lists the options. The same input and font give the same bytes. Exit status: 0; 1 if a
fragment could not be rendered (in batch mode, if any line could not); 2 for a wrong command line or font.

A block is sized in em with a `viewBox`, so it follows the text around it. It has `role="img"` and an `aria-label`
(default `Egyptian hieroglyphs`: pass `--label` with a transliteration or a translation).
What a reader copies is the Unicode as written: a selection of the page copies it, and a script can read it
from the `data-text` attribute of the `svg` (of the `span` with `--separated`).
`--selectable svg` leaves the block a bare `<svg>`. The default is a `span` that holds the `svg` and the
invisible text. Chromium and WebKit put line breaks around the text of a copy of the bare `svg`.
`--kind text` prints the smaller text form, for pages that load the font.

**Inside the spans a page already has.** A page with `<span class="hierojax" ...>` elements and CSS for them
wants the block *in* the span, with the span's class and attributes kept so that its CSS applies.
`--contents` writes only that: the `svg` and, before it, the invisible text. `hieroxide fill` does it
for a whole file:

```sh
target/release/hieroxide render --contents '𓀀𓐱𓏏'                    # only what goes inside a span
target/release/hieroxide fill page.html -o out.html                   # or to stdout; - is stdin
```

`fill` finds each `<span>` whose class has the token `hierojax`, keeps its opening tag byte for byte, and
replaces what is inside with the `--contents` block for the text it had. It reads the options from the
span's `data-*` attributes, as hierojax does (`type` and `standalone` are ignored). Everything else in the file
is copied as it is. A second run gives the same bytes. It follows the HTML tokenizer where it matters (comments;
the raw text of `script`, `style`, `textarea` and `title`; quoted attribute values; depth; character
references), and at anything it cannot be sure of it stops with the line number and status 1, and writes
nothing. A span with `data-border="true"` that is not separated gets a warning: the contents cannot draw a
border, the page's CSS has to. `hieroxide --help` has the details.

### Rust

The API crate is `hieroxide`, at [`crates/hieroxide`](crates/hieroxide) (`use hieroxide::…`). It is 0.x: anything may change before 1.0. Errors are values, with kinds `Font`, `Parse` (with a position in UTF-16 units), `Layout` and
`Unsupported`.

```rust
use hieroxide::{parse, Engine, Font, Options, SvgKind};

fn main() -> Result<(), hieroxide::Error> {
    let font = Font::from_bytes(std::fs::read("docs/NewGardiner.otf").unwrap())?;
    let mut engine = Engine::new(font, Options::default());
    let doc = parse("\u{13000}\u{13431}\u{13001}")?;
    let layout = engine.layout(&doc)?;
    let svg = layout.to_svg(SvgKind::Paths)?;   // one block: a <span> holding the <svg>
    println!("{svg}");
    for g in layout.glyphs() {
        println!("{:?} {:?}", g.glyph_id, g.transform);   // your own output, in em
    }
    Ok(())
}
```

`cargo doc -p hieroxide --open` has the rest.

## Which mode to use

| | Compat | Consistent |
|---|---|---|
| Output | upstream's, byte for byte, in the same browser | the same on every platform and font size |
| Glyph sizes come from | the page's canvas | the font's outlines |
| Output types | SVG, DOM and canvas | SVG (text or paths) |
| Use it when | you have a page that uses hierojax and want it faster, with nothing else changed | you want the same output everywhere, or no script and font on the page |

In compat nothing is fixed, because the rule is that it does what upstream does. Consistent mode fixes the
clear bugs and does a few things differently. The list is in [`CHANGELOG.md`](CHANGELOG.md), and
[`PORT_MAP.md`](PORT_MAP.md) maps the Rust code to the upstream code it ports.

## Browser support

Chromium, Firefox and WebKit are tested with Playwright 1.65.0 on macOS. The WebKit is Playwright's build,
which is the engine's own code. Safari itself (desktop and iOS) is not tested yet, so a release of Safari
could differ. Compat treats every browser that is
not Blink or Gecko as WebKit, which can cost speed and not correctness.

Compat's parity with upstream is checked on about 19,800 fragments: every sign, the options matrix, upstream's
test suite and demo pages, and the real texts. Zero differences in the three engines, apart from the
wording of a parse error and the limit on nesting (see the next section). Upstream differs from itself between
platforms, so "identical" means identical on the platform it is compared on.
[`tests/README.md`](tests/README.md) explains how to run all of it.

## Size and speed

| | Compat | Consistent |
|---|---|---|
| Download (script and `.wasm`, brotli) | about 72 KB | about 80 KB (about 90 KB with path output) |
| Time on a real page, against upstream's script (= 1.00), Chromium / Firefox / WebKit | 0.59 / 0.78 / 0.46 | 0.54 / 0.70 / 0.33 |

The time is the call plus the layout the browser still owes, on one Mac, from `tests/bench`. Other pages
and other machines give other numbers (compat took 0.23 to 0.78 on the pages measured). The font is much larger than the code:
2.6 MB, 1.48 MB brotli. See [`js/compat/README.md`](js/compat/README.md).

## Known limits

- **Do not run upstream's script on a page that `hieroxide fill` has filled.** Upstream reads the text of
  the span, which is the drawing, and may replace it. A page with no script is the point of `fill`. If a script
  must stay, use compat (it reads `data-text`), or read the text from `data-text` and call `render` yourself.
  See "What a script does to a filled page" in [`tests/README.md`](tests/README.md).
- **Selection.** Copying is exact in the three engines. The highlight of the invisible text is exact where the
  `svg` is a block or flex item. In `--contents` output on an inline `svg` in a taller line it sits too high
  (by up to 15.9 px in the cases measured), and a mouse can start a selection only by the box. A browser set to
  never draw text under some size (Chromium's `minimumFontSize`, Firefox's `font.minimum-size`) copies exactly,
  but shows stray glyphs in the selection. Details are in `tests/README.md`, "Selection boxes where the svg is
  inline" and "A minimum font size".
- **`--selectable svg`**: Chromium and WebKit put a line break around a copy that takes in the whole block.
- **Borders.** `--contents` cannot draw `data-border`; the page's CSS has to.
- **Compat is not byte-identical across platforms**, as upstream is not. Use consistent mode if you need that.
- **Compat limits.** More than 256 levels of nesting is a parse error. The parse error text differs from
  upstream's. `syntax.parse` throws until `hierojax.ready` has settled, in the default build (not in `--embed`). The
  classes of `formatting.js` (`Shapes.*`, `Group.*`) are not offered, so upstream's list pages (`insertionlist.html`
  and the like) do not work.
- **Consistent output is canvas-free by design.** `data-type=canvas` and `dom` are not made in consistent mode; it
  writes SVG.
- **Hatching** in consistent mode has a period of 1/5 em, not 4 px, so that it scales.
- **Firefox, `columns: 2`**: in a multi-column container with `type=dom` and separated fragments, compat's boxes
  can differ from upstream's by up to 0.6 px (about 2% of random test pages). Upstream's own layout also varies
  there from run to run. The cause is not known ([#10](https://github.com/storskegg/rust-hierojax-wasm32/issues/10)).
- **The Rust API and the module are 0.x.** They may change until 1.0.

## Getting it

Releases are on GitHub: <https://github.com/storskegg/rust-hierojax-wasm32/releases>. Nothing is on crates.io or npm.

The first is [0.1.0](https://github.com/storskegg/rust-hierojax-wasm32/releases/tag/v0.1.0). Each release holds:

| File | What it is |
|---|---|
| `hierojax-compat-<version>.zip` | Compat: `hierojax.js` and `hieroxide_wasm_bg.wasm`, to serve in place of upstream's `hierojax.js` |
| `hierojax-compat-embedded-<version>.zip` | Compat as one file, `hierojax.js` with the module inside |
| `hieroxide-js-<version>.zip` | Consistent mode, the JavaScript module: `index.js`, `index.d.ts` and the module |
| `hieroxide-js-paths-<version>.zip` | The same, able to draw the signs as paths (`svg: 'paths'`) |
| `hieroxide-<version>-<target>.tar.gz` / `.zip` | The command line `hieroxide`, for `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu` and `x86_64-pc-windows-msvc` |
| `SHA256SUMS` | The sha256 of each file: `shasum -a 256 -c SHA256SUMS` |

Each archive has the licence and a short `README.txt`. The font is in none of them: use `docs/NewGardiner.otf` (OFL 1.1, see
[`docs/LICENSING.md`](docs/LICENSING.md)), and keep upstream's `hierojax.css` for compat. To build from source instead, see the
quick starts.

## Licences

- **The code** is GPL-3.0-only. The text is in [`LICENSE`](LICENSE). Hieroxide is a port of hierojax, which is
  under the GPL-3.0 too.
- **The font**, NewGardiner by Mark-Jan Nederhof, is under the SIL Open Font License 1.1. The text, with the
  font's copyright line, is in [`OFL.txt`](OFL.txt).

What that means for someone who ships the output:

- Path SVG, from the CLI or from `svg: 'paths'`, is a document made with the font. The OFL does not apply to it and it
  needs no notice.
- If your page or your download serves `NewGardiner.otf` itself, keep the font unchanged and include `OFL.txt` with it.
- If you serve Hieroxide's scripts or `.wasm` to browsers, you are distributing GPL code: offer its source.

[`docs/LICENSING.md`](docs/LICENSING.md) gives the reasoning, with the OFL FAQ entries. It is a reading of the licences,
not legal advice.

## History

### Hieroxide (Hierojax+Rust)

My attempt at a Rust port of Dr Nederhof's most excellent HieroJax lib.

### Disclaimer

I am very new to Rust, and still figuring out a lot about the language, which includes some really ugly code,
awkward ways about going through things, as I'm exploring, etc.

Ultimately, I'll actually make something that's production-worthy, but for now I'm just learning.
