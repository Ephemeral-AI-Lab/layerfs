import asyncio
import base64
import json
import math
import re


TAG = "layerfs-worker"


def _pack(value):
    if value is None or isinstance(value, (str, bool, int)):
        return value
    if isinstance(value, float):
        if not math.isfinite(value) or (value == 0 and math.copysign(1, value) < 0):
            return ["number", "-0" if value == 0 else str(value)]
        return value
    if isinstance(value, (bytes, bytearray, memoryview)):
        return ["buffer", base64.b64encode(bytes(value)).decode("ascii")]
    if isinstance(value, (list, tuple)):
        return ["array", [_pack(item) for item in value]]
    if isinstance(value, dict):
        if not all(isinstance(key, str) for key in value):
            raise ValueError("LayerFS worker protocol: object keys must be strings")
        return ["object", [[key, _pack(item)] for key, item in value.items()]]
    raise ValueError(f"LayerFS worker protocol: unsupported value type {type(value).__name__}")


def _unpack(value):
    if not isinstance(value, list):
        if isinstance(value, dict):
            raise ValueError("LayerFS worker protocol: untagged object")
        return value
    if not value:
        raise ValueError("LayerFS worker protocol: invalid value tag")
    tag, *rest = value
    data = rest[0] if rest else None
    if tag == "undefined" and len(value) == 1:
        return None
    if tag == "bigint" and isinstance(data, str) and re.fullmatch(r"-?\d+", data):
        return int(data)
    if tag == "number" and data in ("NaN", "Infinity", "-Infinity", "-0"):
        return float(data) if data != "-0" else -0.0
    if tag in ("buffer", "bytes", "arraybuffer") and isinstance(data, str):
        try:
            return base64.b64decode(data, validate=True)
        except ValueError as error:
            raise ValueError("LayerFS worker protocol: invalid binary value") from error
    if tag == "array" and isinstance(data, list):
        return [_unpack(item) for item in data]
    if tag == "object" and isinstance(data, list) and all(
        isinstance(pair, list) and len(pair) == 2 and isinstance(pair[0], str) for pair in data
    ):
        return {key: _unpack(item) for key, item in data}
    raise ValueError("LayerFS worker protocol: invalid value tag")


