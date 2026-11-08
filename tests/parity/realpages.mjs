#!/usr/bin/env node
// Real-page parity: the pages of docs/ that render fragments, loaded as they stand, once with
// upstream's script and once with the replacement swapped in through the server (the pages
// ask for `hierojax.js`, whatever its directory). Nobody calls anything: each page does what
// it does at DOMContentLoaded (`hierojax.processFragments()`, or the test suite's expansion
// of its demo lines first), in the order and state of a real load. When the page has settled
// the whole document is compared: canonical DOM (lib/canon.mjs), the pixels of every canvas,
// the bounding boxes of the fragments, the visible text, what was logged (`console.*`), the
// `alert`s and the exceptions the page reported.
//
//   node realpages.mjs --impl ../../build/compat [--engines chromium,firefox,webkit]
//                      [--pages texts/PBM10474.html,demo.html] [--corpus-page] [--no-pages]
//                      [--embedded ../../build/embedded] [--site /path/to/patched/hierojax.js]
//
// A page that calls `syntax.parse` while it loads (the test suite does, at DOMContentLoaded) needs
// the build with the module inside the script, where that works at once; with the default build
// it throws, by design (js/compat/README.md). Those pages use --embedded when it is given.
//
// Out of scope, and listed with the reason: pages whose own scripts use the internals
// (`Shapes.*`, `Group.*`, the `Printed*` classes other than `PrintedSVG.EXTERNAL_FONT`),
// which the compat build does not offer.
//
// --corpus-page: instead of the pages, one page with the whole corpus in it (every fragment
// of corpus.json that upstream lays out without throwing, in corpus order, each in a div with
// its font size and colour), and one `hierojax.processFragments()` for all of them, which
// is the call that batches. Compared per fragment: canonical DOM, canvas pixels, bounding
// box; and the log. The canvas and DOM types are in it.
//
// --site FILE: the reference is a patched upstream script (the site's `src/hierojax.js`,
// read at run time) and the replacement gets the site options (`fontUrl`, `fontFormat`,
// `svgScalable`, `autoProcess`) as window.HIEROJAX_CONFIG.
//
// The text of a logged parse error after "Error: " is not compared (the ruling of the plan).
// Exit 1 on a difference.
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { launch } from './lib/engines.mjs';
import { startServer, DOCS } from './lib/server.mjs';
import { CANON_SRC } from './lib/canon.mjs';
import { INIT_SCRIPT } from './lib/driver.mjs';
import { loadCorpus, readJson, runCorpus, writeJson, OUT_DIR } from './lib/runner.mjs';

const { values: v } = parseArgs({
  options: {
    impl: { type: 'string' },
    engines: { type: 'string', default: 'chromium,firefox,webkit' },
    pages: { type: 'string' },
    'corpus-page': { type: 'boolean' },
    'no-pages': { type: 'boolean' },
    site: { type: 'string' },
    embedded: { type: 'string' },
    quiet: { type: 'string', default: '1500' },
  },
});
if (!v.impl) {
  console.error('usage: node realpages.mjs --impl <dir> [--engines ...] [--pages a.html,b.html] [--corpus-page] [--site file.js]');
  process.exit(2);
}

export const SITE_OPTIONS = { fontUrl: '/NewGardiner.woff2', fontFormat: 'woff2', svgScalable: true, autoProcess: true };
const INTERNALS = /\b(Shapes|Group|PrintedCanvas|PrintedDOM|PrintedSVG\.(?!EXTERNAL_FONT)|PrintedAny|PrintedCanvasWithoutExtras)\b|\bPrintedSVG\b(?!\.EXTERNAL_FONT)/;

