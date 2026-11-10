#!/usr/bin/env node
// Run the harness in one browser that Playwright cannot drive, and wait for its result.
//
//   node tests/devices/run.mjs safari                       desktop Safari, through safaridriver
//   node tests/devices/run.mjs ios [--device NAME|UDID]     Mobile Safari in the iOS Simulator
//   node tests/devices/run.mjs android --serial emulator-5554 [--no-lock]
//                                                           Chrome on that adb device
//   common: [--label NAME] [--port 8765] [--query 'n=full&reps=7'] [--timeout MIN]
//
// Each starts the server (server.mjs), points the browser at /run.html, waits for the page to POST
// its result, and prints the table row. Results are in tests/devices/out/.
//
// Safari: needs `safaridriver --enable` once (it asks for sudo; this script never does it). It drives its own
// automation window and nothing of your own Safari. If remote automation is off, the script prints
// safaridriver's message and stops.
// Android: takes the shared-device lock (~/d/storskegg/inkhk3-shared/bin/device) for an emulator, and
// releases it afterwards, whatever happens; --no-lock for a device you hold yourself (a Pixel on USB).
// Only Chrome is opened; no other app is touched.
import { spawn, execFile } from 'node:child_process';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';
import { parseArgs } from 'node:util';
import { startDeviceServer } from './server.mjs';

