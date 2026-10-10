import importlib.util
from pathlib import Path
import unittest

TOOLS = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("run_lifecycle", TOOLS / "run_lifecycle.py")
controller = importlib.util.module_from_spec(spec)
spec.loader.exec_module(controller)

SNAPSHOT = """DAEMON_STATUS
Name:\tlayerfs-daemon
VmHWM:\t   20000 kB
VmRSS:\t   12345 kB
RssAnon:\t    2345 kB
Threads:\t9
BACKING_BYTES
/layerfs-store/global/store.sqlite 1417285632 2768144 512
/layerfs-store/global/store.sqlite-wal 0 0 512
/layerfs-store/global/store.sqlite-journal ABSENT
OWN_CONTAINER_CGROUP
/sys/fs/cgroup/memory.stat
anon 1000
file 2000
slab 5
/sys/fs/cgroup/memory.current
4096000
/sys/fs/cgroup/memory.peak UNAVAILABLE
DAEMON_THREADS
THREAD\t1\tlayerfs-daemon\t1 2 3\t4 5 \t0 0 0 0
THREAD\t7\tlayerfs-fuse-0\t1 2 3\t4 5 \t0 0 0 0
THREAD\t8\tfuser-0\t1 2 3\t4 5 \t0 0 0 0
THREAD\t9\tfuser-0\t1 2 3\t4 5 \t0 0 0 0
THREAD\t10\tlayerfs-mount-3\t1 2 3\t4 5 \t0 0 0 0
CGROUP_CPU
usage_usec 5
KERNEL\t6.10\t8
"""
BASE = {"layerfs-daemon": 1, "layerfs-fuse-0": 1, "layerfs-overlay": 1}
MOUNTED = dict(BASE, **{"fuser-0": 1, "layerfs-mount-1": 1, "layerfs-fence-1": 1})


def points(*changes):
    value = [dict(label="before", mounted=False, directory=None, threads=dict(BASE), mount_rows=[]),
             dict(label="mounted", mounted=True, directory="/workspaces/1", threads=dict(MOUNTED),
                  mount_rows=["/workspaces/1"]),
             dict(label="after", mounted=False, directory=None, threads=dict(BASE), mount_rows=[])]
    for index, key, replacement in changes:
        value[index][key] = replacement
    return value


class Parse(unittest.TestCase):
    def test_every_section_is_read_from_the_runtime_snapshot_text(self):
        parsed = controller.parse_snapshot(SNAPSHOT)
        self.assertEqual(parsed["threads"], {"fuser-0": 2, "layerfs-daemon": 1, "layerfs-fuse-0": 1,
                                             "layerfs-mount-3": 1})
        self.assertEqual(parsed["status"], {"VmHWM": "20000 kB", "VmRSS": "12345 kB",
                                            "RssAnon": "2345 kB", "Threads": "9"})
        self.assertEqual(parsed["backing"]["/layerfs-store/global/store.sqlite"],
                         dict(bytes=1417285632, allocated=2768144 * 512))
        self.assertEqual(parsed["backing"]["/layerfs-store/global/store.sqlite-journal"], "ABSENT")
        self.assertEqual(parsed["cgroup"], {"memory.stat.anon": 1000, "memory.stat.file": 2000,
                                            "memory.current": 4096000})
        self.assertEqual(controller.per_workspace(parsed["threads"]), ["layerfs-mount-3"])


class Judge(unittest.TestCase):
    def test_the_expected_lifecycle_has_no_failure(self):
        self.assertEqual(controller.judge(points()), [])

    def test_each_deviation_is_one_failure(self):
        for change in ((0, "threads", dict(BASE, **{"layerfs-fence-9": 1})),
                       (0, "mount_rows", ["/workspaces/9"]),
                       (1, "threads", dict(BASE, **{"layerfs-mount-1": 1})),
                       (1, "threads", dict(MOUNTED, **{"layerfs-mount-2": 1})),
                       (1, "threads", dict(BASE, **{"layerfs-mount-1": 1, "layerfs-fence-2": 1})),
                       (1, "threads", dict(MOUNTED, **{"layerfs-mount-1": 2})),
                       (1, "mount_rows", []),
                       (1, "mount_rows", ["/workspaces/1", "/workspaces/2"]),
                       (2, "threads", dict(MOUNTED)),
                       (2, "threads", dict(BASE, **{"layerfs-overlay": 2})),
                       (2, "mount_rows", ["/workspaces/1"])):
            failures = controller.judge(points(change))
            if change[0]:
                self.assertEqual(len(failures), 1, change)
            else:
                # A deviation before the first mount also unsettles the later comparison.
                self.assertTrue(failures, change)

    def test_a_surviving_shared_thread_after_unmount_is_reported(self):
        self.assertEqual(len(controller.judge(points((2, "threads", dict(BASE, **{"fuser-0": 1}))))), 1)


if __name__ == "__main__":
    unittest.main()
