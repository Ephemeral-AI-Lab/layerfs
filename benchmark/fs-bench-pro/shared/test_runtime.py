import sys
import json
import sqlite3
import subprocess
import tempfile
from pathlib import Path
import unittest
from unittest.mock import Mock, patch

import runtime


class RuntimeTests(unittest.TestCase):
    def test_bounded_output_and_timeout(self):
        result = runtime.run([sys.executable, "-c", "print('x'*100000)"], deadline=runtime.Deadline.after(2), output_limit=16)
        self.assertTrue(result.truncated)
        self.assertLessEqual(len(result.stdout) + len(result.stderr), 16)
        result = runtime.run([sys.executable, "-c", "import time;time.sleep(2)"], deadline=runtime.Deadline.after(.05), check=False)
        self.assertTrue(result.timed_out)
        self.assertNotEqual(result.returncode, 0)

    def test_native_source_reuse_preserves_all_fresh_output_checks(self):
        def execute(argv, **kwargs):
            output = b"abc prepared/manifest.json\nabc sample/manifest.json\n" if argv[0] == "/usr/bin/sha256sum" else b""
            return runtime.CommandResult(tuple(argv), 0, output, b"", 7, False, False)

        sample = Mock()
        sample.exec.side_effect = execute
        receipt = runtime.prepare_sample(sample, mode="fresh-output", reuse_prepared_input=True,
                                         deadline=runtime.Deadline.after(1))
        commands = [call.args[0] for call in sample.exec.call_args_list]
        self.assertFalse(any("--reflink" in " ".join(argv) for argv in commands))
        copy = next(argv for argv in commands if "reuse-prepared-input" in argv)
        self.assertIn('-maxdepth 1 -type f', copy[2])
        self.assertEqual(receipt["clone_method"], "not-applicable")
        self.assertIsNone(receipt["reflink_attempt_wall_ns"])
        self.assertIsNone(receipt["fallback_wall_ns"])
        self.assertEqual(receipt["prepared_input_root"], runtime.PREPARED_ROOT + "/payload/input")
        self.assertEqual(receipt["fixture_reuse_method"], "prepared-image-source")
        self.assertEqual(receipt["fresh_output_stores"],
                         [runtime.SAMPLE_ROOT + "/work/store.sqlite", runtime.SAMPLE_ROOT + "/payload/store.sqlite"])
        fresh = next(argv for argv in commands if "fresh-output" in argv)
        self.assertEqual(fresh[4:], receipt["fresh_output_stores"])
        # Execute the actual guard with either output present, including the first argument.
        with tempfile.TemporaryDirectory() as directory:
            outputs = [str(Path(directory) / name) for name in ("first.sqlite", "second.sqlite")]
            command = [*fresh[:4], *outputs]
            self.assertEqual(subprocess.run(command, capture_output=True).returncode, 0)
            for output in outputs:
                Path(output).touch()
                self.assertNotEqual(subprocess.run(command, capture_output=True).returncode, 0)
                Path(output).unlink()

        sample.reset_mock()
        with self.assertRaisesRegex(ValueError, "fresh-output"):
            runtime.prepare_sample(sample, mode="clone", reuse_prepared_input=True,
                                   deadline=runtime.Deadline.after(1))
        sample.exec.assert_not_called()

    def test_closed_store_isolation_and_sidecar_rejection(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "master.sqlite"
            connection = sqlite3.connect(source)
            connection.execute("CREATE TABLE state(value)")
            connection.execute("INSERT INTO state VALUES (1)")
            connection.commit()
            connection.close()
            before = runtime.file_sha256(source)
            copies = []
            for index in range(2):
                target = root / f"sample-{index}.sqlite"
                receipt = runtime.closed_store_copy(source, target, deadline=runtime.Deadline.after(2))
                self.assertEqual(receipt["master_store_sha256"], before)
                copies.append(target)
            connection = sqlite3.connect(copies[0])
            connection.execute("UPDATE state SET value=2")
            connection.commit()
            connection.close()
            self.assertEqual(runtime.file_sha256(source), before)
            self.assertEqual(runtime.file_sha256(copies[1]), before)
            connection = sqlite3.connect(source)
            connection.execute("PRAGMA journal_mode=WAL")
            connection.execute("INSERT INTO state VALUES (3)")
            connection.commit()
            with self.assertRaisesRegex(runtime.RuntimeFailure, "sidecars"):
                runtime.closed_store_copy(source, root / "unsafe.sqlite", deadline=runtime.Deadline.after(2))
            connection.close()

    def test_host_endpoint_validation(self):
        value = {"Id": "sample", "Image": "image", "State": {"Running": True},
            "Config": {"Env": ["LAYERFS_BENCH_LOCAL_RUNTIME=0", "LAYERFS_DAEMON_TCP_LISTEN=0.0.0.0:41273", "LAYERFS_FUSE_HOST=host.docker.internal"]},
            "HostConfig": {"NetworkMode": "bridge", "Devices": [{"PathOnHost": "/dev/fuse"}], "CapAdd": ["SYS_ADMIN"],
                "NanoCpus": 2000000000, "Memory": 2048, "MemorySwap": 2048, "PidsLimit": 256,
                "PortBindings": {"41273/tcp": [{"HostIp": "127.0.0.1", "HostPort": ""}]}},
            "NetworkSettings": {"Ports": {"41273/tcp": [{"HostIp": "127.0.0.1", "HostPort": "49152"}]}}}
        runtime._validate_sample_inspection(value, "image", {}, 2, 2048, 256, host_store=True)
        value["NetworkSettings"]["Ports"]["41273/tcp"][0]["HostIp"] = "0.0.0.0"
        with self.assertRaisesRegex(runtime.RuntimeFailure, "loopback"):
            runtime._validate_sample_inspection(value, "image", {}, 2, 2048, 256, host_store=True)
        value["Mounts"] = [{"Type": "bind"}]
        with self.assertRaisesRegex(runtime.RuntimeFailure, "runtime mount"):
            runtime._validate_sample_inspection(value, "image", {}, 2, 2048, 256, host_store=True)

    def test_host_eviction_protects_active_and_refuses_unowned(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for index in range(3):
                path = root / "prepared" / str(index)
                path.mkdir(parents=True)
                (path / "host-owner.json").write_text(json.dumps({"owner": runtime.OWNER}))
                (path / "host-cache.json").write_text(json.dumps({"data_bytes": 10, "created_ns": index}))
            removed = runtime.evict_host_cache(root, root / "prepared/0", max_entries=2)
            self.assertEqual(removed, [str(root / "prepared/1")])
            (root / "prepared/2/host-owner.json").write_text('{}')
            with self.assertRaisesRegex(runtime.RuntimeFailure, "unowned"):
                runtime.evict_host_cache(root, root / "prepared/0", max_entries=1)

    def test_volume_rejected(self):
        value = {"Image": "image", "State": {"Running": True}, "Mounts": [{"Type": "volume"}]}
        with self.assertRaisesRegex(runtime.RuntimeFailure, "runtime mount"):
            runtime._validate_sample_inspection(value, "image", {}, 2, 2048, 256)

    def test_active_cache_entry_is_not_evicted(self):
        cache = runtime.PreparedCache(max_entries=2, max_bytes=100)
        entries = [runtime._CacheEntry("active", "owned:active", "a"*64, "1", 10, {}),
                   runtime._CacheEntry("idle", "owned:idle", "b"*64, "2", 10, {})]
        with patch.object(cache, "entries", return_value=entries), patch.object(cache, "_in_use", side_effect=lambda image, deadline: image == "active"), patch.object(runtime, "run") as command:
            removed = cache.evict(deadline=runtime.Deadline.after(1), incoming_entries=1)
            self.assertEqual(removed, ["idle"])
            self.assertEqual(command.call_args.args[0], ["docker", "image", "rm", "owned:idle"])

    def test_all_protected_cache_fails_without_delete(self):
        cache = runtime.PreparedCache(max_entries=1, max_bytes=100)
        entries = [runtime._CacheEntry("active", "owned:active", "a"*64, "1", 10, {})]
        with patch.object(cache, "entries", return_value=entries), patch.object(runtime, "run") as command:
            with self.assertRaisesRegex(runtime.RuntimeFailure, "protected/active"):
                cache.evict(deadline=runtime.Deadline.after(1), incoming_entries=1, protected=["active"])
            command.assert_not_called()


if __name__ == "__main__":
    unittest.main()
