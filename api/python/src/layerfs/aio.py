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
        live = [True]

        def on_loop(function):
            def deliver(*values):
                if live[0]:
                    function(*values)

            def marshal(*values):
                if live[0]:
                    try:
                        loop.call_soon_threadsafe(deliver, *values)
                    except RuntimeError:
                        # the loop has closed: nothing is left to deliver to
                        live[0] = False

            return marshal

        if keywords.get("stream") is True:
            _streamed(keywords)
        for name in functions:
            if callable(keywords.get(name)):
                keywords[name] = on_loop(keywords[name])
        try:
            # the events were queued on the loop before the call's result,
            # so each runs before this returns
            return await asyncio.to_thread(sync, *given, **keywords)
        finally:
            live[0] = False

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
step = _default.step

__all__ = ["AsyncClient", "LayerFSError", "client"] + FAMILIES
