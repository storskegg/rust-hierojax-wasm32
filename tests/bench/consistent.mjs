#!/usr/bin/env node
// What costs the browser time when consistent mode writes fragments into a page, and what removes it. The
// covered fragments (the fragments of the real pages that compat can lay out, with their options) are laid out by
// build/consistent's `layout()` and written into `<span>`s in one page, in each way below; measured, in ms: the
// layout in wasm (the same for every way), the DOM writes, the layout the browser still owes after them (a forced
// one), and the next two frames (style, layout and paint of the whole page). Rounds are interleaved; the median is
// reported with the range.
//
//   node tools/build-consistent.mjs --only consistent
//   node consistent.mjs [--engines chromium,firefox,webkit] [--reps 5] [--out file.json]
//
// Ways:
//   base        what `render` does: class, width, height, `innerHTML`
//   contain     + `contain: layout style` on each fragment
//   containsize + `contain: size layout style` (the size is known: `width` and `height` are set)
//   cv          + `content-visibility: auto` with `contain-intrinsic-size` = the known size: the browser lays
//               out and paints only what is near the viewport
//   many        all the markup of a block of fragments in one `innerHTML` of their parent (what a page that
//               renders a whole paragraph at once could do), no per-fragment write
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { launch, platformTag } from '../parity/lib/engines.mjs';
import { startServer } from '../parity/lib/server.mjs';
import { coveredPage } from '../parity/lib/covered.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../..');
const { values: v } = parseArgs({ options: { engines: { type: 'string', default: 'chromium,firefox,webkit' }, reps: { type: 'string', default: '5' }, out: { type: 'string' }, coverage: { type: 'string', default: path.join(ROOT, 'build/compat') } } });
const med = (a) => { const s = a.slice().sort((x, y) => x - y); const n = s.length; return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2; };

const WAYS = ['base', 'contain', 'containsize', 'cv', 'many'];
const PAGE = `<!DOCTYPE html><html><head><meta charset="utf-8"></head><body><div id="root"></div>
<script type="module">
import { init, layout } from '/index.js';
await init({ font: '/NewGardiner.otf' });
await document.fonts.ready;
window.__RUN = async (items, way) => {
  const t0 = performance.now();
  const rs = items.map((it) => layout(it.text, it.options));
  const t1 = performance.now();
  const root = document.getElementById('root');
  root.innerHTML = '';
  const cells = items.map(() => { const d = document.createElement('div'); root.appendChild(d); const s = document.createElement('span'); d.appendChild(s); return s; });
  void root.offsetHeight;
  const w0 = performance.now();
  const known = (r) => r.width && r.height;
  if (way === 'many') {
    // all the fragments of ten in one write, into the parent of the first
    for (let i = 0; i < rs.length; i += 10) {
      const parent = cells[i].parentElement.parentElement;
      let html = '';
      for (let j = i; j < Math.min(i + 10, rs.length); j++) {
        const r = rs[j];
        if (!r.ok) continue;
        html += '<div><span class="hierojax' + (r.className ? ' ' + r.className : '') + (r.border ? ' hierojax-border' : '') + '"' + (r.width ? ' style="width:' + r.width + ';height:' + r.height + '"' : '') + '>' + r.markup + '</span></div>';
      }
      for (let j = i + 1; j < Math.min(i + 10, rs.length); j++) cells[j].parentElement.remove();
      cells[i].parentElement.outerHTML = html;
    }
  } else {
    for (let i = 0; i < rs.length; i++) {
      const r = rs[i];
      if (!r.ok) continue;
      const el = cells[i];
      el.classList.add('hierojax');
      if (r.className) el.classList.add(r.className);
      if (r.width) el.style.width = r.width;
      if (r.height) el.style.height = r.height;
      if (r.border) el.classList.add('hierojax-border');
      if (way === 'contain') el.style.contain = 'layout style';
      if (way === 'containsize') el.style.contain = known(r) ? 'size layout style' : 'layout style';
      if (way === 'cv' && known(r)) { el.style.contentVisibility = 'auto'; el.style.containIntrinsicSize = r.width + ' ' + r.height; }
      el.innerHTML = r.markup;
    }
  }
  const w1 = performance.now();
  void root.offsetHeight;
  const w2 = performance.now();
  await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
  const w3 = performance.now();
  return { layoutMs: t1 - t0, writeMs: w1 - w0, layoutAfterMs: w2 - w1, framesMs: w3 - w2, totalMs: w3 - w0, height: root.offsetHeight };
};
window.__READY = true;
</script></body></html>`;

const cov = await coveredPage({ coverageImpl: v.coverage, engine: 'chromium' });
const items = [...cov.html.matchAll(/<span class="hierojax"([^>]*)>([\s\S]*?)<\/span>/g)].map((m) => {
  const options = {};
  for (const a of m[1].matchAll(/data-(\w+)="([^"]*)"/g)) options[a[1]] = a[2].replace(/&quot;/g, '"').replace(/&amp;/g, '&');
  delete options.type;
  return { text: m[2].replace(/&lt;/g, '<').replace(/&amp;/g, '&'), options };
});
console.error(`${items.length} fragments`);
const server = await startServer({ impl: path.join(ROOT, 'build/consistent'), pages: { '/c.html': PAGE } });
const result = { meta: { platform: platformTag(), fragments: items.length, reps: Number(v.reps) }, engines: {} };
for (const engine of v.engines.split(',')) {
  const browser = await launch(engine);
  const rows = Object.fromEntries(WAYS.map((w) => [w, []]));
  for (let r = 0; r < Number(v.reps); r++) {
    for (const way of WAYS.slice(r % WAYS.length).concat(WAYS.slice(0, r % WAYS.length))) {
      const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
      const page = await ctx.newPage();
      page.setDefaultTimeout(300000);
      await page.goto(server.url + '/c.html');
      await page.waitForFunction(() => window.__READY === true);
      rows[way].push(await page.evaluate(([i, w]) => window.__RUN(i, w), [items, way]));
      await ctx.close();
    }
  }
  await browser.close();
  const f = (x) => (x < 100 ? x.toFixed(1) : String(Math.round(x)));
  const out = [['way', 'layout(wasm)', 'writes', 'layout owed', 'frames', 'writes..frames', 'min', 'max']];
  result.engines[engine] = {};
  for (const w of WAYS) {
    const t = rows[w].map((x) => x.totalMs);
    out.push([w, f(med(rows[w].map((x) => x.layoutMs))), f(med(rows[w].map((x) => x.writeMs))), f(med(rows[w].map((x) => x.layoutAfterMs))), f(med(rows[w].map((x) => x.framesMs))), f(med(t)), f(Math.min(...t)), f(Math.max(...t))]);
    result.engines[engine][w] = rows[w];
  }
  const wd = out[0].map((_, c) => Math.max(...out.map((r) => r[c].length)));
  console.log(`${engine}, median of ${v.reps} interleaved rounds, ms`);
  for (const r of out) console.log(r.map((x, c) => (c ? x.padStart(wd[c]) : x.padEnd(wd[c]))).join('  '));
}
await server.close();
const o = v.out ?? path.join(HERE, 'out', `consistent-${platformTag()}.json`);
fs.mkdirSync(path.dirname(o), { recursive: true });
fs.writeFileSync(o, JSON.stringify(result) + '\n');
