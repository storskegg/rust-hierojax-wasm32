# Hieroxide (Hierojax+Rust)

My attempt at a Rust port of Dr Nederhof's most excellent HieroJax lib.

## Disclaimer

I am very new to Rust, and still figuring out a lot about the language, which includes some really ugly code,
awkward ways about going through things, as I'm exploring, etc.

Ultimately, I'll actually make something that's production-worthy, but for now I'm just learning.


## The command line

`hieroxide render` typesets a fragment of Unicode hieroglyphs as SVG. It is made for a build step that
puts the result in static HTML: a page needs no script, no font and no stylesheet to show it.

```sh
cargo build --release -p hieroxide-cli                    # target/release/hieroxide, about 600 kB
export HIEROXIDE_FONT=docs/NewGardiner.otf                # or --font FILE; the font is not bundled

hieroxide render '𓀀𓐱𓏏𓐰𓁐' > block.svg                    # one block on stdout
echo '𓀀𓐱𓏏' | hieroxide render --dir hrl --label 'a scribe'
hieroxide render --batch < lines.jsonl > blocks.jsonl     # many lines, one process
```

Each line of a batch is `{"text": "...", "dir": "hrl", "label": "..."}` (any option, `"id"` is copied), and
each output line is `{"ok":true,"svg":"..."}` or `{"ok":false,"kind":"parse","message":"...","position":3}`.
`hieroxide render --help` lists the options. The output is the same bytes for the same input and font. Exit
status: 0, 1 if a fragment could not be rendered (in batch mode, if any line could not), 2 for a wrong command
line or font.

A block is sized in em with a `viewBox`, so it follows the text around it, and sits on the line as the
JavaScript module's does. It has `role="img"` and an `aria-label` (default `Egyptian hieroglyphs`: pass
`--label` with a transliteration or a translation). What a reader copies is the Unicode as written: a
selection of the page copies it with the signs' format controls, and a script can read it from the
`data-text` attribute of the `svg`, or of the `span` with `--separated` (`navigator.clipboard.writeText(el.dataset.text)`). `--selectable svg` leaves the
block a bare `<svg>` (the default is a `span` that holds the `svg` and the invisible text), but Chromium and WebKit then put line breaks around the text of a copy.
`--kind text` prints the smaller text form for pages that load the font.

**Inside the spans a page already has.** A page with `<span class="hierojax" ...>` elements and CSS for them wants
the block *in* the span, with the span's class and attributes kept so that its CSS applies, and no second wrapper.
`--contents` writes only that: the `svg` (with `role`, `aria-label` and `data-text`) and, before it, the invisible
text, sized in em like the `svg` and at the position the `svg` takes in the span's box (a block, an inline block,
a flex box that centres), with a generic `font-family` so that it never makes the browser fetch a font. Nothing is
said about the block's place on the line: the span has that. `hieroxide fill` does it for a file:

```sh
hieroxide fill page.html -o out.html      # or to stdout; - is stdin
```

It finds each `<span>` whose class has the token `hierojax`, keeps its opening tag byte for byte, and replaces
what is inside with the `--contents` block for the text it had, reading the options from the span's `data-*`
attributes as hierojax does (`type` and `standalone` are for other output and are ignored). Everything else
in the file is copied as it is. A filled span is filled again from the `data-text` of its `svg`, so a second run
gives the same bytes. The scanner follows the HTML tokenizer where it matters (comments; the raw text of `script`,
`style`, `textarea` and `title`; quoted attribute values; depth; character references) and stops, writing
nothing, with the line number and status 1 at anything it cannot be sure of. `--help` has the details.
A page that has been filled must not also run hierojax's script on the same spans: see "What a script does to a
filled page" in `tests/README.md`.

The Rust API is `crates/hieroxide-api` and the browser module is `js/hieroxide` (`index.d.ts` has its types).

# HieroJax

Ancient Egyptian hieroglyphic in Unicode for web pages

## More information

See [GitHub Pages](https://nederhof.github.io/hierojax/).
