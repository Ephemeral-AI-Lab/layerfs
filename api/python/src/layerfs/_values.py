"""Values across the native module: JSON text, bytes as {"base64": ...}."""

import base64
import json


def _plain(value):
    if isinstance(value, (bytes, bytearray, memoryview)):
        return {"base64": base64.b64encode(bytes(value)).decode("ascii")}
    if isinstance(value, dict):
        return {key: _plain(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_plain(item) for item in value]
    return value


def encode(value):
    """A Python value as the library's JSON text."""
    return json.dumps(_plain(value), ensure_ascii=False)


def _bytes(item):
    if len(item) == 1 and isinstance(item.get("base64"), str):
        return base64.b64decode(item["base64"])
    return item


def decode(text):
    """The library's JSON text as Python values; {"base64": ...} becomes bytes."""
    return json.loads(text, object_hook=_bytes)
