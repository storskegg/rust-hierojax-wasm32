#!/usr/bin/env node
// Build the parity corpus: corpus.json (generated, git-ignored) and manifest.json (committed).
//
//   node corpus/build.mjs            write corpus.json, and manifest.json if it is missing
//   node corpus/build.mjs --update   also rewrite manifest.json
//   node corpus/build.mjs --check    rebuild and fail if the result differs from manifest.json
//
// Everything is deterministic: no clock, one fixed seed, and page fragments are read by
// Chromium (pinned through Playwright) with the page scripts that expand them, but with
// hierojax.js replaced by a stub so nothing is rendered. Each fragment carries its real
// data-* attributes and the computed font-size and colour of its context.
//
// Groups: demo, testsuite (the <hierojax-demo> elements, expanded to their 10 variants by
// upstream's own testsuite.js), interactive (the page's defaults), texts (docs/texts/*.html),
// site (the two lines on the site), signs (every sign in the font's cmap), rotsample
// (rotations and mirror on a sample of signs), rotations / ligatures / insertions (every
// entry of upstream's tables), selectors (variation selectors), matrix (options), sizes
// (font sizes), generated (from the grammar).
// Classes (by content, matrix is its own): plain, insertion, enclosure, overlay, bracket, options.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { chromium } from 'playwright';
import { startServer, REPO, DOCS } from '../lib/server.mjs';
import { mulberry32 } from '../lib/runner.mjs';
import { cmapCodepoints } from './cmap.mjs';
import { loadUpstream } from './tables.mjs';
import { readJison, makeGenerator, isSignCp } from './grammar.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const CORPUS = path.join(HERE, 'corpus.json');
const MANIFEST = path.join(HERE, 'manifest.json');

const SEED = 20261108;
const N_GENERATED = 5000;
const N_ROT_SAMPLE = 120;
const SIZES = ['22px', '40px', '33.6px'];

// The two lines on the site (inkhk3.org/src/index.html): text and options, copied.
const SITE_LINES = [
  { text: '𓏌𓐰𓎡𓎛𓂓𓐰𓏝𓀭', attrs: { 'data-standalone': 'true', 'data-type': 'svg' } },
  {
    text: '𓐍𓐱𓊪𓐰𓂋𓈖𓌃𓅱𓏛𓐰𓏥𓍿𓐰𓈖𓐰𓏥𓊏𓐰𓊪𓇳𓈖𓎛𓎡𓄿𓅱𓏛𓐰𓏥𓍿𓐰𓈖𓐰𓏥',
    attrs: { 'data-standalone': 'true', 'data-type': 'svg' },
  },
];

const cp = (n) => String.fromCodePoint(n);
const VS = (rot) => cp(0xfdff + rot); // 1..7
const MIRROR = cp(0x13440);
const PLACE_CHARS = { ts: cp(0x13432), bs: cp(0x13433), te: cp(0x13434), be: cp(0x13435), m: cp(0x13439), t: cp(0x1343a), b: cp(0x1343b) };
const PLACE_ORDER = ['ts', 'bs', 'te', 'be', 'm', 't', 'b'];
const STAND_IN = cp(0x133cf);
const ROT_NUM = { 90: 1, 180: 2, 270: 3, 45: 4, 135: 5, 225: 6, 315: 7 };

const RX = {
  enclosure: /[\u{1343C}-\u{1343F}]/u,
  overlay: /\u{13436}/u,
  insertion: /[\u{13432}-\u{13435}\u{13439}-\u{1343B}]/u,
  bracket: /[[\]{}⟦-⟩⸢⸣\u{13258}-\u{1325D}\u{13282}\u{13286}-\u{13289}\u{13379}-\u{1337B}\u{1342F}]/u,
};
function classify(text) {
  if (RX.enclosure.test(text)) return 'enclosure';
  if (RX.overlay.test(text)) return 'overlay';
  if (RX.insertion.test(text)) return 'insertion';
  if (RX.bracket.test(text)) return 'bracket';
  return 'plain';
}
const len = (t) => [...t].length;

