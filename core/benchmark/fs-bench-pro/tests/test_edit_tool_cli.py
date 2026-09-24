"""Development check: the sealed POSIX editor really performs the frozen edit.

The tool binary is run against a local byte copy of the declared fixture with
the declared command of every registered case, and the result is compared with
the contract's own expected size and digest. This is a correctness check, not a
performance sample: it never enters the product route and it is skipped when the
release binary has not been built.
"""
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
BENCH = HERE.parent
sys.path.insert(0, str(BENCH))
from shared import edit_contract as contract  # noqa: E402

TOOL = BENCH / "workload/target/release/layerfs-edit-tool"
CHECK_SIZES = (1_048_576, 10_485_760)


def fixture_bytes(size):
    out = bytearray()
    offset = 0
    while offset < size:
        take = min(8 << 20, size - offset)
        out.extend(contract.chunk(contract.FIXTURE_GENERATOR_SEED, offset, take))
        offset += take
    return bytes(out)


@unittest.skipUnless(TOOL.is_file(), "release edit tool is not built")
class EditTool(unittest.TestCase):
    def commands(self):
        for row in contract.registry():
            if row["fixture_bytes"] not in CHECK_SIZES:
                continue
            if row["operation_key"] in ("truncate-tail-4k", "append-tail-4k",
                                        "zero-extend-tail-4k", "overwrite-head-4k",
                                        "overwrite-middle-4k", "overwrite-tail-4k",
                                        "overwrite-fixed-64k-chunk-count-preserve",
                                        "overwrite-fixed-64k-chunk-count-increase",
                                        "overwrite-fixed-64k-chunk-count-decrease"):
                continue
            yield row

    def test_declared_commands_produce_the_declared_result(self):
        rows = list(self.commands())
        self.assertGreaterEqual(len(rows), 10)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            payloads = root / "payloads"
            payloads.mkdir()
            for row in contract.registry():
                if not row["replacement_len"]:
                    continue
                data = contract.payload_bytes(row["payload_seed"], row["replacement_len"],
                                              row["replacement_kind"])
                (payloads / f"{row['operation_key']}.bin").write_bytes(data)
            for row in rows:
                with self.subTest(scenario=row["scenario_id"]):
                    shutil.rmtree(root / "work", ignore_errors=True)
                    work = root / "work"
                    work.mkdir()
                    (work / contract.FIXTURE_PATH).write_bytes(fixture_bytes(row["fixture_bytes"]))
                    command = row["command"].replace(contract.TOOL_PATH, str(TOOL)).replace(
                        contract.PAYLOAD_DIRECTORY, str(payloads))
                    result = subprocess.run([part for part in command.split(" ")],
                                            cwd=work, capture_output=True, text=True)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    produced = (work / contract.FIXTURE_PATH).read_bytes()
                    self.assertEqual(len(produced), row["final_bytes"])
                    self.assertEqual(hashlib.sha256(produced).hexdigest(), row["final_sha256"])

    def test_declared_size_and_overwrite_commands_are_unchanged(self):
        rows = [row for row in contract.registry()
                if row["fixture_bytes"] == 1_048_576
                and row["operation_key"] in ("truncate-tail-4k", "append-tail-4k",
                                             "zero-extend-tail-4k", "overwrite-middle-4k",
                                             "overwrite-head-4k", "overwrite-tail-4k",
                                             "overwrite-fixed-64k-chunk-count-preserve",
                                             "overwrite-fixed-64k-chunk-count-increase",
                                             "overwrite-fixed-64k-chunk-count-decrease")]
        self.assertEqual(len(rows), 9)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            payloads = root / "payloads"
            payloads.mkdir()
            for row in rows:
                if row["replacement_len"]:
                    data = contract.payload_bytes(row["payload_seed"], row["replacement_len"],
                                                  row["replacement_kind"])
                    (payloads / f"{row['operation_key']}.bin").write_bytes(data)
                work = root / row["operation_key"]
                work.mkdir()
                (work / contract.FIXTURE_PATH).write_bytes(fixture_bytes(row["fixture_bytes"]))
                command = row["command"].replace(contract.TOOL_PATH, str(TOOL)).replace(
                    contract.PAYLOAD_DIRECTORY, str(payloads))
                result = subprocess.run([part for part in command.split(" ")],
                                        cwd=work, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                produced = (work / contract.FIXTURE_PATH).read_bytes()
                self.assertEqual(len(produced), row["final_bytes"])
                self.assertEqual(hashlib.sha256(produced).hexdigest(), row["final_sha256"])

    def test_the_tool_refuses_a_pre_state_that_differs(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / contract.FIXTURE_PATH
            path.write_bytes(b"x" * 4096)
            result = subprocess.run(
                [str(TOOL), "shift", "--file", str(path), "--offset", "0",
                 "--delete-length", "0", "--length", "4", "--direction", "grow",
                 "--expect-size", "8192"], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("pre-state size", result.stderr)
            self.assertEqual(path.read_bytes(), b"x" * 4096)

    def test_the_tool_refuses_a_contradicted_direction(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / contract.FIXTURE_PATH
            path.write_bytes(b"y" * 4096)
            result = subprocess.run(
                [str(TOOL), "shift", "--file", str(path), "--offset", "0",
                 "--delete-length", "0", "--length", "4", "--direction", "shrink",
                 "--expect-size", "4096"], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("contradicts", result.stderr)
            self.assertEqual(path.read_bytes(), b"y" * 4096)


if __name__ == "__main__":
    unittest.main()
