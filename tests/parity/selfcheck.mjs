#!/usr/bin/env node
// The harness gate: upstream against upstream.
//
//   node selfcheck.mjs [--engines chromium,firefox,webkit] [--hardened] [--no-run]
//
// For each engine: two fresh runs in corpus order (run A and B; the gate is 0
// differences), and one run in a seeded shuffled order (S), compared with A and reported
// but not gated, because upstream's measurement memo keeps the first value for a key
// rounded to two decimals, so results can depend on fragment order.
// --hardened adds Firefox with privacy.resistFingerprinting=true (runs H1, H2), compared
// with itself and with normal Firefox; recorded, not gated.
// --no-run compares existing files in out/ instead of running.
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { runCorpus, writeJson, readJson, OUT_DIR } from './lib/runner.mjs';
import { compareRuns, formatReport, loadAllowlist } from './compare.mjs';
import { ENGINE_NAMES } from './lib/engines.mjs';

const { values: v } = parseArgs({
  options: {
    engines: { type: 'string', default: ENGINE_NAMES.join(',') },
    hardened: { type: 'boolean' },
    'hardened-only': { type: 'boolean' },
    'no-run': { type: 'boolean' },
    shuffle: { type: 'string', default: '1' },
  },
});
const HERE = path.dirname(new URL(import.meta.url).pathname);
const allowlist = loadAllowlist(path.join(HERE, 'allowlist.json'));
const file = (engine, id) => path.join(OUT_DIR, `upstream-${engine}-${id}.json.gz`);
const engines = v['hardened-only'] ? [] : v.engines.split(',');
const RFP = { 'privacy.resistFingerprinting': true };

async function run(engine, id, extra = {}) {
  if (v['no-run']) return;
  const { meta, results } = await runCorpus({ impl: 'upstream', engine, runId: id, ...extra });
  writeJson(file(engine, id), { meta, results });
  console.log(`ran ${engine} ${id}: ${meta.fragments} fragments in ${meta.seconds} s${meta.crashes.length ? `, ${meta.crashes.length} crashed` : ''}${meta.pageLog.length ? `, ${meta.pageLog.length} distinct page log lines` : ''}`);
}

const summary = { date: new Date().toISOString(), gate: {}, shuffled: {}, hardened: null };
let gateFailed = false;

for (const engine of engines) {
  await run(engine, 'A');
  await run(engine, 'B');
  await run(engine, 'S', { shuffle: Number(v.shuffle) });
  const A = readJson(file(engine, 'A')), B = readJson(file(engine, 'B')), S = readJson(file(engine, 'S'));
  const gate = compareRuns(A, B, { allowlist });
  console.log(`\n=== ${engine}: run A vs run B (gate) ===\n${formatReport(gate, A, B)}`);
  summary.gate[engine] = { total: gate.total, differing: gate.differing, open: gate.open, maxNum: gate.maxNum, version: A.meta.engineVersion };
  if (gate.open) gateFailed = true;
  const sh = compareRuns(A, S, { allowlist: [] });
  console.log(`\n=== ${engine}: run A vs shuffled run S (reported, not gated) ===\n${formatReport(sh, A, S)}`);
  summary.shuffled[engine] = {
    total: sh.total,
    differing: sh.differing,
    maxNum: sh.maxNum,
    byClass: Object.fromEntries(Object.entries(sh.classes).map(([k, s]) => [k, s.n - s.same])),
    byGroup: Object.fromEntries(Object.entries(sh.groups).map(([k, s]) => [k, s.n - s.same])),
  };
}

if (v.hardened || v['hardened-only']) {
  await run('firefox', 'H1', { prefs: RFP });
  await run('firefox', 'H2', { prefs: RFP });
  const H1 = readJson(file('firefox', 'H1')), H2 = readJson(file('firefox', 'H2'));
  const self = compareRuns(H1, H2, { allowlist: [] });
  console.log(`\n=== hardened firefox (resistFingerprinting): H1 vs H2 ===\n${formatReport(self, H1, H2, { show: 3 })}`);
  summary.hardened = { versus_self: { total: self.total, differing: self.differing, maxNum: self.maxNum } };
  if (fs.existsSync(file('firefox', 'A'))) {
    const A = readJson(file('firefox', 'A'));
    const vs = compareRuns(A, H1, { allowlist: [] });
    console.log(`\n=== normal firefox A vs hardened H1 ===\n${formatReport(vs, A, H1, { show: 3 })}`);
    summary.hardened.versus_normal = { total: vs.total, differing: vs.differing, maxNum: vs.maxNum, byClass: Object.fromEntries(Object.entries(vs.classes).map(([k, s]) => [k, s.n - s.same])) };
    summary.hardened.errors = {
      normal: Object.values(A.results).filter((r) => r.err || (r.console?.length ?? 0) > 0).length,
      hardened: Object.values(H1.results).filter((r) => r.err || (r.console?.length ?? 0) > 0).length,
    };
  }
}

fs.mkdirSync(OUT_DIR, { recursive: true });
fs.writeFileSync(path.join(OUT_DIR, 'selfcheck-summary.json'), JSON.stringify(summary, null, 2));
console.log('\nsummary:\n' + JSON.stringify(summary, null, 2));
console.log(gateFailed ? '\nGATE FAILED: upstream differs from itself' : '\nGATE OK: upstream against upstream shows 0 differences');
process.exit(gateFailed ? 1 : 0);
