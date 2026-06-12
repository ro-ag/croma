# 12 — Phase-33 forensic triage ledger (2026-06)

Machine-assisted triage of all 15 mismatch categories over the normalized
comparison (sounding-alter fact, see doc 05): one investigator per category,
every claimed cause independently re-verified by an adversarial agent that had
to reproduce the behavior with both tools before ruling. Authority order: the
ABC 2.1 spec decides correctness; abc2xml is the comparison baseline, never an
authority to imitate. Verdicts below are the FINAL (verifier-corrected) ones;
several escalate or refute the blanket per-file classes in docs 01-11 — where
they conflict, this ledger wins.

Comparison state when filed: 8,118 matching files (post alter-normalization).
After the phase-33a fix batch (commit 6635deb): 8,734 matching, +616 files, 0
regressed; round-trip 99.25% / 0 structural diffs unchanged.

Status legend: FIXED(33a) = fixed in the phase-33a batch; OPEN = confirmed
croma bug awaiting fix; QUIRK = abc2xml artifact, croma keeps its behavior;
EQUIV = legitimate equivalent representations (documented; comparator
normalization considered case by case).

2026-06-12 HEAD recheck before phase-35 burn-down: `cargo test -p
croma-core --lib` passed 325/325. The following ledger entries were already
fixed by landed commits after this ledger was filed and have regression tests
in `crates/croma-core/src/musicxml/mod_tests.rs` or
`crates/croma-core/src/lower/mod_tests.rs`: liberal barline run glyph/repeat
and lone-colon boundaries (c8b19d3), body unit-length voice scoping (8eb91bb),
body Q: tempo events (fc4f59c), lowercase-root quoted text classification
(03922fa), chord-member slurs (858b53e), and long/maxima plus tuplet written
note types (8c76fc8). Their headings below are updated to fixed; the remaining
OPEN rows are still the phase-35 work queue unless separately re-verdicted.

2026-06-12 phase-35 multirest fix: decision was to expand ABC `Zn`/`Xn` in
lowering when the current voice has a known meter, producing `n` real
full-measure rest measures in the Score model. MusicXML writes
`<rest measure="yes"/>` for visible full-measure rests and places
`<measure-style><multiple-rest>n</multiple-rest></measure-style>` on the first
visible expanded `Zn` measure as display metadata; free-meter `Zn` keeps the
existing warning and single-duration fallback. Regression tests:
`lowers_multi_measure_rests_in_known_and_free_meter` and
`multimeasure_rest_exports_real_full_measure_rests`. Full 10k compare against
the phase-33 b8 baseline: matches 8858->8861, mismatch rows 167943->164267,
zero match-to-mismatch regressions; tune_005687/tune_013455/tune_013477 fully
fixed, tune_009310/tune_010751/tune_010753 improved. Round-trip remains 99.25%
/ 0 structural diffs.

2026-06-12 phase-36 one-note tuplet fix: `attach_completed_tuplet()` now
attaches both Start and Stop to the same timed event when ABC uses an explicit
`r=1` tuplet such as `(3:2:1G`. Regression tests:
`one_note_tuplet_carries_start_and_stop_on_same_event` and
`one_note_tuplet_emits_balanced_start_and_stop`. Targeted repro export for
tune_010459/tune_010462 now emits adjacent start+stop pairs on the three
one-note tuplets in each file; the remaining 6 targeted tuplet comparison rows
are croma `startStop` vs abc2xml/music21 `null` because abc2xml writes stop-only
notation for those one-note tuplets. Full 10k report-only compare on the
phase-36 export: matches 8861, mismatch rows 164265, tuplet rows 607, zero
MusicXML import failures.


## accidental

### `accidental-grace-ledger-isolation` — **OPEN** (croma_bug, repro=None)

*Share:* ~0.4% rows (17/3876), 9 files; 15% of genuine solo rows — *files:* tune_013736.abc, tune_011097.abc, tune_012949.abc, tune_009036.abc


