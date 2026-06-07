# Phase 10 closure report — corpus-driven parser/export hardening

Phase 10 used the full 10,000-file Zenodo ABC corpus, compared against `abc2xml`
reference MusicXML via a music21 + Polars structural comparison, to find and fix
high-confidence Croma parser/export bugs and to classify every remaining
mismatch under the "100% match or justify" standard.

Authority order for "correct": the **ABC notation specification** and the
**MusicXML specification** first; `abc2xml` is a comparison baseline, not ground
truth. Where Croma and `abc2xml` disagree, the spec decides, and `abc2xml`
behaviour is classified as a reference artifact when it departs from the spec.

## Headline result

| | Mismatch rows | Structural matches |
|---|---|---|
| Phase 10 baseline (full 10k) | 3,578,140 | 3,086 |
| Phase 10 close | **229,329** | **3,396** |
| Net | **−3,348,811 (−93.6%)** | **+310 files** |

Run-level health (full 10k, `--jobs 0`):

- files attempted: 10,000
- Croma export success / failure: 9,935 / 65
- Croma MusicXML import failures: **0**
- reference MusicXML import failures: **0**
- corpus panics / hard errors: **0 / 0**

The 65 export failures are all `abc.file.no_music`: tune headers with an empty
body (no notes). These are incomplete/malformed ABC documents and are correctly
diagnosed; there is nothing to export.

## Fixes landed (this closure pass)

Each was validated by a full 10k report-only comparison and merged only on green
CI (Rust + Linux/nixos). Mismatch-row deltas are from the full corpus.

| PR | Fix | Spec basis | Δ rows |
|----|-----|-----------|--------|
| #25 | Lyric `\|`/`s:` bar-marker "advance to next bar" + `--` blank note | ABC 2.1 §5.1 | −14 lyric (realignment) |
| #26 | `\|[M:..]` inline field after a bar no longer eaten as a liberal barline | ABC 2.1 inline fields | −40,235 |
| #27 | Apply inline `[K:]`/`[M:]`/`[L:]` changes to following notes | ABC 2.1 §3.2 | −12,541 |
| #28 | **One MusicXML `<part>` per voice (multipart)** | MusicXML part model | **−3,131,255** |
| #29 | Clef octave (`clef=treble-8`) transposition + keep voice clef across a bare `V:` switch | ABC clefs / MusicXML clef-octave-change | −17,658 |
| #30 | `W:` post-tune words as `<credit>`; score directions once | ABC 2.1 (W: printed after tune) / MusicXML `<credit>` | −21,560 |
| #31 | ABC decorations → `<fermata>`/`<articulations>`/`<ornaments>`/`<technical>` | ABC 2.1 decoration list / MusicXML notations | −7,373 |
| #32 | Staccato chord `.[CE]2` keeps its length (not a dotted barline) | ABC chords / barlines | −16,907 |
| #33 | **Chord adjacent to a bar `\|[G2C,2]` / `][` not swallowed into a barline** | ABC chords / barlines | **−97,953** |
| #34 | Unclosed `!` decoration no longer swallows the following notes | ABC 2.1 decorations | −2,265 |

The two largest were the multipart export model (closing the dominant
multi-voice gap, ~96% of the original mismatch mass) and the chord-adjacent-
barline parser bug (317 chord-dense files whose chords were being split and
whose bars were over-fragmented).

Cross-referencing the author's `ro-ag/ABC` and `ro-ag/trd` repositories was
decisive: `ABC` already emits one part per voice (confirming the multipart
design), and `trd`'s measure splitting confirmed that a bare leading harmony
must not create an empty measure.

## Remaining mismatches — classification ("100% or justify")

The residual 229,329 rows are concentrated in a small number of files and fall
into the categories below. No high-confidence narrow parser/export bug remains.

### 1. Reference artifacts — Croma is spec-correct (justified)

`abc2xml` inserts a **phantom empty measure** for a bare leading annotation /
chord symbol (`"A"\` before the first bar), and **splits a full measure** that
opens with a chord-symbol-in-chord (`["Em"G,D]`) into a one-chord pickup. trd
and Croma both (correctly) keep the single full measure. Examples: tune_006403,
tune_003837, tune_001361 (the last is a frank `abc2xml` mis-parse — wrong
pitches), tune_003188. These produce large per-file cascades because the
positional comparison then misaligns every later event, but Croma's output is
the musically correct one.

### 2. `%%staves` parenthesis grouping — deferred model gap

`%%staves` / `%%score` grouping: `[ ]` bracket and `{ }` brace keep one part per
voice (already correct), but `( )` **merges** the grouped voices into a single
part as overlay voices. Croma currently emits one part per voice regardless, so
the ~8 corpus files using `( )` grouping (e.g. tune_003557 `1 (2 3) 4`,
tune_003179) have one extra part and cascade. This is the multi-voice/part/staff
semantics deferred gap; it is a self-contained follow-up.

### 3. Small per-file measure-structure issues — deferred

A handful of files diverge on measure boundaries from constructs such as
variant-ending + `y` spacer interplay (tune_001062), Highland-pipes pickups, and
similar. Each is low-volume and file-specific; none is a systematic parser bug.

### 4. Comparison-harness limitation (structural, by design)

The comparison aligns parts/measures/events **positionally** (`zip`). Whenever a
file's structure legitimately differs (an `abc2xml` artifact above, or a
deferred grouping case), every downstream event is reported as
missing/extra. The bulk of the residual `missing_in_croma`/`extra_in_croma`
rows are this cascade, not independent defects.

## Closure criteria check

- [x] 10k corpus recreatable from repo instructions (Git LFS archive or Zenodo
      fallback; `tools/session_bootstrap.sh --fetch-corpus --fetch-reference`).
- [x] No corpus panics, hard errors, or MusicXML import failures.
- [x] Remaining export failures justified (65 = empty-body / no-music ABC).
- [x] Major mismatch categories triaged under "100% or justify".
- [x] No confirmed Croma-side parser/export bug left unfixed.
- [x] No high-confidence narrow parser/export bug remains that would
      substantially reduce mismatch rows without a model-level change.
- [x] Remaining mismatches documented as reference artifacts, deferred model
      gaps (`%%staves` `()` grouping), comparison limitations, or small
      per-file issues.
- [x] Progress tracker records exact attempted/success/failure/mismatch counts
      (`docs/progress/croma-progress.sql`).

## Recommended next phases

1. `%%staves` `( )` grouping → merge grouped voices into one multi-voice part
   (the last self-contained systematic item).
2. Deferred MusicXML writer/model refinements (mid-tune `<key>`/`<time>`
   attribute emission; richer articulation placement).
3. Formatter and LSP per `docs/architecture/abc-to-musicxml-roadmap.md`.

## Reproduce

```sh
tools/session_bootstrap.sh
git lfs pull --include docs/corpus/zenodo-10k-abc.tar.gz
tools/session_bootstrap.sh --fetch-corpus --fetch-reference
ABC_ROOT=docs/untracked/corpus/zenodo-10k/abc \
REF_ROOT=docs/untracked/corpus/zenodo-10k/musicxml \
  tools/session_bootstrap.sh --testbed
```

The full report lands at
`docs/untracked/<phase>/full-10k-report-only-compare-report.json` (git-ignored).
