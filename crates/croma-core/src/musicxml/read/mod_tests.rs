//! Stage-S1 reader tests.
//!
//! Two layers, mirroring the design's verification plan:
//!
//! 1. **Per-element unit tests** (hard asserts): each TDD'd against one element
//!    class, asserting BOTH the XML re-emission idempotence
//!    `write(read(write(score))) == write(score)` AND a reconstructed model
//!    field directly (so the test fails loudly if the reader builds a Score that
//!    happens to re-write the same bytes for the wrong reason).
//! 2. **Corpus measurement** (env-gated, mirrors `croma-fmt`'s `corpus_proof`):
//!    walks the 10k corpus, runs the idempotence loop per file, counts
//!    idempotent files and tallies the first diverging XML tag. It asserts no
//!    hard count for S1 (most files use later-stage elements) — it reports so
//!    the orchestrator can read the number.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::model::{Fraction, Score, TimedEventKind};
use crate::musicxml::read::read_musicxml;
use crate::musicxml::write_score_partwise;
use crate::{export_musicxml, write_musicxml};

/// `export_musicxml` for ABC that is expected to lower cleanly.
fn export(abc: &str) -> String {
    export_musicxml(abc)
        .unwrap_or_else(|error| panic!("ABC should export: {error:?}"))
        .musicxml
}

/// The S1 round-trip: ABC -> X1 -> read -> X2. Returns `(x1, x2, score)`.
fn round_trip(abc: &str) -> (String, String, Score) {
    let x1 = export(abc);
    let report = read_musicxml(&x1);
    let x2 = write_score_partwise(&report.value).value;
    (x1, x2, report.value)
}

/// Remove the `<key>...</key>` and `<time>...</time>` sub-blocks that the writer
/// always emits inside the first `<attributes>`. These are **stage S2** (the
/// reader does not reconstruct `<key>`/`<time>` yet), and every ABC tune
/// necessarily carries a `K:` (and usually an `M:`), so a header-attribute-free
/// fixture is impossible via the ABC path. Stripping exactly these two deferred
/// blocks scopes the unit-test idempotence assertion to the **S1-supported
/// subset** — note/measure/duration/metadata reconstruction — which is the
/// design's "0 diffs on the supported subset" contract. The corpus measurement
/// below deliberately keeps the FULL, unmodified byte comparison so the reported
/// idempotent count stays honest.
fn strip_stage2_attributes(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut skip_until: Option<&'static str> = None;
    for line in xml.lines() {
        if let Some(close) = skip_until {
            if line.trim() == close {
                skip_until = None;
            }
            continue;
        }
        match line.trim() {
            "<key>" => skip_until = Some("</key>"),
            line_trimmed if line_trimmed == "<time>" || line_trimmed.starts_with("<time ") => {
                skip_until = Some("</time>");
            }
            _ => {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    out
}

/// Assert the re-emission is byte-identical on the S1-supported subset (see
/// [`strip_stage2_attributes`]) and return the reconstructed score for direct
/// model-field assertions.
fn assert_idempotent(abc: &str) -> Score {
    let (x1, x2, score) = round_trip(abc);
    assert_eq!(
        strip_stage2_attributes(&x1),
        strip_stage2_attributes(&x2),
        "write(read(write(score))) must equal write(score) on the S1-supported subset"
    );
    score
}

/// Remove only the **stage S3+** writer blocks (`<score-instrument>` /
/// `<midi-instrument>`) so an S2 idempotence assertion keeps `<key>`/`<time>`/
/// `<clef>`/`<transpose>` in the byte comparison — those are reconstructed now —
/// while still ignoring the part-list MIDI that S3 will own.
fn strip_stage3_blocks(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut skip_until: Option<&'static str> = None;
    for line in xml.lines() {
        if let Some(close) = skip_until {
            if line.trim() == close {
                skip_until = None;
            }
            continue;
        }
        let trimmed = line.trim();
        if trimmed.starts_with("<score-instrument") {
            skip_until = Some("</score-instrument>");
        } else if trimmed.starts_with("<midi-instrument") {
            skip_until = Some("</midi-instrument>");
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Assert the re-emission is byte-identical on the **S2-supported subset**: the
/// full header `<attributes>` (`<key>`/`<time>`/`<clef>`/`<transpose>`) must
/// survive verbatim; only the still-deferred S3 part-list MIDI is stripped.
fn assert_idempotent_s2(abc: &str) -> Score {
    let (x1, x2, score) = round_trip(abc);
    assert_eq!(
        strip_stage3_blocks(&x1),
        strip_stage3_blocks(&x2),
        "write(read(write(score))) must equal write(score) on the S2-supported subset"
    );
    score
}

fn first_note_pitch(score: &Score) -> &crate::model::Pitch {
    match &score.parts[0].voices[0].events[0].kind {
        TimedEventKind::Note(note) => &note.pitch,
        other => panic!("expected first event to be a note, got {other:?}"),
    }
}

#[test]
fn malformed_xml_is_total_and_diagnosed() {
    let report = read_musicxml("<score-partwise><part></broken>");
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with("musicxml.read")),
        "a malformed document must yield a reader diagnostic, got {:?}",
        report.diagnostics
    );
    // Totality: an empty/minimal Score, no panic.
    assert!(
        report.value.parts.is_empty() || report.value.parts.iter().all(|p| p.voices.len() <= 1)
    );
}

#[test]
fn non_partwise_root_is_diagnosed() {
    let report = read_musicxml("<?xml version=\"1.0\"?><score-timewise></score-timewise>");
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "musicxml.read.unsupported_root"),
        "a non-partwise root must warn, got {:?}",
        report.diagnostics
    );
}

#[test]
fn single_note_round_trips_and_reconstructs_pitch() {
    let score = assert_idempotent("X:1\nT:One\nL:1/4\nK:C\nC\n");
    let pitch = first_note_pitch(&score);
    assert_eq!(pitch.step, 'C');
    assert_eq!(pitch.octave, 4);
    assert_eq!(pitch.alter, 0);
    assert_eq!(
        score.parts[0].voices[0].events[0].duration,
        Fraction::new(1, 4),
        "a quarter note must reconstruct as 1/4"
    );
}

#[test]
fn altered_pitch_reconstructs_alter_and_written_accidental() {
    // ^F is F-sharp: <alter>1</alter> (sounding pitch +1) plus an explicit
    // <accidental>sharp</accidental> the reader reconstructs as a written mark.
    let score = assert_idempotent("X:1\nT:Sharp\nL:1/4\nK:C\n^F\n");
    let pitch = first_note_pitch(&score);
    assert_eq!(pitch.step, 'F');
    assert_eq!(pitch.alter, 1);
    let written = match &score.parts[0].voices[0].events[0].kind {
        TimedEventKind::Note(note) => note.written_accidental,
        other => panic!("expected a note, got {other:?}"),
    };
    let written = written.expect("an explicit ^ accidental must reconstruct a written mark");
    assert_eq!(written.kind, crate::model::Accidental::Sharp);
    assert!(written.explicit);
}

#[test]
fn rest_round_trips_and_reconstructs_kind() {
    let score = assert_idempotent("X:1\nT:Rest\nL:1/4\nK:C\nz\n");
    assert!(
        matches!(
            score.parts[0].voices[0].events[0].kind,
            TimedEventKind::Rest(_)
        ),
        "a z rest must reconstruct as a Rest event"
    );
}

#[test]
fn dotted_and_typed_durations_round_trip() {
    // C3/2 under L:1/4 is a dotted quarter (<type>quarter</type><dot/>);
    // the following note keeps the bar honest.
    let score = assert_idempotent("X:1\nT:Dotted\nM:3/4\nL:1/4\nK:C\nC3/2 D/2 E\n");
    assert_eq!(
        score.parts[0].voices[0].events[0].duration,
        Fraction::new(3, 8),
        "C3/2 under L:1/4 is 3/8 of a whole note"
    );
}

#[test]
fn multi_measure_round_trips_with_measure_ids() {
    let score = assert_idempotent("X:1\nT:Two Bars\nM:4/4\nL:1/4\nK:C\nC D E F | G A B c |\n");
    let measures = &score.parts[0].voices[0].measures;
    assert_eq!(measures.len(), 2, "two written bars -> two measures");
    assert_eq!(measures[0].id.number, 1);
    assert_eq!(measures[1].id.number, 2);
    // 8 quarter-note events spread over the two bars.
    assert_eq!(score.parts[0].voices[0].events.len(), 8);
}

#[test]
fn whole_measure_rest_reconstructs_measure_attribute() {
    // A bar that is exactly one rest long emits <rest measure="yes">; the
    // reader must reproduce it (so the attribute survives the round-trip).
    let x1 = export("X:1\nT:Full Rest\nM:4/4\nL:1/4\nK:C\nC D E F | z4 |\n");
    assert!(
        x1.contains("measure=\"yes\""),
        "precondition: a full-bar rest must emit measure=\"yes\""
    );
    let score = assert_idempotent("X:1\nT:Full Rest\nM:4/4\nL:1/4\nK:C\nC D E F | z4 |\n");
    // The second measure's single rest reconstructs with expected==actual.
    let second = &score.parts[0].voices[0].measures[1];
    assert_eq!(second.expected_duration, Some(Fraction::new(1, 1)));
    assert_eq!(second.actual_duration, Fraction::new(1, 1));
}

#[test]
fn invisible_rest_round_trips() {
    // `x` is an invisible rest -> <note print-object="no"><rest/>.
    let abc = "X:1\nT:Hidden\nM:4/4\nL:1/4\nK:C\nC x C x |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("print-object=\"no\""),
        "precondition: invisible rest emits print-object=\"no\""
    );
    assert_idempotent(abc);
}

