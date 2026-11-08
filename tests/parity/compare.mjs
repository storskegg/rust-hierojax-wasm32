#!/usr/bin/env node
// Compare two runs fragment by fragment.
//
//   node compare.mjs a.json b.json [--allowlist allowlist.json] [--show 5] [--json report.json]
//                    [--covered-only]
//       Strict: exits 1 on any difference not covered by the allowlist.
//       --ignore-parse-error-text: the message after "Error: " in a logged parse error is not
//       compared (a ruling of the plan: the wasm parser's message may differ from jison's;
//       the logged text, that an error was logged, and everything else still must match).
//       --covered-only: a fragment that either run marks `unsupported` (a replacement that
//       cannot lay it out yet says so) is left out and counted; nothing else is relaxed.
//   node compare.mjs --mode linux-vs-mac mac.json linux.json
//   node compare.mjs --mode linux-vs-mac out/ linux-artifacts/ [--a-run A] [--b-run linux]
//       Report only (exit 0): how much of each class is identical across platforms, and the
//       largest numeric difference. Directories are paired per engine by file name
//       (upstream-<engine>-<run id>.json.gz); the CI workflow's runs are called "linux".
//
// Per class (and per group) it reports the identical share, the largest numeric
// difference (in coordinates found in the canonical markup and in bounding boxes), how
// many fragments differ in which field, and the first few differing fragments with a
// readable diff.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { readJson, loadCorpus } from './lib/runner.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const FIELDS = ['html', 'canvases', 'rects', 'sel', 'text', 'console', 'alerts', 'err', 'crash', 'captureErr'];

export function loadAllowlist(file) {
  const list = JSON.parse(fs.readFileSync(file, 'utf8'));
  if (!Array.isArray(list)) throw new Error(`${file}: expected a JSON array`);
  list.forEach((e, i) => {
    if (!e || typeof e !== 'object') throw new Error(`${file}[${i}]: not an object`);
    if (typeof e.reason !== 'string' || !e.reason.trim()) throw new Error(`${file}[${i}]: every entry needs a non-empty "reason"`);
    if (!('id' in e || 'group' in e || 'class' in e)) throw new Error(`${file}[${i}]: needs "id", "group" or "class" to say what it covers`);
    for (const k of Object.keys(e)) {
      if (!['id', 'group', 'class', 'engine', 'fields', 'reason'].includes(k)) throw new Error(`${file}[${i}]: unknown key "${k}"`);
    }
  });
  return list;
}

function allowed(list, item, engine, field) {
  return list.some(
    (e) =>
      (!('id' in e) || e.id === item.id) &&
      (!('group' in e) || e.group === item.group) &&
      (!('class' in e) || e.class === item.class) &&
      (!e.engine || e.engine === engine) &&
      (!e.fields || e.fields.includes(field)),
  );
}

const NUM = /-?\d+(?:\.\d+)?(?:e[+-]?\d+)?/gi;
const skeleton = (s) => s.replace(NUM, '#');
function maxNumDiff(a, b) {
  // Largest absolute difference between corresponding numbers, or null if the text differs elsewhere.
  if (a === b) return 0;
  if (skeleton(a) !== skeleton(b)) return null;
  const na = a.match(NUM) ?? [], nb = b.match(NUM) ?? [];
  let m = 0;
  for (let i = 0; i < na.length; i++) m = Math.max(m, Math.abs(Number(na[i]) - Number(nb[i])));
  return m;
}

function lineDiff(a, b, max = 6) {
  const x = a.split('\n'), y = b.split('\n');
  let p = 0;
  while (p < x.length && p < y.length && x[p] === y[p]) p++;
  let s = 0;
  while (s < x.length - p && s < y.length - p && x[x.length - 1 - s] === y[y.length - 1 - s]) s++;
  const cut = (arr, sign) => {
    const mid = arr.slice(p, arr.length - s);
    const shown = mid.slice(0, max).map((l) => `      ${sign} ${l.length > 220 ? l.slice(0, 220) + '...' : l}`);
    if (mid.length > max) shown.push(`      ${sign} ... ${mid.length - max} more lines`);
    return shown;
  };
  return [`    @ line ${p + 1}`, ...cut(x, '-'), ...cut(y, '+')].join('\n');
}

