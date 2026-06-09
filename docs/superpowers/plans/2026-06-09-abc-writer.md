# `Score → ABC` Writer Implementation Plan (slice 1)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax. Read the design first: `docs/superpowers/specs/2026-06-09-abc-writer-design.md`. Work on branch `work/abc-writer`, never `main`. Run `tools/session_bootstrap.sh` and read `AGENTS.md` first.

**Goal:** Add `croma_core::write_abc(&Score) -> String` that emits canonical ABC which is a `croma fmt` fixed point and round-trips through the parser with an identical structural projection, for the slice-1 construct set (single voice; notes/rests/durations/accidentals/octaves; common barlines + repeats + endings; ties).

**Architecture:** Walk `Score.parts[0].voices[0].events` (the ordered timeline, which already interleaves `Barline`/`RepeatEnding` events) and emit each event as canonical ABC, after emitting the `X:/T:/M:/L:/K:/Q:` header from `Score.metadata`. Durations are written relative to a unit length `L:` derived from the meter. Pure string generation, no new deps.

**Tech Stack:** Rust 2024 / MSRV 1.96.0; `croma-core` (no new dependency).

**Verified model facts (do not re-derive):**
- `Score.metadata: ScoreMetadata { reference: TextLine, title: Option<TextLine>, composers, tempo: Option<TextLine>, tempo_model: Option<TempoModel>, meter: Option<MeterModel>, key: Option<KeySignatureModel>, .. }`.
- `MeterModel { display: String, duration: Option<Rational>, free_meter: bool }`; `KeySignatureModel { display: String, fifths: i8, .. }`; `TempoModel { beat: Option<TempoBeat{beat_numerator,beat_denominator,bpm: u32}>, text }`.
- `Score.parts: Vec<Part>` → `Part.voices: Vec<Voice>` → `Voice.events: Vec<TimedEvent>`. `TimedEvent { measure, onset: Rational, duration: Rational, kind: TimedEventKind, attachments: EventAttachments }`.
- `TimedEventKind::{ Note(NoteEvent), Chord(ChordEvent), Rest(RestEvent), Spacer, Barline(MeasureBarline), RepeatEnding(RepeatEndingModel) }`.
- `NoteEvent { pitch: Pitch, written_accidental: Option<AccidentalMark>, chord_member }`; `Pitch { step: char, alter: i8, octave: i8 }`; `AccidentalMark { kind: Accidental, explicit: bool, .. }`; `Accidental::{ DoubleFlat, Flat, Natural, Sharp, DoubleSharp }`.
- `RestEvent { visibility: RestVisibility::{Visible, Invisible} }` → `z` / `x`.
- `MeasureBarline { kind: BarlineKind }`; `BarlineKind::{ Regular, Double, Final, Initial, RepeatStart, RepeatEnd, RepeatBoth, Dotted, Invisible, Liberal }`.
- `RepeatEndingModel { endings: Vec<RepeatEndingPartModel> }`; `RepeatEndingPartModel::{ Single(u32), Range{start: u32, end: u32} }`.
- `EventAttachments.ties: Vec<TieAttachment { role: TieRole::{Start, Stop} }>`.
- `Rational = Fraction { numerator: u32, denominator: u32 }`; `Fraction::new(n, d)` auto-reduces (gcd); pull these via `croma_core::{Fraction, Rational}`.
- Octave convention: middle C = octave 4; `C`=oct 4, `c`=oct 5 (verified: `CEG`/`K:C` → C4/E4/G4).

**Slice-1 in-scope filter** (a tune qualifies only if its `Score` satisfies all): exactly one part, one voice; no `TimedEventKind::Chord` and no `Spacer`; every event's `attachments` has empty `tuplets`, `grace_groups`, `slurs`, `lyrics`, `symbols`, `chord_symbols`, `annotations`, `decorations`; every `Barline` kind ∈ {Regular, Double, Final, RepeatStart, RepeatEnd, RepeatBoth}. Tunes outside this set are skipped by the corpus harness (counted as out-of-scope, not failures).

---

## Task 1: module scaffold + header emission

