// Tests for the canonicalizer, run in every engine (the point of it is that the engine's
// own CSSOM does the normalizing). Run: npm test
import test from 'node:test';
import assert from 'node:assert/strict';
import { launch, ENGINE_NAMES } from '../lib/engines.mjs';
import { CANON_SRC } from '../lib/canon.mjs';

for (const engine of ENGINE_NAMES) {
  test(`canonicalizer in ${engine}`, async (t) => {
    const browser = await launch(engine);
    const page = await browser.newPage();
    await page.setContent('<!DOCTYPE html><html><body></body></html>');
    await page.evaluate(`window.__canon = ${CANON_SRC}`);
    // Canonical form of an HTML snippet (the first element in <body>), optionally after
    // running a script on it.
    const canon = (html, script) =>
      page.evaluate(
        ([html, script]) => {
          const box = document.createElement('div');
          box.innerHTML = html;
          document.body.appendChild(box);
          const el = box.firstElementChild;
          if (script) new Function('el', script)(el);
          const out = window.__canon(el);
          box.remove();
          return out;
        },
        [html, script],
      );

    await t.test('attribute order does not matter', async () => {
      assert.equal(await canon('<p b="1" a="2" data-x="3">t</p>'), await canon('<p data-x="3" a="2" b="1">t</p>'));
    });

    await t.test('attribute values still matter', async () => {
      assert.notEqual(await canon('<p a="1">t</p>'), await canon('<p a="2">t</p>'));
      assert.notEqual(await canon('<p a="1">t</p>'), await canon('<p b="1">t</p>'));
    });

    await t.test('style: order, spacing, colour spelling and importance are normalized through the CSSOM', async () => {
      const a = await canon('<p style="color:#f00;font-size:12px;margin:0">t</p>');
      const b = await canon('<p style="margin: 0;  font-size: 12px ;color: rgb(255, 0, 0)">t</p>');
      assert.equal(a, b);
      assert.notEqual(a, await canon('<p style="color:#f00;font-size:13px;margin:0">t</p>'));
      assert.notEqual(await canon('<p style="color:red">t</p>'), await canon('<p style="color:red !important">t</p>'));
    });

    await t.test('style set through the CSSOM equals style written as an attribute', async () => {
      const viaAttr = await canon('<p style="font-size: 33.6px; color: rgb(0, 64, 255)">t</p>');
      const viaCssom = await canon('<p>t</p>', "el.style.setProperty('color', '#0040ff'); el.style.fontSize = '33.6px';");
      assert.equal(viaAttr, viaCssom);
    });

    await t.test('style on SVG elements', async () => {
      const a = await canon('<svg xmlns="http://www.w3.org/2000/svg"><text style="font-size:10px; line-height:10px" x="1" y="2">a</text></svg>');
      const b = await canon('<svg xmlns="http://www.w3.org/2000/svg"><text y="2" x="1" style="line-height: 10px;font-size: 10px">a</text></svg>');
      assert.equal(a, b);
      assert.match(a, /<svg:text style="font-size:10px;line-height:10px" x="1" y="2">/);
    });

    await t.test('adjacent text nodes are merged, empty ones dropped', async () => {
      const split = await canon('<p>ab</p>', "el.appendChild(document.createTextNode('')); el.firstChild.splitText(1);");
      assert.equal(split, await canon('<p>ab</p>'));
      assert.notEqual(await canon('<p>ab</p>'), await canon('<p>ac</p>'));
    });

    await t.test('structure and namespace matter', async () => {
      assert.notEqual(await canon('<p><b>t</b></p>'), await canon('<p><i>t</i></p>'));
      assert.notEqual(await canon('<p><b>t</b></p>'), await canon('<p><b>t</b><b></b></p>'));
      const svgText = await canon('<svg xmlns="http://www.w3.org/2000/svg"><tspan>t</tspan></svg>');
      assert.match(svgText, /<svg:tspan>/);
    });

    await t.test('one node per line, text quoted', async () => {
      assert.equal(await canon('<p a="1">hi<b>x</b></p>'), '<p a="1">\n"hi"\n<b>\n"x"\n</b>\n</p>');
    });

    await browser.close();
  });
}
