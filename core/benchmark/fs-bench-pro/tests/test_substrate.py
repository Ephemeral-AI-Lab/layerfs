import fcntl
import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import runner
from shared import telemetry


class Substrate(unittest.TestCase):
    def test_output_and_target_refusal(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(ValueError):
                runner.owned(Path(directory) / "foreign")
            with self.assertRaises(ValueError):
                runner.owned(runner.RESULTS / "../escape")
            original = runner.os.environ.get("CARGO_TARGET_DIR")
            try:
                runner.os.environ["CARGO_TARGET_DIR"] = directory
                with self.assertRaises(ValueError):
                    runner.target_path()
            finally:
                if original is None:
                    runner.os.environ.pop("CARGO_TARGET_DIR", None)
                else:
                    runner.os.environ["CARGO_TARGET_DIR"] = original

    def test_worktree_locks_are_local(self):
        with tempfile.TemporaryDirectory() as directory:
            one, two = Path(directory) / "one", Path(directory) / "two"
            one.touch()
            two.touch()
            with one.open("a+b") as lock:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                code = "import fcntl,sys; f=open(sys.argv[1], 'a+b'); fcntl.flock(f, fcntl.LOCK_EX | fcntl.LOCK_NB)"
                self.assertNotEqual(subprocess.run([sys.executable, "-c", code, str(one)], stderr=subprocess.DEVNULL).returncode, 0)
                self.assertEqual(subprocess.run([sys.executable, "-c", code, str(two)], stderr=subprocess.DEVNULL).returncode, 0)

    def test_telemetry_loss_and_truncation_keep_capture(self):
        run = "1" * 32
        op = {"v": 1, "kind": "operation", "run": run, "pid": 123, "role": 1,
              "namespace": 10, "success": True, "timing": {"name": "HistoryCommand", "elapsed_ns": 7, "children": []}}
        summary = {"v": 1, "kind": "run-summary", "run": run, "pid": 123, "role": 1,
                   "namespace": 10, "dropped": 0, "failed": 0, "overflow": False}
        raw = b"".join(b"LFT1 " + json.dumps(item).encode() + b"\n" for item in (op, summary))
        producer = {"name": "service", "pid": 123, "role": 1, "namespace": 10,
                    "start": 0, "end": len(raw), "bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}
        self.assertEqual(telemetry.inspect(raw, [producer], run)["status"], "PASS")
        summary["dropped"] = 1
        lost = b"".join(b"LFT1 " + json.dumps(item).encode() + b"\n" for item in (op, summary))
        producer.update(end=len(lost), bytes=len(lost), sha256=hashlib.sha256(lost).hexdigest())
        self.assertEqual(telemetry.inspect(lost, [producer], run)["status"], "INCOMPLETE")
        with tempfile.TemporaryDirectory() as directory:
            capture = Path(directory) / "service.stderr"
            capture.write_bytes(lost[:-1])
            parsed = telemetry.ingest([{ "name": "service", "path": str(capture), "pid": 123,
                                         "role": 1, "namespace": 10}], Path(directory) / "telemetry.lft1", run)
            self.assertEqual(parsed["status"], "INCOMPLETE")
            self.assertTrue(capture.exists())

    def test_one_sample_cardinality_and_retained_hash(self):
        with tempfile.TemporaryDirectory() as directory:
            run = Path(directory)
            for case in runner.init.CASES.values():
                folder = run / "daemon-host" / "init_namespace" / case.id
                folder.mkdir(parents=True)
                runner.write_json(folder / "receipt.json", {"status": "NOT_RUN", "sample_count": 0})
            target = run / "daemon-host" / "init_namespace" / runner.init.SELECTED[0] / "receipt.json"
            runner.write_json(target, {"status": "INCOMPLETE", "sample_count": 2})
            runner.manifest_run(run)
            with self.assertRaisesRegex(ValueError, "one-sample"):
                runner.verify_run(run)
            target.write_text("corrupt")
            with self.assertRaisesRegex(ValueError, "retained evidence"):
                runner.verify_run(run)


if __name__ == "__main__":
    unittest.main()
