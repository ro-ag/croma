#!/usr/bin/env python3
"""Compare two MusicXML files through music21-derived structural facts."""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from pathlib import Path
from typing import Any


def main() -> int:
    args = parse_args()

    try:
        croma = extract_facts(args.croma_xml, "croma")
    except Music21Unavailable as error:
        return emit({"status": "music21_unavailable", "error": str(error)}, args.json)
    except Music21ParseFailure as error:
        return emit(
            {
                "status": "croma_musicxml_parse_failure",
                "error": str(error),
                "path": str(error.path),
            },
            args.json,
        )
    except Exception as error:  # noqa: BLE001 - tool failures must be reported distinctly.
        return emit({"status": "music21_tool_failure", "error": str(error)}, args.json)

    try:
        reference = extract_facts(args.reference_xml, "reference")
    except Music21Unavailable as error:
        return emit({"status": "music21_unavailable", "error": str(error)}, args.json)
    except Music21ParseFailure as error:
        return emit(
            {
                "status": "reference_musicxml_parse_failure",
                "error": str(error),
                "path": str(error.path),
            },
            args.json,
        )
    except Exception as error:  # noqa: BLE001 - tool failures must be reported distinctly.
        return emit({"status": "music21_tool_failure", "error": str(error)}, args.json)

    differences, category_counts = compare_facts(croma, reference, args.max_differences_per_category)
    status = "match" if not category_counts else "difference"
    return emit(
        {
            "status": status,
            "croma_xml": str(args.croma_xml),
            "reference_xml": str(args.reference_xml),
            "differences": differences,
            "category_counts": dict(sorted(category_counts.items())),
            "classification": "unclassified_difference" if category_counts else None,
        },
        args.json,
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Compare MusicXML with music21 facts")
    parser.add_argument("--croma-xml", type=Path, required=True)
    parser.add_argument("--reference-xml", type=Path, required=True)
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--max-differences-per-category", type=int, default=5)
    return parser.parse_args()


class Music21Unavailable(RuntimeError):
    pass


class Music21ParseFailure(RuntimeError):
    def __init__(self, label: str, path: Path, error: Exception) -> None:
        self.label = label
        self.path = path
        super().__init__(f"music21 failed to parse {label} MusicXML `{path}`: {error}")


def extract_facts(path: Path, label: str) -> dict[str, Any]:
    try:
        from music21 import (
            chord,
            converter,
            dynamics,
            expressions,
            harmony,
            note,
            spanner,
            stream,
            tempo,
        )
    except ImportError as error:
        raise Music21Unavailable(
            "music21 is not installed; install it in the local dev environment to run comparison"
        ) from error

    try:
        score = converter.parse(path)
    except Exception as error:  # noqa: BLE001 - parse failures are data, not harness crashes.
        raise Music21ParseFailure(label, path, error) from error

    parts = []
    score_parts = list(score.parts)
    for part_index, part in enumerate(score_parts):
        measures = []
        for measure_index, measure in enumerate(part.getElementsByClass(stream.Measure)):
            voices = list(measure.getElementsByClass(stream.Voice))
            voice_facts = []
            events = []

            if voices:
                for voice_index, voice in enumerate(voices):
                    voice_id = str(getattr(voice, "id", voice_index))
                    voice_events = [
                        event_facts(element, voice_id, event_index, note, chord)
                        for event_index, element in enumerate(voice.notesAndRests)
                    ]
                    voice_facts.append({"id": voice_id, "events": voice_events})
                    events.extend(voice_events)
            else:
                events = [
                    event_facts(element, None, event_index, note, chord)
                    for event_index, element in enumerate(measure.notesAndRests)
                ]
                voice_facts.append({"id": None, "events": events})

            measures.append(
                {
                    "index": measure_index,
                    "number": str(measure.number),
                    "offset": rational_string(getattr(measure, "offset", None)),
                    "duration": rational_string(getattr(measure.duration, "quarterLength", None)),
                    "bar_duration": rational_string(
                        getattr(getattr(measure, "barDuration", None), "quarterLength", None)
                    ),
                    "voices": [
                        {"id": voice["id"], "event_count": len(voice["events"])}
                        for voice in voice_facts
                    ],
                    "events": events,
                    "barlines": measure_barline_facts(measure),
                }
            )

        parts.append(
            {
                "index": part_index,
                "id": getattr(part, "id", None),
                "name": part.partName,
                "measure_count": len(measures),
                "measures": measures,
            }
        )

    return {
        "parts": parts,
        "part_count": len(parts),
        "slurs": slur_facts(score, spanner),
        "repeat_endings": repeat_ending_facts(score, spanner),
        "harmony": harmony_facts(score, harmony, stream),
        "directions": direction_facts(score, dynamics, expressions, tempo, stream),
    }


def event_facts(
    element: Any,
    voice_id: str | None,
    event_index: int,
    note_module: Any,
    chord_module: Any,
) -> dict[str, Any]:
    base = {
        "index": event_index,
        "offset": rational_string(getattr(element, "offset", None)),
        "voice": voice_id,
        "duration": duration_facts(element),
        "lyrics": lyric_facts(element),
    }
    if isinstance(element, note_module.Note):
        base.update(
            {
                "kind": "note",
                "pitch": pitch_facts(element.pitch),
                "tie": tie_facts(element),
            }
        )
    elif isinstance(element, chord_module.Chord):
        base.update(
            {
                "kind": "chord",
                "pitches": [pitch_facts(pitch) for pitch in element.pitches],
                "tie": tie_facts(element),
            }
        )
    elif isinstance(element, note_module.Rest):
        base.update({"kind": "rest"})
    else:
        base.update({"kind": element.__class__.__name__})
    return base


def pitch_facts(pitch: Any) -> dict[str, Any]:
    return {
        "step": pitch.step,
        "octave": pitch.octave,
        "accidental": accidental_name(pitch.accidental),
    }


def accidental_name(accidental: Any) -> str | None:
    return None if accidental is None else accidental.name


def duration_facts(element: Any) -> dict[str, Any]:
    duration = element.duration
    return {
        "quarter_length": rational_string(duration.quarterLength),
        "type": duration.type,
        "dots": duration.dots,
        "tuplets": [
            {
                "actual": tuplet.numberNotesActual,
                "normal": tuplet.numberNotesNormal,
                "type": tuplet.type,
            }
            for tuplet in duration.tuplets
        ],
    }


def tie_facts(element: Any) -> str | None:
    tie = getattr(element, "tie", None)
    return None if tie is None else tie.type


def lyric_facts(element: Any) -> list[str]:
    return [lyric.text for lyric in getattr(element, "lyrics", []) if lyric.text]


def measure_barline_facts(measure: Any) -> dict[str, Any]:
    return {
        "left": barline_facts(getattr(measure, "leftBarline", None)),
        "right": barline_facts(getattr(measure, "rightBarline", None)),
    }


def barline_facts(barline: Any) -> dict[str, Any] | None:
    if barline is None:
        return None
    return {
        "type": optional_string(getattr(barline, "type", None)),
        "style": optional_string(getattr(barline, "style", None)),
        "direction": optional_string(getattr(barline, "direction", None)),
        "times": optional_string(getattr(barline, "times", None)),
    }


def slur_facts(score: Any, spanner_module: Any) -> list[dict[str, Any]]:
    facts = []
    for index, slur in enumerate(score.recurse().getElementsByClass(spanner_module.Slur)):
        elements = list(slur.getSpannedElements())
        facts.append(
            {
                "index": index,
                "count": len(elements),
                "start": spanned_element_name(elements[0]) if elements else None,
                "end": spanned_element_name(elements[-1]) if elements else None,
            }
        )
    return facts


def repeat_ending_facts(score: Any, spanner_module: Any) -> list[dict[str, Any]]:
    repeat_bracket = getattr(spanner_module, "RepeatBracket", None)
    if repeat_bracket is None:
        return []
    facts = []
    for index, ending in enumerate(score.recurse().getElementsByClass(repeat_bracket)):
        measures = [
            str(getattr(measure, "number", ""))
            for measure in ending.getSpannedElements()
        ]
        facts.append(
            {
                "index": index,
                "number": optional_string(getattr(ending, "number", None)),
                "measures": measures,
            }
        )
    return facts


def harmony_facts(score: Any, harmony_module: Any, stream_module: Any) -> list[dict[str, Any]]:
    facts = []
    for item in score.recurse().getElementsByClass(harmony_module.ChordSymbol):
        facts.append(
            {
                "figure": str(item.figure),
                "measure": context_measure_number(item, stream_module),
                "offset": rational_string(getattr(item, "offset", None)),
            }
        )
    return facts


def direction_facts(
    score: Any,
    dynamics_module: Any,
    expressions_module: Any,
    tempo_module: Any,
    stream_module: Any,
) -> list[dict[str, Any]]:
    direction_types = [
        dynamics_module.Dynamic,
        expressions_module.TextExpression,
    ]
    for class_name in ["MetronomeMark", "TempoText"]:
        direction_type = getattr(tempo_module, class_name, None)
        if direction_type is not None:
            direction_types.append(direction_type)
    facts = []
    for item in score.recurse():
        if isinstance(item, tuple(direction_types)):
            facts.append(
                {
                    "kind": item.__class__.__name__,
                    "text": direction_text(item),
                    "measure": context_measure_number(item, stream_module),
                    "offset": rational_string(getattr(item, "offset", None)),
                }
            )
    return facts


def context_measure_number(item: Any, stream_module: Any) -> str | None:
    measure = item.getContextByClass(stream_module.Measure)
    if measure is None:
        return None
    return str(measure.number)


def direction_text(item: Any) -> str:
    if hasattr(item, "content") and item.content:
        return str(item.content)
    if hasattr(item, "value") and item.value:
        return str(item.value)
    if hasattr(item, "text") and item.text:
        return str(item.text)
    if hasattr(item, "number") and item.number:
        return str(item.number)
    return str(item)


def spanned_element_name(element: Any) -> str:
    if hasattr(element, "pitch"):
        return str(element.pitch)
    if hasattr(element, "pitches"):
        return ".".join(str(pitch) for pitch in element.pitches)
    return element.__class__.__name__


def rational_string(value: Any) -> str | None:
    if value is None:
        return None
    return str(value)


def optional_string(value: Any) -> str | None:
    if value is None:
        return None
    return str(value)


def compare_facts(
    croma: dict[str, Any],
    reference: dict[str, Any],
    max_differences_per_category: int,
) -> tuple[list[dict[str, Any]], Counter[str]]:
    builder = DifferenceBuilder(max_differences_per_category)

    compare_scalar(builder, "parts", "part_count", croma, reference, "part_count")
    compare_parts(builder, croma.get("parts", []), reference.get("parts", []))
    compare_list(builder, "ties_slurs", "slurs", croma.get("slurs", []), reference.get("slurs", []))
    compare_list(
        builder,
        "repeats_endings",
        "repeat_endings",
        croma.get("repeat_endings", []),
        reference.get("repeat_endings", []),
    )
    compare_list(
        builder,
        "harmony_chord_symbols",
        "harmony",
        croma.get("harmony", []),
        reference.get("harmony", []),
    )
    compare_list(
        builder,
        "directions",
        "directions",
        croma.get("directions", []),
        reference.get("directions", []),
    )
    return builder.differences, builder.category_counts


def compare_parts(
    builder: "DifferenceBuilder",
    croma_parts: list[dict[str, Any]],
    reference_parts: list[dict[str, Any]],
) -> None:
    for part_index, (croma_part, reference_part) in enumerate(zip(croma_parts, reference_parts)):
        path = f"parts[{part_index}]"
        compare_scalar(builder, "parts", f"{path}.name", croma_part, reference_part, "name")
        compare_scalar(
            builder,
            "measures",
            f"{path}.measure_count",
            croma_part,
            reference_part,
            "measure_count",
        )
        compare_measures(
            builder,
            path,
            croma_part.get("measures", []),
            reference_part.get("measures", []),
        )


def compare_measures(
    builder: "DifferenceBuilder",
    part_path: str,
    croma_measures: list[dict[str, Any]],
    reference_measures: list[dict[str, Any]],
) -> None:
    for measure_index, (croma_measure, reference_measure) in enumerate(
        zip(croma_measures, reference_measures)
    ):
        path = f"{part_path}.measures[{measure_index}]"
        compare_scalar(
            builder,
            "measure_alignment",
            f"{path}.number",
            croma_measure,
            reference_measure,
            "number",
        )
        compare_scalar(
            builder,
            "measure_alignment",
            f"{path}.duration",
            croma_measure,
            reference_measure,
            "duration",
        )
        compare_list(
            builder,
            "voices",
            f"{path}.voices",
            croma_measure.get("voices", []),
            reference_measure.get("voices", []),
        )
        compare_list(
            builder,
            "repeats_endings",
            f"{path}.barlines",
            croma_measure.get("barlines"),
            reference_measure.get("barlines"),
        )
        compare_events(
            builder,
            path,
            croma_measure.get("events", []),
            reference_measure.get("events", []),
        )


def compare_events(
    builder: "DifferenceBuilder",
    measure_path: str,
    croma_events: list[dict[str, Any]],
    reference_events: list[dict[str, Any]],
) -> None:
    if len(croma_events) != len(reference_events):
        builder.add(
            "measure_alignment",
            f"{measure_path}.events.count",
            len(croma_events),
            len(reference_events),
        )

    for event_index, (croma_event, reference_event) in enumerate(zip(croma_events, reference_events)):
        path = f"{measure_path}.events[{event_index}]"
        compare_scalar(
            builder,
            "measure_alignment",
            f"{path}.kind",
            croma_event,
            reference_event,
            "kind",
        )
        compare_scalar(
            builder,
            "voices",
            f"{path}.voice",
            croma_event,
            reference_event,
            "voice",
        )
        compare_duration(builder, path, croma_event, reference_event)
        compare_scalar(builder, "ties_slurs", f"{path}.tie", croma_event, reference_event, "tie")
        compare_list(
            builder,
            "lyrics",
            f"{path}.lyrics",
            croma_event.get("lyrics", []),
            reference_event.get("lyrics", []),
        )
        compare_event_pitch(builder, path, croma_event, reference_event)


def compare_duration(
    builder: "DifferenceBuilder",
    path: str,
    croma_event: dict[str, Any],
    reference_event: dict[str, Any],
) -> None:
    croma_duration = croma_event.get("duration", {})
    reference_duration = reference_event.get("duration", {})
    compare_scalar(
        builder,
        "durations",
        f"{path}.duration.quarter_length",
        croma_duration,
        reference_duration,
        "quarter_length",
    )
    compare_scalar(
        builder,
        "durations",
        f"{path}.duration.type",
        croma_duration,
        reference_duration,
        "type",
    )
    compare_scalar(
        builder,
        "durations",
        f"{path}.duration.dots",
        croma_duration,
        reference_duration,
        "dots",
    )
    compare_list(
        builder,
        "tuplets",
        f"{path}.duration.tuplets",
        croma_duration.get("tuplets", []),
        reference_duration.get("tuplets", []),
    )


def compare_event_pitch(
    builder: "DifferenceBuilder",
    path: str,
    croma_event: dict[str, Any],
    reference_event: dict[str, Any],
) -> None:
    if "pitch" in croma_event or "pitch" in reference_event:
        compare_pitch(builder, f"{path}.pitch", croma_event.get("pitch"), reference_event.get("pitch"))
    if "pitches" in croma_event or "pitches" in reference_event:
        croma_pitches = croma_event.get("pitches", [])
        reference_pitches = reference_event.get("pitches", [])
        if len(croma_pitches) != len(reference_pitches):
            builder.add("pitches", f"{path}.pitches.count", len(croma_pitches), len(reference_pitches))
        for pitch_index, (croma_pitch, reference_pitch) in enumerate(
            zip(croma_pitches, reference_pitches)
        ):
            compare_pitch(
                builder,
                f"{path}.pitches[{pitch_index}]",
                croma_pitch,
                reference_pitch,
            )


def compare_pitch(
    builder: "DifferenceBuilder",
    path: str,
    croma_pitch: dict[str, Any] | None,
    reference_pitch: dict[str, Any] | None,
) -> None:
    if croma_pitch is None or reference_pitch is None:
        if croma_pitch != reference_pitch:
            builder.add("pitches", path, croma_pitch, reference_pitch)
        return
    compare_scalar(builder, "pitches", f"{path}.step", croma_pitch, reference_pitch, "step")
    compare_scalar(builder, "octaves", f"{path}.octave", croma_pitch, reference_pitch, "octave")
    compare_scalar(
        builder,
        "accidentals",
        f"{path}.accidental",
        croma_pitch,
        reference_pitch,
        "accidental",
    )


def compare_scalar(
    builder: "DifferenceBuilder",
    category: str,
    path: str,
    croma: dict[str, Any],
    reference: dict[str, Any],
    key: str,
) -> None:
    if croma.get(key) != reference.get(key):
        builder.add(category, path, croma.get(key), reference.get(key))


def compare_list(
    builder: "DifferenceBuilder",
    category: str,
    path: str,
    croma: Any,
    reference: Any,
) -> None:
    if croma != reference:
        builder.add(category, path, croma, reference)


class DifferenceBuilder:
    def __init__(self, max_per_category: int) -> None:
        self.max_per_category = max(0, max_per_category)
        self.category_counts: Counter[str] = Counter()
        self._stored_counts: Counter[str] = Counter()
        self.differences: list[dict[str, Any]] = []

    def add(self, category: str, path: str, croma: Any, reference: Any) -> None:
        self.category_counts[category] += 1
        if self._stored_counts[category] >= self.max_per_category:
            return
        self._stored_counts[category] += 1
        self.differences.append(
            {
                "category": category,
                "path": path,
                "croma": summarize(croma),
                "reference": summarize(reference),
            }
        )


def summarize(value: Any) -> Any:
    if isinstance(value, list):
        return {"count": len(value), "sample": value[:8]}
    if isinstance(value, dict):
        return {key: summarize_dict_value(inner) for key, inner in value.items()}
    return value


def summarize_dict_value(value: Any) -> Any:
    if isinstance(value, list):
        return {"count": len(value), "sample": value[:8]}
    return value


def emit(payload: dict[str, Any], json_output: bool) -> int:
    if json_output:
        print(json.dumps(payload, indent=2, default=str))
    else:
        print(payload["status"])
    return 0 if payload["status"] in {"match", "difference"} else 3


if __name__ == "__main__":
    raise SystemExit(main())
