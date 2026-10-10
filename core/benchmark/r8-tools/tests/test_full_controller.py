"""External failure-custody fixtures; no Docker, runtime or product execution."""
import importlib.util
import json
from pathlib import Path
import tempfile
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

SOURCE = Path(__file__).resolve().parents[1] / "run_full_oracle.py"
SPEC = importlib.util.spec_from_file_location("full_controller", SOURCE)
controller = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(controller)


class ControllerCustody(unittest.TestCase):
    def test_timeout_exports_partial_rows_even_without_final_json(self):
        calls = []
        def copy(argv, **options):
            calls.append(argv[2])
            if argv[2].endswith("/r8-full-oracle.json"):
                raise RuntimeError("original final JSON is absent")
        result = {}
        with patch.object(controller, "checked", side_effect=copy):
            with self.assertRaisesRegex(RuntimeError, "original final JSON is absent"):
                controller.export_oracle(SimpleNamespace(container="owned"), Path("/tmp/proof"), result,
                                         time.monotonic() + 10)
        self.assertEqual(calls, ["owned:/tmp/r8-full-oracle.json.artifacts", "owned:/tmp/r8-full-oracle.json"])
        self.assertEqual(result["exported_oracle_members"], ["oracle.artifacts"])
        self.assertEqual(len(result["oracle_export_failures"]), 1)

    def test_independent_export_failure_does_not_skip_another_source(self):
        calls = []
        def copy(argv, **options):
            calls.append(argv[2])
            if argv[2].endswith(".artifacts"):
                raise RuntimeError("original artifact-copy failure")
        result = {}
        with patch.object(controller, "checked", side_effect=copy):
            with self.assertRaisesRegex(RuntimeError, "original artifact-copy failure"):
                controller.export_oracle(SimpleNamespace(container="owned"), Path("/tmp/proof"), result,
                                         time.monotonic() + 10)
        self.assertEqual(len(calls), 2)
        self.assertEqual(result["exported_oracle_members"], ["oracle.json"])

    def test_startup_no_child_custody_preserves_original_and_writes_receipt(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            binary = folder / "binary"
            binary.write_bytes(b"verified bytes with deliberately no child")
            manifest = folder / "manifest"
            manifest.write_bytes(b"closed fixture manifest")
            inputs = folder / "inputs.json"
            inputs.write_text(json.dumps(dict(image=controller.IMAGE, setup="full independent streamed byte copy",
                volume="layerfs-r8-full-proof-20261010-owned", manifest=str(manifest),
                manifest_sha256=controller.lifecycle.sha(manifest))))
            tools = {label: dict(sha256=controller.lifecycle.sha(path)) for label, path in
                (("runtime", binary), ("daemon", binary), ("controller", SOURCE),
                 ("comparator", SOURCE.with_name("full_oracle.py")))}
            registration = folder / "registration.json"
            registration.write_text(json.dumps(dict(identities=dict(source_commit="registered-source"),
                full_fixture_proof=dict(binaries_and_tools=tools, comparator_walkers=8,
                                        inputs_sha256=controller.lifecycle.sha(inputs)))))
            original = OSError("original Popen refusal without child")
            retained = []
            carrier = SimpleNamespace(process=None, selector=None, raw=None, stderr=None)
            carrier.retain = lambda cause: retained.append(cause) or {"host_pid": None, "no_child": True}
            original.event_process = carrier
            args = SimpleNamespace(inputs=inputs, registration=registration, output=folder / "out",
                                   runtime=binary, daemon=binary, socket="not-contacted")
            with patch.object(controller, "MANIFEST", manifest), patch.object(controller, "EventProcess", side_effect=original):
                with self.assertRaises(SystemExit) as failure:
                    controller.proof(args)
            self.assertEqual(failure.exception.code, 1)
            receipt = json.loads((args.output / "proof.json").read_text())
            self.assertEqual(receipt["original_failure"], str(original))
            self.assertEqual(receipt["original_failure_type"], "OSError")
            self.assertEqual(receipt["custody"], {"host_pid": None, "no_child": True})
            self.assertEqual(retained, [original])

    def test_copied_rows_must_match_original_oracle_digests(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            (folder / "oracle.artifacts").mkdir()
            for name in ("expected.jsonl", "observed.jsonl"):
                (folder / "oracle.artifacts" / name).write_text("original sealed rows\n")
            facts = {name.replace(".jsonl", "_sha256"): controller.lifecycle.sha(folder / "oracle.artifacts" / name)
                     for name in ("expected.jsonl", "observed.jsonl")}
            controller.validate_exported_rows(folder, facts)
            (folder / "oracle.artifacts/observed.jsonl").write_text("changed export\n")
            with self.assertRaisesRegex(controller.OriginalFailure, "observed.jsonl"):
                controller.validate_exported_rows(folder, facts)


if __name__ == "__main__":
    unittest.main()
