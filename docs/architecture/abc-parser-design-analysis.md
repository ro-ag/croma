# ABC Parser Design Analysis

This document records the design constraints for a serious ABC 2.1 parser in
Croma. It is based on the tracked ABC specification knowledge base, the
MusicXML 4.0 specification, the current `croma-core` skeleton, and the private
lessons document at `docs/untracked/trd-chat-lessons.md`.

The central conclusion is that Croma should not parse ABC directly from
characters into musical events. ABC has too many context-dependent constructs,
overloaded delimiters, dialect switches, delayed attachments, and source-layout
rules. The parser needs a source-preserving surface layer, a recoverable
lossless syntax layer, and a separate semantic lowering pass before MusicXML.

## Source Priority

- Default ABC target: ABC 2.1, from
  `docs/reference/abc-spec-kb/raw/abc-2.1.dokuwiki.txt`.
- Draft compatibility: ABC 2.2, from
  `docs/reference/abc-spec-kb/raw/abc-2.2-draft.dokuwiki.txt`.
- Local section index:
  `docs/reference/abc-spec-kb/generated/section-index.md`.
- MusicXML target: MusicXML 4.0 partwise output.

External source links:

- ABC 2.1: https://abcnotation.com/wiki/abc:standard:v2.1
- ABC 2.2 draft: https://abcnotation.com/wiki/abc:standard:v2.2
- MusicXML 4.0 final report: https://www.w3.org/2021/06/musicxml40/
- MusicXML `score-partwise`: https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/score-partwise/
- MusicXML `measure`: https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/measure-partwise/
- MusicXML `attributes`: https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/attributes/
- MusicXML `note`: https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/note/
- MusicXML `duration`: https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/duration/
- MusicXML `forward`: https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/forward/
- MusicXML `barline`: https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/barline/
- MusicXML `direction`: https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/direction/

## Why A Flat Parser Failed

ABC is intentionally compact, and the same byte often has different meanings in
different phases of the document.

- `[` can start an inline field, chord, repeat ending, thick barline, remark, or
  malformed recovery region.
- `]` can close a chord or participate in a barline.
- `|`, `:`, `[`, and `]` combine into liberal barline and repeat spellings.
- `"` can introduce a chord symbol or an annotation, depending on the first
  character inside the string.
- `!` can be a decoration delimiter or a legacy score line-break, depending on
  dialect state.
- `+` can be a field continuation prefix, a deprecated decoration delimiter, or
  obsolete chord syntax in legacy input.
- `(` can start a slur or a tuplet. A following digit changes the parse.
- `.` can mean staccato, dotted slur or tie, or dotted barline.
- `-` can be a tie in music, a syllable break in lyrics, or plain text.
- `&` means voice overlay in music but is deprecated inside lyrics and symbol
  lines.
- `%` starts a comment outside text strings, but escaped percent is text.

The ABC 2.1 continuation rules make the problem worse. The spec explicitly
separates continuation behavior for music code, fields, comments, and
directives, and warns developers that continuations generally cannot be parsed
by first joining physical lines. Croma therefore needs a physical-line model
with continuation edges instead of a preprocessed logical-line string.

The lessons document reinforces the same point:

- Source spans must be present from the first pass.
- Strict parsing and recovery need separate switches.
- Reference converters are evidence, not truth.
- The CLI, formatter, and LSP must not grow separate parsers.
- MusicXML must be emitted from a semantic score model, not recovery artifacts.

## Required Parser Shape

Croma should use a staged architecture:

1. `SourceText`
   - Owns the original source.
   - Records byte spans, line starts, physical line terminators, and optional
     file identity.
   - Gives cheap line/column lookup for diagnostics and LSP.

2. `LineMap`
   - Classifies physical lines without reducing them to music.
   - Distinguishes version line, empty line, comment, stylesheet directive,
     information field, field continuation, music code, and free text.
   - Records continuation edges and suppressed score line-breaks.
   - Preserves comments and directives as structured trivia or events.

3. `SurfaceToken` stream
   - Tokenizes within a line using document, tune, dialect, and body context.
   - Keeps all tokens lossless, including whitespace and unknown bytes.
   - Allows ambiguous or candidate classifications where syntax must decide.

