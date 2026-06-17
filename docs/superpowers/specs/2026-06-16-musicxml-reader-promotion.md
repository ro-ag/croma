# Decisions — MusicXML reader: reference-dialect coverage, CLI surface, promotion

**Date:** 2026-06-16
**Status:** decisions (document-before-code for the promotion epic)
**Epic:** phase-60 `mxml-read-promote` — earn the reader real-world evidence + a
product surface + un-gate it. Mirrors the formatter's promotion
(corpus-proven → un-gated).
**Predecessor:** [`2026-06-15-musicxml-reader-design.md`](2026-06-15-musicxml-reader-design.md)
(the S1–S6e staging that built `read_musicxml`).

The reader today: `read_musicxml(xml) -> ParseReport<Score>`, feature-gated
`musicxml-reader = ["dep:roxmltree"]`, proven ONLY as the inverse of croma's own
writer (self-loop XML re-emission idempotence **9915/9935 = 99.80%**, totality
0-panic over own 10k + abc2xml-ref 10k + malformed). Two gaps block promotion:
no consumer (library fn unreachable from the CLI), and no foreign-dialect
evidence (the self-loop never exercises abc2xml/MuseScore/Finale/Sibelius XML).

This document settles the four DECIDE-FIRST questions before any code.

---

## Decision 1 — Promotion bar (what un-gates the reader)

Mirror the **formatter** precedent. The formatter was un-gated when parser
quality was proven (raw whitelist 9,390 / 0) **and** the formatter was idempotent
+ lossless over the full 10k (0 violations) — a self-loop proof PLUS a
corpus-wide adjudicated-to-zero worklist. The reader's analog has four legs;
**all four must be green** to un-gate:

1. **Self-loop idempotence (lossless-over-corpus analog).**
   `write(read(write(s))) == write(s)` ≥ **9915/9935** strict, with **every**
   residual either fixed or adjudicated to a documented, principled stop (R3).
   No silent non-idempotent file: each is a named, explained residual.
   **R3 outcome: 9,934/9,935** — the 19-file cross-channel ordering residual was
   FIXED (read-side: a placement-less `<direction><words>` is croma's *demoted*
   chord symbol, now read back into `chord_symbols` so it re-emits in order;
   0 self-loop regressions, R2 reverse parity unchanged at 98.50%). The **1
   remaining** file (nested 21:16 tuplet, `tune_003732`) is adjudicated: the
   writer's reduced `441/256` ratio has no clean inverse to the `7:8`/`3:2`
   nesting — a documented principled stop.

2. **Totality (already met).** 0 panics over croma's own 10k exports +
   abc2xml-reference 10k + hand-crafted malformed inputs (`catch_unwind` fuzz).
   This leg is **DONE** (S6e); R2/R3 must not regress it.

3. **Reference-dialect semantic parity (the NEW real-world evidence, R2).**
   Read FOREIGN abc2xml XML → `read_musicxml` → Score → re-export croma XML →
   **music21 semantic compare vs the original abc2xml XML**. The bar mirrors the
   **forward** raw-comparator methodology exactly
   ([[raw-comparator-triage-methodology]]):
   - a **whitelist** (raw semantic matches) that is the regression baseline,
   - a **dropped.csv** of adjudicated non-reader-bugs (croma-writer-can't-express
     / abc2xml-ism — NEVER mimicked),
   - a **worklist drained to 0** — every divergent file triaged one at a time
     (divergence-triage + abc-divergence-investigator) to a verdict.
   **Floor:** raw semantic match rate ≥ the forward direction's proven floor
   (forward raw whitelist = 9,390/10,000 ≈ **93.9%**). The reverse direction
   re-uses croma's same (lossy) writer, so it cannot exceed the forward parity
   structurally; reaching a comparable rate with the remainder **adjudicated**
   (not raw-percentage-chased) is the bar. A raw rate materially *below* the
   forward floor with un-triaged files is a fail.
   **R2 outcome: 98.50% (9,850/10,000), above the floor.** Reached by three clean
   read-side fixes (DTD-tolerant parse 0→77.1%, textless functional `<harmony>`
   synthesis 77.1→98.27%, decimal `<alter>` 98.27→98.50%); residual categories all
   adjudicated to verdicts (comparator/music21 artifacts, croma writer-default
   tempo, one outlier file, complex-tuplet gap shared with the self-loop residual)
   — none a clean reader bug. Foreign-engraver stretch: 40/40 music21 `.mxl` read
   with 0 panics. See `docs/musicxml-reader.md` "Reference-dialect reading".

