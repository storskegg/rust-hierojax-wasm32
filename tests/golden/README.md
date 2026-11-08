# Reference tests of consistent mode

Consistent mode's output is the same bytes in every browser, on every OS and natively, at every
font size and in any order. These files say which bytes.

| File | What |
|---|---|
| `corpus.tsv.gz` | The parity corpus (`tests/parity`), 19,756 fragments, in a form the native test can read without a browser (`mkcorpus.mjs`) |
| `hashes.txt.gz` | One 64-bit FNV-1a per fragment, in corpus order, of everything the fragment outputs: status, container and markup |
| `sample.tsv.gz` | The output itself of every 40th fragment of each class and all of the site's lines, about 500, for seeing what changed |
| `manifest.txt` | The digest of `pow`, `log`, `sin` and `cos`, the count, and a hash of all the hashes |
| `native.rs` | `cargo test -p hieroxide-outline --test golden`: the hashes, the sample, a shuffled run, independence from the page. No browser, no Node, about 30 s |
| `engines.mjs` | The same output from the wasm build in real browsers, at two font sizes; `--shuffle`, `--prefs` |
| `drift.mjs`, `collisions.mjs` | The report against upstream in Mac Chromium: how far it is, and ink collisions and clipping |

```sh
cargo test -p hieroxide-outline --test golden                  # native

node tools/build-consistent.mjs --only consistent
cd tests/parity && PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 npm ci && cd ../..
node tests/golden/engines.mjs                                  # Chromium, Firefox, WebKit at 22 and 40 px
node tests/golden/engines.mjs --shuffle 7 --sizes 40
node tests/golden/engines.mjs --engines firefox --sizes 40 --prefs '{"privacy.resistFingerprinting":true}'
```

**A change of output is a versioned change.** Say so in `CHANGELOG.md`, then
`HIEROXIDE_GOLDEN_UPDATE=1 cargo test -p hieroxide-outline --test golden` and commit the files. The
corpus changes only if the parity corpus does: `node tests/golden/mkcorpus.mjs`.

On GitHub Actions, `.github/workflows/golden.yml` runs the native test and, by hand, the engines on
Linux.

## Against upstream

Fidelity is measured with upstream's layout and printer over the outline raster, so that a
difference can only come from the raster:

```sh
node tools/build-consistent.mjs --only compat-outline
cd tests/parity
node run.mjs --engine chromium --run-id A
node run.mjs --engine chromium --impl ../../build/compat-outline --run-id F
node compare.mjs out/upstream-chromium-A.json.gz out/compat-outline-chromium-F.json.gz --covered-only --ignore-parse-error-text
```

## How the constants were fitted

`crates/hieroxide-outline/examples/fit.rs`, in this order:

```sh
fit record draws.txt                          # every distinct single-glyph draw of the corpus: 54,483
node tests/golden/replay.mjs draws.txt truth.bin   # what Chromium's canvas makes of each
fit score draws.txt truth.bin                 # exact boxes and exact pixels, per kind of draw
fit edges draws.txt truth.bin                 # where boxes differ, by kind and font-size band
fit grid draws.txt truth.bin "g plain" list   # a list of tunings, best first
fit truthrun draws.txt truth.bin "g plain" out.tsv  # the corpus if those draws were perfect
```

The steps, each measured (share of the corpus's fragments identical, native run; add about 0.4
points for options that the native run has no browser to normalize):

| Step | Identical | Found by |
|---|---|---|
| The spike's model: outset 0.3, 0.3, 0.6, 0; quarter-pixel x and whole-pixel y snap; no snap when rotated | 77.7% | |
| The outset shrinks in proportion below about 52 px per em | 78.9% | `edges`: every band below 50 px had too much ink. Per-size fit: 0.006 and 0.012 times the size |
| Snap at a quarter turn too, with the axes swapped, in single precision (a tie stays a tie) | 80.7% | 90 and 270 degrees' boxes went from 64% to 100% exact: Chromium's quarter turn does fit a fixed model |
| At other rotations no outset at all, quarter-pixel snap on both axes | 91.7% | `edges`: boxes from 19% to 93% exact. Inferred: no font smoothing there |
| Threshold 1/255 | 92.4% | |
| A glyph of 256 px or more is filled as a path, without outset; the outset follows the geometric mean of the scales | 92.5% | Boxes at xScale over 1.7. `kSkSideTooBigForAtlas` is 256 in Skia |
| Flatten curves by flatness, 0.25 px unrotated and 0.12 rotated, instead of finely | 94.3% | `truthrun` said that exact pixels matter, and that the pixels are short of them, not the boxes. Finer curves are further from the browser |
| Threshold 0.003 unrotated and 0.005 rotated | 95.0% | Fragment-level sweep |

In Chromium itself, the frozen constants give 18,026 of the 18,897 SVG fragments identical: 95.39%.

Sensitive: 0.02 more outset at the top takes the whole corpus to 59%, because the reference glyph's
height then rounds differently. What is left is mostly exact pixels: only a quarter of the draws
are exact to the pixel, though 97% have the right box. If every draw were exact, 98.9% of fragments
would be. I stopped there: a closer model means guessing at CoreGraphics' font smoothing.
