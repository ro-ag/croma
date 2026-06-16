# `%%MIDI` directive handling

`%%MIDI` is an **abc2midi convention, not part of ABC 2.1** (ABC 2.1 §11 calls
`%%` lines stylesheet directives and lets a reader ignore ones it does not
recognise). croma treats `%%MIDI` in two parallel, independent paths:

1. **Preserve verbatim — always.** Every line-start `%%MIDI …` is parsed as a
   stylesheet directive and stored as
   `PreservedDirective { name: "MIDI", value: "<args>" }` in
   `Score.metadata.preserved_directives`. The ABC writer (`to_abc.rs`) re-emits
   it unchanged, so `croma fmt` round-trips `%%MIDI` lines in place. Parsing also
   emits `warning[abc.directive.unsupported]`. **This path is never removed** —
   it is what keeps croma lossless over the corpus and is the formatter fixed
   point.

2. **Forward-translate the score-meaningful parts — into MusicXML only.** The
   MusicXML exporter additionally projects the sub-directives that carry genuine
   score-level meaning into MusicXML elements. This is a forward-only path; it
   does **not** replace path 1 and does not touch the ABC writer.

The two paths are deliberately decoupled: the model keeps the raw directives for
round-trip, and a separate projection feeds the MusicXML exporter.

## Spec discipline

abc2xml (v268) is the corpus comparison **baseline, not an authority**. Several
of its `%%MIDI` behaviours are incidental or buggy (see
`docs/untracked/phase-33/midi/oracle-probe.md`): silently dropping a header
`program` placed before the `V:` definitions, attributing such a program to the
last-declared voice, crashing on a bare `%%MIDI channel 10`, and writing a
literal `<instrument-name>no name</instrument-name>` filler. croma follows
genuine abc2midi/MusicXML semantics and does **not** mimic those quirks.

## What translates today

| Sub-directive | MusicXML | Notes |
|---|---|---|
| `%%MIDI program <prog>` | `<part-list>` `<score-instrument><instrument-name>` + `<midi-instrument><midi-program>` | `<midi-program>` is **1-based** (`prog + 1`); abc2midi/GM programs are 0-based. Instrument name = General MIDI Level 1 name (`GM_PROGRAM_NAMES`, abc2xml `inst_tb` spelling). |
| `%%MIDI program <chan> <prog>` | adds `<midi-channel>` | The two-integer form (the dominant multi-voice corpus shape). |
| `%%MIDI channel <n>` *alongside a program* in the same voice | adds `<midi-channel>` | Merged into the voice's one instrument. |
| `%%MIDI transpose <n>` | `<attributes><transpose><chromatic>n` | A signed semitone shift declaring written-vs-sounding pitch; emitted in the scoped part's first-measure `<attributes>` and does **not** shift the written notes. The ABC `transpose=` voice property (ABC 2.1) takes precedence when both are present. `n` parsed as `i16`. |

**Per-voice scoping is load-bearing** (`project_voice_midi` in `lower/mod.rs`): a
directive attaches to the voice of the nearest preceding `V:` declaration
(header *or* body, by source position), or the first/default voice if it
precedes every `V:`. One MusicXML part = one voice in the common case, so each
voice's instrument lands in its own `<score-part>`. Argument parsing is lenient
(matching abc2midi): a trailing `% comment` and any non-numeric trailing words
(e.g. `program 74 recorder`) are ignored; the leading integer run is taken;
out-of-range values are skipped.

## What stays preserved-only (no MusicXML translation)

- **Playback-only sub-directives** — `nobarlines`, `ratio`, `gchord`,
  `chordprog`, `bassprog`, `beat`, `chordvol`, `bassvol`, `chordname`,
  `tuningsystem`, `drum`/`drumon`/`drumoff`, `grace`, `gracedivider`,
  `gchordoff`, `beatstring`, `trim`, … These have no score-level notation
  meaning (abc2xml emits nothing for them either); they are kept verbatim.

## Deliberately deferred / not mimicked (with rationale)

| Item | Status | Why |
|---|---|---|
| `%%MIDI channel <n>` with **no** program | deferred | No instrument identity ⇒ no GM name; MusicXML `<score-instrument>` requires an `<instrument-name>`. abc2xml writes `no name`, which the oracle probe flags as not-to-imitate. 12 corpus files; comparator-invisible. |
| Inline `[I: MIDI=program N]` | deferred | Currently dropped with `warning[abc.field.inline_ignored]` (a round-trip loss distinct from the line-start form). 5 occurrences in 2 files. |
| Mid-tune program change → visible `<words>prog: N</words>` | **not emitted** | abc2xml renders a raw MIDI program number as visible staff text; this is an abc2xml-ism, not standard notation. The affected corpus files are already adjudicated as `dropped.csv` "equivalence" (croma correctly ignores the optional directive). croma emits the part-list instrument identity but not the visible `prog:` words. |
| `%%MIDI drummap` percussion | **not mimicked** | ABC 2.1 standardises only the drum clef and has no special percussion notes; abc2xml's `<midi-unpitched>`/`x`-notehead percussion is a non-standard extension (`dropped.csv` `abc2xml-percussion-extension`). 1 corpus file. |

## Verification & a known gap

The raw structural comparator (`tools/music21_polars_corpus_compare.py`)
extracts **no** instrument / program / channel facts from music21, and it
compares **written** pitch (`note.pitch.alter` is the accidental alteration; it
never calls `.toSoundingPitch()`), so `<transpose>` is invisible to it too. It
compares part counts, measures, written notes/pitch, voices, barlines, slurs,
repeat endings, harmony, and directions. So **every** translation above is
**comparator-invisible**: each produces **0 whitelist graduations and 0
regressions** by construction.

This was verified over the full 10k for both changes (9390 matches, 0
mismatches, whitelist set-diff = 0 each). The transpose case is also confirmed
empirically: 56 of the 60 `%%MIDI transpose` corpus files were **already
whitelisted** with croma emitting *no* `<transpose>` while abc2xml emitted it —
which only holds if the comparator ignores `<transpose>` entirely (the other 4
are dropped for unrelated `duration` reasons). The prompt anticipated transpose
as a risky, comparator-visible "graduation lever"; the evidence shows it is
neither risky nor a lever. Correctness is therefore verified by targeted
`croma-core` unit tests plus byte-level parity spot-checks against abc2xml, not
by the whitelist — these are output-fidelity features, not match-rate movers.

**Projection-coverage gap (writer side):** the ABC writer re-emits `%%MIDI`
verbatim from `preserved_directives`; it has no inverse of the MusicXML
projection (it does not read `Voice::midi_instrument`). A MusicXML→Score reader
that imports `<midi-instrument>` back into the model is a separate epic and is
out of scope here.

## References

- Corpus census: `docs/untracked/phase-33/midi/inventory.md`
- abc2xml oracle probe + per-sub-directive verdicts:
  `docs/untracked/phase-33/midi/oracle-probe.md`
- Spec-is-driver policy:
  `docs/comparison/abc2xml-divergences/README.md`
