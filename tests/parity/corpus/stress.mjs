#!/usr/bin/env node
// A second corpus, for what the real one does not reach: fragments with insertions, enclosures,
// overlays and ligatures under options outside any sensible range (negative and large
// `data-sep`, font sizes of 0, NaN and fractions, an unknown `data-dir`), the insertion
// alternatives whose glyph has no ligature entry, empty enclosures in every position, deep
// nesting, and mirrored or rotated cores. Upstream's numbers go NaN and negative here, and its
// own code throws; the port must do all of that in the same way. It is not part of the gate's
// manifest (a change upstream of how a nonsense option behaves is not news).
//
//   node corpus/stress.mjs                 writes out/stress.json
//   node run.mjs --engine chromium --corpus out/stress.json --run-id X
//   node run.mjs --engine chromium --corpus out/stress.json --impl ../../build/compat --run-id X
//   node compare.mjs out/upstream-chromium-X.json.gz out/compat-chromium-X.json.gz --ignore-parse-error-text
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadUpstream } from './tables.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const up = loadUpstream(path.join(HERE, '../../../docs/hierojax.js'));
const corpus = JSON.parse(fs.readFileSync(path.join(HERE, 'corpus.json'), 'utf8'));

const PLACE_CH = { ts: '\u{13432}', bs: '\u{13433}', te: '\u{13434}', be: '\u{13435}', m: '\u{13439}', t: '\u{1343A}', b: '\u{1343B}' };
const OVERLAY = '\u{13436}', HOR = '\u{13431}', VER = '\u{13430}', BSEG = '\u{13437}', ESEG = '\u{13438}';
const BENC = '\u{1343C}', EENC = '\u{1343D}', BWALL = '\u{1343E}', EWALL = '\u{1343F}';
const OPEN = '\u{13258}', CLOSE = '\u{1325B}', MIRROR = '\u{13440}';
const MAN = '\u{13000}', OWL = '\u{13153}', BLANK = '\u{13441}', LOST = '\u{13443}';

const OPTION_SETS = [
  { 'data-sep': '-0.01' },
  { 'data-sep': '-0.1' },
  { 'data-sep': '-0.5' },
  { 'data-sep': '-3' },
  { 'data-sep': '1' },
  { 'data-sep': '3' },
  { 'data-sep': '1e-9' },
  { 'data-fontsize': '0' },
  { 'data-fontsize': 'x' },
  { 'data-fontsize': '-5' },
  { 'data-fontsize': '1' },
  { 'data-fontsize': '0.5' },
  { 'data-fontsize': '150' },
  { 'data-linesize': '-1' },
  { 'data-linesize': '0.01' },
  { 'data-linesize': '100' },
  { 'data-dir': 'foo' },
  { 'data-dir': 'vrl', 'data-sep': '-0.1' },
  { 'data-dir': 'vlr', 'data-linesize': '0.2' },
  { 'data-dir': 'hrl', 'data-align': 'bottom' },
  { 'data-dir': 'foo', 'data-separated': 'true' },
  { 'data-separated': 'true', 'data-shadepattern': 'hatching', 'data-log': 'true' },
];

const items = [];
let n = 0;
const add = (group, text, attrs = {}, fontSize = '36px') =>
  items.push({ id: `stress-${String(n++).padStart(5, '0')}`, group, class: 'stress', src: group, text, attrs: { 'data-type': 'svg', ...attrs }, fontSize, color: 'rgb(0, 0, 0)' });

// Every overlay ligature in every place set of its insertion alternatives, both ways round.
for (const [ch, lig] of Object.entries(up.ligatures)) {
  if (lig.type !== 'overlay' || lig.alt) continue;
  const side = (signs, sep) =>
    signs.length > 1 ? BSEG + signs.map((s) => s.ch + (s.mirror ? MIRROR : '')).join(sep) + ESEG : signs[0].ch + (signs[0].mirror ? MIRROR : '');
  const a = side(lig.horizontal, HOR), b = side(lig.vertical, VER);
  add('overlays', a + OVERLAY + b);
  if (lig.horizontal.length === 1 && lig.vertical.length === 1) add('overlays', b + OVERLAY + a);
  for (const alt of up.insertions[ch] ?? []) {
    const places = Object.keys(alt).filter((p) => p in PLACE_CH);
    add('overlays', a + OVERLAY + b + places.map((p) => PLACE_CH[p] + MAN).join(''));
    add('overlays', b + OVERLAY + a + places.map((p) => PLACE_CH[p] + OWL).join(''));
    add('overlays', a + OVERLAY + b + places.map((p) => PLACE_CH[p] + MAN).join(''), { 'data-dir': 'vlr' });
  }
}

