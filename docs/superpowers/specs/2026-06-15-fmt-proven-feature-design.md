# Promote `croma fmt` + `--auto-fix` from gated to a proven feature

Date: 2026-06-15
Status: approved (design)
Branches: `feature/fmt-corpus-proof`, `feature/fmt-tier2-curations`,
`feature/fmt-midi-whitespace`, `docs/fmt-ungate`

## Context

croma is an ABC 2.1 → MusicXML parser/exporter. The parser/corpus work is
complete: the RAW comparator partitions the 10k corpus into **whitelist 9,390 /
dropped 545 / worklist 0** — every file is a structural match or an adjudicated
drop. The three-tier parser recovery policy is codified (AGENTS.md "Parser
recovery policy"): the parser is strict; it recovers + always-warns only for a
clear intention spoiled by a minimal mechanical slip; otherwise it strict-rejects
and **defers the repair to `croma fmt --auto-fix`**.

AGENTS.md gates the formatter "until parser quality is proven." That precondition
is now met. This work promotes the formatter to a proven, un-gated feature.

The formatter (`crates/croma-fmt/`, ~1,105 lines): `engine.rs` (canonical
`format`), `fixes.rs` (`--auto-fix` curations), `lib.rs` (public API +
`FixKind`/`Gate`), `verify.rs` (runtime safety gates). CLI:
`croma fmt [--check] [--write] [--auto-fix]`. The `--auto-fix` catalogue is 6
curations (`DetachedLength`, `ChordSymbolInBrackets`, `DoubledTempo`,
`BareTempoSuffix`, `RedundantBarline`, `FieldSpacing`), each runtime-verified
before it is kept. Two gates: `Gate::Pitch` (ordered pitch sequence unchanged —
for fixes that legitimately change the render) and `Gate::Structure` (rendered
MusicXML byte-identical — for cosmetic fixes).

## Strategic decisions (locked with the user, 2026-06-15)

1. **Formatter purpose: Hybrid Option U + one-shot demo.** The formatter stays a
   standalone user tool. The corpus comparison stays RAW; the RAW whitelist
   remains the **sole** parser regression baseline. We do **not** add a permanent
   fmt-first comparison axis. Rationale established by investigation: the
   strict-reject "family" is a single already-paired fix (`BareTempoSuffix`), and
   a permanent fmt-first axis would recover **exactly two** corpus files
   (`tune_001192` `Q:320s`, `tune_009608` `Q:400.`) while reopening whitelist
   bookkeeping. The corpus-scale idempotence + losslessness proof (Part 1)
   delivers essentially all of Option P's *validation* value; a one-shot demo
   (Part 2) delivers its *recovery* value, both without permanent machinery.

2. **Un-gate the formatter now; keep LSP gated.** Parser quality is proven
   (9,390/0). LSP remains the sibling gated item and is out of scope.

3. **Catalogue expansion: tier-2 loose-source curations + %%MIDI.** Reject-pairing
   (source a) is already exhausted (`BareTempoSuffix`). We add two spec-grounded
   tier-2 curations (source b) and one narrow %%MIDI curation (source c), each
   gated and TDD'd, kept only if it clears its gate corpus-wide.

## Hard constraints

- **Lossless.** A fix/format must NEVER change the music. Every curation stays
  gated; prefer `Gate::Structure` unless a fix legitimately restores a dropped
  render aspect (then `Gate::Pitch`). A fix that cannot clear a gate is wrong and
  is reverted + reported as skipped.
- **Idempotent.** `fmt` is a fixed point: `fmt(fmt(x)) == fmt(x)` for all inputs.
- **Spec-grounded canonical forms.** Cite ABC 2.1; do not encode abc2xml-isms.
  Exception, explicitly flagged: %%MIDI is an abc2midi convention, not ABC 2.1
  (see Part 4).
- **Raw whitelist is the regression baseline.** After any parser/exporter touch,
  re-export + re-compare + whitelist set-diff = **0 regressions**.

## Part 1 — Corpus-scale idempotence + losslessness proof

The un-gate proof. An **env-gated, in-process** test module in croma-fmt: reads
`ABC_ROOT`, walks every `*.abc`, and is skipped in a normal `cargo test` run
(the corpus is external). It runs in seconds in-process, reusing the private
`verify` helpers (`format`, `auto_fix`, `pitch_seq_of`, `musicxml_of`).

Per file, four assertions:

- `format(format(x)) == format(x)` — plain `fmt` is idempotent.
- `musicxml_of(x) == musicxml_of(format(x))` — plain `fmt` is lossless
  (whitespace-only; byte-identical render). Files that do not lower (hard errors)
  yield `None == None` and are skipped.
- `pitch_seq_of(x) == pitch_seq_of(auto_fix(x).output)` — `--auto-fix` preserves
  the ordered pitch sequence (the lossless promise).
- `format(auto_fix(x).output) == auto_fix(x).output` — `--auto-fix` output is a
  fmt fixed point.

A thin `tools/fmt_corpus_proof.py` wrapper drives the test and reports a
pass/fail count for humans. Any file that fails any assertion is a real defect:
fix `engine.rs`/`fixes.rs` and re-run (Part 2). Expectation is zero failures
(per-fix gates + the engine's never-drop-content fallback), but surfacing an
exception is the entire point of running it at corpus scale.

Branch: `feature/fmt-corpus-proof`. The design spec lands here too.

## Part 2 — One-shot fmt-first recovery demo

A CI-safe Rust test (no external corpus dependency) mirroring the two corpus
rejects inline: assert `auto_fix("…Q:320s…")` → `Q:320` and `auto_fix("…Q:400.…")`
→ `Q:400`, and that `croma xml` of the fixed source contains `<per-minute>` (the
metronome) rather than `<words>320s</words>`. A short note in the formatter doc
points at the real `tune_001192` / `tune_009608`, whose `dropped.csv`
justifications already name `croma fmt --auto-fix` as the recovery path. This
captures Option P's reject → repair → recover spirit with zero permanent
bookkeeping.

Branch: `feature/fmt-corpus-proof` (with Part 1).

## Part 3 — Tier-2 loose-source curations

Classes the strict parser warns on but does not repair. Two additions, each a new
`FixKind` + a detector in `fixes.rs` hooking the parser's existing diagnostic
code + the right `Gate` + a failing-first unit test. Each is **kept only if its
gate clears corpus-wide** (Part 1 re-run); otherwise it is dropped.

- **`RedundantTie`** — hooks `abc.music.redundant_tie`. Drops a doubled/stray tie
  (`--`, or a leading `-` in `[-CE]`) that ties nothing. Gate `Structure`: an
  inert tie removed must render identically. ABC 2.1 §4.11 (ties join two notes
  of the same pitch; a doubled or partnerless tie is malformed).
- **`UnterminatedTempoQuote`** — hooks `abc.field.tempo.unterminated_quote`.
  Closes an unterminated tempo string, `Q:"Allegro` → `Q:"Allegro"`. Gate
  `Structure`: the tempo-text rendering must be unchanged. ABC 2.1 §3.1.8 (`Q:`
  field accepts a quoted string).

Branch: `feature/fmt-tier2-curations`.

## Part 4 — %%MIDI directive whitespace canonicalization

Corpus investigation (`docs/untracked/phase-33/midi/inventory.md`) constrains
this hard:

- Active `%%MIDI` directives are **already** canonical — always column-0,
  uppercase `%%MIDI`. There is no misplaced-active population to fix.
- The large inert population — 282× `K:C %%MIDI gchordon` tails, commented
  `% %%MIDI …` forms — is **forbidden to touch**. Promoting an inert mid-line
  occurrence to a column-0 active directive changes abc2midi playback, and croma
  renders no MusicXML for %%MIDI, so neither existing gate protects such an edit.
- Per-voice scoping (205 `program` directives sit under `V:` lines) makes any
  relocation or reorder semantically load-bearing → forbidden.
- %%MIDI is an **abc2midi convention, not ABC 2.1** — there is no spec citation
  for a canonical form; this brushes the "cite ABC 2.1 / no third-party quirks"
  constraint and is documented as such.

The only provably-lossless surface is **`MidiDirectiveSpacing`**: within an active
(column-0) `%%MIDI` line, collapse internal whitespace runs in the **argument
region** — everything between the `%%MIDI` token and the first trailing `%`
comment, which is preserved verbatim. Example:
`%%MIDI beat 93 83  73 4` → `%%MIDI beat 93 83 73 4`.

New gate **`Gate::DirectiveTokens`**: keep the edit only if the argument region's
`split_whitespace()` token vector is identical before and after **and** the
trailing comment is byte-identical **and** the line is an active column-0
`%%MIDI`. This never relocates, reorders, or touches inert/commented forms. The
edit cannot be validated by MusicXML equality (not rendered), so this textual
invariant is its dedicated gate.

Value is small (a handful of files with multi-space argument runs). Documented
explicitly as an abc2midi-convention curation, not ABC 2.1. The user explicitly
chose to ship this rather than re-defer (2026-06-15); promotion/relocation remain
off the table as not lossless.

Branch: `feature/fmt-midi-whitespace`.

## Part 5 — Un-gate + documentation

- **AGENTS.md**: rewrite the "Formatter and LSP are gated" standing rule to gate
  **LSP only**, noting the formatter is promoted (parser quality proven,
  9,390/0).
- **`docs/formatter.md`** (new): the formatter's invariants (idempotent,
  lossless), the three gates (`Pitch`, `Structure`, `DirectiveTokens`), the full
  `--auto-fix` catalogue, the corpus proof, the fmt-first demo, and the %%MIDI
  scope/limits.
- **Memory**: update `formatter_lsp_gate` (formatter un-gated; LSP still gated)
  and `fmt-autofix-catalogue-scope` (catalogue extended with the tier-2 fixes +
  `MidiDirectiveSpacing`; %%MIDI promotion/relocation remains out of scope).
- **Tracker**: add a phase row for the formatter promotion, export the SQL
  snapshot.

Branch: `docs/fmt-ungate` (last).

## Landing order & discipline

Proof → tier-2 → %%MIDI → docs. Each branch: failing test first (TDD);
`cargo test --workspace` + `cargo clippy --workspace --all-targets -- -D warnings`
+ `cargo fmt --check` clean; re-run the corpus proof; **0 raw-whitelist
regressions** after any parser/exporter touch; land via
`uv run tools/land.py <branch> -y`; no AI co-author trailer. Session ends with
only `main`.

## Out of scope

- LSP (remains gated).
- A permanent fmt-first comparison axis / re-baselined whitelist (Option P).
- %%MIDI promotion, relocation, reordering, or score-translation (Mission B is a
  separate parser/exporter epic, still deferred).
- Score→ABC writer changes.

## Success criteria

- A corpus-wide idempotence + losslessness proof for `croma fmt`, with any
  lossy/non-idempotent file fixed (Part 1/2).
- An expanded, spec-cited, gated `--auto-fix` catalogue: the tier-2 pairings that
  clear their gates + `MidiDirectiveSpacing` (Part 3/4).
- The formatter un-gated in AGENTS.md, documented in `docs/formatter.md`, memory
  + tracker updated (Part 5).
- Each item landed via `land.py`, green CI, 0 raw-whitelist regressions, no AI
  co-author trailer.
