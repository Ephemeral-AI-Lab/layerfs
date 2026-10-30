import asyncio
import json
import math
from pathlib import Path
import tempfile

from support import ContractCase, ENV


class PythonResultsTests(ContractCase):
    async def test_open_write_refusal_backend_and_close(self):
        opened = self.client.results()
        self.assertEqual([item["method"] for item in opened["items"]], ["project.list_branches"])
        self.assertEqual((opened["next"], opened["evicted"]), (1, 0))
        before = (await self.native.inspect())["calls"]
        self.assertEqual(self.client.results(), opened)
        self.assertEqual((await self.native.inspect())["calls"], before)

        await self.client.project.list_branches()
        await self.client.fs.list("main", "/")
        all_records = self.client.results()
        self.assertEqual([item["seq"] for item in all_records["items"]], [1, 2, 3])

        await self.client.fs.write("feature", "/big.txt", "hello", "Write big")
        await self.client.fs.rm("feature", "/dir")
        await self.client.workspace.commit({"sandbox": "1", "path": "/workspaces/w1"}, "Save")
        await self.client.branch.merge("main", "feature", "merged")
        await self.client.branch.publish("feature")
        await self.client.branch.fork("p10-fork")
        await self.client.fs.read("main", "/README.md", 1, 20)
        records = self.client.results(3)["items"]
        by_method = {item["method"]: item for item in records}
        self.assertEqual(by_method["fs.write"]["kind"], "Committed")
        self.assertEqual(by_method["fs.write"]["commit"], "c0ffee")
        self.assertEqual(
            by_method["fs.write"]["sizes"],
            {"logical": 120, "newCanonical": 40, "newStored": 30, "newMetadata": 8},
        )
        self.assertEqual(by_method["fs.rm"]["kind"], "UpToDate")
        self.assertEqual(by_method["fs.rm"]["commit"], "h3ad")
        self.assertEqual(by_method["branch.merge"]["commit"], "m3rge")
        self.assertEqual(by_method["branch.publish"]["layer"], "lay1")
        self.assertNotIn("commit", by_method["branch.fork"])
        self.assertNotIn("kind", by_method["fs.read"])

        cursor = self.client.results()["next"]
        with self.assertRaisesRegex(Exception, "does not exist"):
            await self.client.fs.read("main", "/missing")
        self.native.refuse = ("fs.list",)
        with self.assertRaises(Exception) as refused:
            await self.client.fs.list("main", "/")
        self.native.refuse = ()
        self.assertEqual(refused.exception.code, "Conflict")
        failures = self.client.results(cursor)["items"]
        self.assertEqual([item["code"] for item in failures], ["NoSuchPath", "Conflict"])
        self.assertEqual(failures[1]["message"], "fs.list refused by the fake")

        cursor = self.client.results()["next"]
        with self.assertRaises(Exception):
            await self.client.call("backend.store.project.get_history", {})
        with self.assertRaises(Exception):
            await self.client.project.list_branches(0)
        self.assertEqual(self.client.results()["next"], cursor)

        cursor = self.client.results()["next"]
        await self.client.backend.store.project.get_storage()
        backend = self.client.results(cursor)["items"]
        self.assertEqual([item["method"] for item in backend], ["backend.store.project.get_storage"])
        await self.client.close()
        self.assertEqual(self.client.results()["next"], backend[0]["seq"])
        self.assertGreaterEqual(self.client.results()["items"][-1]["ms"], 0)

    async def test_parallel_paging_validation_init_and_eviction(self):
        cursor = self.client.results()["next"]
        await asyncio.gather(
            self.client.fs.write("feature", "/a.txt", "a", "one"),
            self.client.fs.read("main", "/README.md", 1, 20),
            self.client.project.list_branches(),
        )
        parallel = self.client.results(cursor)["items"]
        self.assertEqual([item["seq"] for item in parallel], [cursor + 1, cursor + 2, cursor + 3])
        self.assertEqual(
            sorted(item["method"] for item in parallel),
            ["fs.read", "fs.write", "project.list_branches"],
        )
        last = self.client.results()["next"]
        self.assertEqual([item["seq"] for item in self.client.results(last - 2)["items"]], [last - 1, last])
        self.assertEqual(self.client.results(last), {"items": [], "next": last, "evicted": 0})
        self.assertEqual(self.client.results(None), self.client.results())
        for invalid in (-1, 1.5, math.nan, "1", {}, [], 2**53, math.inf, True):
            with self.subTest(after=invalid), self.assertRaises(Exception) as raised:
                self.client.results(invalid)
            self.assertEqual(raised.exception.code, "InvalidInput")

        made = await self.LayerFS.init("p10init", "/src/site")
        self.addAsyncCleanup(made.close)
        self.assertEqual(
            [item["method"] for item in made.results()["items"]],
            ["project.init", "project.list_branches"],
        )
        self.assertEqual(made.results()["items"][0]["kind"], "Initialized")

        heavy = await self.LayerFS.open("site")
        self.addAsyncCleanup(heavy.close)
        for _ in range(1030):
            await heavy.project.list_branches()
        total = 1031
        page = heavy.results()
        self.assertEqual(len(page["items"]), 1024)
        self.assertEqual(page["evicted"], total - 1024)
        self.assertTrue(page["stale"])
        self.assertEqual(page["items"][0]["seq"], total - 1024 + 1)
        self.assertNotIn("stale", heavy.results(total - 1024))
        self.assertTrue(heavy.results(total - 1024 - 2)["stale"])

    async def test_trace_agreement(self):
        with tempfile.TemporaryDirectory(prefix="layerfs-p4-results-") as directory:
            trace = str(Path(directory) / "trace.jsonl")
            client = await self.LayerFS.open("site", {"trace": trace})
            self.addAsyncCleanup(client.close)
            await client.fs.write("feature", "/big.txt", "hello", "Write big")
            with self.assertRaises(Exception):
                await client.fs.read("main", "/missing")
            await client.branch.publish("feature")
            with open(trace, encoding="utf-8") as lines:
                traced = [json.loads(line) for line in lines if line.strip()]
            records = client.results()["items"]
            self.assertEqual(len(records), len(traced))
            fields = ("method", "start", "ms", "ok", "code", "message", "kind", "commit", "layer", "sizes")
            for record, line in zip(records, traced):
                self.assertEqual(
                    {key: record[key] for key in fields if key in record},
                    {key: line[key] for key in fields if key in line},
                )


