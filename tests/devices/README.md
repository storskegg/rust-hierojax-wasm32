# Device harness

Hieroxide is tested in Playwright's Chromium, Firefox and WebKit. This is for the browsers Playwright
cannot drive: desktop Safari, Mobile Safari in the iOS Simulator, Chrome in the Android emulator, Chrome
on a real phone. A small server serves a self-running page; the page runs the checks in the browser and
POSTs one JSON result back, so any browser that can open a URL can be tested.

```sh
# once: tools/ and tests/parity installed, and the builds
(cd tools && npm ci); (cd tests/parity && PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 npm ci)
node tests/parity/corpus/build.mjs
node tools/build-compat.mjs
node tools/build-consistent.mjs

node tests/devices/server.mjs                 # http://127.0.0.1:8765/ ; results into tests/devices/out/
#   in the device's browser: http://localhost:8765/run.html?label=pixel
node tests/devices/report.mjs [--details]     # the runs as a table
```

`run.mjs` does the opening and waiting for the three targets that can be scripted (below). To check the
harness itself, `node tests/devices/playwright.mjs` runs it in Playwright's three engines, where the answers
are known: every check must pass there.

## The checks

Each is run by `page/harness.js` in the page; `?label=NAME` names the run, `?n=2500|full` the parity
sample, `?reps=5` the speed rounds, `?golden=0` and `?speed=0` skip a check.

1. **Compat parity.** The fragments of `tests/parity/corpus` (every fragment of upstream's test suite, 1,370,
   and an even stride through the rest, 2,500 in all; `n=full` for all 19,756) are laid out with upstream's
   `docs/hierojax.js` and with `build/compat/hierojax.js`, each in its own frame of the same page, a fresh frame
   per batch of 200, with the parity driver's own batch function (`tests/parity/lib/driver.mjs`: the canonical
   DOM, boxes, canvas hashes, selection text, logged messages). The comparison is `compare.mjs`'s, field by
   field, with the text of a logged parse error normalized and its allowlist (the `depth` group). Reported:
   identical, differing (allowlisted / open), the largest difference of any box, the count of each differing
   field, and the first five differences.
2. **Consistent golden.** The golden corpus (`tests/golden`, all 19,756 fragments) through `build/consistent`,
   each record hashed with FNV-1a as `engines.mjs` does, compared with `hashes.txt.gz`; the hash of those
   hashes against the manifest's `all` (`2f6cec8bf973a3e1`), and `determinismDigest()` against its `digest`
   (`2aaa73e7d4f0260f`). Whole set, not a subset, in about 10 s on a laptop.
3. **Paths output.** Five blocks through `build/consistent-paths` (`svg: 'paths'`): each must render, with
   an `svg` that has path elements and a box, and the page must log no console error or uncaught error.
4. **Speed, roughly.** Up to 300 of the fragments that parity found clean, laid out on one page by upstream,
   compat and consistent in turn (`processFragmentsNow` for the first two, `render` for the third), plus the
   forced layout after, as `tests/bench/ab.mjs` times. Interleaved rounds (5), median reported with the range.
   Read the ratios, not the milliseconds; a phone is not quiet.
5. **Environment.** User agent, `devicePixelRatio`, screen, and `hieroxideEngine`, the engine compat detected,
   read from the compat frame.

## Running on the targets

All of these serve on `localhost`; none sends anything off the machine.

- **Desktop Safari.** `node tests/devices/run.mjs safari`. It speaks WebDriver to `safaridriver`, which opens
  its own automation window and leaves your Safari alone. It needs Safari's "Allow remote automation"
  (Safari Settings > Developer), which `safaridriver --enable` turns on and which needs sudo: the script
  does not do it and prints safaridriver's message instead. By hand: open the page in Safari.
- **iOS Simulator.** `node tests/devices/run.mjs ios` boots an available iPhone simulator if none is booted
  and runs `xcrun simctl openurl`. The simulator shares the Mac's network, so `localhost` is the Mac. It needs an
  iOS simulator runtime installed (`xcrun simctl list runtimes`; Xcode > Settings > Components).
- **Android emulator.** `node tests/devices/run.mjs android --serial emulator-5554` takes the shared-device lock
  (`~/d/storskegg/inkhk3-shared/bin/device lock SERIAL glyph "hieroxide browser tests"`, waiting up to 20
  minutes), runs `adb -s SERIAL reverse tcp:PORT tcp:PORT`, starts Chrome (`com.android.chrome`) on the URL, waits,
  removes the reverse and unlocks, also on failure. It opens only Chrome and always passes `-s`. A Chrome that has
  never been run shows a first-run screen ("Make Chrome your own", Terms of Service) and the page does not
  load until someone dismisses it ("Use without an account"); the script does not tap on the device, so do
  that once by hand. After 90 s without word from the page it says so.
- **A real phone (Pixel).** The same, with the lock held by whoever uses the phone:

  ```sh
  adb -s SERIAL reverse tcp:8765 tcp:8765
  node tests/devices/run.mjs android --serial SERIAL --no-lock --label pixel
  # or by hand: node tests/devices/server.mjs, then
  adb -s SERIAL shell am start -a android.intent.action.VIEW -d 'http://localhost:8765/run.html?label=pixel' com.android.chrome
  adb -s SERIAL reverse --remove tcp:8765
  ```

  On Wi-Fi instead: `node tests/devices/server.mjs --host 0.0.0.0` and open `http://MAC-IP:8765/run.html` (the
  page works on any origin the browser treats as secure enough for wasm and `TextEncoder`; plain http to a
  LAN address is fine for these).

`node tests/devices/report.mjs` then prints, per run: browser and OS from the user agent, the engine compat
detected, the pixel ratio, parity (identical of fragments), the golden digest, paths, the speed medians in ms
(upstream / compat / consistent) and the length of the run.

## Notes

- `hieroxideEngine` is read with `eval` in the compat frame, because it is a top-level `const` of the script (no
  window property). Compat and upstream are in frames because both define `hierojax`.
- Frames are 1280 px wide and hidden with `visibility`, not `display`, so that they are laid out; the boxes
  compared are the same for both sides whatever the screen.
- The page needs `fetch`, `TextEncoder`, dynamic `import()`, WebAssembly and `FontFace`; nothing newer than
  Safari 14 / Chrome 80 is used by the harness itself.