/** The pages of docs/ that load hierojax.js, split into those in scope and those not. */
export function discoverPages() {
  const files = [];
  for (const dir of ['', 'texts']) {
    for (const f of fs.readdirSync(path.join(DOCS, dir)).sort()) if (f.endsWith('.html')) files.push(path.posix.join(dir, f));
  }
  const inScope = [], outOfScope = [], needsSyncParse = new Set();
  for (const f of files) {
    const html = fs.readFileSync(path.join(DOCS, f), 'utf8');
    if (!/<script[^>]*src="hierojax\.js"/.test(html)) continue;
    const used = [];
    const scripts = [...html.matchAll(/<script[^>]*src="([^"]+)"/g)].map((m) => m[1]).filter((s) => s !== 'hierojax.js');
    const inline = [...html.matchAll(/<script(?![^>]*src=)[^>]*>([\s\S]*?)<\/script>/g)].map((m) => m[1]);
    const sources = [...inline];
    for (const s of scripts) {
      const p = path.join(DOCS, path.posix.dirname(f), s);
      if (fs.existsSync(p)) sources.push(fs.readFileSync(p, 'utf8'));
    }
    for (const src of sources) {
      const m = src.match(INTERNALS);
      if (m) used.push(m[0]);
      if (/\bsyntax\.parse\b/.test(src)) needsSyncParse.add(f);
    }
    if (used.length) outOfScope.push({ page: f, reason: `its scripts use the internals ${[...new Set(used)].join(', ')}` });
    else inScope.push(f);
  }
  return { inScope, outOfScope, needsSyncParse };
}

// In the page: wait until the DOM has not changed for `quiet` ms, then take the snapshot.
// Self-contained: shipped as source.
function waitQuiet(quiet) {
  return new Promise((resolve) => {
    let timer;
    const done = () => { obs.disconnect(); resolve(); };
    const obs = new MutationObserver(() => { clearTimeout(timer); timer = setTimeout(done, quiet); });
    obs.observe(document.documentElement, { subtree: true, childList: true, attributes: true, characterData: true });
    timer = setTimeout(done, quiet);
  });
}

function takeSnapshot(canonSrc) {
  const canon = (0, eval)('(' + canonSrc + ')');
  const hash64 = (u32) => {
    let h1 = 0xdeadbeef | 0, h2 = 0x41c6ce57 | 0;
    for (let i = 0; i < u32.length; i++) {
      const k = u32[i];
      h1 = Math.imul(h1 ^ k, 2654435761);
      h2 = Math.imul(h2 ^ k, 1597334677);
    }
    h1 = Math.imul(h1 ^ (h1 >>> 16), 2246822507) ^ Math.imul(h2 ^ (h2 >>> 13), 3266489909);
    h2 = Math.imul(h2 ^ (h2 >>> 16), 2246822507) ^ Math.imul(h1 ^ (h1 >>> 13), 3266489909);
    return (h2 >>> 0).toString(16).padStart(8, '0') + (h1 >>> 0).toString(16).padStart(8, '0');
  };
  const canvasInfo = (c) => {
    const r = { w: c.width, h: c.height };
    try {
      const d = c.getContext('2d').getImageData(0, 0, c.width, c.height).data;
      r.hash = hash64(new Uint32Array(d.buffer, d.byteOffset, d.byteLength >> 2));
    } catch (e) {
      r.hash = 'ERR ' + (e && e.name);
    }
    return r;
  };
  const frags = [...document.getElementsByClassName('hierojax')];
  return {
    html: canon(document.documentElement),
    canvases: [...document.querySelectorAll('canvas')].map(canvasInfo),
    rects: frags.map((el) => {
      const r = el.getBoundingClientRect();
      return [r.left, r.top + window.scrollY, r.width, r.height];
    }),
    nFragments: frags.length,
    nEmpty: frags.filter((el) => !el.firstChild).length,
    text: document.body ? document.body.innerText : '',
    console: window.__hj.msgs.slice(),
    alerts: window.__hj.alerts.slice(),
  };
}

const PARSE_ERROR = /^(log: Parse error:\n[\s\S]*?Error: )(?:Lexical error|Parse error|parse error)[\s\S]*$/;
const normConsole = (list) => list.map((m) => m.replace(PARSE_ERROR, '$1<parse error>'));

function firstDiff(a, b) {
  const x = a.split('\n'), y = b.split('\n');
  let p = 0;
  while (p < x.length && p < y.length && x[p] === y[p]) p++;
  return `line ${p + 1}:\n      - ${(x[p] ?? '(end)').slice(0, 240)}\n      + ${(y[p] ?? '(end)').slice(0, 240)}`;
}

