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

// --- Stage S5a: <direction> (tempo / dynamics / wedge / coda / segno / words) -

/// Assert FULL-byte idempotence on an S5a single-voice fixture. By S5a the writer
/// also emits the `<direction>` block (tempo `<metronome>`/`<words>`,
/// `<dynamics>`, `<wedge>`, `<coda>`/`<segno>`, and plain annotation `<words>`);
/// nothing in a single-voice direction fixture is deferred, so the whole document
/// must be byte-identical. Returns the reconstructed score for direct field
/// assertions.
fn assert_idempotent_s5(abc: &str) -> Score {
    let (x1, x2, score) = round_trip(abc);
    assert_eq!(
        x1, x2,
        "write(read(write(score))) must equal write(score) byte-for-byte (S5a directions)"
    );
    score
}

#[test]
fn header_tempo_numeric_reconstructs_tempo_model() {
    use crate::model::TempoBeat;
    // Q:1/4=90 -> a header <metronome> (quarter / 90) before the first note; the
    // reader must reconstruct metadata.tempo_model so write_initial_directions
    // re-emits the identical direction.
    let abc = "X:1\nT:Tempo\nQ:1/4=90\nM:4/4\nL:1/4\nK:C\nC D E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<beat-unit>quarter</beat-unit>") && x1.contains("<per-minute>90</per-minute>"),
        "precondition: Q:1/4=90 emits a quarter metronome at 90"
    );
    let score = assert_idempotent_s5(abc);
    let tempo = score
        .metadata
        .tempo_model
        .as_ref()
        .expect("S5a must reconstruct the header <metronome> into metadata.tempo_model");
    assert_eq!(tempo.text, None, "a bare numeric tempo carries no words");
    assert_eq!(
        tempo.beat,
        Some(TempoBeat {
            beat_numerator: 1,
            beat_denominator: 4,
            bpm: 90,
        }),
        "the reconstructed beat must drive the same <beat-unit>/<per-minute>"
    );
}

#[test]
fn header_tempo_dotted_beat_unit_reconstructs() {
    use crate::model::TempoBeat;
    // Q:3/8=60 -> a DOTTED quarter metronome (<beat-unit>quarter</beat-unit>
    // <beat-unit-dot/>); the 3/(2^k) inverse must recover beat 3/8.
    let abc = "X:1\nT:Dotted\nQ:3/8=60\nM:6/8\nL:1/8\nK:C\nC2C2C2 |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<beat-unit-dot/>"),
        "precondition: Q:3/8=60 emits a dotted beat unit"
    );
    let score = assert_idempotent_s5(abc);
    assert_eq!(
        score.metadata.tempo_model.as_ref().and_then(|t| t.beat),
        Some(TempoBeat {
            beat_numerator: 3,
            beat_denominator: 8,
            bpm: 60,
        })
    );
}

#[test]
fn header_tempo_with_text_reconstructs_words_and_beat() {
    // Q:"Allegro" 1/4=120 -> a <words>Allegro</words> direction-type plus the
    // <metronome>; the reader must reconstruct BOTH tempo.text and tempo.beat so
    // the words and metronome direction-types re-emit in order.
    let abc = "X:1\nT:WithText\nQ:\"Allegro\" 1/4=120\nM:4/4\nL:1/4\nK:C\nC D E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<words>Allegro</words>") && x1.contains("<per-minute>120</per-minute>"),
        "precondition: text+numeric tempo emits words AND metronome"
    );
    let score = assert_idempotent_s5(abc);
    let tempo = score.metadata.tempo_model.as_ref().expect("tempo_model");
    assert_eq!(tempo.text.as_deref(), Some("Allegro"));
    assert_eq!(tempo.beat.map(|b| b.bpm), Some(120));
}

#[test]
fn header_text_only_tempo_reconstructs_words_no_beat() {
    // Q:"Andante" with NO numeric tempo -> a voice-less <words> + <sound> header
    // direction (no <metronome>). The reader must reconstruct a tempo_model with
    // text and beat=None so write_tempo_direction re-emits the words + the default
    // <sound tempo="120.00"/>, NOT a voice-bearing annotation direction.
    let abc = "X:1\nT:TextTempo\nQ:\"Andante\"\nM:4/4\nL:1/4\nK:C\nC D E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<words>Andante</words>")
            && x1.contains("<sound tempo=\"120.00\"/>")
            && !x1.contains("<metronome>"),
        "precondition: a text-only tempo emits words + sound, no metronome"
    );
    let score = assert_idempotent_s5(abc);
    let tempo = score
        .metadata
        .tempo_model
        .as_ref()
        .expect("a text-only tempo must reconstruct metadata.tempo_model");
    assert_eq!(tempo.text.as_deref(), Some("Andante"));
    assert_eq!(
        tempo.beat, None,
        "a text-only tempo carries no numeric beat"
    );
}

#[test]
fn mid_tune_tempo_change_reconstructs_event() {
    use crate::model::TimedEventKind;
    // A mid-tune [Q:1/4=160] becomes a TempoChange event emitted as a voice-less
    // tempo <direction> BETWEEN notes; the reader must reconstruct a
    // TimedEventKind::TempoChange at that onset (NOT the header tempo_model) so it
    // re-emits in the same inter-note position.
    let abc = "X:1\nT:MidTempo\nM:4/4\nL:1/4\nK:C\nC D [Q:1/4=160] E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<per-minute>160</per-minute>"),
        "precondition: the inline [Q:] emits a 160 metronome"
    );
    let score = assert_idempotent_s5(abc);
    assert!(
        score.metadata.tempo_model.is_none(),
        "no header Q: -> tempo_model stays None; the inline tempo is an event"
    );
    let tempo_changes: Vec<_> = score.parts[0].voices[0]
        .events
        .iter()
        .filter(|e| matches!(e.kind, TimedEventKind::TempoChange(_)))
        .collect();
    assert_eq!(
        tempo_changes.len(),
        1,
        "the inline [Q:] reconstructs exactly one TempoChange event"
    );
    let bpm = match &tempo_changes[0].kind {
        TimedEventKind::TempoChange(model) => model.beat.map(|b| b.bpm),
        _ => None,
    };
    assert_eq!(bpm, Some(160), "the TempoChange carries the 160 bpm");
}

#[test]
fn header_and_mid_tune_tempo_both_round_trip() {
    use crate::model::TimedEventKind;
    // Header Q: AND an inline [Q:] coexist: the leading metronome (before the
    // first note) is the header tempo_model; the inter-note one is a TempoChange.
    // Both must reconstruct so the two metronome directions re-emit in place.
    let abc = "X:1\nT:Both\nQ:1/4=90\nM:4/4\nL:1/4\nK:C\nC D [Q:1/4=160] E F |\n";
    let score = assert_idempotent_s5(abc);
    assert_eq!(
        score
            .metadata
            .tempo_model
            .as_ref()
            .and_then(|t| t.beat)
            .map(|b| b.bpm),
        Some(90),
        "the leading tempo is the header tempo_model (90)"
    );
    let mid: Vec<_> = score.parts[0].voices[0]
        .events
        .iter()
        .filter(|e| matches!(e.kind, TimedEventKind::TempoChange(_)))
        .collect();
    assert_eq!(mid.len(), 1, "exactly one inline TempoChange (160)");
}

#[test]
fn dynamic_forte_reconstructs_decoration() {
    use crate::model::DecorationSourceKind;
    // !f! -> a <direction placement="below"><dynamics><f/></dynamics> direction
    // (NOT a <notations> element); the reader must reconstruct a "f" decoration on
    // the following note so the dynamics direction re-emits.
    let abc = "X:1\nT:Forte\nM:4/4\nL:1/4\nK:C\n!f!C D E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<dynamics>") && x1.contains("<f/>"),
        "precondition: !f! emits a <dynamics><f/> direction"
    );
    let score = assert_idempotent_s5(abc);
    let decos = &attachments_at(&score, 0).decorations;
    assert_eq!(decos.len(), 1, "one dynamic decoration on the first note");
    assert_eq!(decos[0].name, "f", "the reconstructed name re-emits <f/>");
    assert_eq!(decos[0].source_kind, DecorationSourceKind::Named);
}

#[test]
fn dynamic_pianissimo_reconstructs() {
    let abc = "X:1\nT:PP\nM:4/4\nL:1/4\nK:C\n!pp!C D E F |\n";
    let score = assert_idempotent_s5(abc);
    assert_eq!(attachments_at(&score, 0).decorations[0].name, "pp");
}

#[test]
fn dynamic_sforzando_reconstructs() {
    // !sfz! is the one dynamic whose ABC name differs from its <sfz/> element by
    // case only; ensure the inverse maps the element back to "sfz".
    let abc = "X:1\nT:SF\nM:4/4\nL:1/4\nK:C\n!sfz!C D E F |\n";
    let x1 = export(abc);
    assert!(x1.contains("<sfz/>"), "precondition: !sfz! emits <sfz/>");
    let score = assert_idempotent_s5(abc);
    assert_eq!(attachments_at(&score, 0).decorations[0].name, "sfz");
}

#[test]
fn crescendo_wedge_reconstructs_open_and_close() {
    // !<(! opens a crescendo wedge, !<)! closes it: two voice-bearing
    // <direction><wedge> elements. The reader must reconstruct a "crescendo("
    // decoration on the open note and a "crescendo)" on the close note so both
    // <wedge type="crescendo"/> and <wedge type="stop"/> re-emit.
    let abc = "X:1\nT:Cresc\nM:4/4\nL:1/4\nK:C\n!<(!C D!<)! E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<wedge type=\"crescendo\"/>") && x1.contains("<wedge type=\"stop\"/>"),
        "precondition: a crescendo hairpin emits crescendo + stop wedges"
    );
    let score = assert_idempotent_s5(abc);
    // The open wedge attaches to the first note. In ABC a decoration binds to the
    // FOLLOWING note, so `D!<)! E` places the close wedge before E (event 2): the
    // writer emits its <wedge type="stop"/> direction just before E's <note>.
    let open = &attachments_at(&score, 0).decorations;
    assert!(
        open.iter().any(|d| d.name == "crescendo("),
        "the open note carries a crescendo( decoration, got {:?}",
        open.iter().map(|d| &d.name).collect::<Vec<_>>()
    );
    let close = &attachments_at(&score, 2).decorations;
    assert!(
        close.iter().any(|d| d.name == "crescendo)"),
        "the note after the close marker carries a crescendo) decoration"
    );
}

#[test]
fn diminuendo_wedge_reconstructs() {
    let abc = "X:1\nT:Dim\nM:4/4\nL:1/4\nK:C\n!>(!C D!>)! E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<wedge type=\"diminuendo\"/>"),
        "precondition: !>(! emits a diminuendo wedge"
    );
    let score = assert_idempotent_s5(abc);
    assert!(
        attachments_at(&score, 0)
            .decorations
            .iter()
            .any(|d| d.name == "diminuendo(")
    );
}

#[test]
fn coda_reconstructs_decoration() {
    // !coda! -> a <direction placement="above"><coda/> (voice-bearing); the reader
    // reconstructs a "coda" decoration so write_direction_type re-emits <coda/>.
    let abc = "X:1\nT:Coda\nM:4/4\nL:1/4\nK:C\n!coda!C D E F |\n";
    let x1 = export(abc);
    assert!(x1.contains("<coda/>"), "precondition: !coda! emits <coda/>");
    let score = assert_idempotent_s5(abc);
    assert_eq!(attachments_at(&score, 0).decorations[0].name, "coda");
}

#[test]
fn segno_reconstructs_decoration() {
    let abc = "X:1\nT:Segno\nM:4/4\nL:1/4\nK:C\n!segno!C D E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<segno/>"),
        "precondition: !segno! emits <segno/>"
    );
    let score = assert_idempotent_s5(abc);
    assert_eq!(attachments_at(&score, 0).decorations[0].name, "segno");
}

#[test]
fn pre_barline_segno_reconstructs_on_trailing_spacer() {
    use crate::model::TimedEventKind;
    // `d4 !segno!|` puts the !segno! before the barline with NO following note:
    // the writer flushes it onto a zero-duration Spacer event whose directions
    // emit after the last note. The reader must reconstruct that Spacer so the
    // trailing <segno/> direction re-emits at the end of the measure.
    let abc = "X:1\nT:Trail\nM:4/4\nL:1/4\nK:C\nC D E F !segno!|\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<segno/>"),
        "precondition: a pre-barline !segno! still emits <segno/>"
    );
    let score = assert_idempotent_s5(abc);
    // The last reconstructed event is a zero-duration Spacer carrying the segno.
    let events = &score.parts[0].voices[0].events;
    let last = events.last().expect("at least one event");
    assert!(
        matches!(last.kind, TimedEventKind::Spacer),
        "the trailing segno reconstructs on a Spacer event, got {:?}",
        last.kind
    );
    assert_eq!(
        last.duration,
        Fraction::zero(),
        "the Spacer is zero-duration"
    );
    assert!(
        last.attachments
            .decorations
            .iter()
            .any(|d| d.name == "segno"),
        "the trailing Spacer carries the segno decoration"
    );
}

#[test]
fn annotation_above_reconstructs_text_and_placement() {
    use crate::model::AnnotationPlacementModel;
    // "^Andante" is an above-placed annotation -> a <direction placement="above">
    // <words>Andante</words> (the writer strips the ^ prefix). The reader must
    // reconstruct a TextAttachment whose (text, placement) re-emits BOTH the
    // stripped words and the placement attribute.
    let abc = "X:1\nT:Anno\nM:4/4\nL:1/4\nK:C\n\"^Andante\"C D E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("placement=\"above\"") && x1.contains("<words>Andante</words>"),
        "precondition: ^Andante emits an above words direction"
    );
    let score = assert_idempotent_s5(abc);
    let annotations = &attachments_at(&score, 0).annotations;
    assert_eq!(annotations.len(), 1, "one annotation on the first note");
    assert_eq!(
        annotations[0].placement,
        Some(AnnotationPlacementModel::Above),
        "the above placement must reconstruct"
    );
    // The reconstructed text re-emits the stripped <words>Andante</words> via the
    // writer's annotation_text (which strips the canonical ^ prefix again).
    assert!(
        annotations[0].text.ends_with("Andante"),
        "reconstructed annotation text re-emits Andante, got {:?}",
        annotations[0].text
    );
}

