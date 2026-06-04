# ABC To MusicXML Implementation Roadmap

This roadmap turns the parser design analysis into implementation phases for
`croma-core`, `croma-cli`, and the later formatter and language server. The
goal is a library-first ABC 2.1 to MusicXML 4.0 exporter with reliable
diagnostics, not a quick converter that works only for simple tunes.

Read this with:

- `docs/architecture/library-first.md`
- `docs/architecture/abc-parser-design-analysis.md`
- `docs/reference/abc-spec-kb/abc-2.1-knowledge-base.md`
- `docs/reference/abc-spec-kb/abc-2.2-draft-appendix.md`
- `docs/reference/corpus-inventory.md`

## Product Boundary

First product surface:

- `croma-core`: source model, parser, semantic score lowering, diagnostics, and
  MusicXML writer. This is the primary publishable Rust library crate and must
  remain compatible with crates.io and docs.rs.
- `croma-cli`: thin wrapper over `croma-core`.

Later product surfaces:

- `croma-fmt`: formatting from the shared lossless syntax model.
- `croma-lsp`: diagnostics, semantic tokens, formatting, and code actions from
  the same parser.

Out of initial scope:

- MusicXML to ABC.
- PDF rendering.
- Engraving layout beyond MusicXML semantics.
- Broad support for every application-specific ABC directive.

## Public API Direction

The current API can remain as the simple export surface, but the core crate
needs richer internal and public parse/report APIs.

```rust
pub fn parse_document(source: &str, options: ParseOptions) -> ParseReport<AbcDocument>;
pub fn lower_score(document: &AbcDocument, options: LowerOptions) -> LowerReport<Score>;
pub fn write_musicxml(score: &Score, options: MusicXmlOptions) -> XmlReport;

pub fn export_musicxml_with_options(
    source: &str,
    options: ExportOptions,
) -> Result<MusicXmlExport>;
```

`ExportOptions` should eventually include:

- ABC spec version.
- Strict, loose, or recover mode.
- Whether warnings are returned, printed, or treated as errors.
- MusicXML target version.
- Unknown directive policy.
- Corpus/debug toggles for token/tree/score dumps.

`MusicXmlExport` should eventually include:

- XML string.
- Diagnostics.
- Parse and lowering summaries.
- Optional timing or feature counts for corpus runs.

## Crates.io Compatibility

`croma-core` must be designed as a normal library dependency for downstream
Cargo users, not only as an implementation detail of this workspace.

Manifest and packaging requirements:

- Keep package metadata ready for crates.io: `name`, `version`, `description`,
  `license`, `repository`, `readme`, `keywords`, and categories.
- Keep the crate buildable from its packaged `.crate`, not only from the
  workspace checkout.
- Run `cargo package -p croma-core --list` before release to inspect packaged
  files.
- Run `cargo publish -p croma-core --dry-run` before release.
- Keep package size small; do not include the real corpus, generated reference
  MusicXML, or private `docs/untracked/` material.
- Any dependency used by `croma-core` must be available from crates.io unless it
  is a dev-only local tool that is not required by the published package.
- If later publishable workspace crates depend on `croma-core` with a local
  `path`, also provide a matching `version` so the published crate resolves
  through crates.io.

Public API requirements:

- Treat exported types and functions as SemVer commitments once published.
- Keep experimental parser internals private or gate them behind explicitly
  named unstable features until they are ready.
- Prefer feature flags for optional integrations such as corpus comparison,
  MusicXML validation helpers, or future LSP support.
- Keep the default feature set suitable for ordinary library consumers.
- Provide useful rustdoc examples for the simple export path and diagnostic
  path.
- Avoid requiring network access, external converters, local corpora, or system
  tools to compile the library.

Release readiness checks:

- `cargo test -p croma-core`
- `cargo clippy -p croma-core --all-targets -- -D warnings`
- `cargo doc -p croma-core --no-deps`
- `cargo package -p croma-core --list`
- `cargo publish -p croma-core --dry-run`

## Phase 0: Lock Design Decisions

Deliverables:

