#!/usr/bin/env node
// Does laying out in a Worker pay? `layout()` (and `parse`) touch no page, so the module runs in a Worker as it is. For
// the covered fragments, in each engine: laying them all out on the main thread against in a Worker, measured as
//  - blocked: the longest stretch the main thread did not get to run a 4 ms timer while the work went on (what a
//    visitor feels as a frozen page);
//  - latency: from asking for the layout to having every result in hand on the main thread (in the Worker's case,
//    after the structured clone of the markup, and including the Worker's own start: module, font and wasm);
//  - warm: the same with the Worker already started (a page that renders again later).
//
//   node tools/build-consistent.mjs --only consistent
//   node worker.mjs [--engines chromium,firefox,webkit] [--reps 5]
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { launch } from '../parity/lib/engines.mjs';
import { startServer } from '../parity/lib/server.mjs';
import { coveredPage } from '../parity/lib/covered.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const { values: v } = parseArgs({ options: { engines: { type: 'string', default: 'chromium,firefox,webkit' }, reps: { type: 'string', default: '5' }, coverage: { type: 'string', default: path.join(ROOT, 'build/compat') } } });
const med = (a) => { const s = a.slice().sort((x, y) => x - y); const n = s.length; return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2; };

const WORKER = `import { init, layout } from '/index.js';
let ready = init({ font: '/NewGardiner.otf' });
onmessage = async (e) => {
  await ready;
  const out = e.data.items.map((it) => { const r = layout(it.text, it.options); return r.ok ? { m: r.markup, c: r.className, w: r.width, h: r.height, b: r.border } : null; });
  postMessage(out);
};
postMessage('started');`;
const PAGE = `<!DOCTYPE html><html><head><meta charset="utf-8"></head><body><script type="module">
import { init, layout } from '/index.js';
let gap = 0, last = performance.now();
const beat = setInterval(() => { const n = performance.now(); gap = Math.max(gap, n - last); last = n; }, 4);
const measure = async (fn) => { gap = 0; last = performance.now(); const t0 = performance.now(); await fn(); const t1 = performance.now(); await new Promise((r) => setTimeout(r, 20)); return { latency: t1 - t0, blocked: gap }; };
window.__main = async (items) => {
  await init({ font: '/NewGardiner.otf' });
  return measure(async () => { for (const it of items) layout(it.text, it.options); });
};
window.__worker = async (items) => {
  const w = new Worker('/worker.js', { type: 'module' });
  const started = new Promise((r) => { w.onmessage = () => r(); });
  const run = () => new Promise((r) => { w.onmessage = (e) => r(e.data); w.postMessage({ items }); });
  const cold = await measure(async () => { await started; await run(); });
  const warm = await measure(async () => { await run(); });
  w.terminate();
  return { cold, warm };
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
// the worker script needs its own route, so a small server stands in front of the usual one
const base = await startServer({ impl: path.join(ROOT, 'build/consistent'), pages: { '/w.html': PAGE } });
const front = http.createServer(async (req, res) => {
  if (req.url === '/worker.js') { res.writeHead(200, { 'Content-Type': 'text/javascript' }); return res.end(WORKER); }
  const r = await fetch(base.url + req.url);
  res.writeHead(r.status, { 'Content-Type': r.headers.get('content-type') ?? '' });
  res.end(Buffer.from(await r.arrayBuffer()));
});
await new Promise((ok) => front.listen(0, '127.0.0.1', ok));
const url = `http://127.0.0.1:${front.address().port}`;
console.error(`${items.length} fragments`);
for (const engine of v.engines.split(',')) {
  const browser = await launch(engine);
  const rows = { main: [], cold: [], warm: [] };
  for (let r = 0; r < Number(v.reps); r++) {
    for (const mode of ['main', 'worker']) {
      const ctx = await browser.newContext({ viewport: { width: 800, height: 600 } });
      const page = await ctx.newPage();
      page.setDefaultTimeout(300000);
      await page.goto(url + '/w.html');
      await page.waitForFunction(() => window.__READY === true);
      const res = await page.evaluate(([m, i]) => (m === 'main' ? window.__main(i) : window.__worker(i)), [mode, items]);
      if (mode === 'main') rows.main.push(res); else { rows.cold.push(res.cold); rows.warm.push(res.warm); }
      await ctx.close();
    }
  }
  await browser.close();
  const f = (x) => Math.round(x);
  console.log(`${engine}, median of ${v.reps}: main thread: latency ${f(med(rows.main.map((x) => x.latency)))} ms, blocked ${f(med(rows.main.map((x) => x.blocked)))} ms | worker cold: latency ${f(med(rows.cold.map((x) => x.latency)))} ms, blocked ${f(med(rows.cold.map((x) => x.blocked)))} ms | worker warm: latency ${f(med(rows.warm.map((x) => x.latency)))} ms, blocked ${f(med(rows.warm.map((x) => x.blocked)))} ms`);
}
front.close();
await base.close();
