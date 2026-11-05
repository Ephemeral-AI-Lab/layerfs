from typing import Any

class LayerFSError(Exception):
    code: str
    message: str
    detail: Any
    def __init__(self, code: str, message: str, detail: Any = None) -> None: ...
