#!/usr/bin/env node
// Contents blocks (`hieroxide render --contents`) inside an element the page already has, under the
// page's own CSS, in the three engines. Each fixture is a page whose `<span class="hierojax">` has
// the rules of one shape of real page, and whose span holds the contents block and nothing else:
//
//   upstream     docs/hierojax.css: an inline block, `position: relative`, a baseline of its own, and
//                the font named (`font-family: Hieroglyphic`, which the page serves)
//   upstream-blk the same with `svg { display: block }`
//   no-css       no rule for the span at all (an inline span in a paragraph)
//   box          a fixed box that centres the svg: `display: inline-flex; justify-content: center;
//                align-items: center; contain: layout; width; height` and `svg { display: block;
//                flex: none }`; bigger than the svg
//   box-tight    the same, smaller than the svg (it overflows both sides)
//   box-exact    the same, as big as the svg
//   rtl          `<html dir="rtl">` with the upstream rules
//   rtl-box      `<html dir="rtl">` with the fixed box
//
// and each with a line, a column (`vlr`), a line to the left (`hrl`), a font size of the block's own
// (`fontSize`), and, where the span has no box of its own, a separated line. Measured per page:
//   depth      the svg and the invisible text are children of the page's span: no element between
//   size       the svg is as many em as it says, at the font size it has (the page's or the option's)
//   overlay    the box of the invisible text against the box of the svg (px), and each sign's box
//              against the ink of the sign it is over
//   copy       what a selection of the span gives, of its paragraph, and of a mouse drag from a sign
//              near the start to one near the end, against the source; never the real clipboard
//   quiet      no CSP violation under `default-src 'self'; style-src 'self' 'unsafe-inline'`, no
//              console message, no request for the font the page names for the span
//              Where the svg is an inline box in a line (upstream, no-css, rtl), the text's static position is
//              the top of the line, which is above the svg's top when the line's strut is taller than the
//              svg: the vertical offset is measured and reported there (across and size are exact), not gated.
//   neutral    the page with the same block but no invisible text (`selectable: none`) has the same
//              boxes: the span, the svg and the paragraph, and the text after the span
// A control page whose invisible text has no `font-family` shows that the font check can fail.
//
//   cargo build --release -p hieroxide-cli
//   node tests/paths/host.mjs [--engines chromium,firefox,webkit] [--out tests/paths/out/host]
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { launch } from '../parity/lib/engines.mjs';
import { CSP, FONT, HERE, ROOT, loadCorpus, renderBatch, servePages } from './lib.mjs';

const { values: v } = parseArgs({
  options: { engines: { type: 'string', default: 'chromium,firefox,webkit' }, out: { type: 'string', default: path.join(HERE, 'out/host') }},
});
fs.mkdirSync(v.out, { recursive: true });

const corpus = loadCorpus();
const site = corpus.filter((f) => f.group === 'site').map((f) => f.text);
const LINE = site[2];
const COLUMN = site[0];
const UPSTREAM_CSS = fs.readFileSync(path.join(ROOT, 'docs/hierojax.css'), 'utf8');

const box = (w, h) => `.hierojax{font-family:Hieroglyphic;display:inline-flex;justify-content:center;align-items:center;vertical-align:baseline;contain:layout;white-space:nowrap;line-height:1;width:${w}em;height:${h}em}.hierojax svg{display:block;flex:none}`;
// `box(W, H, fs)`: the fixture's rules, given the em of the svg (W, H) and the font size it has.
const FIXTURES = [
  { name: 'upstream', css: () => UPSTREAM_CSS, separated: true, inlineSvg: true },
  { name: 'upstream-blk', css: () => `${UPSTREAM_CSS}.hierojax svg{display:block}` },
  { name: 'no-css', css: () => '', separated: true, inlineSvg: true },
  { name: 'box', css: (W, H, fs) => box(+(W * fs / 40 + 0.6).toFixed(3), +(H * fs / 40 + 0.4).toFixed(3)) },
  { name: 'box-tight', css: (W, H, fs) => box(+(W * fs / 40 - 0.15).toFixed(3), +(H * fs / 40 - 0.2).toFixed(3)) },
  { name: 'box-exact', css: (W, H, fs) => box(+(W * fs / 40).toFixed(4), +(H * fs / 40).toFixed(4)) },
  { name: 'rtl', css: () => UPSTREAM_CSS, dir: 'rtl', inlineSvg: true },
  { name: 'rtl-box', css: (W, H, fs) => box(+(W * fs / 40 + 0.6).toFixed(3), +(H * fs / 40 + 0.4).toFixed(3)), dir: 'rtl' },
];
const VARIANTS = [
  { name: 'line', text: LINE, o: {} },
  { name: 'column', text: COLUMN, o: { dir: 'vlr' } },
  { name: 'line-hrl', text: LINE, o: { dir: 'hrl' } },
  { name: 'fontsize', text: LINE, o: { fontSize: 30 } },
  { name: 'separated', text: LINE, o: { separated: true }, only: (f) => f.separated },
];

