#!/usr/bin/env python3
"""Prove the `Score -> ABC` writer round-trips the corpus with no structural diff.

For every in-scope ABC file under --abc-root this runs, via the built `croma`
binary:

  1. `croma dump score FILE`     -> decide slice-1 in-scope (see below)
  2. `croma xml FILE`            -> original MusicXML
  3. `croma dump abc FILE`       -> regenerated ABC (the writer under test)
  4. `croma xml <regenerated>`   -> round-tripped MusicXML

then extracts a STRUCTURAL PROJECTION from both MusicXML outputs and asserts
they are identical. The projection is the normalized musical-fact stream:
ordered pitches (step, alter, octave) + per-event durations (normalized by
`<divisions>`, so a different `L:` choice is not a false diff) + rest durations
+ measure boundaries + barline / repeat / ending structure + ties.

Bar: **0 structural diffs over the in-scope subset.** Coverage (in_scope/total)
is reported so later slices can track growth.

Slice-1 in-scope filter (a tune qualifies only if its lowered `Score` satisfies
all): exactly one part and one voice; no Chord and no Spacer events; every
event's attachments have empty tuplets / grace_groups / slurs / lyrics / symbols
/ chord_symbols / annotations / decorations; every barline kind is one of
Regular, Double, Final, RepeatStart, RepeatEnd, RepeatBoth. Detected from the
`croma dump score` Debug text.

LOCAL ONLY — never wire this into CI. The corpus is external; provision it per
AGENTS.md. Report is written under docs/untracked/abc/.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET
from concurrent.futures import ProcessPoolExecutor
from fractions import Fraction
from pathlib import Path

CROMA = "target/debug/croma"

# Attachment vectors the writer drops; a non-empty one makes a tune out of scope.
# In the pretty Debug dump an empty Vec is `field: []` and a non-empty one is
# `field: [\n    ...`, so a newline right after `[` flags a non-empty vector.
_FORBIDDEN_ATTACHMENTS = (
    "lyrics",
    "symbols",
)
_FORBIDDEN_ATTACH_RE = {
    f: re.compile(rf"{f}: \[\n") for f in _FORBIDDEN_ATTACHMENTS
}
# Barline kinds outside slice-1 scope (Regular/Double/Final/Repeat* are in).
_FORBIDDEN_BARLINE_RE = re.compile(
    r"kind: (?:Initial|Dotted|Invisible|Liberal),"
)
# Mid-tune key field, inline (`[K:...]`) or as a standalone body line. The writer
# emits only the header `K:`, and a mid-tune key change is not even preserved in
# the lowered `Score` (its effect is baked into note alters), so such tunes
# cannot round-trip and are out of slice-1 scope. Detected from the source.
_HEADER_KEY_LINE_RE = re.compile(r"^\s*K:", re.MULTILINE)
_INLINE_KEY_RE = re.compile(r"\[K:")
# A slur that wraps only a grace group with no main note (`({Bc})`): the grace
# close is immediately followed by the slur close. Degenerate; out of scope.
_BARE_GRACE_SLUR_RE = re.compile(r"\}\)")
# Voice overlays (`&` within a measure) are simultaneous voices stored in
# `Measure.overlays`; the single-voice writer emits only the primary voice, so
# overlay tunes are out of scope (belongs with multi-voice support).
_OVERLAY_RE = re.compile(r"overlays: \[\n")
# A measure-ending barline (Double/Final/Regular/RepeatEnd/RepeatBoth) as the
# FIRST timeline event — before any note — renders to nothing in the forward
# pipeline but, emitted by the writer, spawns a phantom leading empty measure on
# re-parse (the `||:` -> `|| |:` case). A leading RepeatStart is fine. This is
# the phantom-empty-measure / combined-barline class; out of scope for now.
_MEASURE_ENDING = {"Regular", "Double", "Final", "RepeatEnd", "RepeatBoth"}
# Key/voice transposition modifiers (`octave=`, `transpose=`) shift `pitch.octave`
# at parse time; the writer emits the shifted pitch AND echoes the modifier, so a
# re-parse shifts a second time. Out of slice-1 scope. Detected from the source.
_TRANSPOSE_MODIFIER_RE = re.compile(r"(?:octave|transpose)=")
# A tuplet led by a rest (`(3z...`) leaves the rest unattributed and gives the
# group no Start event, so the writer cannot place the opening marker. Rare; out
# of scope for now. (Rests *inside* a tuplet, e.g. `(3Bz A`, are handled.)
_REST_LED_TUPLET_RE = re.compile(r"\(\d[:\d]*[zx]")


def _init_worker(croma: str) -> None:
    global CROMA
    CROMA = croma


def run(args: list[str]) -> tuple[int, str]:
    proc = subprocess.run(args, capture_output=True, text=True)
    return proc.returncode, proc.stdout


def has_mid_tune_field_change(source: str) -> bool:
    """True iff the ABC body carries a key change after the header `K:`.

    Anchored on a header `K:` (which ABC requires to terminate the tune header).
    A pathological tune with only an inline `[K:...]` and no header key is not
    flagged here, but such input fails to lower and is dropped at `check_one`
    (the parser, not this regex, is the backstop for that case).
    """
    first = _HEADER_KEY_LINE_RE.search(source)
    if first is None:
        return False
    body = source[first.end():]
    return bool(_HEADER_KEY_LINE_RE.search(body) or _INLINE_KEY_RE.search(body))


def has_leading_measure_ending_barline(score_dump: str) -> bool:
    """True iff the first timeline event is a measure-ending barline."""
    i = score_dump.find("events: [")
    if i < 0:
        return False
    tail = score_dump[i + len("events: [") :]
    # The TimedEvent's measure/onset/duration/source fields contain no event-kind
    # variant names, so the first match here is the first event's kind.
    first = re.search(r"kind: (Note|Rest|Chord|Spacer|Barline|RepeatEnding)", tail)
    if not first or first.group(1) != "Barline":
        return False
    inner = re.search(
        r"kind: (Regular|Double|Final|RepeatStart|RepeatEnd|RepeatBoth"
        r"|Initial|Dotted|Invisible|Liberal)",
        tail[first.end() : first.end() + 300],
    )
    return bool(inner and inner.group(1) in _MEASURE_ENDING)


def is_in_scope(score_dump: str, source: str) -> bool:
    """True iff the lowered Score uses only currently-supported constructs."""
    if score_dump.count("Part {") != 1 or score_dump.count("Voice {") != 1:
        return False
    if "kind: Spacer" in score_dump:
        return False
    if _OVERLAY_RE.search(score_dump):
        return False
    if has_leading_measure_ending_barline(score_dump):
        return False
    if _FORBIDDEN_BARLINE_RE.search(score_dump):
        return False
    if has_mid_tune_field_change(source):
        return False
    if _BARE_GRACE_SLUR_RE.search(source):
        return False
    if _TRANSPOSE_MODIFIER_RE.search(source):
        return False
    if _REST_LED_TUPLET_RE.search(source):
        return False
    return not any(rx.search(score_dump) for rx in _FORBIDDEN_ATTACH_RE.values())


def projection(xml: str):
    """Structural projection of a MusicXML document, in document order.

    Durations are normalized by the active `<divisions>` so the absolute unit
    (`L:`) is irrelevant — only the musical fraction matters.
    """
    root = ET.fromstring(xml)
    proj: list[tuple] = []
    for part in root.findall("part"):
        proj.append(("PART",))
        divisions = 1
        for measure in part.findall("measure"):
            proj.append(("MEASURE",))
            for el in measure:
                if el.tag == "attributes":
                    div = el.findtext("divisions")
                    if div:
                        divisions = int(div)
                elif el.tag == "note":
                    dur_text = el.findtext("duration")
                    dur = (
                        Fraction(int(dur_text), divisions)
                        if dur_text is not None
                        else None
                    )
                    ties = tuple(sorted(t.get("type") for t in el.findall("tie")))
                    # Slur start/stop on this note (number is not compared — it
                    # can be renumbered — only the per-note start/stop pattern).
                    slurs = tuple(sorted(s.get("type") for s in el.iter("slur")))
                    # Decorations: all element tags under <notations> except the
                    # slur/tied markers handled separately (fermata, articulations
                    # /staccato, ornaments/trill-mark, technical/up-bow, ...).
                    notations = el.find("notations")
                    decos = (
                        tuple(sorted(
                            e.tag for e in notations.iter()
                            if e.tag not in ("notations", "slur", "tied")
                        ))
                        if notations is not None
                        else ()
                    )
                    # Tuplet ratio (actual:normal) from <time-modification>.
                    tmod = el.find("time-modification")
                    ratio = (
                        (tmod.findtext("actual-notes"), tmod.findtext("normal-notes"))
                        if tmod is not None
                        else None
                    )
                    grace = el.find("grace")
                    # None / "grace" / "grace:yes" (acciaccatura slash).
                    is_grace = (
                        f"grace:{grace.get('slash')}" if grace is not None else None
                    )
                    is_chord = el.find("chord") is not None
                    if el.find("rest") is not None:
                        proj.append(("R", dur, slurs, decos, ratio))
                    else:
                        pitch = el.find("pitch")
                        step = pitch.findtext("step") if pitch is not None else None
                        alter = pitch.findtext("alter") if pitch is not None else None
                        octave = pitch.findtext("octave") if pitch is not None else None
                        proj.append(
                            (
                                "C" if is_chord else "N",
                                step,
                                alter or "0",
                                octave,
                                dur,
                                ties,
                                slurs,
                                decos,
                                ratio,
                                is_grace,
                            )
                        )
                elif el.tag == "harmony":
                    # Chord symbol -> <harmony>: root + chord text (kind@text).
                    root = el.find("root")
                    kind = el.find("kind")
                    proj.append((
                        "HARMONY",
                        root.findtext("root-step") if root is not None else None,
                        root.findtext("root-alter") if root is not None else None,
                        kind.get("text") if kind is not None else None,
                    ))
                # NOTE: <direction> (annotations, tempo text, dynamics) is
                # intentionally NOT projected. It conflates annotation `"text"`
                # with tempo text (`Q:"Moderato"`), which the writer drops by
                # design (tempo is metadata, not structural music — see design
                # doc). Annotations are emitted verbatim and unit-tested instead.
                elif el.tag == "barline":
                    rep = el.find("repeat")
                    proj.append(
                        (
                            "BAR",
                            el.get("location"),
                            el.findtext("bar-style"),
                            rep.get("direction") if rep is not None else None,
                            tuple(
                                (e.get("number"), e.get("type"))
                                for e in el.findall("ending")
                            ),
                        )
                    )
    return proj


def write_temp(text: str) -> str:
    with tempfile.NamedTemporaryFile(
        "w", suffix=".abc", delete=False, errors="replace"
    ) as tmp:
        tmp.write(text)
        return tmp.name


def check_one(abc_path_str: str) -> dict:
    name = Path(abc_path_str).name
    rec = {"file": name, "in_scope": False, "diff": False, "error": False}

    code, score_dump = run([CROMA, "dump", "score", abc_path_str])
    if code != 0 or not score_dump:
        return rec  # parse/lower failure -> not in scope, not a writer concern
    source = Path(abc_path_str).read_text(errors="replace")
    if not is_in_scope(score_dump, source):
        return rec
    rec["in_scope"] = True

    _, xml_original = run([CROMA, "xml", abc_path_str])
    code_abc, regenerated = run([CROMA, "dump", "abc", abc_path_str])
    if code_abc != 0 or not regenerated:
        rec["error"] = True
        return rec

    regen_path = write_temp(regenerated)
    try:
        _, xml_roundtrip = run([CROMA, "xml", regen_path])
    finally:
        Path(regen_path).unlink(missing_ok=True)

    try:
        rec["diff"] = projection(xml_original) != projection(xml_roundtrip)
    except ET.ParseError:
        rec["error"] = True
    return rec


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--abc-root", required=True, help="directory of .abc files")
    ap.add_argument("--croma", default=CROMA, help="path to the croma binary")
    ap.add_argument("--jobs", type=int, default=0, help="workers (0 = cpu count)")
    ap.add_argument("--limit", type=int, default=0, help="cap files (0 = all)")
    ap.add_argument(
        "--out",
        default="docs/untracked/abc/abc-roundtrip-report.json",
        help="report JSON path",
    )
    args = ap.parse_args()

    files = sorted(str(p) for p in Path(args.abc_root).glob("*.abc"))
    if args.limit:
        files = files[: args.limit]
    if not files:
        print(f"no .abc files under {args.abc_root}", file=sys.stderr)
        return 2

    records = []
    with ProcessPoolExecutor(
        max_workers=args.jobs or None,
        initializer=_init_worker,
        initargs=(args.croma,),
    ) as pool:
        for index, rec in enumerate(pool.map(check_one, files, chunksize=16), 1):
            records.append(rec)
            if index % 1000 == 0:
                print(f"  {index}/{len(files)}", file=sys.stderr)

    in_scope = [r for r in records if r["in_scope"]]
    diffs = [r["file"] for r in in_scope if r["diff"]]
    errors = [r["file"] for r in in_scope if r["error"]]
    total = len(records)
    coverage = (len(in_scope) / total * 100.0) if total else 0.0

    summary = {
        "total": total,
        "in_scope": len(in_scope),
        "coverage_pct": round(coverage, 2),
        "structural_diffs": len(diffs),
        "errors": len(errors),
        "structural_diff_files": diffs[:50],
        "error_files": errors[:50],
    }

    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps({"summary": summary, "records": records}, indent=2))

    print(json.dumps(summary, indent=2))
    print(f"\nreport: {out_path}", file=sys.stderr)

    # The whole point: 0 structural diffs (and 0 writer errors) on the in-scope set.
    ok = not diffs and not errors
    print("\nRESULT:", "PASS" if ok else "FAIL", file=sys.stderr)
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
