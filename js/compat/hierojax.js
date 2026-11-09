// Hieroxide compat: a drop-in for upstream's hierojax.js, for the rendering API.
//
// A classic script. tools/build-compat.mjs appends it to the wasm-bindgen glue (which
// defines `wasm_bindgen`) and writes the result as build/compat/hierojax.js. There are two
// builds: one that loads hieroxide_wasm_bg.wasm next to the script (and so is ready some
// time after the script runs), and one with the module inside the script as base64 and
// compiled at once (`HIEROXIDE_WASM_B64` is defined before this code), which is ready as
// upstream's is; see README.md.
//
// It keeps upstream's `HieroJax` class and its order of work: `processFragmentNow` reads the
// element as upstream does, hands the text and options to the wasm module, and applies what
// comes back: the container changes and either one markup string (svg, dom) or the canvas
// operations, which a loop here replays on real canvases with the same calls upstream
// makes. The public surface is `hierojax.*`, `syntax.parse(txt)` (an object with `toString()`
// and `print(element, options)`), `PrintedSVG.EXTERNAL_FONT`, and the helpers of util.js.
//
// Site options, off unless set before this script runs, as `window.HIEROJAX_CONFIG = {...}`:
//   fontUrl      the URL of the font, for the `FontFace` and for `PrintedSVG.EXTERNAL_FONT`
//   fontFormat   the `format()` named for it in a standalone SVG's style (default opentype)
//   svgScalable  every SVG gets a viewBox of `0 0 width height`, and, if the font size is
//                above 0, a width and height in em
//   autoProcess  call `hierojax.processFragments()` at DOMContentLoaded
//   noAlert      on a font that does not load, log to the console instead of `alert`
//
// A fragment the module cannot lay out (a data-sep so large that upstream would never finish)
// is left empty and recorded in `hierojax.lastUnsupported`, which the test harness reads.
//
// An exception from a canvas (`getImageData` on a canvas with a side of 0, say) is caught at
// the import, travels up through the module as a value, and is thrown again from
// `processFragmentNow` after whatever upstream had written by then has been applied.

// An inline script (the benchmarks inject one) has no URL: the module is looked for next to the page.
const hieroxideScriptUrl = (document.currentScript && document.currentScript.src) || location.href;
const hieroxideConfig = window.HIEROJAX_CONFIG || {};
const hieroxideEmbedded = typeof HIEROXIDE_WASM_B64 === 'string';

function hieroxideAlert(message) {
	if (hieroxideConfig.noAlert)
		console.error(message);
	else
		alert(message);
}

// Speed-ups. As shipped, `processFragmentsNow` batches its reads and writes (see there) and the wait
// for the font is a promise. The switches of a test build are between the `test-levers` markers:
// `tools/build-compat.mjs --test-levers` keeps them and the shipped build cuts them out whole, so
// that no page can turn anything on or off, by a variable or by a parameter of its URL, and the
// two builds are the same code but for those regions.
const hieroxideReuse = { adv: false, draw: false };
let hieroxideFontWait = true;
//#test-levers begin
// Levers: batch (default on), fontwait (default on), canvas (reuse one canvas and context for
// `advance` and one for `draw`; `adv` and `draw` switch the two apart; default off), forcebatch (see
// processFragmentsNow; for the tests of batching, which would otherwise stop testing it on a page that
// has that kind of fragments). The list is taken from `window.HIEROXIDE_LEVERS` (a string, set before
// this script), else from the `hx` parameter of the page URL (`page.html?hx=batch,canvas`; `hx=` is
// none), else from HIEROXIDE_LEVERS_DEFAULT, which tests/bench/variants.mjs rewrites.
const HIEROXIDE_TEST_BUILD = true;
const HIEROXIDE_LEVERS_DEFAULT = 'batch,fontwait';
const hieroxideLevers = (() => {
	let s = typeof window.HIEROXIDE_LEVERS === 'string' ? window.HIEROXIDE_LEVERS
		: new URLSearchParams(location.search).get('hx');
	if (s === null) s = HIEROXIDE_LEVERS_DEFAULT;
	const set = new Set(s.split(',').map(x => x.trim()).filter(Boolean));
	if (set.has('canvas')) { set.add('adv'); set.add('draw'); }
	return set;
})();
hieroxideReuse.adv = hieroxideLevers.has('adv');
hieroxideReuse.draw = hieroxideLevers.has('draw');
hieroxideFontWait = hieroxideLevers.has('fontwait');
//#test-levers end

