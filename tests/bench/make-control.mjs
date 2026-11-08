#!/usr/bin/env node
// The control for the batching lever: upstream's own docs/hierojax.js with the same
// read-then-write batching applied to processFragmentsNow, and nothing else changed, so that
// it shows what batching gives plain JS without wasm. Built, never committed: the output
// (build/control/hierojax.js, git-ignored) is upstream's text with one patch applied here.
//
//   node make-control.mjs [--out ../../build/control]
//
// The patch splits processFragmentNow into a read part (computed style, innerText, the data-*
// attributes) and a write part (clear the element, parse, `fragment.print`); `print` writes to
// the element and also lays out and measures canvases, so it stays in the write part, in
// upstream's order, and parse errors still log at the same point. processFragmentsNow then
// reads every element, then writes every element in order. processFragmentNow (one element,
// what processFragment calls) does read then write, as before. Each replaced piece must match
// upstream's text exactly or the script stops.
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const { values: v } = parseArgs({ options: { out: { type: 'string', default: path.join(HERE, '../../build/control') } } });
let s = fs.readFileSync(path.join(HERE, '../../docs/hierojax.js'), 'utf8');

function replace(from, to) {
  if (s.split(from).length !== 2) throw new Error('upstream no longer matches the patch:\n' + from);
  s = s.replace(from, () => to);
}

replace(`	processFragmentsNow() {
		var elems = document.getElementsByClassName('hierojax');
		[...elems].forEach(elem => this.processFragmentNow(elem));
	}
`, `	processFragmentsNow() {
		var elems = [...document.getElementsByClassName('hierojax')];
		const states = elems.map(elem => this.readFragment(elem));
		elems.forEach((elem, i) => this.writeFragment(elem, states[i]));
	}
`);

replace(`	processFragmentNow(elem) {
		var style = window.getComputedStyle(elem, null);
		var fontSize = Number(style.getPropertyValue('font-size').replace('px', ''));
		var signColor = style.getPropertyValue('color');
		const txt = elem.innerText;
		elem.innerHTML = '';
		try {`, `	processFragmentNow(elem) {
		this.writeFragment(elem, this.readFragment(elem));
	}

	readFragment(elem) {
		var style = window.getComputedStyle(elem, null);
		var fontSize = Number(style.getPropertyValue('font-size').replace('px', ''));
		var signColor = style.getPropertyValue('color');
		const txt = elem.innerText;
		var options = {};
		options.fontsize = fontSize;
		options.signcolor = signColor;
		for (const p of ['fontsize', 'signcolor', 'dir', 'linesize', 'sep', 'separated', 'type',
				'bracketcolor', 'shadepattern', 'align', 'border', 'standalone', 'log'])
			if (elem.getAttribute('data-' + p))
				options[p] = elem.getAttribute('data-' + p);
		return { txt, options };
	}

	writeFragment(elem, state) {
		const txt = state.txt;
		elem.innerHTML = '';
		try {`);

replace(`		var options = {};
		options.fontsize = fontSize;
		options.signcolor = signColor;
		for (const p of ['fontsize', 'signcolor', 'dir', 'linesize', 'sep', 'separated', 'type',
				'bracketcolor', 'shadepattern', 'align', 'border', 'standalone', 'log'])
			if (elem.getAttribute('data-' + p))
				options[p] = elem.getAttribute('data-' + p);
		fragment.print(elem, options);`, `		fragment.print(elem, state.options);`);

fs.mkdirSync(v.out, { recursive: true });
fs.writeFileSync(path.join(v.out, 'hierojax.js'), s);
console.log(`wrote ${path.join(v.out, 'hierojax.js')}`);
