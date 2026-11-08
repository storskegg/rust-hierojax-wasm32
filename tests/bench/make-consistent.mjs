#!/usr/bin/env node
// Builds build/consistent-bench/: consistent mode behind `hierojax`, for run.mjs:
//   node tests/bench/make-consistent.mjs [--opt O3|Oz|none]
//   node tests/bench/run.mjs --engine chromium --covered --no-throttle --impl ../../build/consistent-bench
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const { values: v } = parseArgs({ options: { opt: { type: 'string', default: 'O3' } } });
const run = (cmd, args) => execFileSync(cmd, args, { cwd: ROOT, stdio: ['ignore', 'pipe', 'inherit'] }).toString();
run('cargo', ['build', '-p', 'hieroxide-wasm-outline', '--target', 'wasm32-unknown-unknown', '--release']);
const tmp = path.join(ROOT, 'target/consistent-bench-bindgen');
fs.rmSync(tmp, { recursive: true, force: true });
run('wasm-bindgen', ['--target', 'no-modules', '--no-typescript', '--out-name', 'hieroxide_outline', '--out-dir', tmp, 'target/wasm32-unknown-unknown/release/hieroxide_wasm_outline.wasm']);
const out = path.join(ROOT, 'build/consistent-bench');
fs.mkdirSync(out, { recursive: true });
const from = path.join(tmp, 'hieroxide_outline_bg.wasm'), to = path.join(out, 'hieroxide_outline_bg.wasm');
if (v.opt === 'none') fs.copyFileSync(from, to);
else run('wasm-opt', [`-${v.opt}`, '--enable-bulk-memory', '--enable-sign-ext', '--enable-nontrapping-float-to-int', '--enable-mutable-globals', from, '-o', to]);
fs.writeFileSync(path.join(out, 'hierojax.js'), fs.readFileSync(path.join(tmp, 'hieroxide_outline.js'), 'utf8') + '\n' + fs.readFileSync(path.join(ROOT, 'tests/bench/consistent-shim.js'), 'utf8'));
console.log(`wasm ${fs.statSync(to).size} bytes -> build/consistent-bench`);
