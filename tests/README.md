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
  build it fails by design (`js/compat/README.md`).

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

### Speed levers of the compat shim

`js/compat/hierojax.js` has three independent switches: `batch` (read every element, lay all
out, write all; `processFragment(elem)` is unaffected), `canvas` (one long-lived canvas for
`advance`, one scratch canvas for `draw`) and `fontwait` (a promise instead of polling; the
alert for a font that never loads comes after the same 36.4 s). The list comes from
`window.HIEROXIDE_LEVERS`, else the page URL's `?hx=batch,canvas`, else the constant
`HIEROXIDE_LEVERS_DEFAULT` (now `batch,fontwait`).

```sh
node tools/build-compat.mjs
node tests/bench/variants.mjs base= batch canvas fontwait all=batch,canvas,fontwait   # build/variants/<name>/
node tests/bench/make-control.mjs      # build/control/: docs/hierojax.js + the same batching, patched at build time
node run.mjs --engine chromium --covered --impl ../../build/control     # also: --real, the big page
node table.mjs label=out/file.json ...                                  # the rows of a comparison
node profile.mjs --impl ../../build/variants/all                        # Chromium: CPU profile and trace events
node batchedge.mjs --impl ../../build/compat                             # nested, hidden, second pass, shade patterns, a fragment that throws
```

Batching moves the page's style and layout work out of the `processFragmentsNow` call: it is
still owed when the call returns, so `run.mjs` reports it (`layoutAfterMs`, a forced layout
right after) and `totalWithLayoutMs`. Compare implementations on that figure, not on `totalMs`
alone. `tests/parity/run.mjs` calls `processFragmentNow` per fragment, so the batching is only
exercised by `pagecheck.mjs` (and `batchedge.mjs`); the per-fragment gate covers `canvas` and
`fontwait`.

## `jsnum`: `tests/jsnum/` and `crates/hieroxide-core/src/jsnum.rs`

JS number semantics in Rust (`toFixed`, `String(number)`, `Math.round`, `Number(string)`).

```sh
cargo test --workspace                                      # unit tests
cargo test --release -p hieroxide-core matches_real_js -- --ignored --nocapture
```

The ignored test spawns `node tests/jsnum/gen.mjs` (4 seeded shards, 3M doubles each, plus
2M strings; `JSNUM_CASES`, `JSNUM_SHARDS` and `NODE` override) and compares every result with
what real JS computed. About 10 seconds in release mode.