**Files:**
- Create: `crates/croma-core/src/to_abc.rs`
- Modify: `crates/croma-core/src/lib.rs` (add `mod to_abc; pub use to_abc::{write_abc, AbcWriteOptions};`)

- [ ] **Step 1: failing test** (in `to_abc.rs`)
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ParseOptions, LowerOptions, lower_score, parse_document};

    fn score_of(src: &str) -> crate::Score {
        let doc = parse_document(src, ParseOptions::default());
        lower_score(&doc.value, LowerOptions).value.expect("score")
    }

    #[test]
    fn emits_required_headers() {
        let abc = write_abc(&score_of("X:1\nT:Tune\nM:4/4\nL:1/8\nK:C\nC\n"), AbcWriteOptions::default());
        assert!(abc.starts_with("X:1\n"), "got: {abc:?}");
        assert!(abc.contains("\nM:4/4\n"));
        assert!(abc.contains("\nL:1/8\n"));
        assert!(abc.contains("\nK:C\n"));
        assert!(abc.contains("\nT:Tune\n"));
    }
}
```
- [ ] **Step 2: run, expect FAIL** — `cargo test -p croma-core to_abc` (does not compile: `write_abc` missing).
- [ ] **Step 3: implement scaffold**
```rust
//! Canonical `Score` → ABC writer (the reverse of the MusicXML writer).
use crate::{Accidental, BarlineKind, Pitch, Rational, RestVisibility, Score, TimedEventKind};

#[derive(Debug, Clone, Copy, Default)]
pub struct AbcWriteOptions;

/// Emit canonical ABC for `score`. Output is a `croma fmt` fixed point.
pub fn write_abc(score: &Score, _options: AbcWriteOptions) -> String {
    let mut out = String::new();
    let meta = &score.metadata;
    out.push_str(&format!("X:{}\n", meta.reference.text.trim()));
    if let Some(title) = &meta.title {
        out.push_str(&format!("T:{}\n", title.text.trim()));
    }
    let meter_display = meta.meter.as_ref().map(|m| m.display.clone()).unwrap_or_else(|| "4/4".to_string());
    out.push_str(&format!("M:{}\n", meter_display));
    let unit = unit_length(score);
    out.push_str(&format!("L:{}/{}\n", unit.numerator, unit.denominator));
    if let Some(q) = tempo_field(score) {
        out.push_str(&format!("Q:{q}\n"));
    }
    let key_display = meta.key.as_ref().map(|k| k.display.clone()).unwrap_or_else(|| "C".to_string());
    out.push_str(&format!("K:{}\n", key_display));
    out.push_str(&write_body(score, unit));
    if !out.ends_with('\n') { out.push('\n'); }
    out
}

/// Unit note length: ABC 2.1 default — measure duration < 3/4 → 1/16, else 1/8.
fn unit_length(score: &Score) -> Rational {
    let small = Rational::new(1, 16);
    let normal = Rational::new(1, 8);
    match score.metadata.meter.as_ref().and_then(|m| m.duration) {
        Some(dur) if (dur.numerator as u64) * 4 < (dur.denominator as u64) * 3 => small,
        _ => normal,
    }
}

fn tempo_field(score: &Score) -> Option<String> {
    let beat = score.metadata.tempo_model.as_ref()?.beat.as_ref()?;
    Some(format!("{}/{}={}", beat.beat_numerator, beat.beat_denominator, beat.bpm))
}

fn write_body(_score: &Score, _unit: Rational) -> String {
    String::new() // filled in Task 2+
}
```
(Confirm `TextLine` exposes `.text`; if the field name differs, adjust — grep `pub struct TextLine`.)
- [ ] **Step 4: run, expect PASS.** Then `cargo clippy -p croma-core --all-targets -- -D warnings` and `cargo fmt`.
- [ ] **Step 5: commit** — `feat(croma-core): scaffold write_abc with header emission`.

## Task 2: notes & rests (pitch, octave, accidental, duration)

**Files:** `crates/croma-core/src/to_abc.rs`

- [ ] **Step 1: failing test** — round-trip projection helper + a notes case:
```rust
fn roundtrip_pitches(src: &str) -> (Vec<(char,i8,i8)>, Vec<(char,i8,i8)>) {
    let s1 = score_of(src);
    let abc = write_abc(&s1, AbcWriteOptions::default());
    let s2 = score_of(&abc);
    (pitch_seq(&s1), pitch_seq(&s2))
}
fn pitch_seq(score: &crate::Score) -> Vec<(char,i8,i8)> {
    let mut v = Vec::new();
    for p in &score.parts { for voice in &p.voices { for e in &voice.events {
        if let crate::TimedEventKind::Note(n) = &e.kind {
            v.push((n.pitch.step, n.pitch.alter, n.pitch.octave));
        }
    }}}
    v
}

