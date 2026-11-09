#!/usr/bin/env node
// Selecting a path block with the mouse, in real browsers, against upstream's script as the baseline.
//
//   cargo build --release -p hieroxide-cli
//   node tests/paths/gestures.mjs [--engines chromium,firefox,webkit] [--fixtures short,mid,long,long-hrl]
//        [--shapes upstream:text,upstream:alone,nocss:text,box:alone,rtl:text] [--out tests/paths/out/gestures]
//        [--site DIR] [--headed] [--min-font PX] [--no-gate]
//
// Every page has a paragraph above the block, and the block in a paragraph, alone or between `before` and
// `after`. Per page and engine, for `upstream` (docs/hierojax.js drawing the source text, with the page's
// CSS) and `path` (`hieroxide render --contents` filled in), real mouse input (`page.mouse`, `clickCount`
// for double and triple clicks) and what `getSelection().toString()` gives against the source:
//   click      one click on each of 5 signs: W whole block, P a part, N nothing (`W0P0N5` is five clicks
//              that select nothing)
//   dbl, tri   the same for double and triple clicks
//   out, outR  a drag from the left (right) margin of the page, level with the block, to just past the
//              block's far end (and back)
//   f2l        a drag from the middle of the first sign to the middle of the last
//   part       a drag from the middle of the sign a quarter in to the one three quarters in
//   above      a drag from the paragraph above to the block's far end; aboveMid to its middle
//   all        select-all (the keyboard, not the clipboard)
// A cell is `all` (the whole source), `n@k` (n code points of the source, contiguous, from the k-th),
// `0` (nothing), `?n` (n code points that are not a contiguous part of the source), and `+T` if text of the
// page is taken in as well, `+n` if line breaks are.
// Then the selected state is photographed and compared with the unselected one (`ghost`): stray ink is dark
// pixels in the selected picture that are not near dark pixels of the unselected one, and holes in the
// highlight; `lift` is how much the drawing's ink is lightened; the highlight's box and its pixel count are
// reported. Playwright's WebKit paints no selection in a screenshot, so there it cannot be seen (the report
// says `no highlight painted`). Pictures of every selected state go to --out.
//
// Gates (the path block, all pages): select-all is exactly the source; triple-click selects the whole
// block in the three engines, double-click in Chromium and Firefox (WebKit does not take a word at a tiny
// glyph: reported); a drag across part of a line (but not, in WebKit, the 8-sign one) and from first to last sign is a contiguous part of the
// source, never nothing; in Chromium and Firefox the stray ink in the selected picture is under 1.5% of the
// drawing's. Everything else is reported against the baseline and not gated, because the engines
// differ in it (see tests/README.md). With --site DIR: DIR/ref.html and DIR/filled.html of a real page, served as
// they are (a one-off check of a real page's shape; it is nothing the repository holds).
// The clipboard is never used. Needs tests/parity's install and the CLI.
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { ENGINES, launch } from '../parity/lib/engines.mjs';
import { FONT, HERE, ROOT, compareSelected as compare, loadCorpus, renderBatch, servePages } from './lib.mjs';

const { values: v } = parseArgs({
  options: {
    engines: { type: 'string', default: 'chromium,firefox,webkit' },
    fixtures: { type: 'string', default: 'short,mid,long,long-hrl' },
    shapes: { type: 'string', default: 'upstream:text,upstream:alone,nocss:text,box:alone,rtl:text' },
    out: { type: 'string', default: path.join(HERE, 'out/gestures') },
    site: { type: 'string', default: '' },
    headed: { type: 'boolean', default: false },
    'min-font': { type: 'string', default: '0' },
    'no-gate': { type: 'boolean', default: false },
  },
});
fs.mkdirSync(v.out, { recursive: true });

