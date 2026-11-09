// The one read the compat build does that upstream does not: a descendant with `data-text` is the source
// of a block that `hieroxide fill` already wrote (its innerText is not the source). Upstream never writes
// `data-text`, so a span without one must take upstream's path: the same output as upstream, to the byte.
//
// Needs the compat build (node ../../tools/build-compat.mjs); skipped without it.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { launch, ENGINE_NAMES } from '../lib/engines.mjs';
import { startServer } from '../lib/server.mjs';
import { INIT_SCRIPT } from '../lib/driver.mjs';
import { CANON_SRC } from '../lib/canon.mjs';

const HERE = path.dirname(new URL(import.meta.url).pathname);
const COMPAT = path.resolve(HERE, '../../../build/compat');
const have = fs.existsSync(path.join(COMPAT, 'hierojax.js'));
const SRC = '\u{13000}\u{13431}\u{13001}';

// In the page: process spans of the given inner HTML and report what they hold.
function probe(inners, canonSrc) {
  const canon = (0, eval)('(' + canonSrc + ')');
  return inners.map((html) => {
    const span = document.createElement('span');
    span.className = 'hierojax';
    span.innerHTML = html;
    document.body.appendChild(span);
    hierojax.processFragmentNow(span);
    const out = { html: canon(span), svgs: span.querySelectorAll('svg').length, red: span.textContent.startsWith('parse error') };
    span.remove();
    return out;
  });
}

async function run(engine, impl, inners) {
  const server = await startServer({ impl });
  const browser = await launch(engine);
  try {
    const ctx = await browser.newContext();
    await ctx.addInitScript(INIT_SCRIPT);
    const page = await ctx.newPage();
    await page.goto(server.url + '/parity.html');
    await page.waitForFunction('hierojax.nFontsLoaded == hierojax.nFonts');
    return await page.evaluate(`(${probe})(${JSON.stringify(inners)}, ${JSON.stringify(CANON_SRC)})`);
  } finally {
    await browser.close();
    await server.close();
  }
}

for (const engine of ENGINE_NAMES) {
  test(`data-text, compat against upstream, in ${engine}`, { skip: !have && 'build/compat is missing', timeout: 120000 }, async () => {
    // Without a data-text descendant: text, text with other elements and attributes, a bad fragment.
    const plain = [SRC, `<b>${SRC}</b>`, `<i class="x" data-y="1">${SRC}</i>`, 'x', `${SRC}<span data-texts="1"></span>`];
    const up = await run(engine, 'upstream', plain);
    const co = await run(engine, COMPAT, plain);
    assert.deepEqual(co, up, 'no data-text: the same as upstream');
    assert.ok(up[0].svgs === 1 && up[3].red, 'the baseline draws, and the bad fragment is the red message');
    // With one: the block is drawn from it, as if the span held the source.
    const filled = [`<svg data-text="${SRC}"></svg><span>\n\u{13000}\n</span>`, `<span data-text="${SRC}">a\nb</span>`];
    const base = await run(engine, COMPAT, [SRC]);
    const got = await run(engine, COMPAT, filled);
    for (const g of got) assert.equal(g.html, base[0].html);
  });
}
