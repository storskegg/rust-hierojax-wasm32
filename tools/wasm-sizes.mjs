#!/usr/bin/env node
// What takes the space in a wasm module: section sizes, and the largest functions by name
// (from the `name` section, so build with `node tools/build-compat.mjs --keep-names --out X`).
//
//   node tools/wasm-sizes.mjs module.wasm [--top 25]
//
// A stand-in for `twiggy`, which is not installed here. Function sizes are code bytes only
// (no data, no table), and wasm-opt may have inlined or merged what the names say.
import fs from 'node:fs';
import { parseArgs } from 'node:util';

const { values: v, positionals } = parseArgs({ allowPositionals: true, options: { top: { type: 'string', default: '25' } } });
const buf = fs.readFileSync(positionals[0]);
let p = 8;
const leb = () => {
  let r = 0, s = 0, b;
  do {
    b = buf[p++];
    r += (b & 0x7f) * 2 ** s;
    s += 7;
  } while (b & 0x80);
  return r;
};
const sections = [];
let code = null, names = null;
while (p < buf.length) {
  const id = buf[p++];
  const size = leb();
  const start = p;
  let name = '';
  if (id === 0) {
    const n = leb();
    name = buf.toString('utf8', p, p + n);
    p += n;
  }
  sections.push({ id, name, size, start, bodyStart: p, end: start + size });
  if (id === 10) code = sections.at(-1);
  if (id === 0 && name === 'name') names = sections.at(-1);
  p = start + size;
}
const ID = ['custom', 'type', 'import', 'function', 'table', 'memory', 'global', 'export', 'start', 'element', 'code', 'data', 'datacount'];
console.log('sections:');
for (const s of sections) console.log(`  ${(ID[s.id] + (s.name ? ' ' + s.name : '')).padEnd(24)} ${String(s.size).padStart(8)}`);

// Function bodies
const bodies = [];
if (code) {
  p = code.bodyStart;
  const n = leb();
  for (let i = 0; i < n; i++) {
    const size = leb();
    bodies.push(size);
    p += size;
  }
}
// Names: subsection 1 is function names (index -> name); imports come first in the index space.
const fnames = new Map();
if (names) {
  p = names.bodyStart;
  while (p < names.end) {
    const sub = buf[p++];
    const size = leb();
    const end = p + size;
    if (sub === 1) {
      const n = leb();
      for (let i = 0; i < n; i++) {
        const idx = leb();
        const l = leb();
        fnames.set(idx, buf.toString('utf8', p, p + l));
        p += l;
      }
    }
    p = end;
  }
}
// number of imported functions
let nImportedFns = 0;
const imp = sections.find((s) => s.id === 2);
if (imp) {
  p = imp.bodyStart;
  const n = leb();
  for (let i = 0; i < n; i++) {
    for (let k = 0; k < 2; k++) {
      const l = leb();
      p += l;
    }
    const kind = buf[p++];
    if (kind === 0) {
      leb();
      nImportedFns++;
    } else if (kind === 1) {
      p++;
      const f = leb();
      leb();
      if (f & 1) leb();
    } else if (kind === 2) {
      const f = leb();
      leb();
      if (f & 1) leb();
    } else if (kind === 3) {
      p += 2;
    }
  }
}
const ranked = bodies.map((size, i) => ({ size, name: fnames.get(i + nImportedFns) ?? `func[${i + nImportedFns}]` })).sort((a, b) => b.size - a.size);
console.log(`\n${bodies.length} functions, ${bodies.reduce((a, b) => a + b, 0)} bytes of code; largest:`);
for (const f of ranked.slice(0, Number(v.top))) console.log(`  ${String(f.size).padStart(7)}  ${f.name.slice(0, 110)}`);

// Group by crate/module prefix from the demangled-ish names.
const groups = new Map();
for (const f of ranked) {
  const m = /^(?:_ZN)?(\d+)?([A-Za-z_][A-Za-z0-9_]*)/.exec(f.name);
  const key = /hieroxide/i.test(f.name) ? 'hieroxide*' : /core|alloc|std/.test(f.name) ? 'core/alloc/std' : /wasm_bindgen|wbg|wbindgen/.test(f.name) ? 'wasm-bindgen' : f.name.startsWith('func[') ? 'unnamed' : 'other';
  groups.set(key, (groups.get(key) ?? 0) + f.size);
  void m;
}
console.log('\nby origin (by name):');
for (const [k, n] of [...groups].sort((a, b) => b[1] - a[1])) console.log(`  ${k.padEnd(18)} ${String(n).padStart(8)}`);
