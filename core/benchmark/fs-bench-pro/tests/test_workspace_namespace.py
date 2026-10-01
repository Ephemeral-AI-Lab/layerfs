"""Independent Family5 corpus and bounded registry assertions."""
import hashlib
import json
import tempfile
from types import SimpleNamespace
from unittest.mock import patch
from pathlib import Path
import sys
import subprocess
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from families import workspace_namespace as namespace


def rows(text):
    return {parts[0]: parts[1:] for parts in (line.split("\t") for line in text.splitlines())}


class NamespaceOracle(unittest.TestCase):
    def test_shared_shell_helper_imports_before_runner(self):
        module_root = Path(__file__).resolve().parents[1]
        result = subprocess.run([sys.executable, "-c", "import shell_package; assert callable(shell_package.case_spec)"],
                                cwd=module_root, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_move_preserves_complete_extra_corpus_and_replaces_one_file(self):
        old, new = map(rows, namespace.oracle(namespace.SDK["workspace-namespace-move-replace-descendants-67-sdk-v1"]))
        self.assertEqual(len(old), 73)
        self.assertEqual(len(old), len(new))
        self.assertNotIn("packages/old/subtree", new)
        for index in range(64):
            before = f"packages/old/subtree/child/extra{index:03}.txt"
            after = before.replace("packages/old", "packages/new")
            self.assertEqual(old[before], new[after])
        self.assertEqual(new["packages/new/subtree/child/grand.txt"][-1], hashlib.sha256(b"grand-new").hexdigest())

    def test_deep_component_oracles_exceed_both_path_bounds(self):
        old, new = map(rows, namespace.oracle(namespace.SDK["workspace-namespace-deep-4097-sdk-v1"]))
        leaf = next(path for path in new if path.endswith("/leaf"))
        self.assertEqual(len(leaf), 4097)
        self.assertEqual(new[leaf][-1], hashlib.sha256(b"deep-new").hexdigest())
        self.assertLess(max(map(len, old)), 4096)
        _, new = map(rows, namespace.oracle(namespace.SDK["workspace-namespace-components-270-sdk-v1"]))
        leaf = next(path for path in new if path.endswith("/file"))
        self.assertEqual(leaf.count("/"), 270)
        self.assertEqual(new[leaf][-1], hashlib.sha256(b"leaf").hexdigest())

    def test_retained_g1_is_moved_old_bytes_with_same_new_namespace(self):
        case = namespace.SDK["workspace-namespace-retained-g1-replace-sdk-v1"]
        old, new = map(rows, namespace.oracle(case))
        self.assertEqual(set(old), set(new))
        name = "packages/new/subtree/child/grand.txt"
        self.assertEqual(old[name][-1], hashlib.sha256(b"grand-base").hexdigest())
        self.assertEqual(new[name][-1], hashlib.sha256(b"grand-new").hexdigest())

    def test_host_master_calls_public_init_seed_and_full_oracle_only(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            common = SimpleNamespace(RESULTS=root / "results")
            common.RESULTS.mkdir()
            artifacts = {name: {"path": name} for name in
                         ("benchmark_init", "benchmark_shell", "verify_checkpoint5")}
            artifacts.update(image_id="sha256:image", identity={"source_commit": "control"})
            calls = []

            def execute(command, folder, **kwargs):
                calls.append(command)
                record = {"exit_code": 0, "timeout": False, "wall_ns": 1}
                if command[0] == "benchmark_init":
                    Path(command[2]).write_bytes(b"host store")
                    Path(command[3]).write_bytes(b"host history")
                    reply = {"status": "COMPLETE", "project_id": "project", "genesis_layer": "layer",
                             "root": "root", "root_serial": 1}
                    return record, json.dumps(reply).encode(), b""
                if command[0] == "benchmark_shell":
                    self.assertEqual(command[1], "seed")
                    return record, b'RECEIPT\t{"status":"COMPLETE","head_commit":"head","branch_id":"branch"}\n', b""
                self.assertEqual(command[0], "verify_checkpoint5")
                return record, b'{"status":"PASS"}', b""

            with patch.object(namespace.shared, "execute", side_effect=execute), patch.object(
                    namespace.commit, "cleanup_complete", return_value=True):
                result = namespace.sdk_master(root, "wide", common, artifacts)
                reused = namespace.sdk_master(root, "wide", common, artifacts)
            self.assertEqual(len(calls), 3)
            self.assertEqual([call[0] for call in calls],
                             ["benchmark_init", "benchmark_shell", "verify_checkpoint5"])
            self.assertEqual(result["provenance"], "host-public-sdk-init-and-seed-v2")
            self.assertTrue(reused["reuse"])
            source = Path(result["path"]) / "source"
            for name, value in namespace.tree("wide").items():
                self.assertTrue((source / name).is_dir() if value is None else (source / name).read_bytes() == value)
            self.assertFalse((source / ".marker").exists())
            self.assertEqual((Path(result["path"]) / "old.tsv").read_text(),
                             namespace.manifest({**namespace.tree("wide"), ".marker": b"baseline"}))

    def test_unsupported_host_layout_refuses_before_native_master_access(self):
        with patch.object(namespace.shared, "execute") as execute:
            with self.assertRaisesRegex(ValueError, "not qualified"):
                namespace.sdk_master(None, "deep", None, None)
            execute.assert_not_called()

    def test_complete_selection_and_prospective_budgets(self):
        self.assertEqual(len(namespace.NATIVE), 10)
        self.assertEqual(len(namespace.SDK), 5)
        self.assertTrue(all(case.budget_ns == 15_000_000_000 for case in namespace.NATIVE.values()))
        exceptions = {case.id for case in namespace.SDK.values() if case.budget_ns == 25_000_000_000}
        self.assertEqual(exceptions, {"workspace-namespace-deep-4097-sdk-v1", "workspace-namespace-components-270-sdk-v1"})


if __name__ == "__main__":
    unittest.main()
