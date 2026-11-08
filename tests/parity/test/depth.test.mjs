// The one allowlisted difference, pinned: the parser accepts 256 levels of nesting.
//
// Upstream's parser has no such limit. Enclosures nested 257 and 1,000 deep are laid out as
// SVG (the 1,000 slowly); at a few thousand upstream's own `toString` overflows the stack
// (a RangeError, from `parse(...).toString()` and from `processFragmentNow`). The compat build
// gives a parse error from 257 on: the red span and the log line, and no exception. At 256
// both agree. This test runs both in every engine and fails if either side changes.
//
// Needs the compat build (node ../../tools/build-compat.mjs); skipped without it.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { launch, ENGINE_NAMES } from '../lib/engines.mjs';
import { startServer } from '../lib/server.mjs';
import { INIT_SCRIPT } from '../lib/driver.mjs';

const HERE = path.dirname(new URL(import.meta.url).pathname);
const COMPAT = path.resolve(HERE, '../../../build/compat');
const have = fs.existsSync(path.join(COMPAT, 'hierojax.js'));

// In the page: what a nesting of `n` enclosures does.
function probe(n) {
  const text = '\u{1343C}'.repeat(n) + '\u{13000}' + '\u{1343D}'.repeat(n);
  const out = {};
  try {
    const f = syntax.parse(text);
    out.parse = 'ok';
    try {
      out.roundTrip = f.toString() === text;
    } catch (e) {
      out.roundTrip = e.name;
    }
  } catch (e) {
    out.parse = e instanceof Error ? 'Error' : typeof e;
  }
  const span = document.createElement('span');
  span.className = 'hierojax';
  span.textContent = text;
  document.body.appendChild(span);
  window.__hj.msgs.length = 0;
  try {
    hierojax.processFragmentNow(span);
    out.render = span.firstElementChild ? span.firstElementChild.localName + (span.firstElementChild.textContent.startsWith('parse error') ? ' (parse error)' : '') : 'empty';
  } catch (e) {
    out.render = 'throws ' + e.name;
  }
  out.logged = window.__hj.msgs.some((m) => m.startsWith('log: Parse error:\n'));
  span.remove();
  return out;
}

async function run(engine, impl, depths) {
  const server = await startServer({ impl });
  const browser = await launch(engine);
  try {
    const ctx = await browser.newContext();
    await ctx.addInitScript(INIT_SCRIPT);
    const page = await ctx.newPage();
    await page.goto(server.url + '/parity.html');
    await page.waitForFunction('hierojax.nFontsLoaded == hierojax.nFonts');
    const res = {};
    for (const n of depths) res[n] = await page.evaluate(`(${probe})(${n})`);
    return res;
  } finally {
    await browser.close();
    await server.close();
  }
}

for (const engine of ENGINE_NAMES) {
  test(`nesting depth, upstream and compat, in ${engine}`, { skip: !have && 'build/compat is missing', timeout: 180000 }, async () => {
    const depths = [256, 257, 1000, 3000];
    const up = await run(engine, 'upstream', depths);
    const co = await run(engine, COMPAT, depths);
    // 256: the same.
    assert.deepEqual(co[256], up[256]);
    assert.equal(up[256].render, 'svg');
    // 257 and 1,000: upstream lays it out, the port refuses.
    for (const n of [257, 1000]) {
      assert.deepEqual(up[n], { parse: 'ok', roundTrip: true, render: 'svg', logged: false }, `upstream at ${n}`);
      assert.deepEqual(co[n], { parse: 'Error', render: 'span (parse error)', logged: true }, `compat at ${n}`);
    }
    // 3,000: upstream either lays it out or overflows its own stack in toString, depending on the
    // engine's stack (Chromium does, WebKit does not); the port refuses, and does not throw.
    const own = JSON.stringify(up[3000]);
    assert.ok(
      own === JSON.stringify({ parse: 'ok', roundTrip: 'RangeError', render: 'throws RangeError', logged: false }) ||
        own === JSON.stringify({ parse: 'ok', roundTrip: true, render: 'svg', logged: false }),
      `upstream at 3000: ${own}`,
    );
    assert.deepEqual(co[3000], { parse: 'Error', render: 'span (parse error)', logged: true }, 'compat at 3000');
    console.log(`  ${engine}: upstream at 3000 levels: ${up[3000].render}`);
  });
}
