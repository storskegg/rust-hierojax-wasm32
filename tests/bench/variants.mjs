#!/usr/bin/env node
// Copies build/compat/ to build/variants/<name>/ with the shim's default lever list replaced,
// so that every combination can be run by the parity harness and the benchmarks as a plain
// `--impl` directory. (The shim also reads `?hx=` from the page URL and `window.HIEROXIDE_LEVERS`.)
//
//   node variants.mjs batch canvas fontwait adv draw all=batch,canvas,fontwait base=
//
// A word is a lever (directory named after it) or name=lever,lever (`name=` is none).
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const src = path.join(ROOT, 'build/compat');
for (const word of process.argv.slice(2)) {
  const [name, levers = name] = word.includes('=') ? word.split('=') : [word];
  const dir = path.join(ROOT, 'build/variants', name);
  fs.rmSync(dir, { recursive: true, force: true });
  fs.mkdirSync(dir, { recursive: true });
  for (const f of fs.readdirSync(src)) fs.copyFileSync(path.join(src, f), path.join(dir, f));
  const f = path.join(dir, 'hierojax.js');
  const text = fs.readFileSync(f, 'utf8');
  const from = /const HIEROXIDE_LEVERS_DEFAULT\s*=\s*(['"])[^'"]*\1;?/; // the build is minified
  if (!from.test(text)) throw new Error('build/compat is not built from the current shim');
  fs.writeFileSync(f, text.replace(from, () => `const HIEROXIDE_LEVERS_DEFAULT = '${levers}';`));
  console.log(`${path.relative(ROOT, dir)}: ${levers || '(none)'}`);
}