4. Lossless syntax tree
   - Builds recoverable nodes for file headers, tune headers, tune bodies,
     fields, directives, music lines, note groups, chords, grace groups,
     annotations, decorations, tuplets, repeats, lyrics, and malformed regions.
   - Does not compute final pitch spelling, timeline position, or MusicXML.

5. Semantic score lowering
   - Converts syntax into musical meaning: parts, staves, voices, measures,
     events, durations, tuplets, ties, slurs, lyrics, directions, harmony, and
     metadata.
   - Applies default note length, key signatures, accidentals, voice state,
     meter changes, and dialect policy.
   - Produces diagnostics for unsupported or unrecoverable constructs.

6. MusicXML writer
   - Writes only from the semantic score.
   - Does not inspect parser recovery nodes except through explicit diagnostic
     or policy metadata.

This shape keeps the formatter and language server viable. They need the
lossless source and syntax layers. The MusicXML writer needs the semantic layer.
Neither should own tokenization.

## State Machines

The parser should carry explicit states rather than relying on local character
checks.

`DocumentState`:

- `Start`
- `FileHeader`
- `BetweenBlocks`
- `TuneHeader`
- `TuneBody`
- `FreeText`
- `TypesetTextBlock`

`DialectState`:

- ABC spec version: `V21` or `V22Draft`.
- Interpretation mode: strict, loose, recover.
- Decoration delimiter: `!` by default, `+` when `I:decoration +` applies.
- Score line-break mode: physical EOL by default, `$`, `!`, or none according
  to `I:linebreak`.
- Known `U:` user-defined symbols.
- Known `m:` macros, stored but not blindly expanded before syntax.

`TuneState`:

- Header fields and inherited file-header defaults.
- Current key, meter, unit note length, tempo, part label, and voice.
- Voice definitions and properties.
- Active clef/staff/transposition metadata.
- Per-voice accidental state.
- Per-voice lyric alignment cursor.
- Open slurs, tuplets, endings, repeats, and ties.

`BodyScannerContext`:

- Normal music.
- Inside quoted chord symbol or annotation.
- Inside inline field.
- Inside chord.
- Inside grace group.
- Inside lyric field.
- Inside symbol line.
- Inside text directive.

## Token Classification Rules

The scanner should prefer maximal, context-aware tokens, but it must not make
semantic decisions that belong to lowering.

| Source form | Surface classification | Decision point |
| --- | --- | --- |
| `[K:...]`, `[M:...]`, `[L:...]`, `[V:...]`, `[I:...]` | inline field | Body scanner recognizes `[letter:` with no space. |
| `[r:...]` | remark | Parsed as ignored remark with span. |
| `[1`, `[1,3`, `[1-3` | repeat ending start | Syntax validates list/range and spacing. |
| `[|`, `|]`, `||`, `|:`, `:|`, `::`, liberal variants | barline token | Barline parser normalizes style and repeat direction. |
| `[CEG]` | chord group | Chord parser owns contents; spaces are diagnostic. |
| `{abc}` | grace group | Grace parser owns inner note groups; lowering assigns no normal duration. |
| `(3`, `(3:2:4` | tuplet prefix | Syntax creates tuplet span; lowering applies duration ratio to next `r` notes. |
| `(`, `)` | slur delimiters | Syntax records nesting; lowering pairs or diagnoses. |
| `!trill!` | decoration | Dialect state decides delimiter. |
| `+trill+` | decoration only under `I:decoration +` or loose recovery | Strict default warns/errors. |
| `"` string | quoted text | First unescaped character classifies chord symbol vs annotation. |
| `.`, `~`, `H`, `L`, `M`, `O`, `P`, `S`, `T`, `u`, `v` | shorthand decoration candidate | Resolved using default or `U:` definitions. |
| `^`, `_`, `=`, `^^`, `__` | accidental | Only attaches before pitch inside note or key accidentals. |
| `'`, `,` | octave marks | Applies after pitch; mixed runs are normalized during lowering. |
| digits, `/`, `//` | length suffix | Parsed as rational multiplier. |
| `<`, `>`, repeated | broken rhythm | Lowering applies to adjacent time-bearing notes, grace-transparent. |
| `-` | tie candidate | Valid only immediately after a note group in music. |
| `z`, `x` | rest | `x` is invisible rest. |
| `Z`, `X` | multi-measure rest | Duration depends on meter; can represent measure count. |
| `y` | spacer | Direction/decorations may attach to it. |
| `&` | overlay | Rewinds to previous barline in semantic lowering. |
| `#`, `*`, `;`, `?`, `@` | reserved in music | Ignore with warning outside text, field, and annotation contexts. |

