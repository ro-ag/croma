use crate::diagnostic::{Diagnostic, RecoveryNote, Severity, Span, SpecReference};
use crate::fields::{FieldState, MeterKind};
use crate::model::{
    Accidental, BarlineKind, Event, Fraction, RestVisibility, TimedEvent, TimedEventKind, lcm,
};
use crate::parser::ParseReport;
use crate::source::SourceText;
use crate::surface::{LineContext, LineKind, ScoreLineBreak, SurfaceMap};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedMusicDocument {
    pub tunes: Vec<ParsedTuneMusic>,
}

impl ParsedMusicDocument {
    pub fn tune(&self, tune_index: usize) -> Option<&ParsedTuneMusic> {
        self.tunes.iter().find(|tune| tune.tune_index == tune_index)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTuneMusic {
    pub tune_index: usize,
    pub span: Span,
    pub lines: Vec<MusicLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicLine {
    pub line_index: usize,
    pub span: Span,
    pub code_span: Span,
    pub tokens: Vec<MusicToken>,
    pub items: Vec<MusicItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MusicToken {
    pub kind: MusicTokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MusicTokenKind {
    Whitespace,
    Accidental,
    Pitch,
    OctaveMark,
    Length,
    Rest,
    MultiMeasureRest,
    Spacer,
    Barline,
    InlineField,
    Unsupported,
    Malformed,
    Comment,
    ScoreLineBreak,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MusicItem {
    Note(NoteSyntax),
    Rest(RestSyntax),
    MultiMeasureRest(MultiMeasureRestSyntax),
    Spacer(SpacerSyntax),
    Barline(BarlineSyntax),
    InlineField(InlineFieldSyntax),
    Unsupported(UnsupportedSyntax),
    Malformed(MalformedSyntax),
}

impl MusicItem {
    pub fn span(&self) -> Span {
        match self {
            Self::Note(item) => item.span,
            Self::Rest(item) => item.span,
            Self::MultiMeasureRest(item) => item.span,
            Self::Spacer(item) => item.span,
            Self::Barline(item) => item.span,
            Self::InlineField(item) => item.span,
            Self::Unsupported(item) => item.span,
            Self::Malformed(item) => item.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteSyntax {
    pub span: Span,
    pub accidental: Option<AccidentalSyntax>,
    pub pitch: PitchSyntax,
    pub octave_marks: Vec<OctaveMarkSyntax>,
    pub length: Option<LengthSyntax>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PitchSyntax {
    pub step: char,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccidentalSyntax {
    pub sign: Accidental,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OctaveMarkSyntax {
    pub mark: OctaveMark,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OctaveMark {
    Lower,
    Raise,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LengthSyntax {
    pub span: Span,
    pub raw: String,
    pub numerator: Option<SpannedNumber>,
    pub slash_count: u8,
    pub denominator: Option<SpannedNumber>,
    pub multiplier: Fraction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpannedNumber {
    pub value: u32,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestSyntax {
    pub span: Span,
    pub visibility: RestVisibility,
    pub marker_span: Span,
    pub length: Option<LengthSyntax>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiMeasureRestSyntax {
    pub span: Span,
    pub visibility: RestVisibility,
    pub marker_span: Span,
    pub count: Option<SpannedNumber>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpacerSyntax {
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarlineSyntax {
    pub span: Span,
    pub kind: BarlineKind,
    pub dotted: bool,
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineFieldSyntax {
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedSyntax {
    pub span: Span,
    pub kind: UnsupportedSyntaxKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedSyntaxKind {
    Chord,
    GraceGroup,
    Tuplet,
    Slur,
    Tie,
    Decoration,
    BrokenRhythm,
    RepeatEnding,
    QuotedText,
    Reserved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MalformedSyntax {
    pub span: Span,
    pub kind: MalformedSyntaxKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MalformedSyntaxKind {
    DanglingAccidental,
    StandaloneOctave,
    StandaloneLength,
    InvalidLength,
    UnclosedInlineField,
    UnclosedChord,
    UnclosedGraceGroup,
    UnclosedQuotedText,
    InvalidBarline,
    UnknownToken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredMusic {
    pub events: Vec<Event>,
    pub divisions: u32,
}

pub(crate) fn parse_music_document(
    source: &SourceText,
    surface: &SurfaceMap,
) -> ParseReport<ParsedMusicDocument> {
    let mut diagnostics = Vec::new();
    let mut tunes = surface
        .line_map
        .tunes
        .iter()
        .map(|tune| ParsedTuneMusic {
            tune_index: tune.index,
            span: tune.body_span,
            lines: Vec::new(),
        })
        .collect::<Vec<_>>();

    for line in &surface.line_map.lines {
        if line.kind != LineKind::MusicCode {
            continue;
        }
        let LineContext::TuneBody { tune_index } = line.context else {
            continue;
        };

        let Some(tune) = tunes.iter_mut().find(|tune| tune.tune_index == tune_index) else {
            continue;
        };
        let Some(line_text) = source.slice(line.text_span) else {
            continue;
        };

        let code_span = music_code_span(line);
        let Some(code_text) = source.slice(code_span) else {
            continue;
        };

        let mut parser = MusicLineParser::new(code_text, code_span.start);
        let mut parsed_line = parser.parse(line.index, line.span, code_span);
        diagnostics.extend(parser.diagnostics);

        if let ScoreLineBreak::Suppressed { marker_span } = line.score_line_break {
            parsed_line.tokens.push(MusicToken {
                kind: MusicTokenKind::ScoreLineBreak,
                span: marker_span,
            });
        }
        if let Some(comment_span) = line.trailing_comment {
            parsed_line.tokens.push(MusicToken {
                kind: MusicTokenKind::Comment,
                span: comment_span,
            });
        } else if code_span.end < line.text_span.end
            && line_text[code_span.end - line.text_span.start..]
                .trim_start()
                .starts_with('%')
        {
            parsed_line.tokens.push(MusicToken {
                kind: MusicTokenKind::Comment,
                span: Span::new(code_span.end, line.text_span.end),
            });
        }

        parsed_line.tokens.sort_by_key(|token| token.span.start);
        tune.lines.push(parsed_line);
    }

    ParseReport::new(ParsedMusicDocument { tunes }, diagnostics)
}

pub(crate) fn lower_tune_music(
    tune_music: &ParsedTuneMusic,
    field_state: &FieldState,
) -> ParseReport<LoweredMusic> {
    let unit = field_state.unit_note_length_fraction();
    let meter_duration = field_state
        .meter
        .as_ref()
        .and_then(|meter| meter_duration(&meter.value.kind));
    let mut diagnostics = Vec::new();
    let mut lowered = Vec::new();

    for line in &tune_music.lines {
        for item in &line.items {
            match item {
                MusicItem::Note(note) => lowered.push(LoweredEvent::Timed(TimedEvent {
                    kind: TimedEventKind::Note {
                        step: note.pitch.step.to_ascii_uppercase(),
                        octave: lowered_octave(note),
                        accidental: note.accidental.map(|accidental| accidental.sign),
                        span: note.span,
                    },
                    duration: unit.checked_mul(length_multiplier(note.length.as_ref())),
                })),
                MusicItem::Rest(rest) => lowered.push(LoweredEvent::Timed(TimedEvent {
                    kind: TimedEventKind::Rest {
                        visibility: rest.visibility,
                        span: rest.span,
                    },
                    duration: unit.checked_mul(length_multiplier(rest.length.as_ref())),
                })),
                MusicItem::MultiMeasureRest(rest) => {
                    let count = rest.count.map(|count| count.value).unwrap_or(1);
                    let duration = if let Some(meter_duration) = meter_duration {
                        meter_duration.checked_mul_u32(count)
                    } else {
                        diagnostics.push(free_meter_multirest_warning(rest.span));
                        unit.checked_mul_u32(count)
                    };
                    lowered.push(LoweredEvent::Timed(TimedEvent {
                        kind: TimedEventKind::Rest {
                            visibility: rest.visibility,
                            span: rest.span,
                        },
                        duration,
                    }));
                }
                MusicItem::Spacer(spacer) => {
                    lowered.push(LoweredEvent::Untimed(Event::Spacer { span: spacer.span }))
                }
                MusicItem::Barline(barline) => {
                    if matches!(barline.kind, BarlineKind::Dotted | BarlineKind::Invisible) {
                        diagnostics.push(barline_export_policy_info(barline.span, barline.kind));
                    }
                    lowered.push(LoweredEvent::Untimed(Event::Barline {
                        kind: barline.kind,
                        span: barline.span,
                    }));
                }
                MusicItem::InlineField(_) | MusicItem::Unsupported(_) | MusicItem::Malformed(_) => {
                }
            }
        }
    }

    let divisions = lowered.iter().fold(8, |divisions, event| match event {
        LoweredEvent::Timed(event) => lcm(divisions, event.duration.divisions_requirement()),
        LoweredEvent::Untimed(_) => divisions,
    });
    let events = lowered
        .into_iter()
        .map(|event| match event {
            LoweredEvent::Timed(event) => event.into_event(divisions),
            LoweredEvent::Untimed(event) => event,
        })
        .collect();

    ParseReport::new(LoweredMusic { events, divisions }, diagnostics)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LoweredEvent {
    Timed(TimedEvent),
    Untimed(Event),
}

fn music_code_span(line: &crate::surface::ClassifiedLine) -> Span {
    let mut end = line.text_span.end;
    if let Some(comment_span) = line.trailing_comment {
        end = end.min(comment_span.start);
    }
    if let ScoreLineBreak::Suppressed { marker_span } = line.score_line_break {
        end = end.min(marker_span.start);
    }
    Span::new(line.text_span.start, end)
}

fn lowered_octave(note: &NoteSyntax) -> i8 {
    let base_octave = if note.pitch.step.is_ascii_lowercase() {
        5
    } else {
        4
    };
    let adjustment = note
        .octave_marks
        .iter()
        .map(|mark| match mark.mark {
            OctaveMark::Lower => -1,
            OctaveMark::Raise => 1,
        })
        .sum::<i8>();
    base_octave + adjustment
}

fn length_multiplier(length: Option<&LengthSyntax>) -> Fraction {
    length
        .map(|length| length.multiplier)
        .unwrap_or_else(Fraction::one)
}

fn meter_duration(kind: &MeterKind) -> Option<Fraction> {
    match kind {
        MeterKind::CommonTime => Some(Fraction::new(4, 4)),
        MeterKind::CutTime => Some(Fraction::new(2, 2)),
        MeterKind::Fraction {
            numerator,
            denominator,
        } => Some(Fraction::new(*numerator, *denominator)),
        MeterKind::None | MeterKind::Complex => None,
    }
}

struct MusicLineParser<'line> {
    text: &'line str,
    line_offset: usize,
    index: usize,
    tokens: Vec<MusicToken>,
    items: Vec<MusicItem>,
    diagnostics: Vec<Diagnostic>,
}

impl<'line> MusicLineParser<'line> {
    fn new(text: &'line str, line_offset: usize) -> Self {
        Self {
            text,
            line_offset,
            index: 0,
            tokens: Vec::new(),
            items: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn parse(&mut self, line_index: usize, span: Span, code_span: Span) -> MusicLine {
        while self.index < self.text.len() {
            let Some(ch) = self.peek_char() else {
                break;
            };

            match ch {
                ch if ch.is_whitespace() => self.parse_whitespace(),
                '^' | '_' | '=' => self.parse_accidental_or_malformed(),
                'A'..='G' | 'a'..='g' => self.parse_note(None),
                'z' => self.parse_rest(RestVisibility::Visible),
                'x' => self.parse_rest(RestVisibility::Invisible),
                'Z' => self.parse_multi_measure_rest(RestVisibility::Visible),
                'X' => self.parse_multi_measure_rest(RestVisibility::Invisible),
                'y' => self.parse_spacer(),
                '.' => self.parse_dot(),
                '[' => self.parse_left_bracket(),
                '|' | ']' => self.parse_barline(false),
                ':' => self.parse_colon(),
                '"' => self.parse_quoted_text(),
                '{' => self.parse_group(
                    '}',
                    UnsupportedSyntaxKind::GraceGroup,
                    MalformedSyntaxKind::UnclosedGraceGroup,
                    "abc.music.unsupported_grace",
                    "abc.music.unclosed_grace",
                    "Grace groups are preserved but not lowered in this phase",
                ),
                '(' => self.parse_open_paren(),
                ')' => self.parse_unsupported_single(
                    UnsupportedSyntaxKind::Slur,
                    "abc.music.unsupported_slur",
                    "Slurs are preserved but not lowered in this phase",
                ),
                '<' | '>' => self.parse_unsupported_single(
                    UnsupportedSyntaxKind::BrokenRhythm,
                    "abc.music.unsupported_broken_rhythm",
                    "Broken rhythm markers are preserved but not lowered in this phase",
                ),
                '-' => self.parse_unsupported_single(
                    UnsupportedSyntaxKind::Tie,
                    "abc.music.unsupported_tie",
                    "Ties are preserved but not lowered in this phase",
                ),
                '!' | '+' => self.parse_decoration(ch),
                '\'' | ',' => self.parse_malformed_single(
                    MalformedSyntaxKind::StandaloneOctave,
                    "abc.music.malformed_octave",
                    "Octave marks must follow a note",
                ),
                '/' => self.parse_malformed_single(
                    MalformedSyntaxKind::StandaloneLength,
                    "abc.music.malformed_length",
                    "Length suffixes must follow a note or rest",
                ),
                ch if ch.is_ascii_digit() => self.parse_malformed_digits(),
                '#' | '*' | ';' | '?' | '@' => self.parse_unsupported_single(
                    UnsupportedSyntaxKind::Reserved,
                    "abc.music.reserved",
                    "Reserved music character was preserved and skipped",
                ),
                _ => self.parse_malformed_single(
                    MalformedSyntaxKind::UnknownToken,
                    "abc.music.unknown_token",
                    "Unknown music token was preserved and skipped",
                ),
            }
        }

        MusicLine {
            line_index,
            span,
            code_span,
            tokens: std::mem::take(&mut self.tokens),
            items: std::mem::take(&mut self.items),
        }
    }

    fn parse_whitespace(&mut self) {
        let start = self.index;
        while self.peek_char().is_some_and(char::is_whitespace) {
            self.bump_char();
        }
        self.push_token(MusicTokenKind::Whitespace, self.span(start, self.index));
    }

    fn parse_accidental_or_malformed(&mut self) {
        let Some(accidental) = self.parse_accidental_token() else {
            return;
        };
        if self.peek_char().is_some_and(is_note_letter) {
            self.parse_note(Some(accidental));
            return;
        }

        self.push_malformed(
            accidental.span,
            MalformedSyntaxKind::DanglingAccidental,
            "abc.music.malformed_accidental",
            "Accidentals must appear immediately before a note",
        );
    }

    fn parse_note(&mut self, accidental: Option<AccidentalSyntax>) {
        let start = accidental
            .as_ref()
            .map(|accidental| accidental.span.start - self.line_offset)
            .unwrap_or(self.index);
        let pitch_start = self.index;
        let Some(step) = self.bump_char() else {
            return;
        };
        let pitch_span = self.span(pitch_start, self.index);
        self.push_token(MusicTokenKind::Pitch, pitch_span);

        let octave_marks = self.parse_octave_marks();
        let length = self.parse_length_suffix();
        let end = length
            .as_ref()
            .map(|length| length.span.end - self.line_offset)
            .or_else(|| {
                octave_marks
                    .last()
                    .map(|mark| mark.span.end - self.line_offset)
            })
            .unwrap_or(self.index);
        let note = NoteSyntax {
            span: self.span(start, end),
            accidental,
            pitch: PitchSyntax {
                step,
                span: pitch_span,
            },
            octave_marks,
            length,
        };
        self.items.push(MusicItem::Note(note));
    }

    fn parse_accidental_token(&mut self) -> Option<AccidentalSyntax> {
        let start = self.index;
        let sign = if self.starts_with("__") {
            self.index += 2;
            Accidental::DoubleFlat
        } else if self.starts_with("^^") {
            self.index += 2;
            Accidental::DoubleSharp
        } else {
            match self.bump_char()? {
                '_' => Accidental::Flat,
                '^' => Accidental::Sharp,
                '=' => Accidental::Natural,
                _ => return None,
            }
        };
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Accidental, span);
        Some(AccidentalSyntax { sign, span })
    }

    fn parse_octave_marks(&mut self) -> Vec<OctaveMarkSyntax> {
        let mut marks = Vec::new();
        while let Some(ch @ ('\'' | ',')) = self.peek_char() {
            let start = self.index;
            self.bump_char();
            let span = self.span(start, self.index);
            self.push_token(MusicTokenKind::OctaveMark, span);
            marks.push(OctaveMarkSyntax {
                mark: if ch == '\'' {
                    OctaveMark::Raise
                } else {
                    OctaveMark::Lower
                },
                span,
            });
        }
        marks
    }

    fn parse_rest(&mut self, visibility: RestVisibility) {
        let start = self.index;
        self.bump_char();
        let marker_span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Rest, marker_span);
        let length = self.parse_length_suffix();
        let span = length
            .as_ref()
            .map(|length| Span::new(marker_span.start, length.span.end))
            .unwrap_or(marker_span);
        self.items.push(MusicItem::Rest(RestSyntax {
            span,
            visibility,
            marker_span,
            length,
        }));
    }

    fn parse_multi_measure_rest(&mut self, visibility: RestVisibility) {
        let start = self.index;
        self.bump_char();
        let marker_span = self.span(start, self.index);
        self.push_token(MusicTokenKind::MultiMeasureRest, marker_span);
        let count = self.parse_number_token();
        if let Some(count) = count {
            self.push_token(MusicTokenKind::Length, count.span);
        }
        let span = count
            .map(|count| Span::new(marker_span.start, count.span.end))
            .unwrap_or(marker_span);
        self.items
            .push(MusicItem::MultiMeasureRest(MultiMeasureRestSyntax {
                span,
                visibility,
                marker_span,
                count,
            }));
    }

    fn parse_spacer(&mut self) {
        let start = self.index;
        self.bump_char();
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Spacer, span);
        self.items.push(MusicItem::Spacer(SpacerSyntax { span }));
    }

    fn parse_dot(&mut self) {
        if self.peek_next_char().is_some_and(is_barline_char) {
            self.parse_barline(true);
            return;
        }
        self.parse_unsupported_single(
            UnsupportedSyntaxKind::Decoration,
            "abc.music.unsupported_decoration",
            "Decorations are preserved but not lowered in this phase",
        );
    }

    fn parse_left_bracket(&mut self) {
        if self.starts_with("[|]") || self.peek_next_char().is_some_and(is_barline_char) {
            self.parse_barline(false);
            return;
        }

        if self.is_inline_field_start() {
            self.parse_inline_field();
            return;
        }

        if self.peek_next_char().is_some_and(|ch| ch.is_ascii_digit()) {
            let start = self.index;
            self.bump_char();
            while self
                .peek_char()
                .is_some_and(|ch| ch.is_ascii_digit() || matches!(ch, ',' | '-' | '.'))
            {
                self.bump_char();
            }
            let span = self.span(start, self.index);
            self.push_token(MusicTokenKind::Unsupported, span);
            self.items.push(MusicItem::Unsupported(UnsupportedSyntax {
                span,
                kind: UnsupportedSyntaxKind::RepeatEnding,
            }));
            self.push_unsupported_diagnostic(
                span,
                "abc.music.unsupported_repeat_ending",
                "Repeat endings are preserved but not lowered in this phase",
            );
            return;
        }

        self.parse_group(
            ']',
            UnsupportedSyntaxKind::Chord,
            MalformedSyntaxKind::UnclosedChord,
            "abc.music.unsupported_chord",
            "abc.music.unclosed_chord",
            "Chord groups are preserved but not lowered in this phase",
        );
    }

    fn parse_barline(&mut self, dotted: bool) {
        let start = self.index;
        if dotted {
            self.bump_char();
        }
        while self.peek_char().is_some_and(is_barline_char) {
            self.bump_char();
        }
        let span = self.span(start, self.index);
        let raw = self.text[start..self.index].to_owned();
        let raw_without_dot = raw.strip_prefix('.').unwrap_or(&raw);
        let kind = barline_kind(raw_without_dot, dotted);

        self.push_token(MusicTokenKind::Barline, span);
        if kind == BarlineKind::Liberal {
            self.diagnostics.push(liberal_barline_warning(span, &raw));
        } else if kind == BarlineKind::Dotted || kind == BarlineKind::Invisible {
            self.diagnostics
                .push(barline_syntax_policy_info(span, kind));
        }
        self.items.push(MusicItem::Barline(BarlineSyntax {
            span,
            kind,
            dotted,
            raw,
        }));
    }

    fn parse_colon(&mut self) {
        if self.starts_with("::") {
            self.parse_barline(false);
            return;
        }
        if self.peek_next_char() == Some('|') {
            self.parse_barline(false);
            return;
        }
        self.parse_malformed_single(
            MalformedSyntaxKind::InvalidBarline,
            "abc.music.invalid_barline",
            "A repeat dot must be part of a barline spelling",
        );
    }

    fn parse_inline_field(&mut self) {
        let start = self.index;
        let mut closed = false;
        while let Some(ch) = self.bump_char() {
            if ch == ']' {
                closed = true;
                break;
            }
        }
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::InlineField, span);
        if closed {
            self.items
                .push(MusicItem::InlineField(InlineFieldSyntax { span }));
        } else {
            self.push_malformed(
                span,
                MalformedSyntaxKind::UnclosedInlineField,
                "abc.music.unclosed_inline_field",
                "Inline field was preserved and skipped",
            );
        }
    }

    fn parse_quoted_text(&mut self) {
        let start = self.index;
        self.bump_char();
        let mut closed = false;
        while let Some(ch) = self.bump_char() {
            if ch == '"' && !is_escaped(self.text, self.index - ch.len_utf8()) {
                closed = true;
                break;
            }
        }
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Unsupported, span);
        if closed {
            self.items.push(MusicItem::Unsupported(UnsupportedSyntax {
                span,
                kind: UnsupportedSyntaxKind::QuotedText,
            }));
            self.push_unsupported_diagnostic(
                span,
                "abc.music.unsupported_quoted_text",
                "Quoted chord symbols and annotations are preserved but not lowered in this phase",
            );
        } else {
            self.push_malformed(
                span,
                MalformedSyntaxKind::UnclosedQuotedText,
                "abc.music.unclosed_quoted_text",
                "Quoted text was preserved and skipped",
            );
        }
    }

    fn parse_group(
        &mut self,
        close: char,
        unsupported_kind: UnsupportedSyntaxKind,
        malformed_kind: MalformedSyntaxKind,
        unsupported_code: &'static str,
        malformed_code: &'static str,
        message: &'static str,
    ) {
        let start = self.index;
        self.bump_char();
        let mut closed = false;
        while let Some(ch) = self.bump_char() {
            if ch == close {
                closed = true;
                break;
            }
        }
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Unsupported, span);
        if closed {
            self.items.push(MusicItem::Unsupported(UnsupportedSyntax {
                span,
                kind: unsupported_kind,
            }));
            self.push_unsupported_diagnostic(span, unsupported_code, message);
        } else {
            self.push_malformed(span, malformed_kind, malformed_code, message);
        }
    }

    fn parse_open_paren(&mut self) {
        let start = self.index;
        self.bump_char();
        if self.peek_char().is_some_and(|ch| ch.is_ascii_digit()) {
            while self
                .peek_char()
                .is_some_and(|ch| ch.is_ascii_digit() || ch == ':')
            {
                self.bump_char();
            }
            let span = self.span(start, self.index);
            self.push_token(MusicTokenKind::Unsupported, span);
            self.items.push(MusicItem::Unsupported(UnsupportedSyntax {
                span,
                kind: UnsupportedSyntaxKind::Tuplet,
            }));
            self.push_unsupported_diagnostic(
                span,
                "abc.music.unsupported_tuplet",
                "Tuplets are preserved but not lowered in this phase",
            );
        } else {
            let span = self.span(start, self.index);
            self.push_token(MusicTokenKind::Unsupported, span);
            self.items.push(MusicItem::Unsupported(UnsupportedSyntax {
                span,
                kind: UnsupportedSyntaxKind::Slur,
            }));
            self.push_unsupported_diagnostic(
                span,
                "abc.music.unsupported_slur",
                "Slurs are preserved but not lowered in this phase",
            );
        }
    }

    fn parse_decoration(&mut self, delimiter: char) {
        let start = self.index;
        self.bump_char();
        while let Some(ch) = self.peek_char() {
            if ch == delimiter {
                self.bump_char();
                break;
            }
            if ch.is_whitespace() || is_barline_char(ch) {
                break;
            }
            self.bump_char();
        }
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Unsupported, span);
        self.items.push(MusicItem::Unsupported(UnsupportedSyntax {
            span,
            kind: UnsupportedSyntaxKind::Decoration,
        }));
        self.push_unsupported_diagnostic(
            span,
            "abc.music.unsupported_decoration",
            "Decorations are preserved but not lowered in this phase",
        );
    }

    fn parse_unsupported_single(
        &mut self,
        kind: UnsupportedSyntaxKind,
        code: &'static str,
        message: &'static str,
    ) {
        let start = self.index;
        self.bump_char();
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Unsupported, span);
        self.items
            .push(MusicItem::Unsupported(UnsupportedSyntax { span, kind }));
        self.push_unsupported_diagnostic(span, code, message);
    }

    fn parse_malformed_single(
        &mut self,
        kind: MalformedSyntaxKind,
        code: &'static str,
        message: &'static str,
    ) {
        let start = self.index;
        self.bump_char();
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Malformed, span);
        self.push_malformed(span, kind, code, message);
    }

    fn parse_malformed_digits(&mut self) {
        let start = self.index;
        while self.peek_char().is_some_and(|ch| ch.is_ascii_digit()) {
            self.bump_char();
        }
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Malformed, span);
        self.push_malformed(
            span,
            MalformedSyntaxKind::StandaloneLength,
            "abc.music.malformed_length",
            "Length suffixes must follow a note or rest",
        );
    }

    fn parse_length_suffix(&mut self) -> Option<LengthSyntax> {
        let start = self.index;
        let numerator = self.parse_number_token();
        let mut slash_count = 0u8;
        let mut denominator = None;

        if self.peek_char() == Some('/') {
            while self.peek_char() == Some('/') {
                slash_count = slash_count.saturating_add(1);
                self.bump_char();
            }
            denominator = self.parse_number_token();
        }

        if numerator.is_none() && slash_count == 0 {
            return None;
        }

        let end = self.index;
        let span = self.span(start, end);
        self.push_token(MusicTokenKind::Length, span);
        let numerator_value = numerator.map(|number| number.value).unwrap_or(1);
        let denominator_value = match (slash_count, denominator) {
            (0, _) => 1,
            (_, Some(number)) if number.value == 0 => {
                self.diagnostics.push(invalid_length_warning(
                    number.span,
                    "Length denominator cannot be zero; recovered as denominator 1",
                ));
                1
            }
            (_, Some(number)) => number.value,
            (slashes, None) => slash_denominator(slashes).unwrap_or_else(|| {
                self.diagnostics.push(invalid_length_warning(
                    span,
                    "Length slash shorthand is too long; recovered as denominator 1",
                ));
                1
            }),
        };
        let numerator_value = if let Some(number) = numerator
            && number.value == 0
        {
            self.diagnostics.push(invalid_length_warning(
                number.span,
                "Length numerator cannot be zero; recovered as numerator 1",
            ));
            1
        } else {
            numerator_value
        };

        Some(LengthSyntax {
            span,
            raw: self.text[start..end].to_owned(),
            numerator,
            slash_count,
            denominator,
            multiplier: Fraction::new(numerator_value, denominator_value),
        })
    }

    fn parse_number_token(&mut self) -> Option<SpannedNumber> {
        let start = self.index;
        while self.peek_char().is_some_and(|ch| ch.is_ascii_digit()) {
            self.bump_char();
        }
        if start == self.index {
            return None;
        }
        let span = self.span(start, self.index);
        let raw = &self.text[start..self.index];
        let value = raw.parse().unwrap_or_else(|_| {
            self.diagnostics.push(invalid_length_warning(
                span,
                "Number is too large; recovered as 1",
            ));
            1
        });
        Some(SpannedNumber { value, span })
    }

    fn is_inline_field_start(&self) -> bool {
        let mut chars = self.text[self.index..].chars();
        matches!(chars.next(), Some('['))
            && chars.next().is_some_and(|ch| ch.is_ascii_alphabetic())
            && matches!(chars.next(), Some(':'))
    }

    fn push_token(&mut self, kind: MusicTokenKind, span: Span) {
        self.tokens.push(MusicToken { kind, span });
    }

    fn push_malformed(
        &mut self,
        span: Span,
        kind: MalformedSyntaxKind,
        code: &'static str,
        message: &'static str,
    ) {
        self.items
            .push(MusicItem::Malformed(MalformedSyntax { span, kind }));
        self.diagnostics.push(
            Diagnostic::new(Severity::Warning, code, message, span)
                .with_spec_reference(abc_music_reference())
                .with_recovery_note(RecoveryNote::new(
                    "The malformed token was preserved and skipped.",
                )),
        );
    }

    fn push_unsupported_diagnostic(
        &mut self,
        span: Span,
        code: &'static str,
        message: &'static str,
    ) {
        self.diagnostics.push(
            Diagnostic::new(Severity::Warning, code, message, span)
                .with_spec_reference(abc_music_reference())
                .with_recovery_note(RecoveryNote::new(
                    "The construct remains in the syntax tree but does not produce notes yet.",
                )),
        );
    }

    fn peek_char(&self) -> Option<char> {
        self.text[self.index..].chars().next()
    }

    fn peek_next_char(&self) -> Option<char> {
        let mut chars = self.text[self.index..].chars();
        chars.next()?;
        chars.next()
    }

    fn bump_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.index += ch.len_utf8();
        Some(ch)
    }

    fn starts_with(&self, pattern: &str) -> bool {
        self.text[self.index..].starts_with(pattern)
    }

    fn span(&self, start: usize, end: usize) -> Span {
        Span::new(self.line_offset + start, self.line_offset + end)
    }
}

fn barline_kind(raw: &str, dotted: bool) -> BarlineKind {
    if dotted {
        return BarlineKind::Dotted;
    }

    match raw {
        "|" => BarlineKind::Regular,
        "||" => BarlineKind::Double,
        "|]" => BarlineKind::Final,
        "[|" => BarlineKind::Initial,
        "|:" | "|::" => BarlineKind::RepeatStart,
        ":|" | "::|" => BarlineKind::RepeatEnd,
        "::" | ":|:" | ":||:" => BarlineKind::RepeatBoth,
        "[|]" => BarlineKind::Invisible,
        _ => BarlineKind::Liberal,
    }
}

fn is_note_letter(ch: char) -> bool {
    matches!(ch, 'A'..='G' | 'a'..='g')
}

fn is_barline_char(ch: char) -> bool {
    matches!(ch, '|' | '[' | ']' | ':')
}

fn is_escaped(text: &str, offset: usize) -> bool {
    let mut slash_count = 0;
    for byte in text[..offset].bytes().rev() {
        if byte == b'\\' {
            slash_count += 1;
        } else {
            break;
        }
    }
    slash_count % 2 == 1
}

fn slash_denominator(slash_count: u8) -> Option<u32> {
    1u32.checked_shl(u32::from(slash_count))
}

fn invalid_length_warning(span: Span, message: &'static str) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.malformed_length",
        message,
        span,
    )
    .with_spec_reference(abc_music_reference())
    .with_recovery_note(RecoveryNote::new(
        "The length suffix was preserved and a safe duration was used.",
    ))
}

fn liberal_barline_warning(span: Span, raw: &str) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.barline.liberal",
        format!("Liberal barline spelling `{raw}` was normalized as a measure boundary"),
        span,
    )
    .with_spec_reference(abc_barline_reference())
    .with_recovery_note(RecoveryNote::new(
        "The exact spelling is preserved in syntax; lowering uses a regular barline.",
    ))
}

fn barline_syntax_policy_info(span: Span, kind: BarlineKind) -> Diagnostic {
    Diagnostic::new(
        Severity::Info,
        "abc.music.barline.policy",
        match kind {
            BarlineKind::Dotted => "Dotted barline was preserved for MusicXML export policy",
            BarlineKind::Invisible => "Invisible barline was preserved for MusicXML export policy",
            _ => "Barline was preserved for MusicXML export policy",
        },
        span,
    )
    .with_spec_reference(abc_barline_reference())
}

fn barline_export_policy_info(span: Span, kind: BarlineKind) -> Diagnostic {
    Diagnostic::new(
        Severity::Info,
        "abc.musicxml.barline_policy",
        match kind {
            BarlineKind::Dotted => "Dotted barline is exported as a MusicXML dotted bar-style",
            BarlineKind::Invisible => "Invisible barline is exported as a MusicXML none bar-style",
            _ => "Barline export policy applied",
        },
        span,
    )
    .with_spec_reference(abc_barline_reference())
}

fn free_meter_multirest_warning(span: Span) -> Diagnostic {
    Diagnostic::new(
        Severity::Warning,
        "abc.music.multirest.free_meter",
        "Multi-measure rest in free meter has no measure duration; recovered using unit note length",
        span,
    )
    .with_spec_reference(abc_rest_reference())
    .with_recovery_note(RecoveryNote::new(
        "The rest count was preserved and each measure was lowered as one unit note length.",
    ))
}

fn abc_music_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 tune body")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

fn abc_barline_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.8 repeat/bar symbols")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

fn abc_rest_reference() -> SpecReference {
    SpecReference::new("ABC 2.1 section 4.5 rests")
        .with_url("https://abcnotation.com/wiki/abc:standard:v2.1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::ParseOptions;
    use crate::parser::{parse_document, parse_tune_report_from_document};

    fn events_for(source: &str) -> (Vec<Event>, Vec<Diagnostic>) {
        let document = parse_document(source, ParseOptions::default()).value;
        let report = parse_tune_report_from_document(&document);
        (
            report.value.expect("expected tune").events,
            report.diagnostics,
        )
    }

    fn count_diagnostics(diagnostics: &[Diagnostic], code: &'static str) -> usize {
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == code)
            .count()
    }

    #[test]
    fn normalizes_pitch_case_and_mixed_octave_marks() {
        let (events, diagnostics) = events_for("X:1\nL:1/8\nK:C\nC C' c C,',\n");

        assert!(diagnostics.is_empty());
        let octaves = events
            .iter()
            .filter_map(|event| match event {
                Event::Note { octave, .. } => Some(*octave),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(octaves, vec![4, 5, 5, 3]);
    }

    #[test]
    fn recovers_standalone_octave_marks_without_attaching_to_neighbor_notes() {
        let document_report = parse_document("X:1\nL:1/8\nK:C\n' , C\n", ParseOptions::default());
        assert_eq!(
            count_diagnostics(&document_report.diagnostics, "abc.music.malformed_octave"),
            2
        );

        let tune_music = document_report
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");
        let malformed = tune_music.lines[0]
            .items
            .iter()
            .filter_map(|item| match item {
                MusicItem::Malformed(item) => Some(item),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(malformed.len(), 2);
        assert!(malformed.iter().all(|item| !item.span.is_empty()));

        let tune_report = parse_tune_report_from_document(&document_report.value);
        let events = tune_report.value.expect("expected tune").events;
        assert!(matches!(
            events.as_slice(),
            [Event::Note {
                step: 'C',
                octave: 4,
                accidental: None,
                ..
            }]
        ));
    }

    #[test]
    fn preserves_explicit_accidentals_in_semantic_events() {
        let (events, diagnostics) = events_for("X:1\nL:1/8\nK:C\n^C _D =E ^^F __G\n");

        assert!(diagnostics.is_empty());
        let accidentals = events
            .iter()
            .filter_map(|event| match event {
                Event::Note { accidental, .. } => Some(*accidental),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            accidentals,
            vec![
                Some(Accidental::Sharp),
                Some(Accidental::Flat),
                Some(Accidental::Natural),
                Some(Accidental::DoubleSharp),
                Some(Accidental::DoubleFlat),
            ]
        );
    }

    #[test]
    fn recovers_dangling_accidentals_without_leaking_into_later_notes() {
        let document_report = parse_document("X:1\nL:1/8\nK:C\n^ _ = C\n", ParseOptions::default());
        assert_eq!(
            count_diagnostics(
                &document_report.diagnostics,
                "abc.music.malformed_accidental"
            ),
            3
        );

        let tune_report = parse_tune_report_from_document(&document_report.value);
        let events = tune_report.value.expect("expected tune").events;
        assert!(matches!(
            events.as_slice(),
            [Event::Note {
                step: 'C',
                accidental: None,
                ..
            }]
        ));
    }

    #[test]
    fn lowers_fractional_lengths_and_slash_shorthand() {
        let document = parse_document(
            "X:1\nL:1/8\nK:C\nA2 A/ A// A3/2 A/4\n",
            ParseOptions::default(),
        )
        .value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");

        assert!(report.diagnostics.is_empty());
        assert_eq!(tune.divisions, 8);
        let durations = tune
            .events
            .iter()
            .filter_map(|event| match event {
                Event::Note { duration, .. } => Some(*duration),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(durations, vec![8, 2, 1, 6, 1]);
    }

    #[test]
    fn recovers_malformed_lengths_and_preserves_valid_neighbors() {
        let document_report =
            parse_document("X:1\nL:1/8\nK:C\nA0 B/0 C 3 / D\n", ParseOptions::default());
        assert_eq!(
            count_diagnostics(&document_report.diagnostics, "abc.music.malformed_length"),
            4
        );

        let tune_music = document_report
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");
        let malformed_spans = tune_music.lines[0]
            .items
            .iter()
            .filter_map(|item| match item {
                MusicItem::Malformed(item) => Some(item.span),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(malformed_spans.len(), 2);
        assert!(
            malformed_spans
                .iter()
                .all(|span| document_report.value.source.slice(*span).is_some())
        );

        let tune_report = parse_tune_report_from_document(&document_report.value);
        let tune = tune_report.value.expect("expected tune");
        let durations = tune
            .events
            .iter()
            .filter_map(|event| match event {
                Event::Note { duration, .. } => Some(*duration),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(durations, vec![4, 4, 4, 4]);
    }

    #[test]
    fn lowers_multi_measure_rests_in_known_and_free_meter() {
        let (known_events, known_diagnostics) = events_for("X:1\nM:2/4\nL:1/8\nK:C\nZ2 X\n");
        assert!(known_diagnostics.is_empty());
        let known_durations = known_events
            .iter()
            .filter_map(|event| match event {
                Event::Rest {
                    duration,
                    visibility,
                    ..
                } => Some((*duration, *visibility)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            known_durations,
            vec![
                (32, RestVisibility::Visible),
                (16, RestVisibility::Invisible),
            ]
        );

        let document =
            parse_document("X:1\nM:none\nL:1/8\nK:C\nZ3\n", ParseOptions::default()).value;
        let report = parse_tune_report_from_document(&document);
        let tune = report.value.expect("expected tune");
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.multirest.free_meter")
        );
        assert_eq!(tune.events.len(), 1);
        assert!(matches!(tune.events[0], Event::Rest { duration: 12, .. }));
    }

    #[test]
    fn lowers_visible_invisible_rests_and_spacers() {
        let (events, diagnostics) = events_for("X:1\nL:1/8\nK:C\nz x y C\n");

        assert!(diagnostics.is_empty());
        let rests = events
            .iter()
            .filter_map(|event| match event {
                Event::Rest {
                    visibility,
                    duration,
                    ..
                } => Some((*visibility, *duration)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            rests,
            vec![(RestVisibility::Visible, 4), (RestVisibility::Invisible, 4),]
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, Event::Spacer { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn malformed_rest_lengths_recover_to_safe_durations() {
        let document_report = parse_document("X:1\nL:1/8\nK:C\nz0 x/0\n", ParseOptions::default());
        assert_eq!(
            count_diagnostics(&document_report.diagnostics, "abc.music.malformed_length"),
            2
        );

        let tune_report = parse_tune_report_from_document(&document_report.value);
        let rests = tune_report
            .value
            .expect("expected tune")
            .events
            .into_iter()
            .filter_map(|event| match event {
                Event::Rest {
                    visibility,
                    duration,
                    ..
                } => Some((visibility, duration)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            rests,
            vec![(RestVisibility::Visible, 4), (RestVisibility::Invisible, 4),]
        );
    }

    #[test]
    fn lowers_basic_double_and_repeat_barlines() {
        let (events, diagnostics) = events_for("X:1\nK:C\nC|D||E|:F:|G::A[|B|]c\n");

        assert!(diagnostics.is_empty());
        let barlines = events
            .iter()
            .filter_map(|event| match event {
                Event::Barline { kind, .. } => Some(*kind),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            barlines,
            vec![
                BarlineKind::Regular,
                BarlineKind::Double,
                BarlineKind::RepeatStart,
                BarlineKind::RepeatEnd,
                BarlineKind::RepeatBoth,
                BarlineKind::Initial,
                BarlineKind::Final,
            ]
        );
    }

    #[test]
    fn recovers_invalid_barline_fragments_as_skipped_malformed_items() {
        let document_report = parse_document("X:1\nK:C\nC : D\n", ParseOptions::default());
        assert_eq!(
            count_diagnostics(&document_report.diagnostics, "abc.music.invalid_barline"),
            1
        );

        let tune_music = document_report
            .value
            .music
            .tune(0)
            .expect("expected parsed tune music");
        assert!(tune_music.lines[0].items.iter().any(|item| matches!(
            item,
            MusicItem::Malformed(MalformedSyntax {
                kind: MalformedSyntaxKind::InvalidBarline,
                ..
            })
        )));

        let tune_report = parse_tune_report_from_document(&document_report.value);
        let notes = tune_report
            .value
            .expect("expected tune")
            .events
            .into_iter()
            .filter(|event| matches!(event, Event::Note { .. }))
            .count();
        assert_eq!(notes, 2);
    }

    #[test]
    fn parses_liberal_dotted_and_invisible_barlines_with_diagnostics() {
        let report = parse_document("X:1\nK:C\nC |[| D .| E [|] F\n", ParseOptions::default());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.barline.liberal")
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.barline.policy")
        );

        let tune_report = parse_tune_report_from_document(&report.value);
        assert!(
            tune_report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.musicxml.barline_policy")
        );
    }

    #[test]
    fn unclosed_inline_fields_groups_and_strings_are_recoverable_syntax() {
        let document_report = parse_document(
            "X:1\nK:C\nC [M:3/4\nD {ef\nE \"Am\nF [CE\nG\n",
            ParseOptions::default(),
        );

        for code in [
            "abc.music.unclosed_inline_field",
            "abc.music.unclosed_grace",
            "abc.music.unclosed_quoted_text",
            "abc.music.unclosed_chord",
        ] {
            assert!(
                document_report
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == code),
                "expected diagnostic {code}"
            );
        }

        let tune_report = parse_tune_report_from_document(&document_report.value);
        let notes = tune_report
            .value
            .expect("expected tune")
            .events
            .into_iter()
            .filter_map(|event| match event {
                Event::Note { step, .. } => Some(step),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(notes, vec!['C', 'D', 'E', 'F', 'G']);
    }

    #[test]
    fn unsupported_attachment_constructs_warn_and_do_not_create_extra_notes() {
        let document_report = parse_document(
            "X:1\nK:C\n\"Am\" !trill! {g} (3 A>B-C [1 D # E\n",
            ParseOptions::default(),
        );

        for code in [
            "abc.music.unsupported_quoted_text",
            "abc.music.unsupported_decoration",
            "abc.music.unsupported_grace",
            "abc.music.unsupported_tuplet",
            "abc.music.unsupported_broken_rhythm",
            "abc.music.unsupported_tie",
            "abc.music.unsupported_repeat_ending",
            "abc.music.reserved",
        ] {
            assert!(
                document_report
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == code),
                "expected diagnostic {code}"
            );
        }

        let tune_report = parse_tune_report_from_document(&document_report.value);
        let notes = tune_report
            .value
            .expect("expected tune")
            .events
            .into_iter()
            .filter(|event| matches!(event, Event::Note { .. }))
            .count();
        assert_eq!(notes, 5);
    }

    #[test]
    fn non_music_lines_and_unsupported_groups_do_not_create_notes() {
        let document_report = parse_document(
            "X:1\nT:ABC\n+:DEF\nK:C\n%%text GAB\n[CDE] C % FED\n",
            ParseOptions::default(),
        );
        let report = parse_tune_report_from_document(&document_report.value);
        let events = report.value.expect("expected tune").events;

        assert!(
            document_report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "abc.music.unsupported_chord")
        );
        let notes = events
            .iter()
            .filter(|event| matches!(event, Event::Note { .. }))
            .count();
        assert_eq!(notes, 1);
    }
}
