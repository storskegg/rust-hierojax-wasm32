#!/usr/bin/env node
// Edge cases for the batching lever that the corpus has no fragments for, compared between
// upstream (one element at a time) and a replacement (`--impl`, batched): a second
// processFragments() over output already rendered, text in a display:none box, an element that
// holds another `hierojax` element, a fragment with a shade pattern repeated on the page, and
// one with a parse error. Prints what differs, per engine.
//
//   node batchedge.mjs --impl ../../build/variants/all [--engines chromium,firefox,webkit]
import { parseArgs } from 'node:util';
import { launch } from '../parity/lib/engines.mjs';
import { startServer } from '../parity/lib/server.mjs';
import { CANON_SRC } from '../parity/lib/canon.mjs';

const { values: v } = parseArgs({ options: { impl: { type: 'string' }, engines: { type: 'string', default: 'chromium,firefox,webkit' } } });
const shaded = (t) => `<span class="hierojax" data-shadepattern="diagonal">${t}</span>`;
const HTML = `<!DOCTYPE html><html><head><meta charset="utf-8"><link rel="stylesheet" href="hierojax.css"><script src="hierojax.js"></script></head><body>
<div id="a" style="font-size:30px"><span class="hierojax">A1-A2-N5:Z1</span></div>
<div id="b" style="display:none"><span class="hierojax">A1-A2</span></div>
<div id="c" style="font-size:25px"><span class="hierojax" id="outer">A1-<span class="hierojax" id="inner">N5-Z1</span>-A2</span></div>
<div id="d">${shaded('N5-N5-A1')} ${shaded('N5-N5')} ${shaded('A1')}</div>
<div id="e"><span class="hierojax">A1-(((</span></div>
<div id="f" style="visibility:hidden"><span class="hierojax">A1-N5</span></div>
</body></html>`;

async function run(engine, impl) {
  const server = await startServer({ impl, pages: { '/edge.html': HTML } });
  const browser = await launch(engine);
  const page = await (await browser.newContext({ viewport: { width: 1280, height: 800 } })).newPage();
  const log = [];
  page.on('console', (m) => log.push(m.text().slice(0, 120)));
  await page.goto(server.url + '/edge.html', { waitUntil: 'load' });
  await page.waitForFunction(() => hierojax.nFontsLoaded == hierojax.nFonts);
  const out = await page.evaluate(async (src) => {
    const canon = (0, eval)('(' + src + ')');
    const snap = () => [...document.querySelectorAll('body > div')].map((d) => canon(d));
    const res = {};
    hierojax.processFragmentsNow();
    res.first = snap();
    hierojax.processFragmentsNow();
    res.second = snap();
    return res;
  }, CANON_SRC);
  await browser.close();
  await server.close();
  return { out, log };
}

let bad = 0;
for (const engine of v.engines.split(',')) {
  const a = await run(engine, 'upstream');
  const b = await run(engine, v.impl);
  for (const pass of ['first', 'second']) {
    a.out[pass].forEach((x, i) => {
      if (JSON.stringify(x) !== JSON.stringify(b.out[pass][i])) {
        bad++;
        console.log(`${engine} ${pass} pass, div #${i}: differs\n  upstream: ${JSON.stringify(x).slice(0, 300)}\n  replacement: ${JSON.stringify(b.out[pass][i]).slice(0, 300)}`);
      }
    });
  }
  console.log(`${engine}: ${a.out.first.length * 2} snapshots compared; console upstream ${a.log.length}, replacement ${b.log.length}`);
}
process.exit(bad ? 1 : 0);