const corpus = loadCorpus();
const site = corpus.filter((f) => f.group === 'site').map((f) => f.text);
const FIX = {
  short: { text: site[0], o: {} },
  mid: { text: [...site[2]].slice(0, 16).join(''), o: {} },
  long: { text: site[2], o: {} },
  'long-hrl': { text: site[2], o: { dir: 'hrl' } },
};
const UPSTREAM_CSS = fs.readFileSync(path.join(ROOT, 'docs/hierojax.css'), 'utf8');
const FONTFACE = "@font-face{font-family:Hieroglyphic;src:url('NewGardiner.otf') format('opentype')}";
const box = (w, h) => `${FONTFACE}.hierojax{font-family:Hieroglyphic;display:inline-flex;justify-content:center;align-items:center;vertical-align:baseline;contain:layout;white-space:nowrap;line-height:1;width:${w}em;height:${h}em}.hierojax svg{display:block;flex:none}`;
const SHAPES = {
  upstream: { css: () => UPSTREAM_CSS },
  nocss: { css: () => FONTFACE },
  box: { css: (W, H) => box(+(W + 0.6).toFixed(3), +(H + 0.4).toFixed(3)) },
  rtl: { css: () => UPSTREAM_CSS, dir: 'rtl' },
};
const ABOVE = 'Some words above the line.';
const pageOf = (shape, variant, inner, script = '') => {
  const css = SHAPES[shape].css(inner.W ?? 3, inner.H ?? 1);
  const dir = SHAPES[shape].dir;
  const blk = `<span class="hierojax" data-type="svg" data-standalone="true"${inner.dir ? ` data-dir="${inner.dir}"` : ''}>${inner.html}</span>`;
  const p = variant === 'alone' ? blk : `before ${blk} after`;
  return `<!doctype html><html lang="en"${dir ? ` dir="${dir}"` : ''}><head><meta charset="utf-8"><title>g</title><style>body{margin:16px;font:40px/1.4 serif;color:#000;background:#fff}${css}</style></head>
<body><p id="above">${ABOVE}</p><p id="p">${p}</p><p id="below">Below the line.</p>${script}</body></html>`;
};

// ---- pages ----------------------------------------------------------------------------------
const fixtures = v.fixtures === 'none' ? [] : v.fixtures.split(',');
const shapes = v.shapes.split(',').map((s) => s.split(':'));
const rendered = !fixtures.length ? [] : renderBatch(fixtures.map((f) => ({ text: FIX[f].text, contents: true, ...FIX[f].o })));
const files = { '/NewGardiner.otf': fs.readFileSync(FONT), '/hierojax.js': fs.readFileSync(path.join(ROOT, 'docs/hierojax.js')) };
const pages = [];
fixtures.forEach((fx, i) => {
  if (!rendered[i].ok) throw new Error(`${fx}: ${rendered[i].message}`);
  const html = rendered[i].svg;
  const W = +html.match(/<svg width="([\d.]+)em" height="([\d.]+)em"/)[1], H = +html.match(/<svg width="([\d.]+)em" height="([\d.]+)em"/)[2];
  for (const [shape, variant] of shapes) {
    const base = `/${fx}-${shape}-${variant}`;
    files[`${base}-upstream.html`] = pageOf(shape, variant, { html: FIX[fx].text, W, H, dir: FIX[fx].o.dir }, '<script src="/hierojax.js"></script><script>addEventListener("DOMContentLoaded",()=>hierojax.processFragments())</script>');
    files[`${base}-path.html`] = pageOf(shape, variant, { html, W, H });
    for (const kind of ['upstream', 'path']) pages.push({ id: `${base}-${kind}.html`, name: `${fx} ${shape}:${variant}`, kind, want: FIX[fx].text, blockSel: '.hierojax', aboveSel: '#above' });
  }
});
let srv;
if (v.site) {
  // A real page: served as it is; both files, two blocks each.
  const dir = path.resolve(v.site);
  const serve = {};
  for (const f of fs.readdirSync(dir)) serve[`/${f}`] = fs.readFileSync(path.join(dir, f));
  Object.assign(files, serve);
  for (const [kind, file] of [['upstream', 'ref.html'], ['path', 'filled.html']]) {
    pages.push({ id: `/${file}`, name: 'site title', kind, want: site[0], blockSel: 'h1 .hierojax', aboveSel: null, site: true });
    pages.push({ id: `/${file}`, name: 'site amduat', kind, want: site[2], blockSel: '.amduat .hierojax', aboveSel: 'blockquote p', site: true });
  }
}
srv = await servePages(files);

// ---- measuring ------------------------------------------------------------------------------
const cps = (s) => [...s];
function relate(s, want) {
  const alpha = new Set(cps(want));
  const core = cps(s).filter((c) => alpha.has(c)).join('');
  const letters = /[A-Za-z]/.test(s);
  const nl = /\n/.test(s);
  let cell;
  if (core === '') cell = '0';
  else if (core === want) cell = 'all';
  else {
    const i = want.indexOf(core);
    cell = i < 0 ? `?${cps(core).length}` : `${cps(core).length}@${cps(want.slice(0, i)).length}`;
  }
  return cell + (letters ? '+T' : '') + (nl ? '+nl' : '');
}
const cat = (r) => (r.startsWith('all') ? 'W' : r.startsWith('0') ? 'N' : 'P');

