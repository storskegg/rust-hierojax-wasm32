#!/usr/bin/env node
// The size gate: what a browser downloads, in bytes of brotli at quality 11, for each build, against
// the limits in tools/size-limits.json. Exit 1 if a build is over its limit. Local and CI run this.
//
//   node tools/size-check.mjs [--build] [--paths] [--only compat|consistent|consistent-paths] [--json]
//
// --build first runs tools/build-compat.mjs and tools/build-consistent.mjs (the default builds, minified,
// with wasm-opt -O3); without it the builds in build/ are measured as they are. --paths also builds
// consistent-paths (reported, not limited), which a build in CI does not need.
//
// What is counted: the files of a build, each compressed on its own and the sizes added, as a server
// sends them. compat: hieroxide_wasm_bg.wasm + hierojax.js. consistent: hieroxide_outline_bg.wasm +
// hieroxide_outline.js + index.js. consistent-paths (the build with `svg: 'paths'`): the same files,
// reported and not limited. The font is not counted (a page loads it anyway).
//
// Brotli is node's own (zlib.brotliCompressSync, quality 11, window 2^24, the size hint of the file), so
// the number depends on the node version only through the brotli library it bundles; CI pins node, and
// `node --version` and `process.versions.brotli` are printed. The window is larger than any file here,
// so it makes no difference to the result. The `brotli` command line tool at -q 11 (its default window
// is 2^22) gives the same bytes for these inputs, but this is the number that counts.
//
// The limit of compat is a ratchet: it is set to what the build reached, so that it cannot grow back.
// The target is in the same file (`target`): 75,000 bytes. Lower the limit when the build gets smaller.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const { values: v } = parseArgs({
  options: {
    build: { type: 'boolean', default: false },
    paths: { type: 'boolean', default: false },
    only: { type: 'string' },
    json: { type: 'boolean', default: false },
  },
});

const limits = JSON.parse(fs.readFileSync(path.join(ROOT, 'tools/size-limits.json'), 'utf8'));
const BUILDS = {
  compat: ['build/compat', ['hieroxide_wasm_bg.wasm', 'hierojax.js']],
  consistent: ['build/consistent', ['hieroxide_outline_bg.wasm', 'hieroxide_outline.js', 'index.js']],
  'consistent-paths': ['build/consistent-paths', ['hieroxide_outline_bg.wasm', 'hieroxide_outline.js', 'index.js']],
};

export const brotli = (b) =>
  zlib.brotliCompressSync(b, {
    params: {
      [zlib.constants.BROTLI_PARAM_QUALITY]: 11,
      [zlib.constants.BROTLI_PARAM_LGWIN]: 24,
      [zlib.constants.BROTLI_PARAM_SIZE_HINT]: b.length,
    },
  }).length;
const gzip = (b) => zlib.gzipSync(b, { level: 9 }).length;

if (v.build) {
  const run = (script, args) => execFileSync(process.execPath, [path.join(ROOT, 'tools', script), ...args], { cwd: ROOT, stdio: ['ignore', 'inherit', 'inherit'] });
  if (!v.only || v.only === 'compat') run('build-compat.mjs', []);
  if (!v.only || v.only === 'consistent') run('build-consistent.mjs', ['--only', 'consistent']);
  if (v.paths && (!v.only || v.only === 'consistent-paths')) run('build-consistent.mjs', ['--only', 'consistent-paths']);
}

console.log(`node ${process.version}, brotli ${process.versions.brotli}`);
let failed = false;
const result = {};
for (const [name, [dir, files]] of Object.entries(BUILDS)) {
  if (v.only && v.only !== name) continue;
  if (name === 'consistent-paths' && !fs.existsSync(path.join(ROOT, dir))) continue; // an optional build
  let total = 0;
  const parts = {};
  for (const f of files) {
    const b = fs.readFileSync(path.join(ROOT, dir, f));
    const s = { raw: b.length, gzip: gzip(b), brotli: brotli(b) };
    parts[f] = s;
    total += s.brotli;
    if (!v.json) console.log(`  ${name.padEnd(17)} ${f.padEnd(28)} ${String(s.raw).padStart(8)} raw ${String(s.gzip).padStart(8)} gzip -9 ${String(s.brotli).padStart(8)} brotli -q 11`);
  }
  const lim = limits[name];
  const over = lim && total > lim.limit;
  if (over) failed = true;
  result[name] = { brotli: total, limit: lim?.limit ?? null, target: lim?.target ?? null, parts };
  if (!v.json) {
    console.log(`${name}: ${total} B` + (lim ? ` (limit ${lim.limit}${lim.target ? `, target ${lim.target}` : ''}; ${over ? `OVER by ${total - lim.limit}` : `${lim.limit - total} under`})` : ' (reported, not limited)'));
    if (lim?.target && total > lim.target) console.log(`  ${name} is ${total - lim.target} B above its target of ${lim.target}; the limit is a ratchet at what was reached: lower it when the build shrinks`);
    if (lim && total < lim.limit - 500) console.log(`  ${name} is more than 500 B under its limit: lower the limit in tools/size-limits.json to ${total + 50}`);
  }
}
if (v.json) console.log(JSON.stringify(result, null, 2));
if (failed) {
  console.error('size check failed: a build is over its limit (tools/size-limits.json)');
  process.exit(1);
}
