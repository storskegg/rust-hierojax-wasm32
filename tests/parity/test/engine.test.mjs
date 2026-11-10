// The engine that the compat shim detects (`hieroxideEngine`, a top-level const of its script) in each
// browser: Firefox is gecko, Chromium is blink, WebKit is webkit. A wrong answer is still exact output, only
// slower (Firefox 159 was taken for WebKit when `MozAppearance` left the style object, issue #11), so
// nothing else would catch it.
//
// Needs the compat build (node ../../tools/build-compat.mjs); skipped without it.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { launch, ENGINE_NAMES } from '../lib/engines.mjs';
import { startServer } from '../lib/server.mjs';

const HERE = path.dirname(new URL(import.meta.url).pathname);
const COMPAT = path.resolve(HERE, '../../../build/compat');
const have = fs.existsSync(path.join(COMPAT, 'hierojax.js'));

const EXPECTED = { chromium: 'blink', firefox: 'gecko', webkit: 'webkit' };

const PAGE = `<!DOCTYPE html><html><head><meta charset="utf-8">
<script type="text/javascript" src="hierojax.js"></script></head><body></body></html>`;

for (const engine of ENGINE_NAMES) {
  test(`detected engine in ${engine}`, { skip: !have && 'build/compat is missing', timeout: 60000 }, async () => {
    const server = await startServer({ impl: COMPAT, pages: { '/p.html': PAGE } });
    const browser = await launch(engine);
    try {
      const page = await browser.newPage();
      await page.goto(server.url + '/p.html');
      assert.equal(await page.evaluate('hieroxideEngine'), EXPECTED[engine]);
    } finally {
      await browser.close();
      await server.close();
    }
  });
}
