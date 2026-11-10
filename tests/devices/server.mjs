#!/usr/bin/env node
// The device harness's server: serves a self-running page and takes the results back.
//
//   node tests/devices/server.mjs [--port 8765] [--host 127.0.0.1] [--out tests/devices/out]
//
// Open  http://localhost:PORT/run.html?label=NAME  in any browser. The page runs the checks
// itself (page/harness.js) and POSTs its JSON to /result, which is written to out/. GET and HEAD
// serve files; the only writes are POST /result and POST /log, and only into out/.
//
// What is served where (the page loads each implementation it compares, side by side):
//   /up/hierojax.js            docs/hierojax.js, upstream
//   /compat/*                  build/compat (hierojax.js and its wasm)
//   /consistent/*              build/consistent (index.js, the module)
//   /paths/*                   build/consistent-paths
//   /frame-upstream.html, /frame-compat.html   the page each is run in (parity.html's template, with
//                                              the parity driver's hooks and batch function)
//   /hierojax.css, /NewGardiner.otf   docs/, for every implementation alike
//   /data/parity.json?n=2500|full     the compat parity sample, from tests/parity/corpus/corpus.json
//   /data/golden.json          the golden corpus and its reference hashes
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { DOCS, REPO } from '../parity/lib/server.mjs';
import { INIT_SCRIPT, runBatch } from '../parity/lib/driver.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const OUT = path.join(HERE, 'out');
const GOLDEN = path.join(REPO, 'tests/golden');
const PARITY_CORPUS = path.join(REPO, 'tests/parity/corpus/corpus.json');

const TYPES = {
  '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8', '.otf': 'font/otf', '.woff2': 'font/woff2', '.json': 'application/json',
  '.svg': 'image/svg+xml', '.wasm': 'application/wasm',
};

// ---- data ---------------------------------------------------------------------------------

// The tests/golden loader (engines.mjs), as JSON the page can use.
const unesc = (s) => s.replace(/\\(.)/g, (_, c) => (c === 't' ? '\t' : c === 'n' ? '\n' : c === 'r' ? '\r' : c));
const OPTIONS = ['fontsize', 'signcolor', 'dir', 'linesize', 'sep', 'separated', 'type', 'bracketcolor', 'shadepattern', 'align', 'border', 'standalone', 'log'];
let goldenJson;
function golden() {
  if (goldenJson) return goldenJson;
  const rd = (f) => zlib.gunzipSync(fs.readFileSync(path.join(GOLDEN, f))).toString('utf8');
  const items = rd('corpus.tsv.gz').split('\n').filter(Boolean).map((l) => {
    const c = l.split('\t');
    const attrs = {};
    c.slice(5, 18).forEach((a, i) => { if (a) attrs[OPTIONS[i]] = unesc(a); });
    return [c[0], c[2], attrs, unesc(c[18])];
  });
  const hashes = rd('hashes.txt.gz').split('\n').filter(Boolean);
  const manifest = Object.fromEntries(fs.readFileSync(path.join(GOLDEN, 'manifest.txt'), 'utf8').split('\n').filter(Boolean).map((l) => l.split(' ')));
  if (hashes.length !== items.length) throw new Error('tests/golden: corpus and hashes differ in length');
  goldenJson = JSON.stringify({ items, hashes, manifest });
  return goldenJson;
}

/**
 * A deterministic sample of the parity corpus, in corpus order: every fragment of upstream's test
 * suite (group "testsuite"), then an even stride through the rest up to n. n = Infinity is all.
 */
export function paritySample(corpus, n) {
  const items = corpus.items;
  if (!(n < items.length)) return items;
  const must = items.filter((it) => it.group === 'testsuite');
  const rest = items.filter((it) => it.group !== 'testsuite');
  const take = Math.max(0, n - must.length);
  const pick = new Set(must.map((it) => it.id));
  for (let i = 0; i < take; i++) pick.add(rest[Math.floor((i * rest.length) / take)].id);
  return items.filter((it) => pick.has(it.id));
}
let parityCorpus;
function parity(n) {
  parityCorpus ??= JSON.parse(fs.readFileSync(PARITY_CORPUS, 'utf8'));
  const items = paritySample(parityCorpus, n);
  return JSON.stringify({
    sha256: parityCorpus.sha256,
    total: parityCorpus.items.length,
    items: items.map(({ id, group, class: cls, text, attrs, fontSize, color }) => ({ id, group, class: cls, text, attrs, fontSize, color })),
  });
}

// ---- pages --------------------------------------------------------------------------------

// The template of tests/parity/lib/server.mjs, with the parity driver's hooks and batch function in it.
const frame = (script) => `<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><title>frame</title>
<link rel="stylesheet" type="text/css" href="/hierojax.css" />
<script>${INIT_SCRIPT}
window.__runBatch = (${runBatch.toString()});</script>
<script type="text/javascript" src="${script}"></script>
</head><body></body></html>
`;
const FRAMES = { '/frame-upstream.html': frame('/up/hierojax.js'), '/frame-compat.html': frame('/compat/hierojax.js') };

const INDEX = (base) => `<!DOCTYPE html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Hieroxide device harness</title></head>
<body style="font:16px system-ui;margin:1em"><h1>Hieroxide device harness</h1>
<p><a href="/run.html?label=manual">Run the checks</a> (about 1 to 5 minutes). Options: <code>?label=NAME&amp;n=2500|full&amp;reps=5&amp;golden=0</code>.</p>
<p>Served at ${base}.</p></body></html>`;

