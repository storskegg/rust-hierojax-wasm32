#!/usr/bin/env node
// Run the corpus through one implementation in one engine.
//
//   node run.mjs --impl upstream|<dir or .js> --engine chromium|firefox|webkit --run-id X
//                [--prefs '{"privacy.resistFingerprinting":true}'] [--shuffle SEED]
//                [--out file.json[.gz]] [--group texts,matrix] [--limit N] [--batch 200]
//                [--ready 'js expression'] [--covered-by run.json.gz]
//
// --covered-by keeps only the fragments that run did not mark `unsupported`, in corpus
// order. Running upstream and a replacement both this way gives them the same fragments in
// the same order, which matters because upstream's measurement memo depends on order.
//
// --impl upstream serves docs/hierojax.js. Any other value is a directory containing
// hierojax.js (and whatever it loads, such as a .wasm), or a single .js file; it is served
// at /hierojax.js into the same page template with the same stylesheet and font.
import { parseArgs } from 'node:util';
import { runCorpus, defaultOut, writeJson, readJson, READY_DEFAULT } from './lib/runner.mjs';

const { values: v } = parseArgs({
  options: {
    impl: { type: 'string', default: 'upstream' },
    engine: { type: 'string' },
    'run-id': { type: 'string', default: 'run' },
    prefs: { type: 'string' },
    shuffle: { type: 'string' },
    out: { type: 'string' },
    group: { type: 'string' },
    limit: { type: 'string' },
    batch: { type: 'string', default: '200' },
    ready: { type: 'string', default: READY_DEFAULT },
    'covered-by': { type: 'string' },
  },
});
if (!v.engine) {
  console.error('usage: node run.mjs --engine chromium|firefox|webkit [--impl upstream|path] [--run-id X] [--prefs json] [--shuffle seed]');
  process.exit(2);
}
const groups = v.group ? new Set(v.group.split(',')) : null;
let covered = null;
if (v['covered-by']) {
  const prior = readJson(v['covered-by']).results;
  covered = new Set(Object.keys(prior).filter((id) => !prior[id].unsupported));
}
const { meta, results } = await runCorpus({
  impl: v.impl,
  engine: v.engine,
  runId: v['run-id'],
  prefs: v.prefs ? JSON.parse(v.prefs) : undefined,
  shuffle: v.shuffle != null ? Number(v.shuffle) : undefined,
  batchSize: Number(v.batch),
  ready: v.ready,
  filter: groups || covered ? (it) => (!groups || groups.has(it.group)) && (!covered || covered.has(it.id)) : undefined,
  limit: v.limit ? Number(v.limit) : undefined,
});
const out = v.out ?? defaultOut(meta);
writeJson(out, { meta, results });
const n = Object.keys(results).length;
const errs = Object.values(results).filter((r) => r.err || r.crash).length;
console.log(`${meta.impl} ${meta.engine} ${meta.engineVersion} run ${meta.runId}: ${n} fragments, ${errs} threw or crashed, ${meta.seconds} s -> ${out}`);
if (meta.crashes.length) console.log(`crashed fragments: ${meta.crashes.length}`);
