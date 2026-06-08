//! Note, rest, accidental, octave, length, chord, and grace parsing.

use crate::diagnostic::Span;
use crate::model::{Accidental, Fraction, RestVisibility};
use crate::music::invalid_tuplet_warning;
use crate::parse::music::{MusicLineParser, invalid_length_warning, is_note_letter};
use crate::syntax::{
    AccidentalSyntax, ChordMemberSyntax, ChordSyntax, GraceElementSyntax, GraceGroupSyntax,
    LengthSyntax, MalformedSyntax, MalformedSyntaxKind, MultiMeasureRestSyntax, MusicItem,
    MusicTokenKind, NoteSyntax, OctaveMark, OctaveMarkSyntax, PitchSyntax, RestSyntax,
    BrokenRhythmDirection, BrokenRhythmSyntax, SpannedNumber, TupletSyntax,
};

impl<'line> MusicLineParser<'line> {
    pub(super) fn parse_note(&mut self, accidental: Option<AccidentalSyntax>) {
        let attachments = self.take_pending_attachments();
        let core_start = accidental
            .as_ref()
            .map(|accidental| accidental.span.start - self.line_offset)
            .unwrap_or(self.index);
        let start = attachments
            .span_start()
            .map(|start| start - self.line_offset)
            .unwrap_or(core_start)
            .min(core_start);
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
            attachments,
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

    pub(super) fn parse_accidental_token(&mut self) -> Option<AccidentalSyntax> {
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

    pub(super) fn parse_octave_marks(&mut self) -> Vec<OctaveMarkSyntax> {
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

    pub(super) fn parse_rest(&mut self, visibility: RestVisibility) {
        let attachments = self.take_pending_attachments();
        let start = self.index;
        self.bump_char();
        let marker_span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Rest, marker_span);
        let length = self.parse_length_suffix();
        let span = length
            .as_ref()
            .map(|length| {
                Span::new(
                    attachments.span_start().unwrap_or(marker_span.start),
                    length.span.end,
                )
            })
            .unwrap_or_else(|| {
                Span::new(
                    attachments.span_start().unwrap_or(marker_span.start),
                    marker_span.end,
                )
            });
        self.items.push(MusicItem::Rest(RestSyntax {
            span,
            attachments,
            visibility,
            marker_span,
            length,
        }));
    }

    pub(super) fn parse_multi_measure_rest(&mut self, visibility: RestVisibility) {
        self.flush_pending_attachments();
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

    pub(super) fn parse_chord(&mut self) {
        let attachments = self.take_pending_attachments();
        let start = attachments
            .span_start()
            .map(|start| start - self.line_offset)
            .unwrap_or(self.index);
        let open_start = self.index;
        self.bump_char();
        let open_span = self.span(open_start, self.index);
        let mut members = Vec::new();
        let mut closed = false;

        while let Some(ch) = self.peek_char() {
            match ch {
                ']' => {
                    self.bump_char();
                    closed = true;
                    break;
                }
                ch if ch.is_whitespace() => self.parse_whitespace(),
                '^' | '_' | '=' => {
                    let Some(accidental) = self.parse_accidental_token() else {
                        continue;
                    };
                    if self.peek_char().is_some_and(is_note_letter) {
                        if let Some(note) = self.parse_note_syntax(Some(accidental)) {
                            members.push(ChordMemberSyntax {
                                span: note.span,
                                note,
                            });
                        }
                    } else {
                        self.push_malformed(
                            accidental.span,
                            MalformedSyntaxKind::DanglingAccidental,
                            "abc.music.malformed_accidental",
                            "Accidentals must appear immediately before a chord member note",
                        );
                    }
                }
                'A'..='G' | 'a'..='g' => {
                    if let Some(note) = self.parse_note_syntax(None) {
                        members.push(ChordMemberSyntax {
                            span: note.span,
                            note,
                        });
                    }
                }
                '"' => self.parse_quoted_text(),
                '{' => self.parse_grace_group(),
                '!' | '+' => self.parse_decoration(ch),
                '.' => self.parse_dot(),
                '~' | 'H' | 'L' | 'M' | 'O' | 'P' | 'S' | 'T' | 'u' | 'v' => {
                    self.parse_shorthand_decoration()
                }
                ch if self.is_user_symbol(ch) => self.parse_shorthand_decoration(),
                _ => {
                    self.parse_malformed_single(
                        MalformedSyntaxKind::UnknownToken,
                        "abc.music.unknown_chord_token",
                        "Unknown chord-member token was preserved and skipped",
                    );
                }
            }
        }

        let close_span = closed.then(|| self.span(self.index - 1, self.index));
        if !closed {
            let span = self.span(start, self.index);
            self.push_token(MusicTokenKind::Malformed, span);
            self.push_malformed(
                span,
                MalformedSyntaxKind::UnclosedChord,
                "abc.music.unclosed_chord",
                "Chord group was preserved and skipped",
            );
            return;
        }

        let length = self.parse_length_suffix();
        let end = length
            .as_ref()
            .map(|length| length.span.end - self.line_offset)
            .unwrap_or(self.index);
        let span = self.span(start, end);
        self.push_token(MusicTokenKind::Chord, span);
        self.items.push(MusicItem::Chord(ChordSyntax {
            span,
            attachments,
            open_span,
            close_span,
            members,
            length,
        }));
    }

    pub(super) fn parse_grace_group(&mut self) {
        let start = self.index;
        self.bump_char();
        let slash_span = if self.peek_char() == Some('/') {
            let slash_start = self.index;
            self.bump_char();
            Some(self.span(slash_start, self.index))
        } else {
            None
        };
        let mut elements = Vec::new();
        let mut closed = false;

        while let Some(ch) = self.peek_char() {
            match ch {
                '}' => {
                    self.bump_char();
                    closed = true;
                    break;
                }
                ch if ch.is_whitespace() => self.parse_whitespace(),
                '^' | '_' | '=' => {
                    let Some(accidental) = self.parse_accidental_token() else {
                        continue;
                    };
                    if self.peek_char().is_some_and(is_note_letter) {
                        if let Some(note) = self.parse_note_syntax(Some(accidental)) {
                            elements.push(GraceElementSyntax::Note(note));
                        }
                    } else {
                        let malformed = MalformedSyntax {
                            span: accidental.span,
                            kind: MalformedSyntaxKind::DanglingAccidental,
                        };
                        elements.push(GraceElementSyntax::Malformed(malformed.clone()));
                        self.push_malformed(
                            malformed.span,
                            malformed.kind,
                            "abc.music.malformed_accidental",
                            "Accidentals must appear immediately before a grace note",
                        );
                    }
                }
                'A'..='G' | 'a'..='g' => {
                    if let Some(note) = self.parse_note_syntax(None) {
                        elements.push(GraceElementSyntax::Note(note));
                    }
                }
                'z' => {
                    let rest = self.parse_rest_syntax(RestVisibility::Visible);
                    elements.push(GraceElementSyntax::Rest(rest));
                }
                'x' => {
                    let rest = self.parse_rest_syntax(RestVisibility::Invisible);
                    elements.push(GraceElementSyntax::Rest(rest));
                }
                '[' => {
                    self.parse_chord();
                    if let Some(MusicItem::Chord(chord)) = self.items.pop() {
                        elements.push(GraceElementSyntax::Chord(chord));
                    }
                }
                '"' => self.parse_quoted_text(),
                '!' | '+' => self.parse_decoration(ch),
                '.' => self.parse_dot(),
                '~' | 'H' | 'L' | 'M' | 'O' | 'P' | 'S' | 'T' | 'u' | 'v' => {
                    self.parse_shorthand_decoration()
                }
                ch if self.is_user_symbol(ch) => self.parse_shorthand_decoration(),
                _ => {
                    let start = self.index;
                    self.bump_char();
                    let malformed = MalformedSyntax {
                        span: self.span(start, self.index),
                        kind: MalformedSyntaxKind::UnknownToken,
                    };
                    elements.push(GraceElementSyntax::Malformed(malformed.clone()));
                    self.push_malformed(
                        malformed.span,
                        malformed.kind,
                        "abc.music.unknown_grace_token",
                        "Unknown grace-group token was preserved and skipped",
                    );
                }
            }
        }

        let span = self.span(start, self.index);
        if closed {
            self.push_token(MusicTokenKind::GraceGroup, span);
            self.pending_attachments
                .grace_groups
                .push(GraceGroupSyntax {
                    span,
                    slash_span,
                    elements,
                });
        } else {
            self.push_token(MusicTokenKind::Malformed, span);
            self.push_malformed(
                span,
                MalformedSyntaxKind::UnclosedGraceGroup,
                "abc.music.unclosed_grace",
                "Grace group was preserved and skipped",
            );
        }
    }

    pub(super) fn parse_note_syntax(&mut self, accidental: Option<AccidentalSyntax>) -> Option<NoteSyntax> {
        let attachments = self.take_pending_attachments();
        let core_start = accidental
            .as_ref()
            .map(|accidental| accidental.span.start - self.line_offset)
            .unwrap_or(self.index);
        let start = attachments
            .span_start()
            .map(|start| start - self.line_offset)
            .unwrap_or(core_start)
            .min(core_start);
        let pitch_start = self.index;
        let step = self.bump_char()?;
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
        Some(NoteSyntax {
            span: self.span(start, end),
            attachments,
            accidental,
            pitch: PitchSyntax {
                step,
                span: pitch_span,
            },
            octave_marks,
            length,
        })
    }

    pub(super) fn parse_rest_syntax(&mut self, visibility: RestVisibility) -> RestSyntax {
        let attachments = self.take_pending_attachments();
        let start = self.index;
        self.bump_char();
        let marker_span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Rest, marker_span);
        let length = self.parse_length_suffix();
        let span = length
            .as_ref()
            .map(|length| {
                Span::new(
                    attachments.span_start().unwrap_or(marker_span.start),
                    length.span.end,
                )
            })
            .unwrap_or_else(|| {
                Span::new(
                    attachments.span_start().unwrap_or(marker_span.start),
                    marker_span.end,
                )
            });
        RestSyntax {
            span,
            attachments,
            visibility,
            marker_span,
            length,
        }
    }

    pub(super) fn parse_length_suffix(&mut self) -> Option<LengthSyntax> {
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

    pub(super) fn parse_number_token(&mut self) -> Option<SpannedNumber> {
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

    pub(super) fn parse_broken_rhythm(&mut self) {
        let start = self.index;
        let Some(marker) = self.bump_char() else {
            return;
        };
        while self.peek_char() == Some(marker) {
            self.bump_char();
        }
        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::BrokenRhythm, span);
        self.items.push(MusicItem::BrokenRhythm(BrokenRhythmSyntax {
            span,
            direction: if marker == '<' {
                BrokenRhythmDirection::LeftShorter
            } else {
                BrokenRhythmDirection::RightShorter
            },
            count: u8::try_from(self.index - start).unwrap_or(u8::MAX),
        }));
    }

    pub(super) fn parse_tuplet(&mut self) {
        self.flush_pending_attachments();
        let start = self.index;
        self.bump_char();
        let Some(p) = self.parse_number_token() else {
            let span = self.span(start, self.index);
            self.push_token(MusicTokenKind::Malformed, span);
            self.push_malformed(
                span,
                MalformedSyntaxKind::InvalidTuplet,
                "abc.music.invalid_tuplet",
                "Tuplet specifier must start with a number",
            );
            return;
        };

        let mut q = None;
        let mut r = None;
        if self.peek_char() == Some(':') {
            self.bump_char();
            q = self.parse_number_token();
            if self.peek_char() == Some(':') {
                self.bump_char();
                r = self.parse_number_token();
            }
        }

        let span = self.span(start, self.index);
        self.push_token(MusicTokenKind::Tuplet, span);
        if !(2..=9).contains(&p.value) {
            self.diagnostics.push(invalid_tuplet_warning(span));
        }
        self.items
            .push(MusicItem::Tuplet(TupletSyntax { span, p, q, r }));
    }
}

fn slash_denominator(slash_count: u8) -> Option<u32> {
    1u32.checked_shl(u32::from(slash_count))
}