#[test]
fn annotation_below_reconstructs_placement() {
    use crate::model::AnnotationPlacementModel;
    let abc = "X:1\nT:Below\nM:4/4\nL:1/4\nK:C\n\"_sotto\"C D E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("placement=\"below\"") && x1.contains("<words>sotto</words>"),
        "precondition: _sotto emits a below words direction"
    );
    let score = assert_idempotent_s5(abc);
    assert_eq!(
        attachments_at(&score, 0).annotations[0].placement,
        Some(AnnotationPlacementModel::Below)
    );
}

#[test]
fn dynamic_and_annotation_on_same_note_round_trip() {
    // A note can carry BOTH a dynamic and an annotation: the writer emits the
    // annotation <words> direction THEN the <dynamics> direction (annotations
    // before decorations in write_harmony_and_directions). The reader must
    // reconstruct both, on the right channels, so the two directions re-emit in
    // order before the note.
    let abc = "X:1\nT:Mix\nM:4/4\nL:1/4\nK:C\n\"^cresc.\"!f!C D E F |\n";
    let score = assert_idempotent_s5(abc);
    assert_eq!(
        attachments_at(&score, 0).annotations.len(),
        1,
        "the annotation reconstructs"
    );
    assert!(
        attachments_at(&score, 0)
            .decorations
            .iter()
            .any(|d| d.name == "f"),
        "the dynamic reconstructs"
    );
}

#[test]
fn no_direction_leaves_attachments_empty() {
    // A plain note with no directions must not fabricate annotations/decorations
    // or a tempo_model (the writer emits no <direction> for it).
    let score = assert_idempotent_s5("X:1\nT:Plain\nM:4/4\nL:1/4\nK:C\nC D E F |\n");
    assert!(score.metadata.tempo_model.is_none());
    assert!(attachments_at(&score, 0).annotations.is_empty());
    assert!(attachments_at(&score, 0).decorations.is_empty());
}

// --- Stage S5b: <harmony> + <lyric> ------------------------------------------

/// Assert FULL-byte idempotence on an S5b single-voice fixture. By S5b the writer
/// also emits `<harmony>` (chord symbols) and the per-`<note>` `<lyric>` block;
/// nothing in a single-voice harmony/lyric fixture is deferred, so the whole
/// document must be byte-identical. Returns the reconstructed score for direct
/// attachment-field assertions.
fn assert_idempotent_s5b(abc: &str) -> Score {
    let (x1, x2, score) = round_trip(abc);
    assert_eq!(
        x1, x2,
        "write(read(write(score))) must equal write(score) byte-for-byte (S5b harmony + lyrics)"
    );
    score
}

#[test]
fn harmony_major_reconstructs_chord_symbol_text() {
    // A bare "C" major triad emits <harmony><root><root-step>C</root-step></root>
    // <kind text="C">major</kind>. The reader must reconstruct a chord_symbols
    // TextAttachment whose text re-emits the identical <harmony>.
    let abc = "X:1\nT:H\nM:4/4\nL:1/4\nK:C\n\"C\"C D E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<kind text=\"C\">major</kind>"),
        "precondition: a bare C chord emits a major <harmony>"
    );
    let score = assert_idempotent_s5b(abc);
    let symbols = &attachments_at(&score, 0).chord_symbols;
    assert_eq!(symbols.len(), 1, "one chord symbol on the first note");
    assert_eq!(
        symbols[0].text, "C",
        "the reconstructed chord text is the <kind text=...> attribute"
    );
}

#[test]
fn harmony_minor_reconstructs() {
    let abc = "X:1\nT:H\nM:4/4\nL:1/4\nK:C\n\"Dm\"C D E F |\n";
    let x1 = export(abc);
    assert!(x1.contains("<kind text=\"Dm\">minor</kind>"));
    let score = assert_idempotent_s5b(abc);
    assert_eq!(attachments_at(&score, 0).chord_symbols[0].text, "Dm");
}

#[test]
fn harmony_seventh_reconstructs() {
    // A maj7 exercises the longest-token quality match; the text attribute (not
    // the inverted kind value) is what the reader recovers, so round-trip is direct.
    let abc = "X:1\nT:H\nM:4/4\nL:1/4\nK:C\n\"Cmaj7\"C D E F |\n";
    let x1 = export(abc);
    assert!(x1.contains("<kind text=\"Cmaj7\">major-seventh</kind>"));
    let score = assert_idempotent_s5b(abc);
    assert_eq!(attachments_at(&score, 0).chord_symbols[0].text, "Cmaj7");
}

#[test]
fn harmony_slash_bass_reconstructs() {
    // "G7/B" emits a <bass><bass-step>B</bass-step></bass> in addition to the
    // dominant <kind>. The text attribute still carries the whole "G7/B" string.
    let abc = "X:1\nT:H\nM:4/4\nL:1/4\nK:C\n\"G7/B\"C D E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<kind text=\"G7/B\">dominant</kind>")
            && x1.contains("<bass-step>B</bass-step>"),
        "precondition: a slash chord emits a <bass> and preserves the text"
    );
    let score = assert_idempotent_s5b(abc);
    assert_eq!(attachments_at(&score, 0).chord_symbols[0].text, "G7/B");
}

#[test]
fn harmony_altered_root_reconstructs() {
    // "F#m7b5" exercises a sharp root (<root-alter>1) plus a half-diminished kind
    // plus a flatted-fifth degree; the text attribute carries it all verbatim.
    let abc = "X:1\nT:H\nM:4/4\nL:1/4\nK:C\n\"F#m7b5\"C D E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<root-alter>1</root-alter>")
            && x1.contains("<kind text=\"F#m7b5\">half-diminished</kind>"),
        "precondition: a sharp-root altered chord emits root-alter and the text"
    );
    let score = assert_idempotent_s5b(abc);
    assert_eq!(attachments_at(&score, 0).chord_symbols[0].text, "F#m7b5");
}

#[test]
fn harmony_with_added_degree_reconstructs() {
    // "C7b9" emits a <degree> block; the reader recovers only the text, and the
    // writer re-derives the identical <degree> on re-emission.
    let abc = "X:1\nT:H\nM:4/4\nL:1/4\nK:C\n\"C7b9\"C D E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<degree-value>9</degree-value>"),
        "precondition: C7b9 emits an added 9th degree"
    );
    let score = assert_idempotent_s5b(abc);
    assert_eq!(attachments_at(&score, 0).chord_symbols[0].text, "C7b9");
}

#[test]
fn harmony_then_annotation_round_trip_in_order() {
    // The writer emits <harmony> BEFORE annotation <words> for the same event.
    // A chord plus an above-annotation on the same note must reconstruct both
    // (chord_symbols then annotations) so they re-emit in the same order.
    let abc = "X:1\nT:H\nM:4/4\nL:1/4\nK:C\n\"C\"\"^Slow\"C D E F |\n";
    let score = assert_idempotent_s5b(abc);
    assert_eq!(
        attachments_at(&score, 0).chord_symbols.len(),
        1,
        "the chord reconstructs into chord_symbols"
    );
    assert_eq!(
        attachments_at(&score, 0).annotations.len(),
        1,
        "the annotation reconstructs into annotations"
    );
}

#[test]
fn two_chords_before_one_note_round_trip_in_order() {
    // `"C""Am"D` puts two chord symbols on the same note: the writer emits two
    // <harmony> blocks in order. The reader must buffer and flush them in the same
    // order so both re-emit (C then Am).
    let abc = "X:1\nT:H\nM:4/4\nL:1/4\nK:C\n\"C\"\"Am\"D E F G |\n";
    let x1 = export(abc);
    assert_eq!(
        x1.matches("<harmony>").count(),
        2,
        "precondition: two chords emit two <harmony> blocks"
    );
    let score = assert_idempotent_s5b(abc);
    let symbols = &attachments_at(&score, 0).chord_symbols;
    assert_eq!(
        symbols.len(),
        2,
        "both chords reconstruct on the first note"
    );
    assert_eq!(symbols[0].text, "C", "first chord stays first");
    assert_eq!(symbols[1].text, "Am", "second chord stays second");
}

#[test]
fn harmony_before_rest_reconstructs_on_rest_event() {
    use crate::model::TimedEventKind;
    // A chord symbol binds to the FOLLOWING event even when that event is a rest
    // (a rest is still a `<note>` element, so the writer emits the `<harmony>`
    // before it). The reader must flush the buffered chord onto the rest event so
    // the `<harmony>` re-emits in place. (A chord with no following note at all,
    // e.g. before a barline, is dropped on the forward path — there is no
    // `<harmony>` to read back, so it is not a round-trip case.)
    let abc = "X:1\nT:Trail\nM:4/4\nL:1/4\nK:C\nC D E F | \"C\" z4 |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<harmony>"),
        "precondition: a chord before a rest emits <harmony>"
    );
    let score = assert_idempotent_s5b(abc);
    // The chord attaches to the first event of the second measure (the rest).
    let rest_event = score.parts[0].voices[0]
        .events
        .iter()
        .find(|e| matches!(e.kind, TimedEventKind::Rest(_)))
        .expect("a rest event");
    assert_eq!(
        rest_event.attachments.chord_symbols.len(),
        1,
        "the chord reconstructs onto the following rest event"
    );
    assert_eq!(rest_event.attachments.chord_symbols[0].text, "C");
}

#[test]
fn lyric_single_syllable_reconstructs() {
    use crate::model::LyricControl;
    // A one-syllable lyric "la" on a note emits <lyric number="1"><syllabic>single
    // </syllabic><text>la</text></lyric>. The reader reconstructs a Syllable.
    let abc = "X:1\nT:L\nM:4/4\nL:1/4\nK:C\nC D E F |\nw: la la la la\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<syllabic>single</syllabic>") && x1.contains("<text>la</text>"),
        "precondition: a stand-alone syllable emits single"
    );
    let score = assert_idempotent_s5b(abc);
    let lyrics = &attachments_at(&score, 0).lyrics;
    assert_eq!(lyrics.len(), 1, "one lyric on the first note");
    assert_eq!(lyrics[0].verse, 1);
    assert_eq!(lyrics[0].text, "la");
    assert_eq!(lyrics[0].control, LyricControl::Syllable);
}

#[test]
fn lyric_hyphenated_word_reconstructs_begin_end() {
    use crate::model::LyricControl;
    // "Twin-kle" splits across two notes: the writer emits begin/Twin then
    // end/kle. The reader must reconstruct [Syllable, Hyphen] on the first note
    // and [Syllable] on the second so the syllabic state machine re-derives
    // begin/end exactly.
    let abc = "X:1\nT:L\nM:4/4\nL:1/4\nK:C\nC D E F |\nw: Twin-kle star light\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<syllabic>begin</syllabic>") && x1.contains("<syllabic>end</syllabic>"),
        "precondition: a two-syllable word emits begin then end"
    );
    let score = assert_idempotent_s5b(abc);
    let first = &attachments_at(&score, 0).lyrics;
    assert_eq!(
        first.len(),
        2,
        "the begin syllable note carries a Syllable AND a trailing Hyphen"
    );
    assert_eq!(first[0].control, LyricControl::Syllable);
    assert_eq!(first[0].text, "Twin");
    assert_eq!(
        first[1].control,
        LyricControl::Hyphen,
        "a begin syllabic reconstructs a following Hyphen on the same note"
    );
    let second = &attachments_at(&score, 1).lyrics;
    assert_eq!(
        second.len(),
        1,
        "the end syllable note carries only a Syllable"
    );
    assert_eq!(second[0].control, LyricControl::Syllable);
    assert_eq!(second[0].text, "kle");
}

#[test]
fn lyric_three_syllable_word_reconstructs_middle() {
    use crate::model::LyricControl;
    // "ho-li-day" exercises the middle syllabic: begin/ho, middle/li, end/day.
    // The middle note must also reconstruct [Syllable, Hyphen] so it re-emits
    // middle (it both follows an open hyphen and opens another).
    let abc = "X:1\nT:L\nM:4/4\nL:1/4\nK:C\nC D E F |\nw: ho-li-day now\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<syllabic>middle</syllabic>"),
        "precondition: a three-syllable word emits a middle syllabic"
    );
    let score = assert_idempotent_s5b(abc);
    let middle = &attachments_at(&score, 1).lyrics;
    assert_eq!(
        middle.len(),
        2,
        "the middle syllable carries Syllable + Hyphen"
    );
    assert_eq!(middle[0].text, "li");
    assert_eq!(middle[1].control, LyricControl::Hyphen);
}

#[test]
fn lyric_melisma_extender_reconstructs() {
    use crate::model::LyricControl;
    // "yes_" extends "yes" over the next note via <extend/> on that next note.
    // The reader reconstructs an Extender (empty text) on the following note.
    let abc = "X:1\nT:L\nM:4/4\nL:1/4\nK:C\nC D E F |\nw: yes_ no end\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<extend/>"),
        "precondition: a melisma emits <extend/>"
    );
    let score = assert_idempotent_s5b(abc);
    // Event 0 is "yes" (single); event 1 carries the extender.
    assert_eq!(attachments_at(&score, 0).lyrics[0].text, "yes");
    let extend = &attachments_at(&score, 1).lyrics;
    assert_eq!(
        extend.len(),
        1,
        "the extended note carries one Extender lyric"
    );
    assert_eq!(extend[0].control, LyricControl::Extender);
    assert!(extend[0].text.is_empty(), "an extender carries no text");
}

