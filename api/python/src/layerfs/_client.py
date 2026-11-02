"""One client per home (and optional folder scope, O32): nine families
built from the library's call table. Each function takes its positionals
in the table's order; optional positionals and options may be keywords.
Every call is one call into the library, made once."""

import sys

from . import _native
from ._errors import LayerFSError
from ._values import decode, encode

#: The table: ``[{family, call, args, options, help}]``.
TABLE = decode(_native.table())
#: The nine families, in help order.
FAMILIES = list(dict.fromkeys(spec["family"] for spec in TABLE))

_FUNCTION = "function"


def _invalid(message):
    return LayerFSError("InvalidInput", message, None)


def _write(stream, data):
    """One piece of output, text or bytes, to ``stream``."""
    target = getattr(stream, "buffer", None)
    if isinstance(data, str):
        stream.write(data)
        stream.flush()
    elif target is not None:
        stream.flush()
        target.write(data)
        target.flush()
    else:
        stream.write(data.decode("utf-8", "replace"))
        stream.flush()


def _streamed(handlers):
    """``stream=True``: the command's output passed on as it comes, to the
    ``stdout`` and ``stderr`` functions, else to this process's own."""
    handlers.setdefault("stdout", lambda data: _write(sys.stdout, data))
    handlers.setdefault("stderr", lambda data: _write(sys.stderr, data))


def _event_handler(handlers):
    if not handlers:
        return None

    def on_event(text):
        event = decode(text)
        if "progress" in event:
            if "on_progress" in handlers:
                handlers["on_progress"](event["progress"])
        elif event.get("stream") in ("stdout", "stderr") and event["stream"] in handlers:
            handlers[event["stream"]](event["bytes"])

    return on_event


def _bind(client, spec):
    name = f"{spec['family']}.{spec['call']}"
    positional = [arg["name"] for arg in spec["args"]]
    options = {opt["name"]: opt["kind"] for opt in spec["options"]}

    def call(*given, **keywords):
        if len(given) > len(positional):
            raise _invalid(f"{name}: takes {len(positional)} positional arguments")
        args = {}
        for arg, value in zip(positional, given):
            if value is not None:
                args[arg] = value
        handlers = {}
        for key, value in keywords.items():
            if key in positional:
                if key in args:
                    raise _invalid(f"{name}: `{key}` given twice")
                if value is not None:
                    args[key] = value
            elif key in options:
                if options[key] == _FUNCTION:
                    if not callable(value):
                        raise _invalid(f"{name}: {key} must be a function")
                    handlers[key] = value
                elif value is not None:
                    args[key] = value
            else:
                raise _invalid(f'{name}: unknown option "{key}"')
        if args.get("stream") is True:
            _streamed(handlers)
        scope = encode(client.scope) if client.scope is not None else None
        answer = decode(_native.call(name, encode(args), client.home, scope, _event_handler(handlers)))
        refused = answer.get("refused")
        if refused is not None:
            raise LayerFSError(refused["code"], refused["message"], refused.get("detail"))
        return answer["value"]

    call.__name__ = spec["call"]
    call.__qualname__ = name
    call.__doc__ = spec["help"]
    return call


class Family:
    """One family's calls, as attributes."""

    def __init__(self, name):
        self._name = name

    def __repr__(self):
        return f"<layerfs family {self._name}>"


class Client:
    """A client of ``home`` (default ``LAYERFS_HOME``, else ``~/.layerfs``),
    with an optional folder scope ``{"project": ..., "folder": ..., "branch": ...}``."""

    def __init__(self, home=None, scope=None):
        self.home = str(home) if home is not None else None
        self.scope = scope
        for family in FAMILIES:
            setattr(self, family, Family(family))
        for spec in TABLE:
            setattr(getattr(self, spec["family"]), spec["call"], _bind(self, spec))


def client(home=None, scope=None):
    """A client of ``home``, with an optional folder scope."""
    return Client(home, scope)