#[test]
fn multi_part_skeleton_round_trips_with_names() {
    // Two %%score voices -> two parts; each part-name must survive the
    // round-trip and the reader must rebuild both parts with their names.
    let abc = "X:1\nT:Duet\nM:4/4\nL:1/4\n%%score (V1 V2)\nV:1 name=\"Flute\"\nV:2 name=\"Cello\"\nK:C\n[V:1] C D E F |\n[V:2] G,2 A,2 |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<part-name>Flute</part-name>") && x1.contains("<part-name>Cello</part-name>"),
        "precondition: both part-names are emitted"
    );
    let score = assert_idempotent(abc);
    assert_eq!(score.parts.len(), 2, "two %%score voices -> two parts");
    let names: Vec<_> = score
        .parts
        .iter()
        .filter_map(|part| part.name.as_ref().map(|line| line.text.clone()))
        .collect();
    assert_eq!(names, vec!["Flute".to_owned(), "Cello".to_owned()]);
}

#[test]
fn backup_forward_durations_are_read() {
    // A multi-voice bar forces a <backup> between the two voices; reading the
    // backup duration keeps onsets aligned so the bar re-emits identically.
    let abc = "X:1\nT:Backup\nM:4/4\nL:1/4\n%%score (V1 V2)\nV:1\nV:2\nK:C\n[V:1] C2 D2 |\n[V:2] E2 F2 |\n";
    let x1 = export(abc);
    if x1.contains("<backup>") {
        // The reader must at least not panic and must read the duration; full
        // multi-voice idempotence is S6, so we only assert totality here.
        let report = read_musicxml(&x1);
        assert!(
            !report.value.parts.is_empty(),
            "backup/forward parsing must still yield parts"
        );
    }
    // A single-voice file with a leading rest exercises forward-free onset
    // bookkeeping and must be idempotent.
    assert_idempotent("X:1\nT:Lead\nM:4/4\nL:1/4\nK:C\nz C2 z |\n");
}

