#!/usr/bin/env node
// Load strategy: how a page should load each mode, measured. A page of a few fragments is loaded in each engine
// through a proxy that makes the network what a real one is (brotli, a round trip per request, a bandwidth), under
// several scenarios, and the times to the first fragment drawn and the first paint after it are taken from
// navigation start. Scenarios:
//   compat/separate        build/compat: the script and, from it, the .wasm (compiled while it streams in)
//   compat/separate+hints  the same with `<link rel=preload>` for the .wasm and the font in the head
//   compat/embedded        build/embedded: one script with the module inside it as base64
//   consistent             build/consistent: the module script, which fetches the font and the .wasm together
//   consistent/hints       the same with `modulepreload` for its two scripts and `preload` for the .wasm and the font
//
//   node load.mjs [--engines chromium,firefox,webkit] [--profiles local,fast,slow] [--reps 5] [--out file.json]
//
// Profiles: local (no delay), fast (50 ms round trip, 12 Mbit/s: a good phone on 4G), slow (200 ms, 1.6 Mbit/s: a bad
// one). The font is served as it is (2.6 MB, 1.5 MB with brotli), which is most of the time on a slow network: the
// first fragment cannot be drawn before it either way, and a `woff2` of it would be a third of that.
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import zlib from 'node:zlib';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { launch, platformTag } from '../parity/lib/engines.mjs';
import { startServer } from '../parity/lib/server.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../..');
const { values: v } = parseArgs({
  options: {
    engines: { type: 'string', default: 'chromium,firefox,webkit' },
    profiles: { type: 'string', default: 'local,fast,slow' },
    reps: { type: 'string', default: '5' },
    out: { type: 'string' },
  },
});
const PROFILES = { local: { rtt: 0, bps: Infinity }, fast: { rtt: 50, bps: 12e6 / 8 }, slow: { rtt: 200, bps: 1.6e6 / 8 } };
const med = (a) => { const s = a.slice().sort((x, y) => x - y); const n = s.length; return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2; };

const TEXT = ['\u{13000}\u{13431}\u{13001}', '\u{131F3}\u{13430}\u{133E4}', '\u{13000}\u{13431}\u{13050}\u{13431}\u{13001}', '\u{13216}\u{13430}\u{13216}'];
const frags = Array.from({ length: 20 }, (_, i) => TEXT[i % TEXT.length]);
const MARK = `<script>window.__t0 = performance.now();
new MutationObserver((m, o) => { if (document.querySelector('.hierojax > *')) { window.__drawn = performance.now(); o.disconnect(); requestAnimationFrame(() => requestAnimationFrame(() => { window.__paint = performance.now(); })); } }).observe(document, { childList: true, subtree: true });
</script>`;
const HINT_FONT = '<link rel="preload" as="font" type="font/otf" crossorigin href="NewGardiner.otf">';
const HINT_COMPAT = '<link rel="preload" as="fetch" crossorigin href="hieroxide_wasm_bg.wasm">' + HINT_FONT;
const HINT_CONS = '<link rel="modulepreload" href="/index.js"><link rel="modulepreload" href="/hieroxide_outline.js"><link rel="preload" as="fetch" crossorigin href="/hieroxide_outline_bg.wasm"><link rel="preload" as="fetch" crossorigin href="/NewGardiner.otf">';
const legacy = (hints, ready) => `<!DOCTYPE html><html><head><meta charset="utf-8">${hints}${MARK}<link rel="stylesheet" href="hierojax.css"><script src="hierojax.js"></script>
<script>addEventListener('DOMContentLoaded', () => { hierojax.processFragments(); hierojax.ready.then(() => { window.__ready = performance.now(); }); });</script></head><body>
${frags.map((t) => `<div><span class="hierojax">${t}</span></div>`).join('\n')}</body></html>`;
const consistent = (hints) => `<!DOCTYPE html><html><head><meta charset="utf-8">${hints}${MARK}</head><body>
${frags.map((t) => `<div><span>${t}</span></div>`).join('\n')}
<script type="module">
import { init, render } from '/index.js';
await init({ font: '/NewGardiner.otf' });
window.__ready = performance.now();
for (const el of document.querySelectorAll('span')) render(el, el.textContent, {});
</script></body></html>`;

