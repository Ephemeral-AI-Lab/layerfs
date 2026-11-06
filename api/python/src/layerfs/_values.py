"""Values across the native module: JSON text, bytes as {"base64": ...}."""

import base64
import json

from ._errors import LayerFSError


def _invalid(message):
    return LayerFSError("InvalidInput", message, None)


def _plain(value):
    if isinstance(value, (bytes, bytearray, memoryview)):
        return {"base64": base64.b64encode(bytes(value)).decode("ascii")}
    if isinstance(value, dict):
        for key in value:
            if not isinstance(key, str):
                raise _invalid(f"object keys must be text, not {type(key).__name__}")
        return {key: _plain(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_plain(item) for item in value]
    return value


def encode(value):
    """A Python value as the library's JSON text. A value JSON cannot hold
    (NaN, Infinity, a lone surrogate, a non-text key, a type JSON has no
    form for) is refused as ``InvalidInput`` here, before any call."""
    try:
        text = json.dumps(_plain(value), ensure_ascii=False, allow_nan=False)
        text.encode("utf-8")
    except UnicodeEncodeError:
        raise _invalid("text holds a lone surrogate, which is not Unicode text") from None
    except (TypeError, ValueError, OverflowError, RecursionError) as error:
        raise _invalid(f"not a JSON value: {error}") from None
    return text


def path_text(value):
    """A home as text the native module takes (``None`` stays ``None``)."""
    if value is None:
        return None
    text = str(value)
    try:
        text.encode("utf-8")
    except UnicodeEncodeError:
        raise _invalid("text holds a lone surrogate, which is not Unicode text") from None
    return text


def _bytes(item):
    if len(item) == 1 and isinstance(item.get("base64"), str):
        try:
            return base64.b64decode(item["base64"], validate=True)
        except ValueError:
            pass  # not base64: a plain one-key map, as the library reads it
    return item


def decode(text):
    """The library's JSON text as Python values; {"base64": ...} becomes bytes."""
    return json.loads(text, object_hook=_bytes)
