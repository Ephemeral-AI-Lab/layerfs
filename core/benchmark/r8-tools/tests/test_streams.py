import hashlib
import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

TOOLS = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("run_streams", TOOLS / "run_streams.py")
streams = importlib.util.module_from_spec(spec)
spec.loader.exec_module(streams)
SOURCE = TOOLS / "stream_source.py"


def produce(stdout_bytes, stderr_bytes, code=0, extra=()):
    done = subprocess.run([sys.executable, "-B", str(SOURCE), str(stdout_bytes), str(stderr_bytes),
                           str(code), *extra], capture_output=True, timeout=30)
    return done


class Definition(unittest.TestCase):
    def test_source_and_controller_agree_at_every_registered_small_size(self):
        for totals in streams.SIZES:
            done = produce(*totals)
            self.assertEqual(done.returncode, 0)
            for name, data, total in (("stdout", done.stdout, totals[0]), ("stderr", done.stderr, totals[1])):
                self.assertEqual((len(data), hashlib.sha256(data).hexdigest()),
                                 streams.expected(name, total), (name, totals))

    def test_blocks_carry_their_index_so_reordering_changes_the_digest(self):
        data = produce(3 * streams.BLOCK, 0).stdout
        self.assertEqual(data[:8], (0).to_bytes(8, "big"))
        self.assertEqual(data[2 * streams.BLOCK:2 * streams.BLOCK + 8], (2).to_bytes(8, "big"))
        swapped = data[streams.BLOCK:2 * streams.BLOCK] + data[:streams.BLOCK] + data[2 * streams.BLOCK:]
        self.assertNotEqual(hashlib.sha256(swapped).hexdigest(),
                            streams.expected("stdout", 3 * streams.BLOCK)[1])

    def test_exit_status_and_held_descendant_bytes(self):
        self.assertEqual(produce(10, 10, 7).returncode, 7)
        held = produce(6, 0, 0, ("held", "0.2"))
        self.assertEqual((len(held.stdout), hashlib.sha256(held.stdout).hexdigest()),
                         streams.expected("stdout", 6, streams.LATE))


class Compare(unittest.TestCase):
    def files(self, directory, stdout, stderr):
        paths = dict(stdout=Path(directory) / "o", stderr=Path(directory) / "e")
        paths["stdout"].write_bytes(stdout)
        paths["stderr"].write_bytes(stderr)
        return paths

    def test_exact_delivery_passes_and_each_deviation_fails(self):
        done = produce(8193, 8191)
        with tempfile.TemporaryDirectory() as directory:
            exact = self.files(directory, done.stdout, done.stderr)
            self.assertEqual(streams.compare(dict(label="x"), exact, (8193, 8191)), [])
            for stdout, stderr in ((done.stdout[:-1], done.stderr), (done.stdout + b"\0", done.stderr),
                                   (done.stdout, done.stderr[1:]),
                                   (done.stdout[:8] + b"\1" + done.stdout[9:], done.stderr)):
                changed = self.files(directory, stdout, stderr)
                self.assertEqual(len(streams.compare(dict(label="x"), changed, (8193, 8191))), 1)

    def test_only_a_verified_large_stream_is_dropped_after_its_digest_is_recorded(self):
        size = streams.RETAINED_BYTES + 1
        done = produce(size, 8)
        with tempfile.TemporaryDirectory() as directory:
            case = dict(label="x")
            paths = self.files(directory, done.stdout, done.stderr)
            self.assertEqual(streams.compare(case, paths, (size, 8)), [])
            self.assertFalse(paths["stdout"].exists())
            self.assertTrue(paths["stderr"].exists())
            self.assertEqual(case["stdout"]["observed_bytes"], size)
            self.assertIs(case["stdout"]["bytes_retained"], False)
            differing = self.files(directory, done.stdout[:-1], done.stderr)
            self.assertEqual(len(streams.compare(dict(label="x"), differing, (size, 8))), 1)
            self.assertTrue(differing["stdout"].exists())

    def test_delivered_counts_are_read_from_exactly_one_streams_row(self):
        row = dict(event="streams", value="container=c exec=e delivered=OutputProgress "
                                          "{ stdout: 12, stderr: 34, wire_payload: 46, saturated: false }; EOF only")
        self.assertEqual(streams.delivered([dict(event="mount"), row], 0), [12, 34])
        self.assertIsNone(streams.delivered([row, row], 0))
        self.assertIsNone(streams.delivered([dict(event="streams", value="none")], 0))
        self.assertIsNone(streams.delivered([row], 1))
        for change in (("wire_payload: 46", "wire_payload: 45"), ("saturated: false", "saturated: true")):
            self.assertIsNone(streams.delivered([dict(row, value=row["value"].replace(*change))], 0))

    def test_the_exit_marker_is_the_typed_status(self):
        self.assertEqual(streams.EXITED, "exit_code: Some(7)")


if __name__ == "__main__":
    unittest.main()