// Which engine lays the page out, as far as what it does with a batched write matters (README.md):
// 'blink' (Chromium and everything built on it) and 'gecko' (Firefox) are the engines the tests show a
// batch to be exact in; anything else is taken for WebKit (every browser on iOS is one), which is the
// safe side, since a wrong guess there only costs speed. Recognised by what only Blink and Gecko have,
// not by a name.
const hieroxideEngine = (() => {
	let kind = 'webkit';
	if ('MozAppearance' in document.documentElement.style)
		kind = 'gecko';
	else if (navigator.userAgentData !== undefined
			|| (typeof window.chrome === 'object' && /\bChrome\/\d/.test(navigator.userAgent)))
		kind = 'blink';
	//#test-levers begin
	for (const k of ['blink', 'gecko', 'webkit'])
		if (hieroxideLevers.has('engine=' + k)) kind = k;
	//#test-levers end
	return kind;
})();

// What the wasm module asks of the page. They are the same canvas calls upstream makes in
// `Shapes.measureGlyph` and `PrintedCanvas.addSign`, in the same order. `pow` and `log` are
// here because the engines' results differ in the last bit, and upstream's depend on them.
const hieroxideHost = {
	prepareFont(ctx, fontSize, color) {
		ctx.font = fontSize.toString() + 'px Hieroglyphic';
		ctx.fillStyle = color;
		ctx.textBaseline = 'alphabetic';
	},
	advance(cp, fontSize) {
		if (hieroxideReuse.adv) {
			let ctx = hieroxideHost.advCtx;
			if (!ctx) {
				ctx = hieroxideHost.advCtx = document.createElement('canvas').getContext('2d', { willReadFrequently: true });
				hieroxideHost.advSize = -1;
			}
			if (hieroxideHost.advSize !== fontSize) {
				hieroxideHost.prepareFont(ctx, fontSize, 'black');
				hieroxideHost.advSize = fontSize;
			}
			return ctx.measureText(String.fromCodePoint(cp)).width;
		}
		const canvas = document.createElement('canvas');
		const ctx = canvas.getContext('2d', { willReadFrequently: true });
		hieroxideHost.prepareFont(ctx, fontSize, 'black');
		return ctx.measureText(String.fromCodePoint(cp)).width;
	},
	draw(w, h, params, chars) {
		// Either way the drawing starts on a cleared bitmap and a context in its default state:
		// assigning the size clears and resets, even when the value is the same, and every
		// glyph has its own save, font, transform and restore.
		let canvas;
		if (hieroxideReuse.draw) {
			canvas = hieroxideHost.scratch || (hieroxideHost.scratch = document.createElement('canvas'));
		} else {
			canvas = document.createElement('canvas');
			canvas.getContext('2d', { willReadFrequently: true });
		}
		canvas.width = w;
		canvas.height = h;
		const ctx = canvas.getContext('2d', { willReadFrequently: true });
		for (let i = 0; i < chars.length; i++) {
			const p = i * 8;
			ctx.save();
			hieroxideHost.prepareFont(ctx, params[p], 'black');
			ctx.translate(params[p + 1], params[p + 2]);
			ctx.scale(params[p + 3], params[p + 4]);
			ctx.rotate(params[p + 5]);
			ctx.fillText(String.fromCodePoint(chars[i]), params[p + 6], params[p + 7]);
			ctx.restore();
		}
		const data = ctx.getImageData(0, 0, w, h).data;
		return new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
	},
	pow(base, exp) {
		return base ** exp;
	},
	log(x) {
		return Math.log(x);
	},
	hasId(id) {
		return document.getElementById(id) !== null;
	},
	now() {
		return performance.now();
	},
};

