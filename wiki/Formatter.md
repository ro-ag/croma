# Formatter (`croma fmt`)

`croma fmt` is a canonical formatter for ABC 2.1 source, in the style of
`rustfmt` / `gofmt`. It is a **proven, un-gated** capability: idempotent and
lossless over the full 10,000-file corpus (**10,000 / 10,000** — see
[[How-its-Proven]]).

```sh
croma fmt FILE             # print canonical formatting to stdout
croma fmt --check FILE     # exit non-zero if FILE is not already canonical
croma fmt --write FILE     # rewrite FILE in place
croma fmt --auto-fix FILE  # additionally apply safe, gated repairs to loose source
```

## Two modes

- **`format`** (plain `croma fmt`) — **lossless by construction.** Musical
  tokens are copied verbatim by source byte span; only the whitespace *between*
  tokens, blank-line runs, and the final newline are normalized. Information
  fields and stylesheet directives stay byte-stable (trailing whitespace
  trimmed). Because no musical byte is reconstructed, formatting cannot change
  beaming or pitch.
- **`auto_fix`** (`--auto-fix`) — additionally applies a catalogue of **safe
  curations** that sanitise *loose* source into the canonical spelling the
  strict parser reads cleanly (detached note lengths, redundant/malformed
  barlines, field spacing, tempo normalisations, `%%MIDI` whitespace). This is
  the designated home for repairs the strict parser defers. Every curation is
  **runtime-verified by a safety gate** and reverted (reported as `skipped`) if
  it would change the score.

## Invariants

Two invariants are the formatter's contract — unit-tested and proven over the
external corpus:

- **Idempotent** — `format(format(x)) == format(x)`; `auto_fix` output is itself
  a `format` fixed point.
- **Lossless** — plain `format` renders **byte-identical MusicXML**; `auto_fix`
  preserves the **ordered pitch sequence** (step + alter + octave).

This pairs with the strict parser: the parser strict-**rejects** a malformation;
the formatter **repairs** it. The two together are croma's reject → repair →
recover pipeline.

## Full reference

The complete catalogue (each `FixKind`, its example, its safety gate, and its
ABC 2.1 citation), the safety-gate table, the `%%MIDI` scope and limits, and the
corpus-proof harness are documented in
[**`docs/formatter.md`**](https://github.com/ro-ag/croma/blob/main/docs/formatter.md).

See also: [[CLI-Usage]] ·
[[Language-Server]] (the LSP exposes formatting and a `source.fixAll` code
action backed by the same `croma-fmt` functions).
