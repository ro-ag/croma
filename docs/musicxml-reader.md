# MusicXML → Score reader (coverage & policy)

The reader inverts **croma's own writer** (`crates/croma-core/src/musicxml/`).
It is **feature-gated** behind `musicxml-reader` and **experimental** (like the
LSP) until it has corpus round-trip evidence comparable to the formatter's. The
default build never compiles it nor its sole optional dependency (`roxmltree`).

- Entry point: `croma_core::read_musicxml(xml: &str) -> ParseReport<Score>`
  (`#[cfg(feature = "musicxml-reader")]`).
- Design: [`superpowers/specs/2026-06-15-musicxml-reader-design.md`](superpowers/specs/2026-06-15-musicxml-reader-design.md).
- The writer is the spec. The reader inverts croma's dialect **exactly** and
  never mirrors an abc2xml-ism.

## Verification gate

The primary gate is **XML re-emission idempotence**:
`write(read(write(score))) == write(score)` as exact strings. The writer is
deterministic and lossy, so byte-equality of the re-emission is what proves the
reader recovered every writer-emitted fact (drops nothing, invents nothing) with
no second-spec projection to rot. Each stage adds elements; any element the
writer emits that the reader does not yet read makes the gate red, which *drives*
the next stage's work list.

- **Per-element unit tests** (`crates/croma-core/src/musicxml/read/mod_tests.rs`)
  assert idempotence on the supported subset AND a reconstructed model field
  directly.
- **Corpus measurement** (env-gated by `ABC_ROOT`, mirrors
  `croma-fmt/src/corpus_proof.rs`): walks the 10k corpus, runs the
  write→read→write loop per file, reports the idempotent count (strict full-byte
  and the S1-supported-subset view) and a histogram of the first diverging XML
  tag. It asserts no hard count yet — it reports, because most files use
  later-stage elements.

## Totality

`read_musicxml` is **total and non-panicking**. A malformed document yields a
minimal `Score` plus an error diagnostic; unknown elements are ignored (with an
optional warning). There is no `unwrap`/`expect`/`panic`/`todo` and no index that
can panic in the reader module tree. Unreconstructable `Span`s use the documented
sentinel `READER_SPAN` (= `Span::new(0, 0)`); ABC-only model state not present in
the XML (`preserved_directives`, voice clef/transpose text, `reference`) is left
at documented defaults and is invisible to the gate.

## Staging

| Stage | Scope | Status |
|---|---|---|
| **S1** | `<score-partwise>` → parts → measures → `<note>` (`<pitch>` step/octave/alter, `<rest>`, `<duration>`/`<type>`/`<dot>`, `<accidental>`), `<backup>`/`<forward>`, `<divisions>`, work-title/composer/credit metadata | **done** |
| **S2** | header `<attributes>`: `<divisions>`, `<key>`/`<fifths>` (+ explicit `<key-step>`/`<key-alter>`/`<key-accidental>`), `<time>` (incl. `symbol="common"`/`"cut"`, compound, free), `<clef>`, `<transpose>` → `midi_transpose`. Mid-measure `<attributes>` changes (key/meter/clef) deferred. | **done** |
| S3 | `<score-instrument>` / `<midi-instrument>` → `MidiInstrumentModel` (closes the forward/reverse loop) | planned |
| S4 | ties, slurs, tuplets (`<time-modification>`), articulations/decorations, beams | planned |
| S5 | `<direction>`, `<harmony>`, `<lyric>` | planned |
| S6 | multi-voice (`<voice>`/`<backup>`), repeats/endings/barlines, grace, chords (`<chord/>`) | planned |

### S1 reconstruction notes

- `<duration>` (an integer count of divisions) inverts to a reduced
  `Fraction::new(duration, 4 * divisions)`; `<type>`/`<dot>` are re-derived by
  the writer from that fraction, so reconstructing the rational value is exact
  and sufficient. `<divisions>` is read first because it scales every duration.
- Onsets are rebuilt with the same cursor the writer uses: `<forward>` advances
  it, `<backup>` rewinds it, and each non-chord note sits at the current cursor
  then advances it by its duration.
- A full-measure rest (`<rest measure="yes">`) is reconstructed by setting the
  measure's `expected_duration == actual_duration == rest.duration` at onset 0,
  which is exactly the writer's measure-rest predicate. Ordinary measures leave
  `expected_duration` unset so plain rests stay plain.