// An empty enclosure, and one with a blank, wherever a group can go.
const empties = [BENC + EENC, BWALL + EWALL, OPEN + BENC + EENC + CLOSE, BENC + BLANK + EENC, OPEN + BENC + BLANK + EENC + CLOSE, BENC + LOST + EENC];
for (const e of empties) {
  for (const t of [
    e,
    MAN + PLACE_CH.ts + e,
    MAN + PLACE_CH.m + e,
    MAN + MIRROR + PLACE_CH.te + e,
    MAN + HOR + e,
    MAN + VER + e,
    OPEN + BENC + e + EENC + CLOSE,
    BENC + MAN + PLACE_CH.bs + e + EENC,
    MAN + PLACE_CH.ts + BLANK,
    MAN + PLACE_CH.ts + LOST,
    MAN + PLACE_CH.ts + BSEG + MAN + PLACE_CH.bs + e + ESEG,
  ]) {
    for (const dir of ['hlr', 'vlr', 'foo']) add('empties', t, { 'data-dir': dir });
  }
}

// Depth: the nesting that measure-printing makes expensive, within what a browser does quickly.
for (const depth of [2, 3, 4, 6, 9, 14, 32, 64, 128, 256]) {
  let ins = MAN + PLACE_CH.ts + MAN, encl = MAN, both = MAN + PLACE_CH.ts + MAN;
  for (let i = 0; i < depth; i++) {
    ins = MAN + PLACE_CH.ts + BSEG + ins + ESEG;
    encl = OPEN + BENC + encl + EENC + CLOSE;
    both = i % 2 ? OPEN + BENC + both + EENC + CLOSE : MAN + PLACE_CH.m + (both.startsWith(OPEN) ? both : BSEG + both + ESEG);
  }
  for (const dir of ['hlr', 'vrl']) {
    add('deep', ins, { 'data-dir': dir });
    add('deep', encl, { 'data-dir': dir });
    add('deep', both, { 'data-dir': dir });
  }
}

// Long rows in insertions and enclosures: canvases far wider than a sign.
for (const k of [20, 80, 300]) {
  const row = Array(k).fill(MAN).join('');
  const hor = Array(k).fill(MAN).join(HOR);
  add('wide', BENC + row + EENC);
  add('wide', OPEN + BENC + row + EENC + CLOSE);
  add('wide', MAN + PLACE_CH.bs + BSEG + hor + ESEG);
  add('wide', OPEN + BENC + row + EENC + CLOSE, { 'data-dir': 'vlr' });
}

// A seeded sample of the real corpus's fragments that use group pixels, under each odd option set.
let seed = 0x5eed1234;
const rnd = () => {
  seed ^= seed << 13;
  seed ^= seed >>> 17;
  seed ^= seed << 5;
  return (seed >>> 0) / 4294967296;
};
const pixels = corpus.items.filter((it) => ['insertion', 'enclosure', 'overlay', 'bracket'].includes(it.class) && it.attrs['data-type'] !== 'canvas' && it.attrs['data-type'] !== 'dom');
for (const o of OPTION_SETS) {
  for (let i = 0; i < 150; i++) {
    const it = pixels[Math.floor(rnd() * pixels.length)];
    add('options', it.text, { ...it.attrs, ...o }, it.fontSize);
  }
}

const out = path.join(HERE, '../out/stress.json');
fs.mkdirSync(path.dirname(out), { recursive: true });
const body = JSON.stringify(items);
fs.writeFileSync(out, JSON.stringify({ sha256: crypto.createHash('sha256').update(body).digest('hex'), items }));
const byGroup = {};
for (const it of items) byGroup[it.group] = (byGroup[it.group] ?? 0) + 1;
console.log(`${items.length} fragments -> ${out}`, byGroup);
