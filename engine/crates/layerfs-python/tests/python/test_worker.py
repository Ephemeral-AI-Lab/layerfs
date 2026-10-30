import asyncio
import json
import os
from pathlib import Path
import tempfile
import unittest

from layerfs._worker import WorkerTransport, decode, encode, frame


class WorkerProtocolTests(unittest.TestCase):
    def test_tagged_json_values(self):
        value = {
            "none": None,
            "bool": True,
            "int": 7,
            "text": "hello",
            "bytes": b"\x00\xff",
            "list": [1, "two"],
            "object": {"nested": False},
        }
        self.assertEqual(decode(encode(value)), value)
        framed = frame({"secret": "ab" * 32})
        self.assertEqual(int.from_bytes(framed[:4], "big"), len(framed) - 4)
        self.assertEqual(decode(framed[4:]), {"secret": "ab" * 32})
        with self.assertRaises(ValueError):
            decode(b'["wrong",1,null]')


class WorkerClientTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.requests = []
        self.server = await asyncio.start_server(self.serve, "127.0.0.1", 0)
        self.directory = tempfile.TemporaryDirectory(prefix="layerfs-p4-worker-")
        self.endpoint = str(Path(self.directory.name) / "endpoint.json")
        with open(self.endpoint, "w", encoding="utf-8") as output:
            json.dump(
                {
                    "home": "/private/worker-home",
                    "port": self.server.sockets[0].getsockname()[1],
                    "secret": "ab" * 32,
                },
                output,
            )

    async def asyncTearDown(self):
        self.server.close()
        await self.server.wait_closed()
        self.directory.cleanup()

    async def read_frame(self, reader):
        head = await reader.readexactly(4)
        return decode(await reader.readexactly(int.from_bytes(head, "big")))

    async def serve(self, reader, writer):
        try:
            greeting = await self.read_frame(reader)
            if greeting != {"secret": "ab" * 32}:
                writer.close()
                return
            writer.write(frame({"id": 0, "answer": {"value": "hello"}}))
            await writer.drain()
            while request := await self.read_frame(reader):
                self.requests.append(request)
                if request["op"] == "disconnect":
                    writer.close()
                    await writer.wait_closed()
                    return
                writer.write(
                    frame(
                        {
                            "id": request["id"],
                            "answer": {"value": {"op": request["op"], "open": request["open"]}},
                        }
                    )
                )
                await writer.drain()
        except (asyncio.IncompleteReadError, ConnectionError):
            pass
        finally:
            writer.close()

    async def test_authenticated_calls_store_counts_and_binary(self):
        transport = WorkerTransport(self.endpoint)
        args = {"home": "/private/worker-home", "storePath": "/store", "bytes": b"\x00\xff"}
        await transport.store_open(args)
        first = await transport.call("fs.write", args)
        second = await transport.call("fs.read", args)
        self.assertEqual(first["value"]["open"], True)
        self.assertEqual(second["value"]["open"], True)
        self.assertEqual(self.requests[0]["fn"], "call")
        self.assertEqual(self.requests[0]["args"]["bytes"], b"\x00\xff")
        await transport.store_close(args)
        closed = await transport.call("fs.read", args)
        self.assertEqual(closed["value"]["open"], False)
        await transport.close()

    async def test_sent_call_loss_is_unknown_and_not_replayed(self):
        transport = WorkerTransport(self.endpoint)
        args = {"home": "/private/worker-home", "storePath": "/store"}
        answer = await transport.call("disconnect", args)
        self.assertEqual(answer["refused"]["code"], "Unknown")
        await asyncio.sleep(0)
        self.assertEqual(len(self.requests), 1)
        await transport.close()


if __name__ == "__main__":
    unittest.main()
