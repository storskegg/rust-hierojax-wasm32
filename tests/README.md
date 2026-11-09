# Tests

Everything here needs Node 24. The browser tests use Playwright 1.65.0 (pinned in
`parity/package.json`) with the browsers it caches in `~/Library/Caches/ms-playwright`;
nothing downloads browsers for you locally (`PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 npm ci`).
`docs/` and `src/` are upstream's and are only ever read.

## Parity harness: `tests/parity/`

Runs a script (upstream's `docs/hierojax.js`, or any drop-in) over a fixed corpus in a real
browser and records what it produced; `compare.mjs` then says whether two runs agree.

```sh
cd tests/parity
npm ci
npm test                          # canonicalizer and allowlist tests, in all three engines

node corpus/build.mjs             # writes corpus/corpus.json (git-ignored), ~19.8k fragments
node corpus/build.mjs --check     # rebuild and compare with the committed corpus/manifest.json
node corpus/build.mjs --update    # after an intended change, rewrite the manifest

node run.mjs --engine chromium --run-id A
node run.mjs --engine firefox  --run-id S --shuffle 1
node run.mjs --engine firefox  --run-id H --prefs '{"privacy.resistFingerprinting":true}'
node run.mjs --engine webkit   --impl ../../build/compat --run-id wasm     # a replacement
node run.mjs --engine webkit   --impl ../../build/compat --config '{"svgScalable":true}'   # with site options
node compare.mjs out/upstream-chromium-A.json.gz out/<other>.json.gz       # exit 1 on a difference

node selfcheck.mjs [--engines chromium,firefox] [--hardened] [--no-run]    # the gate, below
```

- **Corpus.** Fragments come with their real `data-*` attributes and the computed font size
  and colour of their context: `demo.html`, `testsuite.html` (its `<hierojax-demo>` elements
  expanded by upstream's own script into the 10 variants each), `interactive.html`'s defaults,
  every `docs/texts/*.html`, the site's two lines, every sign in the font, rotations and
  mirror on a sample, every entry of `Shapes.insertions`, `.ligatures` and `.rotations`, each
  variation selector, an options matrix (pairwise plus one factor at a time), three font
  sizes (22, 40 and 33.6), and 5,000 fragments generated from `src/syntax.jison` (fixed
  seed, kept only if upstream's parser accepts them). The class of a fragment (plain,
  insertion, enclosure, overlay, bracket, options) and its group are in each item;
  `corpus/manifest.json` records the count and a hash per group.
- **Run.** `--impl upstream` (default) serves `docs/hierojax.js`. `--impl <dir>` serves
  `<dir>/hierojax.js` in its place (and any other file in `<dir>`, such as a `.wasm`), and
  `--impl <file.js>` a single script. The page template (`/parity.html`), stylesheet and font
  are always `docs/`'s. Fragments run in batches of 200, each batch in a fresh browser context
  (so upstream's global measurement memo starts empty per batch); a batch that crashes is
  retried, then run fragment by fragment. `--ready 'expr'` replaces the font-ready test
  (`hierojax.nFontsLoaded == hierojax.nFonts`) if a replacement exposes something else.
  Per fragment it records: the canonical output (`html`), each canvas's size and a pixel hash
  (`getImageData`), bounding boxes, `console` output (parse errors are `console.log`),
  `alert`s, any exception, the text a selection of the element gives, and its `textContent`.
  Results go to `out/<impl>-<engine>-<run id>.json.gz`.
- **Canonicalizer** (`lib/canon.mjs`). Runs in the page, in the engine that produced the DOM:
  attributes sorted, `style` read back through the CSSOM, adjacent text nodes merged, one node
  per line. Firefox serializes attributes in a different order, so raw `outerHTML` is not
  comparable.
- **Compare.** Per class and per group: identical share, largest numeric difference, which
  fields differ, and a readable diff of the first few. Exit 1 unless every difference is on
  `allowlist.json` (a list of `{id | group | class, engine?, fields?, reason}`; `reason` is
  required; it starts empty).
- **Gate: `selfcheck.mjs`.** Upstream against upstream: two fresh runs in corpus order must show
  0 differences in each engine. A third run in a seeded shuffled order is compared and
  reported but not gated: upstream's memo keeps the first value for a key rounded to two
  decimals, so output can depend on order. `--hardened` adds Firefox with
  `privacy.resistFingerprinting=true`, compared with itself and with normal Firefox, recorded
  and not gated. Full run: about 3 to 4 minutes per engine on an M-series Mac, and the whole thing with
`--hardened` takes about 15.
- **Gate for the port (the compat build).** `node ../../tools/build-compat.mjs` builds
  `build/compat/` (the wasm and its shim); then run it as above with `--impl ../../build/compat`
  and compare, with the one ruling in the plan about text (the port's parse error message
  differs from jison's):

  ```sh
  node run.mjs --engine chromium                                  --run-id A
  node run.mjs --engine chromium --impl ../../build/compat        --run-id W
  node compare.mjs out/upstream-chromium-A.json.gz out/compat-chromium-W.json.gz --ignore-parse-error-text
  ```

  Nothing is unsupported any more (canvas, DOM and every option are ported), so there is no
  `--covered-only` in the gate: every one of the 19,756 fragments is compared, including each
  canvas's pixels, with the allowlist below. `--ignore-parse-error-text` leaves out what follows
  `Error: ` in a logged parse error; everything else in it, the fragment's text included, is
  compared. A fragment the port still declines (a `data-sep` so large that upstream would never
  finish) comes back with `unsupported: <reason>`; there is none in the corpus.
- **Shuffled, port against upstream.** Upstream's output depends on the order (its memo), so the
  same seed must give the same output in both: `--shuffle 7` on both sides, full corpus, no
  `--covered-by`.
- **Allowlist.** One entry, group `depth`: the parser's limit of 256 levels of nesting. Upstream
  lays out 257 and 1,000 levels of enclosures (slowly) and overflows its own stack in `toString`
  at a few thousand (in Chromium; Firefox and WebKit have deeper stacks); the port gives a parse
  error. No real text nests that deep. `test/depth.test.mjs` pins what each side does at 256,
  257, 1,000 and 3,000 in every engine, and the group is in the stress corpus.
- **Stress corpus.** `node corpus/stress.mjs` writes `out/stress.json`: fragments that use group
  pixels under options outside any sensible range (negative `data-sep`, font sizes 0, NaN and
  negative, an unknown `data-dir`), in all three types and with odd colours (a semicolon, a
  comment, `!important`), every overlay ligature with its insertion places, empty enclosures
  everywhere, deep nesting, very wide canvases, and the `depth` group. Upstream's own code throws
  `RangeError`s and `TypeError`s here. Run both with `--corpus out/stress.json`, then compare as
  usual. Not in the manifest.
- **Real pages: `realpages.mjs`.** Every page of `docs/` that loads `hierojax.js`, as it stands,
  once with upstream and once with the replacement swapped in through the server. The test calls
  nothing: the page does what it does at `DOMContentLoaded`, in the order and state of a real
  load. When the page has been quiet for 1.5 s the whole document is compared: canonical DOM, the
  pixels of every canvas, the bounding boxes of the fragments, the visible text, console, alerts
  and exceptions. `interactive.html` is also driven as a visitor does it: each type with two
  directions, with the colours and the hatching changed. Pages that use internals (`Shapes.*`,
  `Group.*`: `insertionlist`, `ligaturelist`, `rotationlist`, `rotationliststandard`) are listed as
  out of scope, with the reason. `testsuite.html` calls `syntax.parse` at `DOMContentLoaded`, so it
  is run with the build that has the module inside the script (`--embedded`); with the default
  build it fails by design (`js/compat/README.md`). The boxes of the fragments are compared as the
  page left them (that is what a visitor sees) and again after a fresh layout of the same DOM.
  Both must be equal, in every engine, with no `--fresh-layout`. In WebKit upstream's incremental
  one-fragment-at-a-time writes leave boxes that a fresh layout does not give, on a page with a
  `type=dom` fragment, and the shim reproduces them by forcing a layout around each such fragment
  (`js/compat/README.md`), so these pages are exact. Because `testsuite.html`, `pagecheck`, `batchedge`
  and the corpus page have `type=dom` fragments, they are also run with the lever `forcebatch`
  (`--levers batch,fontwait,forcebatch`, the test build, below), which leaves that out: there the boxes as
  left differ in WebKit (`--fresh-layout` or `--ignore-boxes` leaves them out; 947 of 959 on the test suite)
  and nothing else may. `batchedge` has a page of its own for the trigger, whose boxes it compares in both runs.

  ```sh
  node ../../tools/build-compat.mjs --embed --out ../../build/embedded
  node realpages.mjs --impl ../../build/compat --embedded ../../build/embedded [--pages demo.html,...]
  node realpages.mjs --impl ../../build/compat --corpus-page     # the corpus in one page, one processFragments()
  ```

  `--corpus-page` puts every corpus fragment that upstream lays out without throwing in one
  document, in corpus order, and calls `hierojax.processFragments()` once, which is the call that
  batches (the per-fragment harness calls `processFragmentNow`, which does not). It compares each
  fragment's canonical DOM, canvas pixels and box, and the log.
- **Site options.** The compat build with the site's options (`HIEROJAX_CONFIG`: `fontUrl`,
  `fontFormat`, `svgScalable`, `autoProcess`) against the site's own patched `src/hierojax.js`,
  read from that repository when the test runs (it is not copied here):

  ```sh
  SITE=/path/to/the/site/src/hierojax.js
  CFG='{"fontUrl":"/NewGardiner.woff2","fontFormat":"woff2","svgScalable":true,"autoProcess":true}'
  node run.mjs --engine chromium --impl $SITE --run-id site --config "$CFG" --out out/site-chromium.json.gz
  node run.mjs --engine chromium --impl ../../build/compat --run-id site --config "$CFG" --out out/compat-site-chromium.json.gz
  node compare.mjs out/site-chromium.json.gz out/compat-site-chromium.json.gz --ignore-parse-error-text
  node realpages.mjs --impl ../../build/compat --embedded ../../build/embedded --site $SITE
  ```

  (`--config` is given to both runs; the site's script ignores it. The server answers
  `/NewGardiner.woff2` with the bytes of `docs/NewGardiner.otf` for both.)
- **Linux.** `.github/workflows/parity.yml` is `workflow_dispatch` only. Its `impl` input is
  `upstream` (the baseline for a platform comparison), `compat` or `both`. With `both` it builds the
  compat drop-in, runs both in the chosen engines (run id `linux`), compares them with
  `compare.mjs --ignore-parse-error-text`, runs `realpages.mjs` (pages, and the corpus page),
  `pagecheck.mjs` and `batchedge.mjs`, and fails on a difference. The artifact (kept for 7 days)
  holds `out/*.json.gz`. After downloading it into `linux/`:
  `node compare.mjs --mode linux-vs-mac out/ linux/ --a-run A --b-run linux`
  (a report; it never fails).

## Consistent mode: `tests/golden/`

Reference files of consistent mode, compared byte for byte in each engine and natively, and the
drift against upstream: see `tests/golden/README.md`.

## Path blocks, the API and the command line

Native, no browser (the corpus is the golden one, 19,756 fragments, 19,636 of which parse):

```sh
cargo test -p hieroxide-api      # api.rs, corpus.rs, outlines.rs; about 30 s
cargo test -p hieroxide-cli      # cli.rs: golden bytes, batch mode, errors; with the unit tests of the JSON reader
```

- `tests/api/corpus.rs`. The text printer through the API is `tests/golden/hashes.txt.gz`, all 19,756. For
  each fragment, in each way of carrying the copyable text, the path block places every sign by the numbers of the
  text block (read from the two markups: the `x`, `y`, `font-size` and `transform` of the text, and the
  `transform` chain of the path: 195,614 signs), with the same `width`, `height` and `viewBox`; what a selection
  copies and `data-text` are the source, character for character (this includes the 1,054 fragments whose
  enclosure ends the tree writes otherwise); no block has an `id`, `defs`, `use`, `url(`, `href`, `style`
  element, script, image or `class`; output in a shuffled order is the same bytes.
- `tests/api/outlines.rs`. The path data of every glyph of the font (1,120) read back, by a reader of the
  path grammar written for the test, is the outline to a tenth of a unit. It found a real bug (an absolute
  `M` makes the implied linetos absolute).
- `tests/api/api.rs`. Errors are values: bad fonts (prefixes and bit flips of the real one), bad text with its
  position, numbers that cannot be used. The data: `glyphs()` puts the outline's box where layout put the sign
  (4,182 signs; at most 0.105 em off, 19 further than 0.02 em), roles, mirroring, units, `Send`.
- `tests/cli/cli.rs` and `tests/cli/golden/`. Golden bytes of sixteen blocks (each also from stdin, and a second
  run; five of them `--contents`), the environment variable, batch mode (golden `batch.out`: ids, a parse error,
  option errors, bad JSON, an empty line, an escaped label), the exit status and message of every error. A change of
  output is a versioned change: `HIEROXIDE_CLI_UPDATE=1 cargo test -p hieroxide-cli --test cli`.
  `hieroxide fill` (golden `fill.html`): a page with a comment, `script`, `style`, `textarea` and `title` that hold
  spans, a quoted `>` in attributes, CRLF and tabs, and three spans (one with character references and `data-*`
  options, one with white space around its text, one separated) comes out as the input with exactly the contents of
  the three spans replaced, byte for byte; a second run gives the same bytes; options of the span win over the command
  line's and `type` and `standalone` are ignored; `-o`, `-` and in-place; every error (a fragment that does not parse,
  an unknown named reference, a comment, tag, quote, `script` or span never closed, a self-closing span, an end tag
  that does not match, a span in a span, a file that is not UTF-8, markup with no `data-text`, a second run after
  `--no-data-text`) names its line, writes nothing and exits 1; all the failing spans are named. The scanner's own
  unit tests are in `crates/hieroxide-cli/src/fill.rs` (`cargo test -p hieroxide-cli --bin hieroxide`).

In the browsers (`tests/paths/`; needs `cargo build --release -p hieroxide-cli` and the `tests/parity` install;
the first three also the builds of `tools/build-consistent.mjs`):

```sh
node tests/paths/page.mjs        # a page with no script, no font, no stylesheet and the CSP header: 1,020 blocks
node tests/paths/pixels.mjs      # path blocks against consistent mode's text blocks, as pixels
node tests/paths/selection.mjs   # selecting and copying, against the source; screenshots in tests/paths/out/selection
node tests/paths/wasm.mjs        # the browser module against the CLI, byte for byte (needs both builds)
node tests/paths/bytes.mjs       # bytes per block, raw, gzip and brotli
node tests/paths/host.mjs        # contents blocks inside a span with the page's own CSS, in eight shapes of page
node tests/paths/rescript.mjs    # what hierojax's scripts do to a filled page (needs build/compat, consistent-bench, consistent-paths)
```

- `page.mjs`. No CSP violation (Playwright adds a listener for `securitypolicyviolation`, which the policy does not
  bar; a control page that does break it shows the listener sees one), no console message, no failed request, no
  request but the page's own, no element with an `id`, no reference or script or style in any block, a named
  `role="img"` for each `data-text`, every `svg` the size in em that it says, and the block on the line where the text
  block is (its bottom 0.1 em below the baseline), as one `svg` and as its wrapper.
- `pixels.mjs`. A font is drawn by the engine with its own anti-aliasing, hinting and (on a Mac) smoothing, so
  path and text cannot be the same pixels. The measure is of where the ink is: a *mark* is a pixel with ink above 0.25;
  the *mismatch* of a pair is the marks of either block that have no mark of the other within 1 CSS px, over all
  marks of both, at a device scale of 2 and 60 px text. Anti-aliasing and hinting move an edge by under a pixel, a
  sign in the wrong place or a missing stroke does not. A pair fails above 0.08, or when the path's ink is under 0.7 or
  over 1.25 of the text's. The measured worst of 300 pairs (every class, `hrl`, `vlr`, separated, border, both
  shadings, rotations and mirror) is 0.032, and the median ink of a path block is 0.88 of the text's
  (smoothing thickens text), so the limit is 2.5 times the worst. The worst pair of each class, side by side, is
  in `tests/paths/out/pixels`.
- `selection.mjs`. Per engine, fixture and kind: the text a selection of the block gives, of its paragraph, what the
  real clipboard pastes into a textarea (only with `--clipboard`, which replaces your clipboard; otherwise skipped, and the output says so), and what a mouse drag from a sign near the
  start to one near the end selects. Exact for the overlay everywhere (exit 1 otherwise); reported for the other
  kind. Playwright's WebKit does not paint a selection in screenshots, whatever the page (a plain paragraph
  too), so its highlight cannot be shown: the ranges are real.
