import sys
import json
import sqlite3
import tempfile
from pathlib import Path
import unittest

import runtime


class RuntimeTests(unittest.TestCase):
    def test_bounded_output_and_timeout(self):
        result = runtime.run([sys.executable, "-c", "print('x'*100000)"], deadline=runtime.Deadline.after(2), output_limit=16)
        self.assertTrue(result.truncated)
        self.assertLessEqual(len(result.stdout) + len(result.stderr), 16)
        result = runtime.run([sys.executable, "-c", "import time;time.sleep(2)"], deadline=runtime.Deadline.after(.05), check=False)
        self.assertTrue(result.timed_out)
        self.assertNotEqual(result.returncode, 0)

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
            "Config": {"Env": ["LAYERFS_DAEMON_TCP_LISTEN=0.0.0.0:41273", "LAYERFS_FUSE_HOST=host.docker.internal"]},
            "HostConfig": {"NetworkMode": "bridge", "Devices": [{"PathOnHost": "/dev/fuse"}], "CapAdd": ["SYS_ADMIN"],
                "NanoCpus": 2000000000, "Memory": 2048, "MemorySwap": 2048, "PidsLimit": 256,
                "PortBindings": {"41273/tcp": [{"HostIp": "127.0.0.1", "HostPort": ""}]}},
            "NetworkSettings": {"Ports": {"41273/tcp": [{"HostIp": "127.0.0.1", "HostPort": "49152"}]}}}
        runtime._validate_sample_inspection(value, "image", {}, 2, 2048, 256)
        value["NetworkSettings"]["Ports"]["41273/tcp"][0]["HostIp"] = "0.0.0.0"
        with self.assertRaisesRegex(runtime.RuntimeFailure, "loopback"):
            runtime._validate_sample_inspection(value, "image", {}, 2, 2048, 256)
        value["Mounts"] = [{"Type": "bind"}]
        with self.assertRaisesRegex(runtime.RuntimeFailure, "runtime mount"):
            runtime._validate_sample_inspection(value, "image", {}, 2, 2048, 256)

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



if __name__ == "__main__":
    unittest.main()
