#!/usr/bin/env node
// Writes tests/golden/corpus.tsv.gz: the parity corpus as the native reference test reads it,
// so that `cargo test` needs neither a browser nor Node. One fragment a line:
//
//   id  group  class  font px  colour  13 data-* values (DATA_ATTRIBUTES order)  text
//
// with backslash, tab, newline and carriage return escaped in the last column.
//
//   node tests/golden/mkcorpus.mjs [tests/parity/corpus/corpus.json]
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const src = process.argv[2] ?? path.join(HERE, '../parity/corpus/corpus.json');
const corpus = JSON.parse(fs.readFileSync(src, 'utf8'));
export const ATTRS = ['fontsize', 'signcolor', 'dir', 'linesize', 'sep', 'separated', 'type', 'bracketcolor', 'shadepattern', 'align', 'border', 'standalone', 'log'];
const esc = (s) => s.replace(/\\/g, '\\\\').replace(/\t/g, '\\t').replace(/\n/g, '\\n').replace(/\r/g, '\\r');
const lines = corpus.items.map((it) => {
  const attrs = ATTRS.map((a) => esc(it.attrs['data-' + a] ?? ''));
  return [it.id, it.group, it.class, String(parseFloat(it.fontSize)), it.color, ...attrs, esc(it.text)].join('\t');
});
const text = lines.join('\n') + '\n';
// mtime 0 in the gzip header (the default of zlib's gzip), so the file is reproducible.
const gz = zlib.gzipSync(text, { level: 9 });
fs.writeFileSync(path.join(HERE, 'corpus.tsv.gz'), gz);
console.log(`${lines.length} fragments, ${text.length} chars, ${gz.length} bytes gzipped; corpus sha256 ${corpus.sha256}`);