#[test]
fn divisions_are_recovered() {
    // Mixed durations push divisions above 1; the reader must read the emitted
    // <divisions> so it can invert <duration>.
    let score = assert_idempotent("X:1\nT:Div\nM:4/4\nL:1/8\nK:C\nC2 D/2 E/2 F G A |\n");
    assert!(
        score.divisions >= 1,
        "divisions must be a positive integer, got {}",
        score.divisions
    );
    // Round-trip a 1/16-ish note: ensure the reconstructed duration is exact.
    let first = &score.parts[0].voices[0].events[0];
    assert_eq!(first.duration, Fraction::new(1, 4), "C2 under L:1/8 is 1/4");
}

// --- Stage S2: <attributes> (key / time / clef / transpose) ----------------

/// The reconstructed header key (`score.metadata.key`).
fn header_key(score: &Score) -> &crate::model::KeySignatureModel {
    score
        .metadata
        .key
        .as_ref()
        .expect("S2 must reconstruct the header <key> into score.metadata.key")
}

#[test]
fn header_key_fifths_sharp_round_trips() {
    // K:D is two sharps -> <fifths>2</fifths>.
    let score = assert_idempotent_s2("X:1\nT:Sharps\nL:1/4\nK:D\nD\n");
    assert_eq!(header_key(&score).fifths, 2, "K:D is 2 sharps");
    assert!(
        header_key(&score).explicit_accidentals.is_empty(),
        "a traditional key emits no explicit key accidentals"
    );
}

#[test]
fn header_key_fifths_flat_round_trips() {
    // K:F is one flat -> <fifths>-1</fifths>.
    let score = assert_idempotent_s2("X:1\nT:Flats\nL:1/4\nK:F\nF\n");
    assert_eq!(header_key(&score).fifths, -1, "K:F is 1 flat");
}

#[test]
fn header_key_fifths_zero_round_trips() {
    // K:C is no accidentals -> <fifths>0</fifths>.
    let score = assert_idempotent_s2("X:1\nT:Natural\nL:1/4\nK:C\nC\n");
    assert_eq!(header_key(&score).fifths, 0, "K:C is 0 fifths");
}

#[test]
fn header_key_minor_negative_fifths_round_trips() {
    // K:Cm is three flats -> <fifths>-3</fifths>.
    let score = assert_idempotent_s2("X:1\nT:Minor\nL:1/4\nK:Cm\nC\n");
    assert_eq!(header_key(&score).fifths, -3, "K:Cm is 3 flats");
}

#[test]
fn explicit_key_accidentals_round_trip() {
    // K:C exp ^f _b emits two (key-step, key-alter, key-accidental) triples that
    // the reader must rebuild into explicit_accidentals, preserving order.
    let abc = "X:1\nT:Exp\nL:1/4\nK:C exp ^f _b\nC\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<key-step>F</key-step>")
            && x1.contains("<key-accidental>flat</key-accidental>"),
        "precondition: explicit key accidentals are emitted"
    );
    let score = assert_idempotent_s2(abc);
    let accidentals = &header_key(&score).explicit_accidentals;
    assert_eq!(accidentals.len(), 2, "two explicit key accidentals");
    assert_eq!(accidentals[0].step, 'F');
    assert_eq!(accidentals[0].accidental, crate::model::Accidental::Sharp);
    assert_eq!(accidentals[1].step, 'B');
    assert_eq!(accidentals[1].accidental, crate::model::Accidental::Flat);
}

#[test]
fn header_meter_round_trips_and_reconstructs() {
    // M:6/8 -> <time><beats>6</beats><beat-type>8</beat-type>.
    let score = assert_idempotent_s2("X:1\nT:Compound\nM:6/8\nL:1/8\nK:C\nC2C2C2\n");
    let meter = score
        .metadata
        .meter
        .as_ref()
        .expect("S2 must reconstruct the header <time> into score.metadata.meter");
    assert_eq!(
        meter.display, "6/8",
        "reconstructed meter display drives re-emission"
    );
    assert!(!meter.free_meter);
}

#[test]
fn common_time_symbol_round_trips() {
    // M:C emits <time symbol="common">; the reconstructed meter must re-emit it.
    let abc = "X:1\nT:Common\nM:C\nL:1/4\nK:C\nCCCC\n";
    let x1 = export(abc);
    assert!(
        x1.contains("symbol=\"common\""),
        "precondition: M:C emits symbol=\"common\""
    );
    let score = assert_idempotent_s2(abc);
    let meter = score.metadata.meter.as_ref().expect("meter present");
    assert_eq!(meter.display, "C");
}

#[test]
fn cut_time_symbol_round_trips() {
    // M:C| emits <time symbol="cut">.
    let abc = "X:1\nT:Cut\nM:C|\nL:1/4\nK:C\nCCCC\n";
    let x1 = export(abc);
    assert!(
        x1.contains("symbol=\"cut\""),
        "precondition: M:C| emits symbol=\"cut\""
    );
    let score = assert_idempotent_s2(abc);
    assert_eq!(score.metadata.meter.as_ref().expect("meter").display, "C|");
}

#[test]
fn free_meter_round_trips() {
    // M:none emits NO <time>; both None and Some(free) meter re-emit nothing, so
    // idempotence holds with the reader leaving meter unset for an absent <time>.
    let abc = "X:1\nT:Free\nM:none\nL:1/4\nK:C\nCCCC\n";
    let x1 = export(abc);
    assert!(
        !x1.contains("<time"),
        "precondition: M:none emits no <time> element"
    );
    assert_idempotent_s2(abc);
}

#[test]
fn bass_clef_round_trips() {
    // clef=bass -> <sign>F</sign><line>4</line>; the reader must populate the
    // staff voice's initial_properties.clef so write_clefs re-emits F/4.
    let abc = "X:1\nT:Bass\nL:1/4\nK:C clef=bass\nC,\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<sign>F</sign>"),
        "precondition: clef=bass emits sign F"
    );
    assert_idempotent_s2(abc);
}

#[test]
fn alto_clef_round_trips() {
    // clef=alto -> <sign>C</sign><line>3</line>.
    let abc = "X:1\nT:Alto\nL:1/4\nK:C clef=alto\nC\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<sign>C</sign>"),
        "precondition: clef=alto emits sign C"
    );
    assert_idempotent_s2(abc);
}

