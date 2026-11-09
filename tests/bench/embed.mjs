#!/usr/bin/env node
// The two ways to make `syntax.parse` available, side by side:
//   embedded  the module inside hierojax.js as base64, compiled when the script runs
//   separate  hieroxide_wasm_bg.wasm next to the script, loaded and compiled asynchronously
//
//   node embed.mjs [--engine chromium|firefox|webkit] [--reps 7] [--embedded dir] [--separate dir]
//
// Sizes (brotli quality 11, and gzip 9) of what a visitor downloads. Then per engine, median of
// fresh loads: the cost of the script itself (fetched first, then inserted as an inline classic
// script between two marks, the way run.mjs measures the upstream script); for the embedded
// build that includes decoding the base64, compiling the module synchronously and
// instantiating it, which the script also reports apart (wasmReady - wasmStart); for the
// separate build the time from the script's start until the module is ready (the compile is
// off the main thread; the instantiation is on it). Chromium also with a 4x CPU throttle.
// Whether `syntax.parse` works at once after the script has run is checked too.
//
// Build both first:
//   node tools/build-compat.mjs --embed --out build/embedded
//   node tools/build-compat.mjs --separate --out build/separate
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { launch } from '../parity/lib/engines.mjs';
import { startServer } from '../parity/lib/server.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const { values: v } = parseArgs({
  options: {
    engine: { type: 'string' },
    reps: { type: 'string', default: '7' },
    embedded: { type: 'string', default: path.join(ROOT, 'build/embedded') },
    separate: { type: 'string', default: path.join(ROOT, 'build/separate') },
  },
});
const med = (a) => [...a].sort((x, y) => x - y)[a.length >> 1];
const r2 = (x) => Math.round(x * 100) / 100;
const brotli = (b) => zlib.brotliCompressSync(b, { params: { [zlib.constants.BROTLI_PARAM_QUALITY]: 11, [zlib.constants.BROTLI_PARAM_SIZE_HINT]: b.length } }).length;
const gzip = (b) => zlib.gzipSync(b, { level: 9 }).length;

const embJs = fs.readFileSync(path.join(v.embedded, 'hierojax.js'));
const sepJs = fs.readFileSync(path.join(v.separate, 'hierojax.js'));
const wasm = fs.readFileSync(path.join(v.separate, 'hieroxide_wasm_bg.wasm'));
const up = fs.readFileSync(path.join(ROOT, 'docs/hierojax.js'));
const sizes = {
  upstream: { raw: up.length, brotli: brotli(up), gzip: gzip(up) },
  embedded: { raw: embJs.length, brotli: brotli(embJs), gzip: gzip(embJs) },
  separate: { raw: sepJs.length + wasm.length, brotli: brotli(sepJs) + brotli(wasm), gzip: gzip(sepJs) + gzip(wasm), script: brotli(sepJs), wasm: brotli(wasm) },
};
sizes.embeddedExtraBrotli = sizes.embedded.brotli - sizes.separate.brotli;
sizes.embeddedExtraGzip = sizes.embedded.gzip - sizes.separate.gzip;
console.log('sizes (bytes):', JSON.stringify(sizes, null, 1));
if (!v.engine) process.exit(0);

async function measure(browser, impl, throttle) {
  const server = await startServer({ impl });
  const rows = [];
  for (let i = 0; i < Number(v.reps); i++) {
    const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
    const page = await ctx.newPage();
    if (throttle && v.engine === 'chromium') {
      const cdp = await ctx.newCDPSession(page);
      await cdp.send('Emulation.setCPUThrottlingRate', { rate: throttle });
    }
    await page.goto(server.url + '/empty.html');
    const r = await page.evaluate(async () => {
      const txt = await (await fetch('/hierojax.js')).text();
      performance.mark('a');
      const s = document.createElement('script');
      s.textContent = txt;
      document.head.appendChild(s);
      performance.mark('b');
      performance.measure('eval', 'a', 'b');
      const evalMs = performance.getEntriesByName('eval')[0].duration;
      let parseNow;
      try {
        parseNow = typeof syntax.parse('\u{13000}').toString();
      } catch (e) {
        parseNow = 'throws: ' + String(e.message).slice(0, 60);
      }
      const t0 = performance.now();
      await hierojax.ready;
      const moduleMs = hierojax.wasmReady - hierojax.wasmStart;
      return { evalMs, moduleMs, parseNow, readyAfterMs: performance.now() - t0 };
    });
    rows.push(r);
    await ctx.close();
  }
  await server.close();
  return {
    scriptEvalMs: r2(med(rows.map((r) => r.evalMs))),
    moduleReadyMs: r2(med(rows.map((r) => r.moduleMs))),
    syntaxParseAtOnce: rows[0].parseNow,
    all: rows.map((r) => r2(r.evalMs)),
  };
}

const browser = await launch(v.engine);
const out = { engine: v.engine, version: browser.version(), load: JSON.stringify(process.loadavg ? (await import('node:os')).loadavg().map((x) => r2(x)) : []) };
for (const throttle of v.engine === 'chromium' ? [0, 4] : [0]) {
  out[throttle ? 'throttle4x' : 'unthrottled'] = {
    embedded: await measure(browser, v.embedded, throttle),
    separate: await measure(browser, v.separate, throttle),
  };
}
await browser.close();
console.log(JSON.stringify(out, null, 1));
