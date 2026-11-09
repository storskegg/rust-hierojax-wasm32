// The readback probe of the drop-in, for a classic script: goes before js/compat/hierojax.js. On a
// browser whose canvas readback is corrupt, compat runs on outlines where that build is next to the
// script (tools/build-consistent.mjs, build/compat-fallback/), and otherwise goes on as upstream does.
//
// What the shim needs is one change, where the module is loaded when the script runs:
//
//   hieroxideWasmReady = wasm_bindgen({ module_or_path: url })
//     .then(() => hieroxideChooseCompat(url))
//     .then((compat) => {
//       hieroxideCompat = compat;
//       ...
//
// instead of `hieroxideCompat = new wasm_bindgen.Compat()`. When readback is sane that costs one
// canvas of 8 by 8 and changes nothing else. It is for the build with a separate module: the embedded
// one has its engine at once, before an outline module could arrive.

// The core's readback_is_sane, on what one known pixel reads back as (layout/readback.rs says which).
function hieroxideReadbackSane() {
	const canvas = document.createElement('canvas');
	canvas.width = 8;
	canvas.height = 8;
	const ctx = canvas.getContext('2d', { willReadFrequently: true });
	ctx.fillStyle = 'rgb(10, 20, 30)';
	ctx.fillRect(3, 2, 1, 1);
	const data = ctx.getImageData(0, 0, 8, 8).data;
	return wasm_bindgen.readback_is_sane(new Uint8Array(data.buffer, data.byteOffset, data.byteLength));
}

// A compat engine: on the page's canvas if it can be believed, else on outlines if they are there.
// Resolves, never rejects, so that a missing fallback leaves upstream's behaviour.
async function hieroxideChooseCompat(moduleUrl) {
	if (hieroxideReadbackSane())
		return new wasm_bindgen.Compat();
	try {
		const base = new URL('.', moduleUrl);
		await new Promise((resolve, reject) => {
			const s = document.createElement('script');
			s.src = new URL('hieroxide_outline_compat.js', base);
			s.onload = resolve;
			s.onerror = reject;
			document.head.append(s);
		});
		const [font] = await Promise.all([
			fetch(new URL('NewGardiner.otf', base)).then((r) => r.arrayBuffer()),
			hieroxideOutline({ module_or_path: new URL('hieroxide_outline_compat_bg.wasm', base) }),
		]);
		hieroxideOutline.set_font(new Uint8Array(font));
		return new hieroxideOutline.Compat();
	} catch (e) {
		return new wasm_bindgen.Compat();
	}
}
