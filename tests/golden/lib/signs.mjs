// Reads a fragment's output into what can be compared: for each `svg`, its box and the place of each
// sign and shading, in px at the given font size. Takes upstream's canonical DOM (a line per node, as
// tests/parity/lib/canon.mjs prints it) and consistent mode's markup.

const attrsOf = (s) => Object.fromEntries([...s.matchAll(/([-\w]+)="([^"]*)"/g)].map((m) => [m[1], m[2]]));

const MATRIX = /translate\(([-\d.e]+) ([-\d.e]+)\) scale\(([-\d.e]+) ([-\d.e]+)\)(?: rotate\(([-\d.e]+)\))?/;

function walk(tokens, unitToPx) {
  // tokens: { open: name, attrs } | { close: name } | { text }
  const svgs = [];
  let svg = null, parents = [], cur = null;
  for (const t of tokens) {
    if (t.open) {
      const a = t.attrs;
      if (t.open === 'svg') {
        svg = { w: parseFloat(a.width), h: parseFloat(a.height), em: /em$/.test(a.width ?? ''), signs: [], shades: [] };
        if (a.viewBox) {
          const v = a.viewBox.split(' ').map(Number);
          svg.w = v[2] * unitToPx;
          svg.h = v[3] * unitToPx;
        }
        svgs.push(svg);
      } else if (t.open === 'rect' && svg) {
        svg.shades.push(['x', 'y', 'width', 'height'].map((k) => Number(a[k]) * unitToPx));
      } else if (t.open === 'text' || t.open === 'tspan') {
        const hidden = (a.class ?? '').includes('hidden');
        const style = /font-size:\s*([-\d.e]+)px/.exec(a.style ?? '');
        const fs = a['font-size'] !== undefined ? Number(a['font-size']) : style ? Number(style[1]) : NaN;
        const reversed = t.open === 'text' && a.transform === 'scale(-1,1)';
        const m = a.transform ? MATRIX.exec(a.transform) : null;
        const sign = {
          kind: t.open,
          hidden,
          reversed,
          x: (Number(a.x ?? 0) + Number(a.dx ?? 0)) * unitToPx,
          y: (Number(a.y ?? 0) + Number(a.dy ?? 0)) * unitToPx,
          fs: fs * unitToPx,
          tx: m ? Number(m[1]) * unitToPx : 0,
          ty: m ? Number(m[2]) * unitToPx : 0,
          sx: m ? Number(m[3]) : 1,
          sy: m ? Number(m[4]) : 1,
          rot: m && m[5] ? Number(m[5]) : 0,
          text: '',
          parentReversed: parents.some((p) => p.reversed),
        };
        parents.push(sign);
        cur = sign;
      }
    } else if (t.close) {
      if (t.close === 'text' || t.close === 'tspan') {
        const s = parents.pop();
        if (s && svg && !s.hidden && s.text && !Number.isNaN(s.fs) && (s.kind === 'tspan' || s.fs > 0)) svg.signs.push(s);
        cur = parents[parents.length - 1] ?? null;
      }
    } else if (cur) {
      cur.text += t.text;
    }
  }
  for (const s of svgs) {
    for (const g of s.signs) {
      const r = (g.rot * Math.PI) / 180;
      const [c, n] = [Math.cos(r), Math.sin(r)];
      // The anchor on the page: the text position through the sign's own transform.
      g.ax = g.tx + g.sx * (c * g.x - n * g.y);
      g.ay = g.ty + g.sy * (n * g.x + c * g.y);
      if (g.parentReversed) {
        g.ax = -g.ax;
        g.sx = -g.sx;
      }
    }
  }
  return svgs;
}

/** Upstream's canonical html, at its own font size. */
export function parseCanonical(html) {
  const tokens = [];
  for (const l of html.split('\n')) {
    const open = /^<svg:(\w+)(.*)>$/.exec(l);
    if (open) tokens.push({ open: open[1], attrs: attrsOf(open[2]) });
    else if (/^<\/svg:/.test(l)) tokens.push({ close: l.slice(6, -1) });
    else if (l.startsWith('"')) {
      try {
        tokens.push({ text: JSON.parse(l) });
      } catch {
        tokens.push({ text: l.slice(1, -1) });
      }
    }
  }
  return walk(tokens, 1);
}

/** Consistent mode's markup, at a font size in px. */
export function parseMarkup(markup, px) {
  const tokens = [];
  const re = /<(\/?)([a-zA-Z]+)((?:\s+[-\w]+="[^"]*")*)\s*>|([^<]+)/g;
  let m;
  let inStyle = false;
  while ((m = re.exec(markup))) {
    if (m[4] !== undefined) {
      if (!inStyle) tokens.push({ text: m[4].replace(/&#13;/g, '\r').replace(/&lt;/g, '<').replace(/&amp;/g, '&') });
    } else if (m[1]) {
      if (m[2] === 'style') inStyle = false;
      tokens.push({ close: m[2] });
    } else {
      if (m[2] === 'style') inStyle = true;
      tokens.push({ open: m[2], attrs: attrsOf(m[3]) });
    }
  }
  return walk(tokens, px / 1000);
}

/**
 * How far apart two parsed outputs are, in px. Upstream's box is the ceiling of the real size and its
 * shadings are rounded, so those are compared after doing the same to the other.
 * Returns { max, structural }.
 */
export function difference(up, ours) {
  if (up.length !== ours.length) return { max: Infinity, structural: true };
  let max = 0;
  for (let i = 0; i < up.length; i++) {
    const [u, o] = [up[i], ours[i]];
    if (u.signs.length !== o.signs.length || u.shades.length !== o.shades.length) return { max: Infinity, structural: true };
    // Upstream's box is the ceiling of a size we have to 1/10000 em: any value in that pixel will do.
    const outside = (b, x) => (b >= x - 0.01 && b < x + 1.01 ? 0 : Math.abs(b - Math.ceil(x)));
    max = Math.max(max, outside(u.w, o.w), outside(u.h, o.h));
    u.signs.forEach((s, k) => {
      const t = o.signs[k];
      max = Math.max(max, Math.abs(s.ax - t.ax), Math.abs(s.ay - t.ay), Math.abs(s.fs - t.fs));
    });
    u.shades.forEach((r, k) => {
      const q = o.shades[k];
      const [x0, y0, x1, y1] = [Math.round(q[0]), Math.round(q[1]), Math.round(q[0] + q[2]), Math.round(q[1] + q[3])];
      max = Math.max(max, Math.abs(r[0] - x0), Math.abs(r[1] - y0), Math.abs(r[0] + r[2] - x1), Math.abs(r[1] + r[3] - y1));
    });
  }
  return { max: Math.round(max * 1e4) / 1e4, structural: false };
}
