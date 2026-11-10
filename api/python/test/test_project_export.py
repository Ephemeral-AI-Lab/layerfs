"""LP53b: run the actual Python shells with a synthetic native call table.

python3 -B api/python/test/test_project_export.py
LP53_EXPORT_NATIVE=1 enables real sync/aio integration after LP53a and a native
build; missing export then FAILS rather than silently skipping. Use an external
wall timeout <=120 seconds. Stub checks prove binding behavior, not filesystem
export, Store snapshot pinning or worker transport.
"""

import asyncio
import importlib
import importlib.util
import json
import os
import sys
import tempfile
import threading
import types
import unittest
from pathlib import Path
from unittest.mock import patch

SOURCE = Path(__file__).resolve().parents[1] / "src" / "layerfs"
SPEC = {
    "family": "project",
    "call": "export",
    "help": "export a committed Branch snapshot",
    "args": [
        {"name": "project", "kind": "project", "optional": False},
        {"name": "destination", "kind": "text", "optional": False},
    ],
    "options": [
        {"name": "branch", "kind": "name", "optional": True},
        {"name": "on_progress", "kind": "function", "optional": True},
    ],
}
RESULT = {
    "project": "shop/api", "store": "shop", "name": "api", "kind": "sqlite",
    "machine": "native", "branch": "main", "snapshot": "main@0123456789ab",
    "node": "01" * 33, "root": "02" * 32, "destination": "/exports/api",
    "files": 1, "directories": 1, "symlinks": 1, "bytes": 4,
}
PROGRESS = [
    {"phase": "prepare", "files": 0, "directories": 0, "symlinks": 0, "bytes": 0, "done": None, "total": None},
    {"phase": "export", "files": 0, "directories": 0, "symlinks": 0, "bytes": 0, "done": 0, "total": None},
    {"phase": "export", "files": 1, "directories": 1, "symlinks": 1, "bytes": 4, "done": 4, "total": None},
    {"phase": "finalize", "files": 1, "directories": 1, "symlinks": 1, "bytes": 4, "done": 4, "total": None},
]

# Isolate the stub in its own package name: do not replace an installed layerfs
# or change product files. The package, _client and aio are loaded from source.
PACKAGE = "_lp53b_export"
native = types.ModuleType(f"{PACKAGE}._native")
FAMILIES = ["branch", "commit", "file", "sandbox", "workspace", "machine", "store", "backup"]
native.table = lambda: json.dumps([SPEC] + [
    {"family": family, "call": "list", "args": [], "options": [], "help": "stub"}
    for family in FAMILIES
])
sys.modules[native.__name__] = native
package_spec = importlib.util.spec_from_file_location(
    PACKAGE, SOURCE / "__init__.py", submodule_search_locations=[str(SOURCE)]
)
sdk = importlib.util.module_from_spec(package_spec)
sys.modules[PACKAGE] = sdk
package_spec.loader.exec_module(sdk)
aio = importlib.import_module(f"{PACKAGE}.aio")


def tearDownModule():
    for name in list(sys.modules):
        if name == PACKAGE or name.startswith(PACKAGE + "."):
            del sys.modules[name]


def check_progress(case, events, result):
    case.assertGreaterEqual(len(events), 3)
    case.assertEqual(events[0]["phase"], "prepare")
    case.assertEqual(events[-1]["phase"], "finalize")
    case.assertIn("export", [event["phase"] for event in events])
    previous = dict(files=0, directories=0, symlinks=0, bytes=0)
    phase = 0
    for event in events:
        next_phase = ["prepare", "export", "finalize"].index(event["phase"])
        case.assertGreaterEqual(next_phase, phase)
        phase = next_phase
        case.assertIsNone(event["total"])
        for key in ("files", "directories", "symlinks", "bytes"):
            case.assertIsInstance(event[key], int)
            case.assertGreaterEqual(event[key], previous[key])
        if event["phase"] == "prepare":
            case.assertIsNone(event["done"])
        else:
            case.assertEqual(event["done"], event["bytes"])
        previous = event
    case.assertEqual(events[-1]["files"], result["files"])
    case.assertEqual(events[-1]["directories"], result["directories"])
    case.assertEqual(events[-1]["symlinks"], result["symlinks"])
    case.assertEqual(events[-1]["bytes"], result["bytes"])