// The module, loaded once per script (`startLoadingFonts` can be called again, upstream's
// reloads the font, and must not load this again).
let hieroxideCompat = null;
let hieroxideWasmReady = null;
const hieroxideWasmStart = performance.now();
let hieroxideWasmAt = 0;
let hieroxideWasmError = null;
if (hieroxideEmbedded) {
	// Synchronously, so that `syntax.parse` works as soon as this script has run.
	try {
		const raw = atob(HIEROXIDE_WASM_B64);
		const bytes = new Uint8Array(raw.length);
		for (let i = 0; i < bytes.length; i++)
			bytes[i] = raw.charCodeAt(i);
		wasm_bindgen.initSync({ module: new WebAssembly.Module(bytes) });
		hieroxideCompat = new wasm_bindgen.Compat();
		hieroxideWasmAt = performance.now();
	} catch (err) {
		hieroxideWasmError = err;
		console.error('Hieroxide: the WebAssembly module could not be started: ' + err);
	}
	hieroxideWasmReady = Promise.resolve();
} else {
	hieroxideWasmReady = wasm_bindgen({ module_or_path: new URL('hieroxide_wasm_bg.wasm', hieroxideScriptUrl) }).then(() => {
		hieroxideCompat = new wasm_bindgen.Compat();
		hieroxideWasmAt = performance.now();
	}, (err) => {
		hieroxideWasmError = err;
		console.error('Hieroxide: the WebAssembly module could not be loaded: ' + err);
		throw err;
	});
}

// util.js of upstream, which hierojax.js bundles and pages use.
function $(id) {
	return document.getElementById(id);
}

function removeChildren(elem) {
	while (elem.firstChild)
		elem.removeChild(elem.firstChild);
}

function getMin(arr) {
	return arr.reduce((acc, x) => Math.min(acc, x), Infinity);
}

function getMax(arr) {
	return arr.reduce((acc, x) => Math.max(acc, x), -Infinity);
}

function getSum(arr) {
	return arr.reduce((acc, x) => acc + x, 0);
}

function isLinux() {
	return navigator.platform && (
			navigator.platform.indexOf('Linux') >= 0 || navigator.platform.indexOf('Android') >= 0);
}

// Of upstream's `PrintedSVG`, what the public API names. Read when a standalone SVG is made.
const PrintedSVG = { EXTERNAL_FONT: hieroxideConfig.fontUrl || 'NewGardiner.otf' };
const hieroxideFont = { url: 'NewGardiner.otf' };