- Composer metadata reconstructs **every** `<creator type="composer">`,
  including a present-but-empty one (`<creator type="composer"></creator>`), and
  uses raw (untrimmed) text so the `<identification>` block round-trips byte for
  byte.
- An explicit `<accidental>` reconstructs the note's written accidental
  (`explicit = true`); absent `<accidental>` means none. `<alter>` always sets
  the sounding pitch.

### S2 reconstruction notes

- `<key>` is **score-level**: the writer emits an identical `<key>` in every
  part's first-measure `<attributes>` from `score.metadata.key`, so the reader
  reads the first part's header into `metadata.key` and re-emits it everywhere.
  `<fifths>` → `KeySignatureModel.fifths`; each consecutive
  `<key-step>`/`<key-alter>`/`<key-accidental>` triple → one
  `KeyAccidentalModel` (the `<key-accidental>` name is the exact inverse of
  `Accidental::musicxml_name`, with `<key-alter>` as a fallback). The
  `KeySignatureModel.display` string is **never emitted** by the writer, so it is
  left empty — the idempotence gate confirms `<key>` is fully driven by
  `fifths` + `explicit_accidentals`.
- `<time>` is score-level too. `symbol="common"` → `display = "C"`,
  `symbol="cut"` → `"C|"`; otherwise the `<beats>`/`<beat-type>` pairs are
  reassembled into `display` (joined with `+` when compound, e.g. `"3/8+2/8"`).
  An **absent** `<time>` (free meter `M:none`, or no meter) leaves
  `metadata.meter = None`; both `None` and a free meter re-emit nothing, so this
  is idempotent. `MeterModel.display` is the only field the writer reads;
  `duration`/`free_meter` get documented defaults.
- `<clef>` is **per-staff**: the writer reads the staff voice's
  `initial_properties.clef` (ABC text). The reader rebuilds a *canonical* clef
  text from `<sign>`/`<line>`/`<clef-octave-change>` that `clef_model` re-maps to
  the same element (`clef_model` is many-to-one, so a representative per
  `(sign,line)` plus an octave suffix `+8`/`-8`/`+15`/`-15` is sufficient).
  Plain treble (G/2, no octave change) reconstructs as `None`, matching a freshly
  lowered score (the writer emits the default `<clef>` either way). The original
  ABC clef string is unrecoverable but irrelevant to the gate.
- `<transpose><chromatic>n` → `voice.midi_transpose = Some(n)`. The ABC
  `transpose=` voice property is not in the XML; on re-write the writer falls
  through to `midi_transpose`, reproducing the element. Scoped per part (the
  writer emits one `<transpose>` per part from the first qualifying voice; S2
  reconstructs a single voice per part).
- **Deferred:** mid-measure `<attributes>` (a second `<attributes>` block from a
  `KeyChange`/`MeterChange`/`ClefChange` event) is **not** reconstructed in S2.
  Doing so requires synthesising the right `TimedEvent` at the exact onset routed
  through the writer's measure-sequence/overlay timeline; it is tracked as
  remaining (≈100 corpus files; first-diverging tag `attributes`).

### S1/S2 corpus metric (10k zenodo set)

- **S1** strict full-byte idempotence was **0 / 9,935** exported files (every ABC
  tune carries a `K:`, so a `<key>` block S1 did not read always diverged first);
  the S1-supported-subset view (deferred `<key>`/`<time>`/`<score-instrument>`/
  `<midi-instrument>` stripped) was **483 / 9,935**.
- **S2** strict full-byte idempotence: **483 / 9,935** — S2 clears the `key`
  divergence entirely (the strict count now equals the old S1-supported-subset
  number, confirming S2 reconstructs the whole header `<attributes>` for every
  file previously blocked only by key/time). The new top first-diverging tags are
  `barline` (~3.3k, S6), `direction` (~3.3k, S5), `notations` (~1.2k, S4),
  `harmony` (~0.7k, S5), `score-instrument` (~426, S3), then `lyric`, `tie`,
  `grace`, and `attributes` (~100, deferred mid-measure changes). These name the
  next stages' work lists.

Re-run the measurement with:

```sh
ABC_ROOT=docs/untracked/corpus/zenodo-10k/abc \
  cargo test -p croma-core --release --features musicxml-reader \
  corpus_idempotence_measurement -- --nocapture
```
