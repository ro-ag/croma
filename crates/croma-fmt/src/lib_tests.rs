
use super::*;

fn fmt(source: &str) -> String {
    format(source, FormatOptions::default())
}

#[test]
fn collapses_music_spaces_but_preserves_beaming_breaks() {
    let out = fmt("X:1\nK:C\nCDE   FGA  |  c2\n");
    assert!(out.contains("CDE FGA | c2"), "got: {out:?}");
    assert!(out.ends_with('\n') && !out.ends_with("\n\n"));
}

#[test]
fn is_idempotent() {
    let src = "X:1\nT:Tune\nK:C\n  CDE   FGA  |  \"C\"  c2  z2  % c o m\n\n\nX:2\nK:G\nGABc\n";
    let once = fmt(src);
    let twice = fmt(&once);
    assert_eq!(once, twice, "not idempotent");
}

#[test]
fn is_lossless() {
    let src = "X:1\nT:Tune\nK:C\n  CDE   FGA  |  c2  z2\n";
    let before = verify::pitch_seq_of(src, ParseOptions::default());
    let after = verify::pitch_seq_of(&fmt(src), ParseOptions::default());
    assert_eq!(before, after);
    assert!(before.is_some());
}

#[test]
fn lyrics_and_directives_are_byte_stable() {
    let out = fmt("X:1\nK:C\nCDE\nw: do  re   mi\n%%MIDI program 1\n");
    assert!(out.contains("w: do  re   mi"), "got: {out:?}");
    assert!(out.contains("%%MIDI program 1"), "got: {out:?}");
}

#[test]
fn trims_trailing_whitespace_and_collapses_blank_runs() {
    let out = fmt("X:1  \nK:C\t\nC   \n\n\n\nD\n");
    assert_eq!(out, "X:1\nK:C\nC\n\nD\n");
}

#[test]
fn empty_source_formats_to_empty() {
    assert_eq!(fmt(""), "");
}

#[test]
fn chords_and_grace_groups_are_not_duplicated() {
    // Top-level runs collapse to one space; bracket/brace contents and a
    // chord length are preserved verbatim and never emitted twice.
    let src = "X:1\nK:C\n[\"Cmaj\"abc]   [CE]2  |\n";
    let out = fmt(src);
    assert_eq!(out, "X:1\nK:C\n[\"Cmaj\"abc] [CE]2 |\n");
    assert_eq!(fmt(&out), out, "not idempotent");
    assert_eq!(
        verify::pitch_seq_of(src, ParseOptions::default()),
        verify::pitch_seq_of(&out, ParseOptions::default()),
    );
}

// --- carrier compaction -------------------------------------------------

/// A tune body carrying every coded carrier in its long spelling — the two
/// harmony-text variants included — plus one quoted value (`number="1,2"`).
///
/// It deliberately holds *no* uncoded carrier: `carrier_migration_is_idempotent`
/// asserts that no `croma-` survives a pass, which only holds because every
/// carrier here has a compact spelling. Adding an uncoded one (or a comment
/// or annotation that merely looks like a carrier) breaks that assertion —
/// those cases have their own tests below.
const LONG_CARRIERS: &str = concat!(
    "X:1\nM:4/4\nL:1/4\nK:C\n",
    "[I:croma-lyric-extend verse=1]C ",
    "[I:croma-direction-placement placement=below]\"^cresc\"D ",
    "[I:croma-harmony-text text=\"maj 7\"]\"Cmaj7\"E ",
    "[I:croma-harmony-text textless=1]\"C\"F |\n",
    "[I:croma-meter-restatement][I:croma-key-restatement]",
    "[I:croma-musicxml-forward]G ",
    "[I:croma-ending-close type=discontinue location=right number=\"1,2\"]A |\n",
);

#[test]
fn fmt_migrates_long_carrier_spellings_by_default() {
    let src = "X:1\nM:4/4\nL:1/4\nK:C\n[I:croma-lyric-extend verse=1]C D E F |\n";
    let out = fmt(src);
    assert!(out.contains("[I:cr le=1]"), "got: {out:?}");
    assert!(!out.contains("croma-lyric-extend"), "got: {out:?}");
}

#[test]
fn carrier_migration_is_idempotent() {
    let once = fmt(LONG_CARRIERS);
    let twice = fmt(&once);
    assert_eq!(once, twice, "a second pass must be a no-op");
    assert!(!once.contains("croma-"), "got: {once:?}");
}

