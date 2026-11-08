// Runs the corpus through one implementation in one engine.
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import { fileURLToPath } from 'node:url';
import { launch, platformTag } from './engines.mjs';
import { startServer } from './server.mjs';
import { INIT_SCRIPT, runBatch } from './driver.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const PARITY_DIR = path.resolve(HERE, '..');
export const CORPUS_PATH = path.join(PARITY_DIR, 'corpus', 'corpus.json');
export const OUT_DIR = path.join(PARITY_DIR, 'out');

export const READY_DEFAULT = 'hierojax.nFontsLoaded == hierojax.nFonts';

export function mulberry32(a) {
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export function shuffled(items, seed) {
  const a = items.slice();
  const rnd = mulberry32(seed);
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(rnd() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
}

export function readJson(file) {
  let buf = fs.readFileSync(file);
  if (file.endsWith('.gz')) buf = zlib.gunzipSync(buf);
  return JSON.parse(buf.toString('utf8'));
}

export function writeJson(file, obj) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const text = JSON.stringify(obj);
  fs.writeFileSync(file, file.endsWith('.gz') ? zlib.gzipSync(text, { level: 6 }) : text);
}

export function loadCorpus() {
  if (!fs.existsSync(CORPUS_PATH)) {
    throw new Error(`${CORPUS_PATH} is missing; build it with: node corpus/build.mjs`);
  }
  return JSON.parse(fs.readFileSync(CORPUS_PATH, 'utf8'));
}

function withTimeout(p, ms, what) {
  let timer;
  const t = new Promise((_, rej) => {
    timer = setTimeout(() => rej(new Error(`timeout after ${ms} ms: ${what}`)), ms);
  });
  return Promise.race([p, t]).finally(() => clearTimeout(timer));
}

/**
 * Run `items` in `engine` against `impl`.
 * Returns { meta, results } where results maps fragment id -> captured record.
 * opts: { impl, engine, runId, prefs, shuffle (seed), batchSize, ready, filter, limit, log }
 */
export async function runCorpus(opts) {
  const { engine, runId = 'run', prefs, shuffle, batchSize = 200, ready = READY_DEFAULT, log = console.error } = opts;
  const corpus = opts.corpus ?? loadCorpus();
  let items = corpus.items;
  if (opts.filter) items = items.filter(opts.filter);
  if (opts.limit) items = items.slice(0, opts.limit);
  if (shuffle != null) items = shuffled(items, shuffle);

  const server = await startServer({ impl: opts.impl });
  let browser = await launch(engine, { prefs });
  const version = browser.version();
  const pageLog = [];
  const crashes = [];
  const t0 = Date.now();

  const freshPage = async () => {
    if (!browser.isConnected()) {
      log(`  ${engine}: browser disconnected, relaunching`);
      browser = await launch(engine, { prefs });
    }
    const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
    await ctx.addInitScript(INIT_SCRIPT);
    const page = await ctx.newPage();
    page.setDefaultTimeout(60000);
    page.on('console', (m) => {
      if (m.type() === 'error' || m.type() === 'warning') pageLog.push(`${m.type()}: ${m.text()}`.slice(0, 300));
    });
    page.on('pageerror', (e) => pageLog.push(`pageerror: ${e.message}`.slice(0, 300)));
    page.on('dialog', (d) => d.dismiss().catch(() => {}));
    await page.goto(server.url + '/parity.html', { waitUntil: 'load' });
    await page.waitForFunction(ready, null, { timeout: 60000 });
    await page.evaluate(() => document.fonts.ready.then(() => true));
    return { ctx, page };
  };

  const runChunk = async (chunk) => {
    const { ctx, page } = await freshPage();
    try {
      return await withTimeout(page.evaluate(runBatch, chunk), 240000, `batch of ${chunk.length}`);
    } finally {
      await ctx.close().catch(() => {});
    }
  };

  // A crash in a batch must not lose the run: retry the batch in a fresh page, then
  // fragment by fragment, and record what still fails as crashed.
  const results = {};
  const record = (r) => {
    results[r.id] = r;
  };
  for (let i = 0; i < items.length; i += batchSize) {
    const chunk = items.slice(i, i + batchSize);
    let got;
    try {
      got = await runChunk(chunk);
    } catch (e1) {
      log(`  ${engine}: batch ${i} failed (${String(e1.message).slice(0, 120)}); retrying`);
      try {
        got = await runChunk(chunk);
      } catch (e2) {
        log(`  ${engine}: retry failed; running its ${chunk.length} fragments one by one`);
        got = [];
        for (const it of chunk) {
          try {
            got.push(...(await runChunk([it])));
          } catch (e3) {
            crashes.push(it.id);
            got.push({ id: it.id, crash: String(e3.message).slice(0, 200) });
          }
        }
      }
    }
    got.forEach(record);
    if ((i / batchSize) % 20 === 19 || i + batchSize >= items.length) {
      log(`  ${engine}/${runId}: ${Math.min(i + batchSize, items.length)}/${items.length} fragments, ${((Date.now() - t0) / 1000).toFixed(0)} s`);
    }
  }

  await browser.close().catch(() => {});
  await server.close();
  const meta = {
    impl: server.impl.name,
    engine,
    engineVersion: version,
    runId,
    platform: platformTag(),
    shuffle: shuffle ?? null,
    prefs: prefs ?? null,
    corpusSha256: corpus.sha256 ?? null,
    batchSize,
    fragments: items.length,
    crashes,
    pageLog: [...new Set(pageLog)].slice(0, 50),
    seconds: Math.round((Date.now() - t0) / 100) / 10,
    date: new Date().toISOString(),
  };
  return { meta, results };
}

export function defaultOut(meta) {
  return path.join(OUT_DIR, `${meta.impl}-${meta.engine}-${meta.runId}.json.gz`);
}