/** Compare two run objects. Returns a report; does not print. */
// jison's message after "Error: " names what it expected; the port's names a position.
const PARSE_ERROR = /^(log: Parse error:\n[\s\S]*?Error: )(?:Lexical error|Parse error|parse error)[\s\S]*$/;
function normalizeParseErrors(r) {
  if (!r || !Array.isArray(r.console)) return r;
  return { ...r, console: r.console.map((m) => m.replace(PARSE_ERROR, '$1<parse error>')) };
}

export function compareRuns(A, B, { allowlist = [], coveredOnly = false, ignoreParseErrorText = false } = {}) {
  const engine = A.meta.engine;
  const corpus = loadCorpus();
  const byId = new Map(corpus.items.map((it) => [it.id, it]));
  const ids = [...new Set([...Object.keys(A.results), ...Object.keys(B.results)])].sort();
  const stat = () => ({ n: 0, same: 0, allowed: 0, maxNum: 0, structural: 0, pixels: 0, unsupported: 0, fields: {} });
  const classes = {}, groups = {};
  const diffs = [];
  let maxNum = 0;
  let unsupportedCount = 0;
  const reasons = {};
  for (const id of ids) {
    const item = byId.get(id) ?? { id, group: '?', class: '?' };
    let ra = A.results[id], rb = B.results[id];
    if (ignoreParseErrorText) {
      ra = normalizeParseErrors(ra);
      rb = normalizeParseErrors(rb);
    }
    const cs = (classes[item.class] ??= stat()), gs = (groups[item.group] ??= stat());
    if (coveredOnly && (ra?.unsupported || rb?.unsupported)) {
      cs.unsupported++;
      gs.unsupported++;
      unsupportedCount++;
      const why = ra?.unsupported || rb?.unsupported;
      reasons[why] = (reasons[why] ?? 0) + 1;
      continue;
    }
    cs.n++;
    gs.n++;
    if (!ra || !rb) {
      const d = { id, item, missing: ra ? 'second' : 'first', fields: ['missing'] };
      diffs.push(d);
      for (const s of [cs, gs]) s.fields.missing = (s.fields.missing ?? 0) + 1;
      continue;
    }
    const differing = [];
    let num = 0, structural = false, pixels = false;
    for (const f of FIELDS) {
      const ja = JSON.stringify(ra[f] ?? null), jb = JSON.stringify(rb[f] ?? null);
      if (ja === jb) continue;
      differing.push(f);
      if (f === 'html') {
        const d = maxNumDiff(ra.html ?? '', rb.html ?? '');
        if (d == null) structural = true;
        else num = Math.max(num, d);
      } else if (f === 'rects') {
        const fa = ra.rects?.flat() ?? [], fb = rb.rects?.flat() ?? [];
        if (fa.length !== fb.length) structural = true;
        else for (let i = 0; i < fa.length; i++) num = Math.max(num, Math.abs(fa[i] - fb[i]));
      } else if (f === 'canvases') {
        const ca = ra.canvases ?? [], cb = rb.canvases ?? [];
        if (ca.length !== cb.length) structural = true;
        else
          for (let i = 0; i < ca.length; i++) {
            num = Math.max(num, Math.abs(ca[i].w - cb[i].w), Math.abs(ca[i].h - cb[i].h));
            if (ca[i].hash !== cb[i].hash) pixels = true;
          }
      }
    }
    num = Math.round(num * 1e6) / 1e6; // hide float noise from subtracting decimals
    maxNum = Math.max(maxNum, num);
    for (const s of [cs, gs]) s.maxNum = Math.max(s.maxNum, num);
    if (!differing.length) {
      cs.same++;
      gs.same++;
      continue;
    }
    const open = differing.filter((f) => !allowed(allowlist, item, engine, f));
    for (const s of [cs, gs]) {
      if (structural) s.structural++;
      if (pixels) s.pixels++;
      for (const f of differing) s.fields[f] = (s.fields[f] ?? 0) + 1;
    }
    if (!open.length) {
      cs.allowed++;
      gs.allowed++;
    }
    diffs.push({ id, item, fields: differing, open, num, structural, pixels });
  }
  const total = ids.length - unsupportedCount;
  const same = diffs.length === 0 ? total : total - diffs.length;
  const open = diffs.filter((d) => d.open?.length || d.missing).length;
  return { engine, total, same, differing: diffs.length, open, allowedCount: diffs.length - open, maxNum, classes, groups, diffs, unsupported: unsupportedCount, unsupportedReasons: reasons };
}

