// The canonicalizer: turns a DOM subtree into one comparable string.
//
// It runs inside the page, in the engine that produced the DOM, so both
// implementations are normalized by the same browser:
//  - attributes are sorted by name (Firefox serializes them in a different order than
//    Chromium and WebKit, so raw outerHTML is not comparable);
//  - `style` is read back through the CSSOM (el.style), not as text, so spacing, case,
//    declaration order and setAttribute-versus-setProperty no longer matter. What the
//    engine does with shorthands is the engine's business; it is the same on both sides;
//  - adjacent text nodes are merged and empty ones dropped;
//  - one node per line, so a line diff reads well.
//
// hjCanon must stay self-contained (no closures, no imports): it is shipped to the page
// as source with Function.prototype.toString.

export function hjCanon(root) {
  const HTML = 'http://www.w3.org/1999/xhtml';
  const SVG = 'http://www.w3.org/2000/svg';
  const lines = [];
  const cmp = (a, b) => (a < b ? -1 : a > b ? 1 : 0);

  const styleOf = (el, raw) => {
    const s = el.style;
    if (!s || typeof s.length !== 'number') return raw;
    const props = [];
    for (let i = 0; i < s.length; i++) {
      const name = s[i];
      const pri = s.getPropertyPriority(name);
      props.push(name + ':' + s.getPropertyValue(name) + (pri ? ' !' + pri : ''));
    }
    props.sort(cmp);
    return props.join(';');
  };

  const walk = (node) => {
    if (node.nodeType === 1) {
      const ns = node.namespaceURI;
      const tag = (ns === HTML || ns == null ? '' : ns === SVG ? 'svg:' : '{' + ns + '}') + node.localName;
      const attrs = [];
      for (const a of node.attributes) {
        attrs.push([a.name, a.name === 'style' ? styleOf(node, a.value) : a.value]);
      }
      attrs.sort((x, y) => cmp(x[0], y[0]));
      lines.push('<' + tag + attrs.map(([k, v]) => ' ' + k + '=' + JSON.stringify(v)).join('') + '>');
      let text = '';
      const flush = () => {
        if (text) lines.push(JSON.stringify(text));
        text = '';
      };
      for (const c of node.childNodes) {
        if (c.nodeType === 3) {
          text += c.data;
        } else {
          flush();
          walk(c);
        }
      }
      flush();
      lines.push('</' + tag + '>');
    } else if (node.nodeType === 8) {
      lines.push('<!--' + node.data + '-->');
    } else if (node.nodeType === 3) {
      if (node.data) lines.push(JSON.stringify(node.data));
    }
  };
  walk(root);
  return lines.join('\n');
}

// Source text for injection into a page: `window.__hjCanon = <source>`.
export const CANON_SRC = hjCanon.toString();
