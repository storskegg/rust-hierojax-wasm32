#!/usr/bin/env node
// Sizes of the compat build for several settings: raw, gzip -9 and brotli -q 11, of the
// wasm module before and after wasm-opt (-O3 and -Oz), at opt-level 3 and "s", and of the
// script (glue + shim, and the shim alone).
//
//   node tools/size-report.mjs [--out target/size-report]
//
// Each variant is built with tools/build-compat.mjs into its own directory, so this takes a
// few minutes. Nothing in build/ is touched.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const { values: v } = parseArgs({ options: { out: { type: 'string', default: path.join(ROOT, 'target/size-report') } } });

const sizes = (buf) => ({
  raw: buf.length,
  gzip: zlib.gzipSync(buf, { level: 9 }).length,
  brotli: zlib.brotliCompressSync(buf, { params: { [zlib.constants.BROTLI_PARAM_QUALITY]: 11, [zlib.constants.BROTLI_PARAM_SIZE_HINT]: buf.length } }).length,
});
const row = (name, s) => `${name.padEnd(34)} ${String(s.raw).padStart(9)} ${String(s.gzip).padStart(9)} ${String(s.brotli).padStart(9)}`;

console.log(`${'variant'.padEnd(34)} ${'raw'.padStart(9)} ${'gzip -9'.padStart(9)} ${'brotli 11'.padStart(9)}`);
for (const level of ['3', 's']) {
  for (const opt of ['none', 'O3', 'Oz']) {
    const dir = path.join(v.out, `opt${level}-${opt}`);
    execFileSync(process.execPath, [path.join(ROOT, 'tools/build-compat.mjs'), '--opt', opt, '--opt-level', level, '--out', dir], {
      cwd: ROOT,
      stdio: ['ignore', 'ignore', 'inherit'],
    });
    console.log(row(`wasm opt-level=${level} wasm-opt ${opt}`, sizes(fs.readFileSync(path.join(dir, 'hieroxide_wasm_bg.wasm')))));
  }
}
const dir = path.join(v.out, 'opt3-O3');
console.log(row('hierojax.js (glue + shim)', sizes(fs.readFileSync(path.join(dir, 'hierojax.js')))));
console.log(row('js/compat/hierojax.js (shim alone)', sizes(fs.readFileSync(path.join(ROOT, 'js/compat/hierojax.js')))));
const up = path.join(ROOT, 'docs/hierojax.js');
console.log(row('upstream docs/hierojax.js', sizes(fs.readFileSync(up))));