#[test]
fn lyric_multiple_verses_reconstruct_numbers() {
    use crate::model::LyricControl;
    // Two w: lines -> two verses; each note carries number="1" then number="2".
    // The reader must reconstruct both verses in order on every note.
    let abc = "X:1\nT:L\nM:4/4\nL:1/4\nK:C\nC D E F |\nw: one two three four\nw: ay bee cee dee\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<lyric number=\"1\">") && x1.contains("<lyric number=\"2\">"),
        "precondition: two w: lines emit two numbered verses"
    );
    let score = assert_idempotent_s5b(abc);
    let lyrics = &attachments_at(&score, 0).lyrics;
    assert_eq!(lyrics.len(), 2, "the first note carries both verses");
    assert_eq!(lyrics[0].verse, 1);
    assert_eq!(lyrics[0].text, "one");
    assert_eq!(lyrics[0].control, LyricControl::Syllable);
    assert_eq!(lyrics[1].verse, 2);
    assert_eq!(lyrics[1].text, "ay");
    assert_eq!(lyrics[1].control, LyricControl::Syllable);
}

#[test]
fn lyric_text_with_trailing_space_round_trips_verbatim() {
    // The writer emits `lyric.text` verbatim, so a syllable that carries a
    // trailing space (the corpus produces these, e.g. via the `~` lyric space)
    // must round-trip byte-for-byte. The reader must NOT trim the <text> content.
    // `la~` is a syllable whose `~` lowers to a trailing space -> `<text>la </text>`.
    let abc = "X:1\nT:L\nM:4/4\nL:1/4\nK:C\nC D E F |\nw: la~ lo me fa\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<text>la </text>"),
        "precondition: a `la~` syllable keeps its trailing space in the <text>, got:\n{x1}"
    );
    let score = assert_idempotent_s5b(abc);
    assert_eq!(
        attachments_at(&score, 0).lyrics[0].text,
        "la ",
        "the reconstructed lyric text preserves the trailing space verbatim"
    );
}

#[test]
fn harmony_and_lyric_on_same_note_round_trip() {
    // A note can carry a chord symbol (emitted before the note) AND a lyric
    // (emitted inside the note). Both must reconstruct so the whole event
    // re-emits byte-identically.
    let abc = "X:1\nT:Both\nM:4/4\nL:1/4\nK:C\n\"C\"C D E F |\nw: la la la la\n";
    let score = assert_idempotent_s5b(abc);
    assert_eq!(attachments_at(&score, 0).chord_symbols.len(), 1);
    assert_eq!(attachments_at(&score, 0).lyrics.len(), 1);
}

// --- Stage S6a: <barline> + <repeat> + <ending> ------------------------------

/// Assert FULL-byte idempotence on an S6a single-voice fixture. By S6a the writer
/// also emits the `<barline>` block (`<bar-style>`, `<repeat>`, `<ending>`) on the
/// left/right of measures; nothing in a single-voice barline/repeat/ending fixture
/// is deferred, so the whole document must be byte-identical. Returns the
/// reconstructed score for direct model-field assertions.
fn assert_idempotent_s6a(abc: &str) -> Score {
    let (x1, x2, score) = round_trip(abc);
    assert_eq!(
        x1, x2,
        "write(read(write(score))) must equal write(score) byte-for-byte (S6a barlines)"
    );
    score
}

/// The `Measure` at `index` of the first part's first voice.
fn measure_at(score: &Score, index: usize) -> &crate::model::Measure {
    &score.parts[0].voices[0].measures[index]
}

#[test]
fn final_barline_reconstructs_kind() {
    use crate::model::BarlineKind;
    // `|]` -> <barline location="right"><bar-style>light-heavy</bar-style></barline>
    // with NO <repeat>; the reader must reconstruct a trailing Final barline.
    let abc = "X:1\nT:F\nM:4/4\nL:1/4\nK:C\nC D E F |]\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<bar-style>light-heavy</bar-style>") && !x1.contains("<repeat"),
        "precondition: |] emits light-heavy with no repeat"
    );
    let score = assert_idempotent_s6a(abc);
    let barlines = &measure_at(&score, 0).barlines;
    assert_eq!(barlines.len(), 1, "one reconstructed right barline");
    assert_eq!(barlines[0].kind, BarlineKind::Final);
}

#[test]
fn double_barline_reconstructs_kind() {
    use crate::model::BarlineKind;
    // `||` -> <bar-style>light-light</bar-style> on the right of the first measure.
    let abc = "X:1\nT:D\nM:4/4\nL:1/4\nK:C\nC D E F || G A B c |\n";
    let x1 = export(abc);
    assert!(x1.contains("<bar-style>light-light</bar-style>"));
    let score = assert_idempotent_s6a(abc);
    assert_eq!(measure_at(&score, 0).barlines[0].kind, BarlineKind::Double);
}

#[test]
fn dotted_barline_reconstructs_kind() {
    use crate::model::BarlineKind;
    let abc = "X:1\nT:Dt\nM:4/4\nL:1/4\nK:C\nC D E F .| G A B c |\n";
    let x1 = export(abc);
    assert!(x1.contains("<bar-style>dotted</bar-style>"));
    let score = assert_idempotent_s6a(abc);
    assert_eq!(measure_at(&score, 0).barlines[0].kind, BarlineKind::Dotted);
}

#[test]
fn invisible_barline_reconstructs_kind() {
    use crate::model::BarlineKind;
    // `[|]` -> <bar-style>none</bar-style> (an invisible barline).
    let abc = "X:1\nT:Inv\nM:4/4\nL:1/4\nK:C\nC D E F [|] G A B c |\n";
    let x1 = export(abc);
    assert!(x1.contains("<bar-style>none</bar-style>"));
    let score = assert_idempotent_s6a(abc);
    assert_eq!(
        measure_at(&score, 0).barlines[0].kind,
        BarlineKind::Invisible
    );
}

#[test]
fn initial_thick_thin_barline_reconstructs_kind() {
    use crate::model::BarlineKind;
    // A mid-tune `[|` (thick-thin) emits <bar-style>heavy-light</bar-style> on the
    // RIGHT with NO <repeat>; this is the Initial kind (distinct from a left
    // RepeatStart, which is heavy-light + <repeat direction="forward">).
    let abc = "X:1\nT:I\nM:4/4\nL:1/4\nK:C\nC D E F [| G A B c |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<bar-style>heavy-light</bar-style>") && !x1.contains("<repeat"),
        "precondition: a mid-tune [| emits heavy-light with no repeat"
    );
    let score = assert_idempotent_s6a(abc);
    assert_eq!(measure_at(&score, 0).barlines[0].kind, BarlineKind::Initial);
}

#[test]
fn repeat_start_reconstructs_leading_barline() {
    use crate::model::BarlineKind;
    // `|:` -> <barline location="left"><bar-style>heavy-light</bar-style>
    // <repeat direction="forward"/></barline>. The reader must reconstruct a
    // LEADING RepeatStart (span.start == measure.source_span.start) so the writer
    // re-emits it on the left.
    let abc = "X:1\nT:R\nM:4/4\nL:1/4\nK:C\n|: C D E F :|\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<repeat direction=\"forward\"/>")
            && x1.contains("<repeat direction=\"backward\"/>"),
        "precondition: |: ... :| emits a forward and a backward repeat"
    );
    let score = assert_idempotent_s6a(abc);
    let barlines = &measure_at(&score, 0).barlines;
    assert!(
        barlines.iter().any(|b| b.kind == BarlineKind::RepeatStart),
        "the first measure carries a leading RepeatStart"
    );
    // The closing `:|` is a RepeatEnd on the (single) measure's right.
    assert!(
        barlines.iter().any(|b| b.kind == BarlineKind::RepeatEnd),
        "the measure also carries the closing RepeatEnd"
    );
}

#[test]
fn repeat_start_end_across_measures_reconstructs() {
    use crate::model::BarlineKind;
    // |: in measure 1, body across two bars, :| at the end of measure 2.
    let abc = "X:1\nT:R2\nM:4/4\nL:1/4\nK:C\n|: C D E F | G A B c :|\n";
    let score = assert_idempotent_s6a(abc);
    assert!(
        measure_at(&score, 0)
            .barlines
            .iter()
            .any(|b| b.kind == BarlineKind::RepeatStart),
        "measure 1 opens the repeat"
    );
    assert!(
        measure_at(&score, 1)
            .barlines
            .iter()
            .any(|b| b.kind == BarlineKind::RepeatEnd),
        "measure 2 closes the repeat"
    );
}

#[test]
fn repeat_both_decomposes_and_round_trips() {
    use crate::model::BarlineKind;
    // `::` (a combined back-then-forward repeat) emits the SAME XML as a
    // RepeatEnd immediately followed by a leading RepeatStart: measure 2's right
    // is light-heavy + repeat-backward, measure 3's left is heavy-light +
    // repeat-forward. The reader decomposes it into RepeatEnd + RepeatStart (it
    // never needs to materialise a RepeatBoth), which re-emits byte-identically.
    let abc = "X:1\nT:RB\nM:4/4\nL:1/4\nK:C\n|: C D E F :: G A B c :|\n";
    let score = assert_idempotent_s6a(abc);
    // Two measures. The `::` seam sits between them: measure 0's RIGHT is the
    // RepeatEnd (the `::` back half), measure 1's LEFT is the leading RepeatStart
    // (the `::` forward half). Measure 0 also opens with `|:` (RepeatStart) and
    // measure 1 closes with `:|` (RepeatEnd), so both measures carry one of each.
    assert!(
        measure_at(&score, 0)
            .barlines
            .iter()
            .any(|b| b.kind == BarlineKind::RepeatEnd),
        "the `::` seam's back half is a RepeatEnd on measure 0's right"
    );
    assert!(
        measure_at(&score, 1)
            .barlines
            .iter()
            .any(|b| b.kind == BarlineKind::RepeatStart),
        "the `::` seam's forward half is a leading RepeatStart on measure 1"
    );
    // No RepeatBoth is ever reconstructed.
    assert!(
        score.parts[0].voices[0]
            .measures
            .iter()
            .all(|m| m.barlines.iter().all(|b| b.kind != BarlineKind::RepeatBoth)),
        "the reader decomposes `::` rather than materialising RepeatBoth"
    );
}

#[test]
fn first_and_second_endings_reconstruct() {
    use crate::model::{BarlineKind, RepeatEndingPartModel};
    // |: ... |1 ... :|2 ... |] : the 1st ending opens on the left of one measure
    // and the 2nd ending opens on the left of the next; the reader reconstructs a
    // RepeatEndingModel { Single(1) } / { Single(2) } at each OPEN measure. The
    // <ending type="stop"> closers are regenerated by the writer's schedule from
    // the open positions + barline kinds, so they are not stored.
    let abc = "X:1\nT:E\nM:4/4\nL:1/4\nK:C\n|: C D E F |1 G A B c :|2 c B A G |]\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<ending number=\"1\" type=\"start\"/>")
            && x1.contains("<ending number=\"2\" type=\"start\"/>"),
        "precondition: 1st/2nd endings emit numbered starts"
    );
    let score = assert_idempotent_s6a(abc);
    let first = score.parts[0].voices[0]
        .measures
        .iter()
        .find_map(|m| m.repeat_endings.first())
        .expect("a 1st-ending RepeatEndingModel is reconstructed");
    assert_eq!(first.endings, vec![RepeatEndingPartModel::Single(1)]);
    let second_count: usize = score.parts[0].voices[0]
        .measures
        .iter()
        .map(|m| m.repeat_endings.len())
        .sum();
    assert_eq!(second_count, 2, "exactly two ending brackets open");
    // The 2nd ending's bracket closes on the |] (Final) of the last measure.
    assert!(
        score.parts[0].voices[0]
            .measures
            .iter()
            .any(|m| m.barlines.iter().any(|b| b.kind == BarlineKind::Final)),
        "the final |] is reconstructed (it closes the 2nd ending via the schedule)"
    );
}

#[test]
fn ending_range_reconstructs_parts() {
    use crate::model::RepeatEndingPartModel;
    // `|1,2` (or `[1,2`) is a single bracket covering passes 1 and 2 -> the writer
    // emits <ending number="1,2" type="start">; the reader splits the comma list
    // back into two Single parts.
    let abc = "X:1\nT:Rng\nM:4/4\nL:1/4\nK:C\n|: C D E F |1,2 G A B c :|\n";
    let x1 = export(abc);
    assert!(
        x1.contains("type=\"start\""),
        "precondition: the combined ending emits a start, got:\n{x1}"
    );
    let score = assert_idempotent_s6a(abc);
    let ending = score.parts[0].voices[0]
        .measures
        .iter()
        .find_map(|m| m.repeat_endings.first())
        .expect("a combined-ending RepeatEndingModel is reconstructed");
    assert_eq!(
        ending.endings,
        vec![
            RepeatEndingPartModel::Single(1),
            RepeatEndingPartModel::Single(2)
        ],
        "the `1,2` number list reconstructs two Single parts"
    );
}

#[test]
fn no_barline_directives_leave_measure_lists_empty() {
    // A plain two-bar tune with only ordinary `|` measure boundaries emits NO
    // <barline> blocks (Regular barlines are implicit). The reader must not
    // fabricate any MeasureBarline or RepeatEndingModel.
    let score = assert_idempotent_s6a("X:1\nT:Plain\nM:4/4\nL:1/4\nK:C\nC D E F | G A B c |\n");
    assert!(
        score.parts[0].voices[0]
            .measures
            .iter()
            .all(|m| m.barlines.is_empty() && m.repeat_endings.is_empty()),
        "plain `|` boundaries reconstruct no barline/ending model entries"
    );
}

// --- Stage S6b: mid-measure <attributes> (key / meter / clef changes) --------

/// Assert FULL-byte idempotence on an S6b single-voice fixture. A mid-measure
/// `<attributes>` block (a SECOND `<attributes>` following notes within a
/// measure, or the first `<attributes>` of a non-leading measure) carries a
/// `KeyChange`/`MeterChange`/`ClefChange`; in a single-voice fixture nothing is
/// deferred, so the whole document must round-trip byte-for-byte. Returns the
/// reconstructed score for direct model-field assertions.
fn assert_idempotent_s6b(abc: &str) -> Score {
    let (x1, x2, score) = round_trip(abc);
    assert_eq!(
        x1, x2,
        "write(read(write(score))) must equal write(score) byte-for-byte (S6b mid-measure attrs)"
    );
    score
}

