#!/usr/bin/env node
// Prints a table from `run.mjs --covered --out` results.
//
//   node table.mjs label=file.json [label=file.json ...]
//
// Per row (median of the loads in each file, ms): total (the processFragmentsNow call),
// parse, format+fit, print (without format), other, canvas operations, the layout still owed
// when the call returns, total + that layout, and the throttled Chromium total when present.
import fs from 'node:fs';

const rows = [['config', 'total', 'parse', 'fmt+fit', 'print', 'other', 'canvas', 'layoutAfter', 'total+layout', 'spread', 'throttled4x']];
for (const arg of process.argv.slice(2)) {
  const i = arg.indexOf('=');
  const label = arg.slice(0, i);
  const r = JSON.parse(fs.readFileSync(arg.slice(i + 1), 'utf8'));
  const p = r.pages.covered;
  const l = p.light;
  const f = (x) => (x == null ? '-' : String(Math.round(x)));
  rows.push([
    label, f(l.totalMs), f(l.parseMs), f(l.formatFitMs), f(l.printExclFormatMs), f(l.otherMs),
    f(p.heavy.canvasOpsMs), f(l.layoutAfterMs), f(l.totalWithLayoutMs), f(l.spreadMs),
    r.cpuThrottle4x ? f(r.cpuThrottle4x.pages.covered.totalMs) : '-',
  ]);
}
const w = rows[0].map((_, c) => Math.max(...rows.map((r) => r[c].length)));
for (const r of rows) console.log(r.map((x, c) => (c ? x.padStart(w[c]) : x.padEnd(w[c]))).join('  '));
