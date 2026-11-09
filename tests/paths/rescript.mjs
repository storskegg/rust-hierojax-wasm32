#!/usr/bin/env node
// What happens to a filled page that still has `class="hierojax"` on its spans and still loads a
// hierojax script. `processFragmentNow` of upstream reads `elem.innerText`, empties the element and
// parses that text; the replacements do the same (the compat build to the byte, the bench stand-in
// of the consistent module as the drop-in does it). A block filled by `hieroxide fill` is not that
// text, so each is measured: what `innerText` of the span is, and what the page looks like after.
//
//   node tools/build-compat.mjs && node tests/bench/make-consistent.mjs && node tools/build-consistent.mjs --only consistent-paths
//   cargo build --release -p hieroxide-cli
//   node tests/paths/rescript.mjs [--engines chromium,firefox,webkit] [--compat DIR]
//
// Scripts: `upstream` is docs/hierojax.js, `compat` is build/compat, `consistent` is the drop-in stand-in of
// the module (build/consistent-bench), and `module` is the module's own `render(el, el.innerText, ...)`
// (build/consistent-paths) as a page that moved to it would call it. Each is run on the same page with the
// span filled three ways (`selectable` overlay, svg, none), and on a page whose span holds the source (the
// baseline: what the script makes of a span it is meant for). Outcomes:
//   redrawn    the span now holds what the script makes of the source (the same markup as the baseline's)
//   changed    the span holds a block, but not the baseline's
//   red        the script's red "parse error; see log" in the span (and the text in the log)
//   wiped      the span is empty, or holds a drawing of nothing (the empty fragment)
//   kept       the block is as it was (the module leaves an element alone on an error)
// This is a record of what the scripts do, with one gate: compat reads `data-text`, so for every kind of
// block and in every engine it must redraw (exit 1 otherwise). The others are recorded, not gated.
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { launch } from '../parity/lib/engines.mjs';
import { FONT, ROOT, loadCorpus, renderBatch, servePages } from './lib.mjs';

const { values: v } = parseArgs({ options: { compat: { type: 'string' }, engines: { type: 'string', default: 'chromium,firefox,webkit' }, out: { type: 'string', default: path.join(path.dirname(new URL(import.meta.url).pathname), 'out/rescript') } } });
fs.mkdirSync(v.out, { recursive: true });

const corpus = loadCorpus();
const SOURCE = corpus.filter((f) => f.group === 'site').map((f) => f.text)[2];
const KINDS = ['overlay', 'svg', 'none'];
const blocks = renderBatch(KINDS.map((k) => ({ text: SOURCE, contents: true, selectable: k })));
for (const b of blocks) if (!b.ok) throw new Error(b.message);

const files = {};
const addDir = (prefix, dir, names) => {
  for (const n of names) files[`/${prefix}/${n}`] = fs.readFileSync(path.join(ROOT, 'build', dir, n));
  files[`/${prefix}/NewGardiner.otf`] = fs.readFileSync(FONT);
};
files['/upstream/hierojax.js'] = fs.readFileSync(path.join(ROOT, 'docs/hierojax.js'));
files['/upstream/NewGardiner.otf'] = fs.readFileSync(FONT);
// `--compat DIR` tries another build of the drop-in (a directory with hierojax.js and the wasm) in its place.
for (const n of ['hierojax.js', 'hieroxide_wasm_bg.wasm']) files[`/compat/${n}`] = fs.readFileSync(path.join(v.compat ?? path.join(ROOT, 'build/compat'), n));
files['/compat/NewGardiner.otf'] = fs.readFileSync(FONT);
addDir('consistent', 'consistent-bench', ['hierojax.js', 'hieroxide_outline_bg.wasm']);
addDir('module', 'consistent-paths', ['index.js', 'hieroxide_outline.js', 'hieroxide_outline_bg.wasm']);

// The page: a span as the site has it, the script, and a call at DOMContentLoaded, as the site's patched script makes.
const page = (impl, contents) => {
  const head = '<!doctype html><html><head><meta charset="utf-8"><title>x</title></head><body style="font:30px serif;margin:20px">';
  const span = `<span class="hierojax" id="s">${contents}</span>`;
  const probe = '<script>addEventListener("DOMContentLoaded",()=>{const s=document.getElementById("s");window.__before={html:s.innerHTML,inner:s.innerText,text:s.textContent};});</script>';
  if (impl === 'module') {
    return `${head}${span}${probe}<script type="module">
      import { init, render } from "/module/index.js";
      await init({ font: "/module/NewGardiner.otf" });
      const s = document.getElementById("s");
      const r = render(s, s.innerText, { svg: "paths", contents: true });
      window.__result = r.ok ? "ok" : r.kind + ": " + r.message;
      window.__called = true;
    </script></body></html>`;
  }
  return `${head}${span}${probe}<script src="/${impl}/hierojax.js"></script><script>addEventListener("DOMContentLoaded",()=>hierojax.processFragments());</script></body></html>`;
};
const IMPLS = ['upstream', 'compat', 'consistent', 'module'];
for (const impl of IMPLS) {
  files[`/${impl}/baseline.html`] = page(impl, SOURCE);
  KINDS.forEach((k, i) => { files[`/${impl}/${k}.html`] = page(impl, blocks[i].svg); });
}
const srv = await servePages(files);

