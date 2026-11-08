# Port map

Each Rust function against the JavaScript function it ports, at upstream commit `da31880`
(`src/formatting.js`, `src/main.js`, `src/util.js`, and the data files). Line numbers are
`src/formatting.js` unless another file is named. When upstream moves, this is the list to
walk. "Stub" means the Rust side answers `LayoutError::Unsupported` where the function is
reached; the milestone that fills it in is given.

Paths are under `crates/hieroxide-core/src/` unless stated.

## Measuring: `layout/shapes.rs`

| Rust | JavaScript | Lines | Notes |
|---|---|---|---|
| `Engine` | `Shapes` instance `shapes`, `measurements` | 1-5, 630 | The memo is the engine's, not a global. |
| `Engine::measure_glyph` | `Shapes.measureGlyph` | 185-220 | One `Raster::advance` per `measureText` (two, as upstream), one `Raster::draw`. |
| `Engine::properties_key` | `Shapes.propertiesKey` | 221-228 | Key built with `MEASURE_SIZE`, `toFixed(2)` on the scales, `Boolean(mirror)`. |
| `Engine::measure_glyph_memo` | `Shapes.measureGlyphMemo` | 229-241 | First value kept; scale-down as upstream. |
| `Engine::em_size_of` | `Shapes.emSizeOf` | 242-246 | |
| `Engine::corrected_measurement` | `PrintedAny.correctedMeasurement` | 692-716 | |
| `layout/raster.rs: Plane::is_dark` | `Shapes.isDark` | 6-11 | Any of the four channels nonzero; out of range is false (a typed array gives `undefined`). |
| `layout/raster.rs: Plane::margins` | `Shapes.margins` | 12-46 | |
| `layout/jsmath.rs` | `getMin`, `getMax`, `getSum` (`util.js`), `Math.min`, `Math.max` | | JS semantics: NaN wins, -0 below +0. |
| (stub, M3) | `Shapes.topMostDark` ... `rightMostDark`, `orthogonalHullWithBuffer`, `plane`, `planeExtended`, `planeRestricted`, `hDistance`, `vDistance`, `openRect`, `distances*`, `displacement` | 47-70, 71-179, 247-491 | Group pixels. |
| (stub, M3) | `Shapes.insertionPosition`, `mirrorPlace`, `allowedPlaces`, `allowedRotations`, `mirrorAdjustment(s)` | 492-587 | The tables they read are generated (`tables/`). |
| `tables/mod.rs: ligatures()`, `Layouter::choose_alt_glyph` | `memoLigatures`, `memoVerticalLigatures`, `memoHorizontalLigatures`, `Vertical.chooseAltGlyph`, `Horizontal.chooseAltGlyph`, `matchLigature` | 588-628, 1311-1335, 1486-1510 | Only whether a ligature matches (it then answers unsupported); the lookup order does not matter for that. M3 builds the memo. |

## Groups: `layout/group.rs`

`Layouter` holds the per-node state upstream keeps on objects (`St`: `scale`, `x`, `y`, `w`,
`h`, `areas`, `xScale`, `logRotation`), indexed by `NodeId`. It persists across `fit` and
`format` calls for as long as the `Layouter` lives.

