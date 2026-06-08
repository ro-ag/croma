//! Meter (`M:`) and unit-note-length (`L:`) field parsing.

use super::*;
use crate::diagnostic::Span;
use crate::options::{ParseMode, ParseOptions};

pub(crate) fn parse_meter(value: &str) -> Meter {
    let trimmed = value.trim();
    let kind = match trimmed {
        "C" => MeterKind::CommonTime,
        "C|" => MeterKind::CutTime,
        value if value.eq_ignore_ascii_case("none") => MeterKind::None,
        value => parse_simple_meter(value).unwrap_or(MeterKind::Complex),
    };
    Meter {
        raw: trimmed.to_owned(),
        kind,
    }
}

fn parse_simple_meter(value: &str) -> Option<MeterKind> {
    let (numerator, denominator) = value.split_once('/')?;
    Some(MeterKind::Fraction {
        numerator: numerator.trim().parse().ok()?,
        denominator: denominator.trim().parse().ok()?,
    })
}

pub(crate) fn parse_unit_note_length(value: &str) -> Option<UnitNoteLength> {
    let (numerator, denominator) = value.trim().split_once('/')?;
    Some(UnitNoteLength {
        fraction: NoteLengthFraction::new(numerator.parse().ok()?, denominator.parse().ok()?),
        origin: UnitNoteLengthOrigin::Explicit,
    })
}

pub(super) fn default_unit_note_length_for_meter(meter: &Meter) -> NoteLengthFraction {
    match meter.kind {
        MeterKind::CommonTime | MeterKind::CutTime | MeterKind::None | MeterKind::Complex => {
            NoteLengthFraction::new(1, 8)
        }
        MeterKind::Fraction {
            numerator,
            denominator,
        } => {
            if numerator.saturating_mul(4) < denominator.saturating_mul(3) {
                NoteLengthFraction::new(1, 16)
            } else {
                NoteLengthFraction::new(1, 8)
            }
        }
    }
}

pub(super) fn ensure_default_unit_note_length(state: &mut FieldState, span: Span, options: ParseOptions) {
    if state.unit_note_length.is_some() {
        return;
    }

    let (fraction, origin, unit_span) = state
        .meter
        .as_ref()
        .map(|meter| {
            (
                default_unit_note_length_for_meter(&meter.value),
                UnitNoteLengthOrigin::DefaultFromMeter,
                meter.span,
            )
        })
        .unwrap_or((
            NoteLengthFraction::new(1, 8),
            UnitNoteLengthOrigin::DefaultFreeMeter,
            span,
        ));

    let unit = UnitNoteLength { fraction, origin };
    state.unit_note_length = Some(Spanned::new(unit, unit_span));
    if state.dialect.mode == ParseMode::Recover {
        state.dialect.mode = options.mode;
    }
}
