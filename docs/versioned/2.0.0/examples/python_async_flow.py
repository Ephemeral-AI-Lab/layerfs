"""LayerFS 2.0.0 from Python (asyncio): the same mountless flow as
python_sync_flow.py through `layerfs.aio`, whose calls are coroutines. Store,
project, files, Branches, diff, merge, history and three refusals. No Sandbox,
no Workspace, no machine.

How it was run (see README.md in this folder for the output): the wheel was
built with `maturin build --release --locked` and installed with `pip install
<wheel>` into a new virtualenv outside the checkout; then, in that
environment,
    TMPDIR=<ext4 dir> python python_async_flow.py
It makes its own Disposable home and project folder in a temporary directory
and removes them at the end. The temporary directory must be on a filesystem
with SQLite locks (ext4, xfs, btrfs).

Cancelling one of these coroutines stops its callbacks, not the call: the
native call runs to its end (read the state again after a cancel).
"""

import asyncio
import json
import os
import shutil
import tempfile
from pathlib import Path

import layerfs
from layerfs import aio


def say(label, value):
    print(f"{label:<28}{value}")


async def refused(label, call):
    try:
        await call()
    except layerfs.LayerFSError as e:
        say(label, json.dumps({"code": e.code, "message": e.message, "detail": e.detail}))
    else:
        say(label, "NOT REFUSED (unexpected)")


async def main(root: Path):
    # the project is a copy of a host folder
    folder = root / "site"
    (folder / "docs").mkdir(parents=True, mode=0o700)
    (folder / "README.md").write_text("# site\n")
    (folder / "docs" / "a.txt").write_text("one\ntwo\n")

    P = "shop/site"
    home = str(root / "home")
    lfs = aio.client(home=home)

    store = await lfs.store.create("shop")
    say("store.create", f"{store['name']} {store['kind']} {store['profile']}")
    phases = []
    await lfs.project.init(P, str(folder), on_progress=lambda p: phases.append(p["phase"]))
    say("project.init phases", " ".join(phases))

    # mountless file work on a Branch (main changes only through publish)
    await lfs.branch.fork(P, "agent", "main")
    await lfs.file.write(P, "agent", "/notes.md", "# notes\n", message="add notes")
    say("file.read", json.dumps((await lfs.file.read(P, "agent", "/notes.md"))["text"]))
    listing = await lfs.file.list(P, "agent", "/")
    say("file.list /", " ".join(f"{e['name']}({e['type']})" for e in listing["items"]))
    await lfs.file.edit(P, "agent", "/docs/a.txt", old="two", new="three", message="edit a")
    say("file.read after edit", json.dumps((await lfs.file.read(P, "agent", "/docs/a.txt"))["text"]))

    # two Branches change different files, then merge
    await lfs.branch.fork(P, "left", "main")
    await lfs.branch.fork(P, "right", "main")
    await lfs.file.edit(P, "left", "/docs/a.txt", old="one", new="uno", message="left edits a")
    await lfs.file.write(P, "right", "/right.txt", "from right\n", message="right adds a file")
    merged = await lfs.branch.merge(P, "merged", "left", "right", message="merge left and right")
    say("branch.merge", merged["kind"])
    diff = await lfs.branch.diff(P, "main", "merged", paths_only=True)
    say("branch.diff main..merged", ", ".join(f"{c['kind']} {c['path']}" for c in diff["items"]))
    say("file.read merged", json.dumps((await lfs.file.read(P, "merged", "/docs/a.txt"))["text"]))

    # history
    history = await lfs.commit.list(P, "merged")
    say("commit.list merged", " | ".join(c["message"] for c in history["items"]))
    say("branch.list", " ".join(b["name"] for b in (await lfs.branch.list(P))["items"]))

    # refusals: the error shape is code, message, detail
    await refused("missing Branch", lambda: lfs.file.read(P, "nope", "/README.md"))
    await refused("write to main", lambda: lfs.file.write(P, "main", "/x.txt", "x\n"))
    await refused("unknown option", lambda: lfs.file.read(P, "agent", "/README.md", colour=True))
    scoped = aio.client(home=home, scope={"project": P, "folder": "/docs", "branch": "ns"})
    await refused("scope: other call", lambda: scoped.store.list())
    await refused("scope: outside folder", lambda: scoped.file.read(P, "main", "/README.md"))


root = Path(tempfile.mkdtemp(prefix="layerfs-guide-", dir=os.environ.get("TMPDIR")))
root.chmod(0o700)
try:
    asyncio.run(main(root))
finally:
    layerfs.close()
    shutil.rmtree(root, ignore_errors=True)
