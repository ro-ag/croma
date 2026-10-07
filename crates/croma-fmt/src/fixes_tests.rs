//! Carrier-registry tripwire: croma-fmt's compact codes must be ones croma-core
//! reads, checked through croma-core's public API because its registry is
//! private.

use super::COMPACT_CARRIERS;

/// A minimal valid payload for each compact code. A code added to
/// `COMPACT_CARRIERS` without a payload here fails the test below, so the
/// tripwire cannot be skipped by accident.
fn sample_payload(code: &str) -> Option<&'static str> {
    Some(match code {
        "dp" => "dp=a",
        "ht" => "ht text=\"C\"",
        "le" => "le=1",
        "mr" => "mr",
        "kr" => "kr",
        "ec" => "ec t=s l=r n=\"1\"",
        "mf" => "mf",
        _ => return None,
    })
}

#[test]
fn every_compact_code_the_migration_writes_is_known_to_croma_core() {
    for (long, code) in COMPACT_CARRIERS {
        let payload = sample_payload(code)
            .unwrap_or_else(|| panic!("add a sample payload for `cr {code}` ({long})"));
        let source = format!("X:1\nL:1/4\nK:C\n[I:cr {payload}]C|\n");
        let export = croma_core::export_musicxml(&source).expect("exports");
        let unknown = export.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "abc.field.inline_ignored"
                && diagnostic.message.contains(&format!("cr {code}"))
        });
        assert!(!unknown, "`cr {code}` ({long}) is not a croma-core carrier");
    }
}

#[test]
fn an_unknown_code_is_reported_so_the_tripwire_can_fire() {
    let export = croma_core::export_musicxml("X:1\nL:1/4\nK:C\n[I:cr zz]C|\n").expect("exports");
    assert!(export.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "abc.field.inline_ignored" && diagnostic.message.contains("cr zz")
    }));
}
