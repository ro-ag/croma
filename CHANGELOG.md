# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.1.0] - 2026-06-29

This release makes croma's `MusicXML → ABC → MusicXML` round-trip lossless across
the foreign-import surface, via a private **carrier** system, and adds a
`croma agent` help surface so AI agents can author those annotations.

### Added

- **Private carrier system (`[I:croma-*]` / `%%croma-*`).** Namespaced
  annotations that round-trip MusicXML facts ABC 2.1 cannot natively express,
  while staying ignorable by other ABC tools (abc2midi / abcm2ps / abcjs). The
  convention, syntax, the `-hex=` rule, and the full catalogue are documented in
  [`docs/carriers.md`](docs/carriers.md). ([#234])
- **`croma agent` — help topics for AI agents / LLMs**, plus a `croma-core`
  library API (`agent_topics()`, `find_agent_topic()`, `AgentTopic`). Each
  carrier is framed as a task with its syntax, a copy-paste ABC example, and a
  `verify` command, so an agent can author ABC annotations that persist to
  MusicXML. `croma-core` stays zero-dependency. ([#236])
- **Cross-voice slur carrier (`[I:croma-xvoice-slur]`).** A slur whose start and
  stop are in different voices — which ABC `(`/`)` cannot span — now round-trips
  losslessly. ([#234])
- **Lossless MusicXML round-trip across the foreign-import surface**
  ([#193]–[#233]): carry-through for part/voice origin metadata and ids, per-note
  and unpitched MIDI instrument maps, functional `<harmony>` text, printed and
  playback-only tempo text, duplicate and extended lyrics, articulations,
  tremolos, technical notations, spanners, grace decorations, extended dynamics,
  tuplet display and wide tuplets, measure labels, sparse-voice gaps, meter
  restatements, `<backup>`/`<forward>` cursor moves, and asymmetric clef-change
  cursors.

### Fixed

- Chord-closing slur stops attach to the chord head ([#229]); chord-led lyric
  extend/duplicate carriers ride to the chord head ([#230]); control characters
  are normalised in carrier names and section-label projection; a bare root is
  emitted for unmodellable harmony kinds; and the final niche PDMX
  reader-roundtrip residual is cleared ([#233]).

[#193]: https://github.com/ro-ag/croma/issues/193
[#229]: https://github.com/ro-ag/croma/issues/229
[#230]: https://github.com/ro-ag/croma/issues/230
[#233]: https://github.com/ro-ag/croma/issues/233
[#234]: https://github.com/ro-ag/croma/issues/234
[#236]: https://github.com/ro-ag/croma/issues/236

## [1.0.2] - 2026-06-27

### Fixed

- **Score→ABC writer now emits per-voice `%%MIDI` directives.** The writer
  (`croma read` / `croma musicxml2abc`) dropped `Voice::midi_instrument` /
  `Voice::midi_transpose`, so a `MusicXML → ABC → MusicXML` round-trip lost all
  instrument routing and collapsed every part onto the default channel. It now
  re-emits `%%MIDI program`/`channel`/`control 7`/`control 10`/`transpose` after
  each voice's `V:` switch — the inverse of the forward MusicXML projection — so
  program, channel and transpose survive value-for-value. ([#189])

[#189]: https://github.com/ro-ag/croma/issues/189

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
