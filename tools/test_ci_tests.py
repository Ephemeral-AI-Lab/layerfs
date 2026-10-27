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

    def test_every_listed_target_exists_and_groups_do_not_overlap(self):
        self.assertEqual(ci_tests.missing_listed(self.meta), [])
        mount = set(ci_tests.mount_targets(self.meta))
        self.assertFalse(set(ci_tests.SLOW) & set(ci_tests.HEAVY))
        self.assertFalse(ci_tests.NIGHTLY & mount)

    def test_fast_targets_leave_out_every_nightly_and_mount_target(self):
        fast = set(ci_tests.fast_targets(self.meta))
        self.assertFalse(fast & (ci_tests.NIGHTLY | set(ci_tests.mount_targets(self.meta))))
        self.assertIn(("layerfs-overlay", "captured_namespace"), fast)
        self.assertNotIn(("layerfs-workspace", "captured_namespace"), fast)
        flags = ci_tests.test_flags(ci_tests.lane_targets("fast", self.meta)).split()
        self.assertEqual(flags.count("--test"), len({name for _, name in fast}))
        self.assertNotIn("mount_control", flags)

    def test_mount_group_includes_moved_sdk_targets(self):
        mount = ci_tests.mount_targets(self.meta)
        names = {name for _, name in mount}
        self.assertIn("forced_unavailable", names)
        self.assertIn("kernel_application", names)
        self.assertNotIn("lane_merge", names)
        self.assertIn(("layerfs-api", "kernel_application"), mount)
        self.assertIn(("layerfs-api", "observed_application"), mount)
        self.assertIn(("layerfs-daemon", "forced_unavailable"), mount)

    def test_fast_filter_excludes_the_nightly_groups_and_nightly_selects_them(self):
        fast = ci_tests.filter_for("fast", self.meta)
        nightly = ci_tests.filter_for("nightly", self.meta)
        self.assertTrue(fast.startswith("not ("))
        self.assertIn("binary_id(layerfs-api::scale_lineage_cap)", fast)
        self.assertIn("binary_id(layerfs-api::mount_control)", fast)
        self.assertIn("binary_id(layerfs-daemon::forced_unavailable)", fast)
        self.assertNotIn("forced_unavailable", nightly)
        self.assertEqual(nightly.count("binary_id("), len(ci_tests.NIGHTLY))


class ExecutablesFromCargoJson(unittest.TestCase):
    def test_binding_tests_stay_out_of_the_ci_lanes(self):
        meta = {
            "packages": [
                {
                    "name": "layerfs-node",
                    "targets": [
                        {
                            "name": "native_discovery",
                            "kind": ["test"],
                            "src_path": "/w/engine/crates/layerfs-node/tests/native_discovery.rs",
                        },
                        {
                            "name": "native_admission",
                            "kind": ["test"],
                            "src_path": "/w/engine/crates/layerfs-node/tests/native_admission.rs",
                        },
                    ],
                }
            ]
        }
        self.assertEqual(list(ci_tests.test_targets(meta)), [])

    def test_keeps_only_mount_test_binaries(self):
        meta = {"packages": [{"name": "layerfs-daemon", "manifest_path": "/w/engine/crates/layerfs-daemon/Cargo.toml"}]}

        def artifact(name, kind, exe):
            return json.dumps(
                {
                    "reason": "compiler-artifact",
                    "manifest_path": "/w/engine/crates/layerfs-daemon/Cargo.toml",
                    "target": {"name": name, "kind": [kind]},
                    "executable": exe,
                }
            )

        lines = [
            "not json",
            artifact("mounted_commit", "test", "/t/mounted_commit-1"),
            artifact("lane_merge", "test", "/t/lane_merge-1"),
            artifact("layerfs-daemon", "bin", "/t/layerfs-daemon"),
        ]
        found = list(ci_tests.executables(lines, [("layerfs-daemon", "mounted_commit")], meta))
        self.assertEqual(found, [(Path("/w/engine/crates/layerfs-daemon"), "/t/mounted_commit-1")])


if __name__ == "__main__":
    unittest.main()
