//! Agent help topics for croma's non-standard ABC notations (the
//! `[I:croma-*]` / `%%croma-*` carriers). Exposed as data so a library user gets
//! the same knowledge the `croma agent` CLI prints, without taking on a JSON or
//! serde dependency — `croma-core` stays zero-dependency.
//!
//! [`docs/carriers.md`] is the canonical spec; these topics are its agent-facing
//! distillation, and a test asserts every carrier there has a topic so the two
//! cannot drift.

/// One agent help topic: a croma notation (or the `syntax` overview), with a
/// copy-paste ABC example in its [`body`](AgentTopic::body).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentTopic {
    /// Short, goal-oriented id (`xvoice-slur`), the primary lookup key.
    pub id: &'static str,
    /// Synonyms — the full `croma-<name>` carrier name and common phrasings.
    pub aliases: &'static [&'static str],
    /// Catalogue group, mirroring `docs/carriers.md` (`Basics`, `Lyrics`, …).
    pub category: &'static str,
    /// One-line description.
    pub summary: &'static str,
    /// The full agent-facing explanation (Markdown): what MusicXML fact it
    /// persists, the syntax, and a runnable ABC example.
    pub body: &'static str,
}

/// Every agent help topic, in catalogue order.
pub fn agent_topics() -> &'static [AgentTopic] {
    TOPICS
}

/// Find a topic by `id` or any alias, case-insensitively.
pub fn find_agent_topic(query: &str) -> Option<&'static AgentTopic> {
    let query = query.trim();
    TOPICS.iter().find(|topic| {
        topic.id.eq_ignore_ascii_case(query)
            || topic
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(query))
    })
}