const settle = async (pg, before) => {
  // Done when the span's contents are not what they were, then a moment more (the scripts wait for the font).
  for (let i = 0; i < 60; i++) {
    await pg.waitForTimeout(250);
    const now = await pg.evaluate(() => document.getElementById('s')?.innerHTML ?? null);
    const called = await pg.evaluate(() => window.__called === true);
    if (now !== before || called) break;
  }
  await pg.waitForTimeout(600);
};

const results = [];
for (const eng of v.engines.split(',')) {
  const browser = await launch(eng);
  for (const impl of IMPLS) {
    const ctx = await browser.newContext({ viewport: { width: 900, height: 300 } });
    const run = async (name) => {
      const pg = await ctx.newPage();
      const logs = [];
      pg.on('console', (m) => logs.push(m.text()));
      pg.on('pageerror', (e) => logs.push(`pageerror: ${e}`));
      pg.on('dialog', (d) => { logs.push(`alert: ${d.message()}`); d.dismiss(); });
      await pg.goto(`${srv.url}/${impl}/${name}.html`);
      const before = await pg.evaluate(() => window.__before ?? null);
      await settle(pg, before?.html);
      const after = await pg.evaluate(() => {
        const s = document.getElementById('s');
        const sel = getSelection();
        sel.selectAllChildren(s);
        const copy = String(sel);
        sel.removeAllRanges();
        return { html: s.innerHTML, text: s.textContent, svgs: s.querySelectorAll('svg').length, canvases: s.querySelectorAll('canvas').length, cls: s.className, style: s.getAttribute('style'), copy, result: window.__result ?? null };
      });
      await pg.close();
      return { before, after, logs };
    };
    const base = await run('baseline');
    for (const k of KINDS) {
      const r = await run(k);
      const a = r.after;
      let outcome;
      if (/parse error/.test(a.text) && a.svgs === 0) outcome = 'red';
      else if (a.html === '') outcome = 'wiped';
      else if (a.html === r.before.html) outcome = 'kept';
      else if (a.html === base.after.html && a.cls === base.after.cls && a.style === base.after.style) outcome = 'redrawn';
      else if (a.copy !== SOURCE && a.svgs <= 1 && !/<path|<tspan/.test(a.html)) outcome = 'wiped (empty drawing)';
      else outcome = 'changed';
      results.push({
        engine: eng, script: impl, selectable: k, outcome,
        innerText: JSON.stringify(r.before.inner.replace(/[\u{13000}-\u{1345f}︀-️]/gu, '·')),
        innerTextIsSource: r.before.inner === SOURCE, textContentIsSource: r.before.text === SOURCE,
        copyAfter: a.copy === SOURCE ? 'source' : JSON.stringify(a.copy.replace(/[\u{13000}-\u{1345f}︀-️]/gu, '·').slice(0, 30)),
        log: r.logs.filter((l) => !/Failed to load resource|layout-shift/.test(l)).map((l) => l.replace(/[\u{13000}-\u{1345f}︀-️]/gu, '·').slice(0, 70)).join(' | '),
        result: a.result,
      });
    }
    // The baseline itself, so that "redrawn" has something to mean.
    results.push({ engine: eng, script: impl, selectable: 'baseline (source text)', outcome: base.after.svgs ? 'drawn' : base.after.canvases ? 'canvas' : 'NOT DRAWN', innerText: '-', innerTextIsSource: base.before.inner === SOURCE, textContentIsSource: true, copyAfter: base.after.copy === SOURCE ? 'source' : 'other', log: base.logs.join(' | ').slice(0, 60), result: base.after.result });
    await ctx.close();
  }
  await browser.close();
}
await srv.close();
fs.writeFileSync(path.join(v.out, 'results.json'), JSON.stringify(results, null, 1));
console.table(results.map((r) => ({ engine: r.engine, script: r.script, 'span filled with': r.selectable, outcome: r.outcome, 'innerText = source': r.innerTextIsSource, innerText: r.innerText.length > 40 ? r.innerText.slice(0, 40) + '…"' : r.innerText, 'copy after': r.copyAfter, log: r.log.slice(0, 40) })));
if (results.some((r) => r.outcome === 'NOT DRAWN')) {
  console.error('a baseline was not drawn: the run is not valid');
  process.exit(1);
}
// The gate: compat reads `data-text` of a block that was filled, so every kind redraws as if never filled.
const notRedrawn = results.filter((r) => r.script === 'compat' && r.selectable !== 'baseline (source text)' && r.outcome !== 'redrawn');
if (notRedrawn.length) {
  console.error(`compat did not redraw: ${notRedrawn.map((r) => `${r.engine}/${r.selectable}: ${r.outcome}`).join('; ')}`);
  process.exit(1);
}
