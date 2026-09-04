"""Small offline checks for sample identity and compact result parsing."""
import json
import time
import unittest
from unittest.mock import patch
from types import SimpleNamespace

import runner


class RunnerTests(unittest.TestCase):
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


if __name__ == "__main__":
    unittest.main()
