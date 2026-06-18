# croma

[![CI](https://github.com/ro-ag/croma/actions/workflows/ci.yml/badge.svg)](https://github.com/ro-ag/croma/actions/workflows/ci.yml)
[![gates](https://github.com/ro-ag/croma/actions/workflows/gates.yml/badge.svg)](https://github.com/ro-ag/croma/actions/workflows/gates.yml)
[![audit](https://github.com/ro-ag/croma/actions/workflows/audit.yml/badge.svg)](https://github.com/ro-ag/croma/actions/workflows/audit.yml)
[![crates.io](https://img.shields.io/crates/v/croma-core.svg)](https://crates.io/crates/croma-core)
[![docs.rs](https://img.shields.io/docsrs/croma-core)](https://docs.rs/croma-core)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![corpus: 10k proven](https://img.shields.io/badge/corpus-10k%20proven-brightgreen.svg)](#how-its-proven)

A Rust-first toolkit for **ABC music notation** — convert ABC to MusicXML and
back, format it, and edit it with real language-server support. Everything is
built on one library (`croma-core`); the CLI, formatter, reader, and language
server are thin layers over the same parse/model pipeline, not separate parsers.

croma targets **ABC 2.1** as the stable spec ([standard](https://abcnotation.com/wiki/abc:standard:v2.1)),
with ABC 2.2 treated as a draft compatibility mode. It is parser-**strict**:
malformed input is rejected with a diagnostic rather than silently guessed at;
loose source is repaired explicitly by `croma fmt --auto-fix`.

## What it does

- **ABC → MusicXML** (`croma xml`, `croma-core`): a library-first ABC 2.1 parser
  and MusicXML 4.0 writer. This is the foundation everything else builds on.
- **Formatter** (`croma fmt`, `croma fmt --auto-fix`, `croma-fmt`): a canonical
  ABC pretty-printer. Formatting is idempotent and lossless; `--auto-fix`
  additionally sanitizes loose source (multi-voice alignment, redundant/malformed
  barlines, whitespace) into spelling the strict parser reads cleanly.
- **MusicXML → ABC** (`croma read`, `croma musicxml2abc`): the reverse reader —
  inverts croma's own writer and reads foreign MusicXML dialects (abc2xml,
  MuseScore, Finale, Sibelius).
- **Language server** (`croma-lsp`): a stdio LSP — diagnostics, formatting,
  semantic tokens, document symbols, folding, hover, completion, and code
  actions — that is a thin adapter over the core (its output is byte-identical to
  the CLI's, by construction).
- **Editor support**: a reusable [`tree-sitter-abc`](tree-sitter-abc/) grammar
  (Zed, web/WASM, Markdown ` ```abc ` injection, Neovim/Helix) and a
  [Zed extension](editors/zed/) wiring the grammar + `croma-lsp` together.

Out of scope: PDF rendering and engraving layout.

## Install

croma is pre-1.0 and not yet on crates.io. Build from source with the pinned
toolchain (Rust 1.96.0, selected automatically by `rust-toolchain.toml`):

```sh
git clone https://github.com/ro-ag/croma
cd croma
cargo build --release            # builds target/release/croma (CLI) and croma-lsp
```

A reader-less, zero-dependency CLI is available with
`cargo build -p croma-cli --no-default-features`.

## Usage

```sh
# ABC -> MusicXML
croma xml tune.abc > tune.musicxml

# Lint / diagnose (text or JSON)
croma check tune.abc
croma check --diagnostics=json tune.abc

# Format (stdout), or rewrite in place, or repair loose source
croma fmt tune.abc
croma fmt --write tune.abc
croma fmt --auto-fix tune.abc

# MusicXML -> ABC
croma read score.musicxml --format abc
croma musicxml2abc score.musicxml
```

As a library:

```rust
use croma_core::abc_to_musicxml;

let xml = abc_to_musicxml("X:1\nT:Scale\nM:4/4\nL:1/8\nK:C\nC D E F G A B c|\n")?;
```

`croma-core`'s default build is **zero-dependency** and crates.io-publishable.
The MusicXML reader's only dependency (`roxmltree`) is opt-in via the
`musicxml-reader` feature and ships on the CLI binary, never the library default.

## Workspace

```text
crates/croma-core   ABC 2.1 parser, model, and ABC<->MusicXML conversion (the library)
crates/croma-cli    the `croma` command-line tool (thin wrapper)
crates/croma-fmt    the formatter / auto-fixer, on the core model
crates/croma-lsp    the stdio language server, a thin adapter over core + fmt
tree-sitter-abc/    reusable ABC grammar (outside the cargo workspace)
editors/zed/        Zed extension (outside the cargo workspace)
```

Per-capability docs live in [`docs/`](docs/): [formatter](docs/formatter.md),
[MusicXML reader](docs/musicxml-reader.md), [LSP](docs/lsp.md),
[editors](docs/editors.md), and the [benchmark baseline](docs/benchmarks.md).

## How it's proven

croma's correctness is validated against a **real-world corpus of 10,000 ABC
files** (the [Zenodo ABC dataset](https://doi.org/10.5281/zenodo.17694747)), not
just hand-written unit tests (though there are ~800 of those too). Every shipped
capability has a corpus-scale gate that must stay green:

| Capability | Gate | Result (10k corpus) |
| --- | --- | --- |
| ABC → MusicXML writer | structural parity vs `abc2xml` (raw comparator) | **9,390 / 9,390** adjudicated matches |
| Formatter | idempotent **and** lossless re-formatting | **10,000 / 10,000** |
| MusicXML → ABC reader | self-loop XML re-emission | **9,935 / 9,935** |
| MusicXML → ABC reader | foreign-dialect parity vs music21 | **98.50%** |
| `croma-lsp` | diagnostics / formatting / token fidelity vs core | **10,000 / 0** mismatches |
| `croma-lsp` | totality (no panics, no hangs on malformed input) | **0 panics / 10,000** |
| `tree-sitter-abc` | clean parse (no ERROR nodes) | **99.46%** (9,946 / 10,000) |

Plus `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings`, and
`cargo fmt --check` on every change. Each gate's residual is *adjudicated* — a
documented, spec-justified non-bug — never an unexplained failure.

The corpus, the abc2xml comparator, the spec knowledge base, and the Python
provers live in a companion repository, **croma-test**, so a developer working on
croma never has to download the heavy proving apparatus. croma builds and tests
standalone (the corpus-scale gates are environment-gated and skip cleanly when the
corpus is absent); the full matrix runs from croma-test.

## How croma compares to abc2xml

[`abc2xml`](https://wim.vree.org/svgParse/abc2xml.html) (by Willem Vree) is the
long-standing reference ABC→MusicXML converter, and it is croma's correctness
*baseline*: croma is validated against it over the full 10k corpus. Where abc2xml
is spec-correct, croma matches it (9,390 / 9,390 structural matches); croma
diverges only where abc2xml departs from the ABC 2.1 spec, and every such case is
adjudicated and documented. croma is a from-scratch, library-first reimplementation
that improves on the reference in several ways:

| | abc2xml | croma |
| --- | --- | --- |
| **Direction** | ABC → MusicXML (the reverse is a separate script, `xml2abc`) | ABC ↔ MusicXML in one library |
| **Form** | a Python script (needs a Python runtime) | a Rust library + native binaries — zero-dependency, embeddable, crates.io-publishable, callable from any language |
| **Speed** | interpreted Python | compiled Rust — **7,081** ABC→MusicXML files/s and **43,247** parse files/s over the 10k corpus |
| **Malformed input** | permissive — silent best-effort heuristics | strict ABC 2.1 — structured **diagnostics** (codes + spans); recovers only when the intent is unambiguous, and always warns |
| **Output artifacts** | inserts spurious elements as heuristic side effects — e.g. empty leading/section measures, phantom measures | spec-faithful, **minimal** MusicXML — declines those artifacts; most croma↔abc2xml divergences are exactly such an artifact croma omits |
| **Beyond conversion** | converter only | also a **formatter** (idempotent + lossless), a **language server** (live editor diagnostics / formatting / completion), a reusable **tree-sitter grammar**, and a **Zed extension** |
| **Validation** | — | corpus-proven over 10k with a documented gate matrix + a reproducible benchmark baseline |
| **Safety** | — | memory-safe Rust (`unsafe` forbidden), pinned toolchain, clippy + fmt gates |

In short: abc2xml is a mature one-way converter script; croma is a fast,
embeddable, **bidirectional** ABC toolkit that holds **stricter to the spec** and
emits **cleaner, lower-artifact** MusicXML, with editor-grade diagnostics —
designed to be linked into applications and proven at scale. (It also stands on abc2xml's
shoulders: using it as the parity baseline is how croma earns its correctness
claims.)

## Benchmarks

Headline throughput/latency (Apple M4 Max, Rust 1.96.0, `--release`). Full
methodology, per-call micro-benchmarks, and reproduction steps:
[`docs/benchmarks.md`](docs/benchmarks.md).

| Layer | Headline |
| --- | --- |
| Parser (corpus, in-process) | **43,247 files/s** · 23.9 MB/s |
| Formatter (corpus, in-process) | **27,450 files/s** · 15.2 MB/s |
| ABC → MusicXML writer (corpus) | **7,081 files/s** · 3.9 MB/s |
| LSP diagnostics, real-size p99 | **≤ 4.76 ms** (release ceiling 50 ms) |
| LSP semantic tokens, real-size p99 | **≤ 0.62 ms** |
| `tree-sitter-abc` (steady state) | **~8.5–8.9 MB/s** |

The corpus median file is 14 lines (max 244); at those sizes every operation is
in the low-millisecond range. The ABC→MusicXML export path is super-linear on
synthetic 1000-line inputs (recorded, no real-use impact — see benchmarks §6).

## Contributing

Contributions are welcome — see [`CONTRIBUTING.md`](CONTRIBUTING.md) for setup,
how to run the gates, and the conventions (strict-spec parser policy, branch and
commit rules, and the "don't regress a proven gate" discipline).

## License

Licensed under the **Apache License, Version 2.0** — see [`LICENSE`](LICENSE) and
[`NOTICE`](NOTICE).

You may use croma freely, including in commercial products. In return, any
redistribution or derivative work must **retain the attribution** in `NOTICE`
(per section 4 of the license) — i.e. commercial users must credit croma. The
software is provided **as-is, without warranty**, and the author carries **no
liability** for its use (sections 7–8).
