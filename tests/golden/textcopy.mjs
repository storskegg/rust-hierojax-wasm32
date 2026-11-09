#!/usr/bin/env node
// What consistent mode's text printer copies, against the source, and what a change of it moved.
//
//   cargo run --release -p hieroxide-outline --example fragments -- consistent new.tsv
//   node tests/golden/textcopy.mjs new.tsv                  # the copied text against the source
//   node tests/golden/textcopy.mjs new.tsv --old old.tsv    # and: only hidden text differs from old.tsv
//
// The copied text of a fragment is the text of its hidden `tspan`s and of its selectable sign `tspan`s, in
// document order (what a selection of the element gives, before the browser's line breaks). The visible
// part of a fragment is its markup with the text of the hidden `tspan`s taken out: it must be the same in
// both files, byte for byte, for every fragment.
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const args = process.argv.slice(2);
const files = args.filter((a, i) => !a.startsWith('--') && args[i - 1] !== '--old');
const oldFile = args.includes('--old') ? args[args.indexOf('--old') + 1] : null;
const unesc = (s) => s.replace(/\\(.)/g, (_, c) => ({ t: '\t', n: '\n', r: '\r' })[c] ?? c);
const read = (f) => new Map(fs.readFileSync(f, 'utf8').split('\n').filter(Boolean).map((l) => { const c = l.split('\t'); return [c[0], { status: c[1], markup: unesc(c.slice(2).join('\t')) }]; }));
const corpus = new Map(zlib.gunzipSync(fs.readFileSync(path.join(ROOT, 'tests/golden/corpus.tsv.gz'))).toString('utf8').split('\n').filter(Boolean).map((l) => { const c = l.split('\t'); return [c[0], unesc(c[18])]; }));
const xml = (s) => s.replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&amp;/g, '&');
const HIDDEN = /<tspan class="hierojax-svg-hidden">([^<]*)<\/tspan>/g;
const COPIED = /<tspan class="(?:hierojax-svg-hidden|hierojax-svg-sign)"[^>]*>([^<]*)<\/tspan>/g;
const copied = (m) => [...m.matchAll(COPIED)].map((x) => xml(x[1])).join('');
const visible = (m) => m.replace(HIDDEN, '<tspan class="hierojax-svg-hidden"></tspan>');

const cur = read(files[0]);
let ok = 0, same = 0, differ = 0;
const examples = [];
for (const [id, r] of cur) {
  if (r.status !== 'ok') continue;
  ok++;
  const c = copied(r.markup), src = corpus.get(id);
  if (c === src) same++;
  else { differ++; if (examples.length < 5) examples.push([id, [...src].map((x) => x.codePointAt(0).toString(16)).join(' '), [...c].map((x) => x.codePointAt(0).toString(16)).join(' ')]); }
}
console.log(`${files[0]}: ${ok} fragments laid out; copied text equals the source in ${same}, differs in ${differ}`);
for (const e of examples) console.log('  ', e[0], 'source', e[1], 'copied', e[2]);

if (oldFile) {
  const old = read(oldFile);
  let identical = 0, hiddenOnly = 0, other = 0, statusChanged = 0;
  const bad = [];
  for (const [id, r] of cur) {
    const o = old.get(id);
    if (!o || o.status !== r.status) { statusChanged++; continue; }
    if (o.markup === r.markup) identical++;
    else if (visible(o.markup) === visible(r.markup)) hiddenOnly++;
    else { other++; if (bad.length < 5) bad.push(id); }
  }
  console.log(`against ${oldFile}: ${identical} identical, ${hiddenOnly} differ in hidden text only, ${other} differ elsewhere, ${statusChanged} changed status`);
  if (other || statusChanged) { console.log('  e.g.', bad.join(' ')); process.exit(1); }
}
