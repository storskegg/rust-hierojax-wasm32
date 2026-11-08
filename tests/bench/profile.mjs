#!/usr/bin/env node
// Chromium only: where the time of processFragmentsNow goes, by a V8 CPU profile (self time per
// function, native DOM calls included) and by trace events for the browser's own style and
// layout work. One load of the covered page, from the call to the forced layout after it.
//
//   node profile.mjs --impl ../../build/variants/all [--top 25]
//
// Native calls show up as functions of their own in the CPU profile (`insertAdjacentHTML`,
// `getComputedStyle`, `innerText`, `offsetHeight`...); their self time includes the HTML
// parser, the tree builder and any style or layout the call forced.
import { parseArgs } from 'node:util';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { launch } from '../parity/lib/engines.mjs';
import { startServer } from '../parity/lib/server.mjs';
import { coveredPage } from '../parity/lib/covered.mjs';
import fs from 'node:fs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const INJECT = fs.readFileSync(path.join(HERE, 'inject.js'), 'utf8');
const { values: v } = parseArgs({
  options: {
    impl: { type: 'string', default: 'upstream' },
    top: { type: 'string', default: '25' },
    'coverage-impl': { type: 'string', default: path.join(HERE, '../../build/compat') },
    throttle: { type: 'string', default: '0' },
  },
});
const covered = await coveredPage({ coverageImpl: v['coverage-impl'], engine: 'chromium' });
const server = await startServer({ impl: v.impl, transformScript: (t) => t + '\n' + INJECT, pages: { '/covered.html': covered.html } });
const browser = await launch('chromium', {});
const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
const page = await ctx.newPage();
const cdp = await ctx.newCDPSession(page);
if (Number(v.throttle)) await cdp.send('Emulation.setCPUThrottlingRate', { rate: Number(v.throttle) });
await page.addInitScript(`window.__HEAVY = false; window.__F = {}; const FL = FontFace.prototype.load;
  FontFace.prototype.load = function (...a) { window.__F.start = performance.now(); const p = FL.apply(this, a); p.then(() => { window.__F.end = performance.now(); }); return p; };`);
await cdp.send('Profiler.enable');
await cdp.send('Profiler.setSamplingInterval', { interval: 100 });
// Start profiling and tracing as the call starts (the page tells us through a console message).
await page.addInitScript(`window.addEventListener('hx-start', () => console.debug('HX-START'));`);
await page.goto(server.url + '/empty.html');
const traceEvents = [];
await cdp.send('Tracing.start', { categories: 'devtools.timeline,disabled-by-default-devtools.timeline', transferMode: 'ReturnAsStream' });
await cdp.send('Profiler.start');
await page.goto(server.url + '/covered.html', { waitUntil: 'load' });
await page.waitForFunction(() => window.__done === true, null, { timeout: 300000 });
const { profile } = await cdp.send('Profiler.stop');
const done = new Promise((res) => cdp.once('Tracing.tracingComplete', res));
await cdp.send('Tracing.end');
const { stream } = await done;
let text = '';
for (;;) {
  const r = await cdp.send('IO.read', { handle: stream });
  text += r.data;
  if (r.eof) break;
}
const S = await page.evaluate(() => window.__S.t);
await browser.close();
await server.close();

// CPU profile: self time per function node.
const nodes = new Map(profile.nodes.map((n) => [n.id, n]));
const self = new Map();
const dts = profile.timeDeltas;
for (let i = 0; i < profile.samples.length; i++) {
  const n = nodes.get(profile.samples[i]);
  const key = `${n.callFrame.functionName || '(anonymous)'} ${n.callFrame.url.split('/').pop()}:${n.callFrame.lineNumber}`;
  self.set(key, (self.get(key) || 0) + (dts[i] || 0));
}
const total = [...self.values()].reduce((a, b) => a + b, 0);
console.log(`call took ${(S.endNow - S.startNow).toFixed(0)} ms, forced layout after it ${(S.endLayout - S.endNow).toFixed(0)} ms (profile covers the whole page load: ${(total / 1000).toFixed(0)} ms of samples)`);
console.log('CPU profile, self time, top:');
for (const [k, us] of [...self].sort((a, b) => b[1] - a[1]).slice(0, Number(v.top))) console.log(`${(us / 1000).toFixed(1).padStart(8)} ms  ${k}`);

// Trace events: browser work by name (inclusive durations of top-level events of each name).
const events = JSON.parse(text.replace(/,\s*$/, '')).traceEvents ?? JSON.parse(text);
const byName = new Map();
for (const e of events) {
  if (e.ph !== 'X' || !e.dur) continue;
  if (!['Layout', 'UpdateLayoutTree', 'ParseHTML', 'HitTest', 'Paint', 'PrePaint', 'Layerize', 'Commit', 'FunctionCall', 'EvaluateScript', 'RunTask', 'v8.callFunction', 'MinorGC', 'MajorGC', 'V8.GC_MC_BACKGROUND_MARKING'].includes(e.name) && !/GC|Wasm|wasm/.test(e.name)) continue;
  const a = byName.get(e.name) || [0, 0];
  a[0] += e.dur; a[1]++;
  byName.set(e.name, a);
}
console.log('trace events (sum of durations; nested events overlap, e.g. Layout inside FunctionCall):');
for (const [k, [us, n]] of [...byName].sort((a, b) => b[1][0] - a[1][0]).slice(0, 14)) console.log(`${(us / 1000).toFixed(1).padStart(8)} ms  ${n} x ${k}`);