- Keep `abc-parser-design-analysis.md` and this roadmap tracked.
- Add a small parser decision log when implementation decisions become
  irreversible.
- Keep `docs/untracked/` private and ignored.
- Keep `croma-core` packaging checks in the release checklist.

Acceptance:

- The parser architecture is explicit before the stub parser is replaced.
- Any future dependency choice names the problem it solves.

## Phase 1: Source And Diagnostics Foundation

Implement:

- `SourceText` with byte spans, line starts, line/column conversion, and
  original EOL preservation.
- `Diagnostic` extensions: stable code, optional spec reference, and optional
  recovery note.
- `ParseOptions` with spec version and mode.
- Test helpers for asserting spans by source substrings.

Tests:

- UTF-8 spans.
- CRLF/LF/CR line endings.
- BOM ignored only at file start.
- Empty input and comment-only input diagnostics.

Acceptance:

- Every parser-facing function can report exact byte spans.

## Phase 2: Physical Line And Block Classifier

Implement:

- Physical line classifier for version lines, empty lines, comments,
  stylesheet directives, information fields, field continuations, music code,
  free text, and typeset text directives.
- Tune boundary detection using `X:` start, `K:` header/body transition, empty
  line termination, and file-header defaults.
- Preservation of comments and directives as structured non-note items.

Tests:

- File header then multiple tunes.
- Free text between tunes.
- `K:` ends the tune header.
- `+:` continuation on string fields.
- Music code with backslash-suppressed score line-breaks.
- Comments before and after music without creating fake empty lines.

Acceptance:

- No comments, directives, free text, or field continuations can leak into the
  note stream.

## Phase 3: Field Parsing And Dialect State

Implement field subparsers for:

- Required/common fields: `X`, `T`, `M`, `L`, `K`, `Q`, `P`, `V`.
- Text metadata fields: `C`, `O`, `R`, `N`, `Z`, `W`.
- Body-only or body-relevant fields: `w`, `s`.
- Interpretation fields: `I:abc-version`, `I:abc-charset`,
  `I:linebreak`, `I:decoration`.
- User symbols: `U:`.
- Macros: `m:` as stored definitions, not global text substitution yet.

Tests:

- Missing `L:` default from meter, including `M:2/4 -> L:1/16`.
- `M:C`, `M:C|`, `M:none`.
- Key modes and explicit key accidentals.
- Unknown fields ignored with warning, not parsed as music.
- Strict vs loose interpretation from `%abc-2.1` and `I:abc-version`.
- `I:decoration +` changes decoration delimiter.
- `I:linebreak !` changes `!` behavior.

Acceptance:

- Dialect state is explicit and inspectable in debug output.

## Phase 4: Core Music Syntax

Implement lossless syntax nodes and semantic lowering for:

- Notes `A-G`, `a-g`.
- Accidentals `^`, `_`, `=`, `^^`, `__`.
- Octave marks with mixed comma/apostrophe normalization.
- Length suffixes including integer, fraction, slash shorthand, and repeated
  slash shorthand.
- Rests `z`, invisible rests `x`, multi-measure rests `Z` and `X`.
- Spacers `y`.
- Basic barlines, double bars, and repeat barlines.
- Dotted and invisible barlines as syntax, with export policy.

Tests:

- Pitch and octave normalization.
- Explicit accidental preservation.
- Fractional durations.
- Multi-measure rests in known meter and free meter.
- Liberal barline spellings with diagnostics.

Acceptance:

- The old stub parser can be removed without losing the existing basic export
  test.

## Phase 5: Attachments And Overlapping Constructs

Implement:

- Prefix attachment bundle in ABC order: grace notes, chord symbols,
  annotations/decorations, accidentals, note/rest/chord, octave, length.
- Grace groups with no normal duration and broken-rhythm transparency.
- Chords with per-member syntax and outer duration multiplier.
- Chord symbols vs annotations from quoted text.
- Decorations: named, shorthand, user-defined, unknown, and legacy delimiter.
- Ties and slurs, including dotted variants and nested slurs.
- Tuplets `(p`, `(p:q`, `(p:q:r`.
- Broken rhythm chains.
- First/second repeats and variant endings.

