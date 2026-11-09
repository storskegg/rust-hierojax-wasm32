# Compat: a drop-in for hierojax's rendering API

`hierojax.js` here replaces `docs/hierojax.js` of upstream (nederhof/hierojax at `da31880`)
in a page that renders fragments: the same `<span class="hierojax">` elements, the same
`data-*` options, the same output in the same browser (SVG, DOM and canvas, separated or not,
standalone SVG), zero tolerance. The layout is the Rust port in `crates/hieroxide-core`; the
canvas is the page's own (`measureText`, `fillText`, `getImageData`), so the numbers are the
browser's, as upstream's are.

```sh
node tools/build-compat.mjs                      # build/compat/: hierojax.js + hieroxide_wasm_bg.wasm
node tools/build-compat.mjs --embed --out build/embedded    # build/embedded/hierojax.js, one file
```

Use `hierojax.js` where `docs/hierojax.js` was, with `hierojax.css` and `NewGardiner.otf` as before.
The default build also needs `hieroxide_wasm_bg.wasm` next to the script, served as
`application/wasm`.

## The API

- `hierojax`: `startLoadingFonts()`, `waitForFonts(f, c)`, `processFragments()`,
  `processFragment(elem)`, `processFragmentsIn(elem)`, `processFragmentsNow()`,
  `processFragmentNow(elem)`, and the fields `fonts`, `nFonts`, `nFontsLoaded`. Also `ready`, a
  promise that settles when the font (and, in the default build, the module) is in.
- `syntax.parse(txt)`: throws an `Error` for text that is not a fragment; otherwise an object
  with `toString()` and `print(element, options)`. `print` normalizes `options` in place as
  upstream's `Fragment.print` does and writes into `element` without clearing it.
- `PrintedSVG.EXTERNAL_FONT` (an object with that one property; the class is not offered), read
  when a standalone SVG is made.
- The globals of `util.js`: `$`, `removeChildren`, `getMin`, `getMax`, `getSum`, `isLinux`.
  `type=dom` asks `isLinux()` for `line-height` against `height`, as upstream does.
- Not offered: `Shapes.*`, `Group.*` and the other classes of `formatting.js`. Pages that use
  them (`insertionlist.html`, `ligaturelist.html`, `rotationlist.html`,
  `rotationliststandard.html`) do not work with this build.

## `syntax.parse` before the module is ready

The module is WebAssembly, and a browser compiles it asynchronously unless it is inside the
script. Two builds:

| | default (separate `.wasm`) | `--embed` (one file) |
|---|---|---|
| `syntax.parse` right after the script | throws an `Error` that says to wait for `hierojax.ready` | works, as upstream's |
| download, brotli (M5: minified script, our own number formatting) | script 5.8 KB + module 65.9 KB = 71.7 KB | 97.5 KB (+25.8 KB: base64 does not compress as binary does) |
| script evaluation, Chromium / Firefox / WebKit, this Mac | 1.5 / 2 / 1 ms | 5.6 / 8 / 5 ms |
| of which the module (decode, compile, instantiate) | async, ready after about 2.5 / 5 / 3 ms | 3.6 / 5 / 4 ms, on the main thread |
| Chromium, 4x CPU throttle | 6.5 ms | 17.5 ms |

