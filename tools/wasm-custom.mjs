// Appends a custom section to a WebAssembly module, in pure JavaScript, so a module can carry a note (the release
// puts the URL of its source in a section named `source`). A custom section is the section id 0, the size of what
// follows (LEB128), the length of the name (LEB128), the name (UTF-8) and the payload. A custom section may stand
// anywhere in a module, so the end is the simplest place: nothing before it moves, and the same input gives the
// same bytes.

function leb128(n) {
  const out = [];
  do {
    let byte = n & 0x7f;
    n >>>= 7;
    if (n !== 0) byte |= 0x80;
    out.push(byte);
  } while (n !== 0);
  return Buffer.from(out);
}

export function appendCustomSection(bytes, name, payload) {
  if (bytes.length < 8 || bytes.readUInt32BE(0) !== 0x0061736d) throw new Error('not a WebAssembly module');
  const nameBytes = Buffer.from(name, 'utf8');
  const body = Buffer.concat([leb128(nameBytes.length), nameBytes, Buffer.from(payload)]);
  return Buffer.concat([bytes, Buffer.from([0]), leb128(body.length), body]);
}