class Binding(unittest.TestCase):
    def setUp(self):
        self.calls = []

        def call(name, args, home, scope, on_event):
            self.calls.append((name, json.loads(args), home, scope, on_event))
            if on_event:
                for progress in PROGRESS:
                    on_event(json.dumps({"progress": progress}))
            return json.dumps({"value": RESULT})

        self.patcher = patch.object(native, "call", call, create=True)
        self.patcher.start()
        self.addCleanup(self.patcher.stop)

    def test_default_main_is_left_to_library_and_result_is_unchanged(self):
        self.assertEqual(sdk.project.export("shop/api", "/exports/api"), RESULT)
        self.assertEqual(len(self.calls), 1)
        name, args, home, scope, handler = self.calls[0]
        self.assertEqual(name, "project.export")
        self.assertEqual(args, {"project": "shop/api", "destination": "/exports/api"})
        self.assertIsNone(home)
        self.assertIsNone(scope)
        self.assertIsNone(handler)

    def test_keywords_home_scope_and_progress_are_forwarded_once(self):
        events = []
        scope = {"project": "shop/api", "folder": "/src", "branch": "agent"}
        result = sdk.client(home="/data/lfs", scope=scope).project.export(
            project="shop/api", destination="/exports/api", branch="agent", on_progress=events.append
        )
        self.assertEqual(len(self.calls), 1)
        name, args, home, encoded_scope, handler = self.calls[0]
        self.assertEqual(name, "project.export")
        self.assertEqual(args, {"project": "shop/api", "destination": "/exports/api", "branch": "agent"})
        self.assertEqual(home, "/data/lfs")
        self.assertEqual(json.loads(encoded_scope), scope)
        self.assertTrue(callable(handler))
        self.assertEqual(events, PROGRESS)
        check_progress(self, events, result)

    def test_commit_nonfunction_and_duplicate_arguments_fail_before_dispatch(self):
        for keywords in ({"commit": "01" * 33}, {"on_progress": None},
                         {"on_progress": "print"}, {"project": "shop/other"}):
            with self.subTest(keywords=keywords):
                with self.assertRaises(sdk.LayerFSError) as caught:
                    sdk.project.export("shop/api", "/exports/api", **keywords)
                self.assertEqual(caught.exception.code, "InvalidInput")
        with self.assertRaises(sdk.LayerFSError):
            sdk.project.export("shop/api", "/exports/api", "extra")
        self.assertEqual(self.calls, [])

    def test_refusal_detail_is_preserved_without_replay(self):
        refusal = {"code": "NotEmpty", "message": "destination changed", "detail": {"phase": "finalize"}}
        with patch.object(native, "call", return_value=json.dumps({"refused": refusal})) as dispatch:
            with self.assertRaises(sdk.LayerFSError) as caught:
                sdk.project.export("shop/api", "/exports/api")
            self.assertEqual(caught.exception.code, refusal["code"])
            self.assertEqual(caught.exception.detail, refusal["detail"])
            dispatch.assert_called_once()


