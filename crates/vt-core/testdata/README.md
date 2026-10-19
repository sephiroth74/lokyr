# Test data

## `UTF-8-test.txt`

Markus Kuhn's UTF-8 decoder capability and stress test, in its 2015-08-28 version, unchanged. The tests of the UTF-8 decoder read it, the fuzz seeds take its lines that are not plain ASCII, and the differential tests in `tools/differential` feed it to the VT core and to `alacritty_terminal` a line at a time.

- **Source**: <https://www.cl.cam.ac.uk/~mgk25/ucs/examples/UTF-8-test.txt>
- **Author**: Markus Kuhn
- **License**: [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)
- **SHA-256**: `b51cfe9a8d2689c90b10a13a3624092d546e0837c6ff835b6e5d713c5749c8c6`

The file is invalid UTF-8 on purpose, so `.gitattributes` marks it as binary.

## `GraphemeBreakTest.txt`

The conformance test of grapheme cluster boundaries, from the Unicode Character Database of the version that `cargo xtask ucd` pins, which writes this copy, unchanged. The tests of the grapheme segmentation read it.

- **Source**: `https://www.unicode.org/Public/<version>/ucd/auxiliary/GraphemeBreakTest.txt`
- **Author**: Unicode, Inc.
- **License**: the [Unicode License v3](https://www.unicode.org/license.txt); see the [terms of use](https://www.unicode.org/terms_of_use.html). The file keeps its copyright notice.

`just ucd-check` checks that the copy matches the pinned version.

## `parser-corpus.rs`

The byte sequences that the parser's tests compare against `vte`'s parser, which the differential tests feed to the VT core and to `alacritty_terminal` too. It is Rust, a constant that both include, so that they read the same sequences.

- **Author**: the Bolt contributors
- **License**: MIT, as the rest of the repository