/// All `TimedEventKind::KeyChange` models in the first part's first voice.
fn key_changes(score: &Score) -> Vec<&crate::model::KeySignatureModel> {
    score.parts[0].voices[0]
        .events
        .iter()
        .filter_map(|event| match &event.kind {
            TimedEventKind::KeyChange(key) => Some(key),
            _ => None,
        })
        .collect()
}

/// All `TimedEventKind::MeterChange` models in the first part's first voice.
fn meter_changes(score: &Score) -> Vec<&crate::model::MeterModel> {
    score.parts[0].voices[0]
        .events
        .iter()
        .filter_map(|event| match &event.kind {
            TimedEventKind::MeterChange(meter) => Some(meter),
            _ => None,
        })
        .collect()
}

/// All `TimedEventKind::ClefChange` models in the first part's first voice.
fn clef_changes(score: &Score) -> Vec<&crate::model::ClefChangeModel> {
    score.parts[0].voices[0]
        .events
        .iter()
        .filter_map(|event| match &event.kind {
            TimedEventKind::ClefChange(clef) => Some(clef),
            _ => None,
        })
        .collect()
}

#[test]
fn inline_key_change_reconstructs_event() {
    // A mid-tune [K:G] becomes a SECOND <attributes> with only a <key> block,
    // emitted between notes. The reader must reconstruct a KeyChange event at the
    // current onset (NOT touch the header metadata.key) so it re-emits in place.
    let abc = "X:1\nT:K\nM:4/4\nL:1/4\nK:C\nC D [K:G] E F |\n";
    let x1 = export(abc);
    assert!(
        x1.matches("<attributes>").count() == 2,
        "precondition: an inline [K:] emits a SECOND mid-measure <attributes>"
    );
    let score = assert_idempotent_s6b(abc);
    // The header key is unchanged (C major, fifths 0).
    assert_eq!(
        header_key(&score).fifths,
        0,
        "the inline key change must not overwrite the header key"
    );
    let changes = key_changes(&score);
    assert_eq!(changes.len(), 1, "exactly one KeyChange event");
    assert_eq!(
        changes[0].fifths, 1,
        "the KeyChange carries G major (fifths 1)"
    );
    // The change sits at onset 2/4 (after two quarter notes), as a zero-duration
    // event in the same measure as the notes.
    let event = score.parts[0].voices[0]
        .events
        .iter()
        .find(|event| matches!(event.kind, TimedEventKind::KeyChange(_)))
        .expect("a KeyChange event");
    assert_eq!(
        event.onset,
        Fraction::new(2, 4),
        "KeyChange onset is after C D"
    );
    assert_eq!(
        event.duration,
        Fraction::zero(),
        "KeyChange is zero-duration"
    );
}

#[test]
fn body_key_change_at_measure_start_reconstructs_event() {
    // A body-field `K:G` between two measures emits a <attributes> with only a
    // <key> as the FIRST child of the next measure (onset 0). That first-child
    // <attributes> in a NON-leading measure is a mid-tune KeyChange, not a header
    // block (the header attributes only appear in the part's first measure).
    let abc = "X:1\nT:K\nM:2/4\nL:1/4\nK:C\nC D |\nK:G\nE F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<measure number=\"2\">") && x1.matches("<attributes>").count() == 2,
        "precondition: the body K: emits a leading <attributes> in measure 2"
    );
    let score = assert_idempotent_s6b(abc);
    let changes = key_changes(&score);
    assert_eq!(changes.len(), 1, "one KeyChange for the body K:");
    assert_eq!(changes[0].fifths, 1, "G major (fifths 1)");
    // It lives in measure 2 (index 1) at onset 0.
    let event = score.parts[0].voices[0]
        .events
        .iter()
        .find(|event| matches!(event.kind, TimedEventKind::KeyChange(_)))
        .expect("a KeyChange event");
    assert_eq!(
        event.measure.index, 1,
        "the KeyChange is in the second measure"
    );
    assert_eq!(
        event.onset,
        Fraction::zero(),
        "at the measure start (onset 0)"
    );
}

#[test]
fn inline_meter_change_reconstructs_event() {
    // A mid-tune [M:2/4] emits a SECOND <attributes> with only a <time> block.
    let abc = "X:1\nT:M\nM:4/4\nL:1/4\nK:C\nC D [M:2/4] E F |\n";
    let x1 = export(abc);
    assert!(
        x1.matches("<attributes>").count() == 2 && x1.contains("<beats>2</beats>"),
        "precondition: inline [M:] emits a mid-measure <time>"
    );
    let score = assert_idempotent_s6b(abc);
    let changes = meter_changes(&score);
    assert_eq!(changes.len(), 1, "exactly one MeterChange event");
    assert_eq!(
        changes[0].display, "2/4",
        "the MeterChange display re-emits the 2/4 <time>"
    );
}

#[test]
fn inline_clef_change_reconstructs_event() {
    // A mid-tune [K:clef=bass] emits a SECOND <attributes> with only a <clef>.
    let abc = "X:1\nT:Cl\nM:4/4\nL:1/4\nK:C\nC D [K:clef=bass] E F |\n";
    let x1 = export(abc);
    assert!(
        x1.matches("<attributes>").count() == 2 && x1.contains("<sign>F</sign>"),
        "precondition: inline clef change emits a mid-measure <clef> (bass = F/4)"
    );
    let score = assert_idempotent_s6b(abc);
    let changes = clef_changes(&score);
    assert_eq!(changes.len(), 1, "exactly one ClefChange event");
    assert_eq!(
        changes[0].clef.text, "bass",
        "the ClefChange reconstructs the canonical bass clef text"
    );
}

#[test]
fn inline_key_and_meter_change_in_one_measure_round_trip() {
    // `[K:G][M:2/4]` are two SEPARATE zero-duration events at the same onset; the
    // writer emits each as its own mid-measure <attributes> (key first, then
    // time), so two events must reconstruct in that order.
    let abc = "X:1\nT:KM\nM:4/4\nL:1/4\nK:C\nC D [K:G][M:2/4] E F |\n";
    let x1 = export(abc);
    assert!(
        x1.matches("<attributes>").count() == 3,
        "precondition: header + key + meter = three <attributes> blocks"
    );
    let score = assert_idempotent_s6b(abc);
    assert_eq!(key_changes(&score).len(), 1, "one KeyChange");
    assert_eq!(meter_changes(&score).len(), 1, "one MeterChange");
    // The KeyChange must be ordered before the MeterChange (writer emits key
    // first), both at the same onset, both before the following E note.
    let kinds: Vec<&str> = score.parts[0].voices[0]
        .events
        .iter()
        .filter_map(|event| match &event.kind {
            TimedEventKind::KeyChange(_) => Some("key"),
            TimedEventKind::MeterChange(_) => Some("meter"),
            TimedEventKind::Note(_) => Some("note"),
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds,
        vec!["note", "note", "key", "meter", "note", "note"],
        "key change precedes meter change, both between the note pairs"
    );
}

#[test]
fn header_attributes_in_first_measure_are_not_an_event() {
    // The leading <attributes> (write_attributes) in the part's FIRST measure is
    // the header key/time/clef, NOT a mid-tune change. A plain tune with no inline
    // attribute change must reconstruct ZERO change events.
    let score = assert_idempotent_s6b("X:1\nT:Plain\nM:4/4\nL:1/4\nK:C\nC D E F |\n");
    assert!(
        key_changes(&score).is_empty()
            && meter_changes(&score).is_empty()
            && clef_changes(&score).is_empty(),
        "the header <attributes> must not be reconstructed as change events"
    );
}

#[test]
fn key_change_then_more_notes_in_later_measure_round_trip() {
    // A body K: in measure 2 followed by more music, plus a header in measure 1:
    // confirms the first-measure header is skipped while the measure-2 leading
    // <attributes> is a KeyChange, and that following notes keep correct onsets.
    let abc = "X:1\nT:Multi\nM:4/4\nL:1/4\nK:C\nC D E F |\nK:Am\nG A B c | d e f g |\n";
    let score = assert_idempotent_s6b(abc);
    let changes = key_changes(&score);
    assert_eq!(changes.len(), 1, "one body KeyChange");
    // A minor has no sharps/flats (fifths 0); the round-trip is what proves the
    // <key> re-emits, the count proves it is an event not a header rewrite.
    assert_eq!(changes[0].fifths, 0, "A minor is fifths 0");
}

#[test]
fn annotation_before_inline_key_change_round_trips() {
    // An annotation immediately before an inline [K:G]: the writer emits the
    // KeyChange <attributes> FIRST (it sorts before the following note at the same
    // onset), THEN the <direction> annotation (attached to the next note), THEN
    // the note. The reader must reconstruct the KeyChange as an event AND keep the
    // annotation buffered onto the following note so the order re-emits.
    let abc = "X:1\nT:E\nM:4/4\nL:1/4\nK:C\nC D \"^hi\"[K:G] E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<words>hi</words>") && x1.matches("<attributes>").count() == 2,
        "precondition: annotation + inline key change both present"
    );
    let score = assert_idempotent_s6b(abc);
    assert_eq!(key_changes(&score).len(), 1, "one KeyChange event");
    // The annotation lands on a NOTE event (the one after the key change), never
    // on the zero-duration KeyChange event itself.
    let total_annotations: usize = score.parts[0].voices[0]
        .events
        .iter()
        .map(|event| event.attachments.annotations.len())
        .sum();
    assert_eq!(total_annotations, 1, "exactly one annotation reconstructed");
    let annotated_is_note = score.parts[0].voices[0].events.iter().any(|event| {
        !event.attachments.annotations.is_empty() && matches!(event.kind, TimedEventKind::Note(_))
    });
    assert!(
        annotated_is_note,
        "the annotation attaches to a note (the one after the key change)"
    );
}

#[test]
fn chord_symbol_before_inline_key_change_round_trips() {
    // A chord symbol "Am" before [K:G]: writer emits the KeyChange <attributes>,
    // then the <harmony>, then the note. The reader keeps the harmony buffered
    // onto the following note while reconstructing the KeyChange event.
    let abc = "X:1\nT:E3\nM:4/4\nL:1/4\nK:C\nC D \"Am\"[K:G] E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<harmony>") && x1.matches("<attributes>").count() == 2,
        "precondition: chord symbol + inline key change both present"
    );
    let score = assert_idempotent_s6b(abc);
    assert_eq!(key_changes(&score).len(), 1, "one KeyChange event");
    let total_chord_symbols: usize = score.parts[0].voices[0]
        .events
        .iter()
        .map(|event| event.attachments.chord_symbols.len())
        .sum();
    assert_eq!(
        total_chord_symbols, 1,
        "exactly one chord symbol reconstructed"
    );
    let on_note = score.parts[0].voices[0].events.iter().any(|event| {
        !event.attachments.chord_symbols.is_empty() && matches!(event.kind, TimedEventKind::Note(_))
    });
    assert!(
        on_note,
        "the chord symbol attaches to the note after the key change"
    );
}

#[test]
fn mid_tune_key_change_with_explicit_accidentals_round_trips() {
    // A mid-tune key with explicit accidentals exercises read_key's
    // <key-step>/<key-alter>/<key-accidental> path inside a mid-measure block.
    let abc = "X:1\nT:Exp\nM:4/4\nL:1/4\nK:C\nC D [K:D exp _b ^f] E F |\n";
    let x1 = export(abc);
    assert!(
        x1.matches("<attributes>").count() == 2 && x1.contains("<key-accidental>"),
        "precondition: the inline key change emits explicit <key-accidental>s"
    );
    let score = assert_idempotent_s6b(abc);
    let changes = key_changes(&score);
    assert_eq!(changes.len(), 1, "one KeyChange event");
    assert!(
        !changes[0].explicit_accidentals.is_empty(),
        "the KeyChange reconstructs its explicit accidentals"
    );
}

#[test]
fn mid_tune_treble_clef_change_round_trips() {
    // A mid-tune clef change BACK to treble still emits a <clef> (G/2) the writer
    // produces unconditionally for a ClefChange. The reader must reconstruct a
    // ClefChange with the canonical "treble" text (NOT None), so the G/2 <clef>
    // re-emits in place rather than being dropped.
    let abc = "X:1\nT:Tr\nM:4/4\nL:1/4\nK:C clef=bass\nC D [K:clef=treble] E F |\n";
    let x1 = export(abc);
    // The header is bass (F/4); the mid-tune change is treble (G/2).
    assert!(
        x1.matches("<attributes>").count() == 2 && x1.matches("<sign>G</sign>").count() == 1,
        "precondition: header bass + a mid-tune treble clef change"
    );
    let score = assert_idempotent_s6b(abc);
    let changes = clef_changes(&score);
    assert_eq!(changes.len(), 1, "one ClefChange event");
    assert_eq!(
        changes[0].clef.text, "treble",
        "a mid-tune treble clef reconstructs the explicit canonical text"
    );
}

#[test]
fn mid_measure_meter_change_before_body_tempo_round_trips() {
    // tune_005141 shape: a header with NO meter (K: only), then a body M:C and a
    // body Q: before the first note. The writer emits the header <attributes>,
    // then the meter-change <attributes> (a MeterChange at onset 0), THEN the
    // body tempo <direction> (a TempoChange, sorted after the meter change). The
    // reader must NOT promote that post-change tempo to the header tempo_model —
    // it follows a mid-measure attributes block, so it is a body TempoChange.
    let abc = "X:1\nT:NoHdrMeter\nK:C\nM:C\nQ:1/8=120\nC D E F |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<time symbol=\"common\">") && x1.contains("<metronome>"),
        "precondition: a body meter AND a body tempo are both present"
    );
    // The meter-change <attributes> must precede the tempo <direction> in X1.
    let meter_pos = x1
        .find("<time symbol=\"common\">")
        .expect("the meter change is present");
    let tempo_pos = x1.find("<metronome>").expect("the body tempo is present");
    assert!(
        meter_pos < tempo_pos,
        "precondition: the writer emits the meter change before the body tempo"
    );
    let score = assert_idempotent_s6b(abc);
    assert_eq!(meter_changes(&score).len(), 1, "one MeterChange event");
    assert!(
        score.metadata.tempo_model.is_none(),
        "a tempo after a mid-measure attributes change is a body TempoChange, \
         not the header tempo_model"
    );
    let tempo_changes = score.parts[0].voices[0]
        .events
        .iter()
        .filter(|event| matches!(event.kind, TimedEventKind::TempoChange(_)))
        .count();
    assert_eq!(
        tempo_changes, 1,
        "the body tempo reconstructs one TempoChange"
    );
}