class AsyncBinding(unittest.IsolatedAsyncioTestCase):
    async def test_callbacks_run_on_loop_in_order_before_return(self):
        loop_thread = threading.get_ident()
        events, threads, native_threads = [], [], []

        def call(name, args, home, scope, on_event):
            native_threads.append(threading.get_ident())
            self.assertEqual(name, "project.export")
            self.assertEqual(json.loads(args)["branch"], "agent")
            for progress in PROGRESS:
                on_event(json.dumps({"progress": progress}))
            return json.dumps({"value": RESULT})

        def handler(event):
            threads.append(threading.get_ident())
            events.append(event)

        with patch.object(native, "call", call, create=True):
            result = await asyncio.wait_for(aio.project.export(
                "shop/api", "/exports/api", branch="agent", on_progress=handler
            ), 5)
        self.assertEqual(events, PROGRESS)
        self.assertEqual(threads, [loop_thread] * len(PROGRESS))
        self.assertNotEqual(native_threads, [loop_thread])
        check_progress(self, events, result)

    async def test_a_call_that_never_yields_still_delivers_its_queued_events(self):
        # A result may resume the coroutine before callbacks already queued
        # with call_soon_threadsafe run (asyncio.wait_for does it). An
        # immediately finished to_thread makes that certain: before aio.py
        # flushed them, `finally` dropped every event.
        events = []

        def call(name, args, home, scope, on_event):
            for progress in PROGRESS:
                on_event(json.dumps({"progress": progress}))
            return json.dumps({"value": RESULT})

        async def immediate(function, *args, **keywords):
            return function(*args, **keywords)

        with patch.object(native, "call", call, create=True), patch.object(asyncio, "to_thread", immediate):
            result = await aio.project.export("shop/api", "/exports/api", on_progress=events.append)
        self.assertEqual(events, PROGRESS)
        check_progress(self, events, result)

    async def test_throwing_aio_handler_reaches_loop_handler_without_aborting_export(self):
        loop = asyncio.get_running_loop()
        errors, events = [], []
        old_handler = loop.get_exception_handler()
        loop.set_exception_handler(lambda loop, context: errors.append(context["exception"]))

        def call(name, args, home, scope, on_event):
            for progress in PROGRESS:
                on_event(json.dumps({"progress": progress}))
            return json.dumps({"value": RESULT})

        def handler(event):
            events.append(event)
            raise RuntimeError("observer failed")

        try:
            with patch.object(native, "call", call, create=True):
                result = await asyncio.wait_for(aio.project.export(
                    "shop/api", "/exports/api", on_progress=handler
                ), 5)
            self.assertEqual(result, RESULT)
            self.assertEqual(events, PROGRESS)
            self.assertEqual(len(errors), len(PROGRESS))
            self.assertTrue(all(str(error) == "observer failed" for error in errors))
        finally:
            loop.set_exception_handler(old_handler)

    async def test_cancellation_stops_observation_but_native_finishes_once(self):
        loop = asyncio.get_running_loop()
        started, finished = loop.create_future(), loop.create_future()
        release = threading.Event()
        seen, calls = [], []

        def call(name, args, home, scope, on_event):
            calls.append(name)
            on_event(json.dumps({"progress": PROGRESS[0]}))
            loop.call_soon_threadsafe(started.set_result, None)
            try:
                if not release.wait(5):
                    raise TimeoutError("test did not release native call")
                for progress in PROGRESS[1:]:
                    on_event(json.dumps({"progress": progress}))
                return json.dumps({"value": RESULT})
            finally:
                loop.call_soon_threadsafe(finished.set_result, None)

        with patch.object(native, "call", call, create=True):
            task = asyncio.create_task(aio.project.export("shop/api", "/exports/api", on_progress=seen.append))
            try:
                await asyncio.wait_for(started, 5)
                self.assertEqual(seen, PROGRESS[:1])
                task.cancel()
                with self.assertRaises(asyncio.CancelledError):
                    await task
            finally:
                release.set()
                await asyncio.wait_for(finished, 5)
                # Drain queued callbacks before checking that cancellation suppressed them.
                await asyncio.sleep(0)
                if not task.done():
                    task.cancel()
                    await asyncio.gather(task, return_exceptions=True)
        self.assertEqual(seen, PROGRESS[:1])
        self.assertEqual(calls, ["project.export"])


