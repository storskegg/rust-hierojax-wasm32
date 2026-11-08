// Seeded case generator for the jsnum test: streams cases computed by real JS to stdout.
//
//   node gen.mjs floats  --seed S --shard K --count N
//   node gen.mjs strings --seed S --shard K --count N
//
// floats: one line per double, tab separated:
//   bits(hex)  toFixed(0) toFixed(1) toFixed(2) toFixed(3)  toFixed(d)  d  String(x)  Math.round(x) bits(hex)
// strings: one line per string:
//   hex(utf8 of s)  bits(hex) of Number(s)
//
// The Rust side (an ignored test in crates/hieroxide-core/src/jsnum.rs) spawns this and compares.
// Same seed + shard + count always gives the same cases.

const [mode, ...rest] = process.argv.slice(2);
const arg = (name, dflt) => {
  const i = rest.indexOf('--' + name);
  return i < 0 ? dflt : Number(rest[i + 1]);
};
const seed = arg('seed', 1), shard = arg('shard', 0), count = arg('count', 1000);
if (mode !== 'floats' && mode !== 'strings') {
  console.error('usage: gen.mjs floats|strings --seed S --shard K --count N');
  process.exit(2);
}

// xoshiro128** seeded by splitmix32
function splitmix32(a) {
  return () => {
    a |= 0; a = (a + 0x9e3779b9) | 0;
    let t = a ^ (a >>> 16); t = Math.imul(t, 0x21f0aaad);
    t ^= t >>> 15; t = Math.imul(t, 0x735a2d97);
    return (t ^ (t >>> 15)) >>> 0;
  };
}
const sm = splitmix32((seed * 1000003 + shard * 7919) | 0);
let s0 = sm(), s1 = sm(), s2 = sm(), s3 = sm();
const rotl = (x, k) => (x << k) | (x >>> (32 - k));
function u32() {
  const r = Math.imul(rotl(Math.imul(s1, 5), 7), 9) >>> 0;
  const t = s1 << 9;
  s2 ^= s0; s3 ^= s1; s1 ^= s2; s0 ^= s3; s2 ^= t; s3 = rotl(s3, 11);
  return r;
}
const rnd = () => (u32() * 2 ** 21 + (u32() >>> 11)) / 2 ** 53; // [0,1)
const ri = (n) => Math.floor(rnd() * n); // [0,n)
const pick = (a) => a[ri(a.length)];

const f64 = new Float64Array(1), u64 = new BigUint64Array(f64.buffer), u32v = new Uint32Array(f64.buffer);
const bitsOf = (x) => { f64[0] = x; return u64[0].toString(16).padStart(16, '0'); };
const fromBits = (hi, lo) => { u32v[0] = lo; u32v[1] = hi; return f64[0]; };
const nextUp = (x) => { f64[0] = x; if (x >= 0) u64[0] += 1n; else u64[0] -= 1n; return f64[0]; };
const stepUlps = (x, k) => { f64[0] = x; const neg = x < 0; u64[0] = u64[0] + BigInt(neg ? -k : k); return f64[0]; };

const SPECIAL = [
  0, -0, NaN, Infinity, -Infinity, Number.MAX_VALUE, -Number.MAX_VALUE, Number.MIN_VALUE, 2.2250738585072014e-308,
  2 ** 53, 2 ** 53 - 1, 2 ** 53 + 2, 2 ** 52 - 0.5, 4503599627370495.5, 0.5, 0.49999999999999994, 0.5000000000000001,
  1.5, 2.5, -0.5, -1.5, -2.5, 1e21, 1e21 - 1e5, 999999999999999900000, 1e20, 1e-6, 1e-7, 9.999999999999999e-7,
  0.000001, 123456789012345680000, 1.005, 8.345, 0.1, 0.2, 0.3, 0.1 + 0.2, 4.35, 1.45, 10.235, 1e-10, -1e-10,
  1 / 3, 2 / 3, 5e-324, 1.7976931348623157e308, 4.9e-324, 0.07, 0.0625, 0.125, 0.375, 0.00005, 0.0005,
];