function table(title, rows) {
  const withU = Object.values(rows).some((s) => s.unsupported);
  const lines = [title, `  ${'name'.padEnd(12)} ${'n'.padStart(6)} ${'identical'.padStart(10)} ${'share'.padStart(8)} ${'max numeric diff'.padStart(17)} ${'pixel diffs'.padStart(12)}${withU ? ' ' + 'unsupported'.padStart(12) : ''}  differing fields`];
  for (const [name, s] of Object.entries(rows).sort()) {
    const share = s.n ? ((100 * s.same) / s.n).toFixed(2) + '%' : '-';
    const f = Object.entries(s.fields).map(([k, v]) => `${k}:${v}`).join(' ');
    lines.push(`  ${name.padEnd(12)} ${String(s.n).padStart(6)} ${String(s.same).padStart(10)} ${share.padStart(8)} ${s.maxNum.toFixed(4).padStart(17)} ${String(s.pixels).padStart(12)}${withU ? ' ' + String(s.unsupported).padStart(12) : ''}  ${f}`);
  }
  return lines.join('\n');
}

export function formatReport(rep, A, B, { show = 5 } = {}) {
  const out = [];
  out.push(`${A.meta.impl}/${A.meta.engine} ${A.meta.engineVersion} [${A.meta.platform}, run ${A.meta.runId}${A.meta.shuffle != null ? ', shuffle ' + A.meta.shuffle : ''}]`);
  out.push(`  vs ${B.meta.impl}/${B.meta.engine} ${B.meta.engineVersion} [${B.meta.platform}, run ${B.meta.runId}${B.meta.shuffle != null ? ', shuffle ' + B.meta.shuffle : ''}]`);
  out.push(`fragments ${rep.total}: identical ${rep.same} (${((100 * rep.same) / Math.max(rep.total, 1)).toFixed(3)}%), differing ${rep.differing} (${rep.allowedCount} allowlisted, ${rep.open} not), largest numeric difference ${rep.maxNum}`);
  if (rep.unsupported) out.push(`left out as unsupported: ${rep.unsupported} (${Object.entries(rep.unsupportedReasons).map(([k, n]) => k + ' ' + n).join(', ')})`);
  out.push(table('by class', rep.classes));
  out.push(table('by group', rep.groups));
  for (const d of rep.diffs.slice(0, show)) {
    out.push(`- ${d.id} (${d.item.class}, ${d.item.group}) fields: ${d.fields.join(', ')}${d.num ? `, numeric diff ${d.num}` : ''}${d.structural ? ', structural' : ''}${d.pixels ? ', pixels' : ''}`);
    out.push(`    text: ${(A.results[d.id]?.text ?? B.results[d.id]?.text ?? '').slice(0, 60)}`);
    for (const f of d.fields) {
      if (f === 'missing') out.push(`    missing from the ${d.missing} run`);
      else if (f === 'html') out.push(lineDiff(A.results[d.id].html ?? '', B.results[d.id].html ?? ''));
      else out.push(`    ${f}: ${JSON.stringify(A.results[d.id][f] ?? null).slice(0, 300)}  vs  ${JSON.stringify(B.results[d.id][f] ?? null).slice(0, 300)}`);
    }
  }
  if (rep.diffs.length > show) out.push(`... and ${rep.diffs.length - show} more differing fragments`);
  return out.join('\n');
}