#[test]
fn notes_rests_octaves_accidentals_roundtrip() {
    for src in [
        "X:1\nL:1/8\nK:C\nC E G c c' C, z2 ^F _B =c\n",
        "X:1\nM:3/4\nL:1/4\nK:G\nGA B z\n",
    ] {
        let (a, b) = roundtrip_pitches(src);
        assert_eq!(a, b, "pitch round-trip failed for {src:?}");
    }
}
```
- [ ] **Step 2: run, expect FAIL** (body is empty → second score has no notes).
- [ ] **Step 3: implement** `write_body` to walk events and the note/rest/duration/pitch helpers:
```rust
fn write_body(score: &Score, unit: Rational) -> String {
    let mut out = String::new();
    let Some(voice) = score.parts.first().and_then(|p| p.voices.first()) else { return out };
    for event in &voice.events {
        match &event.kind {
            TimedEventKind::Note(note) => {
                out.push_str(&accidental_str(note.written_accidental.as_ref()));
                out.push_str(&pitch_str(&note.pitch));
                out.push_str(&length_str(event.duration, unit));
                if note.attachments.ties.iter().any(|t| t.role == crate::TieRole::Start) {
                    out.push('-');
                }
                out.push(' ');
            }
            TimedEventKind::Rest(rest) => {
                out.push(match rest.visibility { RestVisibility::Visible => 'z', RestVisibility::Invisible => 'x' });
                out.push_str(&length_str(event.duration, unit));
                out.push(' ');
            }
            _ => {} // barlines/endings in Task 3
        }
    }
    format!("{}\n", out.trim_end())
}

fn accidental_str(mark: Option<&crate::AccidentalMark>) -> String {
    match mark.map(|m| m.kind) {
        Some(Accidental::DoubleFlat) => "__",
        Some(Accidental::Flat) => "_",
        Some(Accidental::Natural) => "=",
        Some(Accidental::Sharp) => "^",
        Some(Accidental::DoubleSharp) => "^^",
        None => "",
    }.to_string()
}

fn pitch_str(pitch: &Pitch) -> String {
    let letter = pitch.step.to_ascii_uppercase();
    if pitch.octave >= 5 {
        let mut s = letter.to_ascii_lowercase().to_string();
        s.push_str(&"'".repeat((pitch.octave - 5) as usize));
        s
    } else {
        let mut s = letter.to_string();
        s.push_str(&",".repeat((4 - pitch.octave) as usize));
        s
    }
}

