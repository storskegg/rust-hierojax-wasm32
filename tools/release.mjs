#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
// Builds the release artifacts into dist/ (git-ignored): everything a user downloads from a GitHub release.
//
//   node tools/release.mjs [--only web|cli|sums] [--tag v0.1.0]
//
//   web   (default with cli) the JavaScript bundles, any host:
//           hierojax-compat-<v>.zip            the drop-in: hierojax.js + hieroxide_wasm_bg.wasm
//           hierojax-compat-embedded-<v>.zip   the drop-in as one file (the module inside hierojax.js)
//           hieroxide-js-<v>.zip               the new API: index.js, hieroxide_outline.js, the wasm, index.d.ts
//           hieroxide-js-paths-<v>.zip         the same with `svg: 'paths'` (a larger module, so separate)
//   cli   the `hieroxide` command for the host this runs on:
//           hieroxide-<v>-<rust target triple>.tar.gz (.zip on Windows)
//   sums  SHA256SUMS over every artifact in dist/ (CI runs it last, over what all jobs built).
//
// With no --only, web and cli are built and then sums. --tag fails unless it equals `v<version>` (CI passes the
// tag it runs on). The version is the one in Cargo (crates/hieroxide-cli/Cargo.toml; the crates must agree).
//
// The script fails when a build is over its limit in tools/size-limits.json (tools/size-check.mjs), and when a
// file that goes into an artifact holds a path of the build machine (tools/remap.mjs says which). Every
// build passes --remap-path-prefix; the archives are deterministic (tools/archive.mjs: sorted, one fixed time),
// with the time of the commit (or SOURCE_DATE_EPOCH).
//
// Every artifact says where its corresponding source is (GPL-3.0-only: a user of a download must be able to find
// the source of that build). SOURCE is the tree of the release tag on GitHub. The README.txt of every archive has
// it on a `Source:` line. Every .wasm in an archive gets a custom section named `source` holding the URL (appended
// here at release time by tools/wasm-custom.mjs, so the builds and size-check's numbers do not change). The one
// exception is the embedded drop-in: its module is base64 inside hierojax.js, so it has the README line and the
// banner only. Every shipped JavaScript file (hierojax.js, hieroxide_outline.js, index.js) starts with a one-line
// `/*! ... */` banner with the version, the licence and the same URL. The CLI binary carries nothing of its own:
// its archive's README.txt has the line. None of this has a time in it, so the archives stay deterministic.
//
// The Linux x86_64, macOS x86_64 and Windows x86_64 CLIs are built the same way by .github/workflows/release.yml,
// one job each; this script builds the host's only.
import { execFileSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';
import { writeTarGz, writeZip } from './archive.mjs';
import { forbiddenPaths, remapEnv } from './remap.mjs';
import { cleanFirst } from './clean.mjs';
import { appendCustomSection } from './wasm-custom.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const DIST = path.join(ROOT, 'dist');
const { values: v } = parseArgs({ options: { only: { type: 'string' }, tag: { type: 'string' } } });
if (v.only && !['web', 'cli', 'sums'].includes(v.only)) throw new Error('--only is web, cli or sums');

const run = (cmd, args, env = {}) =>
  execFileSync(cmd, args, { cwd: ROOT, stdio: ['ignore', 'inherit', 'inherit'], env: { ...process.env, ...env } });
const out = (cmd, args) => execFileSync(cmd, args, { cwd: ROOT }).toString().trim();
const read = (f) => fs.readFileSync(path.join(ROOT, f));
const fail = (msg) => {
  console.error(`release: ${msg}`);
  process.exit(1);
};

// The version: Cargo's, the same in every crate that ships.
const versionOf = (crate) => /^version = "([^"]+)"/m.exec(read(`crates/${crate}/Cargo.toml`).toString())?.[1];
const VERSION = versionOf('hieroxide-cli');
for (const c of ['hieroxide-wasm', 'hieroxide-wasm-outline', 'hieroxide', 'hieroxide-core']) {
  if (versionOf(c) !== VERSION) fail(`${c} is ${versionOf(c)} but hieroxide-cli is ${VERSION}: the crates must agree`);
}
if (v.tag && v.tag !== `v${VERSION}`) fail(`the tag ${v.tag} is not v${VERSION}, the version in Cargo`);
// The tree of the release tag: the corresponding source of every artifact.
const SOURCE = `https://github.com/storskegg/rust-hierojax-wasm32/tree/v${VERSION}`;
const SOURCE_LINE = `Source: ${SOURCE} (the corresponding source for this build, GPL-3.0-only).`;
const BANNER = `/*! Hieroxide ${VERSION} | GPL-3.0-only | source: ${SOURCE} */\n`;

