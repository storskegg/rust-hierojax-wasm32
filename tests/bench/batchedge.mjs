#!/usr/bin/env node
// Edge cases for the batching lever that the corpus has no fragments for, compared between
// upstream (one element at a time) and a replacement (`--impl`, batched). Prints what differs,
// per engine; exit 1 on a difference.
//
//   node batchedge.mjs --impl ../../build/compat [--engines chromium,firefox,webkit]
//
// The edge page: a second processFragments() over output already rendered, text in a
// display:none box, an element that holds another `hierojax` element (with hatched shading, and
// the inner element is also looked at after the outer one has cut it out of the document), shade
// patterns repeated on the page, and a parse error.
//
// All of that is run for each `data-type` (svg, dom, canvas), and the canvases' pixels are compared too.
//
// The throwing pages: a fragment that throws in the middle of the page, one page per kind of
// exception (the canvas's IndexSizeError, upstream's own TypeError and RangeError), with hatched
// shading before and after it. Upstream's forEach stops at the exception, so what is after it
// stays unprocessed, as text. Each is run two ways: `processFragmentsNow()` called directly,
// twice, comparing the exception that comes out and the DOM after each call; and the real way,
// `processFragments()` while the font is still loading, comparing what reaches the window
// (`error` or `unhandledrejection`) and the DOM.
import { parseArgs } from 'node:util';
import { launch } from '../parity/lib/engines.mjs';
import { startServer } from '../parity/lib/server.mjs';
import { CANON_SRC } from '../parity/lib/canon.mjs';

const { values: v } = parseArgs({ options: { impl: { type: 'string', default: '../../build/compat' }, engines: { type: 'string', default: 'chromium,firefox,webkit' } } });

const A1 = '\u{13000}', A2 = '\u{13001}', N5 = '\u{131F3}', Z1 = '\u{133E4}';
const HOR = '\u{13431}', VER = '\u{13430}';
const DAMAGED = '\u{13447}', ALL = '\u{13455}';
const head = (extra = '') => `<!DOCTYPE html><html><head><meta charset="utf-8"><link rel="stylesheet" href="hierojax.css">${extra}<script src="hierojax.js"></script></head><body>`;
const hatched = (t, attrs = '') => `<span class="hierojax" data-shadepattern="diagonal"${attrs}>${t}</span>`;

const EDGE = `${head()}
<div id="a" style="font-size:30px"><span class="hierojax">${A1}${HOR}${A2}${HOR}${N5}${VER}${Z1}</span></div>
<div id="b" style="display:none"><span class="hierojax">${A1}${HOR}${A2}</span></div>
<div id="c" style="font-size:25px"><span class="hierojax" id="outer">${A1}${HOR}<span class="hierojax" id="inner">${N5}${HOR}${Z1}</span>${HOR}${A2}</span></div>
<div id="d">${hatched(N5 + DAMAGED + HOR + N5 + HOR + A1)} ${hatched(N5 + ALL + HOR + N5)} ${hatched(A1 + DAMAGED)}</div>
<div id="c2" style="font-size:25px;color:blue">${hatched(`${A1}${DAMAGED}${HOR}<span class="hierojax" data-shadepattern="diagonal" id="inner2">${N5}${ALL}</span>`, ' id="outer2"')}</div>
<div id="d2">${hatched(A1 + ALL + HOR + A1 + ALL, ' data-separated="true"')}</div>
<div id="e"><span class="hierojax">${A1}(((</span></div>
<div id="f" style="visibility:hidden"><span class="hierojax">${A1}${HOR}${N5}</span></div>
</body></html>`;

// What throws, from the stress corpus.
const THROWERS = {
  canvas: `<span class="hierojax">\u{13000}\u{13432}\u{1343C}\u{1343D}</span>`,
  type: `<span class="hierojax">\u{13351}\u{13436}\u{133F2}\u{13432}\u{13000}\u{13433}\u{13000}\u{13434}\u{13000}\u{13435}\u{13000}</span>`,
  range: `<span class="hierojax" data-sep="-0.1">\u{130F1}\u{13434}\u{133CF}</span>`,
};
const listen = `<script>window.__events = [];
addEventListener('error', (e) => { __events.push('error ' + (e.error && e.error.name) + ': ' + (e.error && e.error.message)); e.preventDefault(); });
addEventListener('unhandledrejection', (e) => { __events.push('rejection ' + (e.reason && e.reason.name) + ': ' + (e.reason && e.reason.message)); e.preventDefault(); });
</script>`;
const thrower = (t, real) => `${head(listen + (real ? `<script>addEventListener('DOMContentLoaded', () => hierojax.processFragments());</script>` : ''))}
<div>${hatched(A1 + DAMAGED)}</div>
<div>${hatched(N5 + ALL + HOR + A1)}</div>
<div>${t}</div>
<div>${hatched(A2 + DAMAGED)}</div>
<div>${t}</div>
<div><span class="hierojax">${N5}</span></div>
</body></html>`;

