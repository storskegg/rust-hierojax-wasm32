#!/usr/bin/env node
// The browser module (js/hieroxide) against the native build, in the three engines:
//
//   node tools/build-consistent.mjs --only consistent && node tools/build-consistent.mjs --only consistent-paths
//   cargo build --release -p hieroxide-cli
//   node tests/paths/wasm.mjs [--engines chromium,firefox,webkit] [--n 600]
//
// * build/consistent-paths: `layout(text, {svg: 'paths'})` gives, byte for byte, what the CLI prints
//   for the same fragment and options, for a sample of the corpus of every class;
// * build/consistent: the same for text, against the CLI's `--kind text`; and `svg: 'paths'` is an
//   error of kind 'unsupported', not an exception;
// * errors are values with a kind and, for a parse error, a position; `parse` agrees.
import path from 'node:path';
import fs from 'node:fs';
import { parseArgs } from 'node:util';
import { launch } from '../parity/lib/engines.mjs';
import { FONT, ROOT, batchOptions, loadCorpus, renderBatch, servePages } from './lib.mjs';

const { values: v } = parseArgs({ options: { engines: { type: 'string', default: 'chromium,firefox,webkit' }, n: { type: 'string', default: '600' } } });
const corpus = loadCorpus().filter((f) => f.attrs.type !== 'canvas' && f.attrs.type !== 'dom');
const step = Math.max(1, Math.floor(corpus.length / Number(v.n)));
const sample = corpus.filter((_, i) => i % step === 0);
const items = sample.map((f) => ({ text: f.text, ...batchOptions(f.attrs) }));

// The same options under the module's names.
const jsOptions = (o) => {
  const m = { fontSize: 'fontsize', lineSize: 'linesize', signColor: 'signcolor', bracketColor: 'bracketcolor' };
  const out = {};
  for (const [k, val] of Object.entries(o)) {
    if (k === 'text') continue;
    if (k === 'shading') out.shadepattern = val;
    else out[m[k] ?? k] = val;
  }
  return out;
};

const nativePaths = renderBatch(items.map((i) => ({ ...i, kind: 'paths' })));
const nativeText = renderBatch(items.map((i) => ({ ...i, kind: 'text' })));

const files = {
  '/NewGardiner.otf': fs.readFileSync(FONT),
  '/page.html': '<!doctype html><meta charset="utf-8"><title>x</title><body></body>',
};
for (const dir of ['consistent', 'consistent-paths']) {
  for (const f of ['index.js', 'hieroxide_outline.js', 'hieroxide_outline_bg.wasm']) {
    files[`/${dir}/${f}`] = fs.readFileSync(path.join(ROOT, 'build', dir, f));
  }
}
const srv = await servePages(files);
let failures = 0;
const check = (ok, what) => {
  if (!ok) {
    failures++;
    console.error(`FAIL ${what}`);
  }
};

for (const eng of v.engines.split(',')) {
  const browser = await launch(eng);
  const ctx = await browser.newContext();
  for (const dir of ['consistent-paths', 'consistent']) {
    const page = await ctx.newPage();
    await page.goto(`${srv.url}/page.html`);
    const res = await page.evaluate(async ({ dir, items }) => {
      const m = await import(`/${dir}/index.js`);
      await m.init({ font: '/NewGardiner.otf' });
      const out = { paths: [], text: [] };
      for (const it of items) {
        const { text, ...o } = it;
        const p = m.layout(text, { ...o, svg: 'paths' });
        out.paths.push(p.ok ? { ok: true, markup: p.markup, className: p.className, text: p.text } : { ok: false, kind: p.kind });
        const t = m.layout(text, o);
        out.text.push(t.ok ? { ok: true, markup: t.markup, className: t.className, width: t.width, height: t.height, border: t.border } : { ok: false, kind: t.kind });
      }
      const bad = m.layout('\u{13430}');
      const lone = m.layout('\ud800');
      out.errors = { bad, lone, parse: m.parse('\u{13000}\u{13431}\u{13001}'), parseBad: m.parse('x'), parseOk: m.parse('\u{13000}') };
      // render into an element: text gets the class, paths do not
      const el = document.createElement('span');
      document.body.append(el);
      const r1 = m.render(el, '\u{13000}\u{13431}\u{13001}', { svg: 'paths' });
      out.render = { ok: r1.ok, kind: r1.kind, cls: el.className, html: el.innerHTML.slice(0, 60) };
      el.innerHTML = 'kept';
      m.render(el, 'x', { svg: 'paths' });
      out.kept = el.innerHTML;
      return out;
    }, { dir, items: items.map((i) => jsOptions(i) && { text: i.text, ...jsOptions(i) }) });

    const label = `${eng} ${dir}`;
    // text, both builds
    let same = 0;
    res.text.forEach((t, i) => {
      const n = nativeText[i];
      const ok = n.ok === t.ok && (!n.ok || n.svg === t.markup);
      if (ok) same++;
      else if (failures < 6) console.error(`${label}: text differs for ${sample[i].id}`);
    });
    check(same === items.length, `${label}: text output differs from the CLI in ${items.length - same} of ${items.length}`);
    if (dir === 'consistent-paths') {
      let sameP = 0;
      res.paths.forEach((t, i) => {
        const n = nativePaths[i];
        const ok = n.ok === t.ok && (!n.ok || n.svg === t.markup);
        if (ok) sameP++;
        else if (failures < 3) {
          const a = n.svg ?? '', b = t.markup ?? '';
          let k = 0;
          while (k < a.length && a[k] === b[k]) k++;
          console.error(`${label}: paths differ for ${sample[i].id} at ${k}:\n  cli  ${JSON.stringify(a.slice(Math.max(0, k - 60), k + 80))}\n  wasm ${JSON.stringify(b.slice(Math.max(0, k - 60), k + 80))}`);
          failures++;
        }
      });
      check(sameP === items.length, `${label}: path output differs from the CLI in ${items.length - sameP} of ${items.length}`);
      check(res.render.ok && res.render.cls === '' && res.render.html.startsWith('<span style'), `${label}: render of paths into an element`);
      check(res.kept === 'kept', `${label}: an error leaves the element as it was`);
      console.log(`${label}: ${sameP}/${items.length} path blocks and ${same}/${items.length} text blocks identical to the CLI`);
    } else {
      check(res.paths.every((p) => p.ok === false && p.kind === 'unsupported' || p.kind === 'parse' || p.kind === 'layout'), `${label}: paths in the default build are an error of kind unsupported`);
      check(res.paths.some((p) => p.kind === 'unsupported'), `${label}: some path request reports 'unsupported'`);
      console.log(`${label}: ${same}/${items.length} text blocks identical to the CLI; paths are 'unsupported'`);
    }
    const e = res.errors;
    check(!e.bad.ok && e.bad.kind === 'parse' && e.bad.position === 0 && /unexpected token/.test(e.bad.message), `${label}: parse error with its position`);
    check(!e.lone.ok && e.lone.kind === 'parse', `${label}: a lone surrogate is a parse error`);
    check(e.parse.ok && e.parseOk.ok && e.parseOk.text === '\u{13000}', `${label}: parse of a good fragment`);
    check(!e.parseBad.ok && e.parseBad.kind === 'parse' && e.parseBad.position === 0, `${label}: parse of a bad fragment`);
    await page.close();
  }
  await browser.close();
}
await srv.close();
if (failures) {
  console.error(`${failures} checks failed`);
  process.exit(1);
}
console.log('ok');
