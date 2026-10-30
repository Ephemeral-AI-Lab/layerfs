import re
from ._errors import LayerFSError


class _Omit:
    pass


OMIT = _Omit()
MAX_SAFE = 9_007_199_254_740_991


def clean(value):
    if isinstance(value, dict):
        return {key: clean(item) for key, item in value.items() if item is not OMIT}
    if isinstance(value, list):
        return [clean(item) for item in value]
    return value


def invalid(message):
    return LayerFSError("InvalidInput", message)


def safe_integer(value):
    return isinstance(value, int) and not isinstance(value, bool) and abs(value) <= MAX_SAFE


def page(page_size=50, page_pos=0):
    if page_size is None or page_size is OMIT:
        page_size = 50
    if page_pos is None or page_pos is OMIT:
        page_pos = 0
    if not safe_integer(page_size) or not 1 <= page_size <= 1000 or not safe_integer(page_pos) or page_pos < 0:
        raise invalid("pageSize must be 1..1000 and pagePos a nonnegative safe integer")
    return {"pageSize": page_size, "pagePos": page_pos}


def page_of(items, page_size, page_pos):
    if page_size is None or page_size is OMIT:
        page_size = 50
    if page_pos is None or page_pos is OMIT:
        page_pos = 0
    end = page_pos + page_size
    return {
        "items": items[page_pos:end],
        "hasMore": end < len(items),
        "nextPagePos": end if end < len(items) else None,
    }


def checked_text(value, name):
    if value is OMIT or value is None:
        return OMIT
    try:
        size = len(value.encode("utf-8")) if isinstance(value, str) else None
    except UnicodeEncodeError:
        size = None
    if size is None or size > 4096 or any(
        (ord(char) < 32 and char not in "\n\t") or 127 <= ord(char) <= 159 for char in value
    ):
        raise invalid(f"{name} must be UTF-8, at most 4096 bytes, with no controls except newline/tab")
    return value


def checked_message(message):
    return checked_text(message, "message")


def write_options(options):
    if options is OMIT or options is None or isinstance(options, str):
        return {"message": checked_message(options)}
    if not isinstance(options, dict):
        raise invalid("options must be a message string or { message, mode }")
    result = {"message": checked_message(options.get("message", OMIT))}
    if "mode" in options:
        result["mode"] = options["mode"]
    return result


def diff_options(options):
    if options is OMIT or options is None:
        options = {}
    if not isinstance(options, dict):
        raise invalid("cursor must be a path string and hunks a boolean")
    cursor = options.get("cursor")
    hunks = options.get("hunks", OMIT)
    if (cursor is not None and not isinstance(cursor, str)) or (hunks is not OMIT and not isinstance(hunks, bool)):
        raise invalid("cursor must be a path string and hunks a boolean")
    result = {}
    if cursor is not None:
        result["cursor"] = cursor
    if hunks is not OMIT:
        result["hunks"] = hunks
    return result


def workspace_args(workspace):
    if not isinstance(workspace, dict) or not isinstance(workspace.get("sandbox"), str) or not workspace["sandbox"] or not isinstance(workspace.get("path"), str) or not workspace["path"]:
        raise invalid("a Workspace { sandbox, path } from workspace.mount is required")
    return {"sandbox": workspace["sandbox"], "path": workspace["path"]}


def sandbox_arg(sandbox):
    if not isinstance(sandbox, str) or not re.fullmatch(r"[1-9][0-9]{0,9}", sandbox):
        raise invalid('a Sandbox is its number as a string, such as "1"')
    return sandbox


def body(value):
    return bytes(value) if isinstance(value, (bytes, bytearray, memoryview)) else value


def commit_options(options):
    if options is OMIT or options is None:
        options = {}
    if not isinstance(options, dict):
        raise invalid("options must be { author?, labels? }")
    for key in options:
        if key not in ("author", "labels"):
            raise invalid(f'unknown workspace.commit option "{key}"')
    author = checked_text(options.get("author", OMIT), "author")
    labels = options.get("labels", OMIT)
    if labels is not OMIT and (
        not isinstance(labels, list) or len(labels) > 64 or any(not isinstance(label, str) for label in labels)
    ):
        raise invalid("labels must be an array of at most 64 strings")
    if labels is not OMIT:
        labels = [checked_text(label, "each label") for label in labels]
        import json

        if len(json.dumps(labels, separators=(",", ":"), ensure_ascii=False).encode("utf-8")) > 4096:
            raise invalid("labels JSON must be at most 4096 bytes")
    result = {}
    if author is not OMIT:
        result["author"] = author
    if labels is not OMIT:
        result["labels"] = labels
    return result


def deployment(options):
    result = {
        "docker": options.get("docker") or "/var/run/docker.sock",
        "image": options.get("image", OMIT),
        "volume": options.get("volume", OMIT),
        "daemon": options.get("daemon", OMIT),
    }
    for name in ("docker", "image", "volume", "daemon"):
        if result[name] is OMIT or not result[name]:
            raise invalid(f"deploy.{name} is not configured (LAYERFS_{name.upper()})")
    for name in ("containerPrefix", "daemonHost"):
        if options.get(name):
            result[name] = options[name]
    return result