const FRAC_DIGITS = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];
function genFloat(i) {
  const k = ri(100);
  let x;
  if (k < 3) x = SPECIAL[i % SPECIAL.length];
  else if (k < 13) x = fromBits(u32(), u32()); // any bit pattern
  else if (k < 28) x = 10 ** (rnd() * 60 - 30) * (rnd() < 0.5 ? 1 : 1 + rnd()); // many magnitudes
  else if (k < 48) { // layout-like
    x = rnd() * 10 ** (ri(9) - 3);
    if (rnd() < 0.3) x = Math.round(x * 100) / 100;
  } else if (k < 63) { // exact ties and dyadic rationals
    const n = ri(21);
    const big = rnd() < 0.1 ? 2 ** 40 : 1e5;
    x = ri(big * 2 ** (rnd() < 0.2 ? 8 : 0)) / 2 ** n;
    if (rnd() < 0.3) x = (2 * ri(1e5) + 1) / 2 ** (1 + ri(12)); // odd numerators: true ties
  } else if (k < 78) { // short decimals, many ending in 5 (near-ties that are not ties)
    const ip = rnd() < 0.5 ? String(ri(1000)) : String(ri(2 ** 31));
    let fr = '';
    const nd = 1 + ri(5);
    for (let j = 0; j < nd; j++) fr += pick(FRAC_DIGITS);
    if (rnd() < 0.5) fr = fr.slice(0, -1) + '5';
    x = Number(ip + '.' + fr);
  } else if (k < 86) { // integers around and beyond 2^53
    const m = ri(4);
    x = m === 0 ? ri(2 ** 31) : m === 1 ? Math.floor(rnd() * 2 ** 53) : m === 2 ? Math.floor(rnd() * 1e22) : 2 ** (ri(80));
    if (rnd() < 0.3) x += rnd() < 0.5 ? 0.5 : -0.5;
  } else if (k < 92) { // near 1e21 and the toString exponent thresholds
    // also powers of two, where the rounding interval is lopsided and ties are special
    const base = rnd() < 0.5 ? 2 ** (ri(140) - 70) : pick([1e21, 1e21, 1e20, 1e-6, 1e-7, 2 ** 53, 1e22, 123456789012345680000]);
    x = stepUlps(base, ri(41) - 20);
    if (rnd() < 0.3) x = base * (1 + (rnd() - 0.5) * 1e-14);
  } else if (k < 97) { // subnormals and tiny
    x = fromBits(ri(2 ** 20), u32());
    if (rnd() < 0.3) x = Number.MIN_VALUE * ri(1000);
    if (rnd() < 0.2) x = 2.2250738585072014e-308 * (0.5 + rnd());
  } else { // near ties after scaling
    x = (ri(2000) + 0.5) / 10 ** (ri(4));
    x = stepUlps(x, ri(5) - 2);
  }
  if (rnd() < 0.4) x = -x;
  return x;
}