function compareSnapshots(a, b) {
  const out = [];
  if (a.html !== b.html) out.push('document: ' + firstDiff(a.html, b.html));
  const ca = JSON.stringify(a.canvases), cb = JSON.stringify(b.canvases);
  if (ca !== cb) out.push(`canvases: ${a.canvases.length} vs ${b.canvases.length}, ${a.canvases.filter((c, i) => JSON.stringify(c) !== JSON.stringify(b.canvases[i])).length} differ`);
  if (JSON.stringify(a.rects) !== JSON.stringify(b.rects)) out.push('bounding boxes of the fragments');
  if (a.text !== b.text) out.push('visible text');
  if (JSON.stringify(normConsole(a.console)) !== JSON.stringify(normConsole(b.console))) out.push(`console: ${JSON.stringify(a.console.slice(0, 2)).slice(0, 200)} vs ${JSON.stringify(b.console.slice(0, 2)).slice(0, 200)}`);
  if (JSON.stringify(a.alerts) !== JSON.stringify(b.alerts)) out.push('alerts');
  if (JSON.stringify(a.errors) !== JSON.stringify(b.errors)) out.push(`exceptions: ${JSON.stringify(a.errors)} vs ${JSON.stringify(b.errors)}`);
  return out;
}

async function newContext(browser, config) {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
  await ctx.addInitScript(INIT_SCRIPT);
  if (config) await ctx.addInitScript(`window.HIEROJAX_CONFIG = ${JSON.stringify(config)};`);
  return ctx;
}

function watch(page) {
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e.message).slice(0, 300)));
  page.on('dialog', (d) => d.dismiss().catch(() => {}));
  return errors;
}

async function snapshot(browser, server, urlPath, config, quiet, scenario) {
  const ctx = await newContext(browser, config);
  const page = await ctx.newPage();
  page.setDefaultTimeout(120000);
  const errors = watch(page);
  await page.goto(server.url + '/' + urlPath, { waitUntil: 'load' });
  await page.waitForFunction(() => typeof hierojax !== 'undefined' && hierojax.nFontsLoaded == hierojax.nFonts, null, { timeout: 60000 });
  const quietly = () => page.evaluate(([q, fn]) => (0, eval)('(' + fn + ')')(q), [quiet, waitQuiet.toString()]);
  await quietly();
  if (scenario) {
    // The page has finished loading, as a visitor would find it; now the visitor acts.
    errors.length = 0;
    await page.evaluate(() => { window.__hj.msgs.length = 0; window.__hj.alerts.length = 0; });
    await scenario(page);
    await quietly();
  }
  const snap = await page.evaluate(([src, fn]) => (0, eval)('(' + fn + ')')(src), [CANON_SRC, takeSnapshot.toString()]);
  snap.errors = errors;
  await ctx.close();
  return snap;
}

// The interactive page driven the way a visitor does: pick a type, render again.
const INTERACTIVE_SCENARIOS = [];
for (const type of ['svg', 'canvas', 'dom']) {
  for (const dir of ['hlr', 'vrl']) {
    INTERACTIVE_SCENARIOS.push({
      name: `interactive.html with type=${type}, dir=${dir}`,
      page: 'interactive.html',
      run: async (page) => {
        await page.evaluate(([t, d]) => {
          const pick = (name, value) => { document.querySelector(`input[name=${name}][value=${value}]`).checked = true; };
          pick('type', t);
          pick('dir', d);
          pick('signcolor', d == 'hlr' ? 'black' : 'red');
          pick('shadepattern', d == 'hlr' ? 'uniform' : 'hatching');
          pick('bracketcolor', d == 'hlr' ? 'black' : 'blue');
          HieroJaxInteractive.createHTML();
        }, [type, dir]);
      },
    });
  }
}

async function pageMode(engine, refImpl, newImpl, refConfig, newConfig, pages, quiet) {
  const browser = await launch(engine);
  const sa = await startServer({ impl: refImpl });
  const sb = await startServer({ impl: newImpl });
  const sc = v.embedded && !v.site ? await startServer({ impl: v.embedded }) : sb;
  let bad = 0;
  const jobs = pages.map((p) => ({ name: p, page: p }));
  if (pages.includes('interactive.html')) jobs.push(...INTERACTIVE_SCENARIOS);
  for (const job of jobs) {
    const a = await snapshot(browser, sa, job.page, refConfig, quiet, job.run);
    const b = await snapshot(browser, needsSyncParse.has(job.page) ? sc : sb, job.page, newConfig, quiet, job.run);
    const diffs = compareSnapshots(a, b);
    const via = needsSyncParse.has(job.page) && sc !== sb ? ' [embedded build]' : '';
    const what = `${a.nFragments} fragments (${a.nEmpty} empty), ${a.canvases.length} canvases, ${a.console.length} console lines, ${a.errors.length} exceptions`;
    if (diffs.length) {
      bad++;
      console.log(`  DIFF ${job.name}${via}: ${what}\n    ${diffs.join('\n    ')}`);
    } else {
      console.log(`  same ${job.name}${via}: ${what}`);
    }
  }
  await browser.close();
  await sa.close();
  await sb.close();
  if (sc !== sb) await sc.close();
  return bad;
}

