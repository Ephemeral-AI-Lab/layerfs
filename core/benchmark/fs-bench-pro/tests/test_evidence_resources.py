"""Parser/ownership proofs over independent files, not resource qualification."""
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from shared import evidence_resources as resources

STATUS_FIXTURE = {"VmRSS": 7, "RssAnon": 3, "RssFile": 3, "RssShmem": 1, "VmSwap": 0}
ROLLUP_FIXTURE = {"Rss": 8, "Pss": 7, "Anonymous": 4, "Shared_Clean": 1,
                  "Shared_Dirty": 1, "Private_Clean": 2, "Private_Dirty": 4, "Swap": 1}
IO_FIXTURE = {"rchar": 13, "wchar": 0, "syscr": 13, "syscw": 0,
              "read_bytes": 13, "write_bytes": 0, "cancelled_write_bytes": 0}


def status():
    return "\n".join([*(key + ": " + str(value) + " kB" for key, value in STATUS_FIXTURE.items()),
                       "VmHWM: 999999 kB", "Name: independent fixture"])


def io():
    return "\n".join(key + ": " + str(value) for key, value in IO_FIXTURE.items())


def stat(pid, birth):
    fields = ["S"] + ["0"] * 18 + [str(birth)] + ["0"] * 12
    return str(pid) + " (fixture with ) in name) " + " ".join(fields)


def rollup():
    return "0000-ffff ---p 0 00:00 0 [rollup]\n" + "\n".join(
        key + ": " + str(value) + " kB" for key, value in ROLLUP_FIXTURE.items())


class ResourceInputs(unittest.TestCase):
    def test_memory_units_and_overlapping_fields_remain_separate(self):
        result = resources.proc_status(status())
        self.assertEqual(result, {key: value * 1024 for key, value in STATUS_FIXTURE.items()})
        self.assertNotIn("VmHWM", result)

    def test_required_memory_units_fields_duplicates_and_signed_values_refuse(self):
        for source in [status().replace("kB", "MB"), status() + "\nVmRSS: 7 kB",
                       status().replace("VmRSS: 7 kB", "VmRSS: -1 kB"),
                       status().replace("VmRSS: 7 kB", ""),
                       status().replace("VmRSS: 7 kB", "VmRSS: ７ kB")]:
            with self.subTest(source=source), self.assertRaises(ValueError):
                resources.proc_status(source)

    def test_process_io_exact_inventory_and_counter_units(self):
        result = resources.proc_io(io())
        self.assertEqual(result, IO_FIXTURE)
        self.assertEqual(result["read_bytes"], 13)
        for source in [io() + "\nnew_field: 1", io() + "\nrchar: 1",
                       io().replace("read_bytes: 13", ""), io().replace("13", "1.3")]:
            with self.subTest(source=source), self.assertRaises(ValueError):
                resources.proc_io(source)

    def test_rollup_requires_actual_header_units_and_all_owning_fields(self):
        self.assertEqual(resources.proc_rollup(rollup()),
                         {key: value * 1024 for key, value in ROLLUP_FIXTURE.items()})
        for source in [rollup().replace("[rollup]", "other"), rollup().replace("kB", "B"),
                       rollup().replace("Rss: 8 kB", ""), rollup() + "\nRss: 8 kB"]:
            with self.subTest(source=source), self.assertRaises(ValueError):
                resources.proc_rollup(source)

    def test_stat_name_with_parentheses_does_not_shift_start_identity(self):
        self.assertEqual(resources.proc_birth(stat(123, 7788), 123), 7788)
        for source in [stat(124, 7788), stat(123, -1), "123 (short) S 1"]:
            with self.subTest(source=source), self.assertRaises(ValueError):
                resources.proc_birth(source, 123)

    def test_cgroup_memory_does_not_sum_overlapping_subsets(self):
        self.assertEqual(resources.cgroup_memory("anon 5\nfile 6\nshmem 4\npgfault 900"),
                         {"anon": 5, "file": 6, "shmem": 4})
        with self.assertRaises(ValueError):
            resources.cgroup_memory("anon 5\nanon 6")

    def test_cgroup_io_keeps_device_inventory_and_byte_vs_operation_fields(self):
        source = "8:0 rbytes=12 wbytes=13 rios=2 wios=3 dbytes=4 dios=1\n"
        result = resources.cgroup_io(source)
        self.assertEqual(result["8:0"]["rbytes"], 12)
        for value in [source + source, "8:0 rbytes=1", source.replace("rios=2", "rios=-2"),
                      source.replace("8:0", "bad"), source + "8:1 rbytes=1 rbytes=2"]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                resources.cgroup_io(value)


