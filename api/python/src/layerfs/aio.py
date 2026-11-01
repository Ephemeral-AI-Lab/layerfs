"""The same calls as coroutines (O19): each runs the synchronous call on a
thread, where the native call releases the interpreter.

    from layerfs import aio
    await aio.store.create("shop")
    lfs = aio.client(home="/data/lfs")
"""

import asyncio
import functools

from ._client import FAMILIES, TABLE, Client, Family
from ._errors import LayerFSError


class AsyncClient:
    """``Client`` with each call a coroutine function."""

    def __init__(self, home=None, scope=None):
        self._sync = Client(home, scope)
        for family in FAMILIES:
            setattr(self, family, Family(family))
        for spec in TABLE:
            sync = getattr(getattr(self._sync, spec["family"]), spec["call"])
            setattr(getattr(self, spec["family"]), spec["call"], _coroutine(sync))


def _coroutine(sync):
    @functools.wraps(sync)
    async def call(*given, **keywords):
        return await asyncio.to_thread(sync, *given, **keywords)

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

__all__ = ["AsyncClient", "LayerFSError", "client"] + FAMILIES
