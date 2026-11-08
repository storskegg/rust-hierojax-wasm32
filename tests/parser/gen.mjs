// Generates the inputs of the parser differential test, deterministically.
//
// Usage: node gen.mjs [OUTDIR]        (default: ./out, which is git-ignored)
//
// Writes one file per input class, in the format oracle.mjs reads (one input per line,
// UTF-16 code units as 4 hex digits each):
//   docs.txt      fragments extracted from docs/*.html and docs/texts/*.html
//   random.txt    random strings over the token alphabet
//   mutation.txt  mutations of real (and of grammar-derived) fragments
//   derived.txt   sentences derived from the grammar in src/syntax.jison
//   edge.txt      hand-picked edge cases
// and derived.meta.json with the number of productions and how often the generator used each.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../..');
const OUT = path.resolve(process.argv[2] || path.join(HERE, 'out'));
fs.mkdirSync(OUT, { recursive: true });

// ---- deterministic PRNG (mulberry32) ----
let state = 0x5eed1234;
const rnd = () => {
  state = (state + 0x6d2b79f5) >>> 0;
  let t = state;
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};
const ri = n => Math.floor(rnd() * n);
const pick = a => a[ri(a.length)];

const enc = s => {
  let h = '';
  for (let i = 0; i < s.length; i++) h += s.charCodeAt(i).toString(16).padStart(4, '0');
  return h;
};
const write = (name, arr) => fs.writeFileSync(path.join(OUT, name), arr.map(enc).join('\n') + '\n');
const cp = n => String.fromCodePoint(n);

// ---- terminals of the lexer (src/syntax.jison) and sample strings for each ----
const OPENING = [0x13258, 0x13259, 0x1325A, 0x13286, 0x13288, 0x13379, 0x1342F].map(cp);
const CLOSING = [0x1325B, 0x1325C, 0x1325D, 0x13282, 0x13287, 0x13289, 0x1337A, 0x1337B].map(cp);
const DAMAGED = [];
for (let i = 0x13447; i <= 0x13455; i++) DAMAGED.push(cp(i));
const SIGNS = [0x13000, 0x13001, 0x13050, 0x13216, 0x13257, 0x1325E, 0x13281, 0x13283, 0x13285,
  0x1328A, 0x13378, 0x1337C, 0x133E4, 0x1342E, 0x13460, 0x137FF, 0x13800, 0x13C00, 0x14000,
  0x143FA, 0xFFFD, 0x130C0, 0x1309D, 0x13122, 0x1315D, 0x131B7, 0x13171, 0x13310].map(cp);
const BR_OPEN = ['[', '{', '⟦', '⟨', '⸢'];
const BR_CLOSE = [']', '}', '⟧', '⟩', '⸣'];
const VS = [0xFE00, 0xFE01, 0xFE02, 0xFE03, 0xFE04, 0xFE05, 0xFE06].map(cp);
const FIXED = {
  VER: 0x13430, HOR: 0x13431, INSERT_TS: 0x13432, INSERT_BS: 0x13433, INSERT_TE: 0x13434,
  INSERT_BE: 0x13435, OVERLAY: 0x13436, BEGIN_SEGMENT: 0x13437, END_SEGMENT: 0x13438,
  INSERT_M: 0x13439, INSERT_T: 0x1343A, INSERT_B: 0x1343B, MIRROR: 0x13440, FULL_BLANK: 0x13441,
  HALF_BLANK: 0x13442, FULL_LOST: 0x13443, HALF_LOST: 0x13444, TALL_LOST: 0x13445,
  WIDE_LOST: 0x13446,
};
const BEGIN_ENC = [cp(0x1343C), cp(0x1343E)];
const END_ENC = [cp(0x1343D), cp(0x1343F)];

