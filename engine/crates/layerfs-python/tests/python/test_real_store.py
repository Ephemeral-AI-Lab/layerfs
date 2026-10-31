import os
from pathlib import Path
import shutil
import tempfile
import unittest

from layerfs import LayerFS


class LinuxRealStoreTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        if os.name != "posix":
            self.fail("the native real-Store test requires Linux")
        scratch = os.environ.get("LAYERFS_TEST_TMPDIR", "/var/tmp" if os.name == "posix" else None)
        self.directory = tempfile.mkdtemp(prefix="layerfs-p4-real-", dir=scratch)
        self.addCleanup(shutil.rmtree, self.directory, True)
        root = Path(self.directory)
        self.source = root / "source"
        self.source.mkdir()
        (self.source / "README.md").write_text("initial\n", encoding="utf-8")
        (self.source / "src").mkdir()
        (self.source / "src" / "app.txt").write_text("app\n", encoding="utf-8")
        volume = root / "volume" / "global"
        volume.mkdir(parents=True, exist_ok=True)
        (root / "home").mkdir(parents=True, exist_ok=True)
        self.options = {
            "home": str(root / "home"),
            "storePath": str(volume / "store.sqlite"),
        }

    async def test_init_open_fs_branch_history_and_storage(self):
        client = await self.LayerFS_init()
        async with client:
            self.assertEqual(client.created["kind"], "Initialized")
            listing = await client.fs.list("main", "/")
            self.assertIn("README.md", {item["name"] for item in listing["items"]})
            read = await client.fs.read("main", "/README.md")
            self.assertEqual(read["text"], "initial\n")

            forked = await client.branch.fork("feature")
            self.assertEqual(forked["kind"], "Forked")
            feature = await client.fs.write("feature", "/src/new.txt", "new\n", "Add feature")
            self.assertEqual(feature["kind"], "Committed")

            commits = await client.branch.list_commits("feature")
            self.assertGreaterEqual(len(commits["items"]), 1)
            self.assertEqual(commits["items"][0]["id"], feature["commit"]["id"])
            graph = await client.branch.graph("feature")
            self.assertEqual(graph["branch"]["name"], "feature")
            changed = await client.branch.changed_paths(feature["commit"]["id"])
            self.assertIn("/src/new.txt", {item["path"] for item in changed["items"]})

            storage = await client.backend.store.project.get_storage({"commits": True})
            self.assertGreater(storage["disk"]["total"], 0)
            self.assertGreaterEqual(storage["commits"]["Files"]["count"], 1)
            self.assertIn("Workspace", storage["commits"])
            self.assertIn("Merge", storage["commits"])

        reopened = await LayerFS.open("real", self.options)
        async with reopened:
            self.assertEqual((await reopened.fs.read("main", "/README.md"))["text"], "initial\n")
            self.assertEqual((await reopened.fs.read("feature", "/src/new.txt"))["text"], "new\n")

    async def LayerFS_init(self):
        return await LayerFS.init("real", str(self.source), 300000, self.options)

    async def test_project_summary(self):
        client = await self.LayerFS_init()
        async with client:
            summary = await client.backend.store.project.get_summary()
            self.assertEqual(summary["branches"], 1)
            self.assertEqual(summary["commits"], 0)
            self.assertIsNone(summary["mainLayer"])
            self.assertIsNone(summary["lastCommit"])
            self.assertGreater(summary["logical"], 0)


if __name__ == "__main__":
    unittest.main()
