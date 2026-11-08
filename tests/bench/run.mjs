#!/usr/bin/env node
// Benchmarks: where the time goes on the same two pages, per engine, median of 5 fresh loads.
//
//   node run.mjs --engine chromium|firefox|webkit [--impl upstream|dir|file.js] [--reps 5]
//                [--record] [--out file.json] [--no-throttle]
//
// Pages: texts/PBM10474.html (553 fragments, the largest by count) and testsuite.html
// (137 texts x 10 option variants = 1,370 renders). Measured:
//  - script parse+eval: the script text is fetched, then inserted as an inline classic script
//    between two performance marks (the fetch is outside the timed part);
//  - font load (FontFace.load()), wait-to-start (font ready until processFragmentsNow begins:
//    the waitForFonts polling delay), processFragmentsNow total;
//  - its split into parse, format+fit, print (without format) and other (DOM reads, clears,
//    forced layout), from wrappers on the entry points ("light" runs);
//  - canvas operations and counts, from a second set of runs with the canvas calls wrapped
//    ("heavy" runs; their totals are slightly inflated by the timers);
//  - per-fragment p50/p95 by class, pooled over the light loads.
// Chromium is also run with a 4x CPU throttle (CDP) for script eval, font, wait and totals.
// Firefox runs with privacy.reduceTimerPrecision=false so its timers are not rounded.
// WebKit's timer has 1 ms resolution: trust its totals, not its per-fragment numbers.
// Other implementations are served in place of hierojax.js (see tests/parity/lib/server.mjs);
// stage splits need upstream's class names and are null where they are missing.
// Timing is deliberately not part of CI.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { launch, platformTag } from '../parity/lib/engines.mjs';
import { startServer } from '../parity/lib/server.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const INJECT = fs.readFileSync(path.join(HERE, 'inject.js'), 'utf8');
const PAGES = [
  ['PBM10474', 'texts/PBM10474.html'],
  ['testsuite', 'testsuite.html'],
];

const { values: v } = parseArgs({
  options: {
    engine: { type: 'string' },
    impl: { type: 'string', default: 'upstream' },
    reps: { type: 'string', default: '5' },
    record: { type: 'boolean' },
    out: { type: 'string' },
    'no-throttle': { type: 'boolean' },
  },
});
if (!v.engine) {
  console.error('usage: node run.mjs --engine chromium|firefox|webkit [--impl upstream|path] [--reps 5] [--record]');
  process.exit(2);
}
const REPS = Number(v.reps);

const med = (a) => {
  const s = a.slice().sort((x, y) => x - y);
  const n = s.length;
  return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2;
};
const pct = (a, p) => {
  if (!a.length) return null;
  const s = a.slice().sort((x, y) => x - y);
  return s[Math.min(s.length - 1, Math.floor(p * s.length))];
};
const sum = (a) => a.reduce((x, y) => x + y, 0);
const r1 = (x) => (x == null || Number.isNaN(x) ? null : Math.round(x * 100) / 100);

const INIT = (heavy) => `
  window.__HEAVY = ${heavy};
  window.__F = {};
  const FL = FontFace.prototype.load;
  FontFace.prototype.load = function (...a) {
    window.__F.start = performance.now();
    const p = FL.apply(this, a);
    p.then(() => { window.__F.end = performance.now(); });
    return p;
  };
`;

const prefs = v.engine === 'firefox' ? { 'privacy.reduceTimerPrecision': false } : undefined;
const server = await startServer({ impl: v.impl, transformScript: (t) => t + '\n' + INJECT });
const browser = await launch(v.engine, { prefs });
const version = browser.version();

async function newPage(throttle, init) {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
  const page = await ctx.newPage();
  if (throttle && v.engine === 'chromium') {
    const cdp = await ctx.newCDPSession(page);
    await cdp.send('Emulation.setCPUThrottlingRate', { rate: throttle });
  }
  if (init) await page.addInitScript(init);
  return { ctx, page };
}

async function scriptEval(throttle) {
  const rows = [];
  for (let i = 0; i < REPS; i++) {
    const { ctx, page } = await newPage(throttle);
    await page.goto(server.url + '/empty.html');
    const r = await page.evaluate(async () => {
      const txt = await (await fetch('/hierojax.js')).text();
      performance.mark('eval-start');
      const s = document.createElement('script');
      s.textContent = txt; // classic script: parse and top-level evaluation happen synchronously here
      document.head.appendChild(s);
      performance.mark('eval-end');
      performance.measure('eval', 'eval-start', 'eval-end');
      return { evalMs: performance.getEntriesByName('eval')[0].duration, bytes: txt.length };
    });
    rows.push(r);
    await ctx.close();
  }
  return { medianMs: r1(med(rows.map((r) => r.evalMs))), allMs: rows.map((r) => r1(r.evalMs)), scriptBytes: rows[0].bytes };
}

async function loadPage(file, heavy, throttle) {
  const { ctx, page } = await newPage(throttle, INIT(heavy));
  await page.goto(`${server.url}/${file}`, { waitUntil: 'load' });
  await page.waitForFunction(() => window.__done === true, null, { timeout: 300000 });
  const S = await page.evaluate(() => {
    const s = window.__S;
    s.F = window.__F;
    return s;
  });
  await ctx.close();
  return S;
}