def encode(value):
    return json.dumps([TAG, 1, _pack(value)], separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def decode(payload):
    try:
        value = json.loads(payload)
    except (ValueError, UnicodeDecodeError) as error:
        raise ValueError("LayerFS worker protocol: invalid JSON or legacy V8 frame") from error
    if not isinstance(value, list) or len(value) != 3 or value[0] != TAG or value[1] != 1:
        raise ValueError("LayerFS worker protocol: unsupported wire version")
    return _unpack(value[2])


def frame(value):
    payload = encode(value)
    return len(payload).to_bytes(4, "big") + payload


def endpoint_of(file):
    try:
        with open(file, encoding="utf-8") as source:
            info = json.load(source)
        if (
            not isinstance(info.get("port"), int)
            or isinstance(info.get("port"), bool)
            or info["port"] < 1
            or not re.fullmatch(r"[0-9a-f]{64}", info.get("secret", ""))
            or not isinstance(info.get("home"), str)
        ):
            raise ValueError
        return info
    except Exception as error:
        raise ValueError(f"invalid LayerFS worker endpoint {file}") from error


def _refusal(code, message):
    return {"refused": {"code": code, "message": message, "detail": None}}


class _Link:
    def __init__(self, info):
        self.info = info
        self.reader = None
        self.writer = None
        self.pending = {}
        self.next_id = 1
        self.task = None
        self.closed = False
        self.connect_lock = asyncio.Lock()

    async def connect(self):
        async with self.connect_lock:
            if self.closed:
                raise ConnectionError("the LayerFS worker connection is closed; the call was not sent")
            if self.writer:
                return
            self.reader, self.writer = await asyncio.wait_for(
                asyncio.open_connection("127.0.0.1", self.info["port"]), 5
            )
            self.writer.write(frame({"secret": self.info["secret"]}))
            await self.writer.drain()
            greeting = await asyncio.wait_for(self._read(), 5)
            if greeting != {"id": 0, "answer": {"value": "hello"}}:
                raise ConnectionError("invalid greeting")
            self.task = asyncio.create_task(self._receive())

    async def _read(self):
        head = await self.reader.readexactly(4)
        return decode(await self.reader.readexactly(int.from_bytes(head, "big")))

    async def _receive(self):
        why = "socket closed"
        try:
            while True:
                message = await self._read()
                pending = self.pending.pop(message.get("id"), None)
                if pending:
                    answer = message.get("answer")
                    if answer and answer.get("thrown"):
                        answer = _refusal("Unknown", f"the LayerFS worker failed while making the call: {answer['thrown']}; check the project before going on")
                    pending.set_result(answer)
        except Exception as error:
            why = str(error) or type(error).__name__
        finally:
            self.lost(why)

    def lost(self, why):
        if self.closed:
            return
        self.closed = True
        for pending in self.pending.values():
            if not pending.done():
                pending.set_result(_refusal("Unknown", f"the connection to the LayerFS worker was lost ({why}) after the call was sent; it may have completed: check the project before going on"))
        self.pending.clear()
        if self.writer:
            self.writer.close()

    async def call(self, request):
        try:
            await self.connect()
        except Exception as error:
            return _refusal("Failed", f"the LayerFS worker did not answer ({error}); the call was not sent")
        if self.closed:
            return _refusal("Failed", "the LayerFS worker connection is closed; the call was not sent")
        request_id = self.next_id
        self.next_id += 1
        pending = asyncio.get_running_loop().create_future()
        self.pending[request_id] = pending
        try:
            self.writer.write(frame({"id": request_id, **request}))
            await self.writer.drain()
        except Exception as error:
            self.pending.pop(request_id, None)
            return _refusal("Unknown", f"the connection to the LayerFS worker was lost ({error}) after the call was sent; it may have completed: check the project before going on")
        return await pending

    async def close(self):
        self.closed = True
        if self.writer:
            self.writer.close()
            try:
                await self.writer.wait_closed()
            except Exception:
                pass
        if self.task:
            self.task.cancel()
            try:
                await self.task
            except (asyncio.CancelledError, Exception):
                pass


class WorkerTransport:
    def __init__(self, endpoint):
        self.endpoint = endpoint
        self.opened = {}
        self.links = {}

    def _link(self, info):
        key = (info["port"], info["secret"])
        if key not in self.links or self.links[key].closed:
            self.links[key] = _Link(info)
        return self.links[key]

    async def _send(self, fn, op, args):
        try:
            info = endpoint_of(self.endpoint)
        except ValueError as error:
            return _refusal("Failed", f"the LayerFS worker is not running for this home: {error}")
        sent = args
        deploy = dict(args.get("deploy") or {})
        if info.get("daemonHost"):
            deploy["daemonHost"] = deploy.get("daemonHost") or info["daemonHost"]
        if info.get("docker"):
            deploy["docker"] = info["docker"]
        if deploy:
            sent = {**args, "deploy": deploy}
        return await self._link(info).call({
            "fn": fn, "op": op, "args": sent, "open": args.get("storePath") in self.opened,
        })

    async def call(self, op, args):
        return await self._send("call", op, args)

    async def backend_call(self, op, args):
        return await self._send("backendCall", op, args)

    async def store_open(self, args):
        path = args.get("storePath")
        self.opened[path] = self.opened.get(path, 0) + 1
        return {"value": None}

    async def store_close(self, args):
        path = args.get("storePath")
        left = self.opened.get(path, 1) - 1
        if left > 0:
            self.opened[path] = left
        else:
            self.opened.pop(path, None)
        return {"value": None}

    async def exec_output(self, _args):
        return _refusal("Failed", "a worker exec's output arrives whole; there is no spool to read")

    async def close(self):
        for link in self.links.values():
            await link.close()
        self.links.clear()
