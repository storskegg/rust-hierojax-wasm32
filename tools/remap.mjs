// The environment for a cargo build that keeps the build machine's paths out of its output: panic locations and
// debug strings name source files, and without a remap they can carry the home folder, the checkout,
// `~/.cargo/registry` and the toolchain's folder (issue #9). `trim-paths` is not stable in cargo 1.99.1, so this
// passes `--remap-path-prefix` in `CARGO_ENCODED_RUSTFLAGS` (which wins over `RUSTFLAGS` and `[build] rustflags`;
// what those hold is kept). A path that matches several prefixes takes the last, so the most specific come last.
import { execFileSync } from 'node:child_process';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

export function remapPrefixes() {
  const cargoHome = process.env.CARGO_HOME ?? path.join(os.homedir(), '.cargo');
  const rustupHome = process.env.RUSTUP_HOME ?? path.join(os.homedir(), '.rustup');
  const sysroot = execFileSync('rustc', ['--print', 'sysroot'], { cwd: ROOT }).toString().trim();
  return [
    [os.homedir(), '~'],
    [rustupHome, '/rustup'],
    [cargoHome, '/cargo'],
    [sysroot, '/rustup/toolchain'],
    [ROOT, '/hieroxide'],
  ];
}

// The environment to give cargo: `{ ...process.env, ...remapEnv() }`.
export function remapEnv() {
  const own = (process.env.CARGO_ENCODED_RUSTFLAGS ?? process.env.RUSTFLAGS?.trim().split(/\s+/).join('\x1f') ?? '').split('\x1f').filter(Boolean);
  const flags = [...own, ...remapPrefixes().map(([from, to]) => `--remap-path-prefix=${from}=${to}`)];
  return { CARGO_ENCODED_RUSTFLAGS: flags.join('\x1f') };
}

// The strings a shipped file must not hold: what `remapPrefixes` replaces, and the usual folders of a home.
export function forbiddenPaths() {
  return [...new Set([...remapPrefixes().map(([from]) => from), '/Users/', '/home/', '.rustup', '.cargo'])];
}
