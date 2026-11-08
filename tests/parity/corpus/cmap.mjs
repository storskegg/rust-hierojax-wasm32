// Code points in an OpenType font's cmap (formats 4 and 12), with no dependencies.
import fs from 'node:fs';

export function cmapCodepoints(file) {
  const b = fs.readFileSync(file);
  const dv = new DataView(b.buffer, b.byteOffset, b.byteLength);
  const numTables = dv.getUint16(4);
  let cmapOff = -1;
  for (let i = 0; i < numTables; i++) {
    const rec = 12 + 16 * i;
    if (b.toString('latin1', rec, rec + 4) === 'cmap') cmapOff = dv.getUint32(rec + 8);
  }
  if (cmapOff < 0) throw new Error('no cmap table in ' + file);
  const n = dv.getUint16(cmapOff + 2);
  const cps = new Set();
  const seen = new Set();
  for (let i = 0; i < n; i++) {
    const off = cmapOff + dv.getUint32(cmapOff + 4 + 8 * i + 4);
    if (seen.has(off)) continue;
    seen.add(off);
    const fmt = dv.getUint16(off);
    if (fmt === 12) {
      const groups = dv.getUint32(off + 12);
      for (let g = 0; g < groups; g++) {
        const s = dv.getUint32(off + 16 + 12 * g), e = dv.getUint32(off + 16 + 12 * g + 4);
        for (let c = s; c <= e; c++) cps.add(c);
      }
    } else if (fmt === 4) {
      const segX2 = dv.getUint16(off + 6);
      const endO = off + 14, startO = endO + segX2 + 2, deltaO = startO + segX2, rangeO = deltaO + segX2;
      for (let s = 0; s < segX2 / 2; s++) {
        const end = dv.getUint16(endO + 2 * s), start = dv.getUint16(startO + 2 * s);
        const ro = dv.getUint16(rangeO + 2 * s);
        for (let c = start; c <= end && c !== 0xffff; c++) {
          let gid = c;
          if (ro !== 0) {
            const gOff = rangeO + 2 * s + ro + 2 * (c - start);
            gid = dv.getUint16(gOff);
            if (gid !== 0) gid = (gid + dv.getInt16(deltaO + 2 * s)) & 0xffff;
          } else {
            gid = (c + dv.getInt16(deltaO + 2 * s)) & 0xffff;
          }
          if (gid !== 0) cps.add(c);
        }
      }
    }
  }
  return [...cps].sort((a, b2) => a - b2);
}
