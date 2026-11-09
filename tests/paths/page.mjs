#!/usr/bin/env node
// Path blocks on a page with no script, no font and no stylesheet, under the CSP
//   default-src 'self'; style-src 'self' 'unsafe-inline'
// in the three engines, with many blocks (the corpus, every class, every direction, separated):
//   * no CSP violation (the page listens for `securitypolicyviolation` through Playwright, which
//     the page's own policy does not bar), no console error, no failed request;
//   * the page makes no request but its own: no font, no stylesheet, no image, no script;
//   * no `id` in the document, and no reference (`url(#`, `href`) in any block;
//   * every block has its size in em (the `svg` is `width` em wide at the page's font size) and the
//     accessible name; the block sits on the line as consistent mode's does (the bottom of the box
//     is 0.1em below the baseline), measured against a text block in the same page.
//
//   cargo build --release -p hieroxide-cli
//   node tests/paths/page.mjs [--engines chromium,firefox,webkit] [--n 400]
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { launch } from '../parity/lib/engines.mjs';
import { CSP, FONT, ROOT, batchOptions, loadCorpus, renderBatch, servePages } from './lib.mjs';

const { values: v } = parseArgs({ options: { engines: { type: 'string', default: 'chromium,firefox,webkit' }, n: { type: 'string', default: '400' } } });

const corpus = loadCorpus().filter((f) => f.attrs.type !== 'canvas' && f.attrs.type !== 'dom' && f.text.length < 400);
const step = Math.max(1, Math.floor(corpus.length / Number(v.n)));
const sample = corpus.filter((_, i) => i % step === 0);
// Also each class with each direction and separated.
const extra = [];
for (const f of corpus.filter((c) => c.text.length < 60).filter((_, i) => i % 211 === 0)) {
  for (const o of [{ dir: 'hrl' }, { dir: 'vlr' }, { dir: 'vrl' }, { separated: true }, { separated: true, dir: 'hrl' }, { separated: true, dir: 'vlr' }, { border: true }, { shading: 'hatching' }, { fontSize: 40 }]) extra.push({ text: f.text, ...o });
}
const items = [...sample.map((f) => ({ text: f.text, ...batchOptions(f.attrs) })), ...extra];
const blocks = renderBatch(items.map((i) => ({ ...i, label: 'a line of hieroglyphs' })));
const texts = renderBatch(items.map((i) => ({ ...i, kind: 'text' })));
const ok = items.map((_, i) => blocks[i].ok && texts[i].ok);
const shown = items.filter((_, i) => ok[i]).length;

