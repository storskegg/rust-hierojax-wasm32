#!/usr/bin/env node
// How many bytes a block costs, on a line like the site's and on a large text, with and without the
// copyable text, raw and compressed (gzip -9 and brotli -q 11).
//
//   cargo build --release -p hieroxide-cli
//   node tests/paths/bytes.mjs [--json out.json]
import fs from 'node:fs';
import zlib from 'node:zlib';
import { parseArgs } from 'node:util';
import { loadCorpus, renderBatch } from './lib.mjs';

const { values: v } = parseArgs({ options: { json: { type: 'string' } } });
const gz = (s) => zlib.gzipSync(Buffer.from(s), { level: 9 }).length;
const br = (s) => zlib.brotliCompressSync(Buffer.from(s), { params: { [zlib.constants.BROTLI_PARAM_QUALITY]: 11, [zlib.constants.BROTLI_PARAM_LGWIN]: 24, [zlib.constants.BROTLI_PARAM_SIZE_HINT]: Buffer.byteLength(s) } }).length;

const corpus = loadCorpus();
const site = corpus.filter((f) => f.group === 'site').map((f) => f.text);
const texts = corpus.filter((f) => f.group === 'texts');
const large = texts.reduce((a, b) => ([...b.text].length > [...a.text].length ? b : a));
const cases = [
  ['site line (2), 33 signs', site[2]],
  ['site line (1)', site[0]],
  ['a text, 100 signs', texts.find((f) => [...f.text].length > 160 && [...f.text].length < 200)?.text ?? site[2]],
  [`large text (${large.id}, ${[...large.text].length} characters)`, large.text],
];
const kinds = [
  ['paths, overlay + data-text (default)', { kind: 'paths' }],
  ['paths, overlay, no data-text', { kind: 'paths', dataText: false }],
  ['paths, text inside the svg + data-text', { kind: 'paths', selectable: 'svg' }],
  ['paths, no copyable text, data-text', { kind: 'paths', selectable: 'none' }],
  ['paths, no copyable text, no data-text', { kind: 'paths', selectable: 'none', dataText: false }],
  ['paths, only text inside the svg (no data-text)', { kind: 'paths', selectable: 'svg', dataText: false }],
  ['text (consistent mode, needs the font and CSS)', { kind: 'text' }],
];
const rows = [];
for (const [name, text] of cases) {
  const out = renderBatch(kinds.map(([, o]) => ({ text, ...o })));
  const base = out[kinds.findIndex(([n]) => n.startsWith('paths, no copyable text, no data-text'))].svg;
  kinds.forEach(([k], i) => {
    const svg = out[i].svg;
    rows.push({ text: name, variant: k, raw: Buffer.byteLength(svg), gzip: gz(svg), brotli: br(svg), rawVsBare: Buffer.byteLength(svg) - Buffer.byteLength(base) });
  });
}
console.table(rows);
// The same line twice or many times in one page: repeated signs are text a compressor finds.
const lineBlocks = renderBatch(site.map((t) => ({ text: t })));
const page = lineBlocks.map((b) => `<p>${b.svg}</p>`).join('\n');
console.log(`a page of the site's ${site.length} lines: ${Buffer.byteLength(page)} raw, ${gz(page)} gzip, ${br(page)} brotli`);
if (v.json) fs.writeFileSync(v.json, JSON.stringify(rows, null, 1));
