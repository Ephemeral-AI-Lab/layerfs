#!/usr/bin/env python3
"""`unittest` entry points for `receipt.py`, wrapping its `self_check`.

Three rules live in that module and each one is tested at the boundary rather than
in the middle: a receipt is never overwritten, a lock is never silently stolen, and
a command that cannot fit is recorded `NOT_RUN` instead of being made to fit. The
budget tests sit exactly on each limit and one nanosecond past it, because that is
where an off-by-one would turn a miss into a pass.
"""

from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import receipt

HARNESS_ROOT = Path(__file__).resolve().parent.parent
# shared/ -> harness root -> benchmark/ -> core/ -> repository root, the same
# walk `invariants.py` performs.
REPO_ROOT = HARNESS_ROOT.parents[2]


class SelfCheckTest(unittest.TestCase):
    def test_the_module_self_check_is_clean(self) -> None:
        self.assertEqual(receipt.self_check(), [])


class AppendOnlyTest(unittest.TestCase):
    def test_a_json_receipt_is_written_once(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "receipt.json"
            receipt.write_append_only(path, {"a": 1})
            self.assertTrue(path.exists())
            with self.assertRaises(receipt.ReceiptError):
                receipt.write_append_only(path, {"a": 2})
            self.assertEqual(json.loads(path.read_text())["a"], 1)

    def test_a_text_report_is_written_once(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "report.txt"
            receipt.write_text_append_only(path, "first\n")
            with self.assertRaises(receipt.ReceiptError):
                receipt.write_text_append_only(path, "second\n")
            self.assertEqual(path.read_text(), "first\n")

    def test_no_partial_file_survives_a_refused_write(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "receipt.json"
            receipt.write_append_only(path, {"a": 1})
            with self.assertRaises(receipt.ReceiptError):
                receipt.write_append_only(path, {"a": 2})
            leftovers = sorted(entry.name for entry in Path(directory).iterdir())
            self.assertEqual(leftovers, ["receipt.json"])

    def test_the_parent_directory_is_created(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "nested" / "deeper" / "receipt.json"
            receipt.write_append_only(path, {"a": 1})
            self.assertTrue(path.exists())

    def test_a_failed_attempt_is_retained_rather_than_replaced(self) -> None:
        # The rule exists so a rerun cannot destroy the only witness there is.
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "attempt.json"
            receipt.write_append_only(path, {"status": "FAIL"})
            with self.assertRaises(receipt.ReceiptError):
                receipt.write_append_only(path, {"status": "PASS"})
            self.assertEqual(json.loads(path.read_text())["status"], "FAIL")


class BudgetTest(unittest.TestCase):
    def test_the_three_declared_limits_are_the_frozen_ones(self) -> None:
        self.assertEqual(receipt.COMPLETE_COMMAND_LIMIT_NS, 15_000_000_000)
        self.assertEqual(receipt.DECLARED_EXCEPTION_LIMIT_NS, 25_000_000_000)
        self.assertEqual(receipt.VERIFICATION_LIMIT_NS, 60_000_000_000)

    def test_the_complete_command_limit_is_inclusive(self) -> None:
        self.assertEqual(receipt.budget(14_000_000_000).status, "PASS")
        self.assertEqual(receipt.budget(15_000_000_000).status, "PASS")
        self.assertEqual(receipt.budget(15_000_000_001).status, "NOT_RUN")

    def test_the_declared_exception_limit_is_separate_and_inclusive(self) -> None:
        self.assertEqual(receipt.budget(20_000_000_000).status, "NOT_RUN")
        self.assertEqual(receipt.budget(20_000_000_000, declared_exception=True).status, "PASS")
        self.assertEqual(receipt.budget(25_000_000_000, declared_exception=True).status, "PASS")
        self.assertEqual(receipt.budget(25_000_000_001, declared_exception=True).status, "NOT_RUN")

    def test_a_missed_budget_records_the_measured_wall_time_and_the_reason(self) -> None:
        missed = receipt.budget(29_620_000_000, declared_exception=True)
        self.assertEqual(missed.status, "NOT_RUN")
        self.assertEqual(missed.wall_ns, 29_620_000_000)
        self.assertEqual(missed.limit_ns, receipt.DECLARED_EXCEPTION_LIMIT_NS)
        self.assertIn("29.620 s", missed.reason)
        self.assertIn("never shrunk to fit", missed.reason)

    def test_the_budget_classifies_the_formula_and_not_the_raw_wall(self) -> None:
        """`CONTRACT.md` section 4 fixes the budgeted quantity as a formula.

        The wall is still published, and it is no longer what decides: a case whose
        four phases are well inside the limit is inside it however slowly this host
        forks. The allowance is declared and constant, so the outcome does not move
        with process-start jitter.
        """
        # Four phases summing to 14.5 s, on a host whose process lifecycle cost 0.4 s:
        # the wall alone would be NOT_RUN, the formula is PASS.
        formula = receipt.budget(14_900_000_000, declared_ns=14_500_000_000)
        self.assertEqual(formula.status, "PASS")
        self.assertEqual(formula.budgeted_ns, 14_500_000_000 + receipt.LIFECYCLE_ALLOWANCE_NS)
        self.assertEqual(formula.declared_ns, 14_500_000_000)
        self.assertEqual(formula.lifecycle_allowance_ns, receipt.LIFECYCLE_ALLOWANCE_NS)
        self.assertEqual(formula.wall_ns, 14_900_000_000)
        self.assertIn("declared phases", formula.reason)

        # Exactly on the limit passes; one nanosecond past it does not.
        on = receipt.COMPLETE_COMMAND_LIMIT_NS - receipt.LIFECYCLE_ALLOWANCE_NS
        self.assertEqual(receipt.budget(0, declared_ns=on).status, "PASS")
        self.assertEqual(receipt.budget(0, declared_ns=on + 1).status, "NOT_RUN")

        # A caller with no phases to hand classifies the wall, which is what every
        # receipt written before the formula had.
        self.assertEqual(receipt.budget(14_000_000_000).budgeted_ns, 14_000_000_000)
        self.assertEqual(receipt.budget(14_000_000_000).lifecycle_allowance_ns, 0)
        self.assertEqual(receipt.budget(15_000_000_001).status, "NOT_RUN")

        # The declared-exception ladder is unchanged and still applies to the formula.
        self.assertEqual(
            receipt.budget(0, declared_exception=True, declared_ns=25_000_000_000).status,
            "NOT_RUN",
        )
        self.assertEqual(
            receipt.budget(
                0,
                declared_exception=True,
                declared_ns=25_000_000_000 - receipt.LIFECYCLE_ALLOWANCE_NS,
            ).status,
            "PASS",
        )

    def test_a_budget_outcome_renders_the_fields_a_receipt_needs(self) -> None:
        fields = receipt.budget(1_000_000_000).as_fields()
        self.assertEqual(
            sorted(fields),
            [
                "budgeted_ns",
                "declared_exception",
                "declared_ns",
                "lifecycle_allowance_ns",
                "limit_ns",
                "reason",
                "status",
                "wall_ns",
            ],
        )

    def test_verification_has_its_own_sixty_second_limit(self) -> None:
        self.assertEqual(receipt.verification_budget(59_000_000_000).status, "PASS")
        self.assertEqual(receipt.verification_budget(60_000_000_000).status, "PASS")
        self.assertEqual(receipt.verification_budget(60_000_000_001).status, "INCOMPLETE")
        self.assertEqual(
            receipt.verification_budget(60_000_000_001).limit_ns,
            receipt.VERIFICATION_LIMIT_NS,
        )

    def test_a_zero_length_command_fits(self) -> None:
        self.assertEqual(receipt.budget(0).status, "PASS")


class MeasurementLockTest(unittest.TestCase):
    def test_the_lock_is_exclusive_and_released(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "measurement.lock"
            with receipt.measurement_lock(path):
                self.assertTrue(path.exists())
                with self.assertRaises(receipt.ReceiptError):
                    with receipt.measurement_lock(path):
                        self.fail("the measurement lock was taken twice")
            self.assertFalse(path.exists(), "the lock was not released")

    def test_a_stale_lock_is_reported_and_not_stolen(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "measurement.lock"
            path.write_text("pid=999999 since=2026-01-01T00:00:00Z\n", encoding="utf-8")
            with self.assertRaises(receipt.ReceiptError) as raised:
                with receipt.measurement_lock(path):
                    self.fail("a held lock must not be acquired")
            self.assertIn("pid=999999", str(raised.exception))
            self.assertTrue(path.exists(), "a lock this process does not hold must not be removed")
            self.assertEqual(
                path.read_text(encoding="utf-8"),
                "pid=999999 since=2026-01-01T00:00:00Z\n",
            )


class DigestTest(unittest.TestCase):
    def test_file_and_byte_digests_agree(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "payload.bin"
            payload = bytes(range(256)) * 8192  # 2 MiB, so the streaming loop runs twice
            path.write_bytes(payload)
            self.assertEqual(receipt.sha256_file(path), receipt.sha256_bytes(payload))
            self.assertEqual(receipt.sha256_bytes(payload), hashlib.sha256(payload).hexdigest())
            self.assertEqual(len(receipt.sha256_file(path)), 64)

    def test_an_empty_file_hashes_to_the_empty_digest(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "empty.bin"
            path.write_bytes(b"")
            self.assertEqual(
                receipt.sha256_file(path),
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            )


class IdentityTest(unittest.TestCase):
    def test_identity_names_the_tree_the_run_would_use(self) -> None:
        identity = receipt.identify(REPO_ROOT, HARNESS_ROOT, None)
        fields = identity.as_fields()
        self.assertEqual(len(fields["source_commit"]), 40)
        int(fields["source_commit"], 16)
        self.assertIsNone(fields["harness_binary_sha256"])
        self.assertEqual(
            fields["product_lock_sha256"],
            receipt.sha256_file(REPO_ROOT / "core" / "Cargo.lock"),
        )
        self.assertEqual(
            fields["harness_lock_sha256"],
            receipt.sha256_file(HARNESS_ROOT / "Cargo.lock"),
        )
        self.assertEqual(
            fields["registry_tsv_sha256"],
            receipt.sha256_file(HARNESS_ROOT / "tests" / "golden" / "registry.tsv"),
        )
        self.assertIn("construction_workers", fields)
        self.assertTrue(fields["platform"])
        self.assertTrue(fields["python"])


if __name__ == "__main__":
    unittest.main()