#[test]
fn octave_clef_round_trips() {
    // clef=treble-8 -> G/2 plus <clef-octave-change>-1</clef-octave-change>.
    let abc = "X:1\nT:Octave\nL:1/4\nK:C clef=treble-8\nC\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<clef-octave-change>-1</clef-octave-change>"),
        "precondition: clef=treble-8 emits octave-change -1"
    );
    assert_idempotent_s2(abc);
}

#[test]
fn default_treble_clef_round_trips() {
    // No clef= -> the default <sign>G</sign><line>2</line> with no octave change.
    let score = assert_idempotent_s2("X:1\nT:Treble\nL:1/4\nK:C\nC\n");
    // The reconstructed staff voice carries a clef whose text maps back to G/2.
    assert!(
        !score.parts.is_empty(),
        "default clef file must still reconstruct a part"
    );
}

#[test]
fn midi_transpose_reconstructs_chromatic() {
    // %%MIDI transpose -12 -> <transpose><chromatic>-12</chromatic>; the reader
    // reconstructs voice.midi_transpose so re-emission reproduces it.
    let abc = "X:1\nT:Trans\nL:1/4\nK:C\n%%MIDI transpose -12\nC\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<chromatic>-12</chromatic>"),
        "precondition: %%MIDI transpose -12 emits chromatic -12"
    );
    let score = assert_idempotent_s2(abc);
    assert_eq!(
        score.parts[0].voices[0].midi_transpose,
        Some(-12),
        "midi_transpose must reconstruct the chromatic value"
    );
}

#[test]
fn positive_midi_transpose_reconstructs() {
    let abc = "X:1\nT:Up\nL:1/4\nK:C\n%%MIDI transpose 7\nC\n";
    let score = assert_idempotent_s2(abc);
    assert_eq!(score.parts[0].voices[0].midi_transpose, Some(7));
}

// --- Stage S3: <part-list> MIDI instruments (closes the %%MIDI loop) ---------

/// Assert FULL-byte idempotence — S3 reconstructs the `<part-list>`
/// `<score-instrument>`/`<midi-instrument>` blocks, so nothing is stripped any
/// more for a single-voice file. This is the closed-loop gate the stage exists
/// for. Returns the reconstructed score for direct `midi_instrument` asserts.
fn assert_idempotent_s3(abc: &str) -> Score {
    let (x1, x2, score) = round_trip(abc);
    assert_eq!(
        x1, x2,
        "write(read(write(score))) must equal write(score) byte-for-byte (S3 full loop)"
    );
    score
}

/// The reconstructed first voice's MIDI instrument.
fn first_midi(score: &Score) -> crate::model::MidiInstrumentModel {
    score.parts[0].voices[0]
        .midi_instrument
        .expect("S3 must reconstruct voice.midi_instrument from the part-list")
}

#[test]
fn midi_program_reconstructs_zero_based() {
    // %%MIDI program 73 -> <midi-program>74</midi-program> (1-based); the reader
    // must invert to the 0-based program 73, regenerating the same GM name +
    // <midi-program> on re-write.
    let abc = "X:1\nT:P\nL:1/4\nK:C\n%%MIDI program 73\nC\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<midi-program>74</midi-program>")
            && x1.contains("<instrument-name>flute</instrument-name>"),
        "precondition: program 73 emits 1-based 74 + GM name flute"
    );
    let midi = first_midi(&assert_idempotent_s3(abc));
    assert_eq!(
        midi.program,
        Some(73),
        "<midi-program>74 inverts to 0-based 73"
    );
    assert_eq!(midi.channel, None);
    assert_eq!(midi.volume_cc, None);
    assert_eq!(midi.pan_cc, None);
}

#[test]
fn midi_program_with_channel_reconstructs_both() {
    // The two-integer `program <chan> <prog>` form -> <midi-channel> + 1-based
    // <midi-program>; the reader recovers channel and 0-based program.
    let abc = "X:1\nT:PC\nL:1/4\nK:C\n%%MIDI program 1 56\nC\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<midi-channel>1</midi-channel>")
            && x1.contains("<midi-program>57</midi-program>"),
        "precondition: program 1 56 emits channel 1 + 1-based program 57"
    );
    let midi = first_midi(&assert_idempotent_s3(abc));
    assert_eq!(midi.program, Some(56));
    assert_eq!(midi.channel, Some(1));
}

#[test]
fn standalone_channel_reconstructs_with_no_program() {
    // A standalone `%%MIDI channel 10` emits ONLY <midi-channel> and falls back
    // to the PART NAME for <instrument-name> (program is None). The reader must
    // leave program = None so re-write reproduces the part-name fallback, not a
    // GM name.
    let abc = "X:1\nT:Ch\nL:1/4\nK:C\n%%MIDI channel 10\nC\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<midi-channel>10</midi-channel>")
            && !x1.contains("<midi-program>")
            && x1.contains("<instrument-name>Ch</instrument-name>"),
        "precondition: standalone channel emits no program + part-name fallback"
    );
    let midi = first_midi(&assert_idempotent_s3(abc));
    assert_eq!(
        midi.program, None,
        "no program -> re-write must use the part name"
    );
    assert_eq!(midi.channel, Some(10));
}

#[test]
fn control7_volume_reconstructs_cc() {
    // %%MIDI control 7 64 -> <volume>50.39</volume>; the reader must invert the
    // float back to the exact integer CC 64 (round(50.39 * 1.27) == 64).
    let abc = "X:1\nT:Vol\nL:1/4\nK:C\n%%MIDI control 7 64\nC\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<volume>50.39</volume>"),
        "precondition: control 7 64 emits volume 50.39"
    );
    let midi = first_midi(&assert_idempotent_s3(abc));
    assert_eq!(midi.volume_cc, Some(64), "<volume>50.39 inverts to CC 64");
    assert_eq!(midi.program, None);
}

