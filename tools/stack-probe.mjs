#!/usr/bin/env node
// How much of wasm's shadow stack does a fragment nested to the parser's limit use?
//
//   node tools/stack-probe.mjs [--depth 256] [--module build/compat/hieroxide_wasm_bg.wasm]
//                              [--glue target/compat-bindgen/hieroxide_wasm.js]
//
// The module is run in node with a stand-in for the canvas host (every glyph is a solid box),
// once per candidate stack size. The stack size is the initial value of `__stack_pointer`
// (the stack grows down from there towards address 0, and running out of it is an out of
// bounds trap), so lowering that global in the binary is the same as linking with a smaller
// `-z stack-size`. A bisection finds the least size at which the fragment still lays out; that
// is the peak use, to the step given. Needs `wasm-tools` for the edit.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const { values: v } = parseArgs({
  options: {
    depth: { type: 'string', default: '256' },
    module: { type: 'string', default: path.join(ROOT, 'build/compat/hieroxide_wasm_bg.wasm') },
    glue: { type: 'string', default: path.join(ROOT, 'target/compat-bindgen/hieroxide_wasm.js') },
    step: { type: 'string', default: '256' },
  },
});

const MAN = '\u{13000}', VER = '\u{13430}', HOR = '\u{13431}';
export function nested(depth) {
  let s = MAN + VER + MAN;
  for (let i = 0; i < depth; i++) s = MAN + VER + MAN + HOR + '\u{13437}' + s + '\u{13438}';
  return s;
}

// The page's three host functions, with boxes for glyphs.
const hieroxideHost = {
  advance: (cp, fs_) => fs_ * 0.5,
  draw(w, h, params, chars) {
    const data = new Uint8Array(w * h * 4);
    for (let y = 3; y < h - 3; y++) for (let x = 3; x < w - 3; x++) data[(y * w + x) * 4 + 3] = 255;
    return data;
  },
  hasId: () => false,
};

const wasmBytes = fs.readFileSync(v.module);
const glue = fs.readFileSync(v.glue, 'utf8');

function stackPointerInit(bytes) {
  const wat = execFileSync('wasm-tools', ['print'], { input: bytes, maxBuffer: 1 << 30 }).toString();
  const m = /\(global [^\n]*\(mut i32\) i32\.const (\d+)\)/.exec(wat);
  return { wat, init: Number(m[1]), text: m[0] };
}
const base = stackPointerInit(wasmBytes);

function withStack(size) {
  const wat = base.wat.replace(base.text, base.text.replace(String(base.init), String(size)));
  return execFileSync('wasm-tools', ['parse'], { input: wat, maxBuffer: 1 << 30 });
}

async function lays(bytes, text) {
  globalThis.hieroxideHost = hieroxideHost;
  const wb = new Function(glue + '\n;return wasm_bindgen;')();
  await wb({ module_or_path: await WebAssembly.compile(bytes) });
  const compat = new wb.Compat();
  const units = new Uint16Array(text.length);
  for (let i = 0; i < text.length; i++) units[i] = text.charCodeAt(i);
  try {
    const status = compat.process(units, 22, 'black', new Array(13).fill(''), true);
    return status === 0 && compat.markup().startsWith('<svg');
  } catch (e) {
    return false;
  }
}

const text = nested(Number(v.depth));
console.log(`fragment: ${v.depth} nested levels, ${[...text].length} characters; default stack ${base.init} bytes`);
if (!(await lays(wasmBytes, text))) {
  console.log('does not lay out even with the default stack');
  process.exit(1);
}
let lo = 0, hi = base.init; // lo fails (or 0), hi works
const step = Number(v.step);
while (hi - lo > step) {
  const mid = Math.floor((lo + hi) / 2 / step) * step || step;
  if (mid <= lo || mid >= hi) break;
  if (await lays(withStack(mid), text)) hi = mid;
  else lo = mid;
}
console.log(`least stack that works: ${hi} bytes (fails at ${lo}); peak use is between ${lo} and ${hi}, step ${step}`);
console.log(`that is ${(100 * hi / base.init).toFixed(1)}% of the default 1 MiB stack`);
for (const d of [64, 128, 192, 256]) {
  if (d > Number(v.depth)) continue;
  let l = 0, h = base.init;
  const t = nested(d);
  while (h - l > step) {
    const mid = Math.floor((l + h) / 2 / step) * step || step;
    if (mid <= l || mid >= h) break;
    if (await lays(withStack(mid), t)) h = mid;
    else l = mid;
  }
  console.log(`  depth ${String(d).padStart(3)}: ${h} bytes`);
}
void os;
