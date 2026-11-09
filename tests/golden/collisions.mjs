#!/usr/bin/env node
// Ink collisions and clipping by the SVG box, consistent mode against upstream, over the corpus.
// Both are measured the same way, from the numbers each printed: the example `ink` draws each sign
// alone with exact outlines at 40 px, a pixel being ink when at least half covered. So a difference
// comes from where the signs are, not from any browser's raster.
//
//   node tests/golden/drift.mjs        # for upstream's run at 40 px
//   node tests/golden/collisions.mjs
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import readline from 'node:readline';
import { fileURLToPath } from 'node:url';
import { readJson, loadCorpus } from '../parity/lib/runner.mjs';
import { parseCanonical, parseMarkup } from './lib/signs.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../..');
const OUT = path.join(ROOT, 'tests/parity/out');
const up = readJson(path.join(OUT, 'upstream-chromium-40.json.gz')).results;
const items = loadCorpus().items;
const dump = process.argv[2]; // a `fragments consistent` dump
const ours = new Map(fs.readFileSync(dump, 'utf8').split('\n').filter(Boolean).map((l) => {
  const c = l.split('\t');
  return [c[0], c];
}));
const un = (s) => s.replace(/\\(.)/g, (_, c) => (c === 't' ? '\t' : c === 'n' ? '\n' : c === 'r' ? '\r' : c));

const child = spawn(path.join(ROOT, 'target/release/examples/ink'), [], { stdio: ['pipe', 'pipe', 'inherit'] });
const got = new Map();
const rl = readline.createInterface({ input: child.stdout });
rl.on('line', (l) => {
  const [key, pairs, overlap, clipped, ink] = l.split('\t');
  const [id, who] = key.split('|');
  const e = (got.get(id) ?? got.set(id, {}).get(id));
  const w = (e[who] ??= { pairs: 0, overlap: 0, clipped: 0, ink: 0 });
  w.pairs += Number(pairs);
  w.overlap += Number(overlap);
  w.clipped += Number(clipped);
  w.ink += Number(ink);
});
const send = async (id, who, svgs) => {
  for (const s of svgs) {
    const signs = s.signs.flatMap((g) => {
      const cp = [...g.text][0]?.codePointAt(0);
      if (cp === undefined) return [];
      return [cp, g.fs, g.kind === 'text' ? g.tx : 0, g.kind === 'text' ? g.ty : 0, g.sx, g.sy, g.rot, g.x, g.y];
    });
    const line = [`${id}|${who}`, s.w, s.h, ...signs].join('\t') + '\n';
    if (!child.stdin.write(line)) await new Promise((r) => child.stdin.once('drain', r));
  }
};
const classOf = new Map(items.map((it) => [it.id, it.class]));
for (const it of items) {
  const t = it.attrs['data-type'];
  if (t && t !== 'svg') continue;
  const u = up[it.id], o = ours.get(it.id);
  if (!u.html || u.err || !o || o[1] !== 'ok') continue;
  await send(it.id, 'up', parseCanonical(u.html));
  await send(it.id, 'hx', parseMarkup(un(o[2]), 40));
}
child.stdin.end();
await new Promise((r) => rl.once('close', r));

const table = {};
const worse = [];
for (const [id, e] of got) {
  const c = classOf.get(id);
  for (const k of [c, 'ALL']) {
    const t = (table[k] ??= { n: 0, upColl: 0, hxColl: 0, newColl: 0, goneColl: 0, worseColl: 0, upOverlap: 0, hxOverlap: 0, upClip: 0, hxClip: 0, newClip: 0, clipUpPx: 0, clipHxPx: 0 });
    const [u, h] = [e.up ?? { pairs: 0, overlap: 0, clipped: 0 }, e.hx ?? { pairs: 0, overlap: 0, clipped: 0 }];
    t.n++;
    t.upColl += +(u.pairs > 0);
    t.hxColl += +(h.pairs > 0);
    t.newColl += +(u.pairs === 0 && h.pairs > 0);
    t.goneColl += +(u.pairs > 0 && h.pairs === 0);
    t.worseColl += +(h.overlap > u.overlap + 2);
    t.upOverlap += u.overlap;
    t.hxOverlap += h.overlap;
    t.upClip += +(u.clipped > 0);
    t.hxClip += +(h.clipped > 0);
    t.newClip += +(u.clipped === 0 && h.clipped > 0);
    t.clipUpPx += u.clipped;
    t.clipHxPx += h.clipped;
  }
  const [u, h] = [e.up ?? { overlap: 0, clipped: 0 }, e.hx ?? { overlap: 0, clipped: 0 }];
  worse.push({ id, class: classOf.get(id), overlapMore: h.overlap - u.overlap, clipMore: h.clipped - u.clipped, up: u, hx: h });
}
console.log('class: fragments | any collision upstream, consistent; new in consistent, gone; more than 2 px worse | shared px upstream, consistent | any clipped ink upstream, consistent; new; clipped px upstream, consistent');
for (const [k, t] of Object.entries(table).sort()) {
  console.log(`${k.padEnd(10)} ${String(t.n).padStart(6)} | ${t.upColl}, ${t.hxColl}; +${t.newColl} -${t.goneColl}; ${t.worseColl} | ${t.upOverlap}, ${t.hxOverlap} | ${t.upClip}, ${t.hxClip}; +${t.newClip}; ${t.clipUpPx}, ${t.clipHxPx}`);
}
worse.sort((a, b) => b.overlapMore - a.overlapMore);
console.log('most shared ink more than upstream:', worse.slice(0, 5).map((w) => `${w.id} (${w.class}) +${w.overlapMore}`).join(', '));
worse.sort((a, b) => b.clipMore - a.clipMore);
console.log('most clipped more than upstream:', worse.slice(0, 5).map((w) => `${w.id} (${w.class}) +${w.clipMore}`).join(', '));
fs.writeFileSync(path.join(OUT, 'collisions-40.json'), JSON.stringify({ table, worse }));