// `PrintedCanvas` on real canvases: the calls upstream makes, with the numbers the module
// worked out (see layout/canvas_out.rs for the layout of `ops`).
function hieroxideReplayCanvases(elem, ops, text, signColor, bracketColor) {
	const colors = [signColor, bracketColor];
	let i = 0;
	while (i < ops.length) {
		const canvas = document.createElement('canvas');
		canvas.width = ops[i++];
		canvas.height = ops[i++];
		const flags = ops[i++];
		elem.appendChild(canvas);
		if (!(flags & 1))
			elem.style.setProperty('height', canvas.height.toFixed(2) + 'px');
		const ctx = canvas.getContext('2d', { willReadFrequently: true });
		if (flags & 2) {
			ctx.beginPath();
			ctx.rect(0, 0, canvas.width, canvas.height);
			ctx.stroke();
		}
		if (flags & 4) {
			const hidden = document.createElement('div');
			hidden.className = 'hierojax-canvas-hidden';
			hidden.innerHTML = text;
			elem.appendChild(hidden);
		}
		for (;;) {
			const op = ops[i++];
			if (op === 0) {
				break;
			} else if (op === 1) {
				ctx.save();
				hieroxideHost.prepareFont(ctx, ops[i + 1], colors[ops[i]]);
				ctx.translate(ops[i + 2], ops[i + 3]);
				ctx.scale(ops[i + 4], ops[i + 5]);
				ctx.rotate(ops[i + 6]);
				ctx.fillText(String.fromCodePoint(ops[i + 9]), ops[i + 7], ops[i + 8]);
				ctx.restore();
				i += 10;
			} else if (op === 2) {
				ctx.save();
				ctx.fillStyle = 'rgba(0,0,0,.3)';
				ctx.fillRect(ops[i], ops[i + 1], ops[i + 2], ops[i + 3]);
				ctx.restore();
				i += 4;
			} else {
				const xMin = ops[i], yMin = ops[i + 1], width = ops[i + 2], height = ops[i + 3];
				const wAccum = ops[i + 4], hAccum = ops[i + 5];
				i += 6;
				ctx.save();
				ctx.fillStyle = 'rgba(0,0,0,.3)';
				for (let px = 0; px < width; px++)
					for (let py = 0; py < height; py++)
						if ((wAccum+xMin+px + hAccum+yMin+py) % 4 == 0) {
							ctx.beginPath();
							ctx.arc(xMin+px, yMin+py, 1, 0, 2 * Math.PI);
							ctx.fill();
						}
				ctx.restore();
			}
		}
	}
}

class HieroJax {
	constructor() {
		this.startLoadingFonts();
	}

	startLoadingFonts() {
		const hierojax = this;
		const fontUrl = hieroxideConfig.fontUrl || 'NewGardiner.otf';
		this.fonts = [ new FontFace('Hieroglyphic', 'url(' + fontUrl + ')') ];
		this.nFonts = this.fonts.length;
		this.nFontsLoaded = 0;
		const pending = this.fonts.map(f => f.load().then((font) => {
			document.fonts.add(font);
			hierojax.nFontsLoaded++; }));
		this.wasmStart = hieroxideWasmStart;
		this.compat = hieroxideCompat;
		this.wasmReady = hieroxideWasmAt;
		this.lastUnsupported = null;
		//#test-levers begin
		this.profile = false;
		this.profileApplied = false;
		this.lastTimings = null;
		//#test-levers end
		if (!hieroxideEmbedded) {
			// The module is something to wait for, like a font: `waitForFonts` and the
			// `nFontsLoaded == nFonts` test cover it too.
			this.nFonts++;
			pending.push(hieroxideWasmReady.then(() => {
				hierojax.compat = hieroxideCompat;
				hierojax.wasmReady = hieroxideWasmAt;
				hierojax.nFontsLoaded++;
			}));
		}
		// Settles when the font (and the module, if it is separate) are in.
		this.ready = Promise.all(pending);
	}

	waitForFonts(f, c) {
		const hierojax = this;
		if (this.nFonts == this.nFontsLoaded) {
			f();
		} else if (c == 0 && hieroxideFontWait) {
			// The polling below gives up (alerts) at the 42nd look: after 5 + 50 + 3 x 100 + 36 x 1000 ms.
			let late = false;
			const timer = setTimeout(() => {
				late = true;
				hieroxideAlert('Seems unable to load fonts; perhaps try again later?');
			}, 5 + 50 + 3 * 100 + 36 * 1000);
			this.ready.then(() => {
				clearTimeout(timer);
				// Not called from here: an exception from `f` would be an unhandled rejection of
				// this promise, where upstream's, from a timer, is an error event on the window.
				// One from a microtask's callback is reported as that error event.
				if (!late) queueMicrotask(f);
			}, () => {});
		} else if (c > 40) {
			hieroxideAlert('Seems unable to load fonts; perhaps try again later?');
		} else {
			const wait = c == 0 ? 5 : c == 1 ? 50 : c < 5 ? 100 : 1000;
			setTimeout(function(){ hierojax.waitForFonts(f, c+1); }, wait);
		}
	}

