// The device harness, in the browser: runs the checks and POSTs one JSON result to /result.
// Query: label=NAME  n=2500|full (parity sample)  reps=5 (speed rounds)  golden=0 (skip)
//        speed=0 (skip)  id=RUNID (default: a timestamp and the label)
'use strict';
(function () {
  const q = new URLSearchParams(location.search);
  const label = (q.get('label') || 'run').replace(/[^\w.-]/g, '_');
  const runId = q.get('id') || `${new Date().toISOString().replace(/[-:]/g, '').slice(0, 15)}-${label}`;
  const statusEl = document.getElementById('status');
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const med = (a) => { const s = a.slice().sort((x, y) => x - y); const n = s.length; return n ? (n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2) : null; };
  const r1 = (x) => (x == null ? null : Math.round(x * 10) / 10);
  const errStr = (e) => (e && e.name ? e.name + ': ' + e.message : String(e));

  const lines = [];
  const say = (msg) => {
    lines.push(msg);
    statusEl.textContent = lines.slice(-14).join('\n');
    // Best effort: progress for whoever watches the server.
    try { fetch(`/log?id=${encodeURIComponent(runId)}`, { method: 'POST', body: msg }).catch(() => {}); } catch (e) { /* ignore */ }
  };

  // Console errors and uncaught errors of this page (the paths check wants none).
  const pageErrors = [];
  window.addEventListener('error', (e) => pageErrors.push('error: ' + (e.message || e.type)));
  window.addEventListener('unhandledrejection', (e) => pageErrors.push('unhandledrejection: ' + errStr(e.reason)));
  const origErr = console.error;
  console.error = function () {
    pageErrors.push('console.error: ' + Array.prototype.map.call(arguments, (a) => { try { return String(a); } catch (e) { return '?'; } }).join(' '));
    return origErr.apply(console, arguments);
  };

  const getJson = async (url) => {
    const r = await fetch(url);
    if (!r.ok) throw new Error(`${url}: ${r.status}`);
    return r.json();
  };

  // ---- frames: upstream and compat each run in their own document of this page ----------------
  // A fresh frame per batch, as tests/parity/lib/runner.mjs uses a fresh page per batch, so that
  // upstream's measurement memo starts alike on both sides.
  async function openFrame(kind) {
    const f = document.createElement('iframe');
    f.style.cssText = 'position:absolute;left:0;top:0;width:1280px;height:800px;border:0;visibility:hidden';
    f.src = `/frame-${kind}.html`;
    await new Promise((resolve, reject) => {
      f.onload = resolve;
      f.onerror = () => reject(new Error('frame ' + kind + ' did not load'));
      document.body.appendChild(f);
    });
    const w = f.contentWindow;
    for (let i = 0; i < 3000; i++) {
      if (w.eval('typeof hierojax !== "undefined" && hierojax.nFontsLoaded == hierojax.nFonts')) break; // a top-level const, not a window property
      if (i === 2999) throw new Error(`${kind}: the font did not load in 60 s`);
      await sleep(20);
    }
    await w.document.fonts.ready;
    return { frame: f, win: w };
  }

  // ---- 1. compat parity -----------------------------------------------------------------------
  // The comparison of tests/parity/compare.mjs: the same fields, JSON-equal, with the text of a
  // logged parse error normalized (the --ignore-parse-error-text ruling), and its allowlist
  // (the "depth" group: the parser's limit on nesting).
  const FIELDS = ['html', 'canvases', 'rects', 'sel', 'text', 'console', 'alerts', 'err', 'crash', 'captureErr'];
  const PARSE_ERROR = /^(log: Parse error:\n[\s\S]*?Error: )(?:Lexical error|Parse error|parse error)[\s\S]*$/;
  const normParse = (r) => (r && Array.isArray(r.console) ? Object.assign({}, r, { console: r.console.map((m) => m.replace(PARSE_ERROR, '$1<parse error>')) }) : r);
  const ALLOWED_GROUPS = { depth: 'the parser accepts at most 256 levels of nesting' };

  function lineDiff(a, b) {
    const x = String(a).split('\n'), y = String(b).split('\n');
    let p = 0;
    while (p < x.length && p < y.length && x[p] === y[p]) p++;
    const cut = (s) => (s && s.length > 160 ? s.slice(0, 160) + '...' : s);
    return { line: p + 1, upstream: cut(x[p]), compat: cut(y[p]) };
  }

  async function checkParity() {
    const corpus = await getJson('/data/parity.json?n=' + encodeURIComponent(q.get('n') || '2500'));
    const items = corpus.items;
    say(`parity: ${items.length} fragments of ${corpus.total}`);
    const t0 = performance.now();
    const out = { fragments: items.length, corpusTotal: corpus.total, corpusSha256: corpus.sha256, identical: 0, differing: 0, allowlisted: 0, open: 0, maxRectDiff: 0, fields: {}, firstDiffs: [] };
    const good = [];
    const BATCH = 200;
    for (let i = 0; i < items.length; i += BATCH) {
      const chunk = items.slice(i, i + BATCH);
      const res = {};
      // Alternate which side goes first, so neither always has the cold start.
      const order = (i / BATCH) % 2 ? ['compat', 'upstream'] : ['upstream', 'compat'];
      for (const kind of order) {
        const { frame, win } = await openFrame(kind);
        try {
          res[kind] = win.__runBatch(chunk);
        } finally {
          frame.remove();
        }
        if (kind === 'compat') out.engine = out.engine || engineOf(win);
      }
      chunk.forEach((it, j) => {
        const ra = normParse(res.upstream[j]), rb = normParse(res.compat[j]);
        const diff = FIELDS.filter((f) => JSON.stringify(ra[f] ?? null) !== JSON.stringify(rb[f] ?? null));
        if (!diff.length) {
          out.identical++;
          if (!ra.err && !(ra.console || []).some((m) => /Parse error/.test(m)) && (it.group === 'texts' || it.group === 'testsuite') && !it.attrs['data-type']) good.push(it);
          return;
        }
        out.differing++;
        const allowed = Object.prototype.hasOwnProperty.call(ALLOWED_GROUPS, it.group);
        if (allowed) out.allowlisted++; else out.open++;
        diff.forEach((f) => { out.fields[f] = (out.fields[f] || 0) + 1; });
        if (diff.includes('rects') && ra.rects && rb.rects) {
          const fa = [].concat.apply([], ra.rects), fb = [].concat.apply([], rb.rects);
          if (fa.length === fb.length) for (let k = 0; k < fa.length; k++) out.maxRectDiff = Math.max(out.maxRectDiff, Math.abs(fa[k] - fb[k]));
        }
        if (out.firstDiffs.length < 5) {
          const d = { id: it.id, group: it.group, class: it.class, fields: diff, allowlisted: allowed, text: it.text.slice(0, 40) };
          if (diff.includes('html')) d.html = lineDiff(ra.html, rb.html);
          if (diff.includes('rects')) d.rects = { upstream: ra.rects && ra.rects[0], compat: rb.rects && rb.rects[0] };
          for (const f of ['console', 'err', 'sel', 'text', 'captureErr']) if (diff.includes(f)) d[f] = { upstream: JSON.stringify(ra[f]).slice(0, 160), compat: JSON.stringify(rb[f]).slice(0, 160) };
          out.firstDiffs.push(d);
        }
      });
      if ((i / BATCH) % 4 === 3) say(`parity: ${Math.min(i + BATCH, items.length)}/${items.length}, ${out.differing} differing so far`);
    }
    out.maxRectDiff = Math.round(out.maxRectDiff * 1e6) / 1e6;
    out.seconds = r1((performance.now() - t0) / 1000);
    window.__speedItems = good;
    return out;
  }

  // The engine compat detected: a top-level const of its script, so a global lexical binding.
  function engineOf(win) {
    try { return win.eval('typeof hieroxideEngine === "undefined" ? null : hieroxideEngine'); } catch (e) { return 'unreadable: ' + errStr(e); }
  }

  // ---- 2. consistent golden ---------------------------------------------------------------------
  // FNV-1a, 64 bits, as tests/golden/engines.mjs and native.rs; in two 32-bit halves (the prime is
  // 2^40 + 0x1b3), which is much faster than BigInt in every engine.
  const enc = new TextEncoder();
  function fnv64(bytes) {
    let lo = 0x84222325, hi = 0xcbf29ce4;
    for (let i = 0; i < bytes.length; i++) {
      lo = (lo ^ bytes[i]) >>> 0;
      const t = lo * 0x1b3;
      const carry = Math.floor(t / 4294967296);
      const nlo = t % 4294967296;
      hi = (hi * 0x1b3 + carry + ((lo << 8) >>> 0)) % 4294967296;
      lo = nlo;
    }
    return hi.toString(16).padStart(8, '0') + lo.toString(16).padStart(8, '0');
  }

  let consistent;
  async function loadConsistent() {
    if (!consistent) {
      consistent = await import('/consistent/index.js');
      await consistent.init({ font: '/NewGardiner.otf' });
      await document.fonts.ready;
    }
    return consistent;
  }

  async function checkGolden() {
    const t0 = performance.now();
    const ref = await getJson('/data/golden.json');
    const m = await loadConsistent();
    const digest = m.determinismDigest();
    const out = { fragments: ref.items.length, expectedAll: ref.manifest.all, expectedDigest: ref.manifest.digest, digest, digestOk: digest === ref.manifest.digest, differ: 0, firstDiffs: [] };
    // The reference test compares each record's hash; its manifest's "all" is the hash of those
    // hashes, one 16-digit line each. Both are checked here.
    let allText = '';
    for (let i = 0; i < ref.items.length; i++) {
      const it = ref.items[i];
      const r = m.layout(it[3], it[2]);
      let rec;
      if (!r.ok) rec = r.kind === 'parse' ? 'parse' : 'failed';
      else rec = ['ok', r.className, r.width, r.height, String(r.border), r.logs.join('\u0001'), r.markup].join('\n');
      const h = fnv64(enc.encode(rec));
      allText += h + '\n';
      if (h !== ref.hashes[i]) {
        out.differ++;
        if (out.firstDiffs.length < 5) out.firstDiffs.push({ id: it[0], want: ref.hashes[i], got: h, rec: rec.slice(0, 300) });
      }
      if (i % 2000 === 1999) { say(`golden: ${i + 1}/${ref.items.length}, ${out.differ} differ`); await sleep(0); }
    }
    out.all = fnv64(enc.encode(allText));
    out.allOk = out.all === ref.manifest.all;
    out.ok = out.allOk && out.digestOk && out.differ === 0;
    out.seconds = r1((performance.now() - t0) / 1000);
    return out;
  }

  // ---- 3. paths output -----------------------------------------------------------------------------
  async function checkPaths() {
    const ref = await getJson('/data/golden.json');
    const pick = [];
    const wantClass = ['plain', 'overlay', 'bracket'];
    for (const c of wantClass) { const it = ref.items.find((x) => x[1] === c && x[3].length > 3); if (it) pick.push(it); }
    for (const it of ref.items) { if (pick.length >= 5) break; if (!pick.includes(it) && it[2].dir === 'vrl') pick.push(it); }
    for (const it of ref.items) { if (pick.length >= 5) break; if (!pick.includes(it) && it[1] === 'plain') pick.push(it); }
    const errsBefore = pageErrors.length;
    const out = { blocks: [], ok: true };
    const m = await import('/paths/index.js');
    await m.init({ font: '/NewGardiner.otf' });
    const host = document.getElementById('paths');
    for (const it of pick) {
      const el = document.createElement('div');
      host.appendChild(el);
      const r = m.render(el, it[3], Object.assign({}, it[2], { svg: 'paths' }));
      const svg = el.querySelector('svg');
      const paths = el.querySelectorAll('path').length;
      const box = svg ? svg.getBoundingClientRect() : null;
      const b = { id: it[0], renderOk: !!r.ok, svgChars: svg ? svg.outerHTML.length : 0, paths, width: box ? r1(box.width) : 0, height: box ? r1(box.height) : 0, drawn: false };
      b.drawn = b.renderOk && b.svgChars > 0 && paths > 0 && b.width > 0 && b.height > 0;
      if (!b.drawn) { out.ok = false; if (!r.ok) b.message = String(r.message).slice(0, 120); }
      out.blocks.push(b);
    }
    await sleep(50);
    out.consoleErrors = pageErrors.slice(errsBefore);
    if (out.consoleErrors.length) out.ok = false;
    host.textContent = '';
    return out;
  }

  // ---- 4. speed -----------------------------------------------------------------------------------
  // As tests/bench/ab.mjs: the call that does the whole page's fragments, and the layout still owed
  // after it (a forced one); their sum. Rounds are interleaved, and the median of the rounds is reported.
  const esc = (t) => t.replace(/&/g, '&amp;').replace(/</g, '&lt;');
  const escAttr = (t) => esc(t).replace(/"/g, '&quot;');

  async function timeCompat(kind, items) {
    const { frame, win } = await openFrame(kind);
    try {
      const doc = win.document;
      doc.body.innerHTML = items.map((it) => `<div style="font-size:${it.fontSize};color:${it.color}"><span class="hierojax"${Object.keys(it.attrs).map((k) => ` ${k}="${escAttr(it.attrs[k])}"`).join('')}>${esc(it.text)}</span></div>`).join('\n');
      void doc.body.offsetHeight;
      const t0 = performance.now();
      win.eval('hierojax.processFragmentsNow()');
      const t1 = performance.now();
      void doc.body.offsetHeight;
      const t2 = performance.now();
      return { call: t1 - t0, layout: t2 - t1, total: t2 - t0 };
    } finally {
      frame.remove();
    }
  }

  async function timeConsistent(items) {
    const m = await loadConsistent();
    const host = document.createElement('div');
    host.style.cssText = 'position:absolute;left:0;top:0;width:1200px;visibility:hidden';
    const spans = items.map((it) => {
      const d = document.createElement('div');
      d.style.fontSize = it.fontSize;
      d.style.color = it.color;
      const s = document.createElement('span');
      d.appendChild(s);
      host.appendChild(d);
      return s;
    });
    document.body.appendChild(host);
    try {
      const opts = items.map((it) => { const o = {}; for (const k in it.attrs) o[k.replace(/^data-/, '')] = it.attrs[k]; return o; });
      void host.offsetHeight;
      const t0 = performance.now();
      for (let i = 0; i < items.length; i++) m.render(spans[i], items[i].text, opts[i]);
      const t1 = performance.now();
      void host.offsetHeight;
      const t2 = performance.now();
      return { call: t1 - t0, layout: t2 - t1, total: t2 - t0 };
    } finally {
      host.remove();
    }
  }

  async function checkSpeed() {
    const good = window.__speedItems || [];
    const want = 300;
    const items = good.length > want ? Array.from({ length: want }, (_, i) => good[Math.floor((i * good.length) / want)]) : good;
    if (!items.length) throw new Error('no fragments to time (the parity check did not run, or nothing passed)');
    const reps = Number(q.get('reps')) || 5;
    const rounds = { upstream: [], compat: [], consistent: [] };
    await loadConsistent();
    // One untimed pass of each: the first use of a module pays for compiling it.
    await timeConsistent(items.slice(0, 20));
    const kinds = ['upstream', 'compat', 'consistent'];
    for (let r = 0; r < reps; r++) {
      const rot = kinds.slice(r % 3).concat(kinds.slice(0, r % 3));
      for (const k of rot) rounds[k].push(k === 'consistent' ? await timeConsistent(items) : await timeCompat(k, items));
      say(`speed: round ${r + 1}/${reps}`);
    }
    const sum = (k, f) => r1(med(rounds[k].map((x) => x[f])));
    const out = { fragments: items.length, reps, unit: 'ms, median per round: call + layout' };
    for (const k of kinds) out[k] = { total: sum(k, 'total'), call: sum(k, 'call'), layout: sum(k, 'layout'), min: r1(Math.min.apply(null, rounds[k].map((x) => x.total))), max: r1(Math.max.apply(null, rounds[k].map((x) => x.total))) };
    out.compatVsUpstream = r1((out.compat.total / out.upstream.total) * 100) / 100;
    out.consistentVsUpstream = r1((out.consistent.total / out.upstream.total) * 100) / 100;
    return out;
  }

  // ---- 5. environment -------------------------------------------------------------------------------
  function environment() {
    const uad = navigator.userAgentData;
    return {
      userAgent: navigator.userAgent,
      platform: navigator.platform,
      userAgentData: uad ? { brands: uad.brands, mobile: uad.mobile, platform: uad.platform } : null,
      devicePixelRatio: window.devicePixelRatio,
      screen: [screen.width, screen.height],
      inner: [innerWidth, innerHeight],
      hardwareConcurrency: navigator.hardwareConcurrency || null,
      touchPoints: navigator.maxTouchPoints || 0,
      language: navigator.language,
    };
  }

  async function step(name, result, fn) {
    say(`${name}: start`);
    try {
      result[name] = await fn();
      say(`${name}: done`);
    } catch (e) {
      result[name] = { error: errStr(e) };
      result.errors.push(`${name}: ${errStr(e)}`);
      say(`${name}: ERROR ${errStr(e)}`);
    }
  }

  async function main() {
    const t0 = performance.now();
    const result = { id: runId, label, startedAt: new Date().toISOString(), search: location.search, env: environment(), errors: [] };
    await step('parity', result, checkParity);
    result.env.hieroxideEngine = result.parity && result.parity.engine !== undefined ? result.parity.engine : null;
    if (q.get('golden') !== '0') await step('golden', result, checkGolden);
    await step('paths', result, checkPaths);
    if (q.get('speed') !== '0') await step('speed', result, checkSpeed);
    result.seconds = r1((performance.now() - t0) / 1000);
    result.finishedAt = new Date().toISOString();
    say(`finished in ${result.seconds} s; posting`);
    const r = await fetch(`/result?id=${encodeURIComponent(runId)}`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(result) });
    say(r.ok || r.status === 204 ? 'result posted. Done.' : 'POST FAILED ' + r.status);
  }

  main().catch((e) => { say('FATAL ' + errStr(e)); fetch(`/result?id=${encodeURIComponent(runId)}`, { method: 'POST', body: JSON.stringify({ id: runId, label, fatal: errStr(e), env: environment() }) }).catch(() => {}); });
})();
