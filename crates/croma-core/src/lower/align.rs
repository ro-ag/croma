//! Lyric and symbol line alignment onto voice timelines.

use crate::diagnostic::Diagnostic;
use crate::model::{AlignedLyric, AlignedSymbol, AlignedSymbolKind, LyricControl, VoiceTimeline};
use crate::syntax::{
    LyricLineSyntax, LyricTokenKind, LyricTokenSyntax, SymbolLineSyntax, SymbolTokenKind,
};

use crate::lower::{
    VoicedLyricLine, VoicedSymbolLine, lyric_syllable_count_warning, symbol_count_warning,
};

#[derive(Debug, Clone, Copy)]
struct AlignableRef {
    measure_index: usize,
    event: AlignableEventRef,
    line_index: usize,
    source_order: u32,
}

#[derive(Debug, Clone, Copy)]
enum AlignableEventRef {
    Main(usize),
    Overlay {
        overlay_index: usize,
        event_index: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BarMarkerCursor {
    position: usize,
    measure_index: usize,
}

#[derive(Debug, Clone, Copy)]
struct LyricLineContext<'a> {
    line: &'a LyricLineSyntax,
    start: usize,
    end: usize,
    verse: u32,
    block_measure: usize,
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
    let contexts = lyric_line_contexts(&refs, lyric_lines);

    for (index, context) in contexts.iter().copied().enumerate() {
        align_lyric_line(voice, &refs, context, &contexts[index + 1..], diagnostics);
    }
}

fn lyric_line_contexts<'a>(
    refs: &[AlignableRef],
    lyric_lines: &[&'a LyricLineSyntax],
) -> Vec<LyricLineContext<'a>> {
    let mut cursor = 0usize;
    let mut block_start = 0usize;
    let mut block_available_end = 0usize;
    let mut previous_line = None;
    let mut verse = 1u32;
    let mut contexts = Vec::new();

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

        contexts.push(LyricLineContext {
            line,
            start,
            end,
            verse: line_verse,
            block_measure: block_start_measure(refs, start, end),
        });
        previous_line = Some(line.line_index);
    }
    contexts
}

fn align_lyric_line(
    voice: &mut VoiceTimeline,
    refs: &[AlignableRef],
    context: LyricLineContext<'_>,
    future_contexts: &[LyricLineContext<'_>],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let LyricLineContext {
        line,
        start,
        end,
        verse,
        block_measure,
    } = context;
    let mut position = start;
    let mut last_bar_consume: Option<BarMarkerCursor> = None;
    for (token_index, token) in line.tokens.iter().enumerate() {
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
                            same_note_extend: false,
                        },
                    );
                    position += 1;
                } else {
                    diagnostics.push(lyric_syllable_count_warning(token.span));
                }
            }
            LyricTokenKind::Hyphen => {
                let future_tokens = &line.tokens[token_index + 1..];
                if position > start
                    && (trailing_word_hyphen(future_tokens)
                        || future_syllable_can_attach(
                            refs,
                            start,
                            end,
                            position,
                            block_measure,
                            last_bar_consume,
                            future_tokens,
                        )
                        || future_same_verse_syllable_can_attach(refs, verse, future_contexts))
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
                            same_note_extend: false,
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
                            same_note_extend: false,
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

fn trailing_word_hyphen(tokens: &[LyricTokenSyntax]) -> bool {
    !tokens
        .iter()
        .any(|token| matches!(token.kind, LyricTokenKind::Syllable))
}

fn future_same_verse_syllable_can_attach(
    refs: &[AlignableRef],
    verse: u32,
    contexts: &[LyricLineContext<'_>],
) -> bool {
    contexts
        .iter()
        .filter(|context| context.verse == verse)
        .any(|context| {
            future_syllable_can_attach(
                refs,
                context.start,
                context.end,
                context.start,
                context.block_measure,
                None,
                &context.line.tokens,
            )
        })
}

fn future_syllable_can_attach(
    refs: &[AlignableRef],
    start: usize,
    end: usize,
    mut position: usize,
    block_measure: usize,
    mut last_bar_consume: Option<BarMarkerCursor>,
    tokens: &[LyricTokenSyntax],
) -> bool {
    for token in tokens {
        match token.kind {
            LyricTokenKind::Syllable => return position < end && refs.get(position).is_some(),
            LyricTokenKind::Extender | LyricTokenKind::Skip => {
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
            LyricTokenKind::Hyphen => {}
        }
    }
    false
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
                    event: AlignableEventRef::Main(event_index),
                    line_index: event.line_index,
                    source_order: event.source_order,
                });
            }
        }
        for (overlay_index, overlay) in measure.overlays.iter().enumerate() {
            for (event_index, event) in overlay.events.iter().enumerate() {
                if event.alignable {
                    refs.push(AlignableRef {
                        measure_index,
                        event: AlignableEventRef::Overlay {
                            overlay_index,
                            event_index,
                        },
                        line_index: event.line_index,
                        source_order: event.source_order,
                    });
                }
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

fn attach_lyric(voice: &mut VoiceTimeline, reference: AlignableRef, mut lyric: AlignedLyric) {
    if let Some(event) = alignable_event_mut(voice, reference) {
        let mut duplicates = Vec::new();
        if lyric.control == LyricControl::Syllable
            && let Some(index) = event
                .attachments
                .lyric_same_note_extends
                .iter()
                .position(|verse| *verse == lyric.verse)
        {
            event.attachments.lyric_same_note_extends.remove(index);
            lyric.same_note_extend = true;
        }
        if lyric.control == LyricControl::Syllable {
            let mut index = 0;
            while index < event.attachments.lyric_same_note_duplicates.len() {
                if event.attachments.lyric_same_note_duplicates[index].verse == lyric.verse {
                    duplicates.push(event.attachments.lyric_same_note_duplicates.remove(index));
                } else {
                    index += 1;
                }
            }
        }
        event.attachments.lyrics.push(lyric.clone());
        event.lyrics.push(lyric);
        for duplicate in duplicates {
            event.attachments.lyrics.push(duplicate.clone());
            event.lyrics.push(duplicate);
        }
    }
}

fn attach_symbol(voice: &mut VoiceTimeline, reference: AlignableRef, symbol: AlignedSymbol) {
    if let Some(event) = alignable_event_mut(voice, reference) {
        event.attachments.symbols.push(symbol.clone());
        event.symbols.push(symbol);
    }
}

fn alignable_event_mut(
    voice: &mut VoiceTimeline,
    reference: AlignableRef,
) -> Option<&mut crate::model::VoiceTimedEvent> {
    let measure = voice.measures.get_mut(reference.measure_index)?;
    match reference.event {
        AlignableEventRef::Main(event_index) => measure.events.get_mut(event_index),
        AlignableEventRef::Overlay {
            overlay_index,
            event_index,
        } => measure
            .overlays
            .get_mut(overlay_index)
            .and_then(|overlay| overlay.events.get_mut(event_index)),
    }
}
