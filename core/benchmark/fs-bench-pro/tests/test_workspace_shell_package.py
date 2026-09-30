"""Package scale oracle, exact registrations and deferred resource guard."""
import hashlib
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from families import workspace_shell_package as package
from families import workspace_namespace as namespace


class PackageOracle(unittest.TestCase):
    def test_full_large_bytes_and_untouched_package_files(self):
        cases = package.cases()
        case = next(row for row in cases.values() if row.original == "overwrite-4k-v1")
        old, new = package.files(case)
        self.assertEqual(len(new["large.bin"]), 10 << 20)
        self.assertEqual(new["large.bin"], b"A" * (5 << 20) + b"P" * 4096 + b"A" * ((5 << 20) - 4096))
        self.assertEqual({name: value for name, value in new.items() if name != "large.bin"},
                         {name: value for name, value in old.items() if name != "large.bin"})

    def test_refresh_absences_and_complete_recipe(self):
        case = next(row for row in package.cases().values() if row.original == "mixed-refresh-v1")
        old, new = package.files(case)
        self.assertEqual(len(old), 9)
        self.assertEqual(len(new), 9)
        self.assertNotIn("node_modules/@fixture/parser/lib/legacy.js", new)
        self.assertEqual(new["node_modules/@fixture/core/BUILD_ID"], b"2.0.0\n")
        self.assertTrue(new["node_modules/@fixture/ui/dist/ui.js"].startswith(b"export const ui = 2;\n"))

    def test_all_many_file_tiers_and_full_manifest_totals(self):
        counts = set()
        for case in package.cases().values():
            if not case.count or case.retained:
                continue
            counts.add(case.count)
            old, new = package.files(case)
            self.assertEqual(len(new) - len(old), case.count)
            self.assertEqual(sum(name.startswith("many/") for name in new), case.count)
            self.assertEqual(new[f"many/f{case.count-1}" if case.spread == 1 else f"many/d{(case.count-1)%17}/f{case.count-1}"], f"new-{case.count-1}".encode())
            rows = [line.split("\t") for line in package.oracle(case)[1].splitlines()]
            self.assertEqual(sum(int(row[3]) for row in rows), sum(map(len, new.values())))
            self.assertTrue(all(row[4] != "-" for row in rows if row[1] == "f"))
        self.assertEqual(counts, {128, 129, 257, 1025})

    def test_retained_g1_g2_pin_bytes_replacement_and_delete(self):
        case = next(row for row in package.cases().values() if row.retained)
        old, new = package.files(case)
        self.assertEqual(old["many/f0"], b"old-0")
        self.assertEqual(new["many/f0"], b"new-0")
        self.assertEqual(new["many/f127"], b"replacement")
        self.assertNotIn("many/f128", new)
        self.assertEqual(len(old) - len(new), 1)

    def test_frozen_order_budgets_and_deferred_case_cannot_acquire(self):
        cases = package.cases()
        self.assertEqual(len(cases), 8)
        self.assertEqual([row.original for row in list(cases.values())[:3]], ["mixed-refresh-v1", "overwrite-4k-v1", "repeated-one-byte-v1"])
        self.assertEqual([row.id for row in cases.values() if row.budget_ns == 25_000_000_000],
                         ["workspace-shell-package-mixed-refresh-sdk-v2", "workspace-shell-package-many-1025-sdk-v2"])
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary) / "out"
            common = SimpleNamespace(owned=lambda path: out, identities=lambda: {"source_dirty": False}, manifest_run=lambda path: None)
            with patch.object(namespace.shared, "build", side_effect=AssertionError("no resource acquisition")):
                package.run(package.DEFERRED, str(out), common)
            row = json.loads((out / "run.json").read_text())["rows"][0]
            self.assertEqual((row["status"], row["sample_count"]), ("NOT_RUN", 0))

    def test_package_binding_and_cursor_propagate_to_shared_runner(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "fixture.before").write_text("project_id=p\ngenesis_layer=g\nroot=r\nroot_serial=1\nbranch_id=b\n")
            master = {"path": str(root), "binding_key_hex": b"layerfs-bench-pro".hex()}
            self.assertEqual(namespace.fields(master, "case", "true")["binding_key_hex"], master["binding_key_hex"])
            self.assertEqual(namespace.env("45" * 32)["LAYERFS_HISTORY_CURSOR_KEY"], "45" * 32)


if __name__ == "__main__":
    unittest.main()
