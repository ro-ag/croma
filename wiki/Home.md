# croma wiki

[![CI](https://github.com/ro-ag/croma/actions/workflows/ci.yml/badge.svg)](https://github.com/ro-ag/croma/actions/workflows/ci.yml)
[![gates](https://github.com/ro-ag/croma/actions/workflows/gates.yml/badge.svg)](https://github.com/ro-ag/croma/actions/workflows/gates.yml)
[![audit](https://github.com/ro-ag/croma/actions/workflows/audit.yml/badge.svg)](https://github.com/ro-ag/croma/actions/workflows/audit.yml)
[![crates.io](https://img.shields.io/crates/v/croma-core.svg)](https://crates.io/crates/croma-core)
[![docs.rs](https://img.shields.io/docsrs/croma-core)](https://docs.rs/croma-core)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](https://github.com/ro-ag/croma/blob/main/LICENSE)
[![corpus: 10k proven](https://img.shields.io/badge/corpus-10k%20proven-brightgreen.svg)](https://github.com/ro-ag/croma/wiki/How-its-Proven)

**croma** is a Rust-first toolkit for [**ABC music notation**](https://abcnotation.com/wiki/abc:standard:v2.1) —
convert ABC to MusicXML and back, format it, and edit it with real
language-server support. Everything is built on one library (`croma-core`); the
CLI, formatter, reader, and language server are thin layers over the same
parse/model pipeline, not separate parsers. croma targets **ABC 2.1** as the
stable spec (ABC 2.2 is a draft compatibility mode) and is parser-**strict**:
malformed input is rejected with a diagnostic rather than silently guessed at,
and loose source is repaired explicitly by `croma fmt --auto-fix`.

> This wiki is task-oriented (quickstarts, guides, FAQ, troubleshooting). For
> deep per-capability reference, it links out to the canonical
> [`docs/`](https://github.com/ro-ag/croma/tree/main/docs) in the repo.

## 30-second quickstart

```sh
# Install the CLI from crates.io
cargo install croma-cli

# Convert an ABC tune to MusicXML
printf 'X:1\nT:Scale\nM:4/4\nL:1/8\nK:C\nC D E F|G A B c|\n' > scale.abc
croma xml scale.abc > scale.musicxml
```

More install routes (prebuilt binaries, from source, reader-less build):
[[Installation]].

## What it does

- **ABC → MusicXML** (`croma xml`, `croma-core`) — a library-first ABC 2.1
  parser and MusicXML 4.0 writer. The foundation everything else builds on.
- **Formatter** (`croma fmt`, `--auto-fix`) — a canonical ABC pretty-printer;
  idempotent and lossless, with an opt-in catalogue of safe repairs for loose
  source. → [[Formatter]]
- **MusicXML → ABC** (`croma read`, `croma musicxml2abc`) — the reverse reader;
  inverts croma's own writer and reads foreign MusicXML dialects. →
  [[MusicXML-Reader]]
- **Language server** (`croma-lsp`) — a stdio LSP (diagnostics, formatting,
  semantic tokens, symbols, folding, hover, completion, code actions) that is a
  thin adapter over the core. → [[Language-Server]]
- **Editor support** — a reusable `tree-sitter-abc` grammar and a Zed extension.
  → [[Editors-and-Zed]]

**Out of scope:** PDF rendering and engraving layout.

## Table of contents

| Page | What's there |
| --- | --- |
| [[Installation]] | crates.io, prebuilt binaries, from source, reader-less build |
| [[CLI-Usage]] | every `croma` subcommand with real examples and flags |
| [[Library-Usage]] | the `croma-core` API and the zero-dependency guarantee |
| [[Formatter]] | `croma fmt` / `--auto-fix` orientation |
| [[MusicXML-Reader]] | `croma read` / `croma musicxml2abc` orientation |
| [[Language-Server]] | `croma-lsp` orientation |
| [[Editors-and-Zed]] | tree-sitter grammar reuse + Zed extension |
| [[How-its-Proven]] | the corpus gate matrix and the repo split |
| [[Testing-Methodology]] | how accuracy and efficiency are proven |
| [[Conversion-Challenges]] | the hard ABC ↔ MusicXML cases and how they were resolved |
| [[abc2xml-Comparison]] | baseline, parity, and where croma diverges |
| [[Benchmarks]] | headline throughput / latency |
| [[FAQ]] | strict parsing, ABC 2.2, Rust, commercial use, PDF, bugs |
| [[Troubleshooting]] | reading diagnostics, gate skipping, build issues |
| [[Contributing]] | gates discipline, strict-spec policy, conventions |

---

Licensed under [Apache-2.0](https://github.com/ro-ag/croma/blob/main/LICENSE).
Source: [github.com/ro-ag/croma](https://github.com/ro-ag/croma).