const SPACES = [' ', '\t', '\n', '\r', '\v', '\f', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', ' ', '　', '﻿'];
const NOT_SPACES = ['\u0085', '᠎', '​', '⁠', '\u0000', 'x', 'é'];
const D = '0123456789';
const digits = (n, alpha = D) => { let s = ''; for (let j = 0; j < n; j++) s += alpha[ri(alpha.length)]; return s; };

function midpointString() {
  // exact decimal midpoint between two adjacent doubles: a tie for round-to-nearest-even
  const x = 10 ** (rnd() * 9 - 3) * (1 + rnd());
  f64[0] = x;
  const bits = u64[0];
  const e = Number((bits >> 52n) & 0x7ffn) - 1075, m = (bits & ((1n << 52n) - 1n)) | (1n << 52n);
  // midpoint = (2m+1) * 2^(e-1); e-1 < 0 here
  const k = 1 - e;
  const num = (2n * m + 1n) * 5n ** BigInt(k); // /10^k
  let s = num.toString().padStart(k + 1, '0');
  s = s.slice(0, s.length - k) + '.' + s.slice(s.length - k);
  return s;
}

function genString() {
  const k = ri(100);
  let s;
  if (k < 25) { // decimal
    s = (rnd() < 0.2 ? pick(['+', '-']) : '') + digits(ri(22));
    if (rnd() < 0.6) s += '.' + digits(ri(22));
    if (rnd() < 0.3) s += pick(['e', 'E']) + pick(['', '+', '-']) + digits(ri(5));
  } else if (k < 40) { // plausible attribute values
    s = pick(['0.07', '1', '1.3', '0.2', '33.6px', '', 'true', 'abc', '1e3', '  5  ', '0.01', '0.10', '0.15', '2', '.5', '5.', '-1', '+1', '1.', '0', '-0', '00012', '1e-7', ' ', '12px', '1,5']);
  } else if (k < 55) { // radix literals
    const p = pick(['0x', '0X', '0o', '0O', '0b', '0B']);
    const alpha = p[1].toLowerCase() === 'x' ? '0123456789abcdefABCDEF' : p[1].toLowerCase() === 'o' ? '01234567' : '01';
    s = (rnd() < 0.1 ? pick(['-', '+']) : '') + p + digits(ri(rnd() < 0.3 ? 40 : 14), alpha);
    if (rnd() < 0.08) s += pick(['g', '8', '.', 'p1', '_1']);
    if (rnd() < 0.4 && s.length > 20) s = s.slice(0, 2) + '1' + '0'.repeat(ri(30)) + s.slice(2 + ri(10)); // long rounding stress
  } else if (k < 63) {
    s = pick(['Infinity', '-Infinity', '+Infinity', 'infinity', 'INFINITY', 'Infinity1', 'Inf', 'NaN', 'nan', '-NaN', 'Infinit', 'Infinity ', '1Infinity']);
  } else if (k < 78) { // garbage over a small alphabet
    s = digits(1 + ri(8), '0123456789.eE+-xXoObBa _');
  } else if (k < 88) { // midpoints and neighbours
    s = midpointString();
    if (rnd() < 0.4) s += digits(1 + ri(3), '01'); // just above the tie
    if (rnd() < 0.2) s = s.replace(/\.?0*$/, '') + '1';
    if (rnd() < 0.3) s += 'e' + (ri(5) - 2);
  } else if (k < 94) { // extreme exponents and long digit strings
    s = pick(['1e308', '1.7976931348623158e308', '1.7976931348623159e308', '1e309', '4.9e-324', '2.4703282292062327e-324', '2.4703282292062328e-324', '1e-400', '-1e-400', '0e999999999999', '1e99999999999999999999', '1e-99999999999999999999']);
    if (rnd() < 0.3) s = digits(300 + ri(300)) + 'e-' + (300 + ri(300));
  } else { // number text from a double
    s = String(genFloat(ri(1e6)));
  }
  if (rnd() < 0.25) s = pick(SPACES).repeat(1 + ri(2)) + s;
  if (rnd() < 0.25) s = s + pick(SPACES).repeat(1 + ri(2));
  if (rnd() < 0.03) s = pick(NOT_SPACES) + s;
  if (rnd() < 0.03) s = s + pick(NOT_SPACES);
  return s;
}

const out = process.stdout;
let buf = [];
let bufBytes = 0;
async function flush() {
  if (!buf.length) return;
  const chunk = buf.join('');
  buf = []; bufBytes = 0;
  if (!out.write(chunk)) await new Promise((r) => out.once('drain', r));
}
async function emit(line) {
  buf.push(line);
  bufBytes += line.length;
  if (bufBytes > 1 << 20) await flush();
}

const toHexUtf8 = (s) => Buffer.from(s, 'utf8').toString('hex');

(async () => {
  if (mode === 'floats') {
    for (let i = 0; i < count; i++) {
      const x = genFloat(i);
      const d = rnd() < 0.6 ? ri(21) : ri(101);
      await emit([bitsOf(x), x.toFixed(0), x.toFixed(1), x.toFixed(2), x.toFixed(3), x.toFixed(d), d, String(x), bitsOf(Math.round(x))].join('\t') + '\n');
    }
  } else {
    for (let i = 0; i < count; i++) {
      const s = genString();
      await emit(toHexUtf8(s) + '\t' + bitsOf(Number(s)) + '\n');
    }
  }
  await flush();
})();
