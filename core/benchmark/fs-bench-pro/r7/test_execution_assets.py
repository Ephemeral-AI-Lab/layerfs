"""Actual sealed and staged execution operand closure, without product execution."""
import hashlib
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from r7 import deployment, runner


class ExecutionAssets(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory(prefix="layerfs-r7-assets-test-", dir="/tmp")
        self.root = Path(self.folder.name).resolve()
        self.code = self.root / "code"
        self.code.mkdir()
        source = Path(runner.__file__).resolve().parents[2] / "r7-cache"
        for name in ("residency.py", "stream_manifest.py", "generate_manifest.py"):
            (self.code / name).write_bytes((source / name).read_bytes())
        self.binary = b"independent synthetic P bytes; not executed"
        (self.code / "r7-passthrough").write_bytes(self.binary)
        self.config = dict(cache_observer_inside="/code/residency.py", passthrough_inside="/code/r7-passthrough",
                           passthrough_sha256=hashlib.sha256(self.binary).hexdigest())
        self.serial = 0

    def tearDown(self):
        self.folder.cleanup()

    def plan(self):
        self.serial += 1
        seal = deployment.seal_tree(self.code, self.root / ("inventory-%d.jsonl" % self.serial))
        return dict(assets=[dict(target="/code", inventory=seal["inventory"], inventory_sha256=seal["sha256"],
                                 content_metadata_set_sha256=seal["content_metadata_set_sha256"])])

    def test_actual_cache_and_p_bytes_bind_before_and_after_staging(self):
        with mock.patch.object(deployment, "load_plan", return_value=self.plan()):
            wanted = runner.bind_execution_assets(self.config, dict(arm="P"))
            staged = dict(schema="r7-container-staging-v1", status="PASS", helper_sha256=wanted)
            self.assertEqual(runner.bind_execution_assets(self.config, dict(arm="P"), staged), wanted)
            for name in wanted:
                changed = dict(wanted, **{name: "0" * 64})
                with self.assertRaisesRegex(ValueError, "actual staged"):
                    runner.bind_execution_assets(self.config, dict(arm="P"), dict(staged, helper_sha256=changed))

    def test_different_sealed_p_binary_cannot_use_verified_host_label(self):
        (self.code / "r7-passthrough").write_bytes(b"different implementation")
        with mock.patch.object(deployment, "load_plan", return_value=self.plan()):
            with self.assertRaisesRegex(ValueError, "execution bytes"):
                runner.bind_execution_assets(self.config, dict(arm="P"))

    def test_different_sealed_cache_helper_cannot_use_canonical_source_label(self):
        (self.code / "residency.py").write_bytes(b"different cache predicate")
        with mock.patch.object(deployment, "load_plan", return_value=self.plan()):
            with self.assertRaisesRegex(ValueError, "execution bytes"):
                runner.bind_execution_assets(self.config, dict(arm="L"))

    def test_passthrough_cannot_replace_cache_helper_digest(self):
        for name in ("residency.py", "stream_manifest.py", "generate_manifest.py"):
            with mock.patch.object(deployment, "load_plan") as prepared:
                with self.assertRaisesRegex(ValueError, "collides"):
                    runner.bind_execution_assets(dict(self.config, passthrough_inside="/code/" + name), dict(arm="P"))
            prepared.assert_not_called()

    def test_alternate_observer_and_precompiled_asset_cache_refuse(self):
        with self.assertRaisesRegex(ValueError, "canonical /code/residency"):
            runner.bind_execution_assets(dict(self.config, cache_observer_inside="/code/other.py"), dict(arm="N"))
        cache = self.code / "__pycache__"
        cache.mkdir()
        (cache / "residency.cpython.pyc").write_bytes(b"undeclared executable cache")
        with mock.patch.object(deployment, "load_plan", return_value=self.plan()):
            with self.assertRaisesRegex(ValueError, "cached Python bytecode"):
                runner.bind_execution_assets(self.config, dict(arm="N"))


if __name__ == "__main__":
    unittest.main()