// What goes in the page: `before <span class="hierojax" ...>CONTENTS</span> after`.
const SPAN_ATTRS = 'data-type="svg" data-standalone="true"';
const pageOf = (f, css, contents) => `<!doctype html><html lang="en"${f.dir ? ` dir="${f.dir}"` : ''}><head><meta charset="utf-8"><title>host</title><style>body{margin:16px;font:40px/1.4 serif;color:#000}${css}</style></head>
<body><p id="p">before <span class="hierojax" ${SPAN_ATTRS}>${contents}</span> after</p></body></html>`;

const jobs = [];
for (const f of FIXTURES) for (const vr of VARIANTS) if (!vr.only || vr.only(f)) jobs.push({ f, vr });
const blocks = renderBatch(jobs.map((j) => ({ text: j.vr.text, ...j.vr.o, contents: true })));
const twins = renderBatch(jobs.map((j) => ({ text: j.vr.text, ...j.vr.o, contents: true, selectable: 'none' })));
const svgSize = (markup) => {
  const m = markup.match(/<svg[^>]* width="([\d.]+)em" height="([\d.]+)em"/);
  return m ? [Number(m[1]), Number(m[2])] : [1, 1];
};
const files = { '/NewGardiner.otf': fs.readFileSync(FONT) };
jobs.forEach((j, i) => {
  if (!blocks[i].ok || !twins[i].ok) throw new Error(`${j.f.name}/${j.vr.name}: ${blocks[i].message ?? twins[i].message}`);
  j.markup = blocks[i].svg;
  const [W, H] = svgSize(j.markup);
  const css = j.f.css(W, H, j.vr.o.fontSize ?? 40);
  files[`/${i}.html`] = pageOf(j.f, css, j.markup);
  files[`/${i}-twin.html`] = pageOf(j.f, css, twins[i].svg);
});
// The control: the same page with the generic font-family taken out of the invisible text.
const ctl = jobs.findIndex((j) => j.f.name === 'upstream-blk' && j.vr.name === 'line');
files['/control.html'] = pageOf(jobs[ctl].f, jobs[ctl].f.css(...svgSize(jobs[ctl].markup), 40), jobs[ctl].markup.replace(';font-family:sans-serif', ''));
const srv = await servePages(files, { headers: { 'Content-Security-Policy': CSP } });

const show = (s) => JSON.stringify(s.length > 24 ? s.slice(0, 12) + '…' : s).replace(/[\u{13000}-\u{1345f}︀-️]/gu, (c) => `<${c.codePointAt(0).toString(16)}>`);
const results = [];
let failures = 0;
const fail = (what) => {
  failures++;
  console.error(`FAIL ${what}`);
};