// The time in the archives.
let EPOCH = Number(process.env.SOURCE_DATE_EPOCH);
if (!Number.isInteger(EPOCH) || EPOCH <= 0) {
  try { EPOCH = Number(out('git', ['log', '-1', '--format=%ct'])); } catch { EPOCH = 0; }
}

fs.mkdirSync(DIST, { recursive: true });

// Clean first, when anything is built: cargo's output, the earlier artifacts of the parts this run builds, and
// SHA256SUMS, which goes stale. Files that are not artifacts stay. `--only sums` builds nothing and cleans
// nothing: CI's sums job sums what the other jobs built.
if (v.only !== 'sums') {
  cleanFirst();
  const stale = [/^SHA256SUMS$/];
  if (!v.only || v.only === 'web') stale.push(/^hierojax-compat-/, /^hieroxide-js-/, /^\.embedded$/);
  if (!v.only || v.only === 'cli') stale.push(/^hieroxide-\d/);
  for (const f of fs.readdirSync(DIST)) {
    if (stale.some((re) => re.test(f))) fs.rmSync(path.join(DIST, f), { recursive: true, force: true });
  }
}
const artifacts = [];
const FORBIDDEN = forbiddenPaths();

// No path of the build machine in a file that ships.
function checkPaths(name, data) {
  for (const p of FORBIDDEN) {
    const at = data.indexOf(p);
    if (at >= 0) {
      const around = data.subarray(Math.max(0, at - 30), at + 60).toString('latin1').replace(/[^\x20-\x7e]/g, '.');
      fail(`${name} holds "${p}" (a path of the build machine): ...${around}...`);
    }
  }
}

function pack(file, entries, kind) {
  for (const e of entries) checkPaths(`${file}:${e.name}`, e.data);
  const target = path.join(DIST, file);
  (kind === 'zip' ? writeZip : writeTarGz)(target, entries, EPOCH);
  const size = fs.statSync(target).size;
  artifacts.push(file);
  console.log(`  ${file}  ${size} B  (${entries.map((e) => e.name).join(', ')})`);
}

const entry = (name, data, mode = 0o644) => ({ name, data: Buffer.isBuffer(data) ? data : Buffer.from(data), mode });
const LICENSE = read('LICENSE');
// A shipped JavaScript file with its banner; a shipped wasm module with its `source` section.
const js = (name, data) => entry(name, Buffer.concat([Buffer.from(BANNER), data]));
const wasm = (name, data) => entry(name, appendCustomSection(data, 'source', SOURCE));

