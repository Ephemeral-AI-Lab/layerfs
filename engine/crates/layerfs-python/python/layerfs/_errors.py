class LayerFSError(Exception):
    """A typed LayerFS refusal, with Node's prefixed message text."""

    def __init__(self, code, message, detail=None):
        self.code = code
        self.message = f"{code}: {message}"
        self.detail = detail if detail is not None else None
        self.raw_message = message
        super().__init__(self.message)
