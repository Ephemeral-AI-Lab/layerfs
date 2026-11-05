"""LayerFS for Python (#533 step 16): the ten families of the one call
table, synchronous; ``layerfs.aio`` has the same calls as coroutines (O19).

    import layerfs
    layerfs.store.create("shop")
    layerfs.project.init("shop/api", "/src/api", on_progress=print)
    lfs = layerfs.client(home="/data/lfs", scope={"folder": "/web", "branch": "agent"})
"""

from . import _native
from ._client import FAMILIES, TABLE, Client, client
from ._errors import LayerFSError
from ._values import decode


def doctor(home=None):
    """``layerfs doctor``: what is wrong in ``home`` (default ``LAYERFS_HOME``,
    else ``~/.layerfs``), changing nothing and starting no machine. Returns
    ``{"ok": ..., "findings": [{"severity", "entity": {"kind", "name"},
    "message", "fix"}]}``; ``ok`` is false when a finding is an error.
    Outside the call table."""
    answer = decode(_native.doctor(None if home is None else str(home)))
    refused = answer.get("refused")
    if refused is not None:
        raise LayerFSError(refused["code"], refused["message"], refused.get("detail"))
    return answer["value"]


def close():
    """Closes every Store this process holds open with no call running,
    releasing its leases, so it leaves none behind when it exits (a lease
    of a process that ended is stale until the Store's next call). Call it
    before the process exits; a later call opens its Store again. Returns
    how many Stores it closed. Outside the call table."""
    return _native.close()


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
step = _default.step

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
    "doctor",
    "file",
    "machine",
    "project",
    "sandbox",
    "step",
    "store",
    "workspace",
]
