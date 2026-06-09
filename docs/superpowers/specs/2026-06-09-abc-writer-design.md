# `Score → ABC` canonical writer (design)

Status: approved design, pre-implementation
Date: 2026-06-09
Branch: `work/abc-writer`

> **For the next session:** this spec stands alone — it does not depend on the
> brainstorming conversation that produced it. Read it, then run
> `tools/session_bootstrap.sh`, read `AGENTS.md`, and continue with the
> `superpowers:writing-plans` skill to produce the implementation plan, then
> implement with TDD. Do **not** start on `main`; this branch is `work/abc-writer`.

## Why

Croma is today a one-directional ABC → MusicXML toolkit. The strategic next step
is **bidirectional**: MusicXML → ABC, turning Croma into an ABC ↔ MusicXML
bridge. The pivot is the existing semantic `Score` model. The epic decomposes
into two independent sub-projects:

1. **`Score → ABC` writer** — *this spec*. Validated now via self-round-trip over
   the existing 10k corpus, with no new infrastructure.
2. **MusicXML → `Score` reader** — a *later, separate* spec (use `roxmltree`,
   feature-gated; see "Deferred" below).

Building the writer first is deliberate: it is provable *today* with the corpus
and the trusted forward pipeline, before any XML reading exists.

## Guiding principle (the whole point)

**Write ABC in the cleanest possible way, so our own parser reads it like a
charm.** Concretely, the writer emits *canonical* ABC — output that is a fixed
point of `croma fmt` and round-trips through `parse_document` + `lower_score`
with no change to the music. Our parser is the oracle; our formatter
(`croma-fmt`, already shipped — see `crates/croma-fmt`) defines "clean"; the
generator targets both.

## Correctness bar — full structural projection round-trip

Because *we* author the ABC, the bar is stronger than the formatter's pitch-only
gate. For every corpus tune in slice-1 scope:

```
ABC → parse_document → lower_score → Score → write_abc → ABC'
    → parse_document → lower_score → Score'
prove:  projection(Score) == projection(Score')
```

`projection` is a normalized musical-fact tuple, NOT raw `Score` struct equality
(source spans differ): ordered **pitches (step, alter, octave)** + **per-event
durations** + **barline/measure boundaries** + **ties**. As later slices add
constructs, the projection grows (chord membership, tuplet ratios, voice id,
lyric syllables).

Plus two invariants asserted in tests:

- **Canonical fixed point:** `fmt(write_abc(score)) == write_abc(score)`.
- **Round-trip idempotency:** writing `Score'` yields the same ABC as writing
  `Score` (`write_abc(Score) == write_abc(Score')`).

## Slice-1 coverage

**In scope (slice 1):**

- Required header fields `X:`, `M:`, `L:`, `K:`, plus available metadata `T:`
  (title) and `Q:` (tempo) when present on the `Score`.
- A single voice.
- Notes and rests with durations (integer, fraction `a/b`, slash shorthand).
- Accidentals; octave marks.
- Barlines including repeats (`|:`/`:|`) and first/second endings.
- Ties.

**Out of scope (later slices, grow coverage until the whole corpus round-trips):**

- Chords, tuplets, grace notes, slurs, broken rhythm.
- Multi-voice `V:`, overlays `&`.
- Lyrics `w:`/`s:`, harmony chord symbols, annotations, decorations.
- Free text, comments, stylesheet/`%%` directives (these live in `AbcDocument`,
  not `Score`, so they are inherently outside a `Score → ABC` writer).

Validation runs only on the **corpus subset whose lowered `Score` uses solely
in-scope constructs**, detected programmatically (e.g. single voice, no
`TimedEventKind::Chord`, no tuplet attachments). This mirrors how the parser
phases scoped their evidence; coverage expands slice by slice.

## Generation rules

- **Canonical conventions:** emit exactly as `croma fmt` would — no space after a
  field colon (`K:C`, `M:6/8`), single inter-token spacing, canonical barlines.
  Assert the fmt-fixed-point invariant rather than re-deriving the conventions.
- **Unit note length `L:`** derived from the meter by the ABC 2.1 default rule
  (meter value `≥ 3/4` → `L:1/8`, else `L:1/16`), emitted explicitly. Each event
  duration is written as the integer or `a/b` multiple of `L:` (using the
  `Rational`/`Fraction` math already in `croma-core`).
- **Accidentals round-trip by construction:** `NoteEvent.written_accidental:
  Option<AccidentalMark>` already records the *originally written* accidental, so
  the writer emits that directly instead of re-deriving from key + measure
  accidental state. (For the future MusicXML reader, `written_accidental` is
  populated from the `<accidental>` element.)
- **Octave marks:** from `Pitch.octave` (middle C = octave 4; `C` = octave 4, `c`
  = octave 5), emitting `,`/`'` as needed.
- **Headers:** always emit `X:`, `M:`, `L:`, `K:` (required for the result to
  parse); emit `T:`/`Q:` when the `Score` carries them. The bar is *musical*
  fidelity — composer/comments/directives are not in `Score` and are out of
  scope for the round-trip.

## Architecture

