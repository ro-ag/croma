# tree-sitter-abc

A [tree-sitter](https://tree-sitter.github.io/tree-sitter/) grammar for
[ABC music notation](https://abcnotation.com/) (ABC 2.1), authored as part of
the [croma](https://github.com/ro-ag/croma) project.

It is a **reusable syntax asset**: one grammar drives highlighting, folding, and
structural navigation across many consumers — the [Zed](https://zed.dev) editor,
croma's GPUI app, the browser (via `web-tree-sitter` / WASM), and Markdown
` ```abc ` fenced-block injection. Neovim and Helix consume it directly too.

The grammar is **grounded in croma's strict ABC 2.1 recognition** — the
`MusicTokenKind` taxonomy in `crates/croma-core/src/syntax/music.rs` and the
music-line parser in `crates/croma-core/src/parse/`. It mirrors croma's surface
tokens and does not invent syntax. Where input is malformed, it degrades
gracefully (ERROR nodes); the `croma-lsp` semantic-token provider is the
authoritative highlighter and backstops the grammar.

## What it covers

- **File / tune structure** — file-header region (directives, comments, fields,
  free text), tunes separated by blank lines, each a `tune_header` (information
  fields terminated by `K:`) plus a `tune_body`.
- **Music lines** — notes (`accidental? pitch octave* length?`), rests (`z`/`x`),
  multi-measure rests (`Z`/`X`), spacers (`y`), chords `[CEG]`, grace groups
  `{ag}`, chord symbols `"C"`, annotations `"^.."`, decorations `!trill!` / `.`
  / shorthand (`~HLMOPSTuv`), tuplets `(3`, slurs `()`, ties `-`, broken rhythm
  `>`/`<`, the full barline / repeat-ending vocabulary, voice overlay `&`, and
  `\` line continuations.
- **Body fields & lines** — inline fields `[K:G]` / `[M:3/4]` / `[V:1]`, body
  field lines, lyric `w:` lines, symbol `s:` lines.
- **Stylesheet directives** `%%…` (incl. `%%MIDI …`) and comments `%…`.

### Intentionally not modeled (graceful ERROR / croma-lsp backstop)

A from-scratch grammar over 10k real-world ABC files leaves a small, measured
residual (see the coverage gate). The grammar deliberately does **not** add
recovery rules for every malformed edge croma's parser repairs, e.g.: a bare
line-leading `:` repeat-start, octave marks in unusual positions (`(,FG)`,
`'d'`), a `:` glued directly before an annotation, and other rare
transcription/OCR artifacts. These land in ERROR nodes; `croma-lsp` semantic
tokens still highlight them correctly.

## Build & test

The tree-sitter CLI (`tree-sitter` ≥ 0.26) is required.

```sh
tree-sitter generate      # regenerate src/parser.c from grammar.js
tree-sitter test          # run the test/corpus/*.txt parse-tree assertions
tree-sitter parse FILE    # parse one .abc file and print its tree
```

The generated parser (`src/parser.c`, `src/grammar.json`,
`src/node-types.json`) **is committed**, per tree-sitter convention — consumers
do not need the CLI to build the grammar.

### ABI

The committed parser targets **ABI 14** (`tree-sitter generate --abi 14`) for
broad editor compatibility, including the tree-sitter version Zed bundles.

## Queries

Canonical highlight/structure queries live in `queries/`:

- `highlights.scm` — capture names follow tree-sitter conventions and are
  aligned with croma-lsp's semantic-token legend (`crates/croma-lsp/src/tokens.rs`):
  pitches → `@variable`, accidentals/octaves → `@attribute`, lengths/tuplets →
  `@number`, barlines/slurs/ties → `@operator`, chord-symbols/annotations →
  `@string`, decorations → `@function.macro`, field keys / inline fields →
  `@keyword`, comments → `@comment`, rests → `@constant.builtin`.
- `injections.scm` — documents the Markdown-side ` ```abc ` fenced-block
  injection rule (the rule lives in the host markdown grammar).
- `folds.scm` — folds a whole `tune`, its `tune_header`, and its `tune_body`.
- `brackets.scm` — matches chord `[...]` and grace `{...}` pairs.

## Coverage gate

`tools/prove_grammar_coverage.py` (in the croma repo root) parses the full 10k
ABC corpus with `tree-sitter parse --quiet` and reports the clean-parse rate and
the top residual categories — the grammar's analog of croma's corpus proofs.

```sh
uv run python tools/prove_grammar_coverage.py
```

## Reuse story

- **Zed** — the `editors/zed/` extension (croma stage G2) registers the `ABC`
  language, wires this grammar by repository path, and launches `croma-lsp`.
- **web / WASM** — `tree-sitter build --wasm` produces `tree-sitter-abc.wasm`,
  loadable by `web-tree-sitter` in a browser or croma's GPUI web contexts
  (croma stage G3).
- **Markdown** — the `injections.scm` rule highlights ` ```abc ` fenced blocks.
- **Neovim / Helix** — both consume tree-sitter grammars + queries directly.

## License

MIT, as part of croma.