- `wasm.mjs`. 610 fragments of the corpus: the module's path and text markup is the CLI's, byte for byte, in
  each engine; so is `contents: true`, and `render` of it into an element leaves the element's class and attributes
  alone; the default build's `svg: 'paths'` is the error `unsupported`; errors have kinds and positions.
- `host.mjs`. The `--contents` block in a `<span class="hierojax" data-type="svg" data-standalone="true">` under the
  page's own CSS: upstream's `docs/hierojax.css` (`upstream`, and with `svg { display: block }`), no CSS at all
  (`no-css`), a fixed box that centres the svg (`display: inline-flex; justify-content: center; align-items: center;
  contain: layout; width; height` with `svg { display: block; flex: none }`) bigger than, smaller than and as big as
  the svg, and `dir="rtl"` with the upstream rules and with the box; each with a line, a column (`vlr`), `hrl`, a
  font size of the block's own, and (where the span has no box) a separated line; 34 pages per engine, with the
  CSP header, and a control page without the generic `font-family`, which asks for the font. Per page: nothing
  between the span and the svg (its children are the text and the svg: zero elements between; separated: the one
  classless layout span); the span's class and attributes are as written; the svg is its em at its font size; the
  box of the invisible text against the svg's (px) and each sign's box over the ink of a sign; the text a
  selection of the span, of its paragraph, and a mouse drag gives, against the source (never the clipboard); no CSP
  violation, no console message, and no request for the font that the invisible text did not already cause (a
  page that names the font for an element with an inline svg asks for it by itself, for the line's strut: the page
  without the text is the baseline); and the page with the same block and no invisible text (`selectable: none`)
  has the same boxes, to 0.01 px, for the span, the svg, the paragraph and the text after it. Exit 1 on a failure;
  the numbers are in `tests/paths/out/host/results.json`, a picture of each line per engine beside it.
  What it found: the text lands on the svg to 0.02 px (across, and in size) in every page, and down in every page
  where the svg is a block or a flex item (the site's shape, and any `svg { display: block }`). Where the svg is an
  inline box in a line (`upstream`, `no-css`, `rtl`), the static position of the text is the top of the *line*, which is
  above the svg's top when the line is taller above the baseline than the svg is: 5.2 px for a 1.07 em svg in
  40 px text with the font named (the font's strut), 15.9 px for the same svg at 30 px, 8.9 px (Chromium, WebKit) or
  5.9 px (Firefox) with no CSS and the 30 px svg, and 0 when the svg is taller than the line's strut. Copying is exact
  there; only the highlight and the drag's boxes sit too high. The report says so and does not gate it (`inlineSvg` in
  the file). Firefox puts a line break where a selection takes in a block-level svg (`svg { display: block }`) whether
  or not the page has our text (the page without it shows the same), so there a trailing break is accepted when the
  page alone gives one. Upstream's own output in Firefox has no such break, and in WebKit it has one that ours has not.

### What a script does to a filled page

A page that keeps `class="hierojax"` and still loads a hierojax script gives the script spans that hold a block, not
text. Upstream's `processFragmentNow` reads `elem.innerText`, empties the element and parses what it read; the
consistent drop-in does the same. Compat does too, except that it reads the `data-text` of a descendant when there is
one (`readFragment`), so it redraws a filled block; upstream never writes `data-text`, so nothing upstream can receive
is read differently (`tests/parity/test/datatext.test.mjs` pins that a span with no `data-text` descendant gives
upstream's output, in all three engines). `rescript.mjs` runs each of them on a span filled
three ways (`--selectable` overlay, svg, none), against a baseline of a span with the source in it, in the three
engines, and prints what `innerText` was and what the span holds after. Measured (Playwright 1.65.0):

| Span filled with | `innerText` of the span | upstream | the drop-in stand-in | `render(el, el.innerText, ...)` of the module |
|---|---|---|---|---|
| overlay | Chromium, Firefox: the source with a line break between sign boxes (the boxes are positioned, so block-level); WebKit: the source | Chromium, Firefox: the red "parse error; see log" (the text, with its line breaks, is in the console log); WebKit: redrawn, as if never filled | Chromium, Firefox: wiped (it leaves the element empty on a parse error); WebKit: redrawn | the block is kept (errors are values: the element is left as it was); WebKit: the same bytes again |
| svg | the source, in all three | redrawn, markup identical to a span that never had a block | the same | the same |
| none | the empty string | the empty fragment parses: the block is replaced by a drawing of nothing, in all three | the same | the same |

The table is for upstream's script (compat redraws every kind in every engine, which `rescript.mjs` gates). So an
overlay or `none` block is lost or broken by upstream's script run on it (a loud red message in Chromium and Firefox,
silent for `none`), and an `svg` block survives because the text inside an `svg` is what
`innerText` reads. Upstream's script cannot be changed by us, and a filled page should not run it: the way to a correct page is the one
`fill` is for, no script on it. A page that must keep a script has two ways out: use compat, or read the text from
`data-text` and not from `innerText` (`render(el, el.dataset.text ?? el.querySelector('[data-text]').dataset.text, ...)`).
The guard cost 21 B of brotli in compat (71,727 to 71,748 B). `--compat DIR` runs the script with another build of it.

**Selection boxes where the svg is inline.** With `--contents` the invisible text sits at the static position the
svg takes, which is exact when the svg is a block or a flex item and too high by the line's excess when it is an inline
box in a taller line (`host.mjs`). A page with upstream's CSS (an inline-block `.hierojax`) can add
`.hierojax svg { display: block }` to get exact boxes. Do not use it on an inline host that has no `display` of its own
(a bare `<span class="hierojax">` in a paragraph): the svg then becomes a block inside an inline box and breaks the
line. The block rule is the page's choice; there is no second contents mode.

```sh
node tools/size-check.mjs --build --paths                                                                      # sizes, below
(cd tests/types && npm install && npm test)                                                                    # index.d.ts
```

`tests/types` compiles `types.test.ts` against `js/hieroxide/index.d.ts` (what the types promise and what they
refuse, with `@ts-expect-error`) and checks that the names the `.d.ts` declares are those `index.js` exports.

## Benchmarks: `tests/bench/`

```sh
cd tests/bench
node run.mjs --engine chromium            # prints, writes out/<engine>-<platform>.json
node run.mjs --engine firefox --record    # writes baselines/<engine>-mac-arm64.json
node run.mjs --engine webkit --impl <dir> # any implementation, same pages
```

Median of 5 fresh loads of `texts/PBM10474.html` and `testsuite.html`: script parse+eval,
font load, wait-to-start, `processFragmentsNow` total and its split (parse, format+fit,
print, other, canvas operations), counts, per-fragment p50/p95. Chromium also with a 4x CPU
throttle. Committed baselines are in `baselines/`. Timing is not in CI. It reuses
`tests/parity/lib` (so install there first).

### The test build, and the levers of the compat shim

The shipped `build/compat` has no switches: `processFragmentsNow` batches, the wait for the font is a promise,
the two scratch canvases are reused, and no variable or URL parameter changes any of it. The tests that need to
switch something run the **test build**, the same code plus the regions of `js/compat/hierojax.js` between its
`test-levers` markers (`tools/strip-test-levers.mjs` cuts them out of the shipped one, before the minifier reads
it), made beside the shipped one with a `-test` on its name:

```sh
node tools/build-compat.mjs                                                        # build/compat
node tools/build-compat.mjs --embed --out build/embedded
node tools/build-compat.mjs --test-levers                                          # build/compat-test
node tools/build-compat.mjs --embed --test-levers --out build/embedded-test
```

A test given `--levers` (`realpages`, `mixedpages`) or running its second pass (`pagecheck`, `batchedge`) takes
the test build that goes with `--impl` by itself (`tests/parity/lib/testbuild.mjs`), and stops if there is none,
so that a lever is never silently ignored. The levers (a comma-separated list; the default `batch,fontwait`):
`batch` (`processFragment(elem)` is unaffected), `fontwait` (the alert for a font that never loads comes after
the same 36.4 s), `forcebatch` (no layout is forced between writes in WebKit), `engine=blink|gecko|webkit`
(behave as that engine, to test the WebKit path in another one). The list comes from `window.HIEROXIDE_LEVERS`,
else the page URL's `?hx=...`, else `HIEROXIDE_LEVERS_DEFAULT`, which `variants.mjs` rewrites. The profile hooks
that `inject.js` uses (`hierojax.profile`, `lastTimings`) are in the test build too, so the stage splits of
`run.mjs` and `profile.mjs` need it (`--impl ../../build/compat-test`).

```sh
node tests/bench/variants.mjs base= batch fontwait all=batch,fontwait   # build/variants/<name>/, from the test build
node tests/bench/make-control.mjs      # build/control/: docs/hierojax.js + the same batching, patched at build time
node run.mjs --engine chromium --covered --impl ../../build/control     # also: --real, the big page
node table.mjs label=out/file.json ...                                  # the rows of a comparison
node profile.mjs --impl ../../build/compat-test                         # Chromium: CPU profile and trace events
node batchedge.mjs --impl ../../build/compat                             # nested, hidden, second pass, shade patterns, a fragment that throws
node ab.mjs --engine webkit --page covered upstream=upstream mine=../../build/compat   # interleaved rounds, for a busy machine
```

`ab.mjs` loads every build once per round in turn, so that all see the same stretches of a machine that is busy with
something else, and reports the median with the range (`--page real|covered|pbm|testsuite`; `label=impl@levers` for a
lever of the test build). Compare builds within one run of it.

**Mixed pages: `tests/parity/mixedpages.mjs`.** Seeded random pages of corpus fragments of every kind (svg, dom,
canvas; separated or not; every direction, shading, border) in random containers, laid out by upstream and by the
shim and compared in DOM, canvas pixels, boxes as left, boxes after a fresh layout, log and exceptions. It is the
volume test for batching, which the fixed pages cannot give, and how the rule for WebKit was found
(`js/compat/README.md`): `node mixedpages.mjs --impl ../../build/compat --pages 250 --seed 5`; `--only <seed>
--detail` lists the elements that differ for one page, `--save` writes its HTML. Multi-column containers are left out
unless `--multicol` (Firefox lays those out differently by up to 0.6 px, with or without batching).

Consistent mode has its own stand-in for `hierojax` on these pages (`tests/bench/consistent-shim.js`):
`node tests/bench/make-consistent.mjs`, then `--impl ../../build/consistent-bench`.

### The wall benchmark, consistent mode in the browser, Workers and loading

```sh
cargo build --release -p hieroxide-api --example wall                    # the native side, tests/bench/wall.rs
node tools/build-consistent.mjs && node tools/build-compat.mjs
node tests/bench/wall.mjs [--longline] [--sizes 1000,5000,20000,50000]   # out/wall-<platform>.json
node tests/bench/consistent.mjs      # ways of writing consistent mode's output into a page
node tests/bench/worker.mjs          # layout on the main thread against in a Worker
node tests/bench/load.mjs            # how to load: hints, embedded or separate, over a modelled network
```

`wall.mjs`: sets of about 1k, 5k, 20k and 50k signs, as many short fragments (the lines of the real pages in a
seeded order; 50,000 signs are 1,963 of them, none repeated), natively and in the three engines; per size: layout time,
peak memory (`/usr/bin/time -l` natively; the module's memory, which only grows, in the browser, and the JS heap in
Chromium), the placed glyphs, the bytes printed, and the time until the browser has laid out the output
(`innerHTML`, a forced layout) for text and for path output. Compat and upstream are timed on the same fragments as
`<span>`s of one page, upstream with a timeout (`--upstream-timeout`, 240 s; a size it does not finish is reported
and so are the larger ones). `--longline` adds one fragment of N signs. `consistent.mjs` writes the covered
fragments into a page in the ways it lists and times the writes and the layout owed; `worker.mjs` measures how long
the main thread is blocked; `load.mjs` runs five ways of loading through a proxy that adds brotli, a round trip and
a bandwidth (`local`, `fast`: 50 ms and 12 Mbit/s, `slow`: 200 ms and 1.6 Mbit/s).

**Measured (Mac, M-series; `out/wall-mac-arm64.json`, three runs per cell, the machine shared with another build).**
Layout of a set of N signs (many short fragments), then the page's own layout of the output:

| N signs | native layout (peak RSS) | wasm layout in Chromium / Firefox / WebKit | browser layout of the text output, same order | of the path output | compat, call + layout | upstream, call + layout |
|---|---|---|---|---|---|---|
| 1,012 | 27 ms (10 MB) | 43 / 51 / 45 ms | 39 / 20 / 33 ms | 51 / 58 / 63 ms | 54 / 72 / 98 ms | 81 / 98 / 114 ms |
| 5,010 | 63 ms (11 MB) | 92 / 109 / 91 ms | 82 / 61 / 88 ms | 126 / 166 / 185 ms | 139 / 191 / 252 ms | 231 / 260 / 369 ms |
| 20,021 | 143 ms (12 MB) | 206 / 238 / 197 ms | 231 / 199 / 271 ms | 415 / 541 / 604 ms | 458 / 540 / 725 ms | 796 / 761 / 1,395 ms |
| 50,012 | 275 ms (13 MB, 7 MB at start) | 394 / 459 / 379 ms | 513 / 464 / 638 ms | 948 / 1,247 / 1,405 ms | 1,009 / 1,173 / 1,559 ms | 2,134 / 1,669 / 4,635 ms |

Upstream finishes every size in every engine (no timeout was reached). Nothing is superlinear in the layout: native
time per sign falls from 27 to 5.5 us over the sizes and is still falling, because a fixed part (about 20 ms: the
first fragments' outline reads) is spread over more signs; the marginal cost is 4.4 us a sign between 20k and 50k. The
browser's layout of text output is linear (50k signs: 5.7 MB of markup, 75k text elements). Path output is 41 MB of
markup for 50k signs (0.8 KB a sign against 0.11 KB for text), which costs the browser 1.9 to 2.7 times as long as
the text, 86 MB of JS heap in Chromium, and 520 ms of native printing against 13 ms for text.
Upstream in WebKit is the one mildly superlinear curve (4,635 ms for 2.5 times the signs of 1,395: exponent 1.3; the
page is laid out again between fragments). One fragment of N signs: native 2.1, 9, 34, 88 ms and 8.5, 12.5, 27, 66 MB
at 1k, 5k, 20k, 50k: linear in time and in memory (about 1.1 KB a sign beyond a 8 MB floor); in the browser 10.7, 26, 77,
179 ms in Chromium for the text. Native layout is 1.4 times faster than wasm in Chromium.

**Consistent mode in the browser** (`consistent.mjs`, 5,333 fragments, Chromium / Firefox / WebKit, writes + layout owed
+ two frames, median of 5, ms; the wasm layout is 936 / 1,066 / 890 and does not change): `render` as it is 1,146 /
1,224 / 1,642; `contain: layout style` on each fragment 1,405 / 1,268 / 1,658 (worse in Chromium by 23%, no effect
elsewhere); `contain: size layout style` 1,396 / 1,236 / 1,646; `content-visibility: auto` 1,147 / 1,242 / 1,638 (none:
it needs a block-level box, and the fragments are inline-block); all of a block's fragments in one write 1,102 / 1,242
/ 1,532 (4% and 7% less in Chromium and WebKit, 1.5% more in Firefox). `render` does not read layout, so there is no
read to batch. Nothing was changed in `render`. Against compat on the same page (call + layout, `ab.mjs` with the
consistent stand-in): real page 195 against 218 (0.90), 209 against 229 (0.91), 224 against 319 (0.70); covered 2,196
against 2,228 (0.99), 2,671 against 3,233 (0.83), 2,812 against 5,219 (0.54), in Chromium, Firefox, WebKit. A proposed
margin for "consistent beats compat on the same page": at most 1.03 times compat's call + layout in every engine on
both pages (the ranges of one run reach 8% on the covered page in Chromium, 3% elsewhere), and 0.90 or less on the real
page. Chromium on the covered page is at the margin's edge (0.99), where it was 0.86 before compat reused its canvases.

**A Worker** (`worker.mjs`): `layout()` runs in a Worker as it is (`init` checks for `document`). For 5,333 fragments the main
thread is blocked 963 / 1,363 / 881 ms when it lays them out itself and 10 / 9 / 9 ms with a Worker; the answer arrives
after 1,021 / 1,123 / 922 ms cold (the Worker's own start included) against 962 / 1,363 / 880 on the main thread, and
after 781 / 868 / 697 ms from a Worker that is already running. So it is cheap (a dozen lines) and costs 6% and 5% of
latency cold in Chromium and WebKit; the markup still has to be written on the main thread.

**A cache of measurements** was tried as a question: the same line 3,000 times costs 9.1 us a sign natively, distinct
lines 7.7, 20 lines repeated 6.3: repeating does not make layout cheaper, so what the core keeps (outlines read once,
the memo) is not what the time is spent on, and a bigger cache of measurements would not help. Only a cache of whole
results for identical text and options would, and that is the caller's.

Batching moves the page's style and layout work out of the `processFragmentsNow` call: it is
still owed when the call returns, so `run.mjs` reports it (`layoutAfterMs`, a forced layout
right after) and `totalWithLayoutMs`. Compare implementations on that figure, not on `totalMs`
alone. `tests/parity/run.mjs` calls `processFragmentNow` per fragment, so the batching is only
exercised by `pagecheck.mjs` (and `batchedge.mjs`); the per-fragment gate covers the canvas reuse and
`fontwait`.

## Sizes: `tools/size-check.mjs`

What a browser downloads, in bytes of brotli at quality 11, each file on its own and added: compat is
`hieroxide_wasm_bg.wasm` + `hierojax.js`, consistent is `hieroxide_outline_bg.wasm` + `hieroxide_outline.js` +
`index.js`. The limits are in `tools/size-limits.json`: consistent 100,000 B (the product's); compat a ratchet just
above what it reached, with its target (75,000 B) beside it. The same command runs on CI (the `size` job of
`.github/workflows/golden.yml`, on every push and pull request) and fails when a build is over.

```sh
(cd tools && npm ci)                          # uglify-js, pinned: the build's only JavaScript dependency
node tools/size-check.mjs --build             # build compat and consistent (minified, wasm-opt -O3), then measure
node tools/size-check.mjs --build --paths     # also build/consistent-paths: reported, not limited
node tools/size-check.mjs                     # measure what is in build/
```

The tool is node's own brotli (`zlib`, library 1.1.0 in node 24; the `brotli` 1.2.1 command line gives the same
bytes for these files), so a size depends on the node version only through that library; CI pins node 24.12.0, rustc
1.99.1, wasm-bindgen 0.2.129 and binaryen `version_133`, which is what the limits were set with. `tools/wasm-sizes.mjs` and `twiggy` (not a dependency; `cargo install twiggy`) say
where the bytes of a `--keep-names` build go. `node tools/build-compat.mjs --no-minify` leaves a script readable.

`node tests/golden/textcopy.mjs new.tsv [--old old.tsv]` reads the output of `cargo run --release -p hieroxide-outline
--example fragments -- consistent new.tsv` and says what the text printer copies against the source, and (with an
older run) that only hidden text moved.

## `jsnum`: `tests/jsnum/` and `crates/hieroxide-core/src/jsnum.rs`

JS number semantics in Rust (`toFixed`, `String(number)`, `Math.round`, `Number(string)`).

```sh
cargo test --workspace                                      # unit tests
cargo test --release -p hieroxide-core matches_real_js -- --ignored --nocapture
```

The ignored test spawns `node tests/jsnum/gen.mjs` (4 seeded shards, 3M doubles each, plus
2M strings; `JSNUM_CASES`, `JSNUM_SHARDS` and `NODE` override) and compares every result with
what real JS computed. About 10 seconds in release mode. `number_to_string` (shortest digits) and
`number_from_str` are our own, with no use of the standard library's float formatting or parsing (that was 38 KB of
code and 12 KB of tables in the compat wasm); `JSNUM_CASES=8000000 JSNUM_SHARDS=8` checks 64M doubles and the
same 2M strings.
