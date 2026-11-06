"""The progress contract of the package (#37), by running it: which calls
take ``on_progress``; what ``project.init`` reports in this process; that the
events arrive while the call runs, before it returns and not after, on the
calling thread (sync) or the event loop's thread (aio); and what a raising
function does. Init into a Store on this host needs no machine, daemon or
bucket. The page that states this:
docs/architecture/api/call-table/events.md.

Build first, as test_calls.py says: cargo build -p layerfs-python, then copy
target/debug/lib_native.(dylib|so) to src/layerfs/_native.abi3.so.
"""

import asyncio
import os
import shutil
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "src"))

import layerfs  # noqa: E402
from layerfs import aio  # noqa: E402
from layerfs._client import TABLE  # noqa: E402

PHASES = [
    {"phase": "check", "done": None, "total": None},
    {"phase": "install", "done": None, "total": None},
    {"phase": "record", "done": None, "total": None},
]


def temp(tag):
    path = tempfile.mkdtemp(prefix=f"layerfs-py-progress-{tag}-")
    os.chmod(path, 0o700)
    return path


def folder_of(files):
    folder = temp("folder")
    for i in range(files):
        directory = Path(folder, f"d{i % 20}")
        directory.mkdir(exist_ok=True)
        (directory / f"f{i}.txt").write_text(f"file {i}\n")
    return folder


def declaring(option):
    return [
        f"{spec['family']}.{spec['call']}"
        for spec in TABLE
        if any(o["name"] == option and o["kind"] == "function" for o in spec["options"])
    ]


class Progress(unittest.TestCase):
    def setUp(self):
        self.home, self.folder = temp("home"), folder_of(2000)
        self.addCleanup(shutil.rmtree, self.home, True)
        self.addCleanup(shutil.rmtree, self.folder, True)

    def test_the_table_five_calls_take_on_progress_and_machine_create_does_not(self):
        self.assertEqual(
            declaring("on_progress"),
            ["project.init", "store.push", "store.pull", "backup.create", "backup.restore"],
        )
        self.assertEqual(declaring("stdout"), ["workspace.exec"])
        self.assertEqual(declaring("stderr"), ["workspace.exec"])
        # the library emits { phase: "create" } for machine.create, but no function can be given
        with self.assertRaisesRegex(layerfs.LayerFSError, 'unknown option "on_progress"') as raised:
            layerfs.client(home=self.home).machine.create("m1", "lima", on_progress=print)
        self.assertEqual(raised.exception.code, "InvalidInput")

    def test_sync_init_reports_check_install_record_while_it_runs_before_it_returns(self):
        lfs = layerfs.client(home=self.home)
        lfs.store.create("shop")
        events, returned = [], [False]

        def on_progress(progress):
            events.append((time.perf_counter(), progress, threading.current_thread(), returned[0]))

        made = lfs.project.init("shop/api", self.folder, on_progress=on_progress)
        returned[0] = True
        after = time.perf_counter()
        self.assertEqual(made["project"], "shop/api")
        self.assertEqual([e[1] for e in events], PHASES)
        # on the calling thread, before the call returned
        self.assertEqual({e[2] for e in events}, {threading.current_thread()})
        self.assertFalse(any(e[3] for e in events))
        # live: Init spends its time between `install` and `record`
        self.assertGreater(events[2][0] - events[1][0], 0.01)
        self.assertGreaterEqual(after, events[2][0])
        # none after the call returned
        time.sleep(0.2)
        self.assertEqual(len(events), 3)

    def test_a_raising_function_is_reported_as_unraisable_and_the_call_goes_on(self):
        lfs = layerfs.client(home=self.home)
        lfs.store.create("shop")
        phases, unraisable = [], []
        previous = sys.unraisablehook
        sys.unraisablehook = lambda hook: unraisable.append(f"{hook.exc_type.__name__}: {hook.exc_value}")
        try:

            def on_progress(progress):
                phases.append(progress["phase"])
                raise RuntimeError(f"boom at {progress['phase']}")

            made = lfs.project.init("shop/api", self.folder, on_progress=on_progress)
        finally:
            sys.unraisablehook = previous
        self.assertEqual(made["project"], "shop/api")
        self.assertEqual(phases, ["check", "install", "record"])
        self.assertEqual(unraisable, [f"RuntimeError: boom at {p}" for p in phases])
        self.assertEqual(lfs.project.get("shop/api")["project"], "shop/api", "the Init happened")

    def test_aio_init_reports_on_the_loop_thread_before_it_returns_and_a_raise_goes_to_the_loop(self):
        lfs = aio.client(home=self.home)

        async def run():
            loop = asyncio.get_running_loop()
            problems = []
            loop.set_exception_handler(lambda _loop, context: problems.append(str(context.get("exception"))))
            await lfs.store.create("shop")
            events, returned = [], [False]

            def on_progress(progress):
                events.append((time.perf_counter(), progress, threading.current_thread(), returned[0]))
                raise RuntimeError(f"boom at {progress['phase']}")

            made = await lfs.project.init("shop/api", self.folder, on_progress=on_progress)
            returned[0] = True
            at_return = len(events)
            await asyncio.sleep(0.2)
            return made, events, at_return, len(events), problems, threading.current_thread()

        made, events, at_return, later, problems, loop_thread = asyncio.run(run())
        self.assertEqual(made["project"], "shop/api")
        self.assertEqual([e[1] for e in events], PHASES)
        self.assertEqual({e[2] for e in events}, {loop_thread})
        self.assertFalse(any(e[3] for e in events), "every event ran before the coroutine returned")
        self.assertGreater(events[2][0] - events[1][0], 0.01)
        self.assertEqual((at_return, later), (3, 3), "none after the call returned")
        self.assertEqual(problems, [f"boom at {p}" for p in ("check", "install", "record")])


if __name__ == "__main__":
    unittest.main()