// A token is { t: NAME, s: string } with s chosen when the token is created, except for
// DELIMITER, ENCLOSURE_OPENING and ENCLOSURE_CLOSING, which are fixed up after the whole
// token list is known so that adjacent tokens do not lex differently (see fixup()).
function terminal(name) {
  switch (name) {
    case 'BRACKET_OPEN': return { t: name, s: pick(BR_OPEN) };
    case 'BRACKET_CLOSE': return { t: name, s: pick(BR_CLOSE) };
    case 'VS': return { t: name, s: pick(VS) };
    case 'SIGN': return { t: name, s: pick(SIGNS) };
    case 'DAMAGED': return { t: name, s: pick(DAMAGED) };
    case 'EOF': return { t: name, s: '' };
    case 'DELIMITER': return { t: name, s: '' };
    case 'ENCLOSURE_OPENING': return { t: name, s: '' };
    case 'ENCLOSURE_CLOSING': return { t: name, s: '' };
    default:
      if (name in FIXED) return { t: name, s: cp(FIXED[name]) };
      throw new Error('unknown terminal ' + name);
  }
}

// Lexer ambiguities across token boundaries (the only ones are with enclosure/delimiter
// characters):
//  * a DELIMITER that is an opening char, followed by [DAMAGED] and a bare begin-enclosure
//    (an ENCLOSURE_OPENING token), would lex as one ENCLOSURE_OPENING;
//  * an ENCLOSURE_CLOSING without a closing char, followed by a DELIMITER that is a closing
//    char, would absorb it.
function fixup(tokens) {
  for (let i = 0; i < tokens.length; i++) {
    const k = tokens[i];
    if (k.t === 'ENCLOSURE_OPENING') {
      const withDelim = rnd() < 0.4;
      k.s = (withDelim ? pick(OPENING) + (rnd() < 0.3 ? pick(DAMAGED) : '') : '') + pick(BEGIN_ENC);
    } else if (k.t === 'ENCLOSURE_CLOSING') {
      const next = tokens[i + 1];
      const withDelim = rnd() < 0.4 || (next && next.t === 'DELIMITER');
      k.s = pick(END_ENC) + (withDelim ? pick(CLOSING) + (rnd() < 0.3 ? pick(DAMAGED) : '') : '');
    } else if (k.t === 'DELIMITER') {
      let j = i + 1;
      if (tokens[j] && tokens[j].t === 'DAMAGED') j++;
      const next = tokens[j];
      const prev = tokens[i - 1];
      const nextBare = next && next.t === 'ENCLOSURE_OPENING';
      const prevBareClose = prev && prev.t === 'ENCLOSURE_CLOSING' && !CLOSING.some(c => prev.s.includes(c));
      let pool = OPENING.concat(CLOSING);
      if (nextBare) pool = CLOSING;
      if (prevBareClose) pool = OPENING;
      k.s = pick(pool);
    }
  }
  // Re-run the opening check, since the closing fixup above may have made an
  // ENCLOSURE_CLOSING non-bare after a DELIMITER was already decided: the fixup of
  // ENCLOSURE_CLOSING forces a closing char when a DELIMITER follows, so it is never bare there.
  return tokens.map(k => k.s).join('');
}

