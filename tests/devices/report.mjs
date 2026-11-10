#!/usr/bin/env node
// A table of the runs in tests/devices/out/ (or the files given), one row per run.
//
//   node tests/devices/report.mjs [file.json ...] [--details]
//
// --details also prints each run's first differences and anything that went wrong.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const { values: v, positionals } = parseArgs({ allowPositionals: true, options: { details: { type: 'boolean', default: false } } });
const files = positionals.length
  ? positionals
  : fs.existsSync(path.join(HERE, 'out'))
    ? fs.readdirSync(path.join(HERE, 'out')).filter((f) => f.endsWith('.json')).sort().map((f) => path.join(HERE, 'out', f))
    : [];
if (!files.length) {
  console.error('no runs: start the server and open /run.html on a device (README.md)');
  process.exit(1);
}

/** "Safari 18.5 on iOS 18.5" and the like, from the user agent. */
export function browserOf(ua) {
  const m = (re) => (ua.match(re) || [])[1];
  let os = 'unknown';
  if (/iPhone|iPad|iPod/.test(ua)) os = 'iOS ' + (m(/OS (\d+[_\d]*)/) || '?').replace(/_/g, '.');
  else if (/Android/.test(ua)) os = 'Android ' + (m(/Android ([\d.]+)/) || '?');
  else if (/Macintosh/.test(ua)) os = 'macOS';
  else if (/Windows/.test(ua)) os = 'Windows';
  else if (/Linux/.test(ua)) os = 'Linux';
  let b;
  if (/Edg\//.test(ua)) b = 'Edge ' + m(/Edg\/([\d.]+)/);
  else if (/Firefox\/|FxiOS\//.test(ua)) b = 'Firefox ' + (m(/Firefox\/([\d.]+)/) || m(/FxiOS\/([\d.]+)/));
  else if (/CriOS\//.test(ua)) b = 'Chrome ' + m(/CriOS\/([\d.]+)/);
  else if (/Chrome\//.test(ua)) b = (/Headless/.test(ua) ? 'HeadlessChrome ' : 'Chrome ') + m(/Chrome\/([\d.]+)/);
  else if (/Version\/[\d.]+.*Safari/.test(ua)) b = (/Mobile/.test(ua) ? 'Mobile Safari ' : 'Safari ') + m(/Version\/([\d.]+)/);
  else b = ua.slice(0, 40);
  return `${b} (${os})`;
}

const rows = [];
const details = [];
for (const f of files) {
  const r = JSON.parse(fs.readFileSync(f, 'utf8'));
  if (r.fatal) { rows.push([r.label ?? path.basename(f), 'FATAL: ' + r.fatal]); continue; }
  const p = r.parity, g = r.golden, pa = r.paths, s = r.speed;
  const ok = (x) => (x == null ? '-' : x.error ? 'ERR' : x.ok ? 'ok' : 'FAIL');
  rows.push([
    r.label,
    browserOf(r.env.userAgent),
    r.env.hieroxideEngine ?? '-',
    String(r.env.devicePixelRatio),
    p && !p.error ? `${p.identical}/${p.fragments}${p.differing ? ` (${p.differing} differ, ${p.open} open)` : ''}` : 'ERR',
    g && !g.error ? (g.ok ? `ok ${g.all}` : `FAIL ${g.all} (${g.differ} differ)`) : g ? 'ERR' : '-',
    ok(pa),
    s && !s.error ? `${s.upstream.total} / ${s.compat.total} / ${s.consistent.total}` : s ? 'ERR' : '-',
    `${r.seconds} s`,
  ]);
  if (v.details) {
    const d = [`${r.label}: ${r.env.userAgent}`];
    for (const e of r.errors ?? []) d.push('  error: ' + e);
    for (const x of p?.firstDiffs ?? []) d.push('  parity ' + JSON.stringify(x));
    for (const x of g?.firstDiffs ?? []) d.push('  golden ' + JSON.stringify(x));
    if (pa?.consoleErrors?.length) d.push('  paths console errors: ' + pa.consoleErrors.join(' | '));
    if (p && !p.error) d.push(`  parity: max box difference ${p.maxRectDiff}, fields ${JSON.stringify(p.fields)}, engine ${p.engine}`);
    if (s && !s.error) d.push(`  speed ${s.fragments} fragments, ${s.reps} rounds, ms: ` + ['upstream', 'compat', 'consistent'].map((k) => `${k} ${s[k].total} (${s[k].min}-${s[k].max})`).join(', '));
    details.push(d.join('\n'));
  }
}
const head = ['label', 'browser', 'engine', 'dpr', 'compat vs upstream', 'golden (all)', 'paths', 'speed ms: up / compat / consistent', 'run'];
const all = [head, ...rows];
const w = head.map((_, c) => Math.max(...all.map((r) => (r[c] ?? '').length)));
for (const r of all) console.log(r.map((x, c) => (x ?? '').padEnd(w[c])).join('  ').trimEnd());
if (details.length) console.log('\n' + details.join('\n\n'));
