#!/usr/bin/env node
// Random mixed pages: pages made of corpus fragments of every kind (svg, dom and canvas;
// separated or not; every direction, shading, border) in random containers, next to text and
// each other, laid out once by upstream and once by a replacement, and compared. It is the
// volume test for the batching of `processFragmentsNow`, which the fixed pages (`pagecheck`,
// `batchedge`, `realpages`) cannot give: it finds out whether the way a page is written makes
// any difference to what it looks like.
//
//   node mixedpages.mjs --impl ../../build/compat [--levers batch,fontwait,forcebatch]
//                       [--engines chromium,firefox,webkit] [--pages 200] [--seed 1]
//                       [--max-fragments 24] [--max-codepoints 14] [--trigger-only] [--multicol]
//
// `--multicol` adds a two-column container to the ones the fragments are put in. Firefox lays out a
// multi-column block differently when it is written by the shim and by upstream (boxes as left, by up
// to 0.6 px; the DOM, canvases and boxes after a fresh layout are the same), with the shipped shim and
// with the unchanged one before this milestone, and not at all in Chromium and WebKit; so it is off.
//
// Compared per page: the canonical DOM of the body, each canvas's pixels, the bounding box of
// every fragment as the page left it and again after a fresh layout of the same DOM, the log and
// the exceptions. `--levers` (needs the test build, see tests/README.md) names the replacement's
// speed levers; without it the shim runs as shipped. Exit 1 on a difference; the first few are
// printed with the page's seed so that it can be run again alone (`--only <n>`).
import fs from 'node:fs';
import { parseArgs } from 'node:util';
import { launch } from './lib/engines.mjs';
import { startServer } from './lib/server.mjs';
import { CANON_SRC } from './lib/canon.mjs';
import { loadCorpus, mulberry32 } from './lib/runner.mjs';
import { requireTestBuild } from './lib/testbuild.mjs';

const { values: v } = parseArgs({
  options: {
    impl: { type: 'string' },
    levers: { type: 'string' },
    engines: { type: 'string', default: 'chromium,firefox,webkit' },
    pages: { type: 'string', default: '200' },
    seed: { type: 'string', default: '1' },
    only: { type: 'string' },
    'max-fragments': { type: 'string', default: '24' },
    'max-codepoints': { type: 'string', default: '14' },
    'trigger-only': { type: 'boolean' },
    'ignore-boxes': { type: 'boolean' },
    show: { type: 'string', default: '3' },
    init: { type: 'string' },
    detail: { type: 'boolean' },
    save: { type: 'string' },
    multicol: { type: 'boolean' },
  },
});
if (!v.impl) {
  console.error('usage: node mixedpages.mjs --impl <dir> [--levers a,b] [--engines ...] [--pages N] [--seed S]');
  process.exit(2);
}
const impl = v.levers != null || v.init != null ? requireTestBuild(v.impl) : v.impl;

