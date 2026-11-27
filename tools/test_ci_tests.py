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
        self.assertIn(("layerfs-overlay", "custody"), fast)
        self.assertNotIn(("layerfs-vfs", "captured_namespace"), fast)
        # an area binary with only some modules listed is built, its modules filtered
        self.assertIn(("layerfs-store-service", "history"), fast)
        self.assertIn(("layerfs-daemon", "unmount"), fast)
        flags = ci_tests.test_flags(ci_tests.lane_targets("fast", self.meta)).split()
        self.assertEqual(flags.count("--test"), len({name for _, name in fast}))
        self.assertNotIn("mount_control", flags)

    def test_featured_targets_exist_are_gated_and_grouped_by_feature(self):
        sources = {(package, name): source for package, name, source in ci_tests.test_targets(self.meta)}
        features = {package["name"]: package["features"] for package in self.meta["packages"]}
        for (package, name), (feature, _) in ci_tests.FEATURED.items():
            self.assertIn(feature, features[package])
            self.assertNotIn(feature, features[package].get("default", []))
            self.assertIn(f'#![cfg(feature = "{feature}")]', sources[(package, name)].read_text().splitlines())
            self.assertNotIn((package, name), ci_tests.NIGHTLY)
        groups = list(ci_tests.featured_groups())
        self.assertIn(("layerfs-api", "worker-fault", "--test worker_fault connection"), groups)
        self.assertEqual(len(groups), len({(package, feature) for package, feature, _ in groups}))

    def test_mount_group_includes_moved_sdk_targets(self):
        mount = ci_tests.mount_targets(self.meta)
        self.assertIn(("layerfs-sandbox", "kernel_application"), mount)
        self.assertIn(("layerfs-sandbox", "observed_application"), mount)
        self.assertIn(("layerfs-daemon", "unmount", "forced_unavailable"), mount)
        self.assertIn(("layerfs-daemon", "unmount_stalled"), mount)
        # a portable module of a mixed area binary stays out of the mount lane
        self.assertFalse([key for key in mount if key[-1] in {"lane_merge", "staged_lane_merge"}])
        self.assertNotIn(("layerfs-daemon", "unmount"), mount)

    def test_fast_filter_excludes_the_nightly_groups_and_nightly_selects_them(self):
        fast = ci_tests.filter_for("fast", self.meta)
        nightly = ci_tests.filter_for("nightly", self.meta)
        self.assertTrue(fast.startswith("not ("))
        self.assertIn("(binary_id(layerfs-store-service::history) & test(/^scale_lineage_cap::/))", fast)
        self.assertIn("binary_id(layerfs-sandbox::mount_control)", fast)
        self.assertIn("(binary_id(layerfs-daemon::unmount) & test(/^forced_unavailable::/))", fast)
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
                            "src_path": "/w/api/crates/layerfs-node/tests/native_discovery.rs",
                        },
                        {
                            "name": "native_admission",
                            "kind": ["test"],
                            "src_path": "/w/api/crates/layerfs-node/tests/native_admission.rs",
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
            artifact("unmount", "test", "/t/unmount-1"),
            artifact("lane_merge", "test", "/t/lane_merge-1"),
            artifact("layerfs-daemon", "bin", "/t/layerfs-daemon"),
        ]
        mount = [
            ("layerfs-daemon", "mounted_commit"),
            ("layerfs-daemon", "unmount", "forced_smoke"),
            ("layerfs-daemon", "unmount", "detach_stages"),
        ]
        found = list(ci_tests.executables(lines, mount, meta))
        daemon = Path("/w/engine/crates/layerfs-daemon")
        self.assertEqual(
            found,
            [
                (daemon, "/t/mounted_commit-1", ""),
                (daemon, "/t/unmount-1", "detach_stages:: forced_smoke::"),
            ],
        )


if __name__ == "__main__":
    unittest.main()