const results = [];
for (const eng of v.engines.split(',')) {
  // --min-font N: the browser is told never to draw text under N px (Chromium and Firefox; see minfont.mjs).
  const min = Number(v['min-font']);
  const opts = { headless: !v.headed };
  if (min && eng === 'chromium') opts.args = [`--blink-settings=minimumFontSize=${min},minimumLogicalFontSize=${min}`];
  if (min && eng === 'firefox') opts.firefoxUserPrefs = Object.fromEntries(['x-western', 'x-unicode', 'x-cyrillic', 'x-central-euro', 'el', 'he', 'ar', 'ja', 'zh-CN', 'zh-TW', 'ko', 'x-math', 'x-symbol'].map((g) => [`font.minimum-size.${g}`, min]));
  const browser = await ENGINES[eng].launch(opts);
  const ctx = await browser.newContext({ viewport: { width: 1100, height: 700 }, deviceScaleFactor: 2 });
  // Where the signs are, as shares of the svg's box, from the path block (the same drawing in both).
  const frs = new Map();
  for (const pg of [...pages].sort((a, b) => (a.kind === 'path' ? -1 : 1) - (b.kind === 'path' ? -1 : 1))) {
    const page = await ctx.newPage();
    await page.goto(srv.url + pg.id);
    await page.bringToFront();
    if (pg.kind === 'upstream') {
      await page.waitForFunction((sel) => document.querySelector(`${sel} svg`) && (typeof hierojax === 'undefined' || hierojax.nFontsLoaded == hierojax.nFonts), pg.blockSel, { timeout: 20000 }).catch(() => {});
      await page.waitForTimeout(400);
    } else {
      await page.evaluate(() => document.fonts.ready);
      await page.waitForTimeout(150);
    }
    const key = `${pg.name}`;
    const geo = await page.evaluate(({ blockSel, aboveSel }) => {
      const r = (e) => { const b = e.getBoundingClientRect(); return { x: b.x, y: b.y, w: b.width, h: b.height }; };
      const host = document.querySelector(blockSel);
      host.scrollIntoView({ block: 'center' });
      const svg = host.querySelector('svg');
      const s = r(svg);
      const paths = [...svg.querySelectorAll('path[transform]')].map((p) => { const b = r(p); return [(b.x + b.w / 2 - s.x) / s.w, (b.y + b.h / 2 - s.y) / s.h]; });
      const ab = aboveSel ? document.querySelector(aboveSel) : null;
      return { svg: s, paths, above: ab ? r(ab) : null, scrollY: scrollY };
    }, pg);
    if (pg.kind === 'path') frs.set(key, geo.paths);
    const fr = frs.get(key) ?? geo.paths;
    const S = geo.svg;
    const at = (f) => [S.x + f[0] * S.w, S.y + f[1] * S.h];
    const n = fr.length;
    const want = pg.want;
    const sel = () => page.evaluate(() => String(getSelection()));
    const clear = async () => { await page.evaluate(() => getSelection().removeAllRanges()); await page.mouse.click(2, 690); await page.evaluate(() => getSelection().removeAllRanges()); };
    const drag = async (x0, y0, x1, y1) => {
      await page.mouse.move(x0, y0);
      await page.mouse.down();
      await page.mouse.move((x0 + x1) / 2, (y0 + y1) / 2, { steps: 6 });
      await page.mouse.move(x1, y1, { steps: 6 });
      await page.mouse.up();
    };
    const res = { engine: eng, page: pg.name, kind: pg.kind, g: {} };
    const clip = { x: Math.max(0, S.x - 20), y: Math.max(0, S.y - 12), width: Math.min(1000, S.w + 40), height: S.h + 24 };
    const U = await page.screenshot({ clip, type: 'png' });
    const cy = S.y + S.h / 2;
    const pts = [0, 1, 2, 3, 4].map((k) => at(fr[Math.min(n - 1, Math.floor((n * (k + 0.5)) / 5))]));
    const multi = async (name, f) => { const c = { W: 0, P: 0, N: 0 }; for (const [x, y] of pts) { await clear(); await f(x, y); c[cat(relate(await sel(), want))]++; } res.g[name] = `W${c.W}P${c.P}N${c.N}`; };
    const take = async (name, f) => { await clear(); await f(); res.g[name] = relate(await sel(), want); };
    await multi('click', async (x, y) => { await page.mouse.click(x, y); });
    await multi('dbl', async (x, y) => { await page.mouse.move(x, y); for (const k of [1, 2]) { await page.mouse.down({ clickCount: k }); await page.mouse.up({ clickCount: k }); } });
    await multi('tri', async (x, y) => { await page.mouse.move(x, y); for (const k of [1, 2, 3]) { await page.mouse.down({ clickCount: k }); await page.mouse.up({ clickCount: k }); } });
    await take('out', async () => { await drag(4, cy, S.x + S.w + 6, cy); });
    await take('outR', async () => { await drag(1090, cy, S.x - 6, cy); });
    const [a0, b0] = [at(fr[0]), at(fr[n - 1])];
    await take('f2l', async () => { await drag(a0[0], a0[1], b0[0], b0[1]); });
    const [q0, q1] = [at(fr[Math.floor(n / 4)]), at(fr[Math.floor((3 * n) / 4)])];
    await take('part', async () => { await drag(q0[0], q0[1], q1[0], q1[1]); });
    if (geo.above) {
      const ab = geo.above;
      await take('above', async () => { await drag(ab.x + Math.min(150, ab.w / 2), ab.y + ab.h / 2, S.x + S.w + 6, cy); });
      await take('aboveMid', async () => { await drag(ab.x + Math.min(150, ab.w / 2), ab.y + ab.h / 2, S.x + S.w / 2, cy); });
    }
    await clear();
    await page.keyboard.press('ControlOrMeta+a');
    res.g.all = relate(await sel(), want);
    const Sel = await page.screenshot({ clip, type: 'png' });
    res.ghost = await compare(ctx, U, Sel);
    const slug = `${eng}-${pg.name.replace(/[ :]/g, '_')}-${pg.kind}`;
    fs.writeFileSync(path.join(v.out, `${slug}-unselected.png`), U);
    fs.writeFileSync(path.join(v.out, `${slug}-selected.png`), Sel);
    results.push(res);
    await page.close();
  }
  await browser.close();
}
await srv.close();

