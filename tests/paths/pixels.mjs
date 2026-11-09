#!/usr/bin/env node
// Path blocks against consistent mode's text blocks, as pixels, in the three engines.
//
//   cargo build --release -p hieroxide-cli
//   node tests/paths/pixels.mjs [--engines chromium,firefox,webkit] [--per-class 10] [--out tests/paths/out/pixels]
//         [--max-mismatch 0.12]
//
// The two cannot be identical: the engine rasterizes a font with its own anti-aliasing and hinting
// (on a Mac, with font smoothing that thickens strokes), where a path is filled as it is. So the
// comparison is of where the ink is, not of its pixels:
//
//   ink      darkness of a pixel: 1 - luminance (the blocks are black on white; shadings are gray)
//   mark     a pixel with ink above 0.25
//   mismatch the marks of either block that have no mark of the other within 1 CSS px (2 device
//            pixels: the page is drawn at a device scale of 2), as a share of all the marks of
//            both. Anti-aliasing and hinting move an edge by a pixel or less, so they do not count; a sign
//            drawn somewhere else, or a stroke that is missing, does.
//
// A block fails if its mismatch is above --max-mismatch, or if its ink is under 0.7 or over 1.25 of the text's
// (a fifth either way: the engine's font smoothing thickens strokes, so the path's is a little lighter).
// The default is set from the measurements this
// script prints (the worst block of each class and engine, with its picture in --out), with room above the
// worst for another machine's rasterizer: see tests/README.md.
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { launch } from '../parity/lib/engines.mjs';
import { FONT, HERE, ROOT, batchOptions, loadCorpus, renderBatch, servePages } from './lib.mjs';

const { values: v } = parseArgs({
  options: {
    engines: { type: 'string', default: 'chromium,firefox,webkit' },
    'per-class': { type: 'string', default: '10' },
    out: { type: 'string', default: path.join(HERE, 'out/pixels') },
    'max-mismatch': { type: 'string', default: '0.08' },
  },
});
fs.mkdirSync(v.out, { recursive: true });

const corpus = loadCorpus().filter((f) => f.attrs.type !== 'canvas' && f.attrs.type !== 'dom' && [...f.text].length <= 40);
const pick = (cls, n) => {
  const of = corpus.filter((f) => f.class === cls);
  const step = Math.max(1, Math.floor(of.length / n));
  return of.filter((_, i) => i % step === 0).slice(0, n);
};
const items = [];
for (const cls of ['plain', 'insertion', 'enclosure', 'overlay', 'bracket', 'options']) {
  for (const f of pick(cls, Number(v['per-class']))) items.push({ cls, id: f.id, text: f.text, ...batchOptions(f.attrs) });
}
// What the classes do not isolate: direction, separated, rotation and mirror, shading of both kinds.
const damaged = corpus.filter((f) => /[\u{13447}-\u{13455}]/u.test(f.text) && f.class === 'plain' && [...f.text].length < 12);
const rotated = corpus.filter((f) => f.group === 'rotsample').filter((_, i) => i % 90 === 0).slice(0, 8);
const plainShort = corpus.filter((f) => f.class === 'plain' && f.group === 'texts' && [...f.text].length > 8 && [...f.text].length < 24).filter((_, i) => i % 40 === 0).slice(0, 4);
for (const f of plainShort) for (const [name, o] of [['hrl', { dir: 'hrl' }], ['vlr', { dir: 'vlr' }], ['separated', { separated: true }], ['sep-hrl', { separated: true, dir: 'hrl' }], ['border', { border: true }]]) items.push({ cls: name, id: f.id, text: f.text, ...o });
for (const f of damaged.slice(0, 6)) for (const shading of ['uniform', 'hatching']) items.push({ cls: `shade-${shading}`, id: f.id, text: f.text, shading });
for (const f of rotated) items.push({ cls: 'rotation', id: f.id, text: f.text });

const line = ({ cls, id, ...o }) => o;
const paths = renderBatch(items.map((i) => ({ ...line(i), label: 'x' })));
const texts = renderBatch(items.map((i) => ({ ...line(i), kind: 'text' })));
const use = items.map((_, i) => paths[i].ok && texts[i].ok);

