from collections.abc import Mapping
from typing import Any

class LayerFSError(Exception):
    code: str
    error: str
    message: str
    fix: str | None
    detail: Any
    fields: dict[str, Any]
    def __init__(
        self,
        code: str,
        message: str,
        detail: Any = None,
        fix: str | None = None,
        fields: Mapping[str, Any] | None = None,
    ) -> None: ...
    @classmethod
    def from_refusal(cls, refused: Mapping[str, Any]) -> LayerFSError: ...
    def __getattr__(self, name: str) -> Any: ...
