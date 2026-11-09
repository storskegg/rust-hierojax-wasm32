#!/usr/bin/env node
// The drift report: consistent mode against upstream, at one font size, in one engine. Not a gate; it
// is what the plan calls for at each release, and what picks the contact sheets.
//
//   node tests/golden/drift.mjs [--engine chromium] [--px 40] [--out-dir tests/parity/out]
//
// Upstream's run is cached in <out-dir>/upstream-<engine>-<px>.json.gz. Writes
// <out-dir>/drift-<engine>-<px>.json: per fragment, the largest difference in px.
// `same` below means every number within 0.02 px: upstream's own are rounded to 0.01. A difference of
// up to 0.1 px is mostly upstream's scales having two decimals, against four here.
// Fragments with `data-fontsize` are run at this size too: it is taken off.
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';
import { launch } from '../parity/lib/engines.mjs';
import { startServer } from '../parity/lib/server.mjs';
import { runCorpus, loadCorpus as loadParity, readJson, writeJson } from '../parity/lib/runner.mjs';
import { parseCanonical, parseMarkup, difference } from './lib/signs.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../..');
const { values: v } = parseArgs({
  options: {
    engine: { type: 'string', default: 'chromium' },
    px: { type: 'string', default: '40' },
    'out-dir': { type: 'string', default: path.join(ROOT, 'tests/parity/out') },
    impl: { type: 'string', default: path.join(ROOT, 'build/consistent') },
  },
});
const px = Number(v.px);
const parity = loadParity();
const items = parity.items.map((it) => {
  const attrs = { ...it.attrs };
  delete attrs['data-fontsize'];
  return { ...it, attrs, fontSize: px + 'px' };
});

const cache = path.join(v['out-dir'], `upstream-${v.engine}-${px}.json.gz`);
let up;
if (fs.existsSync(cache)) up = readJson(cache).results;
else {
  const r = await runCorpus({ impl: 'upstream', engine: v.engine, runId: String(px), corpus: { items, sha256: `${parity.sha256}@${px}` }, log: console.error });
  writeJson(cache, r);
  up = r.results;
}

// Consistent mode's markup, from the module in the same engine.
const server = await startServer({
  impl: path.join(v.impl, 'index.js'),
  pages: { '/golden.html': '<!DOCTYPE html><html lang="en"><head><meta charset="utf-8"><title>drift</title><link rel="stylesheet" href="hierojax.css"></head><body></body></html>' },
});
const browser = await launch(v.engine, {});
const page = await (await browser.newContext({ viewport: { width: 1280, height: 800 } })).newPage();
await page.goto(server.url + '/golden.html');
await page.evaluate(async () => {
  const m = await import('/index.js');
  await m.init({ font: '/NewGardiner.otf' });
  window.__hx = m;
});
const ATTR = (a) => Object.fromEntries(Object.entries(a).map(([k, x]) => [k.replace(/^data-/, ''), x]));
const ours = new Map();
for (let i = 0; i < items.length; i += 500) {
  const res = await page.evaluate((chunk) => chunk.map((c) => {
    const r = window.__hx.layout(c.text, c.attrs);
    if (!r.ok) return { kind: r.kind };
    const span = document.createElement('span');
    span.style.fontSize = '40px';
    document.body.append(span);
    window.__hx.render(span, c.text, c.attrs);
    const sel = getSelection();
    sel.removeAllRanges();
    const range = document.createRange();
    range.selectNodeContents(span);
    sel.addRange(range);
    const selected = sel.toString();
    sel.removeAllRanges();
    span.remove();
    return { markup: r.markup, text: r.text, selected };
  }), items.slice(i, i + 500).map((it) => ({ text: it.text, attrs: ATTR(it.attrs) })));
  res.forEach((r, j) => ours.set(items[i + j].id, r));
}
await browser.close();
await server.close();

const stat = () => ({ n: 0, same: 0, structural: 0, upThrew: 0, max: 0, sum: 0, hist: [0, 0, 0, 0, 0] });
const classes = {}, groups = {};
const rows = [];
let selSame = 0, selDiffer = 0, selLost = 0;
const VS_AFTER_LOST = /([\u{13443}-\u{13446}])[\uFE00-\uFE06]/gu;
const typeOf = (it) => it.attrs['data-type'];
for (const it of items) {
  const u = up[it.id], o = ours.get(it.id);
  const t = typeOf(it);
  // Upstream's other printers have other output: no numbers to set against ours.
  if (t && t !== 'svg') continue;
  const cs = (classes[it.class] ??= stat()), gs = (groups[it.group] ??= stat()), al = (classes.ALL ??= stat());
  let d;
  if (!o.markup) {
    d = { max: 0, structural: !u.console?.some((m) => m.includes('Parse error')) };
    if (!d.structural) {
      for (const s of [cs, gs, al]) { s.n++; s.same++; }
      continue;
    }
  } else if (u.err) {
    for (const s of [cs, gs, al]) { s.n++; s.upThrew++; }
    continue;
  } else {
    d = difference(parseCanonical(u.html), parseMarkup(o.markup, px));
    if (u.sel === o.selected) selSame++;
    // Upstream writes U+FE00 after a lost sign for any selector: that one is fixed on purpose.
    else if (u.sel.replace(VS_AFTER_LOST, '$1') === o.selected.replace(VS_AFTER_LOST, '$1')) selLost++;
    else selDiffer++;
  }
  const same = !d.structural && d.max <= 0.02;
  for (const s of [cs, gs, al]) {
    s.n++;
    if (same) s.same++;
    else if (d.structural) s.structural++;
    else {
      s.max = Math.max(s.max, d.max);
      s.hist[d.max <= 0.1 ? 0 : d.max <= 0.5 ? 1 : d.max <= 1 ? 2 : d.max <= 5 ? 3 : 4]++;
    }
  }
  if (!same) rows.push({ id: it.id, class: it.class, group: it.group, max: d.max, structural: d.structural });
}
const fmt = ([k, s]) => `${k.padEnd(12)} ${String(s.n).padStart(6)} same ${String(s.same).padStart(6)} ${((100 * s.same) / Math.max(1, s.n + -s.upThrew)).toFixed(2).padStart(7)}%  largest ${s.max.toFixed(2).padStart(6)} px  <=0.1/<=0.5/<=1/<=5/>5: ${s.hist.join('/')}  structural ${s.structural}  upstream threw ${s.upThrew}`;
console.log(`consistent against upstream in ${v.engine} at ${px} px; same = within 0.02 px; the share leaves out what upstream throws on`);
console.log(Object.entries(classes).sort().map(fmt).join('\n'));
console.log(Object.entries(groups).sort().map(fmt).join('\n'));
console.log(`selected text: ${selSame} as upstream, ${selLost} differing only in a lost sign's variation selector, ${selDiffer} otherwise`);
fs.writeFileSync(path.join(v['out-dir'], `drift-${v.engine}-${px}.json`), JSON.stringify({ px, engine: v.engine, classes, groups, rows }));
