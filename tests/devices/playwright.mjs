#!/usr/bin/env node
// The harness page in Playwright's engines: a check of the harness itself, where the answers are
// known (it should agree with tests/parity and tests/golden). Needs tests/parity's install.
//
//   node tests/devices/playwright.mjs [--engines chromium,firefox,webkit] [--query 'n=2500&reps=3']
import { parseArgs } from 'node:util';
import { launch } from '../parity/lib/engines.mjs';
import { startDeviceServer } from './server.mjs';

const { values: v } = parseArgs({ options: { engines: { type: 'string', default: 'chromium,firefox,webkit' }, query: { type: 'string', default: '' } } });
const server = await startDeviceServer();
let bad = 0;
for (const engine of v.engines.split(',')) {
  const browser = await launch(engine, engine === 'firefox' ? { prefs: { 'privacy.reduceTimerPrecision': false } } : {});
  const id = `pw-${engine}`;
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
  const page = await ctx.newPage();
  page.on('pageerror', (e) => console.error(`${engine}: pageerror: ${e.message}`.slice(0, 300)));
  await page.goto(`${server.url}/run.html?label=${id}&id=${id}${v.query ? '&' + v.query : ''}`);
  try {
    const { file } = await server.waitForResult(id, 30 * 60 * 1000);
    console.log(`${engine} ${browser.version()}: ${file}`);
  } catch (e) {
    bad++;
    console.error(`${engine}: ${e.message}`);
  }
  await browser.close();
}
await server.close();
process.exit(bad ? 1 : 0);