const SCENARIOS = [
  ['compat/separate', 'build/compat', legacy('')],
  ['compat/separate+hints', 'build/compat', legacy(HINT_COMPAT)],
  ['compat/embedded', 'build/embedded', legacy(HINT_FONT)],
  ['consistent', 'build/consistent', consistent('')],
  ['consistent/hints', 'build/consistent', consistent(HINT_CONS)],
];

// A proxy in front of a server: brotli for what compresses, a round trip before the first byte, the bytes at a rate.
function proxy(target, { rtt, bps }) {
  const srv = http.createServer(async (req, res) => {
    const r = await fetch(target + req.url);
    let body = Buffer.from(await r.arrayBuffer());
    const type = r.headers.get('content-type') ?? '';
    const headers = { 'Content-Type': type, 'Cache-Control': 'no-store' };
    if (/javascript|wasm|font|html|css/.test(type)) { body = zlib.brotliCompressSync(body, { params: { [zlib.constants.BROTLI_PARAM_QUALITY]: 5 } }); headers['Content-Encoding'] = 'br'; }
    headers['Content-Length'] = body.length;
    await new Promise((ok) => setTimeout(ok, rtt));
    res.writeHead(r.status, headers);
    const chunk = 16384;
    for (let i = 0; i < body.length; i += chunk) {
      res.write(body.subarray(i, i + chunk));
      if (bps !== Infinity) await new Promise((ok) => setTimeout(ok, (Math.min(chunk, body.length - i) / bps) * 1000));
    }
    res.end();
  });
  return new Promise((ok) => srv.listen(0, '127.0.0.1', () => ok({ url: `http://127.0.0.1:${srv.address().port}`, close: () => srv.close() })));
}

const result = { meta: { platform: platformTag(), reps: Number(v.reps), date: new Date().toISOString(), profiles: PROFILES }, rows: {} };
for (const engine of v.engines.split(',')) {
  const browser = await launch(engine);
  for (const pname of v.profiles.split(',')) {
    for (const [name, dir, html] of SCENARIOS) {
      const origin = await startServer({ impl: path.join(ROOT, dir), pages: { '/p.html': html } });
      const px = await proxy(origin.url, PROFILES[pname]);
      const rows = [];
      for (let i = 0; i < Number(v.reps); i++) {
        const ctx = await browser.newContext({ viewport: { width: 1000, height: 600 } });
        const page = await ctx.newPage();
        await page.goto(px.url + '/p.html', { waitUntil: 'commit' });
        await page.waitForFunction(() => window.__paint > 0, null, { timeout: 120000, polling: 50 });
        rows.push(await page.evaluate(() => ({ script: window.__t0, ready: window.__ready, drawn: window.__drawn, paint: window.__paint })));
        await ctx.close();
      }
      px.close();
      await origin.close();
      const m = (k) => Math.round(med(rows.map((r) => r[k])));
      result.rows[`${engine}/${pname}/${name}`] = { scriptStartMs: m('script'), readyMs: m('ready'), firstDrawnMs: m('drawn'), firstPaintMs: m('paint'), allPaintMs: rows.map((r) => Math.round(r.paint)) };
      console.error(`${engine} ${pname} ${name}: script ${m('script')}, ready ${m('ready')}, drawn ${m('drawn')}, paint ${m('paint')} ms`);
    }
  }
  await browser.close();
}
const out = v.out ?? path.join(HERE, 'out', `load-${platformTag()}.json`);
fs.mkdirSync(path.dirname(out), { recursive: true });
fs.writeFileSync(out, JSON.stringify(result, null, 1) + '\n');
console.log(`wrote ${out}`);