#[test]
fn carrier_migration_covers_every_coded_carrier() {
    let out = fmt(LONG_CARRIERS);
    for compact in [
        "[I:cr le=1]",
        "[I:cr dp=b]",
        "[I:cr ht text=\"maj 7\"]",
        "[I:cr htx]",
        "[I:cr mr]",
        "[I:cr kr]",
        "[I:cr mf]",
        "[I:cr ec t=d l=r n=\"1,2\"]",
    ] {
        assert!(out.contains(compact), "missing {compact}: {out:?}");
    }
}

/// The migration is a respelling, not a semantic edit: the score croma
/// exports must be byte-identical before and after.
#[test]
fn migrated_source_reads_identically() {
    let migrated = fmt(LONG_CARRIERS);
    assert!(
        !migrated.contains("croma-"),
        "nothing migrated: {migrated:?}"
    );
    let before = verify::musicxml_of(LONG_CARRIERS, ParseOptions::default());
    let after = verify::musicxml_of(&migrated, ParseOptions::default());
    assert!(before.is_some(), "fixture must lower");
    assert_eq!(before, after, "the respelling changed the score");
}

/// Payload shapes the fixture does not reach: an `ec` without the optional
/// `location=`, an `ec` whose quoted `number` holds the space that
/// `split_fields` exists to keep inside one field, and the hex variant of
/// the harmony text.
#[test]
fn carrier_migration_covers_optional_and_quoted_payloads() {
    for (long, compact) in [
        (
            "croma-ending-close type=stop number=\"1\"",
            "cr ec t=s n=\"1\"",
        ),
        (
            "croma-ending-close type=stop location=left number=\"1, 2\"",
            "cr ec t=s l=l n=\"1, 2\"",
        ),
        (
            "croma-harmony-text text-hex=6d616a2037",
            "cr ht text-hex=6d616a2037",
        ),
    ] {
        let src = format!("X:1\nM:4/4\nL:1/4\nK:C\n[I:{long}]\"Cmaj7\"C D E F |\n");
        let out = fmt(&src);
        assert!(out.contains(&format!("[I:{compact}]")), "got: {out:?}");
        assert!(!out.contains("croma-"), "got: {out:?}");
    }
}

/// The other 15 carriers have no compact code, so `format` must leave each
/// of them exactly as written. `croma-lyric-duplicate` is the sharpest case:
/// its payload is shaped like the forms that *do* migrate and its name
/// shares a prefix with `croma-lyric-extend`.
///
/// `croma-musicxml-instrument` is normally a `%%`-directive line rather than
/// an inline field; it is written inline here because the inline field is
/// the surface this migration scans.
#[test]
fn carrier_migration_leaves_uncoded_carriers_alone() {
    for value in [
        "croma-musicxml-tuplet id=1 actual=3 normal=2 role=start",
        "croma-clef-cursor clef=\"treble\" back-n=1 back-d=4",
        "croma-after-grace",
        "croma-time-symbol symbol=common",
        "croma-measure-number n=12",
        "croma-tempo role=printed text=\"Allegro\"",
        "croma-sound-tempo bpm=120 beat-n=1 beat-d=4",
        "croma-lyric-duplicate verse=1 text=\"John Peel\"",
        "croma-initial-key fifths=2",
        "croma-initial-meter display=\"4/4\"",
        "croma-note-instrument id=\"P1-I1\"",
        "croma-barline-style style=dashed",
        "croma-xvoice-slur pair=1 role=start",
        "croma-musicxml-sequence-backup n=1 d=4",
        "croma-musicxml-instrument id=\"P1-I1\"",
    ] {
        let src = format!("X:1\nM:4/4\nL:1/4\nK:C\n[I:{value}]C D E F |\n");
        let out = fmt(&src);
        assert!(
            out.contains(&format!("[I:{value}]")),
            "only the coded carriers migrate; got: {out:?}"
        );
    }
}

/// A `%` opens an ABC comment and a `"…"` is an annotation; neither is an
/// inline field, so a carrier-shaped sequence inside one must survive.
#[test]
fn carrier_migration_ignores_comments_and_quoted_text() {
    let src = concat!(
        "X:1\nM:4/4\nL:1/4\nK:C\n",
        "C D E F | % [I:croma-lyric-extend verse=1] not a field\n",
        "\"^[I:croma-key-restatement]\"G A B c |\n",
    );
    let out = fmt(src);
    assert!(
        out.contains("% [I:croma-lyric-extend verse=1] not a field"),
        "comment was rewritten: {out:?}"
    );
    assert!(
        out.contains("\"^[I:croma-key-restatement]\""),
        "annotation was rewritten: {out:?}"
    );
}