const sha = (data) => crypto.createHash('sha256').update(data).digest('hex');
const fileSha = (f) => sha(fs.readFileSync(f));

// ---- fragments from the upstream pages -------------------------------------------------

const STUB = `function $(id) { return document.getElementById(id); }
window.syntax = { parse: (t) => ({ toString: () => t }) };
window.hierojax = { processFragments() {}, processFragment() {}, processFragmentsNow() {}, processFragmentNow() {} };`;

async function extractPages() {
  const pages = [
    ['demo', 'demo.html'],
    ['testsuite', 'testsuite.html'],
    ['interactive', 'interactive.html'],
    ...fs
      .readdirSync(path.join(DOCS, 'texts'))
      .filter((f) => f.endsWith('.html'))
      .sort()
      .map((f) => ['texts', 'texts/' + f]),
  ];
  const server = await startServer({ stubScript: STUB });
  const browser = await chromium.launch();
  const out = [];
  for (const [group, file] of pages) {
    const page = await browser.newPage();
    page.on('pageerror', (e) => console.error(`  ${file}: page error ${e.message}`));
    await page.goto(`${server.url}/${file}`, { waitUntil: 'load' });
    await page.evaluate(() => document.fonts.ready.then(() => true));
    const frags = await page.evaluate(() =>
      [...document.querySelectorAll('span.hierojax')].map((el) => {
        const cs = getComputedStyle(el);
        const attrs = {};
        for (const a of [...el.attributes].sort((x, y) => (x.name < y.name ? -1 : 1))) {
          if (a.name.startsWith('data-')) attrs[a.name] = a.value;
        }
        return { text: el.textContent, attrs, fontSize: cs.fontSize, color: cs.color };
      }),
    );
    for (const f of frags) out.push({ group, src: file, ...f });
    await page.close();
  }
  await browser.close();
  await server.close();
  return out;
}

// ---- pairwise option sets ---------------------------------------------------------------

const DIMS = [
  ['dir', ['hlr', 'hrl', 'vlr', 'vrl']],
  ['type', ['svg', 'canvas', 'dom']],
  ['sep', [undefined, '0.01', '0.2', '0.5']],
  ['linesize', [undefined, '0.5', '1.3', '2']],
  ['separated', [undefined, 'true', 'false']],
  ['align', [undefined, 'middle', 'bottom']],
  ['border', [undefined, 'true']],
  ['standalone', [undefined, 'true']],
  ['shadepattern', [undefined, 'uniform', 'hatching']],
  ['bracketcolor', [undefined, 'blue', '#cc40ff']],
  ['signcolor', [undefined, 'red', 'rgb(64, 255, 0)', '#0040ff']],
  ['fontsize', [undefined, '28', '33.6']],
  ['log', [undefined, 'true']],
  ['ctxsize', SIZES],
];

function pairwise(rnd) {
  const key = (i, a, j, b) => `${i}:${a}|${j}:${b}`;
  const uncovered = new Set();
  for (let i = 0; i < DIMS.length; i++)
    for (let j = i + 1; j < DIMS.length; j++)
      for (let a = 0; a < DIMS[i][1].length; a++)
        for (let b = 0; b < DIMS[j][1].length; b++) uncovered.add(key(i, a, j, b));
  const rows = [];
  const gain = (row) => {
    let g = 0;
    for (let i = 0; i < row.length; i++) for (let j = i + 1; j < row.length; j++) g += uncovered.has(key(i, row[i], j, row[j])) ? 1 : 0;
    return g;
  };
  while (uncovered.size) {
    let best = null, bestGain = -1;
    const open = [...uncovered];
    for (let c = 0; c < 60; c++) {
      const row = DIMS.map(([, v]) => Math.floor(rnd() * v.length));
      if (c < 30) {
        // seed a candidate from an uncovered pair, so every row covers at least one new pair
        const m = open[Math.floor(rnd() * open.length)].match(/^(\d+):(\d+)\|(\d+):(\d+)$/);
        row[+m[1]] = +m[2];
        row[+m[3]] = +m[4];
      }
      const g = gain(row);
      if (g > bestGain) (best = row), (bestGain = g);
    }
    rows.push(best);
    for (let i = 0; i < best.length; i++) for (let j = i + 1; j < best.length; j++) uncovered.delete(key(i, best[i], j, best[j]));
  }
  return rows.map((r) => Object.fromEntries(DIMS.map(([name, vals], i) => [name, vals[r[i]]])));
}

