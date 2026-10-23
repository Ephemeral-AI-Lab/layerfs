import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ci_tests  # noqa: E402


class SelectionAgainstTheTree(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.meta = ci_tests.metadata()

    def test_every_slow_target_exists(self):
        self.assertEqual(ci_tests.missing_slow(self.meta), [])

    def test_mount_group_is_the_linux_only_daemon_targets(self):
        mount = ci_tests.mount_targets(self.meta)
        names = {name for _, name in mount}
        self.assertIn("forced_unavailable", names)
        self.assertIn("kernel_application", names)
        self.assertNotIn("lane_merge", names)
        self.assertTrue(all(package == "layerfs-daemon" for package, _ in mount))

    def test_fast_excludes_both_nightly_groups_and_slow_selects_only_slow(self):
        fast = ci_tests.filter_for("fast", self.meta)
        slow = ci_tests.filter_for("slow", self.meta)
        self.assertTrue(fast.startswith("not ("))
        self.assertIn("binary_id(layerfs-api::scale_lineage_cap)", fast)
        self.assertIn("binary_id(layerfs-daemon::forced_unavailable)", fast)
        self.assertNotIn("forced_unavailable", slow)
        self.assertEqual(slow.count("binary_id("), len(ci_tests.SLOW))


class ExecutablesFromCargoJson(unittest.TestCase):
    def test_keeps_only_mount_test_binaries(self):
        meta = {"packages": [{"name": "layerfs-daemon",
                              "manifest_path": "/w/core/crates/layerfs-daemon/Cargo.toml"}]}

        def artifact(name, kind, exe):
            return json.dumps({
                "reason": "compiler-artifact",
                "manifest_path": "/w/core/crates/layerfs-daemon/Cargo.toml",
                "target": {"name": name, "kind": [kind]},
                "executable": exe,
            })

        lines = [
            "not json",
            artifact("mounted_commit", "test", "/t/mounted_commit-1"),
            artifact("lane_merge", "test", "/t/lane_merge-1"),
            artifact("layerfs-daemon", "bin", "/t/layerfs-daemon"),
        ]
        found = list(ci_tests.executables(
            lines, [("layerfs-daemon", "mounted_commit")], meta))
        self.assertEqual(found, [(Path("/w/core/crates/layerfs-daemon"), "/t/mounted_commit-1")])


if __name__ == "__main__":
    unittest.main()
