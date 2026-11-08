#!/usr/bin/env node
// A fragment nested to the parser's limit, laid out by the wasm build in real browsers.
//
//   node tests/stack/browsers.mjs [--impl build/compat] [--engines chromium,firefox,webkit]
//
// The shadow stack of the module (its own 1 MiB in linear memory) is measured by
// tools/stack-probe.mjs; this checks the other half, that the engines' own call stacks
// cope with the depth of recursion, by running 64, 128, 192, 256 and 257 levels (the last is
// a parse error by design) through the parity harness's page.
import path from 'node:path';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';
import { runCorpus } from '../parity/lib/runner.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const { values: v } = parseArgs({
  options: {
    impl: { type: 'string', default: path.join(ROOT, 'build/compat') },
    engines: { type: 'string', default: 'chromium,firefox,webkit' },
  },
});

const MAN = '\u{13000}', VER = '\u{13430}', HOR = '\u{13431}';
function nested(depth) {
  let s = MAN + VER + MAN;
  for (let i = 0; i < depth; i++) s = MAN + VER + MAN + HOR + '\u{13437}' + s + '\u{13438}';
  return s;
}

const depths = [64, 128, 192, 256, 257];
const items = depths.map((d) => ({ id: `nest-${d}`, text: nested(d), attrs: {}, fontSize: '22px', color: 'rgb(0, 0, 0)', group: 'stack', class: 'plain' }));
let bad = 0;
for (const engine of v.engines.split(',')) {
  const { results } = await runCorpus({ impl: v.impl, engine, runId: 'stack', corpus: { items, sha256: 'stack' }, batchSize: 1, log: () => {} });
  const line = depths.map((d) => {
    const r = results[`nest-${d}`];
    const ok = r && !r.err && !r.crash && (d > 256 ? /parse error/.test(r.text) : /<svg:svg/.test(r.html));
    if (!ok) bad++;
    return `${d}:${ok ? 'ok' : 'FAIL ' + (r?.err ?? r?.crash ?? 'no svg')}`;
  });
  console.log(`${engine.padEnd(9)} ${line.join('  ')}`);
}
process.exit(bad ? 1 : 0);
