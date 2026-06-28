# Library usage (`croma-core`)

`croma-core` is the library under the whole toolkit — the CLI, formatter, and
language server all call it rather than reparsing ABC. Its **default build is
zero-dependency and crates.io-publishable**.

```sh
cargo add croma-core
```

## The one-call conversion

```rust
use croma_core::abc_to_musicxml;

let xml = abc_to_musicxml("X:1\nT:Scale\nM:4/4\nL:1/8\nK:C\nC D E F G A B c|\n")?;
// xml: a String of MusicXML 4.0
```

`abc_to_musicxml(source: &str) -> croma_core::Result<String>` returns the
MusicXML string, or an error carrying parse diagnostics on malformed input.

## Conversion plus diagnostics

When you want the warnings alongside the output (e.g. to surface them in your
own UI), use `export_musicxml`, which returns a `MusicXmlExport`:

```rust
use croma_core::export_musicxml;

let export = export_musicxml("X:1\nT:Scale\nL:1/8\nK:C\n^C =D __E\n")?;
println!("{}", export.musicxml);          // the MusicXML String
for d in &export.diagnostics {            // Vec<croma_core::Diagnostic>
    eprintln!("{}: {}", d.code, d.message);
}
```

```rust
pub struct MusicXmlExport {
    pub musicxml: String,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn abc_to_musicxml(source: &str) -> Result<String>;
pub fn export_musicxml(source: &str) -> Result<MusicXmlExport>;
```

`abc_to_musicxml` is just `export_musicxml(...).map(|e| e.musicxml)`. Both
default to **ABC 2.1**.

### Options and lower-level entry points

For non-default parsing (e.g. the ABC 2.2 draft) or staged access to the model,
the crate also re-exports:

- `export_musicxml_with_options(source, ExportOptions)` — convert under explicit
  `ExportOptions` (`ParseMode`, `AbcSpecVersion`, …).
- `parse_document(source, ParseOptions) -> ParseReport<AbcDocument>` — parse only.
- `lower_score(&AbcDocument, LowerOptions)` — lower a parsed document to a `Score`.
- `write_musicxml(&Score) -> MusicXmlExport` — render a `Score` to MusicXML.

The model and option types (`Score`, `Tune`, `Part`, `Voice`, `NoteEvent`,
`Pitch`, `Diagnostic`, `Span`, `ParseOptions`, `ExportOptions`, `ParseMode`,
`AbcSpecVersion`, …) are all re-exported from the crate root. Browse the full
API on [docs.rs/croma-core](https://docs.rs/croma-core).

## The zero-dependency guarantee

`croma-core`'s default feature set pulls in **no runtime dependencies**. This is
a hard project invariant — CI asserts it
(`cargo tree -p croma-core --edges normal` resolves to the crate alone). Keep it
in mind if you contribute: new runtime deps belong on the binaries or behind an
opt-in feature, never the library default ([[Contributing]]).

## Enabling the MusicXML reader

The reverse direction (MusicXML → `Score`) is behind the opt-in
`musicxml-reader` feature, whose **only** dependency is `roxmltree`:

```sh
cargo add croma-core --features musicxml-reader
```

```rust
# // requires the `musicxml-reader` feature
use croma_core::read_musicxml;

let report = read_musicxml(&xml);   // ParseReport<Score>
let score = report.value;           // the reconstructed Score
```

`read_musicxml` and `complete_score_for_abc` are compiled only under that
feature (`#[cfg(feature = "musicxml-reader")]`); the default build never
compiles them nor `roxmltree`. Reader scope and evidence: [[MusicXML-Reader]].

## Related crates

| Crate | Role |
| --- | --- |
| [`croma-core`](https://crates.io/crates/croma-core) | the library — ABC 2.1 parser, model, ABC ↔ MusicXML |
| [`croma-fmt`](https://crates.io/crates/croma-fmt) | the formatter / auto-fixer (`format`, `auto_fix`) over the core model |
| [`croma-cli`](https://crates.io/crates/croma-cli) | the `croma` CLI binary |
| [`croma-lsp`](https://crates.io/crates/croma-lsp) | the stdio language server |