#[test]
fn control10_pan_reconstructs_cc() {
    // %%MIDI control 10 64 -> <pan>0.71</pan>; inverse round((0.71+90)*127/180)==64.
    let abc = "X:1\nT:Pan\nL:1/4\nK:C\n%%MIDI control 10 64\nC\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<pan>0.71</pan>"),
        "precondition: control 10 64 emits pan 0.71"
    );
    let midi = first_midi(&assert_idempotent_s3(abc));
    assert_eq!(midi.pan_cc, Some(64), "<pan>0.71 inverts to CC 64");
}

#[test]
fn pan_extremes_reconstruct() {
    // CC 0 -> <pan>-90.00</pan>, CC 127 -> <pan>90.00</pan>; the boundary values
    // must invert exactly (no off-by-one at the signed extremes).
    let low = "X:1\nT:PanLo\nL:1/4\nK:C\n%%MIDI control 10 0\nC\n";
    assert_eq!(first_midi(&assert_idempotent_s3(low)).pan_cc, Some(0));
    let high = "X:1\nT:PanHi\nL:1/4\nK:C\n%%MIDI control 10 127\nC\n";
    assert_eq!(first_midi(&assert_idempotent_s3(high)).pan_cc, Some(127));
}

#[test]
fn full_midi_instrument_reconstructs_all_fields() {
    // program + channel + CC7 + CC10 in one <midi-instrument>; every field must
    // round-trip and reconstruct, and the whole document must be byte-identical.
    let abc = "X:1\nT:All\nL:1/4\nK:C\n%%MIDI program 1 56\n%%MIDI control 7 100\n%%MIDI control 10 30\nC\n";
    let midi = first_midi(&assert_idempotent_s3(abc));
    assert_eq!(midi.program, Some(56));
    assert_eq!(midi.channel, Some(1));
    assert_eq!(midi.volume_cc, Some(100));
    assert_eq!(midi.pan_cc, Some(30));
}

#[test]
fn inline_midi_program_reconstructs_like_line_start() {
    // The inline `[I:MIDI=program N]` form projects identically to the line-start
    // directive; the reader inverts it the same way (closing the inline loop).
    let abc = "X:1\nT:Inl\nL:1/4\nK:C\n[I:MIDI=program 40]C\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<midi-program>41</midi-program>"),
        "precondition: inline program 40 emits 1-based 41"
    );
    let midi = first_midi(&assert_idempotent_s3(abc));
    assert_eq!(
        midi.program,
        Some(40),
        "inline program 40 inverts to 0-based 40"
    );
}

#[test]
fn no_midi_directive_leaves_instrument_none() {
    // A file with no %%MIDI must not fabricate a midi_instrument (the writer
    // emits no part-list instrument for it).
    let score = assert_idempotent_s3("X:1\nT:Plain\nL:1/4\nK:C\nC\n");
    assert_eq!(
        score.parts[0].voices[0].midi_instrument, None,
        "no %%MIDI -> no reconstructed instrument"
    );
}

#[test]
fn float_cc_round_trip_is_stable_for_every_value() {
    // Design §9 (REQUIRED): the writer formats <volume> as `{:.2}` of cc/1.27 and
    // <pan> as `{:.2}` of cc/127*180-90; the reader inverts with round(v*1.27)
    // and round((p+90)*127/180). Prove the round-trip recovers the EXACT integer
    // CC for every cc in 0..=127, for both volume and pan. This is what makes
    // <volume>/<pan> idempotent under the closed loop.
    for cc in 0u8..=127 {
        let v: f64 = format!("{:.2}", f64::from(cc) / 1.27)
            .parse()
            .expect("formatted volume must parse as f64");
        let back = (v * 1.27).round();
        assert_eq!(
            back,
            f64::from(cc),
            "volume CC {cc}: round({v} * 1.27) = {back}, expected {cc}"
        );

        let p: f64 = format!("{:.2}", f64::from(cc) / 127.0 * 180.0 - 90.0)
            .parse()
            .expect("formatted pan must parse as f64");
        let back = ((p + 90.0) * 127.0 / 180.0).round();
        assert_eq!(
            back,
            f64::from(cc),
            "pan CC {cc}: round(({p} + 90) * 127 / 180) = {back}, expected {cc}"
        );
    }
}

// --- Stage S4: <notations> + <time-modification> -----------------------------

/// Assert FULL-byte idempotence on an S4 single-voice fixture. By S4 the writer
/// emits `<tied>`/`<tie>`, `<slur>`, `<tuplet>`/`<time-modification>` and the
/// `<notations>` decoration groups; nothing in a single-voice notation fixture
/// is deferred, so the whole document must be byte-identical. Returns the
/// reconstructed score for direct attachment-field assertions.
fn assert_idempotent_s4(abc: &str) -> Score {
    let (x1, x2, score) = round_trip(abc);
    assert_eq!(
        x1, x2,
        "write(read(write(score))) must equal write(score) byte-for-byte (S4 notations)"
    );
    score
}

/// The `EventAttachments` of the first part's first voice's event at `index`.
fn attachments_at(score: &Score, index: usize) -> &crate::model::EventAttachments {
    &score.parts[0].voices[0].events[index].attachments
}

#[test]
fn tie_round_trips_and_reconstructs_attachment() {
    // C2-C2 ties two quarters: the first note carries a TieRole::Start, the
    // second a TieRole::Stop. Both <tie> (pre-<voice>) and <tied> (in
    // <notations>) re-emit from the single reconstructed `ties` list.
    use crate::model::TieRole;
    let abc = "X:1\nT:Tie\nM:4/4\nL:1/4\nK:C\nC2- C2 z4 |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<tie type=\"start\"/>") && x1.contains("<tied type=\"start\" number=\"1\"/>"),
        "precondition: a tie emits both <tie> and <tied>"
    );
    let score = assert_idempotent_s4(abc);
    let start = &attachments_at(&score, 0).ties;
    assert_eq!(start.len(), 1, "tie start note has one TieAttachment");
    assert_eq!(start[0].role, TieRole::Start);
    assert!(!start[0].dotted);
    let stop = &attachments_at(&score, 1).ties;
    assert_eq!(stop.len(), 1);
    assert_eq!(stop[0].role, TieRole::Stop);
}