// The same pages with every fragment of the given type (the default, svg, is the plain page).
const TYPES = { '': null, '-dom': 'dom', '-canvas': 'canvas' };
const typed = (html, type) => (type ? html.replaceAll('class="hierojax"', `class="hierojax" data-type="${type}"`) : html);
const pages = {};
for (const [sfx, type] of Object.entries(TYPES)) {
  pages[`/edge${sfx}.html`] = typed(EDGE, type);
  for (const [k, t] of Object.entries(THROWERS)) {
    pages[`/now-${k}${sfx}.html`] = typed(thrower(t, false), type);
    pages[`/real-${k}${sfx}.html`] = typed(thrower(t, true), type);
  }
}

async function run(engine, impl) {
  const server = await startServer({ impl, pages });
  const browser = await launch(engine);
  const res = {};
  const open = async (name) => {
    const page = await (await browser.newContext({ viewport: { width: 1280, height: 800 } })).newPage();
    const log = [];
    page.on('console', (m) => log.push(m.text().slice(0, 120)));
    await page.goto(server.url + name, { waitUntil: 'load' });
    return { page, log };
  };
  const SNAP = `(src) => {
    window.__canon = (0, eval)('(' + src + ')');
    const pixels = (c) => {
      try {
        const d = c.getContext('2d').getImageData(0, 0, c.width, c.height).data;
        let h = 0;
        for (let i = 0; i < d.length; i++) h = (Math.imul(h, 31) + d[i]) | 0;
        return c.width + 'x' + c.height + ':' + h;
      } catch (e) {
        return c.width + 'x' + c.height + ':' + e.name;
      }
    };
    window.__snap = () => [...document.querySelectorAll('body > div')].map((d) => __canon(d)).concat([...document.querySelectorAll('canvas')].map(pixels));
    window.__try = () => { try { hierojax.processFragmentsNow(); return null; } catch (e) { return e.name + ': ' + e.message; } };
  }`;
  for (const sfx of Object.keys(TYPES)) {
    {
      const { page, log } = await open(`/edge${sfx}.html`);
      await page.waitForFunction(() => hierojax.nFontsLoaded == hierojax.nFonts);
      await page.evaluate(`(${SNAP})(${JSON.stringify(CANON_SRC)})`);
      res[`edge${sfx}`] = await page.evaluate(() => {
        const inner = [document.getElementById('inner'), document.getElementById('inner2')];
        const out = {};
        out.err1 = __try();
        out.first = __snap();
        // The elements the outer ones cut out of the document, with what was written into them.
        out.cut = inner.map((e) => __canon(e));
        out.err2 = __try();
        out.second = __snap();
        return out;
      });
      res[`edge${sfx}`].logs = log.length;
      await page.context().close();
    }
    for (const k of Object.keys(THROWERS)) {
      {
        const { page } = await open(`/now-${k}${sfx}.html`);
        await page.waitForFunction(() => hierojax.nFontsLoaded == hierojax.nFonts);
        await page.evaluate(`(${SNAP})(${JSON.stringify(CANON_SRC)})`);
        res[`now-${k}${sfx}`] = await page.evaluate(() => {
          const out = {};
          out.err1 = __try();
          out.first = __snap();
          out.err2 = __try();
          out.second = __snap();
          out.err3 = __try();
          out.third = __snap();
          out.events = __events.slice();
          return out;
        });
        await page.context().close();
      }
      {
        const { page } = await open(`/real-${k}${sfx}.html`);
        await page.waitForFunction(() => window.__events.length > 0, null, { timeout: 40000 });
        await page.evaluate(`(${SNAP})(${JSON.stringify(CANON_SRC)})`);
        await page.waitForTimeout(100);
        res[`real-${k}${sfx}`] = await page.evaluate(() => ({ events: __events.slice(), dom: __snap() }));
        await page.context().close();
      }
    }

  }
  await browser.close();
  await server.close();
  return res;
}

let bad = 0;
for (const engine of v.engines.split(',')) {
  const a = await run(engine, 'upstream');
  const b = await run(engine, v.impl);
  let compared = 0;
  for (const name of Object.keys(a)) {
    for (const key of Object.keys(a[name])) {
      compared++;
      const x = JSON.stringify(a[name][key]), y = JSON.stringify(b[name][key]);
      if (x !== y) {
        bad++;
        let i = 0;
        while (i < x.length && x[i] === y[i]) i++;
        console.log(`${engine} ${name}.${key}: differs at ${i}\n  upstream:    ${x.slice(Math.max(0, i - 60), i + 200)}\n  replacement: ${y.slice(Math.max(0, i - 60), i + 200)}`);
      }
    }
  }
  const ex = Object.keys(THROWERS).map((k) => `${k}: ${a[`real-${k}`].events[0]}`).join(' | ');
  const dom = a['now-canvas'].first.map((d) => (/<svg/.test(d) ? 'done' : /\u{13000}|\u{131F3}|\u{13001}/u.test(d) ? 'text' : 'empty')).join(',');
  console.log(`${engine}: ${compared} comparisons. Upstream's: ${ex}. DOM after the first exception: ${dom}`);
}
console.log(bad ? `${bad} differences` : 'no differences');
process.exit(bad ? 1 : 0);
