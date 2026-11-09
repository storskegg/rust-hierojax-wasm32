#!/usr/bin/env node
// The wall benchmark: how layout scales, on one large set of fragments (about 50,000 signs) and on prefixes of it
// (1k, 5k, 20k, 50k), natively and in the three engines, with no upstream target. Measured per size:
//  - native: the API (`tests/bench/wall.rs`): parse, layout and print time, the placed glyphs, peak resident memory
//    (`/usr/bin/time -l`);
//  - consistent mode in the browser (build/consistent, and build/consistent-paths for path output): layout time of
//    every fragment through `layout()`, the size of the module's memory afterwards (it only grows, so it is the
//    peak), the JS heap (Chromium only), and the time to the browser's layout of all the output: `innerHTML`, a
//    forced layout, and the next two frames;
//  - compat in the browser (build/compat) and upstream (docs/hierojax.js), on the same fragments as `<span>`s in one
//    page: `processFragmentsNow` and the layout after it. Upstream gets a timeout: a size it cannot finish is
//    reported as such, and so are larger ones after it.
// The fragments are the lines of the real pages (docs/texts/, from the parity corpus) in a seeded shuffle, each laid
// out alone as a page would, so a set is many short fragments, not one long one. `--longline` adds the other
// extreme: one fragment of N signs (a horizontal run of two signs at a time), to see how one fragment's size matters.
//
//   cargo build --release -p hieroxide-api --example wall
//   node tools/build-consistent.mjs && node tools/build-compat.mjs
//   node wall.mjs [--engines chromium,firefox,webkit] [--sizes 1000,5000,20000,50000] [--reps 3]
//                 [--upstream-timeout 240] [--skip native,consistent,compat,upstream] [--longline] [--out file.json]
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { launch, platformTag } from '../parity/lib/engines.mjs';
import { startServer, DOCS } from '../parity/lib/server.mjs';
import { loadCorpus, mulberry32 } from '../parity/lib/runner.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../..');
const INJECT = fs.readFileSync(path.join(HERE, 'inject.js'), 'utf8');
const { values: v } = parseArgs({
  options: {
    engines: { type: 'string', default: 'chromium,firefox,webkit' },
    sizes: { type: 'string', default: '1000,5000,20000,50000' },
    reps: { type: 'string', default: '3' },
    'upstream-timeout': { type: 'string', default: '240' },
    skip: { type: 'string', default: '' },
    longline: { type: 'boolean' },
    out: { type: 'string' },
  },
});
const SIZES = v.sizes.split(',').map(Number);
const REPS = Number(v.reps);
const skip = new Set(v.skip.split(',').filter(Boolean));
const med = (a) => { const s = a.slice().sort((x, y) => x - y); const n = s.length; return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2; };
const r1 = (x) => (x == null ? null : Math.round(x * 100) / 100);
const isSign = (ch) => { const x = ch.codePointAt(0); return x >= 0x13000 && x <= 0x1342F; };
const nSigns = (t) => [...t].filter(isSign).length;

// ---- the sets --------------------------------------------------------------------------------------------------
const corpus = loadCorpus();
const pool = [...new Set(corpus.items.filter((it) => it.group === 'texts').map((it) => it.text))];
const rnd = mulberry32(5);
for (let i = pool.length - 1; i > 0; i--) { const j = Math.floor(rnd() * (i + 1)); [pool[i], pool[j]] = [pool[j], pool[i]]; }
const MAXLINE = 60; // a real page's longest lines are hundreds of signs; the set is of lines, so long ones are left to --longline
const lines = pool.filter((t) => nSigns(t) <= MAXLINE);
function setOf(n) {
  const out = [];
  let s = 0;
  for (let i = 0; s < n; i++) {
    const t = lines[i % lines.length]; // wraps if the pool is smaller than the set: repeated fragments, as a long page has
    out.push(t);
    s += nSigns(t);
  }
  return { lines: out, signs: s, repeats: Math.max(0, out.length - lines.length) };
}
const sets = Object.fromEntries(SIZES.map((n) => [n, setOf(n)]));
const longline = (n) => {
  const A = '\u{13000}', B = '\u{13050}', H = '\u{13431}';
  let t = A;
  for (let i = 1; i < n; i++) t += H + (i % 2 ? B : A);
  return t;
};
const wallDir = path.join(ROOT, 'build/wall');
fs.mkdirSync(wallDir, { recursive: true });

const result = { meta: { platform: platformTag(), machine: `${os.cpus()[0]?.model ?? '?'}, ${os.cpus().length} cores`, date: new Date().toISOString(), reps: REPS, sets: Object.fromEntries(SIZES.map((n) => [n, { fragments: sets[n].lines.length, signs: sets[n].signs, repeats: sets[n].repeats }])) }, native: {}, browser: {} };