fs.writeFileSync(path.join(v.out, 'results.json'), JSON.stringify(results, null, 1));
const G = ['click', 'dbl', 'tri', 'out', 'outR', 'f2l', 'part', 'above', 'aboveMid', 'all'];
for (const eng of v.engines.split(',')) {
  console.log(`\n== ${eng}`);
  console.table(results.filter((r) => r.engine === eng).map((r) => ({ page: r.page, kind: r.kind, ...Object.fromEntries(G.map((g) => [g, r.g[g] ?? '-'])) })));
  console.table(results.filter((r) => r.engine === eng).map((r) => ({ page: r.page, kind: r.kind, 'stray ink %': r.ghost.ghostPct, 'ink lift': Math.round(r.ghost.lift), 'highlight px': r.ghost.hl, 'highlight box (x y w h)': (r.ghost.hlBox ?? []).join(' ') || 'no highlight painted' })));
}

// ---- gates ----------------------------------------------------------------------------------
const bad = [];
const contiguous = (c) => /^\d+@\d+(\+T)?(\+nl)?$/.test(c) || c.startsWith('all');
for (const r of results.filter((r) => r.kind === 'path' && !r.page.startsWith('site'))) {
  const where = `${r.engine} ${r.page}`;
  if (!r.g.all.startsWith('all')) bad.push(`${where}: select-all is ${r.g.all}`);
  if (r.g.tri !== 'W5P0N0') bad.push(`${where}: triple-click is ${r.g.tri}`);
  if (r.engine !== 'webkit' && !/^W[45]P/.test(r.g.dbl) && r.g.dbl !== 'W5P0N0') bad.push(`${where}: double-click is ${r.g.dbl}`);
  // WebKit takes nothing for a drag across part of the 8-sign line (it does on lines of 16 and more): reported, not gated.
  for (const g of ['f2l', 'part']) if ((!contiguous(r.g[g]) || r.g[g].startsWith('0')) && !(r.engine === 'webkit' && g === 'part' && r.page.startsWith('short'))) bad.push(`${where}: ${g} is ${r.g[g]}`);
  for (const g of ['out', 'outR', 'above', 'aboveMid']) if (r.g[g] && r.g[g].startsWith('?')) bad.push(`${where}: ${g} is ${r.g[g]}, not a part of the source`);
  if (r.engine !== 'webkit' && r.ghost.ghostPct > 1.5) bad.push(`${where}: stray ink in the selected picture is ${r.ghost.ghostPct}%`);
  if (r.engine !== 'webkit' && r.ghost.hl === 0) bad.push(`${where}: no highlight`);
}
if (bad.length && !v['no-gate']) {
  console.error(bad.join('\n'));
  console.error(`${bad.length} failures`);
  process.exit(1);
}
console.log(`\nok: ${results.length} pages; pictures and results.json in ${v.out}${bad.length ? ` (${bad.length} gate failures ignored)` : ''}`);