	processFragments() {
		this.waitForFonts(() => this.processFragmentsNow(), 0);
	}

	processFragment(elem) {
		this.waitForFonts(() => this.processFragmentNow(elem), 0);
	}

	processFragmentsIn(elem) {
		const spans = elem.getElementsByTagName('span');
		for (let span of spans)
			if (span.classList.contains('hierojax'))
				this.processFragment(span);
	}

	processFragmentsNow() {
		var elems = [...document.getElementsByClassName('hierojax')];
		//#test-levers begin
		if (!hieroxideLevers.has('batch')) {
			elems.forEach(elem => this.processFragmentNow(elem));
			return;
		}
		//#test-levers end
		// Everything that reads the page first (one style recalculation and one layout for all of
		// them), then the layout in wasm and every write. Nothing is written before the first
		// write, so when a fragment throws, this stops as upstream's forEach does: the elements after
		// it are as they were.
		const states = elems.map(elem => this.readFragment(elem));
		let flush = null;
		if (hieroxideEngine == 'webkit')
			flush = HieroJax.layoutPoints(elems, states);
		//#test-levers begin
		if (hieroxideLevers.has('forcebatch'))
			flush = null;
		//#test-levers end
		elems.forEach((elem, i) => {
			this.writeFragment(elem, states[i]);
			// A layout forced here, which upstream's one-at-a-time writes have between every two
			// fragments and a batch has not (README.md).
			if (flush && flush[i])
				void document.documentElement.offsetHeight;
		});
	}

	static isDom(st) {
		const type = st.attrs[6];
		return !!type && type != 'canvas' && type != 'svg';
	}

	// The block that holds an element's lines: the nearest ancestor that is not an inline box.
	static island(elem) {
		for (let a = elem.parentElement; a; a = a.parentElement) {
			const display = window.getComputedStyle(a, null).display;
			if (display != 'inline' && display != 'contents')
				return a;
		}
		return document.documentElement;
	}

	// After which fragments a layout is to be forced, or null for none. Only WebKit needs any (see
	// README.md): for each `type=dom` fragment, after it and after the fragment before it in the
	// same block, because upstream's one-at-a-time writes have a layout there, and the boxes WebKit
	// leaves depend on it. Everything else stays batched.
	static layoutPoints(elems, states) {
		const dom = states.map(st => HieroJax.isDom(st));
		if (!dom.some(Boolean))
			return null;
		const island = elems.map(e => HieroJax.island(e));
		return states.map((st, i) => dom[i] || (i + 1 < states.length && dom[i + 1] && island[i + 1] === island[i]));
	}

	// What the page tells about an element: the part of processFragmentNow that reads.
	readFragment(elem) {
		var style = window.getComputedStyle(elem, null);
		var fontSize = Number(style.getPropertyValue('font-size').replace('px', ''));
		var signColor = style.getPropertyValue('color');
		const txt = elem.innerText;
		const attrs = [];
		for (const p of HieroJax.DATA_ATTRIBUTES)
			attrs.push(elem.getAttribute('data-' + p) || '');
		return { fontSize, signColor, txt, attrs, root: elem.getRootNode() };
	}

	processFragmentNow(elem) {
		this.writeFragment(elem, this.readFragment(elem));
	}

	static units(txt) {
		const units = new Uint16Array(txt.length);
		for (let i = 0; i < txt.length; i++)
			units[i] = txt.charCodeAt(i);
		return units;
	}

