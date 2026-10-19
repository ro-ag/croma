# croma `--engrave`: engraving rules specification (clean-room)

Status: implementation specification for the functions in the engrave
interface contract (`beam::plan`, `beam::tuplet_shows_bracket`, `stem::abs_step`,
`stem::middle_distance`, `stem::note_type_has_stem`, `stem::stem_up`,
`stem::rest_display`, `stem::slur_placement`, `stem::tie_orientation`).

This document is written from published engraving literature and from the
MusicXML 4.0 reference only. It does not derive from the code, tables, or
internal documentation of any notation program. Each rule cites its source. A
rule marked **[choice]** is a decision made here because the sources disagree,
are silent, or describe a judgement call the contract cannot express. Its
reason is given next to it.

Short source keys used below (full list in "Sources"):

- **Gould**: Elaine Gould, *Behind Bars: The Definitive Guide to Music
  Notation* (Faber, 2011). Cited by chapter/topic.
- **Ross**: Ted Ross, *The Art of Music Engraving and Processing* (1970).
- **Read**: Gardner Read, *Music Notation: A Manual of Modern Practice*
  (2nd ed., 1969).
- **Stone**: Kurt Stone, *Music Notation in the Twentieth Century* (1980).
- **MX**: MusicXML 4.0 reference, https://www.w3.org/2021/06/musicxml40/
  (element pages cited by URL).
- **WP-Beam**: Wikipedia, "Beam (music)", https://en.wikipedia.org/wiki/Beam_(music)
- **WP-Tie**: Wikipedia, "Tie (music)", https://en.wikipedia.org/wiki/Tie_(music)
- **KSU**: Kent State University music-rudiments page "Beaming Rules",
  https://www.personal.kent.edu/~sbirch/Common/Encyclopedia/Rudiments/beaming_rules.htm

---

## 0. Common definitions and numeric safety

0.1 **Ticks.** Quarter = 480, whole = 1920. An eighth = 240, a 16th = 120, a
32nd = 60. A "note with L beams" has written value 1920 / 2^(L+2) ticks when it
has no dots.

0.2 **Beam levels.** Level 1 = primary (eighth) beam, level 2 = 16th, and so on.
MusicXML numbers beams 1 to 8 (eighth through 1024th) (MX, `<beam>`:
https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/beam). The
effective beam count of an input is therefore `min(beams, 8)`. **[choice]**
Inputs with `beams > 8` are treated as 8, because MusicXML cannot express a
ninth level.

0.3 **No panics, no unbounded work.** All arithmetic on ticks uses `i64`. Use
floor (Euclidean) division and modulo for anything that may be negative. Widen
`i32` distances to `i64` before negating or taking absolute values (so
`i32::MIN` cannot overflow). The beam planner never allocates anything whose
size depends on the meter numerator or on tick magnitudes. All meter structure
is computed arithmetically per note. Work is O(n) in the number of inputs, plus
an O(log) loop bounded by 64 iterations for the subdivision chain (§1.3).

