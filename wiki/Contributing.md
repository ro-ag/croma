# Contributing

This is the wiki distillation; the canonical guide is
[**`CONTRIBUTING.md`**](https://github.com/ro-ag/croma/blob/main/CONTRIBUTING.md),
and the deeper workflow + parser rationale is in
[**`AGENTS.md`**](https://github.com/ro-ag/croma/blob/main/AGENTS.md). Read both
before a non-trivial change.

## Setup

croma pins **Rust 1.96.0** via `rust-toolchain.toml`, so a plain `cargo` selects
the right toolchain. Run the idempotent bootstrap once per session:

```sh
tools/session_bootstrap.sh                  # git state + toolchain + build the CLI
tools/session_bootstrap.sh --with-suite     # also clone croma-test into ./croma-test/ (git-ignored)
```

You only need `--with-suite` to run the full corpus gates — croma builds and
tests standalone otherwise ([[How-its-Proven]]).

## Build and test

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo run -p croma-cli -- xml examples/basic.abc
```

The corpus-scale gates are environment-gated and skip cleanly when the corpus is
absent. On every pull request the `gates` CI clones croma-test and runs the full
matrix; the cargo-only `ci` workflow runs in parallel.

## The one rule that matters: don't regress a proven gate

croma's value is that its conversions are **corpus-proven**. Any change touching
the parser, writer, formatter, reader, or LSP must keep those gates green
([[How-its-Proven]]). If a change *does* move the numbers, that is acceptable
**only** when the new behaviour is more spec-correct **and** the moved files are
**adjudicated** — documented as spec-justified non-bugs via the
divergence-triage process — never an unexplained regression.

## Strict-spec parser policy

The parser is **strict to ABC 2.1**: reject malformed input with a diagnostic;
recover (and **always warn**) only for a clear intention spoiled by a trivial,
mechanical slip; otherwise reject. Loose-source repair belongs in
`croma fmt --auto-fix`, **not** the parser ([[Formatter]], [[FAQ#why-is-the-parser-strict]]).

## Keep `croma-core` zero-dependency

`croma-core` must stay **zero-dependency and crates.io-publishable** — no
path-only / local runtime assumptions in library code. New runtime deps belong on
the binaries (CLI / LSP) or behind an opt-in feature (like the reader's
`musicxml-reader` / `roxmltree`), never the library default. CI asserts the
zero-dep guard.

## Branches and commits

- **Never commit to `main`.** Branch by change type: `feature/<slug>`,
  `bugfix/<slug>`, or `refactor/<slug>`.
- Use [Conventional Commits](https://www.conventionalcommits.org/) subjects
  (`feat:`, `fix:`, `docs:`, `refactor:`, `ci:`, `test:`, …).
- **No AI co-author trailers** in commit messages.
- Open a focused pull request once the checks above pass.

## Code style

`cargo fmt` is the formatter of record; clippy runs with `-D warnings`. Favour
comments that genuinely aid reading — explain *why* and the flow of non-obvious
or abstract code, and give complex tests a short framing of what each phase
proves. Concise, not absent, and not padding.

## License

By contributing you agree your contributions are licensed under **Apache-2.0**
(see [`LICENSE`](https://github.com/ro-ag/croma/blob/main/LICENSE)), the same
license as the project.
