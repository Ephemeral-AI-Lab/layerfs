"""Independent same-L preservation checks; no SDK or Docker execution."""
import copy
from pathlib import Path
import tempfile
import unittest
from unittest import mock

import git_index_proof as proof


class SameLIndex(unittest.TestCase):
    def observation(self):
        return dict(raw_index_sha256="a" * 64, raw_index_bytes=42,
                    filesystem_metadata=dict(mtime_ns=1234, ctime_ns=1234, device=1, inode=2))

    def test_same_l_supported_times_survive_without_physical_identity_claim(self):
        before = self.observation()
        after = copy.deepcopy(before)
        after["filesystem_metadata"].update(device=9, inode=10)
        with mock.patch.object(proof.git_index_oracle, "compare", return_value=dict(status="PASS")):
            actual = proof.compare_same_l_index(before, after)
        self.assertEqual(actual["same_l_supported_times"]["mtime_ns"], 1234)

    def test_raw_byte_loss_and_supported_time_loss_refuse(self):
        before = self.observation()
        for field, value in (("raw_index_sha256", "b" * 64), ("raw_index_bytes", 43)):
            with mock.patch.object(proof.git_index_oracle, "compare", return_value=dict(status="PASS")):
                with self.assertRaises(proof.OriginalFailure):
                    proof.compare_same_l_index(before, dict(before, **{field: value}))
        for field in ("mtime_ns", "ctime_ns"):
            after = copy.deepcopy(before)
            after["filesystem_metadata"][field] += 1
            with mock.patch.object(proof.git_index_oracle, "compare", return_value=dict(status="PASS")):
                with self.assertRaisesRegex(proof.OriginalFailure, "supported mtime"):
                    proof.compare_same_l_index(before, after)

    def test_generated_probe_disables_asset_bytecode_writes(self):
        with tempfile.TemporaryDirectory(prefix="layerfs-r7-probe-body-", dir="/tmp") as folder:
            path = proof.body(Path(folder), "probe", dict(inside="/tmp/layerfs-r7-probe", mode="observe"))
            self.assertIn("\npython3 -B - <<'R7PY'\n", path.read_text())


if __name__ == "__main__":
    unittest.main()
