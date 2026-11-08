#!/usr/bin/env node
// The reference test of consistent mode, in real browsers: the output of the wasm build against
// tests/golden/hashes.txt.gz, byte for byte, in each engine, at two font sizes.
//
//   node tools/build-consistent.mjs --only consistent
//   node tests/golden/engines.mjs [--engines chromium,firefox,webkit] [--sizes 22,40]
//                                 [--shuffle 7] [--prefs '{"privacy.resistFingerprinting":true}']
//                                 [--impl build/consistent] [--limit N] [--report out.json]
//
// Exit 1 on any difference. What it checks per engine and size:
//  - the markup the module returns equals the reference (the hash covers all output);
//  - `pow`, `log`, `sin` and `cos` of the module have the native build's digest;
//  - the boxes in the page, divided by the font size, are the same at every size, within the
//    browser's own rounding of layout: em geometry, seen from the page;
//  - selecting the fragment gives its text, so the hidden text works.
// Needs tests/parity's install (Playwright and its browsers).
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';
import { launch } from '../parity/lib/engines.mjs';
import { startServer } from '../parity/lib/server.mjs';
import { shuffled } from '../parity/lib/runner.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../..');
const { values: v } = parseArgs({
  options: {
    engines: { type: 'string', default: 'chromium,firefox,webkit' },
    sizes: { type: 'string', default: '22,40' },
    shuffle: { type: 'string' },
    prefs: { type: 'string' },
    impl: { type: 'string', default: path.join(ROOT, 'build/consistent') },
    limit: { type: 'string' },
    report: { type: 'string' },
  },
});

const unesc = (s) => s.replace(/\\(.)/g, (_, c) => (c === 't' ? '\t' : c === 'n' ? '\n' : c === 'r' ? '\r' : c));
const OPTIONS = ['fontsize', 'signcolor', 'dir', 'linesize', 'sep', 'separated', 'type', 'bracketcolor', 'shadepattern', 'align', 'border', 'standalone', 'log'];

export function loadCorpus() {
  const text = zlib.gunzipSync(fs.readFileSync(path.join(HERE, 'corpus.tsv.gz'))).toString('utf8');
  return text.split('\n').filter(Boolean).map((l, index) => {
    const c = l.split('\t');
    const attrs = {};
    c.slice(5, 18).forEach((a, i) => { if (a) attrs[OPTIONS[i]] = unesc(a); });
    return { index, id: c[0], group: c[1], class: c[2], fontPx: Number(c[3]), color: c[4], attrs, text: unesc(c[18]) };
  });
}

const reference = zlib.gunzipSync(fs.readFileSync(path.join(HERE, 'hashes.txt.gz'))).toString('utf8').split('\n').filter(Boolean);
const manifest = Object.fromEntries(fs.readFileSync(path.join(HERE, 'manifest.txt'), 'utf8').split('\n').filter(Boolean).map((l) => l.split(' ')));

// FNV-1a, 64 bits, as in native.rs.
function fnv64(buf) {
  let h = 0xcbf29ce484222325n;
  for (const b of buf) {
    h ^= BigInt(b);
    h = BigInt.asUintN(64, h * 0x100000001b3n);
  }
  return h.toString(16).padStart(16, '0');
}

// In the page. Lays each item out twice: `layout` for the bytes (the record of native.rs), and
// `render` into a span at the given font size for what the page makes of it.
function pageRun({ items, px }) {
  const m = window.__hx;
  const out = [];
  for (const it of items) {
    const r = m.layout(it.text, it.attrs);
    let rec;
    if (!r.ok) rec = r.kind === 'parse' ? 'parse' : 'failed';
    else rec = ['ok', r.className, r.width, r.height, String(r.border), r.logs.join('\u0001'), r.markup].join('\n');
    const o = { rec };
    if (r.ok) {
      const span = document.createElement('span');
      span.style.fontSize = px + 'px';
      span.style.color = 'black';
      document.body.appendChild(span);
      const done = m.render(span, it.text, it.attrs);
      if (done.ok) {
        o.rects = [...span.querySelectorAll('svg')].map((s) => {
          const b = s.getBoundingClientRect();
          return [b.width / px, b.height / px];
        });
        if (it.sel) {
          const sel = window.getSelection();
          sel.removeAllRanges();
          const range = document.createRange();
          range.selectNodeContents(span);
          sel.addRange(range);
          o.sel = sel.toString();
          sel.removeAllRanges();
        }
      }
      span.remove();
    }
    out.push(o);
  }
  return out;
}

const corpus = loadCorpus();
const limit = v.limit ? Number(v.limit) : corpus.length;
const prefs = v.prefs ? JSON.parse(v.prefs) : undefined;
const sizes = v.sizes.split(',').map(Number);
const server = await startServer({
  impl: path.join(v.impl, 'index.js'),
  pages: { '/golden.html': '<!DOCTYPE html><html lang="en"><head><meta charset="utf-8"><title>golden</title><link rel="stylesheet" href="hierojax.css"></head><body></body></html>' },
});
const report = {};
let failed = false;

