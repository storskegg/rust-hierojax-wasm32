#!/usr/bin/env node
// Sizes of the consistent build (limit: wasm + JS at most 100 KB with brotli -q 11).
//   node tools/consistent-sizes.mjs [--dir build/consistent]
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const { values: v } = parseArgs({ options: { dir: { type: 'string', default: path.join(ROOT, 'build/consistent') } } });
const br = (b) => zlib.brotliCompressSync(b, { params: { [zlib.constants.BROTLI_PARAM_QUALITY]: 11, [zlib.constants.BROTLI_PARAM_LGWIN]: 24, [zlib.constants.BROTLI_PARAM_SIZE_HINT]: b.length } }).length;
const gz = (b) => zlib.gzipSync(b, { level: 9 }).length;
let total = 0, tj = 0;
for (const f of ['hieroxide_outline_bg.wasm', 'hieroxide_outline.js', 'index.js']) {
  const b = fs.readFileSync(path.join(v.dir, f));
  console.log(f.padEnd(28), String(b.length).padStart(8), 'raw', String(gz(b)).padStart(8), 'gzip -9', String(br(b)).padStart(8), 'brotli -q 11');
  total += br(b);
  tj += b.length;
}
const js = Buffer.concat(['hieroxide_outline.js', 'index.js'].map((f) => fs.readFileSync(path.join(v.dir, f))));
console.log(`wasm + JS: ${total} bytes brotli, file by file; JS as one file ${br(js)} bytes (limit 100000)`);