const moduleJs = fs.readFileSync(path.join(ROOT, 'js/hieroxide/index.js'), 'utf8');
const CSS = moduleJs.match(/const CSS = `([^`]*)`/)[1];
const cell = (i, inner, extra = '') => `<div class="cell" data-i="${i}" style="margin:14px;font-size:60px;line-height:1.3;${extra}"><span class="blk">${inner}</span></div>`;
// A text block is what `render` makes of an element: class, and a size for a separated one.
const textBlock = (i) => {
  const o = items[i];
  const cls = (o.dir === 'hrl' ? 'hierojax-hrl-container' : o.dir === 'vlr' || o.dir === 'vrl' ? 'hierojax-v-container' : o.separated ? 'hierojax-hlr-container' : '');
  return o.separated ? `<span class="hierojax ${cls}${o.border ? ' hierojax-border' : ''}">${texts[i].svg}</span>` : `<span class="hierojax">${texts[i].svg}</span>`;
};
const files = {
  '/text.html': `<!doctype html><meta charset="utf-8"><title>text</title><style>@font-face{font-family:Hieroglyphic;src:url(/NewGardiner.otf)}${CSS}</style><body style="margin:0;background:#fff">${items.map((_, i) => (use[i] ? cell(i, textBlock(i)) : '')).join('')}</body>`,
  '/paths.html': `<!doctype html><meta charset="utf-8"><title>paths</title><body style="margin:0;background:#fff">${items.map((_, i) => (use[i] ? cell(i, paths[i].svg) : '')).join('')}</body>`,
  '/NewGardiner.otf': fs.readFileSync(FONT),
};
const srv = await servePages(files);

// In a page of its own: decode two PNGs, compare as described above, and draw A | B | marks only in one.
const compare = async (page, a, b, wantPicture) =>
  page.evaluate(async ({ a, b, wantPicture }) => {
    const load = async (s) => {
      const bmp = await createImageBitmap(await (await fetch(`data:image/png;base64,${s}`)).blob());
      const c = new OffscreenCanvas(bmp.width, bmp.height);
      const g = c.getContext('2d');
      g.drawImage(bmp, 0, 0);
      return g.getImageData(0, 0, bmp.width, bmp.height);
    };
    const [A, B] = [await load(a), await load(b)];
    const w = Math.min(A.width, B.width), h = Math.min(A.height, B.height);
    const ink = (img, x, y) => {
      const o = (y * img.width + x) * 4;
      return 1 - (0.2126 * img.data[o] + 0.7152 * img.data[o + 1] + 0.0722 * img.data[o + 2]) / 255;
    };
    const T = 0.25, R = 2;
    const ma = new Uint8Array(w * h), mb = new Uint8Array(w * h);
    let inkA = 0, inkB = 0;
    for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
      const p = ink(A, x, y), q = ink(B, x, y);
      inkA += p; inkB += q;
      ma[y * w + x] = p > T ? 1 : 0;
      mb[y * w + x] = q > T ? 1 : 0;
    }
    const near = (m, x, y) => {
      for (let dy = -R; dy <= R; dy++) for (let dx = -R; dx <= R; dx++) {
        const xx = x + dx, yy = y + dy;
        if (xx >= 0 && yy >= 0 && xx < w && yy < h && m[yy * w + xx]) return true;
      }
      return false;
    };
    let marks = 0, bad = 0, onlyA = 0, onlyB = 0;
    const diff = wantPicture ? new Uint8Array(w * h) : null;
    for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
      const i = y * w + x;
      if (ma[i]) { marks++; if (!near(mb, x, y)) { bad++; onlyA++; if (diff) diff[i] = 1; } }
      if (mb[i]) { marks++; if (!near(ma, x, y)) { bad++; onlyB++; if (diff) diff[i] = 2; } }
    }
    let picture = null;
    if (wantPicture) {
      const c = new OffscreenCanvas(w * 3 + 8, h);
      const g = c.getContext('2d');
      g.fillStyle = '#fff'; g.fillRect(0, 0, c.width, c.height);
      g.putImageData(new ImageData(new Uint8ClampedArray(A.data.buffer.slice(0)), A.width, A.height), 0, 0, 0, 0, w, h);
      g.putImageData(new ImageData(new Uint8ClampedArray(B.data.buffer.slice(0)), B.width, B.height), w + 4, 0, 0, 0, w, h);
      const d = g.createImageData(w, h);
      for (let i = 0; i < w * h; i++) {
        const k = i * 4;
        const base = ma[i] || mb[i] ? 200 : 255;
        d.data[k] = diff[i] === 1 ? 220 : base; d.data[k + 1] = diff[i] ? 0 : base; d.data[k + 2] = diff[i] === 2 ? 220 : base; d.data[k + 3] = 255;
      }
      g.putImageData(d, 2 * w + 8, 0);
      const blob = await c.convertToBlob({ type: 'image/png' });
      const buf = new Uint8Array(await blob.arrayBuffer());
      let s = '';
      for (const x of buf) s += String.fromCharCode(x);
      picture = btoa(s);
    }
    return { mismatch: marks ? bad / marks : 0, onlyA, onlyB, marks, inkRatio: inkA ? inkB / inkA : 1, size: [A.width, A.height, B.width, B.height] };
  }, { a, b, wantPicture });

const results = [];
let failures = 0;
const maxMismatch = Number(v['max-mismatch']);
for (const eng of v.engines.split(',')) {
  const browser = await launch(eng);
  const ctx = await browser.newContext({ viewport: { width: 1000, height: 800 }, deviceScaleFactor: 2 });
  const shots = { text: [], paths: [] };
  for (const kind of ['text', 'paths']) {
    const page = await ctx.newPage();
    await page.goto(`${srv.url}/${kind}.html`);
    await page.evaluate(() => document.fonts.ready);
    await page.waitForTimeout(300);
    for (let i = 0; i < items.length; i++) {
      if (!use[i]) { shots[kind].push(null); continue; }
      const loc = page.locator(`.cell[data-i="${i}"] .blk`);
      await loc.scrollIntoViewIfNeeded();
      // The union of the svgs, 6 px around: the same size in both pages.
      const box = await page.evaluate((i) => {
        // A block's border is on its wrapper, or on the svg itself in text mode: take the box that has it.
        const svgs = [...document.querySelectorAll(`.cell[data-i="${i}"] svg`)].map((s) => (s.closest('[style*="border"]') ?? s).getBoundingClientRect());
        const x0 = Math.min(...svgs.map((r) => r.left)), y0 = Math.min(...svgs.map((r) => r.top));
        const x1 = Math.max(...svgs.map((r) => r.right)), y1 = Math.max(...svgs.map((r) => r.bottom));
        return { x: x0 + scrollX, y: y0 + scrollY, w: x1 - x0, h: y1 - y0 };
      }, i);
      shots[kind].push({ box, png: (await page.screenshot({ clip: { x: Math.max(0, box.x - 6), y: Math.max(0, box.y - 6), width: Math.ceil(box.w + 12), height: Math.ceil(box.h + 12) }, fullPage: true })).toString('base64') });
    }
    await page.close();
  }
  const cmp = await ctx.newPage();
  await cmp.goto(`${srv.url}/NewGardiner.otf`).catch(() => {});
  for (let i = 0; i < items.length; i++) {
    if (!use[i]) continue;
    const a = shots.text[i], b = shots.paths[i];
    const r = await compare(cmp, a.png, b.png, false);
    results.push({ engine: eng, cls: items[i].cls, id: items[i].id, i, ...r, boxText: a.box, boxPaths: b.box });
  }
  // Pictures of the worst of each class.
  const worst = {};
  for (const r of results.filter((x) => x.engine === eng)) if (!worst[r.cls] || r.mismatch > worst[r.cls].mismatch) worst[r.cls] = r;
  for (const [cls, r] of Object.entries(worst)) {
    const png = await cmp.evaluate(async () => null);
    void png;
    const a = shots.text[r.i], b = shots.paths[r.i];
    const picture = await cmp.evaluate(async ({ a, b }) => {
      const load = async (s) => {
        const bmp = await createImageBitmap(await (await fetch(`data:image/png;base64,${s}`)).blob());
        return bmp;
      };
      const [A, B] = [await load(a), await load(b)];
      const w = Math.min(A.width, B.width), h = Math.min(A.height, B.height);
      const c = new OffscreenCanvas(w * 2 + 6, h);
      const g = c.getContext('2d');
      g.fillStyle = '#c33'; g.fillRect(0, 0, c.width, c.height);
      g.drawImage(A, 0, 0); g.drawImage(B, w + 6, 0);
      const buf = new Uint8Array(await (await c.convertToBlob({ type: 'image/png' })).arrayBuffer());
      let s = '';
      for (const x of buf) s += String.fromCharCode(x);
      return btoa(s);
    }, { a: a.png, b: b.png });
    fs.writeFileSync(path.join(v.out, `${eng}-${cls}-worst-${r.id}.png`), Buffer.from(picture, 'base64'));
  }
  await cmp.close();
  await browser.close();
}
await srv.close();

const med = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];
const rows = [];
for (const eng of v.engines.split(',')) {
  for (const cls of [...new Set(results.map((r) => r.cls))]) {
    const rs = results.filter((r) => r.engine === eng && r.cls === cls);
    if (!rs.length) continue;
    const w = rs.reduce((a, b) => (b.mismatch > a.mismatch ? b : a));
    rows.push({ engine: eng, class: cls, blocks: rs.length, median: med(rs.map((r) => r.mismatch)).toFixed(4), worst: w.mismatch.toFixed(4), worstId: w.id, inkRatio: med(rs.map((r) => r.inkRatio)).toFixed(3), boxDiffPx: Math.max(...rs.map((r) => Math.abs(r.boxText.w - r.boxPaths.w) + Math.abs(r.boxText.h - r.boxPaths.h))).toFixed(2) });
  }
}
console.table(rows);
// The ink of a path is within a fifth of the ink of the font's text (the engine's smoothing thickens it).
const over = results.filter((r) => r.mismatch > maxMismatch || r.inkRatio < 0.7 || r.inkRatio > 1.25);
fs.writeFileSync(path.join(v.out, 'results.json'), JSON.stringify(results, null, 1));
console.log(`${results.length} block pairs; ${over.length} above ${maxMismatch}; worst ${Math.max(...results.map((r) => r.mismatch)).toFixed(4)}; pictures of the worst of each class in ${v.out}`);
if (over.length) {
  for (const r of over.slice(0, 10)) console.error(`  ${r.engine} ${r.cls} ${r.id}: ${r.mismatch.toFixed(4)}`);
  process.exit(1);
}