#[test]
fn annotation_then_inline_change_in_noteless_measure_round_trips() {
    // S6e ordering edge (the documented 22-file residual). A standalone chord
    // symbol/annotation immediately followed by an inline key change in a measure
    // with NO following note: `"Trio"[K:F]` alone in its bar, the next note past a
    // `|:` in the following bar. The writer emits the `<direction>` (the buffered
    // attachment) BEFORE the change's `<attributes>` — its Spacer's source predates
    // the change's, so the stable (onset, source.start) sort orders it first. With
    // no following note to host the annotation, the reader builds a trailing Spacer;
    // it must insert that Spacer BEFORE the same-onset KeyChange so the re-emission
    // keeps `<direction>` ahead of `<attributes>`.
    let abc = "X:1\nT:Trio test\nM:4/4\nL:1/8\nK:C\nA2 B2 c2 d2 |\"Trio\"[K:F]|: A2 B2 c2 d2 |\n";
    let x1 = export(abc);
    // Precondition: the writer emits the direction before the key-change attributes
    // inside the note-less measure.
    let dir_pos = x1.find("<words>Trio</words>").expect("the Trio direction");
    let key_pos = x1
        .find("<fifths>-1</fifths>")
        .expect("the inline key change");
    assert!(
        dir_pos < key_pos,
        "precondition: writer emits the <direction> before the key-change <attributes>"
    );
    // The fix: full-byte idempotence (the order is preserved on re-write).
    let score = assert_idempotent_s6b(abc);
    assert_eq!(key_changes(&score).len(), 1, "one KeyChange event");
    // The note-less measure has both a KeyChange and a Spacer at onset 0, and the
    // Spacer (carrying the Trio chord symbol) precedes the KeyChange in the event
    // stream so the stable sort emits them in writer order.
    let measure2: Vec<_> = score.parts[0].voices[0]
        .events
        .iter()
        .filter(|event| event.measure.index == 1)
        .collect();
    let spacer_idx = measure2
        .iter()
        .position(|event| matches!(event.kind, TimedEventKind::Spacer));
    let key_idx = measure2
        .iter()
        .position(|event| matches!(event.kind, TimedEventKind::KeyChange(_)));
    assert!(
        matches!((spacer_idx, key_idx), (Some(s), Some(k)) if s < k),
        "the trailing Spacer must precede the same-onset KeyChange (got spacer={spacer_idx:?}, key={key_idx:?})"
    );
}

#[test]
fn trailing_decoration_after_notes_still_appends_after_them() {
    // Guard the OTHER branch of the S6e Spacer insertion: a trailing decoration
    // with notes BEFORE it and NO mid-tune change at its onset must still append
    // after the notes (the pre-existing pre-barline `!segno!` case), not jump
    // ahead of anything. `A2 B2 c2 d2 !segno!|` puts the segno after the 4 notes.
    let abc = "X:1\nT:Seg\nM:4/4\nL:1/8\nK:C\nA2 B2 c2 d2 !segno!|\n";
    let score = assert_idempotent_s6b(abc);
    // The last event is the Spacer carrying the segno decoration, after the notes.
    let events = &score.parts[0].voices[0].events;
    let last = events.last().expect("at least one event");
    assert!(
        matches!(last.kind, TimedEventKind::Spacer),
        "the trailing decoration lands on a Spacer appended after the notes"
    );
    assert_eq!(
        last.attachments.decorations.len(),
        1,
        "the Spacer carries the segno decoration"
    );
}

// --- Stage S6c: grace notes (<grace>) + chords (<chord/>) --------------------

/// Assert FULL-byte idempotence on an S6c single-voice fixture. By S6c the writer
/// emits `<grace>` notes (slash/before/after-grace, grace chords, grace slurs) and
/// `<chord/>` members; in a single-voice grace/chord fixture nothing is deferred,
/// so the whole document must round-trip byte-for-byte. Returns the reconstructed
/// score for direct model-field assertions.
fn assert_idempotent_s6c(abc: &str) -> Score {
    let (x1, x2, score) = round_trip(abc);
    assert_eq!(
        x1, x2,
        "write(read(write(score))) must equal write(score) byte-for-byte (S6c grace+chord)"
    );
    score
}

/// The first event's `grace_groups` in the first part's first voice.
fn grace_groups_at(score: &Score, index: usize) -> &[crate::model::GraceGroupAttachment] {
    &score.parts[0].voices[0].events[index]
        .attachments
        .grace_groups
}

#[test]
fn single_grace_round_trips_and_reconstructs_group() {
    use crate::model::GraceEventKind;
    // {a}G : one grace note (<type>eighth</type>, base unit 1/8, count 1) before G.
    let abc = "X:1\nT:G\nL:1/4\nK:C\n{a}G\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<grace/>") && !x1.contains("slash"),
        "precondition: a plain {{a}} emits an unslashed <grace/>"
    );
    let score = assert_idempotent_s6c(abc);
    // The grace group attaches to the FOLLOWING main note (event 0 = G).
    let groups = grace_groups_at(&score, 0);
    assert_eq!(groups.len(), 1, "one grace group on the following note");
    assert_eq!(groups[0].note_count, 1, "one grace element");
    assert!(groups[0].slash.is_none(), "an unslashed grace");
    assert_eq!(groups[0].events.len(), 1);
    match &groups[0].events[0].kind {
        GraceEventKind::Note(note) => {
            assert_eq!(note.pitch.step, 'A');
            assert_eq!(note.pitch.octave, 5);
            assert_eq!(
                note.length_multiplier,
                Fraction::one(),
                "a plain grace note has length multiplier 1"
            );
        }
        other => panic!("expected a grace Note, got {other:?}"),
    }
}

#[test]
fn multi_note_grace_group_round_trips() {
    // {abc}G : three grace notes (each <type>16th</type>, base unit 1/16, count 3).
    let abc = "X:1\nT:G\nL:1/4\nK:C\n{abc}G\n";
    let x1 = export(abc);
    assert_eq!(
        x1.matches("<grace/>").count(),
        3,
        "precondition: three <grace/> notes"
    );
    let score = assert_idempotent_s6c(abc);
    let groups = grace_groups_at(&score, 0);
    assert_eq!(groups.len(), 1, "one grace group");
    assert_eq!(groups[0].note_count, 3, "three grace elements");
    assert_eq!(groups[0].events.len(), 3, "three grace events");
}

#[test]
fn slashed_grace_round_trips_and_reconstructs_slash() {
    // {/a}G : an acciaccatura -> <grace slash="yes"/>.
    let abc = "X:1\nT:G\nL:1/4\nK:C\n{/a}G\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<grace slash=\"yes\"/>"),
        "precondition: {{/a}} emits slash=\"yes\""
    );
    let score = assert_idempotent_s6c(abc);
    let groups = grace_groups_at(&score, 0);
    assert_eq!(groups.len(), 1);
    assert!(
        groups[0].slash.is_some(),
        "the reconstructed group records the slash"
    );
}

#[test]
fn grace_length_multiplier_round_trips() {
    use crate::model::GraceEventKind;
    // {a2}G : a single grace note with written length 2 -> 1/8 * 2 = 1/4 ->
    // <type>quarter</type>. The reader must recover length_multiplier = 2.
    let abc = "X:1\nT:G\nL:1/4\nK:C\n{a2}G\n";
    let score = assert_idempotent_s6c(abc);
    let groups = grace_groups_at(&score, 0);
    match &groups[0].events[0].kind {
        GraceEventKind::Note(note) => assert_eq!(
            note.length_multiplier,
            Fraction::new(2, 1),
            "{{a2}} reconstructs a length multiplier of 2"
        ),
        other => panic!("expected a grace Note, got {other:?}"),
    }
}

#[test]
fn grace_half_length_multiplier_round_trips() {
    use crate::model::GraceEventKind;
    // {a/}G : a single grace note with written length 1/2 -> 1/8 * 1/2 = 1/16 ->
    // <type>16th</type>. The reader must recover length_multiplier = 1/2.
    let abc = "X:1\nT:G\nL:1/4\nK:C\n{a/}G\n";
    let score = assert_idempotent_s6c(abc);
    let groups = grace_groups_at(&score, 0);
    match &groups[0].events[0].kind {
        GraceEventKind::Note(note) => assert_eq!(
            note.length_multiplier,
            Fraction::new(1, 2),
            "{{a/}} reconstructs a length multiplier of 1/2"
        ),
        other => panic!("expected a grace Note, got {other:?}"),
    }
}

#[test]
fn grace_with_slur_round_trips() {
    use crate::model::SlurRole;
    // {(ab)}G : a slur opens and closes INSIDE the grace braces, binding the two
    // grace notes. The writer emits <slur type="start"> on the first grace note
    // and <slur type="stop"> on the second (both in <notations>).
    let abc = "X:1\nT:G\nL:1/4\nK:C\n{(ab)}G\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<slur type=\"start\" number=\"1\"/>")
            && x1.contains("<slur type=\"stop\" number=\"1\"/>"),
        "precondition: a grace slur emits start+stop slurs"
    );
    let score = assert_idempotent_s6c(abc);
    let groups = grace_groups_at(&score, 0);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].events.len(), 2, "two grace notes");
    let start = &groups[0].events[0].slurs;
    assert_eq!(start.len(), 1, "first grace note opens a slur");
    assert_eq!(start[0].role, SlurRole::Start);
    let stop = &groups[0].events[1].slurs;
    assert_eq!(stop.len(), 1, "second grace note closes the slur");
    assert_eq!(stop[0].role, SlurRole::Stop);
}

#[test]
fn slur_opening_before_grace_binds_first_grace_note() {
    use crate::model::SlurRole;
    // ({a}G)A : the slur opens BEFORE the grace brace, so its start binds to the
    // first grace note and its stop to the main note G. Re-emission must place the
    // start slur on the grace note again.
    let abc = "X:1\nT:G\nL:1/4\nK:C\n({a}G)A\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<slur type=\"start\" number=\"1\"/>"),
        "precondition: the slur start lands on the grace note"
    );
    let score = assert_idempotent_s6c(abc);
    let groups = grace_groups_at(&score, 0);
    let start = &groups[0].events[0].slurs;
    assert_eq!(start.len(), 1, "the grace note carries the slur start");
    assert_eq!(start[0].role, SlurRole::Start);
}

#[test]
fn grace_chord_round_trips_and_reconstructs_members() {
    use crate::model::GraceEventKind;
    // {[ac]}G : a grace CHORD (one grace element, count 1) whose second member
    // carries <chord/>. The reader must reconstruct a GraceEventKind::Chord with
    // two members, not two separate grace notes.
    let abc = "X:1\nT:G\nL:1/4\nK:C\n{[ac]}G\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<chord/>") && x1.matches("<grace/>").count() == 2,
        "precondition: a grace chord emits two <grace/> with one <chord/>"
    );
    let score = assert_idempotent_s6c(abc);
    let groups = grace_groups_at(&score, 0);
    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0].note_count, 1,
        "a grace chord is ONE grace element (base unit 1/8)"
    );
    assert_eq!(groups[0].events.len(), 1, "one grace event (the chord)");
    match &groups[0].events[0].kind {
        GraceEventKind::Chord(members) => {
            assert_eq!(members.len(), 2, "the grace chord has two members");
            assert_eq!(members[0].pitch.step, 'A');
            assert_eq!(members[1].pitch.step, 'C');
        }
        other => panic!("expected a grace Chord, got {other:?}"),
    }
}

#[test]
fn grace_rest_round_trips() {
    use crate::model::{GraceEventKind, RestVisibility};
    // {x}G : a grace REST (invisible x rest) -> <note print-object="no"><grace/>
    // <rest/>. The reader must reconstruct a GraceEventKind::Rest.
    let abc = "X:1\nT:G\nL:1/4\nK:C\n{x}G\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<grace/>") && x1.contains("<rest/>"),
        "precondition: a grace rest emits <grace/> with <rest/>"
    );
    let score = assert_idempotent_s6c(abc);
    let groups = grace_groups_at(&score, 0);
    assert_eq!(groups[0].events.len(), 1);
    match &groups[0].events[0].kind {
        GraceEventKind::Rest(rest) => {
            assert_eq!(
                rest.visibility,
                RestVisibility::Invisible,
                "an x grace rest is invisible (print-object=no)"
            );
        }
        other => panic!("expected a grace Rest, got {other:?}"),
    }
}

#[test]
fn after_grace_at_measure_end_binds_to_preceding_note() {
    // Te6{de}|... : a trailing grace group with NO following note in measure 1
    // binds as an AFTER-grace on the preceding (decorated) note. The grace <note>s
    // are the last elements of measure 1; the reader must attach them to that
    // note's after_grace_groups so they re-emit after it.
    let abc = "X:1\nT:Trailing\nM:4/4\nL:1/8\nK:C\nTe6{de}|d2f f2f|\n";
    let score = assert_idempotent_s6c(abc);
    let after = &score.parts[0].voices[0].events[0]
        .attachments
        .after_grace_groups;
    assert_eq!(
        after.len(),
        1,
        "the preceding note carries one after-grace group"
    );
    assert_eq!(
        after[0].note_count, 2,
        "the after-grace group has two notes"
    );
    // And it is NOT a (before) grace group on that note.
    assert!(
        score.parts[0].voices[0].events[0]
            .attachments
            .grace_groups
            .is_empty(),
        "the trailing grace is an after-grace, not a before-grace"
    );
}

