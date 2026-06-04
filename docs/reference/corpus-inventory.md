# Corpus Inventory

## Located Corpus

- ABC corpus directory: `/Users/rodox/dev/rs/trd/test/real/abc`
- ABC `.abc` files discovered under that directory: `10000`
- Directory size: `39M`
- Indexed 10k manifest: `/Users/rodox/dev/rs/trd/test/real/manifest.jsonl`
- Manifest rows: `10000`
- Reference MusicXML directory: `/Users/rodox/dev/rs/trd/test/real/musicxml`
- Reference MusicXML size: `308M`
- TRD coverage DB: `/Users/rodox/dev/rs/trd/test/real/coverage/coverage.sqlite3`
- Coverage DB size: `15M`

The corpus should stay outside tracked Croma paths. Reports derived from it
belong under `docs/untracked/`.

## Existing TRD Corpus Tooling

- Python package: `/Users/rodox/dev/rs/trd/src/traduttore_tools`
- Corpus command surface: `uv run trd-corpus ...`
- Coverage DB command surface: `uv run trd-coverage-db ...`
- Reference converter used by TRD: `/Users/rodox/dev/rs/trd/test/real/tools/abc2xml`
- Smoke output directory: `/Users/rodox/dev/rs/trd/test/real/trd-smoke`

TRD uses a SQLite database with tables for samples, feature counts, smoke runs,
semantic categories, finding tags, and dispositions. This is the right shape
for Croma's later corpus harness because it avoids repeatedly rereading a giant
Markdown tracker.

## Latest TRD DB Snapshot

- `corpus_samples`: `10000`
- `test_inventory`: `865`
- `smoke_runs`: `1`
- Latest smoke run path: `test/real/trd-smoke/report.jsonl`
- Entries: `10000`
- Converted: `10000`
- Failed: `0`
- Timed out: `0`
- Invalid XML: `0`
- Schema invalid: `0`
- Reference missing: `0`
- Metric mismatches: `284`
- Measure mismatches: `244`
- Semantic mismatches: `1670`
- Semantic compare errors: `0`

Latest disposition counts:

| Disposition | Count |
| --- | ---: |
| `reference_artifact` | 1225 |
| `investigate` | 372 |
| `recovery_candidate` | 47 |
| `unsupported_notation` | 26 |

Latest first semantic mismatch categories:

| Category | Count |
| --- | ---: |
| `event_or_measure_alignment` | 1214 |
| `voice_part_staff_model` | 210 |
| `pitch_or_octave_interpretation` | 150 |
| `duration_or_tuplet_policy` | 83 |
| `key_signature_or_mode_encoding` | 8 |
| `barline_repeat_ending_encoding` | 3 |
| `directions_text_tempo_encoding` | 2 |

## High-Frequency Features From TRD's Existing DB

The TRD DB already has a feature sampler over the 10k manifest. Top signals by
files affected:

| Feature | Files | Occurrences |
| --- | ---: | ---: |
| `explicit_note_accidentals` | 4281 | 24525 |
| `quoted_text` | 4168 | 55861 |
| `leading_section_barline` | 4120 | 6842 |
| `broken_rhythm` | 3505 | 34733 |
| `unmarked_quoted_text` | 2759 | 51326 |
| `endings` | 2362 | 6951 |
| `tuplets` | 1560 | 6036 |
| `voices` | 1287 | 3155 |
| `grace_groups` | 995 | 5690 |
| `directives` | 925 | 1499 |
| `lyrics` | 807 | 3215 |
| `decorations` | 737 | 8290 |
| `directive_midi` | 571 | 1138 |
| `overlays` | 565 | 891 |
| `old_linebreak_bang` | 513 | 1666 |
| `modifier_clef` | 470 | 1405 |
| `inline_voices` | 442 | 22429 |
| `inline_keys` | 377 | 616 |

These are enough to justify the next parser priorities before Croma can run its
own full parse/export corpus comparison.

## Sources

- Local ABC corpus directory:
  `/Users/rodox/dev/rs/trd/test/real/abc`
- Local 10k corpus manifest:
  `/Users/rodox/dev/rs/trd/test/real/manifest.jsonl`
- Local reference MusicXML directory:
  `/Users/rodox/dev/rs/trd/test/real/musicxml`
- Local TRD coverage SQLite database:
  `/Users/rodox/dev/rs/trd/test/real/coverage/coverage.sqlite3`
- Local TRD corpus tooling:
  `/Users/rodox/dev/rs/trd/src/traduttore_tools/corpus.py`
- Local TRD coverage DB tooling:
  `/Users/rodox/dev/rs/trd/src/traduttore_tools/coverage_db.py`
- Local TRD chat transcript used for project context:
  `/Users/rodox/dev/rs/trd/docs/chats/2026-06-03-traduttore-coverage-continuation.md`
- Counts were produced with local filesystem, `sqlite3`, `find`, `wc`, and
  `du` commands on June 3, 2026. The `.abc` file count was re-verified from
  the local filesystem on June 4, 2026.