4. **Second round-trip parity (R1).** The reader has a real consumer and the
   ABC path round-trips: `XML → Score → write_abc → ABC` is structurally faithful.
   Proven by `tools/prove_reader_abc_roundtrip.py` (the reader in the loop of the
   existing `prove_abc_roundtrip.py` structural projection): `croma xml FILE` →
   X1, `croma read X1 --format abc` → ABC', `croma xml ABC'` → X2, compare the
   normalized musical projection X1 ≡ X2.
   **Recalibrated bar (not 0 diffs):** unlike the *forward* ABC round-trip
   (0 diffs — there `write_abc` consumes a Score co-designed by the lowering),
   the reader→ABC path runs through the **lossy XML intermediate** and yields a
   valid-but-**different** Score (extra `Part.voices` instead of `&` overlays,
   canonical-major `K:` instead of the original mode, no `X:`/`W:`/`V:` text).
   So the honest bar is **high structural round-trip (≥ ~95% in-scope) with the
   residual categorized and adjudicated** — reader-completion-gap (fix) vs
   write_abc-can't-express-the-reader-Score (log) — mirroring the triage
   methodology, NOT a raw 0. **R1 outcome: 9,514 / 9,933 in-scope round-trip
   structurally (95.8%); 419 residual categorized** (≈201 empty-trailing-measure
   / `y`-spacer length, ≈101 multi-voice `V:`-vs-`&`, ≈50 key/barline ordering at
   repeats, ≈25 slur-drop, ≈42 misc/derived-tuplet). The two dominant clean
   classes were fixed in R1 (barline/key-display synthesis in R1b: 9928→1179;
   global tuplet `pair_id` renumber in R1c: 1179→419). The residual is logged for
   the consolidated residual pass (R3-adjacent); it does **not** block R1 (the
   product surface works), and feeds leg-4 of the promotion bar.

**Un-gate decision (R4):** when legs 1–4 are green, promote. The HOW is settled
in Decision 3 below (CLI always-on via a thin import surface; the heavy reader
stays behind the optional dep until the evidence is in, then the dep is made a
default — an explicit, documented, justified flip, exactly as the prompt's hard
constraint requires).

**Stop rule:** if R2/R3 evidence flattens before the floor (a hard residual
class that is neither a reader bug nor cleanly adjudicable), **stop and document**
the principled gap rather than forcing it; promotion then waits or proceeds with
the gap explicitly logged in `docs/musicxml-reader.md` + AGENTS.md. The user
makes the final un-gate call if the floor is missed.

---

## Decision 2 — Reference-dialect verification wiring

**Reuse `tools/music21_polars_corpus_compare.py` unchanged**, with the reader in
the loop via the CLI. The forward comparator already does the hard part — a
music21 **semantic** diff of two MusicXML trees — and takes
`--croma-xml-root <dir>` (croma-emitted XML) vs `--reference-root <dir>`
(abc2xml XML). It does not care HOW the croma XML was produced. So:

```
abc2xml ref XML  ──croma read -o──►  croma re-export XML
        │                                   │
        └────────── music21 compare ────────┘   (existing comparator)
```

- A **sibling driver** `tools/musicxml_reverse_corpus_compare.py` (R2) walks the
  abc2xml-reference dir, runs `croma read <ref.xml> -o <reexport.xml>` (the R1
  CLI surface, built with `--features musicxml-reader`) into a re-export dir
  under `docs/untracked/`, then invokes the existing comparator with
  `--croma-xml-root <reexport-dir> --reference-root <abc2xml-dir>`. Output is the
  same report/whitelist/dropped/worklist artifacts the forward flow produces.
- **Triage flow** is the established one ([[raw-comparator-triage-methodology]]):
  the `divergence-triage` skill drives `abc-divergence-investigator` per file →
  verdict ∈ {fix-croma-reader, croma-writer-can't-express (drop+log),
  abc2xml-ism (drop+log, NEVER mimic)}. Re-export after any reader fix; verify
  regressions via whitelist set-diff (HEAD vs fresh).
- **Why reuse, not a new comparator:** the semantic-equivalence logic, the typed
  fact tables, the cache, and the drop adjudications are all already correct and
  battle-tested. A second comparator would be a second spec to rot. The reverse
  driver is a ~thin wrapper that only swaps the croma-side producer.
- **R1 is the enabler:** R2 cannot run until `croma read -o` exists, so R1 ships
  first.

---

## Decision 3 — CLI surface + gating

**Two subcommands, feature-plumbed, default build untouched:**

- `croma read <file.musicxml> [-o out]` — read XML → Score, **re-export to
  MusicXML** (the inspect/idempotence surface; this is what the R2 driver calls).
  `--format xml|abc|dump` selects the output projection (default `xml`):
  `xml` = `write_musicxml`, `abc` = `write_abc`, `dump` = a Score debug view.
- `croma musicxml2abc <file.musicxml> [-o out.abc]` — read XML → Score →
  `write_abc` → ABC. A convenience alias for `read --format abc`, named for
  discoverability (it is the headline user-facing conversion).

**Gating (preserves zero default deps until R4):**

- New `croma-cli` feature: `musicxml-reader = ["croma-core/musicxml-reader"]`.
- The `Read` / `Musicxml2abc` `Command` enum variants and their handlers are
  `#[cfg(feature = "musicxml-reader")]`. The default `cargo build -p croma-cli`
  pulls **no** roxmltree and exposes **neither** subcommand.
