"""The synchronous and the async package make the same calls, with the same
answers and the same refusals, over a real mountless file and history flow
(#35): no machine, no Sandbox, no daemon. Build first, as for test_calls.py;
temp homes need a filesystem with SQLite locks (ext4, xfs, btrfs): point
TMPDIR at one.
"""

import asyncio
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "src"))

import layerfs  # noqa: E402
from layerfs import aio  # noqa: E402

P = "shop/api"


def refusal(call, *args, **keywords):
    try:
        return {"value": call(*args, **keywords)}
    except layerfs.LayerFSError as error:
        return {"refused": (error.code, error.message, error.detail)}


async def arefusal(call, *args, **keywords):
    try:
        return {"value": await call(*args, **keywords)}
    except layerfs.LayerFSError as error:
        return {"refused": (error.code, error.message, error.detail)}


class Flow(unittest.TestCase):
    def setUp(self):
        self.home = tempfile.mkdtemp(prefix="layerfs-py-flow-")
        self.folder = tempfile.mkdtemp(prefix="layerfs-py-flow-folder-")
        Path(self.folder, "README.md").write_text("hello flow\nsecond line\n")
        self.addCleanup(shutil.rmtree, self.home)
        self.addCleanup(shutil.rmtree, self.folder)
        self.addCleanup(layerfs.close)

    def test_a_flow_written_through_aio_reads_the_same_through_both(self):
        sync, lfs = layerfs.client(home=self.home), aio.client(home=self.home)
        progress = []

        async def write():
            await lfs.store.create("shop")
            await lfs.project.init(P, self.folder, on_progress=progress.append)
            await lfs.branch.fork(P, "agent", "main")
            await lfs.file.mkdir(P, "agent", "/docs", message="dir")
            await lfs.file.write(P, "agent", "/docs/a.txt", "alpha\nbeta\n", message="a")
            await lfs.file.write(P, "agent", "/docs/b.bin", b"\x00\xff\x01", message="b")
            await lfs.file.edit(P, "agent", "/docs/a.txt", old="beta", new="gamma", message="edit")
            await lfs.file.cp(P, "agent", "/docs/a.txt", "/docs/c.txt", message="cp")
            await lfs.file.mv(P, "agent", "/docs/c.txt", "/docs/d.txt", message="mv")
            await lfs.file.rm(P, "agent", "/docs/b.bin", message="rm")

        asyncio.run(write())
        self.assertTrue(progress, "on_progress ran on the loop before init returned")
        self.assertEqual(sync.file.read(P, "agent", "/docs/a.txt")["text"], "alpha\ngamma\n")
        self.assertEqual(sync.file.read(P, "agent", "/docs/d.txt")["text"], "alpha\ngamma\n")
        self.assertEqual(refusal(sync.file.stat, P, "agent", "/docs/b.bin")["refused"][0], "NoSuchPath")
        reads = [
            ("project", "get", [P], {}),
            ("branch", "list", [P], {}),
            ("branch", "get", [P, "agent"], {}),
            ("branch", "diff", [P, "main", "agent"], {}),
            ("commit", "list", [P, "agent"], {"page_size": 2}),
            ("file", "read", [P, "agent", "/docs/a.txt"], {"start_line": 2}),
            ("file", "stat", [P, "agent", "/docs"], {}),
            ("file", "list", [P, "agent", "/"], {}),
            ("file", "glob", [P, "agent", "/", "**/*.txt"], {}),
            ("file", "grep", [P, "agent", "/", "gamma"], {}),
            ("file", "blame", [P, "agent", "/docs/a.txt"], {}),
            ("file", "diff", [P, "main", "/README.md", "agent", "/docs/a.txt"], {}),
            # refusals are the same refusals
            ("file", "read", [P, "agent", "/nosuch"], {}),
            ("branch", "get", [P, "nosuch"], {}),
            ("file", "read", ["nosuch/x", "main", "/a"], {}),
            ("file", "read", [P, "agent", "/docs/a.txt"], {"start_line": 0}),
        ]

        async def read_all():
            return [await arefusal(getattr(getattr(lfs, f), c), *a, **k) for f, c, a, k in reads]

        from_aio = asyncio.run(read_all())
        from_sync = [refusal(getattr(getattr(sync, f), c), *a, **k) for f, c, a, k in reads]
        self.assertEqual(from_aio, from_sync)
        self.assertEqual(sum("refused" in answer for answer in from_sync), 4)
        # a page past the first: the cursor-free page position, same both ways
        first = sync.commit.list(P, "agent", page_size=2)
        self.assertEqual(len(first["items"]), 2)
        self.assertEqual(
            asyncio.run(lfs.commit.list(P, "agent", page_size=2, page_pos=2)),
            sync.commit.list(P, "agent", page_size=2, page_pos=2),
        )

    def test_the_history_of_the_flow_is_one_commit_per_write(self):
        sync = layerfs.client(home=self.home)
        sync.store.create("shop")
        sync.project.init(P, self.folder)
        sync.branch.fork(P, "agent", "main")
        sync.file.write(P, "agent", "/one", "1\n", message="one")
        sync.file.write(P, "agent", "/two", "2\n", message="two")
        messages = [commit["message"] for commit in sync.commit.list(P, "agent")["items"]]
        self.assertEqual(messages[:2], ["two", "one"])
        oldest = sync.commit.list(P, "agent")["items"][-1]["id"]
        # a Commit id as the optional last positional reads that history
        self.assertEqual(sync.file.stat(P, "agent", "/README.md", oldest)["path"], "/README.md")
        self.assertEqual(refusal(sync.file.stat, P, "agent", "/two", oldest)["refused"][0], "NoSuchPath")

    def test_a_scope_refuses_the_same_way_through_both(self):
        sync = layerfs.client(home=self.home)
        sync.store.create("shop")
        sync.project.init(P, self.folder)
        scope = {"project": P, "folder": "/web"}
        scoped, ascoped = layerfs.client(home=self.home, scope=scope), aio.client(home=self.home, scope=scope)
        calls = [
            ("file", "stat", [P, "main", "/README.md"]),
            ("store", "remove", ["shop"]),
            ("file", "stat", ["shop/other", "main", "/web/a"]),
        ]

        async def run():
            return [await arefusal(getattr(getattr(ascoped, f), c), *a) for f, c, a in calls]

        from_sync = [refusal(getattr(getattr(scoped, f), c), *a) for f, c, a in calls]
        self.assertEqual(asyncio.run(run()), from_sync)
        self.assertEqual(
            [answer["refused"][1] for answer in from_sync],
            ["out of scope: path", "out of scope: store.remove", "out of scope: project"],
        )


if __name__ == "__main__":
    unittest.main()
