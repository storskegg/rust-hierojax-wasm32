# Changelog

Consistent mode's output counts as API: any change to a reference file in `tests/golden` is
listed here, and is a versioned change. Compat mode has no entries, because it has one rule: it
does what upstream does.

## Unreleased

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

### The outline raster

`crates/hieroxide-outline`: glyph pixels from the font's outlines, native and wasm, deterministic.
Fitted to Chromium 158's canvas on an Apple-silicon Mac; `Tuning::FROZEN` has the constants and
`tests/golden/README.md` how they were found. "Identical to upstream" is true only against that
target: upstream on Linux differs from upstream on the Mac in about 97% of fragments, so it differs
from this about as much.
