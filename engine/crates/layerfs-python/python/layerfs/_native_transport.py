import asyncio
from ._util import clean


class NativeTransport:
    def __init__(self):
        from . import _native

        self.native = _native

    async def call(self, op, args):
        return await asyncio.to_thread(self.native.call, op, clean(args))

    async def backend_call(self, op, args):
        return await asyncio.to_thread(self.native.backend_call, op, clean(args))

    async def store_open(self, args):
        return await asyncio.to_thread(self.native.store_open, clean(args))

    async def store_close(self, args):
        return await asyncio.to_thread(self.native.store_close, clean(args))

    async def exec_output(self, args):
        return await asyncio.to_thread(self.native.exec_output, clean(args))

    async def close(self):
        return None
