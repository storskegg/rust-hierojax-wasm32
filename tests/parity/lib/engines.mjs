// Browser engines (Playwright's cached builds; nothing is downloaded here).
import { chromium, firefox, webkit } from 'playwright';
import os from 'node:os';

export const ENGINES = { chromium, firefox, webkit };
export const ENGINE_NAMES = Object.keys(ENGINES);

// Firefox's default timer rounding is irrelevant to parity, so no prefs are set unless asked for.
export function launch(engine, { prefs } = {}) {
  const type = ENGINES[engine];
  if (!type) throw new Error(`unknown engine ${engine}; expected one of ${ENGINE_NAMES.join(', ')}`);
  const opts = {};
  if (prefs && Object.keys(prefs).length) {
    if (engine !== 'firefox') throw new Error('--prefs applies to firefox only');
    opts.firefoxUserPrefs = prefs;
  }
  return type.launch(opts);
}

export function platformTag() {
  const p = { darwin: 'mac', linux: 'linux', win32: 'windows' }[os.platform()] ?? os.platform();
  return `${p}-${os.arch()}`;
}
