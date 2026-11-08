// Writes fixtures.txt from a finished differential run (out/*.txt and out/*.oracle):
// a deterministic sample of short cases from every class, with the oracle's answer.
//
// Usage: node run.mjs && node mkfixtures.mjs

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const OUT = path.join(HERE, 'out');
// class: [accepted quota, rejected quota]
const quota = { docs: [60, 0], random: [60, 60], mutation: [100, 100], derived: [240, 0], edge: [120, 60] };
const seen = new Set();
const lines = [];
for (const [c, [qa, qr]] of Object.entries(quota)) {
  const inputs = fs.readFileSync(path.join(OUT, c + '.txt'), 'utf8').split('\n');
  const outputs = fs.readFileSync(path.join(OUT, c + '.oracle'), 'utf8').split('\n');
  let a = 0, r = 0;
  for (let i = 0; i < inputs.length - 1 && (a < qa || r < qr); i += 1) {
    if (c === 'docs' && i % 37 !== 0) continue;
    const input = inputs[i], out = outputs[i];
    if (input.length > 4 * 70 || out.length > 700 || seen.has(input)) continue;
    // Spread over the file rather than taking the first ones.
    if ((i * 2654435761) % 7 !== 0 && c !== 'edge' && c !== 'docs') continue;
    const acc = out.startsWith('A');
    if (acc ? a >= qa : r >= qr) continue;
    seen.add(input);
    if (acc) a++; else r++;
    lines.push(input + '\t' + (out.startsWith('R') ? 'R' : out));
  }
}
fs.writeFileSync(path.join(HERE, 'fixtures.txt'), lines.join('\n') + '\n');
console.log('fixtures:', lines.length, 'bytes:', fs.statSync(path.join(HERE, 'fixtures.txt')).size);