// Turn an option set into the attributes/context of an item.
function withOptions(base, o) {
  const attrs = { ...base.attrs };
  let color = base.color, fontSize = base.fontSize;
  for (const [k, v] of Object.entries(o)) {
    if (v === undefined) continue;
    if (k === 'signcolor') color = v;
    else if (k === 'ctxsize') fontSize = v;
    else attrs['data-' + k] = v;
  }
  return { ...base, attrs: Object.fromEntries(Object.entries(attrs).sort(([a], [b]) => (a < b ? -1 : 1))), color, fontSize };
}

// ---- build -------------------------------------------------------------------------------

async function build() {
  const up = loadUpstream(path.join(DOCS, 'hierojax.js'));
  const cps = cmapCodepoints(path.join(DOCS, 'NewGardiner.otf'));
  const jison = readJison(path.join(REPO, 'src/syntax.jison'));

  const pageFrags = await extractPages();
  const items = [];
  const add = (group, text, extra = {}) =>
    items.push({
      group,
      src: extra.src ?? group,
      text,
      attrs: extra.attrs ?? {},
      fontSize: extra.fontSize ?? '36px',
      color: extra.color ?? 'rgb(0, 0, 0)',
    });

  for (const f of pageFrags) add(f.group, f.text, f);

  for (const [i, s] of SITE_LINES.entries()) for (const size of ['40px', '33.6px']) add('site', s.text, { src: `site line ${i + 1}`, attrs: s.attrs, fontSize: size });

  // Every sign in the font's cmap: the 4,537 in U+13000..U+143FF plus the private-use
  // rotated forms (the values of Shapes.rotatedChars), their two keys, and U+FFFD.
  const signCps = new Set(cps.filter((c) => c >= 0x13000 && c <= 0x143ff));
  for (const v of Object.values(up.rotatedChars)) signCps.add(v.codePointAt(0));
  for (const k of ['', '', '�']) signCps.add(k.codePointAt(0));
  const signList = [...signCps].filter((c) => cps.includes(c)).sort((a, b) => a - b);
  for (const c of signList) add('signs', cp(c), { fontSize: '40px' });

  // Rotations and mirror on an evenly spaced sample of ordinary signs.
  const ordinary = cps.filter(isSignCp).filter((c) => c <= 0x1342e || c >= 0x13460);
  const sample = Array.from({ length: N_ROT_SAMPLE }, (_, i) => ordinary[Math.floor((i * ordinary.length) / N_ROT_SAMPLE)]);
  for (const c of sample) {
    for (let r = 1; r <= 7; r++) add('rotsample', cp(c) + VS(r), { fontSize: '40px' });
    add('rotsample', cp(c) + MIRROR, { fontSize: '40px' });
    add('rotsample', cp(c) + VS(1) + MIRROR, { fontSize: '40px' });
  }

  // Every rotation table entry at each of its rotations (and mirrored), every ligature
  // alone and mirrored.
  for (const [ch, rots] of Object.entries(up.rotations)) {
    add('rotations', ch, { fontSize: '40px' });
    for (const deg of Object.keys(rots)) {
      add('rotations', ch + VS(ROT_NUM[deg]), { fontSize: '40px' });
      add('rotations', ch + VS(ROT_NUM[deg]) + MIRROR, { fontSize: '40px' });
    }
  }
  for (const ch of Object.keys(up.ligatures)) {
    add('ligatures', ch, { fontSize: '40px' });
    add('ligatures', ch + MIRROR, { fontSize: '40px' });
  }

  // Every insertion table entry: each allowed place alone, all its places together,
  // and mirrored, with a stand-in sign inserted.
  for (const [ch, alts] of Object.entries(up.insertions)) {
    for (const alt of alts) {
      const rot = alt.rot ? VS(ROT_NUM[alt.rot]) : '';
      const places = PLACE_ORDER.filter((p) => p in alt);
      for (const p of places) add('insertions', ch + rot + PLACE_CHARS[p] + STAND_IN, { fontSize: '40px' });
      const all = ch + rot + places.map((p) => PLACE_CHARS[p] + STAND_IN).join('');
      if (places.length > 1) add('insertions', all, { fontSize: '40px' });
      add('insertions', ch + rot + MIRROR + places.map((p) => PLACE_CHARS[p] + STAND_IN).join(''), { fontSize: '40px' });
    }
  }

  // Each variation selector, on plain, rotatable and placeholder signs, and on its own.
  const rotKeys = Object.keys(up.rotations);
  const selSigns = [cp(0x13000), rotKeys[0], rotKeys[Math.floor(rotKeys.length / 2)], rotKeys[rotKeys.length - 1]];
  for (let r = 1; r <= 7; r++) {
    for (const s of selSigns) add('selectors', s + VS(r), { fontSize: '40px' });
    for (const lost of [0x13443, 0x13444, 0x13445, 0x13446]) add('selectors', cp(lost) + VS(r), { fontSize: '40px' });
    add('selectors', VS(r), { fontSize: '40px' });
  }

  // Representative fragments for the options matrix, chosen from the page fragments.
  const pool = items.filter((i) => ['demo', 'testsuite', 'texts'].includes(i.group));
  const first = (pred, name) => {
    const f = pool.find(pred);
    if (!f) throw new Error('no representative fragment for ' + name);
    return f;
  };
  const reps = [
    first((i) => i.group === 'testsuite' && classify(i.text) === 'plain' && len(i.text) >= 8 && /[\u{13430}\u{13431}]/u.test(i.text), 'plain groups'),
    first((i) => i.group === 'texts' && classify(i.text) === 'plain' && len(i.text) >= 25 && len(i.text) <= 40 && !/[︀-︆]/.test(i.text), 'plain line'),
    first((i) => classify(i.text) === 'bracket' && len(i.text) >= 5 && len(i.text) <= 20, 'brackets'),
    first((i) => i.group === 'testsuite' && classify(i.text) === 'insertion' && len(i.text) <= 8, 'insertion'),
    first((i) => i.group === 'testsuite' && classify(i.text) === 'insertion' && len(i.text) >= 12, 'long insertion'),
    first((i) => i.group === 'testsuite' && classify(i.text) === 'enclosure', 'enclosure'),
    first((i) => i.group === 'testsuite' && classify(i.text) === 'overlay', 'overlay'),
    first((i) => /[\u{13447}-\u{13455}]/u.test(i.text), 'damage and shading'),
    first((i) => /[\u{13441}-\u{13446}]/u.test(i.text), 'placeholders'),
    first((i) => /[︀-︆\u{13440}]/u.test(i.text) && len(i.text) <= 40, 'rotation and mirror'),
    first((i) => len(i.text) >= 100, 'long line'),
  ].map((i) => ({ text: i.text, attrs: {}, fontSize: '36px', color: 'rgb(0, 0, 0)', src: i.src }));

  const rnd = mulberry32(SEED);
  for (const o of pairwise(rnd)) for (const r of reps) add('matrix', r.text, withOptions(r, o));
  // one factor at a time, on every representative fragment
  for (const [name, vals] of DIMS) {
    for (const v of vals) {
      if (v === undefined) continue;
      for (const r of reps) add('matrix', r.text, withOptions(r, { [name]: v }));
    }
  }
  // font sizes crossed with the three output types
  for (const size of SIZES)
    for (const type of ['svg', 'canvas', 'dom'])
      for (const r of reps) add('sizes', r.text, withOptions(r, { ctxsize: size, type }));

  // Grammar-generated fragments: only those upstream's own parser accepts are kept.
  const signs = cps.filter(isSignCp).map(cp);
  const interesting = [...Object.keys(up.insertions), ...Object.keys(up.ligatures), ...Object.keys(up.rotations)];
  const gen = makeGenerator(jison, { rnd: mulberry32(SEED ^ 0x9e3779b9), signs, interesting });
  let produced = 0, accepted = 0;
  const seen = new Set();
  while (accepted < N_GENERATED) {
    const t = gen();
    produced++;
    if (!t || len(t) > 150 || seen.has(t) || !up.accepts(t)) continue;
    seen.add(t);
    accepted++;
    add('generated', t, { fontSize: SIZES[accepted % 3] });
  }

  // classes, ids
  const counters = {};
  for (const it of items) {
    it.class = it.group === 'matrix' ? 'options' : classify(it.text);
    counters[it.group] = (counters[it.group] ?? -1) + 1;
    it.id = `${it.group}-${String(counters[it.group]).padStart(5, '0')}`;
  }
  const ordered = items.map((it) => ({ id: it.id, group: it.group, class: it.class, src: it.src, text: it.text, attrs: it.attrs, fontSize: it.fontSize, color: it.color }));

  const groups = {};
  const classes = {};
  for (const it of ordered) {
    (groups[it.group] ??= []).push(it);
    classes[it.class] = (classes[it.class] ?? 0) + 1;
  }
  const manifest = {
    note: 'Generated by corpus/build.mjs. The corpus itself (corpus.json) is not committed; rebuild it and compare with --check.',
    inputs: {
      'docs/hierojax.js': fileSha(path.join(DOCS, 'hierojax.js')),
      'docs/NewGardiner.otf': fileSha(path.join(DOCS, 'NewGardiner.otf')),
      'src/syntax.jison': fileSha(path.join(REPO, 'src/syntax.jison')),
    },
    seed: SEED,
    generated: { requested: N_GENERATED, produced, rejectedOrDuplicate: produced - accepted },
    total: ordered.length,
    groups: Object.fromEntries(Object.entries(groups).map(([g, v]) => [g, { count: v.length, sha256: sha(JSON.stringify(v)) }])),
    classes: Object.fromEntries(Object.entries(classes).sort()),
  };
  const body = JSON.stringify(ordered);
  const corpus = { sha256: sha(body), items: ordered };
  manifest.sha256 = corpus.sha256;
  return { corpus, manifest };
}

