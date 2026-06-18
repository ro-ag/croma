# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.1] - 2026-06-18

### Changed

- Adopt **lsp-types 0.97** in `croma-lsp` (migrate the removed `Url` to the new
  `Uri` type; key the document store and workspace edits on it).
- Dependency updates: `roxmltree` 0.20→0.21, `anstream` 0.6→1.0, `criterion`
  (dev) 0.5→0.8, plus GitHub Actions bumps (checkout, cache, setup-uv,
  upload/download-artifact).

### CI

- The `gates` workflow now skips on Dependabot PRs (they can't read the
  `CROMA_TEST_TOKEN` secret needed to clone croma-test); `ci` + `audit` still
  gate those PRs.

## [1.0.0] - 2026-06-18

First **public** release. The four crates (`croma-core`, `croma-fmt`,
`croma-cli`, `croma-lsp`) are published to crates.io in lockstep at `1.0.0`, and
prebuilt CLI + `croma-lsp` binaries ship for macOS / Linux / Windows via GitHub
Releases.

### Changed

- **Relicensed to Apache-2.0** (from MIT). Commercial use is allowed but must
  retain the attribution in `NOTICE`; the software is provided as-is, with no
  warranty or liability.
- **Repository split.** The corpus-scale proving suite — the Python provers, the
  10k ABC corpus, the abc2xml comparator + whitelist/dropped baseline, the ABC
  spec knowledge base, the divergence-triage tooling, the progress tracker, and
  the design-decisions trail — moved to the separate companion `croma-test`
  repository. croma is now a lean Rust toolkit that builds and tests standalone;
  corpus-scale proofs run from croma-test. `croma-core` remains zero-dependency
  and crates.io-publishable.

### Added

- A comprehensive README (capabilities, the 10k-corpus proof results, a benchmark
  baseline, and an `abc2xml` comparison) and a `CONTRIBUTING` guide.

## [0.9.0] - 2026-06-17

First public, crates.io-ready release of the Croma toolkit. All four workspace
crates (`croma-core`, `croma-fmt`, `croma-cli`, `croma-lsp`) ship in lockstep at
`0.9.0`.

### Added

- **ABC -> MusicXML exporter** (`croma-core`): a library-first ABC 2.1 parser and
  MusicXML writer. The exporter is corpus-proven, producing a structural match
  against abc2xml on 9390 of 9390 adjudicated files in the 10k-file ABC corpus.
  The default build is zero-dependency and publishable as a normal crates.io crate.
- **Formatter** (`croma-fmt`, `croma fmt` / `croma fmt --auto-fix`): a canonical
  ABC pretty-printer built on the core surface model. Formatting is idempotent and
  lossless over the full 10k-file corpus; `--auto-fix` additionally sanitizes loose
  source (multi-voice alignment, redundant/malformed barlines, whitespace).
- **MusicXML -> ABC reader** (`croma read` / `croma musicxml2abc`): inverts croma's
  own writer (self-loop 9935/9935) and reads foreign MusicXML dialects (abc2xml,
  MuseScore, Finale, Sibelius) with 98.50% structural parity against music21. The
  reader's only dependency (`roxmltree`) is opt-in via the `croma-core`
  `musicxml-reader` feature and ships with the CLI binary, never the library default.
- **Language server** (`croma-lsp`): a stdio LSP implementation, a thin adapter over
  `croma-core` and `croma-fmt`, providing diagnostics, formatting, semantic tokens,
  document symbols, folding ranges, hover, completion, and code actions.
- **Editor integration**: a reusable `tree-sitter-abc` grammar (Zed, web/WASM,
  Markdown ` ```abc ` injection, Neovim/Helix) and a Zed editor extension wiring the
  grammar to `croma-lsp`.
- **Benchmark suite**: a criterion-based performance baseline covering parser,
  writer, reader, formatter, corpus throughput, and LSP latency, with a committed
  reference report in [`docs/benchmarks.md`](docs/benchmarks.md).

[Unreleased]: https://github.com/ro-ag/croma/compare/v0.9.0...HEAD
[0.9.0]: https://github.com/ro-ag/croma/releases/tag/v0.9.0