static TOPICS: &[AgentTopic] = &[
    AgentTopic {
        id: r#"syntax"#,
        aliases: &[r#"carriers"#, r#"carrier"#, r#"annotations"#, r#"notation"#],
        category: r#"Basics"#,
        summary: r#"the carrier convention: the two vehicles, key=value, the -hex= rule"#,
        body: r#"croma round-trips MusicXML through ABC. ABC 2.1 cannot natively express every MusicXML fact, so croma stores those facts in namespaced **carriers** that ride inside the ABC text and are re-applied on the way back to MusicXML. Every other ABC tool ignores them, so the file stays playable in abc2midi / abcm2ps / abcjs while croma keeps full fidelity.

Two vehicles:
- inline `[I:croma-<name> k=v ...]` — anchored to the following note / chord / barline / `[M:]` / `[K:]`. The default; use it for per-note and per-measure facts.
- header `%%croma-<name> ...` — anchored to a voice or the score. Use only for score/voice-level facts.

Fields are `key=value`, space-separated; double-quote a value with spaces (`name="Snare Drum"`). A boolean carrier carries no fields — the bare name is the flag (e.g. `[I:croma-musicxml-forward]`).

The `-hex=` rule: inside an inline `[I:...]`, the characters `]`, `%`, and raw control characters break the ABC tokenizer. When a free-text value contains one, croma emits a hex variant of the field instead — `text="John"` becomes `text-hex=4a6f686e` (the UTF-8 bytes as lowercase hex). Header `%%` lines are line-level and do not need it.

Run `croma agent <topic>` for any specific notation, then `croma xml file.abc` to confirm it persisted."#,
    },
    AgentTopic {
        id: r#"part-instrument"#,
        aliases: &[
            r#"croma-musicxml-instrument"#,
            r#"instrument-def"#,
            r#"midi-instrument"#,
            r#"drum-kit"#,
        ],
        category: r#"Instruments & chord symbols"#,
        summary: r#"a part's declared instrument (name, MIDI channel/program/volume/pan), incl. drum kits"#,
        body: r#"Persists a part's `<score-instrument>` + `<midi-instrument>` — id, name, and MIDI channel/program/volume/pan/midi-unpitched, including percussion kits ABC cannot name.

Vehicle: header `%%croma-musicxml-instrument` (one per declared instrument), placed under the voice's `V:` line.

Example:
```
V:P1
%%croma-musicxml-instrument id="P1-I1" name="Snare Drum" channel=10 midi-unpitched=39
```
Verify: `croma xml f.abc | grep -E '<score-instrument|<midi-instrument'`"#,
    },
    AgentTopic {
        id: r#"note-instrument"#,
        aliases: &[r#"croma-note-instrument"#, r#"per-note-instrument"#],
        category: r#"Instruments & chord symbols"#,
        summary: r#"which declared instrument sounds a single note"#,
        body: r#"Persists a per-note `<instrument id=.../>` reference — which of the part's declared instruments sounds this note (a multi-instrument part, e.g. a drum staff).

Vehicle: inline `[I:croma-note-instrument id="..."]` before the note.

Example: `[I:croma-note-instrument id="P1-I1"]C`

Pair it with `croma agent part-instrument`, which declares the ids.
Verify: `croma xml f.abc | grep '<instrument '`"#,
    },
    AgentTopic {
        id: r#"harmony-text"#,
        aliases: &[r#"croma-harmony-text"#, r#"chord-symbol-text"#],
        category: r#"Instruments & chord symbols"#,
        summary: r#"the printed text/provenance of a chord symbol (<harmony><kind text=...>)"#,
        body: r#"Persists a chord symbol's `<harmony><kind text=...>` provenance: `Textless` (a kind with no printed text) vs `Text(value)` (an explicit printed quality) vs (carrier absent) an ABC-native chord whose text croma rebuilds from the chord string.

Vehicle: inline `[I:croma-harmony-text ...]` immediately before the quoted chord symbol. Free text uses the `-hex=` rule.

Examples:
```
[I:croma-harmony-text text="dim"]"Bdim"B
[I:croma-harmony-text textless=1]"C"C
```
Verify: `croma xml f.abc | grep -A2 '<harmony'`"#,
    },
    AgentTopic {
        id: r#"lyric-extend"#,
        aliases: &[r#"croma-lyric-extend"#, r#"melisma"#, r#"extend"#],
        category: r#"Lyrics"#,
        summary: r#"a same-note melisma <extend/> on the primary syllable of a verse"#,
        body: r#"Persists the primary syllable's same-note `<extend/>` melisma flag for a verse — a syllable held with an extender that `w:` alone cannot mark on that note.

Vehicle: inline `[I:croma-lyric-extend verse=N]` before the note; `w:` still carries the syllable text.

Example: `[I:croma-lyric-extend verse=1]A2`
Verify: `croma xml f.abc | grep '<extend'`"#,
    },
    AgentTopic {
        id: r#"lyric-duplicate"#,
        aliases: &[r#"croma-lyric-duplicate"#, r#"same-note-syllable"#],
        category: r#"Lyrics"#,
        summary: r#"an extra same-note, same-verse syllable that w: cannot spell (hex text)"#,
        body: r#"Persists an additional `<lyric>` syllable on the SAME note and SAME verse — two syllables on one note, which `w:` cannot express. Free text uses the `-hex=` rule.

Vehicle: inline `[I:croma-lyric-duplicate verse=N text="..."]` (optional `extend=1`) before the note.

Example: `[I:croma-lyric-duplicate verse=1 text="John"]C`
Verify: `croma xml f.abc | grep -c '<lyric'`"#,
    },
    AgentTopic {
        id: r#"tempo"#,
        aliases: &[r#"croma-tempo"#, r#"tempo-text"#],
        category: r#"Tempo"#,
        summary: r#"a printed tempo whose <words> text Q: cannot hold (hex text)"#,
        body: r#"Persists a printed/sound tempo whose `<words>` text the ABC `Q:` field cannot hold — role, words, bpm and reference beat. Free text uses the `-hex=` rule.

Vehicle: inline `[I:croma-tempo role=printed ...]` (voice-scoped).

Example: `[I:croma-tempo role=printed text-hex=416c6c6567726f bpm=100 beat-n=1 beat-d=4]`
Verify: `croma xml f.abc | grep -E '<metronome|<words'`"#,
    },
    AgentTopic {
        id: r#"sound-tempo"#,
        aliases: &[r#"croma-sound-tempo"#, r#"playback-tempo"#],
        category: r#"Tempo"#,
        summary: r#"a playback-only tempo (<sound tempo>) with no printed metronome"#,
        body: r#"Persists a playback-only tempo — a MusicXML `<sound tempo=...>` with NO printed metronome — as bpm + reference beat, so the round trip does not print a metronome the source never showed.

Vehicle: inline `[I:croma-sound-tempo bpm=N beat-n=.. beat-d=.. text=".."]` (voice-scoped).

Example: `[I:croma-sound-tempo bpm=80 beat-n=1 beat-d=4 text="80"]`
Verify: `croma xml f.abc | grep '<sound tempo'`"#,
    },
    AgentTopic {
        id: r#"initial-key"#,
        aliases: &[r#"croma-initial-key"#, r#"per-voice-key"#],
        category: r#"Key & meter"#,
        summary: r#"a per-voice initial <key> the header K: cannot encode"#,
        body: r#"Persists a per-voice initial `<key>` that differs from the score `K:` header — fifths plus explicit accidentals the header cannot encode for one voice.

Vehicle: inline `[I:croma-initial-key fifths=N accidentals=STEP:ALTER,...]` at the start of the voice.

Example: `[I:croma-initial-key fifths=0 accidentals=F:1,C:1]`
Verify: `croma xml f.abc | grep -A3 '<key'`"#,
    },
    AgentTopic {
        id: r#"initial-meter"#,
        aliases: &[r#"croma-initial-meter"#, r#"per-voice-meter"#],
        category: r#"Key & meter"#,
        summary: r#"a per-voice initial <time> (display + common/cut symbol) the header M: cannot encode"#,
        body: r#"Persists a per-voice initial `<time>` — its display string and common/cut symbol — that differs from the score `M:` header (e.g. header `M:C|` but this part starts numeric `2/2`).

Vehicle: inline `[I:croma-initial-meter display=".." symbol=common|cut]` at the start of the voice.

Example: `[I:croma-initial-meter display="2/2" symbol=cut]`
Verify: `croma xml f.abc | grep -A3 '<time'`"#,
    },
    AgentTopic {
        id: r#"key-restatement"#,
        aliases: &[r#"croma-key-restatement"#, r#"redundant-key"#],
        category: r#"Key & meter"#,
        summary: r#"flag: the following [K:] is a redundant restatement that must survive ABC dedupe"#,
        body: r#"A flag marking the following inline `[K:]` as a redundant restatement of the already-effective key that must survive ABC's dedupe (the source restated `<key>` even though it was unchanged).

Vehicle: boolean inline `[I:croma-key-restatement]` immediately before the `[K:]`.

Example: `[I:croma-key-restatement] [K:F]`
Verify: `croma xml f.abc | grep -c '<key'`"#,
    },
    AgentTopic {
        id: r#"meter-restatement"#,
        aliases: &[r#"croma-meter-restatement"#, r#"redundant-meter"#],
        category: r#"Key & meter"#,
        summary: r#"flag: the following [M:] is a redundant restatement that must survive ABC dedupe"#,
        body: r#"A flag marking the following inline `[M:]` as a redundant restatement of the already-effective meter that must survive ABC's dedupe.

Vehicle: boolean inline `[I:croma-meter-restatement]` immediately before the `[M:]`.

Example: `[I:croma-meter-restatement] [M:4/4]`
Verify: `croma xml f.abc | grep -c '<time'`"#,
    },
    AgentTopic {
        id: r#"time-symbol"#,
        aliases: &[r#"croma-time-symbol"#, r#"common-cut"#],
        category: r#"Key & meter"#,
        summary: r#"a <time symbol="common|cut"> when the M: display is numeric, not C / C|"#,
        body: r#"Persists `<time symbol="common|cut">` when the meter DISPLAY is numeric (not the ABC `C` / `C|` shorthand) — e.g. a 4/4 printed with the common-time glyph.

Vehicle: a whole-line `%%croma-time-symbol ...` before the header `M:`, or inline `[I:croma-time-symbol symbol=cut]` before a mid-tune `[M:]`.

Example: `[I:croma-time-symbol symbol=cut]`
Verify: `croma xml f.abc | grep '<time symbol'`"#,
    },
    AgentTopic {
        id: r#"forward"#,
        aliases: &[r#"croma-musicxml-forward"#, r#"invisible-rest-gap"#],
        category: r#"Cursor & structure"#,
        summary: r#"flag: re-emit <forward> (a silent cursor advance) for an invisible-rest gap, not a <rest>"#,
        body: r#"A flag telling croma to re-emit the following invisible-rest gap as a MusicXML `<forward>` (a silent cursor advance) rather than a `<note><rest>`.

Vehicle: boolean inline `[I:croma-musicxml-forward]` before the invisible rest (`x`).

Example: `[I:croma-musicxml-forward]x4`
Verify: `croma xml f.abc | grep '<forward'`"#,
    },
    AgentTopic {
        id: r#"sequence-backup"#,
        aliases: &[r#"croma-musicxml-sequence-backup"#, r#"backup"#],
        category: r#"Cursor & structure"#,
        summary: r#"the explicit <backup> duration between two voice sequences in a measure"#,
        body: r#"Persists the explicit `<backup>` duration between two voice sequences in a measure when it overshoots the prior cursor — a backup ABC's voice model cannot otherwise spell.

Vehicle: inline `[I:croma-musicxml-sequence-backup n=NUM d=DEN]` on the first event of the later sequence (voice-scoped).

Example: `[I:croma-musicxml-sequence-backup n=3 d=8]`
Verify: `croma xml f.abc | grep '<backup'`"#,
    },
    AgentTopic {
        id: r#"wide-tuplet"#,
        aliases: &[r#"croma-musicxml-tuplet"#, r#"tuplet"#],
        category: r#"Cursor & structure"#,
        summary: r#"a tuplet whose actual_notes is outside ABC's (p:q:r range (2..=9)"#,
        body: r#"Persists a tuplet whose `actual_notes` is outside ABC's `(p:q:r` range (2..=9) — e.g. an 11:8 — as id, actual, normal and role, so the `<time-modification>` / `<tuplet>` survive.

Vehicle: inline `[I:croma-musicxml-tuplet id=N actual=A normal=B role=start|continue|stop]` on each member.

Example: `[I:croma-musicxml-tuplet id=1 actual=11 normal=8 role=start]C`
Verify: `croma xml f.abc | grep -A2 '<time-modification'`"#,
    },
    AgentTopic {
        id: r#"after-grace"#,
        aliases: &[r#"croma-after-grace"#, r#"trailing-grace"#],
        category: r#"Cursor & structure"#,
        summary: r#"flag: the following {...} grace group is an after-grace bound to the PRECEDING note"#,
        body: r#"A flag marking the following `{...}` grace group as an AFTER-grace bound to the preceding note (a trailing ornament), distinct from an ordinary leading grace.

Vehicle: boolean inline `[I:croma-after-grace]` between the note and its trailing `{...}`.

Example: `C[I:croma-after-grace]{de}`
Verify: `croma xml f.abc | grep '<grace'`"#,
    },
    AgentTopic {
        id: r#"clef-change"#,
        aliases: &[r#"croma-clef-cursor"#, r#"mid-tune-clef"#],
        category: r#"Cursor & structure"#,
        summary: r#"a mid-tune <clef> wrapped in <backup>/<forward> (clef text + cursor durations; hex)"#,
        body: r#"Persists a mid-tune `<clef>` that the source wrapped in `<backup>`/`<forward>` cursor moves — the clef text plus the back / pre-back durations. Clef text uses the `-hex=` rule when hostile.

Vehicle: inline `[I:croma-clef-cursor clef=".." back-n=.. back-d=.. [pre-back-n=.. pre-back-d=..]]`.

Example: `[I:croma-clef-cursor clef="bass" back-n=1 back-d=4]`
Verify: `croma xml f.abc | grep '<clef'`"#,
    },
    AgentTopic {
        id: r#"barline-style"#,
        aliases: &[r#"croma-barline-style"#, r#"dashed-barline"#],
        category: r#"Cursor & structure"#,
        summary: r#"a dashed barline (<bar-style>dashed</bar-style>) — ABC has no glyph"#,
        body: r#"Persists a barline style ABC has no glyph for — currently `dashed` (`<bar-style>dashed</bar-style>`).

Vehicle: inline `[I:croma-barline-style style=dashed]` before the barline.

Example: `[I:croma-barline-style style=dashed] |`
Verify: `croma xml f.abc | grep '<bar-style'`"#,
    },
    AgentTopic {
        id: r#"measure-number"#,
        aliases: &[r#"croma-measure-number"#, r#"pickup-number"#],
        category: r#"Cursor & structure"#,
        summary: r#"a <measure number=...> that differs from croma's canonical 1-based index (hex)"#,
        body: r#"Persists a `<measure number=...>` that differs from croma's canonical 1-based index — a pickup `0`, a repeated movement-local number, or a label like `X1`. Odd values use the `-hex=` rule.

Vehicle: inline `[I:croma-measure-number n=...]` at the start of the measure.

Example: `[I:croma-measure-number n=0]`
Verify: `croma xml f.abc | grep '<measure number'`"#,
    },
    AgentTopic {
        id: r#"ending-close"#,
        aliases: &[r#"croma-ending-close"#, r#"volta-close"#],
        category: r#"Cursor & structure"#,
        summary: r#"an explicit volta-bracket close (<ending type="stop|discontinue">)"#,
        body: r#"Persists an explicit volta-bracket close — `<ending type="stop|discontinue">` plus its side (location) and numbers — that ABC's repeat syntax does not spell on its own.

Vehicle: inline `[I:croma-ending-close type=stop|discontinue location=right number=".."]`.

Example: `[I:croma-ending-close type=discontinue location=right number="1,2"]`
Verify: `croma xml f.abc | grep '<ending'`"#,
    },
    AgentTopic {
        id: r#"xvoice-slur"#,
        aliases: &[
            r#"croma-xvoice-slur"#,
            r#"slur-across-voices"#,
            r#"cross-voice-slur"#,
        ],
        category: r#"Cursor & structure"#,
        summary: r#"a slur whose start and stop are in different voices"#,
        body: r#"A `<slur>` whose start and stop are in different voices cannot be spelled with ABC `(`/`)`, which pair within one `V:` stream. Carry each end instead.

Syntax: `[I:croma-xvoice-slur pair=N role=start|stop]` on the note at each end. `pair` is a shared id that re-pairs the two ends across voices; `role` says which end.

Example (slur from voice 1 into voice 2):
```
%%score (1 2)
V:1
[I:croma-xvoice-slur pair=1 role=start]C D E F |
V:2
[I:croma-xvoice-slur pair=1 role=stop]G A B z |
```
Becomes `<slur type="start"/>` on C (voice 1) and `<slur type="stop"/>` on G (voice 2).
Verify: `croma xml f.abc | grep '<slur'`"#,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topics_are_nonempty_and_have_a_syntax_overview() {
        assert!(!agent_topics().is_empty());
        assert_eq!(find_agent_topic("syntax").map(|t| t.id), Some("syntax"));
    }

    #[test]
    fn find_matches_id_and_alias_case_insensitively() {
        let by_alias = find_agent_topic("CROMA-XVOICE-SLUR")
            .expect("the croma-xvoice-slur alias must resolve");
        assert_eq!(by_alias.id, "xvoice-slur");
        assert!(find_agent_topic("no-such-topic").is_none());
    }

    #[test]
    fn carrier_topics_show_their_inline_or_header_syntax() {
        for topic in agent_topics() {
            if topic.aliases.iter().any(|a| a.starts_with("croma-")) {
                assert!(
                    topic.body.contains("[I:croma-") || topic.body.contains("%%croma-"),
                    "topic `{}` must show a copy-paste carrier example",
                    topic.id
                );
            }
        }
    }

    /// Anti-drift: every carrier in `docs/carriers.md` must have a topic, so
    /// adding a carrier forces a topic here.
    #[test]
    fn every_carrier_in_the_spec_has_a_topic() {
        let spec = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/carriers.md"
        ))
        .expect("docs/carriers.md must be readable");
        let carriers: Vec<String> = spec
            .lines()
            .filter_map(|line| {
                let rest = line.trim_start().strip_prefix("| `croma-")?;
                Some(format!("croma-{}", rest.split('`').next()?))
            })
            .collect();
        assert!(
            carriers.len() >= 20,
            "expected catalogue carriers; got {carriers:?}"
        );

        let haystack = agent_topics()
            .iter()
            .map(|t| format!("{} {} {}", t.id, t.aliases.join(" "), t.body))
            .collect::<Vec<_>>()
            .join("\n");
        for carrier in carriers {
            assert!(
                haystack.contains(&carrier),
                "carrier `{carrier}` from docs/carriers.md has no agent topic"
            );
        }
    }
}
