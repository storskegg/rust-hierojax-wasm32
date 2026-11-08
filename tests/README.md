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
- **Linux.** `.github/workflows/parity.yml` is `workflow_dispatch` only: it runs upstream in
  the chosen engines on `ubuntu-latest` (run id `linux`) and uploads `out/*.json.gz` for 7 days.
  After downloading the artifact into `linux/`:
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

## `jsnum`: `tests/jsnum/` and `crates/hieroxide-core/src/jsnum.rs`

JS number semantics in Rust (`toFixed`, `String(number)`, `Math.round`, `Number(string)`).

```sh
cargo test --workspace                                      # unit tests
cargo test --release -p hieroxide-core matches_real_js -- --ignored --nocapture
```

The ignored test spawns `node tests/jsnum/gen.mjs` (4 seeded shards, 3M doubles each, plus
2M strings; `JSNUM_CASES`, `JSNUM_SHARDS` and `NODE` override) and compares every result with
what real JS computed. About 10 seconds in release mode.