/// Hand-crafted MusicXML the writer never emits: a `<grace>` note immediately
/// followed by a `<chord/>` member note (no plain main note between to drain the
/// open grace run). The chord member folds into the previous event and `continue`s
/// before the buffered before-grace groups flush, so `pending_graces` reaches the
/// measure-end finalisation non-empty. This is the path the removed `debug_assert!`
/// guarded; totality (design §2.2/§6) requires a graceful degrade, not a panic.
/// Here a preceding `G` note hosts the orphaned group as a before-grace group.
#[test]
fn grace_before_chord_member_degrades_without_panic() {
    use crate::model::TimedEventKind;
    // G (main) ; {a} (grace, opens the run) ; [chord member c] carrying <chord/>
    // with no plain note between. roxmltree DOM is fed directly — there is no ABC
    // that lowers to this shape, so it must be authored by hand.
    let xml = r#"<?xml version="1.0"?>
<score-partwise>
  <part-list><score-part id="P1"><part-name>P</part-name></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <attributes><divisions>8</divisions></attributes>
      <note>
        <pitch><step>G</step><octave>4</octave></pitch>
        <duration>8</duration>
        <voice>1</voice>
        <type>quarter</type>
      </note>
      <note>
        <grace/>
        <pitch><step>A</step><octave>5</octave></pitch>
        <voice>1</voice>
        <type>eighth</type>
      </note>
      <note>
        <chord/>
        <pitch><step>C</step><octave>5</octave></pitch>
        <duration>8</duration>
        <voice>1</voice>
        <type>quarter</type>
      </note>
    </measure>
  </part>
</score-partwise>"#;
    // Totality: must return, never panic.
    let report = read_musicxml(xml);
    let score = report.value;
    // The G note was promoted to a chord (the <chord/> member folded into it),
    // and the orphaned before-grace group was re-bound to that event rather than
    // dropped or panicking on.
    let event = &score.parts[0].voices[0].events[0];
    assert!(
        matches!(event.kind, TimedEventKind::Chord(_)),
        "the <chord/> member folds the leading G note into a chord"
    );
    assert_eq!(
        event.attachments.grace_groups.len(),
        1,
        "the orphaned before-grace group is re-bound to the hosting event, not lost"
    );
    // No panic occurred (we reached here); the degrade need not emit a diagnostic
    // on this arm because the group was preserved, not dropped.
}

/// The other fallback arm: a `<grace>` run followed by a `<chord/>` member note
/// with NO preceding main event at all. The grace run drains to `pending_graces`,
/// the chord member finds no head to fold onto (warned), and the orphaned before-
/// grace groups reach finalisation with `last_main_event == None`. The reader must
/// drop them with a diagnostic — and, crucially, not panic.
#[test]
fn orphan_before_grace_with_no_host_is_dropped_with_diagnostic() {
    // {a} (grace) then [chord member c] as the very first elements of the measure.
    let xml = r#"<?xml version="1.0"?>
<score-partwise>
  <part-list><score-part id="P1"><part-name>P</part-name></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <attributes><divisions>8</divisions></attributes>
      <note>
        <grace/>
        <pitch><step>A</step><octave>5</octave></pitch>
        <voice>1</voice>
        <type>eighth</type>
      </note>
      <note>
        <chord/>
        <pitch><step>C</step><octave>5</octave></pitch>
        <duration>8</duration>
        <voice>1</voice>
        <type>quarter</type>
      </note>
    </measure>
  </part>
</score-partwise>"#;
    // Totality: must return, never panic.
    let report = read_musicxml(xml);
    // The orphaned before-grace run was dropped with the documented diagnostic
    // (no host note existed); the chord member without a head also warns.
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "musicxml.read.orphan_before_grace"),
        "the host-less before-grace run must be dropped with a diagnostic, not panic"
    );
}

/// The first `ChordEvent` in the first part's first voice, with its event index.
fn first_chord(score: &Score) -> &crate::model::ChordEvent {
    score.parts[0].voices[0]
        .events
        .iter()
        .find_map(|event| match &event.kind {
            TimedEventKind::Chord(chord) => Some(chord),
            _ => None,
        })
        .expect("expected a ChordEvent")
}

#[test]
fn plain_chord_round_trips_and_reconstructs_members() {
    // [CEG] : the first note starts the chord; E and G carry <chord/>. The reader
    // must reconstruct ONE TimedEventKind::Chord with three members at one onset.
    let abc = "X:1\nT:C\nL:1/4\nK:C\n[CEG]\n";
    let x1 = export(abc);
    assert_eq!(
        x1.matches("<chord/>").count(),
        2,
        "precondition: a 3-note chord emits two <chord/> marks"
    );
    let score = assert_idempotent_s6c(abc);
    // Exactly one timed event (the chord), not three separate notes.
    assert_eq!(
        score.parts[0].voices[0].events.len(),
        1,
        "a chord is one TimedEvent::Chord, not three notes"
    );
    let chord = first_chord(&score);
    assert_eq!(chord.members.len(), 3, "three chord members");
    assert_eq!(chord.members[0].pitch.step, 'C');
    assert_eq!(chord.members[1].pitch.step, 'E');
    assert_eq!(chord.members[2].pitch.step, 'G');
    assert_eq!(
        chord.members[0].duration,
        Fraction::new(1, 4),
        "each member is a quarter"
    );
}

#[test]
fn chord_then_note_round_trips() {
    // [CEG] D E F | : a chord followed by plain notes. The chord is one event; the
    // following notes advance the cursor from the chord's onset.
    let abc = "X:1\nT:C\nM:4/4\nL:1/4\nK:C\n[CEG] D E F|\n";
    let score = assert_idempotent_s6c(abc);
    let events = &score.parts[0].voices[0].events;
    assert!(
        matches!(events[0].kind, TimedEventKind::Chord(_)),
        "the first event is the chord"
    );
    assert_eq!(events.len(), 4, "chord + three notes = four events");
    // The note after the chord starts at the chord's onset + its duration (1/4).
    assert_eq!(
        events[1].onset,
        Fraction::new(1, 4),
        "the note after the chord is at onset 1/4"
    );
}

#[test]
fn chord_with_ties_round_trips_per_member() {
    use crate::model::TieRole;
    // [CEG]2-[CEG]2 : a tied chord. Each member gets its OWN tie pair (numbers
    // 1,2,3 across the members), so the reader must reconstruct per-member ties
    // whose pair_ids re-derive the same <tied number=...>.
    let abc = "X:1\nT:C\nM:4/4\nL:1/4\nK:C\n[CEG]2-[CEG]2 z4|\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<tied type=\"start\" number=\"1\"/>")
            && x1.contains("<tied type=\"start\" number=\"3\"/>"),
        "precondition: tied chord members get distinct tie numbers 1..3"
    );
    let score = assert_idempotent_s6c(abc);
    let chord = first_chord(&score);
    assert_eq!(chord.members.len(), 3);
    // Every member of the first chord carries a tie START.
    for (member_index, member) in chord.members.iter().enumerate() {
        assert_eq!(
            member.attachments.ties.len(),
            1,
            "member {member_index} carries one tie"
        );
        assert_eq!(member.attachments.ties[0].role, TieRole::Start);
    }
}

#[test]
fn chord_member_decoration_round_trips() {
    // [C!trill!EG] : a decoration (trill) on the SECOND chord member. The reader
    // must reconstruct it onto that member's attachments so the writer re-emits
    // the <ornaments><trill-mark/> on the same member.
    let abc = "X:1\nT:C\nM:4/4\nL:1/4\nK:C\n[C!trill!EG] D E F|\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<trill-mark/>"),
        "precondition: the chord member decoration emits a trill"
    );
    let score = assert_idempotent_s6c(abc);
    let chord = first_chord(&score);
    // The trill lands on the E member (index 1), per the ABC ordering.
    let decorated = chord
        .members
        .iter()
        .filter(|member| !member.attachments.decorations.is_empty())
        .count();
    assert_eq!(decorated, 1, "exactly one member carries a decoration");
}

#[test]
fn two_chords_in_a_measure_round_trip() {
    // [CE][GB] : two chords back to back. Each must reconstruct as its own
    // ChordEvent with the right first-member attachments (this exercises the
    // writer's per-chord first-member attachment lookup, which keys on
    // source_span — so the reader must give each chord a distinct source span).
    let abc = "X:1\nT:C\nM:4/4\nL:1/4\nK:C\n[CE]2 [GB]2 z4|\n";
    let score = assert_idempotent_s6c(abc);
    let chords = score.parts[0].voices[0]
        .events
        .iter()
        .filter(|event| matches!(event.kind, TimedEventKind::Chord(_)))
        .count();
    assert_eq!(chords, 2, "two distinct chord events");
}

// --- S6d: multi-voice (<backup>/<voice>) + multiple-rest --------------------

/// Full-byte idempotence helper for S6d. Nothing in a multi-voice or
/// multiple-rest fixture is deferred (key/time/clef/notations/directions are all
/// reconstructed), so the whole document must be byte-identical.
fn assert_idempotent_full(abc: &str) -> Score {
    let (x1, x2, score) = round_trip(abc);
    assert_eq!(
        x1, x2,
        "write(read(write(score))) must equal write(score) byte-for-byte (S6d)"
    );
    score
}

#[test]
fn multiple_rest_reconstructs_measure_field() {
    // `Z2` expands to a two-bar multi-rest; the writer marks the first bar with
    // <measure-style><multiple-rest>2</multiple-rest>. The reader must set
    // Measure.multiple_rest so the glyph re-emits.
    let abc = "X:1\nT:Multi Rest\nM:4/4\nL:1/4\nK:C\nC D E F | Z2 | G A B c |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<multiple-rest>2</multiple-rest>"),
        "precondition: Z2 emits a multiple-rest of 2"
    );
    let score = assert_idempotent_full(abc);
    // The first measure of the rest run (measure index 1, number 2) carries the
    // multiple_rest count; the rest of the run does not.
    let with_mr = score.parts[0].voices[0]
        .measures
        .iter()
        .filter(|m| m.multiple_rest == Some(2))
        .count();
    assert_eq!(
        with_mr, 1,
        "exactly one measure carries multiple_rest = Some(2)"
    );
}

#[test]
fn two_voice_overlay_reconstructs_second_voice() {
    // A `&` overlay inside one voice forces a <backup> and a <voice>2 block.
    // The reader reconstructs the second voice as an additional Part.voices
    // entry so the writer re-emits the identical <voice>1 .. <backup> .. <voice>2.
    let abc = "X:1\nT:Overlay\nM:4/4\nL:1/4\nK:C\nC2 D2 & E2 F2 |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<backup>") && x1.contains("<voice>2</voice>"),
        "precondition: the overlay emits a <backup> and a <voice>2 block"
    );
    let score = assert_idempotent_full(abc);
    assert_eq!(
        score.parts[0].voices.len(),
        2,
        "the overlay reconstructs a second voice in the part"
    );
    // Voice 2 carries the E/F notes at onsets 0 and 1/2.
    let v2 = &score.parts[0].voices[1];
    assert_eq!(v2.events.len(), 2, "voice 2 has two notes");
    assert_eq!(
        v2.events[0].onset,
        Fraction::zero(),
        "voice 2 restarts at onset 0"
    );
    assert_eq!(
        v2.events[1].onset,
        Fraction::new(1, 2),
        "second voice-2 note at 1/2"
    );
}

#[test]
fn three_voice_overlay_round_trips() {
    // Two `&` overlays produce <voice>1/2/3 separated by two <backup>s.
    let abc = "X:1\nT:Triple\nM:4/4\nL:1/4\nK:C\nC2 D2 & E2 F2 & G2 A2 |\n";
    let x1 = export(abc);
    assert_eq!(
        x1.matches("<backup>").count(),
        2,
        "precondition: two overlays emit two backups"
    );
    assert!(
        x1.contains("<voice>3</voice>"),
        "precondition: a third voice"
    );
    let score = assert_idempotent_full(abc);
    assert_eq!(score.parts[0].voices.len(), 3, "three reconstructed voices");
}

#[test]
fn unequal_voice_lengths_round_trip() {
    // Voice 1 is a full bar; the overlay (voice 2) is half a bar. The writer
    // emits a single <backup> of the full voice-1 duration before voice 2.
    let abc = "X:1\nT:Unequal\nM:4/4\nL:1/4\nK:C\nC2 D2 E2 F2 & G2 A2 |\n";
    let score = assert_idempotent_full(abc);
    assert_eq!(score.parts[0].voices.len(), 2);
    assert_eq!(
        score.parts[0].voices[0].events.len(),
        4,
        "voice 1: four notes"
    );
    assert_eq!(
        score.parts[0].voices[1].events.len(),
        2,
        "voice 2: two notes"
    );
}

#[test]
fn two_voice_with_direction_round_trips() {
    // A dynamic on the main voice plus an overlay: the voice-bearing direction
    // must attach to its own voice's note and re-emit with the correct <voice>.
    let abc = "X:1\nT:Dyn Overlay\nM:4/4\nL:1/4\nK:C\n!f!C2 D2 & E2 F2 |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<dynamics>") && x1.contains("<backup>"),
        "precondition: a dynamic and an overlay backup are both emitted"
    );
    assert_idempotent_full(abc);
}

#[test]
fn two_voice_overlay_with_measure_rest_round_trips() {
    // The overlay voice is a full-measure rest (z4); the writer emits
    // <rest measure="yes"> for voice 2. The reader must reconstruct the
    // overlay voice's measure expected/actual so the attribute re-emits.
    let abc = "X:1\nT:Rest Overlay\nM:4/4\nL:1/4\nK:C\nC2 D2 E2 F2 & z4 |\n";
    let x1 = export(abc);
    if x1.contains("<backup>") {
        assert_idempotent_full(abc);
    }
}

#[test]
fn grace_in_overlay_voice_round_trips() {
    // A grace group on the main voice plus an overlay. Grace notes carry their
    // own <voice>, so the per-voice grace run must finalise onto its own voice's
    // following main note (not leak across the backup into the overlay).
    let abc = "X:1\nT:Grace Overlay\nM:4/4\nL:1/4\nK:C\n{ag}C2 D2 & E2 F2 |\n";
    let x1 = export(abc);
    if x1.contains("<backup>") && x1.contains("<grace") {
        assert_idempotent_full(abc);
    }
}