const esc = (t) => t.replace(/&/g, '&amp;').replace(/</g, '&lt;');
const escAttr = (t) => esc(t).replace(/"/g, '&quot;');

function corpusHtml(items) {
  const body = items
    .map((it) => {
      const attrs = Object.entries(it.attrs).map(([k, val]) => ` ${k}="${escAttr(val)}"`).join('');
      return `<div style="font-size:${it.fontSize};color:${it.color}"><span class="hierojax"${attrs}>${esc(it.text)}</span></div>`;
    })
    .join('\n');
  return `<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><title>corpus</title>
<link rel="stylesheet" type="text/css" href="hierojax.css" />
<script type="text/javascript" src="hierojax.js"></script>
<script type="text/javascript">
window.addEventListener("DOMContentLoaded", () => { hierojax.processFragments(); });
</script></head><body>
${body}
</body></html>
`;
}

// In the page: per fragment, a hash of its canonical DOM, its canvases and its box.
async function corpusSnapshot(canonSrc, quiet) {
  const canon = (0, eval)('(' + canonSrc + ')');
  await new Promise((resolve) => {
    let timer;
    const done = () => { obs.disconnect(); resolve(); };
    const obs = new MutationObserver(() => { clearTimeout(timer); timer = setTimeout(done, quiet); });
    obs.observe(document.documentElement, { subtree: true, childList: true, attributes: true });
    timer = setTimeout(done, quiet * 4);
  });
  const fnv = (s) => {
    let h1 = 0x811c9dc5 | 0, h2 = 0x1000193 | 0;
    for (let i = 0; i < s.length; i++) {
      const c = s.charCodeAt(i);
      h1 = Math.imul(h1 ^ c, 16777619);
      h2 = Math.imul(h2 + c, 2246822519) ^ (h2 >>> 15);
    }
    return (h1 >>> 0).toString(16) + (h2 >>> 0).toString(16);
  };
  const pix = (c) => {
    try {
      const d = c.getContext('2d').getImageData(0, 0, c.width, c.height).data;
      let h1 = 0xdeadbeef | 0, h2 = 0x41c6ce57 | 0;
      const u = new Uint32Array(d.buffer, d.byteOffset, d.byteLength >> 2);
      for (let i = 0; i < u.length; i++) {
        h1 = Math.imul(h1 ^ u[i], 2654435761);
        h2 = Math.imul(h2 ^ u[i], 1597334677);
      }
      return c.width + 'x' + c.height + ':' + (h1 >>> 0).toString(16) + (h2 >>> 0).toString(16);
    } catch (e) {
      return 'ERR ' + (e && e.name);
    }
  };
  const spans = [...document.querySelectorAll('body > div > span')];
  const rows = spans.map((s) => {
    const r = s.getBoundingClientRect();
    return fnv(canon(s)) + '|' + [...s.querySelectorAll('canvas')].map(pix).join(',') + '|' + [r.left, r.top + window.scrollY, r.width, r.height].join(',');
  });
  window.__canonOf = (i) => canon(spans[i]);
  return { rows, nEmpty: spans.filter((s) => !s.firstChild).length, console: window.__hj.msgs.slice(), alerts: window.__hj.alerts.slice() };
}

async function corpusMode(engine, impl, quiet) {
  let items = loadCorpus().items;
  // What upstream throws on would stop a real page's loop; those are the batching tests' business.
  const ref = path.join(OUT_DIR, `upstream-${engine}-A.json.gz`);
  let upstream;
  if (fs.existsSync(ref)) {
    upstream = readJson(ref).results;
  } else {
    console.log(`  (no ${path.relative(process.cwd(), ref)}: running upstream per fragment first)`);
    const r = await runCorpus({ impl: 'upstream', engine, runId: 'A', log: () => {} });
    writeJson(ref, r);
    upstream = r.results;
  }
  items = items.filter((it) => upstream[it.id] && !upstream[it.id].err && !upstream[it.id].crash);
  const html = corpusHtml(items);
  const browser = await launch(engine);
  const opened = [];
  for (const which of ['upstream', impl]) {
    const server = await startServer({ impl: which, pages: { '/corpus.html': html } });
    const ctx = await newContext(browser);
    const page = await ctx.newPage();
    page.setDefaultTimeout(900000);
    const errors = watch(page);
    await page.goto(server.url + '/corpus.html', { waitUntil: 'load' });
    // The page may be busy laying out the whole corpus by now: wait as long as that takes.
    await page.waitForFunction(() => hierojax.nFontsLoaded == hierojax.nFonts, null, { timeout: 900000 });
    const snap = await page.evaluate(([src, q, fn]) => (0, eval)('(' + fn + ')')(src, q), [CANON_SRC, Number(quiet), corpusSnapshot.toString()]);
    snap.errors = errors;
    opened.push({ page, ctx, server, snap });
  }
  const [a, b] = opened;
  let diff = 0, first = -1;
  for (let i = 0; i < a.snap.rows.length; i++) {
    if (a.snap.rows[i] !== b.snap.rows[i]) {
      diff++;
      if (first < 0) first = i;
    }
  }
  const con = JSON.stringify(normConsole(a.snap.console)) === JSON.stringify(normConsole(b.snap.console));
  const same = a.snap.rows.length === b.snap.rows.length;
  const errs = JSON.stringify(a.snap.errors) === JSON.stringify(b.snap.errors);
  console.log(`  ${items.length} corpus fragments in one page: ${a.snap.rows.length - diff} identical, ${diff} differ` +
    `; ${a.snap.nEmpty}/${b.snap.nEmpty} left empty; console ${con ? 'same' : 'DIFFERS'} (${a.snap.console.length} lines); exceptions ${errs ? 'same' : 'DIFFER'} (${a.snap.errors.length})` +
    (same ? '' : `; counts ${a.snap.rows.length} vs ${b.snap.rows.length}`));
  if (diff) {
    const it = items[first];
    const [x, y] = await Promise.all(opened.map((o) => o.page.evaluate((i) => window.__canonOf(i), first)));
    console.log(`    first at ${it.id}: ${a.snap.rows[first].split('|').slice(1).join('|')} vs ${b.snap.rows[first].split('|').slice(1).join('|')}\n    ${firstDiff(x, y)}`);
  }
  if (!con) console.log('    console:', JSON.stringify(a.snap.console.slice(0, 2)).slice(0, 200), JSON.stringify(b.snap.console.slice(0, 2)).slice(0, 200));
  for (const o of opened) {
    await o.ctx.close();
    await o.server.close();
  }
  await browser.close();
  return diff || !con || !errs || !same ? 1 : 0;
}

const { inScope, outOfScope, needsSyncParse } = discoverPages();
const wanted = v.pages ? v.pages.split(',') : inScope;
let bad = 0;
if (!v['no-pages'] && !v['corpus-page']) {
  console.log(`pages in scope (${inScope.length}): ${inScope.filter((p) => !p.startsWith('texts/')).join(' ')} and ${inScope.filter((p) => p.startsWith('texts/')).length} in texts/`);
  console.log(`  of which call syntax.parse while loading: ${[...needsSyncParse].join(' ') || 'none'}${needsSyncParse.size && !v.embedded ? ' (no --embedded build given: they will fail with the default one)' : ''}`);
  console.log(`out of scope (${outOfScope.length}):\n` + outOfScope.map((o) => `  ${o.page}: ${o.reason}`).join('\n'));
}
for (const engine of v.engines.split(',')) {
  console.log(`${engine}${v.site ? ' (site options)' : ''}:`);
  const refImpl = v.site ? path.resolve(v.site) : 'upstream';
  const newConfig = v.site ? SITE_OPTIONS : undefined;
  if (v['corpus-page']) {
    bad += await corpusMode(engine, v.impl, v.quiet);
  } else if (!v['no-pages']) {
    bad += await pageMode(engine, refImpl, v.impl, undefined, newConfig, wanted, Number(v.quiet));
  }
}
console.log(bad ? `${bad} difference(s)` : 'no differences');
process.exit(bad ? 1 : 0);
