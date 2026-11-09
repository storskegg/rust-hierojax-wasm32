// Types of the `hieroxide` module (index.js). 0.x: not frozen until reviewed.
//
//   import { init, parse, layout, render } from 'hieroxide';
//   await init({ font: '/NewGardiner.otf' });
//   const r = render(element, '\u{13000}\u{13431}\u{13001}', { dir: 'hrl' });
//   if (!r.ok) console.log(r.kind, r.message);
//
// Errors are values: nothing here throws but `init`, whose promise rejects.

/** The font and, optionally, where the module's wasm is. */
export interface InitOptions {
  /** The font: a URL (fetched), or the bytes of the unmodified NewGardiner.otf. */
  font: string | URL | ArrayBuffer | ArrayBufferView;
  /** Where the module's `.wasm` is, if not beside the script. */
  wasm?: string | URL | Response | BufferSource;
}

/**
 * The options of a fragment: those of hierojax's `data-*` attributes under their names, then the
 * output's. A value that cannot be used (a size that is not a finite positive number) is the
 * default. Numbers may be given as strings.
 */
export interface Options {
  /** The CSS font size of the block in px, if not the page's. It does not affect layout, which is in em. */
  fontsize?: number | string;
  /** A CSS colour for signs; default the page's text colour. */
  signcolor?: string;
  /** Direction of writing; default `'hlr'`. */
  dir?: 'hlr' | 'hrl' | 'vlr' | 'vrl';
  /** Space between lines or columns, in em; default 1. */
  linesize?: number | string;
  /** Space between groups, in em; default 0.07. */
  sep?: number | string;
  /** One `svg` per group, so that the line can break between them. */
  separated?: boolean;
  /** A CSS colour for brackets; default black. */
  bracketcolor?: string;
  /** How shaded (damaged) areas are drawn; default `'uniform'`. */
  shadepattern?: 'uniform' | 'hatching';
  /** Where signs sit on the line; default `'middle'`. */
  align?: 'middle' | 'bottom';
  /** A border of 1px. */
  border?: boolean;
  /** Text output: the `svg` carries its own style, with the font's URL as upstream's does. */
  standalone?: boolean;
  /** Collect upstream's messages into `logs`. */
  log?: boolean;
  /**
   * `'text'` (default): signs as text in the font; the page needs the font and the stylesheet
   * `init` adds. `'paths'`: signs as paths, self-contained, with copyable text; only in the build with
   * paths (see `Failure`: `'unsupported'`).
   */
  svg?: 'text' | 'paths';
  /** Paths: the accessible name of the block (`aria-label`); default `'Egyptian hieroglyphs'`. */
  label?: string;
  /**
   * Paths: how a selection of the page copies the text. `'overlay'` (default): HTML beside the `svg`,
   * in a `span` that wraps both; copies exactly in every browser. `'svg'`: SVG text inside a bare
   * `svg`; Chromium and WebKit add a line break before and after it. `'none'`.
   */
  selectable?: 'overlay' | 'svg' | 'none';
  /** Paths: write the text as `data-text` on the block, for a script to read; default true. */
  datatext?: boolean;
  /** Paths: `xmlns` on each `svg`, so that the markup is also an SVG file; default false. */
  xmlns?: boolean;
}

export type ErrorKind = 'parse' | 'layout' | 'unsupported';

/** Why a fragment gave no result. */
export interface Failure {
  ok: false;
  /**
   * `'parse'`: not a hieroglyph fragment (`position` says where); `'layout'`: it parsed but has no
   * finite size; `'unsupported'`: this build has no path output.
   */
  kind: ErrorKind;
  message: string;
  /** For `'parse'`: where, in UTF-16 code units from the start of the text (an index into the string). */
  position?: number;
}

export interface ParseFailure extends Failure {
  kind: 'parse';
  position: number;
}

export interface ParseOk {
  ok: true;
  text: string;
}

export interface LayoutOk {
  ok: true;
  /** The markup. For text, the `svg`s of the fragment; for paths, a whole block (an `svg`, or a `span` holding it). */
  markup: string;
  /** The fragment as given: what is copied from a block. */
  text: string;
  /** For text: the class, size and border of the element that holds the markup, as the stylesheet knows them. Empty for paths. */
  className: string;
  width: string;
  height: string;
  border: boolean;
  logs: string[];
}

export type ParseResult = ParseOk | ParseFailure;
export type LayoutResult = LayoutOk | Failure;

/**
 * Load the module and the font; text is then drawn in the font from this page too. Safe to call
 * again: the first call counts. Rejects with an `Error` whose `kind` is `'font'` if the font or the
 * module does not load.
 */
export function init(options: InitOptions): Promise<void>;

/** Whether a fragment parses. Lays nothing out; usable in a Worker. */
export function parse(text: string): ParseResult;

/** Lay a fragment out, without touching a page; usable in a Worker. */
export function layout(text: string, options?: Options): LayoutResult;

/**
 * Lay a fragment out and put it in an element, replacing what is there. On an error the element is
 * left as it was. Returns what `layout` returns.
 */
export function render(element: Element, text: string, options?: Options): LayoutResult;

/** For the reference tests: a digest of the module's own `pow`, `log`, `sin` and `cos`. */
export function determinismDigest(): string;