// ---- native ----------------------------------------------------------------------------------------------------
if (!skip.has('native')) {
  const bin = path.join(ROOT, 'target/release/examples/wall');
  if (!fs.existsSync(bin)) throw new Error('cargo build --release -p hieroxide-api --example wall');
  const font = path.join(DOCS, 'NewGardiner.otf');
  const run = (file, mode) => {
    const r = spawnSync('/usr/bin/time', [process.platform === 'darwin' ? '-l' : '-v', bin, font, file, mode], { encoding: 'utf8', timeout: 600000 });
    const rss = process.platform === 'darwin' ? Number(/(\d+)\s+maximum resident set size/.exec(r.stderr)?.[1]) : Number(/Maximum resident set size \(kbytes\): (\d+)/.exec(r.stderr)?.[1]) * 1024;
    return { ...JSON.parse(r.stdout), peakRssBytes: rss };
  };
  const tiny = path.join(wallDir, 'tiny.txt');
  fs.writeFileSync(tiny, '\u{13000}\n');
  result.native.baselineRssBytes = med([0, 1, 2].map(() => run(tiny, 'layout').peakRssBytes));
  for (const n of SIZES) {
    const f = path.join(wallDir, `set-${n}.txt`);
    fs.writeFileSync(f, sets[n].lines.join('\n') + '\n');
    for (const mode of ['layout', 'text', 'paths']) {
      const rows = Array.from({ length: REPS }, () => run(f, mode));
      result.native[`${n}/${mode}`] = { ...rows[0], layoutMs: r1(med(rows.map((x) => x.layoutMs))), parseMs: r1(med(rows.map((x) => x.parseMs))), printMs: r1(med(rows.map((x) => x.printMs))), totalMs: r1(med(rows.map((x) => x.totalMs))), peakRssBytes: med(rows.map((x) => x.peakRssBytes)), allTotalMs: rows.map((x) => r1(x.totalMs)) };
      console.error(`native ${n} ${mode}: total ${result.native[`${n}/${mode}`].totalMs} ms, peak ${(result.native[`${n}/${mode}`].peakRssBytes / 1e6).toFixed(0)} MB`);
    }
  }
  if (v.longline) {
    for (const n of SIZES) {
      const f = path.join(wallDir, `long-${n}.txt`);
      fs.writeFileSync(f, longline(n) + '\n');
      const r = run(f, 'layout');
      result.native[`long-${n}/layout`] = r;
      console.error(`native one fragment of ${n} signs: total ${r.totalMs} ms, peak ${(r.peakRssBytes / 1e6).toFixed(0)} MB`);
    }
  }
}

// ---- browsers --------------------------------------------------------------------------------------------------
const CONSISTENT_PAGE = (kind) => `<!DOCTYPE html><html><head><meta charset="utf-8"><title>wall</title></head><body><div id="box"></div>
<script type="module">
import { init, layout } from '/index.js';
import wasmInit from '/hieroxide_outline.js';
await init({ font: '/NewGardiner.otf' });
const exports = await wasmInit();
window.__RUN = async (lines, svg) => {
  const mem0 = exports.memory.buffer.byteLength;
  const t0 = performance.now();
  const out = [];
  let failed = 0;
  for (const t of lines) {
    const r = layout(t, { svg });
    if (r.ok) out.push(r); else failed++;
  }
  const t1 = performance.now();
  const markup = out.map((r) => '<span class="' + (r.className || '') + '"' + (r.width ? ' style="width:' + r.width + ';height:' + r.height + '"' : '') + '>' + r.markup + '</span>').join('');
  const bytes = markup.length;
  const glyphs = (markup.match(svg === 'paths' ? /<path /g : /<text|<tspan/g) || []).length;
  const box = document.getElementById('box');
  const t2 = performance.now();
  box.innerHTML = markup;
  const t3 = performance.now();
  void box.offsetHeight;
  const t4 = performance.now();
  await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
  const t5 = performance.now();
  const heap = performance.memory ? performance.memory.usedJSHeapSize : null;
  box.innerHTML = '';
  return { layoutMs: t1 - t0, innerHtmlMs: t3 - t2, browserLayoutMs: t4 - t3, framesMs: t5 - t4, toLayoutMs: t4 - t2, wasmMemBytes: exports.memory.buffer.byteLength, wasmMemBeforeBytes: mem0, jsHeapBytes: heap, markupBytes: bytes, glyphs, failed };
};
window.__READY = true;
</script></body></html>`;

const esc = (t) => t.replace(/&/g, '&amp;').replace(/</g, '&lt;');
const LEGACY_PAGE = (ls) => `<!DOCTYPE html><html><head><meta charset="utf-8"><link rel="stylesheet" href="hierojax.css"><script src="hierojax.js"></script></head><body>
${ls.map((t) => `<div style="font-size:22px"><span class="hierojax">${esc(t)}</span></div>`).join('\n')}
<script>addEventListener('DOMContentLoaded', () => hierojax.processFragments());</script></body></html>`;

async function consistentRun(engine, browser, server, lines, svg) {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
  const page = await ctx.newPage();
  page.setDefaultTimeout(600000);
  await page.goto(server.url + '/wall.html');
  await page.waitForFunction(() => window.__READY === true);
  await page.evaluate(() => document.fonts.ready);
  const r = await page.evaluate(([l, s]) => window.__RUN(l, s), [lines, svg]);
  await ctx.close();
  return r;
}

