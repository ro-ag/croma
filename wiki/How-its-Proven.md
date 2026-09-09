# How it's proven

croma's correctness is validated against a **real-world corpus of 10,000 ABC
files** (the [Zenodo ABC dataset](https://doi.org/10.5281/zenodo.17694747)), not
just hand-written unit tests (though there are ~800 of those too). Every shipped
capability has a corpus-scale gate that must stay green.

## The gate matrix

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
`cargo fmt --check` on every change.

**Every gate's residual is _adjudicated_** — a documented, spec-justified
non-bug — never an unexplained failure. That is the core discipline: a change
may move a number only when the new behaviour is more spec-correct and the moved
files are adjudicated via the divergence-triage process; otherwise the gate
holds. See [[Contributing]] and
[[abc2xml-Comparison]] for what "adjudicated" means in practice.

For the **methodology** behind these gates — the two-independent-proofs
principle, the invariant safety gates, the totality test, and how efficiency is
measured — see [[Testing-Methodology]].

## The repo split — you don't need the proving suite to build croma

croma is deliberately split into two repositories so a developer never has to
download the heavy proving apparatus:

| Repo | Contents | When you need it |
| --- | --- | --- |
| **[`ro-ag/croma`](https://github.com/ro-ag/croma)** (this one) | the toolkit: `croma-core`, `croma-cli`, `croma-fmt`, `croma-lsp`, the `tree-sitter-abc` grammar, the Zed extension, and the per-capability `docs/` | always — it **builds and tests standalone** |
| **`ro-ag/croma-test`** (private) | the corpus-scale proving suite: the Python provers, the 10k ABC corpus, the abc2xml comparator + whitelist/dropped baseline, the ABC 2.1 spec knowledge base, the divergence-triage tooling, the progress tracker, and the design-decisions trail | only to **prove croma at corpus scale** or track project history |

The dependency is one-way: **croma-test depends on croma, never the reverse.**

Because the corpus-scale gates are **environment-gated** (`ABC_ROOT`), a normal
`cargo test --workspace` in this repo **skips them cleanly** when the corpus is
absent. The full matrix runs from croma-test; on every pull request croma's
`gates` CI clones croma-test and runs that matrix against the change, while the
cargo-only `ci` workflow runs in parallel.

```sh
# Standalone, no corpus needed — these are what a contributor runs:
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo run -p croma-cli -- xml examples/basic.abc
```

For the throughput/latency side of the proof (a separate, reproducible
performance baseline), see [[Benchmarks]].