- Built with `cargo build -p croma-cli --features musicxml-reader`, the commands
  appear and the reader is reachable. CI adds this feature arm (it already tests
  `croma-core --features musicxml-reader`).
- When the feature is **off**, running `croma read` simply isn't a known
  subcommand (clap rejects it) — no stub, no runtime dep.

**R4 promotion mechanism (decided now, executed in R4):** when the evidence is
in, **make `musicxml-reader` a default feature of `croma-cli`** so `croma read`
/ `musicxml2abc` are always present in the shipped binary, while **`croma-core`
keeps `musicxml-reader` OPT-IN** (its default build stays zero-dep +
crates.io-publishable — the library contract from the design spec is preserved).
This is the explicit, documented dep flip the hard constraint allows: roxmltree
becomes a dep of the *CLI binary*, never of the *core library*'s default build.
Rationale over the alternatives: a separate `croma-import` crate duplicates the
GM tables / formulas the co-located reader reuses (rejected in the design spec);
making `croma-core`'s reader default would break the publishable zero-dep
library contract. CLI-default + core-opt-in satisfies both.

---

## Decision 4 — Foreign-engraver scope

- **Primary (have it):** the abc2xml-reference 10k
  (`docs/untracked/corpus/zenodo-10k/musicxml/`) — the foreign dialect R2 proves.
  abc2xml/music21 emit elements croma's writer never does; this is the real
  cross-writer test.
- **Stretch (if offline samples exist):** MuseScore / Finale / Sibelius sample
  MusicXML. music21 ships a bundled corpus with some non-abc2xml MusicXML
  (`music21.corpus`); R2 will probe it for a handful of foreign-engraver files
  and run them through the same read→re-export→compare. If none are readily
  available offline, **log the scope limit**: abc2xml is the proven foreign
  dialect; other engravers are deferred for lack of offline samples (not a
  silent omission). No network fetches.

---

## Staging (each stage: TDD, gated, landed via `land.py`, terse child receipt)

| Stage | Scope | Gate |
|---|---|---|
| **R1** | CLI `read` / `musicxml2abc`, feature-plumbed (no default dep). Reader Score-completion for the ABC projection. Structural reader→ABC round-trip prover. | **DONE**: both feature states build; reader→ABC structural 9514/9933 (95.8%), residual categorized; XML idempotence held 9915/9935; forward byte-identical |
| **R2** | reverse driver + abc2xml-ref music21 parity; triage divergences; fix reader bugs, adjudicate the rest; probe other engravers. | **DONE**: parity 0→**98.50%** (9850/10000) via 3 clean read-side fixes (DTD-tolerant parse, textless functional harmony, decimal alter); residual adjudicated to verdicts; idempotence held 9915/9935; forward byte-identical; 40/40 music21 `.mxl` totality |
| **R3** | self-loop residual closeout (19 cross-channel ordering + 1 nested-21:16 tuplet). | **DONE**: 9915→**9934/9935** (19 fixed read-side, 0 regressions, R2 parity unchanged); 1 nested-tuplet adjudicated (principled stop) |
| **R4** | promotion: CLI-default feature flip, AGENTS.md / docs / README / tracker. | **DONE**: legs 1–4 green; `croma-cli` default = `["musicxml-reader"]` (reader ships in CLI, roxmltree CLI-binary-only); `croma-core` default stays zero-dep + publishable; `--no-default-features` = reader-less CLI; AGENTS.md/README/docs updated; forward proofs intact |

**Hard constraints (restated):** zero default deps for `croma-core` preserved
through R4 (R4 flips only the *CLI* default); additive-only to forward (raw
whitelist 9390/0, fmt 10000/0, ABC round-trip unchanged, forward source
byte-identical); writer-is-spec, foreign dialects tolerated never mimicked; no
panics; reader stays gated until R4 meets the bar.