class ResourceOwnership(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def proc(self):
        folder = self.root / "123"
        folder.mkdir()
        for name, text in {"stat": stat(123, 100), "status": status(), "io": io(),
                           "smaps_rollup": rollup()}.items():
            (folder / name).write_text(text)
        return folder

    def test_one_process_read_returns_actual_interval_without_peak_claim(self):
        self.proc()
        result = resources.linux_process(123, self.root)
        self.assertEqual(result["birth_start_ticks"], 100)
        self.assertEqual(result["memory_status_reported_bytes"]["VmRSS"], 7168)
        self.assertEqual(result["smaps_rollup_bytes"]["Rss"], 8192)
        self.assertEqual(result["io_calls"]["syscr"], 13)
        self.assertGreaterEqual(result["read_closed_ns"], result["read_open_ns"])
        self.assertNotIn("peak", result)
        self.assertEqual(result["clock"]["cross_clock_calibration"], "UNAVAILABLE")

    def test_changed_process_identity_and_missing_original_file_refuse(self):
        folder = self.proc()
        original = resources.read_once
        count = 0

        def replaced(path):
            nonlocal count
            if Path(path).name == "stat":
                count += 1
                return stat(123, count)
            return original(path)

        with patch.object(resources, "read_once", replaced), self.assertRaisesRegex(ValueError, "incarnation"):
            resources.linux_process(123, self.root)
        (folder / "io").unlink()
        with self.assertRaises(FileNotFoundError) as cause:
            resources.linux_process(123, self.root)
        self.assertEqual(Path(cause.exception.filename), folder / "io")

    def test_read_and_close_failures_retain_original_read_and_secondary_close(self):
        primary = OSError("original observation read refusal")
        secondary = OSError("original observation close refusal")

        class Raw:
            reads = closes = 0

            def __enter__(self):
                return self

            def __exit__(self, *exception):
                self.close()

            def read(self, size):
                self.reads += 1
                self.requested_bytes = size
                raise primary

            def close(self):
                self.closes += 1
                raise secondary

        raw = Raw()
        with patch.object(Path, "open", return_value=raw) as opened:
            with self.assertRaises(OSError) as cause:
                resources.read_once(self.root / "read-and-close")
        opened.assert_called_once_with("rb", buffering=0)
        self.assertEqual((raw.reads, raw.closes), (1, 1))
        self.assertEqual(raw.requested_bytes, resources.WINDOW + 1)
        self.assertIs(cause.exception, primary)
        self.assertIs(cause.exception.close_failure, secondary)

    def test_read_close_only_failure_is_original_without_second_read_or_close(self):
        original = OSError("original observation close refusal")

        class Raw:
            reads = closes = 0

            def __enter__(self):
                return self

            def __exit__(self, *exception):
                self.close()

            def read(self, size):
                self.reads += 1
                self.requested_bytes = size
                return b"original complete observation\n"

            def close(self):
                self.closes += 1
                raise original

        raw = Raw()
        with patch.object(Path, "open", return_value=raw) as opened:
            with self.assertRaises(OSError) as cause:
                resources.read_once(self.root / "close-only")
        opened.assert_called_once_with("rb", buffering=0)
        self.assertEqual((raw.reads, raw.closes), (1, 1))
        self.assertEqual(raw.requested_bytes, resources.WINDOW + 1)
        self.assertIs(cause.exception, original)

    def test_one_cgroup_snapshot_uses_current_without_lifetime_peak(self):
        for name, text in {"memory.current": "50\n", "memory.stat": "anon 20\nfile 35",
                           "io.stat": "8:0 rbytes=12 wbytes=13 rios=2 wios=3",
                           "memory.peak": "99999999\n"}.items():
            (self.root / name).write_text(text)
        result = resources.linux_cgroup(self.root)
        self.assertEqual(result["memory_current_bytes"], 50)
        self.assertEqual(result["memory_stat_bytes"], {"anon": 20, "file": 35})
        self.assertEqual(result["memory_stat_unavailable_fields"],
                         sorted(resources.CGROUP_BYTES - {"anon", "file"}))
        self.assertNotIn("memory_peak_bytes", result)
        with self.assertRaises(ValueError):
            resources.linux_cgroup(Path("relative"))

    def test_artifact_reports_only_actual_stat_allocation(self):
        path = self.root / "one"
        path.write_bytes(b"independent closed bytes")
        result = resources.artifact_allocation(path)
        self.assertEqual(result["logical_bytes"], path.stat().st_size)
        self.assertEqual(result["allocated_bytes"], path.stat().st_blocks * 512)
        self.assertNotIn("physical_io_bytes", result)
        with self.assertRaises(FileNotFoundError):
            resources.artifact_allocation(self.root / "missing")

    def test_stream_keeps_acknowledged_prefix_and_original_failure_without_replay(self):
        path = self.root / "observations.jsonl"
        output = resources.Stream(path)
        self.addCleanup(output.close)
        output.append(lambda: {"schema": "fixture", "count": 1})
        original = OSError("original observer refusal")

        def refused():
            raise original

        with self.assertRaises(OSError) as cause:
            output.append(refused)
        self.assertIs(cause.exception, original)
        calls = []
        with self.assertRaisesRegex(ValueError, "terminal"):
            output.append(lambda: calls.append(1))
        self.assertEqual(calls, [])
        self.assertEqual(output.records, 1)
        self.assertEqual(len(path.read_text().splitlines()), 1)
        with self.assertRaises(FileExistsError):
            resources.Stream(path)

    def test_window_overflow_stops_original_stream_without_truncated_record(self):
        path = self.root / "one.jsonl"
        output = resources.Stream(path)
        self.addCleanup(output.close)
        with self.assertRaisesRegex(ValueError, "window"):
            output.append(lambda: {"too_large": "x" * resources.WINDOW})
        self.assertEqual(path.stat().st_size, 0)
        self.assertTrue(output.failed)
        too_large = self.root / "counter"
        too_large.write_bytes(b"x" * (resources.WINDOW + 1))
        with self.assertRaisesRegex(ValueError, "window"):
            resources.read_once(too_large)

    def test_original_write_or_flush_refusal_has_no_post_failure_write_on_close(self):
        for phase in ("write", "flush"):
            with self.subTest(phase=phase):
                original = OSError("original " + phase)

                class Raw:
                    closed = False
                    writes = flushes = closes = 0

                    def write(self, data):
                        self.writes += 1
                        if phase == "write":
                            raise original
                        return len(data)

                    def flush(self):
                        self.flushes += 1
                        if phase == "flush":
                            raise original

                    def close(self):
                        self.closes += 1
                        self.closed = True

                raw = Raw()
                with patch.object(Path, "open", return_value=raw) as opened:
                    output = resources.Stream(self.root / phase)
                opened.assert_called_once_with("xb", buffering=0)
                with self.assertRaises(OSError) as cause:
                    output.append(lambda: {"original": "observation"})
                self.assertIs(cause.exception, original)
                self.assertIs(output.failure, original)
                calls = []
                with self.assertRaisesRegex(ValueError, "terminal"):
                    output.append(lambda: calls.append(1))
                self.assertEqual(calls, [])
                before = (raw.writes, raw.flushes)
                output.close()
                output.close()
                self.assertEqual((raw.writes, raw.flushes), before)
                self.assertEqual(raw.closes, 1)
                self.assertEqual(output.records, 0)

    def test_short_raw_write_retains_acknowledged_prefix_without_writing_remainder(self):
        class Raw:
            closed = False
            writes = 0

            def write(self, data):
                self.writes += 1
                self.prefix = data[:3]
                return 3

            def close(self):
                self.closed = True

        raw = Raw()
        with patch.object(Path, "open", return_value=raw):
            output = resources.Stream(self.root / "short")
        with self.assertRaisesRegex(OSError, "short") as cause:
            output.append(lambda: {"independent": 1})
        self.assertIs(output.failure, cause.exception)
        self.assertEqual(output.acknowledged_bytes, len(raw.prefix))
        self.assertEqual(output.records, 0)
        output.close()
        self.assertEqual(raw.writes, 1)

    def test_close_failure_preserves_primary_and_secondary_without_second_close(self):
        primary = OSError("original writer refusal")
        secondary = OSError("original close refusal")

        class Raw:
            closed = False
            closes = 0

            def write(self, data):
                raise primary

            def close(self):
                self.closes += 1
                raise secondary

        raw = Raw()
        with patch.object(Path, "open", return_value=raw):
            output = resources.Stream(self.root / "close")
        with self.assertRaises(OSError) as cause:
            output.append(lambda: {"original": 1})
        self.assertIs(cause.exception, primary)
        with self.assertRaises(OSError) as cause:
            output.close()
        self.assertIs(cause.exception, secondary)
        self.assertIs(output.failure, primary)
        self.assertIs(output.close_failure, secondary)
        output.close()
        self.assertEqual(raw.closes, 1)


if __name__ == "__main__":
    unittest.main()
