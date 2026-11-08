// What runs inside the page: hooks installed before the implementation loads, and the
// per-batch function that renders fragments and captures everything about them.
import { CANON_SRC } from './canon.mjs';

// Installed before any page script. console.* and alert are captured instead of printed
// (upstream reports parse errors with console.log, and a lost font with alert).
export const INIT_SCRIPT = `
(() => {
  window.__hjCanon = ${CANON_SRC};
  const hj = (window.__hj = { msgs: [], alerts: [] });
  for (const t of ['log', 'info', 'warn', 'error', 'debug']) {
    console[t] = (...a) => { hj.msgs.push(t + ': ' + a.map((x) => { try { return String(x); } catch (e) { return '[unprintable]'; } }).join(' ')); };
  }
  window.alert = (m) => { hj.alerts.push(String(m)); };
})();
`;

/**
 * Render each item in a fresh wrapper and capture the result.
 * item: { id, text, attrs: {data-*: value}, fontSize: '33.6px', color: 'rgb(0, 0, 0)' }
 * The real page context is its data-* attributes plus the computed font-size and colour,
 * which is all processFragmentNow reads, so the span gets exactly those.
 * Must be self-contained: Playwright ships it to the page as source.
 */
export function runBatch(items) {
  const hj = window.__hj;
  const canon = window.__hjCanon;

  const hash64 = (u32) => {
    let h1 = 0xdeadbeef | 0, h2 = 0x41c6ce57 | 0;
    for (let i = 0; i < u32.length; i++) {
      const k = u32[i];
      h1 = Math.imul(h1 ^ k, 2654435761);
      h2 = Math.imul(h2 ^ k, 1597334677);
    }
    h1 = Math.imul(h1 ^ (h1 >>> 16), 2246822507) ^ Math.imul(h2 ^ (h2 >>> 13), 3266489909);
    h2 = Math.imul(h2 ^ (h2 >>> 16), 2246822507) ^ Math.imul(h1 ^ (h1 >>> 13), 3266489909);
    return (h2 >>> 0).toString(16).padStart(8, '0') + (h1 >>> 0).toString(16).padStart(8, '0');
  };
  const rect = (el, o) => {
    const r = el.getBoundingClientRect();
    return [r.left - o.left, r.top - o.top, r.width, r.height];
  };
  const errStr = (e) => (e && e.name ? e.name + ': ' + e.message : String(e));

  const out = [];
  for (const it of items) {
    hj.msgs.length = 0;
    hj.alerts.length = 0;
    const box = document.createElement('div');
    box.style.position = 'relative';
    box.style.width = '1200px';
    const span = document.createElement('span');
    span.className = 'hierojax';
    for (const k of Object.keys(it.attrs)) span.setAttribute(k, it.attrs[k]);
    span.style.fontSize = it.fontSize;
    span.style.color = it.color;
    span.textContent = it.text;
    box.appendChild(span);
    document.body.appendChild(box);

    let error = null;
    try {
      hierojax.processFragmentNow(span);
    } catch (e) {
      error = errStr(e);
    }

    const rec = { id: it.id, err: error };
    // A replacement may decline a fragment it cannot lay out yet and say why in
    // hierojax.lastUnsupported; compare.mjs leaves such fragments out and counts them.
    if (!error && typeof hierojax.lastUnsupported === 'string') rec.unsupported = hierojax.lastUnsupported;
    try {
      rec.html = canon(span);
      const o = box.getBoundingClientRect();
      rec.rects = [rect(span, o)].concat([...span.children].map((c) => rect(c, o)));
      rec.canvases = [...span.querySelectorAll('canvas')].map((c) => {
        const r = { w: c.width, h: c.height };
        try {
          const d = c.getContext('2d').getImageData(0, 0, c.width, c.height).data;
          r.hash = hash64(new Uint32Array(d.buffer, d.byteOffset, d.byteLength >> 2));
        } catch (e) {
          r.hash = 'ERR ' + errStr(e);
        }
        return r;
      });
      const sel = window.getSelection();
      sel.removeAllRanges();
      const range = document.createRange();
      range.selectNodeContents(span);
      sel.addRange(range);
      rec.sel = sel.toString();
      sel.removeAllRanges();
      rec.text = span.textContent;
    } catch (e) {
      rec.captureErr = errStr(e);
    }
    rec.console = hj.msgs.slice();
    rec.alerts = hj.alerts.slice();
    box.remove();
    out.push(rec);
  }
  return out;
}
