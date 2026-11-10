# Changelog

Consistent mode's output counts as API: any change to a reference file in `tests/golden` is
listed here, and is a versioned change. Compat mode has no entries, because it has one rule: it
does what upstream does.

## Unreleased

### Firefox is recognised again (issue #11)

No change of output. Compat's engine test looked for `MozAppearance` in the style object, which Firefox 159 no longer
has, so Firefox was taken for WebKit and the shim forced a layout around each `type=dom` fragment: still exact, but slower.
Gecko is now found by `mozInnerScreenX` in `window` or by `CSS.supports('-moz-appearance', 'none')`, with the old test as a
fallback for older Firefox. `tests/parity/test/engine.test.mjs` pins the engine found in each of the three browsers.

### Crate names

No change of output. The public API crate `hieroxide-api` is now `hieroxide` (at `crates/hieroxide`, imported as `hieroxide`), and the
Unicode tables crate that was `hieroxide` at the repository root is now `hieroxide-tables` (lib `hieroxide_tables`). The CLI is
unchanged: package `hieroxide-cli`, binary `hieroxide`.

### Release artifacts

No change of output: the golden digest, the parity corpus and every test are as before.

* `tools/release.mjs` builds what a release holds into `dist/`: the compat drop-in (separate and embedded), the JavaScript module
  (default and `paths`), the `hieroxide` CLI for the host, and `SHA256SUMS`; `.github/workflows/release.yml` does it for a `v*` tag
  or by hand (Linux, macOS and Windows) and uploads a draft GitHub release. It fails over a size limit or on a build-machine path.
  The archives are deterministic.
* The shipped wasm modules no longer export `set_profile` and `timings` (nor import `performance.now`): they are behind the Cargo
  feature `test-levers`, which `build-compat.mjs --test-levers` turns on. Compat: 72,089 B to 71,997 B; its ratchet 72,500 B to 72,050 B.
