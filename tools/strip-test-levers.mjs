// The shim's test switches live between `//#test-levers begin` and `//#test-levers end` lines (js/compat/hierojax.js).
// A shipped build has those regions cut out whole; a test build (`tools/build-compat.mjs --test-levers`) keeps them,
// so the two builds are the same code but for the regions. Both fail on a region that is not closed.
//
// Run before anything that reads the shim as code (a minifier included), since the markers are comments.
const BEGIN = /^\s*\/\/#test-levers begin\s*$/;
const END = /^\s*\/\/#test-levers end\s*$/;

/** @param {string} text the shim's source @param {boolean} keep true for a test build @returns {string} */
export function stripTestLevers(text, keep) {
  const out = [];
  let inside = false;
  let n = 0;
  for (const line of text.split('\n')) {
    if (BEGIN.test(line)) {
      if (inside) throw new Error('test-levers: a region begins inside another');
      inside = true;
      n++;
      if (keep) out.push(line);
    } else if (END.test(line)) {
      if (!inside) throw new Error('test-levers: an end without a begin');
      inside = false;
      if (keep) out.push(line);
    } else if (!inside || keep) {
      out.push(line);
    }
  }
  if (inside) throw new Error('test-levers: a region is not closed');
  if (n === 0) throw new Error('test-levers: the shim has no regions; this tool and the shim have come apart');
  const result = out.join('\n');
  // The word that tells a test build from a shipped one must be in the one and only in the one.
  if (keep !== result.includes('HIEROXIDE_TEST_BUILD')) throw new Error('test-levers: HIEROXIDE_TEST_BUILD is in the wrong build');
  if (!keep && /HIEROXIDE_LEVERS|hieroxideLevers|forcebatch|[?&]hx=/.test(result.replace(/\/\/.*$/gm, ''))) {
    throw new Error('test-levers: a switch is left in the shipped build outside the regions');
  }
  return result;
}