The classification for `[` should be ordered by strongest syntactic signal:

1. `[letter:` inline field or remark.
2. `[|` or other recognized thick-barline pattern.
3. `[digits/ranges` repeat ending.
4. Chord group if the following content is note-group-like.
5. Malformed bracketed region with diagnostic.

The classification for `!` should use dialect state first. In loose mode, when
`!` may mean both decoration and line-break, use the ABC 2.1 suggested lookahead
heuristic: a closing `!` before barline-like delimiters, whitespace, or line end
means decoration; otherwise treat it as a line-break and report the assumption.

## Lossless Syntax Model

The syntax model should be shaped for formatter/LSP work, not only export.

```rust
pub struct SourceText {
    text: String,
    line_starts: Vec<usize>,
}

pub struct SyntaxToken {
    kind: SyntaxKind,
    span: Span,
    role: TokenRole,
}

pub enum SyntaxElement {
    Node(SyntaxNode),
    Token(SyntaxToken),
}

pub struct SyntaxNode {
    kind: SyntaxKind,
    span: Span,
    children: Vec<SyntaxElement>,
}

pub struct ParseReport<T> {
    value: T,
    diagnostics: Vec<Diagnostic>,
}
```

This can be a small in-house tree first. If dependencies become worthwhile,
`rowan` is a good fit for an immutable green-tree style CST, but Croma should
not block the parser design on adopting it.

Important node families:

- `AbcFile`, `FileHeader`, `Tune`, `TuneHeader`, `TuneBody`, `FreeText`.
- `FieldLine`, `InlineField`, `DirectiveLine`, `CommentLine`,
  `FieldContinuation`.
- `MusicLine`, `MusicRun`, `Barline`, `RepeatEnding`, `OverlaySegment`.
- `NoteGroup`, `Note`, `Rest`, `MultiMeasureRest`, `Spacer`.
- `Chord`, `ChordMember`, `GraceGroup`, `Tuplet`, `Slur`.
- `Decoration`, `Annotation`, `ChordSymbol`, `SymbolLine`, `LyricLine`.
- `Malformed`, `Skipped`, `Reserved`.

Malformed nodes are mandatory. They let the formatter preserve input, the LSP
give targeted diagnostics, and the exporter skip or recover deterministically.

## Semantic Score Model

The semantic model should make voices, staves, measures, and timelines explicit
from the first real implementation pass. Flattening these early is the most
expensive mistake to fix later.

```rust
pub struct Score {
    metadata: Metadata,
    parts: Vec<Part>,
    diagnostics: Vec<Diagnostic>,
}

pub struct Part {
    id: PartId,
    name: Option<String>,
    staves: Vec<Staff>,
    voices: Vec<Voice>,
}

pub struct Voice {
    id: VoiceId,
    staff: StaffId,
    events: Vec<TimedEvent>,
}

pub struct TimedEvent {
    measure: MeasureId,
    onset: Rational,
    duration: Rational,
    source: Span,
    kind: EventKind,
    attachments: Attachments,
}
```

Core value types:

- `Rational` for all ABC durations before MusicXML divisions are chosen.
- `Pitch { step, alter, octave, spelling_source }`.
- `Accidental { kind, explicit, courtesy, source }`.
- `KeySignature { tonic, mode, fifths, explicit_accidentals }`.
- `Meter { display, duration, free_meter }`.
- `Clef`, `Transpose`, `VoiceProperties`, `StaffGroup`.
- `AttachmentBundle` for decorations, annotations, chord symbols, slurs, ties,
  articulations, ornaments, dynamics, and directions.

Duration lowering must apply, in order:

1. Unit note length from `L:` or ABC 2.1 meter-derived default.
2. Note/rest length suffix.
3. Chord outer multiplier.
4. Broken rhythm adjustment between adjacent time-bearing groups, ignoring
   intervening grace groups.
5. Tuplet ratio over the specified number of following notes.
6. Multi-measure rest expansion or compact representation.