* Every cargo build of the tools passes `--remap-path-prefix` (`tools/remap.mjs`), so no path of the build machine reaches the wasm or the
  CLI (issue #9).

### Toolchain and dependencies

No change of output: the golden digest, the parity corpus and every test are as before.

* Rust 1.99.1 is pinned in `rust-toolchain.toml` (with the `wasm32-unknown-unknown` target); both workflows install the same version.
* wasm-bindgen 0.2.129 to 0.2.130 (crate and CLI, still pinned exactly). `phf` 0.13 to 0.14; `regex` 1.13.1, `lazy_static` 1.5.1,
  `libm` 0.2.16, `flate2` 1.1.11 and the rest of `Cargo.lock` brought up with `cargo update`.
* `typescript` 5.9 to 7.0.2 (`tests/types`). `uglify-js` 3.19.3 and `playwright` 1.65.0 were already the latest.
* GitHub Actions: `checkout` v7, `setup-node` v7, `cache` v6, `upload-artifact` v7; node 24.22.2. Binaryen stays at `version_133`, the latest release.
* Size: the build with wasm-bindgen 0.2.130 is larger than with 0.2.129 (compat 71,687 B to 72,299 B, consistent 79,804 B to 80,484 B). Compat's ratchet moves from 72,000 B to 72,500 B; the 75,000 B target is unchanged.

### Path output: the invisible text of a line is one run, and the drawing is not selectable

A versioned change of path output (`--selectable overlay`, the default, and `--contents`; the CLI goldens in
`tests/cli/golden` for those changed; `svg` and `none`, the text printer, the golden digest and compat did not).

* The invisible text of a line of signs (`hlr`, `hrl`) is no longer a box over each sign. It is one run of text in a tiny font
  (a hundredth of the block's) in one flat `span`, with an inner `span` per sign whose `letter-spacing` puts the sign's text where
  the sign is drawn. The browser sees one line of text, so a double or triple click, a drag from beside the block and a drag across
  it select as they do on upstream's text, and the highlight is one band (Chromium) or a line (Firefox). The glyph that a browser
  paints when it paints a selection is the size of a speck, so no page CSS (`::selection`) is needed to keep it from showing over
  the drawing. A column (`vlr`, `vrl`) and a block that is not one line keep a box per sign.
* The `svg` of a block with an overlay has `user-select:none;-webkit-user-select:none` in its `style` (after a font size, if the
  block has one), so that in Firefox a drag that starts beside the block ends in the text, not in the drawing.
* With `--contents` the text's box is the svg's size in em over the run's font size (`width:317.37em` for a 3.1737 em svg), and with
  `--font-size N` the text's own font size is `N / 100` px.
* Copying is unchanged: exact in the three engines for every case of `tests/paths/selection.mjs`. `innerText` of the block is the
  source, so upstream's script, run on a filled page, redraws it (before, it showed a red parse error in Chromium and Firefox).
* Bytes (`hieroxide render`, 8 signs and 33 signs; raw / gzip -9 / brotli -q 11), default: 5,294 / 2,438 / 1,990 to
  4,963 / 2,418 / 1,964, and 18,567 / 4,985 / 3,970 to 16,847 / 4,813 / 3,853; `--contents`: 5,215 / 2,380 / 1,981 to
  4,883 / 2,348 / 1,941, and 18,489 / 4,911 / 3,954 to 16,768 / 4,729 / 3,816. `build/consistent-paths` (reported, not limited):
  88,986 to 90,357 B brotli. Compat (71,748 B) and consistent (79,804 B) do not move.
* The text's place on an inline svg is as it was (the static position, up to 15.9 px high; `tests/README.md`, "Selection boxes where
  the svg is inline").
* The run is clipped to the svg's box (`overflow:hidden;overflow:clip`), so a browser that is told never to draw text under some size
  (Chromium's `minimumFontSize`, Firefox's `font.minimum-size.*`), which draws the run longer than the block, adds no scrollable
  overflow to the page. Characters of Unicode 15 and after (U+1342F, U+13439 and up), which Firefox draws as a 17 px box when the
  system has no font for them, are in a font of size 0 inside the run, so they do not push the signs after them along.
* `tests/paths/gestures.mjs`: the mouse on a block against upstream's script, the selected look, and a gate; `tests/paths/minfont.mjs`:
  copy, overflow and stray ink with a minimum font size (12 px in Chromium and Firefox) and with Chromium's text autosizing.


### Path blocks inside the spans a page already has, and `hieroxide fill`

Additions only: the default output of every build, the golden digest and compat are unchanged (and compat and
consistent do not move in size: `tools/size-check.mjs`; path output is a separate build).

* `Options::contents` (CLI `--contents`, batch key `contents`, option `contents` of the module in the paths
  build): write only what goes *inside* an element the page has, such as its own `<span class="hierojax">`, which
  keeps its class and attributes and so its CSS. No wrapper `span` and no `vertical-align` (the span has the place on
  the line); `role`, `aria-label` and `data-text` are on the `svg`; the invisible text (`Selectable::Overlay`)
  comes first, is the `svg`'s own size in em (`width`, `height`) with no `left` or `top`, so that it takes the static
  position the `svg` takes in the span's box (block, inline block, flex box that centres, either direction), and
  has `font-family: sans-serif` so that a font the page names for the span is not fetched for it. `--font-size` is
  set on the `svg` and on that text; `--border` has no effect. With `--separated` one `span` with no class and
  no `vertical-align` stays, because the `svg`s need a box that the page's span cannot be. Where the `svg` is an
  inline box in a line taller above the baseline than the `svg`, the text sits too high by the difference (copying
  is exact; `tests/README.md`, `host.mjs`).
* `hieroxide fill FILE [-o OUT]`: fills the `<span>`s with the class token `hierojax` of an HTML file for a build
  step. The opening tag is kept byte for byte, everything outside the spans is the input's bytes, the options are the
  span's `data-*` attributes as upstream's `processFragmentNow` reads them (`type` and `standalone` are ignored), a
  filled span is filled again from the `data-text` of its `svg` (a second run gives the same bytes), and anything
  the scanner is not sure of (a comment, tag, quote, `script`, `textarea` or span never closed, a self-closing
  span, a span in a span, an end tag that does not match, markup without `data-text`, a named character
  reference other than `amp`, `lt`, `gt`, `quot`, `apos` and `nbsp`, a fragment that does not parse, a file that is
  not UTF-8) is an error with its line, exit status 1 and no output. No new dependency.
  A non-separated span with `data-border="true"` (or `--border`) gets a `FILE:LINE: warning`: its contents have no
  element to put the border on, so the page's CSS has to draw it.
* Compat: `readFragment` reads the `data-text` of a descendant when there is one, so a block that `fill` wrote
  redraws instead of showing a parse error when a script runs on it. Upstream never writes `data-text`, so
  nothing upstream can receive is read differently (`tests/parity/test/datatext.test.mjs`). +21 B (compat 71,748 B,
  252 under the ratchet).
* API: `Options::with_data_attributes`, the options of an element's `data-*` attributes as hierojax and layout read
  them.
* `tests/paths/host.mjs` (the contents block under four shapes of page CSS, in eight fixtures, in three engines) and
  `tests/paths/rescript.mjs` (what hierojax's scripts do to a page that was filled and still loads one).

### Size (M5, the size half)

Compat (`build/compat`, wasm + script, brotli -q 11): 96,909 B to 71,727 B (target 75,000; the wasm 85,722 to 65,880
and the script 11,187 to 5,847). Consistent: 99,755 B to 79,799 B (limit 100,000), including the text-copy change
below. Output is unchanged by all of it (compat: zero differences from upstream; consistent: only the change below).

* The built JavaScript is minified with uglify-js 3.19.3, a build-time dev dependency of `tools/` (`npm ci` there);
  the source in `js/` is as it was, and `--no-minify` leaves a build readable.
* `jsnum`'s `number_to_string` and `number_from_str` are our own (exact shortest digits; exact decimal parsing),
  not the standard library's float formatting and parsing; checked against real JS on 64M doubles and 2M strings.
* `tools/size-check.mjs` is the size gate (the `size` job of `.github/workflows/golden.yml`); limits are in
  `tools/size-limits.json`. It replaces `tools/consistent-sizes.mjs`.
* Measured and not taken: the LLVM inline threshold, `opt-level` s and z, wasm-opt variants, and a nightly
  `build-std` with immediate-abort panics (PORT_MAP.md, "Size").

### Consistent mode: hidden text is the source (versioned output change)

In consistent text mode, the hidden text of a fragment (what a selection copies) is now the characters of the
source, in order. Before, an enclosure's end was written as its opening's pair (U+1343F after U+1343C came out
as U+1343D), so the copied text differed from the source in 1,054 of the 19,636 corpus fragments that are laid
out. Lost signs already kept their own selector (`to_string_exact`); all 5,685 laid-out fragments with a variation
selector now copy their source. Compat is unchanged (upstream's text stays there), and so is path output, which
already took its characters from the source.

* The golden reference changes: `all ca7c955c1c161d46` became `all 2f6cec8bf973a3e1` (the `pow`/`log`/`sin`/`cos`
  digest, `2aaa73e7d4f0260f`, is the same). 1,054 of 19,756 fragments have new hashes, and in each of them only the
  text of hidden `tspan`s differs: `node tests/golden/textcopy.mjs new.tsv --old old.tsv` shows it (the markup with
  the hidden text taken out is byte-identical for every fragment).
* 32 fragments still copy a different character from the source: a sign drawn with another glyph (the PUA glyphs
  of an enclosure's delimiters) has the glyph's character in the `tspan` that is both what is seen and what is
  copied, and changing that would change the visible markup.
* API: `layout::render_consistent`, `print_em` and `write_text` take the source text.

### Speed (M5)

No output changes: compat is still zero differences from upstream, and consistent mode's golden digest is the same.

- **Compat.** `processFragmentsNow` batches on every page. In WebKit (and any engine not recognised as Blink or Gecko) it
  forces a layout after each `type=dom` fragment and after the one before it in the same block, which reproduces
  upstream's boxes there, instead of writing a whole call one fragment at a time as 0.x did when a `type=dom`
  fragment and a separated one were on the page. On the test suite in WebKit: 1,401 ms upstream, 844 ms now. One scratch
  canvas is reused for measuring and one for drawing. Against upstream, call plus layout: 0.23 to 0.78 on the
  measured pages in the three engines (`js/compat/README.md`).
- **Shipped builds have no test switches.** `window.HIEROXIDE_LEVERS`, the `hx` URL parameter, `forcebatch` and the
  profile hooks exist only in the build made with `tools/build-compat.mjs --test-levers`.
- **Consistent mode** is unchanged. Measured and not adopted: CSS containment on each fragment (slower in Chromium),
  `content-visibility` (no effect on inline-block fragments), one write per block of fragments (4 to 7% faster in
  Chromium and WebKit, a different API); layout in a Worker is possible as it is (main thread blocked for about 10 ms
  instead of 0.9 to 1.4 s for 5,333 fragments, `tests/bench/worker.mjs`).
- **Known, not explained:** in Firefox a `columns: 2` container holding `type=dom` and separated fragments is laid
  out by up to 0.6 px differently (boxes as left, in about 2% of random pages containing one) by the shim and by
  upstream; present before this change and not caused by batching.

### Consistent mode: what it does differently from upstream

Consistent mode is `Engine::with_mode(Mode::Consistent)` with `layout::render_consistent`, and in
JavaScript the `hieroxide` module. Compat keeps every behaviour below as upstream has it
(`PORT_MAP.md`, "Upstream's oddities, kept").

**Geometry**

| Upstream | Consistent |
|---|---|
| Layout and memo at the page's font size, from `Math.ceil`ed px boxes and `toFixed(2)` px numbers. | A virtual font size of 1000. `<svg width="3.2346em" height="1.07em" viewBox="0 0 3234.6 1070">`: the same markup at every font size. Numbers are on a grid of 1/10000 em (`layout/em.rs` says why). |
| `x` and `dx` printed separately, each rounded. | One `x`, added first, then rounded. |
| `scale(0.29 1.00)`: scales with two decimals. | Four decimals (`scale(0.2914 1)`). |
| A shading's edges rounded to px, so touching shadings have no seam. | Edges at their em values with `shape-rendering="crispEdges"` on the `svg`; no seam, and no rounding in the numbers. |
| Hatching with a 4 px period at any font size. | 200 units, so 1/5 em: 4 px at 20 px, 8 px at 40 px. It is the one look that had to change: a fixed px pattern cannot be size-independent. |
| `data-type=canvas` and `dom` print pixels and DOM. | Always SVG, for now. A canvas is pixels by nature, so the size-independent numbers cannot carry over to one: when the new API gets canvas output (M4c) it draws the same placed glyphs at `em * font size`, with no reference test across sizes. |
| The sign colour is the page's computed colour, written on each sign. | `fill="currentColor"` on the `svg`, and a `fill` per sign only for `data-signcolor`, so the markup does not depend on the page and follows later CSS changes. The bracket colour's default stays `black`. |

**Fixed: clearly bugs**

| Old | New |
|---|---|
| The memo keys `xScale` and `yScale` on two decimals and keeps the first value, so a fragment's output depends on what was laid out before it. | Keyed on the exact values. A shuffled run is byte-identical (`tests/golden`). |
| `document.getElementById` for a hatching pattern, so output depends on what is on the page. | Each `svg` defines the patterns it uses. |
| `getImageData` throws `IndexSizeError` for a canvas with a side of 0: an insertion into an empty enclosure, 116 fragments of the corpus, leaves the element empty and aborts `processFragments`. | The canvas reads as empty, and the fragment is laid out. |
| `TypeError` on `lig.horizontal` for U+13353's insertion alternative U+E49D, which has no ligature entry. | The glyph is drawn; there are just no shaded areas to work out. |
| `Lost.toString` writes U+FE00 for any variation selector, so selected text differs from the source. | The selector that was written (`Tree::to_string_exact`). |
| The parser reads an enclosure's end by its opening (U+1343F after U+1343C is U+1343D), so selected text differs from the source in 1,054 of the 19,636 fragments the corpus lays out. | The hidden text is the source's characters, taken in order (`render_consistent` and `write_text` take the source text, as path output does). |
| `Enclosure.print` adds an empty hidden `tspan` after each delimiter. | Left out. |
| `<rect width="-3">` for a shading with a negative size, an error in the console. | Left out. |
| `data-sep` negative or infinite, `data-linesize` likewise, an unknown `data-dir`: `RangeError`s, or NaN coordinates. `data-fontsize` that is not a positive number: a font size of 0 or NaN. | The default. |
| `alert` when the font does not load; exceptions from the core. | Errors are values: `LayoutError` in Rust, `{ ok: false, kind, message }` from `render`, and a rejected promise from `init`. |

`hPx + hPx` in `PrintedDOM.addShading` needs no fix here, as consistent mode has no DOM printer. It
stays in compat (M4a). `&` for `&&` in `Shapes.openRect` has no effect on any result, so
neither mode changes.

**Left as they are: merely unusual, and part of upstream's look**

- `measureGlyph` measures at 150 px with whole-pixel margins, and `fit` measures on canvases at 50
  px with `Math.round`. So layout has its steps of 1/150 and 1/50 em. They are not px of the
  page: the result does not depend on the font size. Removing them would change every fragment.
- `Math.round` of a canvas position in `PrintedCanvasWithoutExtras`, and `Math.floor` of its
  font size: the same, on the measuring canvases only.
- `getMin` and `getMax` over negative indices in hulls, the different stops of `distancesRightmost`
  and `distancesLeftmost`, the size of the last group computed twice: they change no result worth a
  risk, or none at all.
- `BracketOpen.format` with `Math.max` and `BracketClose.format` with `Math.min`, and a NaN ratio
  taking the `else` branch.
- A bracket's default colour of `black`, whatever the text's colour.
- `data-border`'s border of 1 px on each side, from `hierojax.css`: the one part of a fragment's box
  that does not scale with the font size. The markup is the same at every size.
- `this.reversed ^ mirror`, tested as raw `mirror` when choosing `text` or `tspan`: this decides what
  is selectable only, and upstream's choice is kept.
- `Literal.rotation` setting `logRotation` as a side effect: harmless with `log` off.
- Duplicate `id`s of hatching patterns across `svg`s on a page: they have the same content, so
  which one a browser uses does not matter, unless the first is in a `display: none` subtree.

### Path SVG, the public API and the command line

Consistent mode's text output is unchanged: the reference digest of `tests/golden` is the same, in
the engines and natively, and the drop-in's wasm is the same bytes as before.

**Path SVG** (`SvgKind::Paths`, `svg: 'paths'`, `hieroxide render --kind paths`, the default of the CLI). The
same placed signs as the text printer, drawn as `<path>`s from the font's outlines, so the markup needs
no font, no stylesheet and no script, works under `default-src 'self'; style-src 'self' 'unsafe-inline'`,
and has no `id`, `defs` or `use` (hatching is the bands of upstream's pattern, clipped to the shaded
rectangle by calculation), so any number of blocks fit on a page. Each sign is `translate(..) scale(..)
rotate(..) translate(..) scale(k -k)` with the numbers of the text printer, over its outline in font
units (relative commands, a tenth of a unit: it costs nothing in NewGardiner, whose coordinates are whole
but for 35). A block has the size in em and the `viewBox` of the text block, sits on the line as it does
(`vertical-align:-.1em`, which is the one use of an inline `style` besides `data-fontsize` and
`data-border`), and has `role="img"` and an `aria-label` (default `Egyptian hieroglyphs`).

| Text block | Path block |
|---|---|
| `<svg>` with `<text>` in the font, a class per sign; needs the font and `hierojax.css` | A block with no dependency, and more bytes: on the site's longest line (33 signs) 18,567 against 2,528 raw, 3,970 against 398 with brotli (`tests/paths/bytes.mjs`; 15,191 and 3,536 without the copyable text) |
| Hidden text: `font-size:0` tspans, a lost sign's selector written U+FE00 (upstream) or as written (consistent) | Copyable text as written, below |

**Copyable Unicode.** Every path block carries the fragment as it was given, in two ways. A selection of
the page copies it (`Selectable::Overlay`, the default): a `span` that wraps the `svg` and has, beside it,
an invisible box of HTML text over each sign (the sign and the format controls after it), in the order of
the text. Nothing is drawn from it, so no font is needed; selected, it highlights the signs. A script reads
it from `data-text` (on the `svg`; with `separated`, on the `span` that holds the `svg`s). Measured against the
source, not against upstream: exact in Chromium, Firefox and WebKit, for the block, for the paragraph that
holds it, and through the real clipboard, including the 1,054 corpus fragments whose closing enclosure
the tree writes differently (the tree normalizes U+1343F after a U+1343C to U+1343D; the printer takes the
characters of the copy from the source) and the lost signs with their own selectors. The other forms:
`Selectable::Svg` keeps the block a bare `<svg>` with SVG text in it, and then Chromium and WebKit put a
line break before and after the text of a copy that takes in the whole block (Firefox does not) and a
mouse cannot start a selection on it; `Selectable::Off`.

**The API.** `crates/hieroxide-api` (Rust, 0.x) and `js/hieroxide` (`index.d.ts`). Errors are values with
kinds (`font`, `parse` with a position in UTF-16 units, `layout`, `unsupported`). In the module: `parse`
is new, `layout` and `render` take the options of the block (`svg`, `label`, `selectable`, `datatext`,
`xmlns`), a failure has a `kind` and for a parse error a `position`, `init` rejects with an error whose
`kind` is `'font'`, and the `text` of a result is the text as it was given (it was the tree's rewriting of
it: the same but for the enclosure ends). `svg: 'paths'` is not in the default browser build, which stays
under 100,000 bytes brotli (99,755 against 99,164 before); `build/consistent-paths` has it (108,747, from
the Cargo feature `paths`).

**The command line.** `hieroxide render`, one block or JSON lines in and out (README).

### The outline raster

`crates/hieroxide-outline`: glyph pixels from the font's outlines, native and wasm, deterministic.
Fitted to Chromium 158's canvas on an Apple-silicon Mac; `Tuning::FROZEN` has the constants and
`tests/golden/README.md` how they were found. "Identical to upstream" is true only against that
target: upstream on Linux differs from upstream on the Mac in about 97% of fragments, so it differs
from this about as much.
