# croma fix candidates (surfaced by divergence triage)

Real **croma** bugs found while triaging the raw-comparator worklist. A file is
**kept** in the worklist (not dropped) until its bug is fixed, then graduates into
`whitelist.csv` on the next run. Each was confirmed by the
`abc-divergence-investigator` reasoning from the ABC 2.1 spec, with an adversarial
"find a croma error" pass where noted. Investigated 2026-06-13.

## Resolved

### Bug 1 — accidental dropped on the misplaced-length token (`^/c`) — FIXED 2026-06-13

When an accidental (`^` `=` `_`) was immediately followed by a `/` (or digit)
length operator **before** the note (e.g. `^/c`, `a^/ge`), croma discarded the
accidental and emitted the note natural. abc2xml leniently recovers the
accidental.

- **Spec:** ABC 2.1 §4.2 (KB raw line 855) — `^`/`=`/`_` notate sharp/natural/flat;
  §4.20 construct order is `<accidental><note><octave><length>`. The token is
  malformed, but the author's intent (a sharp) is unambiguous (cf. parallel
  well-formed `^c` bars).
- **Fix:** `parse_accidental_or_malformed` now recovers — when a misplaced length
  run sits between the accidental and a note, it flags the length as
  `malformed_length` (still not applied to the note's duration) but attaches the
  accidental to the following note instead of dropping it.
  (`crates/croma-core/src/parse/music.rs`, test in
  `crates/croma-core/src/lower/mod_tests.rs`).
- **Graduated:** `tune_001009`, `tune_002562`, `tune_001875`, `tune_003353`.
- **Note:** registering this fix required upgrading the comparator to compare the
  sounding `pitch.alter` rather than the display-accidental name — abc2xml emits a
  self-contradictory `<alter>1>` + `<accidental>natural>` glyph at these tokens, so
  a name-based comparison could not see that the corrected croma sharp now sounds
  identical. That comparator change also graduated 8 contradictory-glyph files that
  were previously parked in `dropped.csv` as `equivalence`.

### Bug 2 — explicit key signature `K:<tonic> exp <accidentals>` — FIXED 2026-06-13

A space-less explicit accidental list (`K:D exp _B^g`, `K:D exp ^f_B_e`) arrives as
a single token, and the key parser read only the **first** accidental per token
(`_B`), dropping the rest (`^g`). The dropped pitches then resolved to natural.
(Space-separated lists like `K:D exp _b _e ^f` already worked, and `key_fifths`
already returns 0 for `exp`, so the per-note resolution and key-step emission were
correct — the bug was purely the parser dropping accidentals.)

- **Spec:** ABC 2.1 §3.1.14 (KB raw line 688) — `K:<tonic> exp <accidentals>`
  explicitly defines **all** the accidentals; `K:D Phr ^f` ≡ `K:D exp _b _e ^f`,
  so the tonic contributes nothing beyond the explicit list.
- **Fix:** `parse_key_accidentals` (in `crates/croma-core/src/parse/field/key.rs`)
  now walks the whole token, capturing every `<sign><note>` pair. Test in
  `crates/croma-core/src/parse/field/mod_tests.rs`.
- **Outcome:** croma is now spec-correct for `exp` keys (corrected ~56 corpus rows
  — the G♯/E♭/F♯ that were dropped). `tune_003838`/`tune_003836` do **not** fully
  graduate because abc2xml **over-reaches**, injecting the tonic's D-major F♯/C♯ on
  top of the explicit list; per §3.1.14 those bare F/C are natural, so croma is
  correct and the files are dropped as `abc2xml-accidental` (residual abc2xml bug).

### Bug 3 — illegal post-barline tie `a|-a` — FIXED 2026-06-13

A tie `-` placed immediately **after** a barline (`d4|-{c}d2`) was bound backward
across the barline to the pre-barline note, fabricating an illegal cross-bar tie.
ABC 2.1 §4.11: a tie must be adjacent to the **first** note of the pair — the legal
cross-bar form is `a-|a` (`-` before the bar); `abc|-cba` (`-` after the bar) is
"not legal".

- **Spec:** ABC 2.1 §4.11 (KB raw line 1048) — "The tie symbol must always be
  adjacent to the first note of the pair ... `abc|-cba` ... not legal".
- **Fix:** `apply_tie` (in `crates/croma-core/src/lower/tie.rs`) now rejects a tie
  marker when no timed note exists in the current measure
  (`broken_left_available` is false — the same §4.4 anti-cross-bar state used for
  broken rhythm, reset at every barline but surviving line breaks), emitting
  `unmatched_tie` instead of binding backward. Legal `a-|a` cross-bar and
  cross-line ties are unaffected. Test in `crates/croma-core/src/lower/mod_tests.rs`.
- **Graduated:** `tune_008162`, `tune_008163`, `tune_008166`, `tune_008168`,
  `tune_011106`, `tune_014796`.

### Bug 10 — empty `Q:` field emits a content-free `<words></words>` direction — FIXED 2026-06-13

A `Q:` field with no value (`Q:\n`, the only such file in the corpus) carries neither a
numeric tempo nor a quoted text string, yet croma emitted a degenerate empty
`<direction><direction-type><words></words></direction-type></direction>`. abc2xml emits
nothing, and croma already suppresses empty `""` annotations (cf. the dropped
`abc2xml-phantom-direction` files), so this was an inconsistency, not a policy call.

- **Spec:** ABC 2.1 §3.1.8 (KB raw line 579/591) — `Q:` defines beats-per-minute with an
  *optional* quoted text string; an empty `Q:` supplies neither, so nothing is printed.
- **Root cause:** an empty `Q:` produces `metadata.tempo` with blank text and no
  `tempo_model`, so the fallback branch in `write_initial_directions`
  (`crates/croma-core/src/musicxml/direction.rs`) emitted it verbatim. Now guarded on
  `!tempo.text.trim().is_empty()`. Test `empty_tempo_field_emits_no_direction`
  (`crates/croma-core/src/musicxml/mod_tests.rs`); text-only and numeric `Q:` paths
  unaffected (their tests still pass).
- **Graduated:** `tune_011103` (whitelist 9,259 → 9,260; `git diff whitelist.csv` = +1
  insertion, 0 removals — no regression).

## Undetermined / co-fault (kept in worklist, flagged for human — NOT a clean fix)

### empty-bar collapse in multi-voice — spec-ambiguous, do not "fix" lightly

croma collapses runs of consecutive **bare** empty bars (`... | | | | ...`):
`tune_011865.abc` lower voices come out 10 measures vs the upper voice's 17, a
real multi-voice misalignment. **But this is not a clean croma bug** and a fix was
deliberately deferred (root-caused 2026-06-13):

- The collapse is **intentional and §4.8-grounded**: a *run of bar lines*
  (`||`, `|]`, `]|`, split barlines) is **one boundary**, not multiple measures.
  croma's `is_empty_measure`/barline coalescing (`crates/croma-core/src/lower/timeline.rs`)
  implements exactly this and has tests for `||`/`|]`/pickup measures. The bare
  `| | | |` runs are literally consecutive barlines → §4.8 = one boundary.
- The **source is internally inconsistent**: the upper voice fills empty bars with
  `z` rests (→ kept, 17 measures) while the lower voices use bare `| |` (→ collapsed).
  ABC 2.1 §7's silent-voice-alignment example uses **explicit `x` rests**
  (`[V:B2] x8 | x8`), not bare barlines — so the lower voices are non-standard.
- A gate fix (skip coalescing in multi-voice) would **break legitimate `||`/`|]`
  handling**; distinguishing `||` (adjacent = one boundary) from `| |` (intended
  empty measure) needs source-adjacency info the timeline no longer carries.
- It **won't graduate the file**: abc2xml drops the empty bars to 6 measures, so
  even croma→17 stays a mismatch.

Disposition: `tune_011865` kept in the worklist (not dropped, not fixed),
flagged `undetermined`. If revisited, a real fix must preserve §4.8 barline-run
coalescing while distinguishing intended empty measures — likely needs the parser
to carry barline-adjacency into the timeline.

### Bug 5 — whitespace-surrounded lone `:` treated as malformed, not a barline (DEBATABLE — deliberate behavior)

croma rejects a `:` with **whitespace on both sides** as a barline: `parse_colon`
(`crates/croma-core/src/parse/barline.rs`) already accepts a `:` *adjacent* to
notes as a Liberal barline (case 4, fixed 71 files), but a free-floating `: `
deliberately becomes an `invalid_barline` malformed stray dot. In
`tune_005539.abc` (`... d3/2e/ : d/B/c/d/ ... :|`) croma drops the three
space-surrounded `:` and collapses 4 bars into one 32-quarter measure (4× the
`M:8/4` meter); abc2xml segments into 4.

- **Not a clean fix — deliberate & tested:** the case-5 behavior is asserted by
  `recovers_invalid_barline_fragments_as_skipped_malformed_items` (`C : D` →
  `invalid_barline` + `InvalidBarline`). "Fixing" it = relaxing case 5 so a
  whitespace-surrounded `:` becomes a Liberal barline, **reversing a tested
  decision**.
- **Spec-debatable:** §4.8 line 1001 ("liberal ... sequence of `|` and `:`") and
  abc2xml favour treating it as a barline; croma's stance is that a free-floating
  `:` is an ambiguous stray dot. The investigator rated croma at-fault high (the
  4×-overfull measure is clearly bad output), but croma's choice is defensible.
- **Disposition:** kept in worklist, flagged `undetermined`. A real fix is a
  one-site change (case 5 → `parse_barline`) but must be regression-tested across
  the corpus and the `C : D` test updated; it's a deliberate policy reversal, so
  it needs an explicit decision, not a drive-by fix.

### Bug 6 — dangling lyric hyphen rendered `single`/`end`, not `begin`/`middle` (DEBATABLE — deliberate, tested gate)

When a lyric syllable carries a trailing hyphen but **no continuing syllable
attaches to a following note** — the hyphen is followed by `*` (skip), ends the
`w:` line/tune, or the would-be continuation lands in a different verse under a
repeat — croma drops the hyphen and emits MusicXML `<syllabic>single</syllabic>`
(word-initial) or `end` (word-medial). abc2xml stays faithful to the authored
hyphen and emits `begin`/`middle`.

- **Surfaced by** (lyric category, all `croma`/high from the investigator, but
  reclassified **undetermined** here): `tune_002758` (`di-`, last note → `end` vs
  `middle`), `tune_011626` & `tune_011627` (`be-*` → `single` vs `begin`),
  `tune_008447` (`af-`, `Sav-` → `single` vs `begin`). All four kept in the
  worklist; **none dropped**. No `abc.lyric.syllable_count` diagnostic fires.
- **Deliberate & tested:** the hyphen is only recorded when
  `future_syllable_can_attach(...) || future_same_verse_syllable_can_attach(...)`
  holds (`crates/croma-core/src/lower/align.rs:148`). Tests pin this both ways:
  `orphan_lyric_hyphen_does_not_start_syllabic_word` asserts dangling-hyphen →
  `single`, while `lyric_hyphen_across_non_adjacent_blocks_exports_begin_end`
  asserts a hyphen **with** a real future continuation → `begin`/`end`. So croma
  already does begin/end for genuinely-continued words; it only collapses the
  *dangling* case.
- **Spec-debatable:** §5.1 (KB raw line 1383) defines `-` as "break between
  syllables within a word" but is silent on a hyphen whose next syllable has no
  note. MusicXML `<syllabic>begin</syllabic>` with no matching `end` is arguably
  malformed, so croma's locally-consistent `single` is defensible; abc2xml's
  `begin` is merely faithful to the source token. The four files also involve
  non-standard input (multi-verse `*`-skip alignment under repeats, end-of-tune
  hyphens) where the verse a continuation belongs to is itself ambiguous.
- **Disposition:** kept in worklist, flagged `undetermined`. A "fix" = relaxing the
  `future_syllable_can_attach` gate so any authored trailing hyphen yields
  `begin`/`middle`, which **reverses `orphan_lyric_hyphen_does_not_start_syllabic_word`**
  and may still not graduate the files (the `*`-skip/verse alignment need not match
  abc2xml's begin/middle/end run). Deliberate policy reversal → needs an explicit
  human decision, not a drive-by fix. Same class as Bug 4 / Bug 5.

### Bug 9 — leading `[|` thick-thin glyph suppressed (DEBATABLE — deliberate, abc2xml also wrong)

`tune_006695` (`!segno![|` at body start): croma treats `[|` (`BarlineKind::Initial`,
thick-thin per §4.8 line 986 → MusicXML `heavy-light`) as a plain measure opener and
emits **no** right-barline glyph (`to_abc.rs:824-828`; test
`initial_barlines_do_not_emit_musicxml_heavy_light`, `mod_tests.rs:978`). abc2xml emits
`light-heavy` — which is the *thin-thick* (`|]`) mapping, also wrong. Investigator rated
croma at-fault **medium**. Deliberate, tested suppression of a visual-only leading
barline + a wrong reference value ⇒ won't graduate even if "fixed"; flagged
`undetermined`, kept, human call. Same class as Bug 4/5/6.

### whitespace-separated barline tokens dropped (`| |`, bare `:`) — same family as Bug 5

- `tune_005339` (`E12 z4 | |`, end of tune): croma treats the space in `| |` as a hard
  separator, splitting it into two single bars (the second a zero-content bar that
  produces no measure) and emits **no** final barline; spec §4.8 line 985 + §4.9 (space
  adjacent to a bar is non-significant, `| [1` legal) → the intended thin-thin double
  (`light-light`, abc2xml's value). Isolated repro: `E4 ||` → `light-light` ✓ but
  `E4 | |` → no barline ✗. Investigator **high**.
- `tune_008105` (`...E4D4:` closing a `|:` section): bare trailing `:` (dropped-`|`
  repeat-end) → croma emits no barline at all; abc2xml emits `dotted` (also wrong).
  Investigator **medium**.
- **Why not a drive-by fix:** identical mechanism to Bug 4 (empty-bar collapse) and
  Bug 5 (free-floating `:`) — croma's barline tokenizer treats whitespace between
  barline glyphs as significant, and the timeline no longer carries the source-adjacency
  needed to distinguish `||` (one boundary) from `| |` (intended double / empty measure).
  A real fix is the same architectural change Bug 4 flagged. Kept, `undetermined`,
  human policy call.

### Bug 12 — unsupported decorations preserved as `<words>` instead of ignored (DEBATABLE — deliberate preserve-don't-drop policy)

`tune_014712` (`!-(!`, `!-)!` in the body): croma renders unknown/unimplemented
decorations as visible `<direction><words>-(</words></direction>` text (and warns
`abc.musicxml.decoration.unsupported: Decoration '-(' is preserved as MusicXML direction
text`), whereas §4.14 (KB line 1198) says an "unimplemented or unknown symbol … should be
ignored" — abc2xml ignores them. All legitimate decorations (wedges, accent, up-bows) and
all 7 notes match; only the 2 fabricated word-directions differ.

- **Why debatable, not a clean fix:** croma's preservation is a *deliberate* policy (explicit
  warning + fallback code) favoring "don't silently drop author markup" over §4.14's "should
  ignore" (a recommendation, not "must"). Reversing it is a policy decision like Bug 9. **But**
  it likely graduates *many* files (any tune with unknown decorations currently mismatches as
  `extra_in_croma`, and the fix aligns croma with abc2xml — low regression risk since such
  files are already off the whitelist), so it's a strong candidate for the focused fix session
  *if* the policy call is to follow §4.14. Kept, flagged for a human decision.

## Open — clean croma bugs found in barline triage (2026-06-13, NOT yet fixed)

These are **genuine croma defects** (clear spec violations, abc2xml correct, no test
asserts the current behavior), kept in the worklist as fix candidates. Deferred to a
focused fix session — the barline subsystem is heavily tested (≈6 leading-barline-policy
tests + the 9,259-match whitelist), so each needs TDD + a full corpus regression run,
not a triage drive-by. Fixing Bug 7/8 should graduate the listed files.

### Bug 7 — invisible barline `[|]` dropped on an annotation-only measure (HIGH)

`tune_004088`, `tune_010209` (`"^A"[|] ...`): croma logs
`info[abc.musicxml.barline_policy]: Invisible barline is exported as a MusicXML none
bar-style` but emits **no `<barline>`** — self-contradicting its own stated policy.
abc2xml correctly emits `<barline location="right"><bar-style>none</bar-style></barline>`.

- **Spec:** §4.8 line 999 — "An invisible bar line may be notated … e.g. `[|]`" → MusicXML
  `bar-style none`.
- **Root cause:** the export is correct (`barline.rs:27` maps `Invisible → none`); the
  bug is in `unique_barlines` (`crates/croma-core/src/musicxml/score.rs:319-349`): a
  closing-style barline (`Invisible`/`Double`/`Final`/`Dotted`) that **leads** its
  measure (annotation-only measure: `is_leading_barline` true) matches neither the
  right-barline filter (requires `leading=false`) nor the left-barline filter (only
  `RepeatStart`), so it is silently dropped. Confirmed in isolation: `"^A"[|] CDEF`
  drops the barline; `C[|] DEFG` (note first → non-leading) emits `none` correctly.
- **Fix sketch:** emit closing-style barlines as the measure's right barline regardless
  of `leading`, WITHOUT regressing `leading_double_and_final_barlines_do_not_create_empty_measure`
  (leading `||`/`|]` *before content in the same measure* must stay absorbed). Needs
  model tracing of how an annotation-only measure sets `source_span` vs the barline span.

### Bug 8 — split left-barline edge drops the forward repeat (`:|:[2`) (HIGH)

`tune_005957` (`...:|:[2 ...`): a forward-repeat (`|:`) and a second-ending start (`[2`)
land on one measure's left edge. croma emits **two** separate `<barline location="left">`
elements (first bar-style+repeat, second the ending); music21 (a standard consumer)
keeps only the second and **silently drops the forward repeat** (reads `type=regular`).
abc2xml emits **one** `<barline location="left">` carrying bar-style + ending + repeat
together.

- **Spec:** §4.8 line 993 (`:|:` equivalences) + §4.9 line 1019 (variant endings) — the
  forward repeat and the ending-2 start share the single measure-9 left edge.
- **Root cause:** croma serializes the repeat and the ending as two `<barline
  location="left">` elements for the same edge instead of merging them into one. All
  other variant-ending measures in the tune (single combined barline) match.
- **Fix sketch:** when both a left-repeat and an ending-start fall on the same measure
  edge, emit a single `<barline location="left">` containing bar-style + `<ending>` +
  `<repeat>` (the MusicXML content-model order already in `write_barline`).

### Bug 11 — chord-internal slur marks collapse onto the chord head (MEDIUM / entangled)

`tune_007779` (`[(C(G] ... [G,,)G,)]`): an ABC chord with per-note slur marks (`(`/`)`
adjacent to specific chord members) is serialized by croma with **both** slur-starts on
the first chord note (C) — G gets none — and both stops on one member; abc2xml honors
per-note placement (start#1 on C, start#2 on G). §4.11 stresses the into/out-of/between-
chord slur distinction is "particularly important." Demonstrable collapse, but rated
**medium**: the exact reported endpoint figures are second-order artifacts of music21
flattening chords + the source being slur-unbalanced (P1 4 opens/3 closes; croma warns
`unclosed_slur`/`unmatched_slur`). Kept as a fix candidate; corpus impact unclear until
the chord-internal placement is fixed and re-measured.

### Bug 13 — closing barline dropped on a note-less (directive+spacer) measure (HIGH)

`tune_003124` (`...!segno!y |]`): a measure whose only content is a directive (`!segno!`)
plus a spacer `y` — no real note — makes croma **drop the `|]` closing barline** entirely.
abc2xml emits `<bar-style>light-heavy</bar-style>`. Controlled repro isolates it: `ded dBc |]`
(note), `y |]` (spacer only), and `!segno!c |]` (segno+note) all emit `light-heavy`, but
`!segno!y |]` (segno+spacer, no note) drops it — croma suppresses the barline when the
measure has no note event.

- **Spec:** §4.8 line 984 — `|]` thin-thick double bar → `light-heavy`.
- **Same family as Bug 7** (closing barline dropped on a content-less / annotation-only
  measure): croma's barline emission appears gated on the measure having a real note event.

### Bug 14 — liberal repeat-end `:]` normalized away to a bare boundary (HIGH)

`tune_000746` (`...d4 :]`): the liberal spelling `:]` (= `:|]`, repeat-end + thin-thick
final bar) makes croma emit **no `<barline>` at all** and warn
`abc.music.barline.liberal: Liberal barline spelling ':]' was normalized as a measure
boundary` — discarding both the backward repeat and the final style. abc2xml emits
`light-heavy` + `<repeat direction="backward"/>`. croma handles ordinary `:|` correctly
elsewhere in the same tune, so this is the `:]`/liberal-spelling path dropping the style.

- **Spec:** §4.8 lines 988/1001 — `:` is the end-of-repeat dots; `]` a thick bar under
  liberal recognition, so `:]` ≡ `:|]`.
- **Same cluster:** croma's *liberal-barline normalization* reduces non-canonical spellings
  (`:]`, and the whitespace `| |` of Bug 9) to a bare boundary, discarding the style/repeat.

> **Barline croma-bug cluster (the session's main fix lead).** Bugs 7, 8, 13, 14 (clean,
> HIGH) plus the whitespace `| |` family (Bug 9, debatable) share two root themes worth one
> focused fix session: (a) croma **gates barline emission on a real note event**, so a
> closing barline on a note-less measure (annotation/spacer/empty) is dropped (Bug 7, 13);
> and (b) croma's **liberal-barline normalization** discards the style/repeat of
> non-canonical spellings (`:]` Bug 14, `| |` Bug 9, bare `:` in tune_008105). A fix here
> would graduate many barline-category files (181 single-cat in the worklist). High
> regression risk (≈6 leading-barline-policy tests + the 9,260 whitelist) → needs TDD +
> full corpus regression, the reason it is deferred from triage.

> Bugs 4, 5, 6, 9, 12 and the whitespace-barline family remain **deliberate/debatable
> policy calls** (kept, flagged for a human), distinct from these clean defects.
> Bug 11 is a real fidelity gap but medium-confidence / entangled with malformed input.
