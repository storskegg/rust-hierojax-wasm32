// A small static server for docs/, read only (GET and HEAD, nothing written).
//
// The implementation under test is whatever script set --impl names. It is served as
// /hierojax.js (and as <dir>/hierojax.js, since docs/texts/ has its own copy of the
// script tag). Everything else a page asks for comes from docs/, so the HTML pages, the
// stylesheet and the font are the same for every implementation. An implementation
// that is a directory may also ship extra files (a .wasm, say); those are served first
// for any path other than the stylesheet and the font, which can never be overridden.
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const REPO = path.resolve(HERE, '../../..');
export const DOCS = path.join(REPO, 'docs');

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.otf': 'font/otf',
  '.woff2': 'font/woff2',
  '.json': 'application/json',
  '.svg': 'image/svg+xml',
  '.wasm': 'application/wasm',
};

// The template every implementation is run in: the real stylesheet, then the script.
const PARITY_PAGE = `<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><title>parity</title>
<link rel="stylesheet" type="text/css" href="hierojax.css" />
<script type="text/javascript" src="hierojax.js"></script>
</head><body></body></html>
`;

/** Resolve --impl to { name, script, dir }. 'upstream' is docs/hierojax.js. */
export function resolveImpl(impl) {
  if (!impl || impl === 'upstream') {
    return { name: 'upstream', script: path.join(DOCS, 'hierojax.js'), dir: null };
  }
  const p = path.resolve(impl);
  const st = fs.statSync(p);
  if (st.isDirectory()) {
    const script = path.join(p, 'hierojax.js');
    if (!fs.existsSync(script)) throw new Error(`${p} has no hierojax.js`);
    return { name: path.basename(p), script, dir: p };
  }
  return { name: path.basename(path.dirname(p)) || path.basename(p, '.js'), script: p, dir: path.dirname(p) };
}

/**
 * Start the server on an ephemeral port.
 * opts.impl: see resolveImpl. opts.stubScript: serve this text as hierojax.js instead
 * (used by the corpus builder to read pages without rendering them).
 * opts.transformScript: (text) => text, applied to the implementation script (the
 * benchmarks use it to add instrumentation).
 * opts.pages: { '/path.html': html } extra pages (the benchmarks' page of covered fragments).
 */
export async function startServer(opts = {}) {
  const impl = resolveImpl(opts.impl);
  const server = http.createServer((req, res) => {
    if (req.method !== 'GET' && req.method !== 'HEAD') {
      res.writeHead(405, { Allow: 'GET, HEAD' });
      return res.end();
    }
    let p;
    try {
      p = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    } catch {
      res.writeHead(400);
      return res.end();
    }
    const send = (status, type, body) => {
      res.writeHead(status, { 'Content-Type': type, 'Cache-Control': 'no-store' });
      res.end(req.method === 'HEAD' ? undefined : body);
    };
    if (p === '/parity.html') return send(200, TYPES['.html'], PARITY_PAGE);
    if (opts.pages && Object.hasOwn(opts.pages, p)) return send(200, TYPES['.html'], opts.pages[p]);
    if (p === '/empty.html') return send(200, TYPES['.html'], '<!DOCTYPE html><html><head><meta charset="utf-8"></head><body></body></html>');
    const base = path.posix.basename(p);
    if (base === 'hierojax.js') {
      if (opts.stubScript != null) return send(200, TYPES['.js'], opts.stubScript);
      let text = fs.readFileSync(impl.script, 'utf8');
      if (opts.transformScript) text = opts.transformScript(text);
      return send(200, TYPES['.js'], text);
    }
    const pinned = base === 'hierojax.css' || base === 'NewGardiner.otf';
    // The pages in docs/texts/ load the script by a relative URL, so a file the script
    // fetches (a .wasm) is looked for there too: any directory, by name.
    if (!pinned && impl.dir && opts.stubScript == null && /^[\w.-]+$/.test(base)) {
      const f = path.join(impl.dir, base);
      if (fs.existsSync(f) && fs.statSync(f).isFile()) return send(200, TYPES[path.extname(f)] ?? 'application/octet-stream', fs.readFileSync(f));
    }
    const roots = !pinned && impl.dir && opts.stubScript == null ? [impl.dir, DOCS] : [DOCS];
    for (const root of roots) {
      const file = path.resolve(root, '.' + path.posix.normalize('/' + p));
      if (file !== root && !file.startsWith(root + path.sep)) continue;
      let st;
      try {
        st = fs.statSync(file);
      } catch {
        continue;
      }
      if (!st.isFile()) continue;
      return send(200, TYPES[path.extname(file)] ?? 'application/octet-stream', fs.readFileSync(file));
    }
    send(404, 'text/plain', 'not found');
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const { port } = server.address();
  return {
    impl,
    url: `http://127.0.0.1:${port}`,
    close: () => new Promise((resolve) => server.close(resolve)),
  };
}
