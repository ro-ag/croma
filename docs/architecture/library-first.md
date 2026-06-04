# Library-First Architecture

Croma starts as a Rust library for ABC -> MusicXML. Everything else is a client
of that library.

## Pipeline

1. `abc_surface`
   - Classifies source spans before parsing.
   - Resolves overloaded characters such as `[`, `]`, `|`, `:`, `"`, `!`, `+`,
     slurs, ties, decorations, grace braces, and inline fields.
   - Keeps byte spans for diagnostics, formatting, and editor tooling.
   - Current crate module: `croma_core::surface`.

2. `abc_parse`
   - Consumes the surface map.
   - Produces a lossless tune model with recoverable malformed nodes.
   - Does not invent music to satisfy a reference converter.
   - Current crate module: `croma_core::parser`.

3. `score`
   - Lowers ABC syntax to musical semantics: voices, measures, durations,
     pitches, rests, tuplets, repeats, lyrics, and directives.
   - Current crate module: `croma_core::model`.

4. `musicxml`
   - Emits MusicXML from the score model only.
   - Parser-specific recovery should not leak into the MusicXML writer.
   - Current crate module: `croma_core::musicxml`.

5. `compare`
   - Uses MusicXML-aware comparison for corpus work.
   - Reference converters are evidence, not ground truth.

## Product Order

1. `croma-core`: ABC -> MusicXML library.
2. `croma-cli`: thin command wrapper over `croma-core`.
3. `croma-fmt`: token-preserving formatter from the shared surface model.
4. `croma-lsp`: diagnostics, semantic tokens, formatting, and code actions from
   the same model.

`croma-core` is the publishable Rust library. It must stay compatible with
normal crates.io packaging, docs.rs builds, SemVer versioning, and downstream
Cargo dependency use.

## Design Documents

- `docs/architecture/abc-parser-design-analysis.md`
  - Deep parser, token, state, and score-model analysis for ABC 2.1,
    draft 2.2 compatibility, and MusicXML 4.0 export.
- `docs/architecture/abc-to-musicxml-roadmap.md`
  - Implementation phases for the library, CLI, corpus harness, formatter, and
    language server.

## Invariants

- ABC 2.1 is the stable default.
- ABC 2.2 features are explicit draft-mode compatibility.
- Every diagnostic carries a source span.
- `croma-core` remains crates.io-publishable; no parser feature may depend on
  local private paths, untracked corpus data, or CLI-only behavior.
- Full-corpus runs validate a coherent change; they are not a substitute for
  small fixtures.
- Mismatches end as a fixed Croma bug, a documented reference artifact, or a
  documented spec/MusicXML policy decision.