@unittest.skipUnless(os.environ.get("LP53_EXPORT_NATIVE") == "1", "NOT_RUN: enable native export after LP53a")
class NativeExport(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        sys.path.insert(0, str(SOURCE.parent))
        cls.real = importlib.import_module("layerfs")
        cls.real_aio = importlib.import_module("layerfs.aio")
        if not callable(getattr(cls.real.project, "export", None)):
            raise AssertionError("LP53a project.export table entry is required")

    def test_sync_aio_branch_fidelity_refusal_and_unraisable_callback(self):
        with tempfile.TemporaryDirectory(prefix="layerfs-py-lp53b-") as temporary:
            parent = Path(temporary)
            home, source = parent / "home", parent / "source"
            home.mkdir(mode=0o700)
            source.mkdir()
            (source / "empty").mkdir()
            (source / ".git").mkdir()
            (source / ".git" / "config").write_bytes(b"[core]\n")
            (source / ".gitignore").write_bytes(b"ignored.bin\n")
            (source / "ignored.bin").write_bytes(bytes([0, 255, 128, 1]))
            (source / "empty.bin").write_bytes(b"")
            (source / "héllo ☃.txt").write_text("unicode ☃\n", encoding="utf-8")
            (source / "link").symlink_to("ignored.bin")
            timestamp = 1_700_000_000_123_456_789
            os.utime(source / "ignored.bin", ns=(timestamp, timestamp))
            lfs = self.real.client(home=home)
            try:
                lfs.store.create("shop")  # public create selects only Disposable
                self.assertEqual(lfs.store.get("shop")["profile"], "disposable")
                lfs.project.init("shop/api", str(source))
                lfs.branch.fork("shop/api", "agent", "main")
                lfs.file.write("shop/api", "agent", "/branch.txt", "agent\n")
                events = []
                dest = parent / "out"
                result = lfs.project.export("shop/api", str(dest), on_progress=events.append)
                self.assertEqual(result["branch"], "main")
                self.assertEqual(result["destination"], str(dest))
                self.assertTrue(RESULT.keys() <= result.keys())
                check_progress(self, events, result)
                self.assertEqual((dest / "ignored.bin").read_bytes(), bytes([0, 255, 128, 1]))
                self.assertEqual((dest / "ignored.bin").stat().st_mtime_ns, (source / "ignored.bin").stat().st_mtime_ns)
                self.assertEqual((dest / "empty.bin").read_bytes(), b"")
                self.assertEqual((dest / "héllo ☃.txt").read_text(encoding="utf-8"), "unicode ☃\n")
                self.assertEqual((dest / "link").readlink(), Path("ignored.bin"))
                self.assertEqual(list((dest / "empty").iterdir()), [])
                self.assertEqual((dest / ".git" / "config").read_bytes(), b"[core]\n")
                with self.assertRaises(self.real.LayerFSError) as caught:
                    lfs.project.export("shop/api", str(dest))
                self.assertEqual(caught.exception.code, "NotEmpty")
                self.assertEqual((dest / "ignored.bin").read_bytes(), bytes([0, 255, 128, 1]))
                plain = parent / "plain-file"
                plain.write_text("not a folder")
                with self.assertRaises(self.real.LayerFSError) as caught:
                    lfs.project.export("shop/api", str(plain))
                self.assertIn(caught.exception.code, ("Exists", "NotDirectory"))
                self.assertEqual(plain.read_text(), "not a folder")

                async def export_branch():
                    destination = parent / "agent"
                    destination.mkdir()  # existing empty directory
                    events, threads = [], []
                    loop_thread = threading.get_ident()

                    def handler(event):
                        threads.append(threading.get_ident())
                        events.append(event)

                    result = await asyncio.wait_for(self.real_aio.client(home=home).project.export(
                        "shop/api", str(destination), branch="agent", on_progress=handler
                    ), 15)
                    check_progress(self, events, result)
                    self.assertEqual(threads, [loop_thread] * len(events))
                    self.assertEqual((destination / "branch.txt").read_text(), "agent\n")

                asyncio.run(export_branch())
                errors, seen = [], []

                def throwing(event):
                    seen.append(event)
                    raise RuntimeError("native observer failed")

                with patch.object(sys, "unraisablehook", errors.append):
                    result = lfs.project.export("shop/api", str(parent / "throwing"), on_progress=throwing)
                check_progress(self, seen, result)
                self.assertEqual(len(errors), len(seen))
                self.assertTrue(all(str(error.exc_value) == "native observer failed" for error in errors))
                self.assertFalse(any(path.name.startswith(".layerfs-export-") for path in parent.iterdir()))
            finally:
                self.real.close()


if __name__ == "__main__":
    unittest.main()