| Rust | JavaScript | Lines |
|---|---|---|
| `Layouter::resize` | `Group.resize`, `Vertical.resize`, `Horizontal.resize` | 1076-1078, 1354-1357, 1533-1536 |
| `Layouter::fit_group` | `Group.fit` | 1079-1087 |
| `Layouter::fit` | `Vertical.fit`, `Horizontal.fit`, (`Enclosure.fit`, `Basic.fit`: stubs, M3) | 1358-1361, 1537-1541 |
| `Layouter::size` | `size` of every class | |
| `Layouter::size_fragment` | `Fragment.size` | 1184-1197 |
| `Layouter::format_fragment` | `Fragment.format` | 1198-1242 |
| `Layouter::plan` | `Fragment.print` (after the option normalization, which is `Options::from_raw`) | 1243-1298 |
| `Layouter::format_vertical` | `Vertical.format` | 1362-1383 |
| `Layouter::net_height_vertical`, `net_height_of` | `Vertical.netHeight`, `Vertical.netHeightOf` | 1351-1353, 1431-1444 |
| `Layouter::nested_vertical_spaces`, `nested_vertical_spaces_of` | `Vertical.nestedVerticalSpaces`, `nestedVerticalSpacesOf` | 1404-1406, 1420-1430 |
| `Layouter::fit_proper_groups` | `Vertical.fitProperGroups` | 1407-1419 |
| `Layouter::proper_groups`, `net_width` | `Horizontal.properGroups`, `Horizontal.netWidth` | 1612-1614, 1528-1532 |
| `Layouter::format_horizontal` | `Horizontal.format` | 1542-1591 |
| `Layouter::format_bracket` | `BracketOpen.format`, `BracketClose.format` (`open` picks `Math.max` or `Math.min`) | 2638-2648, 2666-2676 |
| `Layouter::literal_rotation` | `Literal.rotation` (and `Shapes.allowedRotations`) | 2475-2485, 543-546 |
| `Layouter::singleton_ch_rotated` | `Singleton.chRotated` | 2539-2541 |
| `format` arms for Literal, Singleton, Blank, Lost | `Literal.format`, `Singleton.format`, `Blank.format`, `Lost.format` | 2486-2498, 2542-2554, 2578-2579, 2609-2618 |
| `Layouter::print` | `print` of Vertical, Horizontal, Literal, Singleton, Blank, Lost, BracketOpen, BracketClose | 1445-1456, 1615-1633, 2499-2519, 2555-2561, 2580-2582, 2619-2623, 2649-2651, 2677-2679 |
| `damage_areas` | `Group.damageAreas` | 1127-1140 |
| `num_to_rotate` | `Group.numToRotate` | 1103-1114 |
| (`tree.rs`) `damage_char`, `variation_char`, `Tree::write_node` | `Group.numToDamage`, `numToVariation`, every `toString` | 1097-1102, 1181 and the classes |
| `LayoutOpts::h`, `v`, `rl` | `Group.h`, `Group.v`, `Group.rl` | 1088-1096 |
| (stub, M3) | `Group.measureOptions`, `Enclosure` (all), `Basic` (all), `Overlay` (all), `Vertical.sizeLigature`, `formatLigature`, `printLigature`, and the same three of `Horizontal` | 1141-1145, 1345-1349, 1384-1403, 1457-1462, 1522-1527, 1592-1611, 1634-1639, 1642-2426 |
| `Options::from_raw` | `processFragmentNow` option collection (`main.js` 49-69) and the first twelve lines of `Fragment.print` | 1244-1256 |

## Printing: `layout/plan.rs`, `layout/svg.rs`

The layout walk records the calls a printer would receive (`Plan`); the SVG printer reads
them. Upstream's `print` methods call the printer directly.

| Rust | JavaScript | Lines |
|---|---|---|
| `Plan`, `Unit`, `Item` | the sequence of `addSign`, `addHidden`, `addShading`, `addLog` calls on one `Printed*` | |
| `svg.rs: Printer::unit` | `new PrintedSVG(...)` (constructor) | 912-931 |
| `svg.rs: Printer::add_sign` | `PrintedSVG.addSign` | 939-982 |
| `svg.rs: Printer::add_shading` | `PrintedSVG.addShading` | 983-1003 |
| `svg.rs: add_hidden` | `PrintedSVG.addHidden` | 1004-1009 |
| `svg.rs: Printer::shade_pattern` | `PrintedSVG.shadePattern` (`document.getElementById` through `DocIds`) | 1010-1028 |
| `svg.rs: internal_css` | `PrintedSVG.internalCSS` | 1032-1063 |
| `plan.rs: Container` | the changes `Fragment.print` makes to the element in separated mode | 1267-1279 |
| `PrintedAny` (`reverse`, `emToPx`, `width`, `height`) | inlined in the SVG printer | 657-690, 933-938 |
| (stub, M4a) | `PrintedCanvas`, `PrintedCanvasWithoutExtras`, `PrintedDOM` | 724-907 |

## Tables: `tables/`, generated by `tools/gen-tables.mjs`

| Rust | JavaScript | File |
|---|---|---|
| `tables::insertions`, `is_topup`, `is_bottomdown` | `Shapes.insertions`, `topupChars`, `bottomdownChars` | `src/insertions.js` |
| `tables::ligatures`, `ligature` | `Shapes.ligatures` | `src/ligatures.js` |
| `tables::rotations` | `Shapes.rotations` | `src/standardizedvariants.js` |
| `group.rs: ROTATED_CHARS` | `Shapes.rotatedChars` | formatting.js 638-655 |

