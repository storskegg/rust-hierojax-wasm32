// The shim's test switches (the speed levers, `forcebatch`, the profile hooks) exist only in a build made with
// `tools/build-compat.mjs --test-levers`, which is written next to the shipped one with `-test` on its name:
// build/compat-test beside build/compat, build/embedded-test beside build/embedded. A test that sets levers must
// run one of those; running the shipped build instead would ignore the levers without a word and test nothing.
import fs from 'node:fs';
import path from 'node:path';

export const TEST_MARK = 'HIEROXIDE_TEST_BUILD';

/** Whether the script of the build in `dir` (or the script file) is a test build. */
export function isTestBuild(dir) {
  const file = fs.statSync(dir).isDirectory() ? path.join(dir, 'hierojax.js') : dir;
  return fs.readFileSync(file, 'utf8').includes(TEST_MARK);
}

/** The test build that goes with the shipped build in `dir`; `dir` itself if it is one. Throws if there is none. */
export function requireTestBuild(dir) {
  dir = path.resolve(dir);
  if (isTestBuild(dir)) return dir;
  const sibling = dir + '-test';
  if (fs.existsSync(path.join(sibling, 'hierojax.js')) && isTestBuild(sibling)) return sibling;
  throw new Error(`${dir} is the shipped build, which has no levers to set. Build the test build beside it: `
    + `node tools/build-compat.mjs --test-levers [--embed] --out ${sibling}`);
}
