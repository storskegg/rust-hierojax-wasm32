#!/usr/bin/env node
// Replays the draws `fit record` wrote on real canvases, one by one, and writes which pixels came
// out dark: the ground truth `fit score` compares the outline raster with.
//
//   node tests/golden/replay.mjs <draws.txt> <truth.bin> [--engine chromium]
//
// Each record: u32 x, y, w, h of the bounding box of the dark pixels, then its bits, row by row,
// low bit first. Needs tests/parity's install (Playwright).
import fs from 'node:fs';
import { launch } from '../parity/lib/engines.mjs';
import { startServer } from '../parity/lib/server.mjs';
import { READY_DEFAULT } from '../parity/lib/runner.mjs';

const [draws, out] = process.argv.slice(2);
const engine = process.argv.includes('--engine') ? process.argv[process.argv.indexOf('--engine') + 1] : 'chromium';
const lines = fs.readFileSync(draws, 'utf8').split('\n').filter(Boolean).map((l) => l.split(' ').map(Number));

function replay(chunk) {
  const out = [];
  for (const [w, h, cp, size, tx, ty, sx, sy, rot, x, y] of chunk) {
    const canvas = document.createElement('canvas');
    canvas.getContext('2d', { willReadFrequently: true });
    canvas.width = w;
    canvas.height = h;
    const ctx = canvas.getContext('2d', { willReadFrequently: true });
    if (!canvas.width || !canvas.height) { out.push(''); continue; }
    ctx.save();
    ctx.font = size.toString() + 'px Hieroglyphic';
    ctx.fillStyle = 'black';
    ctx.textBaseline = 'alphabetic';
    ctx.translate(tx, ty);
    ctx.scale(sx, sy);
    ctx.rotate(rot);
    ctx.fillText(String.fromCodePoint(cp), x, y);
    ctx.restore();
    const W = canvas.width, H = canvas.height;
    const d = ctx.getImageData(0, 0, W, H).data;
    let x0 = W, y0 = H, x1 = 0, y1 = 0;
    for (let yy = 0; yy < H; yy++) for (let xx = 0; xx < W; xx++) {
      if (d[(yy * W + xx) * 4 + 3]) { if (xx < x0) x0 = xx; if (xx >= x1) x1 = xx + 1; if (yy < y0) y0 = yy; if (yy >= y1) y1 = yy + 1; }
    }
    const bw = x1 > x0 ? x1 - x0 : 0, bh = x1 > x0 ? y1 - y0 : 0;
    const head = new Uint32Array([bw ? x0 : 0, bw ? y0 : 0, bw, bh]);
    const bits = new Uint8Array((bw * bh + 7) >> 3);
    for (let yy = 0; yy < bh; yy++) for (let xx = 0; xx < bw; xx++) {
      if (d[((yy + y0) * W + xx + x0) * 4 + 3]) { const i = yy * bw + xx; bits[i >> 3] |= 1 << (i & 7); }
    }
    let s = '';
    const all = new Uint8Array(16 + bits.length);
    all.set(new Uint8Array(head.buffer), 0);
    all.set(bits, 16);
    for (let i = 0; i < all.length; i += 8192) s += String.fromCharCode.apply(null, all.subarray(i, i + 8192));
    out.push(btoa(s));
  }
  return out;
}

const server = await startServer({ impl: 'upstream' });
const browser = await launch(engine, {});
const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
const page = await ctx.newPage();
await page.goto(server.url + '/parity.html', { waitUntil: 'load' });
await page.waitForFunction(READY_DEFAULT);
await page.evaluate(() => document.fonts.ready.then(() => true));
const fd = fs.openSync(out, 'w');
for (let i = 0; i < lines.length; i += 500) {
  const res = await page.evaluate(replay, lines.slice(i, i + 500));
  for (const b of res) fs.writeSync(fd, b ? Buffer.from(b, 'base64') : Buffer.alloc(16));
  if (i % 10000 === 0) console.error(`${i}/${lines.length}`);
}
fs.closeSync(fd);
await browser.close();
await server.close();
