//! Lyric and symbol line alignment onto voice timelines.

use crate::diagnostic::Diagnostic;
use crate::model::{AlignedLyric, AlignedSymbol, AlignedSymbolKind, LyricControl, VoiceTimeline};
use crate::syntax::{LyricLineSyntax, LyricTokenKind, SymbolLineSyntax, SymbolTokenKind};

use crate::lower::{
    VoicedLyricLine, VoicedSymbolLine, lyric_syllable_count_warning, symbol_count_warning,
};

#[derive(Debug, Clone, Copy)]
struct AlignableRef {
    measure_index: usize,
    event_index: usize,
    line_index: usize,
    source_order: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BarMarkerCursor {
    position: usize,
    measure_index: usize,
}

pub(crate) fn align_lyrics(
    voices: &mut [VoiceTimeline],
    lyric_lines: &[VoicedLyricLine],
    diagnostics: &mut Vec<Diagnostic>,
) {
    for voice in voices {
        let voice_lines = lyric_lines
            .iter()
            .filter(|line| line.voice_id == voice.id.value)
            .map(|line| &line.line)
            .collect::<Vec<_>>();
        align_lyrics_for_voice(voice, &voice_lines, diagnostics);
    }
}

fn align_lyrics_for_voice(
    voice: &mut VoiceTimeline,
    lyric_lines: &[&LyricLineSyntax],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let refs = alignable_refs(voice);
    let mut cursor = 0usize;
    let mut block_start = 0usize;
    let mut block_available_end = 0usize;
    let mut previous_line = None;
    let mut verse = 1u32;

    for line in lyric_lines {
        let available_end = refs
            .iter()
            .take_while(|reference| reference.line_index < line.line_index)
            .count();
        let adjacent = previous_line.is_some_and(|previous| previous + 1 == line.line_index);
        let (start, end, line_verse) = if adjacent {
            verse = verse.saturating_add(1);
            (block_start, block_available_end, verse)
        } else {
            verse = 1;
            block_start = cursor;
            block_available_end = available_end;
            cursor = available_end;
            (block_start, block_available_end, verse)
        };

        align_lyric_line(voice, &refs, start, end, line_verse, line, diagnostics);
        previous_line = Some(line.line_index);
    }
}

fn align_lyric_line(
    voice: &mut VoiceTimeline,
    refs: &[AlignableRef],
    start: usize,
    end: usize,
    verse: u32,
    line: &LyricLineSyntax,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut position = start;
    let mut last_bar_consume: Option<BarMarkerCursor> = None;
    let block_measure = block_start_measure(refs, start, end);
    for token in &line.tokens {
        match token.kind {
            LyricTokenKind::Syllable => {
                if let Some(reference) = refs.get(position).copied().filter(|_| position < end) {
                    attach_lyric(
                        voice,
                        reference,
                        AlignedLyric {
                            verse,
                            text: token.text.clone(),
                            span: token.span,
                            control: LyricControl::Syllable,
                        },
                    );
                    position += 1;
                } else {
                    diagnostics.push(lyric_syllable_count_warning(token.span));
                }
            }
            LyricTokenKind::Hyphen => {
                if position > start
                    && let Some(reference) = refs.get(position - 1).copied()
                {
                    attach_lyric(
                        voice,
                        reference,
                        AlignedLyric {
                            verse,
                            text: token.text.clone(),
                            span: token.span,
                            control: LyricControl::Hyphen,
                        },
                    );
                }
            }
            LyricTokenKind::Extender => {
                if let Some(reference) = refs.get(position).copied().filter(|_| position < end) {
                    attach_lyric(
                        voice,
                        reference,
                        AlignedLyric {
                            verse,
                            text: String::new(),
                            span: token.span,
                            control: LyricControl::Extender,
                        },
                    );
                    position += 1;
                }
            }
            LyricTokenKind::Skip => {
                position = position.saturating_add(1).min(end);
            }
            LyricTokenKind::Bar => {
                position = advance_bar_marker(
                    refs,
                    start,
                    end,
                    position,
                    block_measure,
                    &mut last_bar_consume,
                );
            }
        }
    }
    if position < end
        && line
            .tokens
            .iter()
            .any(|token| matches!(token.kind, LyricTokenKind::Syllable))
    {
        diagnostics.push(lyric_syllable_count_warning(line.value.span));
    }
}

pub(crate) fn align_symbols(
    voices: &mut [VoiceTimeline],
    symbol_lines: &[VoicedSymbolLine],
    diagnostics: &mut Vec<Diagnostic>,
) {
    for voice in voices {
        let voice_lines = symbol_lines
            .iter()
            .filter(|line| line.voice_id == voice.id.value)
            .map(|line| &line.line)
            .collect::<Vec<_>>();
        align_symbols_for_voice(voice, &voice_lines, diagnostics);
    }
}

fn align_symbols_for_voice(
    voice: &mut VoiceTimeline,
    symbol_lines: &[&SymbolLineSyntax],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let refs = alignable_refs(voice);
    let mut cursor = 0usize;
    let mut block_start = 0usize;
    let mut block_available_end = 0usize;
    let mut previous_line = None;
    let mut layer = 1u32;

    for line in symbol_lines {
        let available_end = refs
            .iter()
            .take_while(|reference| reference.line_index < line.line_index)
            .count();
        let adjacent = previous_line.is_some_and(|previous| previous + 1 == line.line_index);
        let (start, end, line_layer) = if adjacent {
            layer = layer.saturating_add(1);
            (block_start, block_available_end, layer)
        } else {
            layer = 1;
            block_start = cursor;
            block_available_end = available_end;
            cursor = available_end;
            (block_start, block_available_end, layer)
        };
        align_symbol_line(voice, &refs, start, end, line_layer, line, diagnostics);
        previous_line = Some(line.line_index);
    }
}

fn align_symbol_line(
    voice: &mut VoiceTimeline,
    refs: &[AlignableRef],
    start: usize,
    end: usize,
    layer: u32,
    line: &SymbolLineSyntax,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut position = start;
    let mut last_bar_consume: Option<BarMarkerCursor> = None;
    let block_measure = block_start_measure(refs, start, end);
    for token in &line.tokens {
        match token.kind {
            SymbolTokenKind::Skip => {
                position = position.saturating_add(1).min(end);
            }
            SymbolTokenKind::Bar => {
                position = advance_bar_marker(
                    refs,
                    start,
                    end,
                    position,
                    block_measure,
                    &mut last_bar_consume,
                );
            }
            SymbolTokenKind::Decoration
            | SymbolTokenKind::ChordSymbol
            | SymbolTokenKind::Annotation
            | SymbolTokenKind::Raw => {
                if let Some(reference) = refs.get(position).copied().filter(|_| position < end) {
                    attach_symbol(
                        voice,
                        reference,
                        AlignedSymbol {
                            layer,
                            text: token.text.clone(),
                            span: token.span,
                            kind: match token.kind {
                                SymbolTokenKind::Decoration => AlignedSymbolKind::Decoration,
                                SymbolTokenKind::ChordSymbol => AlignedSymbolKind::ChordSymbol,
                                SymbolTokenKind::Annotation => AlignedSymbolKind::Annotation,
                                SymbolTokenKind::Raw => AlignedSymbolKind::Raw,
                                SymbolTokenKind::Skip | SymbolTokenKind::Bar => {
                                    AlignedSymbolKind::Raw
                                }
                            },
                        },
                    );
                    position += 1;
                } else {
                    diagnostics.push(symbol_count_warning(token.span));
                }
            }
        }
    }
}

fn alignable_refs(voice: &VoiceTimeline) -> Vec<AlignableRef> {
    let mut refs = Vec::new();
    for (measure_index, measure) in voice.measures.iter().enumerate() {
        for (event_index, event) in measure.events.iter().enumerate() {
            if event.alignable {
                refs.push(AlignableRef {
                    measure_index,
                    event_index,
                    line_index: event.line_index,
                    source_order: event.source_order,
                });
            }
        }
    }
    refs.sort_by_key(|reference| reference.source_order);
    refs
}

fn block_start_measure(refs: &[AlignableRef], start: usize, end: usize) -> usize {
    if start == 0 {
        return 0;
    }
    let after_previous = refs
        .get(start - 1)
        .map(|reference| reference.measure_index.saturating_add(1))
        .unwrap_or(0);
    let first_visible = refs
        .get(start)
        .filter(|_| start < end)
        .map(|reference| reference.measure_index)
        .unwrap_or(after_previous);
    after_previous.min(first_visible)
}

/// Resolve a `|` bar marker inside a `w:`/`s:` alignment line per ABC 2.1
/// section 5.1: a bar marker "advances to the next bar", i.e. any remaining
/// notes of the current measure are skipped (they receive blank syllables).
/// Rest-only measures still count as bars even though they have no alignable
/// refs, so the cursor tracks the concrete measure reached by prior markers
/// instead of inferring all movement from gaps between note refs.
///
/// `start` is the first alignable index of the current verse/layer block and
/// `last_consume` records which measure the previous marker reached at a given
/// alignable position, so consecutive markers (e.g. a leading `||`) each
/// advance one further bar instead of collapsing into a single no-op.
fn advance_bar_marker(
    refs: &[AlignableRef],
    start: usize,
    end: usize,
    position: usize,
    block_measure: usize,
    last_consume: &mut Option<BarMarkerCursor>,
) -> usize {
    if refs.is_empty() || start >= end {
        return position;
    };

    let current_measure =
        if let Some(cursor) = last_consume.filter(|cursor| cursor.position == position) {
            cursor.measure_index
        } else if position > start {
            refs.get(position - 1)
                .map(|reference| reference.measure_index)
                .unwrap_or(0)
        } else if position == start {
            block_measure
        } else {
            refs.get(start)
                .map(|reference| reference.measure_index.saturating_sub(1))
                .unwrap_or(0)
        };
    let target_measure = current_measure.saturating_add(1);
    let mut next = position.min(end);
    while next < end && refs[next].measure_index < target_measure {
        next += 1;
    }
    *last_consume = Some(BarMarkerCursor {
        position: next,
        measure_index: target_measure,
    });
    next
}

fn attach_lyric(voice: &mut VoiceTimeline, reference: AlignableRef, lyric: AlignedLyric) {
    if let Some(event) = voice
        .measures
        .get_mut(reference.measure_index)
        .and_then(|measure| measure.events.get_mut(reference.event_index))
    {
        event.attachments.lyrics.push(lyric.clone());
        event.lyrics.push(lyric);
    }
}

fn attach_symbol(voice: &mut VoiceTimeline, reference: AlignableRef, symbol: AlignedSymbol) {
    if let Some(event) = voice
        .measures
        .get_mut(reference.measure_index)
        .and_then(|measure| measure.events.get_mut(reference.event_index))
    {
        event.attachments.symbols.push(symbol.clone());
        event.symbols.push(symbol);
    }
}
