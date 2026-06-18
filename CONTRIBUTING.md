# Contributing to croma

Thanks for your interest in croma. This guide covers setup, how to run the
gates, and the conventions that keep the project's correctness guarantees intact.
For the deeper workflow and the rationale behind the parser's design, read
[`AGENTS.md`](AGENTS.md).

## Setup

croma pins **Rust 1.96.0** via [`rust-toolchain.toml`](rust-toolchain.toml), so a
plain `cargo`/`rustc` selects the right toolchain on any host. Run the idempotent
bootstrap once per session — it reports git state, provisions the toolchain, and
builds the CLI:

```sh
tools/session_bootstrap.sh
```

The corpus-scale proving suite lives in a separate, optional repository
(**croma-test**: the Python provers, the 10k ABC corpus, the abc2xml comparator,
the spec knowledge base). You only need it to run the full corpus gates. Clone it
alongside croma with:

```sh
tools/session_bootstrap.sh --with-suite     # clones croma-test into ./croma-test/ (git-ignored)
```

## Build and test

croma builds and tests standalone — the corpus-scale gates are environment-gated
and skip cleanly when the corpus is absent:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo run -p croma-cli -- xml examples/basic.abc
```

The **full corpus gate matrix** (ABC→MusicXML parity, fmt idempotence/losslessness,
reader round-trip, LSP fidelity/totality, grammar coverage) runs from croma-test —
see its `README` and `bootstrap.sh`. On every pull request, croma's `gates` CI
clones croma-test and runs that matrix against your change; the cargo-only `ci`
workflow runs in parallel.

## The one rule that matters: don't regress a proven gate

croma's value is that its conversions are corpus-proven (see
[the README](README.md#how-its-proven)). Any change that touches the parser,
writer, formatter, reader, or LSP must keep those gates green. If a change *does*
move the numbers, that is acceptable **only** when the new behavior is more
spec-correct and the moved files are *adjudicated* — documented as spec-justified
non-bugs via the divergence-triage process — never an unexplained regression.

The parser is **strict to ABC 2.1**: reject malformed input with a diagnostic;
recover (and always warn) only for a clear intention spoiled by a trivial,
mechanical slip; otherwise reject. Loose-source repair belongs in
`croma fmt --auto-fix`, not the parser. Full policy in [`AGENTS.md`](AGENTS.md).

`croma-core` must stay **zero-dependency and crates.io-publishable** — no
path-only/local runtime assumptions in library code. New runtime deps belong on
the binaries (CLI/LSP) or behind an opt-in feature, never the library default.

## Branches and commits

- **Never commit to `main`.** Branch by change type: `feature/<slug>`,
  `bugfix/<slug>`, or `refactor/<slug>`.
- Use [Conventional Commits](https://www.conventionalcommits.org/) subjects
  (`feat:`, `fix:`, `docs:`, `refactor:`, `ci:`, `test:`, …).
- **No AI co-author trailers** in commit messages.
- Open a pull request when the checks above pass. Keep PRs focused.

## Code style

- `cargo fmt` is the formatter of record; clippy runs with `-D warnings`.
- Favor comments that genuinely aid reading — explain *why* and the flow of
  non-obvious or abstract code; give complex tests a short framing of what each
  phase proves. Concise, not absent, and not padding.

## License

By contributing, you agree that your contributions are licensed under the
**Apache License, Version 2.0** (see [`LICENSE`](LICENSE)), the same license as
the project.
