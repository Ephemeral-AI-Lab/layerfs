import unittest

from support import ContractCase
from layerfs import LayerFS, LayerFSError

SCOPE = {"folder": "/packages/editor", "branch": "team.editor"}
WS = {"sandbox": "1", "path": "/workspaces/w1"}
COMMIT = "12" + "a" * 64


class PythonScopeTests(ContractCase):
    async def open_scoped(self, options=None):
        client = await LayerFS.open("site", {"scope": dict(options or SCOPE)})
        self.addAsyncCleanup(client.close)
        return client

    async def assert_outside(self, awaitable, kind="path"):
        with self.assertRaises(LayerFSError) as ctx:
            await awaitable
        self.assertEqual(ctx.exception.code, "InvalidInput")
        self.assertEqual(ctx.exception.raw_message, f"out of scope: {kind}")

    async def calls_since(self, before):
        state = await self.native.inspect()
        return state["calls"][before:], state["args"][before:]

    async def test_scope_validation(self):
        invalid = [
            None, [], "scope", {}, {"branch": "team"}, {"folder": "/a", "unknown": True},
            {"folder": "relative", "branch": "team"}, {"folder": "", "branch": "team"},
            {"folder": "/a/", "branch": "team"}, {"folder": "/a//b", "branch": "team"},
            {"folder": "/a/./b", "branch": "team"}, {"folder": "/a/../b", "branch": "team"},
            {"folder": "/a", "branch": ""}, {"folder": "/a", "branch": "team/editor"},
            {"folder": "/a", "branch": "Team"}, {"folder": "/a", "branch": ".team"},
            {"folder": "/a", "branch": "team."}, {"folder": "/a", "branch": "a" * 64},
        ]
        for scope in invalid:
            with self.subTest(scope=scope):
                with self.assertRaises(LayerFSError) as ctx:
                    await LayerFS.open("site", {"scope": scope})
                self.assertEqual(ctx.exception.code, "InvalidInput")
        with self.assertRaises(LayerFSError) as ctx:
            await LayerFS.init("fresh", "/tmp/fresh", options={"scope": SCOPE})
        self.assertEqual(ctx.exception.code, "InvalidInput")
        self.assertEqual(
            ctx.exception.raw_message,
            "LayerFS.init does not accept scope; open the existing project with a scope instead",
        )
        root = await self.open_scoped({"folder": "/", "branch": "a"})

    async def test_file_operations_preserve_arguments_and_results(self):
        plain = self.client
        scoped = await self.open_scoped()
        prefix = "team.editor"

        async def pair(method, args):
            await plain.call(method, args)
            expected = (await self.native.inspect())["args"][-1]
            await scoped.call(method, args)
            actual = (await self.native.inspect())["args"][-1]
            self.assertEqual(actual, {**expected, "branch": prefix + ".main"} if "branch" in args else expected)

        before = len((await self.native.inspect())["calls"])
        await pair("fs.list", {"branch": "main", "path": "/packages/editor", "pageSize": 5, "pagePos": 0})
        await pair("fs.read", {"branch": "main", "path": "/packages/editor/src/index.js", "startLine": 1, "endLine": 5})
        await pair("fs.write", {"branch": "main", "path": "/packages/editor/a", "content": "new", "message": "write"})
        await pair("fs.cp", {"branch": "main", "from": "/packages/editor/a", "to": "/packages/editor/b"})
        calls, _ = await self.calls_since(before)
        self.assertEqual(calls, ["fs.list", "fs.list", "fs.read", "fs.read", "fs.write", "fs.write", "fs.cp", "fs.cp"])

        # fs.diff_files maps the Branch names on both sides while keeping the paths.
        await scoped.call("fs.diff_files", {
            "branch": "main",
            "left": {"commit": "main", "path": "/packages/editor/a"},
            "right": {"commit": "feature", "path": "/packages/editor/b"},
        })
        args = (await self.native.inspect())["args"][-1]
        self.assertEqual(args["branch"], "team.editor.main")
        self.assertEqual(args["left"], {"commit": "team.editor.main", "path": "/packages/editor/a"})
        self.assertEqual(args["right"], {"commit": "team.editor.feature", "path": "/packages/editor/b"})

    async def test_out_of_scope_paths_refuse_before_dispatch(self):
        client = await self.open_scoped()
        before = len((await self.native.inspect())["calls"])
        for call in (
            lambda: client.fs.list("main", "/packages"),
            lambda: client.fs.stat("main", "/packages/editor2"),
            lambda: client.fs.read("main", "/packages/editor/../other"),
            lambda: client.fs.read("main", "/packages/editor//src"),
            lambda: client.fs.read("main", "packages/editor/src"),
            lambda: client.fs.apply("main", [
                {"op": "write", "path": "/packages/editor/a", "content": "x"},
                {"op": "rm", "path": "/packages/other"},
            ]),
            lambda: client.fs.apply("main", [{"op": "cp", "from": "/packages/other/a", "to": "/packages/editor/a"}]),
            lambda: client.fs.cp("main", "/packages/editor/a", "/packages/other/a"),
            lambda: client.fs.diff_files("main", "/packages/other/a", "main", "/packages/editor/a"),
            lambda: client.call("fs.read", {"branch": "main", "path": "/packages/other.txt"}),
        ):
            await self.assert_outside(call())
        self.assertEqual(len((await self.native.inspect())["calls"]), before)
        await client.fs.read("main", "/packages/editor/src")
        await client.fs.read("main", "/packages/editor")
        root = await self.open_scoped({"folder": "/", "branch": "team"})
        await root.fs.read("main", "/any/canonical/path")

    async def test_refs_map_branch_names_and_preserve_identities(self):
        client = await self.open_scoped()
        await client.fs.read("main", "/packages/editor/a")
        self.assertEqual((await self.native.inspect())["args"][-1]["branch"], "team.editor.main")
        await client.fs.read(COMMIT, "/packages/editor/a")
        self.assertEqual((await self.native.inspect())["args"][-1]["branch"], COMMIT)
        await client.branch.fork("from-commit", COMMIT)
        self.assertEqual((await self.native.inspect())["args"][-1]["from"], COMMIT)
        files_only = await self.open_scoped({"folder": "/packages/editor"})
        await files_only.fs.read(COMMIT, "/packages/editor/a")
        self.assertEqual((await self.native.inspect())["args"][-1]["branch"], COMMIT)
        await self.assert_outside(files_only.fs.read("main", "/packages/editor/a"), "branch")

    async def test_history_and_events_refuse_before_dispatch(self):
        client = await self.open_scoped()
        before = len((await self.native.inspect())["calls"])
        for call in (
            lambda: client.branch.list_commits("main"),
            lambda: client.branch.graph("main"),
            lambda: client.branch.changed_paths("c0ffee"),
            lambda: client.branch.diff_commits("c0ffee", "decaf"),
            lambda: client.branch.diff_branches("main", "feature"),
            lambda: client.call("branch.list_commits", {"branch": "main"}),
        ):
            await self.assert_outside(call(), "history")
        await self.assert_outside(client.call("events.subscribe", {}), "events")
        self.assertEqual(len((await self.native.inspect())["calls"]), before)

    async def test_branch_mutation_and_publication_stay_in_the_namespace(self):
        client = await self.open_scoped()
        await client.branch.fork("feature", "main")
        args = (await self.native.inspect())["args"][-1]
        self.assertEqual((args["name"], args["from"]), ("team.editor.feature", "team.editor.main"))
        await client.branch.merge("main", "feature", "merged", "merge")
        args = (await self.native.inspect())["args"][-1]
        self.assertEqual(
            (args["branch"], args["with"], args["name"]),
            ("team.editor.main", "team.editor.feature", "team.editor.merged"),
        )
        await client.branch.publish("feature", "c0ffee")
        self.assertEqual((await self.native.inspect())["args"][-1]["branch"], "team.editor.feature")
        before = len((await self.native.inspect())["calls"])
        for call in (
            lambda: client.fs.read("team/editor/main", "/packages/editor/a"),
            lambda: client.branch.fork("../escape", "main"),
            lambda: client.branch.merge("main", "outsider/main", "merged"),
            lambda: client.branch.publish("Main", "c0ffee"),
            lambda: client.call("branch.fork", {"name": "a" * 64, "from": "main"}),
        ):
            await self.assert_outside(call(), "branch")
        self.assertEqual(len((await self.native.inspect())["calls"]), before)
        long = await self.open_scoped({"folder": "/packages/editor", "branch": "a" * 60})
        long_before = len((await self.native.inspect())["calls"])
        await self.assert_outside(long.branch.fork("bbbb", "main"), "branch")
        self.assertEqual(len((await self.native.inspect())["calls"]), long_before)
        files_only = await self.open_scoped({"folder": "/packages/editor"})
        files_before = len((await self.native.inspect())["calls"])
        await self.assert_outside(files_only.branch.fork("feature", "main"), "branch")
        await self.assert_outside(files_only.project.list_branches(), "branch")
        self.assertEqual(len((await self.native.inspect())["calls"]), files_before)

    async def test_branch_listing_filters_and_strips_with_scoped_paging(self):
        await self.native.prepare({"branchList": ["main", "team.editor.main", "team.other", "team.editor.feature", "team.editor2.bad", "team.editor.review"]})
        try:
            client = await self.open_scoped()
            first = await client.project.list_branches(2, 0)
            self.assertEqual([item["name"] for item in first["items"]], ["main", "feature"])
            self.assertTrue(first["hasMore"])
            self.assertEqual(first["nextPagePos"], 2)
            second = await client.call("project.list_branches", {"pageSize": 2, "pagePos": 2})
            self.assertEqual([item["name"] for item in second["items"]], ["review"])
            self.assertFalse(second["hasMore"])
            self.assertIsNone(second["nextPagePos"])
        finally:
            await self.native.prepare({"branchList": None})

    async def test_workspace_mount_and_exec_bind_to_the_scope(self):
        client = await self.open_scoped()
        mounted = await client.workspace.mount("1")
        self.assertEqual(mounted, {"sandbox": "1", "path": "/workspaces/w1", "image": "fake-image"})
        self.assertEqual((await self.native.inspect())["args"][-1]["branch"], "team.editor.main")
        await client.call("workspace.mount", {"sandbox": "1"})
        self.assertEqual((await self.native.inspect())["args"][-1]["branch"], "team.editor.main")
        before = len((await self.native.inspect())["calls"])
        await self.assert_outside(client.workspace.mount("1", "feature"), "branch")
        await self.assert_outside(client.call("workspace.mount", {"sandbox": "1", "branch": "feature"}), "branch")
        await self.assert_outside(client.call("workspace.mount", {"sandbox": "1", "branch": None}), "branch")
        self.assertEqual(len((await self.native.inspect())["calls"]), before)
        files_only = await self.open_scoped({"folder": "/packages/editor"})
        await self.assert_outside(files_only.workspace.mount("1"), "branch")

        await client.workspace.exec(WS, "exit 0", "scope test")
        exec_args = (await self.native.inspect())["args"][-1]
        self.assertEqual(exec_args["cwd"], "/packages/editor")
        await client.call("workspace.exec", {**WS, "command": "exit 0"})
        self.assertEqual((await self.native.inspect())["args"][-1]["cwd"], "/packages/editor")
        before = len((await self.native.inspect())["calls"])
        await self.assert_outside(client.call("workspace.exec", {**WS, "command": "pwd", "cwd": "/"}))
        self.assertEqual(len((await self.native.inspect())["calls"]), before)

    async def test_backend_unavailable_and_unscoped_shapes_unchanged(self):
        client = await self.open_scoped()
        self.assertIsNone(client.backend)
        plain = self.client
        await plain.workspace.exec(WS, "exit 0")
        self.assertNotIn("cwd", (await self.native.inspect())["args"][-1])
        await plain.workspace.mount("1", "feature")
        self.assertEqual((await self.native.inspect())["args"][-1]["branch"], "feature")
        await plain.fs.read("main", "/packages/editor/a")
        self.assertEqual((await self.native.inspect())["args"][-1]["branch"], "main")


if __name__ == "__main__":
    unittest.main()
