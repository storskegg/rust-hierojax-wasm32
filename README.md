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
`data-text` attribute (`navigator.clipboard.writeText(el.dataset.text)`). `--selectable svg` leaves the
block a bare `<svg>`, but Chromium and WebKit then put line breaks around the text of a copy.
`--kind text` prints the smaller text form for pages that load the font.

The Rust API is `crates/hieroxide-api` and the browser module is `js/hieroxide` (`index.d.ts` has its types).

# HieroJax

Ancient Egyptian hieroglyphic in Unicode for web pages

## More information

See [GitHub Pages](https://nederhof.github.io/hierojax/).
