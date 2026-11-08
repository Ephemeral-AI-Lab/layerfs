"""LayerFS for Python (#533 step 16): the nine families of the one call
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
from ._values import decode, encode, path_text


def doctor(home=None, *, target=None, name=None, timeout=None, quick=None, write_test=None):
    """``layerfs doctor``: what is wrong in ``home`` (default ``LAYERFS_HOME``,
    else ``~/.layerfs``), without changing LayerFS state or starting a machine. Returns
    ``{"ok": ..., "findings": [{"severity", "entity": {"kind", "name"},
    "message", "fix"}]}``; ``ok`` is false when a finding is an error.
    Explicit target/name options select one diagnostic scope and add check
    outcomes and completeness. timeout uses positive ms/s/m/h duration strings.
    quick applies only to store.integrity; write_test only to backup.connection.
    Explicit backup write_test may create and delete its own temporary S3 version.
    Outside the call table."""
    options = {key: value for key, value in {"target": target, "name": name, "timeout": timeout, "quick": quick, "write_test": write_test}.items() if value is not None}
    answer = decode(_native.doctor(path_text(home), encode(options)))
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
    "store",
    "workspace",
]
