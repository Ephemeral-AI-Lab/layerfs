"""Small offline checks for sample identity and compact result parsing."""
import json
import time
import tempfile
import sqlite3
from pathlib import Path
import unittest
from unittest.mock import patch
from types import SimpleNamespace

import runner


class RunnerTests(unittest.TestCase):
    def test_host_cache_semantic_reuse_and_sample_isolation(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(runner, "HOST_ROOT", Path(directory)):
            fixture = {"fixture_profile": "test-v1", "input_mode": "store", "input_plan_sha256": "plan", "fixture_bytes": 0}
            selection = {"family": "payload_create_read", "case": "first", "seed": 1, "setup_identity": "clone", "image": "image-a", "host_executor": {"source": "a", "schema_sha256": "schema-a"}}
            args = SimpleNamespace(host_binary="fake")
            preparations = []
            def command(argv, deadline, **kwargs):
                if argv[1] == "infra-prepare":
                    root = Path(argv[-1])
                    (root / "payload").mkdir(parents=True)
                    db = sqlite3.connect(root / "payload/store.sqlite")
                    db.execute("CREATE TABLE state(value)")
                    db.execute("INSERT INTO state VALUES (1)")
                    db.commit()
                    db.close()
                    (root / "payload/branch-id").write_text("branch")
                    (root / "manifest.json").write_text('{}')
                    preparations.append(root)
                return SimpleNamespace(stdout=json.dumps(fixture))
            with patch.object(runner, "_command", side_effect=command):
                first = runner._host_acquire(args, selection, time.monotonic() + 5)
                changed_executor = {**selection, "case": "second", "image": "image-b", "host_executor": {"source": "b", "schema_sha256": "schema-a"}}
                second = runner._host_acquire(args, changed_executor, time.monotonic() + 5)
                self.assertFalse(first["cache_hit"])
                self.assertTrue(second["cache_hit"])
                self.assertEqual(len(preparations), 1)
                self.assertEqual(second["producer"], selection["host_executor"])
                one = runner._host_sample(first, selection, "one", time.monotonic() + 5)
                two = runner._host_sample(second, changed_executor, "two", time.monotonic() + 5)
                self.assertNotEqual(one["sample_store_inode"], two["sample_store_inode"])
                self.assertEqual(one["sample_store_sha256"], two["sample_store_sha256"])
                fixture["input_plan_sha256"] = "changed"
                third = runner._host_acquire(args, selection, time.monotonic() + 5)
                self.assertFalse(third["cache_hit"])
                self.assertNotEqual(first["cache_key"], third["cache_key"])

    def test_deadline_units(self):
        remaining = runner._deadline(time.monotonic() + 5).remaining()
        self.assertGreater(remaining, 4)
        self.assertLessEqual(remaining, 5)

    def resolve(self, argv, row):
        args = runner.build_parser().parse_args(["--family", "example", "--image", "sealed", "--case", "case", *argv])
        image = {"Id": "sha256:image", "Os": "linux", "Architecture": "arm64",
                 "Config": {"Labels": {"dev.layerfs.source-seal": "source"}}}
        with patch.object(runner, "image_info", return_value=image), patch.object(runner, "_command", return_value=SimpleNamespace(stdout=json.dumps({"family_id": "example", "scenario_id": "case", **row}))):
            return args, runner.resolve_selection(args, 999999999)

    def test_n_does_not_change_seed(self):
        one_args, one = self.resolve(["--seed", "2", "--perf-fast"], {})
        many_args, many = self.resolve(["--seed", "2", "--perf-samples", "10"], {})
        self.assertEqual(one["input_identity"], many["input_identity"])
        self.assertEqual(many["seed"], 2)
        self.assertEqual(many_args.perf_samples, 10)

    def test_initialization_rejects_clone(self):
        with self.assertRaisesRegex(ValueError, "fresh output"):
            self.resolve(["--setup", "clone"], {"setup_policy": "fresh-output"})

    def test_inherited_requires_repetition(self):
        with self.assertRaisesRegex(ValueError, "repetition"):
            self.resolve(["--seed", "1"], {"inherited": True})
        _, selection = self.resolve(["--repetition", "1"], {"inherited": True})
        self.assertEqual(selection["repetition"], 1)

    def test_source_and_input_authentication(self):
        with self.assertRaisesRegex(ValueError, "source identity"):
            self.resolve(["--source", "other"], {})
        with self.assertRaisesRegex(ValueError, "input identity"):
            self.resolve(["--input", "other"], {})

    def test_count_rejected(self):
        with self.assertRaisesRegex(ValueError, "positive"):
            self.resolve(["--perf-samples", "0"], {})

    def test_parse_only_complete_json(self):
        self.assertEqual(runner.records('log\nPREFIX\t{"kind":"done"}\n{"truncated":'), [{"kind": "done"}])

    def test_native_diagnostics_remain_explicit_debug_text(self):
        line = "layerfs-initialization-producer-v1 nonce=abcd producer=0 files=100"
        self.assertEqual(runner.initialization_diagnostics("unrelated\n" + line),
                         [{"kind": "initialization-debug-text", "details": line}])


if __name__ == "__main__":
    unittest.main()
