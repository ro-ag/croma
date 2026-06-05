# ABC 2.1 Knowledge Base

Primary source: `raw/abc-2.1.dokuwiki.txt`.

Generated source manifest: `generated/source-manifest.json`.

Section lookup: `generated/section-index.md`.

## Status

ABC 2.1 is Croma's stable default. `ExportOptions::default()` should continue
to use ABC 2.1 unless the caller explicitly asks for draft compatibility.

The downloaded source is the full DokuWiki source for "The abc music standard
2.1 (Dec 2011)", including contents, sample ABC tunes, appendix, and errata.

## Parser And Exporter Reading Order

For Croma's ABC-to-MusicXML milestone, read and implement from the following
2.1 sections before using any 2.2 draft material:

| Priority | ABC 2.1 section | Why it matters |
| ---: | --- | --- |
| 1 | 2.1 Abc file identification | Version detection, BOM handling, strict versus loose interpretation. |
| 2 | 2.2 Abc file structure | Tune boundaries, file headers, free text, comments, line continuations. |
| 3 | 3.0 and 3.1 Information fields | Header semantics, required fields, repeated metadata, `K:` as body transition. |
| 4 | 3.2 Use of fields within the tune body | Inline fields and mid-tune state changes. |
| 5 | 3.3 Field continuation | Continuation handling for long text fields. |
| 6 | 4.1 to 4.5 Pitch, accidentals, note lengths, broken rhythm, rests | Core note/rest/duration model. |
| 7 | 4.6 Clefs and transposition | Volatile in 2.1; implement conservatively and compare with 2.2 appendix. |
| 8 | 4.7 to 4.13 Beams, barlines, repeats, ties/slurs, grace notes, tuplets | Measure and timing structure required by MusicXML. |
| 9 | 4.14 to 4.20 Decorations, symbol lines, chords, chord symbols, annotations, order | Non-note notation and construct ordering. |
| 10 | 5 Lyrics | Lyric attachment and syllable semantics. |
| 11 | 6 Typesetting and playback | Only map to MusicXML when semantics are clear. |
| 12 | 7 Multiple voices | Voice/staff model; 2.1 marks parts of this as under review. |
| 13 | 8 Standardisation | Character set, string handling, and backwards compatibility. |
| 14 | 9 Macros, 10 Outdated syntax, 11 Stylesheet directives | Recovery and compatibility policy. |
| 15 | 12 Conformance, 13 samples, 14 appendix/errata | Validation policy and regression examples. |

## Croma Policy From 2.1

- Default mode is ABC 2.1.
- Every parser diagnostic should carry a span and, where possible, the relevant
  2.1 section.
- `K:` is the required key field and also the transition into the tune body for
  ordinary tunes.
- Missing `L:` must not default blindly to `1/8`; implement ABC 2.1 default
  unit-note-length rules based on meter.
- Clef and transposition behavior in 2.1 is marked volatile. Use 2.1 for
  default syntax, but keep a cross-reference to the 2.2 appendix before making
  broad model decisions.
- Stylesheet directives are not formally part of the core ABC standard. Croma
  may preserve, diagnose, or map them to MusicXML where there is a clear
  semantic target, but they must not become notes.
- Reference converters do not override this source.

### Barline Normalization Policy

ABC 2.1 section 4.8 defines canonical barlines and explicitly recommends that
parsers be liberal with arbitrary `|`, `[`, `]`, and `:` sequences. Croma
normalizes source-spanned barline syntax before semantic lowering:

- Right-edge measure closers: `|`, `||`, `|]`, `:|`, `:||`, `::|`, and `:|]`
  close the current measure. `:||` and `:|]` are normalized as repeat-end
  barlines; the repeat-end MusicXML barline already carries the final-style
  visual boundary, so Croma does not emit a second adjacent double/final
  barline.
- Left-edge repeat starts: `|:`, `||:`, `|::`, and `[|:` emit a MusicXML
  left-repeat edge for the measure they start. `||:` is lowered as a double
  boundary plus repeat start; `[|:` is lowered as an initial boundary plus
  repeat start. At the first body position these attach to measure 1 and must
  not create an empty measure 1.
- Combined repeat boundaries: `::`, `:|:`, and `:||:` are normalized as
  repeat-both boundaries: a right-edge repeat end on the current measure plus a
  left-edge repeat start on the following measure. At the first body position
  they are recovered without inserting an empty measure.
- Triple-repeat spellings such as `|::` and `::|` preserve the start/end repeat
  edge. Croma does not currently model a repeat playback count for these
  spellings.
- Variant endings start at `[N` or adjacent shorthand such as `|1` and `:|2`.
  Per section 4.10, endings stop at `||`, `:|`, or `|]`; Croma emits the ending
  stop on the same MusicXML barline rather than double-emitting another
  boundary.
- Unrecognized liberal sequences are preserved in syntax, produce a
  source-spanned `abc.music.barline.liberal` warning, and lower as stable
  measure boundaries without changing note timing.

## Immediate Parser Backlog Anchored In 2.1

1. Source text wrapper with byte spans and line/column lookup.
2. Diagnostic codes that can cite ABC 2.1 sections.
3. File/tune boundary parser: file header, tune header, tune body, blank-line
   termination.
4. Information field parser for `X`, `T`, `M`, `L`, `K`, `Q`, `P`, `V`, `w`,
   `W`, `I`, and `%%`.
5. Music body parser for pitches, rests, accidentals, octave marks, lengths,
   broken rhythm, barlines, repeats, tuplets, chords, grace notes, ties/slurs,
   annotations, and decorations.
6. Lowering model that separates syntax recovery from semantic MusicXML export.
7. MusicXML writer tests that prove valid XML escaping for titles, lyrics,
   annotations, comments, and directives.

## Sources

- Primary ABC 2.1 raw export:
  `https://abcnotation.com/wiki/abc%3Astandard%3Av2.1?do=export_raw`
- Primary ABC 2.1 rendered page:
  `https://abcnotation.com/wiki/abc%3Astandard%3Av2.1`
- ABC standard index:
  `https://abcnotation.com/wiki/abc%3Astandard`
- ABC standard route map:
  `https://abcnotation.com/wiki/abc%3Astandard%3Aroute-map`
- Local downloaded raw source:
  `docs/reference/abc-spec-kb/raw/abc-2.1.dokuwiki.txt`
- Local rendered snapshot:
  `docs/reference/abc-spec-kb/raw/abc-2.1.html`
- Local download headers:
  `docs/reference/abc-spec-kb/raw/abc-2.1.headers.txt`
- Local media assets referenced by the spec:
  `docs/reference/abc-spec-kb/raw/media/`
- Media manifest with SHA-256:
  `docs/reference/abc-spec-kb/generated/media-manifest.tsv`
- Download manifest with SHA-256:
  `docs/reference/abc-spec-kb/generated/source-manifest.json`
- License shown on rendered abcnotation.com wiki pages:
  `CC Attribution-Noncommercial-Share Alike 3.0 Unported`,
  `http://creativecommons.org/licenses/by-nc-sa/3.0/`