#[test]
fn dotted_tie_reconstructs_line_type() {
    // `.-` is a dotted tie -> <tied ... line-type="dotted"/>; the reader must
    // recover `dotted = true` so the attribute re-emits.
    let abc = "X:1\nT:DotTie\nM:4/4\nL:1/4\nK:C\nC.-C z2 |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("line-type=\"dotted\""),
        "precondition: dotted tie emits line-type=\"dotted\""
    );
    let score = assert_idempotent_s4(abc);
    assert!(
        attachments_at(&score, 0).ties[0].dotted,
        "a dotted tie reconstructs dotted = true"
    );
}

#[test]
fn slur_round_trips_and_reconstructs_attachment() {
    use crate::model::SlurRole;
    let abc = "X:1\nT:Slur\nM:4/4\nL:1/4\nK:C\n(C D) z2 |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<slur type=\"start\" number=\"1\"/>"),
        "precondition: a slur emits number=1"
    );
    let score = assert_idempotent_s4(abc);
    let start = &attachments_at(&score, 0).slurs;
    assert_eq!(start.len(), 1);
    assert_eq!(start[0].role, SlurRole::Start);
    let stop = &attachments_at(&score, 1).slurs;
    assert_eq!(stop[0].role, SlurRole::Stop);
    // pair_id is chosen so the writer's SlurNumbers re-derives number=1.
    assert_eq!(
        start[0].pair_id, stop[0].pair_id,
        "a slur pair shares pair_id"
    );
}

#[test]
fn nested_slurs_reconstruct_distinct_numbers() {
    // (C (D E) F): outer slur is number 1, inner is number 2. The reader must
    // assign distinct pair_ids so the writer re-derives 1 (outer) and 2 (inner).
    let abc = "X:1\nT:Nest\nM:4/4\nL:1/4\nK:C\n(C (D E) F) |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<slur type=\"start\" number=\"1\"/>")
            && x1.contains("<slur type=\"start\" number=\"2\"/>"),
        "precondition: nested slurs emit numbers 1 and 2"
    );
    let score = assert_idempotent_s4(abc);
    // The outer start (note 0) and inner start (note 1) must have different
    // pair_ids, else they would collide on re-emission.
    let outer = attachments_at(&score, 0).slurs[0].pair_id;
    let inner = attachments_at(&score, 1).slurs[0].pair_id;
    assert_ne!(
        outer, inner,
        "overlapping slurs must reconstruct distinct pair_ids"
    );
}

#[test]
fn dotted_slur_reconstructs_line_type() {
    let abc = "X:1\nT:DotSlur\nM:4/4\nL:1/4\nK:C\n.(C D.) z2 |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<slur type=\"start\" number=\"1\" line-type=\"dotted\"/>"),
        "precondition: dotted slur emits line-type=\"dotted\""
    );
    let score = assert_idempotent_s4(abc);
    assert!(attachments_at(&score, 0).slurs[0].dotted);
}

#[test]
fn triplet_round_trips_and_reconstructs_tuplet() {
    // (3CDE is a 3:2 triplet of eighths under L:1/8: the first note carries a
    // TupletRole::Start, the middle a Continue (only <time-modification>, no
    // <tuplet>), the last a Stop. Every member emits <time-modification>.
    use crate::model::TupletRole;
    let abc = "X:1\nT:Trip\nM:4/4\nL:1/8\nK:C\n(3CDE A2 z2 |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<tuplet type=\"start\" number=\"1\"/>")
            && x1.contains("<actual-notes>3</actual-notes>")
            && x1.contains("<normal-notes>2</normal-notes>"),
        "precondition: triplet emits start tuplet + 3:2 time-modification"
    );
    let score = assert_idempotent_s4(abc);
    let start = &attachments_at(&score, 0).tuplets;
    assert_eq!(start.len(), 1, "the first triplet note has one tuplet");
    assert_eq!(start[0].role, TupletRole::Start);
    assert_eq!(start[0].actual_notes, 3);
    assert_eq!(start[0].normal_notes, 2);
    // The middle note carries a Continue (time-modification only).
    assert_eq!(
        attachments_at(&score, 1).tuplets[0].role,
        TupletRole::Continue
    );
    assert_eq!(attachments_at(&score, 2).tuplets[0].role, TupletRole::Stop);
}

#[test]
fn quintuplet_reconstructs_ratio() {
    // (5 under 4/4, L:1/8: a 5:2 tuplet (abc2xml's default q for 5 in simple
    // time). The exact ratio must reconstruct from <time-modification>.
    let abc = "X:1\nT:Quint\nM:4/4\nL:1/8\nK:C\n(5CDEFG z3 |\n";
    let x1 = export(abc);
    let score = assert_idempotent_s4(abc);
    let start = &attachments_at(&score, 0).tuplets[0];
    // Whatever ratio the writer chose, the reconstruction reproduces it.
    let actual = start.actual_notes;
    let normal = start.normal_notes;
    assert!(
        x1.contains(&format!("<actual-notes>{actual}</actual-notes>"))
            && x1.contains(&format!("<normal-notes>{normal}</normal-notes>")),
        "reconstructed tuplet ratio {actual}:{normal} must match the emitted time-modification"
    );
}

#[test]
fn two_separate_tuplets_in_a_measure_round_trip() {
    // Two consecutive triplets: both re-emit as number=1 (the second reuses the
    // freed number after the first stops). The reader must give them distinct
    // pair_ids so the active-set re-derivation reproduces number=1 each.
    let abc = "X:1\nT:TwoTrip\nM:4/4\nL:1/8\nK:C\n(3CDE (3FGA z2 |\n";
    let score = assert_idempotent_s4(abc);
    let first = attachments_at(&score, 0).tuplets[0].pair_id;
    let second = attachments_at(&score, 3).tuplets[0].pair_id;
    assert_ne!(first, second, "separate tuplets get distinct pair_ids");
}