## Entry points

| Rust / JS | JavaScript | |
|---|---|---|
| `layout::render_svg` | `Fragment.print` for `type=svg` | |
| `hieroxide-wasm: Compat::process` + `js/compat/hierojax.js: processFragmentNow` | `HieroJax.processFragmentNow` | `main.js` 49-70 |
| `js/compat/hierojax.js` | `startLoadingFonts`, `waitForFonts`, `processFragments`, `processFragment`, `processFragmentsIn`, `processFragmentsNow` | `main.js` 1-47 |
| `js/compat/hierojax.js: hieroxideHost.advance`, `draw` | the canvas calls in `measureGlyph` (`createElement`, `getContext`, `prepareFont`, `measureText`, size assignment, `translate`, `scale`, `rotate`, `fillText`, `getImageData`) | 180-220 |

## Upstream's oddities, kept

Each is kept on purpose and marked with a comment where it lives.

1. **The memo is order-dependent.** The key rounds `xScale` and `yScale` to two decimals and
   the first measurement under a key is kept (`measureGlyphMemo`). Brackets scale between 0.5
   and 1, so two bracket fragments in the same hundredth give the first one's measurement.
   Skipping any fragment that upstream would have laid out can change a later fragment, which
   is why the parity gate also runs the covered fragments alone (see `tests/README.md`).
2. **`Math.min`/`Math.max` and `getMax` of nothing.** NaN wins; `getMax([])` is `-Infinity`,
   so a fragment with no groups has `h = Math.max(-Infinity, linesize)`.
3. **`data-fontsize` is a string.** `options.fontsize` is a number from the computed style
   until a `data-fontsize` replaces it with a string; `|| 22` then only replaces a falsy
   number, so `data-fontsize="0"` gives a size of 0 and `"abc"` gives NaN.
4. **`Number(x) || default`.** `data-sep="0"` and `data-linesize="0"` fall back to 0.07 and 1.
5. **An unknown `data-dir`** is neither `h` nor `v`: `Fragment.size` and `format` take their
   vertical branches, `rl` is false, and a separated container gets the `hierojax-v-container`
   class without a width or height.
6. **Any `data-type` other than `svg` and `canvas` is the DOM printer.**
7. **`this.reversed ^ mirror` is a number.** It is passed on as the `mirror` argument of
   `correctedMeasurement` (where `Boolean(mirror)` goes into the key), and `PrintedSVG.addSign`
   tests the raw `mirror` for choosing `text` or `tspan` while the transform uses the
   combined value. For `scale(...)` it writes `-xScaleStr`, a number (`"1.00"` gives `-1`,
   `"0.50"` gives `-0.5`), and `-xTrans` likewise for `dx`.
8. **`Literal.rotation` is called by `size` and by `print`** and sets `logRotation` as a side
   effect; the log is written at the end of the literal's `print` if `data-log` is true.
9. **`Fragment.format` calls `this.size(options)` again for the last group** instead of using
   the sum it already has; the result is the same here, but the work is repeated.
10. **`shadePattern` asks `document.getElementById`,** which is document-wide: a pattern left
    by an earlier fragment is reused, and the first group of a separated fragment holds it for
    the rest. The host answers `DocIds::has_id`; the element's own earlier patterns are
    counted only if it is in the document.
11. **`BracketOpen.format` uses `Math.max`, `BracketClose.format` `Math.min`**, and a NaN
    `availableRatio` (`w / h` with `h` 0) takes the `else` branch, `xScale = 1`.
12. **`measureGlyph` measures twice** (`measureText` for `width` and again for
    `widthScaled`) and draws on a canvas whose size is set after its first context exists.
13. **Oddities in code not ported yet:** `PrintedDOM.addShading` tests `hPx + hPx < this.hPx-1`
    (M4a); `Shapes.openRect` uses `&` where `&&` is meant (M3).
14. **Parse errors:** upstream logs jison's message; the port logs its own (a ruling of the
    plan). The harness has `--ignore-parse-error-text` for exactly this.