// The CSS the module gives a page for the text printer, and the font: for the comparison block only.
const moduleJs = fs.readFileSync(path.join(ROOT, 'js/hieroxide/index.js'), 'utf8');
const CSS = moduleJs.match(/const CSS = `([^`]*)`/)[1];
const textCss = `@font-face{font-family:Hieroglyphic;src:url(/NewGardiner.otf)}${CSS}`;

const body = items.map((it, i) => (ok[i] ? `<p class="row" data-i="${i}">line ${i}: ${blocks[i].svg} end</p>` : '')).join('\n');
// One text block and one path block on a line of their own, for the alignment.
const aligned = (svgBlock) => `<p id="align" style="font-size:40px;margin:30px 10px"><span class="mk" style="display:inline-block;width:0;height:0"></span>${svgBlock}<span class="mk2" style="display:inline-block;width:0;height:0"></span></p>`;
const probe = { text: '\u{13000}\u{13431}\u{13001}' };
const [probePath, probeText] = [renderBatch([{ ...probe }])[0].svg, renderBatch([{ ...probe, kind: 'text' }])[0].svg];

const files = {
  // The page: no script, no link, no font.
  '/page.html': `<!doctype html><html lang="en"><head><meta charset="utf-8"><title>path blocks</title></head><body style="font:30px/1.5 serif;margin:12px">\n${body}\n</body></html>`,
  '/aligned-paths.html': `<!doctype html><meta charset="utf-8"><title>a</title><body style="margin:0">${aligned(probePath)}</body>`,
  '/aligned-text.html': `<!doctype html><meta charset="utf-8"><title>a</title><style>${textCss}</style><body style="margin:0">${aligned(`<span class="hierojax">${probeText}</span>`)}</body>`,
  '/NewGardiner.otf': fs.readFileSync(FONT),
  // A control: a page that does break the policy, to show that the listener sees a violation.
  '/control.html': '<!doctype html><meta charset="utf-8"><title>c</title><img src="https://example.invalid/x.png"><svg><rect style="fill:red" width="1" height="1"/></svg>',
};
const srv = await servePages(files, { headers: { 'Content-Security-Policy': CSP } });
const csp = srv; // the same server answers with the header on every response

let failures = 0;
const check = (ok2, what) => {
  if (!ok2) {
    failures++;
    console.error(`FAIL ${what}`);
  }
};

for (const eng of v.engines.split(',')) {
  const browser = await launch(eng);
  const ctx = await browser.newContext({ viewport: { width: 1100, height: 800 } });
  const page = await ctx.newPage();
  const problems = { violations: [], console: [], failed: [], requests: [] };
  await page.addInitScript(() => {
    window.__csp = [];
    document.addEventListener('securitypolicyviolation', (e) => window.__csp.push(`${e.violatedDirective} ${e.blockedURI}`));
  });
  page.on('console', (m) => { if (['error', 'warning'].includes(m.type())) problems.console.push(m.text()); });
  page.on('pageerror', (e) => problems.console.push(String(e)));
  page.on('requestfailed', (r) => problems.failed.push(r.url()));
  page.on('request', (r) => problems.requests.push(new URL(r.url()).pathname));
  // The control first: the listener must see a violation where there is one.
  await page.goto(`${srv.url}/control.html`);
  await page.waitForTimeout(300);
  check((await page.evaluate(() => window.__csp)).length > 0, `${eng}: the control page's violation is seen`);
  for (const k of Object.keys(problems)) problems[k].length = 0;
  const resp = await page.goto(`${srv.url}/page.html`);
  check(resp.headers()['content-security-policy'] === CSP, `${eng}: the page is served with the CSP header`);
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(300);
  const r = await page.evaluate(() => {
    const rows = [...document.querySelectorAll('p.row')];
    const bad = [];
    let svgs = 0;
    for (const p of rows) {
      for (const svg of p.querySelectorAll('svg')) {
        svgs++;
        const w = parseFloat(svg.getAttribute('width')); // em
        const vb = svg.viewBox.baseVal;
        const px = parseFloat(getComputedStyle(svg).fontSize) * w;
        const box = svg.getBoundingClientRect();
        // The border of a block adds 2 px to the box of a wrapper, not to the svg's own.
        if (Math.abs(box.width - px) > (svg.hasAttribute('style') && /border/.test(svg.getAttribute('style')) ? 2.5 : 0.6) && !svg.closest('[style*="border"]')) bad.push(`${p.dataset.i}: svg is ${box.width}px, ${w}em is ${px}px`);
        if (!(vb.width > 0 && vb.height > 0)) bad.push(`${p.dataset.i}: no viewBox`);
      }
    }
    const named = [...document.querySelectorAll('[role="img"]')];
    return {
      rows: rows.length,
      svgs,
      ids: document.querySelectorAll('[id]').length,
      refs: document.documentElement.outerHTML.match(/url\(#|href=|xlink|<use|<defs|<image|<script|<link|<style/g)?.length ?? 0,
      named: named.length,
      unnamed: named.filter((e) => !e.getAttribute('aria-label')).length,
      bad,
      csp: window.__csp,
      overlays: document.querySelectorAll('[aria-hidden="true"]').length,
      dataText: document.querySelectorAll('[data-text]').length,
    };
  });
  check(r.rows === shown, `${eng}: ${r.rows} rows, ${shown} expected`);
  check(r.csp.length === 0, `${eng}: CSP violations: ${r.csp.slice(0, 3).join('; ')}`);
  check(problems.console.length === 0, `${eng}: console messages: ${problems.console.slice(0, 3).join('; ')}`);
  check(problems.failed.length === 0, `${eng}: failed requests: ${problems.failed.join(', ')}`);
  const other = problems.requests.filter((p) => p !== '/page.html' && p !== '/favicon.ico');
  check(other.length === 0, `${eng}: the page asked for ${other.join(', ')}`);
  check(r.ids === 0, `${eng}: ${r.ids} elements with an id`);
  check(r.refs === 0, `${eng}: ${r.refs} references, scripts, styles or links`);
  check(r.unnamed === 0 && r.named === r.dataText, `${eng}: ${r.named} named, ${r.unnamed} without a label, ${r.dataText} with data-text`);
  check(r.bad.length === 0, `${eng}: sizes: ${r.bad.slice(0, 3).join('; ')}`);

  // Aligned on the line as consistent mode: the box's bottom against the baseline.
  const measure = async (name) => {
    const p2 = await ctx.newPage();
    await p2.goto(`${srv.url}/${name}`);
    await p2.evaluate(() => document.fonts.ready);
    await p2.waitForTimeout(200);
    const m = await p2.evaluate(() => {
      const mk = document.querySelector('.mk').getBoundingClientRect();
      const svg = document.querySelector('svg').getBoundingClientRect();
      const line = document.getElementById('align').getBoundingClientRect();
      const wrap = document.querySelector('svg').parentElement;
      const box = wrap.localName === 'span' && wrap.style.position === 'relative' ? wrap.getBoundingClientRect() : svg;
      return { below: svg.bottom - mk.bottom, top: svg.top - mk.bottom, left: svg.left - mk.left, w: svg.width, h: svg.height, line: line.height, wrapH: box.height };
    });
    await p2.close();
    return m;
  };
  const t = await measure('aligned-text.html');
  const p = await measure('aligned-paths.html');
  check(Math.abs(t.below - 4) < 0.6, `${eng}: text block's bottom is ${t.below.toFixed(2)}px below the baseline, 0.1em is 4px`);
  check(Math.abs(p.wrapH - p.h) < 0.01, `${eng}: the wrapper of a block is ${p.wrapH}px tall, its svg ${p.h}px`);
  check(Math.abs(p.below - t.below) < 0.5 && Math.abs(p.top - t.top) < 0.5 && Math.abs(p.w - t.w) < 0.5 && Math.abs(p.h - t.h) < 0.5, `${eng}: path block ${JSON.stringify(p)} against text block ${JSON.stringify(t)}`);
  console.log(`${eng}: ${r.rows} blocks, ${r.svgs} svgs, ${r.named} named, ${r.dataText} with data-text, ${r.overlays} overlays; no violations, ${problems.requests.length} request(s) (${[...new Set(problems.requests)].join(', ')}); alignment path ${p.below.toFixed(2)}px / text ${t.below.toFixed(2)}px below the baseline, line height ${p.line.toFixed(1)} / ${t.line.toFixed(1)}`);
  await browser.close();
}
await srv.close();
if (failures) {
  console.error(`${failures} checks failed`);
  process.exit(1);
}
console.log('ok');