- **`croma-core`** — new module `to_abc` (sibling of the `musicxml` writer), with
  a public entry point mirroring `write_musicxml`:

  ```rust
  pub fn write_abc(score: &Score, options: AbcWriteOptions) -> String;
  ```

  Pure string generation; **no new dependencies**; stays crates.io-publishable.
  Keep files focused (well under the 1000-line module-breakdown rule).

- **`croma-cli`** — add `croma dump abc FILE.abc` (parse → lower → `write_abc` →
  stdout). This is the debug surface and the validation hook. It slots into the
  existing clap `Dump` subcommand next to `tokens`/`tree`/`score`.

- **Corpus harness** — `tools/prove_abc_roundtrip.py` (local only, never CI).
  For each in-scope corpus tune: compute the structural projection of
  `croma xml FILE` and of `croma xml <(croma dump abc FILE)` and assert they are
  identical; report total / in-scope / structural-diff counts. Reuse the
  `pitch_seq` idea from `tools/prove_divergences.py` and extend the projection
  with `<duration>`, measure boundaries, and `<tie>`. Generated output stays
  under `docs/untracked/`.

## Relevant existing API (verified)

- `parse_document(source, ParseOptions) -> ParseReport<AbcDocument>`;
  `lower_score(&AbcDocument, LowerOptions) -> ParseReport<Option<Score>>`;
  `write_musicxml(&Score) -> MusicXmlExport` (the writer to mirror).
- `Score.parts: Vec<Part>` → `Part.voices: Vec<Voice>` →
  `Voice.events: Vec<TimedEvent>`; `TimedEvent { measure, onset, duration, kind,
  .. }`; `TimedEventKind::{Note(NoteEvent), Chord(ChordEvent), Rest(RestEvent),
  Barline(..), RepeatEnding(..), Spacer }`.
- `NoteEvent { pitch: Pitch, written_accidental: Option<AccidentalMark>,
  chord_member }`; `Pitch { step: char, alter: i8, octave: i8 }`.
- `ScoreMetadata` carries title/tempo (`tempo`, `tempo_model`); meter/key models
  (`MeterModel`, `KeySignatureModel`) and `Rational`/`Fraction` are re-exported
  from `croma-core`.
- Octave/middle-C convention confirmed: `CEG` under `K:C` lowers to C4/E4/G4.

(The implementation must confirm exact field names for measure/barline/tie
representation in the `Score` while writing the failing tests.)

## Testing & proof

### Unit / TDD (in `cargo test --workspace`)

- Per construct: a fixture ABC whose round-trip projection is asserted equal,
  plus the expected canonical text, plus the fmt-fixed-point assertion.
- No-happy-path: a `Score` with an in-scope edge (e.g. dotted/fractional
  duration, tie across a barline, second ending) round-trips structurally.
- Idempotency: `write_abc(Score) == write_abc(Score')`.

### Corpus round-trip (LOCAL only, never CI)

`tools/prove_abc_roundtrip.py` over the in-scope subset of the external 10k:
**0 structural diffs** on the projection. Report how many tunes are in scope
(coverage %) so later slices can track growth. Provision the corpus per
`AGENTS.md`.

## Process / gates

- Branch `work/abc-writer` (never `main`). TDD; subagents for investigation /
  implementation, orchestrator runs the corpus round-trip.
- Before every commit: `cargo test --workspace`;
  `cargo clippy --workspace --all-targets -- -D warnings`;
  `cargo fmt --all -- --check`; `uv run pytest -q` (if Python touched);
  `git diff --check`.
- Commit messages carry **no Co-Authored-By / AI / tool trailer**
  (`git log main..HEAD --format=%b | grep -ci Co-Authored-By` == 0).
- `croma-core` stays crates.io-publishable (no new runtime deps in the writer).
- Open a PR when the slice is met; merge only when both CI checks
  (Rust + Linux/nixos) are green; then delete the branch and update the tracker
  (runtime DB + exported SQL snapshot), recording the in-scope coverage %.

## Definition of done (slice 1)

`write_abc` exists in `croma-core`; `croma dump abc` exposes it; the writer emits
canonical ABC that is a `croma fmt` fixed point; the corpus round-trip proves
**0 structural diffs** over the in-scope subset (with coverage % reported);
tests/clippy/fmt green; PR merged on green CI; tracker updated. Then iterate the
slice coverage (chords → tuplets → voices → lyrics …) until the whole corpus
round-trips, after which the **MusicXML → `Score` reader** (`roxmltree`,
feature-gated) is the next sub-project.

## Deferred — MusicXML → `Score` reader (next sub-project, own spec)

- Use **`roxmltree`** (read-only DOM; measured footprint = 1 net new crate, since
  `memchr` is already in the workspace lock; zero proc-macros; crates.io-clean).
  `quick-xml` is the only fallback (pick it only to standardize one XML crate for
  read + the currently hand-rolled write, or if streaming huge files matters).
  `serde-xml-rs` is rejected (13-crate tree; MusicXML's irregular schema fights a
  serde mapping).
- **Feature-gate** the reader so `croma-core`'s default surface stays lean.
- Validate the reader by the full round-trip: `ABC → xml → Score(via reader) →
  abc → xml`, note-identical against the trusted forward pipeline and the corpus
  reference MusicXML.
