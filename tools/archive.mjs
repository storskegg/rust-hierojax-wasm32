// Deterministic .zip and .tar.gz writers for tools/release.mjs: the same entries give the same bytes on every
// machine (sorted names, one fixed time, fixed modes and owners, no platform fields), so an artifact can be
// rebuilt and compared. Entries are `{ name, data: Buffer, mode?: 0o644 }`.
import fs from 'node:fs';
import zlib from 'node:zlib';

const sorted = (entries) => [...entries].sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));

const crcTable = new Uint32Array(256).map((_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
function crc32(buf) {
  let c = 0xffffffff;
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

// `epoch` is in seconds. A zip stores local time from 1980; the time is written as UTC fields, so it does not
// depend on the machine's zone.
export function writeZip(file, entries, epoch) {
  const t = new Date(Math.max(epoch, 315532800) * 1000);
  const dosTime = (t.getUTCHours() << 11) | (t.getUTCMinutes() << 5) | (t.getUTCSeconds() >> 1);
  const dosDate = ((t.getUTCFullYear() - 1980) << 9) | ((t.getUTCMonth() + 1) << 5) | t.getUTCDate();
  const parts = [];
  const central = [];
  let offset = 0;
  for (const e of sorted(entries)) {
    const name = Buffer.from(e.name);
    const deflated = zlib.deflateRawSync(e.data, { level: 9 });
    const store = deflated.length >= e.data.length;
    const body = store ? e.data : deflated;
    const method = store ? 0 : 8;
    const crc = crc32(e.data);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4); // version needed
    local.writeUInt16LE(0x0800, 6); // flags: UTF-8 names
    local.writeUInt16LE(method, 8);
    local.writeUInt16LE(dosTime, 10);
    local.writeUInt16LE(dosDate, 12);
    local.writeUInt32LE(crc, 14);
    local.writeUInt32LE(body.length, 18);
    local.writeUInt32LE(e.data.length, 22);
    local.writeUInt16LE(name.length, 26);
    parts.push(local, name, body);
    const c = Buffer.alloc(46);
    c.writeUInt32LE(0x02014b50, 0);
    c.writeUInt16LE((3 << 8) | 20, 4); // made by: Unix, 2.0
    c.writeUInt16LE(20, 6);
    c.writeUInt16LE(0x0800, 8);
    c.writeUInt16LE(method, 10);
    c.writeUInt16LE(dosTime, 12);
    c.writeUInt16LE(dosDate, 14);
    c.writeUInt32LE(crc, 16);
    c.writeUInt32LE(body.length, 20);
    c.writeUInt32LE(e.data.length, 24);
    c.writeUInt16LE(name.length, 28);
    c.writeUInt32LE(((0o100000 | (e.mode ?? 0o644)) << 16) >>> 0, 38); // Unix mode in the external attributes
    c.writeUInt32LE(offset, 42);
    central.push(c, name);
    offset += local.length + name.length + body.length;
  }
  const centralBytes = Buffer.concat(central);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(entries.length, 8);
  end.writeUInt16LE(entries.length, 10);
  end.writeUInt32LE(centralBytes.length, 12);
  end.writeUInt32LE(offset, 16);
  fs.writeFileSync(file, Buffer.concat([...parts, centralBytes, end]));
}

export function writeTarGz(file, entries, epoch) {
  const blocks = [];
  for (const e of sorted(entries)) {
    const name = Buffer.from(e.name);
    if (name.length > 100) throw new Error(`tar: name too long: ${e.name}`);
    const h = Buffer.alloc(512);
    name.copy(h, 0);
    h.write((e.mode ?? 0o644).toString(8).padStart(7, '0') + '\0', 100, 'ascii');
    h.write('0000000\0', 108, 'ascii'); // uid
    h.write('0000000\0', 116, 'ascii'); // gid
    h.write(e.data.length.toString(8).padStart(11, '0') + '\0', 124, 'ascii');
    h.write(epoch.toString(8).padStart(11, '0') + '\0', 136, 'ascii');
    h.write('        ', 148, 'ascii'); // checksum, as spaces while summing
    h.write('0', 156, 'ascii'); // regular file
    h.write('ustar\0' + '00', 257, 'ascii');
    let sum = 0;
    for (const b of h) sum += b;
    h.write(sum.toString(8).padStart(6, '0') + '\0 ', 148, 'ascii');
    blocks.push(h, e.data, Buffer.alloc((512 - (e.data.length % 512)) % 512));
  }
  blocks.push(Buffer.alloc(1024));
  const gz = zlib.gzipSync(Buffer.concat(blocks), { level: 9 });
  gz[4] = gz[5] = gz[6] = gz[7] = 0; // mtime
  gz[9] = 3; // OS: Unix, whatever built it
  fs.writeFileSync(file, gz);
}
