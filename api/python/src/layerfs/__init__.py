"""LayerFS for Python (#533 step 16): the twelve families of the one call
table, synchronous; ``layerfs.aio`` has the same calls as coroutines (O19).

    import layerfs
    layerfs.store.create("shop")
    layerfs.project.init("shop/api", "/src/api", on_progress=print)
    lfs = layerfs.client(home="/data/lfs", scope={"project": "shop/api", "folder": "/web", "branch": "agent"})
"""

import importlib

try:
    _native = importlib.import_module("._native", __name__)
except ImportError as err:
    import platform as _platform

    raise ImportError(
        f"The compiled module 'layerfs._native' of the 'layerfs' package is missing "
        f"or could not be loaded for platform '{_platform.platform()}'. "
        f"A wheel for this platform must be installed, or run 'maturin develop' in a checkout."
    ) from err
from ._client import FAMILIES, TABLE, Client, client
from ._errors import LayerFSError


def close():
    """Closes every Store this process holds open with no call running,
    releasing its leases, so it leaves none behind when it exits (a lease
    of a process that ended is stale until the Store's next call). Call it
    before the process exits; a later call opens its Store again. It also
    ends this process's idle Sandbox daemon sessions inside a short bound, so
    the exit is not a dropped connection in a daemon's log. Returns how many
    Stores it closed. Outside the call table."""
    return _native.close()


_default = Client()
home = _default.home
project = _default.project
branch = _default.branch
commit = _default.commit
file = _default.file
sandbox = _default.sandbox
workspace = _default.workspace
machine = _default.machine
store = _default.store
backup = _default.backup
logs = _default.logs

__all__ = [
    "FAMILIES",
    "TABLE",
    "Client",
    "LayerFSError",
    "backup",
    "branch",
    "client",
    "close",
    "commit",
    "file",
    "home",
    "logs",
    "machine",
    "project",
    "sandbox",
    "store",
    "workspace",
]
