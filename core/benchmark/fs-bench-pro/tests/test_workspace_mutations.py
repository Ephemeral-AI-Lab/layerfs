"""Independent mixed-mutation oracle, failure semantics and proof registration."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import unittest
import tempfile
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from families import workspace_mutations as mutations
from families import workspace_namespace as namespace


def rows(text):
    return {parts[0]: parts[1:] for parts in (line.split("\t") for line in text.splitlines())}


class MutationOracle(unittest.TestCase):
    def test_retired_dirty_discard_method_does_not_acquire_resources(self):
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary) / "out"
            common = SimpleNamespace(owned=lambda path: out, identities=lambda: {}, manifest_run=lambda path: None)
            with patch.object(namespace, "run", side_effect=AssertionError("retired method cannot run")):
                mutations.run("workspace-mutations-shell-exit7-no-commit-sdk-v1", str(out), common)
            row = json.loads((out / "run.json").read_text())["rows"][0]
            self.assertEqual((row["status"], row["sample_count"]), ("NOT_RUN", 0))

    def test_explicit_recovery_keeps_exec_failure_and_requires_new_head_in_proof(self):
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            case = mutations.SDK["workspace-mutations-shell-exit7-retained-explicit-commit-sdk-v2"]
            driver = {"status": "COMPLETE", "head_commit": "new", "commit_called": True, "exec_exit_status": 7,
                      "unmount_ok": True, "sandbox_delete_ok": True, "projection_counts": "rename=0"}
            control = {"recovery_complete": True, "unmounted_before_recovery": True, "pin_observation_ok": True,
                       "pinned_bytes": 7, "pinned_sha256": hashlib.sha256(b"private").hexdigest(), "pin_release_ok": True}
            stdout = ("RECEIPT\t" + json.dumps(driver) + "\nCONTROL\t" + json.dumps(control) + "\n").encode()
            answer = ({"exit_code": 0, "wall_ns": 1, "timeout": False}, stdout, b"")
            prepared = {"identity": {}, "image_id": "sealed", "artifacts": {"benchmark_shell": {"path": "never executed"}}}
            with patch.object(namespace, "copy_master"), patch.object(namespace, "fields", return_value={"expected_failure": "0"}), patch.object(namespace.shared, "execute", return_value=answer):
                receipt = mutations.attempt(out, case, {"old_commit": "old"}, prepared)
            self.assertEqual(receipt["status"], "COMPLETE_DIAGNOSTIC")
            self.assertTrue(receipt["expected_exec_failure"])
            self.assertFalse(receipt["expected_failure"])
            values = dict(line.split("=", 1) for line in (out / case.id / "case.verify").read_text().splitlines())
            before = dict(line.split("=", 1) for line in (out / case.id / "case.before").read_text().splitlines())
            self.assertEqual((before["expected_failure"], values["expected_failure"]), ("1", "0"))

    def test_nonadvancing_proof_only_passes_the_explicit_no_commit_case(self):
        for expected_failure, advanced, wanted in ((False, False, "FAIL"), (True, False, "PASS"), (True, True, "FAIL")):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                run, out = root / "run", root / "proof"
                folder = run / "case"
                folder.mkdir(parents=True)
                (root / ".run.lock").touch()
                binary = root / "verifier"
                binary.write_bytes(b"sealed verifier")
                receipt = {"status": "COMPLETE_DIAGNOSTIC", "expected_failure": expected_failure,
                           "artifacts": {"verify_checkpoint5": {"path": str(binary), "sha256": hashlib.sha256(binary.read_bytes()).hexdigest()}}}
                (folder / "receipt.json").write_text(json.dumps(receipt))
                (run / "run.json").write_text(json.dumps({"identity": {}, "rows": [{"case": "case"}]}))
                common = SimpleNamespace(owned=lambda path: out, RESULTS=root,
                                         verify_run_manifest=lambda path: None, manifest_run=lambda path: None)
                answer = ({"exit_code": 0, "wall_ns": 1}, json.dumps({"status": "PASS", "advanced": advanced}).encode(), b"")
                with patch.object(namespace.shared, "execute", return_value=answer):
                    namespace.prove(run, str(out), common)
                self.assertEqual(json.loads((out / "run.json").read_text())["rows"][0]["status"], wanted)

    def test_write_shrink_extend_has_exact_zero_tail_and_hardlink_oracle(self):
        old, new = map(rows, mutations.oracle(mutations.SDK["workspace-mutations-mixed-ordinary-sdk-v1"]))
        expected = b"abXYZf\0\0\0\0"
        self.assertEqual(len(old), 9)
        self.assertEqual(len(new), 13)
        self.assertEqual(new["work/file"], ["f", "384", "10", hashlib.sha256(expected).hexdigest()])
        self.assertEqual(new["work/file"], new["work/moved"])
        self.assertEqual(new["work/link2"], ["l", "511", "4", hashlib.sha256(b"file").hexdigest()])
        self.assertNotIn("work/empty", new)
        self.assertNotIn("work/remove", new)
        self.assertEqual(new["packages/old/subtree/sibling.txt"][-1], hashlib.sha256(b"sibling-new").hexdigest())

    def test_retained_g1_and_later_g2_have_separate_bytes_names_and_metadata(self):
        case = mutations.SDK["workspace-mutations-retained-g1-live-g2-sdk-v1"]
        old, new = map(rows, mutations.oracle(case))
        self.assertIn("work/moved", old)
        self.assertNotIn("work/moved", new)
        self.assertIn("work/link3", new)
        self.assertNotIn("work/link2", new)
        self.assertEqual(old["work/file"][-1], hashlib.sha256(b"abXYZf\0\0\0\0").hexdigest())
        self.assertEqual(new["work/file"][-1], hashlib.sha256(b"QQXYZf\0\0\0\0").hexdigest())
        properties = mutations.properties(case)
        self.assertEqual(bytes.fromhex(properties["old_alias_pairs_hex"]), b"work/file\twork/moved\t2")
        self.assertEqual(bytes.fromhex(properties["alias_pairs_hex"]), b"work/file\twork/file\t1")
        self.assertEqual(bytes.fromhex(properties["mtime_oracle_hex"]), b"work/file\t1578020645\t0")

    def test_known_failed_shell_leaves_canonical_head_oracle_unchanged(self):
        case = mutations.SDK["workspace-mutations-shell-exit7-no-commit-sdk-v1"]
        old, new = mutations.oracle(case)
        self.assertEqual(old, new)
        self.assertNotIn("uncommitted", new)
        self.assertEqual(mutations.properties(case), {"expected_failure": "1", "expected_exit_status": "7"})

    def test_explicit_recovery_is_a_distinct_profile_with_accepted_private_bytes(self):
        case = mutations.SDK["workspace-mutations-shell-exit7-retained-explicit-commit-sdk-v2"]
        old, new = map(rows, mutations.oracle(case))
        self.assertNotIn("uncommitted/file", old)
        self.assertEqual(new["uncommitted/file"][-1], hashlib.sha256(b"private").hexdigest())
        self.assertEqual(mutations.properties(case)["recovery_commit"], "1")
        self.assertNotIn("workspace-mutations-shell-exit7-no-commit-sdk-v1", mutations.SELECTED)

    def test_refused_namespace_operations_add_no_forbidden_bindings(self):
        _, new = map(rows, mutations.oracle(mutations.SDK["workspace-mutations-known-posix-refusals-sdk-v1"]))
        self.assertIn("checks/child/file", new)
        self.assertIn("checks/result", new)
        self.assertNotIn("checks/dirlink", new)
        self.assertNotIn("checks/child/nested", new)

    def test_fixed_membership_limits_and_reused_proof_identities(self):
        self.assertEqual(len(mutations.SDK), 5)
        self.assertEqual(len(mutations.SELECTED), 4)
        self.assertEqual(len(mutations.NATIVE), 3)
        self.assertTrue(all(case.budget_ns == 15_000_000_000 for case in (*mutations.SDK.values(), *mutations.NATIVE.values())))
        self.assertEqual(sum(case.clones for case in mutations.NATIVE.values()), 4)
        for reused in mutations.REUSED.values():
            self.assertEqual(len(reused["source_commit"]), 40)
            self.assertEqual(len(reused["compact_receipt_sha256"]), 64)
            self.assertEqual(len(reused["raw_manifest_sha256"]), 64)

    def test_standalone_shell_helper_import_remains_valid(self):
        result = subprocess.run([sys.executable, "-c", "import shell_package; assert callable(shell_package.case_spec)"],
                                cwd=Path(__file__).resolve().parents[1], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
