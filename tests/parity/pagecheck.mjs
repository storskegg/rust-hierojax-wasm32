#!/usr/bin/env node
// Whole-page check, as a page really runs: all the covered fragments in one document, in the
// real way (`hierojax.processFragments()` once the page is loaded), so that things the
// per-fragment harness cannot see are exercised: shade patterns an earlier fragment left in
// the document (`document.getElementById` is document-wide), and fragments that stay in the
// page while later ones are laid out. The canonical markup of each fragment of upstream and of
// the replacement is compared.
//
//   node pagecheck.mjs --impl ../../build/compat [--engines chromium,firefox,webkit]
//
// The page holds the fragments of docs/texts/ and testsuite.html that the replacement can
// lay out (see lib/covered.mjs). Exit 1 on a difference.
//
// Run twice: with the shim's defaults, and with batching forced (`forcebatch`), because the page has the
// kind of fragments for which the shim by default writes one at a time (js/compat/README.md) and so
// would stop testing the batching. This compares markup only, which both ways must give.
import { parseArgs } from 'node:util';
import { launch } from './lib/engines.mjs';
import { startServer } from './lib/server.mjs';
import { CANON_SRC } from './lib/canon.mjs';
import { coveredPage } from './lib/covered.mjs';

const { values: v } = parseArgs({
  options: {
    impl: { type: 'string' },
    engines: { type: 'string', default: 'chromium,firefox,webkit' },
  },
});
if (!v.impl) {
  console.error('usage: node pagecheck.mjs --impl <dir> [--engines chromium,firefox,webkit]');
  process.exit(2);
}

async function run(engine, impl, html, levers) {
  const server = await startServer({ impl, pages: { '/covered.html': html } });
  const browser = await launch(engine);
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
  if (levers != null) await ctx.addInitScript(`window.HIEROXIDE_LEVERS = ${JSON.stringify(levers)};`);
  const page = await ctx.newPage();
  page.setDefaultTimeout(120000);
  const log = [];
  page.on('console', (m) => log.push(`${m.type()}: ${m.text()}`.slice(0, 200)));
  page.on('pageerror', (e) => log.push('pageerror: ' + e.message.slice(0, 200)));
  await page.goto(server.url + '/covered.html', { waitUntil: 'load' });
  await page.waitForFunction(() => {
    const spans = [...document.querySelectorAll('body > div > span')];
    return spans.length > 0 && spans.every((s) => s.firstElementChild);
  });
  const out = await page.evaluate((src) => {
    const canon = (0, eval)('(' + src + ')');
    return [...document.querySelectorAll('body > div > span')].map((s) => canon(s));
  }, CANON_SRC);
  await browser.close();
  await server.close();
  return { out, log };
}

let bad = 0;
for (const engine of v.engines.split(',')) {
  const page = await coveredPage({ coverageImpl: v.impl, engine });
  const a = await run(engine, 'upstream', page.html);
  for (const [mode, levers] of [['defaults', null], ['forced batching', 'batch,fontwait,forcebatch']]) {
  const b = await run(engine, v.impl, page.html, levers);
  let diff = 0;
  let first = -1;
  for (let i = 0; i < a.out.length; i++) {
    if (a.out[i] !== b.out[i]) {
      diff++;
      if (first < 0) first = i;
    }
  }
  const same = a.out.length === b.out.length ? a.out.length - diff : 0;
  console.log(`${engine.padEnd(9)} ${page.count} covered fragments in one page (${mode}): ${same} identical, ${diff} differ` + (a.out.length !== b.out.length ? ` (counts ${a.out.length} vs ${b.out.length})` : ''));
  if (diff || a.out.length !== b.out.length) {
    bad++;
    if (first >= 0) {
      const x = a.out[first].split('\n'), y = b.out[first].split('\n');
      let p = 0;
      while (p < x.length && x[p] === y[p]) p++;
      console.log(`  first at fragment ${first}, line ${p + 1}:\n  - ${(x[p] ?? '').slice(0, 200)}\n  + ${(y[p] ?? '').slice(0, 200)}`);
    }
    if (a.log.length || b.log.length) console.log('  console:', JSON.stringify(a.log.slice(0, 3)), JSON.stringify(b.log.slice(0, 3)));
  }
  }
}
process.exit(bad ? 1 : 0);
