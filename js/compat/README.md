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
- **Batching.** `processFragmentsNow` reads every element, lays all of them out, then writes all
  (`batch`, see `tests/README.md`), so the page's style and layout work happens once. Output is the
  same; a script that changes the page between two fragments of one call would see a difference.
  One exception, from a measured effect in WebKit: upstream's one-at-a-time writes are laid out
  incrementally, and on a page that has a `type=dom` fragment (a `data-type` other than `canvas`
  or `svg`) together with a separated one (`testsuite.html` is such a page) the boxes of the
  fragments are not what a fresh layout of the same DOM gives, and a batched write gives the
  fresh one (off by up to 2,934 px further down that page; screenshots differ). Upstream's boxes
  do not move on their own, nor on a scroll, a resize or a wait; only a forced fresh layout
  (`display: none` and back) changes them. So `processFragmentsNow` does not batch a call whose
  fragments include both kinds, which it can tell from the attributes it has already read: it
  writes them one at a time as `processFragmentNow` does, and the layout is upstream's, in every
  engine and without looking at the browser. Every other page keeps the batching. The lever
  `forcebatch` turns the exception off, for the tests of batching (`tests/README.md`).
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
