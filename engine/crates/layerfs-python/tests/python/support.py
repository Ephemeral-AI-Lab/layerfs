import asyncio
import base64
import json
import os
from pathlib import Path
import subprocess
import unittest
import unittest.mock

ROOT = Path(__file__).resolve().parents[5]
FIXTURE = ROOT / "engine/crates/layerfs-python/tests/fixture"
ENV = {
    "LAYERFS_HOME": "/private/cases-home",
    "LAYERFS_DOCKER": "/fake.sock",
    "LAYERFS_IMAGE": "fake-image",
    "LAYERFS_VOLUME": "fake-volume",
    "LAYERFS_DAEMON": "/fake/daemon",
    "LAYERFS_STORE_PATH": "/fake/store.sqlite",
    "LAYERFS_PROJECT": "site",
}


def plain(value):
    if isinstance(value, (bytes, bytearray, memoryview)):
        return {"base64": base64.b64encode(bytes(value)).decode("ascii")}
    if isinstance(value, list):
        return [plain(item) for item in value]
    if isinstance(value, dict):
        return {key: plain(item) for key, item in value.items()}
    return value


def contains(have, want):
    if want is None or not isinstance(want, (dict, list)):
        return have == want
    if isinstance(want, list):
        return isinstance(have, list) and len(have) == len(want) and all(
            contains(left, right) for left, right in zip(have, want)
        )
    return isinstance(have, dict) and all(
        key in have and contains(have[key], item) for key, item in want.items()
    )


def load_cases():
    completed = subprocess.run(
        ["node", str(FIXTURE / "export_cases.mjs")],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
        timeout=30,
    )
    return json.loads(completed.stdout)


class FakeNative:
    def __init__(self):
        self.process = None
        self.next_id = 1
        self.lock = asyncio.Lock()

    async def start(self):
        self.process = await asyncio.create_subprocess_exec(
            "node",
            str(FIXTURE / "fake_native_bridge.mjs"),
            cwd=ROOT,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            limit=16 * 1024 * 1024,
        )

    async def reset(self):
        await self.request(cmd="reset")

    async def request(self, **request):
        async with self.lock:
            request["id"] = self.next_id
            self.next_id += 1
            self.process.stdin.write((json.dumps(request, separators=(",", ":")) + "\n").encode())
            await self.process.stdin.drain()
            line = await asyncio.wait_for(self.process.stdout.readline(), 10)
            if not line:
                stderr = await self.process.stderr.read()
                raise AssertionError(f"fake native bridge exited: {stderr.decode()}")
            response = json.loads(line)
            assert response["id"] == request["id"]
            return response["answer"]

    async def prepare(self, values):
        await self.request(cmd="prepare", values=values)

    async def inspect(self):
        return (await self.request(cmd="inspect"))["value"]

    async def call(self, op, args):
        return await self.request(cmd="call", op=op, args=plain(args), refuse=self.refuse)

    async def backend_call(self, op, args):
        return await self.request(cmd="backendCall", op=op, args=plain(args), refuse=self.refuse)

    async def store_open(self, args):
        return await self.request(cmd="storeOpen", args=plain(args))

    async def store_close(self, args):
        return await self.request(cmd="storeClose", args=plain(args))

    async def exec_output(self, args):
        return await self.request(cmd="execOutput", args=plain(args))

    async def close(self):
        if self.process and self.process.returncode is None:
            await self.request(cmd="close")
            await asyncio.wait_for(self.process.wait(), 10)

    refuse = ()


class ContractCase(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.native = FakeNative()
        await self.native.start()
        self.addAsyncCleanup(self.native.close)
        self.old_env = os.environ.copy()
        os.environ.update(ENV)
        self.addCleanup(self._restore_env)
        from layerfs import _client

        self.transport_patch = unittest.mock.patch.object(
            _client, "make_transport", return_value=self.native
        )
        self.transport_patch.start()
        self.addCleanup(self.transport_patch.stop)
        from layerfs import LayerFS

        self.LayerFS = LayerFS
        self.client = await LayerFS.open("site")
        self.addAsyncCleanup(self.client.close)

    def _restore_env(self):
        os.environ.clear()
        os.environ.update(self.old_env)

    async def invoke(self, record):
        if record["path"].startswith("LayerFS."):
            target = self.LayerFS
            parts = record["path"].split(".")[1:]
        else:
            target = self.client
            parts = record["path"].split(".")
        for part in parts[:-1]:
            target = getattr(target, part)
        args = [self._binary(item) for item in record["args"]]
        result = await getattr(target, parts[-1])(*args)
        if record["path"] == "LayerFS.init":
            self.addAsyncCleanup(result.close)
            return result.created
        return result

    def _binary(self, value):
        if isinstance(value, list):
            return [self._binary(item) for item in value]
        if isinstance(value, dict):
            if set(value) == {"base64"}:
                return base64.b64decode(value["base64"])
            return {key: self._binary(item) for key, item in value.items()}
        return value