#[test]
fn staccato_articulation_round_trips() {
    use crate::model::DecorationSourceKind;
    let abc = "X:1\nT:Stac\nM:4/4\nL:1/4\nK:C\n.C D E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<articulations>") && x1.contains("<staccato/>"),
        "precondition: . emits <staccato/>"
    );
    let score = assert_idempotent_s4(abc);
    let decos = &attachments_at(&score, 0).decorations;
    assert_eq!(decos.len(), 1, "one decoration on the first note");
    // The reconstructed name must re-map to <staccato/> via decoration_notation.
    assert_eq!(decos[0].name, "staccato");
    assert_eq!(decos[0].source_kind, DecorationSourceKind::Named);
}

#[test]
fn accent_articulation_round_trips() {
    let abc = "X:1\nT:Acc\nM:4/4\nL:1/4\nK:C\n!>!C D E F |\n";
    let score = assert_idempotent_s4(abc);
    assert_eq!(attachments_at(&score, 0).decorations[0].name, "accent");
}

#[test]
fn trill_ornament_round_trips() {
    let abc = "X:1\nT:Tr\nM:4/4\nL:1/4\nK:C\n!trill!C D E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<ornaments>") && x1.contains("<trill-mark/>"),
        "precondition: !trill! emits <trill-mark/>"
    );
    let score = assert_idempotent_s4(abc);
    assert_eq!(attachments_at(&score, 0).decorations[0].name, "trill");
}

#[test]
fn mordent_ornament_round_trips() {
    let abc = "X:1\nT:Mord\nM:4/4\nL:1/4\nK:C\n!mordent!C D E F |\n";
    let score = assert_idempotent_s4(abc);
    // `mordent` and `lowermordent` both emit <mordent/>; the canonical inverse
    // is the name that re-emits identically.
    assert_eq!(attachments_at(&score, 0).decorations[0].name, "mordent");
}

#[test]
fn fermata_round_trips() {
    let abc = "X:1\nT:Ferm\nM:4/4\nL:1/4\nK:C\n!fermata!C D E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<fermata type=\"upright\"/>"),
        "precondition: !fermata! emits <fermata type=\"upright\"/>"
    );
    let score = assert_idempotent_s4(abc);
    assert_eq!(attachments_at(&score, 0).decorations[0].name, "fermata");
}

#[test]
fn inverted_fermata_round_trips() {
    let abc = "X:1\nT:IFerm\nM:4/4\nL:1/4\nK:C\n!invertedfermata!C D E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<fermata type=\"inverted\"/>"),
        "precondition: inverted fermata emits type=\"inverted\""
    );
    let score = assert_idempotent_s4(abc);
    assert_eq!(
        attachments_at(&score, 0).decorations[0].name,
        "invertedfermata"
    );
}

#[test]
fn upbow_technical_round_trips() {
    let abc = "X:1\nT:Up\nM:4/4\nL:1/4\nK:C\n!upbow!C D E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<technical>") && x1.contains("<up-bow/>"),
        "precondition: !upbow! emits <up-bow/>"
    );
    let score = assert_idempotent_s4(abc);
    assert_eq!(attachments_at(&score, 0).decorations[0].name, "upbow");
}

#[test]
fn fingering_technical_text_round_trips() {
    // !1! is a fingering -> <technical><fingering>1</fingering></technical>; the
    // reader must reconstruct the decoration whose name re-emits the text element.
    let abc = "X:1\nT:Fing\nM:4/4\nL:1/4\nK:C\n!1!C D E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<fingering>1</fingering>"),
        "precondition: !1! emits <fingering>1</fingering>"
    );
    let score = assert_idempotent_s4(abc);
    assert_eq!(attachments_at(&score, 0).decorations[0].name, "1");
}

#[test]
fn arpeggio_round_trips() {
    let abc = "X:1\nT:Arp\nM:4/4\nL:1/4\nK:C\n!arpeggio![CEG] z2 z |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<arpeggiate/>"),
        "precondition: !arpeggio! emits <arpeggiate/>"
    );
    let score = assert_idempotent_s4(abc);
    // The arpeggiate decoration attaches to the chord's first member (event 0).
    assert_eq!(attachments_at(&score, 0).decorations[0].name, "arpeggio");
}

#[test]
fn multiple_decorations_on_one_note_round_trip() {
    // A note can carry an ornament AND an articulation AND a fermata; the writer
    // groups them per category in schema order, and the reader must reconstruct
    // every one so the whole grouped block re-emits identically.
    let abc = "X:1\nT:Multi\nM:4/4\nL:1/4\nK:C\n!trill!.!fermata!C D E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<trill-mark/>")
            && x1.contains("<staccato/>")
            && x1.contains("<fermata type=\"upright\"/>"),
        "precondition: all three notations are emitted"
    );
    let score = assert_idempotent_s4(abc);
    let names: Vec<&str> = attachments_at(&score, 0)
        .decorations
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    assert!(names.contains(&"trill"));
    assert!(names.contains(&"staccato"));
    assert!(names.contains(&"fermata"));
}

#[test]
fn beams_are_derived_not_stored_and_round_trip() {
    // The model has NO beam field; the writer derives beaming purely from note
    // durations/positions (in fact croma's writer emits no <beam> element at
    // all — beaming is left implicit). Reading the S1 notes correctly therefore
    // makes any beam behaviour round-trip with ZERO beam-specific reader code.
    // This test pins that: consecutive eighths (which are beamed when rendered)
    // round-trip byte-for-byte, and the writer emits no <beam> we failed to read.
    let abc = "X:1\nT:Beam\nM:4/4\nL:1/8\nK:C\nCDEF GABc |\n";
    let x1 = export(abc);
    assert!(
        !x1.contains("<beam"),
        "precondition: croma's writer derives beams and emits no <beam> element"
    );
    assert_idempotent_s4(abc);
}

