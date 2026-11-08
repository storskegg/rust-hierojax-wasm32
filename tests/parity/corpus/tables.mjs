// Read upstream's own tables (and its jison parser, as an accept/reject oracle) by
// evaluating docs/hierojax.js in a bare Node vm with just enough stubs for its
// constructor. Nothing is rendered; only data is read.
import vm from 'node:vm';
import fs from 'node:fs';

export function loadUpstream(file) {
  const src = fs.readFileSync(file, 'utf8');
  const ctx = vm.createContext({
    console: { log() {}, error() {} },
    setTimeout() {},
    window: {},
    document: { fonts: { add() {} }, createElement: () => ({ getContext: () => ({}) }) },
    FontFace: class {
      load() {
        return new Promise(() => {});
      }
    },
    alert() {},
  });
  vm.runInContext(src, ctx, { filename: 'hierojax.js' });
  const get = (e) => vm.runInContext(e, ctx);
  // Plain copies, so the vm's objects do not leak out.
  const copy = (e) => JSON.parse(get(`JSON.stringify(${e})`));
  return {
    insertions: copy('Shapes.insertions'),
    ligatures: copy('Shapes.ligatures'),
    rotations: copy('Shapes.rotations'),
    rotatedChars: copy('Shapes.rotatedChars'),
    // true if upstream's parser accepts the text
    accepts(text) {
      ctx.__t = text;
      try {
        get('syntax.parse(__t)');
        return true;
      } catch {
        return false;
      }
    },
  };
}
