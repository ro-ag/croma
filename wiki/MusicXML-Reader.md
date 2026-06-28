# MusicXML → ABC reader

The reader is the reverse direction: MusicXML → croma's `Score` → ABC (or
re-emitted MusicXML). It **inverts croma's own writer** and reads foreign
MusicXML dialects (abc2xml, MuseScore, Finale, Sibelius). It is a **proven,
un-gated** capability and ships in the default CLI build.

```sh
croma read score.musicxml --format abc    # MusicXML -> ABC
croma read score.musicxml --format xml    # re-emit MusicXML (the writer's inverse)
croma read score.musicxml --format dump   # debug-print the reconstructed Score
croma musicxml2abc score.musicxml         # = read --format abc
```

## Evidence

| Gate | Result (10k corpus) |
| --- | --- |
| Self-loop XML re-emission (`write_musicxml(read_musicxml(xml))`) | **9,935 / 9,935** |
| Foreign-dialect parity vs music21 | **98.50%** |

The writer is the spec: the reader inverts croma's dialect exactly and never
mirrors an abc2xml-ism. Foreign dialects are read gracefully (with diagnostics),
never mimicked back into the writer.

## Library vs CLI: where the dependency lives

The reader's **only** dependency is `roxmltree`, and it is opt-in:

- **`croma-core` (library)** — the reader is behind the `musicxml-reader`
  feature, so the **default library build stays zero-dependency and
  crates.io-publishable**. Enable it with
  `cargo add croma-core --features musicxml-reader`, then call
  `read_musicxml(xml) -> ParseReport<Score>` ([[Library-Usage#enabling-the-musicxml-reader]]).
- **`croma-cli` (binary)** — the reader is **promoted (un-gated)** and on by
  default, so `roxmltree` reaches the CLI binary. A
  `cargo build -p croma-cli --no-default-features` build omits the reader, the
  two subcommands, and the dependency.

## Full reference

Reader coverage, the foreign-dialect policy, the ABC-projection completion pass
(`complete_score_for_abc`), the reader → ABC structural round-trip (**97.9%**,
9,724 / 9,933 in-scope), and the adjudicated residual are documented in detail
in
[**`docs/musicxml-reader.md`**](https://github.com/ro-ag/croma/blob/main/docs/musicxml-reader.md).

See also: [[CLI-Usage]] · [[How-its-Proven]] · [[Conversion-Challenges]] (the
lossy-intermediate round-trip cases and the deferred items).
