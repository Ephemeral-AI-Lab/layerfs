#!/usr/bin/env python3
"""`unittest` entry points for `copyladder.py`, wrapping its `self_check`.

The ladder exists to make one refusal unavoidable: **R1 is forbidden wherever
`st_blocks` gates**, because a COW clone's allocated blocks are shared with its
master and `store_allocated_bytes` would double-count them. A warning would be
ignorable, so the module raises; these tests pin that it raises for both reasons
it can (the family is allocation-gated, or the row itself gates allocated bytes)
and that it still permits R1 where nothing gates.
"""

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import copyladder


class SelfCheckTest(unittest.TestCase):
    def test_the_module_self_check_is_clean(self) -> None:
        self.assertEqual(copyladder.self_check(), [])


class RungSelectionTest(unittest.TestCase):
    def test_every_setup_token_means_the_frozen_rung(self) -> None:
        expected = {
            "clone": copyladder.R2,
            "copy": copyladder.R2,
            "reflink": copyladder.R1,
            "master": copyladder.R0,
            "fresh": copyladder.R3,
            "regenerate": copyladder.R3,
        }
        self.assertEqual(copyladder.SETUP_TOKENS, expected)
        for token, rung in expected.items():
            self.assertEqual(copyladder.rung_for_setup(token, "c1.edit", False), rung)

    def test_the_rung_tokens_are_the_declared_four(self) -> None:
        self.assertEqual(
            copyladder.RUNGS,
            (
                "read-only-master-mmap-no-copy",
                "apfs-clonefile-cow-v1",
                "closed-quiescent-byte-copy",
                "regenerated-in-process",
            ),
        )

    def test_reflink_is_refused_for_an_allocation_gated_family(self) -> None:
        for family in copyladder.ALLOCATION_GATED_FAMILIES:
            with self.assertRaises(copyladder.LadderError):
                copyladder.rung_for_setup("reflink", family, False)

    def test_reflink_is_refused_when_the_row_gates_allocated_bytes(self) -> None:
        with self.assertRaises(copyladder.LadderError):
            copyladder.rung_for_setup("reflink", "c1.edit.length-preserving", True)

    def test_reflink_is_allowed_where_nothing_gates_allocated_bytes(self) -> None:
        self.assertEqual(
            copyladder.rung_for_setup("reflink", "c1.edit.length-preserving", False),
            copyladder.R1,
        )

    def test_an_unknown_setup_token_is_refused(self) -> None:
        with self.assertRaises(copyladder.LadderError):
            copyladder.rung_for_setup("nonsense", "c1.edit", False)


class EnospcPreflightTest(unittest.TestCase):
    def test_the_declared_factor_is_the_frozen_one(self) -> None:
        self.assertEqual(copyladder.ENOSPC_FACTOR, 1.05)

    def test_an_impossible_request_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(copyladder.LadderError):
                copyladder.preflight_enospc(directory, 1 << 62)

    def test_a_possible_request_returns_the_free_bytes_it_measured(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            available = copyladder.preflight_enospc(directory, 4096)
            self.assertGreater(available, 4096)
            self.assertEqual(available, copyladder.free_bytes(directory))

    def test_the_preflight_demands_a_margin_over_the_master(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            free = copyladder.free_bytes(directory)
            # Just over what is free, before the 1.05 factor is even applied.
            with self.assertRaises(copyladder.LadderError):
                copyladder.preflight_enospc(directory, free)


class AcquisitionTest(unittest.TestCase):
    def master(self, directory: str, payload: bytes = b"layerfs" * 4096) -> Path:
        path = Path(directory) / "master.bin"
        path.write_bytes(payload)
        return path

    def test_a_byte_copy_preserves_the_artifact_and_declares_r2(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            master = self.master(directory)
            acquisition = copyladder.acquire(
                copyladder.R2, master, Path(directory) / "copy.bin", False, "c1.edit"
            )
            self.assertEqual(acquisition.rung, copyladder.R2)
            self.assertEqual(acquisition.master_bytes, acquisition.destination_bytes)
            self.assertEqual((Path(directory) / "copy.bin").read_bytes(), master.read_bytes())
            self.assertEqual(acquisition.attribution, "exclusive")
            fields = acquisition.as_fields()
            self.assertEqual(fields["clone_method"], copyladder.R2)
            self.assertFalse(fields["master_path_released_to_sample"])
            self.assertEqual(fields["allocation_attribution"], "exclusive")

    def test_the_master_is_never_written(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            master = self.master(directory)
            before = master.read_bytes()
            copyladder.acquire(copyladder.R2, master, Path(directory) / "copy.bin", False, "c1.edit")
            self.assertEqual(master.read_bytes(), before)

    def test_r0_is_the_master_itself_and_is_shared_with_it(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            master = self.master(directory)
            acquisition = copyladder.acquire(copyladder.R0, master, master, False, "c1.edit")
            self.assertEqual(acquisition.rung, copyladder.R0)
            self.assertEqual(acquisition.attribution, "shared-with-master")
            self.assertEqual(acquisition.destination, str(master))

    def test_acquire_refuses_r1_for_an_allocation_gated_row(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            master = self.master(directory)
            with self.assertRaises(copyladder.LadderError):
                copyladder.acquire(
                    copyladder.R1, master, Path(directory) / "clone.bin", True, "c2.footprint"
                )
            self.assertFalse((Path(directory) / "clone.bin").exists())

    def test_acquire_refuses_r3_because_it_is_not_a_copy_step(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            master = self.master(directory)
            with self.assertRaises(copyladder.LadderError):
                copyladder.acquire(copyladder.R3, master, Path(directory) / "fresh.bin", False, "c1.edit")

    def test_acquire_refuses_an_unknown_rung_and_a_missing_master(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            master = self.master(directory)
            with self.assertRaises(copyladder.LadderError):
                copyladder.acquire("r9", master, Path(directory) / "x.bin", False, "c1.edit")
            with self.assertRaises(copyladder.LadderError):
                copyladder.acquire(
                    copyladder.R2, Path(directory) / "absent.bin", Path(directory) / "x.bin", False, "c1.edit"
                )

    @unittest.skipUnless(hasattr(copyladder._libc, "clonefile"), "clonefile(2) is unavailable")
    def test_a_clone_carries_the_master_content_and_declares_r1(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            master = self.master(directory)
            target = Path(directory) / "clone.bin"
            acquisition = copyladder.acquire(copyladder.R1, master, target, False, "c1.edit")
            self.assertEqual(acquisition.rung, copyladder.R1)
            self.assertEqual(acquisition.attribution, "shared-with-master")
            self.assertEqual(target.read_bytes(), master.read_bytes())
            self.assertEqual(
                acquisition.as_fields()["allocation_attribution"], "shared-with-master"
            )


if __name__ == "__main__":
    unittest.main()
