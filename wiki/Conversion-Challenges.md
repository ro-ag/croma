# Conversion challenges

croma is a long project, and converting between **messy real-world ABC** and
**MusicXML** — in *both* directions — surfaced a lot of genuinely hard cases.
This page collects the notable ones, grouped by area, each with how it was
resolved: **fixed**, **adjudicated** (a documented, spec-justified
non-bug), or **deferred** (out of scope, with a reason). The full archaeology
lives in the per-area repo docs linked at each section.

Every item below is drawn from croma's own docs — nothing here is folklore. The
numbers are the final gate values:

| Gate | Value |
| --- | --- |
| ABC → MusicXML writer parity vs abc2xml (raw) | **9,390 / 0** |
| Formatter idempotent + lossless | **10,000 / 0** |
| Reader self-loop XML idempotence | **9,935 / 9,935** |
| Reader → ABC structural round-trip (internal proof) | **9,724 / 9,933** (209 diffs; 207 sounding-equal + 2 HARD) |
| Reader foreign-dialect parity vs music21 | **9,850 / 10,000** (98.50%) |

## The cross-cutting tension: strict spec vs. lenient reference

The challenge under all the others. [`abc2xml`](https://wim.vree.org/svgParse/abc2xml.html)
(the correctness baseline) is **permissive** and inserts heuristic artifacts; the
10k corpus is **messy real-world ABC**. croma's whole stance is the answer:
a **strict** parser with a three-tier recovery rule (reject / recover-and-warn /
reject), a **formatter** that repairs loose source explicitly, and **every
divergence adjudicated** rather than silently matched. Each specific challenge
below is an instance of holding that line. → [[How-its-Proven]],
[[abc2xml-Comparison]], [[FAQ]].

## Forward path (ABC → MusicXML): parser & model gaps

Found while proving the writer over the corpus. A representative set (all closed
with regression tests; the full 14-item ledger with its `FIXED`/`TRIAGED`
archaeology is in
[`docs/parser-backlog.md`](https://github.com/ro-ag/croma/blob/main/docs/parser-backlog.md)):

| Challenge | What went wrong | Resolution |
| --- | --- | --- |
| **Mid-tune key/meter changes** (~5.5% of tunes — the single biggest unlock) | `[K:]`/`[M:]` and body `K:`/`M:` lines changed parser state but left no event in the lowered model; no writer could reproduce them | added `KeyChange`/`MeterChange` events flowing through lowering → MusicXML `<attributes>` |
| **Silent data loss** of quoted text | chord symbols, annotations, decorations, and grace groups before a barline or before a `{…}` / `(` / `(3` marker (`"F"{AB}c`, `"G7"(DE)`) were dropped | buffered in the lowering state and flushed correctly; leftovers warn (`dangling_quoted_text` / `dangling_grace_group`) |
| **Ties + accidental carry** | a tie to a non-adjacent same-pitch note was dropped, yet its accidental carry wrongly survived the barline (`^a- \| b a a`) | tie provenance; an unfinished tie undoes its barline-preserved carry |
| **Rest-led tuplets** | `(3zBA` lost the tuplet start; `(3BAz` lost the stop | tuplet roles now attach to any timed event, rests included |
| **Nested tuplets** | collapsed to the innermost level or saturated the ratio | composite `<time-modification>` + ordered start/stop notations |
| **Bare-grace slurs** | `({Bc})` slur markers drifted to the neighbouring note | attach to the grace group itself |
| **Lyrics** | `<syllabic>` was always `single`; `+:` continuations joined with a raw newline; orphan hyphens stored a dangling control | hyphen-control tracking per voice+verse, emitting `begin`/`middle`/`end`/`single` |
| **Overlay voice collisions** | simultaneous overlays from different source voices collided on `<voice>` number | part-wide stable overlay voice numbers |

## Robustness: never panic on adversarial input

| Challenge | Resolution |
| --- | --- |
| **Arithmetic overflow panics** — `V:1 clef=treble+15 octave=125` and absurd octave-mark runs panicked at `i8` sites | accumulate in `i32`, clamp `octave=` to ±9 and the total to ±12, saturate the additions; the writer's replica clamps identically |
| **Truncated corpus fragments** — 65 header-only files (the body cut off at extraction) fail `abc.file.no_music`; abc2xml fakes an empty voice | triaged as a **corpus artifact**, not a parser bug — matching abc2xml would add no real coverage |

The no-panic discipline is what later let the language server prove **totality**
(0 panics / 0 hangs over 10k) — [[Testing-Methodology]].

## Where croma intentionally differs from abc2xml

abc2xml inserts spurious elements as heuristic side effects — most often **empty
leading / section measures** and **phantom measures**. croma emits spec-faithful,
**minimal** MusicXML and declines them. Most croma ↔ abc2xml divergences are
*exactly* such an artifact croma omits, and each is an **adjudicated drop** (croma
strict-correct, abc2xml lenient), never a silent guess. → [[abc2xml-Comparison]].

## `%%MIDI`: carrying meaning from a non-spec convention

`%%MIDI` is an **abc2midi convention, not ABC 2.1**, and several of abc2xml's
`%%MIDI` behaviours are incidental or buggy (it drops a header `program` placed
before the `V:` lines, crashes on a bare `%%MIDI channel 10`, writes a literal
`no name` instrument filler). The challenge: carry the genuine score-level meaning
into MusicXML **without** mimicking those quirks and **without** breaking the
formatter's losslessness.

Resolution — **two decoupled paths**:

1. **Preserve verbatim, always.** Every `%%MIDI` line is stored and re-emitted
   unchanged, so `croma fmt` round-trips it in place (this is what keeps croma
   lossless).
2. **Forward-translate the score-meaningful parts into MusicXML only** —
   `program` / `channel` / `control 7` (volume) / `control 10` (pan) /
   `transpose`, in both line-start and inline `[I:MIDI=…]` forms, **per-voice
   scoped**. It does *not* mimic abc2xml's visible `prog:` staff text or its
   `drummap` percussion extension.

**Sub-challenge — comparator blindness.** The raw corpus comparator extracts *no*
instrument facts from music21 and compares *written* pitch, so every `%%MIDI`
translation is **invisible** to it (0 whitelist graduations by construction).
These features are therefore proven by **targeted unit tests + byte-level abc2xml
spot-checks**, not the corpus gate. Full policy:
[`docs/midi-directives.md`](https://github.com/ro-ag/croma/blob/main/docs/midi-directives.md).

## Reverse path (MusicXML → ABC): inverting through a lossy intermediate

The reader was built as the **exact inverse of croma's own writer**, staged
S1–S6 and proven by **self-loop XML idempotence** —
`write_musicxml(read_musicxml(xml)) == xml` — driven to **9,935 / 9,935 (zero
residual)**.

The deeper difficulty: **MusicXML is a lossy intermediate for ABC**, so the
reader → ABC structural round-trip caps at **97.9% (9,724 / 9,933; 209 diffs)**.
The decisive triage test — *does any sounding fact (pitch + alter + octave +
duration) get dropped or added?* — shows **207 of 209 are valid-but-different**
(sounding-equal structural equivalence the lossy XML cannot byte-match), **not**
defects; only **2** are HARD/degenerate stops. Notable specific cases:

| Challenge | Count | Verdict & resolution |
| --- | ---: | --- |
| **Multi-voice overlay** | 134 | **adjudicated** — an `&`-overlay / `%%staves` tune comes back as explicit sequential `V:` voices instead of a single `&`-overlaid voice; musically identical, forcing `&` is cosmetic |
| **Empty / directive-only measures** | (107 fixed) | a bar that anchors no content was dropped on re-parse → synthesize an inert `Spacer` so the boundary survives; XML stayed byte-identical, 0 regressions |
| **Demoted chord-symbol ordering** | 19 (fixed) | a placement-less quoted string (`"tr"`) is demoted to `<direction><words>` and read back as an *annotation*, reversing its order vs `<harmony>` → read placement-less, trim-stable words back into the chord-symbol channel, preserving document order |
| **Nested-tuplet inverse** | 1 (fixed) | a composite `21/16` `<time-modification>` read back wrong → recover the outer ratio from the post-inner-close tail notes; closed the last self-loop residual (→ 9,935 / 9,935) |
| **`w:` lyric redistribution** | 9 | **adjudicated** — every syllable survives (multiset identical) but a lossy `<lyric>` round-trip lands one on a neighbouring note; a documented deep stop |
| **The 2 HARD residuals** | 2 | a tempo whose text contains `"` quotes has *no ABC escape* and fabricates phantom notes on re-parse (`tune_014467`); a 3-staff multi-voice tangle flips one accidental flat↔natural (`tune_013508`) — both bounded/degenerate, adjudicated |

Reading **foreign** MusicXML (MuseScore / Finale / Sibelius / abc2xml) hits
**98.50%** music21 parity. Full coverage map:
[`docs/musicxml-reader.md`](https://github.com/ro-ag/croma/blob/main/docs/musicxml-reader.md).

## Reverse path: what's deferred (and why)

Following the design rule **"stop where coverage flattens"** — ABC can't express
it, or no corpus file needs it, and no sounding fact is lost:

| Deferred item | Why |
| --- | --- |
| **2-staff-per-part** | ABC has no 2-staff-per-part concept; staff routing collapses to one staff, **0 sounding notes** dropped. Revisit only if classical keyboard / SATB import becomes a goal |
| **Sustain pedal** `<direction>` | ABC 2.1 / 2.2 has no pedal syntax; 0 corpus files; left unread with a diagnostic |
| **3-level nested `<part-group>`** | the `%%score` synthesis handles one level of sub-group; deeper nesting has 0 corpus files and degrades gracefully to the top-level grouping |
| **Header `P:ABAB` play-order macro** | non-trivial play-order semantics targeting the header (not the body); stays dropped with a diagnostic |

(Phase-62 also *added* reverse-path capability: synthesising `%%score` from
`<part-group>` for 351 foreign files, and round-tripping a body `P:` section label
through MusicXML `<rehearsal>`.)

## See also

[[MusicXML-Reader]] · [[Formatter]] · [[abc2xml-Comparison]] ·
[[How-its-Proven]] · [[Testing-Methodology]]. Canonical docs:
[`parser-backlog.md`](https://github.com/ro-ag/croma/blob/main/docs/parser-backlog.md),
[`midi-directives.md`](https://github.com/ro-ag/croma/blob/main/docs/midi-directives.md),
[`musicxml-reader.md`](https://github.com/ro-ag/croma/blob/main/docs/musicxml-reader.md).
