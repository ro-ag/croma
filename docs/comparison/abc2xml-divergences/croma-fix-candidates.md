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

- **Reconfirmed 2026-06-14** (content-category triage, adversarial "find a croma
  error" pass): croma collapses the 9-empty run to 4 and additionally emits the
  surviving empty bars as **content-less `<measure></measure>` shells** (no
  full-measure rest) — invalid MusicXML in its own right, but the same
  empty-measure-handling code as the collapse, entangled with the deferred policy
  call. Pitched-note counts still match the source exactly per voice
  (37/28/9/20/5), so no *pitched* music is lost. Still kept, still deferred — the
  file won't graduate regardless (abc2xml drops the empties to 6 measures).

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

### tonic-less accidental-only `K:^F` rejected (DEBATABLE — spec grammar requires a tonic)

`tune_005044` (`K:^F`, a body key-modifier line): croma rejects the field
(`warning[abc.field.invalid_k]`) and drops the intended F♯, so 9 bare f/F notes in part 2
sound natural instead of sharp (G-minor raised leading tone). abc2xml keeps the running
G-minor signature and adds F♯ (alter=1, no printed glyph). croma **does** honor the
tonic-ful form `K:Gm ^f` (→ F♯), so only the tonic-less `K:^F` fails.

- **Spec-debatable:** §3.1.14's grammar is `K:<tonic> <mode> <accidentals>` and every
  spec example includes a tonic, so a literal reading makes `K:^F` malformed (croma's
  defense); but line 690 ("software … should mark the individual notes … with the
  accidentals that apply") and the universal real-world idiom favor abc2xml. Investigator
  rated croma at-fault **medium**. Distinct from Bug 15 (a *valid* tonic `Bb` wrongly
  rejected over a stray comma — a clean bug); this one is a missing-feature / policy call.
  Kept, flagged `undetermined` for a human decision.

## Open — clean croma bugs found in barline triage (2026-06-13, NOT yet fixed)

These are **genuine croma defects** (clear spec violations, abc2xml correct, no test
asserts the current behavior), kept in the worklist as fix candidates. Deferred to a
focused fix session — the barline subsystem is heavily tested (≈6 leading-barline-policy
tests + the 9,259-match whitelist), so each needs TDD + a full corpus regression run,
not a triage drive-by. Fixing Bug 7/8 should graduate the listed files.

### Bug 7 — invisible barline `[|]` dropped on an annotation-only measure — RESOLVED

**Resolved (verified 2026-06-14):** `tune_004088`/`tune_010209` now match (graduated by
prior barline work; the general direction-only-barline path works). Regression guards
added: `leading_invisible_barline_on_annotation_only_measure_is_emitted`,
`closing_barline_on_directive_and_spacer_only_measure_is_emitted`. Original report below.

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

### Bug 8 — split left-barline edge drops the forward repeat (`:|:[2`) — FIXED 2026-06-14

**Fixed:** when a measure's left edge carries both a forward-repeat and an ending start,
`score.rs` now emits a SINGLE `<barline location="left">` (bar-style + `<ending>` +
`<repeat>`) via `write_ending_barline(.., Some(RepeatStart))` instead of two elements.
Test `forward_repeat_and_second_ending_share_one_left_barline`. Graduated `tune_005957`.
Original report below.


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

### Bug 11 — chord-internal slur marks collapse onto the chord head — FIXED 2026-06-14

`tune_007779` (`[(C(G] ... [G,,)G,)]`) and **`tune_011866`** (`[(B,3/(D3/] [F3/)B3/)]`,
4-voice): an ABC chord with per-note slur marks (`(`/`)` adjacent to specific chord
members) is serialized by croma with **all** slur-starts on the chord's first note and
**all** stops on its last note; abc2xml honors per-note placement (e.g. start#1 on C,
start#2 on G; stop on B3 and on D4 separately). §4.11 stresses the into/out-of/between-
chord slur distinction is "particularly important." Reconfirmed 2026-06-14: both files'
investigators rate **high** (ground-truth note counts match exactly — 119=119 and
284=284 — so no music is dropped; the divergence is purely chord-member slur placement,
and croma's own `unclosed_slur`/`unmatched_slur` warnings are the downstream symptom of
its slur-matcher not handling `(`/`)` bound to individual chord notes).

- **Fix (2026-06-14):** `ChordMemberSyntax` now carries `slur_starts`/`slur_ends`
  (mirroring its `tie`); `parse_chord` captures a chord-internal `(` as the next
  member's start and `)` as the preceding member's end (instead of emitting voice-level
  `MusicItem::Slur`s before/after the chord); `push_chord_group` attaches them to the
  specific member event via the voice-level open-slur stack (so slurs into/out of the
  chord still pair). Test `chord_internal_slurs_bind_to_their_own_member`. Verified:
  croma now puts the start on each member (e.g. `[(C(G]` → start on C **and** G),
  matching abc2xml. 0 regressions. **Partial:** `tune_007779`/`tune_011866` improve but
  do not fully graduate — residual rows come from their **unbalanced-slur sources**
  (007779: 4 opens/3 closes; 011866 multi-voice) where croma's LIFO pairing and
  abc2xml's differ on the leftover; that is a separate, lesser issue than the collapse.

### Bug 13 — closing barline dropped on a note-less (directive+spacer) measure — RESOLVED

**Resolved (verified 2026-06-14):** `tune_003124` now matches; covered by the
`closing_barline_on_directive_and_spacer_only_measure_is_emitted` regression guard.
Original report below.


`tune_003124` (`...!segno!y |]`): a measure whose only content is a directive (`!segno!`)
plus a spacer `y` — no real note — makes croma **drop the `|]` closing barline** entirely.
abc2xml emits `<bar-style>light-heavy</bar-style>`. Controlled repro isolates it: `ded dBc |]`
(note), `y |]` (spacer only), and `!segno!c |]` (segno+note) all emit `light-heavy`, but
`!segno!y |]` (segno+spacer, no note) drops it — croma suppresses the barline when the
measure has no note event.

- **Spec:** §4.8 line 984 — `|]` thin-thick double bar → `light-heavy`.
- **Same family as Bug 7** (closing barline dropped on a content-less / annotation-only
  measure): croma's barline emission appears gated on the measure having a real note event.

### Bug 14 — liberal repeat-end `:]` normalized away to a bare boundary — FIXED 2026-06-14

**Fixed:** `parse_colon` now routes a `:` followed by `]` (as well as `|`) into
`parse_barline`, and `barline_kind`'s liberal arm treats leading dots over a `]` thick
bar as a repeat-end, so `:]` = `:|]` → light-heavy + backward repeat. Test
`liberal_colon_thick_barline_is_a_backward_repeat`. Graduated `tune_000746`.

### Bug 18 — `|`+`|` double bar across a `\` line-continuation seam dropped — FIXED 2026-06-14

`tune_001312` and ~many: a music line ending `...|\` continued to a line starting `|`
forms an adjacent `||` thin-thin double bar, but `merge_continued_barline_run`
(`parse/music.rs`) only coalesced runs where the previous bar carried a `]`, so the plain
`|`+`|` seam was dropped (the bar before the seam got no right barline).

- **Fix:** also merge when both seam glyphs are pipe-only (`|`/`||`) → `||`, but NOT when
  the next line opens a repeat (`|:`/`:|`, not pipe-only) or a variant ending — its
  leading `|` is a deliberate new boundary, not a double-bar component (that over-merge
  regressed `tune_002255`/`tune_013361`; the narrowed rule graduates 45 files, 0
  regressions). Test `double_bar_across_backslash_continuation_is_coalesced`.

### (original) Bug 14 — liberal repeat-end `:]` normalized away to a bare boundary (HIGH)

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

### Bug 15 — `K:` tonic with a trailing comma rejects the whole field — FIXED 2026-06-14

`tune_004340` (`K:Bb, F` — a mid-tune key change): croma emits
`warning[abc.field.invalid_k]: Invalid K: field value was ignored` and **discards the
entire field**, so the key change to B♭ major (fifths −2) is dropped and the prior D
major (F♯/C♯) persists — wrong accidentals on E/B/F/C across the whole section (32
accidental rows; 4 slur rows are the same notes shifted by the wrong key). croma already
parses `K:Bb F` correctly (fifths −2); the bug is triggered specifically by the **stray
comma after the valid tonic** (`Bb,`). Probe: `K:Bb F`→−2 ✓ but `K:Bb,`/`K:Bb, F`→rejected.

- **Spec:** §3.1.14 (KB raw line 661/686) — a key is `<tonic>` (A–G + optional #/b) then
  optional mode/accidentals; `Bb` = 2 flats. The valid tonic should yield fifths −2 and
  the unparseable trailing tokens be discarded (as croma already does for `K:Bb F`).
- **Fix (2026-06-14):** `parse_tonic_token` (`crates/croma-core/src/parse/field/key.rs`)
  now recovers the leading valid tonic when the trailing junk is **non-alphabetic**
  (cannot be a mode or clef word), matching the already-working spaced `K:Bb F`;
  alphabetic remainders (`Bass`, `Cmaj`) still reject as before. Same family as the
  resolved Bug 2. Test `key_tonic_recovers_from_trailing_comma_junk`. **Graduated
  `tune_004340`** (whitelist +1, 0 regressions, comparator `facts_misses:1`).

### Bug 16 — slur start is not anchored on a rest — FIXED 2026-06-14

`tune_008749` (`(6:4:6(z/G/A/B/c/d/)`): the slur-open `(` sits immediately before the
rest `z/`, so the slur must start **on the rest** (abc2xml emits `<slur type="start"/>`
on the rest, `start=""`). croma was relocating the slur start onto the **next pitched
note G**, skipping the rest (`start="G4"`). Same in measure 5.

- **Spec:** §4.11/§4.20 (KB raw line 1050/1330) — `()` slurs enclose the note sequence
  beginning where `(` appears; the spec does not forbid a slur starting on a rest, and
  §4.5 allows rests to be modified like notes.
- **Fix (2026-06-14):** `attach_pending_slur_starts` (`lower/voice.rs`) now anchors a
  pending start on the first timed Note **or Rest** of the group (skipping spacers); the
  existing `attach_slur`/`write_note` paths already emit `<notations>` on rest `<note>`
  elements. Test `slur_start_anchors_on_a_following_rest`. **Graduated `tune_008749`**
  (whitelist +1, 0 regressions).

### Bug 17 — second `||` dropped on an empty measure between consecutive barlines (HIGH)

`tune_003874` (lines 17–19 `| DD EF ||` / `"B"` / `||"Gm"G4`): two consecutive `||`
double-bars bracket an orphan chord `"B"`. croma emits a fully **empty** measure 12 with
**no right barline at all** (the second `||` is dropped) and pushes `"B"` into m13;
abc2xml keeps `"B"` + a `light-light` right barline on m12. Minimal repro: consecutive
`||` around an orphan chord → croma emits an empty measure with no right barline. Note
count 129=129 (no music dropped). **Same family as the barline cluster / Bug 4** (croma
gates barline emission on a real note event and mishandles the empty measure between a
run of barlines) — high regression risk, belongs in the focused barline session, not a
drive-by.

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

### Bug 18 — bare-number tempo with trailing decimal/suffix dropped to words (`Q:400.`, `Q:320s`) — FIXED 2026-06-14

The deprecated bare-number tempo form (ABC 2.1 §10.1, `Q:120` = "play 120 unit
note-lengths per minute") was rejected by croma whenever the integer carried a trailing
decimal point (`Q:400.`) or a legacy abc2mtex suffix letter (`Q:320s`): `parse_tempo_beat`
called `parse_u32` which requires the whole token to parse as `u32`, so any trailing
character made the field fall through to literal `<words>400.</words>` / `<words>320s</words>`
with no metronome. abc2xml leniently reads the leading integer.

- **Spec:** ABC 2.1 §10.1 (KB raw line 1909–1915) — `Q:120` "programs should accept it" as
  a tempo; the unit note length `L:` is the beat unit.
- **Fix:** new `parse_bare_tempo_bpm` (`crates/croma-core/src/lower/tempo.rs`) reads the
  leading digit run as the bpm, tolerating a decimal tail (`.`/`.123`) or purely-alphabetic
  legacy suffix, while rejecting internal whitespace / no-leading-digit fields (free text such
  as `Q:Fast`, `Q:3 dancers` stay verbatim words). Only the **bare-number** branch is
  loosened; the `beat=bpm` branch stays strict. Tests `tempo_bare_number_with_trailing_dot_emits_metronome`,
  `tempo_bare_number_with_legacy_suffix_emits_metronome`, guard
  `tempo_bare_leading_digits_with_space_stays_words` (`crates/croma-core/src/musicxml/mod_tests.rs`).
- **Graduates:** `tune_009608` (`Q:400.` → metronome eighth=400, sound tempo 200.00),
  `tune_001192` (`Q:320s` → metronome eighth=320, sound tempo 160.00).

### Bug 19 — text-only `y`-spacer bar emits spurious empty measure + mis-anchors annotation (`tune_003230`) — DEFERRED (barline/note-less-measure cluster)

`tune_003230` final line `... f4 "Da Capo"c4 |] "Final measure at end."y16 | f8 |]`: the bar
`"Final measure at end."y16 |` is note-less (the `y` spacer creates no rest, §6.1.2) and should
be its own measure carrying the annotation, with `f8` as the next measure (abc2xml: text in m15,
`f8` in m16). croma instead emits an **empty measure 15** and pushes the annotation into the
`f8` measure (m16), offsetting the direction by one measure. Confirmed croma bug (high), 1
comparator row, 50=50 notes (no music dropped). **Same family as Bugs 7/13/17** (croma gates
measure/barline handling on a real note event and mishandles a note-less measure) → high
regression risk, belongs in the focused barline/empty-measure session, **not a drive-by**.

### Bug 20 — leading-space annotation `" >"` placement-glyph ambiguity (`tune_007910`) — UNDETERMINED (flagged for human)

`tune_007910` (`" >"G2 ...`, bytes `0x22 0x20 0x3e 0x22` = quote, space, `>`, quote): §4.19 is
ambiguous whether a placement specifier (`>`) preceded by whitespace is consumed as placement
(abc2xml → empty `<words/>`) or kept as literal text (croma → `<words>></words>`). Both render at
identical note onsets; 95=95 notes. Strict reading (specifier must lead the string) favors croma;
lenient reading favors abc2xml. Genuine spec edge case → kept, flagged for a human policy call
(same class as Bugs 4/5/6/9/12), not droppable and not a clean croma fault.

### Bug 21 — bare-accidental mid-tune key modification dropped (`K:^F` after `K:Gmin`) (`tune_005044`) — DEFERRED (key-composition + spec-ambiguity)

`tune_005044` has header `K: Gmin` then a body `K:^F`. croma's `key_is_invalid_for_lowering`
(`crates/croma-core/src/lower/mod.rs:1186`) flags a tonic-less, clef-less, default-Major K: as
invalid and drops it (warning `abc.field.invalid_k`), so the 9 bare F/f notes in the second part
stay F-natural (Gmin) instead of F#. abc2xml applies it as Gmin **+ F#** (key steps Eb, Bb, F#).
50=50 notes, no music dropped — purely the lost key modification.

**Why deferred, not a drive-by:** (a) §3.1.14's modification format is `K:<tonic> <mode>
<accidentals>` — every spec example carries a tonic (`K:D Phr ^f`, `K:D =c`); a *bare* tonic-less
`K:^F` is **not** in the documented format, so abc2xml's "modify the current key" is a reasonable
but not spec-mandated interpretation (genuine ambiguity). (b) Even granting that reading, the
correct output is the **prevailing Gmin signature merged with F#** (Eb, Bb, F#), which needs
key-composition logic croma's key-change lowering lacks; naively un-gating would emit an F#-only
signature (fifths=0), **losing** Gmin's Bb/Eb — a different wrong answer. Real fidelity gap, kept
in the worklist, but needs a spec call + key-merge work, like Bug 11.

### Bug 22 — backslash-joined `||` before a `|N` ending drops the double bar (`tune_006302`) — FIXED 2026-06-14 (Cluster D)

**Fixed:** `merge_continued_barline_run` (`crates/croma-core/src/parse/music.rs`) dropped the
`!next_starts_variant_ending` guard that blocked Bug 18's `\`-seam coalesce whenever the next
line opened a `|N` ending. Two investigators (tune_006302 + tune_002255) confirmed from §6.1.1 +
§4.8 that a two-pipe `||1` join is a `||` (light-light) double bar followed by the ending — the
ending start is a separate `VariantEnding` item that survives the `|`-merge intact. To avoid
regressing `tune_013361` — where the `\` continuation bridges an intervening `M:1/4` information
field (`...| \` / `M:1/4` / `|1 ...`), so the bars are NOT adjacent — the merge now also requires
the seam to join **directly consecutive** lines (`edge.to_line == edge.from_line + 1`). Tests
`double_bar_across_backslash_continuation_before_variant_ending_is_coalesced` and
`backslash_seam_across_intervening_field_does_not_coalesce` (`musicxml/mod_tests.rs`).
**Graduated `tune_006302`** (whitelist 9,387 → 9,388, 0 regressions; tune_013361 stays matched).
`tune_002255` was a one-pipe seam (prev line ends ` \`, no trailing `|`) — already correct, no
change. Original report below.



`tune_006302` line 12 ends `... edB/c/ |\` and line 13 begins `|1 "Am"A3- ...`. Per §6.1.1 (KB
raw line 1502) the trailing `\` "effectively joins two lines together for processing," so the
stream is `edB/c/ ||1` — the two `|` become **adjacent `||`** (thin-thin double, §4.8 line 985)
immediately before the `|1` first-ending. abc2xml emits measure-32 right `light-light` + measure-33
ending-1-start; croma drops the double bar, emitting no right barline on m32 and only the
ending-start on m33. croma DOES emit `light-light` for a plain `||` at m36, so the engine works —
it mishandles the `\`-joined `||N` adjacency specifically. 127=127=127 notes, 45 measures aligned
1:1 (no music dropped). **Same barline-emission cluster as Bugs 7/8/13/14/19** (croma drops a
barline at a line-continuation/note-less boundary) → deferred to the focused TDD+regression session,
not a drive-by.

> Also kept this pass: `tune_008105` — trailing bare `:` (truncated `:|` end-repeat) renders as
> croma's deliberate `abc.music.barline.liberal` normalization (no glyph) vs abc2xml's spec-unsupported
> `dotted`; neither matches the spec-correct `:|`, so `undetermined`/low, kept. Same **bare-`:`
> liberal-barline family as Bug 9** (human policy call).

### Bug 23 — thick-thin `[|` did not close an open Nth ending bracket — FIXED 2026-06-14

ABC 2.1 §4.10 (KB raw line 1034): "The Nth ending starts with `[N` and ends with one of `||`, `:|`
`|]` or `[|`." croma's `stops_repeat_ending_barline` (`crates/croma-core/src/musicxml/score.rs:439`)
matched `Double | Final | RepeatEnd | RepeatBoth` but **omitted `BarlineKind::Initial`** — the kind
`[|` parses to (`parse/barline.rs:237`) — so a 2nd ending closed by `[|` was left a dangling
`<ending type="start">` with no stop, or its stop leaked to the next closing bar (carrying the
volta span too far). The adjacent code comment (score.rs:434) already listed `[|` among the closing
bars, so the implementation contradicted its own doc.

- **Fix:** add `BarlineKind::Initial` to `stops_repeat_ending_barline`. It is consulted only under the
  `open.is_some()` guard (score.rs:424), so it closes a `[|` **only** when an ending is in flight and
  never affects a section-opening `[|`. Test `volta_ending_closed_by_thick_thin_bar_emits_stop`
  (`crates/croma-core/src/musicxml/mod_tests.rs`); existing `volta_unclosed_in_source_stays_open_at_part_end`
  still passes.
- **Outcome:** resolved the ending-span rows for `tune_004922` / `tune_002878`; both then dropped as
  `abc2xml-barline-style` (their only residual divergence is abc2xml's separate `[|`-reversal, croma's
  heavy-light being spec-correct). No whitelist regression (matches held at 9378).

### Bug 24 — `]||:` run mis-tokenized: thick-bar `]` dropped / `|:` misplaced (`tune_014316`, `tune_004475`) — FIXED 2026-06-14 (tune_014316); tune_004475 DEFERRED to Bug 26

**Fixed (Cluster A):** the fused-run split (see Bug 25) graduates **`tune_014316`** — the `]`
now closes ending 2 with `light-heavy` (+ ending stop) and the `||:` opens the next repeated
section on the following measure's left (`heavy-light` + forward). `tune_004475` is **not** a
fused-run case (its `]|` and `|:` sit on two plain lines with no `\` continuation, so they are
never merged into one run); its `|:` lands one measure late at a **line seam**, which is the
**Bug 26** root (`is_leading_barline` / seam placement), not `barline_kind` tokenization. Moved
to Bug 26's file list. Original report below.

When a 2nd-ending close abuts a repeat-start across a `\`-continuation or directly — logical run
`]||:` — croma's `barline_kind` (`parse/barline.rs:245`, `raw.contains(']') && has_repeat_start(raw)`)
classifies the **whole** `]||:` as one `RepeatStart`, firing before the colon-less-`]`→`Final` branch
(line 268). Consequences: `tune_014316` — the `]` thick-bar that should close ending 2 (light-heavy)
is absorbed, leaving m10 right = `regular`; `tune_004475` — the `|:` forward-repeat lands one measure
late (m11 left instead of m10 left). 53=53 / 66=66 notes, no music dropped. Real croma bug, but the
fix means re-splitting a fused barline run (touching `barline_kind` tokenization + repeat/ending
attachment) → **barline cluster**, deferred to the focused TDD+regression session with Bugs 7/8/13/14/19/22.

## Open — barline triage round 2 (2026-06-14): more deferred-cluster croma bugs (NOT fixed)

Second per-file investigator sweep of the single-category `barline` worklist (all verdicts
high confidence; ground-truth note counts matched on every file → no music dropped). Every
croma fault below falls into the **already-deferred barline cluster** (fused-run tokenization /
repeat placement / `| |` whitespace / note-less-measure emission gate), so all are **kept,
recorded, not drive-by-fixed** per the cluster's TDD+regression requirement. abc2xml-at-fault
files were dropped (`abc2xml-barline-style`).

### Bug 25 — fused `|]:` run drops the `|]` thin-thick closer (7 files) — FIXED 2026-06-14 (Cluster A)

**Fixed:** `barline_lowering_kinds` (`crates/croma-core/src/lower/mod.rs`) now splits a
`RepeatStart`-classified run whose raw carries a thick `]` (`|]:`, `]||:`, `||]:`) into
`[Final, RepeatStart]` — mirroring the existing `||:`→`[Double, RepeatStart]` and
`[|:`→`[Initial, RepeatStart]` split pairs. The two same-span events flow through the proven
split-pair machinery: the `Final` closer lands on the current measure's right (`light-heavy`) and
the `RepeatStart` leads the next measure's left (`heavy-light` + forward repeat). One arm clears
the whole fused-run cluster (Bug 25 + Bug 24's `tune_014316`) — every tokenization site
(`parse_barline` scan, `parse_colon` glued-merge, `merge_continued_barline_run`) already routes
these runs through `barline_kind`→`RepeatStart`, so no parser change was needed. Tests
`fused_final_then_repeat_start_emits_closer_and_left_repeat`,
`fused_thick_bar_repeat_start_closes_ending_and_starts_repeat`
(`crates/croma-core/src/musicxml/mod_tests.rs`), roundtrip guard
`split_token_barline_pairs_rejoin` extended for `|]:`. **Graduated all 7:** `tune_009754`,
`tune_009394`, `tune_009381`, `tune_012513`, `tune_012511`, `tune_009392`, `tune_009382`
(whitelist 9,379 → 9,387 = +8 with `tune_014316`, **0 regressions**). Original report below.

`|]:` (a `|]` thin-thick double bar fused with a following `:` repeat-start — the classic
tune-A→tune-B transition `... |]:[K:...]`) is mis-tokenized exactly like Bug 24's `]||:`:
`barline_kind` (`parse/barline.rs:245`, `raw.contains(']') && has_repeat_start(raw)`) classifies
the whole run as one `RepeatStart`, emitting only the forward repeat on the next measure's left and
**dropping the `|]` light-heavy right closer** entirely (croma right=null). Isolation probes confirm
`|]` alone → `light-heavy` correctly, only `|]:` drops it; no warning. abc2xml is spec-correct
(`|]`→light-heavy, no reversal).

- **Spec:** §4.8 KB raw line 984 (`|]` thin-thick double bar) + liberal recognition line 1001.
- **Files (kept):** `tune_009754`, `tune_009394`, `tune_009381`, `tune_012513`, `tune_012511`,
  `tune_009392`. High graduation value — one `barline_kind` re-split clears all six.
- **Fix:** same as Bug 24 — split the fused run so `|]` closes the current measure before the
  repeat-start opens the next. Belongs in the focused barline session.

### Bug 26 — `|:` forward-repeat placed one measure late after a line/inline-field seam — PARTIALLY FIXED 2026-06-14 (`]|` seam, Cluster C)

**Fixed (thick-thin `]|` seam):** a measure closing `]|` (thick then thin) was tokenized as `]`
(final) **plus a stray `|`** (regular) — the scan in `parse_barline` broke after `]` unless `]||`
followed. That stray bar consumed the next measure's leading slot, so a following `|:` became
**non-leading** and its forward repeat was deferred a measure (off the end → dropped). The scan now
keeps `]|`/`]||`/`]|:` as one boundary (breaks after `]` only when a `|` does **not** follow). Test
`thick_thin_seam_barline_keeps_following_repeat_leading`. **Graduated `tune_007014`, `tune_009389`,
`tune_004928`, and `tune_004475`** (the file deferred from Bug 24) — whitelist 9,388 → 9,392, barline
worklist rows 59 → 24, **0 regressions**.

**Fixed (leading `:|` repeat-END at a line seam) 2026-06-14:** a `RepeatEnd`/`RepeatBoth` that LEADS a
note-less seam measure (an empty measure, or one holding only a `K:`/`M:`/clef/tempo change) now
retro-closes the previous real measure with its backward repeat, instead of being dropped by the
`unique_barlines` leading-barline filter. The gate `repeat_end_closes_previous_measure`
(`crates/croma-core/src/lower/timeline.rs`) was generalized two ways: (a) its prior-measure check
`previous_measure_ends_with_invisible_barline` → `previous_measure_has_timed_content` (the seam may
follow any closing bar `|`/`[|]`/…, not just an invisible one); (b) its current-measure check
`events.is_empty()` → `is_empty_measure` (a note-less `K:` change measure is still an empty seam).
Tests `leading_repeat_end_after_plain_bar_seam_closes_previous_measure`,
`leading_repeat_end_after_key_change_seam_closes_previous_measure`
(`crates/croma-core/src/musicxml/mod_tests.rs`); the existing `[|]:|` invisible-seam test still passes.
**`tune_000205`** (`:|2` after `...^f|`) and **`tune_011411`** (bare `:|` after a note-less `K:D` line)
are now spec-correct — m17/m34 close with `light-heavy` + backward + the ending stop, and the seam
measure is absorbed; the 2nd ending no longer leaks (011411 m34→m43). Both adversarial-verified
(152/222 notes byte-identical, no music lost). abc2xml is **also** wrong on both (forward repeat on a
phantom empty seam measure), so each **dropped as `abc2xml-phantom-measure`** (0 whitelist regressions;
worklist 10→8).

**Still open** (`tune_010091`): a *different* root — the seam `...ecA A2: |` is colon-SPACE-pipe, so
under the strict-spec policy (§4.9 KB line 1009: bar/repeat glyphs must be contiguous) croma is correct
that m9's right is a plain bar (no backward repeat) and abc2xml over-reads `: |` as a `:|`. croma's real
bug is narrower: the loose `:` normalizes into a measure boundary whose stray slot mis-places the
following `|:` forward repeat onto m11 instead of the `a`-pickup m10. Separate fix; abc2xml is at fault on
the residual `:` so the file drops regardless. Original report below.

When `|:` opens a body line that follows a line ending in a closing/thick bar (`]|`) or a standalone
inline-field line (`[K:Em]`, `K:Ddor`), croma leaves the first real measure barline-less and defers
the `heavy-light` forward repeat to the **next** measure (one too late, no compensating barline) —
the same symptom as Bug 24's `tune_004475`.

- **Spec:** §4.8 KB raw line 987 (`|:` start of repeated section — opens the measure it precedes).
- **Files (kept):** `tune_007014` (`]||:` split across a plain line break), `tune_009389` (`|:`
  after a `[K:Em]` line), `tune_004928` (`|:` after `...A4]|` + `K:Ddor` line), `tune_004475`
  (`]|` line 2 / `|:` line 3 across a plain break — `|:` lands m11 instead of m10; moved here from
  Bug 24 after the Cluster A fused-run fix confirmed it is a seam, not a tokenization, fault).
  50–173 notes, all 1:1 measure-aligned, no music dropped.

### Bug 27 — `| |` same-line whitespace double bar dropped — RESOLVED 2026-06-14 as DROP (strict-spec policy)

**Resolved (human policy call, Q1):** the parser/exporter is **strict** to ABC 2.1; §4.8 line 1001's
liberal recognition applies to a *contiguous sequence* of `|`/`[`/`]`/`:` (examples `|[|`, `[|:::`),
so a whitespace-separated `| |` is **not** a `||` double bar. croma's plain measure boundary is
spec-correct; abc2xml's light-light is the liberal over-reach. All seven files
(`tune_014544`, `tune_014545`, `tune_014559`, `tune_002876`, `tune_002874` + the undetermined
`tune_013523`, `tune_006454`) have **matching measure and note counts** — the sole divergence is the
interior `| |` bar style — and the multi-voice `tune_013523` has a plain `|` in its parallel voice at
the same boundary (light-light would be cross-voice-inconsistent). **Dropped as
`abc2xml-barline-style`** (no export change). Loose `| |` source is a `croma fmt --auto-fix`
sanitization concern, not a parser bend. Original report below.

Bug 18 fixed the `\`-continuation seam (`...|\` + `|` → `light-light`) but the merge in
`parse/music.rs:180-219` is **gated to the backslash seam**, so a same-line `| |` (bar–space–bar)
still falls through and croma emits no right barline. abc2xml renders `light-light` (matching croma's
own Bug-18 convention that `| |` ≡ `||`).

- **Spec:** §4.8 KB raw line 985 (`||` thin-thin double) + §4.9 (spaces around bar tokens tolerated).
- **Files (kept):** `tune_014544`, `tune_014545`. Same whitespace-significance root as Bug 9; extend
  the Bug-18 merge to the same-line case (regression-test the `| [1` / empty-measure cases).

### Bug 28 — `[|]` invisible bar dropped on a note-less EMPTY measure (multi-voice) — FIXED 2026-06-14 (Q2, Cluster C)

**Fixed:** `closes_empty_measure_barline` (`crates/croma-core/src/lower/timeline.rs`) now returns true
for `Invisible` as well as `Final`. On a note-less measure, only `Final` previously kept the measure's
existing span start; every other closing bar fell through to `extend_span`, which collapsed the empty
span onto the bar line itself, making it *leading* — so `unique_barlines` filtered it from both edges
and dropped it. With `Invisible` added, a `[|]`-only measure keeps its span start, stays non-leading,
and exports `<bar-style>none</bar-style>` like the already-working `|]` empty-final case. `Double` is
deliberately **excluded** (a section-leading `||` on an empty measure must stay absorbed — guard
`continued_section_leading_double_barline_does_not_close_empty_measure` — so Bug 17's orphan-`||`
needs separate handling). Test `multi_voice_empty_measure_keeps_invisible_barline`. The five files
(`tune_006755`, `tune_006759`, `tune_006758`, `tune_006750`, `tune_006753`) now emit `none`
(spec-correct, Q2 decision); abc2xml renders some empty-`[|]` as `light-heavy` (wrong) so they do not
graduate — **dropped as `abc2xml-barline-style`** (0 regressions). Original report below.

Bug 7 was resolved for *annotation-only* measures, but a note-less **empty** measure in a multi-voice
tune (`[V:1]  [|]`) still drops the `[|]` invisible bar: croma emits an empty `<measure>` with no
`<barline>` despite logging `barline_policy: exported as none` — the emission gate still requires a
real note event. (abc2xml is also wrong here, rendering `[|]` as `light-heavy`/final, but that does
not clear croma.)

- **Spec:** §4.8 KB raw line 999 (`[|]` invisible bar line → MusicXML `bar-style none`).
- **File (kept):** `tune_006759` (429=429 notes, 29=29 measures). Same emission-gate root as Bug 7/13.

### Bug 29 — bare `:` / `:[2` repeat-end mis-tokenized (direction inversion or drop) — FIXED 2026-06-14 (tune_013149); tune_003603 is the free-floating-`:` policy case

**Fixed (`:[2`):** `parse_colon` now routes a bare `:` directly before a variant ending (`:[<digit>`)
to a `RepeatEnd` boundary (`parse_bare_colon_repeat_end`), mirroring §4.9's `:|2` shorthand — it
closes the open ending and repeats backward, then `[N` parses as the next ending. The branch is gated
on a digit after `[`, so a section transition `|]:[K:..]` keeps its Cluster-A glued-merge. Test
`bare_colon_before_variant_ending_is_a_backward_repeat`. **`tune_013149`** now emits light-heavy +
backward repeat (spec-correct) where it previously produced a structurally impossible all-forward /
no-backward tune; abc2xml renders the bare `:` as `dotted` (spec-wrong), so the file does **not**
graduate — **dropped as `abc2xml-barline-style`** (0 regressions). `tune_003603` is **not** this
construct: its `:` are free-floating mid-line dots (` :D`, `D:d`), which under the strict-spec policy
(§4.8 line 1001 liberal recognition is contiguous-only) stay malformed/skipped — a Q3 free-floating-`:`
case, not a `:[N` repeat-end. Original report below.

croma classifies a bare `:` repeat-end (no leading `|`) as a forward `RepeatStart`, or drops it, instead
of a backward repeat-end:

- `tune_013149` (`:[2` first-ending terminator): the `:` before `[2` is tokenized `kind: RepeatStart`
  (tree dump), emitting heavy-light+forward where §4.9 line 1021 / §4.8 line 988 require a **backward**
  repeat — producing a structurally impossible 4-forward / 0-backward tune. **Spec-unambiguous and clean,
  but** (a) same `parse_colon`/`barline_kind` subsystem the cluster defers to the focused TDD+regression
  session, and (b) abc2xml is also wrong here (renders `:` as `dotted`), so a croma fix does **not**
  graduate the file (it would flip to abc2xml-at-fault and drop) — zero match-rate gain against real
  whitelist-regression risk, hence deferred not drive-by-fixed.
- `tune_003603` (two mid-line bare `:` dots): croma promotes the first to a forward repeat and drops the
  second's style, handling identical tokens inconsistently; abc2xml renders both as `dotted` (spec-correct
  per §4.8 line 997/1001).

Same `parse_colon`/`barline_kind` subsystem as Bug 14 (`:]`, fixed) and Bugs 24/25 (fused runs) → focused session.

### Wave-3 additions to the round-2 bugs

- **Bug 25** (`|]:` fused-run) now **7 files**: + `tune_009382`.
- **Bug 26** (`|:`/`:|` misplaced/dropped at a line/inline-field seam) + `tune_010091`, `tune_011411`
  (the latter: a bare `:|` repeat-end dropped on a note-less `K:D` key-change measure, with the 2nd ending
  left unterminated — runs m34→m43).
- **Bug 27** (`| |` same-line whitespace double bar) + `tune_014559`.

### Undetermined `| |` (spec genuinely silent) — RESOLVED 2026-06-14 as DROP (Q1, strict-spec)

**Resolved with Bug 27** (see above): the human policy call (Q1) was **strict spec** — §4.8 line 1001's
liberal recognition is contiguous-only, so a space-separated `| |` is not a `||` and croma's plain bar is
correct. `tune_013523`, `tune_006454` **dropped as `abc2xml-barline-style`** alongside the Bug-27 files.
The multi-voice `tune_013523` confirmed it: its parallel voice has a plain `|` at the same boundary, so
light-light would be cross-voice-inconsistent. Original note below.

`tune_013523`, `tune_006454`: recurring space-separated `| |` where abc2xml emits `light-light` and croma a
plain bar. §4.8/§4.9 define **no** style for space-separated `| |` (only contiguous sequences), so neither
side provably violates the spec; investigators returned `undetermined`/low (note counts and alignment
intact: 210=210, 430=430). Same family as Bug 27 but not clearable either way — an explicit human policy
call (should croma treat `| |` ≡ `||`?). Kept, not dropped.

### Wave-4 additions to the round-2 bugs

- **Bug 27** (`| |` same-line whitespace double bar) + `tune_002876` (high), `tune_002874` (medium — the
  `| |`≡`||` merge rests on §4.8's "be liberal in recognizing bar lines" clause + idiom, not an explicit rule).
- **Bug 26 / Bug 7 root** + `tune_000205`: a `:|2` repeat-end split across a line break lands on an empty seam
  measure; croma's `is_leading_barline` flags it (`source_span.start == barline.span.start`) and `unique_barlines`
  filters it from BOTH edges → the backward repeat is dropped. This is the **Bug 7 `unique_barlines`
  leading-barline-filter** root surfacing at a line seam. (abc2xml also wrong — emits forward instead of backward —
  so the file won't graduate; it flips to abc2xml on a croma fix.)

### Bug 28 expanded — `[|]` on note-less measures: investigators split, KEEP on doubt

The `[|]` (invisible barline, §4.8 KB line 999 → `bar-style none`) on a **note-less / empty** measure produced
**contradictory investigator verdicts** across files of the identical construct:

- croma-at-fault / high: `tune_006755`, `tune_006759` (croma omits the required `<bar-style>none</bar-style>`).
- abc2xml-at-fault: `tune_006758` / `tune_006750` (high, "phantom final"), `tune_006753` (medium) — these cleared
  croma on the premise that its omission "renders identically to invisible."

**Reconciliation (orchestrator):** in MusicXML an **omitted** `<barline>` is NOT equivalent to
`<bar-style>none</bar-style>` — the boundary renders as a default thin bar — so croma's omission is a real defect
(the Bug 7/28 emission gate: croma gates barline emission on a real note event and drops the `none` style on empty
measures). abc2xml is **also** wrong (synthesizes `light-heavy`/final). Both diverge from the spec-correct `none`, so
**all five are kept** (none dropped), per keep-bias and the asymmetric cost of hiding a croma bug. A spec/human call
is needed on whether croma's empty-measure omission must emit an explicit `none`. Files: `tune_006755`,
`tune_006759`, `tune_006758`, `tune_006750`, `tune_006753`.

### Round-2 barline-category triage — final tally (2026-06-14)

39 un-adjudicated single-category `barline` files investigated (one subagent each, all four instruments treated as
fallible). **12 dropped** as `abc2xml-barline-style` (croma spec-correct: `[|` thick-thin reversal ×5; abc2xml
double-bar synthesis/demotion near repeats ×4; `[|]` phantom-final ×1 [`tune_006750` — note its sibling empty-`[|]`
files were kept]; bare-`:` ending truncation ×1; `|]:`-vs-style ×1). **27 kept** as croma fix candidates / undetermined,
all in the deferred barline-tokenization/emission cluster: Bug 25 `|]:` (7), Bug 26 seam `|:`/`:|` (6), Bug 27 `| |`
(5), Bug 28 `[|]` note-less (5), Bug 29 bare-`:`/`:[2` (2), undetermined `| |` (2). No drive-by fixes (whole subsystem
deferred to a focused TDD+regression session).
