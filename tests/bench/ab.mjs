#!/usr/bin/env node
// Interleaved comparison of several builds on one page in one engine, for a machine whose load
// changes while it runs (a second build, say): round r loads every build once, in turn, so all of
// them see the same stretches of the machine's time; the median of the rounds is what is reported,
// with the range (min and max) as the spread.
//
//   node ab.mjs --engine chromium --page real|covered|pbm|testsuite [--reps 7]
//               label=upstream  label=../../build/compat  label=../../build/compat-test@batch,fontwait,canvas ...
//
// A spec is `label=impl` or `label=impl@levers` (levers need the test build, js/compat/README.md);
// `impl` is `upstream`, a build directory, or `control` for build/control (make-control.mjs).
// Pages: `real` is docs/texts/PBM10474.html with the fragments the port cannot lay out taken out;
// `covered` the 5,333 fragments of the real pages in one document; `pbm` and `testsuite` as they
// are (testsuite needs a build with the module embedded). Measured, in ms, as `run.mjs` does:
// the call (`processFragmentsNow`), the layout still owed after it (a forced layout), and their
// sum, which is the figure to compare. `--cpu N` throttles Chromium by N.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { launch } from '../parity/lib/engines.mjs';
import { startServer } from '../parity/lib/server.mjs';
import { coveredPage } from '../parity/lib/covered.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const INJECT = fs.readFileSync(path.join(HERE, 'inject.js'), 'utf8');
const { values: v, positionals } = parseArgs({
  allowPositionals: true,
  options: {
    engine: { type: 'string' },
    page: { type: 'string', default: 'covered' },
    reps: { type: 'string', default: '7' },
    cpu: { type: 'string', default: '0' },
    'coverage-impl': { type: 'string', default: path.join(HERE, '../../build/compat') },
    json: { type: 'string' },
  },
});
if (!v.engine || !positionals.length) {
  console.error('usage: node ab.mjs --engine E --page real|covered|pbm|testsuite [--reps N] label=impl[@levers] ...');
  process.exit(2);
}
const specs = positionals.map((s) => {
  const eq = s.indexOf('=');
  const label = s.slice(0, eq);
  let [impl, levers] = s.slice(eq + 1).split('@');
  if (impl === 'control') impl = path.join(HERE, '../../build/control');
  return { label, impl: impl === 'upstream' ? 'upstream' : path.resolve(impl), levers };
});

const med = (a) => { const s = a.slice().sort((x, y) => x - y); const n = s.length; return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2; };

const needCovered = v.page === 'covered' || v.page === 'real';
const covered = needCovered ? await coveredPage({ coverageImpl: v['coverage-impl'], engine: v.engine }) : null;
const file = { real: 'texts/PBM10474.html', covered: 'covered.html', pbm: 'texts/PBM10474.html', testsuite: 'testsuite.html' }[v.page];
if (!file) throw new Error('unknown page ' + v.page);

const KEEP_COVERED = v.page === 'real' ? `
  (() => {
    const html = ${JSON.stringify(covered?.html ?? '')};
    const norm = (t) => t.replace(/\\s+/g, ' ').trim();
    const key = (e) => norm(e.textContent) + '\\u0000' + [...e.attributes].filter((a) => a.name.startsWith('data-')).map((a) => a.name + '=' + a.value).sort().join('&');
    document.addEventListener('DOMContentLoaded', () => {
      const keep = new Set([...new DOMParser().parseFromString(html, 'text/html').querySelectorAll('span.hierojax')].map(key));
      for (const e of [...document.querySelectorAll('span.hierojax')]) if (!keep.has(key(e))) e.classList.remove('hierojax');
    });
  })();
` : '';

const servers = [];
for (const s of specs) servers.push(await startServer({ impl: s.impl, transformScript: (t) => t + '\n' + INJECT, pages: covered ? { '/covered.html': covered.html } : undefined }));
const prefs = v.engine === 'firefox' ? { 'privacy.reduceTimerPrecision': false } : undefined;
const browser = await launch(v.engine, { prefs });

async function once(i) {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
  const page = await ctx.newPage();
  if (Number(v.cpu) && v.engine === 'chromium') await (await ctx.newCDPSession(page)).send('Emulation.setCPUThrottlingRate', { rate: Number(v.cpu) });
  await page.addInitScript('window.__HEAVY = false; window.__F = {};');
  if (specs[i].levers != null) await page.addInitScript(`window.HIEROXIDE_LEVERS = ${JSON.stringify(specs[i].levers)};`);
  if (KEEP_COVERED) await page.addInitScript(KEEP_COVERED);
  await page.goto(`${servers[i].url}/${file}`, { waitUntil: 'load' });
  await page.waitForFunction(() => window.__done === true, null, { timeout: 600000 });
  const t = await page.evaluate(() => window.__S.t);
  await ctx.close();
  return { call: t.endNow - t.startNow, layout: t.endLayout - t.endNow, total: t.endLayout - t.startNow };
}

const rows = specs.map(() => []);
const order = specs.map((_, i) => i);
for (let r = 0; r < Number(v.reps); r++) {
  // a different first build each round, so no build is always the one that follows the same neighbour
  const rot = order.slice(r % order.length).concat(order.slice(0, r % order.length));
  for (const i of rot) rows[i].push(await once(i));
}
await browser.close();
for (const s of servers) await s.close();

const f = (x) => (x < 100 ? x.toFixed(1) : String(Math.round(x)));
const base = med(rows[0].map((x) => x.total));
const out = [['build', 'call', 'layout', 'call+layout', 'min', 'max', 'vs first']];
const json = {};
specs.forEach((s, i) => {
  const t = rows[i].map((x) => x.total);
  out.push([s.label, f(med(rows[i].map((x) => x.call))), f(med(rows[i].map((x) => x.layout))), f(med(t)), f(Math.min(...t)), f(Math.max(...t)), (med(t) / base).toFixed(2) + 'x']);
  json[s.label] = rows[i];
});
const w = out[0].map((_, c) => Math.max(...out.map((r) => r[c].length)));
console.log(`${v.engine} ${v.page}${v.cpu !== '0' ? ' cpu x' + v.cpu : ''}, median of ${v.reps} interleaved rounds, ms${covered ? `, ${covered.count} fragments covered` : ''}`);
for (const r of out) console.log(r.map((x, c) => (c ? x.padStart(w[c]) : x.padEnd(w[c]))).join('  '));
if (v.json) fs.writeFileSync(v.json, JSON.stringify({ engine: v.engine, page: v.page, rows: json }, null, 1));
