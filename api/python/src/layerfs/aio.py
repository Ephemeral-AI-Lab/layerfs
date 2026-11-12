"""The same calls as coroutines (O19): each runs the synchronous call on a
thread, where the native call releases the interpreter. Its functions
(``on_progress``, ``stdout``, ``stderr``) run on the event loop's thread,
in order, all before the coroutine returns.

Cancelling the coroutine stops its functions at once, but not the call: a
native call cannot be cancelled, so it runs to its end on its thread and
its outcome is dropped. Read the state again (``list``, ``get``,
``status``) before acting on what it may have done.

    from layerfs import aio
    await aio.store.create("shop")
    lfs = aio.client(home="/data/lfs")
"""

import asyncio
import functools
import threading

from ._client import _FUNCTION, FAMILIES, TABLE, Client, Family, _streamed
from ._errors import LayerFSError


class AsyncClient:
    """``Client`` with each call a coroutine function."""

    def __init__(self, home=None, scope=None):
        self._sync = Client(home, scope)
        for family in FAMILIES:
            setattr(self, family, Family(family))
        for spec in TABLE:
            sync = getattr(getattr(self._sync, spec["family"]), spec["call"])
            functions = [opt["name"] for opt in spec["options"] if opt["kind"] == _FUNCTION]
            setattr(getattr(self, spec["family"]), spec["call"], _coroutine(sync, functions))


def _coroutine(sync, functions):
    @functools.wraps(sync)
    async def call(*given, **keywords):
        loop = asyncio.get_running_loop()
        lock = threading.Lock()
        live = True
        pending = 0
        drained = asyncio.Event()

        def on_loop(function):
            def deliver(*values):
                nonlocal pending
                try:
                    with lock:
                        active = live
                    # User code runs on the loop, outside the state lock.
                    if active:
                        function(*values)
                finally:
                    with lock:
                        pending -= 1
                        if pending == 0:
                            drained.set()

            def marshal(*values):
                nonlocal live, pending
                with lock:
                    if not live:
                        return
                    pending += 1
                    try:
                        loop.call_soon_threadsafe(deliver, *values)
                    except RuntimeError:
                        # The loop has closed; undo this undeliverable event.
                        pending -= 1
                        live = False

            return marshal

        async def drain():
            while True:
                with lock:
                    if pending == 0:
                        return
                    # Only the loop touches the Event; clear and recheck share
                    # the counter lock so a notification cannot be lost.
                    drained.clear()
                await drained.wait()

        if keywords.get("stream") is True:
            _streamed(keywords)
        for name in functions:
            if callable(keywords.get(name)):
                keywords[name] = on_loop(keywords[name])
        try:
            try:
                result = await asyncio.to_thread(sync, *given, **keywords)
            except asyncio.CancelledError:
                # Cancellation closes delivery without waiting for the worker.
                raise
            except BaseException:
                await drain()
                raise
            await drain()
            return result
        finally:
            with lock:
                live = False

    return call


def client(home=None, scope=None):
    """An async client of ``home``, with an optional folder scope."""
    return AsyncClient(home, scope)


_default = AsyncClient()
project = _default.project
branch = _default.branch
commit = _default.commit
file = _default.file
sandbox = _default.sandbox
workspace = _default.workspace
machine = _default.machine
store = _default.store
backup = _default.backup
logs = _default.logs

__all__ = ["AsyncClient", "LayerFSError", "client"] + FAMILIES
