# The croma formatter (`croma fmt`)

`croma fmt` is a canonical formatter for ABC 2.1 source, in the style of
`rustfmt`/`gofmt`. It is a **proven, un-gated** feature: parser quality is
established (raw whitelist 9,390 / dropped 545 / worklist 0) and the formatter is
idempotent and lossless over the full 10k corpus (see [Invariants](#invariants)).

```sh
croma fmt FILE            # print canonical formatting to stdout
croma fmt --check FILE    # exit 1 (and report) if FILE is not already canonical
croma fmt --write FILE    # rewrite FILE in place
croma fmt --auto-fix FILE # additionally apply safe, gated curations of loose source
```

`--check` and `--write` are mutually exclusive.

## Two modes

- **`format`** (plain `croma fmt`) — canonical, **lossless by construction**.
  Musical tokens are copied verbatim by source byte span; only the whitespace
  *between* tokens, blank-line runs, and the final newline are normalized.
  Information-field values and stylesheet directives are kept byte-stable (only
  trailing whitespace is trimmed). Because no musical byte is reconstructed,
  collapsing a run of spaces to one space cannot change beaming or pitch.

- **`auto_fix`** (`--auto-fix`) — additionally applies a catalogue of safe
  curations that sanitise *loose* source into the canonical spelling the strict
  parser reads cleanly. This is the designated home for repairs the strict
  parser defers (the three-tier recovery policy in
  [`AGENTS.md`](../AGENTS.md)): the parser strict-rejects a malformation; the
  formatter rewrites it. Every curation is **runtime-verified by a safety gate**
  and reverted (reported as `skipped`) if it would change the score.

## Invariants

Two invariants are the formatter's contract, exercised in unit tests and proven
over the external 10k corpus:

- **Idempotent** — `format(format(x)) == format(x)` for all `x`; `auto_fix`
  output is itself a `format` fixed point.
- **Lossless** — plain `format` renders **byte-identical MusicXML**;
  `auto_fix` preserves the **ordered pitch sequence** (step+alter+octave).

### The corpus proof

`crates/croma-fmt/src/corpus_proof.rs` is an env-gated, **in-process** test that
asserts all four invariant checks over every corpus file, reusing the crate's
own gate machinery (`verify::pitch_seq_of` / `musicxml_of`). It is skipped by a
normal `cargo test` (no corpus) and runs in ~20 s when pointed at the corpus:

```sh
ABC_ROOT=$PWD/docs/untracked/corpus/zenodo-10k/abc \
  cargo test -p croma-fmt --release corpus_proof -- --nocapture
# corpus formatter proof: 10000 files, 0 violations
```

A `>= 9000` file-count guard rejects a vacuous run (mis-set `ABC_ROOT`).
`tools/prove_fmt_lossless.py` is the complementary **black-box** proof — it
drives the built binary over the same corpus and writes a JSON report. The two
are independent (in-process gate reuse vs. binary + regex pitch-seq) and agree:
`10000 files, 0 notes_changed / 0 not_idempotent / 0 canonical_xml_changed`.

## Safety gates

Every `--auto-fix` curation declares the gate it must clear (`FixKind::gate()`).
A candidate edit is applied to a trial string, re-checked, and kept only if its
gate holds; otherwise it is reverted and listed in `skipped`.

| Gate | Invariant enforced | Used by |
|---|---|---|
| `Pitch` | the ordered pitch sequence is unchanged (the edit may legitimately change the render, e.g. restoring a dropped duration/tempo) | `DetachedLength`, `ChordSymbolInBrackets`, `DoubledTempo`, `BareTempoSuffix` |
| `Structure` | the full MusicXML rendering is byte-identical (no rendered aspect changes) | `RedundantBarline`, `FieldSpacing` |
| `DirectiveTokens` | only whitespace inside active `%%MIDI` argument regions moves — no directive token, comment, or other line changes | `MidiDirectiveSpacing` |

`DirectiveTokens` exists because `%%MIDI` is **not rendered into MusicXML**, so
neither the pitch nor the structure gate can constrain it; the invariant is a
textual one over the directive lines.

## The `--auto-fix` catalogue

Each curation cites ABC 2.1 (canonical forms are spec-grounded, never
abc2xml-isms). The one exception, `%%MIDI`, is flagged below.

| `FixKind` | Example | Gate | Reference |
|---|---|---|---|
| `DetachedLength` | `g 2` → `g2` | `Pitch` | ABC 2.1 §4.3 (note length) |
| `ChordSymbolInBrackets` | `["C"abc]` → `"C"abc` | `Pitch` | ABC 2.1 §4.18 / §4.19 |
| `DoubledTempo` | `Q:1/4=1/4=160` → `Q:1/4=160` | `Pitch` | ABC 2.1 §3.1.8 |
| `BareTempoSuffix` | `Q:320s` → `Q:320`, `Q:400.` → `Q:400` | `Pitch` | ABC 2.1 §3.1.8 / §10.1 |
| `RedundantBarline` | `\| \|` → `\|`, `]\|\|:` → `\|]:` | `Structure` | ABC 2.1 §4.8 |
| `FieldSpacing` | `K: C` → `K:C` | `Structure` | ABC 2.1 §3 (field notation) |
| `MidiDirectiveSpacing` | `%%MIDI beat 97 87  77 4` → `…87 77 4` | `DirectiveTokens` | abc2midi convention (see below) |

### `BareTempoSuffix` and the reject → repair → recover pipeline

`BareTempoSuffix` is the formatter's pairing for the one strict-reject the parser
defers. ABC 2.1 §10.1's deprecated bare tempo is a bare *integer*; `Q:320s`
(legacy suffix) and `Q:400.` (decimal tail) are outside the grammar, so the
strict parser rejects them to a verbatim `<words>320s</words>` rather than a
metronome. `--auto-fix` rewrites the field to its canonical integer, and the
score recovers `<per-minute>320</per-minute>`. This is the only place a corpus
`dropped.csv` adjudication names `croma fmt --auto-fix` as the recovery path
(`tune_001192.abc`, `tune_009608.abc`); see `fmt_first_demo.rs`. The raw
comparison axis still sees the strict-correct reject — the parser is not
weakened; the formatter recovers the loose source.

### `%%MIDI` scope and limits

`MidiDirectiveSpacing` collapses internal whitespace runs inside the **argument
region** of an active (column-0) `%%MIDI` directive, e.g.
`%%MIDI beat 97 87  77 4` → `%%MIDI beat 97 87 77 4`. The comment tail is
preserved verbatim.

`%%MIDI` is an **abc2midi convention, not ABC 2.1** — there is no spec citation
for a canonical form, and the canonical spelling follows abc2midi's whitespace
tokenization. croma renders no MusicXML for `%%MIDI`, so the `DirectiveTokens`
gate (a textual invariant) is its only protection.

Deliberately **out of scope** (all would change abc2midi playback, so they are
not lossless and there is no render gate to catch a mistake):

- **Promotion** of an inert mid-line `%%MIDI` tail (e.g. `K:C %%MIDI gchordon`,
  282 corpus files) to a column-0 active directive — inert → active is a
  semantic change.
- **Relocation / reordering** of directives — per-voice scoping makes a
  directive's position relative to `V:`/`K:` lines load-bearing.
- **Score translation** of `%%MIDI program`/`channel`/`transpose` into
  `<midi-instrument>`/`<transpose>` — a separate parser/exporter epic, still
  deferred.

## Layout

`crates/croma-fmt/`:

- `engine.rs` — the canonical, token-preserving `format`.
- `fixes.rs` — the `--auto-fix` curation detectors and the gate dispatch.
- `verify.rs` — the pitch-sequence and MusicXML-equality gate machinery.
- `lib.rs` — the public API (`format`, `auto_fix`, `FixKind`, `FixResult`).
- `corpus_proof.rs`, `fmt_first_demo.rs` — the proof and demo described above.
