// The site options of the compat build (window.HIEROJAX_CONFIG) and the API around them, in every
// engine: off by default, and each does what js/compat/README.md says. The comparison with the
// site's own patched script (realpages.mjs --site, run.mjs --config) is the larger test of the
// same options; this one is small and runs without the other repository.
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
const EMBEDDED = path.resolve(HERE, '../../../build/embedded');
const have = fs.existsSync(path.join(COMPAT, 'hierojax.js'));

const PAGE = `<!DOCTYPE html><html><head><meta charset="utf-8">
<link rel="stylesheet" type="text/css" href="hierojax.css" />
<script>window.addEventListener('DOMContentLoaded', () => { window.__ready = true; });</script>
<script type="text/javascript" src="hierojax.js"></script></head>
<body><span class="hierojax" data-standalone="true" style="font-size: 20px">\u{13000}\u{13431}\u{13000}</span></body></html>`;

async function open(engine, impl, config) {
  const server = await startServer({ impl, pages: { '/p.html': PAGE } });
  const browser = await launch(engine);
  const ctx = await browser.newContext();
  if (config) await ctx.addInitScript(`window.HIEROJAX_CONFIG = ${JSON.stringify(config)};`);
  const page = await ctx.newPage();
  const requests = [];
  const messages = [];
  const dialogs = [];
  page.on('request', (r) => requests.push(new URL(r.url()).pathname));
  page.on('console', (m) => messages.push(`${m.type()}: ${m.text()}`));
  page.on('dialog', (d) => {
    dialogs.push(d.message());
    d.dismiss().catch(() => {});
  });
  return { server, browser, page, requests, messages, dialogs, close: async () => { await browser.close(); await server.close(); } };
}

for (const engine of ENGINE_NAMES) {
  test(`site options in ${engine}`, { skip: !have && 'build/compat is missing', timeout: 120000 }, async () => {
    // Off by default: nothing is processed by itself, no viewBox, the font where upstream has it.
    let t = await open(engine, COMPAT);
    try {
      await t.page.goto(t.server.url + '/p.html');
      await t.page.waitForFunction('hierojax.nFontsLoaded == hierojax.nFonts');
      await t.page.waitForTimeout(300);
      assert.equal(await t.page.evaluate(() => document.querySelector('.hierojax').children.length), 0, 'nothing processes by itself');
      assert.ok(t.requests.includes('/NewGardiner.otf'));
      assert.equal(await t.page.evaluate(() => PrintedSVG.EXTERNAL_FONT), 'NewGardiner.otf');
      await t.page.evaluate(() => hierojax.processFragmentNow(document.querySelector('.hierojax')));
      const svg = await t.page.evaluate(() => ({ vb: document.querySelector('svg').getAttribute('viewBox'), style: document.querySelector('svg').getAttribute('style'), css: document.querySelector('svg style').textContent }));
      assert.equal(svg.vb, null);
      assert.equal(svg.style, null);
      assert.match(svg.css, /url\('NewGardiner\.otf'\) format\('opentype'\)/);
    } finally {
      await t.close();
    }

    // All four on.
    t = await open(engine, COMPAT, { fontUrl: '/NewGardiner.woff2', fontFormat: 'woff2', svgScalable: true, autoProcess: true });
    try {
      await t.page.goto(t.server.url + '/p.html');
      await t.page.waitForFunction("document.querySelector('.hierojax svg')");
      // (hierojax.css still names the .otf in its own @font-face; the site's copy of it does not.)
      assert.ok(t.requests.includes('/NewGardiner.woff2'), 'the font comes from fontUrl');
      const svg = await t.page.evaluate(() => {
        const s = document.querySelector('svg');
        return { w: s.getAttribute('width'), h: s.getAttribute('height'), vb: s.getAttribute('viewBox'), sw: s.style.width, sh: s.style.height, css: s.querySelector('style').textContent, hidden: !!s.querySelector('.hierojax-svg-hidden') };
      });
      assert.equal(svg.vb, `0 0 ${svg.w} ${svg.h}`);
      assert.equal(svg.sw, `${Number(svg.w) / 20}em`);
      assert.equal(svg.sh, `${Number(svg.h) / 20}em`);
      assert.match(svg.css, /url\('\/NewGardiner\.woff2'\) format\('woff2'\)/);
      assert.ok(svg.hidden, 'the hidden selectable text stays');
    } finally {
      await t.close();
    }
  });

  test(`a font that does not load, in ${engine}`, { skip: !have && 'build/compat is missing', timeout: 120000 }, async () => {
    for (const noAlert of [false, true]) {
      const t = await open(engine, COMPAT, { fontUrl: '/missing.otf', noAlert });
      try {
        await t.page.clock.install();
        await t.page.goto(t.server.url + '/p.html');
        await t.page.evaluate(() => hierojax.processFragments());
        await t.page.clock.fastForward(60000);
        await t.page.waitForTimeout(200);
        const said = 'Seems unable to load fonts; perhaps try again later?';
        if (noAlert) {
          assert.deepEqual(t.dialogs, []);
          assert.ok(t.messages.includes(`error: ${said}`), `console.error says it: ${JSON.stringify(t.messages)}`);
        } else {
          assert.deepEqual(t.dialogs, [said]);
        }
      } finally {
        await t.close();
      }
    }
  });

  test(`syntax.parse in ${engine}`, { skip: !(have && fs.existsSync(EMBEDDED)) && 'build/compat or build/embedded is missing', timeout: 120000 }, async () => {
    const MAN = '\u{13000}';
    // The default build: not at once, then yes.
    let t = await open(engine, COMPAT);
    try {
      await t.page.goto(t.server.url + '/p.html', { waitUntil: 'commit' });
      await t.page.waitForFunction("typeof hierojax !== 'undefined'");
      const early = await t.page.evaluate(() => {
        // Before the module is in: a clear error. (It may already be in on a fast machine.)
        try { syntax.parse('x'); } catch (e) { return e instanceof Error ? e.message : 'not an Error'; }
        return 'parsed';
      });
      assert.match(early, /wait for hierojax\.ready|parse error/);
      await t.page.evaluate(() => hierojax.ready);
      const r = await t.page.evaluate(async (man) => {
        const f = syntax.parse(man + '\u{13431}' + man);
        let err;
        try { syntax.parse('x'); } catch (e) { err = e instanceof Error; }
        const el = document.createElement('span');
        document.body.appendChild(el);
        const opts = { type: 'dom', sep: '0', fontsize: 30 };
        f.print(el, opts);
        return { str: f.toString(), err, kids: el.children.length, cls: el.firstElementChild.className, opts };
      }, MAN);
      assert.equal(r.str, MAN + '\u{13431}' + MAN);
      assert.equal(r.err, true);
      assert.ok(r.kids > 0 && /hierojax-dom/.test(r.cls));
      assert.deepEqual(r.opts, { type: 'dom', sep: 0.07, fontsize: 30, dir: 'hlr', linesize: 1, separated: false, signcolor: 'black', bracketcolor: 'black', shadepattern: 'uniform', align: 'middle', border: false, standalone: false, log: false });
    } finally {
      await t.close();
    }
    // The single-file build: at once, in the script that follows it.
    t = await open(engine, EMBEDDED);
    try {
      await t.page.goto(t.server.url + '/p.html');
      assert.equal(await t.page.evaluate((man) => syntax.parse(man).toString(), MAN), MAN);
      assert.equal(await t.page.evaluate(() => hierojax.nFonts), 1, 'the module is not counted as a font');
    } finally {
      await t.close();
    }
  });
}
