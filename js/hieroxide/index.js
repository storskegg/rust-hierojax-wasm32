// Hieroxide: the new API, in consistent mode (a first version, 0.x: index.d.ts has the types).
//
//   import { init, parse, layout, render } from 'hieroxide';
//   await init({ font: '/NewGardiner.otf' });     // or an ArrayBuffer or Uint8Array of it
//   const r = render(element, text, { dir: 'hrl' });
//   if (!r.ok) console.log(r.kind, r.message);
//
// Layout comes from the font's outlines, so the markup is the same in every browser and at every
// font size: its boxes are in em, and follow the CSS font size around them. Nothing here uses
// a canvas, and nothing throws but `init`, whose promise rejects: errors are values.
//
// `svg: 'paths'` prints signs as paths, for a page with no font and no stylesheet: that needs the
// build with the Cargo feature `paths` (build/consistent-paths); in the default build it is an error
// of kind 'unsupported'.
import wasmInit, { Hieroxide, determinism_digest_hex } from './hieroxide_outline.js';

// In the order of the core's DATA_ATTRIBUTES, as the names of `render`'s options, then the extra ones
// of the module (`label`, `svg`, `selectable`, `datatext`, `xmlns`, `contents`: those of path output).
const OPTIONS = ['fontsize', 'signcolor', 'dir', 'linesize', 'sep', 'separated', 'type', 'bracketcolor',
  'shadepattern', 'align', 'border', 'standalone', 'log', 'label', 'svg', 'selectable', 'datatext', 'xmlns', 'contents'];

// What upstream's hierojax.css says about the output, so that a page needs no stylesheet. The font
// is named as in that stylesheet, so the two can be on one page.
const CSS = `.hierojax{font-family:Hieroglyphic;display:inline-block;position:relative;vertical-align:-0.1em;text-indent:0}
.hierojax-hlr-container{}
.hierojax-hrl-container{display:inline-block;direction:rtl}
.hierojax-hrl-container svg{direction:ltr}
.hierojax-v-container{display:inline-flex;flex-direction:column;vertical-align:top}
.hierojax-border{border:solid 1px black}
.hierojax-svg-sign{font-family:Hieroglyphic}
.hierojax-svg-visual{font-family:Hieroglyphic;user-select:none;-webkit-user-select:none;-ms-user-select:none;-moz-user-select:none}
.hierojax-svg-hatching{stroke:gray}
.hierojax-svg-uniform{fill:darkgray}
.hierojax-svg-hidden{fill-opacity:0;stroke-opacity:0;color:transparent;font-size:0;user-select:all}`;

let engine = null;
let pending = null;

const toBytes = async (font) => {
  if (font instanceof Uint8Array) return font;
  if (font instanceof ArrayBuffer) return new Uint8Array(font);
  if (ArrayBuffer.isView(font)) return new Uint8Array(font.buffer, font.byteOffset, font.byteLength);
  if (typeof font === 'string' || font instanceof URL) {
    const res = await fetch(font);
    if (!res.ok) throw new Error(`hieroxide: the font did not load: ${res.status} ${res.url}`);
    return new Uint8Array(await res.arrayBuffer());
  }
  throw new TypeError('hieroxide: init needs { font }: a URL or the bytes of the font');
};

/**
 * Load the module and the font. One fetch of the font serves both the page's text (`FontFace`)
 * and the layout. Safe to call again: the first call counts.
 * @param {{font: string|URL|ArrayBuffer|ArrayBufferView, wasm?: string|URL|Response|BufferSource}} options
 * @returns {Promise<void>}
 */
export function init(options) {
  pending ??= (async () => {
    const { font, wasm } = options ?? {};
    const [bytes] = await Promise.all([toBytes(font), wasmInit(wasm === undefined ? undefined : { module_or_path: wasm })]);
    engine = new Hieroxide(bytes);
    if (typeof FontFace !== 'undefined' && typeof document !== 'undefined') {
      const face = await new FontFace('Hieroglyphic', bytes).load();
      document.fonts.add(face);
      if (!document.querySelector('style[data-hieroxide]')) {
        const style = document.createElement('style');
        style.dataset.hieroxide = '';
        style.textContent = CSS;
        document.head.prepend(style);
      }
    }
  })().catch((e) => {
    pending = null;
    e.kind = 'font';
    throw e;
  });
  return pending;
}

const needEngine = () => {
  if (!engine) throw new Error('hieroxide: call and await init first');
  return engine;
};

const attrs = (o = {}) => OPTIONS.map((k) => (o[k] === undefined || o[k] === null ? '' : String(o[k])));

const units = (text) => {
  const u = new Uint16Array(text.length);
  for (let i = 0; i < text.length; i++) u[i] = text.charCodeAt(i);
  return u;
};

const fail = (e, status) => ({ ok: false, kind: status === 1 ? 'parse' : status === 4 ? 'unsupported' : 'layout', message: e.message(), position: status === 1 ? e.position() : undefined });

/**
 * Whether a fragment parses. Usable in a Worker.
 * @returns {{ok: true, text: string} | {ok: false, kind: 'parse', message: string, position: number}}
 */
export function parse(text) {
  const e = needEngine();
  return e.render(units(text), attrs({ svg: 'parse' })) ? fail(e, 1) : { ok: true, text };
}

/**
 * Lay out a fragment, without touching a page. Usable in a Worker.
 * @returns {{ok: true, markup: string, text: string, className: string, width: string, height: string, border: boolean, logs: string[]}
 *          | {ok: false, kind: 'parse'|'layout'|'unsupported', message: string, position?: number}}
 */
export function layout(text, options) {
  const e = needEngine();
  const status = e.render(units(text), attrs(options));
  if (status !== 0) return fail(e, status);
  return {
    ok: true,
    markup: e.markup(),
    text,
    className: e.container_class(),
    width: e.container_width(),
    height: e.container_height(),
    border: e.container_border(),
    logs: e.logs(),
  };
}

/**
 * Lay out a fragment and put it in an element, replacing what is there. On an error the element
 * stays as it was. Returns what `layout` returned.
 */
export function render(element, text, options) {
  const r = layout(text, options);
  if (!r.ok) return r;
  if (options?.svg !== 'paths') element.classList.add('hierojax');
  if (r.className) element.classList.add(r.className);
  if (r.width) element.style.width = r.width;
  if (r.height) element.style.height = r.height;
  if (r.border) element.classList.add('hierojax-border');
  element.innerHTML = r.markup;
  return r;
}

/** For the reference tests: see `tests/golden/`. */
export const determinismDigest = () => determinism_digest_hex();
