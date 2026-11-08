// Hieroxide compat shim (minimal, milestone M2).
//
// A classic script that stands in for upstream's hierojax.js. tools/build-compat.mjs
// appends it to the wasm-bindgen glue (which defines `wasm_bindgen`) and writes the result
// as build/compat/hierojax.js next to the .wasm.
//
// It keeps upstream's `HieroJax` class and its order of work, fragment by fragment:
// `processFragmentNow` reads the element as upstream does, hands the text and options to
// the wasm module, and applies what comes back: the container changes and one markup
// string. The module is counted among the things to wait for, so `waitForFonts` and the
// `nFontsLoaded == nFonts` test cover it too.
//
// Not here yet (milestone M4a): the rest of the public surface (`syntax.parse`, the options
// of the site's patched script) and the canvas and DOM printers. A fragment the module cannot
// lay out yet (the other printers) is left empty and recorded in `hierojax.lastUnsupported`,
// which the test harness reads.
//
// An exception from a canvas (`getImageData` on a canvas with a side of 0, say) is caught at
// the import, travels up through the module as a value, and is thrown again from
// `processFragmentNow` after whatever upstream had written by then has been applied.

const hieroxideScriptUrl = document.currentScript ? document.currentScript.src : location.href;

// Speed-ups, each an independent switch, given as a comma-separated list (on by default:
// batch and fontwait; canvas passes the same checks but gains nothing measurable):
//   batch      processFragmentsNow reads every element first (computed style, innerText,
//              attributes), then lays all of them out, then writes them all, so the browser
//              recalculates style and layout once instead of once per fragment.
//   canvas     reuse one canvas and context for `advance` and one for `draw`
//              (`adv` and `draw` switch the two apart).
//   fontwait   wait for the font and the module with a promise rather than polling;
//              the alert for a font that never loads comes at the same time as before.
// The list is taken from `window.HIEROXIDE_LEVERS` (a string, set before this script), else
// from the `hx` parameter of the page URL (`page.html?hx=batch,canvas`; `hx=` is none), else
// from HIEROXIDE_LEVERS_DEFAULT. `processFragment(elem)` for a single element is unaffected
// by `batch`.
const HIEROXIDE_LEVERS_DEFAULT = 'batch,fontwait';
const hieroxideLevers = (() => {
	let s = typeof window.HIEROXIDE_LEVERS === 'string' ? window.HIEROXIDE_LEVERS
		: new URLSearchParams(location.search).get('hx');
	if (s === null) s = HIEROXIDE_LEVERS_DEFAULT;
	const set = new Set(s.split(',').map(x => x.trim()).filter(Boolean));
	if (set.has('canvas')) { set.add('adv'); set.add('draw'); }
	return set;
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
		if (hieroxideLevers.has('adv')) {
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
		if (hieroxideLevers.has('draw')) {
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

class HieroJax {
	constructor() {
		this.startLoadingFonts();
	}

	startLoadingFonts() {
		const hierojax = this;
		this.fonts = [ new FontFace('Hieroglyphic', 'url(NewGardiner.otf)') ];
		// The font, and the wasm module.
		this.nFonts = this.fonts.length + 1;
		this.nFontsLoaded = 0;
		const pending = this.fonts.map(f => f.load().then((font) => {
			document.fonts.add(font);
			hierojax.nFontsLoaded++; }));
		this.wasmStart = performance.now();
		this.compat = null;
		this.lastUnsupported = null;
		this.profile = false;
		this.profileApplied = false;
		this.lastTimings = null;
		const url = new URL('hieroxide_wasm_bg.wasm', hieroxideScriptUrl);
		pending.push(wasm_bindgen({ module_or_path: url }).then(() => {
			hierojax.compat = new wasm_bindgen.Compat();
			hierojax.wasmReady = performance.now();
			hierojax.nFontsLoaded++;
		}));
		// Settles when the font and the module are both in (for the `fontwait` lever).
		this.ready = Promise.all(pending);
	}

	waitForFonts(f, c) {
		const hierojax = this;
		if (this.nFonts == this.nFontsLoaded) {
			f();
		} else if (c == 0 && hieroxideLevers.has('fontwait')) {
			// The polling below gives up (alerts) at the 42nd look: after 5 + 50 + 3 x 100 + 36 x 1000 ms.
			let late = false;
			const timer = setTimeout(() => {
				late = true;
				alert('Seems unable to load fonts; perhaps try again later?');
			}, 5 + 50 + 3 * 100 + 36 * 1000);
			this.ready.then(() => {
				clearTimeout(timer);
				// Not called from here: an exception from `f` would be an unhandled rejection of
				// this promise, where upstream's, from a timer, is an error event on the window.
				// One from a microtask's callback is reported as that error event.
				if (!late) queueMicrotask(f);
			});
		} else if (c > 40) {
			alert('Seems unable to load fonts; perhaps try again later?');
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
		if (hieroxideLevers.has('batch')) {
			// Everything that reads the page first (one style recalculation and one layout
			// for all of them), then the layout in wasm and every write. Nothing is written
			// before the first write, so when a fragment throws, this stops as upstream's
			// forEach does: the elements after it are as they were.
			const states = elems.map(elem => this.readFragment(elem));
			elems.forEach((elem, i) => this.writeFragment(elem, states[i]));
		} else {
			elems.forEach(elem => this.processFragmentNow(elem));
		}
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

	// The rest of processFragmentNow: clear the element, lay the text out, write the result.
	writeFragment(elem, state) {
		this.lastUnsupported = null;
		this.lastTimings = null;
		// A fragment nested in another was cut out of the document when the outer one was
		// cleared, after the batch read it. Upstream reads it only then: an empty computed
		// style, and its own svg is not one `getElementById` finds. Asking costs no layout.
		const root = elem.getRootNode();
		if (root !== state.root)
			state = this.readFragment(elem);
		const txt = state.txt;
		elem.innerHTML = '';
		const units = new Uint16Array(txt.length);
		for (let i = 0; i < txt.length; i++)
			units[i] = txt.charCodeAt(i);
		const compat = this.compat;
		if (this.profile !== this.profileApplied) {
			compat.set_profile(this.profile);
			this.profileApplied = this.profile;
		}
		const status = compat.process(units, state.fontSize, state.signColor, state.attrs, root === document);
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
		if (this.profile) {
			// For the benchmarks: [parse, format and fit, print + building the DOM] in ms.
			const t = compat.timings();
			const d0 = performance.now();
			elem.insertAdjacentHTML('beforeend', compat.markup());
			this.lastTimings = [t[0], t[1], t[2] + (performance.now() - d0)];
		} else {
			elem.insertAdjacentHTML('beforeend', compat.markup());
		}
		for (const message of compat.logs())
			console.error(message);
		if (status == 4)
			throw compat.take_exception();
		if (status == 5)
			HieroJax.throwFault(compat.fault_kind(), compat.fault_value());
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

// Just enough of upstream's generated parser for the pages that call it directly (the test
// suite checks `syntax.parse(txt).toString()`): the full fragment object comes with M4a.
const syntax = {
	parse(txt) {
		// The wasm module loads asynchronously, and the test suite page asks at DOMContentLoaded.
		// Until it is ready this checks nothing and returns a stand-in whose toString is the
		// input, which that page takes as "parsed and printed unchanged". M4a decides what a
		// real early call does (the plan: an error, plus a ready promise).
		if (!hierojax.compat)
			return { toString() { return txt; } };
		const units = new Uint16Array(txt.length);
		for (let i = 0; i < txt.length; i++)
			units[i] = txt.charCodeAt(i);
		const s = wasm_bindgen.parse_round_trip(units);
		return { toString() { return s; } };
	},
};

const hierojax = new HieroJax();