/// Length suffix for `duration` relative to `unit`: `mult = duration / unit`.
fn length_str(duration: Rational, unit: Rational) -> String {
    // mult = (dn*ud)/(dd*un), reduced.
    let mult = Rational::new(
        duration.numerator.saturating_mul(unit.denominator),
        duration.denominator.saturating_mul(unit.numerator),
    );
    match (mult.numerator, mult.denominator) {
        (1, 1) => String::new(),
        (n, 1) => n.to_string(),
        (1, 2) => "/".to_string(),
        (1, d) => format!("/{d}"),
        (n, d) => format!("{n}/{d}"),
    }
}
```
- [ ] **Step 4: run, expect PASS.** clippy + fmt.
- [ ] **Step 5: commit** — `feat(croma-core): write_abc note/rest emission`.

## Task 3: barlines

**Files:** `crates/croma-core/src/to_abc.rs`

- [ ] **Step 1: failing test**
```rust
#[test]
fn barlines_roundtrip() {
    for src in [
        "X:1\nL:1/4\nK:C\nCDEF | GABc |\n",
        "X:1\nL:1/4\nK:C\n|: CDEF :| GABc |]\n",
        "X:1\nL:1/4\nK:C\nCDEF || GABc\n",
    ] {
        let s1 = score_of(src);
        let abc = write_abc(&s1, AbcWriteOptions::default());
        let s2 = score_of(&abc);
        assert_eq!(barline_kinds(&s1), barline_kinds(&s2), "barlines for {src:?} -> {abc:?}");
    }
}
fn barline_kinds(score: &crate::Score) -> Vec<crate::BarlineKind> {
    let mut v = Vec::new();
    for p in &score.parts { for voice in &p.voices { for e in &voice.events {
        if let crate::TimedEventKind::Barline(b) = &e.kind { v.push(b.kind); }
    }}}
    v
}
```
- [ ] **Step 2: run, expect FAIL** (barlines dropped → empty vs non-empty).
- [ ] **Step 3: implement** — add a `Barline` arm in `write_body` using:
```rust
fn barline_str(kind: BarlineKind) -> &'static str {
    match kind {
        BarlineKind::Regular => "|",
        BarlineKind::Double => "||",
        BarlineKind::Final => "|]",
        BarlineKind::RepeatStart => "|:",
        BarlineKind::RepeatEnd => ":|",
        BarlineKind::RepeatBoth => "::",
        // out of slice-1 scope; emit a plain bar so output still parses
        BarlineKind::Initial | BarlineKind::Dotted | BarlineKind::Invisible | BarlineKind::Liberal => "|",
    }
}
```
In the `Barline` arm: `out.push_str(barline_str(b.kind)); out.push(' ');`
- [ ] **Step 4: run, expect PASS.** clippy + fmt.
- [ ] **Step 5: commit** — `feat(croma-core): write_abc barline emission`.

## Task 4: repeat endings + tie round-trip assertion

**Files:** `crates/croma-core/src/to_abc.rs`

- [ ] **Step 1: failing test**
```rust
#[test]
fn endings_and_ties_roundtrip() {
    let src = "X:1\nL:1/4\nK:C\n|: CDEF |1 GABc :|2 cBAG |]\n";
    let s1 = score_of(src);
    let abc = write_abc(&s1, AbcWriteOptions::default());
    let s2 = score_of(&abc);
    assert_eq!(ending_labels(&s1), ending_labels(&s2), "endings: {abc:?}");

    let tie = score_of("X:1\nL:1/4\nK:C\nC2- C2 |\n");
    let tie_abc = write_abc(&tie, AbcWriteOptions::default());
    assert!(tie_abc.contains("-"), "tie not emitted: {tie_abc:?}");
    assert_eq!(pitch_seq(&tie), pitch_seq(&score_of(&tie_abc)));
}
fn ending_labels(score: &crate::Score) -> Vec<String> {
    let mut v = Vec::new();
    for p in &score.parts { for voice in &p.voices { for e in &voice.events {
        if let crate::TimedEventKind::RepeatEnding(r) = &e.kind { v.push(format!("{:?}", r.endings)); }
    }}}
    v
}
```
- [ ] **Step 2: run, expect FAIL** (endings dropped).
- [ ] **Step 3: implement** — add a `RepeatEnding` arm:
```rust
fn ending_str(model: &crate::RepeatEndingModel) -> String {
    use crate::RepeatEndingPartModel::*;
    let parts: Vec<String> = model.endings.iter().map(|p| match p {
        Single(n) => n.to_string(),
        Range { start, end } => format!("{start}-{end}"),
    }).collect();
    format!("[{}", parts.join(","))
}
```
Arm: `out.push_str(&ending_str(r)); out.push(' ');` (ties already handled in Task 2).
- [ ] **Step 4: run, expect PASS.** clippy + fmt. Add a fmt-fixed-point test:
```rust
#[test]
fn output_is_a_fmt_fixed_point() {
    let abc = write_abc(&score_of("X:1\nL:1/8\nK:C\nCDE FGA | c2 z2\n"), AbcWriteOptions::default());
    assert_eq!(croma_fmt_format(&abc), abc);
}
```
where `croma_fmt_format` re-parses+formats. If pulling `croma-fmt` into a `croma-core` dev-dependency is undesirable (cycle risk), instead assert idempotency of `write_abc` over its own re-parse: `write_abc(score_of(&abc)) == abc`. **Prefer the idempotency form** to avoid a dev-dep cycle.
- [ ] **Step 5: commit** — `feat(croma-core): write_abc endings + tie + idempotency`.

## Task 5: CLI `croma dump abc`

**Files:** `crates/croma-cli/src/cli.rs` (add `Abc` to the `DumpKind` value enum), `crates/croma-cli/src/main.rs` (handle it in `run_dump`), `crates/croma-cli/tests/cli.rs`.

- [ ] **Step 1: failing CLI test** (in `tests/cli.rs`)
```rust
#[test]
fn dump_abc_roundtrips_a_simple_tune() {
    let dir = TestDir::new("dump-abc");
    let file = dir.write("t.abc", "X:1\nM:4/4\nL:1/8\nK:C\nCDEF GABc |\n");
    let output = run_croma([os("dump"), os("abc"), file.as_os_str()]);
    assert_success(&output);
    assert!(stdout(&output).contains("K:C"));
    assert!(stdout(&output).contains("CDEF"));
}
```
- [ ] **Step 2: run, expect FAIL** (`abc` not a valid dump kind).
- [ ] **Step 3: implement** — add `Abc` to the dump enum; in `run_dump`, the `Abc` branch lowers and prints `write_abc`:
```rust
DumpKind::Abc => {
    let lower = lower_score(&document, LowerOptions);
    diagnostics.extend(lower.diagnostics);
    emit_diagnostics(&options, &source_text, &input, &diagnostics)?;
    if diagnostics_should_fail(&diagnostics, options.warnings_as_errors) { return Ok(ExitCode::FAILURE); }
    let Some(score) = lower.value else { return Ok(ExitCode::FAILURE) };
    print!("{}", croma_core::write_abc(&score, croma_core::AbcWriteOptions::default()));
    flush_stdout()?;
}
```
- [ ] **Step 4: run, expect PASS** — `cargo test -p croma-cli`. clippy + fmt workspace.
- [ ] **Step 5: commit** — `feat(croma-cli): croma dump abc round-trips Score -> ABC`.

## Task 6: corpus round-trip harness

**Files:** Create `tools/prove_abc_roundtrip.py` (model on `tools/prove_fmt_lossless.py`).

- [ ] **Step 1** — write the harness: for each `*.abc` under `--abc-root`, decide in-scope by checking `croma dump score` (or a small `croma` flag) for chord/tuplet/multi-voice markers; for in-scope tunes compare the structural projection of `croma xml FILE` vs `croma xml <(croma dump abc FILE)` — extend `prove_fmt_lossless.py`'s `pitch_seq` with `<duration>`, `<bar-style>`/measure count, and `<tie>`. Report `total`, `in_scope`, `structural_diffs` (must be 0), and coverage %. Write JSON to `docs/untracked/abc/`. Local only; never CI.
- [ ] **Step 2** — run locally over the 10k; confirm `structural_diffs == 0` on the in-scope subset; record coverage %.
- [ ] **Step 3: commit** — `test: ABC round-trip corpus harness (local-only)`.

## Task 7: close out

- [ ] `cargo test --workspace`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo fmt --all -- --check`; `uv run pytest -q` (if touched); `git diff --check`.
- [ ] `git log main..HEAD --format=%b | grep -ci Co-Authored-By` == 0.
- [ ] `cargo publish -p croma-core --dry-run` still succeeds (no new deps).
- [ ] Update tracker (new phase row + validations + coverage %), export SQL snapshot, commit.
- [ ] Push; open PR; merge only when both CI checks are green; delete branch.

## Self-review notes

- Spec coverage: headers (T1), notes/rests/durations/accidentals/octaves (T2), barlines+repeats (T3), endings+ties (T4), `dump abc` (T5), corpus projection harness (T6), gates/publish/tracker (T7) — all design sections mapped.
- Known unconfirmed field name: `TextLine.text` (Task 1) — grep `pub struct TextLine` and adjust the accessor if needed. `croma_fmt` dev-dep cycle is avoided by the idempotency form of the fixed-point test (Task 4).
