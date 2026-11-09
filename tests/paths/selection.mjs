#!/usr/bin/env node
// Selecting and copying path blocks, in real browsers, on a page with no script and no font.
//
//   cargo build --release -p hieroxide-cli
//   node tests/paths/selection.mjs [--engines chromium,firefox,webkit] [--out tests/paths/out/selection]
//
// For each engine, fixture and kind of copyable text (`overlay`: a span beside the svg; `svg`: SVG
// text inside a bare svg; both are `hieroxide render --selectable ...`) it measures, against the
// *source* of the fragment (not against upstream's output):
//   block      `getSelection().toString()` after selecting all of the block
//   paragraph  the same for the paragraph "before BLOCK after"
//   clipboard  only with --clipboard: the real clipboard: select the block, copy with the keyboard,
//              paste into a textarea
//   drag       the mouse, from the middle of a sign near the start to one near the end: the selection
//              must be a part of the source, and begin and end at those signs
//   attribute  `data-text` of the block
// and writes a screenshot of the drag's highlight per engine, fixture and kind to --out.
// The page has no script: the only code that runs is Playwright's. The overlay must be exact
// everywhere (exit 1 otherwise); the svg kind is measured and reported, not gated, because
// Chromium and WebKit are known to add line breaks to it.
//
// The clipboard check is opt-in (`--clipboard`), because it is the real clipboard and replaces what is on
// yours; without the flag it is skipped, and the output says so. Needs tests/parity's install.
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { launch } from '../parity/lib/engines.mjs';
import { CSP, HERE, loadCorpus, renderBatch, servePages } from './lib.mjs';

const { values: v } = parseArgs({
  options: {
    engines: { type: 'string', default: 'chromium,firefox,webkit' },
    out: { type: 'string', default: path.join(HERE, 'out/selection') },
    kinds: { type: 'string', default: 'overlay,svg' },
    clipboard: { type: 'boolean', default: false },
  },
});
fs.mkdirSync(v.out, { recursive: true });

const corpus = loadCorpus();
const site = corpus.filter((f) => f.group === 'site').map((f) => f.text);
// A fragment of the corpus that parses and has what is asked for.
const firstOk = (re, ok = () => true) => {
  const cands = corpus.filter((f) => re.test(f.text) && f.text.length < 160 && ok(f)).slice(0, 80);
  const got = renderBatch(cands.map((c) => ({ text: c.text })));
  return cands[got.findIndex((g) => g.ok)]?.text;
};
// The enclosure end that the tree writes differently from the source (begin enclosure, end walled).
const mismatched = firstOk(/\u{1343F}/u, (f) => !f.text.includes('\u{1343E}'));
// A lost sign with a variation selector other than the first: upstream writes U+FE00 for it.
const lost = firstOk(/[\u{13443}-\u{13446}][\uFE01-\uFE06]/u, (f) => [...f.text].length >= 24);
// Signs that are rotated or mirrored, and a column to the left.
const rotated = corpus.filter((f) => f.group === 'rotsample' && [...f.text].length >= 3 && [...f.text].length < 30)[7]?.text;
const long = corpus.filter((f) => f.group === 'texts').map((f) => f.text).sort((a, b) => b.length - a.length)[0];

const FIXTURES = [
  { name: 'line', text: site[2], opts: {} },
  { name: 'line-hrl', text: site[2], opts: { dir: 'hrl' } },
  { name: 'column', text: site[0], opts: { dir: 'vlr' } },
  { name: 'separated', text: site[2], opts: { separated: true } },
  { name: 'enclosure-end', text: mismatched, opts: {} },
  { name: 'lost-selector', text: lost, opts: {} },
  { name: 'column-vrl', text: site[0], opts: { dir: 'vrl' } },
  { name: 'rotated', text: rotated, opts: {} },
  { name: 'rotated-hrl', text: rotated, opts: { dir: 'hrl' } },
  { name: 'wall', text: long, opts: {} },
].filter((f) => f.text);

