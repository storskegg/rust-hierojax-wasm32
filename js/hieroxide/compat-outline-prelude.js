// Goes between the wasm-bindgen glue and the compat shim in build/compat-outline/hierojax.js. The
// shim calls `wasm_bindgen({ module_or_path })` and `new wasm_bindgen.Compat()`; the module over
// outlines needs the font's bytes first, from the same file the page loads for its text.
wasm_bindgen = ((load) => Object.assign(async (o) => {
	const [r, font] = await Promise.all([
		load(o),
		fetch(new URL('NewGardiner.otf', document.currentScript?.src ?? location.href)).then((f) => f.arrayBuffer()),
	]);
	load.set_font(new Uint8Array(font));
	return r;
}, load))(wasm_bindgen);
