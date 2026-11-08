// Oracle for the parser differential test.
//
// Loads the checked-in jison parser (src/syntax.js) together with the real
// classes from src/formatting.js (and the data files they read), unmodified
// and at run time, in a vm context with a stubbed `document`.
//
// Usage: node oracle.mjs INPUT OUTPUT [COVERAGE.json]
//
// INPUT  one test input per line: its UTF-16 code units as 4 hex digits each,
//        concatenated (an empty line is the empty string).
// OUTPUT one line per input, tab-separated:
//          R                          the parser threw (reject)
//          A <TAB> toString <TAB> dump  accepted
//        toString is the code units of Fragment.toString(), same encoding as INPUT.
//        If the parser threw something that is not a jison parse/lex error
//        (a bug in a constructor, say) the line is "R!" followed by the message.
//
// Tree dump (class name plus constructor-supplied fields, fixed order; "hex" is the
// lowercase code point, no padding; lists are comma separated, no spaces):
//   Fragment    F[g,...]
//   Vertical    V[g,...]
//   Horizontal  H[g,...]
//   Enclosure   E<w|p>(delimOpen|-,damageOpen,delimClose|-,damageClose)[g,...]
//                 w = walled, p = plain; delimiters as hex or '-' for null
//   Basic       B[core|ts=g|bs=g|te=g|be=g|m=g|t=g|b=g]   places in that order, null ones omitted
//   Overlay     O[lit,...][lit,...]
//   Literal     L<hex>.<vs>.<mirror 0|1>.<damage>
//   Singleton   S<hex>.<damage>
//   Blank       Z<dim>                (1 or 0.5)
//   Lost        X<width>,<height>,<expand>   (expand is the raw variation number 0..7)
//   BracketOpen BO<hex>      BracketClose BC<hex>
//
// If COVERAGE.json is given, the number of reductions of each jison production over
// all accepted and rejected inputs is written there (index = production number).

import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';

const SRC = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../src');
const read = f => fs.readFileSync(path.join(SRC, f), 'utf8');

const noop = () => {};
const ctx = vm.createContext({
  console: { log: noop, error: noop, warn: noop },
  FontFace: class { constructor() {} load() { return { then: noop }; } },
  document: { fonts: { add: noop }, createElement() { throw new Error('no dom'); } },
  window: {}, navigator: { platform: 'MacIntel' }, setTimeout, alert: noop,
});
// The same order as src/Makefile, without main.js.
const bundle = ['util.js', 'syntax.js', 'formatting.js', 'insertions.js',
  'standardizedvariants.js', 'ligatures.js'].map(read).join('\n ;\n') +
  '\n;globalThis.__o = { syntax, Fragment, Vertical, Horizontal, Enclosure, Basic, Overlay,' +
  ' Literal, Singleton, Blank, Lost, BracketOpen, BracketClose, Group };';
vm.runInContext(bundle, ctx, { filename: 'bundle.js' });
const { syntax, Fragment, Vertical, Horizontal, Enclosure, Basic, Overlay, Literal, Singleton,
  Blank, Lost, BracketOpen, BracketClose, Group } = ctx.__o;

const hex = s => s.codePointAt(0).toString(16);
const hexOrDash = s => (s ? hex(s) : '-');
const list = gs => gs.map(dump).join(',');

function dump(g) {
  if (g instanceof Fragment) return 'F[' + list(g.groups) + ']';
  if (g instanceof Vertical) return 'V[' + list(g.groups) + ']';
  if (g instanceof Horizontal) return 'H[' + list(g.groups) + ']';
  if (g instanceof Enclosure)
    return 'E' + (g.type === 'walled' ? 'w' : 'p') + '(' + hexOrDash(g.delimOpen) + ',' +
      g.damageOpen + ',' + hexOrDash(g.delimClose) + ',' + g.damageClose + ')[' + list(g.groups) + ']';
  if (g instanceof Basic) {
    let s = 'B[' + dump(g.core);
    for (const p of Group.INSERTION_PLACES) if (g[p]) s += '|' + p + '=' + dump(g[p]);
    return s + ']';
  }
  if (g instanceof Overlay) return 'O[' + list(g.lits1) + '][' + list(g.lits2) + ']';
  if (g instanceof Literal) return 'L' + hex(g.ch) + '.' + g.vs + '.' + (g.mirror ? 1 : 0) + '.' + g.damage;
  if (g instanceof Singleton) return 'S' + hex(g.ch) + '.' + g.damage;
  if (g instanceof Blank) return 'Z' + g.dim;
  if (g instanceof Lost) return 'X' + g.width + ',' + g.height + ',' + g.expand;
  if (g instanceof BracketOpen) return 'BO' + hex(g.ch);
  if (g instanceof BracketClose) return 'BC' + hex(g.ch);
  throw new Error('unknown class');
}

const unhex = line => {
  let s = '';
  for (let i = 0; i < line.length; i += 4) s += String.fromCharCode(parseInt(line.slice(i, i + 4), 16));
  return s;
};
const enc = s => {
  let h = '';
  for (let i = 0; i < s.length; i++) h += s.charCodeAt(i).toString(16).padStart(4, '0');
  return h;
};

// Count reductions per production.
const counts = new Array(syntax.productions_.length).fill(0);
const orig = syntax.performAction;
syntax.performAction = function (yytext, yyleng, yylineno, yy, yystate) {
  counts[yystate]++;
  return orig.apply(this, arguments);
};

const [inFile, outFile, covFile] = process.argv.slice(2);
const lines = fs.readFileSync(inFile, 'utf8').split('\n');
if (lines.length && lines[lines.length - 1] === '') lines.pop();
const out = [];
for (const line of lines) {
  const input = unhex(line);
  try {
    const f = syntax.parse(input);
    out.push('A\t' + enc(f.toString()) + '\t' + dump(f));
  } catch (e) {
    const m = String(e && e.message);
    out.push(/^(Parse error|Lexical error)/.test(m) ? 'R' : 'R!' + m.replace(/\s+/g, ' ').slice(0, 80));
  }
}
fs.writeFileSync(outFile, out.join('\n') + '\n');
if (covFile) fs.writeFileSync(covFile, JSON.stringify(counts));
