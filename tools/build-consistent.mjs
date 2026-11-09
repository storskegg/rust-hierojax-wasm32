#!/usr/bin/env node
// Builds what uses the outline raster, each into its own directory under build/ (git-ignored):
//
//   build/consistent/      the new API: index.js, hieroxide_outline.js, hieroxide_outline_bg.wasm
//   build/consistent-paths/ the same with the Cargo feature `paths`: `svg: 'paths'` works. Larger: the
//                          default build's size limit does not hold it (tools/consistent-sizes.mjs says by how much).
//   build/compat-outline/  the drop-in over outlines: hierojax.js and its .wasm, the same shim as
//                          build/compat. Measures the raster against upstream with the parity
//                          harness (--impl build/compat-outline); the fallback of a corrupt canvas
//                          readback is made from it.
//
//   build/compat-fallback/ build/compat plus the readback probe, and the module over outlines
//                          beside it for a browser that fails the probe.
//
//   node tools/build-consistent.mjs [--opt O3|Oz|none] [--only consistent|consistent-paths|compat-outline|compat-fallback]
//                                   [--no-minify]
//
// The JavaScript is minified (tools/minify.mjs), as in tools/build-compat.mjs; `--no-minify` leaves it
// readable. Needs what tools/build-compat.mjs needs.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';
import { minify } from './minify.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const { values: v } = parseArgs({
  options: {
    opt: { type: 'string', default: 'O3' },
    only: { type: 'string' },
    'opt-level': { type: 'string' },
    'no-minify': { type: 'boolean', default: false },
  },
});
const run = (cmd, args, env = {}) =>
  execFileSync(cmd, args, { cwd: ROOT, stdio: ['ignore', 'pipe', 'inherit'], env: { ...process.env, ...env } }).toString();

const WASM = path.join(ROOT, 'target/wasm32-unknown-unknown/release/hieroxide_wasm_outline.wasm');
const env = v['opt-level'] ? { CARGO_PROFILE_RELEASE_OPT_LEVEL: v['opt-level'] } : {};

function buildWasm(features) {
  run('cargo', ['build', '-p', 'hieroxide-wasm-outline', '--target', 'wasm32-unknown-unknown', '--release', ...features], env);
}

const js = (source, options) => (v['no-minify'] ? source : minify(source, options));

function opt(from, to) {
  if (v.opt === 'none') fs.copyFileSync(from, to);
  else run('wasm-opt', [`-${v.opt}`, '--enable-bulk-memory', '--enable-sign-ext', '--enable-nontrapping-float-to-int', '--enable-mutable-globals', from, '-o', to]);
}

// The new API: the default build, and the one with paths.
for (const [name, features] of [['consistent', []], ['consistent-paths', ['--features', 'paths']]]) {
  if (v.only && v.only !== name) continue;
  buildWasm(features);
  const tmp = path.join(ROOT, `target/${name}-bindgen`);
  fs.rmSync(tmp, { recursive: true, force: true });
  run('wasm-bindgen', ['--target', 'web', '--no-typescript', '--out-name', 'hieroxide_outline', '--out-dir', tmp, WASM]);
  const out = path.join(ROOT, `build/${name}`);
  fs.mkdirSync(out, { recursive: true });
  opt(path.join(tmp, 'hieroxide_outline_bg.wasm'), path.join(out, 'hieroxide_outline_bg.wasm'));
  const read = (f) => fs.readFileSync(f, 'utf8');
  fs.writeFileSync(path.join(out, 'hieroxide_outline.js'), js(read(path.join(tmp, 'hieroxide_outline.js')), { module: true }));
  fs.writeFileSync(path.join(out, 'index.js'), js(read(path.join(ROOT, 'js/hieroxide/index.js')), { module: true }));
  console.log(`${name}: wasm ${fs.statSync(path.join(out, 'hieroxide_outline_bg.wasm')).size} bytes`);
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
  fs.writeFileSync(path.join(out, 'hierojax.js'), js(glue + '\n' + prelude + '\n' + shim));
  console.log(`compat-outline: wasm ${fs.statSync(path.join(out, 'hieroxide_wasm_bg.wasm')).size} bytes`);
}

// The drop-in with the readback probe: build/compat with js/hieroxide/compat-fallback.js and its one
// change to the shim, and the outline module beside it to load when readback is corrupt. The shim
// itself stays as it is for now: the change is made here, at build time.
if (!v.only || v.only === 'compat-fallback') {
  // Unminified: the hook is spliced into the script's text below, and the whole is minified after.
  run('node', ['tools/build-compat.mjs', '--opt', v.opt, '--out', 'build/compat-fallback', '--separate', '--no-minify']);
  const out = path.join(ROOT, 'build/compat-fallback');
  buildWasm(['--features', 'compat-api']);
  const tmp = path.join(ROOT, 'target/compat-fallback-bindgen');
  fs.rmSync(tmp, { recursive: true, force: true });
  run('wasm-bindgen', ['--target', 'no-modules', '--no-modules-global', 'hieroxideOutline', '--no-typescript', '--out-name', 'hieroxide_outline_compat', '--out-dir', tmp, WASM]);
  opt(path.join(tmp, 'hieroxide_outline_compat_bg.wasm'), path.join(out, 'hieroxide_outline_compat_bg.wasm'));
  fs.writeFileSync(path.join(out, 'hieroxide_outline_compat.js'), js(fs.readFileSync(path.join(tmp, 'hieroxide_outline_compat.js'), 'utf8')));
  const script = path.join(out, 'hierojax.js');
  const hook = fs.readFileSync(path.join(ROOT, 'js/hieroxide/compat-fallback.js'), 'utf8');
  const s = fs.readFileSync(script, 'utf8');
  const url = `new URL('hieroxide_wasm_bg.wasm', hieroxideScriptUrl)`;
  const before = `hieroxideWasmReady = wasm_bindgen({ module_or_path: ${url} }).then(() => {
		hieroxideCompat = new wasm_bindgen.Compat();`;
  if (!s.includes(before)) throw new Error('the compat shim changed: see js/hieroxide/compat-fallback.js');
  const after = `hieroxideWasmReady = wasm_bindgen({ module_or_path: ${url} }).then(() => hieroxideChooseCompat(${url})).then((compat) => {
		hieroxideCompat = compat;`;
  const at = s.indexOf('// Hieroxide compat:');
  if (at < 0) throw new Error('the compat shim changed: see js/hieroxide/compat-fallback.js');
  fs.writeFileSync(script, js(s.slice(0, at) + hook + '\n' + s.slice(at).replace(before, after)));
  console.log('compat-fallback: built');
}