const esc = (t) => t.replace(/&/g, '&amp;').replace(/</g, '&lt;');
const escAttr = (t) => esc(t).replace(/"/g, '&quot;');

// The fragments the pages are made of: short ones, from the groups that carry options.
const corpus = loadCorpus();
const maxCp = Number(v['max-codepoints']);
const usable = corpus.items.filter((it) => ['testsuite', 'texts', 'matrix', 'sizes', 'selectors', 'demo', 'site'].includes(it.group)
  && it.class !== 'depth' && [...it.text].length <= maxCp && [...it.text].length >= 1 && !/data-sep="-|data-fontsize="[^"]*[a-z]{3}/.test(JSON.stringify(it.attrs)));
const type = (it) => it.attrs['data-type'] || 'svg';
const isDom = (it) => type(it) !== 'canvas' && type(it) !== 'svg';
const isSep = (it) => it.attrs['data-separated'] === 'true';
const dom = usable.filter(isDom), sep = usable.filter(isSep), canvas = usable.filter((it) => type(it) === 'canvas');
const optioned = usable.filter((it) => Object.keys(it.attrs).length > 0);
const plain = usable.filter((it) => Object.keys(it.attrs).length === 0);
const domSep = usable.filter((it) => isDom(it) && isSep(it));
if (!dom.length || !sep.length) throw new Error('the corpus has no dom or separated fragments to draw from');

const CONTAINERS = [
  (c) => `<p>${c}</p>`,
  (c) => `<div>${c}</div>`,
  (c) => `<div style="width:300px">${c}</div>`,
  (c) => `<p style="line-height:2">${c}</p>`,
  (c) => `<h3>${c}</h3>`,
  (c) => `<ul><li>${c}</li></ul>`,
  (c) => `<table><tr><td>${c}</td><td>x</td></tr></table>`,
  (c) => `<div style="display:flex;gap:4px">${c}</div>`,
  (c) => `<div style="font-size:12px">text ${c} text</div>`,
  (c) => `<blockquote style="margin:0 20px"><span>${c}</span></blockquote>`,
  (c) => `<div style="display:none">${c}</div>`,
  (c) => `<div style="columns:2">${c}</div>`, // Firefox: see --multicol
];

function makePage(seed) {
  const rnd = mulberry32(seed);
  const pick = (a) => a[Math.floor(rnd() * a.length)];
  const nFrag = 2 + Math.floor(rnd() * (Number(v['max-fragments']) - 1));
  const forced = v['trigger-only'] || rnd() < 0.6; // the pages that have the trigger are the ones that matter
  const parts = [];
  let n = 0;
  const frag = (it) => {
    const attrs = Object.entries(it.attrs).map(([k, val]) => ` ${k}="${escAttr(val)}"`).join('');
    const style = `font-size:${it.fontSize};color:${it.color}`;
    return `<span class="hierojax" data-mx="${n++}"${attrs} style="${style}">${esc(it.text)}</span>`;
  };
  const draw = () => {
    const r = rnd();
    if (r < 0.3) return pick(dom);
    if (r < 0.5) return pick(sep);
    if (r < 0.58) return pick(canvas);
    if (r < 0.62) return pick(domSep);
    if (r < 0.85) return pick(optioned);
    return pick(plain);
  };
  const items = [];
  if (forced) items.push(pick(dom), pick(sep));
  while (items.length < nFrag) items.push(draw());
  // shuffle: the order of dom and separated fragments is part of what is tested
  for (let i = items.length - 1; i > 0; i--) { const j = Math.floor(rnd() * (i + 1)); [items[i], items[j]] = [items[j], items[i]]; }
  let k = 0;
  while (k < items.length) {
    const take = Math.min(items.length - k, 1 + Math.floor(rnd() * 5));
    let inner = '';
    for (let i = 0; i < take; i++) {
      const r = rnd();
      if (r < 0.15) inner += 'some text ';
      else if (r < 0.25) inner += '<br>';
      else if (r < 0.32) inner += `<span style="font-size:${10 + Math.floor(rnd() * 30)}px">small</span>`;
      inner += frag(items[k + i]);
    }
    k += take;
    parts.push(pick(v.multicol ? CONTAINERS : CONTAINERS.slice(0, -1))(inner));
    if (rnd() < 0.1) parts.push('<hr>');
  }
  // sometimes an element inside a fragment, as in `batchedge`: the outer one clears it
  return `<!DOCTYPE html><html lang="en"><head><meta charset="utf-8"><link rel="stylesheet" href="hierojax.css">
<script src="hierojax.js"></script>
<script>
window.__log = []; window.__errors = [];
for (const k of ['log', 'error', 'warn']) { const f = console[k]; console[k] = (...a) => { __log.push(k + ': ' + a.join(' ').slice(0, 300)); }; }
addEventListener('error', (e) => { __errors.push(String(e.message).slice(0, 200)); });
const pfs = HieroJax.prototype.processFragmentsNow;
HieroJax.prototype.processFragmentsNow = function () { try { return pfs.apply(this, arguments); } finally { window.__done = true; } };
addEventListener('DOMContentLoaded', () => hierojax.processFragments());
</script></head><body>
${parts.join('\n')}
</body></html>`;
}

const PARSE_ERROR = /^(log: Parse error:\n[\s\S]*?Error: )(?:Lexical error|Parse error|parse error)[\s\S]*$/;
const norm = (l) => l.map((m) => m.replace(PARSE_ERROR, '$1<parse error>'));

const SNAP = `(canonSrc) => {
  const canon = (0, eval)('(' + canonSrc + ')');
  const hash = (c) => {
    try {
      const d = c.getContext('2d').getImageData(0, 0, c.width, c.height).data;
      let h = 0;
      for (let i = 0; i < d.length; i++) h = (Math.imul(h, 31) + d[i]) | 0;
      return c.width + 'x' + c.height + ':' + h;
    } catch (e) { return c.width + 'x' + c.height + ':' + e.name; }
  };
  const frs = [...document.querySelectorAll('[data-mx]')];
  const box = () => frs.map((e) => { const r = e.getBoundingClientRect(); return [r.left, r.top + scrollY, r.width, r.height].map((x) => Math.round(x * 1000) / 1000); });
  const left = box();
  const all = [...document.body.querySelectorAll('*')].filter((e) => !e.closest('svg') || e.localName == 'svg').map((e) => { const r = e.getBoundingClientRect(); return e.localName + (e.className && !e.className.baseVal ? '.' + e.className : '') + (e.dataset.mx ? '#' + e.dataset.mx : '') + ' ' + [r.left, r.top + scrollY, r.width, r.height].map((x) => Math.round(x * 1000) / 1000).join(','); });
  document.body.style.display = 'none'; void document.body.offsetHeight; document.body.style.display = '';
  const fresh = box();
  return { all, dom: canon(document.body), canvases: [...document.querySelectorAll('canvas')].map(hash), left, fresh, log: __log, errors: __errors };
}`;

async function run(browser, impl, levers, html, init) {
  const server = await startServer({ impl, pages: { '/m.html': html } });
  const ctx = await browser.newContext({ viewport: { width: 1000, height: 700 }, deviceScaleFactor: 1 });
  if (levers != null) await ctx.addInitScript(`window.HIEROXIDE_LEVERS = ${JSON.stringify(levers)};`);
  if (init) await ctx.addInitScript(init);
  const page = await ctx.newPage();
  page.on('dialog', (d) => d.dismiss().catch(() => {}));
  page.setDefaultTimeout(60000);
  await page.goto(server.url + '/m.html', { waitUntil: 'load' });
  await page.waitForFunction(() => window.__done === true || __errors.length > 0, null, { timeout: 60000 }).catch(() => {});
  await page.waitForTimeout(30);
  const r = await page.evaluate(`(${SNAP})(${JSON.stringify(CANON_SRC)})`);
  await ctx.close();
  await server.close();
  return r;
}

const first = Number(v.seed) * 100000;
const seeds = v.only != null ? [Number(v.only)] : Array.from({ length: Number(v.pages) }, (_, i) => first + i);
let bad = 0;
for (const engine of v.engines.split(',')) {
  const browser = await launch(engine);
  const stats = { pages: 0, fragments: 0, dom: 0, sep: 0, both: 0, domSepPages: 0 };
  const diffs = { dom: 0, canvases: 0, left: 0, fresh: 0, log: 0, errors: 0 };
  const shown = [];
  let leftStale = 0;
  for (const seed of seeds) {
    const html = makePage(seed);
    if (v.save) fs.writeFileSync(v.save, html);
    const a = await run(browser, 'upstream', null, html);
    const b = await run(browser, impl, v.levers, html, v.init);
    stats.pages++;
    stats.fragments += a.left.length;
    const hasDom = /data-type="(?!canvas|svg)[^"]*"/.test(html), hasSep = /data-separated="true"/.test(html);
    if (hasDom && hasSep) stats.domSepPages++;
    if (JSON.stringify(a.left) !== JSON.stringify(a.fresh)) leftStale++;
    const bad1 = [];
    if (a.dom !== b.dom) bad1.push('dom');
    if (JSON.stringify(a.canvases) !== JSON.stringify(b.canvases)) bad1.push('canvases');
    if (JSON.stringify(a.fresh) !== JSON.stringify(b.fresh)) bad1.push('fresh');
    if (!v['ignore-boxes'] && JSON.stringify(a.left) !== JSON.stringify(b.left)) bad1.push('left');
    if (JSON.stringify(norm(a.log)) !== JSON.stringify(norm(b.log))) bad1.push('log');
    if (JSON.stringify(a.errors) !== JSON.stringify(b.errors)) bad1.push('errors');
    for (const k of bad1) diffs[k]++;
    if (bad1.length) {
      bad++;
      if (shown.length < Number(v.show)) {
        shown.push(seed);
        const i = a.left.findIndex((r, k) => JSON.stringify(r) !== JSON.stringify(b.left[k]));
        if (v.detail) { const d = a.all.map((x, k) => [x, b.all[k]]).filter(([x, y]) => x !== y); console.log('   elements that differ: ' + d.length + ' of ' + a.all.length + '\n' + d.slice(0, 8).map(([x, y]) => `     ${x}\n  vs ${y}`).join('\n')); }
        console.log(`  ${engine} page seed ${seed}: differs in ${bad1.join(', ')}` + (i >= 0 ? `; first box at fragment ${i}: ${JSON.stringify(a.left[i])} upstream, ${JSON.stringify(b.left[i])} replacement` : ''));
      }
    }
  }
  await browser.close();
  console.log(`${engine.padEnd(9)} ${stats.pages} pages, ${stats.fragments} fragments (${stats.domSepPages} pages with a dom and a separated fragment; upstream's own boxes differ from a fresh layout on ${leftStale}): `
    + (Object.values(diffs).some(Boolean) ? 'DIFFERENT ' + JSON.stringify(diffs) : 'no differences'));
}
console.log(bad ? `${bad} pages differ` : 'no differences');
process.exit(bad ? 1 : 0);
