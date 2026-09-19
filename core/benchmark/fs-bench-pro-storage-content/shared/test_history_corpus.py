#!/usr/bin/env python3
"""Self-checks for the retained-history corpus reader.

`preparation.md` §8 lists nine obligations. Seven of them are testable here or in
the Rust reader's own tests, without a product run and without the real corpus:

| # | obligation | where |
| --: | --- | --- |
| 1 | a manifest with one byte changed is refused with `ManifestIdentity` | here |
| 2 | a wrong `tip` is refused with `SourceTip` | here |
| 3 | a checkpoint list with 156 entries is refused with `CheckpointCount` | here |
| 4 | a selection that is not the declared shape is refused with `SelectionShape` | here |
| 5 | a `blobs/` entry whose bytes do not match its oid or its digest is refused with `BlobIdentity` | the Rust reader's tests |
| 6 | a state whose tree needs an oid the forward map lacks is refused | the Rust reader's tests |
| 7 | oracle drift makes the row `INCOMPLETE` | Phase 3 (the golden table pins the digest) |
| 8 | an absent `--corpus` is refused with `CorpusMissing`, never defaulted | here |
| 9 | the corpus's hashes are unchanged after a full run | the campaign evidence |

Tests 1–4 and 8 build a **synthetic** corpus, because each refusal has to be
reachable on its own: against the real pin a wrong tip is unreachable, since the
manifest's SHA-256 refuses first. The synthetic documents are handed an explicit
expectation, which is why `identity` takes one.

    python3 core/benchmark/fs-bench-pro-storage-content/shared/test_history_corpus.py
"""

from __future__ import annotations

import hashlib
import json
import shutil
import tempfile
import unittest
from pathlib import Path

import history_corpus as hc


def build_corpus(
    root: Path,
    *,
    checkpoints: int = hc.CHECKPOINTS,
    tip: str = hc.SOURCE_TIP,
    oracle_entries: int = 3,
) -> str:
    """Writes a synthetic corpus and returns its manifest's SHA-256."""
    (root / "oracles").mkdir(parents=True, exist_ok=True)
    entries = []
    for index in range(1, checkpoints + 1):
        sha = f"{index:040x}"
        entries.append(
            {
                "index": index,
                "sha": sha,
                "tree": f"{index + 1000:040x}",
                "manifest_sha256": f"{index + 2000:064x}",
                "logical_bytes": 1000 + index,
                "files": 10 + index,
            }
        )
        oracle = {
            f"{index:02x}": ["40755", 0, "-"],
            f"{index:02x}2f61": ["100644", index, "ab" * 32],
        }
        while len(oracle) < oracle_entries:
            oracle[f"{index:02x}2f{len(oracle):02x}"] = ["100644", 1, "cd" * 32]
        (root / "oracles" / f"{sha}.json").write_text(json.dumps(oracle))
    document = {
        "schema": "deepseek-checkpoints-v1",
        "tip": tip,
        "selected_count": checkpoints,
        "reachable_commits": 15632,
        "total_logical_bytes": sum(entry["logical_bytes"] for entry in entries),
        "unique_blob_bytes": 1,
        "checkpoints": entries,
    }
    raw = json.dumps(document).encode()
    (root / "checkpoint-manifest.json").write_bytes(raw)
    return hashlib.sha256(raw).hexdigest()