function summarize(runs, { withStages }) {
  const tot = (r) => r.t.endNow - r.t.startNow;
  const col = (r, i) => sum(r.frag.map((f) => f[i]));
  const o = {
    fontLoadMs: r1(med(runs.map((r) => r.F.end - r.F.start))),
    waitToStartMs: r1(med(runs.map((r) => r.t.startNow - r.F.end))),
    totalMs: r1(med(runs.map(tot))),
    allTotalsMs: runs.map((r) => r1(tot(r))),
  };
  if (withStages && runs[0].stages) {
    o.parseMs = r1(med(runs.map((r) => col(r, 1))));
    o.formatFitMs = r1(med(runs.map((r) => col(r, 2))));
    o.printExclFormatMs = r1(med(runs.map((r) => col(r, 3) - col(r, 2))));
    o.otherMs = r1(med(runs.map((r) => tot(r) - col(r, 1) - col(r, 3))));
  }
  return o;
}

function heavySummary(runs) {
  const col = (r, i) => sum(r.frag.map((f) => f[i]));
  const cc = (r) => r.tCreate + r.tGetCtx + r.tSize;
  const ops = (r) => cc(r) + r.tGID + r.tFill;
  const h = runs[0];
  return {
    totalMs: r1(med(runs.map((r) => r.t.endNow - r.t.startNow))),
    canvasOpsMs: r1(med(runs.map(ops))),
    canvasCreateGetContextResizeMs: r1(med(runs.map(cc))),
    getImageDataMs: r1(med(runs.map((r) => r.tGID))),
    fillTextMs: r1(med(runs.map((r) => r.tFill))),
    measureTextMs: r1(med(runs.map((r) => r.tMeasure))),
    canvasOpsShareOfFormatPrint: runs[0].stages ? r1(med(runs.map((r) => ops(r) / col(r, 3)))) : null,
    counts: {
      canvases: h.canvases, getContext: h.getContext, getImageData: h.getImageData,
      fillText: h.fillText, measureText: h.measureText, pixelsReadMillions: r1(h.pixelsRead / 1e6),
    },
  };
}

function perFragment(runs) {
  const all = runs.flatMap((r) => r.frag);
  const classes = {
    plain: (f) => !f[4] && !f[5],
    insertion: (f) => f[4],
    'enclosureOrOverlay': (f) => !f[4] && f[5],
  };
  const out = {};
  for (const [k, fn] of Object.entries(classes)) {
    const xs = all.filter(fn).map((f) => f[0]);
    out[k] = { perLoad: r1(xs.length / runs.length), p50Ms: r1(pct(xs, 0.5)), p95Ms: r1(pct(xs, 0.95)), maxMs: r1(Math.max(...xs, 0)) };
  }
  return out;
}

const result = {
  meta: {
    engine: v.engine,
    engineVersion: version,
    platform: platformTag(),
    machine: `${os.cpus()[0]?.model ?? '?'}, ${os.cpus().length} cores`,
    os: `${os.type()} ${os.release()}`,
    node: process.version,
    impl: server.impl.name,
    repsPerMeasure: REPS,
    date: new Date().toISOString(),
    notes: [
      'median of fresh page loads, each in its own browser context',
      v.engine === 'firefox' ? 'privacy.reduceTimerPrecision=false' : null,
      v.engine === 'webkit' ? 'WebKit performance.now() has 1 ms resolution; compare totals, not per-fragment times' : null,
      'heavy runs wrap canvas calls, so their totals are slightly inflated; stage splits come from light runs',
    ].filter(Boolean),
  },
  scriptParseEval: await scriptEval(0),
  pages: {},
};

for (const [name, file] of PAGES) {
  console.error(`${v.engine}: ${name}`);
  const light = [], heavy = [];
  for (let i = 0; i < REPS; i++) light.push(await loadPage(file, false, 0));
  for (let i = 0; i < REPS; i++) heavy.push(await loadPage(file, true, 0));
  result.pages[name] = {
    file,
    fragments: light[0].fragments,
    light: summarize(light, { withStages: true }),
    heavy: heavySummary(heavy),
    perFragment: light[0].stages ? perFragment(light) : null,
  };
}

if (v.engine === 'chromium' && !v['no-throttle']) {
  console.error('chromium: 4x CPU throttle');
  const t = { rate: 4, scriptParseEval: await scriptEval(4), pages: {} };
  for (const [name, file] of PAGES) {
    const runs = [];
    for (let i = 0; i < REPS; i++) runs.push(await loadPage(file, false, 4));
    const s = summarize(runs, { withStages: false });
    t.pages[name] = { fontLoadMs: s.fontLoadMs, waitToStartMs: s.waitToStartMs, totalMs: s.totalMs, allTotalsMs: s.allTotalsMs };
  }
  result.cpuThrottle4x = t;
}

await browser.close();
await server.close();

const out = v.out ?? path.join(HERE, v.record ? 'baselines' : 'out', `${v.engine}-${platformTag()}.json`);
fs.mkdirSync(path.dirname(out), { recursive: true });
fs.writeFileSync(out, JSON.stringify(result, null, 2) + '\n');
console.log(`wrote ${out}`);
console.log(JSON.stringify({ scriptParseEvalMs: result.scriptParseEval.medianMs, pages: Object.fromEntries(Object.entries(result.pages).map(([k, p]) => [k, p.light])), throttle: result.cpuThrottle4x }, null, 1));
