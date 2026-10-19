
use super::{ReadFormat, project_reconstructed, read_score_from_xml};
use croma_core::{
    AbcWriteOptions, LowerOptions, ParseOptions, lower_score, parse_document, write_abc,
    write_musicxml,
};

/// Lower an ABC fixture to a Score (panicking on diagnostics) for the
/// command-path round-trip tests.
fn lower(abc: &str) -> croma_core::Score {
    let document = parse_document(abc, ParseOptions::default()).value;
    lower_score(&document, LowerOptions)
        .value
        .expect("fixture ABC should lower cleanly")
}

#[test]
fn read_format_abc_round_trips_the_pitch_sequence_through_xml() {
    // The command path: ABC -> Score s -> X (write_musicxml) -> read_musicxml
    // -> project(--format abc), exactly what `croma read --format abc` /
    // `musicxml2abc` produce, without spawning a process. The proven reader
    // invariant is XML re-emission idempotence, NOT full ABC-document
    // identity (the X: number, an empty-`K:`, the surfaced `V:` id and the
    // trailing barline are not reconstructed through the writer's subset), so
    // the load-bearing assertion is that the projected ABC carries the SAME
    // pitch sequence — the semantic guarantee — and re-parses cleanly.
    let abc = "X:1\nT:Round Trip\nM:4/4\nL:1/8\nK:C\nC D E F G A B c |\n";
    let source_score = lower(abc);

    let xml = write_musicxml(&source_score).musicxml;
    let (reconstructed, diagnostics) =
        read_score_from_xml(&xml).expect("reconstruction must succeed");
    assert!(
        diagnostics.is_empty(),
        "a clean round-trip emits no reader diagnostics, got {diagnostics:?}"
    );
    let projected_abc = project_reconstructed(&reconstructed, ReadFormat::Abc, false);
    assert!(
        !projected_abc.trim().is_empty(),
        "the ABC projection must be non-empty"
    );

    // The Score reconstructed from XML already carries the source pitches...
    assert_eq!(
        pitch_sequence(&reconstructed),
        pitch_sequence(&source_score),
        "read_musicxml must reconstruct the source pitch sequence"
    );
    // ...and the projected ABC must re-parse to that same pitch sequence,
    // closing the ABC -> XML -> Score -> ABC loop semantically.
    let reparsed = lower(&projected_abc);
    assert_eq!(
        pitch_sequence(&reparsed),
        pitch_sequence(&source_score),
        "the projected ABC must reparse to the same pitch sequence"
    );
}

#[test]
fn read_format_abc_equals_write_abc_of_the_reconstructed_score() {
    // The second arm the spec offers: `read --format abc` is exactly
    // `write_abc(read_musicxml(X))`. This pins the projection wiring (the
    // `abc` branch routes through `write_abc` with default options) against
    // the reconstructed Score itself, independent of the source-vs-reader
    // subset gap exercised above.
    let abc = "X:1\nT:Abc\nM:4/4\nL:1/4\nK:C\nC D E F |\n";
    let xml = write_musicxml(&lower(abc)).musicxml;
    let (reconstructed, _diags) = read_score_from_xml(&xml).expect("reconstruction must succeed");
    assert_eq!(
        project_reconstructed(&reconstructed, ReadFormat::Abc, false),
        write_abc(&reconstructed, AbcWriteOptions::default()),
        "read --format abc must equal write_abc of the reconstructed Score"
    );
}

#[test]
fn read_format_xml_reemits_the_writer_inverse_image() {
    // --format xml projects write_musicxml(read(xml)); for the supported
    // subset this is byte-identical to the input XML (the reader is the
    // writer's inverse). Asserts the projection wires to write_musicxml.
    let abc = "X:1\nT:Xml\nM:4/4\nL:1/4\nK:C\nC D E F |\n";
    let source_score = lower(abc);
    let xml = write_musicxml(&source_score).musicxml;
    let (reconstructed, _diags) = read_score_from_xml(&xml).expect("reconstruction must succeed");
    let projected = project_reconstructed(&reconstructed, ReadFormat::Xml, false);
    assert_eq!(
        projected, xml,
        "read --format xml must re-emit the writer's inverse-image bytes"
    );
}

#[test]
fn read_format_abc_threads_the_legend_option() {
    // Pins that `project_reconstructed`'s `legend` parameter reaches
    // `write_abc` unchanged — the CLI wiring behind
    // `croma read --format abc --legend` / `croma musicxml2abc --legend`.
    let abc = "X:1\nT:Legend\nM:4/4\nL:1/4\nK:C\nC D E F |\n";
    let xml = write_musicxml(&lower(abc)).musicxml;
    let (reconstructed, _diags) = read_score_from_xml(&xml).expect("reconstruction must succeed");
    assert_eq!(
        project_reconstructed(&reconstructed, ReadFormat::Abc, true),
        write_abc(&reconstructed, AbcWriteOptions { legend: true }),
        "read --format abc --legend must equal write_abc with legend enabled"
    );
}

#[test]
fn read_format_dump_renders_the_score_debug() {
    let abc = "X:1\nT:Dump\nM:4/4\nL:1/4\nK:C\nC |\n";
    let (reconstructed, _diags) =
        read_score_from_xml(&write_musicxml(&lower(abc)).musicxml).expect("reconstruction");
    let dump = project_reconstructed(&reconstructed, ReadFormat::Dump, false);
    assert!(
        dump.contains("Score"),
        "the dump projection must be the Score debug representation, got {dump:?}"
    );
}

#[test]
fn malformed_xml_yields_a_diagnostic_and_does_not_panic() {
    // Feeding non-MusicXML must surface a reader diagnostic and must not
    // panic. `read_score_from_xml` returns the (possibly skeletal) Score
    // plus the diagnostics, exactly what the command prints to stderr.
    let (_score, diagnostics) =
        read_score_from_xml("this is not musicxml at all <<<").expect("reader is total");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code.starts_with("musicxml.read")),
        "non-MusicXML input must yield a reader diagnostic, got {diagnostics:?}"
    );
}

/// The flat (step, alter, octave) sequence of every note in voice 0 of every
/// part — a pitch fingerprint stable across the ABC round-trip.
fn pitch_sequence(score: &croma_core::Score) -> Vec<(char, i32, i32)> {
    use croma_core::TimedEventKind;
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
