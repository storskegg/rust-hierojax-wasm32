# Port map

Each Rust function against the JavaScript function it ports, at upstream commit `da31880`
(`src/formatting.js`, `src/main.js`, `src/util.js`, and the data files). Line numbers are
`src/formatting.js` unless another file is named. When upstream moves, this is the list to
walk. Nothing is a stub now; `LayoutError::Unsupported` is only for a `data-sep` so large that
upstream would never finish.

`LayoutError` also carries what upstream throws: `Raster` for an exception from the page's
canvas (the shim rethrows the original object), `Js` for a place where upstream's own code
throws (`JsFault`; the shim makes the same mistake, so the message is the engine's own).

Paths are under `crates/hieroxide-core/src/` unless stated.

## Measuring: `layout/shapes.rs`

| Rust | JavaScript | Lines | Notes |
|---|---|---|---|
| `Engine` | `Shapes` instance `shapes`, `measurements` | 1-5, 630 | The memo is the engine's, not a global. |
| `Engine::measure_glyph` | `Shapes.measureGlyph` | 185-220 | One `Raster::advance` per `measureText` (two, as upstream), one `Raster::draw`. The sizes go to the host as they are, NaN included. |
| `Engine::properties_key` | `Shapes.propertiesKey` | 221-228 | Key built with `MEASURE_SIZE`, `toFixed(2)` on the scales, `Boolean(mirror)`. |
| `Engine::measure_glyph_memo` | `Shapes.measureGlyphMemo` | 229-241 | First value kept; scale-down as upstream. |
| `Engine::em_size_of` | `Shapes.emSizeOf` | 242-246 | |
| `Engine::corrected_measurement` | `PrintedAny.correctedMeasurement` | 692-716 | |
| `layout/raster.rs: Plane::is_dark` | `Shapes.isDark` | 6-11 | Any of the four channels nonzero; out of range is false (a typed array gives `undefined`). |
| `layout/raster.rs: Plane::margins` | `Shapes.margins` | 12-46 | |
| `layout/jsmath.rs` | `getMin`, `getMax`, `getSum` (`util.js`), `Math.min`, `Math.max` | | JS semantics: NaN wins, -0 below +0. |
| `layout/raster.rs: Plane::is_dark_at` | `Shapes.isDark` at coordinates that are JS numbers | 6-11 | NaN, infinities, fractions: no such index, false. |
| `layout/raster.rs: canvas_dim` | assigning to `canvas.width`, `canvas.height` | 204, 729 | ToUint32, then the reflected attribute's default above 2147483647. |
| `layout/raster.rs: Raster::pow`, `log` | `**`, `Math.log` | 2138, 2140 | The engine's own in compat; they differ between engines in the last bit. |
| `layout/hull.rs: ScanPlane`, `PlaneKind` | `Shapes.plane`, `planeExtended`, `planeRestricted` | 247-268 | The pixels come from `MeasCanvas::pixels`. |
| `ScanPlane::top_most_dark`, `bottom_most_dark`, `left_most_dark`, `right_most_dark` | the same names | 47-70 | NaN for none. |
| `hull.rs: orthogonal_hull_with_buffer`, `Buffered`, `JsArray` | `Shapes.orthogonalHullWithBuffer`, `addBufferedVertical`, `addBufferedHorizontal`, the arrays | 71-179 | `JsArray` is an array with negative indices and holes. |
| `hull.rs: h_distance`, `v_distance` | `Shapes.hDistance`, `vDistance` | 269-302 | The caller reads the two planes first, in upstream's order. |
| `hull.rs: open_rect` | `Shapes.openRect` | 303-333 | |
| `hull.rs: distances`, `distances_bottommost`, `_topmost`, `_rightmost`, `_leftmost` | `Shapes.distances`, `distancesBottommost`, ... | 334-452 | |
| `hull.rs: displacement` | `Shapes.displacement` | 453-491 | `None` is `null`. |
| `hull.rs: insertion_position`, `mirror_place`, `mirror_adjustment`, `allowed_places`; `group.rs: St::adjustment` | `Shapes.insertionPosition`, `mirrorPlace`, `mirrorAdjustment`, `allowedPlaces`; `mirrorAdjustments` | 492-587 | A place is its index in `Group.INSERTION_PLACES`. `allowed_places` has no caller in layout, as upstream's has none outside the editor. |
| `Layouter::literal_rotation` | `Shapes.allowedRotations` with `Literal.rotation` | 543-546, 2478-2485 | |
| `tables/mod.rs: is_topup`, `is_bottomdown` | `Shapes.topupChars`, `bottomdownChars` | | |
| `overlay.rs: choose_alt_glyph` | `memoVerticalLigatures`, `memoHorizontalLigatures`, `Vertical.chooseAltGlyph`, `Horizontal.chooseAltGlyph`, `matchLigature` | 588-628, 1311-1335, 1486-1510 | A scan of the table in its order finds what the memo finds; no memo yet. |
| `overlay.rs: find_overlay_ligature` | `memoOverlayLigatures`, `Overlay.findLigature`, `Overlay.matchLigature` | 588-618, 2255-2285 | Candidates of `lits1[0]`, then of `lits2[0]`. |

## Measuring canvases: `layout/canvas.rs`

| Rust | JavaScript | Lines | Notes |
|---|---|---|---|
| `MeasCanvas::new` | `new PrintedCanvasWithoutExtras(fontSize, w, h)` (and `PrintedAny`, `PrintedCanvas`) | 666-677, 726-742, 805-807 | Sizes as `Math.ceil`, through `canvas_dim`. |
| `MeasCanvas::add_sign` | `PrintedCanvasWithoutExtras.addSign`, `PrintedCanvas.addSign` | 755-770, 808-811 | Records one `DrawCmd`: `Math.floor(fontSize * scale)`, translate, scale, rotate, fillText. |
| `MeasCanvas::pixels` | `ctx.getImageData` in `Shapes.plane*` | 247-268 | Replays all `DrawCmd`s on a fresh canvas through `Raster::draw`; not repeated if nothing was drawn since (the JS cannot tell). |
| `Layouter::print_on_canvas` | `group.print(measOptions, printed)` | | `print` with a `Sink` that keeps no text; `addHidden`, `addShading`, `addLog` do nothing on these canvases. |
| `LayoutOpts::measure_options` | `Group.measureOptions` | 1141-1144 | |

## Groups: `layout/group.rs`

`Layouter` holds the per-node state upstream keeps on objects (`St`: `scale`, `x`, `y`, `w`,
`h`, `areas`, `xScale`, `logRotation`, `alt`, `swap`, `adjustments`, `logInsertion`, `kernOpen`,
`kernClose`, `delimOpenRect`, `delimCloseRect`, `outlines`, `extension`, `rect`), indexed by
`NodeId`. It persists across `fit` and `format` calls for as long as the `Layouter` lives, and
nothing saves or restores it around a measuring print, as upstream does not: `scale` is
multiplied by every `fit`; what a `format` with measure options leaves stays until the next
`format`; `kernOpen` and `extension` are from the latest `fit`.

| Rust | JavaScript | Lines |
|---|---|---|
| `Layouter::resize` | `Group.resize`, and that of `Vertical`, `Horizontal`, `Enclosure`, `Basic`, `Overlay` | 1076-1078, 1354-1357, 1533-1536, 1732-1735, 2011-2017, 2317-2321 |
| `Layouter::fit_group` | `Group.fit` | 1079-1087 |
| `Layouter::fit` | `Vertical.fit`, `Horizontal.fit`, and dispatch to `Enclosure.fit`, `Basic.fit` | 1358-1361, 1537-1541 |
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
| `Layouter::prepare`, `choose_alt_of_core` | what the constructors decide: `chooseAltGlyph` of `Vertical` and `Horizontal`, `findLigature`, `Basic`'s `this.core.chooseAltGlyph(this.places())`, `Literal.chooseAltGlyph`, `Overlay.chooseAltGlyph` | 1311, 1486, 1973, 2236, 2286-2297, 2452-2468 |
| `Options::from_raw` | `processFragmentNow` option collection (`main.js` 49-69) and the first twelve lines of `Fragment.print` | 1244-1256 |

## Enclosures: `layout/enclosure.rs`

| Rust | JavaScript | Lines |
|---|---|---|
| `size_enclosure`, `inner_size`, `open_size`, `close_size`, `outline_size` | `Enclosure.size`, `innerSize`, `openSize`, `closeSize`, `outlineSize` | 1669-1705, 1728-1731 |
| `kern_open_size`, `kern_close_size`, `thickness` | `kernOpenSize`, `kernCloseSize`, `thickness` | 1706-1711, 1813-1816 |
| `delim_rotated`, `outline_ch` | `delimOpenRotated`, `delimCloseRotated`, `outlineCh` | 1712-1727 |
| `fit_enclosure` | `Enclosure.fit` | 1736-1756 |
| `fit_open`, `fit_close`, `delim_canvas` | `fitOpen`, `fitClose` | 1757-1812 |
| `format_enclosure` | `Enclosure.format` | 1817-1931 |
| `print_enclosure` | `Enclosure.print` | 1932-1962 |

## Insertions: `layout/basic.rs`

| Rust | JavaScript | Lines |
|---|---|---|
| `Layouter::places` | `Basic.places`, `this[place]` | 1975-1983 |
| `size_basic` | `Basic.size` | 2004-2010 |
| `fit_basic` | `Basic.fit` | 2018-2060 |
| `meas_printed` | `Basic.measPrinted` | 2061-2069 |
| `add_inserted` | `Basic.addInserted` | 2070-2079 |
| `fit_inserted_hull` | `Basic.fitInsertedHull` | 2080-2095 |
| `modify_rect` | `Basic.modifyRect` | 2096-2105 |
| `fit_inserted_position` | `Basic.fitInsertedPosition` | 2106-2134 |
| `fit_grow`, `valid_array_length` | `Basic.fitGrow` | 2135-2159 |
| `remove_margins` | `Basic.removeMargins` | 2160-2176 |
| `format_basic` | `Basic.format` | 2177-2205 |
| `print_basic` | `Basic.print`, `printInsertedExpression`, `isBracketedInsert` | 1994-1998, 2206-2225 |

## Overlays and ligatures: `layout/overlay.rs`

| Rust | JavaScript | Lines |
|---|---|---|
| `size_overlay` | `Overlay.size` | 2298-2310 |
| `size_ligature` | `sizeLigature` of `Vertical`, `Horizontal`, `Overlay` | 1345-1350, 1522-1527, 2311-2316 |
| `format_overlay` | `Overlay.format` | 2322-2361 |
| `format_ligature_vertical`, `_horizontal`, `_overlay`, `place_ligature` | `formatLigature` of each | 1384-1403, 1592-1611, 2362-2390 |
| `print_overlay`, `print_overlay_side` | `Overlay.print` | 2391-2419 |
| `print_ligature` | `printLigature` of each | 1457-1462, 1634-1639, 2420-2425 |

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
| `svg.rs: print_svg_partial` | what stays in the element when `addSign` throws: everything before it | |
| `svg.rs: push_font_style` | `style.setProperty('font-size', ...)`, `line-height` with a value CSS refuses | 962-963, 975-976 |
| `canvas.rs` (above) | `PrintedCanvasWithoutExtras`, and `PrintedCanvas.addSign` for measuring | 755-770, 804-818 |
| `canvas_out.rs: Printer::unit` | `new PrintedCanvas(...)` (constructor: canvas size, border) and `Fragment.print`'s `addText` | 726-742, 1295 |
| `canvas_out.rs: Printer::add_sign` | `PrintedCanvas.addSign` | 755-770 |
| `canvas_out.rs: Printer::add_shading` | `PrintedCanvas.addShading` (the loops of the hatching are in the shim) | 771-796 |
| `canvas_out.rs: print_canvas_partial` | the calls `PrintedCanvas` makes, as a buffer of `f64` (layout in the module's docs) | |
| `js/compat/hierojax.js: hieroxideReplayCanvases` | the canvas, `getContext`, `save`, `font`, `translate`, `scale`, `rotate`, `fillText`, `restore`, `rect`, `stroke`, `fillRect`, `arc`, `fill`, the hidden `div` | 724-802 |
| `dom.rs: Printer::unit` | `new PrintedDOM(...)` (constructor: the span of separated mode, width and height) | 821-834 |
| `dom.rs: Printer::add_sign` | `PrintedDOM.addSign` (`isLinux()` is a parameter) | 835-866 |
| `dom.rs: Printer::add_shading` | `PrintedDOM.addShading` | 867-897 |
| `dom.rs: Printer::unit`, `Item::Hidden` | `PrintedDOM.addHidden` | 898-903 |
| `dom.rs: Style`, `size_ok`, `finite`, `plain_color`, `COLOR_MARK` | `style.setProperty` ignoring a value the CSS parser refuses; the colour that goes through the property | |
| `group.rs: printed_extent` | `PrintedSVG.width()` (`Math.ceil`), `PrintedDOM.width()` (unrounded), `PrintedCanvas.width()` (the canvas's) for the offsets of separated mode | 678-683, 743-748, 933-938 |
| `plan.rs: Container::prop_width`, `prop_height` | `PrintedDOM`'s `style.setProperty` on the element itself | 828-833 |

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
| `layout::render`, `print_plan` | `Fragment.print`, for each `type` | 1243-1297 |
| `layout::render_svg` | `Fragment.print` for `type=svg` | |
| `hieroxide-wasm: Compat::process` + `js/compat/hierojax.js: processFragmentNow` | `HieroJax.processFragmentNow` | `main.js` 49-70 |
| `js/compat/hierojax.js` | `startLoadingFonts`, `waitForFonts`, `processFragments`, `processFragment`, `processFragmentsIn`, `processFragmentsNow` | `main.js` 1-47 |
| `js/compat/hierojax.js: syntax`, `HieroxideFragment` | `syntax.parse(txt)` and the `toString`, and the normalization at the top of `print(element, options)`, of `Fragment` | 1181, 1243-1256 |
| `js/compat/hierojax.js: $`, `removeChildren`, `getMin`, `getMax`, `getSum`, `isLinux` | `util.js` | |
| `js/compat/hierojax.js: PrintedSVG` | `PrintedSVG.EXTERNAL_FONT`, set at 1065 | 1065 |
| `js/compat/hierojax.js: HieroJax.makeScalable`, `hieroxideConfig` | the site's local changes (`viewBox`, em width and height; font URL and format; `DOMContentLoaded`) | |
| `js/compat/hierojax.js: hieroxideHost.advance`, `draw` | the canvas calls in `measureGlyph` and `PrintedCanvas.addSign` (`createElement`, `getContext`, `prepareFont`, `measureText`, size assignment, `save`, `translate`, `scale`, `rotate`, `fillText`, `restore`, `getImageData`) | 180-220, 755-770 |
| `js/compat/hierojax.js: hieroxideHost.pow`, `log` | `**`, `Math.log` in `Basic.fitGrow` | 2138-2140 |
| `hieroxide-wasm: Compat::take_exception`, shim `throw` | an exception from the canvas, passed up through `processFragmentNow` | |
| `hieroxide-wasm: Compat::fault_kind`, `fault_value`, shim `HieroJax.throwFault` | the `TypeError` and `RangeError` of upstream's own code (`JsFault`) | 2139, 2363, 2372 |

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
13. **`PrintedDOM.addShading` tests `hPx + hPx < this.hPx-1`** (twice the shade's height against the
    unit's height, not its bottom edge, where the test of the width is `xPx + wPx < this.wPx-1`).
    `PrintedDOM.addSign` picks `line-height` or `height` by `isLinux()`, and gives `bottom` as
    `this.hPx - meas.height/2` (the unit's height, not the sign's). `PrintedDOM` sizes its element
    with `toFixed(2)` of the unrounded pixel size, `PrintedCanvas` with `Math.ceil`, which is also
    what it hands `wAccum` and `hAccum` through `canvas.width`, clamped by the browser.
14. **Parse errors:** upstream logs jison's message; the port logs its own (a ruling of the
    plan). The harness has `--ignore-parse-error-text` for exactly this.
15. **`Shapes.openRect` has `&` for `&&`.** Same result; neither operand has a side effect.
16. **`almostLast` in `Shapes.distances*`** is computed and only read by commented-out code.
    Left out here, because `Math.round` has no side effect; the commented-out condition stays out.
17. **`Shapes.distancesRightmost` and `distancesLeftmost` stop their backward scan at `-hull.dist`,**
    the other two at `-hull.dist-1`, so one row of the buffer is not visited.
18. **`getMin` and `getMax` of the hull's arrays pass over negative indices,** which are plain
    properties to `reduce`: a buffer reaching above or left of the canvas does not count for
    `xMin` and so on, except in the branch for an empty canvas, which sets them directly.
19. **With a negative `data-sep`** the hull's arrays have holes and writes past their ends: `Math.min`
    of `undefined` stores NaN, and `reduce` skips the holes but not the NaNs. `JsArray` keeps both.
20. **`Array(nSteps)` in `fitGrow`** throws a `RangeError` when the starting scale is 0, negative or NaN.
    Reached with a negative `data-sep` (not by the corpus; the stress corpus has it).
21. **`Shapes.insertions[U+13353]` has an alternative whose glyph (U+E49D) is not in
    `Shapes.ligatures`.** `Overlay.formatLigature` then throws a `TypeError` on `lig.horizontal`,
    after `size` has measured and `x`, `y`, `w`, `h`, `areas` are set. Kept, in each engine's wording.
22. **`Enclosure.print` calls `addHidden(numToDamage(0))`,** which in SVG adds an empty `tspan` after
    each delimiter.
23. **`Enclosure` and `Basic` measure with `options` and print with `measOptions` in one breath:**
    `fitOpen` sizes the delimiter and group with the real font size, then formats and prints
    the group at size 50, whose `x`, `y`, `w`, `h`, `areas` stay until the real `format`.
24. **A canvas side of 0 throws from `getImageData`** (an insertion into an empty enclosure, or a NaN
    size): 116 fragments of the corpus, in all three engines, all in `format`, before anything is
    written, so the element is left empty. **A negative size** is 300 by 150 after the reflected
    attribute's rule (`canvas_dim`), and the code goes on with that.
25. **A negative or NaN font size in `setProperty` is ignored,** so such a sign has no `style`
    attribute (`push_font_style`).
26. **`PrintedCanvas` draws hatching pixel by pixel** (`(wAccum+xMin+px + hAccum+yMin+py) % 4`, an
    `arc` and a `fill` each), and the loop is upstream's, run by the shim.
27. **`Fragment.print` of a canvas fragment adds the hidden text `div` after the canvas,** not
    inside it, and only when the fragment is not separated; `PrintedCanvas.addHidden` does nothing.
28. **`type` other than `canvas` and `svg` is the DOM printer** (`data-type="foo"`), as the
    conditional in `Fragment.print` reads.
