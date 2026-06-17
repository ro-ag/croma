# Croma

Croma is a Rust-first toolkit for ABC notation. The first deliverable is a
library that exports ABC to MusicXML. The CLI, formatter, and language server
are built on that library API instead of maintaining separate parsers.

## Scope

- Primary: ABC -> MusicXML from Rust library code.
- Next: thin CLI over the library.
- Later: formatter and language server using the same parse/surface model.
- MusicXML -> ABC: the reverse reader is shipped in the CLI
  (`croma read` / `croma musicxml2abc`), inverting croma's own writer and reading
  foreign MusicXML (abc2xml/MuseScore/Finale/Sibelius). See
  [`docs/musicxml-reader.md`](docs/musicxml-reader.md).
- Packaging: the core library must remain publishable as a normal crates.io
  Rust crate. The reader's only dependency (`roxmltree`) is opt-in
  (`croma-core` feature `musicxml-reader`) and ships with the CLI binary, never
  the library's default build.
- Out of initial scope: PDF rendering, broad engraving layout.

## Specification Target

ABC 2.1 is the stable target. ABC 2.2 is treated as a draft compatibility mode,
not as the default grammar.

- ABC 2.1: https://abcnotation.com/wiki/abc:standard:v2.1
- ABC 2.2 draft: https://abcnotation.com/wiki/abc:standard:v2.2

## Workspace

```text
crates/croma-core  Rust library and ABC -> MusicXML exporter
crates/croma-cli   Thin command-line wrapper
crates/croma-fmt   Formatter crate, built on the core model
crates/croma-lsp   Language-server support, built on the core model
```

`croma-core` is organized as a pipeline: `surface`, `parser`, `model`, and
`musicxml`. The current implementation is intentionally small; the module shape
is the contract for the fuller parser.

## Development

This repository targets the latest stable Rust toolchain, currently Rust 1.96.0,
pinned by `rust-toolchain.toml`. Work in a Linux cloud sandbox (`rustup` + `uv`)
or a local Nix flake; both are described in
[`docs/development-environment.md`](docs/development-environment.md).

Agents (and humans) should start each session with the idempotent bootstrap,
which provisions the toolchain, restores the progress database, and reports
testbed status. See [`AGENTS.md`](AGENTS.md) for the full workflow.

```sh
tools/session_bootstrap.sh
cargo test --workspace
cargo run -p croma-cli -- xml examples/basic.abc
just check
```

Private analysis, scratch notes, and review transcripts belong under
`docs/untracked/`; that directory is ignored by Git.
