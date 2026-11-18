"""A refused call."""


class LayerFSError(Exception):
    """A refused call. ``code`` (also ``error``) is one of the thirteen
    public codes: NotFound, Exists, InUse, Busy, Unresolved, Conflict,
    Dirty, Capacity, InvalidInput, Unsupported, Unavailable, Failed,
    Unknown. ``message`` says why; ``fix`` is one runnable shell line, or
    None. The fields the next call needs (a Conflict's ``commit``, a Busy's
    ``op``, a Dirty's ``paths``) are in ``fields`` and are attributes of the
    error. ``detail`` is the library's structured detail, present when the
    call asked for the long form (``detail=True``), else None."""

    def __init__(self, code, message, detail=None, fix=None, fields=None):
        super().__init__(message)
        self.code = code
        self.error = code
        self.message = message
        self.fix = fix
        self.detail = detail
        self.fields = dict(fields or {})
        for key, value in self.fields.items():
            # a field never replaces what the error already has
            if not hasattr(self, key):
                setattr(self, key, value)

    @classmethod
    def from_refusal(cls, refused):
        """The error of a refusal as the library answers it:
        ``{"error", "message", "fix"?, "detail"?, ...fields}``."""
        fields = {key: value for key, value in refused.items() if key not in ("error", "message", "fix", "detail")}
        return cls(refused["error"], refused["message"], refused.get("detail"), refused.get("fix"), fields)

    def __repr__(self):
        return f"LayerFSError({self.code!r}, {self.message!r})"