Tests:

- Spec example shape: `"Gm7"v.=G,2`.
- Chord with decorations inside and outside.
- `[C2E2G2]3` duration multiplication.
- Variable-duration chord member emits diagnostic.
- `A<{g}A` and `A{g}<A` lower equivalently.
- `(3.a.b.c` staccato triplet.
- `:|2` line-start and adjacent-ending cases.
- Malformed unclosed chord/grace/slur recovery.

Acceptance:

- Ambiguous punctuation is resolved by syntax context and dialect state, not by
  ad hoc event parsing.

## Phase 6: Voices, Overlays, Lyrics, And Symbol Lines

Implement:

- Header and body `V:` fields.
- Inline `[V:...]`.
- Voice properties: name, subname, clef, stem, octave/transposition placeholders.
- `%%score` / `I:score` preservation first, then basic staff grouping.
- Voice timelines by measure.
- Overlay `&` as measure-local temporary voices.
- `w:` lyric alignment cursor per voice.
- `W:` lyrics as post-tune text.
- `s:` symbol line alignment.

Tests:

- Sequential voice blocks.
- Interleaved `[V:...]` lines.
- Lyrics postponed to the end of the tune under ABC 2.1 rules.
- Empty `w:` line consuming notes without printed lyrics.
- Lyrics do not align to rests, spacers, or grace notes.
- Overlay rewinds to previous barline and requires complete measure duration.

Acceptance:

- The score model can represent multi-voice music before the MusicXML writer
  tries to encode it.

## Phase 7: Semantic Score Lowering

Implement:

- `Score`, `Part`, `Staff`, `Voice`, `Measure`, and `TimedEvent`.
- Rational duration math and divisions planning.
- Measure construction with pickup and free-meter handling.
- Key signature and accidental propagation policy.
- Tie and slur pairing.
- Tuplet duration transformation.
- Direction/harmony/lyric attachment.
- Unsupported construct diagnostics.

Tests:

- Measure duration checks in fixed meter.
- Pickup measure marked as implicit or otherwise exportable.
- Accidentals reset at barlines according to chosen policy.
- Multiple voices produce correct onsets and durations.
- Unsupported constructs remain source-linked.

Acceptance:

- MusicXML writing can be treated as serialization, not interpretation.

## Phase 8: MusicXML 4.0 Writer

Implement:

- XML writer layer, preferably with `quick-xml` after validating ergonomics.
- `score-partwise version="4.0"`.
- `part-list`, `score-part`, stable part IDs, and part names.
- `attributes` for divisions, key, time, clefs, staves, and transposition.
- Notes, rests, invisible rests, multi-measure rests where supported.
- Multiple voices with `backup` and `forward`.
- Chords using `chord` when timing is representable.
- Ties, slurs, tuplets, articulations, ornaments, dynamics, lyrics, harmony,
  directions, repeats, and endings.
- XML escaping tests for all text-bearing output.

Tests:

- Schema-valid simple score.
- Text escaping in title, composer, lyrics, chord symbols, annotations, and
  directives.
- Multiple voices and overlays preserve timing.
- Repeats and endings encode as MusicXML barlines.
- Grace notes omit normal duration.

Acceptance:

- Exported XML is valid XML and MusicXML-shaped before corpus comparison begins.

## Phase 9: CLI

Keep the CLI thin.

Commands:

- `croma xml <file.abc> [-o out.musicxml]`
- `croma check <file.abc>`
- `croma dump tokens <file.abc>`
- `croma dump tree <file.abc>`
- `croma dump score <file.abc>`

Options:

- `--strict`
- `--loose`
- `--recover`
- `--abc-2.2-draft`
- `--diagnostics text|json`
- `--warnings-as-errors`

Acceptance:

- CLI commands call `croma-core` APIs only.
- JSON diagnostics include spans and codes.
- Dump commands are good enough for corpus debugging.

## Phase 10: Corpus Harness