class PythonStorageOptionsTests(ContractCase):
    async def test_get_storage_options_and_numbers(self):
        await self.native.prepare(
            {
                "storageResult": {
                    "logical": 1000,
                    "canonical": 800,
                    "stored": 600,
                    "metadata": 200,
                    "file": 16384,
                    "free": 4096,
                    "dedupe": "1.250000",
                    "compression": "1.333333",
                    "init": None,
                    "disk": {"main": 12288, "wal": 4096, "total": 16384},
                    "commits": {
                        "Workspace": {"count": 3, "newCanonical": 1200, "newStored": 900, "newMetadata": 300},
                        "Files": {"count": 1, "newCanonical": 400, "newStored": 300, "newMetadata": 100},
                        "Merge": {"count": 0, "newCanonical": 0, "newStored": 0, "newMetadata": 0},
                    },
                }
            }
        )
        project = self.client.backend.store.project
        default = await project.get_storage()
        self.assertEqual(default["dedupe"], 1.25)
        self.assertEqual(default["compression"], 1.333333)
        self.assertEqual(default["disk"], {"main": 12288, "wal": 4096, "total": 16384})
        commits = await project.get_storage({"commits": True})
        self.assertEqual(commits["commits"]["Merge"]["count"], 0)
        before = await self.native.inspect()
        for invalid in (None, "bad", 123, True, [1, 2], {"unknown": True}, {"commits": "yes"}, {"commits": 1}, {"commits": None}, {"commits": {}}):
            with self.subTest(options=invalid), self.assertRaises(Exception) as raised:
                await project.get_storage(invalid)
            self.assertEqual(raised.exception.code, "InvalidInput")
        after = await self.native.inspect()
        self.assertEqual(len(after["calls"]) - len(before["calls"]), 0)


if __name__ == "__main__":
    import unittest

    unittest.main()