const exec = promisify(execFile);
const { values: v, positionals } = parseArgs({
  allowPositionals: true,
  options: {
    label: { type: 'string' }, port: { type: 'string', default: '8765' }, query: { type: 'string', default: '' },
    timeout: { type: 'string', default: '30' }, serial: { type: 'string' }, device: { type: 'string' },
    'no-lock': { type: 'boolean', default: false }, 'safari-port': { type: 'string', default: '4445' },
  },
});
const target = positionals[0];
if (!['safari', 'ios', 'android'].includes(target)) {
  console.error('usage: node run.mjs safari|ios|android [--serial S] [--device D] [--label L] [--query Q]');
  process.exit(2);
}
const label = v.label ?? target;
const stamp = new Date().toISOString().replace(/[-:]/g, '').slice(0, 15);
const id = `${stamp}-${label}`;
const sh = async (cmd, args, opts) => {
  const r = await exec(cmd, args, { maxBuffer: 20e6, ...opts });
  return r.stdout.trim();
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const LOCK = path.join(os.homedir(), 'd/storskegg/inkhk3-shared/bin/device');

const server = await startDeviceServer({ port: Number(v.port) });
const url = `http://localhost:${server.port}/run.html?label=${encodeURIComponent(label)}&id=${encodeURIComponent(id)}${v.query ? '&' + v.query : ''}`;
let cleanup = async () => {};
let exitCode = 0;
// Echo the page's progress lines while waiting.
const ticker = setInterval(() => { for (const lines of server.logs.values()) if (lines.length) console.log('  page: ' + lines.splice(0).join(' | ')); }, 2000);

// Ctrl-C or a kill still releases the lock and the reverse.
for (const sig of ['SIGINT', 'SIGTERM']) process.on(sig, async () => { await cleanup(); process.exit(130); });
const hint = setTimeout(() => { if (!server.logs.size) console.log('no word from the page after 90 s: is the browser showing a first-run or consent screen? Dismiss it by hand (this script does not accept anything on the device).'); }, 90000);

try {
  if (target === 'safari') {
    const port = v['safari-port'];
    const drv = spawn('safaridriver', ['-p', port], { stdio: ['ignore', 'pipe', 'pipe'] });
    let drvOut = '';
    drv.stdout.on('data', (d) => (drvOut += d));
    drv.stderr.on('data', (d) => (drvOut += d));
    const wd = `http://127.0.0.1:${port}`;
    let session;
    cleanup = async () => {
      if (session) await fetch(`${wd}/session/${session}`, { method: 'DELETE' }).catch(() => {});
      drv.kill();
    };
    for (let i = 0; i < 50; i++) { try { await fetch(`${wd}/status`); break; } catch { await sleep(100); } }
    const r = await fetch(`${wd}/session`, { method: 'POST', body: JSON.stringify({ capabilities: { alwaysMatch: { browserName: 'safari' } } }) });
    const body = await r.json();
    if (!r.ok || body.value?.error) {
      console.error('safaridriver refused the session:\n' + JSON.stringify(body.value ?? body) + (drvOut ? '\n' + drvOut : ''));
      console.error('If remote automation is off, run `safaridriver --enable` once (sudo) and tick Safari > Develop > Allow Remote Automation; this script does not.');
      exitCode = 3;
      throw new Error('blocked');
    }
    session = body.value.sessionId;
    console.log(`safari ${body.value.capabilities.browserVersion}, session ${session}`);
    await fetch(`${wd}/session/${session}/url`, { method: 'POST', body: JSON.stringify({ url }) });
  } else if (target === 'ios') {
    const list = JSON.parse(await sh('xcrun', ['simctl', 'list', 'devices', '--json']));
    const all = Object.entries(list.devices).flatMap(([rt, ds]) => ds.map((d) => ({ ...d, runtime: rt })));
    const booted = all.filter((d) => d.state === 'Booted' && /iPhone/.test(d.name));
    let dev = v.device ? all.find((d) => d.udid === v.device || d.name === v.device) : booted[0];
    if (!dev) {
      dev = all.filter((d) => d.isAvailable && /iPhone/.test(d.name) && /iOS/.test(d.runtime)).sort((a, b) => b.runtime.localeCompare(a.runtime) || a.name.localeCompare(b.name))[0];
      if (!dev) { console.error('no iPhone simulator is available (xcrun simctl list devices)'); exitCode = 3; throw new Error('blocked'); }
    }
    console.log(`simulator ${dev.name} (${dev.runtime.replace(/.*SimRuntime\./, '')}, ${dev.udid}), ${dev.state}`);
    if (dev.state !== 'Booted') { await sh('xcrun', ['simctl', 'boot', dev.udid]); await sh('xcrun', ['simctl', 'bootstatus', dev.udid, '-b'], { timeout: 300000 }); }
    await sh('open', ['-a', 'Simulator']).catch(() => {});
    await sh('xcrun', ['simctl', 'openurl', dev.udid, url]);
  } else {
    if (!v.serial) throw new Error('android needs --serial (an emulator-* or a device you hold)');
    const serial = v.serial;
    const emulator = /^emulator-/.test(serial);
    if (!emulator && !v['no-lock']) throw new Error('only an emulator is locked by this script; for a physical device take the lock yourself and pass --no-lock');
    const adb = (...a) => sh('adb', ['-s', serial, ...a]);
    let locked = false;
    cleanup = async () => {
      await adb('reverse', '--remove', `tcp:${server.port}`).catch(() => {});
      if (locked) {
        const out = await sh(LOCK, ['unlock', serial, 'glyph']).catch((e) => 'UNLOCK FAILED ' + e.message);
        console.log(`${new Date().toISOString()} unlock ${serial}: ${out}`);
      }
    };
    if (!v['no-lock']) {
      console.log('lock status: ' + (await sh(LOCK, ['status']).catch((e) => e.message)).replace(/\n/g, ' | '));
      const deadline = Date.now() + 20 * 60 * 1000;
      for (;;) {
        try {
          const out = await sh(LOCK, ['lock', serial, 'glyph', 'hieroxide browser tests']);
          locked = true;
          console.log(`${new Date().toISOString()} lock ${serial}: ${out}`);
          break;
        } catch (e) {
          if (Date.now() > deadline) { console.error('the lock stayed held for 20 minutes: ' + e.message); exitCode = 3; throw new Error('blocked'); }
          console.log('lock held, waiting: ' + String(e.stderr || e.message).trim().slice(0, 200));
          await sleep(30000);
        }
      }
    }
    const pkgs = await adb('shell', 'pm', 'list', 'packages', 'com.android.chrome');
    if (!/com\.android\.chrome/.test(pkgs)) { console.error('Chrome (com.android.chrome) is not installed on ' + serial); exitCode = 3; throw new Error('blocked'); }
    const ver = (await adb('shell', 'dumpsys', 'package', 'com.android.chrome')).match(/versionName=(\S+)/)?.[1];
    console.log(`${serial}: Chrome ${ver}`);
    await adb('reverse', `tcp:${server.port}`, `tcp:${server.port}`);
    await adb('shell', 'am', 'start', '-a', 'android.intent.action.VIEW', '-d', `'${url}'`, 'com.android.chrome');
  }
  console.log(`opened ${url}`);
  const { file, data } = await server.waitForResult(id, Number(v.timeout) * 60000);
  console.log(`result: ${file}`);
  if (data.errors?.length) console.log('errors in the run: ' + data.errors.join(' ; '));
} catch (e) {
  if (e.message !== 'blocked') { console.error(e.message); exitCode = exitCode || 1; }
} finally {
  clearInterval(ticker);
  clearTimeout(hint);
  await cleanup();
  await server.close();
}
process.exit(exitCode);
