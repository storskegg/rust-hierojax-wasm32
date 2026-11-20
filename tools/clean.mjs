// Clean before every build (decisions.md; the user, 2026-11-20): every script that builds calls
// cleanFirst() before its first build step, so a build starts from nothing and the clean cannot be skipped.
//
// What is cleaned: this checkout's `cargo clean` (its target/ folder), then each output folder the script
// is about to write. A script removes only its own output folders and not all of build/, so a test build
// can sit beside the shipped one (tests/parity/lib/testbuild.mjs relies on build/compat and
// build/compat-test side by side).
//
// A script that another script starts (release.mjs runs size-check.mjs, which runs build-compat.mjs) must
// not run `cargo clean` again: it would throw away the wasm that the step before it built. So the first
// clean sets HIEROXIDE_CLEANED to the checkout's root, which child processes inherit, and a script that
// finds it set to its own root skips `cargo clean`. Its own output folders are still removed.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

export function cleanFirst(outputs = []) {
  if (process.env.HIEROXIDE_CLEANED !== ROOT) {
    execFileSync('cargo', ['clean'], { cwd: ROOT, stdio: ['ignore', 'inherit', 'inherit'] });
    process.env.HIEROXIDE_CLEANED = ROOT;
  }
  for (const output of outputs) {
    const p = path.resolve(ROOT, output);
    const inside = path.relative(ROOT, p);
    if (inside === '') continue;
    if (inside.startsWith('..') || path.isAbsolute(inside)) {
      console.error(`clean: ${p} is outside this checkout and is left as it is`);
      continue;
    }
    fs.rmSync(p, { recursive: true, force: true });
  }
}
