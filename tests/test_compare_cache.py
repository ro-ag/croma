from __future__ import annotations

import sys
import time
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO_ROOT / "tools"))

from compare_cache import (  # noqa: E402
    CompareCache,
    facts_cache_version,
    file_sha256,
    pair_result_key,
    result_cache_version,
)


def test_facts_roundtrip_and_version_isolation(tmp_path: Path) -> None:
    cache = CompareCache.open(tmp_path / "cache.sqlite")
    facts = {"parts": [{"id": "P1", "measures": []}], "part_count": 1}
    cache.put_facts("hash-a", "v1", "tune.abc", "croma", facts)

    assert cache.get_facts("hash-a", "v1") == facts
    assert cache.get_facts("hash-a", "v2") is None
    assert cache.get_facts("hash-b", "v1") is None
    cache.close()


def test_result_roundtrip_and_invalidate_by_name(tmp_path: Path) -> None:
    cache = CompareCache.open(tmp_path / "cache.sqlite")
    payload = {"counters": {"structural_matches": 1}, "mismatch_rows": 0}
    key = pair_result_key("hash-a", "hash-b", "v1", "tune.abc", {"component_filter": None})
    cache.put_result(key, "tune.abc", payload)

    assert cache.get_result(key) == payload
    assert cache.invalidate_relative_path("tune.abc") == 1
    assert cache.get_result(key) is None
    cache.close()


def test_prune_removes_stale_rows(tmp_path: Path) -> None:
    cache = CompareCache.open(tmp_path / "cache.sqlite")
    cache.put_facts("hash-a", "v1", "old.abc", "croma", {"part_count": 0})
    stale = int(time.time()) - 30 * 86400
    cache._connection.execute("UPDATE facts_cache SET last_used_at = ?", (stale,))
    cache._connection.commit()
    cache.put_facts("hash-b", "v1", "fresh.abc", "croma", {"part_count": 1})

    assert cache.prune_stale(max_age_days=14) == 1
    assert cache.get_facts("hash-a", "v1") is None
    assert cache.get_facts("hash-b", "v1") == {"part_count": 1}
    cache.close()


def test_corrupt_db_recreated_on_open(tmp_path: Path) -> None:
    path = tmp_path / "cache.sqlite"
    path.write_bytes(b"garbage, not sqlite")
    cache = CompareCache.open(path)
    cache.put_facts("hash-a", "v1", "tune.abc", "croma", {"part_count": 1})
    assert cache.get_facts("hash-a", "v1") == {"part_count": 1}
    cache.close()


def test_unserializable_facts_raise_without_writing(tmp_path: Path) -> None:
    cache = CompareCache.open(tmp_path / "cache.sqlite")
    try:
        cache.put_facts("hash-a", "v1", "tune.abc", "croma", {"bad": object()})
    except TypeError:
        pass
    else:  # pragma: no cover - the put must not silently coerce values.
        raise AssertionError("expected TypeError for non-JSON-serializable facts")
    assert cache.get_facts("hash-a", "v1") is None
    cache.close()


def test_versions_and_file_hash_are_stable_strings(tmp_path: Path) -> None:
    facts_version = facts_cache_version()
    assert facts_version == facts_cache_version()
    assert len(facts_version) == 16
    result_version = result_cache_version(facts_version)
    assert result_version == result_cache_version(facts_version)
    assert result_version != facts_version

    sample = tmp_path / "sample.xml"
    sample.write_text("<score/>", encoding="utf-8")
    digest = file_sha256(sample)
    assert digest == file_sha256(sample)
    assert file_sha256(tmp_path / "missing.xml") is None
