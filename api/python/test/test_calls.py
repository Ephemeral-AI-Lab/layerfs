"""The Python package over the real native module (#533 step 16), compared
with the `layerfs` command, which step 15 compares with Node: every call of
the table is refused the same way, a live Store reads the same both ways,
and `layerfs.aio` makes the same calls as coroutines.

Build first: cargo build -p layerfs-python -p layerfs-cli, then copy
target/debug/lib_native.(dylib|so) to src/layerfs/_native.abi3.so.
"""

import asyncio
import json
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "src"))

import layerfs  # noqa: E402
from layerfs import aio  # noqa: E402
from layerfs._values import decode  # noqa: E402

ROOT = HERE.parent.parent.parent
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
COMMAND = (ROOT / TARGET if not TARGET.is_absolute() else TARGET) / "debug" / "layerfs"

REFUSING = {"project": "nosuch/x", "name": "Bad!", "text": "/x", "json": [], "content": "x", "int": 1, "size": "1G"}


def temp(tag):
    path = tempfile.mkdtemp(prefix=f"layerfs-py-{tag}-")
    os.chmod(path, 0o700)
    return path


def shell(home, words, data=b""):
    run = subprocess.run(
        [str(COMMAND), *words],
        input=data if isinstance(data, bytes) else data.encode(),
        capture_output=True,
        env={**os.environ, "LAYERFS_HOME": home},
        timeout=60,
        check=False,
    )
    if run.returncode == 0:
        return {"status": 0, "value": decode(run.stdout.decode())}
    last = run.stderr.decode().strip().splitlines()[-1]
    return {"status": run.returncode, "refused": decode(last) if last.startswith("{") else last}


def python(lfs, family, call, positionals):
    try:
        return {"value": getattr(getattr(lfs, family), call)(*positionals)}
    except layerfs.LayerFSError as error:
        return {"refused": {"code": error.code, "message": error.message, "detail": error.detail}}


