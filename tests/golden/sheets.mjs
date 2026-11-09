#!/usr/bin/env node
// Contact sheets of consistent mode against upstream: one PNG per fragment, device scale 2, 40 px,
// white background, upstream on top, consistent below, same engine, and an index with a line for each.
// Run tests/golden/drift.mjs first, which picks the fragments.
//
//   node tests/golden/sheets.mjs --dir <directory>
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';
import { launch } from '../parity/lib/engines.mjs';
import { startServer } from '../parity/lib/server.mjs';
import { loadCorpus } from '../parity/lib/runner.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../..');
const { values: v } = parseArgs({
  options: {
    dir: { type: 'string' },
    'out-dir': { type: 'string', default: path.join(ROOT, 'tests/parity/out') },
    impl: { type: 'string', default: path.join(ROOT, 'build/consistent') },
  },
});
fs.mkdirSync(v.dir, { recursive: true });
const items = new Map(loadCorpus().items.map((it) => [it.id, it]));
const drift = JSON.parse(fs.readFileSync(path.join(v['out-dir'], 'drift-chromium-40.json'), 'utf8'));
const byClass = (c) => drift.rows.filter((r) => r.class === c && !r.structural).sort((a, b) => b.max - a.max);
const label = (r) => `${r.id}, ${r.class}, largest difference ${r.structural ? 'structural' : r.max.toFixed(2) + ' px'}`;
const known = (id) => drift.rows.find((r) => r.id === id) ?? { id, class: items.get(id).class, max: 0 };

const median3 = (xs) => {
  const m = Math.floor(xs.length / 2);
  return xs.slice(Math.max(0, m - 1), m + 2);
};
const sheets = [];
const add = (kind, rows) => rows.forEach((r, i) => sheets.push({ name: `${kind}_${i + 1}_${r.id}`, row: r, kind }));
for (const c of ['plain', 'bracket', 'insertion', 'enclosure', 'overlay', 'options']) add(`worst_${c}`, byClass(c).slice(0, 3));
add('typical_insertion', median3(byClass('insertion')));
add('typical_enclosure', median3(byClass('enclosure')));
add('site', [...items.values()].filter((it) => it.group === 'site').map((it) => known(it.id)));
add('text_largest', drift.rows.filter((r) => r.group === 'texts' && !r.structural).sort((a, b) => b.max - a.max).slice(0, 1));

const PAGE = `<!DOCTYPE html><html lang="en"><head><meta charset="utf-8"><title>sheet</title>
<link rel="stylesheet" href="hierojax.css"><script src="hierojax.js"></script>
<style>
html, body { background: white; margin: 0; }
body { width: max-content; padding: 10px 16px; font: 13px/1.3 Helvetica, Arial, sans-serif; color: #333; }
.t { margin: 6px 0 2px; color: #666; }
.row { font-size: 40px; color: black; padding: 2px 0 6px; white-space: nowrap; }
h1 { font: bold 14px Helvetica, Arial, sans-serif; margin: 0 0 6px; }
</style></head><body></body></html>`;

function draw({ title, rows }) {
  document.body.innerHTML = '';
  const h = document.createElement('h1');
  h.textContent = title;
  document.body.append(h);
  for (const r of rows) {
    const t = document.createElement('div');
    t.className = 't';
    t.textContent = r.label;
    const row = document.createElement('div');
    row.className = 'row';
    const span = document.createElement('span');
    span.className = 'hierojax';
    span.textContent = r.text;
    row.append(span);
    document.body.append(t, row);
    if (r.who === 'upstream') {
      for (const [k, x] of Object.entries(r.attrs)) span.setAttribute(k, x);
      hierojax.processFragmentNow(span);
    } else {
      const o = Object.fromEntries(Object.entries(r.attrs).map(([k, x]) => [k.replace(/^data-/, ''), x]));
      span.replaceChildren();
      const res = window.__hx.render(span, r.text, o);
      if (!res.ok) span.textContent = res.kind + ': ' + res.message;
    }
  }
}

const server = await startServer({ impl: 'upstream', pages: { '/sheet.html': PAGE } });
// The module's files come from the build, under their own names.
const moduleServer = await startServer({ impl: path.join(v.impl, 'index.js'), pages: {} });
async function open(engine) {
  const browser = await launch(engine, {});
  const ctx = await browser.newContext({ viewport: { width: 400, height: 80 }, deviceScaleFactor: 2 });
  const page = await ctx.newPage();
  await page.route('**/hx/**', async (route) => {
    const u = new URL(route.request().url());
    const res = await fetch(moduleServer.url + '/' + u.pathname.split('/').pop());
    await route.fulfill({ status: res.status, contentType: res.headers.get('content-type'), body: Buffer.from(await res.arrayBuffer()) });
  });
  await page.goto(server.url + '/sheet.html');
  await page.waitForFunction('hierojax.nFontsLoaded == hierojax.nFonts');
  await page.evaluate(async () => {
    const m = await import('/hx/index.js');
    await m.init({ font: '/NewGardiner.otf', wasm: '/hx/hieroxide_outline_bg.wasm' });
    await document.fonts.ready;
    window.__hx = m;
  });
  return { browser, page };
}
const noSize = (it) => Object.fromEntries(Object.entries(it.attrs).filter(([k]) => k !== 'data-fontsize'));

const index = [];
const chromium = await open('chromium');
for (const s of sheets) {
  const it = items.get(s.row.id);
  const attrs = noSize(it);
  await chromium.page.evaluate(draw, {
    title: label(s.row),
    rows: [
      { who: 'upstream', label: 'upstream, Chromium on this Mac', text: it.text, attrs },
      { who: 'consistent', label: 'consistent, Chromium', text: it.text, attrs },
    ],
  });
  const file = s.name + '.png';
  await chromium.page.screenshot({ path: path.join(v.dir, file), fullPage: true });
  index.push(`${file}  ${label(s.row)}`);
}
await chromium.browser.close();

const worst = byClass('insertion')[0];
if (worst) {
  const webkit = await open('webkit');
  const it = items.get(worst.id);
  const attrs = noSize(it);
  await webkit.page.evaluate(draw, {
    title: `WebKit: ${label(worst)} (the difference is against upstream in Chromium)`,
    rows: [
      { who: 'upstream', label: 'upstream, WebKit on this Mac', text: it.text, attrs },
      { who: 'consistent', label: 'consistent, WebKit', text: it.text, attrs },
    ],
  });
  const file = `webkit_worst_insertion_${it.id}.png`;
  await webkit.page.screenshot({ path: path.join(v.dir, file), fullPage: true });
  index.push(`${file}  WebKit: ${label(worst)}`);
  await webkit.browser.close();
}
await server.close();
await moduleServer.close();
fs.writeFileSync(path.join(v.dir, 'index.txt'), index.join('\n') + '\n');
console.log(`${index.length} sheets in ${v.dir}`);