const kinds = v.kinds.split(',');
const items = [];
for (const f of FIXTURES) for (const k of kinds) items.push({ f, k, line: { text: f.text, ...f.opts, selectable: k === 'svg' ? 'svg' : 'overlay' } });
const rendered = renderBatch(items.map((i) => i.line));
rendered.forEach((r, i) => {
  if (!r.ok) throw new Error(`${items[i].f.name}: ${r.message}`);
  items[i].svg = r.svg;
});

const page = (svg, text) => `<!doctype html><html lang="en"><head><meta charset="utf-8"><title>selection</title></head>
<body style="margin:24px;font:36px/1.4 serif;color:#000"><p id="p">before ${svg} after</p>
<textarea id="ta" rows="4" cols="60" style="font:14px monospace"></textarea></body></html>`;
const files = {};
items.forEach((i, n) => { files[`/${n}.html`] = page(i.svg); });
const srv = await servePages(files, { headers: { 'Content-Security-Policy': CSP } });

const show = (s) => (s === null ? 'null' : JSON.stringify(s.length > 24 ? s.slice(0, 12) + '…' : s).replace(/[\u{13000}-\u{1345f}︀-️]/gu, (c) => `<${c.codePointAt(0).toString(16)}>`));
// How a string differs from the source: exact, extra line breaks around, or something else.
const verdict = (got, want) => (got === want ? 'exact' : got.trim() === want ? 'line breaks' : got.replace(/\n/g, '') === want ? 'inner line breaks' : 'DIFFERENT');

