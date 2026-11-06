from collections.abc import Mapping, Sequence
from os import PathLike
from typing import Any, Literal
from typing_extensions import NotRequired, Required, TypeAlias, TypedDict

#: What a call takes as JSON: whole numbers (64-bit signed, never a float),
#: text, ``None``, lists and text-keyed maps. ``bytes``, ``bytearray`` and
#: ``memoryview`` are sent as ``{"base64": ...}`` and read back as ``bytes``.
JSON: TypeAlias = None | bool | int | str | bytes | bytearray | memoryview | Sequence[JSON] | Mapping[str, JSON]
#: File content: text, or bytes.
Content: TypeAlias = str | bytes | bytearray | memoryview
#: A size: bytes as a number, or text such as ``"512M"`` or ``"8G"``.
Size: TypeAlias = int | str
#: What a function option (``on_progress``) receives: the progress event.
Progress: TypeAlias = Mapping[str, Any]
#: What an exec output function (``stdout``, ``stderr``) receives: text when
#: the output is UTF-8, else bytes.
Output: TypeAlias = str | bytes
#: A call's result. The call table has no result schema, so this is loose:
#: the JSON value the call answered (a dict with snake_case keys for most
#: calls), with ``{"base64": ...}`` read back as ``bytes``.
Result: TypeAlias = Any

class Scope(TypedDict):
    project: Required[str]
    folder: Required[str]
    branch: NotRequired[str | None]

class Entity(TypedDict):
    kind: str
    name: str

class Finding(TypedDict):
    severity: Literal["error", "warning"]
    entity: Entity
    message: str
    fix: str

class DoctorReport(TypedDict):
    ok: bool
    findings: list[Finding]

def encode(value: JSON) -> str: ...
def decode(text: str) -> Any: ...
def path_text(value: str | PathLike[str] | None) -> str | None: ...
