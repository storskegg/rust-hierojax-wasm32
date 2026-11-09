#!/usr/bin/env node
// Builds the compat drop-in into build/compat/ (git-ignored): hierojax.js (the
// wasm-bindgen glue followed by js/compat/hierojax.js) and hieroxide_wasm_bg.wasm. The
// parity harness runs it with `--impl build/compat`.
//
//   node tools/build-compat.mjs [--opt O3|Oz|O2|none] [--out build/compat]
//                               [--embed|--separate] [--profile release] [--keep-names]
//                               [--test-levers]
//
// --embed writes one file: the module is inside hierojax.js as base64 and compiled when the
// script runs, so `syntax.parse` works at once, as upstream's does. --separate (the other
// build) loads hieroxide_wasm_bg.wasm next to the script and is ready later. See
// js/compat/README.md for which is the default and why.
//
// --test-levers keeps the shim's test switches (the speed levers, `forcebatch`, the profile hooks:
// js/compat/hierojax.js, between its `test-levers` markers), which a shipped build cuts out; the
// default output directory then is build/compat-test. The tests that set levers run that build,
// beside the shipped one (tests/README.md).
//
// Needs cargo with the wasm32-unknown-unknown target, the wasm-bindgen CLI at the version
// the `wasm-bindgen` crate is pinned to (checked below), and wasm-opt unless `--opt none`.
// The default is `--opt O3`, as the plan says speed comes first within the size budget; the
// size notes compare O3 with Oz and with opt-level s.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';
import { stripTestLevers } from './strip-test-levers.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const { values: v } = parseArgs({
  options: {
    opt: { type: 'string', default: 'O3' },
    out: { type: 'string' },
    'opt-level': { type: 'string' },
    'keep-names': { type: 'boolean', default: false },
    'test-levers': { type: 'boolean', default: false },
    embed: { type: 'boolean', default: false },
    separate: { type: 'boolean', default: false },
  },
});

v.out = path.resolve(v.out ?? path.join(ROOT, v['test-levers'] ? 'build/compat-test' : 'build/compat')); // from the caller's folder; the tools below run in ROOT
const EMBED_DEFAULT = false;

const run = (cmd, args, env = {}) =>
  execFileSync(cmd, args, { cwd: ROOT, stdio: ['ignore', 'pipe', 'inherit'], env: { ...process.env, ...env } }).toString();

// The CLI and the crate must agree, or the glue and the module do not fit.
const cli = run('wasm-bindgen', ['--version']).trim().split(/\s+/)[1];
const lock = fs.readFileSync(path.join(ROOT, 'Cargo.lock'), 'utf8');
const m = /name = "wasm-bindgen"\nversion = "([^"]+)"/.exec(lock);
if (!m || m[1] !== cli) {
  console.error(`wasm-bindgen CLI is ${cli} but Cargo.lock has ${m ? m[1] : 'none'}; they must match`);
  process.exit(1);
}

const env = {};
if (v['opt-level']) env.CARGO_PROFILE_RELEASE_OPT_LEVEL = v['opt-level'];
if (v['keep-names']) env.CARGO_PROFILE_RELEASE_STRIP = 'false';
run('cargo', ['build', '-p', 'hieroxide-wasm', '--target', 'wasm32-unknown-unknown', '--release'], env);

const raw = path.join(ROOT, 'target/wasm32-unknown-unknown/release/hieroxide_wasm.wasm');
const tmp = path.join(ROOT, 'target/compat-bindgen');
fs.rmSync(tmp, { recursive: true, force: true });
run('wasm-bindgen', ['--target', 'no-modules', '--no-typescript', ...(v['keep-names'] ? ['--keep-debug'] : []), '--out-dir', tmp, raw]);

fs.mkdirSync(v.out, { recursive: true });
const wasmOut = path.join(v.out, 'hieroxide_wasm_bg.wasm');
const bg = path.join(tmp, 'hieroxide_wasm_bg.wasm');
if (v.opt === 'none') {
  fs.copyFileSync(bg, wasmOut);
} else {
  run('wasm-opt', [`-${v.opt}`, '--enable-bulk-memory', '--enable-sign-ext', '--enable-nontrapping-float-to-int', '--enable-mutable-globals', ...(v['keep-names'] ? ['-g'] : []), bg, '-o', wasmOut]);
}
const glue = fs.readFileSync(path.join(tmp, 'hieroxide_wasm.js'), 'utf8');
const shim = stripTestLevers(fs.readFileSync(path.join(ROOT, 'js/compat/hierojax.js'), 'utf8'), v['test-levers']);
const embed = v.embed ? !v.separate : EMBED_DEFAULT && !v.separate;
const prelude = embed ? `const HIEROXIDE_WASM_B64 = '${fs.readFileSync(wasmOut).toString('base64')}';\n` : '';
fs.writeFileSync(path.join(v.out, 'hierojax.js'), prelude + glue + '\n' + shim);
const wasmBytes = fs.statSync(wasmOut).size;
if (embed) fs.rmSync(wasmOut);
console.log(`wasm ${wasmBytes} bytes (${embed ? 'embedded' : 'separate'}${v['test-levers'] ? ', test levers' : ''}), hierojax.js ${fs.statSync(path.join(v.out, 'hierojax.js')).size} bytes -> ${path.relative(ROOT, v.out)}`);