	// The rest of processFragmentNow: clear the element, lay the text out, write the result.
	writeFragment(elem, state) {
		this.lastUnsupported = null;
		//#test-levers begin
		this.lastTimings = null;
		//#test-levers end
		// A fragment nested in another was cut out of the document when the outer one was
		// cleared, after the batch read it. Upstream reads it only then: an empty computed
		// style, and its own svg is not one `getElementById` finds. Asking costs no layout.
		const root = elem.getRootNode();
		if (root !== state.root)
			state = this.readFragment(elem);
		const txt = state.txt;
		elem.innerHTML = '';
		this.render(elem, txt, state.fontSize, state.signColor, state.attrs, root === document);
	}

	// Lay a text out with these options and write the result into `elem`: the part of
	// processFragmentNow after the clearing, and what `Fragment.print` does. `attrs` is the
	// data-* values in the order of DATA_ATTRIBUTES; `fontSize` and `signColor` are the
	// computed ones.
	render(elem, txt, fontSize, signColor, attrs, attached) {
		const compat = this.compat;
		if (!compat)
			throw new Error('Hieroxide: the WebAssembly module is not available'
				+ (hieroxideWasmError ? ': ' + hieroxideWasmError : ' yet; wait for hierojax.ready'));
		//#test-levers begin
		if (this.profile !== this.profileApplied) {
			compat.set_profile(this.profile);
			this.profileApplied = this.profile;
		}
		//#test-levers end
		if (hieroxideFont.url !== PrintedSVG.EXTERNAL_FONT) {
			compat.set_external_font(String(PrintedSVG.EXTERNAL_FONT), hieroxideConfig.fontFormat || 'opentype');
			hieroxideFont.url = PrintedSVG.EXTERNAL_FONT;
		}
		const status = compat.process(HieroJax.units(txt), fontSize, signColor, attrs, attached, !!isLinux());
		if (status == 1) {
			console.log('Parse error:\n' + txt + 'Error: ' + compat.message());
			elem.innerHTML = '<span style="font-size: 20px; color: red;">parse error; see log</span>';
			return;
		}
		if (status == 2) {
			this.lastUnsupported = compat.message();
			return;
		}
		if (status == 3)
			throw new Error(compat.message());
		const cls = compat.container_class();
		if (cls)
			elem.className = cls;
		const width = compat.container_width();
		if (width)
			elem.style['width'] = width;
		const height = compat.container_height();
		if (height)
			elem.style['height'] = height;
		if (compat.container_border())
			elem.classList.add('hierojax-border');
		const propWidth = compat.container_prop_width();
		if (propWidth) {
			elem.style.setProperty('width', propWidth);
			elem.style.setProperty('height', compat.container_prop_height());
		}
		const signColorUsed = attrs[1] || signColor || 'black';
		const bracketColorUsed = attrs[7] || 'black';
		if (compat.has_canvas()) {
			hieroxideReplayCanvases(elem, compat.canvas_ops(), compat.canvas_text(), signColorUsed, bracketColorUsed);
		} else {
			//#test-levers begin
			const d0 = this.profile ? performance.now() : 0;
			//#test-levers end
			const markup = compat.markup();
			elem.insertAdjacentHTML('beforeend', markup);
			if (markup.includes('data-hx-color'))
				HieroJax.setMarkedColors(elem, signColorUsed, bracketColorUsed);
			//#test-levers begin
			if (this.profile) {
				// For the benchmarks: [parse, format and fit, print + building the DOM] in ms.
				const t = compat.timings();
				this.lastTimings = [t[0], t[1], t[2] + (performance.now() - d0)];
			}
			//#test-levers end
		}
		if (hieroxideConfig.svgScalable)
			HieroJax.makeScalable(elem, attrs[0] || fontSize || 22);
		for (const message of compat.logs())
			console.error(message);
		if (status == 4)
			throw compat.take_exception();
		if (status == 5)
			HieroJax.throwFault(compat.fault_kind(), compat.fault_value());
	}

	// A colour the module did not put in the markup (it holds characters that could end the
	// declaration) is set as upstream sets it, by the property.
	static setMarkedColors(elem, signColor, bracketColor) {
		for (const el of [...elem.querySelectorAll('[data-hx-color]')]) {
			el.style.setProperty('color', el.getAttribute('data-hx-color') == '1' ? bracketColor : signColor);
			el.removeAttribute('data-hx-color');
		}
	}

