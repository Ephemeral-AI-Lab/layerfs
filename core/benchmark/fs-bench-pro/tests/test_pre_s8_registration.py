"""Read-only registration refuses changed workload/build/binary identities."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from shared import pre_s8_registration as registration
ROOT = Path(__file__).resolve().parents[4]


class Registration(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        binary = Path(self.temp.name) / "engine_finite"
        binary.write_bytes(b"external unit registration fixture; not a route proof")
        registry = json.loads((ROOT / registration.REGISTRY).read_text())
        source = "core/crates/layerfs-daemon/examples/engine_finite.rs"
        self.value = {"schema": registration.SCHEMA, "case": "engine-finite",
                      "registry_sha256": registration.digest(ROOT / registration.REGISTRY),
                      "mode": "diagnostic", "numeric_acceptance": "OWNER_DEFERRED",
                      "workload": registry["cases"]["engine-finite"], "cache": registry["cache"],
                      "budget_ns": registry["budget_ns"], "profile": {"overlay": "schema16-memory-off-exclusive", "store": "NOT_IN_SCOPE"},
                      "authority_sha256": registration.digest(ROOT / registry["authority"]),
                      "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                      "sources": {source: registration.digest(ROOT / source)},
                      "build_inputs": {name: registration.digest(ROOT / name) for name in (".cargo/config.toml", "core/Cargo.toml", "core/Cargo.lock")},
                      "build_command": ["cargo", "+1.85.1", "build", "--locked"],
                      "construction_workers": "1", "binary": {"path": str(binary), "sha256": registration.digest(binary)},
                      "environment": {"platform": "unit-fixture"}, "prepared": None}

    def rejected(self, field, value):
        candidate = copy.deepcopy(self.value)
        candidate[field] = value
        self.assertEqual(registration.admit(candidate, ROOT)["status"], "INCOMPLETE")

    def test_registration_is_not_qualification(self):
        result = registration.admit(self.value, ROOT)
        self.assertEqual(result["status"], "PASS")
        self.assertTrue(result["registration_only"])
        self.assertEqual(result["qualification"], "NOT_EVALUATED")

    def test_workload(self):
        self.rejected("workload", {"workspaces": 1})

    def test_numeric_promotion(self):
        self.rejected("numeric_acceptance", "PASS")

    def test_unlocked_build(self):
        self.rejected("build_command", ["cargo", "build"])

    def test_binary(self):
        self.rejected("binary", {**self.value["binary"], "sha256": "0" * 64})

    def test_profile(self):
        self.rejected("profile", {"overlay": "durable"})

    def test_source(self):
        self.rejected("sources", {next(iter(self.value["sources"])): "0" * 64})

    def test_cache(self):
        self.rejected("cache", "cold")

    def test_build_inputs(self):
        self.rejected("build_inputs", {"core/Cargo.toml": self.value["build_inputs"]["core/Cargo.toml"]})

    def test_owner_authority(self):
        self.rejected("authority_sha256", "0" * 64)


if __name__ == "__main__":
    unittest.main()
