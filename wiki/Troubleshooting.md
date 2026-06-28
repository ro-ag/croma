# Troubleshooting

## Reading diagnostics

Run `croma check FILE` to see diagnostics without producing output. Each
diagnostic has a **code**, a **severity**, a **1-based line:column span**, and a
**byte span** into the source:

```text
$ croma check nokey.abc
nokey.abc:3:1-3:1: error[abc.file.missing_k]: ABC source is missing a K: field
  byte span 13..13
  |
  | ^
```

- `nokey.abc:3:1-3:1` — file, then `startLine:startCol-endLine:endCol` (1-based).
- `error[abc.file.missing_k]` — the **severity** and the dotted **code**. Codes
  are stable identifiers (e.g. `abc.file.empty`, `abc.file.missing_k`) you can
  grep for or match on.
- `byte span 13..13` — the half-open byte range, which lines up exactly with
  `croma-core`'s `Span` and the LSP's UTF-8 ranges.

For tooling, use JSON — it carries the same fields structurally:

```sh
croma check --diagnostics=json FILE
```

```json
[
  {
    "code": "abc.file.missing_k",
    "line_span": {
      "end": { "column": 1, "line": 3 },
      "start": { "column": 1, "line": 3 }
    },
    "message": "ABC source is missing a K: field",
    "path": "nokey.abc",
    "severity": "error",
    "snippet": { "line": "", "marker": "^" },
    "span": { "end": 13, "start": 13 }
  }
]
```

`--diagnostics <text|json>` and `--warnings-as-errors` are available on `xml`,
`check`, `fmt`, and `dump` ([[CLI-Usage#parse-modes-shared-flags]]).

### "It rejected my file" → try the formatter

croma is **strict by design**. If a file is rejected for a trivial loose-source
issue (detached note length, redundant barline, spaced field, a legacy tempo
suffix), run the formatter's repair pass first:

```sh
croma fmt --auto-fix FILE     # sanitise loose source into canonical spelling
```

Every `--auto-fix` curation is pitch/structure-gated and reverted if it would
change the score ([[Formatter]]). The parser stays strict; the formatter
recovers the loose source. (`--loose` / `--recover` parse modes also exist for
one-off cases — see [[CLI-Usage#parse-modes-shared-flags]].)

## Corpus-scale gates "skip" — that's expected

The corpus-scale proofs (`corpus_proof` tests, throughput harnesses) are
**environment-gated** by `ABC_ROOT`. With no corpus present, a normal
`cargo test --workspace` **skips them cleanly** — that is the intended behaviour,
not a failure. croma builds and tests standalone; the full matrix runs from the
companion **croma-test** repo ([[How-its-Proven]]). A mis-set `ABC_ROOT` is
caught by a `>= 9000` file-count guard that rejects a vacuous run.

## Common build issues

- **Wrong toolchain.** croma pins **Rust 1.96.0** via `rust-toolchain.toml`;
  plain `cargo` / `rustc` selects it automatically. Don't hardcode a toolchain
  path. If a build complains about edition 2024, your `cargo` is too old — let
  the pin take over (or update via `rustup`).
- **Don't want `roxmltree`?** The MusicXML reader is the only thing that pulls
  it. Build a reader-less, zero-dependency CLI with
  `cargo build -p croma-cli --no-default-features` (this also removes
  `croma read` / `croma musicxml2abc`).
- **`croma read` / `musicxml2abc` "unknown subcommand".** You're on a
  `--no-default-features` (reader-less) build. Rebuild with default features.
- **Editor / grammar pieces.** `editors/zed/` and `tree-sitter-abc/` are
  **excluded from the cargo workspace** (they target wasm / use tree-sitter
  tooling). A root `cargo build` won't touch them; build them from their own
  directories per
  [`docs/editors.md`](https://github.com/ro-ag/croma/blob/main/docs/editors.md).

## Where to file bugs

- **Bugs / feature requests:**
  [GitHub Issues](https://github.com/ro-ag/croma/issues) — include the input and
  the exact command.
- **Security vulnerabilities:** GitHub **Security** tab → **Report a
  vulnerability** (private), per
  [`SECURITY.md`](https://github.com/ro-ag/croma/blob/main/SECURITY.md). Crashes,
  hangs, or excessive resource use on malformed input are valid security reports.