/// A carrier field whose payload is not what croma's writer emits is left
/// alone: a compact carrier the reader cannot expand would be dropped, so
/// guessing is worse than doing nothing.
#[test]
fn carrier_migration_skips_unrecognized_payloads() {
    for value in [
        "croma-direction-placement placement=sideways",
        "croma-lyric-extend verse=one",
        "croma-meter-restatement extra=1",
        "croma-ending-close type=stop",
        "croma-ending-closest type=stop number=\"1\"",
    ] {
        let src = format!("X:1\nM:4/4\nL:1/4\nK:C\n[I:{value}]C D E F |\n");
        let out = fmt(&src);
        assert!(out.contains(&format!("[I:{value}]")), "got: {out:?}");
    }
}

/// `croma fmt` and `croma fmt --auto-fix` must not disagree about the
/// migration, so `--check` in either mode reports the same rewrite.
#[test]
fn auto_fix_reports_and_matches_the_default_migration() {
    let fixed = auto_fix(LONG_CARRIERS, FormatOptions::default());
    assert_eq!(fixed.output, fmt(LONG_CARRIERS));
    assert!(fixed.skipped.is_empty(), "got: {:?}", fixed.skipped);
    let migrations = fixed
        .changes
        .iter()
        .filter(|change| change.kind == FixKind::CarrierCompaction)
        .count();
    assert_eq!(migrations, 8, "got: {:?}", fixed.changes);
    assert_eq!(
        carrier_migrations(LONG_CARRIERS, FormatOptions::default()).len(),
        8,
    );
}

/// The migration is gated like every other source-changing edit here: a
/// source that does not lower cannot be proven score-preserving, so the
/// respelling is dropped — by plain `format` as much as by `auto_fix`.
/// Without that, the two modes would disagree about *whether* the file needs
/// changing, and `croma fmt --check` and `croma fmt --auto-fix --check`
/// would exit differently on the same file.
#[test]
fn unlowerable_source_keeps_its_long_carrier_in_both_modes() {
    // A carrier and no time-bearing event: it parses, it holds a migratable
    // inline field, and it has no rendering to compare.
    let src = "X:1\nM:4/4\nL:1/4\nK:C\n[I:croma-lyric-extend verse=1]\n";
    assert!(verify::musicxml_of(src, ParseOptions::default()).is_none());
    assert!(
        !carrier_migrations(src, FormatOptions::default()).is_empty(),
        "fixture must offer a migration to reject"
    );

    let formatted = fmt(src);
    let fixed = auto_fix(src, FormatOptions::default());
    assert!(
        formatted.contains("[I:croma-lyric-extend verse=1]"),
        "an unverifiable respelling was applied: {formatted:?}"
    );
    assert_eq!(
        fixed.output, formatted,
        "the two modes produced different bytes"
    );
    assert!(
        is_formatted(src, FormatOptions::default()),
        "`--check` would report a rewrite that `--auto-fix` refuses to make"
    );
    assert!(fixed.changes.is_empty(), "got: {:?}", fixed.changes);
}

#[test]
fn carrier_migration_is_lossless_for_pitches() {
    assert_eq!(
        verify::pitch_seq_of(LONG_CARRIERS, ParseOptions::default()),
        verify::pitch_seq_of(&fmt(LONG_CARRIERS), ParseOptions::default()),
    );
}

#[test]
fn grace_group_appears_once() {
    let out = fmt("X:1\nK:C\n{ge}A B\n");
    assert_eq!(out.matches("{ge}").count(), 1, "got: {out:?}");
}

#[test]
fn inline_voice_prefix_is_never_dropped() {
    // Regression: a leading `V:1` voice marker on a music line is not covered
    // by the music tokens; it must survive (else notes move between voices).
    let src = "X:1\nM:4/4\nL:1/8\nV:1\nV:2\nK:C\nV:1  CDE   FGA |\nV:2  C,2  x4    |\n";
    let out = fmt(src);
    assert!(out.contains("V:1  CDE FGA |"), "got: {out:?}");
    assert!(out.contains("V:2  C,2 x4 |"), "got: {out:?}");
    assert_eq!(fmt(&out), out, "not idempotent");
    assert_eq!(
        verify::pitch_seq_of(src, ParseOptions::default()),
        verify::pitch_seq_of(&out, ParseOptions::default()),
    );
}
