# MusicXML reader — deferred-item closeout: decisions (phase-62)

Date: 2026-06-16. Author: orchestrator (croma reader epic). Governs the
phase-62 stages. Companion to the promotion bar
[`2026-06-16-musicxml-reader-promotion.md`](2026-06-16-musicxml-reader-promotion.md)
and the coverage/policy doc [`../../musicxml-reader.md`](../../musicxml-reader.md).

## Context

The reader (`read_musicxml`, `crates/croma-core/src/musicxml/read/`) is
PROMOTED/un-gated. Every CLEAN reader bug across both directions is fixed
(phase-60/61, PRs #126–#143). What remained was FEATURE/HARD scope deferred by an
explicit user decision: (P1) multi-staff / SATB `%%score` regroup, (P2)
nested-tuplet inverse, (P3) pedal / rehearsal `<direction>`, (P4) the 3 HARD
reader→ABC files. Figured bass stays PARKED (ABC 2.1 + 2.2 have no equivalent).

## Decision method

Each item was investigated read-only and decided **FIX vs ADJUDICATE** with the
phase-61 evidence test — **does any sounding pitch+duration OR structural-grouping
fact actually drop?** A FIX must move (or keep green) a corpus-proven gate; an
ADJUDICATE is documented with the evidence that no real fact is lost (or that the
fact is unrepresentable in ABC). The scope chosen by the user is the **maximal**
set: FIX P2, FIX P1a, EXTEND P3-rehearsal; adjudicate the remainder.

## Per-item decisions

### P2 — nested-tuplet inverse → **FIX** (reader-only, additive)

The lone self-loop residual `tune_003732`. Measure 5 (divisions=168) nests an
inner `3:2` inside an outer `7:8`: the first note carries TWO stacked
`<tuplet type="start">` and the **composite** `<time-modification>21/16`; the
writer reduced stacked 21:16 → `441/256`. The reader read both nested levels as
`21/16`, so on re-write the inner three notes come back **2× too long — a real
sounding-duration drop**, and `<type>` re-derives `quarter` not `eighth`.

**The inverse is deterministic.** The outer `7:8` ratio is explicitly present on
the post-inner-close tail notes (notes 4–8 carry `<time-modification>7/8`). When a
`Continue` note's `<time-modification>` diverges from the open tuplet's stored
composite, the outer factor is directly readable; inner = composite ÷ outer =
`(21/16) ÷ (7/8) = 3/2`. No ambiguity. Fix in `OpenTuplets::resolve`
(`read/mod.rs`, Continue branch): on a composite/stored mismatch, read the note's
`<time-modification>` as the corrected outer ratio and retro-patch the open
tuplet's already-emitted `TupletAttachment`s. ~25–35 LOC + a backward-scan helper.
Risk LOW (fires only on the rare composite mismatch; 1/9935 files). Target:
self-loop idempotence **9934 → 9935/9935**. Also closes P4's `tune_003732`.

The reverse comparator `tuplet` category (16 files / 93 rows) is a **different**
shape (abc2xml marks the final note `type="stop"`, croma's writer does not — a
dialect difference, not a nesting bug); P2 does not target it.

### P1a — `%%score` from `<part-group>` → **FIX** (reader-only, additive, no corpus gate)

The reader hardcodes one staff/part (`read_part` builds `staves: vec![Staff{1}]`;
every `Voice.staff` = that StaffId) and reads **zero** `<staff>`/`<part-group>`/
`<staves>`. abc2xml emits `<part-group>` on **351/10000** files (305 bracket + 46
brace), and these wrap **separate single-staff parts** (only 1 file has any
`<staff>`, 0 have `<staves>`). Today croma reconstructs the N parts but **drops the
bracket/brace grouping** — a structural-grouping fact lost.

croma's FORWARD model stores `%%score` as **raw text** in
`ScoreMetadata.directives`, re-emitted verbatim by `write_abc`. So the reader can
**synthesize a `%%score`/`%%staves` directive string** from the `<part-group>`
`type="start|stop"` + `<group-symbol>` (bracket → `[…]`, brace → `{…}`) over the
reconstructed part/voice ids — additive, reader-only, landing into an existing
model field, **no new grouping model**. This recovers the grouping for
`musicxml2abc` of foreign multi-part scores (choir/quartet/abc2xml multi-part).

**No corpus gate** measures it (the reverse comparator is grouping-blind — it
extracts notes/pitch/duration only), so P1a is **unit-test-proven** with a small
targeted before/after count (files whose `musicxml2abc` now emits `%%score`). It is
forward-neutral (croma's writer never emits `<part-group>`, so the self-loop never
exercises it) and must not regress any existing gate.

### P1b — foreign 2-staff-per-part staff-split → **ADJUDICATE** (writer-can't-express)

The genuinely-hard multi-staff case (a single MusicXML part with 2+ `<staff>` —
piano/SATB keyboard, per-note `<staff>N` routing, clef-split). Evidence: on the
music21 `.mxl` classical corpus, croma collapses all staves onto staff 1 with
**0 sounding-note loss** (Schubert *Lindenbaum*: notes 1812→1810, the 2 lost are
unrelated `<cue>`/`<pedal>`, `<staff>` 1836→0; Schumann *Dichterliebe №2*: 278→276,
`<staff>2` 84→0). The dropped fact is **engraving staff-routing only**. ABC has
**no 2-staff-per-part concept** and croma deliberately lacks a structured grouping
model, so this cannot round-trip faithfully. Documented as a reader residual
(already covered by the Decision-4 totality probe). Revisit only if classical
keyboard/SATB import becomes an explicit goal.

### P3 — pedal / rehearsal `<direction>` → **EXTEND rehearsal**, **ADJUDICATE pedal**

Corpus scan of all `<direction-type>` children: the only genuinely **unread** type
is `<rehearsal>` (**1109 occ / 410 files**). `<pedal>` and `<bracket>`/`<dashes>`/
`<octave-shift>`/`<harp-pedals>` are **0 files**. Both are **non-sounding**
engraving metadata; the reverse comparator does not extract `RehearsalMark` (the
R2 `direction=34` category is unrelated metronome/text alignment, not rehearsal),
so 403/410 rehearsal files already whitelist.

- **Pedal → ADJUDICATE (impossible).** 0 corpus files; ABC 2.1/2.2 has no
  sustain-pedal syntax and croma's decoration table has no `!ped!`. Writer-can't-
  express; nothing to round-trip.
- **Rehearsal → EXTEND.** abc2xml emits `<rehearsal>` as the MusicXML encoding of
  the ABC `P:` (section/part label, ABC 2.1 §4.3). croma currently **parses `P:`
  then drops it** (lowering no-op `misc.rs`), and the model has no section-label
  node. Extend the **forward writer + model + reader** so a body `P:` round-trips:
  model field → `lower` (P:→model) → `write_abc` (model→`P:`) → `direction.rs`
  (model→`<rehearsal>`) → reader (`<rehearsal>`→model). This is a **FORWARD change**
  that **must re-prove the forward gates** (raw whitelist 9390/0, fmt 10000/0, ABC
  round-trip) AND the self-loop. The comparator is rehearsal-blind so raw-whitelist
  parity is expected neutral, but the proof is mandatory, not assumed.

  Scope caution (for the implementer): handle the **body/inline `P:` section
  label**. The header `P:ABAB` *play-order macro* is a different construct — do not
  conflate. If any forward gate cannot stay green, report BLOCKED; the fallback is
  to adjudicate rehearsal too (the user accepted the gate-re-proof cost up front).

### P4 — the 3 HARD reader→ABC files

| File | Issue | Verdict |
| --- | --- | --- |
| `tune_003732` | nested 21:16 tuplet | **FIX via P2** |
| `tune_013508` | 3-staff `%%staves`+`&`-within-`[V:]`+mid-`[K:]`; one accidental flips flat↔natural, **total duration conserved** | **ADJUDICATE** (multi-voice-overlay rewrite the residual deliberately does not chase; sounding-equal) |
| `tune_014467` | `Q:"Figs 1-3" 3/2=84 …` tempo text **contains `"`**; ABC has no escape for `"` inside a `"…"` annotation, re-parse fabricates phantom notes | **ADJUDICATE** (degenerate source; a read-side strip would perturb the byte-identical `--format xml` inverse) |

## Stage plan (each TDD'd, own branch `feature/mxml-<slug>`, landed via land.py)

1. **P2** nested-tuplet inverse — reader-only; gate: self-loop 9934→9935/9935.
2. **P1a** `%%score` from `<part-group>` — reader-only; unit tests + targeted count;
   no existing gate regresses.
3. **P3** rehearsal `P:`↔`<rehearsal>` — forward+reader; **re-prove** raw whitelist
   9390/0, fmt 10000/0, ABC round-trip, self-loop; BLOCKED-fallback = adjudicate.
4. **Closeout** — fold the adjudications (P1b, pedal, `tune_013508`, `tune_014467`)
   into `docs/musicxml-reader.md`; update memory; session ends on main.

## Hard constraints (unchanged from the epic)

- **No regression** to any proven gate: self-loop 9934/9935 (→9935 after P2),
  reader→ABC 9930/9933 musically-faithful, foreign reverse parity 98.50%, totality
  0 panics. Re-measure after each stage.
- **Forward byte-identical** EXCEPT P3, which justifiably extends the writer and
  **re-proves** the forward gates.
- **Reader stays un-gated; `croma-core` default stays zero-dep + crates.io-
  publishable.** No new default library deps. P1a/P2 are reader-only; P3's model
  field is plain data (no dep).
- Writer-is-spec for the self-loop; foreign dialects tolerated, never mimicked
  (P1a/P3 read foreign `<part-group>`/`<rehearsal>` into croma's OWN model surface).
- **No panics. No AI co-author trailer.**
