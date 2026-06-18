# AGENTS.md

Guidance for AI agents (Claude Code, Codex, etc.) working in **croma** — the Rust
toolkit repository. Read this first, every session.

## Two repositories

croma is split in two:

- **croma** (this repo) — the toolkit: `croma-core`, `croma-cli`, `croma-fmt`,
  `croma-lsp`, the reusable `tree-sitter-abc` grammar, and the Zed extension. It
  builds and tests standalone. This is what a developer clones to build, fix, and
  ship.
- **croma-test** (private, <https://github.com/ro-ag/croma-test>) — the
  corpus-scale proving suite: the Python provers, the 10k ABC corpus, the abc2xml
  comparator + whitelist/dropped baseline, the ABC 2.1 spec knowledge base + the
  divergence-triage tooling, the progress tracker, and the full design-decisions
  trail (`specs/`). Clone it only to prove croma at corpus scale or to track the
  project.

The dependency is one-way: **croma-test depends on croma**, never the reverse.
`croma-core` stays zero-dependency and crates.io-publishable.

## Start every session here

```sh
tools/session_bootstrap.sh              # git state + toolchain + build croma
tools/session_bootstrap.sh --with-suite # also clone/update ./croma-test/ (the suite)
```

Bootstrap reports git state, provisions the pinned Rust toolchain, and builds
`target/debug/croma`. With `--with-suite` it clones the private croma-test repo
into the git-ignored `./croma-test/` for corpus-scale gate runs.

## Environment

Two interchangeable environments, same pinned toolchain (details:
[`docs/development-environment.md`](docs/development-environment.md)):

- **Linux cloud sandbox** — provisioned with `rustup`. Ephemeral: commit and push
  anything worth keeping.
- **Local Nix flake** — `nix develop` / direnv, any OS.

Rust 1.96.0 is pinned by `rust-toolchain.toml`; plain `cargo`/`rustc` select it on
any host. Never hardcode an absolute toolchain path.

## Standing rules

- **Never work on `main`.** Branch per change by type: `feature/<slug>`,
  `bugfix/<slug>`, or `refactor/<slug>`.
- `croma-core` must stay crates.io-publishable and zero-dependency — no
  path-only/local runtime assumptions in library code. The MusicXML reader's only
  dependency (`roxmltree`) is opt-in via the `croma-core` `musicxml-reader` feature
  and ships on the CLI binary, never the library default. Build a reader-less CLI
  with `cargo build -p croma-cli --no-default-features`.
- The cloned `./croma-test/` subdir is git-ignored. Never commit it, or its
  generated artifacts, into croma.
- The four capabilities are **promoted (un-gated)** and ship by default:
  - **Formatter** (`croma fmt` / `--auto-fix`): canonical ABC pretty-printer,
    idempotent + lossless over the 10k corpus — [`docs/formatter.md`](docs/formatter.md).
  - **MusicXML→ABC reader** (`croma read` / `croma musicxml2abc`): inverts croma's
    own writer (self-loop 9935/9935) and reads foreign MusicXML (abc2xml/MuseScore/
    Finale/Sibelius) at 98.50% music21 parity — [`docs/musicxml-reader.md`](docs/musicxml-reader.md).
  - **LSP** (`croma-lsp`): a thin stdio adapter over the core/formatter —
    diagnostics + formatting byte-identical to the core, ~1 ms latency —
    [`docs/lsp.md`](docs/lsp.md).
  - **Editors**: the reusable `tree-sitter-abc` grammar + Zed extension —
    [`docs/editors.md`](docs/editors.md).
  Any LSP/reader-vs-core mismatch is a bug, not a new spec; re-prove the relevant
  legs (in croma-test) after any touch.

## Parser recovery policy

The parser is **strict to ABC 2.1**. When it meets malformed input it follows one
three-tier rule (loose source is the formatter's job, not the parser's):

1. **Default: reject.** Input that does not match the spec grammar is not silently
   accepted.
2. **Recover *and warn* — only for a clear intention spoiled by a minimal,
   mechanical slip** (a stray space/comma, a missing space). Recover the obvious
   intent and **always emit a diagnostic** — recovery is **never silent**. A silent
   recovery is indistinguishable from mimicking `abc2xml`; the warning is what makes
   recovery defensible as transparent strict-recognition.
3. **Otherwise: strict reject.** If the intention is not unambiguous, or the mistake
   is not a trivial slip, reject it. Repair belongs in `croma fmt --auto-fix`, which
   sanitises loose source into canonical spelling the strict parser then reads
   cleanly.

Corpus impact: warnings are stderr diagnostics, so adding one is **whitelist-neutral**
(the MusicXML is unchanged) and always safe to land. Converting a recovery into a
reject **changes the MusicXML**, so it can drop files out of the whitelist; that is
acceptable only as an **adjudicated** drop (croma is strict-correct, `abc2xml` is
lenient), never a silent regression. The comparator, the whitelist/dropped baseline,
the ABC spec KB, and the divergence-triage process all live in **croma-test**
(`comparison/abc2xml-divergences/`); triage each such file there, one at a time.

## Validate before committing

```sh
cargo test --workspace                              # Rust unit + integration tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo run -p croma-cli -- xml examples/basic.abc
```

Corpus-scale proofs (fmt 10k, reader 10k, LSP legs A–E, abc2xml whitelist 9390/0,
grammar coverage) run from **croma-test** — see its README. `cargo test` here skips
them cleanly when the corpus is absent (they are `ABC_ROOT`-gated).

## Landing

`uv run tools/land.py <branch>` is the standard push → PR → green-CI → squash-merge
→ cleanup flow. Open a pull request only when validation passes and the user asks
for one.

## Progress tracking & decisions trail

The progress tracker, the per-phase ledger, and the full design-decisions trail
(`specs/`) live in **croma-test** (`progress/`, `specs/`). Consult them there for
project history or the rationale behind a capability or promotion.
