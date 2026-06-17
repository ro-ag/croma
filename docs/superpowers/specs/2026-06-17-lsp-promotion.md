# Decisions — croma LSP: transport, gating, promotion bar (DECIDE-FIRST)

**Date:** 2026-06-17
**Status:** decisions (document-before-code for the LSP promotion epic)
**Epic:** phase-63 `lsp-promote` — build the language server to a promotable,
evidence-backed product and un-gate it, mirroring the formatter
([`2026-06-15-fmt-proven-feature-design.md`](2026-06-15-fmt-proven-feature-design.md))
and reader ([`2026-06-16-musicxml-reader-promotion.md`](2026-06-16-musicxml-reader-promotion.md))
promotions.

The LSP is the **one remaining gated capability** (AGENTS.md: "LSP remains gated
until it has comparable evidence"). The other three core capabilities are
promoted/corpus-proven: forward writer (raw whitelist 9390/0), `croma fmt` +
`--auto-fix` (10000/0 idempotent + lossless), MusicXML→ABC reader (self-loop
9935/9935, foreign parity 98.50%).

Current state = **empty skeleton**: `crates/croma-lsp/` is a workspace member but
**excluded from `default-members`** (that is the gate). The whole crate is
`Cargo.toml` (single dep `croma-core`) + `src/lib.rs` (~33 lines): one
`analyze_document(source) -> DocumentAnalysis` wrapping `export_musicxml()`. No
transport, no JSON-RPC, no handlers, no document store, no tests. So this epic is
**CREATE**, not finish.

This document settles the four DECIDE-FIRST questions before any code.

---

## The architectural spine (the decision under all four)

**Split the crate into a pure, transport-free analysis layer + a thin transport
shell.** The `croma-lsp` *library* exposes pure, synchronous, panic-free
functions that map source text to LSP payloads:

```
diagnostics(text)        -> Vec<lsp_types::Diagnostic>
formatting(text)         -> Vec<lsp_types::TextEdit>          (one full-doc edit)
semantic_tokens(text)    -> lsp_types::SemanticTokens
document_symbols(text)   -> Vec<lsp_types::DocumentSymbol>
folding_ranges(text)     -> Vec<lsp_types::FoldingRange>
hover(text, pos)         -> Option<lsp_types::Hover>
completion(text, pos)    -> Vec<lsp_types::CompletionItem>
code_actions(text, range)-> Vec<lsp_types::CodeAction>        (wraps auto_fix)
```

The `croma-lsp` *binary* (`[[bin]] croma-lsp`) is a thin `lsp-server` main loop:
own the document store, dispatch each request/notification to the matching pure
function, publish the result. **No business logic in the transport.**

Why this matters: every promotion-bar leg (A diagnostics fidelity, B formatting
identity, C totality, D semantic tokens, E latency) is proven by driving the
**pure functions in-process over the 10k corpus**, exactly as the formatter's
[`corpus_proof.rs`](../../../crates/croma-fmt/src/corpus_proof.rs) drives
`format`/`auto_fix` in-process — seconds, not 10k subprocess spawns, and no
transport/runtime in the test path. The transport is then a small, separately
smoke-tested adapter.

---

## Decision 1 — Transport + framework: `lsp-server` + `lsp-types` (sync)

**Pick: `lsp-server` + `lsp-types`** (the rust-analyzer crates), not `tower-lsp`,
not hand-rolled.

Rationale:
- Our handlers are **synchronous and fast** — `parse`/`format` are in-process,
  in-memory, millisecond-scale CPU work over a single document. There is no I/O
  to overlap, no concurrency to exploit. `tower-lsp` would impose a **tokio
  runtime + async-trait + tower** dependency tree to wrap synchronous work in
  `async` for **zero** benefit.
- `lsp-server` is the **lightest** option that "covers stdio + incremental sync
  cleanly" (the prompt's stated preference): it handles the Content-Length
  framing and JSON-RPC plumbing over stdio, hands us typed `Request`/
  `Notification`, and leaves the dispatch loop to us. Dep tree is small
  (`crossbeam-channel`, `serde`, `serde_json`, `log`); `lsp-types` adds the typed
  protocol structs (`serde`, `url`).
- **Explicit loop ⇒ provable totality.** Owning the dispatch loop (vs. a
  framework trait) makes the totality gate (C) tractable: we control every code
  path, wrap each handler so a malformed request can never panic the loop, and
  there is no hidden async executor to hang.
- **Testability.** Because the analysis layer is transport-free and the loop is
  synchronous, the corpus gate harness is a plain `for file in ABC_ROOT` loop
  calling the pure functions (+ `catch_unwind` for C) — no runtime, no client
  process, mirroring `corpus_proof.rs`.
- Hand-rolled JSON-RPC is rejected: re-implementing framing + base-protocol
  edge cases is a second spec to rot for no gain.

These deps land on the **`croma-lsp` crate/binary ONLY**. `croma-core` and
`croma-fmt` gain nothing.

## Decision 2 — Gating model + un-gate mechanism

**During the build (R1–R3): stay gated** = `croma-lsp` stays **out of
`default-members`**. `cargo build` / `cargo build --release` (the shipped build)
do not build the server. We exercise it explicitly with `-p croma-lsp` (and CI
already lints/tests it via `cargo clippy --workspace --all-targets` /
`cargo test --workspace`, which include every workspace member regardless of
`default-members`). This is the exact analog of how the reader stayed gated
behind an opt-in feature until its evidence was in.

**Un-gate (R4): add `crates/croma-lsp` to `default-members`.** Then `cargo build`
builds the `croma-lsp` binary by default — the server ships. This mirrors the
reader's R4 (flip the optional surface into the default build) and the formatter's
promotion (un-gate once corpus-proven). The flip is the explicit, documented,
justified change the hard constraint requires.

**Zero-dep contract preserved:** the flip adds `croma-lsp` (and its `lsp-server`/
`lsp-types` deps) to the *default workspace build set*, **never to `croma-core`'s
dependency graph**. `croma-core`'s default build stays zero-dep +
crates.io-publishable. The LSP binary depending on `lsp-types` is exactly like
the CLI binary depending on `roxmltree`: a *binary* dep, not a *library* dep.

## Decision 3 — The promotion bar legs (locked) + which stage proves each

The five legs from the prompt, locked, each mapped to the stage that proves it.
**All five green to un-gate.**

| Leg | Bar | Proven in |
|---|---|---|
| **A. Diagnostics corpus fidelity** | LSP-path diagnostics **== core diagnostics** over the 10k: same set of (code, severity, byte-span) — and every emitted LSP `Range` is in-bounds and round-trips back to the core byte-span. **10000/10000.** | R2 |
| **B. Formatting identity** | LSP `textDocument/formatting` applied to the doc **== `croma_fmt::format(src)` byte-for-byte** over the 10k. **10000/10000.** | R2 |
| **C. Totality** | **0 panics / 0 hangs** over the 10k via a test client (didOpen + a scripted didChange sequence incl. malformed mid-edit truncations), each analysis wrapped in `catch_unwind`. **No `unwrap`/`expect`/`panic!`/indexing-that-panics/`debug_assert!` in LSP `src/`** (tests may use `expect`). | R1 |
| **D. Semantic-token correctness** | Over a fixture set (and corpus-sampled), the token spans are **exhaustive** (cover every non-whitespace source byte the parser tokenized), **non-overlapping**, and **in-bounds**; the LSP delta-encoding is monotonic. | R2 |
| **E. Latency** | diagnostics + semantic tokens **< ~50 ms** on an average ~200-line file (CI machine), measured and reported. | R3 |

Harness: an `ABC_ROOT`-gated, in-process test module in `croma-lsp`
(`corpus_proof`-style) proves A/B/C/D over the corpus; a `tools/` black-box
wrapper reports human-readable pass/fail counts (mirroring
`tools/prove_fmt_lossless.py`). C additionally drives a real client transcript.

## Decision 4 — `croma-core` stays zero-dep + publishable (confirmed)

Confirmed and enforced:
- `croma-lsp` deps = `croma-core` (path) + `croma-fmt` (path, **added in R2** for
  formatting/code-action) + `lsp-server` + `lsp-types`. Nothing flows into
  `croma-core`.
- The existing CI guard `! cargo tree -p croma-core --edges normal | grep -q
  roxmltree` stays; R4 additionally asserts `croma-core`'s normal dep tree is a
  **single line** (the crate itself) so any future leak fails CI.
- `croma-fmt` is itself zero-external-dep (path dep on `croma-core` only), so
  adding it to `croma-lsp` introduces no third-party crate beyond the LSP ones.

---

## Cross-cutting correctness decisions (so the gates are clean)

1. **Position encoding.** LSP `Position` is 0-based line + 0-based *character*,
   where the character unit is the negotiated `PositionEncoding` (default
   UTF-16). `SourceText::line_column(byte)` returns **1-based line + 1-based
   Unicode-scalar column**, so it is *not* directly an LSP Position. The LSP layer
   owns conversion:
   - **Negotiate UTF-8** (`InitializeParams.capabilities.general.
     positionEncodings`) when the client offers it (LSP 3.17): then character =
     `byte − line_start_byte`, an exact match to our byte spans — trivial and
     lossless.
   - **Fall back to UTF-16** otherwise: character = sum of `char::len_utf16` over
     the line prefix `[line_start, byte)`. A small, unit-tested helper; for ASCII
     ABC (the vast majority) it equals the byte delta.
   This is a totality/fidelity dependency: every emitted `Range` must be
   in-bounds and reversible to the originating byte span (leg A checks it).

2. **Incremental sync.** Advertise `TextDocumentSyncKind::INCREMENTAL`. The
   document store keeps a `String` per URI. Each `didChange`
   `TextDocumentContentChangeEvent` with a `range` is applied by converting the
   range to byte offsets against the *current* text (clamped — never panic) and
   splicing; a change with no `range` replaces the whole text (full-sync
   fallback). `SourceText` is rebuilt per analysis from the current string. The
   parser is panic-free on malformed/mid-edit input (recovery `Malformed`
   nodes), so every intermediate keystroke state is safe to analyze.

3. **codeAction = whole-document replace.** `auto_fix` *formats first*, so its
   `Change` spans refer to the **formatted** source, not the client's buffer —
   mapping them back is fragile. Instead the "fix all" code action emits a single
   `TextEdit` replacing the whole document with `auto_fix(src).output` (a
   `source.fixAll` kind), and `textDocument/formatting` likewise emits one
   full-document edit with `format(src)`. Simple, exact, and matches the proven
   core byte-for-byte (leg B). Per-fix granular actions are out of scope for
   promotion (can be layered later if a client needs them).

4. **Hover/completion are static tables.** Hover = ABC field-key docs (`X: T: M:
   L: K: Q: V: …`) + decoration docs, from a static table in the LSP crate
   (sourced from ABC 2.1 §3/§4). Completion = header field keys + decoration
   names. No new spec, no core change — pure presentation over `croma-core`'s
   existing taxonomy.

5. **The LSP never diverges from the core.** It adapts `croma-core`/`croma-fmt`
   output; it never reparses or reformats independently. Any LSP-vs-core mismatch
   is a bug in the adapter, not a new spec (legs A/B make this measurable).

---

## Staging (each stage: TDD, gated, landed via `land.py`, terse child receipt)

| Stage | Scope | Gate |
|---|---|---|
| **R1** `feature/lsp-skeleton-diagnostics` | `[[bin]]` + `lsp-server` transport; `initialize`/`initialized`/`shutdown` w/ capability negotiation (incl. position-encoding); document store; `didOpen`/`didChange` incremental sync; `publishDiagnostics` (Span→Range). Position-mapping helper + unit tests. | **C (totality):** in-process corpus harness + scripted-client transcript, 0 panics / 0 hangs over 10k incl. malformed mid-edit; no `unwrap`/`expect`/`panic` in `src/`. |
| **R2** `feature/lsp-formatting-tokens` | Add `croma-fmt` dep. `textDocument/formatting` (`format`); `semanticTokens/full` (walk `MusicTokenKind` spans); `documentSymbol` + `foldingRange` (tune/field spans). A/B/D corpus harness + `tools/` wrapper. | **A** diagnostics fidelity 10000/10000; **B** formatting identity 10000/10000; **D** semantic-token spans exhaustive + non-overlapping + in-bounds. |
| **R3** `feature/lsp-hover-completion` | Hover (field/decoration static table); completion (header keys + decoration names); codeAction (`auto_fix` → `source.fixAll` whole-doc edit). Latency probe. | Fixture-driven correctness; **E** latency < ~50 ms (diagnostics + semantic tokens) on an avg ~200-line file. |
| **R4** `feature/lsp-promote` | Add `crates/croma-lsp` to `default-members`; `docs/lsp.md` (coverage/policy, mirroring `docs/formatter.md`/`docs/musicxml-reader.md`); AGENTS.md gate language; tracker; strengthen the dep-free guard; (optional) minimal editor-client smoke note. | Legs A–E green + measured; `croma-core` still zero-dep; no regression to forward 9390/0, fmt 10000/0, reader 9935/9935 + 98.50%. |

**Hard constraints (restated):** `croma-core` default build stays zero-dep +
publishable (LSP deps on the LSP crate/binary only); additive-only — no
regression to any proven gate; the LSP adapts the proven core, never reimplements
parsing/formatting/diagnostics (any mismatch is a bug); no panics; no AI
co-author trailer; CI clippy is `-D warnings` over `--workspace --all-targets`
(use `.expect()` not `.unwrap()` in tests).

**Discipline:** the orchestrator holds this doc + the tracker + lands PRs +
verifies gates; **all** LSP/test code is written by per-stage subagents returning
terse receipts. Branch per stage (`feature/lsp-*`, never `main`). Session ends on
`main`.
