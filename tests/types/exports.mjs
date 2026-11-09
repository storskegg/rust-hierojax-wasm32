// The names index.d.ts declares as values are the names index.js exports, and no others.
//   node tools/build-consistent.mjs --only consistent   (the module is read from its build)
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const dts = fs.readFileSync(path.join(ROOT, 'js/hieroxide/index.d.ts'), 'utf8');
const declared = [...dts.matchAll(/^export (?:function|const|class) (\w+)/gm)].map((m) => m[1]).sort();
// The values the source exports: `export function f`, `export const f`.
const src = fs.readFileSync(path.join(ROOT, 'js/hieroxide/index.js'), 'utf8');
const exported = [...src.matchAll(/^export (?:async )?(?:function|const) (\w+)/gm)].map((m) => m[1]).sort();
if (JSON.stringify(declared) !== JSON.stringify(exported)) {
  console.error(`index.d.ts declares ${declared.join(', ')}; index.js exports ${exported.join(', ')}`);
  process.exit(1);
}
// The built module (minified) must export the same: it is imported, which loads no wasm until `init`.
const built = path.join(ROOT, 'build/consistent/index.js');
if (fs.existsSync(built)) {
  const b = Object.keys(await import(pathToFileURL(built))).sort();
  if (JSON.stringify(b) !== JSON.stringify(exported)) {
    console.error(`build/consistent/index.js exports ${b.join(', ')}`);
    process.exit(1);
  }
}
console.log(`ok: ${exported.join(', ')}`);