const { values: v } = parseArgs({ options: { update: { type: 'boolean' }, check: { type: 'boolean' } } });
const { corpus, manifest } = await build();
fs.writeFileSync(CORPUS, JSON.stringify(corpus));
const text = JSON.stringify(manifest, null, 2) + '\n';
if (v.check) {
  const old = fs.readFileSync(MANIFEST, 'utf8');
  if (old !== text) {
    console.error('manifest differs from a fresh build:\n' + text);
    process.exit(1);
  }
  console.log(`corpus matches manifest: ${manifest.total} fragments, sha256 ${manifest.sha256.slice(0, 12)}`);
} else {
  if (v.update || !fs.existsSync(MANIFEST)) fs.writeFileSync(MANIFEST, text);
  console.log(`wrote ${CORPUS}: ${manifest.total} fragments, sha256 ${manifest.sha256.slice(0, 12)}`);
  for (const [g, s] of Object.entries(manifest.groups)) console.log(`  group ${g.padEnd(11)} ${s.count}`);
  for (const [c, n] of Object.entries(manifest.classes)) console.log(`  class ${c.padEnd(11)} ${n}`);
  console.log(`  generated: ${manifest.generated.produced} produced, ${manifest.generated.rejectedOrDuplicate} rejected or duplicate`);
}
