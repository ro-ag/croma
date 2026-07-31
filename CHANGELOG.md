# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.2.1] - 2026-07-31

### Added

- **`score` agent help topic.** `croma agent score` (aliases: `%%score`,
  `grouping`, `staves`, `%%staves`, `staff-grouping`) documents the `%%score` /
  `%%staves` staff-grouping semantics with a runnable example: `( )` overlays its
  voices on ONE staff of one part, `{ }` is a grand staff only when every member
  is a staff of the same voice (`Piano`, `Piano#2`), and `[ ]` / bare voices keep
  one single-staff part per voice — so LLM agents driving the dialect stop
  cramming separate instruments onto one staff or merging unrelated voices into a
  phantom keyboard part. ([#259], [#260])

### Changed

- **Dependency bumps.** `clap` 4.6.4, `serde` 1.0.229, `serde_json` 1.0.151, and
  `lsp-server` 0.10.0; the croma-lsp transport test is ported to `lsp-server`
  0.10's merged `Response::response_result` field. No library API changes.
  ([#261])

## [1.2.0] - 2026-07-08

### Added

- **Opt-in engraving export (`croma xml --engrave`).** A render-oriented MusicXML
  mode that emits computed engraving hints so downstream renderers no longer have to
  auto-beam or guess layout: beam grouping (`<beam>` with secondary sub-beams and
  partial-beam hooks), convention-default `<tuplet>` bracket (number-only for a
  fully-beamed tuplet), stem direction (`<stem>`), multi-voice rest placement
  (`<display-step>`/`<display-octave>`), and slur/tie placement. The rules are ported
  from MuseScore's engraving core. The default export stays byte-for-byte the
  round-trip form, and each hint is individually toggleable via
  `MusicXmlWriteOptions`. `croma-core` stays zero-dependency. ([#250], [#251])

## [1.1.4] - 2026-07-06

### Fixed

- **Vocal dynamics and hairpins default above lyrics.** Placement-less ABC
  dynamics and crescendo/diminuendo wedges now export above lyric-bearing parts,
  avoiding collisions with `<lyric>` text, while instrumental parts keep the
  existing below-staff default and explicit direction-placement carriers still
  win. ([#248])

## [1.1.3] - 2026-07-05

### Added

- **Direction placement carrier.** MusicXML-imported dynamics, wedges, coda, and
  segno marks now preserve their `placement` attribute through ABC via
  `[I:croma-direction-placement]`, so above/below annotations survive the
  `MusicXML -> ABC -> MusicXML` projection. ([#244])

### Fixed

- **Trailing wedge stops on spacers.** Hairpin stop directions that land after the
  last note now survive ABC projection on a zero-duration spacer, avoiding open
  hairpins and over-long Voice-lane wedges. ([#245])
- **Croma-emitted `%%MIDI` warning noise.** Carrier-warning suppression now also
  covers croma's own `%%MIDI` program/control/channel/transpose directives, plus
  inline `[I:MIDI=...]` spelling, so MusicXML-imported scores do not re-warn on
  round-trip parse when callers opt into carrier suppression. ([#246])

[1.2.1]: https://github.com/ro-ag/croma/releases/tag/v1.2.1
[1.2.0]: https://github.com/ro-ag/croma/releases/tag/v1.2.0
[1.1.4]: https://github.com/ro-ag/croma/releases/tag/v1.1.4
[1.1.3]: https://github.com/ro-ag/croma/releases/tag/v1.1.3
[#259]: https://github.com/ro-ag/croma/issues/259
[#260]: https://github.com/ro-ag/croma/pull/260
[#261]: https://github.com/ro-ag/croma/pull/261
[#250]: https://github.com/ro-ag/croma/issues/250
[#251]: https://github.com/ro-ag/croma/pull/251
[#244]: https://github.com/ro-ag/croma/issues/244
[#245]: https://github.com/ro-ag/croma/issues/245
[#246]: https://github.com/ro-ag/croma/issues/246
[#248]: https://github.com/ro-ag/croma/issues/248

## [1.1.2] - 2026-07-04

### Added

- **Croma private carrier warning suppression.** Library callers can now silence
  expected forward-compatibility warnings for unknown `[I:croma-*]`, `I:croma-*`,
  and `%%croma-*` carriers via `ParseOptions::suppress_croma_carrier_warnings()`
  or `ExportOptions::suppress_croma_carrier_warnings()`. The CLI exposes the same
  filter as `--silence-croma-carrier-warnings`; ordinary unsupported directives
  still warn.

[1.1.2]: https://github.com/ro-ag/croma/releases/tag/v1.1.2

## [1.1.1] - 2026-06-30

A MusicXML-reader fidelity patch: multi-part scores with heterogeneous
`<divisions>`, `<movement-title>`-only metadata, named composers, and piano grand
staves now survive the `MusicXML → ABC → MusicXML` round-trip. ([#241])

### Added

- **Multi-staff grand-staff round-trip.** A piano grand staff — one `<part>` with
  `<staves>2` and a `<clef>` per staff — now reconstructs its staves, routes each
  voice to its staff, and projects a `%%score {…}` brace, so the lower staff's
  bass clef survives the round trip instead of reading in treble. A brace over
  distinct part ids (a `<part-group symbol="brace">`) still stays separate parts.
  ([#241])

### Fixed

- **Per-part `<divisions>`** ([#239]). The reader took the first `<divisions>` in
  document order and applied it to every part, so a part declaring a different
  divisions value — e.g. a piano staff at `8` against a vocal staff at `48` — had
  every duration scaled by the ratio, shrinking each bar to a fraction of its
  length. Each part, and each measure, now decodes `<duration>` against its own
  `<divisions>`.
- **`<movement-title>` fallback** ([#240]). A score titled only via top-level
  `<movement-title>` (common in Finale/MuseScore exports) read back with no title
  and lost its `T:` line. The reader now falls back to `<movement-title>` when
  `<work><work-title>` is absent; `<work-title>` still wins when both are present.
- **Composer projection.** `<creator type="composer">` now projects to the ABC
  `C:` field, so a composer survives `MusicXML → ABC → MusicXML` instead of
  surviving only as `<credit>` words. ([#241])

[1.1.1]: https://github.com/ro-ag/croma/releases/tag/v1.1.1
[#239]: https://github.com/ro-ag/croma/issues/239
[#240]: https://github.com/ro-ag/croma/issues/240
[#241]: https://github.com/ro-ag/croma/pull/241

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

[Unreleased]: https://github.com/ro-ag/croma/compare/v1.2.0...HEAD
[0.9.0]: https://github.com/ro-ag/croma/releases/tag/v0.9.0
