# Phase-65 — Code-quality pass: decisions

**Date:** 2026-06-17
**Epic:** A — review + de-comment + simplify the session's freshest code.
**Nature:** Non-functional. No behavior change; no proven-gate regression. Additive.

This doc is the steering artifact: it locks scope, the comment policy, and how
findings are dispositioned, *before* any code is touched. Stages A1–A4 are
delegated to subagents (orchestrator holds this doc + tracker + lands PRs +
verifies gates).

---

## Decision 1 — Scope boundary (locked)

**Primary (review + edit):** the session's newest, least-reviewed,
subagent-written code.

- `crates/croma-lsp/src/` — all 13 files (5,137 LoC). The R1–R4 language server.
- `tree-sitter-abc/` — `grammar.js` (372), `src/scanner.c` (132),
  `queries/*.scm` (~120), `web/highlights.scm` (78).
- `editors/zed/src/lib.rs` (236) — the Zed extension.

**Light review only (NO edits unless a finding is a confirmed, isolated
correctness bug):** the most-recently-touched reader/writer code
(`read_musicxml`, recent `to_abc.rs` deltas). These feed the raw-whitelist and
reader gates; comment trims there are deferred to a later, dedicated pass — not
worth the re-export/re-prove cost in a comment epic.

**Excluded:** broad rewrites of mature, proven `croma-core` / `croma-fmt` logic.
Battle-tested across ~64 phases; high risk, low value. They are the *reference
style* for this pass, not its target.

**`corpus_proof.rs` (croma-lsp, croma-fmt):** these ARE the gate harnesses.
Comment trims only; logic untouched; re-run the proof after any touch.

### Why the LSP code is the target

Comment-line density (lines matching `^\s*(//|/\*|\*)` / total), measured
2026-06-17:

| in-scope (croma-lsp + zed) | density | mature `croma-core` reference | density |
|---|---|---|---|
| lib.rs 23/83 | 28% | model.rs 74/894 | 8% |
| position.rs 95/386 | 25% | lib.rs 14/184 | 8% |
| tokens.rs 130/519 | 25% | source.rs 0/368 | 0% |
| hover.rs 67/285 | 24% | diagnostic.rs 0/153 | 0% |
| completion.rs 82/364 | 23% | to_abc.rs 275/2279 | 12% |
| formatting.rs 26/125 | 21% | options.rs 0/68 | 0% |
| zed lib.rs 44/236 | 19% | | |
| main.rs 100/918 | 11% (high absolute) | | |

The fresh LSP code runs ~2–3× the comment density of mature core. Target: bring
strippable "what" narration down toward core's ~8–12% **without touching the
KEEP categories below**. (Raw counts include `///` doc comments we KEEP, so the
strippable fraction is smaller than the gap looks — match density, do not zero.)

---

## Decision 2 — Comment policy (the heart of the epic)

### KEEP (never strip)
- ABC 2.1 §spec citations and policy references (AGENTS.md, the gates,
  `docs/*.md`).
- Safety-invariant / "why" / non-obvious-gotcha comments (e.g. "widest-at-start
  so containers overlap correctly", LSP↔core fidelity invariants, ABI/encoding
  notes in `scanner.c`).
- Public-API contract docs (`///` on `pub` items): what a caller must know —
  params, panics, ordering guarantees.
- License headers.

### STRIP / TRIM
- "What" narration that restates the next line (`// increment i`, `// return the
  result`, `// loop over voices`).
- Comments that merely duplicate the function/variable name.
- Decorative banners / section-divider art with no information.
- Verbose doc-blocks on **trivial private** items (one-line private helpers whose
  body is self-evident).
- Stale / aspirational TODOs (confirmed done or never-going-to-happen).

### MATCH, don't zero
Match the surrounding module's density; mature `croma-core` modules are the
reference. A module that genuinely needs "why" comments keeps them. The diff
must read as **comments / whitespace / mechanical refactor only**.

---

## Decision 3 — Findings disposition

- **Confirmed correctness bug** → fix in-epic, TDD (failing test first), re-prove
  the affected gate. (A3)
- **Would change a proven gate's OUTPUT** → almost certainly *intended* (the
  gates are corpus-proven). Triage, do **not** "fix". A gate that moves during a
  cleanup is a bug in the cleanup → revert + report.