Implement outside the core parser:

- Feature scanner backed by parser tokens, not regex-only long term.
- Batch export runner.
- MusicXML-aware comparison against references.
- SQLite or JSONL result store.
- Disposition categories: Croma bug, reference artifact, policy decision,
  unsupported notation, recovery candidate.

Tests and reports:

- Small fixtures for every corpus bug fixed.
- Corpus summary with raw mismatch count and actionable mismatch count.
- No generated reference output in tracked paths.

Acceptance:

- A corpus run can explain failures with source spans and categories.

## Phase 11: Formatter Preparation

Do not build the formatter until the lossless syntax layer is stable enough.
Prepare now by preserving:

- Original whitespace and comments.
- Field ordering.
- Physical line breaks and suppressed score line-breaks.
- Trivia around note groups and barlines.
- Malformed nodes.

Formatter later:

- `croma-fmt` calls `croma-core` parse APIs.
- Formatting changes trivia and layout only.
- Formatting never changes the semantic score unless explicitly requested.
- `croma fmt --check` compares formatted output to input.

Acceptance:

- Parser snapshots include enough trivia to round-trip input before formatting.

## Phase 12: Language Server Preparation

Prepare now by ensuring:

- Parser can return diagnostics without exporting MusicXML.
- Spans convert to line/column positions cheaply.
- Tune boundaries are explicit, allowing future per-tune incremental parse.
- Syntax nodes have stable kinds useful for semantic tokens.
- Recovery nodes preserve invalid source for editor ranges.

LSP later:

- Diagnostics from parse and lowering.
- Semantic tokens for fields, notes, barlines, decorations, lyrics, comments,
  and directives.
- Formatting through `croma-fmt`.
- Code actions for common repairs, such as missing `K:`, legacy decoration
  delimiter, unmatched slur, and invalid field placement.

Acceptance:

- `croma-lsp` remains a client of `croma-core`, not a parser fork.

## Vertical Slices

The implementation should advance by narrow vertical slices rather than
isolated broad modules.

1. Header plus core notes:
   parse one tune, lower pitch/duration/barlines, write MusicXML.

2. Ambiguous attachments:
   parse decorations, quoted text, accidentals, chords, and ties around a note
   group, then export what is semantically supported.

3. Rhythm:
   implement broken rhythm and tuplets with rational timing and fixture tests.

4. Repeats:
   implement barline variants, endings, and repeat MusicXML.

5. Voices:
   implement `V:`, `[V:...]`, timelines, `backup`, and `forward`.

6. Lyrics:
   implement lyric cursor and MusicXML lyric output.

7. Corpus loop:
   run the real corpus, classify failures, and add small fixtures for every
   accepted fix.

## Risk Register

| Risk | Mitigation |
| --- | --- |
| Overloaded punctuation collapses too early | Surface tokens and CST before semantic lowering. |
| Formatter/LSP cannot reuse parser | Preserve trivia, spans, and malformed nodes from phase 1. |
| Reference converter drives wrong behavior | Classify mismatch as bug, artifact, or policy decision. |
| Variable-duration chords force redesign | Represent per-member duration in the model immediately. |
| Multi-voice output breaks timing | Model voices and measures before MusicXML; use backup/forward. |
| Continuation preprocessing loses context | Keep physical line map and continuation edges. |
| Directives become fake music | Line and field classifier owns directives before music scan. |
| XML escaping becomes incomplete | Use XML writer or exhaustive text-output tests. |
| ABC 2.2 leaks into default mode | Gate draft features through options and diagnostics. |

## Immediate Next Coding Tasks

1. Extend diagnostics with code/spec/recovery metadata.
2. Add `SourceText` and span test helpers.
3. Replace `surface::analyze` with a physical line classifier.
4. Add parser snapshots for line/block classification.
5. Add `ParseOptions` and carry spec/mode through the pipeline.
6. Implement field parsing for `X`, `T`, `M`, `L`, `K`, and `I:linebreak`.
7. Rebuild the existing simple note export on top of the new source and field
   layers before adding more music constructs.