#[test]
fn derived_time_modification_creates_no_tuplet_attachment() {
    // `C2/3` is a 1/6-of-a-beat note: the writer SYNTHESISES a 3:2
    // <time-modification> from the odd duration alone (no <tuplet>, no
    // <notations>), exactly like a derived beam. The reader must NOT fabricate a
    // tuplet attachment here — S1's duration reconstruction already re-emits the
    // identical <time-modification>. Proves the open-tuplet logic ignores
    // tuplet-less time-modifications.
    let abc = "X:1\nT:Odd\nM:4/4\nL:1/4\nK:C\nC2/3 D2/3 E2/3 z |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<time-modification>") && !x1.contains("<tuplet"),
        "precondition: an odd duration emits a derived time-modification with no <tuplet>"
    );
    let score = assert_idempotent_s4(abc);
    assert!(
        attachments_at(&score, 0).tuplets.is_empty(),
        "a derived time-modification must NOT reconstruct a tuplet attachment"
    );
}

#[test]
fn notation_and_tuplet_combine_round_trip() {
    // A triplet whose first note also carries a slur start and a staccato: ties
    // the tuplet/time-modification path together with the decoration grouping in
    // one note, proving the combined <notations> block re-emits in order.
    let abc = "X:1\nT:Combo\nM:4/4\nL:1/8\nK:C\n(3.CDE (FG) z2 |\n";
    let score = assert_idempotent_s4(abc);
    // First note: a tuplet start AND a staccato decoration.
    assert_eq!(
        attachments_at(&score, 0).tuplets[0].role,
        crate::model::TupletRole::Start
    );
    assert!(
        attachments_at(&score, 0)
            .decorations
            .iter()
            .any(|d| d.name == "staccato"),
        "the first triplet note keeps its staccato"
    );
}

// --- Corpus measurement (env-gated; mirrors croma-fmt::corpus_proof) --------

/// Collect every `*.abc` file directly under `dir`, sorted.
fn abc_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = match fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "abc"))
            .collect(),
        Err(_) => Vec::new(),
    };
    files.sort();
    files
}

/// The first XML tag at which two documents diverge (line-oriented, since the
/// writer emits one element per line). `None` when the strings are equal.
fn first_diverging_tag(x1: &str, x2: &str) -> Option<String> {
    for (line1, line2) in x1.lines().zip(x2.lines()) {
        if line1 != line2 {
            return Some(tag_of(line1).unwrap_or_else(|| tag_of(line2).unwrap_or("?".to_owned())));
        }
    }
    // One is a prefix of the other: name the first extra line's tag.
    let (longer, shorter) = if x1.lines().count() >= x2.lines().count() {
        (x1, x2)
    } else {
        (x2, x1)
    };
    longer.lines().nth(shorter.lines().count()).and_then(tag_of)
}

/// Extract the element name from a writer-emitted line like `  <note>` or
/// `  </measure>` or `  <clef number="1">`.
fn tag_of(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix('<')?;
    let rest = rest.strip_prefix('/').unwrap_or(rest);
    let name: String = rest
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '>' && *c != '/')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// Strip the writer-emitted blocks for elements that belong to **later stages**
/// (S2 `<key>`/`<time>`, S3 `<score-instrument>`/`<midi-instrument>`), giving
/// the "S1-supported subset" view. A file that is byte-equal after this strip is
/// one S1 fully reconstructs modulo the deferred stages — the honest secondary
/// metric alongside the strict full-byte count.
fn strip_deferred_blocks(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut skip_until: Option<&'static str> = None;
    for line in xml.lines() {
        if let Some(close) = skip_until {
            if line.trim() == close {
                skip_until = None;
            }
            continue;
        }
        let trimmed = line.trim();
        if trimmed == "<key>" {
            skip_until = Some("</key>");
        } else if trimmed == "<time>" || trimmed.starts_with("<time ") {
            skip_until = Some("</time>");
        } else if trimmed.starts_with("<score-instrument") {
            skip_until = Some("</score-instrument>");
        } else if trimmed.starts_with("<midi-instrument") {
            skip_until = Some("</midi-instrument>");
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

#[test]
fn corpus_idempotence_measurement() {
    let Ok(root) = std::env::var("ABC_ROOT") else {
        eprintln!("ABC_ROOT unset — skipping S1 corpus idempotence measurement");
        return;
    };
    let files = abc_files(&PathBuf::from(&root));
    if files.is_empty() {
        eprintln!("no .abc files under {root} — skipping");
        return;
    }

    let mut total = 0usize;
    let mut exported = 0usize;
    let mut idempotent = 0usize;
    let mut idempotent_supported = 0usize;
    let mut divergences: BTreeMap<String, usize> = BTreeMap::new();

    for path in &files {
        let Ok(bytes) = fs::read(path) else { continue };
        let source = String::from_utf8_lossy(&bytes);
        total += 1;

        // Only files that export cleanly have an X1 to invert.
        let Ok(export) = export_musicxml(&source) else {
            continue;
        };
        exported += 1;
        let x1 = export.musicxml;

        let report = read_musicxml(&x1);
        let x2 = write_musicxml(&report.value).musicxml;

        if x1 == x2 {
            idempotent += 1;
        } else if let Some(tag) = first_diverging_tag(&x1, &x2) {
            *divergences.entry(tag).or_default() += 1;
        } else {
            *divergences
                .entry("<equal-lines-unequal-bytes>".to_owned())
                .or_default() += 1;
        }

        // Secondary, honest "supported subset" metric: equal once the deferred
        // S2/S3 blocks are removed from both sides.
        if strip_deferred_blocks(&x1) == strip_deferred_blocks(&x2) {
            idempotent_supported += 1;
        }
    }

    let mut top: Vec<(String, usize)> = divergences.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    eprintln!(
        "S1 corpus idempotence (strict, full bytes): {idempotent}/{exported} exported files round-trip ({total} total .abc)"
    );
    eprintln!(
        "S1 corpus idempotence (S1-supported subset, deferred S2/S3 blocks stripped): {idempotent_supported}/{exported}"
    );
    eprintln!("S1 top first-diverging tags:");
    for (tag, count) in top.iter().take(10) {
        eprintln!("  {tag}: {count}");
    }

    // No hard count for S1 — most files use later-stage elements. We only
    // require the loop to be total (no panic) over the whole corpus.
    assert!(exported > 0, "expected at least one corpus file to export");
}
