#!/usr/bin/env python3
"""Compare two MusicXML files through music21-derived structural facts."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


def main() -> int:
    args = parse_args()
    try:
        croma = extract_facts(args.croma_xml)
        reference = extract_facts(args.reference_xml)
    except Music21Unavailable as error:
        return emit({"status": "music21_unavailable", "error": str(error)}, args.json)
    except Exception as error:  # noqa: BLE001 - tool failures must be reported distinctly.
        return emit({"status": "music21_tool_failure", "error": str(error)}, args.json)

    differences = compare_facts(croma, reference)
    status = "match" if not differences else "difference"
    return emit(
        {
            "status": status,
            "croma_xml": str(args.croma_xml),
            "reference_xml": str(args.reference_xml),
            "differences": differences,
            "classification": "unclassified_difference" if differences else None,
        },
        args.json,
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Compare MusicXML with music21 facts")
    parser.add_argument("--croma-xml", type=Path, required=True)
    parser.add_argument("--reference-xml", type=Path, required=True)
    parser.add_argument("--json", action="store_true")
    return parser.parse_args()


class Music21Unavailable(RuntimeError):
    pass


def extract_facts(path: Path) -> dict[str, Any]:
    try:
        from music21 import chord, converter, dynamics, expressions, harmony, note, spanner
    except ImportError as error:
        raise Music21Unavailable(
            "music21 is not installed; install it in the local dev environment to run comparison"
        ) from error

    score = converter.parse(path)
    parts = []
    for part_index, part in enumerate(score.parts):
        measures = []
        for measure in part.getElementsByClass("Measure"):
            events = []
            for element in measure.notesAndRests:
                if isinstance(element, note.Note):
                    events.append(
                        {
                            "kind": "note",
                            "voice": voice_id(element),
                            "pitch": {
                                "step": element.pitch.step,
                                "octave": element.pitch.octave,
                                "accidental": accidental_name(element.pitch.accidental),
                            },
                            "duration": duration_facts(element),
                            "tie": tie_facts(element),
                            "lyrics": lyric_facts(element),
                        }
                    )
                elif isinstance(element, chord.Chord):
                    events.append(
                        {
                            "kind": "chord",
                            "voice": voice_id(element),
                            "pitches": [
                                {
                                    "step": pitch.step,
                                    "octave": pitch.octave,
                                    "accidental": accidental_name(pitch.accidental),
                                }
                                for pitch in element.pitches
                            ],
                            "duration": duration_facts(element),
                            "lyrics": lyric_facts(element),
                        }
                    )
                elif isinstance(element, note.Rest):
                    events.append(
                        {
                            "kind": "rest",
                            "voice": voice_id(element),
                            "duration": duration_facts(element),
                        }
                    )
            measures.append(
                {
                    "number": str(measure.number),
                    "voices": sorted(
                        {
                            event.get("voice")
                            for event in events
                            if event.get("voice") is not None
                        }
                    ),
                    "events": events,
                }
            )
        parts.append(
            {
                "id": getattr(part, "id", None),
                "name": part.partName,
                "measure_count": len(measures),
                "measures": measures,
            }
        )

    return {
        "parts": parts,
        "part_count": len(parts),
        "slurs": len(score.recurse().getElementsByClass(spanner.Slur)),
        "harmony": [str(item.figure) for item in score.recurse().getElementsByClass(harmony.ChordSymbol)],
        "directions": [
            str(item)
            for item in score.recurse()
            if isinstance(item, (expressions.TextExpression, dynamics.Dynamic))
        ],
    }


def accidental_name(accidental: Any) -> str | None:
    return None if accidental is None else accidental.name


def duration_facts(element: Any) -> dict[str, Any]:
    return {
        "quarter_length": str(element.duration.quarterLength),
        "tuplets": [
            {
                "actual": tuplet.numberNotesActual,
                "normal": tuplet.numberNotesNormal,
                "type": tuplet.type,
            }
            for tuplet in element.duration.tuplets
        ],
    }


def tie_facts(element: Any) -> str | None:
    return None if element.tie is None else element.tie.type


def lyric_facts(element: Any) -> list[str]:
    return [lyric.text for lyric in element.lyrics if lyric.text]


def voice_id(element: Any) -> str | None:
    if element.activeSite is not None and element.activeSite.classes[0] == "Voice":
        return str(element.activeSite.id)
    return None


def compare_facts(croma: dict[str, Any], reference: dict[str, Any]) -> list[dict[str, Any]]:
    differences = []
    for key in [
        "part_count",
        "parts",
        "slurs",
        "harmony",
        "directions",
    ]:
        if croma.get(key) != reference.get(key):
            differences.append(
                {
                    "category": category_for_key(key),
                    "field": key,
                    "croma": summarize_value(key, croma.get(key)),
                    "reference": summarize_value(key, reference.get(key)),
                }
            )
    return differences


def summarize_value(key: str, value: Any) -> Any:
    if key == "parts" and isinstance(value, list):
        return {
            "part_count": len(value),
            "parts": [summarize_part(part) for part in value[:4]],
        }
    if isinstance(value, list):
        return {
            "count": len(value),
            "sample": value[:20],
        }
    return value


def summarize_part(part: dict[str, Any]) -> dict[str, Any]:
    measures = part.get("measures", [])
    return {
        "id": part.get("id"),
        "name": part.get("name"),
        "measure_count": part.get("measure_count"),
        "measures": [summarize_measure(measure) for measure in measures[:4]],
    }


def summarize_measure(measure: dict[str, Any]) -> dict[str, Any]:
    events = measure.get("events", [])
    return {
        "number": measure.get("number"),
        "voices": measure.get("voices", []),
        "event_count": len(events),
        "events": events[:8],
    }


def category_for_key(key: str) -> str:
    return {
        "part_count": "parts",
        "parts": "measure_alignment",
        "slurs": "ties_slurs",
        "harmony": "harmony_chord_symbols",
        "directions": "directions",
    }.get(key, "structural")


def emit(payload: dict[str, Any], json_output: bool) -> int:
    if json_output:
        print(json.dumps(payload, indent=2))
    else:
        print(payload["status"])
    return 0 if payload["status"] in {"match", "difference"} else 3


if __name__ == "__main__":
    raise SystemExit(main())
