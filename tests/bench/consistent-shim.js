// Consistent mode behind upstream's `hierojax` global, for the benchmark pages only: they call
// hierojax.processFragments(), and run.mjs wraps the methods of HieroJax. It does what
// js/hieroxide/index.js's `render` does, in batch: read every element, lay all out, write all.
// make-consistent.mjs appends it to the wasm-bindgen glue.
const hieroxideScriptUrl = document.currentScript ? document.currentScript.src : location.href;

class HieroJax {
	constructor() {
		this.nFonts = 2;
		this.nFontsLoaded = 0;
		this.engine = null;
		this.lastTimings = null;
		this.profile = false;
		const url = (name) => new URL(name, hieroxideScriptUrl);
		const bytes = fetch(url('NewGardiner.otf')).then((r) => r.arrayBuffer()).then((b) => new Uint8Array(b));
		const module_ = wasm_bindgen({ module_or_path: url('hieroxide_outline_bg.wasm') });
		const engine = Promise.all([bytes, module_]).then(([b]) => {
			this.engine = new wasm_bindgen.Hieroxide(b);
			this.nFontsLoaded++;
		});
		const face = bytes.then((b) => new FontFace('Hieroglyphic', b).load()).then((f) => {
			document.fonts.add(f);
			this.nFontsLoaded++;
		});
		this.ready = Promise.all([engine, face]);
	}

	processFragments() {
		this.ready.then(() => queueMicrotask(() => this.processFragmentsNow()));
	}

	processFragmentsNow() {
		const elems = [...document.getElementsByClassName('hierojax')];
		const states = elems.map((e) => this.readFragment(e));
		elems.forEach((e, i) => this.writeFragment(e, states[i]));
	}

	readFragment(elem) {
		const attrs = [];
		for (const p of HieroJax.DATA_ATTRIBUTES)
			attrs.push(elem.getAttribute('data-' + p) || '');
		return { txt: elem.innerText, attrs };
	}

	writeFragment(elem, { txt, attrs }) {
		const units = new Uint16Array(txt.length);
		for (let i = 0; i < txt.length; i++)
			units[i] = txt.charCodeAt(i);
		elem.innerHTML = '';
		const e = this.engine;
		if (e.render(units, attrs) !== 0)
			return;
		const cls = e.container_class();
		if (cls)
			elem.classList.add(cls);
		const width = e.container_width();
		if (width)
			elem.style.width = width;
		const height = e.container_height();
		if (height)
			elem.style.height = height;
		if (e.container_border())
			elem.classList.add('hierojax-border');
		elem.insertAdjacentHTML('beforeend', e.markup());
	}
}
HieroJax.DATA_ATTRIBUTES = ['fontsize', 'signcolor', 'dir', 'linesize', 'sep', 'separated', 'type',
	'bracketcolor', 'shadepattern', 'align', 'border', 'standalone', 'log'];

const hierojax = new HieroJax();