- **Risky / out-of-scope / larger refactor** → file as a separate task
  (`spawn_task` / backlog note), not scope creep into this epic.

---

## Gates (all stay green; measured per stage that touches the relevant code)

| Gate | Command | When |
|---|---|---|
| workspace tests | `cargo test --workspace` | every edit stage |
| clippy | `cargo clippy --workspace --all-targets -- -D warnings` | every edit stage |
| rustfmt | `cargo fmt --all --check` | every edit stage |
| LSP legs A–E | `croma-lsp` `corpus_proof` + `tools/prove_lsp_fidelity.py` / `tools/prove_lsp_totality.py` (ABC_ROOT set) | if `crates/croma-lsp` touched |
| fmt corpus proof | `croma-fmt` `corpus_proof` + `tools/prove_fmt_lossless.py` | if `crates/croma-fmt` touched |
| grammar | `tree-sitter test` (17/17) + `tools/prove_grammar_coverage.py` (≥99.46%) | if `tree-sitter-abc/` touched |
| raw whitelist 9390/0 | re-export + set-diff (HEAD vs fresh, LF) | only if writer/parser touched |
| reader self-loop 9935/9935 + foreign 98.50% | reader proofs | only if reader touched |

Corpus root for the heavy gates: `docs/untracked/corpus/zenodo-10k/{abc,musicxml}`
(set `ABC_ROOT` / `REF_ROOT` absolute).

Invariants that hold **by construction** when only LSP/grammar/zed comments are
touched: croma-core zero-dep (`cargo tree -p croma-core --edges normal` = 1
line), 4 workspace members, `tree-sitter-abc/` + `editors/zed/` excluded from the
cargo workspace. Re-checked anyway in the final stage.

---

## Stage plan (branch per stage; never main; land via `tools/land.py <branch> -y`)

- **A1 — Review (find, don't fix).** Subagent fan-out (cavecrew-reviewer +
  deeper correctness/comment passes) over the in-scope files. Output: the
  categorized findings list appended below. No edits.
- **A2 — De-comment + simplify.** Apply the policy + safe simplifications. Gate:
  provably non-functional (gates byte-identical; behavior-bearing edits tested).
- **A3 — Fix confirmed bugs (TDD).** Only real bugs from A1. Failing test first.
- **A4 — Close out.** Tracker + SQL snapshot + memory + this doc finalized.

---

## A1 findings

Source: 2× `cavecrew-reviewer` (correctness/dead-code over LSP, and grammar+zed)
+ 2× deep-review agents (over-comment census; simplification/reuse). `/code-review`
reviews the working-tree diff (empty — code is committed), so it was substituted
by the file-scoped census + simplification agents over the same ground.

### (a) Correctness — no runtime bugs found

The LSP position/offset math (UTF-16↔UTF-8, span→range) is correct and clamped
throughout; no escaping panics; scanner enum order correct (symbolic). Two
**comment-correctness** issues (fix as part of A2's comment pass, behavior
unaffected):
- `crates/croma-lsp/src/hover.rs:71` — comment says "half-open containment" but
  the fn (`span_contains_inclusive`) is inclusive; comment is misleading. **A2.**
- `tree-sitter-abc/src/scanner.c:15-21` — token-index comment is stale (omits
  `KEY_FIELD_KEY_TOK` at index 1); code correct via symbolic enums. **A2.**

Two zed flags require verification before A3 — likely **non-bugs, deferred**:
- `editors/zed/src/lib.rs:~150` — download path returns `Command{env: Vec::new()}`
  while the PATH path passes `worktree.shell_env()`. Agent speculated "fails if it
  needs ABC_ROOT/PATH". A stdio LSP server needs neither at runtime (reads doc
  text from the client; invoked by absolute path). **Verify in A3; expect
  no-bug / cosmetic-consistency-only → file, don't fix in-epic.**
- `editors/zed/src/lib.rs:~134` — `version_dir()` doesn't sanitize
  `release.version` (path-traversal). Version comes from the first-party GitHub
  release API; not attacker-controlled. **Defer → file as hardening task.**

### (b) Over-commenting hotspots — ~187 strippable lines (of 1,109 in scope)

