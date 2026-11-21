#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
// Checks that every first-party source file starts with its SPDX licence line, and lists those that do not.
//
//   node tools/spdx-check.mjs
//
// The line is `// SPDX-License-Identifier: GPL-3.0-only` and is the first line of the file, or the second when the
// first is a shebang (`#!`). The files are:
//
//   Rust         every .rs under crates/, hieroxide-tables/ and tests/
//   JavaScript   tools/*.mjs, and every .mjs, .js, .ts and .d.ts under tests/ and js/
//
// Not checked: src/ and docs/ at the top (upstream's files), node_modules, target, build, dist, JSON, Markdown,
// HTML and the golden data. The generated tables (crates/hieroxide-core/src/tables/) are checked like the rest:
// tools/gen-tables.mjs writes the line, so regenerating keeps it.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export const LINE = '// SPDX-License-Identifier: GPL-3.0-only';
const SKIP = new Set(['node_modules', 'target', 'build', 'dist', '.git']);

function walk(dir, out = []) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    if (SKIP.has(e.name)) continue;
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, out);
    else out.push(p);
  }
  return out;
}

// The first-party source files, as paths relative to the repository root, sorted.
export function sourceFiles() {
  const rel = (dir) => walk(path.join(ROOT, dir)).map((f) => path.relative(ROOT, f).split(path.sep).join('/'));
  const rust = ['crates', 'hieroxide-tables', 'tests'].flatMap(rel).filter((f) => f.endsWith('.rs'));
  const script = (f) => /\.(mjs|js|ts)$/.test(f);
  const js = [...rel('tests'), ...rel('js')].filter(script);
  const tools = fs.readdirSync(path.join(ROOT, 'tools')).filter((f) => f.endsWith('.mjs')).map((f) => `tools/${f}`);
  return [...new Set([...rust, ...js, ...tools])].sort();
}

// True when the file's first line, or its second after a shebang, is the SPDX line.
export function hasHeader(text) {
  const lines = text.split('\n', 2);
  const at = lines[0].startsWith('#!') && !lines[0].startsWith('#![') ? 1 : 0;
  return lines[at]?.replace(/\r$/, '') === LINE;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const files = sourceFiles();
  const missing = files.filter((f) => !hasHeader(fs.readFileSync(path.join(ROOT, f), 'utf8')));
  if (missing.length) {
    console.error(`${missing.length} of ${files.length} files lack "${LINE}" as their first line (second after a shebang):`);
    for (const f of missing) console.error(`  ${f}`);
    process.exit(1);
  }
  console.log(`spdx: all ${files.length} first-party source files have the header`);
}