	// The `svgScalable` option: what the site's patch does in the PrintedSVG constructor.
	static makeScalable(elem, fontSize) {
		for (const svg of elem.children) {
			if (svg.localName != 'svg')
				continue;
			const w = Number(svg.getAttribute('width'));
			const h = Number(svg.getAttribute('height'));
			svg.setAttribute('viewBox', `0 0 ${w} ${h}`);
			if (fontSize > 0) {
				svg.style.width = `${w / fontSize}em`;
				svg.style.height = `${h / fontSize}em`;
			}
		}
	}

	// The two places in upstream's own code that throw, written as upstream writes them: the
	// message of the exception is the engine's, and some engines name the variable in it.
	static throwFault(kind, nSteps) {
		if (kind == 1) {
			const steps = [...Array(nSteps).keys()];
			return steps;
		}
		const lig = undefined;
		for (let i = 0; i < lig.horizontal.length; i++)
			;
	}
}
HieroJax.DATA_ATTRIBUTES = ['fontsize', 'signcolor', 'dir', 'linesize', 'sep', 'separated', 'type',
		'bracketcolor', 'shadepattern', 'align', 'border', 'standalone', 'log'];

// What `syntax.parse` gives: the part of upstream's `Fragment` that is public, `toString()`
// and `print(element, options)`.
class HieroxideFragment {
	constructor(txt, str) {
		this.txt = txt;
		this.str = str;
	}

	toString() {
		return this.str;
	}

	print(element, options) {
		// The normalization at the top of upstream's `Fragment.print`, which also leaves it in
		// the object it was given.
		options.dir = options.dir || 'hlr';
		options.linesize = Number(options.linesize) || 1;
		options.fontsize = options.fontsize || 22;
		options.sep = Number(options.sep) || 0.07;
		options.separated = options.separated == 'true';
		options.type = options.type || 'svg';
		options.signcolor = options.signcolor || 'black';
		options.bracketcolor = options.bracketcolor || 'black';
		options.shadepattern = options.shadepattern || 'uniform';
		options.align = options.align || 'middle';
		options.border = options.border == 'true';
		options.standalone = options.standalone == 'true';
		options.log = options.log == 'true';
		// As the data-* values processFragmentNow reads, which the module normalizes again to
		// the same: the font size is the number when it is one.
		const num = typeof options.fontsize == 'number';
		const attrs = [num ? '' : String(options.fontsize), String(options.signcolor), String(options.dir),
			String(options.linesize), String(options.sep), options.separated ? 'true' : '', String(options.type),
			String(options.bracketcolor), String(options.shadepattern), String(options.align),
			options.border ? 'true' : '', options.standalone ? 'true' : '', options.log ? 'true' : ''];
		hierojax.render(element, this.txt, num ? options.fontsize : 0, '', attrs,
			element.getRootNode() === document);
	}
}

// Of upstream's generated parser, `parse`: it throws an `Error` for text that is not a
// fragment, and gives an object with `toString()` and `print(element, options)`.
var syntax = {
	parse(txt) {
		if (!hieroxideCompat)
			throw new Error('Hieroxide: syntax.parse is not available until the WebAssembly module has loaded; '
				+ 'wait for hierojax.ready' + (hieroxideWasmError ? ' (it failed: ' + hieroxideWasmError + ')' : ''));
		let str;
		try {
			str = wasm_bindgen.parse_round_trip(HieroJax.units(txt));
		} catch (err) {
			throw new Error(typeof err == 'string' ? err : String(err));
		}
		return new HieroxideFragment(txt, str);
	},
};

const hierojax = new HieroJax();

if (hieroxideConfig.autoProcess) {
	window.addEventListener("DOMContentLoaded", () => {
		hierojax.processFragments();
	});
}