// ---- 1. fragments from docs ----
const decodeEntities = s => s
  .replace(/&#x([0-9a-fA-F]+);/g, (_, h) => String.fromCodePoint(parseInt(h, 16)))
  .replace(/&#(\d+);/g, (_, d) => String.fromCodePoint(parseInt(d, 10)))
  .replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&amp;/g, '&');

function extractDocs() {
  const frags = [];
  const files = [];
  const docs = path.join(ROOT, 'docs');
  for (const f of fs.readdirSync(docs).sort()) if (f.endsWith('.html')) files.push(path.join(docs, f));
  const texts = path.join(docs, 'texts');
  if (fs.existsSync(texts))
    for (const f of fs.readdirSync(texts).sort()) if (f.endsWith('.html')) files.push(path.join(texts, f));
  for (const file of files) {
    const html = fs.readFileSync(file, 'utf8');
    // <span class="hierojax ..."> ... </span> (fragments have no nested spans)
    for (const m of html.matchAll(/<span\b[^>]*\bclass="[^"]*\bhierojax\b[^"]*"[^>]*>([\s\S]*?)<\/span>/g))
      frags.push(decodeEntities(m[1].replace(/<br\s*\/?>/g, '\n').replace(/<[^>]*>/g, '')));
    if (path.basename(file) === 'testsuite.html')
      for (const m of html.matchAll(/<hierojax-demo\b[^>]*>([\s\S]*?)<\/hierojax-demo>/g))
        frags.push(decodeEntities(m[1].replace(/<[^>]*>/g, '')));
  }
  return frags;
}
const docsFrags = extractDocs();
write('docs.txt', docsFrags);

// ---- grammar from src/syntax.jison ----
function readGrammar() {
  const src = fs.readFileSync(path.join(ROOT, 'src/syntax.jison'), 'utf8');
  const parts = src.split(/^%%\s*$/m);
  // parts: [options+lexer header, lexer rules + "/lex ... %start", grammar, user code]
  const body = parts[2];
  const prods = []; // { lhs, rhs: [symbols] } in jison numbering (1-based: index + 1)
  let i = 0;
  const skipWs = () => { while (i < body.length && /\s/.test(body[i])) i++; };
  const skipAction = () => {
    // at '{': skip balanced braces
    let depth = 0;
    do {
      if (body[i] === '{') depth++;
      else if (body[i] === '}') depth--;
      i++;
    } while (depth > 0 && i < body.length);
  };
  const ident = () => { const m = /^[A-Za-z_][A-Za-z_0-9]*/.exec(body.slice(i)); i += m[0].length; return m[0]; };
  skipWs();
  while (i < body.length) {
    const lhs = ident();
    skipWs();
    if (body[i] !== ':') throw new Error('expected : after ' + lhs);
    i++;
    let rhs = [];
    for (;;) {
      skipWs();
      const c = body[i];
      if (c === '{') { skipAction(); continue; }
      if (c === '|' || c === ';') {
        prods.push({ lhs, rhs });
        rhs = [];
        i++;
        if (c === ';') break;
        continue;
      }
      rhs.push(ident());
    }
    skipWs();
  }
  return prods;
}
const prods = readGrammar();
const isTerminal = s => /^[A-Z_]+$/.test(s);
const nts = [...new Set(prods.map(p => p.lhs))];
const alts = Object.fromEntries(nts.map(n => [n, []]));
prods.forEach((p, idx) => alts[p.lhs].push(idx));
for (const p of prods) for (const s of p.rhs) if (!isTerminal(s) && !alts[s]) throw new Error('undefined nonterminal ' + s);

// minimal derivation height per nonterminal / production
const minH = Object.fromEntries(nts.map(n => [n, Infinity]));
const prodH = idx => 1 + Math.max(0, ...prods[idx].rhs.map(s => (isTerminal(s) ? 0 : minH[s])));
for (let changed = true; changed;) {
  changed = false;
  for (const n of nts) for (const idx of alts[n]) {
    const h = prodH(idx);
    if (h < minH[n]) { minH[n] = h; changed = true; }
  }
}
// reach[n]: nonterminals derivable from n (including n)
const reach = Object.fromEntries(nts.map(n => [n, new Set([n])]));
for (let changed = true; changed;) {
  changed = false;
  for (const p of prods) for (const s of p.rhs) if (!isTerminal(s))
    for (const r of reach[s]) if (!reach[p.lhs].has(r)) { reach[p.lhs].add(r); changed = true; }
}

// dist[T][n]: fewest derivation steps from n down to a nonterminal T
const dist = {};
function distTo(T) {
  if (dist[T]) return dist[T];
  const d = Object.fromEntries(nts.map(n => [n, Infinity]));
  d[T] = 0;
  for (let changed = true; changed;) {
    changed = false;
    for (const p of prods) for (const s of p.rhs) if (!isTerminal(s) && d[s] + 1 < d[p.lhs]) { d[p.lhs] = d[s] + 1; changed = true; }
  }
  return (dist[T] = d);
}

const hits = new Array(prods.length).fill(0);
function derive(nt, budget, force, out) {
  // choose an alternative
  let cands;
  if (force !== null && nt === prods[force].lhs) {
    cands = [force];
    force = null;
  } else if (force !== null) {
    const d = distTo(prods[force].lhs);
    cands = alts[nt].filter(idx => prods[idx].rhs.some(s => !isTerminal(s) && d[s] === d[nt] - 1));
  } else {
    cands = alts[nt];
    if (budget <= 0) {
      const m = Math.min(...cands.map(prodH));
      cands = cands.filter(idx => prodH(idx) === m);
    }
  }
  let idx;
  if (cands.length === 1) idx = cands[0];
  else {
    const w = cands.map(c => 1 / (1 + hits[c] / 40));
    let r = rnd() * w.reduce((a, b) => a + b, 0);
    idx = cands[cands.length - 1];
    for (let k = 0; k < cands.length; k++) { r -= w[k]; if (r <= 0) { idx = cands[k]; break; } }
  }
  hits[idx]++;
  const rhs = prods[idx].rhs;
  let carrier = -1;
  if (force !== null) {
    const d = distTo(prods[force].lhs);
    const ok = rhs.map((s, k) => (!isTerminal(s) && d[s] === d[nt] - 1 ? k : -1)).filter(k => k >= 0);
    carrier = pick(ok);
  }
  rhs.forEach((s, k) => {
    if (isTerminal(s)) out.push(terminal(s));
    else derive(s, budget - 1, k === carrier ? force : null, out);
  });
}
function sentence(force) {
  const out = [];
  derive('fragment', 6 + ri(14), force, out);
  return out;
}
const derived = [];
const PER_FORCED = 40;
for (let p = 0; p < prods.length; p++) {
  for (let k = 0; k < PER_FORCED; k++) {
    const toks = sentence(p);
    if (toks.length <= 400) derived.push(fixup(toks.filter(t => t.t !== 'EOF')));
  }
}
while (derived.length < 22000) {
  const toks = sentence(null);
  if (toks.length <= 400) derived.push(fixup(toks.filter(t => t.t !== 'EOF')));
}
write('derived.txt', derived);
fs.writeFileSync(path.join(OUT, 'derived.meta.json'), JSON.stringify({ productions: prods.length, hits }));

// ---- 2. random strings over the token alphabet ----
const ALPHABET = [];
for (const set of [SIGNS, SIGNS, SIGNS, BR_OPEN, BR_CLOSE, VS, DAMAGED, OPENING, CLOSING, BEGIN_ENC, END_ENC])
  ALPHABET.push(...set);
for (const k of Object.keys(FIXED)) { const c = cp(FIXED[k]); ALPHABET.push(c, c); }
const JUNK = ['a', ' ', '\n', '\u0000', '\ud80c', '\udc00', '\ud80d', '�', '︇', '\u{1f600}',
  cp(0x13456), cp(0x1345F), cp(0x143FB), cp(0x13258) + cp(0x13447), 'ab'];
const W = { sign: 0.38, ver: 0.07, hor: 0.1, ovl: 0.04, bs: 0.06, es: 0.06, ins: 0.07, other: 0.22 };
function weighted() {
  const r = rnd();
  if (r < W.sign) return pick(SIGNS);
  if (r < W.sign + W.ver) return cp(FIXED.VER);
  if (r < W.sign + W.ver + W.hor) return cp(FIXED.HOR);
  if (r < W.sign + W.ver + W.hor + W.ovl) return cp(FIXED.OVERLAY);
  if (r < W.sign + W.ver + W.hor + W.ovl + W.bs) return cp(FIXED.BEGIN_SEGMENT);
  if (r < W.sign + W.ver + W.hor + W.ovl + W.bs + W.es) return cp(FIXED.END_SEGMENT);
  if (r < W.sign + W.ver + W.hor + W.ovl + W.bs + W.es + W.ins)
    return cp(pick([0x13432, 0x13433, 0x13434, 0x13435, 0x13439, 0x1343A, 0x1343B]));
  return pick(ALPHABET);
}
const random = [];
for (let n = 0; n < 45000; n++) {
  const len = ri(4) === 0 ? ri(30) : ri(12);
  let s = '';
  for (let k = 0; k < len; k++) s += n % 3 === 0 ? pick(ALPHABET) : n % 3 === 1 ? weighted() : (rnd() < 0.03 ? pick(JUNK) : weighted());
  random.push(s);
}
write('random.txt', random);

// ---- 3. mutations ----
const units = s => Array.from({ length: s.length }, (_, i) => s[i]);
function mutate(base) {
  const useUnits = rnd() < 0.3;
  const a = useUnits ? units(base) : Array.from(base);
  const k = 1 + ri(3);
  for (let m = 0; m < k; m++) {
    const pos = ri(a.length + 1);
    const piece = rnd() < 0.15 ? pick(JUNK) : pick(ALPHABET);
    switch (ri(8)) {
      case 0: a.splice(pos, 0, ...(useUnits ? units(piece) : Array.from(piece))); break;
      case 1: if (a.length) a.splice(ri(a.length), 1); break;
      case 2: if (a.length > 1) { const i = ri(a.length - 1); [a[i], a[i + 1]] = [a[i + 1], a[i]]; } break;
      case 3: if (a.length) { const i = ri(a.length); a.splice(i, 0, a[i]); } break;
      case 4: if (a.length) a.splice(ri(a.length), 1, ...(useUnits ? units(piece) : Array.from(piece))); break;
      case 5: { // duplicate a run
        if (a.length) { const i = ri(a.length), l = 1 + ri(Math.min(6, a.length - i)); a.splice(i, 0, ...a.slice(i, i + l)); }
        break;
      }
      case 6: if (a.length) a.length = ri(a.length + 1); break; // truncate
      case 7: { // split a surrogate pair: drop one half, or put something between the halves
        const s = a.join('');
        const idx = [];
        for (let i = 0; i + 1 < s.length; i++) if (s.charCodeAt(i) >= 0xd800 && s.charCodeAt(i) < 0xdc00) idx.push(i);
        if (idx.length) {
          const i = pick(idx);
          const u = units(s);
          const op = ri(3);
          if (op === 0) u.splice(i, 1);
          else if (op === 1) u.splice(i + 1, 1);
          else u.splice(i + 1, 0, pick(['a', 'x', '�', '\ud80c']));
          a.length = 0; a.push(...u);
        }
        break;
      }
    }
  }
  return a.join('');
}
const baseDocs = [...new Set(docsFrags)].filter(f => f.length > 0 && f.length < 300);
const mutations = [];
for (let n = 0; n < 32000; n++) mutations.push(mutate(pick(baseDocs)));
const baseDerived = derived.filter(f => f.length < 200);
for (let n = 0; n < 16000; n++) mutations.push(mutate(pick(baseDerived)));
write('mutation.txt', mutations);

// ---- 5. edge cases ----
const edge = [];
const E = {};
for (const [k, v] of Object.entries(FIXED)) E[k] = cp(v);
const a = cp(0x13000), b = cp(0x13001), c = cp(0x13002), d = cp(0x13003);
edge.push('', ' ', 'a', '\n', a, a + a, '�', '��', '�' + E.VER + '�', '�' + E.OVERLAY + a);
// only delimiters
for (const x of OPENING.concat(CLOSING)) edge.push(x, x + x, x + cp(0x13447), x + a, a + x);
// lone surrogates in all places
for (const lone of ['\ud80c', '\ud80d', '\udc00', '\ude58', '\udfff']) {
  edge.push(lone, a + lone, lone + a, lone + lone, a + E.HOR + lone + b, E.BEGIN_SEGMENT + lone, lone + E.VER + a);
}
edge.push('\ud80c𓀀', a.slice(0, 1) + 'a', a.slice(1) + a, cp(0x13258).slice(0, 1) + cp(0x13258).slice(1));
// every control character alone and next to a literal, and all unassigned ones
for (let x = 0x13430; x <= 0x13460; x++) { const s = cp(x); edge.push(s, a + s, s + a, a + s + b); }
// brackets: every pairing, in the places the grammar knows
for (const o of BR_OPEN) for (const cl of BR_CLOSE) {
  edge.push(o + a + cl, o + a, a + cl, o + a + E.HOR + b + cl, a + E.HOR + o + b + cl, o + a + E.HOR + b,
    a + E.HOR + b + cl, o + E.BEGIN_SEGMENT + a + E.VER + b + E.END_SEGMENT + cl,
    o + a + E.TALL_LOST, o + E.FULL_BLANK + cl, o + cl, cl + o, o + o + a + cl + cl,
    E.BEGIN_SEGMENT + o + a + cl + E.VER + b + E.END_SEGMENT,
    a + E.INSERT_TS + E.BEGIN_SEGMENT + o + b + cl + E.END_SEGMENT);
}
// insertions: every non-empty subset of places, with several cores and several fillers
const PLACE_TOK = [E.INSERT_TS, E.INSERT_BS, E.INSERT_TE, E.INSERT_BE, E.INSERT_M, E.INSERT_T, E.INSERT_B];
const cores = [a, a + E.OVERLAY + b, E.BEGIN_SEGMENT + a + E.HOR + b + E.END_SEGMENT + E.OVERLAY + c,
  a + E.OVERLAY + E.BEGIN_SEGMENT + b + E.VER + c + E.END_SEGMENT];
const fillers = [b, E.FULL_BLANK, E.HALF_LOST + VS[0], cp(0x1343C) + b + cp(0x1343D),
  E.BEGIN_SEGMENT + b + E.VER + c + E.END_SEGMENT, E.BEGIN_SEGMENT + b + E.HOR + c + E.END_SEGMENT,
  E.BEGIN_SEGMENT + b + E.TALL_LOST.slice(0, 0) + E.INSERT_TS + c + E.END_SEGMENT,
  b + E.OVERLAY + c, E.BEGIN_SEGMENT + b + E.HOR + c + E.END_SEGMENT + E.OVERLAY + d];
for (let mask = 1; mask < 128; mask++) {
  for (let ci = 0; ci < cores.length; ci++) {
    const fill = fillers[(mask + ci) % fillers.length];
    let s = cores[ci];
    for (let p = 0; p < 7; p++) if (mask & (1 << p)) s += PLACE_TOK[p] + fill;
    edge.push(s);
    // the same, with the places in reverse order (invalid unless one place)
    let r = cores[ci];
    for (let p = 6; p >= 0; p--) if (mask & (1 << p)) r += PLACE_TOK[p] + a;
    edge.push(r);
    // as an inserted subgroup of an insertion group, wrapped by segments
    edge.push(a + E.INSERT_TS + E.BEGIN_SEGMENT + s + E.END_SEGMENT);
    edge.push(E.BEGIN_SEGMENT + s + E.END_SEGMENT);
    edge.push(a + E.HOR + s + E.HOR + b);
  }
}
// all pairs of insertion places, with a literal and a doubled place
for (let p = 0; p < 7; p++) for (let q = 0; q < 7; q++) edge.push(a + PLACE_TOK[p] + b + PLACE_TOK[q] + c);
// vertical / horizontal / segment structure
const seg = (...xs) => E.BEGIN_SEGMENT + xs.join('') + E.END_SEGMENT;
edge.push(a + E.VER + b, a + E.VER + b + E.VER + c, a + E.HOR + b, a + E.HOR + b + E.VER + c,
  a + E.VER + b + E.HOR + c, a + E.VER + b + E.HOR + c + E.VER + d, a + E.HOR + seg(b, E.VER, c),
  a + E.HOR + seg(b, E.VER, c) + E.HOR + d, seg(a, E.VER, b), seg(a, E.VER, b) + E.HOR + c,
  seg(a, E.HOR, b), seg(a, E.HOR, b) + E.OVERLAY + c, seg(a, E.HOR, b, E.HOR, c) + E.OVERLAY + seg(d, E.VER, a),
  seg(a, E.HOR, b) + E.OVERLAY + c + E.INSERT_TS + d, seg(a, E.HOR, b) + E.OVERLAY + seg(c, E.VER, d, E.VER, a),
  a + E.INSERT_TS + seg(a, E.HOR, b), a + E.INSERT_TS + seg(a, E.VER, b), a + E.INSERT_TS + seg(a, E.INSERT_TS, b),
  a + E.INSERT_TS + seg(a, E.OVERLAY, b, E.INSERT_B, c),
  a + E.INSERT_TS + seg(seg(a, E.HOR, b), E.OVERLAY, c, E.INSERT_B, d),
  seg(seg(a, E.VER, b), E.HOR, c) + E.VER + d, seg(seg(a, E.VER, b), E.VER, c), seg(seg(a, E.VER, b)),
  seg(a), seg(a, E.END_SEGMENT), seg(), seg(seg(a, E.HOR, b)), seg('[', a, E.HOR, b, ']'),
  seg('[', a, ']'), seg('[', a), seg(a, ']'), seg('[', seg(a, E.VER, b), ']', E.HOR, c),
  a + E.OVERLAY, E.OVERLAY + a, a + E.OVERLAY + b + E.OVERLAY + c, a + E.OVERLAY + seg(b, E.VER, c),
  a + E.OVERLAY + seg(b, E.HOR, c), a + E.OVERLAY + seg(b), a + E.OVERLAY + seg(b, E.VER),
  a + E.HOR + b + E.OVERLAY + c, seg(a, E.HOR, b) + E.OVERLAY + seg(c, E.VER, d) + E.MIRROR);
// literals with all options, placeholders with all options
for (const v of ['', VS[0], VS[6]]) for (const m of ['', E.MIRROR]) for (const dm of ['', DAMAGED[0], DAMAGED[14]])
  edge.push(a + v + m + dm, a + m + v + dm, a + dm + v, a + v + dm + m, a + v + m + dm + E.HOR + b + v + m + dm);
for (const t of ['FULL_BLANK', 'HALF_BLANK', 'FULL_LOST', 'HALF_LOST', 'TALL_LOST', 'WIDE_LOST'])
  for (const v of ['', VS[0], VS[2], VS[6]]) edge.push(E[t] + v, a + E.HOR + E[t] + v + E.HOR + b, E[t] + v + E.INSERT_TS + a,
    a + E.INSERT_TS + E[t] + v + E.INSERT_B + E[t], E[t] + v + E.VER + E[t] + v, E[t] + v + E[t] + v, E[t] + E.MIRROR);
// enclosures: all openings and closings
for (const o of ['', ...OPENING.slice(0, 2), OPENING[6]]) for (const od of ['', DAMAGED[0], DAMAGED[14]])
  for (const be of BEGIN_ENC) for (const ee of END_ENC) for (const cl of ['', CLOSING[0], CLOSING[7]])
    for (const cd of ['', DAMAGED[2]]) {
      edge.push(o + (o ? od : '') + be + a + b + ee + cl + (cl ? cd : ''));
      edge.push(o + (o ? od : '') + be + ee + cl + (cl ? cd : ''));
      edge.push(a + E.HOR + o + (o ? od : '') + be + a + ee + cl + (cl ? cd : '') + E.HOR + b);
    }
edge.push(cp(0x1343C) + cp(0x1343C) + a + cp(0x1343D) + a + cp(0x1343D), cp(0x1343C) + a, a + cp(0x1343D),
  cp(0x1343C) + OPENING[0] + cp(0x1343D), cp(0x1343C) + a + E.VER + b + cp(0x1343D),
  cp(0x1343C) + a + E.INSERT_TS + cp(0x1343C) + b + cp(0x1343D) + cp(0x1343D),
  cp(0x1343C) + CLOSING[0] + cp(0x1343D), OPENING[0] + cp(0x1343C) + a + cp(0x1343D) + CLOSING[0] + DAMAGED[0],
  OPENING[0] + OPENING[0] + cp(0x1343C) + a + cp(0x1343D), CLOSING[0] + cp(0x1343D));
// deep nesting, within the parser's depth limit and what jison handles
const nestEnc = n => cp(0x1343C).repeat(n) + a + cp(0x1343D).repeat(n);
const nestSeg = n => { let s = a + E.OVERLAY + b; for (let i = 0; i < n; i++) s = E.BEGIN_SEGMENT + a + E.INSERT_TS + s + E.END_SEGMENT; return a + E.INSERT_TS + s; };
const nestVer = n => { let s = a; for (let i = 0; i < n; i++) s = E.BEGIN_SEGMENT + c + E.VER + a + E.HOR + s + E.END_SEGMENT; return s; };
const nestIns = n => { let s = b; for (let i = 0; i < n; i++) s = a + E.INSERT_TS + s; return s; };
for (const n of [1, 2, 3, 10, 50, 100, 200, 255, 256])
  edge.push(nestEnc(n), nestSeg(n), nestVer(n), nestIns(n + 1), c + E.HOR + nestEnc(n) + E.HOR + d);
write('edge.txt', edge);

console.log(JSON.stringify({
  out: OUT, productions: prods.length,
  counts: { docs: docsFrags.length, random: random.length, mutation: mutations.length, derived: derived.length, edge: edge.length },
}));
