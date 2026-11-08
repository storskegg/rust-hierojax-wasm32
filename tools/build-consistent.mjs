#!/usr/bin/env node
// Builds what uses the outline raster, each into its own directory under build/ (git-ignored):
//
//   build/consistent/      the new API: index.js, hieroxide_outline.js, hieroxide_outline_bg.wasm
//   build/compat-outline/  the drop-in over outlines: hierojax.js and its .wasm, the same shim as
//                          build/compat. Measures the raster against upstream with the parity
//                          harness (--impl build/compat-outline); the fallback of a corrupt canvas
//                          readback is made from it.
//
//   build/compat-fallback/ build/compat plus the readback probe, and the module over outlines
//                          beside it for a browser that fails the probe.
//
//   node tools/build-consistent.mjs [--opt O3|Oz|none] [--only consistent|compat-outline|compat-fallback]
//
// Needs what tools/build-compat.mjs needs.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const { values: v } = parseArgs({
  options: {
    opt: { type: 'string', default: 'O3' },
    only: { type: 'string' },
    'opt-level': { type: 'string' },
  },
});
const run = (cmd, args, env = {}) =>
  execFileSync(cmd, args, { cwd: ROOT, stdio: ['ignore', 'pipe', 'inherit'], env: { ...process.env, ...env } }).toString();

const WASM = path.join(ROOT, 'target/wasm32-unknown-unknown/release/hieroxide_wasm_outline.wasm');
const env = v['opt-level'] ? { CARGO_PROFILE_RELEASE_OPT_LEVEL: v['opt-level'] } : {};

function buildWasm(features) {
  run('cargo', ['build', '-p', 'hieroxide-wasm-outline', '--target', 'wasm32-unknown-unknown', '--release', ...features], env);
}

function opt(from, to) {
  if (v.opt === 'none') fs.copyFileSync(from, to);
  else run('wasm-opt', [`-${v.opt}`, '--enable-bulk-memory', '--enable-sign-ext', '--enable-nontrapping-float-to-int', '--enable-mutable-globals', from, '-o', to]);
}

if (!v.only || v.only === 'consistent') {
  buildWasm([]);
  const tmp = path.join(ROOT, 'target/consistent-bindgen');
  fs.rmSync(tmp, { recursive: true, force: true });
  run('wasm-bindgen', ['--target', 'web', '--no-typescript', '--out-name', 'hieroxide_outline', '--out-dir', tmp, WASM]);
  const out = path.join(ROOT, 'build/consistent');
  fs.mkdirSync(out, { recursive: true });
  opt(path.join(tmp, 'hieroxide_outline_bg.wasm'), path.join(out, 'hieroxide_outline_bg.wasm'));
  // Comments and indentation out; nothing cleverer, so no dependency.
  const slim = (f) => fs.readFileSync(f, 'utf8').replace(/\/\*\*[\s\S]*?\*\//g, '').split('\n').map((l) => l.trim()).filter((l) => l && !l.startsWith('//')).join('\n') + '\n';
  fs.writeFileSync(path.join(out, 'hieroxide_outline.js'), slim(path.join(tmp, 'hieroxide_outline.js')));
  fs.writeFileSync(path.join(out, 'index.js'), slim(path.join(ROOT, 'js/hieroxide/index.js')));
  console.log(`consistent: wasm ${fs.statSync(path.join(out, 'hieroxide_outline_bg.wasm')).size} bytes`);
}

if (!v.only || v.only === 'compat-outline') {
  buildWasm(['--features', 'compat-api']);
  const tmp = path.join(ROOT, 'target/compat-outline-bindgen');
  fs.rmSync(tmp, { recursive: true, force: true });
  run('wasm-bindgen', ['--target', 'no-modules', '--no-typescript', '--out-dir', tmp, WASM]);
  const out = path.join(ROOT, 'build/compat-outline');
  fs.mkdirSync(out, { recursive: true });
  opt(path.join(tmp, 'hieroxide_wasm_outline_bg.wasm'), path.join(out, 'hieroxide_wasm_bg.wasm'));
  const glue = fs.readFileSync(path.join(tmp, 'hieroxide_wasm_outline.js'), 'utf8');
  const shim = fs.readFileSync(path.join(ROOT, 'js/compat/hierojax.js'), 'utf8');
  const prelude = fs.readFileSync(path.join(ROOT, 'js/hieroxide/compat-outline-prelude.js'), 'utf8');
  fs.writeFileSync(path.join(out, 'hierojax.js'), glue + '\n' + prelude + '\n' + shim);
  console.log(`compat-outline: wasm ${fs.statSync(path.join(out, 'hieroxide_wasm_bg.wasm')).size} bytes`);
}

// The drop-in with the readback probe: build/compat with js/hieroxide/compat-fallback.js and its one
// change to the shim, and the outline module beside it to load when readback is corrupt. The shim
// itself stays as it is for now: the change is made here, at build time.
if (!v.only || v.only === 'compat-fallback') {
  run('node', ['tools/build-compat.mjs', '--opt', v.opt, '--out', 'build/compat-fallback']);
  const out = path.join(ROOT, 'build/compat-fallback');
  buildWasm(['--features', 'compat-api']);
  const tmp = path.join(ROOT, 'target/compat-fallback-bindgen');
  fs.rmSync(tmp, { recursive: true, force: true });
  run('wasm-bindgen', ['--target', 'no-modules', '--no-modules-global', 'hieroxideOutline', '--no-typescript', '--out-name', 'hieroxide_outline_compat', '--out-dir', tmp, WASM]);
  opt(path.join(tmp, 'hieroxide_outline_compat_bg.wasm'), path.join(out, 'hieroxide_outline_compat_bg.wasm'));
  fs.copyFileSync(path.join(tmp, 'hieroxide_outline_compat.js'), path.join(out, 'hieroxide_outline_compat.js'));
  const script = path.join(out, 'hierojax.js');
  const hook = fs.readFileSync(path.join(ROOT, 'js/hieroxide/compat-fallback.js'), 'utf8');
  const s = fs.readFileSync(script, 'utf8');
  const before = `pending.push(wasm_bindgen({ module_or_path: url }).then(() => {
			hierojax.compat = new wasm_bindgen.Compat();`;
  if (!s.includes(before)) throw new Error('the compat shim changed: see js/hieroxide/compat-fallback.js');
  const after = `pending.push(wasm_bindgen({ module_or_path: url }).then(() => hieroxideChooseCompat(url)).then((compat) => {
			hierojax.compat = compat;`;
  const at = s.indexOf('// Hieroxide compat shim');
  fs.writeFileSync(script, s.slice(0, at) + hook + '\n' + s.slice(at).replace(before, after));
  console.log('compat-fallback: built');
}
