// A page of the fragments of the real pages (docs/texts/ and testsuite.html, with their
// options and font sizes) that a replacement can lay out, so that upstream and the
// replacement can be run, timed and compared on the very same fragments in the same
// order. Which ones is decided by running the replacement over them first, in the same
// engine. A fragment that throws is left out, because in a page it would stop every later
// fragment from being processed.
import { loadCorpus, runCorpus } from './runner.mjs';

const esc = (t) => t.replace(/&/g, '&amp;').replace(/</g, '&lt;');
const escAttr = (t) => esc(t).replace(/"/g, '&quot;');

/** Returns { count, html } for `engine`, with coverage decided by `coverageImpl`. */
export async function coveredPage({ coverageImpl, engine }) {
  const items = loadCorpus().items.filter((it) => it.group === 'texts' || it.group === 'testsuite');
  const { results } = await runCorpus({ impl: coverageImpl, engine, runId: 'coverage', corpus: { items, sha256: 'coverage' }, log: () => {} });
  const keep = items.filter((it) => {
    const r = results[it.id];
    return r && !r.unsupported && !r.err && !/^parse error/.test(r.text ?? '');
  });
  const body = keep
    .map((it) => {
      const attrs = Object.entries(it.attrs).map(([k, val]) => ` ${k}="${escAttr(val)}"`).join('');
      return `<div style="font-size:${it.fontSize};color:${it.color}"><span class="hierojax"${attrs}>${esc(it.text)}</span></div>`;
    })
    .join('\n');
  return {
    count: keep.length,
    considered: items.length,
    html: `<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><title>covered</title>
<link rel="stylesheet" type="text/css" href="hierojax.css" />
<script type="text/javascript" src="hierojax.js"></script>
<script type="text/javascript">
window.addEventListener("DOMContentLoaded", () => { hierojax.processFragments(); });
</script></head><body>
${body}
</body></html>
`,
  };
}
