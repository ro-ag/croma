# Design — MusicXML → Score reader (reverse direction)

**Date:** 2026-06-15
**Status:** design (awaiting approval) → staged implementation
**Epic:** close the forward/reverse loop. Forward (`ABC → Score → write_musicxml → XML`)
is complete, including `%%MIDI` translation (PRs #122–#125). This builds the inverse:
`read_musicxml(xml: &str) -> ParseReport<Score>`.

## 1. Goal & scope

Add a **total, non-panicking** MusicXML reader that inverts croma's own writer. It is
staged by structural dependency (S1 skeleton → S6 structure). The motivating slice is
re-importing `<midi-instrument>` / `<transpose>` into the model (S2–S3), which closes the
forward/reverse loop. The reader stays **experimental / gated** (like the LSP) until it
has corpus round-trip evidence comparable to the formatter's.

The writer is the spec. The reader inverts croma's writer exactly; it never mirrors an
abc2xml-ism. Reading the abc2xml reference corpus is a **stretch** goal (broader dialect
coverage via music21), never the driver.

## 2. Architecture decisions (the "DECIDE FIRST" set)

### 2.1 Crate placement — feature-gated in `croma-core`
- New module tree `crates/croma-core/src/musicxml/read/`.
- Public entry `read_musicxml(xml: &str) -> ParseReport<Score>`, re-exported from `lib.rs`,
  **both behind `#[cfg(feature = "musicxml-reader")]`**.
- New `[features]` in `crates/croma-core/Cargo.toml`:
  ```toml
  [features]
  musicxml-reader = ["dep:roxmltree"]

  [dependencies]
  roxmltree = { version = "0.20", optional = true }
  ```
- **Default build stays zero-dep + crates.io-publishable.** `cargo build -p croma-core`
  (default features) pulls in no deps and does not compile the reader.
- **Rationale over a separate `croma-import` crate:** the reader's correctness is defined by
  inverting the writer. Co-locating them lets the reader reuse the writer's `pub(crate)`
  GM-program-name table (`score.rs`) and the CC7/CC10 + transpose formulas **directly** —
  no re-export across a crate boundary, no duplicated constant tables. The prompt's hard
  constraint is "do not invent a second dialect"; one crate is the strongest guarantee of
  that. The separate-crate route matches the LSP precedent but would force widening
  visibility or duplicating tables (drift risk) — rejected.

### 2.2 XML parser — `roxmltree`
Read-only DOM, small, pure-Rust, MIT/Apache-2.0 (compatible with croma's MIT). A whole
MusicXML file fits in memory; a DOM is far simpler than streaming for a reader that walks
parts → measures → notes. `quick-xml` (streaming) would be faster but materially more code
for no benefit at corpus file sizes — rejected. roxmltree is the optional dep; it never
enters the default build.

### 2.3 Verification — XML re-emission idempotence (primary corpus gate)
**Primary gate:** prove `write(read(write(score))) == write(score)` as **exact strings**
over the full 10k.

Why this over the prompt's "semantic Score projection": the writer is **lossy** — it drops
ABC-only model state (`VoicePropertiesModel` clef/transpose/name text, `MeterModel.display`,
`KeySignatureModel.display`, `preserved_directives`, every `Span`, `diagnostics`). So
`read ∘ write = id` over the raw `Score` is impossible, and a hand-maintained
`SemanticScore` projection becomes a *second spec* that silently hides reader gaps when a
field is omitted from it. The writer is already the spec and is **deterministic** (verified:
`MusicXmlWriter` is a pure ordered string builder, no hashmap ordering, no clocks). Byte
equality of `write(read(write(s)))` and `write(s)` therefore proves the reader recovered
**every** writer-emitted fact — drops nothing, invents nothing — with no projection to rot.

This gate is **self-policing across stages**: any element the writer emits that the reader
does not yet read back makes `X2 != X1`, turning the gate red. The gate thus *drives* each
stage's work list instead of hiding it.

- **Per-element unit tests** (TDD, in-crate, `#[cfg(feature = "musicxml-reader")]`) assert
  reconstructed **model fields directly** (e.g. `score.parts[0].voices[0].midi_transpose ==
  Some(-12)`), independent of the corpus gate.
- **Per-stage corpus metric:** count of 10k files that round-trip idempotently (monotonic,
  honest — a file counts only when the reader handles *every* element its export uses), plus
  a **histogram of the first diverging XML tag** among non-idempotent files (this names the
  next stage's targets).
- **Stretch gate:** read the abc2xml **reference** MusicXML → `Score` → re-export → music21
  compare (`tools/music21_polars_corpus_compare.py`). Catches dialect elements abc2xml emits
  that croma's writer never does. Reported, not required for landing a stage.
- **Totality fuzz:** `read_musicxml` must not panic on any file in the abc2xml reference set
  **or** croma's own 10k exports.

### 2.4 Unreconstructable fields
- `Span`: filled with a documented sentinel `Span::new(0, 0)` (call it `READER_SPAN`). The
  idempotence gate is span-agnostic because the writer never emits spans.
- `Score.diagnostics` / `ParseReport.diagnostics`: carry reader warnings only (unknown tags,
  malformed values). Empty on clean croma-emitted input.
- `preserved_directives`, `score_directives`, ABC-only `VoiceProperties*` text, the legacy
  `Voice::events`-vs-`Tune` representations: left at documented defaults. Not in the XML, so
  out of scope by construction — and invisible to the idempotence gate.

## 3. Model-construction contract

The reader builds a `Score`. It populates **exactly the fields the writer reads** (determined
per stage by reading the corresponding writer module); all others get documented
defaults/sentinels. The contract is enforced automatically: the idempotence gate goes red if
the reader leaves any writer-read field wrong, so the field list need not be enumerated
up-front — the gate reveals it.

The writer consumes (per `musicxml/mod.rs` + modules): `score.metadata` (title, composer,
tempo_model, meter, key), `score.parts[].{id,name,voices[],staves[]}`,
`voice.{events: Vec<TimedEvent>, measures: Vec<Measure>, midi_instrument, midi_transpose}`,
and `score.divisions`. The reader reconstructs these from the XML; the dual `events`/`measures`
representation is rebuilt only insofar as the writer reads it.

## 4. Staging (each stage = one branch + one PR, landed via `land.py`)

Each stage: **failing unit test(s) first** → reader code → idempotence test on a minimal
snippet → full-10k corpus idempotence delta → never regress a prior stage → land.

- **S1 — skeleton.** `<score-partwise>` → parts → measures → `<note>` (pitch
  step/octave/alter, `<rest>`, `<duration>`/`<type>`/`<dot>`), `<backup>`/`<forward>`. The
  irreducible core. `read_musicxml` first exists here.
- **S2 — attributes.** `<divisions>`, `<key><fifths>` (+ explicit `<key-step>/<key-alter>`),
  `<time>`, `<clef>`, `<transpose>` → `midi_transpose`.
- **S3 — part-list MIDI.** `<score-instrument>` / `<midi-instrument>` → `MidiInstrumentModel`.
  **Closes the forward/reverse loop (the motivator).**
- **S4 — notations.** ties, slurs, tuplets (`<time-modification>`), articulations/decorations,
  beams.
- **S5 — directions / harmony / lyrics.** `<direction>`, `<harmony>`, `<lyric>`.
- **S6 — structure.** multi-voice (`<voice>` / `<backup>`), repeats/endings/barlines, grace
  notes, chords (`<chord/>`).

Stop staging where corpus idempotence coverage flattens; `log` what is unsupported.

## 5. MIDI inverse spec (S2 transpose, S3 instruments) — exact inverse of `docs/midi-directives.md`

| MusicXML (writer emits) | Model (reader reconstructs) |
|---|---|
| `<midi-program>N` | `program = N − 1` (0-based; forward is `prog + 1`) |
| `<midi-channel>n` | `channel = n` |
| `<volume>v`  (writer: `{:.2}` of `cc/1.27`) | `volume_cc = round(v × 1.27)` |
| `<pan>p`  (writer: `{:.2}` of `cc/127×180 − 90`) | `pan_cc = round((p + 90) × 127 / 180)` |
| `<attributes><transpose><chromatic>n` | `midi_transpose = n` (`i16`) |

Idempotence requires the inverse to recover the exact integer CC the writer started from. At
`{:.2}` precision the round-trip is stable for all `cc ∈ 0..=127`; an exhaustive unit test
proves `round(fmt(cc) × 1.27) == cc` (and the pan analogue) for every value.

## 6. Testing & totality

- Unit tests: in `crates/croma-core/src/musicxml/read/`, `#[cfg(feature = "musicxml-reader")]`.
- Corpus idempotence prover: mirror `crates/croma-fmt/src/corpus_proof.rs` — a feature-gated
  Rust test driven by `ABC_ROOT` that runs `write → read → write` over the corpus and asserts
  0 diffs on the supported subset, plus a standalone path emitting the first-divergence
  histogram for iteration. Pure Rust (read + write both in croma-core); no Python needed for
  the primary gate.
- No `unwrap`/`expect`/`panic`/`todo` in reader code (workspace lints + the totality rule);
  use `?` and explicit diagnostics. roxmltree parse failure → `ParseReport` with a diagnostic
  and an empty `Score`, never a panic. Unknown elements → ignored (+ optional diagnostic).
- CI matrix must add `--features musicxml-reader` to `cargo test` and
  `cargo clippy --workspace --all-targets`. Default (reader-off) build must also stay green.

## 7. Execution model (keep the orchestrator light)

The main session is the **orchestrator**: it holds this design + tracker state, lands PRs,
and verifies gates. Each stage's code work is delegated to a **child agent** that reads the
relevant writer module, TDDs the reader element, runs the idempotence gate, and returns a
**terse receipt only** (branch, files touched, tests added, corpus idempotent-count
before/after, gate status). Stages are sequential (structural dependency), so children run
one at a time. Children must **strip any AI co-author trailer** before pushing
(`no-coauthor-trailer`). Landing is via `uv run tools/land.py <branch> -y`.

## 8. Out of scope (YAGNI)

- MusicXML features croma's writer never emits (timewise scores, figured bass, …) — relevant
  only to the abc2xml-reference stretch; they get diagnostics, never errors.
- Reconstructing ABC-only model state (clef text, `preserved_directives`) — not in the XML.
- Promoting the reader out of the gate — it stays experimental until corpus-proven.
- Changing any forward behavior. Additive only. Forward proofs must stay green: raw whitelist
  9390 / 0 mismatches, fmt corpus proof 10000/0, ABC round-trip unchanged.

## 9. Risks

- **Float CC round-trip (S3).** Mitigated by the exhaustive `0..=127` stability test (§5).
- **Reader builds a valid-but-different `Score` that still re-writes identically.** Accepted by
  design — the gate proves writer-observable equivalence, which is exactly the contract; model
  identity is neither required nor achievable (writer is lossy).
- **Feature-matrix CI gap.** Mitigated by §6 (test both feature states).
- **Writer non-determinism.** Ruled out (verified deterministic).

## 10. Deliverable

A feature-gated, dep-isolated, non-panicking `read_musicxml(xml) -> ParseReport<Score>`
inverting croma's writer; staged S1–S6, each TDD'd and proven by the XML re-emission
idempotence gate over the 10k (0 diffs on the supported subset, reported per stage); the MIDI
slice closing the forward/reverse loop; `docs/musicxml-reader.md` (coverage/policy, created
during impl) linked from `docs/parser-backlog.md`; forward proofs intact; each stage landed
via `land.py` with green CI; session ends on `main`.
