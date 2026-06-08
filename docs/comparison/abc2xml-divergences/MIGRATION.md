# Migrating from abc2xml to Croma

This is the evidence-based case for moving an ABC → MusicXML pipeline from
`abc2xml` to Croma. Every claim below is backed by the per-file
[`per-file-manifest.csv`](per-file-manifest.csv) over the public 10,000-tune
Zenodo corpus and is reproducible with
[`tools/prove_divergences.py`](../../../tools/prove_divergences.py).

## 1. Migration is low-risk: the notes are the same

Across 10,000 real-world tunes, Croma's note content is **identical to abc2xml in
9,738 files (97.4%)** — 7,031 byte-for-byte exact, plus 2,707 more whose ordered
pitch sequence (step + alter + octave) matches exactly. The prover verifies this by
comparing the actual extracted notes, not a checksum. So for the overwhelming
majority of a library, switching engines changes nothing musical.

## 2. Where they differ, Croma is the more correct engine

Of the 2,969 files that differ at all, **2,929 are `croma_correct = yes`** and the
remaining **40 are `defensible`** — **zero are a Croma defect**. The differences are
cases where Croma follows the ABC 2.1 / MusicXML spec and abc2xml does not:

| What abc2xml does | What Croma does (correct) | Files | Doc |
|---|---|--:|---|
| Hardcodes `L:1/8`; rounds durations to its `<divisions>` grid | Applies the §4.6 meter-derived default unit length; keeps durations exact (e.g. `F////` = 1/128, tuplet `3/64`) | 104 | [06](06-duration.md) |
| Inserts a phantom empty `<measure>` for a bare annotation / section label / inline key | Treats a bar line / annotation as a boundary, not a measure of music (§2.2.1, §4.8) | 240 | [02](02-phantom-measures.md) |
| Drops a music line / silently parse-fails on some tunes | Parses and exports the music | 4 | [02](02-phantom-measures.md) |
| Drops multi-voice **tacet** (silent) bars, misaligning the voice | Keeps tacet bars so every voice stays measure-aligned (§4.8) | 2 | [11](11-multipart-and-partgroup.md) |
| Re-serializes a redundant `<alter>0>` on a natural carried through the bar | Prints the accidental once (§6); semantically identical | 577 | [05](05-accidental-alter.md) |
| Renders a whitespace-separated `\| \|` / line-split run as a double bar | Emits a plain bar line — §4.8 liberal recognition is for *contiguous* runs, §4.9 makes spaces significant | 389 | [04](04-barline-spaced-double.md) |
| Expands a `Z`/`X` multi-measure rest into N measures | Keeps one measure; §4.5 calls the forms "musically equivalent" | 6 | [03](03-multi-measure-rest.md) |

(The remaining differing files are `POSITIONAL_CASCADE` / `CASCADE`: the notes are
identical and only the positional comparison alignment shifts — no real difference.)

## 3. Additional engine advantages

- **One `<part>` per voice (multipart).** Croma emits the spec-faithful
  part-per-voice content model; verified identical part counts to abc2xml on every
  multi-voice file, with the correct per-voice content. [11](11-multipart-and-partgroup.md)
- **Precise, spec-cited diagnostics.** Croma reports *why* a construct is malformed
  or unsupported (e.g. `abc.file.no_music`, `abc.music.unclosed_chord`) with source
  spans, instead of silently fabricating output. On the 65 header-only tunes Croma
  declines to emit an empty score; abc2xml fabricates an empty measure. [01](01-export-failures.md)
- **A maintained Rust library.** `croma-core` is a crates.io-publishable library
  (not a script), fast, with no runtime/path assumptions — embeddable in tools,
  servers, and the forthcoming formatter / LSP.
- **A proven test corpus.** The full 10k differential against abc2xml is part of the
  project's regression process; every parser change is measured against it.

## 4. Honest caveats

These are the only places a migrating user might notice a *behavioural* (not
correctness) change, all `defensible` and none a Croma bug:

- **Default playback tempo for a text-only `Q:`** (e.g. `Q:"Allegretto"`): the spec
  gives no BPM, so Croma and abc2xml pick different defaults. (24 files) [10](10-directions.md)
- **Tie/slur edge cases** on malformed or single-note input (§4.11). (16 files) [09](09-ties-and-slurs.md)
- **Bar-line *style* glyphs** (`light-heavy` on a repeat/volta, `light-light` on
  `| |`): Croma emits the `<repeat>`/`<ending>` semantics but omits the extra
  decorative `<bar-style>` glyph abc2xml adds. The music and structure are identical;
  if exact glyph parity matters for a downstream renderer, that is a candidate
  formatter option. (389 files) [04](04-barline-spaced-double.md)

## 5. Verify it yourself

```sh
# Build the 10k testbed (corpus is external; see docs/testing/corpus-reproducibility.md)
PHASE=audit ABC_ROOT=…/zenodo-10k/abc REF_ROOT=…/zenodo-10k/musicxml \
  tools/session_bootstrap.sh --testbed
# Re-derive every per-file verdict + the genuine-issue count (must be 0)
uv run python tools/prove_divergences.py --phase-dir docs/untracked/audit \
  --abc-root docs/untracked/corpus/zenodo-10k/abc \
  --ref-root docs/untracked/corpus/zenodo-10k/musicxml \
  --out /tmp/manifest.csv
# Spot-check any single tune
target/debug/croma xml path/to/tune.abc
```

**Bottom line:** migrating keeps your notes identical 97.4% of the time, and in the
cases that differ Croma is the engine that follows the spec.
