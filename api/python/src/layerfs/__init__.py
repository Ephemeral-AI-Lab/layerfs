"""LayerFS for Python (#533 step 16): the nine families of the one call
table, synchronous; ``layerfs.aio`` has the same calls as coroutines (O19).

    import layerfs
    layerfs.store.create("shop")
    layerfs.project.init("shop/api", "/src/api", on_progress=print)
    lfs = layerfs.client(home="/data/lfs", scope={"folder": "/web", "branch": "agent"})
"""

from ._client import FAMILIES, TABLE, Client, client
from ._errors import LayerFSError

_default = Client()
project = _default.project
branch = _default.branch
commit = _default.commit
file = _default.file
sandbox = _default.sandbox
workspace = _default.workspace
machine = _default.machine
store = _default.store
backup = _default.backup

__all__ = [
    "FAMILIES",
    "TABLE",
    "Client",
    "LayerFSError",
    "backup",
    "branch",
    "client",
    "commit",
    "file",
    "machine",
    "project",
    "sandbox",
    "store",
    "workspace",
]
