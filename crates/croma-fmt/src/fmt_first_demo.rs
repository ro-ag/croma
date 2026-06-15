//! One-shot demonstration of the strict-reject → `--auto-fix` repair → recovery
//! pipeline, the formatter's reason to exist under the three-tier recovery
//! policy.
//!
//! The strict parser rejects a deprecated bare-number tempo that carries a junk
//! suffix (`Q:320s`, `Q:400.`) — ABC 2.1 §10.1 defines the deprecated bare form
//! as a bare *integer*, so the suffix is outside the grammar and lowers to a
//! verbatim `<words>` rather than a metronome. `croma fmt --auto-fix`
//! (`BareTempoSuffix`) rewrites the field to its canonical bare integer, and the
//! score then recovers the intended `<per-minute>` metronome.
//!
//! These two inputs mirror the only two corpus files whose `dropped.csv`
//! adjudication names `croma fmt --auto-fix` as the recovery path —
//! `tune_001192.abc` (`Q:320s`) and `tune_009608.abc` (`Q:400.`). The raw
//! comparison axis still sees the strict-correct reject (the parser is not
//! weakened); the formatter is what recovers the loose source.

use croma_core::ParseOptions;

use crate::verify::musicxml_of;
use crate::{FixKind, FormatOptions, auto_fix};

/// `Q:<int><suffix>` → strict reject (`<words>`) raw, but `--auto-fix` recovers
/// the `<per-minute>` metronome.
fn assert_recovers(raw: &str, junk_words: &str, per_minute: &str) {
    let parse = ParseOptions::default();

    // Raw: the strict parser rejects the junk suffix to a verbatim <words>.
    let raw_xml = musicxml_of(raw, parse).expect("raw source lowers to a score");
    assert!(
        raw_xml.contains(&format!("<words>{junk_words}</words>")),
        "raw source should reject the tempo to <words>{junk_words}</words>; got:\n{raw_xml}",
    );
    assert!(
        !raw_xml.contains(&format!("<per-minute>{per_minute}</per-minute>")),
        "raw source must NOT already render the metronome",
    );

    // --auto-fix strips the suffix (BareTempoSuffix) ...
    let fixed = auto_fix(raw, FormatOptions::default());
    assert!(
        fixed
            .changes
            .iter()
            .any(|c| c.kind == FixKind::BareTempoSuffix),
        "auto_fix should apply BareTempoSuffix; changes: {:?}",
        fixed.changes,
    );

    // ... and the repaired source recovers the <per-minute> metronome.
    let fixed_xml = musicxml_of(&fixed.output, parse).expect("fixed source lowers");
    assert!(
        fixed_xml.contains(&format!("<per-minute>{per_minute}</per-minute>")),
        "fixed source should recover <per-minute>{per_minute}</per-minute>; got:\n{fixed_xml}",
    );
    assert!(
        !fixed_xml.contains(junk_words),
        "fixed source must not retain the junk suffix {junk_words}",
    );
}

#[test]
fn auto_fix_recovers_legacy_suffix_tempo() {
    // Mirrors tune_001192.abc.
    assert_recovers("X:1\nQ:320s\nK:C\nC\n", "320s", "320");
}

#[test]
fn auto_fix_recovers_trailing_dot_tempo() {
    // Mirrors tune_009608.abc.
    assert_recovers("X:1\nQ:400.\nK:C\nC\n", "400.", "400");
}
