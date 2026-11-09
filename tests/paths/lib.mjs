// Shared by the tests of path blocks: the CLI, a page server with a CSP header, the corpus.
import { execFileSync, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import zlib from 'node:zlib';
import { fileURLToPath } from 'node:url';

export const HERE = path.dirname(fileURLToPath(import.meta.url));
export const ROOT = path.resolve(HERE, '../..');
export const FONT = path.join(ROOT, 'docs/NewGardiner.otf');
export const CSP = "default-src 'self'; style-src 'self' 'unsafe-inline'";

/** Where `cargo build --release -p hieroxide-cli` puts the binary; builds it if it is not there. */
export function cliPath() {
  const p = path.join(ROOT, 'target/release/hieroxide');
  if (!fs.existsSync(p)) execFileSync('cargo', ['build', '--release', '-p', 'hieroxide-cli'], { cwd: ROOT, stdio: 'inherit' });
  return p;
}

/**
 * Render many fragments with one process (the batch form). `items` are `{text, ...options}` as the
 * batch lines take them; returns the parsed output lines.
 */
export function renderBatch(items, extraArgs = []) {
  const input = items.map((i) => JSON.stringify(i)).join('\n') + '\n';
  const r = spawnSync(cliPath(), ['render', '--font', FONT, '--batch', ...extraArgs], { input, encoding: 'utf8', maxBuffer: 1 << 30 });
  if (r.status !== 0 && r.status !== 1) throw new Error(`hieroxide exited ${r.status}: ${r.stderr}`);
  return r.stdout.split('\n').filter(Boolean).map((l) => JSON.parse(l));
}

const unesc = (s) => s.replace(/\\(.)/g, (_, c) => (c === 't' ? '\t' : c === 'n' ? '\n' : c === 'r' ? '\r' : c));
const OPTIONS = ['fontsize', 'signcolor', 'dir', 'linesize', 'sep', 'separated', 'type', 'bracketcolor', 'shadepattern', 'align', 'border', 'standalone', 'log'];

/** The reference corpus (tests/golden): `{id, group, class, attrs, text}` per fragment. */
export function loadCorpus() {
  const text = zlib.gunzipSync(fs.readFileSync(path.join(ROOT, 'tests/golden/corpus.tsv.gz'))).toString('utf8');
  return text.split('\n').filter(Boolean).map((l, index) => {
    const c = l.split('\t');
    const attrs = {};
    c.slice(5, 18).forEach((a, i) => { if (a) attrs[OPTIONS[i]] = unesc(a); });
    return { index, id: c[0], group: c[1], class: c[2], attrs, text: unesc(c[18]) };
  });
}

/** The corpus attributes as the options of a batch line. */
export function batchOptions(attrs) {
  const o = {};
  const num = (k) => { const n = Number(attrs[k]); return Number.isFinite(n) && n > 0 ? n : undefined; };
  if (['hlr', 'hrl', 'vlr', 'vrl'].includes(attrs.dir)) o.dir = attrs.dir;
  if (num('fontsize')) o.fontSize = num('fontsize');
  if (num('linesize')) o.lineSize = num('linesize');
  if (num('sep')) o.sep = num('sep');
  if (attrs.separated === 'true') o.separated = true;
  if (attrs.signcolor) o.signColor = attrs.signcolor;
  if (attrs.bracketcolor) o.bracketColor = attrs.bracketcolor;
  if (attrs.shadepattern === 'hatching') o.shading = 'hatching';
  if (attrs.align === 'bottom') o.align = 'bottom';
  if (attrs.border === 'true') o.border = true;
  return o;
}

/** A static server for in-memory pages and files; `headers` go on every response. */
export async function servePages(files, { headers = {} } = {}) {
  const requests = [];
  const server = http.createServer((req, res) => {
    const p = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    requests.push({ url: p, status: Object.hasOwn(files, p) ? 200 : 404 });
    if (!Object.hasOwn(files, p)) {
      res.writeHead(404, headers);
      return res.end();
    }
    const f = files[p];
    const type = p.endsWith('.otf') ? 'font/otf' : p.endsWith('.css') ? 'text/css' : p.endsWith('.js') ? 'text/javascript' : p.endsWith('.wasm') ? 'application/wasm' : 'text/html; charset=utf-8';
    res.writeHead(200, { 'Content-Type': type, 'Cache-Control': 'no-store', ...headers });
    res.end(f);
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  return { url: `http://127.0.0.1:${server.address().port}`, requests, close: () => new Promise((r) => server.close(r)) };
}

/** The text of `n` fragments of one class, from the corpus. */
export function pick(corpus, cls, n, step = 97) {
  const of = corpus.filter((f) => f.class === cls && f.attrs.type !== 'canvas' && f.attrs.type !== 'dom');
  const out = [];
  for (let i = 0; i < of.length && out.length < n; i += step) out.push(of[i]);
  return out;
}
