//! Deterministic, in-process ABC fixtures shared by the croma-fmt benches.
//!
//! A self-contained copy of the croma-core bench fixtures (a sibling crate's
//! `benches/` directory cannot be shared without packaging churn). Lives under
//! `benches/common/` so cargo does **not** auto-discover it as its own
//! `[[bench]]` target; pulled in via `#[path = "common/fixtures.rs"] mod
//! fixtures;`.
//!
//! Each fixture is valid ABC (parses + formats + auto-fixes cleanly). Size is the
//! body-line count after a fixed 7-line header; adapted from `croma-lsp`'s proven
//! `synthetic_abc_200()` generator (a representative cycle of notes, chords,
//! grace groups, decorations, tuplets, chord symbols, barlines, and
//! accidentals).

#![allow(dead_code)]

/// The fixed 7-line ABC header every fixture starts with.
const HEADER: &str = "X:1\nT:Bench Fixture\nC:croma\nM:4/4\nL:1/8\nQ:1/4=120\nK:C\n";

/// Representative body lines, cycled to reach the requested length.
const BODIES: [&str; 4] = [
    "CDEF GABc | defg abc'd' | !trill!c2 B2 A2 G2 |",
    "[CEG]2 {ab}c2 | (3def (3gab c4 | \"Am\"A2 \"G\"G2 F4 |",
    ".C.D.E.F | G>A B<c d2 e2 | z2 c2 B2 A2 |]",
    "=c ^d _e f | A,B,C,D, E,F,G,A, | C/2D/2E/2F/2 G2 |",
];

/// The three size buckets as `(label, body_line_count)`: the target total-line
/// count minus the 7 header lines, so the documents land on ~20 / ~200 / ~1000
/// lines.
pub const SIZES: &[(&str, usize)] = &[("small", 13), ("avg", 193), ("large", 993)];

/// Build a valid ABC document with `body_lines` music lines after the header.
pub fn fixture(body_lines: usize) -> String {
    let mut out = String::with_capacity(HEADER.len() + body_lines * 48);
    out.push_str(HEADER);
    for i in 0..body_lines {
        out.push_str(BODIES[i % BODIES.len()]);
        out.push('\n');
    }
    out
}
