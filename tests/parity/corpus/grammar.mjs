// A small random generator driven by upstream's grammar (src/syntax.jison).
// The productions and the fixed tokens are read from the .jison file; only the
// character classes (signs, brackets, delimiters, damage marks) are listed here.
import fs from 'node:fs';

export function readJison(file) {
  const text = fs.readFileSync(file, 'utf8');
  const lexEnd = text.indexOf('/lex');
  const lexText = text.slice(0, lexEnd);

  // Fixed tokens:   '𓐰'	return 'VER';
  const fixed = {};
  for (const m of lexText.matchAll(/^'((?:\\u[0-9A-Fa-f]{4})+)'\s+return '(\w+)';/gm)) {
    fixed[m[2]] = JSON.parse(`"${m[1]}"`);
  }

  // Productions: between the first two %% after the lexer block.
  const g0 = text.indexOf('%%', lexEnd) + 2;
  const g1 = text.indexOf('%%', g0);
  let g = text.slice(g0, g1).replace(/\/\*[\s\S]*?\*\//g, '');
  // Drop semantic actions, which are balanced {...}.
  let out = '';
  for (let i = 0, depth = 0; i < g.length; i++) {
    if (g[i] === '{') depth++;
    else if (g[i] === '}') depth--;
    else if (depth === 0) out += g[i];
  }
  const rules = {};
  for (const chunk of out.split(';')) {
    const m = chunk.match(/^\s*(\w+)\s*:([\s\S]*)$/);
    if (!m) continue;
    rules[m[1]] = m[2].split('|').map((alt) => alt.trim().split(/\s+/).filter(Boolean));
  }
  return { rules, fixed, start: 'fragment' };
}

const cp = (n) => String.fromCodePoint(n);
const range = (a, b) => Array.from({ length: b - a + 1 }, (_, i) => cp(a + i));

// Character classes from the lexer header comment and rules in syntax.jison.
export const CLASSES = {
  BRACKET_OPEN: ['[', '{', '⟦', '⟨', '⸢'],
  BRACKET_CLOSE: [']', '}', '⟧', '⟩', '⸣'],
  VS: range(0xfe00, 0xfe06),
  DAMAGED: range(0x13447, 0x13455),
  OPENING: [0x13258, 0x13259, 0x1325a, 0x13286, 0x13288, 0x13379, 0x1342f].map(cp),
  CLOSING: [0x1325b, 0x1325c, 0x1325d, 0x13282, 0x13287, 0x13289, 0x1337a, 0x1337b].map(cp),
  BEGIN_ENCLOSURE: [cp(0x1343c), cp(0x1343e)],
  END_ENCLOSURE: [cp(0x1343d), cp(0x1343f)],
};

// The lexer's SIGN ranges.
export const SIGN_RANGES = [
  [0x13000, 0x13257], [0x1325e, 0x13281], [0x13283, 0x13285], [0x1328a, 0x13378],
  [0x1337c, 0x1342e], [0x13460, 0x143fa], [0xfffd, 0xfffd],
];
export const isSignCp = (c) => SIGN_RANGES.some(([a, b]) => c >= a && c <= b);

/**
 * rnd: () => [0,1). signs: array of sign characters. interesting: signs to favour
 * (those the upstream tables mention, so insertions and ligatures actually fire).
 */
export function makeGenerator({ rules, fixed }, { rnd, signs, interesting }) {
  const pick = (a) => a[Math.floor(rnd() * a.length)];
  const isTerminal = (s) => !(s in rules);

  // Smallest number of terminals each symbol can derive.
  const min = {};
  for (const s of Object.keys(rules)) min[s] = Infinity;
  const sizeOf = (s) => (isTerminal(s) ? (s === 'EOF' ? 0 : 1) : min[s]);
  for (let changed = true; changed; ) {
    changed = false;
    for (const [s, alts] of Object.entries(rules)) {
      for (const alt of alts) {
        const n = alt.reduce((t, x) => t + sizeOf(x), 0);
        if (n < min[s]) {
          min[s] = n;
          changed = true;
        }
      }
    }
  }

  const terminal = (t) => {
    switch (t) {
      case 'EOF': return '';
      case 'SIGN': return rnd() < 0.4 ? pick(interesting) : pick(signs);
      case 'BRACKET_OPEN': return pick(CLASSES.BRACKET_OPEN);
      case 'BRACKET_CLOSE': return pick(CLASSES.BRACKET_CLOSE);
      case 'VS': return pick(CLASSES.VS);
      case 'DAMAGED': return pick(CLASSES.DAMAGED);
      case 'DELIMITER': return pick(rnd() < 0.5 ? CLASSES.OPENING : CLASSES.CLOSING);
      case 'ENCLOSURE_OPENING':
        return (rnd() < 0.4 ? pick(CLASSES.OPENING) + (rnd() < 0.3 ? pick(CLASSES.DAMAGED) : '') : '') + pick(CLASSES.BEGIN_ENCLOSURE);
      case 'ENCLOSURE_CLOSING':
        return pick(CLASSES.END_ENCLOSURE) + (rnd() < 0.4 ? pick(CLASSES.CLOSING) + (rnd() < 0.3 ? pick(CLASSES.DAMAGED) : '') : '');
      default:
        if (t in fixed) return fixed[t];
        throw new Error('no text for terminal ' + t);
    }
  };

  return function generate() {
    // Budget of terminals; once spent, always take the smallest derivation.
    let budget = 1 + Math.floor(Math.sqrt(rnd()) * 90);
    const expand = (s, depth) => {
      if (isTerminal(s)) {
        budget--;
        return terminal(s);
      }
      const alts = rules[s];
      let alt;
      if (budget <= 0 || depth > 40) {
        const best = Math.min(...alts.map((a) => a.reduce((t, x) => t + sizeOf(x), 0)));
        alt = pick(alts.filter((a) => a.reduce((t, x) => t + sizeOf(x), 0) === best));
      } else {
        alt = pick(alts);
      }
      return alt.map((x) => expand(x, depth + 1)).join('');
    };
    return expand('fragment', 0);
  };
}