for (const eng of v.engines.split(',')) {
  const browser = await launch(eng);
  const ctx = await browser.newContext({ viewport: { width: 1100, height: 800 }, deviceScaleFactor: 2 });
  await ctx.addInitScript(() => {
    window.__csp = [];
    document.addEventListener('securitypolicyviolation', (e) => window.__csp.push(`${e.violatedDirective} ${e.blockedURI}`));
  });
  const open = async (url) => {
    const page = await ctx.newPage();
    const seen = { console: [], requests: [] };
    page.on('console', (m) => { if (['error', 'warning'].includes(m.type())) seen.console.push(m.text()); });
    page.on('pageerror', (e) => seen.console.push(String(e)));
    page.on('request', (r) => seen.requests.push(new URL(r.url()).pathname));
    await page.goto(`${srv.url}${url}`);
    await page.evaluate(() => document.fonts.ready);
    await page.waitForTimeout(120);
    return { page, seen };
  };
  // The geometry of a page, in one evaluation.
  const measure = (page) => page.evaluate(() => {
    const r = (e) => { const b = e.getBoundingClientRect(); return [b.x, b.y, b.width, b.height]; };
    const host = document.querySelector('.hierojax');
    const p = document.getElementById('p');
    const svgs = [...host.querySelectorAll('svg')];
    const overlays = [...host.querySelectorAll('[aria-hidden="true"]')];
    // The text after the span: a range over the last text node.
    const last = p.lastChild;
    const rg = document.createRange();
    rg.selectNodeContents(last);
    const after = rg.getBoundingClientRect();
    const sel = getSelection();
    sel.selectAllChildren(p);
    const para = String(sel);
    sel.removeAllRanges();
    return {
      para,
      host: r(host), p: r(p), after: [after.x, after.y, after.width, after.height], svgs: svgs.map(r),
      fontPx: svgs.map((s) => parseFloat(getComputedStyle(s).fontSize)),
      attrEm: svgs.map((s) => [parseFloat(s.getAttribute('width')), parseFloat(s.getAttribute('height'))]),
      overlays: overlays.map(r),
      direct: [...host.children].map((c) => c.tagName.toLowerCase() + (c.hasAttribute('aria-hidden') ? '[aria-hidden]' : '')),
      svgParentIsHost: svgs.map((s) => s.parentElement === host),
      hostClass: host.className, hostAttrs: [...host.attributes].map((a) => `${a.name}=${a.value}`).join(' '),
      dataText: host.querySelector('[data-text]')?.getAttribute('data-text') ?? null,
      vertical: getComputedStyle(host).verticalAlign,
    };
  });
  const maxDiff = (a, b) => Math.max(...a.map((x, i) => Math.abs(x - b[i])));

  // The control first: with no `font-family` on the invisible text, the page's font is asked for.
  {
    const { page, seen } = await open('/control.html');
    if (!seen.requests.includes('/NewGardiner.otf')) fail(`${eng}: the control page (no generic font-family) did not ask for the font, so the font check proves nothing`);
    await page.close();
  }

  for (let n = 0; n < jobs.length; n++) {
    const { f, vr } = jobs[n];
    const name = `${f.name}/${vr.name}`;
    const want = vr.text;
    const { page, seen } = await open(`/${n}.html`);
    const m = await measure(page);
    const twin = await open(`/${n}-twin.html`);
    const mt = await measure(twin.page);
    await twin.page.close();
    const rec = { engine: eng, fixture: f.name, variant: vr.name };

    // depth
    const sep = vr.o.separated === true;
    rec.children = m.direct.join(' ');
    if (sep) {
      if (!(m.direct.length === 1 && m.direct[0] === 'span')) fail(`${eng} ${name}: separated: expected one layout span in the host, got ${rec.children}`);
    } else if (!(m.direct.length === 2 && m.direct[0] === 'span[aria-hidden]' && m.direct[1] === 'svg' && m.svgParentIsHost.every(Boolean))) fail(`${eng} ${name}: expected [overlay, svg] as the host's children, got ${rec.children}`);
    if (m.hostClass !== 'hierojax' || m.hostAttrs !== `class=hierojax ${SPAN_ATTRS.replace(/"/g, '')}`) fail(`${eng} ${name}: the host's class or attributes changed: ${m.hostAttrs}`);
    if (m.dataText !== want) fail(`${eng} ${name}: data-text is ${show(m.dataText ?? 'null')}`);

    // size: the svg is its em at its font size
    const sizeBad = m.svgs.map((b, i) => Math.max(Math.abs(b[2] - m.attrEm[i][0] * m.fontPx[i]), Math.abs(b[3] - m.attrEm[i][1] * m.fontPx[i])));
    rec.sizeOff = Math.max(...sizeBad).toFixed(3);
    if (Math.max(...sizeBad) > 0.05) fail(`${eng} ${name}: an svg is not its size in em (off by ${rec.sizeOff}px; boxes ${JSON.stringify(m.svgs)})`);
    // The svg's size is the twin's.
    rec.svgVsTwin = Math.max(...m.svgs.map((b, i) => maxDiff(b.slice(2), mt.svgs[i].slice(2)))).toFixed(3);

    // overlay: the invisible text over the svg
    const ov = await page.evaluate((separated) => {
      const host = document.querySelector('.hierojax');
      const out = { deltas: [], boxes: 0, off: [] };
      const rect = (e) => e.getBoundingClientRect();
      for (const o of host.querySelectorAll('[aria-hidden="true"]')) {
        // Its svg: the next element in the host, or the previous one in the wrapper of a separated unit.
        const svg = o.nextElementSibling?.tagName.toLowerCase() === 'svg' ? o.nextElementSibling : o.previousElementSibling;
        const a = rect(o), b = rect(svg);
        out.deltas.push([a.x - b.x, a.y - b.y, a.width - b.width, a.height - b.height]);
        const inks = [...svg.querySelectorAll('path[transform]')].map(rect);
        for (const c of o.children) {
          out.boxes++;
          const q = rect(c);
          // A box has to hold the centre of some sign's ink, and be a box (not nothing).
          const hit = inks.some((k) => { const cx = k.x + k.width / 2, cy = k.y + k.height / 2; return cx >= q.x - 0.5 && cx <= q.right + 0.5 && cy >= q.y - 0.5 && cy <= q.bottom + 0.5; });
          if (!hit) out.off.push([q.x, q.y, q.width, q.height]);
        }
      }
      return out;
    }, sep);
    // Static position: the top of the line box. Where the svg is an inline box of the line, and the line is
    // taller above the baseline than the svg is (the strut of the font is), the svg's top is lower than
    // the line's and so is the offset of the text: measured and reported, not gated (see the note at
    // the top). Across, and in size, it is always exact.
    const worst = Math.max(0, ...ov.deltas.flat().map(Math.abs));
    const across = Math.max(0, ...ov.deltas.flatMap((d) => [d[0], d[2], d[3]]).map(Math.abs));
    const down = Math.max(0, ...ov.deltas.map((d) => Math.abs(d[1])));
    rec.overlayOff = (f.inlineSvg ? down : worst).toFixed(3);
    rec.boxes = ov.boxes;
    rec.boxesOffSign = ov.off.length;
    if ((f.inlineSvg ? across : worst) > 0.6) fail(`${eng} ${name}: the invisible text is off the svg by ${(f.inlineSvg ? across : worst).toFixed(2)}px (${JSON.stringify(ov.deltas)})`);
    if (ov.off.length && !(f.inlineSvg && down > 0.6)) fail(`${eng} ${name}: ${ov.off.length} of ${ov.boxes} boxes hold no sign's ink: ${JSON.stringify(ov.off.slice(0, 2))}`);

    // copy
    const info = await page.evaluate(() => {
      const host = document.querySelector('.hierojax');
      const sel = getSelection();
      sel.selectAllChildren(host);
      const block = String(sel);
      sel.selectAllChildren(document.getElementById('p'));
      const paragraph = String(sel);
      sel.removeAllRanges();
      const boxes = [...host.querySelectorAll('[aria-hidden="true"] > span')].map((e) => {
        const r = e.getBoundingClientRect();
        return { x: r.x, y: r.y, w: r.width, h: r.height };
      });
      return { block, paragraph, boxes };
    });
    // Firefox puts a line break at a block-level svg (a page's `svg { display: block }`), with or without
    // our text: the page with the same block and no text shows it (`mt.para`). Where the page alone breaks
    // the line, a line break where the selection meets the svg is the page's, and the text must be exact
    // besides it; where it does not, the text must be exact.
    const pageBreaks = mt.para.includes('\n');
    const squash = (s) => s.replace(/\s+/g, ' ').trim();
    const sameText = (got, exp) => (got === exp ? 'exact' : pageBreaks && squash(got) === squash(exp) ? "exact but for the page's block svg" : 'DIFFERENT');
    rec.block = sameText(info.block, want);
    rec.paragraph = sameText(info.paragraph, `before ${want} after`);
    rec.pageBreaks = pageBreaks;
    if (rec.block === 'DIFFERENT') fail(`${eng} ${name}: selecting the span gives ${show(info.block)}, not the source`);
    if (rec.paragraph === 'DIFFERENT') fail(`${eng} ${name}: selecting the paragraph gives ${show(info.paragraph)}`);
    const boxes = info.boxes.filter((b) => b.w >= 6 && b.h >= 6 && b.x >= 0 && b.y >= 0 && b.x + b.w < 1100 && b.y + b.h < 800);
    if (boxes.length >= 3) {
      const i = Math.floor(boxes.length * 0.25), j = Math.floor(boxes.length * 0.75);
      const at = (b) => [b.x + b.w / 2, b.y + b.h / 2];
      const [x0, y0] = at(boxes[i]), [x1, y1] = at(boxes[j]);
      await page.mouse.move(x0, y0);
      await page.mouse.down();
      await page.mouse.move((x0 + x1) / 2, (y0 + y1) / 2, { steps: 4 });
      await page.mouse.move(x1, y1, { steps: 4 });
      await page.mouse.up();
      const drag = await page.evaluate(() => String(getSelection()));
      rec.drag = drag.length === 0 ? 'EMPTY' : drag.includes('\n') ? 'LINE BREAKS' : want.includes(drag) ? 'part' : 'NOT A PART';
      rec.dragLen = [...drag].length;
      if (rec.drag !== 'part') fail(`${eng} ${name}: a drag from sign ${i} to ${j} selects ${show(drag)} (${rec.drag})`);
    } else rec.drag = 'no boxes';

    // quiet
    const csp = await page.evaluate(() => window.__csp);
    rec.csp = csp.length;
    rec.console = seen.console.length;
    // A page that names the font for an element with an inline svg in it may ask for it by itself (the
    // line's strut is in that font): what counts is that the invisible text adds no request.
    rec.fontRequests = seen.requests.filter((u) => u.endsWith('.otf')).length;
    rec.fontRequestsWithout = twin.seen.requests.filter((u) => u.endsWith('.otf')).length;
    if (csp.length) fail(`${eng} ${name}: CSP violations: ${csp.join('; ')}`);
    if (seen.console.length) fail(`${eng} ${name}: console: ${seen.console.join('; ')}`);
    if (rec.fontRequests > rec.fontRequestsWithout) fail(`${eng} ${name}: the invisible text made the page ask for the font`);

    // neutral: the invisible text moves nothing
    const nb = [['host', m.host, mt.host], ['p', m.p, mt.p], ['after', m.after, mt.after], ...m.svgs.map((b, i) => [`svg${i}`, b, mt.svgs[i]])];
    rec.shift = Math.max(...nb.map(([, a, b]) => maxDiff(a, b))).toFixed(3);
    if (Number(rec.shift) > 0.01) fail(`${eng} ${name}: the invisible text moves things: ${nb.filter(([, a, b]) => maxDiff(a, b) > 0.01).map(([k, a, b]) => `${k} ${JSON.stringify(a)} vs ${JSON.stringify(b)}`).join('; ')}`);
    rec.verticalAlign = m.vertical;

    if (vr.name === 'line') await page.screenshot({ path: path.join(v.out, `${eng}-${f.name}-${vr.name}.png`), clip: { x: 0, y: 0, width: 700, height: 140 } });
    results.push(rec);
    await page.close();
  }
  await browser.close();
}
await srv.close();

fs.writeFileSync(path.join(v.out, 'results.json'), JSON.stringify(results, null, 1));
console.table(results.map((r) => ({ engine: r.engine, page: `${r.fixture}/${r.variant}`, children: r.children.replace(/span\[aria-hidden\]/g, 'ov'), 'svg-ov px': r.overlayOff, 'box off sign': r.boxesOffSign, block: r.block, para: r.paragraph, drag: r.drag, shift: r.shift, size: r.sizeOff, font: `${r.fontRequests}/${r.fontRequestsWithout}`, csp: r.csp })));
if (failures) {
  console.error(`${failures} failures`);
  process.exit(1);
}
console.log(`ok: ${results.length} pages (${jobs.length} per engine) with the invisible text over the svg, exact copies, no font request, no CSP violation, nothing moved`);