class SyntheticCorpusTest(unittest.TestCase):
    """Tests 1–4 and 8: every refusal is reachable on its own."""

    def setUp(self) -> None:
        self.root = Path(tempfile.mkdtemp(prefix="history-corpus-"))
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)
        self.sha = build_corpus(self.root)

    @property
    def expectation(self) -> hc.Expectation:
        return hc.Expectation(sha=self.sha)

    def test_a_valid_corpus_authenticates(self) -> None:
        document = hc.identity(self.root, self.expectation)
        self.assertEqual(document["checkpoints"], hc.CHECKPOINTS)
        self.assertEqual(document["source_tip"], hc.SOURCE_TIP)
        self.assertEqual(document["manifest_sha256"], self.sha)

    def test_1_one_changed_byte_is_refused_with_manifest_identity(self) -> None:
        path = hc.manifest_path(self.root)
        raw = bytearray(path.read_bytes())
        raw[len(raw) // 2] ^= 0x01
        path.write_bytes(bytes(raw))
        with self.assertRaises(hc.CorpusError) as caught:
            hc.identity(self.root, self.expectation)
        self.assertIn("ManifestIdentity", str(caught.exception))

    def test_2_a_wrong_tip_is_refused_with_source_tip(self) -> None:
        # The synthetic SHA is the expectation, so the tip check is the one that
        # fires; against the real pin this branch is unreachable by construction.
        sha = build_corpus(self.root, tip="0" * 40)
        with self.assertRaises(hc.CorpusError) as caught:
            hc.identity(self.root, hc.Expectation(sha=sha))
        self.assertIn("SourceTip", str(caught.exception))

    def test_3_a_truncated_checkpoint_list_is_refused(self) -> None:
        sha = build_corpus(self.root, checkpoints=156)
        with self.assertRaises(hc.CorpusError) as caught:
            hc.identity(self.root, hc.Expectation(sha=sha))
        self.assertIn("CheckpointCount", str(caught.exception))

    def test_8_an_absent_corpus_is_refused_rather_than_defaulted(self) -> None:
        missing = self.root / "not-here"
        with self.assertRaises(hc.CorpusError) as caught:
            hc.identity(missing)
        self.assertIn("CorpusMissing", str(caught.exception))
        # And the module's own default is not substituted for an explicit path.
        self.assertNotEqual(str(missing), str(hc.DEFAULT_ROOT))

    def test_derive_counts_oracle_entries_not_manifest_files(self) -> None:
        # Erratum E1, on a corpus whose two numbers differ by construction.
        sha = build_corpus(self.root, oracle_entries=4)
        hc.identity(self.root, hc.Expectation(sha=sha))
        derived = hc.derive(self.root, "history-stride1", self.expectation)
        self.assertEqual(derived["states"], 157)
        self.assertEqual(derived["path_states"], 157 * 4)
        self.assertEqual(derived["manifest_files"], sum(range(11, 168)))
        self.assertNotEqual(derived["path_states"], derived["manifest_files"])

    def test_an_absent_oracle_is_refused(self) -> None:
        sha = build_corpus(self.root)
        expectation = hc.Expectation(sha=sha)
        hc.identity(self.root, expectation)
        next(iter((self.root / "oracles").iterdir())).unlink()
        with self.assertRaises(hc.CorpusError) as caught:
            hc.derive(self.root, "history-stride1", expectation)
        self.assertIn("OracleMissing", str(caught.exception))


class SelectionTest(unittest.TestCase):
    """Test 4, and the pins, without a corpus."""

    def test_the_three_selections_are_the_declared_indices(self) -> None:
        self.assertEqual(hc.selection("history-stride10"), tuple(sorted(set(range(1, 158, 10)) | {157})))
        self.assertEqual(hc.selection("history-stride3"), tuple(range(1, 158, 3)))
        self.assertEqual(hc.selection("history-stride1"), tuple(range(1, 158)))
        self.assertEqual(len(hc.selection("history-stride10")), 17)
        self.assertEqual(len(hc.selection("history-stride3")), 53)
        self.assertEqual(len(hc.selection("history-stride1")), 157)

    def test_4_a_selection_that_is_not_the_declared_shape_is_refused(self) -> None:
        for lane, wrong in (
            ("history-stride3", list(range(1, 158))),
            ("history-stride10", list(range(1, 158, 10))),
            ("history-stride1", list(range(1, 157))),
            ("history-stride3", list(reversed(list(range(1, 158, 3))))),
        ):
            with self.assertRaises(hc.CorpusError) as caught:
                hc.check_selection(lane, wrong)
            self.assertIn("SelectionShape", str(caught.exception))
        with self.assertRaises(hc.CorpusError):
            hc.selection("history-stride7")

    def test_pins_never_invent_a_zero(self) -> None:
        self.assertEqual(hc.pins("history-stride3")["canonical_objects"], 73_476)
        self.assertEqual(hc.pins("history-stride3")["canonical_bytes"], 589_423_458)
        self.assertEqual(hc.pins("history-stride1")["canonical_bytes"], 871_588_115)
        self.assertEqual(hc.pins("history-stride1")["canonical_objects"], 104_705)
        # stride10 has no recorded canonical total; `None` is not `0`.
        self.assertIsNone(hc.pins("history-stride10")["canonical_bytes"])
        self.assertIsNone(hc.pins("history-stride10")["canonical_objects"])
        self.assertEqual(hc.pins("no-such-lane"), {})
        # And the recorded pins are not silently missing.
        for lane in hc.SELECTIONS:
            for field in ("states", "path_states", "logical_bytes"):
                self.assertIsNotNone(hc.pins(lane)[field], f"{lane}.{field}")

    def test_the_declared_verification_ceilings_are_the_ruled_ones(self) -> None:
        self.assertEqual(hc.ceiling("history-stride10"), 10_000_000_000)
        self.assertEqual(hc.ceiling("history-stride3"), 20_000_000_000)
        self.assertEqual(hc.ceiling("history-stride1"), 30_000_000_000)
        self.assertIsNone(hc.ceiling("no-such-lane"))
        # Tighter than the 60 s contract default, which does not transfer.
        for lane in hc.SELECTIONS:
            self.assertLess(hc.ceiling(lane), 60_000_000_000)

    def test_the_storage_gate_is_v016s_recorded_bytes_at_all_three_sizes(self) -> None:
        self.assertEqual(hc.V016_ALLOCATED["history-stride10"], 49_344_512)
        self.assertEqual(hc.V016_ALLOCATED["history-stride3"], 64_024_576)
        self.assertEqual(hc.V016_ALLOCATED["history-stride1"], 83_947_520)
        self.assertEqual(set(hc.V016_ALLOCATED), set(hc.SELECTIONS))


class RealCorpusTest(unittest.TestCase):
    """The pins against the corpus, when it is present.

    A missing corpus is a **skip with its reason**, not a pass and not a failure:
    the corpus is a campaign input, and the 217-row self-check must still run on a
    machine that has never held it.
    """

    def setUp(self) -> None:
        if not hc.manifest_path(hc.DEFAULT_ROOT).is_file():
            self.skipTest(f"{hc.DEFAULT_ROOT} is absent")

    def test_the_corpus_authenticates_and_the_pins_reproduce(self) -> None:
        document = hc.identity(hc.DEFAULT_ROOT)
        self.assertEqual(document["manifest_sha256"], hc.MANIFEST_SHA)
        self.assertEqual(document["source_tip"], hc.SOURCE_TIP)
        self.assertEqual(document["checkpoints"], hc.CHECKPOINTS)
        for lane, pin in hc.PINS.items():
            derived = hc.derive(hc.DEFAULT_ROOT, lane)
            for field in ("states", "path_states", "logical_bytes"):
                self.assertEqual(derived[field], pin[field], f"{lane}.{field}")
            self.assertLess(derived["manifest_files"], derived["path_states"])

    def test_self_check_is_clean(self) -> None:
        self.assertEqual(hc.self_check(), [])


if __name__ == "__main__":
    unittest.main()
