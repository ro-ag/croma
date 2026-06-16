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
| S2 | `<divisions>`, `<key>`/`<fifths>`, `<time>`, `<clef>`, `<transpose>` → `midi_transpose` | planned |
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

### S1 corpus metric (10k zenodo set)

- Strict full-byte idempotence: **0 / 9,935** exported files — expected, because
  every ABC tune carries a `K:`, so the writer always emits a `<key>` block that
  S1 does not read yet (S2). The histogram confirms it: the dominant
  first-diverging tag is `key` (~9.5k), then `score-instrument` (S3, ~426).
- S1-supported-subset idempotence (deferred S2 `<key>`/`<time>` and S3
  `<score-instrument>`/`<midi-instrument>` blocks stripped from both sides):
  **483 / 9,935** — the files S1 fully reconstructs modulo the deferred stages.
  The remaining files diverge on other later-stage elements (notations,
  directions, lyrics, multi-voice, chords).

Re-run the measurement with:

```sh
ABC_ROOT=docs/untracked/corpus/zenodo-10k/abc \
  cargo test -p croma-core --release --features musicxml-reader \
  corpus_idempotence_measurement -- --nocapture
```
