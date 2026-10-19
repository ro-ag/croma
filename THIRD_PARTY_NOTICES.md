# Third-party notices

croma is licensed under the Apache License 2.0 (see `LICENSE` and `NOTICE`).
This file lists third-party material that ships in this repository, with its
license.

## tree-sitter (MIT)

`tree-sitter-abc/` includes files from the tree-sitter project:

- `tree-sitter-abc/src/tree_sitter/alloc.h`, `array.h`, `parser.h` — runtime
  headers emitted by the tree-sitter CLI into every generated grammar.
- `tree-sitter-abc/web/vendor/web-tree-sitter.js` and `web-tree-sitter.wasm` —
  the `web-tree-sitter` 0.26.9 runtime, vendored for the browser demo.

```text
The MIT License (MIT)

Copyright (c) 2018 Max Brunsfeld

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## abc2xml (credit)

croma uses abc2xml.py (Copyright (C) 2012-2025 Willem G. Vree and
contributors, Lesser GNU General Public License,
<https://wim.vree.org/svgParse/abc2xml.html>) as its correctness baseline, and
follows its output conventions in three places so the two converters agree:

- the chord-quality token list in `crates/croma-core/src/musicxml/harmony.rs`
  (abc2xml's `compChordTab`);
- the General MIDI Level 1 instrument-name spellings in
  `crates/croma-core/src/musicxml/score.rs` (abc2xml's `inst_tb`; the names
  themselves are the General MIDI standard's);
- the octave shift of a `middle=` clef modifier in
  `crates/croma-core/src/lower/voice.rs` (abc2xml's `gtrans`).

These are short factual mappings re-expressed in Rust, not copied abc2xml
code. They are credited here and at each site.