const results = [];
for (const eng of v.engines.split(',')) {
  const browser = await launch(eng);
  const ctx = await browser.newContext({ viewport: { width: 1100, height: 700 }, deviceScaleFactor: 2 });
  for (let n = 0; n < items.length; n++) {
    const { f, k } = items[n];
    const want = f.text;
    const pg = await ctx.newPage();
    await pg.goto(`${srv.url}/${n}.html`);
    await pg.bringToFront();
    const info = await pg.evaluate(() => {
      const blk = document.querySelector('#p > span, #p > svg');
      const out = { data: document.querySelector('[data-text]')?.dataset.text ?? null };
      const sel = getSelection();
      sel.selectAllChildren(blk);
      out.block = String(sel);
      sel.selectAllChildren(document.getElementById('p'));
      out.paragraph = String(sel);
      sel.removeAllRanges();
      // the boxes of the signs, in document order
      const boxes = [...blk.querySelectorAll('tspan[x], span[style*="absolute"], span[style*="letter-spacing"]')].filter((e) => !e.matches('[aria-hidden]')).map((e) => {
        // A sign of a run (a line of signs) is as wide as its room on the line and as tall as the text's box.
        const run = e.style.letterSpacing !== '';
        const r = e.getBoundingClientRect();
        const pr = run ? e.parentElement.getBoundingClientRect() : r;
        return { x: r.x, y: pr.y, w: r.width, h: pr.height, len: e.textContent.length };
      });
      const r = blk.getBoundingClientRect();
      out.box = [r.x, r.y, r.width, r.height];
      out.boxes = boxes;
      return out;
    });
    // The real clipboard.
    let pasted = null;
    if (v.clipboard) {
      await pg.evaluate(() => getSelection().selectAllChildren(document.querySelector('#p > span, #p > svg')));
      await pg.keyboard.press('ControlOrMeta+c');
      await pg.click('#ta');
      await pg.keyboard.press('ControlOrMeta+v');
      pasted = await pg.evaluate(() => document.getElementById('ta').value);
      await pg.evaluate(() => { document.getElementById('ta').value = ''; getSelection().removeAllRanges(); });
    }
    // A drag from near the start to near the end.
    const rec = { engine: eng, fixture: f.name, kind: k, source: [...want].length };
    rec.attribute = info.data === want ? 'exact' : info.data === null ? 'absent' : 'DIFFERENT';
    rec.block = verdict(info.block, want);
    rec.paragraph = verdict(info.paragraph, `before ${want} after`.replace(/^before (?=.*)/, 'before '));
    rec.clipboard = pasted === null ? 'skipped' : verdict(pasted, want);
    // Boxes a mouse can hit: a bracket is a box 1 px wide at this size.
    // A run's signs can be narrow (a stack shares its column): take what is wide enough.
    const boxes = info.boxes.filter((b) => b.w >= 3 && b.h >= 6 && b.x >= 0 && b.y >= 0 && b.x + b.w < 1100 && b.y + b.h < 700);
    if (boxes.length >= 6) {
      const i = Math.floor(boxes.length * 0.25), j = Math.floor(boxes.length * 0.75);
      const at = (b) => [b.x + b.w / 2, b.y + b.h / 2];
      const [x0, y0] = at(boxes[i]), [x1, y1] = at(boxes[j]);
      await pg.mouse.move(x0, y0);
      await pg.mouse.down();
      await pg.mouse.move((x0 + x1) / 2, (y0 + y1) / 2, { steps: 4 });
      await pg.mouse.move(x1, y1, { steps: 4 });
      await pg.mouse.up();
      const drag = await pg.evaluate(() => String(getSelection()));
      const clean = drag.replace(/\n/g, '');
      const idx = want.indexOf(clean);
      const startOf = (b) => boxes.slice(0, boxes.indexOf(b)).reduce((s, q) => s + q.len, 0);
      // in code points of the source, as far as the boxes say (a box has its sign and the controls after it)
      rec.drag = clean.length === 0 ? 'EMPTY' : idx < 0 ? 'NOT A PART' : 'part';
      rec.dragLen = [...clean].length;
      rec.dragText = show(drag);
      rec.dragFrom = `${idx < 0 ? '?' : [...want.slice(0, idx)].length}/${[...want].length} (sign ${i} of ${boxes.length} is at ${startOf(boxes[i])})`;
      rec.dragLineBreaks = drag.length - clean.length;
      const [bx, by, bw, bh] = info.box;
      await pg.screenshot({ path: path.join(v.out, `${eng}-${f.name}-${k}.png`), clip: { x: Math.max(0, bx - 130), y: Math.max(0, by - 24), width: Math.min(1100 - Math.max(0, bx - 130), bw + 300), height: bh + 48 } });
    } else rec.drag = 'no boxes';
    results.push(rec);
    await pg.close();
  }
  await browser.close();
}
await srv.close();

if (!v.clipboard) console.log('the real clipboard check was SKIPPED (opt in with --clipboard; it replaces your clipboard)');
console.table(results.map((r) => ({ engine: r.engine, fixture: r.fixture, kind: r.kind, block: r.block, paragraph: r.paragraph, clipboard: r.clipboard, drag: r.drag, dragLen: r.dragLen, dragBreaks: r.dragLineBreaks, dragText: r.dragText, data: r.attribute })));
fs.writeFileSync(path.join(v.out, 'results.json'), JSON.stringify(results, null, 1));
// The wall is a line 19,000 px wide: a mouse drag on it selects nothing in Chromium at a device scale of 2, for the svg
// kind too (so it is the page, not the text); its copy is gated, its drag is not. tests/paths/gestures.mjs drags on lines.
const bad = results.filter((r) => r.kind === 'overlay' && (r.block !== 'exact' || r.paragraph !== 'exact' || (r.clipboard !== 'exact' && r.clipboard !== 'skipped') || r.attribute !== 'exact' || (r.drag !== 'part' && r.drag !== 'no boxes' && r.fixture !== 'wall') || r.dragLineBreaks));
if (bad.length) {
  console.error(`${bad.length} overlay results are not exact`);
  process.exit(1);
}
console.log(`ok: ${results.filter((r) => r.kind === 'overlay').length} overlay results exact; screenshots in ${v.out}`);