0.4 **MusicXML beam text.** Exactly the five strings `"begin"`, `"continue"`,
`"end"`, `"forward hook"`, `"backward hook"` (MX, `beam-value`:
https://www.w3.org/2021/06/musicxml40/musicxml-reference/data-types/beam-value).

Notation for worked examples. An input is written `(rtick, dur, beams)`, with
`R` for a rest and `T<id>` for a tuplet member. Output per note is written as
`[1:begin, 2:begin]`, meaning `<beam number="1">begin</beam><beam
number="2">begin</beam>`. `[]` means no `<beam>`.

---

## 1. Beam grouping: `beam::plan`

### 1.1 Principles (from the literature)

P1. **Beams show the beat.** Notes are beamed so that the beat (and, inside a
beat, its main subdivision) can be seen at a glance. Divisions of a beat are
beamed together, and a beam does not normally cross a beat boundary (Gould,
"Beams": beaming according to the metre. Read, "Beams". Ross, "Beams". KSU,
"Beam according to the beat... the beat should always be visible". WP-Beam,
"Beams and metre").

P2. **Exceptions that join beats.** In 4/4 a group of eighths may be beamed in
half-bars (beats 1–2, 3–4), but never across the middle of the bar (beats 2–3)
(Gould, "Beams", simple time. KSU 4/4). In 2/4 and 3/4 several beats of eighths
may be beamed together (Gould, "Beams". KSU 3/4).

P3. **Compound time beams in dotted-note beats.** In 6/8, 9/8, 12/8 and
similar, the beat is a dotted value. Eighths are beamed in threes (Gould,
"Beams", compound time. Read, "Beams". WP-Beam).

P4. **Irregular (additive) metres beam by their 2- and 3-unit groups.** 5/8,
7/8 and the like are grouped into units of two and three eighths. The grouping
should match the intended pulse, and a composer may state it in the time
signature (e.g. 3+2/8) (Gould, "Beams", irregular metres, and "Time
signatures". Stone, "Meter: irregular meters").

P5. **Only notes with flags are beamed.** Quarter notes and longer values have
no flag and are never beamed (Gould, "Beams". Read, "Beams").

P6. **Secondary beams subdivide.** Inside a primary beam, secondary beams (16th
and shorter) may break to show the beat's subdivision. Partial beams (hooks)
are used when a single note has more beams than its neighbours, for example
dotted-eighth + 16th (Gould, "Beams": secondary beams and partial beams. Ross,
"Beams". WP-Beam, "Secondary beams" / "broken beams").

P7. **Partial-beam direction.** A partial beam points toward the note it
belongs with rhythmically. On the last note of a group it points backward
(left). On the first note it points forward (right). Inside a group it points
toward the side of the subdivision it completes, so in a dotted figure the
short note's hook points to its dotted partner (Gould, "Beams": partial beams.
Read, "Beams").

P8. **Tuplets beam as a unit.** A tuplet's notes are beamed together when they
can be beamed. This lets the tuplet number sit on the beam without a bracket
(Gould, "Tuplets". Read, "Irregular rhythmic groupings". Stone, "Tuplets").

### 1.2 Meter structure: the beat grid

Inputs: `Meter { numerator N, denominator D }`. Define the unit
`u = 1920 / D` when `D >= 1` and `1920 % D == 0`. Otherwise use the fallback
grid (rule G0).

For every tick position `t` (an onset, measured from the barline), the grid
gives:

- a **bar index** `k` (so overfull measures and negative ticks are handled),
- a **beat** containing `t`: its start tick `bs` (relative to the bar start)
  and its length `B`,
- the beat's **first division** `d1`, which is 2 or 3.

Two onsets are **in the same beat** iff they have the same `(k, beat start)`.

Rules (each is O(1) arithmetic, with no per-beat tables):

**G0. Fallback** (N == 0, D == 0, or `1920 % D != 0`, e.g. 3/7, 4/0, 4/256).
**[choice]** The music is treated as a run of quarter-note beats from the
barline: `B = 480`, `d1 = 2`, `k = 0`, beat index = `floor(t / 480)`,
`bs = index * 480`. Reason: when the meter cannot be put on the tick grid, the
quarter is the most neutral pulse. It is never wrong in a way that hides the
rhythm, and it needs no knowledge of the bar length.

Otherwise let `bar = N * u` (fits in `i64`: at most about 4.3e9 * 1920).
`k = floor(t / bar)` and `tl = t - k * bar` (local tick, `0 <= tl < bar`).
`p = floor(tl / u)` is the unit index inside the bar.

**G1. Compound meters**: `N >= 6` and `N % 3 == 0` (6/8, 9/8, 12/8, 15/8, 6/4,
9/4, 12/4, 6/16, 9/16, 12/16, 6/2 ...). `B = 3u`, `d1 = 3`,
`bs = floor(tl / B) * B`. (P3. Gould "Beams", compound time.)

**G2. Simple meters with a long unit** (`u > 240`, i.e. the unit is a quarter
or longer, and G1 does not apply; for example 2/4, 3/4, 4/4, 5/4, 7/4, 2/2,
3/2, 4/2, 2/1). `B = u`, `d1 = 2`, `bs = p * u`. (P1.) **[choice]** 5/4, 7/4
and other long-unit numerators that are not multiples of 3 beam per unit beat,
not in 2+3 groupings. Reason: the quarter is already a beat that cannot be
beamed (P5), so per-beat beaming of eighths is always correct and shows the
pulse. Any larger grouping would need composer intent that the contract does
not carry.

**G3. Short-unit meters with numerator 1 to 4** (`u <= 240`: x/8, x/16, x/32 ...,
and G1 does not apply). The unit note has a flag, so the beat is a group of
units:
- N = 1: `B = u`, `d1 = 2`.
- N = 2: `B = 2u` (the whole bar), `d1 = 2`. (2/8: both eighths beamed.)
- N = 3: `B = 3u` (the whole bar), `d1 = 3`. (3/8, 3/16: the whole bar beamed,
  felt as one dotted beat. Gould "Beams": 3/8 beamed as a single group.)
- N = 4: `B = 2u`, `d1 = 2`. (4/8 beamed 2+2, the analogue of 4/4 half-bars.
  Gould: "in 4/8 ... semiquavers ... may be beamed into half-bars", via
  secondary citation.)
`bs = floor(tl / B) * B`.

**G4. Irregular short-unit meters** (`u <= 240`, `N >= 5`, `N % 3 != 0`: 5/8,
7/8, 8/8, 10/8, 11/8, 13/8, 5/16, 7/16 ...). (P4.) **[choice]** Without an
additive signature (the contract's `Meter` cannot carry one), group the units
in 2s, with a single 3 at the end when N is odd:
- N even: every beat is `B = 2u`, `d1 = 2`, `bs = (p - p % 2) * u`.
  (8/8 = 2+2+2+2.)
- N odd: if `p < N - 3`, the beat is `B = 2u`, `d1 = 2`, `bs = (p - p % 2) * u`.
  Otherwise (the last three units) `B = 3u`, `d1 = 3`, `bs = (N - 3) * u`.
  (5/8 = 2+3, 7/8 = 2+2+3, 11/8 = 2+2+2+2+3.)
Reason: Gould and Stone say the grouping should follow the music and accept
any order of 2s and 3s. Lacking that information, the simplest deterministic
rule, computable in O(1) for any numerator, keeps the regular 2-grouping (as in
4/8) and puts the one irregular beat last.

### 1.3 Subdivision chain and beam spans

For a beat of length `B` with first division `d1`, the **subdivision chain** is:
`c0 = B`; `c1 = B / d1` if `B % d1 == 0`; then each next element is half the
previous while the previous is even. Stop when an element is odd, when it would
fall below 1, or after 64 elements. (Compound beat 720 gives 720, 240, 120, 60,
30, 15. Simple beat 480 gives 480, 240, 120, 60, 30, 15.)

For each beam level `L` (1..8) define the **span limit** `lim(L) = 2880 >> L`:
1440, 720, 360, 180, 90, 45, 22, 11. The **level-L span** `S_L` is the largest
chain element `<= lim(L)`. If no element is that small, use the smallest
element.

The meaning: a level-L beam joins at most about six notes of its own value (or
four or so of a shorter subdivision), aligned to the beat's subdivisions. This
is how P1 and P6 are applied uniformly. **[choice]** This one arithmetic rule
stands in for the literature's per-meter descriptions. It gives:
- 4/4, 2/4, 3/4 (beat 480): primary groups per beat. 16th beams continuous
  across the beat (`S_2 = 480`). 32nd beams broken at the half-beat
  (`S_3 = 240`), so eight 32nds in a beat show two groups of four while the 8th
  and 16th beams stay continuous.
- 2/2, 3/2, 4/2 (beat 960): eighths four to a beam (`S_1 = 960`). 16th beams
  broken at each quarter (`S_2 = 480`). (Gould "Beams", 2/2: semiquavers
  grouped in crotchets.)
- 6/8, 9/8, 12/8, 3/8 (beat 720): eighths in threes. Six 16ths under one
  continuous double beam (`S_2 = 720`). 32nd beams broken at each eighth
  (`S_3 = 240`).
- 6/16, 9/16, 12/16 (beat 360): 16ths in threes. 32nd beams continuous through
  the beat (`S_3 = 360`).
- 6/4 (beat 1440): six eighths under one primary beam (`S_1 = 1440`). 16ths
  broken at each quarter (`S_2 = 480`). **[choice]** Gould also allows 6/4
  eighths in pairs. Beaming the dotted-half beat is chosen because it shows the
  compound beat (P3).
- 2/1 (beat 1920): eighths in fours per half (`S_1 = 960`).

For a non-tuplet input with local tick `tl` in a beat starting at `bs`, its
**level-L cell** is `floor((tl - bs) / S_L)`.

### 1.4 Rules

B1. **Beamable.** An input is beamable iff it is not a rest and its effective
beam count is at least 1 (P5).

B2. **Rests.** **[choice]** A rest is never beamed and always interrupts a beam.
The notes before and after it belong to different groups, and the rest gets no
`<beam>`. Reason: Read and Ross beam only between notes. Gould allows beaming
over rests inside a beat as an option, not a requirement. Breaking at rests is
the conventional default and avoids MusicXML beams on rests, which many
consumers handle poorly.

B3. **Adjacency.** Only inputs adjacent in the input order can be in the same
group (a chord is one input). If `rtick[i] < rtick[i-1]` (out of time order),
the beam is broken between them.

B4. **Same-beat joining (non-tuplet).** Two adjacent beamable inputs `a`, `b`
that are both non-tuplet join the same provisional group iff they are in the
same beat (same `k` and beat start) and in the same level-1 cell. The test uses
onsets only (P1).

B5. **Tuplets as units.** Two adjacent beamable inputs that both have
`tuplet_id == Some(x)` with the same `x` join, whatever beat boundaries lie
between them (P8). **[choice]** A tuplet member never joins a non-tuplet note
or a member of a different tuplet id, so a tuplet is never merged with its
neighbours across its boundary. Reason: Gould and Read beam a tuplet as a
separate unit so its number belongs to one group. Mixed groups would make the
bracket rule (§2) ambiguous. Limitation: `tuplet_id` is the innermost tuplet,
so two inner tuplets of a nested tuplet form separate groups.

B6. **Eighth-beat merging (P2).** Applies only when the grid is G2 with
`u == 480` (a quarter beat) and `N ∈ {2, 3, 4}`. The merge spans are:
- N = 2 (2/4): beats {0, 1} (the whole bar).
- N = 3 (3/4): beats {0, 1, 2} (the whole bar).
- N = 4 (4/4, also `C`): beats {0, 1} and {2, 3} (half-bars, never across the
  middle).
A beat is a **complete eighth beat** iff its provisional group has exactly two
members, both non-tuplet, both with effective beams == 1 and `dur == 240`, with
onsets at the beat start and at beat start + 240 (local ticks). All the
provisional groups of a merge span (in the same bar index `k`) are merged into
one group iff every beat of the span is a complete eighth beat and the groups
are consecutive in input order (the last index of one + 1 == the first index
of the next). Otherwise nothing in that span is merged.
**[choice]** In 3/4 only a full bar of six eighths merges. Partial merges such
as four eighths followed by a quarter stay per beat. Reason: KSU and Gould
allow several beats of eighths beamed together in 3/4, but a four-eighth group
in 3/4 suggests a 2/4 + 1/4 bar. The full-bar case is the unambiguous one.
**[choice]** Merging applies only to pure eighths. Any 16th or dotted value
keeps per-beat beaming. Reason: Gould states the half-bar exception for
quavers. Once shorter values appear, the beat must be visible again.

B7. **Minimum size.** A provisional or merged group with fewer than 2 members is
dropped. Its note keeps its flag and gets no `<beam>` (contract: groups have
2 or more members).

B8. **Level 1 output.** In each group the first member gets `1:begin`, the last
gets `1:end`, and every member between gets `1:continue`. Every member has at
least one beam, so level 1 is never interrupted inside a group.

B9. **Secondary levels (L >= 2) output.** For each group and each level `L`
from 2 to the group's largest effective beam count, two consecutive members
`a`, `b` are **connected at level L** iff both have effective beams `>= L` and:
- in a non-tuplet group: `a` and `b` are in the same beat and the same
  level-L cell (§1.3);
- in a tuplet group: always (**[choice]**: no internal subdivision breaks
  inside a tuplet. Reason: the tuplet's own division is not described by the
  meter grid, and continuous beams through a tuplet are the usual practice in
  Gould's tuplet examples, e.g. a sextuplet under unbroken double beams).

Maximal chains of connected members are **runs**. A run of 2 or more gets
`L:begin`, `L:continue`..., `L:end`. A member with effective beams `>= L` that
belongs to no run of length 2 or more gets a single hook (B10). Members with
fewer than L beams get nothing at level L. Because `S_L` divides `S_(L-1)`
(they come from one chain), every level-L run lies inside a level-(L-1) run,
which keeps the MusicXML valid.

B10. **Hook direction (P7).** For a lone member `n` at level `L`:
1. If `n` is the first member of its group: `L:forward hook`.
2. Else if `n` is the last member of its group: `L:backward hook`.
3. Else (interior): let `v = 1920 >> (L + 1)` (the value of a note with L-1
   beams: 240 for L = 2, 120 for L = 3 ...). Let `pos` be `tl - bs` for a
   non-tuplet group, or, for a tuplet group, the sum of the written `dur` of
   the group members before `n`. If `pos % v == 0` (Euclidean), the note
   starts a subdivision: `forward hook`. Otherwise it completes one:
   `backward hook`.
Reason: the hook points to the note it shares a subdivision with (Gould,
partial beams. Read). A note starting on the subdivision belongs with what
follows, and a note in the second half belongs with what precedes. The tuplet
case uses written durations because tuplet onsets are scaled.

B11. **Segment order.** Each note's segment list is in ascending level order,
one entry per level the note takes part in. `segments.len() == notes.len()`.

B12. **`groups` output.** Exactly the level-1 begin..end runs, each as
ascending indices, listed in ascending order of first index.

### 1.5 Decision procedure (plain steps)

1. If `notes` is empty, return empty `segments` and `groups`.
2. Compute the grid kind (G0–G4) once from the meter.
3. For each input compute: beamable? (B1); effective beams; for non-tuplets
   the bar index, the beat start, the beat length, `d1`, and the level-1 cell.
4. Scan left to right, building provisional groups: input `i` continues the
   current group iff `i - 1` and `i` are both beamable, in time order (B3),
   and join under B4 (both non-tuplet) or B5 (same tuplet id). Otherwise `i`
   starts a new provisional group if it is beamable, and is ungrouped if it is
   not.
5. Apply eighth-beat merging (B6) when the grid qualifies.
6. Drop groups with fewer than 2 members (B7).
7. For each remaining group emit level 1 (B8), then each level 2..max (B9 with
   hooks per B10).
8. Fill `segments` (empty for ungrouped inputs) and `groups`.

### 1.6 Edge cases

- Empty input: empty plan.
- One note: no group, `segments = [[]]`.
- All rests, or all quarter-or-longer: no groups, all segments empty.
- Two eighths in different beats (e.g. 4/4 at 240 and 480): no group, both
  flagged.
- Huge numerator (e.g. N = 4294967295): the bar length and unit index are
  computed with `i64` arithmetic. G4's "last three units" test uses `N - 3`
  directly. Nothing is allocated per beat.
- Huge or unusual denominator: any `D` with `1920 % D != 0` (e.g. 256, 7) uses
  G0. A non-power-of-two `D` that divides 1920 (e.g. 3, 6) uses G1–G4
  normally, judged by `u`.
- Overfull measure or onsets past the bar: the bar index `k` repeats the grid
  bar by bar. Notes in different `k` never join.
- Negative `rtick`: floor division puts it in bar `k = -1`. The rules apply
  unchanged.
- `dur <= 0`: no effect on joining (onsets decide). It only fails the "complete
  eighth beat" test in B6.
- `beams > 8`: treated as 8 (0.2).

### 1.7 Worked examples (`plan`)

E1. **4/4, eight eighths** `(0..1680 step 240, 240, 1)`.
groups `[[0,1,2,3],[4,5,6,7]]`.
n0 `[1:begin]`, n1 `[1:continue]`, n2 `[1:continue]`, n3 `[1:end]`,
n4 `[1:begin]`, n5 `[1:continue]`, n6 `[1:continue]`, n7 `[1:end]`.

E2. **4/4, never across the middle**: q `(0,480,0)`, e `(480,240,1)`,
e `(720,240,1)`, e `(960,240,1)`, e `(1200,240,1)`, q `(1440,480,0)`.
Beats 1 and 2 lie in different half-bars, so they do not merge.
groups `[[1,2],[3,4]]`. n0 `[]`, n1 `[1:begin]`, n2 `[1:end]`, n3 `[1:begin]`,
n4 `[1:end]`, n5 `[]`.

E3. **4/4, half-bar plus beat**: six eighths at 0..1200, then q `(1440,480,0)`.
groups `[[0,1,2,3],[4,5]]`. n0 begin, n1 cont, n2 cont, n3 end (level 1),
n4 `[1:begin]`, n5 `[1:end]`, n6 `[]`.

E4. **3/4, six eighths** `(0..1200 step 240, 240, 1)`: one group
`[[0..5]]`. n0 `[1:begin]`, n1–n4 `[1:continue]`, n5 `[1:end]`.

E5. **3/4, four eighths and a quarter**: e e e e at 0, 240, 480, 720, then
q `(960,480,0)`. No full-bar merge. groups `[[0,1],[2,3]]`.

E6. **2/4, 16ths then eighths**: s `(0,120,2)`, s `(120,120,2)`,
s `(240,120,2)`, s `(360,120,2)`, e `(480,240,1)`, e `(720,240,1)`.
Beat 0 is not a complete eighth beat, so no merge. groups
`[[0,1,2,3],[4,5]]`.
n0 `[1:begin, 2:begin]`, n1 `[1:continue, 2:continue]`,
n2 `[1:continue, 2:continue]`, n3 `[1:end, 2:end]`, n4 `[1:begin]`,
n5 `[1:end]`.

E7. **Dotted-eighth + 16th** (any meter with a quarter beat):
`(0,360,1)`, `(360,120,2)`. groups `[[0,1]]`.
n0 `[1:begin]`, n1 `[1:end, 2:backward hook]`.

E8. **16th + dotted-eighth**: `(0,120,2)`, `(120,360,1)`.
n0 `[1:begin, 2:forward hook]`, n1 `[1:end]`.

E9. **6/8, six eighths**: groups `[[0,1,2],[3,4,5]]`. Each group: begin,
continue, end on level 1.

E10. **6/8, e. s e | e e e**: `(0,360,1)`, `(360,120,2)`, `(480,240,1)`,
`(720,240,1)`, `(960,240,1)`, `(1200,240,1)`.
groups `[[0,1,2],[3,4,5]]`. n1 is interior with pos 360, and
`360 % 240 != 0`, so it gets a backward hook.
n0 `[1:begin]`, n1 `[1:continue, 2:backward hook]`, n2 `[1:end]`,
n3 `[1:begin]`, n4 `[1:continue]`, n5 `[1:end]`.

E11. **6/8, interior forward hook**: `(0,240,1)`, `(240,120,2)`,
`(360,360,1)`. n1 is interior with pos 240, and `240 % 240 == 0`, so it gets a
forward hook.
n0 `[1:begin]`, n1 `[1:continue, 2:forward hook]`, n2 `[1:end]`.

E12. **6/8, six 16ths and a dotted quarter**: `(0..600 step 120, 120, 2)`,
`(720,720,0)`. `S_2 = 720`, so the double beam is continuous.
groups `[[0..5]]`. n0 `[1:begin, 2:begin]`, n1–n4
`[1:continue, 2:continue]`, n5 `[1:end, 2:end]`, n6 `[]`.

E13. **4/4, eight 32nds in beat 1**: `(0..420 step 60, 60, 3)`, then
h `(480,960,0)`, q `(1440,480,0)`. `S_2 = 480` and `S_3 = 240`, so level 3
breaks between n3 (onset 180, level-3 cell 0) and n4 (onset 240, cell 1).
n0 `[1:begin, 2:begin, 3:begin]`, n1 `[1:continue, 2:continue, 3:continue]`,
n2 same as n1, n3 `[1:continue, 2:continue, 3:end]`,
n4 `[1:continue, 2:continue, 3:begin]`, n5 and n6
`[1:continue, 2:continue, 3:continue]`, n7 `[1:end, 2:end, 3:end]`.

E14. **Three levels with a hook**: quarter beat, e `(0,240,1)`,
s. `(240,180,2)`, t `(420,60,3)`.
n0 `[1:begin]`, n1 `[1:continue, 2:begin]`,
n2 `[1:end, 2:end, 3:backward hook]`.

E15. **Rest interrupts (2/4)**: s `(0,120,2)`, R `(120,120,0)`,
s `(240,120,2)`, s `(360,120,2)`. groups `[[2,3]]`.
n0 `[]`, n1 `[]`, n2 `[1:begin, 2:begin]`, n3 `[1:end, 2:end]`.

E16. **2/4, triplet then eighths**: T1 `(0,240,1)`, T1 `(160,240,1)`,
T1 `(320,240,1)`, e `(480,240,1)`, e `(720,240,1)`. The tuplet is its own
group, and beat 0 is not a complete eighth beat, so no merge.
groups `[[0,1,2],[3,4]]`.

E17. **3/8, six 16ths**: `(0..600 step 120, 120, 2)`. G3 gives one beat of
720 with `S_2 = 720`. groups `[[0..5]]`, with levels 1 and 2 both
begin/continue.../end.

E18. **5/8 and 7/8 eighths**: 5/8 with five eighths at 0..960 gives
`[[0,1],[2,3,4]]`. 7/8 with seven eighths at 0..1440 gives
`[[0,1],[2,3],[4,5,6]]`.

E19. **9/16, nine 16ths** `(0..960 step 120, 120, 2)`:
`[[0,1,2],[3,4,5],[6,7,8]]`, each with `1:` and `2:` begin/continue/end.

E20. **2/2, eight 16ths in the first half**: `(0..840 step 120, 120, 2)`,
then h `(960,960,0)`. `S_1 = 960` and `S_2 = 480`.
groups `[[0..7]]`. Level 1: begin, continue x6, end. Level 2: n0 begin,
n1–n2 continue, n3 end, n4 begin, n5–n6 continue, n7 end.

E21. **Huge odd numerator**: N = 4294967295, D = 8. Eighths at units N-3,
N-2, N-1, i.e. rticks 1030792150080, 1030792150320, 1030792150560. All three
lie in the final 3-unit beat, so groups `[[0,1,2]]`. Two eighths at units
N-5 and N-4 (an even pair) form one group.

E22. **Fallback meter 3/7**: eighths at 0, 240, 480, 720 give
`[[0,1],[2,3]]` (quarter beats, G0).

E23. **Overfull 2/4**: six eighths at 0..1200. Bar 0 is two complete eighth
beats, merged into `[0,1,2,3]`. Bar 1 (`k = 1`) has one pair, and its span is
incomplete, so no merge: `[4,5]`.

E24. **Edge**: `[]` gives an empty plan. A single eighth `(0,240,1)` gives
`segments [[]]`, `groups []`. Two eighth rests give `[[],[]]`.

---

## 2. Tuplet bracket: `beam::tuplet_shows_bracket`

### 2.1 Rules

T1. The members of tuplet `pair_id` are the inputs whose `tuplet_id ==
Some(pair_id)`.

T2. When all of a tuplet's notes are joined by one beam, the beam already shows
the grouping. The bracket is omitted and only the number is printed, on the
beam side (Gould, "Tuplets": brackets. Read, "Irregular rhythmic groupings".
Stone, "Tuplets". Wikibooks "Musical notation/Tuplet": "when the notes are
beamed together the bracket may be omitted").

T3. When the tuplet contains unbeamed values (quarters or longer), rests at its
ends, or a beam break, a bracket is required so the extent of the tuplet is
clear (Gould, "Tuplets". Read).

T4. MusicXML leaves the default bracket behaviour implementation-dependent
(MX, `<tuplet>` `bracket` attribute:
https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/tuplet/),
so the writer always states it explicitly from this function.

### 2.2 Decision procedure

1. Collect the member index set M (ascending).
2. If M is empty, return `true` (**[choice]**: a harmless default. Nothing is
   drawn anyway, and a bracket never misstates a tuplet's extent).
3. If some entry of `groups` is exactly equal to M (same indices, no extra
   members), return `false` (number only).
4. Otherwise return `true`.

Equality, not "contains", is required: a beam that also covers non-tuplet notes
would not show the tuplet's extent. (`plan` never builds such a group, B5, but
the function must be correct for any `groups`.)

### 2.3 Edge cases

Empty `notes` or `groups`: `true`. A `pair_id` not present: `true`. A member
that is a rest: it can never be in a group, so `true`. A one-member tuplet: no
group has 1 member, so `true`.

### 2.4 Worked examples

X1. E16's tuplet (ids 0–2 = T1, groups `[[0,1,2],[3,4]]`), pair 1: `false`.
X2. Quarter-note triplet: T1 `(0,480,0)`, T1 `(320,480,0)`, T1 `(640,480,0)`
(written quarters, beams 0), so `groups []`: `true`.
X3. Eighth triplet with a middle rest: T1 e, T1 R, T1 e, with no groups:
`true`.
X4. Eighth triplet ending in a rest: T1 e, T1 e, T1 R, groups `[[0,1]]`. The
group is not equal to {0,1,2}: `true`.
X5. Two adjacent triplets T1 x3, T2 x3, groups `[[0,1,2],[3,4,5]]`: pair 1
gives `false`, pair 2 gives `false`.
X6. Sextuplet of 16ths T7 x6 in one group: `false`.
X7. pair_id 9 absent: `true`.
X8. Hypothetical `groups [[0,1,2,3]]` where index 3 is not a member of T1
(M = {0,1,2}): `true`.

---

## 3. Stems: `abs_step`, `middle_distance`, `note_type_has_stem`, `stem_up`

### 3.1 `abs_step(step, octave)`

`7 * (octave + 1) + index`, computed in `i32`, with C=0, D=1, E=2, F=3, G=4,
A=5, B=6, case-insensitive (contract). **[choice]** An unrecognised step letter
maps to index 0 (C). Reason: the function must be total. C keeps the result
inside the octave the caller gave.

Examples: ('C',4) → 35. ('b',4) → 41. ('G',5) → 46. ('D',3) → 29.
('A',-1) → 5. ('x',4) → 35. (`i8` extremes ('B',127) → 902 and ('C',-128) → -889
do not overflow.)

### 3.2 `middle_distance(clef, step, octave)`

Rules:

S0. The staff has five lines, numbered 1–5 from the bottom, and the middle line
is line 3. Adjacent lines are two diatonic steps apart (Gould, "The staff" /
"Clefs". MX `<clef>` line semantics:
https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/clef/).

S1. A clef fixes one pitch on its line: G clef = G4, F clef = F3, C clef = C4
(Gould, "Clefs". Read, "Clefs").

S2. The middle-line step is `m = ref + 2 * (3 - line)`, with ref = 39 (G4),
31 (F3) or 35 (C4). It comes from `clef_model(clef)` (`sign`, `line`). Results:
treble G/2 → 41 (B4), French violin G/1 → 43 (D5), bass F/4 → 29 (D3),
baritone F/3 → 31 (F3), alto C/3 → 35 (C4), tenor C/4 → 33 (A3), soprano C/1 →
39 (G4), mezzo C/2 → 37 (E4).

S3. Percussion clef (`sign == "percussion"`): positions are read as in treble
clef (MX `<display-step>`: "For percussion clef, the step and octave are read
as if the staff used treble clef", G4 on line 2,
https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/display-step/).
So `m = 41` whatever the line.

S4. **[choice]** An unparseable or out-of-range line ("" or not "1"–"5") uses
the sign's standard line (G→2, F→4, C→3). An unknown sign uses treble
(`m = 41`). Reason: totality, and the contract's `None` means treble.

S5. **[choice]** Octave-transposing clefs (e.g. treble-8vb) are not adjusted.
`clef_model` gives only sign and line, and the note's step/octave are taken as
the staff position the writer gives.

S6. `middle_distance = m - abs_step(step, octave)` (positive = below the middle
line).

Examples: treble B4 → 0. Treble C4 → 6. Treble G5 → -5. Treble A3 → 8. Bass D3 →
0. Bass C4 → -6. Bass G2 → 4. Alto C4 → 0. Alto F3 → 4. Tenor A3 → 0. Tenor C4 →
-2. Percussion (any line) B4 → 0, E4 → 4. `None` clef with 'b',4 → 0.

### 3.3 `note_type_has_stem(note_type)`

N1. Half notes and all shorter values carry stems. Whole notes and breves do
not (Gould, "Stems". Read, "Note values"). True for `"half"`, `"quarter"`,
`"eighth"`, `"16th"`, `"32nd"`, `"64th"`, `"128th"`, `"256th"`, `"512th"`,
`"1024th"`. False for `"whole"`, `"breve"`.

N2. **[choice]** `"long"` and `"maxima"` → false. Their tail is part of the
notehead glyph in modern renderings, not a stem whose direction an engraver
chooses, so a `<stem>` element would be misleading.

N3. **[choice]** Matching is exact (MusicXML note-type values are lowercase,
MX `note-type-value`). Any other string → false. Reason: stating no stem is the
safe default, because the consumer then computes one itself.

Examples: "quarter" → true. "1024th" → true. "half" → true. "whole" → false.
"breve" → false. "long" → false. "" → false. "Quarter" → false.

### 3.4 `stem_up(own, beam, multivoice, slot)`

Rules:

D1. **Single note.** Below the middle line: stem up. Above it: stem down
(Gould, "Stems": direction. Read, "Stems". Ross, "Stems". Dynamic-music-room
and music-theory-practice guides).

D2. **Middle line.** A note on the middle line takes a down stem (Gould,
"Stems". Read). **[choice]** Always down. Gould and Ross allow an up stem when
the neighbouring stems are up, but that context is not in the contract's
inputs.

D3. **Chord.** The note farthest from the middle line decides: if the lowest
note is farther below than the highest note is above, stem up; in the opposite
case, stem down (Gould, "Stems": chords. Read, "Stems". Ross).

D4. **Equidistant outer notes.** The majority of notes on one side decides: more
notes below → up, more above → down. If that also balances, stem down (Gould,
"Stems": chords. Read). **[choice]** The final tie goes down, matching the
middle-line default D2.

D5. **Beamed group.** All stems of a beam group point the same way. The
direction is decided by D3/D4 applied to all notes of the group: its highest
and lowest notes, then the majority count (Gould, "Beams": stem direction of
beamed groups. Read. WP-Beam). The same direction must result for every member
of a group. That is guaranteed because every member is given the same
`beam_distances`.

D6. **Multiple voices on one staff.** Direction depends on the voice, not on
pitch. The upper voice has stems up and the lower voice stems down, so the two
parts stay distinct (Gould, "Two or more parts on a stave". Read, "Multiple
voices". Ross). **[choice]** Slots by parity: even slots (0, 2, 4 ...) up, odd
slots (1, 3, 5 ...) down. Reason: the literature describes two parts per
staff, and for third and fourth parts the common convention is that the
additional upper-type part also points up and the lower-type part down.
Parity extends that to any number of parts.

Decision procedure:
1. If `multivoice`: return `slot % 2 == 0`.
2. Let `ds` = `beam_distances` if it is `Some` and non-empty, else `own`.
3. If `ds` is empty: return `false` (as if a middle-line note).
4. Widen to `i64`. `lo = max(ds)` (the farthest below), `hi = min(ds)`
   (the farthest above). `below = max(lo, 0)`, `above = max(-hi, 0)`.
5. If `below > above`: return `true`. If `above > below`: return `false`.
6. Count `nb` = entries > 0 and `na` = entries < 0. If `nb > na`: `true`.
   Otherwise `false`.

Edge cases: all on the middle line ([0,0]) → down. Extreme values
(`i32::MIN`, `i32::MAX`) are safe through `i64`. `Some(&[])` beam distances
fall back to `own`. `own` empty and `beam` `None` → down.

Worked examples (`own`, `beam`, `mv`, `slot` → result):

| # | own | beam | mv | slot | result | why |
|---|-----|------|----|------|--------|-----|
| ST1 | [6] | None | false | 0 | true | below middle (D1) |
| ST2 | [-5] | None | false | 0 | false | above (D1) |
| ST3 | [0] | None | false | 0 | false | middle line (D2) |
| ST4 | [3,-5] | None | false | 0 | false | top note farther (D3) |
| ST5 | [5,-3] | None | false | 0 | true | bottom note farther (D3) |
| ST6 | [4,-2,-4] | None | false | 0 | false | outer equal, 2 above vs 1 below (D4) |
| ST7 | [4,2,-4] | None | false | 0 | true | outer equal, 2 below (D4) |
| ST8 | [4,-4] | None | false | 0 | false | full balance → down (D4) |
| ST9 | [1] | Some([1,2,-6]) | false | 0 | false | group's farthest is above (D5) |
| ST10 | [-1] | Some([-1,3,4]) | false | 0 | true | group's farthest below (D5) |
| ST11 | [-6] | None | true | 0 | true | upper voice (D6) |
| ST12 | [6] | None | true | 1 | false | lower voice (D6) |
| ST13 | [0] | None | true | 2 | true | parity (D6) |
| ST14 | [0] | None | true | 3 | false | parity (D6) |
| ST15 | [i32::MIN, i32::MAX] | None | false | 0 | false | above = 2147483648 > below = 2147483647 |

---

## 4. Rests in multi-voice staves: `rest_display`

### 4.1 Rules

R1. In a staff carrying several parts, each part's rests are displaced toward
that part's side of the staff: the upper (stem-up) part's rests are raised and
the lower (stem-down) part's rests are lowered. This shows which part rests and
avoids collisions. They may go to the outer staff lines or beyond (Gould, "Two
or more parts on a stave": rests. Read, "Rests". Ross, "Rests").

R2. MusicXML positions a rest with `<display-step>`/`<display-octave>`,
interpreted in the current clef. For percussion clef they are read as treble
(MX `<rest>`, `<display-step>`).

R3. **[choice]** The offset is two staff spaces (four diatonic steps) from the
middle line: the upper part's rest is placed on the top line, the lower part's
on the bottom line. Reason: one space is not enough to clear a second voice
whose notes sit in the middle of the staff, while the outer line keeps
whole/half rests within the staff and avoids ledger lines. This is the smallest
offset that meets R1's goals.

R4. **[choice]** Voice slot parity matches D6: even slot → upper position,
odd slot → lower position.

### 4.2 Decision procedure

1. Compute the middle-line step `m` from `clef_model(clef)` exactly as in §3.2
   (S2–S4).
2. Target `t = m + 4` if `slot` is even, else `t = m - 4`.
3. Convert: `octave = floor(t / 7) - 1`, `index = t mod 7` (Euclidean),
   step letter = "CDEFGAB"[index]. The octave always fits in `i8` for valid
   clefs.

### 4.3 Table (derived from the procedure; for checking)

| clef_model (sign/line) | typical clef | middle | upper (even slot) | lower (odd slot) |
|---|---|---|---|---|
| G/2 | treble (`None`) | B4 | ('F', 5) | ('E', 4) |
| F/4 | bass | D3 | ('A', 3) | ('G', 2) |
| C/3 | alto | C4 | ('G', 4) | ('F', 3) |
| C/4 | tenor | A3 | ('E', 4) | ('D', 3) |
| percussion/any | percussion | B4 (as treble) | ('F', 5) | ('E', 4) |
| C/1 | soprano | G4 | ('D', 5) | ('C', 4) |
| G/1 | French violin | D5 | ('A', 5) | ('G', 4) |

### 4.4 Edge cases and examples

- `slot` huge (`usize::MAX`) → parity decides (odd → lower).
- An unknown or garbled clef model → treble values (S4).
- The function is called only on multi-voice staves (contract). It is still
  defined for slot 0.

Examples: (None, 0) → ('F',5). (None, 1) → ('E',4). (bass, 0) → ('A',3).
(bass, 1) → ('G',2). (alto, 0) → ('G',4). (alto, 1) → ('F',3).
(tenor, 1) → ('D',3). (perc, 0) → ('F',5). (None, 2) → ('F',5).
(None, 3) → ('E',4).

---

## 5. Slurs and ties: `slur_placement`, `tie_orientation`

### 5.1 Slur rules

L1. **Single voice.** A slur goes on the notehead side, opposite the stems:
stems up → slur below, stems down → slur above. When stems are mixed within the
slur, the slur goes above (Gould, "Slurs": position. Read, "Slurs". Ross,
"Slurs"). The contract gives only the starting note's stem, so the
starting-note rule is applied.

L2. **Multiple voices.** Slurs go on the voice's own outer side: the upper part's
slurs above, the lower part's below, whatever the stems (Gould, "Two or more
parts on a stave". Read). The voice is identified by slot parity
(**[choice]**, as D6).

L3. **[choice]** A stemless note (`stem_up == None`) in a single voice →
`"above"`. Reason: the contract gives no pitch to imagine a stem from. A
stemless note is treated like the middle-line default (D2, stem down), which
places the slur above. Above is also the general default slur position
(Gould, "Slurs").

L4. MusicXML `placement` on `<slur>` means above or below the note (MX
`<slur>`: https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/slur/).

Procedure: if `multivoice` → `"above"` if `slot` is even, else `"below"`.
Else `Some(true)` → `"below"`, `Some(false)` → `"above"`, `None` → `"above"`.

### 5.2 Tie rules

K1. **Single voice.** A tie curves away from the stem, on the notehead side:
stem up → tie under the notes (underhand, tips up), stem down → tie over the
notes (overhand, tips down) (Gould, "Ties". Read, "Ties". Ross. WP-Tie: "If the
stem points down, the tie goes above ... if up, below").

K2. **Multiple voices.** Ties go on the stem side, away from the other part:
the upper part (stems up) ties over, the lower part (stems down) ties under
(Gould, "Ties" and "Two or more parts on a stave". WP-Tie: the rule changes
when two voices share a staff). In multi-voice staves the stem direction
follows the voice (D6), so `stem_up` identifies the voice.

K3. **[choice]** Stemless notes (`None`): `"over"` in both single and
multi-voice cases. Reason: with no pitch and no slot available, a whole note
is treated like the middle-line default (stem down → tie over, K1). In
multi-voice this matches the upper part, the more common case for whole notes
held in a top line.

K4. MusicXML `orientation` on `<tied>`: `over` = overhand (tips down), `under` =
underhand (tips up) (MX `<tied>`:
https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/tied/).

K5. Limitation: in chords, the literature curves the outer notes' ties away
from the chord and lets inner ties go either way (Gould, "Ties": chords.
WP-Tie). The contract's single per-note answer gives every tie on the note the
same orientation. A finer rule would need the chord's pitches.

Procedure: if `multivoice` → `Some(true)` → `"over"`, `Some(false)` → `"under"`,
`None` → `"over"`. Else → `Some(true)` → `"under"`, `Some(false)` → `"over"`,
`None` → `"over"`.

### 5.3 Worked examples

| # | function | stem_up | multivoice | slot | result |
|---|---|---|---|---|---|
| SL1 | slur | Some(true) | false | 0 | "below" |
| SL2 | slur | Some(false) | false | 0 | "above" |
| SL3 | slur | None | false | 0 | "above" |
| SL4 | slur | Some(false) | true | 0 | "above" (upper voice wins over stem) |
| SL5 | slur | Some(true) | true | 1 | "below" |
| SL6 | slur | None | true | 3 | "below" |
| SL7 | slur | None | true | 2 | "above" |
| TI1 | tie | Some(true) | false | – | "under" |
| TI2 | tie | Some(false) | false | – | "over" |
| TI3 | tie | None | false | – | "over" |
| TI4 | tie | Some(true) | true | – | "over" |
| TI5 | tie | Some(false) | true | – | "under" |
| TI6 | tie | None | true | – | "over" |

---

## 6. Summary of [choice] decisions

1. Beam counts above 8 are clamped to 8 (MusicXML's maximum) (0.2).
2. Fallback grid for unusable meters: quarter-note beats from the barline (G0).
3. Long-unit simple meters (5/4, 7/4 ...) beam per unit beat (G2).
4. Irregular short-unit meters group as 2s, with a final 3 when the numerator is
   odd (G4).
5. One span-limit rule (`lim(L) = 2880 >> L` over the beat's subdivision chain)
   decides primary group length and secondary-beam breaks (§1.3). 6/4 eighths
   are beamed by the dotted half.
6. Rests are never beamed and always break beams (B2).
7. Tuplets beam as separate units and never merge with neighbours or other
   tuplets (B5). There are no subdivision breaks inside a tuplet (B9).
8. Eighth merging applies only to pure eighth pairs. 3/4 merges only a full bar
   of six eighths (B6).
9. Interior hooks use the position rule: on a subdivision start → forward,
   otherwise backward (B10).
10. `tuplet_shows_bracket` with no members → true. A number only requires an
    exactly equal group (§2.2).
11. An unknown step letter → C. A bad clef line → the sign's standard line.
    An unknown sign → treble. Octave clefs are not adjusted (§3.1, S4, S5).
12. `long`/`maxima` are stemless. Note types match exactly, and unknown types
    → no stem (N2, N3).
13. A middle-line note is always stem down. A full chord/group balance → down
    (D2, D4).
14. Voice slots by parity: even = upper (stems up, raised rests, slurs above),
    odd = lower (D6, R4, L2).
15. Multi-voice rests sit on the top/bottom staff line (±4 steps from the
    middle) (R3).
16. A stemless slur → above. A stemless tie → over (L3, K3).

---

## Sources

- Elaine Gould, *Behind Bars: The Definitive Guide to Music Notation*, Faber
  Music, 2011. Chapters/topics used: "Beams" (beaming in simple, compound and
  irregular metres; half-bar beaming in 4/4; secondary and partial beams; stem
  direction of beamed groups), "Stems" (direction, middle line, chords),
  "Rests", "Two or more parts on a stave", "Ties", "Slurs", "Tuplets",
  "Clefs", "Time signatures". (The book itself was not consulted directly in
  this session. Wordings were checked through public secondary descriptions,
  e.g. forum quotations of the 4/4 and 4/8 half-bar rule.)
- Ted Ross, *The Art of Music Engraving and Processing*, Hansen Books, 1970.
  Sections on stems, beams, rests, ties and slurs.
- Gardner Read, *Music Notation: A Manual of Modern Practice*, 2nd ed., 1969.
  Chapters on clefs, stems, beams, rests, ties and slurs, irregular rhythmic
  groupings.
- Kurt Stone, *Music Notation in the Twentieth Century*, Norton, 1980. Meter
  (irregular meters) and tuplets.
- MusicXML 4.0 reference (W3C Music Notation CG):
  `<beam>` https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/beam ,
  `beam-value` https://www.w3.org/2021/06/musicxml40/musicxml-reference/data-types/beam-value ,
  `<tuplet>` https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/tuplet/ ,
  `<rest>` https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/rest ,
  `<display-step>` https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/display-step/ ,
  `<slur>` https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/slur/ ,
  `<tied>` https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/tied/ ,
  `<clef>` https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/clef/
- Wikipedia, "Beam (music)", https://en.wikipedia.org/wiki/Beam_(music)
- Wikipedia, "Tie (music)", https://en.wikipedia.org/wiki/Tie_(music)
- Wikibooks, "Musical notation / Tuplet" (mirror:
  https://englishgratis.com/1/wikibooks/musicalnotation/tuplet.htm), on omitting
  brackets for beamed tuplets.
- Kent State University, "Beaming Rules",
  https://www.personal.kent.edu/~sbirch/Common/Encyclopedia/Rudiments/beaming_rules.htm
- Stem-direction guides: https://dynamicmusicroom.com/note-stem-direction/ ,
  https://music-theory-practice.com/guides/note-stem-direction
