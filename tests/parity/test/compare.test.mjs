import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { loadAllowlist } from '../compare.mjs';

const tmp = (obj) => {
  const f = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'allow-')), 'allowlist.json');
  fs.writeFileSync(f, JSON.stringify(obj));
  return f;
};

test('the committed allowlist is valid and empty', () => {
  const here = path.dirname(new URL(import.meta.url).pathname);
  assert.deepEqual(loadAllowlist(path.join(here, '..', 'allowlist.json')), []);
});

test('allowlist entries need a reason and something to cover', () => {
  assert.throws(() => loadAllowlist(tmp([{ id: 'plain-00001' }])), /reason/);
  assert.throws(() => loadAllowlist(tmp([{ id: 'plain-00001', reason: '  ' }])), /reason/);
  assert.throws(() => loadAllowlist(tmp([{ reason: 'because' }])), /"id", "group" or "class"/);
  assert.throws(() => loadAllowlist(tmp([{ id: 'x', reason: 'r', fild: 1 }])), /unknown key/);
  assert.throws(() => loadAllowlist(tmp({})), /array/);
  assert.equal(loadAllowlist(tmp([{ group: 'signs', engine: 'webkit', fields: ['html'], reason: 'known WebKit raster difference' }])).length, 1);
});
