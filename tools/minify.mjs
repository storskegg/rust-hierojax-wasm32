// Minifies the JavaScript of a build (the compat script, the consistent module and its glue).
// Used by tools/build-compat.mjs and tools/build-consistent.mjs only: the shipped source in js/
// stays as it is, readable, and nothing at run time depends on this.
//
// The one dependency is uglify-js, a development dependency of tools/ (tools/package.json, pinned
// to an exact version; it has no dependencies of its own). Install it with `npm ci` in tools/.
//
// What is and is not changed: names of local variables and whitespace go, and so do dead branches.
// Property names do not (they are the API, and the glue's names for the module's exports), and a
// classic script's top-level names do not either (they are globals: `hierojax`, `syntax`, ...). The
// `unsafe` options of the compressor are off.
import { createRequire } from 'node:module';

let UglifyJS;
try {
  UglifyJS = createRequire(import.meta.url)('uglify-js');
} catch {
  console.error('tools/minify.mjs: uglify-js is not installed; run `npm ci` in tools/ (or build with --no-minify)');
  process.exit(1);
}

/**
 * @param {string} source JavaScript
 * @param {{module?: boolean}} [options] `module`: an ES module (its top-level names may be renamed; exports stay)
 */
export function minify(source, { module = false } = {}) {
  const r = UglifyJS.minify(source, {
    module,
    compress: { passes: 2 },
    mangle: true,
    output: { comments: false },
  });
  if (r.error) throw r.error;
  return r.code + '\n';
}
