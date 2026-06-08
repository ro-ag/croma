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
    measure_number: u32,
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
    let mut last_bar_consume: Option<usize> = None;
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
                position = advance_bar_marker(refs, start, end, position, &mut last_bar_consume);
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
    let mut last_bar_consume: Option<usize> = None;
    for token in &line.tokens {
        match token.kind {
            SymbolTokenKind::Skip => {
                position = position.saturating_add(1).min(end);
            }
            SymbolTokenKind::Bar => {
                position = advance_bar_marker(refs, start, end, position, &mut last_bar_consume);
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
                    measure_number: measure.index,
                });
            }
        }
    }
    refs.sort_by_key(|reference| reference.source_order);
    refs
}

/// Resolve a `|` bar marker inside a `w:`/`s:` alignment line per ABC 2.1
/// section 5.1: a bar marker "advances to the next bar", i.e. any remaining
/// notes of the current measure are skipped (they receive blank syllables).
/// The marker is ignored only when the current measure has already been filled
/// — that is, the cursor sits exactly at a real barline that has not yet been
/// absorbed by an earlier marker on this line.
///
/// `start` is the first alignable index of the current verse/layer block and
/// `last_consume` records where the previous marker on this line was absorbed,
/// so consecutive markers (e.g. a leading `||`) each advance one further bar
/// instead of collapsing into a single no-op.
fn advance_bar_marker(
    refs: &[AlignableRef],
    start: usize,
    end: usize,
    position: usize,
    last_consume: &mut Option<usize>,
) -> usize {
    let at_unconsumed_barline = position > start
        && position < end
        && refs[position - 1].measure_number != refs[position].measure_number
        && *last_consume != Some(position);
    if at_unconsumed_barline {
        // The measure is already full and a real barline lines up here, so the
        // marker is a no-op; remember it so a following marker advances past it.
        *last_consume = Some(position);
        return position;
    }
    let Some(current) = refs.get(position).copied().filter(|_| position < end) else {
        return position;
    };
    let measure = current.measure_number;
    let mut next = position;
    while next < end
        && refs
            .get(next)
            .is_some_and(|reference| reference.measure_number == measure)
    {
        next += 1;
    }
    *last_consume = Some(next);
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
