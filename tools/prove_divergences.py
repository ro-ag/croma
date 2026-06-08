#!/usr/bin/env python3
"""Per-file verdict for every croma-vs-abc2xml divergence in the 10k corpus.

For each .abc file this assigns a transparent verdict from concrete signals
(export status, measure-count delta, mismatch-category set, and ABC-source
features) so that the question "how many files have a genuine Croma issue?" can
be answered file-by-file with an auditable trail.

Verdict taxonomy (see docs/comparison/abc2xml-divergences/):
  MATCH                      - identical, no differing rows
  EXPORT_FAILURE_NO_MUSIC    - header-only tune, nothing to export        (doc 01)
  ARTIFACT_PHANTOM_MEASURE   - abc2xml empty measure at annotation/section (doc 02)
  ARTIFACT_MULTIREST         - abc2xml expands Z/X multi-measure rest      (doc 03)
  ARTIFACT_BARLINE           - spaced `| |` / line-split rendered double   (doc 04)
  ARTIFACT_ACCIDENTAL_ALTER  - redundant <alter>0> serialization           (doc 05)
  ARTIFACT_DURATION          - §4.6 default length / abc2xml rounding      (doc 06)
  CASCADE                    - positional cascade of a structural offset   (doc 07)
  ARTIFACT_TUPLET            - tuplet bracket-marker placement             (doc 08)
  TIE_SLUR_EDGE              - dropped-legal / malformed / endpoint edge   (doc 09)
  DIRECTION                  - tempo/annotation text edge                  (doc 10)
  REVIEW                     - not explained by any rule -> inspect by hand

Usage:
  uv run python tools/prove_divergences.py --phase-dir docs/untracked/phase-21 \
      --abc-root docs/untracked/corpus/zenodo-10k/abc \
      --ref-root docs/untracked/corpus/zenodo-10k/musicxml \
      --out docs/comparison/abc2xml-divergences/per-file-manifest.csv
"""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

CASCADE_ONLY = {"pitch", "octave", "harmony", "lyric"}
STRUCTURAL = {"missing_in_croma", "extra_in_croma", "measure_alignment", "voice"}

ANNOTATION_LINE = re.compile(r'(?m)^\s*(?:"[^"]*"|\[[A-Za-z]:[^\]]*\]|\\|\s)+$')
MULTIREST = re.compile(r"(?:^|[\s|])[ZX]\d+")
SPACED_BAR = re.compile(r"\|\s+\||\|\\\s*$")


def count_measures(path: Path) -> int:
    try:
        return path.read_text(errors="replace").count("<measure ")
    except OSError:
        return -1


def abc_features(text: str) -> dict:
    body = []
    for ln in text.splitlines():
        s = ln.strip()
        if not s or re.match(r"^[A-Za-z]:", s) or s.startswith("%"):
            continue
        body.append(ln)
    body_text = "\n".join(body)
    has_ann = any(
        ANNOTATION_LINE.match(ln) and '"' in ln for ln in body
    ) or bool(re.search(r'"[^"]*"\s*\[[A-Za-z]:', body_text))
    return {
        "has_annotation_line": has_ann,
        "has_multirest": bool(MULTIREST.search(body_text)),
        "has_spaced_bar": bool(SPACED_BAR.search(body_text)),
        "no_l_field": not re.search(r"(?m)^L:", text),
    }


