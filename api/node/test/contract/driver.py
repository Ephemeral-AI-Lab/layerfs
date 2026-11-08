"""Runs a list of calls through the Python package for api/node/test/contract.mjs.

stdin: JSON `{ "home": ..., "aio": bool, "steps": [{ "call": "file.read", "args": [...], "opts": {...} }] }`
with bytes as `{"$bytes": base64}` and a float as `{"$float": text}` (a one-key `{"base64": ...}` map stays a map).
stdout: JSON `[{ "ok": value } | { "refused": { code, message, detail } }]`, one per step,
bytes as `{"$bytes": base64}`. `call` "doctor" is `layerfs.doctor(home)`.
Nothing here converts values: the package's own encoder and decoder do.
"""

import asyncio
import base64
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "python" / "src"))

import layerfs  # noqa: E402
from layerfs import aio  # noqa: E402


def revive(item):
    if set(item) == {"$bytes"}:
        return base64.b64decode(item["$bytes"])
    if set(item) == {"$float"}:
        return float(item["$float"])
    return item


def plain(value):
    if isinstance(value, (bytes, bytearray)):
        return {"$bytes": base64.b64encode(bytes(value)).decode("ascii")}
    if isinstance(value, dict):
        return {key: plain(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [plain(item) for item in value]
    return value


def refusal(error):
    return {"refused": {"code": error.code, "message": error.message, "detail": error.detail}}


def run_sync(request):
    client = layerfs.client(home=request["home"], scope=request.get("scope"))
    out = []
    for step in request["steps"]:
        try:
            if step["call"] == "doctor":
                out.append({"ok": layerfs.doctor(request["home"])})
                continue
            family, call = step["call"].split(".")
            out.append({"ok": getattr(getattr(client, family), call)(*step["args"], **step["opts"])})
        except layerfs.LayerFSError as error:
            out.append(refusal(error))
    return out


async def run_aio(request):
    client = aio.client(home=request["home"], scope=request.get("scope"))
    out = []
    for step in request["steps"]:
        try:
            if step["call"] == "doctor":
                out.append({"ok": layerfs.doctor(request["home"])})
                continue
            family, call = step["call"].split(".")
            out.append({"ok": await getattr(getattr(client, family), call)(*step["args"], **step["opts"])})
        except layerfs.LayerFSError as error:
            out.append(refusal(error))
    return out


request = json.loads(sys.stdin.buffer.read().decode("utf-8"), object_hook=revive)
try:
    results = asyncio.run(run_aio(request)) if request["aio"] else run_sync(request)
finally:
    layerfs.close()
sys.stdout.buffer.write(json.dumps(plain(results), ensure_ascii=False).encode("utf-8"))