// ---- server -------------------------------------------------------------------------------

/** Start the server. Returns { url, port, close, results (Map id -> result), logs (Map id -> lines[]), waitForResult }. */
export async function startDeviceServer({ port = 0, host = '127.0.0.1', out = OUT } = {}) {
  const results = new Map();
  const logs = new Map();
  const waiters = new Map();
  const server = http.createServer((req, res) => {
    const send = (status, type, body, extra = {}) => {
      res.writeHead(status, { 'Content-Type': type, 'Cache-Control': 'no-store', ...extra });
      res.end(req.method === 'HEAD' ? undefined : body);
    };
    let u;
    try { u = new URL(req.url, 'http://x'); } catch { return send(400, 'text/plain', 'bad url'); }
    let p;
    try { p = decodeURIComponent(u.pathname); } catch { return send(400, 'text/plain', 'bad url'); }
    if (req.method === 'POST' && (p === '/result' || p === '/log')) {
      const id = (u.searchParams.get('id') || 'run').replace(/[^\w.-]/g, '_').slice(0, 80);
      const chunks = [];
      let size = 0;
      req.on('data', (c) => { size += c.length; if (size <= 20e6) chunks.push(c); });
      req.on('end', () => {
        if (size > 20e6) return send(413, 'text/plain', 'too large');
        const body = Buffer.concat(chunks).toString('utf8');
        if (p === '/log') {
          if (!logs.has(id)) logs.set(id, []);
          logs.get(id).push(body.slice(0, 500));
          return send(204, 'text/plain', '');
        }
        let data;
        try { data = JSON.parse(body); } catch { return send(400, 'text/plain', 'not json'); }
        fs.mkdirSync(out, { recursive: true });
        const file = path.join(out, `${id}.json`);
        fs.writeFileSync(file, JSON.stringify(data, null, 1));
        results.set(id, { file, data });
        (waiters.get(id) ?? []).forEach((f) => f({ file, data }));
        send(204, 'text/plain', '');
      });
      return;
    }
    if (req.method !== 'GET' && req.method !== 'HEAD') {
      res.writeHead(405, { Allow: 'GET, HEAD, POST' });
      return res.end();
    }
    const base = `http://${req.headers.host}`;
    if (p === '/' || p === '/index.html') return send(200, TYPES['.html'], INDEX(base));
    if (FRAMES[p]) return send(200, TYPES['.html'], FRAMES[p]);
    if (p === '/data/golden.json') return send(200, TYPES['.json'], golden());
    if (p === '/data/parity.json') {
      const n = u.searchParams.get('n');
      return send(200, TYPES['.json'], parity(n === 'full' ? Infinity : Number(n) || 2500));
    }
    if (p === '/up/hierojax.js') return send(200, TYPES['.js'], fs.readFileSync(path.join(DOCS, 'hierojax.js')));
    const mounts = [
      ['/compat/', path.join(REPO, 'build/compat')],
      ['/consistent/', path.join(REPO, 'build/consistent')],
      ['/paths/', path.join(REPO, 'build/consistent-paths')],
    ];
    let roots = [];
    let rel = p;
    const m = mounts.find(([pre]) => p.startsWith(pre));
    if (m) { roots = [m[1]]; rel = p.slice(m[0].length - 1); }
    else if (p === '/harness.js' || p === '/run.html') roots = [path.join(HERE, 'page')];
    else if (p === '/hierojax.css' || p === '/NewGardiner.otf') roots = [DOCS];
    else if (p === '/NewGardiner.woff2') { rel = '/NewGardiner.otf'; roots = [DOCS]; }
    for (const root of roots) {
      const file = path.resolve(root, '.' + path.posix.normalize('/' + rel));
      if (!file.startsWith(root + path.sep)) continue;
      let st;
      try { st = fs.statSync(file); } catch { continue; }
      if (!st.isFile()) continue;
      return send(200, TYPES[path.extname(file)] ?? 'application/octet-stream', fs.readFileSync(file));
    }
    send(404, 'text/plain', 'not found');
  });
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(port, host, resolve); });
  const actual = server.address().port;
  return {
    port: actual,
    url: `http://localhost:${actual}`,
    results,
    logs,
    waitForResult: (id, timeoutMs) => new Promise((resolve, reject) => {
      if (results.has(id)) return resolve(results.get(id));
      const t = setTimeout(() => reject(new Error(`no result for ${id} after ${timeoutMs} ms`)), timeoutMs);
      if (!waiters.has(id)) waiters.set(id, []);
      waiters.get(id).push((r) => { clearTimeout(t); resolve(r); });
    }),
    close: () => new Promise((resolve) => { server.closeAllConnections?.(); server.close(resolve); }),
  };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { values: v } = parseArgs({ options: { port: { type: 'string', default: '8765' }, host: { type: 'string', default: '127.0.0.1' }, out: { type: 'string', default: OUT } } });
  const s = await startDeviceServer({ port: Number(v.port), host: v.host, out: v.out });
  console.log(`device harness on http://${v.host}:${s.port}/  (open /run.html?label=NAME; results go to ${v.out})`);
  const seen = new Set();
  setInterval(() => {
    for (const [id, lines] of s.logs) if (lines.length) console.log(`[${id}] ${lines.splice(0).join('\n  ')}`);
    for (const [id, r] of s.results) if (!seen.has(id)) { seen.add(id); console.log(`[${id}] result -> ${r.file}`); }
  }, 1000);
}