def classify(rec: dict) -> str:
    cats = rec["cats"]
    if rec["export_failure"]:
        return "EXPORT_FAILURE_NO_MUSIC"
    if not cats:
        return "MATCH"
    cm, rm = rec["croma_measures"], rec["ref_measures"]
    feat = rec["features"]

    # Structural measure-count divergence -> phantom measure / multirest artifact.
    if cm >= 0 and rm >= 0 and cm < rm:
        if feat["has_multirest"]:
            return "ARTIFACT_MULTIREST"
        return "ARTIFACT_PHANTOM_MEASURE"
    if cm >= 0 and rm >= 0 and cm > rm:
        return "REVIEW"  # croma adding measures is suspicious -> inspect

    # Equal measure counts below here.
    if cats == {"accidental"}:
        return "ARTIFACT_ACCIDENTAL_ALTER"
    if cats == {"barline"}:
        return "ARTIFACT_BARLINE" if feat["has_spaced_bar"] else "REVIEW"
    if cats == {"tuplet"}:
        return "ARTIFACT_TUPLET"
    if cats == {"duration"}:
        return "ARTIFACT_DURATION"
    if cats == {"direction"}:
        return "DIRECTION"
    if cats <= {"tie", "slur"}:
        return "TIE_SLUR_EDGE"
    # A structural offset present -> the per-note categories are cascades.
    if cats & STRUCTURAL:
        return "CASCADE"
    # Only cascade-prone per-note categories, equal measures: still a local
    # positional cascade (within-measure event-count offset).
    if cats <= (CASCADE_ONLY | {"accidental", "duration", "tuplet", "tie", "slur", "barline", "direction"}):
        return "CASCADE"
    return "REVIEW"


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--phase-dir", required=True)
    ap.add_argument("--abc-root", required=True)
    ap.add_argument("--ref-root", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    phase = Path(args.phase_dir)
    abc_root = Path(args.abc_root)
    ref_root = Path(args.ref_root)
    xml_root = phase / "full-10k-xml"

    summary = {
        json.loads(l)["filename"]: json.loads(l)
        for l in (phase / "per-file-summary.jsonl").read_text().splitlines()
    }

    report = json.loads((phase / "full-10k-report-only-compare-report.json").read_text())
    export_failures = {
        f["relative_path"] for f in (report.get("croma_failures") or [])
    }
    # croma_failures in the report is capped; recover the full set from results jsonl.
    for l in (phase / "full-10k-export-results.jsonl").read_text().splitlines():
        r = json.loads(l)
        if r.get("returncode", 0) == 1:
            export_failures.add(r["relative_path"])

    rows = []
    for fn, rec in summary.items():
        mc = rec.get("mismatch_categories")
        cats = set(json.loads(mc)) if isinstance(mc, str) and mc else (set(mc) if mc else set())
        stem = fn[:-4] if fn.endswith(".abc") else fn
        is_fail = fn in export_failures
        try:
            abc_text = (abc_root / fn).read_text(errors="replace")
        except OSError:
            abc_text = ""
        out = {
            "filename": fn,
            "export_failure": is_fail,
            "mismatch_rows": rec.get("mismatch_rows", 0) or 0,
            "cats": cats,
            "croma_measures": count_measures(xml_root / f"{stem}.croma.musicxml") if not is_fail else -1,
            "ref_measures": count_measures(ref_root / f"{stem}.xml"),
            "features": abc_features(abc_text),
        }
        out["verdict"] = classify(out)
        rows.append(out)

    # write CSV manifest (only files that are not perfect MATCH)
    import csv

    out_path = Path(args.out)
    diff_rows = [r for r in rows if r["verdict"] != "MATCH"]
    with out_path.open("w", newline="") as fh:
        w = csv.writer(fh)
        w.writerow(["filename", "verdict", "mismatch_rows", "categories",
                    "croma_measures", "ref_measures", "measure_delta"])
        for r in sorted(diff_rows, key=lambda r: (r["verdict"], -r["mismatch_rows"])):
            md = (r["ref_measures"] - r["croma_measures"]) if r["croma_measures"] >= 0 else ""
            w.writerow([r["filename"], r["verdict"], r["mismatch_rows"],
                        "|".join(sorted(r["cats"])), r["croma_measures"],
                        r["ref_measures"], md])

    # summary
    import collections
    counts = collections.Counter(r["verdict"] for r in rows)
    total = len(rows)
    match = counts.get("MATCH", 0)
    review = [r["filename"] for r in rows if r["verdict"] == "REVIEW"]
    print(f"total files            : {total}")
    print(f"MATCH (identical)      : {match}")
    print(f"differing / failed     : {total - match}")
    print("verdict breakdown:")
    for v, n in counts.most_common():
        if v == "MATCH":
            continue
        print(f"  {n:>5}  {v}")
    print(f"\nREVIEW (potential genuine Croma issue): {len(review)}")
    for f in review[:50]:
        print(f"  {f}")
    print(f"\nmanifest: {out_path}  ({len(diff_rows)} rows)")


if __name__ == "__main__":
    main()