The **non-test** LSP code is already at reference density — its module docs are
the spec-decision / totality-gate / widest-at-start invariants the policy KEEPS.
Strippable mass concentrates in:
- `main.rs` transport-test numbered step markers (`// 1.`/`// 2.`…) + restatement
  of the following `notify(...)` — ~22 lines (keep the L596-599 "deliberately
  broken state" why).
- Test-block "what" narration in `document.rs`, `position.rs`, `completion.rs`,
  `tokens.rs`, `structure.rs`, `diagnostics.rs`, `formatting.rs`, `code_action.rs`
  — ~55 lines (keep the few non-obvious clamp/closed-pair/container notes).
- `corpus_proof.rs` `// ----` banner dividers (×4) + array-restating lines — ~16
  (keep leg-property tags + summary-line-format contracts read by the provers).
- `.scm` section dividers in `queries/highlights.scm` **and its byte-identical
  twin** `web/highlights.scm` — ~22 (keep the L1-21 capture→legend header).
- `grammar.js` `// ---- … ----` section dividers (~6, lowest priority; keep all
  design-rationale blocks); `tables.rs` decoration section labels (~14, lowest
  priority; the `doc:` payload strings are hover content → KEEP).
- `editors/zed/src/lib.rs:32-35` `TODO(epic-C)` block — trim/drop (epic-C caveat
  already stated inline at L91-93).

KEEP-confirmed clean (flag count 0): `croma-lsp/lib.rs`, `scanner.c`,
`injections.scm`, `brackets.scm`, `folds.scm`.

**Cross-file gotcha for A2:** `web/highlights.scm` is byte-identical to
`queries/highlights.scm` — edit both identically or create a silent maintenance
divergence.

### (c) Simplification / reuse

**RISK:none — apply in A2:**
- `position.rs:51-57` & `document.rs:108-114` — two byte-identical
  `clamp_to_boundary` → hoist one to `position`, call from `document`.
- `completion.rs:139,155` — `line_is_music_body(source, …)` never uses `source`
  (`let _ = source;` discard) → drop the param; update call sites L79, L99.
- `completion.rs:130-178` — `prefix_is_information_field` /
  `is_information_field_line` / `is_field_line` share one body → collapse.
- `editors/zed/src/lib.rs:48` — `suffix` re-compares the already-matched `os` →
  fold `.exe` into the `os` match arm.
- (optional, low value) `completion.rs` `sort_text` helper; `tokens.rs:166`
  `Vec::with_capacity`.

**RISK:defer — file as separate tasks (out of this epic):**
- `Span::ordered`/`normalized` helper in `croma-core::diagnostic` to kill the
  4× open-coded span-ordering idiom — additive + zero-dep, but touches mature
  core; excluded per Decision 1.
- `grammar.js:362,364` merge `_line_text`/`_free_text_line` — changes parse-tree
  node types → moves the grammar gate. Out.
- `tokens.rs:146` "defensive" `sort_by_key` — invariant guard, removing it is a
  behavior risk. Keep.
- `corpus_proof.rs` test-oracle re-derivation of `measure`/clamp/order — by
  design (independent oracle); keep.

### (d) Dead code / naming — none actionable

`scanner.c` lifecycle fns are required ABI boilerplate (not dead);
`croma-lsp/lib.rs` re-exports are all consumed. The only true dead item is the
unused `source` param above (folded into (c)).

---

## A2 outcome — applied (branch `feature/quality-a2-cleanup`)

14 files, **+36 / −101** (net ~−52 comment lines, KEEP-leaning per policy).

**Comment trims (HIGH+MEDIUM applied; LOW mostly skipped to avoid vandalism):**
`main.rs` transport-test numbered step markers → prose / removed; test-block
"what" narration in `document.rs`/`completion.rs`/`code_action.rs`/
`diagnostics.rs`/`structure.rs`/`tokens.rs`; `corpus_proof.rs` four `// ----`
banner rules → plain leg labels + one array-restating line; the section dividers
in `queries/highlights.scm` **and its byte-identical twin** `web/highlights.scm`;
shrank the zed `TODO(epic-C)` to one line. **Skipped** (LOW / by policy):
`tables.rs` decoration labels, `grammar.js` dividers (regeneration risk), and the
optional micro-simplifications.

**Comment-correctness fixes:** `hover.rs` "half-open" → "inclusive of the closing
edge" (matches `span_contains_inclusive`); `scanner.c` token-index table
corrected to the real 7-`externals` order (`_key_field_key_tok` at index 1). Both
comment-only; no enum/code touched.

**Behavior-preserving simplifications (the only code edits, each verified):**
1. `clamp_to_boundary` deduped — `position::clamp_to_boundary` is now
   `pub(crate)`; `document.rs`'s byte-identical copy deleted and both call sites
   route to it (rationale preserved at the call site).
2. `completion::line_is_music_body` — dropped the unused `source` param + the
   `let _ = source;` no-op (and the now-needless trailing `continue`, which
   `-D warnings` would flag); two call sites updated. Control flow identical.
3. `completion::prefix_is_information_field` now delegates to
   `is_information_field_line(prefix.trim_start())` (the inlined body was equal).
4. `editors/zed` `asset_name` — folded the `.exe` suffix into the `Os` match arm.

**Gates — all byte-identical to the pre-A2 baseline (proof, not assertion):**

| Gate | Baseline | After A2 |
|---|---|---|
| LSP `corpus_proof` (legs A/B/D + totality + E) | 3 pass / 0 fail | **3 pass / 0 fail** — tokens 10000/0, totality 10000 files/0 panics, latency under ceiling |
| grammar coverage (10k) | 9946 / 99.46% / 54 ERROR | **9946 / 99.46% / 54 ERROR** |
| `tree-sitter test` | 17/17 | **17/17** |
| `cargo test --workspace` | green | **834 / 0** |
| `clippy --workspace --all-targets -D warnings` | clean | **clean** |
| `cargo fmt --all --check` | clean | **clean** |
| `editors/zed` `cargo test` | — | **8/8** |

Untouched → green by construction (no source change): `croma-fmt` corpus proof,
raw whitelist 9390/0, MusicXML→ABC reader self-loop/foreign parity, croma-core
zero-dep, 4 workspace members, `tree-sitter-abc`/`editors/zed` workspace
exclusion.

## A3 — no in-epic fixes

A1 surfaced no confirmed runtime bug. The two comment-correctness issues were
folded into A2. The two `editors/zed` flags are **inactive future code**
(`download_from_release` only fires once epic-C publishes release binaries;
`latest_github_release` errors today and the resolver falls through to the PATH
path / actionable error) and are filed as separate hardening tasks, not fixed
in-epic:
- download-path `Command{env: Vec::new()}` vs the PATH path's
  `worktree.shell_env()` — cosmetic for a stdio LSP invoked by absolute path;
  revisit when release binaries exist.
- `version_dir()` doesn't sanitize `release.version` (path traversal) — version
  is first-party (GitHub release tag); low risk; harden alongside epic-C.

### Hardening done (branch `bugfix/zed-download-hardening`)

Both deferred flags are now closed (the download path is still inactive, so this
is pre-emptive — and the version sanitizer is pure, so it is tested on the host
today regardless of epic-C):

- **env parity.** `download_from_release` now takes `&Worktree` and returns
  `Command { env: worktree.shell_env(), .. }` instead of an empty env, matching
  the PATH-resolution branch. Decision was *inherit* (not zero both): the PATH
  branch is the active, working-today path, so consistency means the download
  branch matches it. Harmless today (the stdio server reads no runtime env —
  `ABC_ROOT` is read only by the `corpus_proof` harness, never the runtime
  server), but it removes a latent divergence.
- **version sanitize.** New pure, host-testable `sanitize_version(&str) ->
  Option<&str>` (sibling of `asset_name`) rejects empty, any `..` parent-dir
  segment, and any byte outside `[A-Za-z0-9._-]` (notably `/` and `\`) before the
  version reaches `version_dir`/`bin_path`; a rejected tag becomes an actionable
  error instead of an escaped cache path. 6 host unit tests added (14 total).
- `TODO(epic-C)` in `asset_name` left as-is: it tracks asset-name reconciliation
  with the release workflow, a separate epic-C concern.
- Verified: host `cargo test` 14/14; `wasm32-wasip1` build + clippy clean; fmt
  clean. (wasm toolchain gotcha: the shell's `cargo`/`rustc` resolve to the
  `stable` dir, whose sysroot lacks the wasm std — build/clippy the wasm target
  with the `1.96.0-*` toolchain bin dir prepended to `PATH`.)

## A4 — closeout

Tracker: `phase-65-quality-a1` merged (#160), `-a2` complete, `-a3`
complete (no-fix). Comment-policy decision recorded to auto-memory. Session ends
on main.