class Calls(unittest.TestCase):
    def same(self, name, from_shell, from_python):
        if "refused" in from_python:
            self.assertEqual(from_shell["status"], 125, name)
            self.assertEqual(from_shell["refused"], from_python["refused"], name)
        else:
            self.assertEqual(from_shell["status"], 0, f"{name}: {from_shell.get('refused')}")
            self.assertEqual(from_shell["value"], from_python["value"], name)

    def test_the_table(self):
        self.assertEqual(
            layerfs.FAMILIES,
            ["project", "branch", "commit", "file", "sandbox", "workspace", "machine", "store", "backup"],
        )
        self.assertEqual(len(layerfs.TABLE), 64)
        for spec in layerfs.TABLE:
            self.assertTrue(callable(getattr(getattr(layerfs, spec["family"]), spec["call"])))
        with self.assertRaises(layerfs.LayerFSError) as caught:
            layerfs.client(home=temp("bogus")).store.get("shop", bogus=1)
        self.assertEqual(caught.exception.code, "InvalidInput")

    @unittest.skipUnless(COMMAND.exists(), "the layerfs command is not built")
    def test_every_call_is_refused_as_the_command_refuses_it(self):
        home = temp("refused")
        lfs = layerfs.client(home=home)
        for spec in layerfs.TABLE:
            args = [a for a in spec["args"] if not a["optional"]]
            positionals = [REFUSING[a["kind"]] for a in args]
            words, data = [spec["family"], spec["call"]], b""
            for arg, value in zip(args, positionals):
                if arg["kind"] == "content":
                    data = value
                elif arg["kind"] == "json":
                    data = json.dumps(value)
                else:
                    words.append(str(value))
            name = f"{spec['family']}.{spec['call']}"
            self.same(name, shell(home, words, data), python(lfs, spec["family"], spec["call"], positionals))
        shutil.rmtree(home)

    @unittest.skipUnless(COMMAND.exists(), "the layerfs command is not built")
    def test_a_live_store_reads_the_same_both_ways(self):
        home, folder = temp("live"), temp("folder")
        Path(folder, "README.md").write_text("hello python\n")
        lfs = layerfs.client(home=home)
        lfs.store.create("shop")
        progress = []
        made = lfs.project.init("shop/api", folder, on_progress=progress.append)
        self.assertEqual(made["project"], "shop/api")
        lfs.branch.fork("shop/api", "agent", "main")
        lfs.file.write("shop/api", "agent", "/b.bin", b"\x00\xff\x01", message="bytes")
        lfs.file.write("shop/api", "agent", "/a.txt", "from python\n")
        self.assertEqual(lfs.file.stat("shop/api", "agent", "/b.bin")["size"], 3)
        for family, call, *positionals in [
            ["store", "get", "shop"],
            ["project", "get", "shop/api"],
            ["branch", "list", "shop/api"],
            ["commit", "list", "shop/api", "agent"],
            ["file", "read", "shop/api", "agent", "/a.txt"],
            ["file", "read", "shop/api", "agent", "/b.bin"],
            ["file", "list", "shop/api", "agent", "/"],
            ["file", "grep", "shop/api", "agent", "/", "hello"],
            ["branch", "diff", "shop/api", "main", "agent"],
        ]:
            self.same(
                f"{family}.{call}", shell(home, [family, call, *positionals]), python(lfs, family, call, positionals)
            )
        # an optional positional as a keyword, and a folder scope
        first = lfs.commit.list("shop/api", "agent")["items"][-1]["id"]
        self.assertEqual(lfs.file.read("shop/api", "agent", "/README.md", commit=first)["text"], "hello python\n")
        scoped = layerfs.client(home=home, scope={"project": "shop/api", "folder": "/web"})
        with self.assertRaisesRegex(layerfs.LayerFSError, "out of scope: path"):
            scoped.file.stat("shop/api", "main", "/README.md")
        # a scope covers its project's files, Branches and Workspaces only
        with self.assertRaisesRegex(layerfs.LayerFSError, "out of scope: store.remove"):
            scoped.store.remove("shop")
        with self.assertRaisesRegex(layerfs.LayerFSError, "out of scope: project"):
            scoped.file.stat("shop/other", "main", "/web/a")
        lfs.store.get("shop")
        shutil.rmtree(home)
        shutil.rmtree(folder)

    def test_aio_makes_the_same_calls_as_coroutines(self):
        home = temp("aio")
        lfs = aio.client(home=home)

        async def run():
            await lfs.store.create("one")
            await asyncio.gather(lfs.store.create("two"), lfs.store.create("three"))
            return await lfs.store.list()

        listed = asyncio.run(run())
        self.assertEqual(sorted(s["name"] for s in listed["items"]), ["one", "three", "two"])
        shutil.rmtree(home)

    def test_aio_functions_run_on_the_loop_before_the_call_returns(self):
        home = temp("aio-loop")
        lfs = aio.client(home=home)
        seen = []

        async def run():
            await lfs.store.create("shop")
            # a push sends its first progress event, then finds no machine
            try:
                await lfs.store.push("shop", "nosuch", on_progress=lambda p: seen.append((threading.get_ident(), p)))
            except layerfs.LayerFSError as error:
                return threading.get_ident(), error.code, list(seen)
            return None

        loop_thread, code, at_return = asyncio.run(run())
        self.assertEqual(code, "NotFound")
        self.assertEqual([p["phase"] for _, p in at_return], ["check"], "delivered before the call returned")
        self.assertEqual({thread for thread, _ in at_return}, {loop_thread}, "on the loop's thread")
        shutil.rmtree(home)

    def test_a_cancelled_aio_call_stops_its_functions_and_the_call_runs_to_its_end(self):
        home, folder = temp("aio-cancel"), temp("aio-cancel-folder")
        for i in range(5000):
            Path(folder, f"f{i}.txt").write_text(f"file {i}\n")
        lfs = aio.client(home=home)
        seen = []

        async def run():
            await lfs.store.create("shop")
            loop = asyncio.get_running_loop()
            first = asyncio.Event()

            def on_progress(progress):
                seen.append(progress)
                # wakes the loop from any thread
                loop.call_soon_threadsafe(first.set)

            task = asyncio.create_task(lfs.project.init("shop/api", folder, on_progress=on_progress))
            await asyncio.wait_for(first.wait(), 60)
            task.cancel()
            with self.assertRaises(asyncio.CancelledError):
                await task
            at_cancel = len(seen)
            # the native call runs on to its end on its thread
            await asyncio.get_running_loop().shutdown_default_executor()
            await asyncio.sleep(0.2)
            return at_cancel

        at_cancel = asyncio.run(run())
        self.assertGreaterEqual(at_cancel, 1)
        self.assertEqual(len(seen), at_cancel, "no function ran after the cancel")
        self.assertEqual(layerfs.client(home=home).project.get("shop/api")["project"], "shop/api")
        shutil.rmtree(home)
        shutil.rmtree(folder)

    @unittest.skipUnless(os.environ.get("LAYERFS_DAEMON") and COMMAND.exists(), "NOT_RUN: LAYERFS_DAEMON unset")
    def test_exec_output_through_functions_and_stream_without_them(self):
        home, folder = temp("stream"), temp("stream-folder")
        Path(folder, "README.md").write_text("streamed\n")
        lfs = layerfs.client(home=home)
        lfs.store.create("shop")
        imported = subprocess.run(
            [str(COMMAND), "import-env"], env={**os.environ, "LAYERFS_HOME": home}, capture_output=True, timeout=30
        )
        self.assertEqual(imported.returncode, 0, imported.stderr)
        lfs.project.init("shop/api", folder)
        lfs.branch.fork("shop/api", "dev", "main")
        lfs.sandbox.create("shop/api", "s1", "native")
        ws = None
        try:
            ws = lfs.workspace.mount("s1", "dev")["ws"]
            out, err = [], []
            ran = lfs.workspace.exec(ws, "cat README.md; echo oops >&2", stdout=out.append, stderr=err.append)
            self.assertEqual(ran["exit_code"], 0)
            # output is text when it is UTF-8, else bytes
            self.assertEqual("".join(out), "streamed\n")
            self.assertEqual("".join(err), "oops\n")
            # stream=True and no functions: this process's own stdout and stderr
            script = (
                "import sys; sys.path.insert(0, sys.argv[1]); import layerfs; "
                "layerfs.client().workspace.exec(sys.argv[2], 'cat README.md; echo oops >&2', stream=True)"
            )
            child = subprocess.run(
                [sys.executable, "-c", script, str(HERE.parent / "src"), ws],
                env={**os.environ, "LAYERFS_HOME": home},
                capture_output=True,
                timeout=60,
            )
            self.assertEqual(child.returncode, 0, child.stderr)
            self.assertEqual(child.stdout, b"streamed\n")
            self.assertEqual(child.stderr, b"oops\n")
        finally:
            if ws:
                lfs.workspace.unmount(ws)
            lfs.sandbox.close("s1")
            shutil.rmtree(home)
            shutil.rmtree(folder)


if __name__ == "__main__":
    unittest.main()