Minimal repros, K:C. `{^f}f2 f2 f4`: croma gives the main f's NO alter (natural); abc2xml gives all three alter 1 (grace ^f propagates through the bar). `^c2 {dc}d2 c4`: croma's grace c has no alter; abc2xml's grace c has alter 1 (bar's ^c applies to the grace). Corpus: tune_013736 m5 ({^f} then f in `efge`), tune_009036 m21 ({^A}B4- B2{A}B2- — second grace A), tune_011097 m7 (^c then grace {dc}), tune_012949 m11/m13 (bar's _d/_B-vs-=B not applied to graces {d}/{Bcd}), plus tune_007903/008772/008773/009334/011162. ABC 2.1 §11.3: accidentals apply 'to all the notes of the same pitch in all octaves up to the end of the bar' with no grace exemption (§4.12 is silent), and universal engraving convention (and abc2xml) propagates grace accidentals through the bar — croma silently changes sounding pitch. Sharpens doc 05's 'spec-unspecified edge' to a fixable defect. Code: crates/croma-core/src/l…


*Fix:* Lowering layer: route grace-note alter resolution through the same MeasureAccidental ledger used for main notes — crates/croma-core/src/lower/voice.rs (grace_note_event_model / grace_event_model, ~lines 700-748, currently context-free pure functions) plus crates/croma-core/src/lower/accidental.rs (ledger read for inherited alters, ledger write for written grace accidentals). Check crates/croma-cor


### `accidental-late-voice-key-seed` — **OPEN** (croma_bug, repro=None)

*Share:* ~0.5% rows (21/3876), 2 files; 19% of genuine solo rows — *files:* tune_010949.abc, tune_012206.abc


Minimal repro (V:1 body, then standalone K:Dm, then V:2 body — the Village Music Project two-voice layout): croma gives P2 m1 BOTH <fifths>2</fifths> and <fifths>-1</fifths> and renders all of P2 in Dm from bar 1 (B alter -1); abc2xml gives P2 m1-2 in D major (header key) and Dm only from the voice's own K:Dm at m3 — musically correct, since V:2's music at time 0 sounds simultaneously with V:1's D-major bars; a body K: 'applies from that point' (§3, §7.3) and that point is inside V:1's stream. Croma changes sounding pitches for 16 bars of tune_010949 (B2 flat instead of natural in K:G bars, E3 flat/F3 natural instead of E natural/F# — 11 rows) and tune_012206 (B2/C3 wrong in K:D bars — 10 rows). Deliberate but wrong seeding in crates/croma-core/src/lower/mod.rs: `key_changed`/`self.key` are global, and the block at ~line 589 ('A voice defined after a standalone K:/M: change seeds from th…


*Fix:* Lowering layer: crates/croma-core/src/lower/mod.rs, MultiVoiceLowering (fields ~lines 142-155, seeding block ~lines 580-615). Scope standalone body K:/M: changes to the voice that is current when they appear; seed a voice whose body starts later from the HEADER key/meter (header_key_display/header_meter_display already exist as the dedupe baseline). Keep the changed-state seed only for voices that


### `accidental-tie-carry-pollutes-next-bar` — **OPEN** (croma_bug, repro=None)

*Share:* ~0.1% rows (4/3876), 2 files — *files:* tune_006821.abc, tune_005543.abc


Minimal repro `K:C\n^a4- | a2 b2 a4 |]`: croma gives A#, A#(tied), B, A# — the final non-tied a inherits the carried sharp; abc2xml gives A#, A#, B, A-natural. ABC 2.1 §11.3 scopes accidentals 'up to the end of the bar'; §4.20 makes the tie continue the same pitch for the stop note only ('two successive notes of the same pitch... played as a single note'); conventional notation likewise reverts subsequent notes to the key signature. Corpus: tune_006821 m17 (m16 `_a... a2-` ties into m17 `ag/^f/ g2-ga/b/ ag`; croma flats the two later a's, ref naturals — 2 rows), tune_005543 m40 (m39 `...Jc3-` with bar-propagated C# ties into m40 `c2d/c/A/c/...`; croma sharps the later c's — 2 rows). This is the surviving half of parser-backlog item 10: the fix unwound carries for DROPPED ties but 'matched ties confirm theirs' deliberately keeps the carry live for the whole bar — that confirmation semanti…


*Fix:* Lowering layer: crates/croma-core/src/lower/accidental.rs — preserve_for_open_ties (~lines 149-192) seeds the next bar's ledger with from_pending_tie entries; instead of confirm_pending_tie_carry (~line 256) leaving the entry live for the rest of the measure, the carried entry must be consumed when the tie's stop note resolves (or marked stop-note-only so other same-pitch notes resolve against the


### `accidental-cascade-from-structural-misalignment` — **QUIRK** (reference_quirk, repro=None)

*Share:* ~97% rows (3765/3876), 227/258 files — *files:* tune_008774.abc, tune_008060.abc, tune_006695.abc


3765/3876 rows sit in files whose refreshed-manifest verdict is PHANTOM_MEASURE (3445), CASCADE (271), ABC2XML_DROPS_MUSIC (23), MULTIREST_EXPANSION (22) or ABC2XML_DROPS_TACET (4) — all with croma_correct=yes. Spot-verified tune_008774.abc: the malformed header line `notC:Prefixed as 'No.9' in MS` (illegal field, malformed input) makes abc2xml fabricate music from prose — reference m1 is a single C and m2 is E,F,E,D,... while croma m1 is the tune's real opening B,D,F,G (`B>df>g`). Reference has 49 measures vs croma 48; per-measure positional alignment then pairs different source notes for the whole file, producing 94 'accidental' rows from m1 onward (plus pitch/octave/duration rows in the same files). Already documented in docs/comparison/abc2xml-divergences/02-phantom-measures.md and 07-cascade-artifacts.md; this verification confirms those verdicts hold for the accidental projection o…


### `accidental-halfsharp-extension-syntax` — **QUIRK** (reference_quirk, repro=None)

*Share:* ~1.2% rows (45/3876), but 41% of the 111 genuine accidental-only rows (12/31 solo files) — *files:* tune_001009.abc, tune_001829.abc, tune_003353.abc


Malformed input. ABC 2.1 §4.2 defines only ^ = _ ^^ __; `^/` is an abcMIDI microtone extension, illegal in 2.1 (duration shorthand may not precede the pitch letter). Minimal repro `K:G\nB2^/c2 c2 c2 |]`: abc2xml prints `-- error: unknown accidental ^1 in note: ^1_2c` then emits <alter>1.0</alter> with <accidental>natural</accidental> (incoherent: natural glyph, sharp alter) and propagates 1.0 to every later c in the bar; croma emits warning[abc.music.malformed_accidental] + warning[abc.music.malformed_length] and outputs plain naturals. Both are recovery choices on illegal input; croma's is diagnosed, not silent. Explains tune_001009 (11 rows: 4 `^/c` sites, sharps propagate to later same-pitch notes in ref), tune_001202/002209/002562/002731 (`_/e` → ref alter -1)/002956/003026/003353/003354/003390/003511/001829 (the `d^/cBA` row in evidence-pack sample). Related: tune_003732's 2 rows wi…


### `accidental-mode-abbreviation-aeo` — **QUIRK** (reference_quirk, repro=None)

*Share:* ~0.5% rows (19/3876), 1 file; 17% of genuine solo rows — *files:* tune_011837.abc


Minimal repro `K:Daeo\nF C B F |]`: croma <fifths>-1</fifths>, F/C natural, B alter -1 (D aeolian = D minor signature). abc2xml: <fifths>2</fifths> with alter 1 on F and C (D MAJOR). ABC 2.1 §4.6 (mode table section, ~line 630): 'The spaces can be left out, capitalisation is ignored for the modes and in fact only the first three letters of each mode are parsed' — Aeolian is in the mode table, so K:Daeo is D aeolian. abc2xml simply doesn't recognize 'aeo' and drops the mode. Explains all 19 rows of tune_011837.abc (mid-body K:Daeo section: ref sharps F4/C4/C5 alter 1 vs croma 0; ref B4 natural 0 vs croma -1). The stored manifest verdict ALTER_SERIALIZATION for this file is wrong (stale, pre-normalization).


### `accidental-tie-rewritten-glyph-ignored-by-ref` — **QUIRK** (reference_quirk, repro=None)

*Share:* ~0.1% rows (5/3876), 5 files — *files:* tune_011121.abc, tune_011122.abc, tune_014441.abc, tune_013526.abc


Minimal repro `K:D\n=c4- | =c2 B2 c4 |]` — the natural glyph is literally re-written in bar 2. croma: final c natural (the written =c applies 'up to the end of the bar', §11.3 — no exemption for glyphs on tie continuations; even engraving convention has any printed accidental rule the bar). abc2xml: final c alter 1 (C#) — it consumes the rewritten glyph as part of tie continuation and never enters it in the bar's ledger, discarding a written source accidental's effect. Corpus: tune_011121 m12 (`_B-|_BAGB`: croma B-flat at onset 1.5, ref natural), tune_011122/011119 m12 (`=c-|=cBAc`), tune_014441 m7 (chord `[B,2-^D2-F2-]|[B,^DF] [B,DF]`: croma D# on second chord, ref D natural), tune_013526 m38 (same chord pattern `[D,-^F,-]|[D,^F,] [D,,F,]`). Distinct from accidental-tie-carry-pollutes-next-bar: here the glyph IS in the new bar's source text, so croma's propagation is the spec's plain re…



## barline

### `barline-final-dropped-on-empty-voice-measure` — **FIXED(37b)** (croma_bug, repro=True)

*Share:* <1% rows (a handful of multi-voice files) — *files:* tune_010088.abc


Minimal: two-voice tune where V:2's last music line is just `|]` (its final measure empty) — croma emits P2's last measure with NO right barline; abc2xml emits <bar-style>light-heavy</bar-style>. Corpus: tune_010088 (Antra deserti) `[V:2]  |]` → ref P2 m19-right light-heavy, croma absent, while P1/P3/P4 (which have notes before `|]`) all get it. The source explicitly notates the final bar for that voice; croma silently drops it. Likely interaction between the barline-only-measure coalescing path (crates/croma-core/src/lower/timeline.rs is_barline_only_measure / may_coalesce_barline_only, lines 60-95 and 389-404) and final-barline emission for empty measures in multi-voice scores.


*Fix:* lower layer: crates/croma-core/src/lower/timeline.rs — when finishing a voice, preserve the Final/Double barline kind on the last (possibly empty) measure instead of losing it with the coalesced/empty measure; export already handles Final on empty measures (P2 m2 exists, just lacks the barline).

**Phase 37b verification:** `crates/croma-core/src/lower/timeline.rs` now preserves the source span of a non-first empty measure closed by `|]`, allowing MusicXML export to classify the explicit empty-voice final as a right `light-heavy` barline. The fix is deliberately limited to `BarlineKind::Final`: a regression test keeps the phase-36/reference shape for tune_006306-style section-leading `||` after a suppressed line break, where an empty measure is retained but must not gain a right double barline. Targeted comparison for `tune_010088.abc` and `tune_006306.abc`: 2 structural matches, 0 mismatches. Verification: `cargo fmt --all -- --check`, focused final/double-barline tests, `cargo test -p croma-core barline`, `cargo test --workspace`, `cargo run -p croma-cli -- xml examples/basic.abc`, full 10k report-only comparison, and ABC round-trip proof. Phase-36 -> phase-37b aggregate corpus delta (including 37a): structural matches 8861 -> 8868, mismatch rows 164265 -> 164215, `barline` 3545 -> 3540, `direction` 463 -> 459, `extra_in_croma` 47606 -> 47565, no harness/import failures. ABC round-trip remains total=10000, in_scope=9925, structural_diffs=0, errors=0.


### `barline-liberal-run-glyph-dropped` — **FIXED(33b)** (croma_bug, repro=True)

*Share:* ~5-8% rows (47 aligned files/90 rows ref-final, 32 of them warning-tagged; plus shifted analogues) — *files:* tune_012890.abc, tune_011970.abc, tune_004928.abc


Minimal: `CDEF|GABc||]` — croma warns `abc.music.barline.liberal: Liberal barline spelling ||] was normalized as a measure boundary` and emits NO barline element; abc2xml emits light-heavy (final). Corpus: tune_012890 ends `D3||]` → ref m18-right light-heavy, croma nothing; tune_004928 volta-2 ends `A4]|` → `]` becomes Liberal→nothing (and the Regular leftover also fails to close the volta, compounding cause 1). ABC 2.1 §4.8 (961-962) instructs liberal RECOGNITION of such runs as bar lines whose shape is the glyph sequence (`|]` thin-thick is line 945's final); croma recognizes the boundary but erases the shape entirely — source presentation silently lost in output (warning notwithstanding). Root: crates/croma-core/src/parse/barline.rs barline_kind() (lines 137-159) falls through to BarlineKind::Liberal for `||]`/`]`, and crates/croma-core/src/musicxml/barline.rs:13,35 exports Liberal ex…


*Fix:* parse layer: crates/croma-core/src/parse/barline.rs barline_kind() — classify colon-less liberal runs to their strongest component (any `]`/`[` → Final/Initial, >=2 bars → Double) instead of Liberal; or lower layer: crates/croma-core/src/lower/mod.rs barline_lowering_kinds (line 1199) split the raw run. Heed docs/parser-backlog.md lines 156-158: round-trip span-equality detection depends on barlin


### `barline-liberal-run-repeat-dropped` — **FIXED(33b)** (croma_bug, repro=True)

*Share:* ~2-3% rows (31 aligned files/52 rows ref_repeat_croma_none + a few shifted) — *files:* tune_006040.abc, tune_003884.abc, tune_012511.abc


Minimal 1: `|:CDEF:|` newline `|:|GABc:|` — croma warns liberal and emits an empty m2 with NO left barline (forward repeat LOST); abc2xml emits heavy-light + <repeat direction="forward"/> on the empty m2 (both create the empty measure, only the repeat differs — exactly tune_006040 m10). Minimal 2: trailing `:[|]|]` — croma's parse_colon (parse/barline.rs:63-78) only treats `::`/`:|` as barlines, so the `:` becomes `abc.music.invalid_barline` ("A repeat dot must be part of a barline spelling") and is dropped; croma then emits bar-style `none` from the `[|]` — the backward repeat AND final glyph are lost; abc2xml emits light-heavy + backward repeat (tune_003884 m16). tune_012511 `[B,D]4 |]:` — the trailing `:` (repeat-start for the next section) is likewise dropped. ABC 2.1 §4.8 (961-962): runs of `|`,`[`,`]`,`:` are bar lines and `:` are repeat dots; dropping them changes playback semanti…


*Fix:* parse layer: crates/croma-core/src/parse/barline.rs — (a) barline_kind(): a liberal run ending in `:` → RepeatStart, starting with `:` → RepeatEnd, both → RepeatBoth (the existing has_repeat_start/has_repeat_end helpers only fire when raw contains `]`); (b) parse_colon() (lines 63-78): accept `:[`-led runs (e.g. `:[|]`) as barlines instead of malformed; (c) the tokenizer break at `]` (lines 33-37)


### `barline-lone-colon-boundary-dropped` — **FIXED(33b)** (croma_bug, repro=True)

*Share:* ~2% of barline rows (71 files, ~77 positional diffs) plus larger cascades booked under measure_alignment — *files:* tune_008774.abc, tune_006702.abc, tune_007591.abc


Minimal: `CDEF:GABc|cdef|]` — croma warns `abc.music.invalid_barline` and produces 2 measures (the first 8-beats over-full, boundary gone); abc2xml produces 3 measures with bar-style `dotted` at m1-right. 71 corpus files (e.g. tune_008774, tune_006702), all in the shifted set because the lost boundary changes measure counts and cascades into measure_alignment/missing/extra rows. ABC 2.1 §4.8 (961-962) says parsers "should be quite liberal in recognizing bar lines ... a sequence of | , [ or ] , and :" — croma already accepts the colon-only sequence `::` as a barline, so refusing the one-colon sequence and losing the measure boundary is the less-liberal, structure-mangling recovery. abc2xml's specific 'dotted' glyph choice is its own invention (spec dotted bar is `.|`) and need not be copied — only the boundary matters. Borderline malformed-input case, hence medium confidence, but croma's …


*Fix:* parse layer: crates/croma-core/src/parse/barline.rs parse_colon() — treat a lone `:` adjacent to note groups as a liberal barline (measure boundary, no glyph/repeat) instead of MalformedSyntaxKind::InvalidBarline; keep the warning.


### `barline-volta-multimeasure-stop-missing` — **FIXED(33a)** (croma_bug, repro=True)

*Share:* ~35-40% rows (483/755 files contain it; 404 of the 527 measure-aligned files; ~956 aligned barline-fact diffs + most 'volta_other' diffs) — *files:* tune_009012.abc, tune_009013.abc, tune_013771.abc, tune_010641.abc


Minimal: `X:1 L:1/4 K:C |:CDEF|[1 GABc|GGGG:|[2 cdef|]` — croma emits `<ending number="1" type="start"/>` on m2-left but NOTHING on m3-right except `<repeat direction="backward"/>`; abc2xml emits `<ending number="1" type="stop"/>` + light-heavy there. Same-line single-measure volta (`[1 GABc:|`) closes correctly, proving the trigger is the volta spanning >1 measure, not line breaks. ABC 2.1 §4.9 makes the first ending run to the `:|`; MusicXML requires the bracket extent be closed — croma leaves a dangling start (lost extent). Also drops volta-2 stop when a section/part ends on a plain `|` (tune_009012 m10: ref right barline with ending-2 stop, croma nothing). Root: crates/croma-core/src/musicxml/score.rs write_part (~lines 88-110) computes `endings = unique_endings(&measure_refs)` from the CURRENT measure only and emits EndingType::Stop only when that same measure contains the ending st…


*Fix:* musicxml export: crates/croma-core/src/musicxml/score.rs write_part — carry an open-ending state across the measure loop and emit Stop at the next RepeatEnd/Double/Final barline, at any barline that begins another ending, or at part/section end (including Regular barlines: relax stops_repeat_ending_barline at score.rs:312 and the Regular early-return guard in crates/croma-core/src/musicxml/barline


### `barline-abc2xml-drops-trailing-double` — **QUIRK** (reference_quirk, repro=None)

*Share:* ~1-2% rows (10 files croma-keeps-double + 9 files invisible-placement) — *files:* tune_009412.abc, tune_010391.abc


tune_009412 ends `.c3||` — croma emits light-light on the last measure's right in both parts; fresh abc2xml run emits no barline there at all, silently dropping the source's explicit `||` (ABC 2.1 §4.8 line 945: `||` is the thin-thin double bar). 10 measure-aligned files / 22 rows show croma_double-or-final_ref_plain with croma matching the source. Related family: the 9 aligned files (14 rows) where abc2xml emits `[|]` (bar-style none) at a position croma absorbs the invisible barline — both spec-valid serializations of §4.8's invisible bar (line 959); at most a comparator-normalization candidate. In all of these croma preserves more of the source than the reference; document, keep croma's behavior.


### `barline-phantom-measure-alignment-cascade` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~30-35% rows (the 228 shifted files' pair-rows) — *files:* tune_003837.abc, tune_000289.abc, tune_004928.abc


228 of the 755 barline-category files have measure_delta != 0 (refreshed manifest). Within exactly those files my positional re-extraction shows complementary pairs — 175 files with croma_repeat_ref_none vs 173 with ref_repeat_croma_none, plus matched ref_double/croma_double pairs — i.e. the SAME repeat/double exists in both outputs but at a shifted measure index, so the positional comparator books one missing + one extra barline row. Driver is doc 02 (verified): abc2xml fabricates a zero-note <measure> for standalone annotations/part labels/inline K: at bar boundaries (e.g. tune_003837: ref 78 vs croma 70 measures, the 8 surplus all zero-note). ABC 2.1 §2.2.1/§4.8: a bar line or annotation does not constitute a measure of music; croma's folded output is more correct. These barline rows are symptoms of the measure-structure divergence, not barline disagreements.


### `barline-spaced-and-newline-split-coalesced` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~10-12% rows (66 aligned files/127 rows ref-double + part of ref-final + shifted analogues) — *files:* tune_002874.abc, tune_002876.abc, tune_002327.abc, tune_008028.abc


Minimal: `CDEF| |GABc| |` — abc2xml emits two light-light doubles; croma emits two plain barlines (no <bar-style>), zero warnings, identical 2-measure structure. Same for line-split forms: tune_008028 (`...d6 |\` newline `|: ...` → ref adds light-light on m24-right; croma plain + same repeat on m25-left) and tune_009113 (line ends `GA |`, next line starts `|: ` with NO backslash — abc2xml still merges to `||:`). ABC 2.1 §4.8 (line 961-962): liberal recognition applies to a contiguous *sequence*; §4.9 (966-969): whitespace around bar tokens is significant (`| [1` legal vs `| 1` not). A space/newline-separated `| |` is two thin bars, so croma is spec-defensible. VERIFIES doc 04 and EXTENDS it: coalescing also happens across plain newlines without the `\` continuation. tune_002874/002876: 6 light-light each missing in croma, measure counts equal (18/18). Comparator could normalize adjacent-…



## direction

### `direction-dangling-annotation-dropped` — **FIXED(33a)** (croma_bug, repro=True)

*Share:* ~10-15% rows directly (66-93 rows, ~48-60 files; mostly the MetronomeMark-vs-TextExpression pairs), plus it originates many alignment-cascade rows — *files:* tune_000377.abc, tune_000381.abc, tune_000453.abc, tune_000529.abc


Minimal: 'X:1\nM:C|\nK:G\n"Single Reel"\nB2 efgf eBB2|' — croma output contains no 'Single Reel' at all (grep count 0, no stderr warning); abc2xml emits <words>Single Reel</words> attached to the first note. Same for an annotation at end of a line with the note on the next line ('C D "next"\nE F|'). Per ABC 2.1 §4.19 an annotation positions relative to the following note, and a code line break is not a musical boundary (§6.1.1), so the text must survive. This is the line-break sibling of parser-backlog item 4 ('quoted text before a barline is silently dropped' — same catch-all discard). Real corpus hit: tune_000377 ('"Single Reel"' on its own line after K:) — its manifest verdict POSITIONAL_CASCADE/'Croma correct' is wrong; croma loses the text.


*Fix:* Lower layer: /Users/rodox/dev/rs/croma/crates/croma-core/src/lower/mod.rs ~lines 543-547 — the catch-all arm 'MusicItem::ChordSymbol(_) | MusicItem::Annotation(_) | ... => {}' discards standalone items that the parser flushed at line end (parse/music.rs flush_pending_attachments). Buffer them like pending_grace_groups and attach to the next timed event; decide jointly with backlog item 4 (barline 


### `direction-plus-decoration-words-vs-stopped` — **FIXED(33a)** (croma_bug, repro=True)

*Share:* ~8% rows (50 rows, 42 files) — *files:* tune_001035.abc, tune_001020.abc, tune_001037.abc, tune_001666.abc


Minimal: 'X:1\nK:C\n!+!C D !f!E F|' — croma emits <direction-type><words>+</words> (with an unsupported-decoration warning); abc2xml emits <notations><technical><stopped/> on the note (the '+' glyph). ABC 2.1 spec line 1101 defines !+! as 'left-hand pizzicato, or rasp for French horns' — a decoration of the following note, so a note-attached notation is the faithful mapping; croma's detached direction loses the note binding (and, because abc2xml's version is not a direction at all, every !+! shifts the comparator's direction alignment, producing the '+' vs 'f'/'2.F.' pair rows). Low severity: the information is still visible as text.


*Fix:* Export layer: /Users/rodox/dev/rs/croma/crates/croma-core/src/musicxml/notation.rs decoration_notation() — add '"+" | "plus" => NotationKind::Technical("stopped")' (MusicXML's + glyph); the words fallback in musicxml/direction.rs then stops firing.


### `direction-wedge-decorations-as-words` — **FIXED(33a)** (croma_bug, repro=True)

*Share:* ~6% rows (37 rows, 9 files; plus knock-on cascade rows in those files) — *files:* tune_006011.abc, tune_013137.abc, tune_008744.abc, tune_010034.abc


Minimal: 'X:1\nK:C\n!<(!C D !<)!E F|!>(!G A !>)!B c|' (also long forms !crescendo(! etc., incl. via U: redefinition as in tune_006011). croma emits <direction-type><words><(</words> / <words>crescendo(</words> — i.e. the raw decoration name printed as visible text; abc2xml emits <wedge type="crescendo"/> / <wedge type="diminuendo"/> / <wedge type="stop"/>. ABC 2.1 spec (abc_standard_v2.1.full.md lines 1114-1121) defines !crescendo(!/!<(!/!diminuendo(!/!>(! and their close forms as hairpin marks, so printing 'crescendo(' as score text mangles the notation. Croma does log an unsupported-decoration warning, but the output is wrong. REFUTES doc 10's 'no genuine Croma direction bug' and the manifest's 'Croma correct' for tune_006011 (its POSITIONAL_CASCADE justification only checked pitch sequences).


*Fix:* MusicXML export layer: /Users/rodox/dev/rs/croma/crates/croma-core/src/musicxml/direction.rs (decoration dispatch at ~lines 113-127 falls through to write_direction_words) — add a wedge arm for the 8 names ('crescendo(', '<(', 'crescendo)', '<)', 'diminuendo(', '>(', 'diminuendo)', '>)') emitting <direction-type><wedge type=crescendo|diminuendo|stop/>; the name table lives next to decoration_notat


### `direction-alignment-cascade` — **EQUIV** (legitimate_difference, repro=None)

*Share:* ~55-65% rows (284 directly measurable + most of the 180-row shifted-text tail) — *files:* tune_001366.abc, tune_001361.abc, tune_000456.abc, tune_006936.abc


172/643 rows pair IDENTICAL text+kind at shifted measure numbers (e.g. tune_001366: croma m12/'*' vs ref m13/'*', m14/'trill' vs m15/'trill' — a constant +1 measure offset from a structural divergence already triaged in docs 02/07), and another ~112 rows sit in files dominated by other categories (e.g. tune_001361: 271 missing + 155 extra rows; its 3 direction rows pair 'For'@11 vs 'p'@2 — pure index shift). Most of the remaining shifted-text pairs ('c.' vs 'b.', '1f' vs '2f', '2.F.' vs '1.F.') are one-item shifts whose origin is one of the per-construct causes above (a croma-dropped dangling annotation, a croma-extra '+' words, abc2xml turning '""' into an empty <words/> that croma reasonably drops — verified: croma omits the degenerate '""' annotation, abc2xml fabricates <words/>). These rows are symptoms, not direction bugs. Comparator suggestion: align directions per (measure, offset…


### `direction-text-only-q-default-bpm` — **EQUIV** (legitimate_difference, repro=True)

*Share:* ~4% rows (26 rows) but ~9% of files (24 files where it is the file's only direction row); 7 of the 8 evidence-pack samples — *files:* tune_003833.abc, tune_005558.abc, tune_004543.abc, tune_004826.abc


Minimal: 'Q:"Allegretto"' — both emit <words>Allegretto</words> plus a sound-only tempo; croma <sound tempo="120.00"/>, abc2xml <sound tempo="112.00"/> (its built-in table: Andante=88, Allegretto=112, ...). music21 surfaces this as 'MetronomeMark Quarter=120 (playback only)' vs Quarter=88/112. ABC 2.1 §3.1.8 explicitly leaves interpretation of a string-only tempo to the program, so neither BPM is mandated; no information is lost (the visible words match). CONFIRMS doc 10 sub-cause 1. Comparator suggestion: ignore the playback-only sound tempo (or the BPM value) when the source Q: carries no numeric tempo — would clear 24 files outright.


### `direction-malformed-annotation-and-q-fields` — **QUIRK** (reference_quirk, repro=None)

*Share:* ~3-5% rows (~20-30 rows) — *files:* tune_007910.abc, tune_006535.abc, tune_006279.abc, tune_014467.abc


Malformed input. (a) '" >"G2' (tune_007910): first char is a space so per §4.18/§4.19 it is neither a valid placement-prefixed annotation nor a parseable chord; croma keeps the visible text <words>></words>, abc2xml emits an EMPTY <words/> (verified on minimal snippet; 11 corpus rows have abc2xml-emptied text) — abc2xml drops source text, croma is the defensible recovery. (b) 'Q:400.' (tune_009608), 'Q:1/4=1/4=160' (tune_006535), 'Q:"Figs 1-3" 3/2=84 "Fig 4" 3/2=76' (tune_014467): croma falls back to emitting the literal field text as words (preserving everything), abc2xml partial-parses (e.g. takes '400' or only the first string). (c) Free text lines inside the tune header (tune_006279 'for this tune. Feel free...'): abc2xml parses the prose AS NOTES (reference measure 1 starts F,E,F fabricated from the words — verified in tune_006279.xml), shifting croma's tempo direction offset; croma…



## duration

### `duration-missing-long-maxima-note-types` — **FIXED(HEAD 8c76fc8)** (croma_bug, repro=True)

*Share:* ~0.4% rows (78/19,973) — *files:* tune_000386.abc, tune_004577.abc, tune_001028.abc


Minimal: L:1/4 / M:4/2 / 'C16|' (16 quarters = 4 whole notes = a long): croma emits <type>breve</type> (duration 128 is correct, written type wrong); abc2xml emits <type>long</type>. Code: crates/croma-core/src/musicxml/note.rs note_type_candidates() starts at breve (2/1) — no 'long' (4/1) or 'maxima' (8/1) candidates, although MusicXML note-type-value includes both; durations not expressible as breve x permitted ratio fall through to the unsupported quarter fallback (hence the 4 quarter->longa rows). 78 notation-only type rows (breve->longa 70, quarter->longa 4, breve->complex 4) across 31 early-music files. Note abc2xml handles breve itself fine (it emits type breve for 8-ql rests in tune_001029), so this is purely croma's cap.


*Fix:* crates/croma-core/src/musicxml/note.rs note_type_candidates(): add NoteTypeCandidate entries for 'long' (4/1) and 'maxima' (8/1) ahead of breve; the existing dot/ratio search then covers dotted longs without touching the fallback.


### `duration-tuplet-written-type-from-sounding` — **FIXED(HEAD 8c76fc8)** (croma_bug, repro=True)

*Share:* ~1% rows (~150-200/19,973), but over-represented in single-category files (6 of the 8 evidence-pack samples) — *files:* tune_005074.abc, tune_003186.abc, tune_008083.abc, tune_006746.abc


Minimal 1: M:6/8 'CCC (4BABd|' (quadruplet, 4-in-time-of-3 per ABC 2.1 section 4.13): croma emits <type>16th</type><dot/> + <actual-notes>4</actual-notes><normal-notes>3</normal-notes> with duration 3/8 quarter — internally inconsistent (dotted-16th x 3/4 = 0.28125, not 0.375); abc2xml emits the conventional <type>eighth</type> + TM 4:3 (0.5 x 3/4 = 0.375, consistent). Minimal 2: M:4/4 '(3F>GG' (broken rhythm inside triplet): croma writes the lengthened F as eighth with NO dot (duration says dotted-triplet-eighth, 0.5 ql); abc2xml writes eighth + <dot/> + TM 3:2 (consistent). Mechanism in code: crates/croma-core/src/musicxml/note.rs note_spelling() tries to spell the SOUNDING duration as plain type+dots first (first loop) and only consults explicit_time_modification as a fallback; the writer then re-attaches the tuplet TM anyway (note.rs ~line 283: explicit_time_modification.or(spelling.…


*Fix:* crates/croma-core/src/musicxml/note.rs note_spelling(): when explicit_time_modification is Some, compute normal_duration = duration x actual/normal and try spelling THAT first (returning the explicit TM), before the sounding-duration loop; keep sounding-duration spelling only for the no-TM path. The existing test 'reduced_duration_note_types_do_not_emit_spurious_tuplets' (musicxml/mod_tests.rs:186


### `duration-voice-scoped-unit-length-global-leak` — **FIXED(33b)** (croma_bug, repro=True)

*Share:* ~4.4% rows (884/19,973) — *files:* tune_014637.abc, tune_005353.abc, tune_000346.abc


Minimal: header L:1/4, M:4/4, K:C, then 'V:1 treble' newline 'L:1/8' newline 'V:2 bass', bodies for both. Croma renders V:2's C,D,E,F, as eighths (duration 4, type eighth); abc2xml keeps V:2 at header L:1/4 (duration 120, type quarter). Same bug both directions in the corpus: tune_014637 (L:1/8 under V:1 leaks to voices 2-5; croma bars fill 2.0 of a 4/4 bar — verified via music21: croma parts 1-4 m2 fill 2.0 vs ref 4.0) and tune_005353 ([L:1/4] inline under [V:2] leaks into V:1 from m5; croma V:1 bars overfull 4x, ql ratio 4). ABC 2.1 section 7 is VOLATILE/silent on body-field voice scoping, but croma's global reading silently mangles the music (under/overfull bars destroy vertical voice alignment the transcriber encoded), which meets the croma_bug rubric; per-voice scoping is the de-facto convention abc 2.2 codifies and croma itself just adopted for [M:] (PR #70, lower/mod.rs apply_inli…


*Fix:* Parse layer: crates/croma-core/src/parse/field/mod.rs line 220 keeps a single global state.unit_note_length; crates/croma-core/src/parse/field/misc.rs (ParsedFieldKind::UnitNoteLength arm, ~line 420) overwrites it unconditionally. Note durations are materialized at parse time from this slot, so the fix is a per-voice unit-note-length map keyed by voice ID, switched on V:/[V:] (header L: as the fal


### `duration-abc2xml-grid-rounding` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~2% rows (~400/19,973 incl. associated type/dots rows) — *files:* tune_004607.abc, tune_005443.abc, tune_010337.abc


Signature: quarter_length ratios of 945/944 (87 rows), 315/314 (60), 63/62 (40), 315/628 (1) — off-by-one-lattice-unit errors impossible from any musical reading, only from quantizing exact values like 1/160-whole quintuplet members (e.g. tune_004607 '(5A/8B/8A/8G/8F/8' with L:1/8: each member 1/64-note x 2/5) onto abc2xml's fixed divisions grid. Croma's values are the exact rationals. ABC 2.1 section 4.3 requires handling lengths down to 1/128 and nothing sanctions rounding; croma is exact and spec-correct. 361 rows in 13 files (plus their knock-on type/dots rows). Verifies prior doc 06 sub-cause (b) for this family — but see duration-measure-yes-rest-truncation: doc 06's tune_001029 'abc2xml caps the duration' example actually belongs to a different mechanism and should be re-attributed.


### `duration-cascade-from-structural-divergences` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~70% rows (13,927/19,973) — *files:* tune_001062.abc, tune_011865.abc, tune_014784.abc


Joining the 19,973 duration rows (docs/untracked/phase-33/full-10k-mismatches.parquet, mismatch_category='duration') against per-file-manifest-refreshed.csv: 13,025 rows sit in PHANTOM_MEASURE files, 474 CASCADE, 284 MULTIREST_EXPANSION, 100 ABC2XML_DROPS_MUSIC, 44 ABC2XML_DROPS_TACET = 13,927 rows. Spot check tune_001062.abc (294 duration rows, measures 15-46, alongside 480 extra_in_croma + 500 missing_in_croma + 76 measure_alignment rows): the duration rows start exactly where the measure-number shift begins, comparing different events positionally. These are alignment fallout from causes already verified in docs/comparison/abc2xml-divergences/02-phantom-measures.md, 03-multi-measure-rest.md, 07-cascade-artifacts.md, 11-multipart-and-partgroup.md — abc2xml fabricates/drops measures; croma is spec-correct per those docs. No independent duration behavior involved.


### `duration-chained-broken-rhythm` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~0.3% rows (~60/19,973) — *files:* tune_002873.abc, tune_009791.abc, tune_013002.abc


Minimal: M:4/4 L:1/8 'f2 e>f<g/ a2 z2|': croma gives e=0.75 (dotted by >), f=0.125 (halved by > and again by <), g/=0.375 (halved by /, dotted by <) — total 1.25 ql, exactly the nominal e+f+g/ total (duration-conserving, each operator applied to its own note pair). abc2xml gives e=0.375, f=0.25, g=0.375 — total 1.0 ql, i.e. it silently DROPS 0.25 ql and leaves the bar underfull. ABC 2.1 section 4.4 defines a>b (dotted a, halved b) and repeated signs (a>>b double-dot) but never chains across three notes, so the construct is under-specified; croma's extension is the natural compositional one and conserves written duration, abc2xml's is lossy. ~60 rows in 4-6 files (tune_002873 16, tune_009791 25, tune_013002 15, tune_001629 6; likely also tune_009205/9207). Not in prior doc 06.


### `duration-default-unit-length-meter-derived` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~23% rows (~4,550/19,973); ~75% of the non-cascade rows — *files:* tune_005004.abc, tune_005964.abc, tune_000399.abc, tune_007488.abc


Minimal: X:1 / M:2/4 / K:C / CDEF|GABc| -> croma emits all <type>16th</type>, abc2xml emits all <type>eighth</type> (2x every duration). ABC 2.1 spec lines 554-556 (/Users/rodox/dev/abc/docs/reference/abc_standard_v2.1.full.md): meter as decimal < 0.75 => default unit is a sixteenth; 2/4 = 0.5 is the spec's own example. Croma follows the spec; abc2xml ignores it. Signature in data: quarter_length ratio croma/ref = 1/2 (2,679 ql rows) plus one-step-shorter type pairs (16th->eighth 1,427, eighth->quarter 938, quarter->half 217, 32nd->16th 118, half->whole 57). Bucketing DURATION_EXACT_VS_ROUNDED rows by file: 22 no-L: files = 4,408 rows, plus tune_007488.abc (146 rows; malformed multi-tune file whose second tune starts at a bare T: with no X: — its first tune has no L:, M:2/4; note 'malformed input' for that one file). Verifies prior doc 06 sub-cause (a).


### `duration-measure-yes-rest-truncation` — **QUIRK** (reference_quirk, repro=True)

*Share:* <0.1% rows (~20/19,973) — *files:* tune_001029.abc, tune_001728.abc


tune_001029 (M:C|, L:1/2, %%MIDI nobarlines, opens 'z4' = 8 ql): RAW reference XML has <rest/> duration 960 (divisions 120 = 8.0 ql) <type>breve</type> — identical sounding value to croma's. But abc2xml also emits <rest measure="yes"/> on three LATER 4-ql rests inside the same giant unbarred measure (lines 784/892/1296). music21 (v10.3.0, musicxml/xmlToM21.py PartParser.xmlMeasureToMeasure) sets fullMeasureRest for the whole measure when ANY rest carries measure='yes', then rewrites the FIRST rest: ql != barDuration and type in (whole, breve) => quarterLength := 4.0, type whole. Hence the comparator rows 'croma 8.0/breve vs ref 4.0/whole' at m1 onset 0.0 — verified by instrumented parse (counts rest=12 note=151, fullMeasureRest=True). Same fingerprint in tune_001728 (1 measure="yes" in ref, 0 in croma). This REFUTES prior doc 06 sub-cause (b)'s explanation for tune_001029 ('abc2xml caps …



## extra_in_croma

### `extra-abc2xml-drops-text-and-second-staff-dynamics` — **QUIRK** (reference_quirk/stale_cascade, repro=True)

*Share:* ~0.3% rows (~120) — *files:* tune_015359.abc, tune_012129.abc, tune_008744.abc, tune_008746.abc


(a) tune_015359 'Q:”Slow”' (Unicode curly quotes; SS3.1.8 requires ASCII double quotes, so malformed input): croma emits <words>”Slow”</words> preserving the text; abc2xml emits nothing (minimal repro confirmed: croma <words>, abc2xml no words/metronome/sound at all). Recovery-choice difference on illegal input - note 'malformed input'. (b) tune_008744 ('V: 1 staves=2' two-staff part): croma keeps V:2's !p!/!f! dynamics (extra Dynamic rows at measures 3/11/15/20/24 match V:2's marks exactly); abc2xml drops dynamics from the second merged staff. A bare '!p!' on a single voice round-trips identically in both tools (minimal repro), isolating the staves-merge as abc2xml's loss. All 25 Dynamic-kind extra rows in the corpus fit this pattern. Croma preserves source information in both sub-cases; abc2xml silently loses it.


**Verifier correction:** The cause bundles two unrelated phenomena and should be split. (a) Curly-quoted Q: text (tune_015359, tune_012129, 2 TextExpression rows): verified, correctly reference_quirk — abc2xml's Q: regex requires ASCII quotes (ABC 2.1 SS3.1.8) and silently drops the malformed field; croma preserves the text. (b) All 25 Dynamic-kind extra rows: the triage mechanism ('abc2xml drops !p!/!f! on second staff of staves=2 merged part') is factually wrong — abc2xml drops nothing and neither tool merges the part.

*Re-verdicted in 37i:* Current four-file target export/compare confirms no
Croma code change is warranted. The only remaining rows are two
`extra_in_croma` direction/TextExpression rows for malformed curly-quoted `Q:`
fields: `tune_015359.abc` (`Q:”Slow”`) and `tune_012129.abc`
(`Q:”Slow & sad”`). Croma preserves the source text as MusicXML words; abc2xml
silently drops it because its Q-text regex accepts only ASCII quotes. The
alleged second-staff dynamics rows in `tune_008744.abc` and `tune_008746.abc`
are stale cascade artifacts from older direction alignment: current Croma and
reference XML both contain the same ten dynamics across P1/P2, and both files
compare as structural matches. Targeted compare after 37i: 4 exports, 2
structural matches, 2 mismatch rows, `extra_in_croma` 2, no import, harness,
or worker failures.


### `extra-decoration-words-fallback` — **FIXED(37a)** (croma_bug, repro=True)

*Share:* ~0.6% rows (~300; spread over ~80 files) — *files:* tune_003450.abc, tune_008364.abc, tune_012804.abc, tune_003269.abc


Minimal repro '!+!C4 !3!D4 | !diminuendo(!E2F2 !diminuendo)!G4 |': croma emits <words>+</words>, <words>3</words>, <words>diminuendo(</words>, <words>diminuendo)</words> (with warning abc.musicxml.decoration.unsupported 'preserved as MusicXML direction text'); abc2xml emits <technical><stopped/>, <technical><fingering>3</fingering>, and <wedge type="diminuendo"/>+<wedge type="stop"/>. '!arpeggio![CEG]4' -> croma <words>arpeggio</words>, abc2xml <arpeggiate/>. ABC 2.1 SS4.14 defines all of these (spec lines 1100-1119: '!0! - !5! fingerings', '!+! left-hand pizzicato', '!diminuendo(! start of a > diminuendo mark'); rendering a hairpin as the floating text 'diminuendo(' or a fingering as a free-text '3' mangles the source semantics in the output. Direction-extra text census across the corpus: '+' 176 rows, diminuendo(/) 75, crescendo(/) 15, arpeggio 19, slide 5, digits ~15 - i.e. ~300 of th…


*Fix:* MusicXML export layer only (parse already carries the decoration name). crates/croma-core/src/musicxml/notation.rs: decoration_notation() (line 160) lacks arms for "0"..="5" (needs a NotationKind variant carrying fingering text -> <technical><fingering>), "+"/"plus" -> Technical("stopped"), "arpeggio" -> new arpeggiate notation, "slide" -> <slide>/scoop. Wedges are spanners, so "diminuendo("/")", 

**Phase 37a verification:** `crates/croma-core/src/musicxml/notation.rs` now maps the remaining supported decoration fallbacks to MusicXML note-attached notations: `!0!`-`!5!` -> `<technical><fingering>`, `!arpeggio!` -> `<arpeggiate/>`, and `!slide!` -> `<articulations><scoop/>`, matching an abc2xml 268 probe for the minimal repro shape. Regression test `fingering_arpeggio_and_slide_decorations_emit_notations_not_words` fails on main for the missing fingering and passes after the fix. Verification: `cargo fmt --all -- --check`, `cargo test -p croma-core musicxml::tests::`, `cargo test --workspace`, `cargo run -p croma-cli -- xml examples/basic.abc`, and full 10k report-only comparison. Phase-36 -> phase-37a aggregate corpus delta: structural matches 8861 -> 8865, mismatch rows 164265 -> 164220, `direction` 463 -> 459, `extra_in_croma` 47606 -> 47565, no harness/import failures.


### `extra-multirest-collapsed-vs-expanded` — **FIXED(35a)** (croma_bug, repro=True)

*Share:* ~2.5% rows (1,231; 4 files) — *files:* tune_009310.abc, tune_005687.abc, tune_013477.abc, tune_013455.abc


tune_009310 contains 'Z2' twice: croma 54 measures, reference 56 (+1 per Z2). ABC 2.1 SS4.5 (spec lines 866-873) states the collapsed and expanded forms are 'musically equivalent (although they are typeset differently)' - both encodings are spec-valid. Confirms prior doc 03-multi-measure-rest.md verdict. Comparator normalization candidate: expand croma's multirest (or collapse the reference's run of whole-measure rests) before measure-indexed alignment; this would also clear the files' missing_in_croma/measure_alignment rows.


**Verifier correction:** The mismatch mechanism in the claim is correct (Zn collapse vs expansion shifts measure-indexed alignment, making croma's later notes 'extra'), but the legitimate_difference verdict misapplies ABC 2.1 §4.5: the spec equates Z4 with z4|z4|z4|z4 — both denote n MEASURES — which licenses abc2xml's expansion, not croma's collapse. MusicXML has no single-measure multirest construct; the standard encoding (even with <multiple-rest> measure-style) requires all n measures present. Croma's overfull measu


### `extra-liberal-chord-symbol-parsing` — **EQUIV** (legitimate_difference, repro=True)

*Share:* ~0.5% rows (209-252; 41 files) — *files:* tune_000456.abc, tune_002486.abc, tune_009872.abc


Minimal repro '"c"C4' -> croma <harmony><root-step>C</root-step><kind text="c">major</kind>; abc2xml <words>c</words>. '"G/b"gBB' (tune_002486) -> croma full harmony G with <bass-step>B</bass-step>; abc2xml <words>G/b</words>. ABC 2.1 SS4.18 says chord roots are A-G (so lowercase is out-of-grammar) but the same section is marked VOLATILE and instructs 'programs should treat chord symbols quite liberally'; SS4.19 annotations must start with ^ _ < > @, which these strings do not, so abc2xml's words reading is no more spec-grounded than croma's harmony reading. Both are defensible interpretations of an ambiguous string; croma's preserves the harmonic intent (and keeps the literal spelling in kind/@text). Comparator normalization candidate: compare harmony figures and words as one text channel when the other side lacks the aligned object.


### `extra-abc2xml-drops-music-legacy-linebreak` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~3.9% rows (1,907; 4 files) — *files:* tune_011320.abc, tune_014784.abc, tune_008657.abc, tune_011364.abc


tune_011320 body uses old-syntax '!' line breaks including mid-line ('c2d2e2|!d2c2B2|f6|...'). Source contains 25 bars; croma emits 25 measures, reference emits 21 - abc2xml loses 4 measures of real music (it treats the mid-line '!' as a decoration delimiter and swallows the span to the next '!'). Croma preserves all source music (it warns 'abc.music.unclosed_decoration: Decoration delimiter was preserved and skipped' and continues). ABC 2.1 deprecates '!' as a line-break but never licenses dropping bars; croma's recovery is information-preserving. Matches manifest verdict ABC2XML_DROPS_MUSIC (doc 02 'Croma preserved the real music abc2xml lost'). Note: legacy '!' input is technically out-of-2.1-spec, so this is also a malformed/legacy-input recovery difference - still not a croma bug.


### `extra-abc2xml-drops-tacet-bars` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~1.6% rows (765; 2 files) — *files:* tune_011164.abc, tune_011865.abc


tune_011164 has four [V:] voices each starting with runs of empty bars ('[V: P1_1]  | | | | | | c3/4B/4 ...'). Croma emits 36 measures total, reference 30 - abc2xml silently discards the tacet bars, desynchronizing voices that the source keeps bar-aligned. In a multi-voice score those bars exist (other voices sound in them), so croma's empty/rest measures are the structurally correct MusicXML. Matches manifest verdict ABC2XML_DROPS_TACET (2 files: tune_011164, tune_011865). The empty-bar input is unusual (machine-extracted ABC) but croma's choice preserves score structure; abc2xml's loses it.


### `extra-phantom-measure-cascade` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~89.8% rows (43,838/48,800; 217/367 files) — *files:* tune_005959.abc, tune_003837.abc, tune_014970.abc, tune_012496.abc


Minimal repro A: 'CDEF GABc|\n[M:2/4]!\n|CD EF|GA Bc|' -> croma 3 measures [8,4,4]; abc2xml 4 measures [8,0,4,4] (empty measure 2 carries only the <time> change). Minimal repro B: '"A"\\\n|:dddd d2A2 | ffff f2d2 :|' -> croma 2 measures [6,6]; abc2xml 3 [0,6,6] (empty measure 1 holds the annotation). Corpus confirmation: tune_005959 ref measure 2 is '<measure number="2"><attributes><time>2/4</time></attributes></measure>' with zero notes; croma 88 vs ref 89 measures, pitches otherwise identical. Because the comparator aligns by measure index, one inserted empty measure shifts every later measure pair, producing ~5 extra rows per croma note (note/pitch/duration/tie/tuplet) plus symmetric missing_in_croma rows. ABC 2.1 SS4.8 (bar lines are boundaries, not measures), SS4.16/SS4.19 (inline fields and annotations are not music) - nothing creates a measure from an annotation or [M:] change. Mat…



## harmony

### `harmony-chord-before-plus-decoration-dropped` — **FIXED(33a)** (croma_bug, repro=True)

*Share:* ~3% rows (73/2428), 7 files — *files:* tune_006965.abc, tune_006966.abc, tune_007676.abc


Minimal repro /tmp/harmony-probe/t2_chord_before_plusdeco.abc: `"F"+>+a4 gfed | "C7"c4 ...` — croma emits only C7; abc2xml emits F and C7. The `+...+` decoration path calls flush_pending_attachments (crates/croma-core/src/parse/decoration.rs:69 invalid/plus-delimiter branch), and lowering discards the flushed standalone ChordSymbol (lower/mod.rs:543-547). `+...+` is outdated 2.0 syntax (spec §10.2.2) but even if croma rejects the decoration it must not destroy the preceding chord symbol — same silent-data-loss family as backlog item 9 (FIXED for grace/slur/tuplet) which missed this path. Corpus: tune_006965/66/67 (`"F"+>+a4` loses F:major, confirmed by full harmony-sequence diff: croma 21 vs ref 22 items, missing F:major) and tune_007676/77/78.


*Fix:* Parse layer: crates/croma-core/src/parse/decoration.rs:69 (and sibling flush sites in that file) — stash/restore the pending bundle around `+...+` parsing exactly like the fixed parse_grace_group/parse_slur/parse_tuplet cases (backlog item 9), so the quoted text rides through to the following note.


### `harmony-quoted-in-unterminated-bracket` — **OPEN** (croma_bug, repro=True)

*Share:* ~0.2% rows (6/2428), 1 file — *files:* tune_005224.abc


Malformed input. Minimal repro /tmp/harmony-probe/t5_bracket_quoted.abc: `"G"B2A2 G2F2 |["cont" "Am7"F2G2 A2B2 | ...` — croma emits G, D7, G (drops Am7 and the 'cont' annotation); abc2xml emits G, Am7, D7, G. The source violates ABC 2.1 §4.20 (chord delimiters [ ] should enclose note sequences with chord symbols OUTSIDE the bracket) and the bracket is never closed — illegal input, so the recovery-choice difference is not a croma bug per triage policy. Note: croma's recovery does silently lose a well-formed chord symbol, and the cause-1 fix (carrying pending quoted text to the next timed event) would likely rescue this case for free. Single corpus file: tune_005224 (manifest verdict PHANTOM_MEASURE; harmony-sequence diff shows croma 20 vs ref 23 items, missing Am7 x2 + D:dominant).


**Verifier correction:** The claimed difference reproduces, but the verdict reference_quirk rests on a false premise. `|["cont"` is not an unterminated chord bracket with illegally-placed chord symbols: abc2xml parses `[` + quoted string as a quoted-text volta (variant ending label) via an explicit grammar production (abc2xml.py line 420 `volta_text = Suppress(Literal('[')) + pas(r'"[^"]+"')`, emitted as <ending>cont</ending> at lines 1927-1930). This is the well-known abcm2ps/abc2svg/abcjs text repeat-bracket extension


### `harmony-quoted-symbol-dropped-at-boundary` — **FIXED(33a)** (croma_bug, repro=True)

*Share:* ~54% rows (1302/2428), ~44/115 files — *files:* tune_008391.abc, tune_006398.abc, tune_013008.abc, tune_015066.abc


Minimal repro /tmp/harmony-probe/t1_label_before_barline.abc: `"A"|: E2 F2 G2 A2 | "C"c8 "B":| ...` — croma emits only the C harmony; abc2xml emits A, C and B. Repro t6: `"Dm"Z|Z|...` — croma drops Dm, abc2xml keeps it. Corpus variants: `"A"\` + newline + `|:` (tune_006398), chord at end of a music line with its note on the next line (tune_013008 `"Em7"` then newline `(3efg`), `"A":c4` (tune_004767). Mechanism: parse-time flush_pending_attachments (crates/croma-core/src/parse/music.rs:727, called from parse/barline.rs:14,72 and at line end music.rs:577) converts pending ChordSymbol items to standalone MusicItem::ChordSymbol, which the lowering catch-all at crates/croma-core/src/lower/mod.rs:543-547 silently discards. ABC 2.1 §4.18 binds a chord symbol to the note it precedes; dropping it is silent data loss. Already cataloged as parser-backlog item 4 (barline case only, marked 'Open: sil…


*Fix:* Parse/lower layers. Either stop flushing pending chord symbols (and arguably annotations) at barlines/line-ends in crates/croma-core/src/parse/barline.rs:14,72 + parse/music.rs:577 so they bind to the next note per §4.18, or buffer standalone MusicItem::ChordSymbol in lowering the way pending_grace_groups already works (crates/croma-core/src/lower/mod.rs:530-542) and attach to the next timed event


### `harmony-noncanonical-quoted-text-classified-as-chord` — **EQUIV** (legitimate_difference, repro=True)

*Share:* ~22% rows (~530/2428), ~30 files — *files:* tune_009872.abc, tune_003217.abc, tune_009012.abc, tune_002633.abc


Minimal repro /tmp/harmony-probe/t4_lowercase_chord.abc: `"a"AA "d"Bc` — croma emits <harmony> A:major/D:major (original text preserved in kind@text="a"); abc2xml emits <words>a</words>. Repro t7: `"D "A2 ... " f"c2` — croma: D:major and F:major harmonies; abc2xml: <words>. Spec: §4.18 chord root 'can be A-G' (uppercase) and the VOLATILE note says 'programs should treat chord symbols quite liberally'; §4.19 annotations require a ^ _ < > @ prefix — bare lowercase/prose strings fit neither bucket, so interpretation is implementation-defined. Croma loses no information (full string kept in kind@text); abc2xml loses none either (words). Both spec-defensible. Biggest single contributor: tune_009872 (142 rows; melodeon-style mixed `"D"(d2 "d"d)` uppercase+lowercase symbols → croma 143 harmonies vs ref 51). Candidate comparator normalization: treat a croma <harmony> aligned with a reference <wo…


### `harmony-lowercase-bass-chord-demoted-by-abc2xml` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~10% rows (246/2428 pure + parts of 2 mixed files), ~13 files — *files:* tune_002486.abc, tune_009870.abc, tune_003705.abc, tune_006445.abc


Minimal repro /tmp/harmony-probe/t3_lowercase_bass.abc: `"C"edB "G/b"gBB` — croma emits <harmony> G major with <bass-step>B</bass-step>; abc2xml emits <words>G/b</words> (chord lost from harmony stream). Control t8 with uppercase `"G/B"` shows abc2xml handles it fine, so it is specifically the lowercase bass that trips abc2xml. ABC 2.1 §4.18 is explicit: 'The bass note can be any letter (A-G or a-g) ... The case of the letter used for the bass note does not affect the pitch.' Croma is the spec-correct side; abc2xml drops harmony semantics the spec mandates. Document as divergence, keep croma's behavior; the comparator could also apply the harmony-vs-words normalization from the previous cause.


### `harmony-pure-positional-shift-cascade` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~11% rows (275/2428), 22 files — *files:* tune_014430.abc, tune_005004.abc, tune_015555.abc


22 files have byte-identical normalized harmony sequences in both XMLs; only the (measure, offset) coordinates differ. Two documented drivers: (a) abc2xml phantom empty measures at annotation/section/inline-key boundaries shift all subsequent measure numbers (doc 02; e.g. tune_014430: croma Am@m1/G@m2/Dm@m3 vs ref Am@m2/G@m3/Dm@m4, manifest PHANTOM_MEASURE delta 3); (b) abc2xml hardcodes L:1/8 instead of the §4.6 meter-derived default — tune_005004 has M:2/4 and NO L: field, so unit note is 1/16 per §4.6; croma's harmony offsets are exactly half abc2xml's (C@m44 off1.0 vs off2.0), matching doc 06. Croma is spec-correct in both; these harmony rows are pure comparator cascades. This portion of the prior verdicts (docs 02/06/07) is VERIFIED.



## lyric

### `lyric-bar-advance-blind-to-rest-only-measures` — **OPEN** (croma_bug, repro=None)

*Share:* ~4% rows (5/140), 1/9 files — *files:* tune_013577.abc


Minimal: `C C C C | z4 | D D D D |` + `w: one two three four||five six sev- en` — abc2xml puts five/six/sev/en on bar 3 (spec reading: each `|` advances one bar, the rest-only bar takes the empty segment); croma drops all four syllables with abc.lyric.syllable_count warnings. Second repro: rest-only pickup `z4 | z C C z | z E E E | G4 |` + `w: |A- wake!|a- wake! *|so|` — abc2xml starts 'A- wake!' in m2; croma starts in m3 and loses 'so'. This is exactly tune_013577 V:3 (`z | z[C,G,] [C,G,] z | ...` + `w: |A- wake!|a- wake! *|||`): croma P3 m3 carries 'A|Ring' where reference (and the spec) put it in m2, producing the m3 'A vs a'/'Ring vs ring' rows; the m46 rows ('And let His' vs 'grand and glo-') come from the same lag re-triggered by `...world,||And...` over a rest-only `z` bar. Spec: ABC 2.1 §6.1.2 — `|` in w: 'advances to the next bar'; the bar structure is the music line's, and a ba…


*Fix:* Lower layer. crates/croma-core/src/lower/align.rs: replace the refs-gap heuristic in advance_bar_marker with an explicit bar cursor over the voice's measure list (current measure number; each `|` increments it by exactly one, then position jumps to the first alignable ref with measure_number >= cursor). Also consider upgrading the end-of-line syllable spill (line 138-145) so dropped syllables are 


### `lyric-liberal-barline-bracket-pair-note-drop` — **OPEN** (croma_bug, repro=None)

*Share:* ~45% rows (63/140), 4/9 files — *files:* tune_012835.abc, tune_009178.abc, tune_009179.abc, tune_012962.abc


Minimal: `X:1\nM:6/8\nL:1/8\nK:G\ne2 E E2 ][ f | g3 f3 |\nw: one-ly, Tam. They snool me`. Croma emits warnings abc.music.barline.liberal (for `]` alone) + abc.music.unclosed_chord ('Chord group was preserved and skipped') and OMITS the f note entirely; the syllable 'They' shifts onto the next real note and the last syllable falls off the end. abc2xml treats `][` as one barline (light-heavy) and keeps F in its own measure with lyric 'They'. Corpus-verified: tune_012835 croma=57 vs ref=58 pitched notes (ordered lyric streams diverge exactly at the missing 'For'); 009178/009179 83v84; 012962 64v66 — so the prior PHANTOM_MEASURE verdict ('Same N notes ... phantom EMPTY measure ... Croma's folded layout is correct [doc 02]') in docs/untracked/phase-33/per-file-manifest-refreshed.csv is REFUTED for these 4 files: the reference's extra measure is NOT empty, it holds a real source note croma los…


*Fix:* Parse layer. crates/croma-core/src/parse/barline.rs parse_barline: the break-after-`]` rule (lines 33-37) should keep consuming a following `[` when it continues a barline shape (safe heuristic: `[` followed by whitespace/EOL/another barline char — i.e. not a digit (variant ending), not letter+`:` (inline field), not chord content), emitting one Liberal barline for the whole `][` run. Secondary ha


### `lyric-voice-overlay-syllable-matching` — **OPEN** (croma_bug, repro=None)

*Share:* ~4% rows (5/140), 1/9 files — *files:* tune_006565.abc


Minimal: `C D E F & G, A, B, C, |` + `w: one two three four five six sev- en` — abc2xml assigns five/six/sev/en to the overlay notes G,A,B,C (code order); croma leaves the overlay notes lyric-less and drops the four syllables with abc.lyric.syllable_count warnings. Corpus: tune_006565 m7 `D2D2 D2\"^*\"D> D&x6C> D` + `w:|***And tis down by the green-wood side-oh!` — reference puts 'down','by' on the overlay C>,D notes (v2) and 'the green wood side oh!' in m8/m9; croma resumes at the next main-voice note so its m8 is two syllables behind ('down by the green wood'), producing all 5 rows. Spec: ABC 2.1 §7.4 — 'Words in w: lines ... are matched to the corresponding notes as per the normal rules for lyric alignment, disregarding any overlay in the accompanying music code', and §7.4's own example (`g4 f4 | e6 e2 |` / `&& (d8 | c6) c2|` with `w: ha-la-| lu-yoh` `+: lu- | -yoh`) only adds up if t…


*Fix:* Lower layer. Overlay events are lowered into a separate `{voice}.overlayN` timeline (crates/croma-core/src/lower/timeline.rs, OverlayBuilder around lines 142-151), so crates/croma-core/src/lower/align.rs `alignable_refs` never sees them. Fix: when building alignment refs for a voice, merge in its overlay timelines' alignable events ordered by source_order (AlignableRef already sorts on source_orde


### `lyric-malformed-header-field-parsed-as-music` — **QUIRK** (reference_quirk, repro=None)

*Share:* ~6% rows (9/140), 2/9 files — *files:* tune_001361.abc, tune_001365.abc


tune_001361/tune_001365 both contain the invalid field line `notC:Composed by Edwd. Towerzey` before `K:`. abc2xml treats it as a music line: reference measure 2 contains fabricated pitches C4 C4 E5 D5 B5 E4 D5 D5 E5 rest E5 — exactly the note letters [C C e d b E d d e z e] occurring in 'notC:Composed by Edwd. Towerzey' — and the first w: line's syllables (E-rect ... your he_ads ...) attach to that garbage, shifting all later lyric/measure alignment. Croma ignores the malformed line and aligns w: per ABC 2.1 §6.1.2 (verified croma m1-m4: E/rect/.../your | he/_/ads/E — matches the source `w:E-rect__ your he_ads E-ter__nal Gates Un-` exactly). Malformed input: `notC:` is not a legal information field in a tune header (§3/§4.16); recovery-choice differences on illegal input are not croma bugs, and abc2xml's choice fabricates structure. Verifies the prior doc-07 'header/prose mis-segmentati…


### `lyric-phantom-empty-measure-cascade` — **QUIRK** (reference_quirk, repro=None)

*Share:* ~41% rows (58/140), 1/9 files — *files:* tune_006403.abc


tune_006403 ('Al Hanisim', sections `\"A\"\\`, `\"B\"\\`, `\"Bridge\"`): ordered pitched-note streams are IDENTICAL (131=131) and ordered lyric-syllable streams are IDENTICAL (84=84); reference just has 51 measures vs croma 48 — e.g. reference m1 is a zero-note measure carrying only the 'A' annotation, so croma m1 'Al ha' aligns against ref m2 and every one of the 58 lyric rows is the same text offset by 1-3 measure numbers. Spec: ABC 2.1 §2.2.1 (music code = notes/barlines/symbols) and §4.8 (a barline marks a boundary, it is not a measure); an annotation string (§4.19) is not music, so no measure should be fabricated. This VERIFIES the prior verdict (docs/comparison/abc2xml-divergences/02-phantom-measures.md + 07-cascade-artifacts.md) for this file. Optionally teach the comparator to align by note-bearing measure index (skip zero-note measures) to suppress this cascade.



## measure_alignment

### `measure-alignment-complex-meter-time-dropped` — **FIXED(37c)** (croma_bug, repro=True)

*Share:* ~0.5% rows (34 rows in tune_011528 + a few '?' bar_duration files like tune_003733/tune_003808) — *files:* tune_011528.abc, mod.rs, attributes.rs


Minimal repros: header `M:(2+3+2)/8` (/tmp/ma_complex.abc) — explicitly legal per ABC 2.1 SS3.1.6 line 539 ('It is also possible to specify a complex meter, e.g. M:(2+3+2)/8') — croma emits NO <time> element and NO diagnostic; abc2xml emits <beats>(2+3+2)</beats><beat-type>8</beat-type>. Header `M:3/4+2/4` (/tmp/ma_additive.abc, extension syntax, tune_011528): croma again silent + no <time>; abc2xml fabricates 4/4 (also wrong — its 34 bar_duration rows compare croma's free-meter fallback against that fabrication). Mechanism: crates/croma-core/src/lower/mod.rs:1195 meter_duration() returns None for MeterKind::Complex -> meter_model() sets free_meter=true (line 1015) -> write_time_element() early-returns (crates/croma-core/src/musicxml/attributes.rs:43-48). The mid-tune path warns abc.music.meter.unsupported_complex (lower/mod.rs:302-306) but the header path is silent — spec-legal meter in…


*Fix:* Lower + export. `crates/croma-core/src/lower/mod.rs` `meter_duration`: compute the additive sum for `MeterKind::Complex` so measures validate and `free_meter` is false. `crates/croma-core/src/musicxml/attributes.rs` `meter_parts`: emit beats `2+3+2` for grouped complex meter and, for the `a/b+c/d` extension form, emit a composite MusicXML `<time>` with repeated `<beats>/<beat-type>` pairs.

**Phase 37c verification:** `crates/croma-core/src/lower/mod.rs` now computes additive durations for supported complex meters (`M:(2+3+2)/8` -> 7/8, `M:3/4+2/4` -> 5/4 / composite parts) instead of treating them as free meter, and `crates/croma-core/src/musicxml/attributes.rs` now emits `<time>` for both the grouped ABC 2.1 form and the additive extension form. Regression coverage: `complex_header_meter_uses_additive_duration`, `additive_extension_header_meter_uses_summed_duration`, `supported_body_additive_meter_change_does_not_warn`, `complex_header_meter_exports_additive_time`, and `additive_extension_header_meter_exports_composite_time`. Minimal before/after XML probes under `docs/untracked/phase-37-ledger-burndown/probes/` show both repros changed from `NO_TIME` to explicit `<time>` elements (`2+3+2/8` and composite `3/4 + 2/4`). Target corpus compare for `tune_011528.abc`, `tune_003733.abc`, and `tune_003808.abc` remains mismatch-count neutral because the comparator does not score this time-signature surface directly and abc2xml fabricates the additive extension differently. Verification: `cargo fmt --all -- --check`, `cargo test -p croma-core`, `cargo test --workspace`, `cargo run -p croma-cli -- xml examples/basic.abc`, full 10k export + report-only comparison, targeted compare, and ABC round-trip proof. Full 10k aggregate remains at structural matches 8868 / mismatch rows 164215 with no harness/import failures; ABC round-trip remains 99.25 pct in-scope with 0 structural diffs.


### `measure-alignment-multirest-encoding` — **FIXED(35a)** (croma_bug, repro=True)

*Share:* ~2% rows (216 rows, 6 files; MULTIREST_EXPANSION class) — *files:* tune_009310.abc, mod.rs, 03-multi-measure-rest.md


REFINES prior doc 03 ('croma correct, representation choice'). Minimal repro /tmp/ma_multirest.abc with `Z4` in 4/4: abc2xml expands to 4 whole-rest measures (6 total, spec-equivalent per SS4.5 lines 869-872, though it omits <measure-style>); croma emits 3 measures total — the Z4 becomes a single <measure> holding one <rest> of duration 128 (16 quarters inside a 4/4 measure), spelled <type>breve</type> with <time-modification>2/4 — i.e. tuplet semantics abused to express a 4-bar rest. The ABC-side equivalence argument justifies not copying abc2xml's count, but croma's serialization mangles the source structure ('4 measures of rest' becomes 1 overfull pseudo-tuplet measure) and no mainstream MusicXML consumer reads that as a multimeasure rest. The standard encoding (<measure-style><multiple-rest>n</multiple-rest></measure-style> + n measures of <rest measure="yes"/>) would render as the s…


*Fix:* Lower + export. crates/croma-core/src/lower/mod.rs:452-478 (MusicItem::MultiMeasureRest arm) flattens count n into a single duration=meter*n rest event, losing n; keep the count (dedicated event kind or per-measure attribute). crates/croma-core/src/musicxml/note.rs note_spelling fallback (~lines 410-460) is where the breve+2:4 time-modification gets fabricated for the unexpressible duration. The M


### `measure-alignment-voice-scoped-unit-length` — **FIXED(33b)** (croma_bug, repro=True)

*Share:* ~1.6% rows (161 rows, 2 files) — *files:* tune_000346.abc, tune_014637.abc


tune_000346 (V:2 L:1/8, V:3 L:1/16, V:4 no L:): croma carries V:3's 1/16 into V:4 -> bars exactly full (16 sixteenths per C bar); abc2xml resets V:4 to default 1/8 -> every reference bar 2x overfull (8.0 in 4/4). tune_014637 (header L:1/4; L:1/8 after the V:1 declaration; V:2-V:5 no own L:): the polarity flips — abc2xml's per-voice reset to header L:1/4 gives exactly-full bars, croma's carry of 1/8 gives half-full bars (croma 2.0 vs ref 4.0, 124 rows). ABC 2.1 SS7 is VOLATILE and never specifies whether a body L: survives a V: switch; the corpus transcribers themselves assumed both conventions. What's missing: a decision on croma's model — the dominant de-facto convention (abcm2ps/abc2midi: body fields apply to the current voice; new voices inherit header defaults) matches abc2xml and would fix tune_014637 while 'breaking' tune_000346 (whose V:4 is arguably mis-transcribed under either r…


**Verifier correction:** Triage analysis is factually accurate (both corpus cases, the flipped polarity, the SS7 VOLATILE spec silence, and the broadcasting loop in apply_unit_change at crates/croma-core/src/lower/mod.rs:309-314 all check out), but the verdict is too cautious. Corrections: (1) the claimed trade-off is illusory — adopting abc2xml's per-voice scoping makes croma match the reference on BOTH tunes (tune_000346 P4 would become identically overfull at 8.0 quarters, eliminating the mismatch there too), so ther


*Fix:* If per-voice scoping is adopted: crates/croma-core/src/lower/mod.rs apply_unit_change (~line 310) currently does `for voice in &mut self.voices { voice.unit = self.unit; }` — restrict to the current voice and seed new voices from the header unit.


### `measure-alignment-perc-unpitched` — **EQUIV** (legitimate_difference, repro=True)

*Share:* ~0.8% rows (77 rows, 1 file) — *files:* tune_011134.abc


tune_011134 V:3 is `name=drum clef=perc stafflines=4` with K:none and %%MIDI channel 10/drummap lines. abc2xml encodes its notes as <unpitched> (music21 kind 'Unpitched'); croma keeps literal <pitch> letters (kind 'note') — 77 'kind' rows, systematic, not positional cascade (refines the manifest's CASCADE label for this file). ABC 2.1 SS4.6 defines perc only as 'the drum clef' (display); %%MIDI drummap is an abcMIDI extension, not spec. Both serializations are valid MusicXML for the same staff positions: <unpitched> is arguably better engraving practice, but croma's literal encoding loses nothing (the ABC letters ARE pitches syntactically). Comparator could normalize kind note~=Unpitched on percussion-clef staves; mapping perc-clef notes to <unpitched> would be a croma enhancement, not a bug fix.


### `measure-alignment-abc2xml-drops-music-residual` — **QUIRK** (reference_quirk, repro=None)

*Share:* ~1% rows (107 rows, 11 files) — *files:* tune_011865.abc, tune_014784.abc, 11-multipart-and-partgroup.md


Already fully documented in prior docs and re-checked via the manifest join: ABC2XML_DROPS_MUSIC (38 rows, 4 files — abc2xml parse-failed and dropped real music croma kept, doc 02); ABC2XML_DROPS_TACET (43 rows, 2 files — abc2xml drops silent bars from a multi-voice part, breaking SS7 measure alignment across voices, doc 11); BARLINE_STYLE spillover (26 rows, 5 files, doc 04). In each, croma preserves source content the reference loses, or differs in a presentation glyph the spec does not mandate. No fresh counter-evidence found.


### `measure-alignment-broken-rhythm-edge` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~6% rows (~630 rows, ~32 files: DURATION_EXACT_VS_ROUNDED files that do have a header L:) — *files:* tune_001629.abc, tune_002873.abc, tune_004317.abc


(a) Grace group inside the pair, /tmp/ma_gracebr.abc `(f>{gf}e)`: croma f=dotted-eighth, e=sixteenth (bar exactly 4.0 in C|); abc2xml dots the f but FAILS to halve the e (duration 60 = full eighth) -> bar 4.25, overfull. ABC 2.1 SS4.4 defines '>' as 'previous note dotted, next note halved' and SS4.12 grace notes 'nominally have no melodic time value' — the next principal note is the e; croma is spec-correct, abc2xml deviates. ~12 files (tune_001629, tune_004317, tune_005637...) with +0.25/+0.125 reference-side overfull bars. (b) Chained markers on unequal lengths, tune_002873 `e>f<g/`: croma bar 4.25 vs ref 4.0 — SS4.4 lines 861-862: 'broken rhythm markers between notes of unequal lengths will produce undefined results, and should be avoided' -> MALFORMED/UNDEFINED INPUT, both readings permissible, recovery difference only (~5 files). Remaining header-L DURATION files are similar one-off…


### `measure-alignment-default-unit-length` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~12% rows (~1180: 1100 in DURATION_EXACT_VS_ROUNDED no-L files + 80 in CASCADE no-L files; ~25 files) — *files:* tune_005004.abc, tune_005966.abc, tune_006793.abc


Minimal repro /tmp/ma_defaultL.abc: `M:2/4` with no L: field, bar `c6a2|` -> croma: dotted-quarter + eighth (bar total 2.0 quarters = exact 2/4); abc2xml: dotted-half + quarter (bar total 4.0, 2x overfull). ABC 2.1 SS3.1.7: 'If there is no L: field defined ... if it [the meter as a decimal] is less than 0.75 the default unit note length is a sixteenth note' — 2/4 = 0.5 -> L:1/16. abc2xml uses 1/8 regardless, doubling every bar; croma implements the spec. Each affected tune yields ~2 rows per measure (duration + offset cascade). Confirmed on tune_005004 (croma 2.0 vs ref 4.0 every measure), tune_005966, tune_006793 (the CASCADE-classified ones share the identical signature). Matches prior doc 06 / DURATION_EXACT_VS_ROUNDED verdict.


### `measure-alignment-fabricated-default-time-signature` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~4% rows (400 bar_duration rows, 111 files; the dominant cause among single-row 'POSITIONAL_CASCADE' files) — *files:* tune_005414.abc, tune_000659.abc, tune_012281.abc


Two flavors, one mechanism. (a) No M: anywhere (/tmp/ma_nom.abc): croma emits no <time> (ABC 2.1 SS3.1.6: 'When there is no M: field defined, free meter is assumed'); abc2xml emits <time>4/4</time> it invented — music21 then reports bar_duration 4.0 for the reference vs actual-content for croma. (b) M: after K: — K: ends the header (SS2.2.1), so M:6/8 is a body line effective from the first bar (/tmp/ma_mafterk.abc): croma emits only 6/8 in m1; abc2xml emits TWO <attributes> in m1, the fabricated 4/4 followed by the real 6/8, and the comparator picks up the 4/4 for m1 bar_duration (croma 3.0 vs ref 4.0). Six of the eight evidence-pack samples (tune_005414 etc.) are flavor (b). Of 111 bar_duration-row files, 49 are M-after-K and most of the rest have no M: at all. Croma is the more spec-correct on both.


### `measure-alignment-overlay-number-skip` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~5% rows (526 'number' rows, 30 files) — *files:* tune_006706.abc, tune_006707.abc, tune_013470.abc


Minimal repro /tmp/ma_overlay.abc: 4 bars where bar 2 is `C8 & E8` -> croma numbers measures 1,2,3,4; abc2xml numbers 1,2,4,5 (the overlay segment consumed number 3). In tune_006706 the reference numbering runs 1..14,16..27,29,31..80 — the skips (15, 28, 30) land exactly after the three source bars containing `&` overlays; croma is 1..77 sequential. ABC 2.1 SS4.10: `&` repeats 'the previous part of the bar' — an overlay is the SAME bar, so there is no spec basis for incrementing a bar counter. MusicXML's measure number is a display attribute and croma's sequential numbering matches universal practice. Every one of the 30 'number'-row files contains `&` overlays (some phantom-measure files also shift numbers — that part belongs to cause 1). Candidate comparator normalization: compare measures by index, not by the number attribute.


### `measure-alignment-phantom-empty-measure` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~61% rows (5986/9840, 240 files; PHANTOM_MEASURE manifest class) — *files:* tune_014970.abc, tune_001562.abc, 02-phantom-measures.md


Minimal repro /tmp/ma_phantom.abc: body line `"Trio"[K:G]|:` between two 2-bar phrases -> croma emits 4 measures, abc2xml emits 5: its <measure number="3"> contains only <words>Trio</words> + the key change, zero notes; the real Trio music starts in m4. ABC 2.1 SS2.2.1 (music code = notes, bar lines, symbols; an annotation SS4.19 / inline field SS4.16 is not music) and SS4.8 (a bar line marks a boundary, it does not constitute a measure) — nothing creates a measure from an annotation or key change, so croma's folded output is the spec-correct one. Verified live on tune_014970 (`"Coda 1"`, `"Coda 2"` standalone lines): reference 162 measures vs croma 160. The one extra reference measure shifts every later aligned position, producing the offset/event_count/duration/measure_count rows (and the note-vs-chord 'kind' rows, e.g. tune_001562, are the same cascade comparing misaligned events). Ma…


### `measure-alignment-prose-as-music` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~6.5% rows (~637 rows, ~30 files in the CASCADE class) — *files:* tune_009832.abc, tune_009832.xml


MALFORMED INPUT. tune_009832 has `H:Written for ...` followed by a bare continuation line 'on the day the good news reached Frankfurt.' before K: — illegal per ABC 2.1 SS3.3 (multi-line fields require +: continuation). abc2xml parses the prose letters as music: its measure 1 contains 17 fabricated eighth notes (E D A E G D E E A C E D F A F ... — the note-letters of the prose), giving m1 duration 8.5 vs croma 1.0 (the real `ag` pickup), then ~50 cascading offset rows. Croma ignores the dangling line (no notes fabricated). Recovery-choice difference on illegal input, and abc2xml's recovery invents music — croma defensible. 29-30 of the 59 CASCADE-classified files carry a non-field free-text line before K: with this signature (584+ rows). Matches doc 07's 'header/prose mis-segmentation' note.



## missing_in_croma

### `missing-body-tempo-q-field-dropped` — **FIXED(33b)** (croma_bug, repro=True)

*Share:* ~0.25% rows (148), ~120 files — *files:* tune_007548.abc, qb.abc, qc.abc


Minimal: `K:C` then line `Q:1/4=132` then music (/tmp/qb.abc, also mid-body /tmp/qc.abc) — croma emits 0 per-minute/metronome elements and 0 bytes of warnings; abc2xml emits <metronome><beat-unit>quarter</beat-unit><per-minute>132</per-minute> + <sound tempo>. Header placement works (/tmp/qa.abc → 1). Corpus exemplar tune_007548 has `Q: 1/4=132` on the line after K: — reference has the tempo direction, croma nothing (the sampled "MetronomeMark text=allegro" is music21's auto-name for 132 BPM). ABC 2.1 §3.1.8 and the §3 field table explicitly allow Q: in the tune body. Silent loss of source information. Drives the 148 MetronomeMark direction rows.


*Fix:* No mid-tune tempo concept exists: metadata holds a single header `tempo: Option<TextLine>` (crates/croma-core/src/lower/mod.rs:819, parse_tempo_model at mod.rs:982). Mirror the PR #70 mid-tune K:/M: event design: lower body Q: fields into a tempo-change event on the timeline (crates/croma-core/src/lower/), then emit a <direction><metronome>+<sound tempo> at that offset in crates/croma-core/src/mus


### `missing-chord-member-slurs-dropped` — **FIXED(HEAD 858b53e)** (croma_bug, repro=True)

*Share:* ~0.1% rows (subset of 108 slur rows, 23 files) — *files:* tune_011867.abc, slur2.abc


Minimal: `[(C(E] [C)D)]|` (/tmp/slur2.abc) — croma emits 0 slur elements with warning abc.music.unknown_chord_token; abc2xml emits 4 (two per-member slurs), matching the sampled tune_011867 rows (start "C4.E4" end "C4.D4" etc.). ABC 2.1 §4.11 lines 1007–1008: "Both ties and slurs may be used into, out of and between chords" — the phrasing information is real even though the inside-bracket per-member syntax is a de-facto extension. Simple into/out-of-chord slurs `(D[BD])` work in croma (/tmp/slur1.abc, both sides 2). Small but croma loses notated phrasing; at least it warns. Prior doc 09 did not cover this shape.


*Fix:* Parse layer: chord-member tokenizer in crates/croma-core/src/parse/note.rs — accept '(' / ')' on chord members and lower them as slur start/stop attachments on the chord (or per member), reusing the existing slur attachment path.


### `missing-multimeasure-rest-expansion` — **FIXED(35a)** (croma_bug, repro=True)

*Share:* ~2.8% rows (1,700) — *files:* tune_010751.abc, mr1.abc


Minimal: `CDEF|Z3|GABc|` (/tmp/mr1.abc) — croma 3 measures, abc2xml 5. ABC 2.1 §4.5 (lines 866–873) declares the collapsed and expanded forms "musically equivalent (although they are typeset differently)". The 1,700 MULTIREST_EXPANSION-verdict missing rows are reference rest/note facts in the expansion measures plus the local realignment. Prior doc 03 verified. Candidate for COMPARATOR normalization (expand croma's multi-bar rest, or collapse reference's run of whole-bar rests, before aligning); do not change croma.


**Verifier correction:** The factual root cause is correct, but the legitimate_difference verdict (and doc 03's "not a Croma bug" conclusion) conflates ABC-level equivalence with MusicXML-level validity. ABC 2.1 §4.5 does declare Zn and n single-bar rests musically equivalent, but that licenses either ABC input form — it says nothing about how MusicXML must encode the result. MusicXML's only encoding for an n-bar rest is n real measures (whole-measure rests), with <measure-style><multiple-rest>n</multiple-rest> as the o


### `missing-nospace-key-global-accidentals-misparse` — **FIXED(37d)** (croma_bug, repro=True)

*Share:* <0.1% of this category's rows (cross-category: ~42 accidental/pitch rows in tune_003837 alone) — *files:* tune_003837.abc, key7.abc, key3.abc


Minimal: header `K:D_B^g` (/tmp/key7.abc) → croma emits <fifths>0</fifths> with NO diagnostic (D major is fifths 2); mid-line `[K:D_B^g]` (/tmp/key3.abc) → key change silently not applied at all, while `[K:D _B ^g]` with spaces (/tmp/key5.abc) and `[K:D]` work. Consequence in tune_003837: 42 of 380 notes sound F-natural in croma where the reference has F# (regex pitch-sequence diff at indices 228+), directly contradicting the refreshed manifest's justification "Same 384 notes" — the prior PHANTOM_MEASURE verdict is right about the measures but missed this sounding difference. ABC 2.1 §3.1.14ff (lines 661–669) documents accidentals "separated by spaces", so the no-space form is nonstandard input — but croma's silent fallback to fifths 0 mangles the music without any warning, vs abc2xml's obvious-intent reading (D + _B + ^g). Corpus footprint: 4 inline occurrences ([K:D_B^g], [K:D^f_B_e], …


*Fix:* Parse layer: crates/croma-core/src/parse/field/key.rs — tokenize trailing global accidentals without requiring whitespace separators (each begins with __,_,=,^,^^ + note letter), or at minimum emit a warning + ignore only the accidental tail instead of silently degrading the whole key to fifths 0.

**Phase 37d verification:** Implemented the conservative fallback: compact malformed key tokens such as `K:D_B^g` and `[K:D^f_B_e]` are split far enough to preserve the valid base tonic/mode (`D` -> fifths 2), emit `abc.field.key.compact_accidentals_ignored`, and ignore the nonstandard no-space accidental tail instead of degrading the entire key to fifths 0. A first attempt that applied the compact tail as real global accidentals fixed some F/C key-signature rows but added more B/G/E accidental mismatches; it was rejected by targeted and full-corpus evidence. Regression coverage: `parses_nospace_key_global_accidentals_after_tonic`, `parses_nospace_key_global_accidentals_with_sharp_first_as_base_key`, `nospace_header_key_global_accidentals_preserve_base_key`, `nospace_inline_key_global_accidentals_preserve_base_key`, and `nospace_key_global_accidentals_export_base_key`. Targeted compare for `tune_003837.abc` + `tune_004610.abc`: structural matches 0 -> 1, mismatch rows 3088 -> 3086, accidental rows 69 -> 67. Full 10k phase-36 -> phase-37d aggregate: structural matches 8861 -> 8869, mismatch rows 164265 -> 164188, accidental rows 3883 -> 3856, with no harness/import failures. Verification: `cargo fmt --all -- --check`, `cargo test -p croma-core nospace -- --nocapture`, `cargo test -p croma-core`, `cargo test --workspace`, `cargo run -p croma-cli -- xml examples/basic.abc`, `uv run pytest`, full 10k export + report-only comparison, targeted compare, and ABC round-trip proof (99.25 pct in-scope, 0 structural diffs).


### `missing-pending-text-dynamics-before-barline-or-eol` — **FIXED(33a)** (croma_bug, repro=True)

*Share:* ~0.9% rows (~550), ~250 files — *files:* tune_006277.abc, tune_001562.abc, tune_004792.abc, dyn1.abc


Three fresh minimal repros: (1) `"Trio"[K:G]|:` boundary — croma keeps the key change but emits 0 <words> for Trio, abc2xml keeps it (/tmp/phantom1.abc); (2) standalone annotation line `"^A"\` (and `"^A"` without backslash) before music — croma 0 <words>, abc2xml 1 (/tmp/ann1.abc, /tmp/ann4.abc; with the annotation glued to the note, `"^A"CDEF`, croma keeps it — /tmp/ann3.abc); (3) `CDEF !f!|` newline `GABc|` — croma 0 dynamics, abc2xml 2 (/tmp/dyn1.abc; corpus exemplars tune_004792–96 use `!p!\` / `!f!|\`). All drops emit zero diagnostics. ABC 2.1 §4.19 (annotations apply to the following note/bar line) and §4.14 (decorations precede the decorated symbol); the following note exists across the barline/line break, so this is silent data loss. ALREADY CATALOGED as docs/parser-backlog.md item 4 for the chord-symbol-before-barline case (`parse_barline` flushes pendings that lowering's catch-…


*Fix:* Parse layer per backlog item 4: crates/croma-core/src/parse/barline.rs flushes pending quoted-text/decorations into standalone items that crates/croma-core/src/lower discards at the catch-all arm — carry pendings across barlines and line ends to the next note (the grace/slur/tuplet pending-stash fix from backlog item 9 is the template).


### `missing-quoted-text-harmony-vs-words-classification` — **FIXED(33b)** (croma_bug, repro=True)

*Share:* ~0.2% rows (subset of TextExpression misses; 40 files) — *files:* tune_000534.abc, tune_000456.abc


tune_000534 `"d"bc |]`: croma emits <harmony><root-step>D</root-step><kind text="d">major, reference emits <words>d</words>; all other quoted texts in the file (a., b., c., d.) match as <words> on both sides. ABC 2.1 §4.18/§4.19: a quoted string not starting with ^_<>@ is a chord symbol, but "d" is not conventional chord syntax, so recovery is package-specific; neither side loses the text. 40 of the 258 TextExpression-miss files also carry extra_in_croma harmony rows — the same string classified differently. Candidate for comparator normalization (treat croma harmony whose kind text equals the reference TextExpression text as a match); never change croma to mimic abc2xml.


**Verifier correction:** The triage agent's facts reproduce but the verdict is mislabeled. (1) ABC 2.1 §4.18 defines the chord root as A-G and grants case-insensitivity ONLY to the bass note ("can be any letter (A-G or a-g)"), so "d" is not valid chord syntax and abc2xml's words fallback is spec-faithful, not a quirk or package-specific recovery. (2) Croma's own doc comments in harmony.rs claim parse_chord_symbol "exactly mirror[s] abc2xml's pyparsing behaviour" and document parse_chord_tone as [A-G][#b]? — the to_ascii


### `missing-volta-ending-stop-never-emitted` — **FIXED(33a)** (croma_bug, repro=True)

*Share:* ~3.5% rows (2,175) but 905 files — the single biggest croma-fixable file count in this category — *files:* tune_009505.abc, tune_011829.abc, tune_000461.abc, volta2.abc


Minimal: `CDEF|1 GABc | cdef :|2 cBAG | fedc ||` (/tmp/volta2.abc). croma emits <ending number="1" type="start"/> and <ending number="2" type="start"/> but NO stop for either; the 1-bar variant (/tmp/volta1.abc) closes fine. ABC 2.1 §4.9/§4.10 (lines 971–981): "The Nth ending starts with [N and ends with one of ||, :| |] or [|" — endings legally span multiple bars. An <ending start> with no stop is malformed MusicXML (the bracket never closes), so music21 builds no RepeatBracket on the croma side: 2,175 missing repeat_ending rows and ZERO extra_in_croma repeat_ending rows corpus-wide. Spot-check of 8 random affected files: croma ending stops always < reference stops (e.g. tune_008287 croma 0 stops vs ref 2/2; tune_003325 0 vs 4/4). Root in code: crates/croma-core/src/musicxml/score.rs write_part derives `endings` from the CURRENT measure's `measure.repeat_endings` and emits Stop only if …


*Fix:* Export layer: crates/croma-core/src/musicxml/score.rs write_part — carry an open-ending state across measures (like the existing pending_left_repeat) and emit <ending type="stop"> at the first barline matching stops_repeat_ending_barline (or type="discontinue"/forced stop at next ending start or part end). Also reorder ending before repeat in crates/croma-core/src/musicxml/barline.rs write_barline


### `missing-dangling-tie-slur-fabrication` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~0.05% rows (remainder of the 108 slur rows) — *files:* tune_011035.abc, tune_009227.abc


Malformed input. tune_011035 has no '(' slurs in the body, yet the reference contains a D4→F#4 slur: the source `...D4-:|2[M:3/2]FDEC...` carries a tie into a different-pitch note across the volta. ABC 2.1 §4.11 (lines 992–1006): ties connect successive notes OF THE SAME PITCH; this `-` is not a legal tie, so abc2xml's slur is fabricated structure. Croma drops it (per backlog item 10's pending-tie machinery, which also undoes the accidental carry — the spec-correct part). tune_009227 (G#5→F#5) is the same shape. Matches prior doc 09 verdict (defensible recovery of non-tie input); note: malformed input.


### `missing-phantom-empty-measure-cascade` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~84% rows (52,086/61,703; 240 files plus most of the 486 single-category POSITIONAL_CASCADE files) — *files:* tune_003837.abc, tune_000289.abc, tune_001562.abc, phantom1.abc


Minimal: body `CDEF|GABc|` + line `"Trio"[K:G]|:` + `GABc|cBAG:|`. croma emits 4 measures; abc2xml emits 5 — its measure 3 has ZERO notes, only <words>Trio</words> and the key change (verified /tmp/phantom1.abc, both tools). ABC 2.1 §2.2.1/§4.8: a bar line or annotation is not a measure of music, so croma's folded layout is spec-correct. Mechanics of the row mass: each measure-shift misaligns every later reference event; reference events with no aligned croma slot emit a ~10-row fact bundle (pitch step/alter/octave + note kind/voice + duration ×3 + tie + tuplet), mirrored by extra_in_croma for the same notes (tune_003837: 1235 missing vs 1106 extra). Attribution via per-file-manifest-refreshed.csv joined to full-10k-mismatches.parquet: PHANTOM_MEASURE-verdict files carry 52,086 of 61,703 missing rows. Pitch-sequence extraction on tune_003837 confirms both sides contain the same 380 notes…


### `missing-structural-cascade-misc` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~8.5% rows (~5,250) — *files:* tune_011865.abc, tune_014784.abc, tune_011263.abc


CASCADE-verdict files contribute 4,623 missing rows, ABC2XML_DROPS_TACET 435, ABC2XML_DROPS_MUSIC 125, DURATION_EXACT_VS_ROUNDED 17; component mix is the same note-fact bundles (pitch 1,359 / duration 1,353 / note 906 / tie+tuplet 902) as the phantom cascade, i.e. misalignment, not absent music. Prior docs 02/07/11 verified by the prover's pitch-sequence identity: when abc2xml drops a line or tacet bars, croma has MORE music (ABC 2.1 §4.8 voice/measure alignment) and the within-file realignment produces reference-side orphans. The 392 metadata/voice rows ({"event_count":N,"id":null}) sit in the same files (e.g. tune_000289, the doc-02 Trio exemplar) — phantom/extra reference voice blocks from the same artifact family. The 132 lyric, 128 harmony and 160 pitch-count rows are the doc-07 cascade slice (0 single-category files).



## octave

### `octave-midtune-clef-octave-shift-dropped` — **OPEN** (croma_bug, repro=None)

*Share:* ~0.15% rows (8 rows, 1 file) — but the only genuine sounding-octave error in the category — *files:* tune_006650.abc


Minimal repro /tmp/octave-triage/clef_midtune.abc: `G,CEG|[K:treble-8]G,CEG|[K:treble+8]G,CEG|` — croma emits NO mid-tune <clef> and unshifted pitches (G3 C4 E4 G4 in every measure); abc2xml emits <clef> with <clef-octave-change>-1/+1 and shifts sounding pitch (m2: G2 C3 E3 G3, m3: G4 C5 E5 G5). ABC 2.1 clef spec (line 888): '+8|-8 ... The player will transpose the notes one octave higher or lower', and K: fields are legal in the tune body — so the shift MUST apply. Croma's own header path agrees (`K:C treble-8` correctly emits clef-octave-change -1 and G2 C3 E3 G3), proving only the mid-tune path is broken. Probes show the guard is the blocker: `[K:treble-8]`, `[K:clef=treble-8]`, `[K:octave=-1]`, and even whole-line body `K:clef=bass` are ALL rejected (warning[abc.field.invalid_k] or silently) because key_is_invalid_for_lowering (crates/croma-core/src/lower/mod.rs:1076) treats any toni…


*Fix:* Lower layer: reorder guards in crates/croma-core/src/lower/mod.rs — apply_key_change (lines 316-330) and apply_inline_field 'K' arm (lines 408-420) must route K: values whose properties parse to recognized clef/octave/middle/transpose settings to the clef-only merge (apply_inline_key_clef_properties / merge_voice_properties) instead of letting key_is_invalid_for_lowering (line 1076) reject them; o


### `octave-staves-order-ignored` — **OPEN** (croma_bug, repro=None)

*Share:* ~0.2% rows (1 file; octave subset of its 416 rows) — *files:* tune_013106.abc


tune_013106 declares voices in body order V:1, V:3, V:2 with explicit `%%staves [1 2 3]`. Croma emits parts in body order (P1=V1, P2=V3 first notes G4 E4 E4..., P3=V2 G4 G4 G4...); abc2xml emits directive order (P2=V2, P3=V3) — verified by per-part first-note extraction. The comparator pairs P2-with-P2, generating pitch/octave/duration rows for the two swapped voices (6 step-equal octave diffs of 66 stream diffs; the file's only structural difference is this permutation — corpus-wide scan found exactly 1 such file among the 247). Root: deliberate fallback in crates/croma-core/src/lower/mod.rs part_voice_groups lines 890-895 ('Only honour the directive ordering/grouping when it actually merges voices; otherwise keep the simple one-part-per-voice order'). ABC 2.1 voice-grouping section (spec lines 1981-2011) defines `%%score <voice-id1> ... <voice-idn>` (and the equivalent %%staves) as spe…


*Fix:* crates/croma-core/src/lower/mod.rs part_voice_groups (lines 837-897): drop or narrow the merge-only fallback at lines 890-895 — when the directive mentions all voices, honor its ordering even without parenthesis groups (unmentioned voices appended after, as today). Single-function change in the lower layer; build_score_model (line 899) consumes the groups unchanged.


### `octave-croma-unclosed-bracket-drops-notes` — **QUIRK** (reference_quirk, repro=None)

*Share:* ~2% rows (5/247 files) — *files:* tune_005224.abc, tune_013144.abc, tune_012835.abc


MALFORMED INPUT (unclosed chord bracket, illegal per ABC 2.1 §4.17), but croma is the lossier side. Minimal repro /tmp/octave-triage/bracket_annot.abc: `ABcd efga |["x" "Am7"ABcd efga | B8 |` — croma emits warning[abc.music.unclosed_chord] 'Chord group was preserved and skipped' and drops ALL 8 notes to the next barline; abc2xml ignores the stray `[` and keeps the measure. Corpus: tune_005224 (`["cont"`, `["Trans"`) loses 16 notes (croma 117 vs ref 133 pitched notes — the manifest justification 'Same 133 notes... phantom EMPTY measure' is factually WRONG for this file and tune_013144); tune_012835/tune_009178/tune_012962 have `][` mid-line (e.g. `E2 ][ f |`) where croma drops the single following note. Per task rule this recovery-choice difference on illegal input is classed reference_quirk, but croma's skip-to-barline recovery is a real improvement opportunity: recovering the notes afte…


### `octave-documented-structural-residuals` — **QUIRK** (reference_quirk, repro=None)

*Share:* ~3% rows (7/247 files) — *files:* tune_008657.abc, tune_006695.abc


Remaining 7 files re-verified against the refreshed manifest: 4 MULTIREST_EXPANSION files differ only in rest counts (pitched-note streams identical — doc 03's Z-expansion divergence), and 2 ABC2XML_DROPS_MUSIC + 1 ABC2XML_DROPS_TACET files have MORE pitched notes in croma because abc2xml parse-failed and lost real music (doc 02; croma correct). Their octave rows are the same positional-cascade artifact as cause 1, riding on rest-count or note-loss offsets. One 2-note unexplained insert remains in tune_006695 (near `!segno![|` constructs) — negligible.


### `octave-escaped-quote-annotation-as-music` — **QUIRK** (reference_quirk, repro=None)

*Share:* ~0.1% rows (2 files) — *files:* tune_004433.abc, tune_006529.abc


tune_004433 contains `"^compare \"Begone dull care\" "c BAB`: abc2xml ends the annotation at `\"` and parses 'Begone dull care' as music — its 8 inserted notes B4 E5 G5 E5 D5 C5 A5 E5 spell exactly the note letters B,e,g,e,d,c,a,e of that title. tune_006529's `"^\"Alla kullor uti Västanå...\""` likewise yields fabricated A4 A5 A5 (letters A,a,a). Croma keeps the annotation as one string. ABC 2.1 'Special characters' (spec ~line 1671): a double quote inside an annotation is written `&quot;` or `"`, and `\"<letter>` is defined as an umlaut diacritic in text strings — under either reading the content stays TEXT, so abc2xml's note fabrication is indefensible and croma is spec-correct.


### `octave-header-prose-parsed-as-music` — **QUIRK** (reference_quirk, repro=None)

*Share:* ~30% rows (89/247 files) — *files:* tune_007395.abc, tune_008774.abc, tune_005569.abc, tune_009970.abc


MALFORMED INPUT. 89 of 247 octave files contain a non-field line inside the tune header (a field value wrapped onto a second physical line without `+:` continuation, or a pseudo-field like `notC:`). abc2xml parses those lines as music, fabricating notes from any a-g/A-G letters (and `x` as invisible rest) and prepending them to the first measure(s); croma keeps them as header text. Verified decodings: tune_007395 line `9.abc` (wrapped F: URL) -> reference measure 1 starts A5 B5 C5 before croma's real pickup A4; tune_008774 line `notC:Prefixed as 'No.9' in MS` -> reference prepends exactly C4 E5 F5 rest(x) E5 D5 A5; tune_005569 line `Go figure!` (H: continuation) -> reference prepends G4 F5 G5 E5; tune_009970 (5 wrapped H: prose lines) -> reference has 70 fabricated notes (201 vs 131). The insertion shifts positional alignment for the whole voice; octave rows are the step-equal slice of t…


### `octave-phantom-measure-numbering-shift` — **QUIRK** (reference_quirk, repro=None)

*Share:* ~60-65% rows (143/247 files, including the largest mismatch files) — *files:* tune_003837.abc, tune_006650.abc


143 of 247 octave files have IDENTICAL step+octave note streams (verified by index-aligned extraction over all 247 files); their octave rows exist only because abc2xml inserts empty measures at annotation/section boundaries, shifting measure numbers so the comparator pairs croma measure N with different music in reference measure N. Minimal repro /tmp/octave-triage/phantom.abc (two sections of `"A"\` annotation + `|:CDEF GABc:|`): croma emits 2 measures, abc2xml emits 4 — one phantom EMPTY measure per standalone-annotation-before-`|:` line. Corpus witness tune_003837: exactly 8 section labels "A"–"H" written as `"A"\` lines, reference has 8 extra measures (78 vs 70) while the 384-note streams are identical (only alter diffs). ABC 2.1 §2.2.1/§4.8: an annotation or bar line is not a measure of music — croma's folded layout is spec-correct. Confirms prior docs 02/07.



## pitch

### `pitch-multirest-expansion-shift` — **FIXED(35a)** (croma_bug, repro=True)

*Share:* ~0.7% rows (89 rows, 4 files) — *files:* tune_009310.abc, tune_013477.abc, tune_005687.abc, tune_013455.abc


tune_009310 contains two `Z2 [|]` multi-measure rests; croma emits 54 measures, reference 56 — abc2xml materializes each Zn as n literal empty measures while croma uses the compressed form. Both are spec-valid encodings of n measures of rest (ABC 2.1 §4.9 'Zn is a shorthand for n measures of rest'; MusicXML supports both <multiple-rest> and literal measures). The 2-measure offset shifts measure-indexed alignment, yielding the pitch rows. Already adjudicated in prior doc 03 (justify, don't fix); comparator-side normalization (expanding multiple-rests before alignment) would eliminate these rows.


**Verifier correction:** The mechanism (Zn measure-count shift causing alignment-artifact pitch rows) reproduces exactly, and comparator-side multirest expansion before alignment would indeed eliminate the rows. But the verdict 'legitimate_difference' rests on a false premise: croma does not emit a spec-valid compressed representation. MusicXML's <multiple-rest> is a measure-style display directive that still requires the n measures to be written out individually; croma emits neither that nor literal measures, but a sin


### `pitch-trailing-grace-group-dropped` — **FIXED(37e)** (croma_bug, repro=True)

*Share:* ~1% rows (<=211 rows, 1 corpus file; bounded above because tune_006695 also carries a phantom measure) — *files:* tune_006695.abc


Minimal repro /tmp/trailgrace.abc: `Te6{de}|d2f f2f|`. abc2xml emits the principal E plus two `<grace/>` notes D,E after it; croma emits only the E — exit 0, zero bytes on stderr (verified). Corpus: tune_006695 (`Te6{de}` trill with termination) — reference pitch stream has (E5,D5) at index 268 that croma lacks. ABC 2.1 §4.12 allows grace groups and §4.20 orders them before a note; a group before a barline is spec-unspecified, but the notation is real musical content (a standard trill termination) and croma's own code documents the drop as deliberate: crates/croma-core/src/lower/voice.rs lines ~88-94 ('Dropped at hard boundaries (barline, voice switch, end of tune) when no timed note follows') and crates/croma-core/src/lower/mod.rs ~512-514 clears pending_grace_groups at barlines. This is silent source-information loss per the task's croma_bug definition. Related to parser-backlog item 4…


*Fix:* Phase 37e adds an `after_grace_groups` attachment slot and resolves a pending standalone grace group backward only for a standalone trill-decorated note immediately before a hard boundary. MusicXML now writes those groups after the owning note, and ABC dump preserves them as an event suffix. The target repro `Te6{de}|d2f f2f|` now exports the principal E followed by D/E grace notes in measure 1, matching abc2xml ordering; ordinary `{g}|d` cross-bar leading-grace behavior remains unchanged. A guard keeps chord-member trills from draining graces into member attachments that exporters do not emit. Evidence: focused lower/MusicXML/to_abc grace tests pass; target `tune_006695.abc` exports nine `<grace/>` notes, matching the reference count and placing the new D/E pair after the trill E; full 10k compare vs phase 37d improves structural matches 8869->8870 and mismatch rows 164188->164052 (`missing_in_croma -64`, `extra_in_croma -64`, `duration -3`, `measure_alignment -4`, `voice -4`, with small positional `accidental +2`/`pitch +1` in the same target file), with no harness or MusicXML import failures.


### `pitch-unclosed-chord-recovery-drops-notes` — **FIXED(37f)** (croma_bug, repro=True)

*Share:* ~0.3% rows (37 rows, 2 files) — *files:* tune_009179.abc, tune_005224.abc


Malformed input. tune_009179 `e2 E E2 ][ f |`: minimal repro /tmp/bracketbar.abc shows croma warns `abc.music.barline.liberal` for `]` then `abc.music.unclosed_chord: Chord group was preserved and skipped` (crates/croma-core/src/parse/note.rs:250) and emits no F — the pickup note is gone from the XML; abc2xml keeps F as its own pickup measure, so the reference has one extra F#5 at stream index 41 and 32 pitch rows cascade. tune_005224 `|["cont" "Am7"FGAB cAFA |` and `|["Trans"...`: the unterminated `[` swallows the whole 8-note run twice (croma 115 vs ref 131 pitches). An unclosed chord (§4.17 requires `[...]`) and `][` (not among §4.8 barline tokens) are illegal ABC, so this is a recovery-choice difference on malformed input, not a spec violation — but note croma's recovery is the lossier one (it does warn, unlike the grace case). If robustness is ever wanted: the recovery in crates/cro…


**Verifier correction:** The factual claim is fully accurate (mechanism, code location note.rs:244-252, both corpus examples, exact pitch counts 83v84 and 115v131, cascade shape). Only the verdict label is wrong. Triage's spec basis — "][ not among §4.8 barline tokens, so illegal ABC" — misses §4.8's liberal-barline paragraph (abc_standard_v2.1.full.md line 961): "bar lines may have any shape, using a sequence of | (thin), [ or ] (thick), and : (dots), e.g. |[| or [|:::" and "Abc parsers should be quite liberal in recog

*Fix:* Phase 37f keeps the existing `abc.music.unclosed_chord` diagnostic but recovers parsed members as ordinary notes when a top-level unclosed `[` is stopped by a barline, leaving that barline to be parsed by the main music loop. The target `e2 E E2 ][ f |` now exports the lost F pickup, and the quoted bracket runs in `tune_005224.abc` now export their eight note sequences instead of swallowing them. Recovery is gated to the top-level music-line chord path; grace-internal malformed chords keep the old no-leak behavior (`{[CDE | G} A|` only yields mainline A). Evidence: `cargo test -p croma-core unclosed_chord -- --nocapture` passes 8 focused tests, target `tune_009179.abc`/`tune_005224.abc` exports both succeed and compare to 1 structural match plus 5 residual rows in `tune_005224.abc` (separate text/barline artifacts), and full 10k compare vs phase 37e improves structural matches 8870->8873 and mismatch rows 164052->162483 (`missing_in_croma -859`, `extra_in_croma -312`, `pitch -95`, `duration -91`, `lyric -63`, plus smaller category improvements), with no harness or MusicXML import failures.


### `pitch-staves-voice-order` — **EQUIV** (legitimate_difference, repro=True)

*Share:* ~0.4% rows (54 rows, 1 file) — *files:* tune_013106.abc


tune_013106 declares voices in source order V:1, V:3, V:2 under `%%staves [1 2 3]`. Both engines emit parts P1,P2,P3 (verified via grep of both XMLs), but croma's P2 holds the second-DECLARED voice (V:3) while abc2xml's P2 holds V:2 per the %%staves layout. The pitch sequences are identical as multisets — the diff is one 32-note block transposed (croma c[38:70] == ref r[74:106]). %%staves is an abcm2ps/abc2xml typesetting directive, not ABC 2.1 core (§11 stylesheet directives are optional); both part orders are valid MusicXML encodings of the same score. Croma could optionally honor %%staves ordering for layout fidelity (musicxml/score.rs part assembly), or the comparator could match parts by voice id instead of position.


### `pitch-abc2xml-swallows-line-after-legacy-linebreak` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~0.5% rows (62 rows, 2 files) — *files:* tune_008657.abc, tune_011364.abc


tune_008657 line 15 ends `...D3/2D|!\` (legacy `!` hard line break, ABC 2.1 §10 outdated syntax, plus `\` continuation). abc2xml's stderr shows it consumed the ENTIRE next line as a decoration: `unhandled note decorations: ['DFAddf|dFGA2A/A/|f>edc2B/c/|d<dAFD"*"f/2|']` — a whole measure-line of real music vanishes from the reference (croma 21 measures vs ref 17). Croma keeps the music; the comparator then pairs croma's extra notes against later reference notes, producing the 42 pitch rows (croma-extra deletes in the sequence diff). Same for tune_011364 (20 rows). abc2xml loses source music — croma is strictly more correct. Matches prior verdict ABC2XML_DROPS_MUSIC (doc 02 family).


### `pitch-phantom-measure-misalignment` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~59% rows (7,521/12,745; 146+4 files with identical or alter-only-differing note sequences) — *files:* tune_014970.abc, tune_003837.abc, tune_012496.abc


Minimal repro /tmp/phantom.abc: `CDE|FGA|` then a standalone annotation line `"Coda"` then `|cde|`. abc2xml emits 4 measures — measure 3 contains only `<words>Coda</words>` and zero notes; croma emits 3 measures. The comparator aligns per measure index, so after the insertion croma m3 (c,d,e) is compared against ref m3 (empty) / ref m4, producing step rows for every subsequent measure whose notes differ from its neighbor's. Corpus confirmation: tune_014970 (160 vs 162 measures, standalone `"Coda 1"`/`"Coda 2"` lines) has its 226 pitch rows starting exactly at measure 65 (the first boundary) and running to 160; tune_003837 (70 vs 78, part labels `"A"\`..`"H"\`) same pattern. Independent re-extraction of the full (step,alter,octave) sequence from both XMLs shows the sequences are IDENTICAL for 146 of the 250 pitch files (7,016 rows in PHANTOM_MEASURE files + 505 rows in files whose only se…


### `pitch-prose-fabricated-notes` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~36% rows (4,597/12,745; 90 files) — *files:* tune_006279.abc, tune_013237.abc, tune_008774.abc, tune_004433.abc


Malformed input. tune_006279 has a bare prose continuation of N: (`for this tune. Feel free to try your own!`) inside the header; abc2xml stderr shows `unhandled note decorations: ['o','r','t','h','i','s','t','n']` and its measure 1 opens with fabricated F5/E5/F4 notes from the letters f/e, while it also demotes the following M:/Q:/K: lines to body fields (emits fifths=0, 4/4 instead of K:D, 6/8). Croma keeps the header intact and starts at the real pickup G — hence the pack row 'croma G vs ref F at m1'. Same mechanism: tune_013237 (`notC:"Ret(r)eat."` -> abc2xml fabricates a lone C4 measure; croma m1=A,F,D), tune_008774/tune_006702/tune_006730 (`notC:Prefixed as 'No.9'...` -> fabricated C,E,F...), tune_008060/tune_007591/tune_004321 (Z:-field prose wrap `BRENNAN July 2003 (HTTP://...)` -> fabricated B,E,A), tune_013317 (wrapped F: URL line `0.abc` -> fabricated a,b notes, the pack's 'B …



## slur

### `slur-chord-member-slurs-dropped` — **FIXED(HEAD 858b53e)** (croma_bug, repro=True)

*Share:* ~32% rows (41/129: 011938=26, 011866=11, 004626=4) — *files:* tune_011938.abc, tune_011866.abc, tune_004626.abc


Minimal: `[(C2(E2] [C)E)] z2|]` -> croma emits ZERO <slur> elements and warns `abc.music.unknown_chord_token` 4x; abc2xml emits both member slurs (start num=1 on C4, start num=2 on E4, stop num=2 on C4, stop num=1 on E4). ABC 2.1 §4.11 (lines 1007-1008): 'Both ties and slurs may be used into, out of and between chords'. Corpus: tune_011938 V:P2 m14 `[(C2(E2] [C/)E/)]` (croma 51 slurs vs ref 55), tune_011866 (croma 28 vs ref 74), tune_004626 (croma 4 vs ref 36 — nearly every slur in the tune is chord-member style, e.g. `[(B,3/4(G3/4]D/4)`). The dropped slurs shift the positional slur-fact index, cascading every later slur row in those files; they also generate most of the 108 missing_in_croma slur-component rows (cross-category).


*Fix:* Parse layer: crates/croma-core/src/parse/note.rs ~line 230-240 (the `abc.music.unknown_chord_token` branch) skips `(`/`)` inside chord brackets instead of producing slur tokens. Accept member-level slur open/close inside `[...]` and route them through crates/croma-core/src/lower/voice.rs `apply_slur` (line 424), attaching to the chord event (hoisting to chord-level start/stop is sufficient for Mus


### `slur-grace-group-slurs-dropped` — **FIXED(37g)** (croma_bug, repro=True)

*Share:* ~1.5% rows (2/129) — *files:* tune_006367.abc


Minimal: `{(fg)}a2 {(ef)}g2|]` -> croma emits no slurs and warns `abc.music.unknown_grace_token`; abc2xml emits F5(grace) start -> G5(grace) stop etc. Corpus tune_006367 `{(fg)}a2 d2{(fg)}a2d2` / `{(ef)}g2e2`: croma 2 slurs vs ref 8; the 2 both-present rows are the resulting index shift (croma (D4,E4) aligned against ref (F5,G5)). The 2.1 spec grammar for §4.12 grace notes does not define slur tokens inside braces, so croma's warn+skip is not a spec violation — but the construct is a widespread de-facto convention (abcm2ps/abc2xml both engrave it) and croma's output loses real notation. Adjacent to parser-backlog item 2 (bare-grace slurs `({Bc})`, gated by _BARE_GRACE_SLUR_RE), but this inside-braces form `{(..)}` is a distinct, uncataloged variant.


*Fix:* Parse layer: crates/croma-core/src/parse/note.rs ~line 374 (`abc.music.unknown_grace_token` branch) — tokenize `(`/`)` inside grace braces as slur markers scoped to the grace group; lower via the existing grace slur path (crates/croma-core/src/musicxml/grace.rs handles grace slur emission already for `({g}a)` shapes).

*Fixed in 37g:* Parser now tokenizes `(`/`)` inside grace groups as
grace-scoped slurs instead of `abc.music.unknown_grace_token`; lowering pairs
them onto grace note/chord events, diagnoses malformed grace-internal slurs with
the existing `abc.music.unclosed_slur` / `abc.music.unmatched_slur` policy, and
the MusicXML/ABC writers preserve the scoped slur markers without leaking them
to the main note. Target `tune_006367.abc` now compares as 1 structural match
with 0 mismatch rows and no harness/import failures. Full 10k report-only
compare after 37g: structural matches 8873->8874 vs 37f, mismatch rows
162483->162475, `missing_in_croma` 55766->55760, `slur` 104->102; no import,
harness, or worker failures.


### `slur-number-collision-shared-part` — **FIXED(37h)** (croma_bug, repro=True)

*Share:* ~2% rows (3/129: 010662=2, 013200=1 — 013200's count=3 row shows abc2xml's own number=1 reuse colliding too) — *files:* tune_010662.abc, tune_013200.abc


Minimal: two voices on one staff (`%%staves (1 2)`), `V:1 (e2 e|e) z z` / `V:2 (c2 c|c) z z` -> croma emits both overlapping slurs as number=1 (start1 E5, start1 C5, stop1 E5, stop1 C5); abc2xml gives the second voice number=2. With the same number, MusicXML readers cannot pair them: music21 merges croma's into one 3-element spanner (A4->G4 count=3) plus an orphan 1-element spanner — exactly the corpus rows for tune_010662 m16-17 (`(A3|G)` in V:1 simultaneous with `(D3|G)` in V:2; croma {count:3,A4->G4}+{count:1,G4} vs ref {count:2,A4->G4}+{count:2,D4->G4}). MusicXML's number attribute exists precisely to distinguish overlapping slurs; croma's output silently mangles which notes are slurred. Root: `next_slur_id` is a per-voice counter (crates/croma-core/src/lower/voice.rs:84,147,428-431) and is exported verbatim as `number` (crates/croma-core/src/musicxml/notation.rs:44). The prior manif…


*Fix:* Export layer: crates/croma-core/src/musicxml/notation.rs:43-58 — stop emitting raw `slur.pair_id` as the number; allocate per-part live numbers the way tuplets already do (`tuplet_numbers.number_for(pair_id)`, notation.rs:68): pick the lowest number not currently open in the part at the start event, free it at the stop. Alternative: allocate `next_slur_id` per part instead of per voice in crates/c

*Fixed in 37h:* MusicXML export now keeps a per-part live slur-number
allocator, keyed by semantic source voice plus lowered slur pair id, so
simultaneous slurs from voices merged into one part receive distinct MusicXML
`number` values while overlay continuation events still stop the number opened
by their source voice. The allocator preserves raw pair ids unless another slur
with that number is already active in the same part, which avoids regressing
files that already use high slur numbers. Target `tune_010662.abc` now compares
as a structural match; the two-file target changed from 0 structural matches /
19 mismatch rows / 3 slur rows to 1 structural match / 17 mismatch rows / 1 slur
row, leaving only the known residual `tune_013200.abc` abc2xml numbering
behavior. Full 10k report-only compare after 37h: structural matches
8874->8875 vs 37g, mismatch rows 162475->162473, `slur` 102->100; no import,
harness, or worker failures.


### `slur-single-target-count-encoding` — **EQUIV** (legitimate_difference, repro=None)

*Share:* ~1.5% rows (2/129) — *files:* tune_006887.abc, tune_006796.abc


tune_006887 `([C6E6G6c6])` (slur around one chord): croma fact {count:1, start=end=C4.E4.G4.C5} — start and stop on the same chord member, music21 dedupes to one spanned element; ref {count:2, start=end=C4.E4.G4.C5} — abc2xml puts start and stop on different member <note>s of the same chord, so music21 registers the chord twice. tune_006796 analogous on a single note (F5,F5 count 1 vs 2). Same displayed music — a slur on one chord/note (spec-supported, §4.11 line 999-1000). Pure serialization difference; teach the COMPARATOR to treat slur items with start==end as equal regardless of count (never change croma).


### `slur-abc2xml-structural-cascade` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~30% rows (39/129: 005352=19, 011364=15, 001365/66/67=3, 006803=1, 007141=1) — *files:* tune_005352.abc, tune_011364.abc, tune_001366.abc


tune_005352: invalid header line `notC:Thomas Arne,...` (plus `!` line-break dialect) is parsed by abc2xml as tune-body music — its reference m1/m2 contain fabricated notes C4, A4, E4 and a fabricated slur start on E4/stop on A5 that exist nowhere in the ABC; croma's slur list is identical from index 1 on, so all 19 rows are a +1 index shift. tune_011364: abc2xml drops 4 measures of real music at `:||:!` (manifest ABC2XML_DROPS_MUSIC, croma 36 measures vs ref 32); croma has 25 slurs vs ref 21 because croma preserved the music — slur lists match until the dropped section (indexes 0-5 equal), then shift. Already documented as docs 02/07 (phantom measures, header-prose segmentation); croma is the spec-correct side (ABC 2.1 §2.2.1: a bar line/annotation is not a measure; header prose is not music).


### `slur-malformed-input-recovery` — **QUIRK** (reference_quirk, repro=None)

*Share:* ~15% rows (19/129: 011831=7, 008437=4, 004340=4, 002101-03=3, 008954=1) — *files:* tune_008437.abc, tune_011831.abc, tune_004340.abc, tune_002101.abc


Malformed input. (a) tune_008437 line `(E6 z B |` opens a slur never closed (whole-tune parens unbalanced): croma keeps slurs locally balanced and emits a dangling <slur type=start num=5> at m8 plus a single-note slur at m15 `(E6)`; abc2xml FIFO-pairs the m15 `)` with the m8 open (stop-then-start at m15 E4), leaving ITS m15 start dangling instead — reproduced minimally with `(E2 z2 B4|(E4)z2 e2|]`. (b) tune_011831 `(e/2c/2()|` (empty `()` pair): croma emits a stop on C#5 BEFORE its paired start on the next D5 (music21 then synthesizes an extra 1-element spanner); abc2xml drops the empty pair. (c) tune_004340 invalid `K:Bb, F` field: croma warns `abc.field.invalid_k` and ignores (stays Dmaj, slur endpoint E5); abc2xml salvages 'Bb' (Eb5) — verified minimally; this is the accidental-category key issue leaking into slur endpoint pitches. (d) tune_002101-03 `("(F)"cA"(Bb)")B` — parens inside…


### `slur-nested-close-lifo-vs-fifo` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~11% rows (14/129: 013483/84=8, 006724/25=4, 012447/48=2) — *files:* tune_013483.abc, tune_006724.abc, tune_012447.abc


Minimal repro is the spec's own example `(c d (e) f g a)` (ABC 2.1 §4.11 lines 997-1000: 'Slurs may be nested ... they may also start and end on the same note'): croma emits nested slurs C5->A5 (num=1) + single-note E5 (num=2), exactly the spec semantics; abc2xml emits two CHAINED slurs C5->E5 and E5->A5 (FIFO pairing), which contradicts 'start and end on the same note'. Corpus: tune_013483 m6 `a/2 (d'3 (d'3) c')` — croma outer D6->C6 + single-note D6, ref D6->D6 + D6->C6 (4 rows incl. repeat); tune_006724/006725 V:3 `(F,|G,A,B,C (D2)CB,)` — croma F#3->B3 + single-note D4, ref F#3->D4 + D4->B3; tune_012447/012448 `(e|(e6)|e)`. The prior doc 09 called 012447 a 'single-note slur count' and 006724 an 'endpoint off-by-one at chord boundary' — both are actually this one FIFO/FIFO mechanism; the sharpened verdict stands: croma spec-correct.


### `slur-rest-anchor` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~1.5% rows (2/129) — *files:* tune_008749.abc


Minimal: `(zGAB) c4|]` -> croma: slur start on G4, stop on B4; abc2xml: slur start on the REST <note>, stop on B4. Corpus tune_008749 `(6:4:6(z/G/A/B/c/d/)`: ref slur fact start="" (music21 Rest has empty .pitches), croma start=G4 — the two sampled rows. ABC 2.1 §4.11 line 1006: 'slurs connect the first and last note of any series of notes' — a rest is not a note, so croma anchoring on the first sounding note is the more spec-faithful reading; a MusicXML slur start on a rest is semantically dubious for most readers. Both are recoveries of an under-specified construct; croma's is defensible. Prior doc 09 did not cover this case.


### `slur-tie-to-slur-conversion` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~5% rows (7/129: 006084=5, 014677=2) — *files:* tune_006084.abc, tune_014677.abc


Minimal: `A- B A2|]` -> abc2xml emits <slur> A4 start / B4 stop (no tie); croma emits nothing and warns `abc.music.unmatched_tie`. Same for `|: A A |1 A A- :|2 B A |]` — abc2xml slurs A4(end of volta 1)->B4(start of volta 2), two notes never played consecutively. ABC 2.1 §4.11 lines 992 & 1004-1006: ties connect 'two notes of the same pitch' and ties/slurs 'have completely different meanings ... and should not be interchanged' — abc2xml's fabricated slur deviates; croma's warn+drop is spec-defensible. Corpus: tune_006084 ref has 2 extra slurs (P1 m5->m6 A4->B4, m14->m15 A4->E5) at volta boundaries `A3 A- :|2 B...`, shifting 5 rows; tune_014677 `[^de]-[ee]-[ee]` chord ties -> ref per-chord slurs (D#5.E5 -> E5.E5), 2 rows. Extends prior doc 09's tie finding into the slur category.



## tie

### `tie-after-slur-close-dropped-by-abc2xml` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~1.5% rows (6/392), 2/44 files — *files:* tune_005286.abc, tune_008488.abc


Minimal repro /tmp/tie-triage/r3_slurclose_tie.abc: '(G2 E>G F>E|E2)-E2 G4|'. croma ties E4->E4 (<tie>+<tied> start/stop); abc2xml emits no tie on either note. The tie connects two successive notes of the same pitch (ABC 2.1 §4.11, lines 1004-1006) and §4.20 (line 1260) says the tie 'should come immediately after a note group' — here it trails the group's slur-close, a placement abcm2ps and abc2midi accept. abc2xml silently loses the author's tie; croma is the spec-correct side. Corpus: tune_005286 m9/m13 ('E2)-E2', 'E2)-E', croma start+stop vs ref none — verified) and tune_008488 m17-18 ('(G/2 G3/2)-|G3/2', croma G(start)|G(stop) vs ref none — verified). Confirms and extends doc 09's tune_005286 bullet (tune_008488 is the same construct, previously unattributed).


### `tie-cascade-structural-artifacts` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~90% rows (~350/392), 33/44 files — *files:* tune_015281.abc, tune_003837.abc, tune_012819.abc


33 of the 44 tie-category files are not tie problems at all: per docs/untracked/phase-33/per-file-manifest-refreshed.csv, 30 are PHANTOM_MEASURE, 2 CASCADE, 1 DURATION_EXACT_VS_ROUNDED (tune_012819). In these files abc2xml inserts phantom empty measures at annotation/section/inline-key boundaries (or rounds durations to a hardcoded L:1/8 grid), shifting positional alignment so tie facts land on differently-numbered measures/onsets; the same shift generates the bulk of pitch/octave/barline/measure_alignment rows for the same files (e.g. tune_015281: 2032 rows across 11 categories from 2 phantom measures). Root causes already adjudicated croma-correct in docs/comparison/abc2xml-divergences/02-phantom-measures.md, 06-duration.md, 07-cascade-artifacts.md (ABC 2.1 §2.2.1/§4.8: a barline or annotation is not a measure; §4.6: unit length is meter-derived).


### `tie-dangling-at-tune-end` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~1% rows (3/392), 3/44 files — *files:* tune_004529.abc, tune_003600.abc, tune_015600.abc


Minimal repros /tmp/tie-triage/r1_dangling_end.abc ('G2 E4 EG-||', EOF) and r2_tie_repeat_end.abc ('...d2-:|', EOF): abc2xml emits <tie type="start"/>+<tied type="start"/> on the final note with no stop anywhere; croma emits no tie but diagnoses it (warning abc.music.unmatched_tie, 'Tie marker does not connect two matching notes') — not silent loss. ABC 2.1 §4.11 defines a tie as connecting TWO successive notes of the same pitch; a pair-less trailing '-' fails that definition, and abc2xml's unpaired <tie> (a playback element that never resolves) is a fabrication. Croma is not over-strict: when a same-pitch note DOES follow a ':|' (tune_015600 m9 'd2-:|' then '|:d>f'), croma ties across the repeat boundary identically to abc2xml (verified, no mismatch row); only the truly pair-less final ties differ. Corpus verified: tune_004529 m21 ('EG-||'), tune_003600 m26 ('E2-||'), tune_015600 m18 ('…


### `tie-detached-leading-tie-malformed` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~5% rows (~20/392), 6/44 files — *files:* tune_008163.abc, tune_008162.abc, tune_008166.abc, tune_008168.abc


Minimal repro /tmp/tie-triage/r4_detached_tie.abc: 'F2 E4|\n-E2 G4|]'. croma emits <tie start> on m1 E4 and <tie stop> on m2 E2 (no diagnostic); abc2xml emits no tie. ABC 2.1 §4.11 (spec lines 992-994) explicitly rules this illegal: 'The tie symbol must always be adjacent to the first note of the pair ... abc|-cba are not legal.' MALFORMED INPUT — recovery choices on illegal input differ; croma's recovery preserves the transcriber's evident intent (same-pitch tie across the bar), abc2xml discards it. Corpus: tune_008163 m15-16 croma 'E(start)|E(stop)' vs ref no tie (verified in both XMLs); same construct ('-{c}d2', '-A2', '-d2', '-FEF' at line starts) in tune_008162, tune_008166, tune_008168, tune_011106, tune_014796. Confirms doc 09's 'detached tie' bullet; note the last three are manifest-filed as POSITIONAL_CASCADE but their tie rows are this genuine divergence (tune_008168: croma 23 …



## tuplet

### `tuplet-fabricated-time-modification-for-inexpressible-durations` — **OPEN** (croma_bug, repro=None)

*Share:* ~80/691 rows (~11.6%), 35 files — *files:* tune_000464.abc, tune_006760.abc, tune_006472.abc


Minimal repro /tmp/tuplet-probe/t4.abc (`M:none L:1/4 K:C` + `F16`): croma emits `<type>breve</type><time-modification><actual-notes>2</actual-notes><normal-notes>4</normal-notes>` while abc2xml emits `<type>long</type>` with no time-modification (music21 reads croma as a 1:2 tuplet, ref as none). Second shape: tune_006472 (M:5/4) whole-measure rest -> croma `<type>breve</type>` + 8:5 time-modification vs abc2xml `<rest measure="yes"/>` with no type. Grepping the croma XML of all 34 affected files finds exactly 76 breve+2/4 emissions = the 76 (1,2)-vs-none rows, plus 4 (8,5)-vs-none rows in tune_006472; 1:1 accounting. Sounding <duration> is exact on both sides; croma invents tuplet structure absent from the ABC source (silent mangling). Mechanism: note_spelling() in crates/croma-core/src/musicxml/note.rs — note_type_candidates() (line ~470) starts at breve (2/1), so 4-whole durations mi…


*Fix:* MusicXML export layer only: (1) add NoteTypeCandidate entries for "long" (4/1) and "maxima" (8/1) in note_type_candidates() in /Users/rodox/dev/rs/croma/crates/croma-core/src/musicxml/note.rs; (2) for rests spanning the full measure, emit `<rest measure="yes"/>` (note.rs line ~256, currently `self.xml.empty("rest", &[])`) and omit <type>, matching MusicXML semantics; (3) keep the time-modification


### `tuplet-one-note-group-dangling-start` — **FIXED(36a)** (croma_bug, repro=True)

*Share:* 8/691 rows (~1.2%), 2 files — *files:* tune_010459.abc, tune_010462.abc


Minimal repro /tmp/tuplet-probe/t1.abc (`(3:2:1G(3:2:1B(3:2:1g Bc dc (3ccc|`, from tune_010459/010462 m14): croma emits `<tuplet type="start" number="1..3"/>` on each single-member tuplet and NEVER closes them (numbers keep growing; downstream readers see three dangling open brackets — music21 even misreads the genuine stop of the later (3ccc as null because of them). ABC 2.1 §4.13 allows r=1 ('put p notes into the time of q for the next r notes'); the correct MusicXML for a one-member group is start+stop on the same note. Mechanism: attach_completed_tuplet() in /Users/rodox/dev/rs/croma/crates/croma-core/src/lower/tuplet.rs lines 64-86 — for groups_len==1 the `index == 0` branch assigns TupletRole::Start and the Stop branch is unreachable; one attachment per event means the Stop is silently lost. Timing (time-modification 3/2 and durations) is identical on both sides.


*Fix:* Lower layer: `attach_completed_tuplet()` now pushes both
`TupletRole::Start` and `TupletRole::Stop` when a completed tuplet contains
exactly one note group. `attachments.tuplets` is already a `Vec`, and the
MusicXML writer already iterates all attachments, so the export emits both
`<tuplet type="start">` and `<tuplet type="stop">` on the same timed event.

*Phase-36 verification:* Regression tests
`one_note_tuplet_carries_start_and_stop_on_same_event` and
`one_note_tuplet_emits_balanced_start_and_stop` cover the lowerer attachment
vector and exported MusicXML pair. Targeted after-compare for tune_010459 and
tune_010462 shows 6 residual tuplet rows, all at measure 14 events 0-2: croma
is now `[{"actual":3,"normal":2,"type":"startStop"}]`, while abc2xml/music21
reports `[{"actual":3,"normal":2,"type":null}]` from its stop-only encoding.
Those residual rows are a reference/comparator artifact, not an open
dangling-start Croma bug.


### `tuplet-nested-tuplets` — **REVIEW** (needs_deeper_look, repro=None)

*Share:* 1 direct row + ~5 rows folded into the cascade bucket (<1%), 1 file — *files:* tune_003732.abc


tune_003732 m5: comparator reads croma as [(3,2,start),(3,2,start),(2,3,null)] vs ref [(3,2,start)] — croma's nested-tuplet output yields multiple/odd music21 tuplet components (also the source of the (3,2)+(7,12) croma-vs-none rows folded into the cascade bucket: 7:12 x 3:2 = 7:8, the outer ratio). Already cataloged: docs/parser-backlog.md item 3 ('Nested tuplets', 1 corpus tune, harness gate _NESTED_TUPLET_RE) — the model/writer does not represent nesting; which side's MusicXML is more faithful needs the dedicated nested-tuplet work promised there. Not re-derived here since it is a known, gated, single-tune item.


### `tuplet-barline-truncated-group` — **QUIRK** (reference_quirk, repro=True)

*Share:* 1/691 rows (<1%), 1 file — *files:* tune_013273.abc


Malformed input. Minimal repro /tmp/tuplet-probe/t2.abc (`(3cBA (3B2G|F2|`, from tune_013273 m16): `(3` promises 3 note-groups (§4.13; the intended music needed the explicit `(3:2:2 B2G` form) but the bar arrives after 2. Both tools truncate with IDENTICAL timing (croma B=16/G=8 at quarter=24; ref B=80/G=40 at quarter=120; both keep time-modification 3/2). Difference is only recovery notation: abc2xml emits `<tuplet type="start" bracket="yes">` on B2 and never a stop (dangling bracket); croma attaches no start/stop markers and emits diagnostic abc.music.tuplet.too_few_notes (finish_open_tuplets_at_boundary in crates/croma-core/src/lower/tuplet.rs discards the incomplete tuplet without calling attach_completed_tuplet). Recovery-choice difference on illegal input; croma's unmarked-but-warned output is at least as defensible as abc2xml's unbalanced start.


### `tuplet-grace-led-bracket-on-grace-note` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~27/691 rows (~4%), 8 files — *files:* tune_015503.abc, tune_011562.abc, tune_001046.abc


Minimal repro /tmp/tuplet-probe/t3.abc (`g>c (3({d}cBc) gcac`): abc2xml puts `<time-modification>3/2` AND `<tuplet type="start" bracket="yes">` on the GRACE note {d} (a note with no <duration> element), leaving the first real triplet note unmarked (comparator: ref type null vs croma start at onset 1.0). Croma marks start on the first principal note c and stop on the last — the standard engraving convention. ABC 2.1 §4.12: gracenote duration is transparent to the melody; graces do not count toward the tuplet's p notes, so giving the ornament tuplet membership/timing is fabricated structure on abc2xml's side. Croma is spec-defensible; comparator could normalize by ignoring tuplet marks on grace notes. Affected files all contain `(3{x}...` or `(3({x}...` constructs (verified by grep): tune_015503/15504/15505/15506/15513 (O'Neill `(3({d}cBc)`), tune_011562 (`(3{f}fgf)`), tune_001046.


### `tuplet-phantom-measure-cascade` — **QUIRK** (reference_quirk, repro=None)

*Share:* ~574/691 rows (~83%), ~45 files — *files:* tune_000289.abc, tune_015281.abc, tune_009183.abc, tune_012468.abc


tune_000289: croma emits 32 measures, abc2xml 33 (the doc-02 phantom measure). The triplet `(3b/b/b/` lands in croma m26 but ref m27, producing mirrored row pairs: m26 croma [{3,2,start},{3,2,null},{3,2,stop}] vs ref [], then m27 croma [] vs ref [{3,2,start},...]. The whole-category pattern counts are near-mirrored (croma start-vs-none 113 / none-vs-ref-start 94; null 100/99; stop 79/75), the signature of positional shift, and 556 of these rows sit in files the refreshed manifest already stamps PHANTOM_MEASURE. Verified a second file (tune_015281, croma 46 vs ref 48 measures: its 'marker-differs' rows at m22/m45 compare entirely different music). Same fact verified raw: croma m26 = B24 B24 B4T B4T B4T while ref m26 = B120 B30 z30 B60 B30 z30 B60. Root cause is the structural artifact documented in docs/comparison/abc2xml-divergences/02-phantom-measures.md and 07-cascade-artifacts.md; onc…



## voice

### `voice-grace-group-dropped-at-barline` — **FIXED(33a)** (croma_bug, repro=True)

*Share:* ~1-2% rows but ~14-20 of 281 files (typically 1 voice row each; 5 of the 8 evidence-pack samples) — *files:* tune_014161.abc, tune_006796.abc, tune_006422.abc, tune_006717.abc


Minimal: `X:1\nL:1/8\nM:6/8\nK:D\nGB2Af2{e/}|d3D2|]` -> abc2xml keeps the grace (m1 = G,B,A,F + grace E); croma emits only 4 notes, exit 0, EMPTY stderr — the transcriber's note is silently lost. Same construct in 5 of the 8 evidence-pack samples: tune_014161 `GB2Af2{e/}|` (m8: croma 4 vs ref 5), tune_006796 `(f4{ef})|` (m13: croma 3 vs ref 5), tune_006422 `d>e fa {g}|` (m24: 4 vs 5), tune_006717 `f2Te2{de} |` (m17: 6 vs 8), tune_007141 `({Bc})|` (m7: 4 vs 6). The drop is deliberate: crates/croma-core/src/lower/mod.rs:512-514 clears pending_grace_groups at every barline ('a grace flushed ahead of its note but with no note before the bar is void'), but ABC 2.1 §4.12 defines graces with no void rule, and §4.20 only says graces precede the note they decorate — here that note exists, across the bar (`{e/}|d3`). Rubric: silent loss of source information = croma_bug. REFUTES the prior manifest…


*Fix:* Lower layer: crates/croma-core/src/lower/mod.rs:512-514 — stop clearing pending_grace_groups at MusicItem::Barline; keep the buffer so lower/voice.rs (pending_grace_groups merge at lines 154-167/264-292) attaches the graces to the next timed event across the bar (the §4.20-literal reading), or alternatively model a trailing/after-grace on the last event of the closing measure. Serialization alread


### `voice-multirest-collapsed-single-measure` — **FIXED(35a)** (croma_bug, repro=True)

*Share:* ~2-3% rows (4 files, but each Zn shifts all following measures: 446/14524 raw rows) — *files:* tune_009310.abc, tune_010751.abc


Minimal: `X:1\nL:1/4\nM:4/4\nK:C\nCDEF|Z2|GABc|]` -> abc2xml emits 4 measures (m2 and m3 each `<rest measure=\"yes\"/>` full-bar); croma emits 3 measures, with m2 a single `<note><rest/><duration>64</duration><type>breve</type>` — twice the 4/4 bar length, no `<measure-style><multiple-rest>`. Corpus: tune_009310 (M:2/1, two `Z2 [|]` lines) exports 54 croma measures vs 56 reference; per-measure dump shows ref has two consecutive whole-bar-rest measures where croma has one, shifting every later measure's event_count. ABC 2.1 §4.5 (spec lines 866-873) states `Z4|CD EF` is musically EQUIVALENT to `z4|z4|z4|z4|CD EF` — Zn IS n measures of music, so an export with one measure mangles the measure count, every subsequent measure number, and emits a measure whose duration contradicts its time signature (MusicXML has no single-measure multirest encoding; the multirest glyph is `<measure-style><mul…


*Fix:* Lower layer: crates/croma-core/src/lower/mod.rs:452-465 currently lowers MultiMeasureRest as one timed rest of meter_duration*count; instead expand into count whole-measure rest events separated by measure boundaries (or carry the count on the model so the writer in crates/croma-core/src/musicxml/ emits `<measure-style><multiple-rest>count</multiple-rest></measure-style>` plus count measures each 


### `voice-abc2xml-drops-music` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~2% rows (4 files; 232/14524 raw rows) — *files:* tune_014784.abc, tune_011320.abc


tune_014784 ('The Baggpipe tune', deliberately 'rather unconventional ABC notation' per its own comments): `V:2 merge down transpose = +2` plus interleaved `V:1`/`V:2` body lines — abc2xml exports only 2 measures where croma exports 20 (manifest: 1144 mismatch rows, measure_delta -18); croma preserved the real music abc2xml lost. Similar: tune_008657, tune_011364, tune_011320. Every croma-vs-ref measure after the dropped material mismatches event_count, producing voice rows. abc2xml losing music on hard input is its artifact (and partly malformed/nonstandard input); croma is more spec-faithful. Verifies the ABC2XML_DROPS_MUSIC verdicts (doc 02) in per-file-manifest-refreshed.csv.


### `voice-abc2xml-drops-tacet-bars` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~1% rows (2 files; 164/14524 raw rows) — *files:* tune_011865.abc, tune_011164.abc


tune_011865 (croma 57 vs ref 41 measures) and tune_011164 (36 vs 30): manifest justification, backed by doc 11 (11-multipart-and-partgroup.md), shows abc2xml dropping 16/6 silent bars from a voice while croma keeps them so sibling voices stay measure-aligned (ABC 2.1 §4.8 — in multi-voice music each voice must carry the same bar structure). With bars missing on the reference side, each croma measure pairs against shifted reference content, emitting event_count (voice) rows for the remainder of the part. Verified the measure-count deltas against the manifest; did not build a fresh minimal repro (multi-voice merge setup), hence medium confidence; consistent with the prior, documented verdict.


### `voice-header-prose-parsed-as-music` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~4-6% rows (~36 of the 56 CASCADE-verdict files) — *files:* tune_005569.abc, tune_004011.abc, tune_009970.abc, tune_013208.abc


Malformed input: header fields wrapped across lines without the ABC 2.1 `+:` continuation. Minimal: `X:1\nT:Test\nH:I heard this on the internet.\nGo figure!\nM:4/4\nL:1/4\nK:C\nCDEF|]` -> abc2xml emits 8 notes in m1: G,F,G,E (the note-letters of 'Go figure!') + C,D,E,F; croma emits the real 4 notes and ignores the orphan line. Corpus confirmations: tune_005569 (ref m1 = G4 F5 G5 E5 + real notes, from the wrapped H: line 'Go figure!'), tune_004011 (ref m1 prepends A5 B5 C5 from the F:-URL continuation line '2.abc'), and the same pattern in tune_009970, tune_013208, tune_006655 (wrapped H:/N: prose). Fabricating pitches from prose on illegal input is an abc2xml recovery artifact; croma's skip is the defensible recovery (malformed input). Verifies doc 07's 'header-prose segmentation' cascade. Minor note: croma's only diagnostic on the minimal is 'Unknown ABC field H:' — H: (history) is a l…


### `voice-phantom-empty-measure-shift` — **QUIRK** (reference_quirk, repro=True)

*Share:* ~85-90% rows (215 of 281 files; 13131/14524 rows in the raw mismatch parquet) — *files:* tune_011491.abc, tune_003837.abc


Minimal: `X:1\nL:1/8\nM:6/8\nK:G\nGAB GAB||\n\"variation, bars 2 and 6\"|dBd dBd|]` -> croma emits 2 measures; abc2xml emits 3, the middle one containing ONLY `<words>variation, bars 2 and 6</words>` and zero notes. After the phantom, every croma measure N pairs against ref measure N holding different music, so the comparator emits one voice (event_count) row per following measure. Corpus: tune_011491 (ref m11 = 0 events, croma m11 = the 10-note variation bar; verified by dumping both XMLs) and tune_003837 (8 phantom measures from section-letter annotations `"A"\` before `|:`; ref 78 vs croma 70 measures). An annotation before a barline is not a measure of music (ABC 2.1 §2.2.1, §4.8 — annotations attach to the following note); croma's folded layout is spec-correct. Verifies docs/comparison/abc2xml-divergences/02-phantom-measures.md. Comparator-side realignment that skips zero-note refer…
