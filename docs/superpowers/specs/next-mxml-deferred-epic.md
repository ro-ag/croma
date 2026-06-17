# Next epic (ready-to-use prompt) — MusicXML reader: deferred-item closeout

This is a **copy-paste continuation prompt** for a future session. The MusicXML reader
reached a principled stop after PRs #126–#143 (every clean reader bug fixed); the items
below are the FEATURE/HARD work deferred by an explicit user decision on 2026-06-16. Paste
the fenced block as the session prompt.

---

```
# PROMPT — Epic: MusicXML reader — deferred-item closeout (multi-staff regroup, nested tuplets, directions)

## Context
croma's MusicXML→Score reader (`read_musicxml`, `crates/croma-core/src/musicxml/read/`)
is PROMOTED/un-gated and shipping: `croma read` / `croma musicxml2abc` in the default
CLI build; `croma-core` keeps `musicxml-reader` opt-in so its default build stays zero-dep
+ crates.io-publishable. Prior epics (PRs #126–#143) reached a PRINCIPLED STOP:
- self-loop XML re-emission idempotence **9934/9935**;
- foreign abc2xml music21 parity **98.50%** (DTD-tolerant parse, textless `<harmony>`,
  decimal `<alter>`);
- reader→ABC (`musicxml2abc`) **musically faithful 9930/9933** (97.88% byte-structural;
  208 sounding-equal valid-but-different + 3 HARD).
Every CLEAN reader bug is fixed. The remaining work is FEATURE/HARD scope, deferred by an
explicit user decision. This epic addresses those deferred items.

FIRST run `tools/session_bootstrap.sh`; read `AGENTS.md` + memory (`uv run python
tools/progress/progress.py memory`), especially [[musicxml-reader-state]] (full state +
deferred list), [[figured-bass-parked]], [[raw-comparator-triage-methodology]]; and
`docs/musicxml-reader.md` (coverage/policy, incl. the "Reader→ABC residual (adjudicated)"
section). Existing measurement tools: `tools/prove_reader_abc_roundtrip.py` (reader→ABC
structural + first-divergence histogram), `tools/musicxml_reverse_corpus_compare.py`
(foreign music21 parity), the `corpus_idempotence_measurement` cargo test (self-loop).
Corpus: `docs/untracked/corpus/zenodo-10k/{abc,musicxml}`; music21 `.mxl` (Bach/SATB) for
the classical stretch.

## DECIDE FIRST — document before code (a short decisions doc under docs/superpowers/specs/)
1. Per deferred item: FIX vs ADJUDICATE, with the evidence test = "does any sounding
   pitch+duration/structural-grouping fact actually drop?" (the test that closed phase-61).
2. For pedal/rehearsal `<direction>` (item P3): decide whether to EXTEND the forward writer
   (model + write_musicxml + write_abc) so it round-trips — a FORWARD change that must
   re-prove the forward gates — or adjudicate it as writer-can't-express. Default to
   adjudicate unless the writer extension is clean and justified.
3. Confirm the un-gate stays: reader work is additive; `croma-core` default stays zero-dep.

## The work (staged; each stage TDD'd, landed via land.py; stop where coverage flattens)
- **P1 — multi-staff / SATB `%%score` regroup (highest value).** Reconstruct `%%score {…}`
  brace / `[…]` bracket groups + clef-split voices from MusicXML `<staff>` (2+ staves per
  part) and `<part-group>`. Reader-only (croma forward already supports `%%score`). Target:
  raise foreign reverse-parity on multi-staff files + close the reader→ABC multi-voice
  `V:`-vs-`&` equivalence where a real staff-grouping fact is currently lost. Measure on the
  music21 `.mxl` classical corpus (totality already 40/40; now chase parity) AND the abc2xml
  reverse corpus.
- **P2 — nested-tuplet inverse.** Reconstruct nested tuplet spelling (writer's reduced
  `441/256` from stacked 21:16 → `7:8`/`3:2` nesting). Closes the 1 self-loop residual
  (`tune_003732`) + the foreign `tuplet` divergence category (~47 files). Target
  9935/9935 self-loop or a documented principled stop.
- **P3 — pedal / rehearsal-mark `<direction>` (decide P-scope first).** abc2xml/foreign emit
  `<pedal>`, rehearsal `<rehearsal>`, etc. that croma's writer never emits → reader leaves
  them unread with a diagnostic. Per DECIDE-FIRST #2: either extend model+writer+reader so
  they round-trip (FORWARD change — re-prove gates), or adjudicate as writer-can't-express
  and document. ~34 foreign files.
- **P4 — the 3 HARD reader→ABC files.** `tune_013508` (sounding-diff), `tune_003732` (nested
  tuplet — overlaps P2), `tune_014467` (degenerate un-escapable embedded quotes — likely
  adjudicate). Fix where clean, adjudicate the rest with evidence.
- **Figured bass — DO NOT pursue.** PARKED ([[figured-bass-parked]]): ABC 2.1 AND 2.2 have no
  equivalent (verified). Only revisit if classical-score import is made an explicit goal.

## HARD CONSTRAINTS
- **No regression to any proven gate:** self-loop idempotence 9934/9935, reader→ABC
  9930/9933 musically-faithful, foreign reverse parity 98.50%, totality 0 panics. Re-measure
  each after every stage.
- **Forward byte-identical** UNLESS a stage (P3) explicitly + justifiably extends the writer.
  If it does, RE-PROVE the forward gates: raw whitelist 9390/0, fmt 10000/0, ABC round-trip.
  Default reader work is additive and forward-neutral.
- **Reader stays un-gated; `croma-core` default stays zero-dep + publishable.** No new default
  deps on the library.
- **Writer-is-spec for the self-loop; foreign dialects TOLERATED, never mimicked** — degrade
  with diagnostics on anything croma can't represent ([[spec-is-driver-abc2xml-is-baseline]]).
- **No panics.** **No AI co-author trailer** (strip from subagents before landing).

## Discipline — KEEP THE ORCHESTRATOR SMALL
- The main agent is an ORCHESTRATOR ONLY: it holds the plan + decisions doc + tracker, lands
  PRs via `uv run tools/land.py <branch> -y`, and verifies gates. **The main agent must NOT
  write reader/test code itself.** ALL code work is delegated to SUBAGENTS, one per stage (or
  per investigation), each: reads the relevant module, TDDs the change (failing test first),
  runs the measurement gates, and returns a TERSE RECEIPT only (branch, files, before/after
  numbers, gate pass/fail, commit hash). Keep main-context lean — relay receipts, don't dump.
- Branch per stage (`feature/mxml-<slug>`; NEVER main). Tracker `phase-62-mxml-reader-*` +
  SQL snapshot, updated/exported right before each stage's commit (restore wipes uncommitted
  tracker rows — see [[tracker-testbed-restore-gotcha]]). Session ends on main only.

## Deliverable
Each deferred item fixed (TDD, landed) or adjudicated-with-evidence and documented in
`docs/musicxml-reader.md`; all proven gates intact (re-measured); reader still un-gated;
forward proofs green; each stage landed via land.py with green CI; orchestrator stayed small
(subagents did the work); session ends on main.
```
