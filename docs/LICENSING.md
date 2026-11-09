# Licensing: the font and the pages Hieroxide makes

This is a reading of the licences by the people who wrote the code. It is not legal advice. If a
lot depends on it, ask a lawyer.

## What is under which licence

| What | Licence | Text |
|---|---|---|
| Hieroxide's code (Rust, JavaScript, the built `.wasm`) | GPL-3.0-only | [`LICENSE`](../LICENSE) |
| `docs/NewGardiner.otf` and `docs/texts/NewGardiner.otf` (the same bytes) | SIL Open Font License 1.1 | [`OFL.txt`](../OFL.txt) |

The font is by Mark-Jan Nederhof, who also wrote hierojax. Its `name` table says (nameID 0):
"Created by Mark-Jan Nederhof. Distributed at https://github.com/nederhof/newgardiner . Released
under OFL 1.1 http://scripts.sil.org/OFL". The font's repository says it "is released under the SIL OFL 1.1
license". Neither states a year or a Reserved Font Name, so none is reserved (OFL FAQ 5.7: nothing is
reserved by default). The font's embedding flags (`fsType`) are 0: installable.

Hieroxide's binaries do not contain the font. They read it at run time, from a file or a URL you give.

## The OFL questions

The conditions of OFL 1.1 that matter here are these (the text is in [`OFL.txt`](../OFL.txt)):

- Condition 1: the font may not be "sold by itself".
- Condition 2: a copy that is "bundled, redistributed and/or sold with any software" has to contain the
  copyright notice and the licence.
- Condition 5: the font "must be distributed entirely under this license", but this "does not apply to
  any document created using the Font Software".

The OFL FAQ is at <https://openfontlicense.org/ofl-faq/>. Entry numbers below are its.

### (a) A page that contains path outlines

`hieroxide render` (the default `--kind paths`) and `svg: 'paths'` in the JavaScript module write each
sign as an SVG `<path>` taken from the font's outlines. Does the OFL apply to such a page?

Our reading: the page is a document made with the font, not a distribution of the font. The OFL
does not reach it, and it needs no copyright notice or licence text.

- FAQ 1.1 asks whether one can use the fonts "to create logos or other graphics or even to manufacture
  objects based on their outlines". The answer: "Yes. You are very welcome to do so."
- FAQ 1.13: "Referencing or embedding an OFL font in any document does not change the license of the
  document itself."
- FAQ 1.1.1 asks whether that restricts "the license or distribution of that artwork". The answer: "No. You
  remain the author and copyright holder of that newly derived graphic or object."
- Condition 5 itself exempts "any document created using the Font Software".

So the page can have any licence, and the path markup is not a font file. Credit to Dr Nederhof is
welcome, and nothing requires it. The limit of this reading: FAQ 1.14 is about fonts extracted from
a document (it "strongly" discourages "any font extraction"), and the OFL stays on a font that is. A page of outlines of a few signs is not a font that a browser can use,
but a page that carried outlines of every sign, to be reassembled as a font, would be heading toward
redistribution, and then case (c) applies.

Separately, the code that made the page is GPL-3.0. Section 2 of the GPL says the output of a covered
work "is covered by this License only if the output, given its content, constitutes a covered work".
SVG markup typeset from your text is not a copy of Hieroxide's code. If you ship Hieroxide's own
JavaScript or `.wasm` to browsers (compat, or the module), you are distributing GPL code and must
offer its source on GPL terms. That is a different matter from the markup.

### (b) The release artifacts, if they bundle the font file

The compiled `.wasm` and the scripts contain no font data. If a release adds `NewGardiner.otf` to an
archive (for example as a convenience, so that the quick starts work), then:

- Condition 2 applies: the archive has to carry the copyright notice and the licence. Put
  [`OFL.txt`](../OFL.txt) beside the font in the archive. FAQ 1.7 says to "include the full font package,
  including any copyright notices and licensing information".
- Condition 1 is met, because the font is not sold by itself: it comes with software, and the release is
  free.
- The font stays under the OFL and the code stays under the GPL. They are separate works in one
  archive, each with its licence text in it.
- Ship the font unchanged. Do not subset or convert it (see (c)).

An archive without the font needs no OFL text. The README says where to get the font.

### (c) A page that serves the font itself

This is a page that has `@font-face { src: url(NewGardiner.otf) }`, which upstream's compat mode and
the module's `svg: 'text'` mode need, or the consistent module's `init({ font: '/NewGardiner.otf' })`,
which fetches the font from your server. It is a redistribution of the font, so:

- FAQ 2.1 ("Can I make webpages using these fonts?") says "Yes! Go ahead!"
- The copy you serve has to keep its copyright notice and licence. The unmodified file does: the notice and
  the words "Released under OFL 1.1" are in its `name` table, which condition 2 allows as "machine-readable
  metadata fields". To be safe, also put [`OFL.txt`](../OFL.txt) on your site and link to it (FAQ 1.8, "Can I host the fonts on a website for others to use?", asks for the full font package; FAQ 2.4 asks for the notices to
  be "sufficiently visible" for webfont services). A page that only uses its own copy does not need
  more than this.
- Do not sell the font by itself (condition 1).
- Converting to WOFF2 "normally is considered modification" (FAQ 2.2), with an exception for a conversion that
  keeps the font data and its metadata; subsetting is modification too (FAQ 2.6, "Yes"). A modified version is
  allowed, under the OFL, and it may not use a Reserved Font Name. NewGardiner reserves none. Keep the
  notice and the licence with it, and do not use the author's name to promote it (condition 4).

The consistent module reads the font's bytes in the browser (and the CLI on disk) only to take outlines
from it. If your server sends the font to visitors for that, this case applies to that file.

## What to do, in short

| You ship | Do |
|---|---|
| Pages with path SVG (CLI output, or `svg: 'paths'`) | Nothing for the font. Keep your own licence for the page. |
| Pages that load the font | Serve the file unchanged; put `OFL.txt` on the site. |
| A download that includes the font | Include `OFL.txt` beside it. |
| Hieroxide's scripts or `.wasm` | Offer the source under GPL-3.0 (link to this repository, the tag you used). |

The questions this document leaves open: whether outlines count as the "Font Software" in a way the FAQ
did not foresee (we think not, for the reasons in (a)), and how far FAQ 1.14 reaches. SIL's FAQ is
guidance by the licence's steward, not part of the licence.
