"""The nine registered schedules and the frozen independent master."""

import importlib.util
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch


HERE = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("workspace_write", HERE / "families/workspace_write.py")
write = importlib.util.module_from_spec(SPEC)
import sys
sys.modules[SPEC.name] = write
SPEC.loader.exec_module(write)


class WorkspaceWriteRegistry(unittest.TestCase):
    def test_removed_mechanism_records_every_selected_case_without_build(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            out = root / "out"
            common = SimpleNamespace(ROOT=root, owned=lambda path: out,
                identities=lambda: {"source_dirty": True}, manifest_run=lambda path: None)
            with patch.object(write, "ROOT", root), patch.object(write, "build", side_effect=AssertionError("no build")):
                self.assertEqual(write.run("workspace_write", str(out), common), out)
            summary = json.loads((out / "run.json").read_text())
            self.assertEqual(summary["reason"], "mechanism removed")
            self.assertEqual([row["case"] for row in summary["rows"]], list(write.SELECTED))
            self.assertTrue(all(row["status"] == "NOT_RUN" and row["sample_count"] == 0 for row in summary["rows"]))

    def test_removed_mechanism_build_guard_precedes_residency_and_target(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            common = SimpleNamespace(target_path=lambda: self.fail("no Cargo metadata"))
            with patch.object(write, "ROOT", root), patch.object(write.residency, "self_check", side_effect=AssertionError("no residency preparation")):
                with self.assertRaisesRegex(RuntimeError, "NOT_RUN.*mechanism removed"):
                    write.build(root / "out", common, {})
            self.assertFalse((root / "out").exists())

    def test_compatible_historical_source_keeps_the_frozen_route(self):
        from shared import retired_families
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = root / "core/crates/layerfs-server/Cargo.toml"
            manifest.parent.mkdir(parents=True)
            manifest.write_text('[package]\nname = "layerfs-server"\n')
            self.assertTrue(retired_families.compatible(root))
            retired_families.require_compatible(root)
            manifest.write_text('[package]\nname = "unrelated"\n')
            self.assertFalse(retired_families.compatible(root))

    def test_cold_source_and_direct_io_do_not_qualify_complete_commit(self):
        row = {"family_id": "workspace_write", "cache_contract": write.CACHE_CONTRACT,
               "source_cache_status": "PASS", "host_disk_read_bytes": 712704,
               "device_read_floor_bytes": 641433, "performance_budget_status": "PASS"}
        result = write.assess_numeric_cache(row)
        self.assertEqual(result["status"], "INELIGIBLE")
        self.assertIn("Commit cache domain", result["reason"])

    def test_nine_schedules(self):
        self.assertEqual(len(write.CASES), 9)
        self.assertEqual(tuple(write.CASES), write.SELECTED)
        manifests = write.manifests()
        self.assertEqual(len(manifests), 10)
        for case in write.CASES.values():
            data = write.expected(case.pattern, case.writes)
            self.assertEqual(len(data), write.SIZE + (case.writes if case.pattern == "append" else 0))
            self.assertIn(str(len(data)), manifests[case.id])
            self.assertEqual(case.command_budget_ns, (25 if case.writes == 4097 else 15) * 1_000_000_000)
        self.assertEqual(write.expected("repeated", 4097)[5 << 20], ord("B") + 4096 % 24)


if __name__ == "__main__":
    unittest.main()