function metaProblems(A, B) {
  const p = [];
  if (A.meta.corpusSha256 !== B.meta.corpusSha256) p.push(`different corpora (${A.meta.corpusSha256?.slice(0, 12)} vs ${B.meta.corpusSha256?.slice(0, 12)})`);
  if (A.meta.engine !== B.meta.engine) p.push(`different engines (${A.meta.engine} vs ${B.meta.engine})`);
  return p;
}

function listRuns(dir) {
  return fs
    .readdirSync(dir)
    .filter((f) => /\.json(\.gz)?$/.test(f))
    .sort()
    .map((f) => path.join(dir, f));
}

// Only run as a script, not when imported by selfcheck.mjs.
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { values: v, positionals } = parseArgs({
    allowPositionals: true,
    options: {
      allowlist: { type: 'string', default: path.join(HERE, 'allowlist.json') },
      show: { type: 'string', default: '5' },
      json: { type: 'string' },
      mode: { type: 'string', default: 'strict' },
      'linux-vs-mac': { type: 'boolean' },
      impl: { type: 'string', default: 'upstream' },
      'covered-only': { type: 'boolean', default: false },
      'ignore-parse-error-text': { type: 'boolean', default: false },
      'a-run': { type: 'string', default: 'A' },
      'b-run': { type: 'string', default: 'linux' },
    },
  });
  const mode = v['linux-vs-mac'] ? 'linux-vs-mac' : v.mode;
  if (positionals.length !== 2 || !['strict', 'linux-vs-mac'].includes(mode)) {
    console.error('usage: node compare.mjs [--mode strict|linux-vs-mac] [--allowlist f] [--show N] [--json out] a.json b.json');
    process.exit(2);
  }
  let pairs;
  if (fs.statSync(positionals[0]).isDirectory()) {
    // Two directories of runs: pair <impl>-<engine>-<run id>.json[.gz] for each engine.
    const find = (dir, engine, runId) =>
      listRuns(dir).find((f) => new RegExp(`^${v.impl}-${engine}-${runId}\\.json(\\.gz)?$`).test(path.basename(f)));
    pairs = [];
    for (const engine of ['chromium', 'firefox', 'webkit']) {
      const fa = find(positionals[0], engine, v['a-run']), fb = find(positionals[1], engine, v['b-run']);
      if (fa && fb) pairs.push([readJson(fa), readJson(fb)]);
      else console.error(`${engine}: need ${v.impl}-${engine}-${v['a-run']} in the first directory and ...-${v['b-run']} in the second; skipped`);
    }
  } else {
    pairs = [[readJson(positionals[0]), readJson(positionals[1])]];
  }
  const allowlist = mode === 'strict' && fs.existsSync(v.allowlist) ? loadAllowlist(v.allowlist) : [];
  let failed = false;
  const reports = [];
  for (const [A, B] of pairs) {
    const problems = metaProblems(A, B);
    if (problems.length) {
      console.log(`cannot compare: ${problems.join('; ')}`);
      failed = true;
      continue;
    }
    const rep = compareRuns(A, B, { allowlist, coveredOnly: v['covered-only'], ignoreParseErrorText: v['ignore-parse-error-text'] });
    console.log(formatReport(rep, A, B, { show: Number(v.show) }) + '\n');
    reports.push({ engine: rep.engine, ...rep, diffs: rep.diffs.map((d) => ({ id: d.id, fields: d.fields, num: d.num })) });
    if (rep.open > 0) failed = true;
  }
  if (v.json) fs.writeFileSync(v.json, JSON.stringify(reports, null, 1));
  if (mode === 'linux-vs-mac') {
    console.log('(linux-vs-mac is a report; the exit status does not depend on the result)');
    process.exit(0);
  }
  process.exit(failed ? 1 : 0);
}
