#!/usr/bin/env node
// Run the corpus through one implementation in one engine.
//
//   node run.mjs --impl upstream|<dir or .js> --engine chromium|firefox|webkit --run-id X
//                [--prefs '{"privacy.resistFingerprinting":true}'] [--shuffle SEED]
//                [--out file.json[.gz]] [--group texts,matrix] [--limit N] [--batch 200]
//                [--ready 'js expression']
//
// --impl upstream serves docs/hierojax.js. Any other value is a directory containing
// hierojax.js (and whatever it loads, such as a .wasm), or a single .js file; it is served
// at /hierojax.js into the same page template with the same stylesheet and font.
import { parseArgs } from 'node:util';
import { runCorpus, defaultOut, writeJson, READY_DEFAULT } from './lib/runner.mjs';

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
  },
});
if (!v.engine) {
  console.error('usage: node run.mjs --engine chromium|firefox|webkit [--impl upstream|path] [--run-id X] [--prefs json] [--shuffle seed]');
  process.exit(2);
}
const groups = v.group ? new Set(v.group.split(',')) : null;
const { meta, results } = await runCorpus({
  impl: v.impl,
  engine: v.engine,
  runId: v['run-id'],
  prefs: v.prefs ? JSON.parse(v.prefs) : undefined,
  shuffle: v.shuffle != null ? Number(v.shuffle) : undefined,
  batchSize: Number(v.batch),
  ready: v.ready,
  filter: groups ? (it) => groups.has(it.group) : undefined,
  limit: v.limit ? Number(v.limit) : undefined,
});
const out = v.out ?? defaultOut(meta);
writeJson(out, { meta, results });
const n = Object.keys(results).length;
const errs = Object.values(results).filter((r) => r.err || r.crash).length;
console.log(`${meta.impl} ${meta.engine} ${meta.engineVersion} run ${meta.runId}: ${n} fragments, ${errs} threw or crashed, ${meta.seconds} s -> ${out}`);
if (meta.crashes.length) console.log(`crashed fragments: ${meta.crashes.length}`);
