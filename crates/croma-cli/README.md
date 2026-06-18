# croma-cli

`croma-cli` is the command-line interface for [Croma](https://github.com/ro-ag/croma),
a Rust-first toolkit for ABC notation. It installs the `croma` binary, a thin
wrapper over the `croma-core` library API and the `croma-fmt` formatter.

The CLI exposes the toolkit's core capabilities:

- `croma xml` — export ABC to MusicXML.
- `croma fmt` (with `--auto-fix`) — format and sanitize ABC source.
- `croma read` / `croma musicxml2abc` — read MusicXML back into ABC, inverting
  croma's own writer and accepting foreign MusicXML (abc2xml, MuseScore, Finale,
  Sibelius).

```sh
cargo install croma-cli
croma xml examples/basic.abc
```

The MusicXML reader is enabled by default; build a reader-less, dependency-free
CLI with `--no-default-features`. See the
[project README](https://github.com/ro-ag/croma) for the full workflow.