if (!v.only || v.only === 'web') {
  console.log(`release ${VERSION}: JavaScript bundles`);
  // Build everything the size gate measures, then check it, before anything is packed.
  run('node', ['tools/size-check.mjs', '--build', '--paths']);
  const b = (dir, f) => fs.readFileSync(path.join(ROOT, 'build', dir, f));

  const compatReadme = (where) => `Hieroxide compat ${VERSION}: a drop-in for hierojax's rendering API

Use hierojax.js where docs/hierojax.js of hierojax was, with hierojax.css and NewGardiner.otf as before.
${where}
See README-compat.md for the API. Licence: GPL-3.0-only (LICENSE).
${SOURCE_LINE}
`;
  pack(`hierojax-compat-${VERSION}.zip`, [
    js('hierojax.js', b('compat', 'hierojax.js')),
    wasm('hieroxide_wasm_bg.wasm', b('compat', 'hieroxide_wasm_bg.wasm')),
    entry('README.txt', compatReadme('hieroxide_wasm_bg.wasm goes next to the script and must be served as application/wasm.')),
    entry('README-compat.md', read('js/compat/README.md')),
    entry('LICENSE', LICENSE),
  ], 'zip');

  // The embedded drop-in: a build of its own, in scratch space under dist/.
  const emb = path.join(DIST, '.embedded');
  fs.rmSync(emb, { recursive: true, force: true });
  run('node', ['tools/build-compat.mjs', '--embed', '--out', emb]);
  pack(`hierojax-compat-embedded-${VERSION}.zip`, [
    // The module is base64 inside hierojax.js: no wasm file to carry a section, so the README line and the banner say it.
    js('hierojax.js', fs.readFileSync(path.join(emb, 'hierojax.js'))),
    entry('README.txt', compatReadme('The module is inside hierojax.js: it is the only file, and nothing is served beside it.')),
    entry('README-compat.md', read('js/compat/README.md')),
    entry('LICENSE', LICENSE),
  ], 'zip');
  fs.rmSync(emb, { recursive: true, force: true });

  for (const [dir, file, what] of [
    ['consistent', `hieroxide-js-${VERSION}.zip`, 'text output (HTML and SVG with a font)'],
    ['consistent-paths', `hieroxide-js-paths-${VERSION}.zip`, 'text output and svg: \'paths\' (signs as paths, no font needed)'],
  ]) {
    pack(file, [
      js('index.js', b(dir, 'index.js')),
      entry('index.d.ts', read('js/hieroxide/index.d.ts')),
      js('hieroxide_outline.js', b(dir, 'hieroxide_outline.js')),
      wasm('hieroxide_outline_bg.wasm', b(dir, 'hieroxide_outline_bg.wasm')),
      entry('README.txt', `Hieroxide ${VERSION}: the JavaScript module, ${what}

import { init, render } from './index.js';
await init({ font: '/NewGardiner.otf' });
render(element, text, { dir: 'hrl' });

index.js, hieroxide_outline.js and hieroxide_outline_bg.wasm stay together (the module finds the wasm beside
hieroxide_outline.js; serve it as application/wasm). index.d.ts has the types. Licence: GPL-3.0-only (LICENSE).
${SOURCE_LINE}
`),
      entry('LICENSE', LICENSE),
    ], 'zip');
  }
}

if (!v.only || v.only === 'cli') {
  const triple = /^host: (.+)$/m.exec(out('rustc', ['-vV']))?.[1];
  if (!triple) fail('rustc -vV does not say its host');
  const windows = triple.includes('windows');
  console.log(`release ${VERSION}: hieroxide CLI for ${triple}`);
  run('cargo', ['build', '--release', '--locked', '-p', 'hieroxide-cli'], remapEnv());
  const exe = windows ? 'hieroxide.exe' : 'hieroxide';
  const bin = fs.readFileSync(path.join(ROOT, 'target/release', exe));
  const readme = `hieroxide ${VERSION} (${triple}): renders hieroglyph fragments to SVG in a build step.

Run \`${exe} --help\`. Licence: GPL-3.0-only (LICENSE).
${SOURCE_LINE}
`;
  const entries = [entry(exe, bin, 0o755), entry('README.txt', readme), entry('LICENSE', LICENSE)];
  if (windows) pack(`hieroxide-${VERSION}-${triple}.zip`, entries, 'zip');
  else pack(`hieroxide-${VERSION}-${triple}.tar.gz`, entries, 'tar');
}

if (!v.only || v.only === 'sums') {
  const files = fs.readdirSync(DIST).filter((f) => /\.(zip|tar\.gz)$/.test(f)).sort();
  if (!files.length) fail('no artifacts in dist/ to sum');
  const lines = files.map((f) => `${crypto.createHash('sha256').update(fs.readFileSync(path.join(DIST, f))).digest('hex')}  ${f}`);
  fs.writeFileSync(path.join(DIST, 'SHA256SUMS'), lines.join('\n') + '\n');
  console.log('SHA256SUMS');
  for (const l of lines) console.log(`  ${l}`);
}
