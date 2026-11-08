// Driver of the parser differential test.
//
// Usage: node run.mjs [--no-gen] [--no-build]
//
// Generates the inputs (gen.mjs), runs the jison oracle (oracle.mjs) and the Rust parser
// (examples/parser_dump) over each class of input, and compares accept/reject, toString
// and the tree dump line by line. Prints totals by class, how many inputs were accepted
// and rejected, and the coverage of the grammar's productions by the grammar-derived
// inputs (measured by counting jison's own reductions). Exits non-zero on any
// difference or if a production is never hit.

import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../..');
const OUT = path.join(HERE, 'out');
const args = new Set(process.argv.slice(2));
const run = (cmd, a, opts = {}) => execFileSync(cmd, a, { stdio: ['ignore', 'inherit', 'inherit'], cwd: ROOT, maxBuffer: 1 << 30, ...opts });

if (!args.has('--no-gen')) run(process.execPath, [path.join(HERE, 'gen.mjs'), OUT]);
if (!args.has('--no-build')) run('cargo', ['build', '--release', '-p', 'hieroxide-core', '--example', 'parser_dump']);
const dumpBin = path.join(ROOT, 'target/release/examples/parser_dump');

const classes = ['docs', 'random', 'mutation', 'derived', 'edge'];
let total = 0, totalDiff = 0, totalAcc = 0, totalRej = 0, nonParseErrors = 0;
const covTotal = [];
const rows = [];
let derivedCov = null;
for (const c of classes) {
  const inFile = path.join(OUT, c + '.txt');
  const oracleOut = path.join(OUT, c + '.oracle');
  const rustOut = path.join(OUT, c + '.rust');
  const covFile = path.join(OUT, c + '.cov.json');
  run(process.execPath, [path.join(HERE, 'oracle.mjs'), inFile, oracleOut, covFile]);
  run(dumpBin, [inFile, rustOut]);
  const o = fs.readFileSync(oracleOut, 'utf8').split('\n');
  const r = fs.readFileSync(rustOut, 'utf8').split('\n');
  const inputs = fs.readFileSync(inFile, 'utf8').split('\n');
  const n = inputs.length - 1;
  let acc = 0, rej = 0, diff = 0, nonParse = 0;
  const examples = [];
  for (let i = 0; i < n; i++) {
    let ol = o[i];
    if (ol.startsWith('R!')) { nonParse++; ol = 'R'; }
    if (ol === 'R') rej++; else acc++;
    if (ol !== r[i]) {
      diff++;
      if (examples.length < 5) examples.push({ line: i + 1, input: inputs[i].slice(0, 200), oracle: o[i].slice(0, 300), rust: r[i].slice(0, 300) });
    }
  }
  if (o.length !== r.length) { diff += Math.abs(o.length - r.length); console.log(c + ': output length mismatch', o.length, r.length); }
  rows.push({ class: c, inputs: n, accepted: acc, rejected: rej, diffs: diff, nonParseErrors: nonParse });
  total += n; totalDiff += diff; totalAcc += acc; totalRej += rej; nonParseErrors += nonParse;
  for (const e of examples) console.log('DIFF in ' + c, JSON.stringify(e));
  const cov = JSON.parse(fs.readFileSync(covFile, 'utf8'));
  cov.forEach((v, k) => { covTotal[k] = (covTotal[k] || 0) + v; });
  if (c === 'derived') derivedCov = cov;
}

console.table(rows);
console.log(`total inputs ${total}: accepted ${totalAcc}, rejected ${totalRej}, differences ${totalDiff}` +
  (nonParseErrors ? `, non-parse exceptions in oracle ${nonParseErrors}` : ''));

// Production coverage: jison's reductions while parsing the grammar-derived inputs.
// Index 0 is unused ($accept has no action).
const nProd = derivedCov.length - 1;
const missing = [];
for (let k = 1; k <= nProd; k++) if (!derivedCov[k]) missing.push(k);
const gmeta = JSON.parse(fs.readFileSync(path.join(OUT, 'derived.meta.json'), 'utf8'));
console.log(`productions: ${nProd} (generator read ${gmeta.productions}); hit by derived inputs: ${nProd - missing.length}; ` +
  `min reductions of any production in derived: ${Math.min(...derivedCov.slice(1))}`);
const missingAll = [];
for (let k = 1; k <= nProd; k++) if (!covTotal[k]) missingAll.push(k);
console.log(`hit by all classes: ${nProd - missingAll.length}`);
let ok = totalDiff === 0;
if (nProd !== gmeta.productions) { console.log('production count mismatch between jison and the generator'); ok = false; }
if (missing.length) { console.log('productions never hit by derived inputs:', missing.join(' ')); ok = false; }
const derivedRow = rows.find(x => x.class === 'derived');
console.log(`derived inputs accepted by jison: ${derivedRow.accepted} of ${derivedRow.inputs}`);
process.exit(ok ? 0 : 1);