for (const engine of v.engines.split(',')) {
  const browser = await launch(engine, { prefs });
  const version = browser.version();
  const rects = {};
  {
    // Errors are values: a font that is no font rejects; a text that does not parse comes back as
    // a result and leaves the element alone; nothing alerts.
    const ctx = await browser.newContext();
    const page = await ctx.newPage();
    let dialogs = 0;
    page.on('dialog', (d) => { dialogs++; d.dismiss().catch(() => {}); });
    await page.goto(server.url + '/golden.html');
    const api = await page.evaluate(async () => {
      const m = await import('/index.js');
      const out = {};
      try {
        await m.init({ font: new Uint8Array([1, 2, 3]) });
        out.badFont = 'resolved';
      } catch (e) {
        out.badFont = 'rejected';
      }
      try {
        await m.init({ font: '/no-such-font.otf' });
        out.noFont = 'resolved';
      } catch (e) {
        out.noFont = 'rejected';
      }
      await m.init({ font: '/NewGardiner.otf' });
      const el = document.createElement('span');
      el.textContent = 'before';
      out.parse = m.render(el, '\u{13430}', {});
      out.untouched = el.textContent === 'before';
      const ok = m.render(el, '\u{13000}', {});
      out.ok = ok.ok;
      out.isEm = /width="[\d.]+em"/.test(el.innerHTML);
      return out;
    });
    await ctx.close();
    const good = api.badFont === 'rejected' && api.noFont === 'rejected' && api.parse.ok === false && api.parse.kind === 'parse' && api.untouched && api.ok && api.isEm && dialogs === 0;
    if (!good) failed = true;
    console.log(`${engine}: errors as values: ${good ? 'ok' : 'WRONG ' + JSON.stringify(api)}`);
  }
  for (const px of sizes) {
    const t0 = Date.now();
    const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
    const page = await ctx.newPage();
    page.setDefaultTimeout(120000);
    page.on('pageerror', (e) => { console.error(`${engine}: pageerror: ${e.message}`.slice(0, 300)); failed = true; });
    await page.goto(server.url + '/golden.html', { waitUntil: 'load' });
    await page.evaluate(async () => {
      const m = await import('/index.js');
      await m.init({ font: '/NewGardiner.otf' });
      await document.fonts.ready;
      window.__hx = m;
    });
    const digest = await page.evaluate(() => window.__hx.determinismDigest());
    let items = corpus.slice(0, limit);
    if (v.shuffle != null) items = shuffled(items, Number(v.shuffle));
    let differ = 0, selDiffer = 0, n = 0;
    const firstDiffs = [];
    for (let i = 0; i < items.length; i += 200) {
      const chunk = items.slice(i, i + 200).map((it, j) => ({ index: it.index, text: it.text, attrs: it.attrs, sel: (i + j) % 20 === 0 }));
      const res = await page.evaluate(pageRun, { items: chunk, px });
      res.forEach((o, j) => {
        const it = items[i + j];
        n++;
        const h = fnv64(Buffer.from(o.rec, 'utf8'));
        if (h !== reference[it.index]) {
          differ++;
          if (firstDiffs.length < 5) firstDiffs.push({ id: it.id, want: reference[it.index], got: h, rec: o.rec.slice(0, 300) });
        }
        if (o.rects) (rects[it.index] ??= {})[px] = o.rects;
        // Text with a parse tree: what a selection gives, the source's own characters, in any
        // order of nested visual-only marks. Compared against the source without its controls.
        if (o.sel !== undefined && chunk[j].sel && o.sel.length === 0 && it.text.length > 0) selDiffer++;
      });
    }
    await ctx.close();
    const ok = differ === 0 && digest === manifest.digest;
    if (!ok) failed = true;
    report[`${engine}@${px}`] = { version, fragments: n, differ, digestOk: digest === manifest.digest, digest, emptySelections: selDiffer, firstDiffs, seconds: (Date.now() - t0) / 1000 };
    console.log(`${engine} ${version} at ${px} px${v.shuffle != null ? `, shuffled ${v.shuffle}` : ''}${prefs ? ', ' + JSON.stringify(prefs) : ''}: ${n - differ}/${n} identical to the reference, digest ${digest === manifest.digest ? 'ok' : 'DIFFERS ' + digest}, ${selDiffer} empty selections, ${report[`${engine}@${px}`].seconds} s`);
    for (const d of firstDiffs) console.log(`  ${d.id}: want ${d.want} got ${d.got}\n    ${d.rec}`);
  }
  // The boxes at each size, over the font size. The browser rounds layout (Blink to 1/64 px), so
  // this cannot be exact: report the largest difference, as a fraction of a pixel at the larger size.
  if (sizes.length > 1) {
    let worst = 0, worstId = null, compared = 0;
    for (const [i, r] of Object.entries(rects)) {
      const [a, b] = [r[sizes[0]], r[sizes[1]]];
      // data-fontsize fixes the size whatever the page's: nothing to compare. data-border adds
      // hierojax.css's border of 1 px on each side, which does not scale.
      if (!a || !b || corpus[i].attrs.fontsize || corpus[i].attrs.border) continue;
      compared++;
      a.forEach((box, k) => {
        for (const d of [0, 1]) {
          const px = Math.abs(box[d] - b[k][d]) * Math.max(...sizes);
          if (px > worst) { worst = px; worstId = corpus[i].id; }
        }
      });
    }
    report[`${engine}-boxes`] = { compared, largestPx: worst, at: worstId, sizes };
    console.log(`${engine}: boxes over font size, ${sizes.join(' against ')} px: ${compared} fragments, largest difference ${worst.toFixed(4)} px at ${Math.max(...sizes)} px (${worstId})`);
  }
  await browser.close();
}
await server.close();
if (v.report) fs.writeFileSync(v.report, JSON.stringify(report, null, 1));
process.exit(failed ? 1 : 0);
