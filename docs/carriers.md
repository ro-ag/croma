# croma private carriers (`[I:croma-*]` / `%%croma-*`)

croma round-trips MusicXML through ABC: `MusicXML → ABC → MusicXML`. ABC 2.1 cannot
natively express every MusicXML fact (per-note instrument switches, functional
`<harmony>` text, `<forward>`/`<backup>` cursor moves, wide tuplets, …). Rather than
drop those facts, croma stores them in **namespaced carriers** that travel inside the
ABC text and are re-applied on the way back to MusicXML.

A carrier is *not* a comment that humans read — it is a structured, machine-owned
annotation in croma's `croma-` namespace. Every other ABC tool ignores it (see
[§7](#7-compatibility-other-tools-ignore-carriers)), so the ABC stays playable in
abc2midi / abcm2ps / abcjs while croma keeps full fidelity.

## 1. The two vehicles

| Vehicle | Form | Anchored to | Use for |
|---|---|---|---|
| **inline instruction** | `[I:croma-<name> k=v …]` (or, for the seven coded carriers, `[I:cr <code> …]`) | a position in the music stream (the following note / chord / barline / `[M:]` / `[K:]`) | per-note and per-measure facts |
| **header directive** | `%%croma-<name> …` | a voice or the score | score/voice-level facts (e.g. the instrument list) |

Inline is the default and the one to reach for: it rides *with* the construct it
annotates, so it survives editing the surrounding music. A header directive is only
right when the fact is genuinely score/voice-scoped, not tied to one note. The
`%%croma-<name>` header form is **never compacted** — see [§3](#3-compact-spelling-icr-code).

Two carriers are special: `croma-musicxml-instrument` is header-only; `croma-time-symbol`
is **both** (a whole-line form before the header `M:`, and an inline form before a
mid-tune `[M:]`).

## 2. Syntax

```
[I:croma-<name> field=value field2="value with spaces" field3-hex=<utf8-hex>]
%%croma-<name> field=value …
```

- **Name** — `croma-` prefix + lowercase-kebab. Group MusicXML-origin carriers under
  `croma-musicxml-*` when the name would otherwise be ambiguous.
- **Fields** — `key=value`, space-separated. A value containing spaces is double-quoted;
  `\` and `"` inside are backslash-escaped (`abc_carrier_quoted` in `to_abc.rs`).
- **Boolean carriers** carry no fields — the bare name *is* the flag (e.g.
  `[I:croma-musicxml-forward]`, `[I:croma-after-grace]`, `[I:croma-key-restatement]`).
- **Numeric/rational** values are plain (`actual=11`) or split into `n=`/`d=` integer
  pairs for fractions (`[I:croma-musicxml-sequence-backup n=3 d=8]`).
- Parsers accept a few **aliases** for robustness (e.g. `n` / `number` / `label`;
  `actual` / `actual-notes` / `a`) but emit the canonical short spelling.

## 3. Compact spelling (`[I:cr <code> …]`)

The seven most frequent carriers also have a **compact** inline spelling. `croma xml` /
`croma read` / `croma musicxml2abc` emit compact by default — it cuts exported ABC size
by roughly 14.5% across the corpus. The long `[I:croma-<name> …]` spelling for these
seven is still **read**, with a one-time-per-kind deprecation warning
(`abc.lower.deprecated_carrier_spelling`), until croma 2.0; plain `croma fmt` (no flag
needed) rewrites any long spelling it finds back to compact.

The registry is closed — only these eight codes exist, and no other carrier gets one
without a spec change:

| Code | Long name | Fields |
|---|---|---|
| `dp` | `croma-direction-placement` | `dp=<a\|b>` (`a`=above, `b`=below) |
| `ht` | `croma-harmony-text` | `ht text="…"` or `ht text-hex=…` |
| `htx` | `croma-harmony-text` | `htx` (the textless flag; no fields — same long name as `ht`, disambiguated by whether the value carries `textless`) |
| `le` | `croma-lyric-extend` | `le=<verse>` |
| `mr` | `croma-meter-restatement` | `mr` (flag; no fields) |
| `kr` | `croma-key-restatement` | `kr` (flag; no fields) |
| `ec` | `croma-ending-close` | `ec t=<s\|d> l=<l\|r> n="…"` (`t`: `s`=stop, `d`=discontinue; `l`: `l`=left, `r`=right) |
| `mf` | `croma-musicxml-forward` | `mf` (flag; no fields) |

Examples (compact, left) next to the long spelling they are read from as well:

```
[I:cr dp=a]!p!C          ==  [I:croma-direction-placement placement=above]!p!C
[I:cr ht text="dim"]"Bdim"B  ==  [I:croma-harmony-text text="dim"]"Bdim"B
[I:cr htx]"C"C            ==  [I:croma-harmony-text textless=1]"C"C
[I:cr le=1]A2              ==  [I:croma-lyric-extend verse=1]A2
[I:cr mr] [M:4/4]          ==  [I:croma-meter-restatement] [M:4/4]
[I:cr kr] [K:F]            ==  [I:croma-key-restatement] [K:F]
[I:cr ec t=d l=r n="1,2"]  ==  [I:croma-ending-close type=discontinue location=right number="1,2"]
[I:cr mf]x4                ==  [I:croma-musicxml-forward]x4
```

The **other 15 carriers** — `croma-musicxml-tuplet`, `croma-clef-cursor`,
`croma-after-grace`, `croma-time-symbol`, `croma-measure-number`, `croma-tempo`,
`croma-sound-tempo`, `croma-lyric-duplicate`, `croma-initial-key`, `croma-initial-meter`,
`croma-note-instrument`, `croma-barline-style`, `croma-xvoice-slur`,
`croma-musicxml-sequence-backup`, `croma-musicxml-instrument` — keep their long spelling
only; they are infrequent enough in the corpus that a code would not pay for the
registry entry. The `%%croma-<name>` **header** vehicle is unchanged for every carrier,
coded or not — only the seven coded carriers' **inline** form gets a compact spelling.

Implementation: compact is a *spelling*, not a second semantics. `expand_compact_carrier`
/ `compact_code_for_long` (`crates/croma-core/src/lower/carrier.rs`) rewrite a compact
value into the exact long-form string the existing `parse_*_instruction` functions
already understand, so there is still exactly one parser per carrier and the two
spellings cannot drift apart. `croma-fmt`'s migration
(`crates/croma-fmt/src/fixes.rs`) mirrors the same registry in the other direction; the
two tables are kept in sync by hand (see [§8](#8-adding-a-carrier-checklist)).

## 4. The `-hex=` rule for hostile characters

Inside an inline `[I:…]` field, three characters break the ABC tokenizer: `%` starts a
comment, `]` closes the field early, and raw control characters (newline, …) split the
line. When a value contains any of these, croma emits a **hex variant** of the field
instead:

```
text="John"        →  text-hex=4a6f686e          # value bytes as lowercase UTF-8 hex
clef="bass"        →  clef-hex=…                  # when the clef text is hostile
n="1 a"            →  n-hex=…                      # measure label with odd chars
```

Guard: `needs_hex_inline_carrier(text)` (true if the text contains `]`, `%`, or a
control char). Encode with `hex_utf8`; decode with `parse_croma_hex_utf8`. Carriers that
can hold free text use this: `croma-harmony-text` (`ht text-hex=…`),
`croma-lyric-duplicate`, `croma-tempo`, `croma-sound-tempo`, `croma-clef-cursor`,
`croma-measure-number`. (Header `%%` directives are line-level and do not need the
`]`/`%` escape, but a free-text value such as an instrument name still maps any control
character to a space — header names are single-line.)

## 5. The round-trip contract (three-sided — all or it is not lossless)

A carrier is only lossless if all three stages agree:

```
1. EMIT     model → ABC     to_abc.rs            writes  [I:croma-<name> …] (or [I:cr <code> …])
2. APPLY    ABC   → model   lower/mod.rs         parses it back onto the model
3. RE-EMIT  model → XML     musicxml/*.rs        writes the original <element>
```

- **Emit** lives in `to_abc.rs` — one small `*_instruction(...)` builder per carrier,
  called from the note/chord/barline/header writer at the right position; for the seven
  coded carriers the builder emits the compact spelling directly.
- **Apply** is dispatched from `apply_inline_field` (`lower/mod.rs`, the `'I'` arm) for
  inline carriers, or from the `%%`/score-directive path for headers. A compact
  `[I:cr <code> …]` value is expanded to its long form first (`carrier::expand_compact_carrier`),
  so every carrier still has exactly one `parse_<thing>_instruction(...)` that
  `strip_prefix("croma-<name>")`s the value, whichever spelling was written. The parsed
  fact is usually staged in a `pending_musicxml_*` slot and **drained onto the next timed
  event** (see `lower/voice.rs`) — including a parallel drain on the chord path so a
  carrier flushed before a chord lands on the chord head, not a later member.
- **Re-emit** lives in `musicxml/*.rs` (`note.rs`, `score.rs`, `attributes.rs`,
  `barline.rs`, `harmony.rs`, `lyric.rs`, `direction.rs`, `notation.rs`).

Adding or changing a carrier touches all three, plus the model field it rides on, plus a
TDD test, plus the gates in [§8](#8-adding-a-carrier-checklist).

## 6. Catalogue (22 carriers)

`grep <name> crates/croma-core/src` finds the emit / parse / re-emit sites for any row.
Examples below show the **current** spelling: compact (`[I:cr <code> …]`) for the seven
carriers coded in [§3](#3-compact-spelling-icr-code), long for the other fifteen. The
long spelling of a coded carrier is still accepted on read (deprecated until 2.0).

### Instruments & chord symbols
| Carrier | Vehicle · scope | Carries | Example |
|---|---|---|---|
| `croma-musicxml-instrument` | `%%` · part | `<score-instrument>`+`<midi-instrument>` (id, name, channel/program/volume/pan/midi-unpitched) — incl. drum kits ABC can't express | `%%croma-musicxml-instrument id="P1-I1" name="Snare Drum" channel=10 midi-unpitched=39` |
| `croma-note-instrument` | `[I:]` · note | per-note `<instrument id=…/>` reference (which declared instrument sounds this note) | `[I:croma-note-instrument id="P1-I1"]` |
| `croma-harmony-text` (code `ht`/`htx`) | `[I:]` · note | `<harmony><kind text=…>` provenance — `Textless` vs `Text(value)` vs (absent =) ABC-native | `[I:cr ht text="dim"]"Bdim"` |

### Lyrics
| Carrier | Vehicle · scope | Carries | Example |
|---|---|---|---|
| `croma-lyric-extend` (code `le`) | `[I:]` · note | the primary syllable's same-note `<extend/>` melisma flag (verse only) | `[I:cr le=1]` |
| `croma-lyric-duplicate` | `[I:]` · note · **hex** | an extra same-note same-verse syllable `w:` can't spell (verse, text, optional `extend=1`) | `[I:croma-lyric-duplicate verse=1 text="John"]` |

### Tempo
| Carrier | Vehicle · scope | Carries | Example |
|---|---|---|---|
| `croma-tempo` | `[I:]` · voice · **hex** | a printed/sound tempo whose `<words>` text `Q:` can't hold (role, words, bpm + beat) | `[I:croma-tempo role=printed text-hex=… bpm=100 beat-n=1 beat-d=4]` |
| `croma-sound-tempo` | `[I:]` · voice · **hex** | a playback-only tempo (no printed metronome): bpm + reference beat | `[I:croma-sound-tempo bpm=80 beat-n=1 beat-d=4 text="80"]` |

### Directions
| Carrier | Vehicle · scope | Carries | Example |
|---|---|---|---|
| `croma-direction-placement` (code `dp`) | `[I:]` · decoration | `<direction placement=…>` for ABC decorations that emit MusicXML directions (dynamics, wedges, coda, segno) | `[I:cr dp=a]!p!` |

### Key & meter
| Carrier | Vehicle · scope | Carries | Example |
|---|---|---|---|
| `croma-initial-key` | `[I:]` · voice | a per-voice initial `<key>` that the header `K:` can't encode (fifths + explicit accidentals) | `[I:croma-initial-key fifths=0 accidentals=F:1,C:1]` |
| `croma-initial-meter` | `[I:]` · voice | a per-voice initial `<time>` (display string + common/cut symbol) the header `M:` can't encode | `[I:croma-initial-meter display="2/2" symbol=cut]` |
| `croma-key-restatement` (code `kr`) | `[I:]` · voice | flag: the following `[K:]` is a redundant restatement that must survive ABC dedupe | `[I:cr kr] [K:F]` |
| `croma-meter-restatement` (code `mr`) | `[I:]` · voice | flag: the following `[M:]` is a redundant restatement that must survive ABC dedupe | `[I:cr mr] [M:4/4]` |
| `croma-time-symbol` | `%%`+`[I:]` · score/measure | `<time symbol="common\|cut">` when the `M:` display is numeric (not `C`/`C\|`) | `[I:croma-time-symbol symbol=cut]` |

### Cursor & structure
| Carrier | Vehicle · scope | Carries | Example |
|---|---|---|---|
| `croma-musicxml-forward` (code `mf`) | `[I:]` · note | flag: re-emit `<forward>` (silent cursor advance) for this invisible-rest gap, not a `<rest>` | `[I:cr mf]` |
| `croma-musicxml-sequence-backup` | `[I:]` · voice | the explicit `<backup>` duration between two voice sequences in a measure | `[I:croma-musicxml-sequence-backup n=3 d=8]` |
| `croma-musicxml-tuplet` | `[I:]` · note | a tuplet whose `actual_notes` is outside ABC's `(p:q:r` range (2..=9): id, actual, normal, role | `[I:croma-musicxml-tuplet id=1 actual=11 normal=8 role=start]` |
| `croma-after-grace` | `[I:]` · note | flag: the following `{…}` grace group is an *after*-grace bound to the preceding note | `[I:croma-after-grace]{de}` |
| `croma-clef-cursor` | `[I:]` · note · **hex** | a mid-tune `<clef>` wrapped in `<backup>`/`<forward>` (clef text + back/pre-back durations) | `[I:croma-clef-cursor clef="bass" back-n=1 back-d=4]` |
| `croma-barline-style` | `[I:]` · measure | a dashed barline (`<bar-style>dashed</bar-style>`) — ABC has no glyph | `[I:croma-barline-style style=dashed]` |
| `croma-measure-number` | `[I:]` · measure · **hex** | a `<measure number=…>` that differs from croma's canonical 1-based index (pickup `0`, labels) | `[I:croma-measure-number n=0]` |
| `croma-ending-close` (code `ec`) | `[I:]` · measure | an explicit volta-bracket close (`<ending type="stop\|discontinue">` + side + numbers) | `[I:cr ec t=d l=r n="1,2"]` |
| `croma-xvoice-slur` | `[I:]` · note | one end of a slur that does not pair within its own `V:` stream — a `<slur>` reaching into/out of another voice, which `(`/`)` cannot span: a shared `pair=` re-pairs the two ends across voices, `role=start\|stop` is which end | `[I:croma-xvoice-slur pair=7 role=start]` |

> Not a carrier: `croma-fmt` is the name of the formatter crate (`crates/croma-fmt`),
> not an ABC token. A few facts also ride encoded in a **decoration name** rather than an
> `[I:]` field (e.g. tremolo `!musicxml-tremolo-start-2!`, whose `-tm-a-n` suffix carries
> the time-modification ratio) — same idea, different vehicle; see `musicxml/notation.rs`.

## 7. Compatibility (other tools ignore carriers)

The `croma-` namespace is what buys compatibility:

- **Inline `[I:croma-…]` / `[I:cr …]`** — ABC 2.1 §3.1.2 defines `I:` as the instruction
  field and says a reader ignores instructions it does not recognise. abc2midi /
  abcm2ps / abcjs skip an unknown `I:` instruction, compact or long. The carrier
  therefore renders/plays as nothing in other tools; the music around it is untouched.
- **Header `%%croma-…`** — ABC 2.1 §11 calls `%%` lines stylesheet directives and lets a
  reader ignore unrecognised ones (a non-directive-aware tool treats the `%`-leading line
  as a comment outright).

So the file stays a valid, playable ABC tune everywhere; only croma reads the carriers.

**croma-vs-croma forward compatibility is *not* automatic.** An inline `[I:croma-*]` (or
`[I:cr <code>]`) that the current croma does not recognise falls through
`apply_inline_field` (`lower/mod.rs`, the `'I'` arm tail) and is **dropped with an
`inline_instruction_ignored` diagnostic** — it is *not* preserved verbatim. So an older
croma reading a tune written by a newer croma loses any newer carrier (and an older
croma that predates compact spelling drops every `[I:cr …]` it meets, long-spelling
fallback notwithstanding). If a future need requires cross-version preservation, that
would be a deliberate change (keep unknown `croma-*` instructions as an opaque preserved
directive), not the current behaviour.

If those forward-compatibility warnings — or the deprecated-long-spelling warning from
[§3](#3-compact-spelling-icr-code) — are expected noise in a workflow, library callers
can use `ParseOptions::suppress_croma_carrier_warnings()` (or
`ExportOptions::suppress_croma_carrier_warnings()`), and the CLI exposes the same
filter as `--silence-croma-carrier-warnings`. The filter applies to private
`croma-` carrier names and to croma-managed `%%MIDI` / `[I:MIDI]` playback
carriers; ordinary unsupported directives still warn.

## 8. Adding a carrier (checklist)

Only add a carrier for a **meaningful** construct ABC genuinely cannot express. Do *not*
carry degenerate/no-op source data (e.g. redundant net-zero cursor moves, dangling ties)
— dropping those is correct, and a carrier there would re-introduce noise. Never add a
generic "raw-XML blob" carrier to the default writer: it makes the round-trip metric green
while making the ABC un-editable (an editor can't touch the blob, and editing the notes
makes it stale).

A new carrier gets a **compact code only if it is frequent** enough across the corpus for
the code to pay for its registry entry — the existing seven were the corpus's most common
carriers and, coded, cut exported ABC by ~14.5% (see [§3](#3-compact-spelling-icr-code)).
Default to leaving a new carrier long-spelling only; only add a code with corpus evidence
it earns its keep. If it does qualify, the two registries **must change together**:
`croma_core::lower::carrier::COMPACT_CARRIERS` (`crates/croma-core/src/lower/carrier.rs`,
crate-private) and its mirror `COMPACT_CARRIERS` in `crates/croma-fmt/src/fixes.rs` — the
first drives what the reader expands and the writer emits, the second drives what
`croma fmt` migrates; if only one is updated, `croma fmt` and the reader disagree about
the carrier's spelling.

1. **Model** — add the field the carrier rides on (`EventAttachments`, `Voice`, `Measure`,
   `KeySignatureModel`, …).
2. **Reader** (`musicxml/read/`) — read the foreign `<element>` into that field.
3. **Emit** (`to_abc.rs`) — a `*_instruction(...)` builder; call it at the right writer
   position; use the `-hex=` variant for any free-text field; emit compact only if the
   carrier is coded (step above).
4. **Apply** (`lower/mod.rs`) — a `parse_<thing>_instruction(...)`; dispatch it in
   `apply_inline_field`'s `'I'` arm (or the header path); stage in a `pending_*` slot and
   **drain onto the next event on both the note and chord paths** (`lower/voice.rs`). If
   coded, also extend `expand_compact_carrier`/`compact_code_for_long`
   (`lower/carrier.rs`) and croma-fmt's mirror table.
5. **Re-emit** (`musicxml/`) — write the original `<element>` from the model field.
6. **Test** — a TDD round-trip test (see `musicxml/read/mod_tests.rs` `foreign_*` helpers
   and `musicxml/mod_tests.rs` export tests); if coded, also a `carrier_tests.rs` /
   `croma-fmt` migration test for the new code.
7. **Gates** — reader self-loop must stay at its baseline (currently **135** structural
   diffs over the 10k ABC corpus), fmt-lossless **0 not-idempotent / 0 notes-changed**,
   no new PDMX regression (uncapped record diff). A forward-writer change re-proves both
   corpus gates; a reader-only change only needs PDMX + the unit suite.

## 9. Known limitations

- **`croma fmt --check` can name a migration that did not happen.** `--check` reports
  which long-spelling carriers plain `croma fmt` would compact by calling
  `carrier_migrations` — the *pre-gate candidate list* — before it separately checks
  whether the file is already formatted. The actual migration
  (`migrate_carriers` in `crates/croma-fmt/src/fixes.rs`) is gated at runtime: it is kept
  only if the migrated source still lowers and renders byte-identical MusicXML, and
  dropped wholesale otherwise (see [§8](#8-adding-a-carrier-checklist)'s gates). On a file
  that fails to lower, `--check` still prints a `migrated [carrier-compaction] …` line for
  a migration the gate then rejects. The file itself is left untouched and the reported
  "would reformat" exit code is still correct — only that one informational line is
  misleading in this case.

See also [`midi-directives.md`](midi-directives.md) for the related `%%MIDI`
preserve-verbatim + forward-translate policy, and
[`musicxml-reader.md`](musicxml-reader.md) for the reader pipeline.
</content>