Chord members can legally carry different lengths in ABC, but the spec warns
that staff rendering is undefined. The model should represent per-member
duration so the exporter can either encode it using explicit voices/backup
where possible or emit an unsupported-notation diagnostic. It should not coerce
all members to the first duration without reporting that choice.

## MusicXML Consequences

MusicXML 4.0 `score-partwise` contains a score header and one or more `part`
elements. Each `part` contains measures. A measure can contain notes, backups,
forwards, directions, attributes, harmony, barlines, and related musical data.

Implications for Croma:

- Use `score-partwise version="4.0"` as the default output form.
- Use `part-list` and stable part IDs even for single-voice tunes.
- Emit `attributes` for divisions, key, time, staves, clefs, and transposition.
- Compute a divisions value after semantic durations are known, not while
  parsing.
- Use `backup` and `forward` for multiple voices, overlays, and multi-staff
  timelines.
- Use `note` with `chord` for same-onset chord tones only when MusicXML timing
  remains correct.
- Preserve explicit ABC accidentals with MusicXML accidental notation when they
  are source-significant.
- Use `notations` for slurs, tied notation, articulations, ornaments, and
  technical marks.
- Use `tie` for sound and `tied` for notation where ABC tie semantics apply.
- Use `harmony` for chord symbols, not `direction` words, when the string is
  semantically a chord symbol.
- Use `direction` for text annotations, tempo text, dynamics, coda/segno-style
  directions, and unsupported-but-preservable directives.
- Use `barline` with repeat and ending children for repeats and variants.
- Use `lyric` on notes after the lyric alignment pass.

The writer should use an XML writer or a tightly tested output layer. Manual
string concatenation is acceptable for the current skeleton only; it will not
scale once text, lyrics, annotations, and directives enter the output.

## Diagnostics And Recovery

Diagnostics should carry:

- Severity.
- Stable code.
- Message.
- Source span.
- Spec reference, when known.
- Recovery action, when recovery was applied.
- Compatibility note, when loose or draft behavior was used.

Example codes:

- `abc.file.missing_x`
- `abc.file.missing_k`
- `abc.field.disallowed_in_body`
- `abc.field.unknown`
- `abc.dialect.legacy_decoration`
- `abc.music.unclosed_chord`
- `abc.music.unclosed_grace`
- `abc.music.unmatched_slur`
- `abc.music.invalid_tuplet`
- `abc.music.broken_rhythm_without_neighbor`
- `abc.music.variable_chord_duration`
- `abc.voice.overlay_incomplete_measure`
- `abc.lyric.syllable_count`
- `abc.musicxml.unsupported_construct`

Recovery must be deterministic and visible. It is better to emit valid output
with a precise unsupported diagnostic than to silently reinterpret malformed
text as notes.

## Parser Technology Recommendation

Use a hand-written scanner and recursive/event parser for the top-level ABC
grammar. The hard parts are not context-free parsing; they are stateful
classification, source preservation, dialect handling, and recovery.

Parser combinators or generated parsers can still help in narrow subparsers:

- Field values such as `K:`, `M:`, `L:`, `Q:`, and `V:`.
- Chord symbol grammar.
- Lyric syllable splitting.
- `%%score` voice grouping.

Do not make a parser generator own the whole grammar unless it can preserve
trivia, recover malformed nodes, and accept dynamic dialect state without
turning the grammar into a maze.

Dependency candidates to validate during implementation:

- `quick-xml` for MusicXML writing.
- `clap` for the CLI.
- `insta` for snapshot fixtures.
- `rowan` for a future immutable CST if the in-house tree becomes limiting.
- `lsp-types` or `tower-lsp` for the later language server.

The first implementation can stay dependency-light while proving the tree and
score model. Add crates when they remove real complexity.

## Non-Negotiable Invariants

- ABC 2.1 is the default.
- ABC 2.2 draft behavior is opt-in or explicitly documented compatibility.
- Every token and diagnostic has a byte span.
- The parser produces a lossless representation before semantic lowering.
- The semantic model keeps voices, staves, parts, and timelines explicit.
- The CLI calls `croma-core`; it never parses ABC itself.
- The formatter uses the same surface/syntax tree.
- The LSP uses the same parser and diagnostics.
- Reference converter mismatch is classified, not blindly optimized away.