(Medians of 7 fresh loads, `tests/bench/embed.mjs`, load average 6 to 7 from other builds on the
same machine, so read them as upper bounds. Upstream's own script evaluates in 52 / 35 / 44 ms.) The embedded build was meant to be the default if
it cost under about 5 KB of brotli and under about 5 ms to start; it costs six times the first
and 1 to 1.6 times the second, so the default is the separate module and the single file stays
a build option.

What it means for upstream's own pages: `docs/testsuite.js` calls `syntax.parse` for each demo
line at `DOMContentLoaded` (`testParsing`), before the module is there in the default build.
The exception stops the page's script, so its demo lines are not expanded and nothing is
rendered. That page needs the embedded build, or a script of its own that waits for
`hierojax.ready` first. The other pages of `docs/` never call `syntax.parse` themselves, and
work with either build. (The real-page parity run uses the embedded build for the pages that do.)

## Site options

Off unless set, before the script, as `window.HIEROJAX_CONFIG = {...}`; they are the local
changes of the site's patched `hierojax.js`:

| option | effect |
|---|---|
| `fontUrl` | the font's URL, for the `FontFace` and for `PrintedSVG.EXTERNAL_FONT` (default `NewGardiner.otf`) |
| `fontFormat` | the `format()` the style of a standalone SVG names for it (default `opentype`) |
| `svgScalable` | every SVG gets `viewBox="0 0 width height"` and, if the font size is above 0, a width and height in em |
| `autoProcess` | `hierojax.processFragments()` at `DOMContentLoaded` |
| `noAlert` | a font that does not load is logged with `console.error` instead of `alert` (after the same wait) |

The hidden, selectable text stays in all SVG output.

## What differs from upstream, on purpose

- **A parse error.** The red span, and the log line through `console.log`, are the same up to
  and including `Error: `; the text after it differs, because it is the wording of a different
  parser (jison names what it expected; this one names a position). Ruled on by the project:
  the test harness compares everything else of a logged parse error, including the fragment's
  text, and not what follows `Error: `.
- **Nesting.** More than 256 levels of enclosures and bracketed segments inside each other is a
  parse error. Upstream lays out 257 or 1,000 levels (slowly) and overflows its stack in
  `toString` at a few thousand. No real text nests that deep, and the limit keeps the recursion
  inside WebAssembly's stack. This is the one entry on the parity allowlist; `test/depth.test.mjs`
  pins both sides.
- **`syntax.parse` before the module is ready** (above).
- **Markup, not DOM calls.** SVG and `type=dom` are written with `insertAdjacentHTML`, one string
  per fragment, with the declarations `style.setProperty` would have refused left out (so
  that an element which upstream gives no `style` attribute has none). A page whose
  Content-Security-Policy forbids inline `style` attributes in markup (`style-src` without
  `'unsafe-inline'`) would drop them; `type=canvas` is not affected, since it makes the same
  calls as upstream. WebAssembly needs `'wasm-unsafe-eval'` in a CSP that has `script-src`.
- **Batching.** `processFragmentsNow` reads every element, lays all of them out, then writes all, so
  the page's style and layout work happens once (`tests/README.md`). Output is the same; a script
  that changes the page between two fragments of one call would see a difference. One engine
  needs more, from a measured effect (below): in WebKit, and in any engine that is not recognised
  as Blink or Gecko (every browser on iOS is WebKit, whatever its name), the shim forces a layout
  after each `type=dom` fragment (a `data-type` other than `canvas` or `svg`) and after the fragment
  before it in the same block, where upstream's one-at-a-time writes have one.
- **Why WebKit, and how that was found.** WebKit lays out upstream's one-at-a-time writes
  incrementally, and on a page with `type=dom` fragments the boxes it leaves are not what a fresh
  layout of the same DOM gives (a `display: none` and back changes them, nothing else does; on
  `testsuite.html` the boxes are off by up to 2,934 px further down the page, 947 of 959 of them).
  Upstream's boxes are those, so parity needs them. What decides them is the sequence of layouts
  between the writes, not the DOM that results: a batch with no forced layout gives the fresh boxes
  on a page that has a separated fragment next to the `type=dom` one, and the stale ones on a page
  that has not (a page with no `type=dom` fragment has no difference at all). Which layouts matter was
  found on random mixed pages (`tests/parity/mixedpages.mjs`, seeded; each page is laid out by upstream
  and by the shim and compared in DOM, canvas pixels, boxes as left and boxes after a fresh layout): a
  layout after every write is exact, after every `type=dom` fragment alone it is not (8 of 40
  pages), after every write up to the last `type=dom` fragment of a block it is, and after each
  `type=dom` fragment and the write before it only, it is: 0 differences in 550 pages and 7,367
  fragments in WebKit, against 19 of 40 pages different with no forced layout. A block is the nearest
  ancestor that is not an inline box (the lines of different blocks do not affect each other). Only a
  block that has a `type=dom` fragment gets any forced layout, so a page with none is batched whole,
  and on the test suite WebKit goes from 1,400 ms (upstream) to 840 ms (it was 1.16 to 1.4 times
  upstream with the earlier rule, which wrote the whole call one at a time).
- **Chromium and Firefox batch.** Over the same random pages a batch has no difference in Chromium (550
  pages, 7,257 fragments) and none in Firefox (550 pages), and upstream's own boxes there are the
  fresh ones. The exception is a multi-column container in Firefox (`columns: 2`), where the boxes as
  left differ from upstream's by up to 0.6 px in about 2% of pages (the DOM, the canvases and the boxes after a
  fresh layout are the same): the shim as shipped, the shim before this change, which wrote that page
  one fragment at a time, and a forced layout after every write all show it, so it is not
  batching, but it is a difference from upstream that this port has not explained (upstream
  differs from itself in 1 page of 300 there too). `mixedpages.mjs` leaves such containers out unless
  asked (`--multicol`).
- **What was tested, and what was not.** Playwright's WebKit (build 2388 on macOS), Chromium and
  Firefox. Not real Safari: the iOS simulator is not installed on the machine this was done on, and
  desktop Safari cannot be driven by the harness. Playwright's WebKit is the engine's own code, but
  its layout could differ from a release of Safari, so the engine test is by what a browser has,
  not by its name: Gecko by `MozAppearance` in the style object, Blink by `navigator.userAgentData`
  or `window.chrome` together with `Chrome/` in the user agent, and everything else is taken for
  WebKit, which only costs speed when it is wrong. A page on plain http that is not `localhost` has no
  `userAgentData`; Chromium is still found by `window.chrome` and its user agent.
- **Waiting for the font** uses a promise rather than polling (`fontwait`): the work starts as
  soon as the font is in, where upstream looks again after its next timer. The alert for a font
  that never comes is at the same time as upstream's.
- **`startLoadingFonts()`** loads the font again as upstream's does and does not load the module again.

## Oddities of upstream that are kept

The port keeps what upstream does, including what looks like a mistake, because the output is
what is compared: `hPx + hPx < this.hPx-1` in `PrintedDOM.addShading` (twice the shade's
height against the unit's height, not its bottom edge); `isLinux()` choosing `line-height` or
`height`; `PrintedCanvas.width()` being the canvas's width as assigned (so the offsets of
separated canvases follow the browser's clamping) but `PrintedDOM.width()` the unrounded pixel
width; `-xScaleStr` turning the text `"1.00"` into `-1`; the SVG test `if (rotate || mirror || ...)`
using the raw `mirror` where the transform uses `reversed ^ mirror`; the hatching of a canvas
looping over every pixel of the shade. See `PORT_MAP.md` for each against its JavaScript.

## Colours

A colour from `data-signcolor`, `data-bracketcolor` or the page's computed `color` is passed to
`type=dom` as part of a `style` declaration only if it holds letters, digits, spaces and
`# ( ) , . % + -`; any other value (a semicolon, a quote, a comment, `!important`) is set
afterwards with `style.setProperty`, as upstream does, so it cannot end the declaration.
