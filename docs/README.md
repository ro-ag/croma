# Croma documentation

Documentation for the **croma** toolkit. The corpus-scale proving suite, abc2xml
comparator baseline, progress tracker, and design-decisions trail live in the
separate, private [croma-test](https://github.com/ro-ag/croma-test) repository.

## Capabilities

- [formatter.md](formatter.md) — `croma fmt` / `--auto-fix`: canonical ABC
  pretty-printer, idempotent + lossless over the 10k corpus.
- [musicxml-reader.md](musicxml-reader.md) — `croma read` / `croma musicxml2abc`:
  MusicXML → ABC reader (self-loop 9935/9935, foreign music21 parity 98.50%).
- [lsp.md](lsp.md) — `croma-lsp`: stdio language server, a thin adapter over the
  core/formatter.
- [editors.md](editors.md) — the reusable `tree-sitter-abc` grammar + Zed
  extension (also web/WASM, Markdown ` ```abc ` injection, Neovim/Helix).

The forward ABC → MusicXML writer is the foundation under all of the above; its
behavior is documented across the capability docs.

## Reference & policy

- [carriers.md](carriers.md) — the `[I:croma-*]` / `%%croma-*` private carrier
  namespace: how croma round-trips MusicXML facts ABC can't express while staying
  ignorable by other tools. Definition, syntax, the round-trip contract, and the
  full 20-carrier catalogue.
- [midi-directives.md](midi-directives.md) — `%%MIDI` directive policy and its
  MusicXML translation.
- [parser-backlog.md](parser-backlog.md) — open parser/model/export items.

## Operations

- [development-environment.md](development-environment.md) — toolchain + dev setup.
- [benchmarks.md](benchmarks.md) — performance baseline (criterion + corpus
  throughput + LSP latency).
- [releasing.md](releasing.md) — release mechanics and runbook.
