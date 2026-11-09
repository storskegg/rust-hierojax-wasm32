// The type test of js/hieroxide/index.d.ts: `npm test` in this directory compiles it (nothing runs).
// What the types promise is used here as a caller would, and what they refuse is marked
// `@ts-expect-error`, which is itself an error if the line turns out to compile.
import { init, layout, parse, render, determinismDigest } from '../../js/hieroxide/index.js';
import type { Failure, LayoutResult, Options, ParseResult } from '../../js/hieroxide/index.js';

declare const element: HTMLElement;
declare const bytes: ArrayBuffer;

// init takes a URL or bytes, and says when it is done.
const ready: Promise<void> = init({ font: '/NewGardiner.otf' });
init({ font: new URL('https://example.com/NewGardiner.otf'), wasm: '/hieroxide.wasm' });
init({ font: bytes });
init({ font: new Uint8Array(bytes) });
// @ts-expect-error a font is needed
init({});
// @ts-expect-error a number is not a font
init({ font: 3 });

// Options are the data-* names, and what the output adds.
const opts: Options = { dir: 'hrl', sep: 0.1, linesize: '1.2', separated: true, border: true, svg: 'paths', label: 'a line', selectable: 'overlay', datatext: false, xmlns: true, contents: true, shadepattern: 'hatching', align: 'bottom' };
// @ts-expect-error not a direction
const badDir: Options = { dir: 'up' };
// @ts-expect-error not a kind of output
const badSvg: Options = { svg: 'png' };
// @ts-expect-error contents is a switch, not a name
const badContents: Options = { contents: 'yes' };
// @ts-expect-error not an option
const unknown: Options = { colour: 'red' };

// Errors are values: narrow on `ok`, then on `kind`.
const r: LayoutResult = layout('\u{13000}', opts);
if (r.ok) {
  const markup: string = r.markup;
  const text: string = r.text;
  const logs: string[] = r.logs;
  const border: boolean = r.border;
  // @ts-expect-error no kind on a success
  r.kind;
} else {
  const kind: 'parse' | 'layout' | 'unsupported' = r.kind;
  const message: string = r.message;
  const position: number | undefined = r.position;
  // @ts-expect-error a failure has no markup
  r.markup;
}

// parse says where.
const p: ParseResult = parse('\u{13430}');
if (!p.ok) {
  const at: number = p.position; // always there for a parse error
  const kind: 'parse' = p.kind;
} else {
  const text: string = p.text;
}

// render takes an element, and returns what layout does.
const shown: LayoutResult = render(element, 'x');
render(element, 'x', { dir: 'vlr' });
// @ts-expect-error an element, not a selector
render('#id', 'x');
// @ts-expect-error the text is required
render(element);

const failure: Failure = { ok: false, kind: 'parse', message: 'm', position: 1 };
const digest: string = determinismDigest();
void [ready, opts, badDir, badSvg, unknown, shown, failure, digest];
