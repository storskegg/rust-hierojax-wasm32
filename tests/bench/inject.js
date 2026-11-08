
;(function () {
  // Appended to the implementation's script by the benchmark runner. It wraps the entry
  // points so stage times can be read after the page has run, and it is installed after the
  // script body, before DOMContentLoaded. Wrappers for pieces an implementation does not
  // have (Fragment, syntax) are skipped, and the matching totals are then null.
  const HEAVY = !!window.__HEAVY;
  const S = (window.__S = {
    heavy: HEAVY, stages: false, fragments: 0, canvases: 0, getContext: 0, getImageData: 0, fillText: 0, measureText: 0,
    tCreate: 0, tGetCtx: 0, tSize: 0, tGID: 0, tFill: 0, tMeasure: 0, pixelsRead: 0, frag: [], t: {},
  });
  const now = () => performance.now();
  S.cur = null;

  if (HEAVY) {
    const ce = document.createElement;
    document.createElement = function (tag, ...r) {
      if (String(tag).toLowerCase() !== 'canvas') return ce.call(this, tag, ...r);
      const t0 = now(); const c = ce.call(this, tag, ...r); S.tCreate += now() - t0; S.canvases++; return c;
    };
    const gc = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function (...a) { const t0 = now(); const c = gc.apply(this, a); S.tGetCtx += now() - t0; S.getContext++; return c; };
    for (const dim of ['width', 'height']) {
      const d = Object.getOwnPropertyDescriptor(HTMLCanvasElement.prototype, dim);
      Object.defineProperty(HTMLCanvasElement.prototype, dim, { get: d.get, set(v) { const t0 = now(); d.set.call(this, v); S.tSize += now() - t0; }, configurable: true });
    }
    const P = CanvasRenderingContext2D.prototype;
    const gid = P.getImageData;
    P.getImageData = function (...a) { const t0 = now(); const r = gid.apply(this, a); S.tGID += now() - t0; S.getImageData++; S.pixelsRead += a[2] * a[3]; return r; };
    const ft = P.fillText;
    P.fillText = function (...a) { const t0 = now(); const r = ft.apply(this, a); S.tFill += now() - t0; S.fillText++; return r; };
    const mt = P.measureText;
    P.measureText = function (...a) { const t0 = now(); const r = mt.apply(this, a); S.tMeasure += now() - t0; S.measureText++; return r; };
  }

  if (typeof syntax !== 'undefined' && typeof Fragment !== 'undefined') {
    S.stages = true;
    const sp = syntax.parse;
    syntax.parse = function (...a) {
      const t0 = now();
      try { return sp.apply(this, a); } finally { if (S.cur) S.cur.parse += now() - t0; }
    };
    const ff = Fragment.prototype.format;
    Fragment.prototype.format = function (...a) {
      const t0 = now();
      try { return ff.apply(this, a); } finally { if (S.cur) S.cur.format += now() - t0; }
    };
    const fp = Fragment.prototype.print;
    Fragment.prototype.print = function (...a) {
      const t0 = now();
      try { return fp.apply(this, a); } finally { if (S.cur) S.cur.print += now() - t0; }
    };
  }

  // The wasm build cannot be wrapped from here (parse, format and print are inside the
  // module); its shim times the phases when asked and leaves them in hierojax.lastTimings.
  const COMPAT = typeof hierojax !== 'undefined' && 'lastTimings' in hierojax;
  if (COMPAT) {
    S.stages = true;
    hierojax.profile = true;
  }

  const INS = /[\u{13432}-\u{13435}\u{13439}-\u{1343B}]/u, ENC = /[\u{1343C}-\u{1343F}]/u, OVL = /\u{13436}/u;
  // Per fragment: the outermost of processFragmentNow (one element at a time) and writeFragment
  // (batched runs, where the reads of every element happen before any of them is written, so
  // the per-fragment total leaves them out; the page totals do not).
  function wrapFragment(name) {
    const pfn = HieroJax.prototype[name];
    if (typeof pfn !== 'function') return;
    HieroJax.prototype[name] = function (elem, ...rest) {
      if (S.cur) return pfn.call(this, elem, ...rest);
      const txt = elem.textContent;
      S.cur = { parse: 0, format: 0, print: 0 };
      const t0 = now();
      try { return pfn.call(this, elem, ...rest); } finally {
        const dt = now() - t0;
        if (COMPAT && hierojax.lastTimings) {
          const lt = hierojax.lastTimings;
          S.cur.parse = lt[0]; S.cur.format = lt[1]; S.cur.print = lt[1] + lt[2];
        }
        // [total, parse, format, print (includes format), has insertion, has enclosure or overlay, code points]
        S.frag.push([dt, S.cur.parse, S.cur.format, S.cur.print, INS.test(txt) ? 1 : 0, ENC.test(txt) || OVL.test(txt) ? 1 : 0, [...txt].length]);
        S.fragments++; S.cur = null;
      }
    };
  }
  wrapFragment('processFragmentNow');
  wrapFragment('writeFragment');
  const pfs = HieroJax.prototype.processFragmentsNow;
  HieroJax.prototype.processFragmentsNow = function (...a) {
    S.t.startNow = now();
    try { return pfs.apply(this, a); } finally {
      S.t.endNow = now();
      // The style and layout work still owed when the call returns (the page needs it before
      // it can paint): not in totalMs, reported as layoutAfterMs.
      void document.documentElement.offsetHeight;
      S.t.endLayout = now();
      window.__done = true;
    }
  };
  S.F = window.__F;
})();