async function legacyRun(engine, browser, server, lines, timeoutS) {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
  const page = await ctx.newPage();
  await page.addInitScript('window.__HEAVY = false; window.__F = {};');
  const t0 = Date.now();
  try {
    await page.goto(`${server.url}/wall-legacy.html`, { waitUntil: 'load', timeout: timeoutS * 1000 });
    await page.waitForFunction(() => window.__done === true, null, { timeout: timeoutS * 1000, polling: 500 });
  } catch (e) {
    await ctx.close().catch(() => {});
    return { timedOut: true, afterS: (Date.now() - t0) / 1000 };
  }
  const t = await page.evaluate(() => window.__S.t);
  await ctx.close();
  return { callMs: t.endNow - t.startNow, layoutMs: t.endLayout - t.endNow, totalMs: t.endLayout - t.startNow };
}

for (const engine of v.engines.split(',')) {
  console.error(`== ${engine}`);
  const R = (result.browser[engine] = {});
  const browser = await launch(engine, engine === 'firefox' ? { prefs: { 'privacy.reduceTimerPrecision': false } } : {});
  if (!skip.has('consistent')) {
    for (const [dirName, svgs] of [['consistent', ['text']], ['consistent-paths', ['paths']]]) {
      const server = await startServer({ impl: path.join(ROOT, 'build', dirName), pages: { '/wall.html': CONSISTENT_PAGE() } });
      for (const svg of svgs) {
        for (const n of SIZES) {
          const rows = [];
          for (let i = 0; i < REPS; i++) rows.push(await consistentRun(engine, browser, server, sets[n].lines, svg));
          const pick = (k) => r1(med(rows.map((x) => x[k])));
          R[`consistent-${svg}/${n}`] = { layoutMs: pick('layoutMs'), innerHtmlMs: pick('innerHtmlMs'), browserLayoutMs: pick('browserLayoutMs'), framesMs: pick('framesMs'), toLayoutMs: pick('toLayoutMs'), wasmMemMB: r1(rows[0].wasmMemBytes / 1e6), jsHeapMB: rows[0].jsHeapBytes ? r1(rows[0].jsHeapBytes / 1e6) : null, markupMB: r1(rows[0].markupBytes / 1e6), glyphs: rows[0].glyphs, failed: rows[0].failed, allLayoutMs: rows.map((x) => r1(x.layoutMs)) };
          console.error(`${engine} consistent ${svg} ${n}: layout ${pick('layoutMs')} ms, to browser layout ${pick('toLayoutMs')} ms, wasm ${(rows[0].wasmMemBytes / 1e6).toFixed(0)} MB`);
        }
        if (v.longline) {
          for (const n of SIZES) {
            const rows = [];
            for (let i = 0; i < 1; i++) rows.push(await consistentRun(engine, browser, server, [longline(n)], svg));
            R[`consistent-${svg}/long-${n}`] = { layoutMs: r1(rows[0].layoutMs), toLayoutMs: r1(rows[0].toLayoutMs), wasmMemMB: r1(rows[0].wasmMemBytes / 1e6), glyphs: rows[0].glyphs, failed: rows[0].failed };
            console.error(`${engine} consistent ${svg} one fragment of ${n}: layout ${r1(rows[0].layoutMs)} ms`);
          }
        }
      }
      await server.close();
    }
  }
  for (const [name, impl] of [['compat', path.join(ROOT, 'build/compat')], ['upstream', 'upstream']]) {
    if (skip.has(name)) continue;
    let dead = false;
    for (const n of SIZES) {
      if (dead) { R[`${name}/${n}`] = { timedOut: true, note: 'a smaller size already timed out' }; continue; }
      const server = await startServer({ impl, transformScript: (t) => t + '\n' + INJECT, pages: { '/wall-legacy.html': LEGACY_PAGE(sets[n].lines) } });
      const timeoutS = name === 'upstream' ? Number(v['upstream-timeout']) : 600;
      const reps = name === 'upstream' ? 1 : REPS;
      const rows = [];
      for (let i = 0; i < reps && !dead; i++) {
        const r = await legacyRun(engine, browser, server, sets[n].lines, timeoutS);
        if (r.timedOut) dead = true;
        rows.push(r);
      }
      await server.close();
      R[`${name}/${n}`] = rows.some((r) => r.timedOut) ? rows.find((r) => r.timedOut) : { callMs: r1(med(rows.map((r) => r.callMs))), layoutMs: r1(med(rows.map((r) => r.layoutMs))), totalMs: r1(med(rows.map((r) => r.totalMs))), allTotalMs: rows.map((r) => r1(r.totalMs)) };
      console.error(`${engine} ${name} ${n}: ${JSON.stringify(R[`${name}/${n}`])}`);
    }
  }
  await browser.close();
}

const out = v.out ?? path.join(HERE, 'out', `wall-${platformTag()}.json`);
fs.mkdirSync(path.dirname(out), { recursive: true });
fs.writeFileSync(out, JSON.stringify(result, null, 1) + '\n');
console.log(`wrote ${out}`);