#[test]
fn chord_in_overlay_round_trips() {
    // A chord on the main voice plus an overlay: the per-voice chord head /
    // <chord/> folding must stay scoped to its own voice.
    let abc = "X:1\nT:Chord Overlay\nM:4/4\nL:1/4\nK:C\n[CEG]2 D2 & E2 F2 |\n";
    let x1 = export(abc);
    if x1.contains("<backup>") && x1.contains("<chord/>") {
        assert_idempotent_full(abc);
    }
}

#[test]
fn multiple_rest_second_bar_is_plain_measure_rest() {
    // The Z2 run's SECOND bar is an ordinary <rest measure="yes"> with no
    // multiple-rest glyph; only the first bar carries the count.
    let abc = "X:1\nT:Multi Rest 2\nM:4/4\nL:1/4\nK:C\nC D E F | Z3 | G A B c |\n";
    let x1 = export(abc);
    assert!(
        x1.contains("<multiple-rest>3</multiple-rest>"),
        "precondition: Z3 emits a multiple-rest of 3"
    );
    let score = assert_idempotent_full(abc);
    let counts: Vec<_> = score.parts[0].voices[0]
        .measures
        .iter()
        .filter_map(|m| m.multiple_rest)
        .collect();
    assert_eq!(
        counts,
        vec![3],
        "exactly one measure carries multiple_rest = 3"
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
    // When `READER_IDEMPOTENT_LIST` names a path, the sorted list of strictly
    // idempotent files is written there. Two runs (baseline vs a change) can then
    // be set-diffed to prove "0 regressions" for a fix — exactly the gate the S6e
    // ordering edge required.
    let mut idempotent_names: Vec<String> = Vec::new();

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
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                idempotent_names.push(name.to_owned());
            }
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

    if let Ok(list_path) = std::env::var("READER_IDEMPOTENT_LIST") {
        idempotent_names.sort();
        let _ = fs::write(&list_path, idempotent_names.join("\n"));
        eprintln!(
            "wrote {} idempotent file names to {list_path}",
            idempotent_names.len()
        );
    }

    let mut top: Vec<(String, usize)> = divergences.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    eprintln!(
        "reader corpus idempotence (strict, full bytes): {idempotent}/{exported} exported files round-trip ({total} total .abc)"
    );
    eprintln!(
        "reader corpus idempotence (supported subset, deferred S2/S3 blocks stripped): {idempotent_supported}/{exported}"
    );
    eprintln!("reader top first-diverging tags:");
    for (tag, count) in top.iter().take(10) {
        eprintln!("  {tag}: {count}");
    }

    // No hard count for S1 — most files use later-stage elements. We only
    // require the loop to be total (no panic) over the whole corpus.
    assert!(exported > 0, "expected at least one corpus file to export");
}

// --- Totality fuzz (design §6: read_musicxml must not panic on any file) -----

/// Collect every file with one of `exts` directly under `dir`, sorted. Used to
/// gather the abc2xml reference `.musicxml`/`.xml` set for the fuzz arm.
fn files_with_ext(dir: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = match fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|ext| exts.contains(&ext))
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    files.sort();
    files
}

/// Run `read_musicxml(input)` under `catch_unwind` and return `true` when it
/// completed without panicking. Totality (design §2.2/§6) requires this to be
/// `true` for every input — well-formed croma output, the abc2xml dialect, and
/// hand-crafted garbage alike.
fn reads_without_panic(input: &str) -> bool {
    // `&str` is unwind-safe; `AssertUnwindSafe` wraps the closure capturing it so
    // a panic anywhere in the reader becomes a caught error rather than aborting
    // the test binary. The reader holds no `&mut` across the boundary.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = read_musicxml(input);
    }))
    .is_ok()
}

/// Design §6 (REQUIRED): `read_musicxml` must not panic on any file in croma's
/// own 10k exports OR the abc2xml reference set, plus a handful of hand-crafted
/// malformed inputs. Each arm reports how many files it actually exercised; the
/// reference arm SAYS SO when `REF_ROOT`/the reference dir is absent. A panic in
/// any arm is collected (naming the file) and fails the test rather than aborting.
#[test]
fn totality_fuzz_read_musicxml_never_panics() {
    // Quiet the default panic hook so a *caught* panic does not spew a backtrace
    // for every offending file; we report offenders ourselves. Restored at the end.
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    // --- Arm 1: croma's own exports (always available via ABC_ROOT). ----------
    let mut own_exercised = 0usize;
    let mut own_panics: Vec<String> = Vec::new();
    match std::env::var("ABC_ROOT") {
        Ok(root) => {
            for path in abc_files(&PathBuf::from(&root)) {
                let Ok(bytes) = fs::read(&path) else { continue };
                let source = String::from_utf8_lossy(&bytes);
                // Only files that export cleanly produce XML to feed back.
                let Ok(export) = export_musicxml(&source) else {
                    continue;
                };
                own_exercised += 1;
                if !reads_without_panic(&export.musicxml) {
                    own_panics.push(path.display().to_string());
                }
            }
        }
        Err(_) => {
            eprintln!("totality fuzz: ABC_ROOT unset — own-exports arm skipped");
        }
    }

    // --- Arm 2: the abc2xml reference set (raw bytes; via REF_ROOT). ----------
    // These are the abc2xml/music21 dialect — elements croma's writer never
    // emits — so this arm exercises totality on foreign input, not idempotence.
    let mut ref_exercised = 0usize;
    let mut ref_panics: Vec<String> = Vec::new();
    let mut ref_available = false;
    if let Ok(ref_root) = std::env::var("REF_ROOT") {
        let dir = PathBuf::from(&ref_root);
        let files = files_with_ext(&dir, &["musicxml", "xml"]);
        if files.is_empty() {
            eprintln!(
                "totality fuzz: REF_ROOT={ref_root} has no .musicxml/.xml files — reference arm skipped"
            );
        } else {
            ref_available = true;
            for path in files {
                let Ok(bytes) = fs::read(&path) else { continue };
                let source = String::from_utf8_lossy(&bytes);
                ref_exercised += 1;
                if !reads_without_panic(&source) {
                    ref_panics.push(path.display().to_string());
                }
            }
        }
    } else {
        eprintln!("totality fuzz: REF_ROOT unset — abc2xml reference arm UNAVAILABLE (skipped)");
    }

    // --- Arm 3: hand-crafted malformed inputs (always run). -------------------
    // Two classes, both of which must NEVER panic:
    //
    // `must_diagnose` — genuinely malformed / unsupported documents (un-parseable
    //   XML, a non-MusicXML or timewise root, out-of-spec numbers). These degrade
    //   to a minimal Score PLUS at least one reader diagnostic.
    //
    // `no_panic_only` — well-formed-but-degenerate documents the reader is
    //   designed to accept silently: an empty-but-valid `<score-partwise>` and a
    //   tree sprinkled with unknown elements. Per design §2.2 unknown elements are
    //   "ignored (+ OPTIONAL diagnostic)", so these are asserted not to panic but
    //   are NOT required to warn.
    let must_diagnose: &[&str] = &[
        // Truncated / not well-formed XML (roxmltree parse error).
        "<score-partwise><part></broken>",
        "not xml at all <<< >>>",
        "<?xml version=\"1.0\"?>",
        // Well-formed XML whose root is not a partwise score.
        "<?xml version=\"1.0\"?><html><body>nope</body></html>",
        "<?xml version=\"1.0\"?><score-timewise></score-timewise>",
        // Bad numbers everywhere the reader parses an integer/float.
        "<score-partwise><part-list><score-part id=\"P1\"><midi-instrument id=\"x\">\
         <midi-channel>NaN</midi-channel><midi-program>-7</midi-program>\
         <volume>lots</volume><pan>left</pan></midi-instrument></score-part></part-list>\
         <part id=\"P1\"><measure number=\"one\">\
         <attributes><divisions>zero</divisions><key><fifths>sharp</fifths></key>\
         <time><beats>x</beats><beat-type>y</beat-type></time>\
         <transpose><chromatic>lots</chromatic></transpose></attributes>\
         <note><pitch><step>H</step><octave>nine</octave><alter>up</alter></pitch>\
         <duration>long</duration><voice>v</voice></note>\
         <forward><duration>bad</duration></forward>\
         <backup><duration>bad</duration></backup>\
         </measure></part></score-partwise>",
        // A grace run with no following note and no host (the S6e fallback arm).
        "<score-partwise><part id=\"P1\"><measure number=\"1\">\
         <note><grace/><pitch><step>A</step><octave>5</octave></pitch><voice>1</voice>\
         <notations><tuplet type=\"stop\" number=\"9\"/></notations></note>\
         <note><chord/><pitch><step>C</step><octave>5</octave></pitch>\
         <duration>8</duration><voice>1</voice></note>\
         </measure></part></score-partwise>",
    ];
    // Inputs that must merely NOT panic (silent acceptance is in spec).
    let no_panic_only: &[&str] = &[
        "",
        "   \n\t  ",
        // Empty but well-formed partwise score (no parts) — legitimately empty.
        "<score-partwise></score-partwise>",
        // Unknown elements scattered through an otherwise-valid tree.
        "<score-partwise><part id=\"P1\"><measure number=\"1\">\
         <quux><frob/></quux><note><blarg/><pitch><step>C</step><octave>4</octave></pitch>\
         <duration>8</duration><voice>1</voice><notations><wibble/></notations></note>\
         </measure></part></score-partwise>",
    ];
    let mut malformed_panics: Vec<String> = Vec::new();
    let mut malformed_without_diagnostic: Vec<String> = Vec::new();
    for (i, input) in must_diagnose.iter().enumerate() {
        let caught =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read_musicxml(input)));
        match caught {
            Ok(report) => {
                if report.diagnostics.is_empty() {
                    malformed_without_diagnostic.push(format!("must_diagnose #{i}: {input:.60?}"));
                }
            }
            Err(_) => malformed_panics.push(format!("must_diagnose #{i}: {input:.60?}")),
        }
    }
    for (i, input) in no_panic_only.iter().enumerate() {
        if !reads_without_panic(input) {
            malformed_panics.push(format!("no_panic_only #{i}: {input:.60?}"));
        }
    }
    let malformed_total = must_diagnose.len() + no_panic_only.len();

    // Restore the panic hook before asserting (so a genuine test-framework panic
    // still prints normally).
    std::panic::set_hook(previous_hook);

    eprintln!(
        "totality fuzz: own-exports exercised {own_exercised} files ({} panicked); \
         abc2xml-reference {} ({ref_exercised} files, {} panicked); \
         malformed cases {} ({} panicked, {} missing diagnostic)",
        own_panics.len(),
        if ref_available {
            "available"
        } else {
            "UNAVAILABLE"
        },
        ref_panics.len(),
        malformed_total,
        malformed_panics.len(),
        malformed_without_diagnostic.len(),
    );

    // Totality assertions: ZERO panics anywhere, and every malformed input
    // produced a diagnostic.
    assert!(
        own_panics.is_empty(),
        "read_musicxml panicked on {} own-export file(s); first few: {:?}",
        own_panics.len(),
        own_panics.iter().take(5).collect::<Vec<_>>()
    );
    assert!(
        ref_panics.is_empty(),
        "read_musicxml panicked on {} abc2xml-reference file(s); first few: {:?}",
        ref_panics.len(),
        ref_panics.iter().take(5).collect::<Vec<_>>()
    );
    assert!(
        malformed_panics.is_empty(),
        "read_musicxml panicked on malformed input(s): {malformed_panics:?}"
    );
    assert!(
        malformed_without_diagnostic.is_empty(),
        "malformed input(s) produced NO diagnostic (must degrade with a warning): {malformed_without_diagnostic:?}"
    );
}

// --- R1b: complete_score_for_abc (the ABC-projection completion pass) ---------

#[cfg(feature = "musicxml-reader")]
mod abc_completion {
    use super::*;
    use crate::model::BarlineKind;
    use crate::musicxml::read::complete_score_for_abc;
    use crate::to_abc::{AbcWriteOptions, write_abc};
    use crate::{LowerOptions, ParseOptions, lower_score, parse_document};

    /// Reconstruct a Score from `abc`'s forward MusicXML and run the ABC
    /// completion pass — exactly what `croma read --format abc` does.
    fn completed_from_abc(abc: &str) -> Score {
        let x1 = export(abc);
        let mut score = read_musicxml(&x1).value;
        complete_score_for_abc(&mut score);
        score
    }

    /// Lower an ABC fixture to a Score (panicking on a lower failure).
    fn lower(abc: &str) -> Score {
        let document = parse_document(abc, ParseOptions::default()).value;
        lower_score(&document, LowerOptions)
            .value
            .expect("fixture ABC should lower cleanly")
    }

    /// Every barline kind in `voice.events`, in event order.
    fn event_barline_kinds(score: &Score) -> Vec<BarlineKind> {
        let mut kinds = Vec::new();
        for part in &score.parts {
            for voice in &part.voices {
                for event in &voice.events {
                    if let TimedEventKind::Barline(b) = &event.kind {
                        kinds.push(b.kind);
                    }
                }
            }
        }
        kinds
    }

    /// The flat (step, alter, octave) pitch sequence of voice 0 of every part.
    fn pitch_sequence(score: &Score) -> Vec<(char, i32, i32)> {
        let mut pitches = Vec::new();
        for part in &score.parts {
            for voice in &part.voices {
                for event in &voice.events {
                    if let TimedEventKind::Note(note) = &event.kind {
                        pitches.push((
                            note.pitch.step,
                            i32::from(note.pitch.alter),
                            i32::from(note.pitch.octave),
                        ));
                    }
                }
            }
        }
        pitches
    }

    /// Measure count = number of `Measure`s in voice 0 of part 0.
    fn measure_count(score: &Score) -> usize {
        score.parts[0].voices[0].measures.len()
    }

