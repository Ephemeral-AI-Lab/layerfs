"""A refused call."""


class LayerFSError(Exception):
    """A refused call: ``code`` is stable (InvalidInput, NotFound, InUse,
    Exists, Conflict, Unsupported, Unavailable, Unknown, Failed, ...),
    ``message`` says why and ``detail`` is the library's structured detail."""

    def __init__(self, code, message, detail=None):
        super().__init__(message)
        self.code = code
        self.message = message
        self.detail = detail

    def __repr__(self):
        return f"LayerFSError({self.code!r}, {self.message!r})"