    #[test]
    fn completion_synthesizes_voice_barline_events_from_measure_structure() {
        // Before the pass, the reader leaves voice.events barline-free.
        let x1 = export("X:1\nL:1/4\nK:C\nC D E F | G A B c |\n");
        let raw = read_musicxml(&x1).value;
        assert!(
            event_barline_kinds(&raw).is_empty(),
            "the raw reader output must carry NO barline events (they live in Measure.barlines)"
        );
        // After the pass, the INTERNAL boundary `|` is materialised as a Barline
        // event. Both `|` are plain Regular so neither produced a `<barline>`
        // element; the final trailing `|` on the LAST measure is structurally
        // inert and unrecoverable, so only the internal boundary is synthesized
        // (forward lowering of this tune also yields exactly one event per
        // internal boundary in the round-tripped projection).
        let mut completed = raw;
        complete_score_for_abc(&mut completed);
        assert_eq!(
            event_barline_kinds(&completed),
            vec![BarlineKind::Regular],
            "the completion pass must synthesize the internal measure boundary `|`"
        );
        // The structural gate is measure count, which must be preserved exactly.
        let projected = write_abc(&completed, AbcWriteOptions::default());
        assert_eq!(
            measure_count(&lower(&projected)),
            2,
            "both measures must survive XML -> ABC -> Score; abc {projected:?}"
        );
    }

    #[test]
    fn completed_abc_reparses_to_same_measure_count_and_barline_kinds() {
        // A multi-measure tune with a leading repeat, a backward repeat, and a
        // final barline. The completed ABC must carry the right glyphs and
        // re-parse to the same measure count + barline kinds.
        let abc = "X:1\nL:1/4\nK:C\n|: C D E F | G A B c :| c B A G |]\n";
        let completed = completed_from_abc(abc);
        let projected = write_abc(&completed, AbcWriteOptions::default());

        // Glyph spot-check: every special barline survives into the ABC text.
        assert!(projected.contains("|:"), "leading repeat: {projected:?}");
        assert!(projected.contains(":|"), "backward repeat: {projected:?}");
        assert!(projected.contains("|]"), "final barline: {projected:?}");

        // Structural: re-parsing the projected ABC yields the same measure count
        // and the same barline-kind sequence as lowering the original ABC.
        let reference = lower(abc);
        let reparsed = lower(&projected);
        assert_eq!(
            measure_count(&reparsed),
            measure_count(&reference),
            "measure count must survive XML -> ABC -> Score; abc {projected:?}"
        );
        assert_eq!(
            event_barline_kinds(&reparsed),
            event_barline_kinds(&reference),
            "barline kinds must survive XML -> ABC -> Score; abc {projected:?}"
        );
    }

    #[test]
    fn completion_sets_canonical_major_key_display_from_fifths() {
        // fifths = -3 (E-flat major). The raw reader leaves display empty; the
        // pass must fill the canonical major spelling so `K:` is non-empty.
        let x1 = export("X:1\nL:1/4\nK:Eb\nE F G A | B c d e |\n");
        let raw = read_musicxml(&x1).value;
        assert_eq!(
            raw.metadata.key.as_ref().map(|k| k.display.as_str()),
            Some(""),
            "the raw reader leaves the key display empty"
        );
        let mut completed = raw;
        complete_score_for_abc(&mut completed);
        assert_eq!(
            completed.metadata.key.as_ref().map(|k| k.display.as_str()),
            Some("Eb"),
            "fifths -3 must spell as the canonical major key Eb"
        );
        // And the projected ABC carries that `K:Eb`.
        let projected = write_abc(&completed, AbcWriteOptions::default());
        assert!(
            projected.contains("\nK:Eb\n"),
            "completed ABC must emit K:Eb, got {projected:?}"
        );
    }

    #[test]
    fn non_c_key_pitch_sequence_survives_xml_to_abc_to_score() {
        // The empty-K: bug dropped the key fifths (-3 -> 0) and re-applied
        // accidentals wrongly. With the canonical display, the sounding pitch
        // sequence must survive ABC -> XML -> Score -> ABC -> Score.
        let abc = "X:1\nL:1/8\nK:Eb\nE F G A B c d e |\n";
        let reference = lower(abc);
        let completed = completed_from_abc(abc);
        // The reconstructed Score already carries the source sounding pitches.
        assert_eq!(
            pitch_sequence(&completed),
            pitch_sequence(&reference),
            "reconstruction must preserve the sounding pitch sequence in a non-C key"
        );
        // ...and the projected ABC re-parses to that same sequence.
        let projected = write_abc(&completed, AbcWriteOptions::default());
        let reparsed = lower(&projected);
        assert_eq!(
            pitch_sequence(&reparsed),
            pitch_sequence(&reference),
            "completed ABC must reparse to the same pitches in a non-C key; abc {projected:?}"
        );
    }

    #[test]
    fn mid_tune_key_change_display_is_filled_canonically() {
        // A mid-tune `[K:F]` (fifths 1) is reconstructed as a KeyChange event
        // with empty display, which write_abc would emit as `[K:]` -> fifths 0,
        // dropping the change. The pass must fill the canonical major spelling so
        // the inline change re-parses to the same fifths.
        let abc = "X:1\nL:1/4\nK:C\nC D E F | [K:F] G A B c |\n";
        let mut score = read_musicxml(&export(abc)).value;
        complete_score_for_abc(&mut score);
        let change_displays: Vec<String> = score.parts[0].voices[0]
            .events
            .iter()
            .filter_map(|e| match &e.kind {
                TimedEventKind::KeyChange(k) => Some(k.display.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            change_displays,
            vec!["F".to_string()],
            "mid-tune KeyChange display must be the canonical major spelling for fifths 1"
        );
        let projected = write_abc(&score, AbcWriteOptions::default());
        assert!(
            projected.contains("[K:F]"),
            "completed ABC must carry the inline [K:F], got {projected:?}"
        );
        // Structural: the inline change survives a re-parse (same fifths sequence).
        let reference = lower(abc);
        let reparsed = lower(&projected);
        let key_fifths = |s: &Score| -> Vec<i8> {
            let mut v = Vec::new();
            for p in &s.parts {
                for voice in &p.voices {
                    for e in &voice.events {
                        if let TimedEventKind::KeyChange(k) = &e.kind {
                            v.push(k.fifths);
                        }
                    }
                }
            }
            v
        };
        assert_eq!(
            key_fifths(&reparsed),
            key_fifths(&reference),
            "mid-tune key fifths must survive XML -> ABC -> Score; abc {projected:?}"
        );
    }

    #[test]
    fn first_and_second_endings_round_trip_through_completed_abc() {
        // A volta bracket: `[1 ... :| [2 ... |]`. The completion pass must
        // synthesize the RepeatEnding events so write_abc emits the brackets.
        let abc = "X:1\nL:1/4\n K:C\n|: C D E F |1 G A B c :|2 c B A G |]\n";
        let completed = completed_from_abc(abc);
        let projected = write_abc(&completed, AbcWriteOptions::default());
        assert!(projected.contains("[1"), "first ending: {projected:?}");
        assert!(projected.contains("[2"), "second ending: {projected:?}");
        // Structural: same measure count + barline kinds after a re-parse.
        let reference = lower(abc);
        let reparsed = lower(&projected);
        assert_eq!(measure_count(&reparsed), measure_count(&reference));
        assert_eq!(
            event_barline_kinds(&reparsed),
            event_barline_kinds(&reference),
            "ending barline kinds must survive; abc {projected:?}"
        );
    }

    #[test]
    fn format_xml_inverse_is_byte_identical_with_and_without_completion() {
        // The load-bearing isolation invariant: the completion pass must NOT
        // perturb the `write_musicxml` view. For several fixtures, the XML
        // re-emission must be byte-identical whether or not the pass ran.
        for abc in [
            "X:1\nL:1/4\nK:C\nC D E F | G A B c |\n",
            "X:1\nL:1/4\nK:Eb\n|: E F G A :| B c d e |]\n",
            "X:1\nL:1/4\nK:D\n|: A B c d |1 e f g a :|2 a g f e |]\n",
        ] {
            let x1 = export(abc);
            let mut score = read_musicxml(&x1).value;
            let xml_before = write_musicxml(&score).musicxml;
            complete_score_for_abc(&mut score);
            let xml_after = write_musicxml(&score).musicxml;
            assert_eq!(
                xml_before, xml_after,
                "completion must not change the write_musicxml inverse for {abc:?}"
            );
            // And both equal the original forward XML (the pure inverse holds).
            assert_eq!(
                xml_after, x1,
                "the XML inverse must stay byte-identical to the forward XML for {abc:?}"
            );
        }
    }

    #[test]
    fn key_display_is_left_untouched_when_metadata_key_is_none() {
        // A reconstructed Score with no key must keep `metadata.key == None`
        // (write_abc defaults to K:C); the pass must not synthesize one.
        let mut score = read_musicxml(&export("X:1\nL:1/4\nK:C\nC D E F |\n")).value;
        score.metadata.key = None;
        complete_score_for_abc(&mut score);
        assert!(
            score.metadata.key.is_none(),
            "the pass must not synthesize a key when none is present"
        );
        let projected = write_abc(&score, AbcWriteOptions::default());
        assert!(
            projected.contains("\nK:C\n"),
            "write_abc must default a key-less Score to K:C, got {projected:?}"
        );
    }
}

// --- R1: ABC -> XML -> Score -> ABC re-emission measurement ------------------

/// Totality arm for the ABC re-emission loop, run over the whole corpus. For
/// each corpus `.abc` that lowers cleanly, `s1 = lower(parse(abc))`, then we
/// drive `write_abc(read_musicxml(write_musicxml(&s1).xml).value)` against
/// `write_abc(&s1)` and count byte-identical full-document matches.
///
/// **This arm asserts TOTALITY ONLY (0 panics over ~10k files); the reported
/// byte-identity count is expected to be near-zero and is NOT a gate.** Full
/// ABC-*document* byte equality is the wrong bar here: `write_abc` emits
/// ABC-only state — the `X:` reference number, `W:` post-tune lyrics, `V:` ids,
/// an empty display `K:` — that the lossy MusicXML round-trip legitimately
/// drops, so the documents differ by construction even when every structural
/// musical fact is preserved. The byte count therefore measures nothing useful
/// and the test makes no hard count assertion on it; it exists to prove the
/// reader is *total* over the corpus (it never panics). The **real** R1 metric
/// — a normalized STRUCTURAL projection that ignores this ABC-only state — lives
/// in `tools/prove_reader_abc_roundtrip.py` (the structural sibling of
/// `tools/prove_abc_roundtrip.py`), which is where the round-trip *correctness*
/// proof belongs.
///
/// **Report-only**, mirroring the XML measurement's discipline: it prints
/// `N/total` plus a first-divergence note for a few non-matches and asserts no
/// hard count. Env-gated on `ABC_ROOT`; a no-op when unset, exactly like the
/// sibling measurement.
#[test]
fn corpus_abc_reemission_through_xml() {
    use crate::to_abc::{AbcWriteOptions, write_abc};

    let Ok(root) = std::env::var("ABC_ROOT") else {
        eprintln!("ABC_ROOT unset — skipping R1 ABC-reemission-through-XML measurement");
        return;
    };
    let files = abc_files(&PathBuf::from(&root));
    if files.is_empty() {
        eprintln!("no .abc files under {root} — skipping");
        return;
    }

    let mut total = 0usize;
    let mut lowered = 0usize;
    let mut idempotent = 0usize;
    let mut first_divergences: Vec<String> = Vec::new();

    for path in &files {
        let Ok(bytes) = fs::read(path) else { continue };
        let source = String::from_utf8_lossy(&bytes);
        total += 1;

        // Only files that export cleanly have an X1 to invert; that same set is
        // the one that lowers to a Score, so `export_musicxml` is the gate.
        let Ok(export) = export_musicxml(&source) else {
            continue;
        };
        lowered += 1;
        let x1 = export.musicxml;

        // s1 = lower(parse(abc)); its canonical ABC is the reference.
        let document = crate::parse_document(&source, crate::ParseOptions::default()).value;
        let Some(s1) = crate::lower_score(&document, crate::LowerOptions).value else {
            continue;
        };
        let expected_abc = write_abc(&s1, AbcWriteOptions::default());

        let reconstructed = read_musicxml(&x1).value;
        let round_trip_abc = write_abc(&reconstructed, AbcWriteOptions::default());

        if round_trip_abc == expected_abc {
            idempotent += 1;
        } else if first_divergences.len() < 5 {
            let line = first_diverging_line(&expected_abc, &round_trip_abc);
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("<unknown>");
            first_divergences.push(format!("{name}: {line}"));
        }
    }

    eprintln!(
        "ABC re-emission through XML (ABC -> XML -> Score -> ABC): {idempotent}/{lowered} \
         files are BYTE-identical full documents ({total} total .abc). NOTE: this byte \
         count is EXPECTED to be near-zero and is not a metric — write_abc emits \
         ABC-only state (X:/W:/V: ids, empty display K:) that the lossy XML round-trip \
         legitimately drops, so the documents differ by construction. This arm asserts \
         TOTALITY ONLY (0 panics). The real structural round-trip proof lives in \
         tools/prove_reader_abc_roundtrip.py."
    );
    if !first_divergences.is_empty() {
        eprintln!("first ABC divergences (up to 5):");
        for note in &first_divergences {
            eprintln!("  {note}");
        }
    }

    // No hard count on the byte-identity tally: full ABC-document byte equality
    // is expected to be near-zero (write_abc emits ABC-only state absent from
    // the lossy XML round-trip), so it is not a gate. The structural proof is
    // tools/prove_reader_abc_roundtrip.py. Here we require only that the loop is
    // total over the corpus (no panic) and that at least one file was measured.
    assert!(
        lowered > 0,
        "expected at least one corpus file to lower and export"
    );
}

/// The first line at which two ABC documents diverge, rendered as
/// `` `<expected>` != `<actual>` ``. `None`-free: returns a sentinel when one is
/// a prefix of the other. Used only for the human-readable divergence note.
fn first_diverging_line(expected: &str, actual: &str) -> String {
    for (line_e, line_a) in expected.lines().zip(actual.lines()) {
        if line_e != line_a {
            return format!("`{line_e}` != `{line_a}`");
        }
    }
    let expected_lines = expected.lines().count();
    let actual_lines = actual.lines().count();
    if expected_lines == actual_lines {
        "<equal-lines-unequal-bytes>".to_owned()
    } else {
        format!("<line-count differs: expected {expected_lines}, actual {actual_lines}>")
    }
}
