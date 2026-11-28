"""LayerFS 2.0.0 from Python (asyncio): the same mountless flow as
python_sync_flow.py through `layerfs.aio`, whose calls are coroutines. Store,
project, files, Branches, diff, merge, publish, history and refusals. No
Sandbox, no Workspace, no machine.

How it was run for this manual (see README.md in this folder for the output):
on macOS with the worktree's debug module copied to
api/python/src/layerfs/_native.abi3.so,
    PYTHONPATH=api/python/src TMPDIR=<scratch dir> python3 python_async_flow.py
It makes its own Disposable home and project folder in a temporary directory
and removes them at the end. On Linux the temporary directory must be on a
filesystem with SQLite locks (ext4, xfs, btrfs).

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
        say(label, json.dumps({"error": e.error, "message": e.message, "fix": e.fix}))
    else:
        say(label, "NOT REFUSED (unexpected)")


async def main(root: Path):
    # the project is a copy of a host folder
    folder = root / "site"
    (folder / "docs").mkdir(parents=True, mode=0o700)
    (folder / "README.md").write_text("# site\n")
    (folder / "docs" / "a.txt").write_text("one\ntwo\n")

    P = "shop/site"
    # the LayerFS root; its home is root/home
    lfs = aio.client(root=str(root))

    await lfs.store.create("shop")
    say("store.get", json.dumps((await lfs.store.get("shop"))))
    phases = []
    await lfs.project.init(P, str(folder), on_progress=lambda p: phases.append(p["phase"]))
    say("project.init phases", " ".join(phases))

    # mountless file work on a Branch (main changes only through publish)
    await lfs.branch.fork(P, "agent", "main")
    await lfs.file.write(P, "agent", "/notes.md", "# notes\n", message="add notes")
    say("file.read", json.dumps((await lfs.file.read(P, "agent", "/notes.md"))["text"]))
    listing = await lfs.file.list(P, "agent", "/")
    say("file.list /", " ".join(f"{e['path']}({e['type']})" for e in listing["items"]))
    await lfs.file.edit(P, "agent", "/docs/a.txt", old_string="two", new_string="three", message="edit a")
    say("file.read after edit", json.dumps((await lfs.file.read(P, "agent", "/docs/a.txt"))["text"]))

    # two Branches change different files, then merge into a new Branch
    await lfs.branch.fork(P, "left", "main")
    await lfs.branch.fork(P, "right", "main")
    await lfs.file.edit(P, "left", "/docs/a.txt", old_string="one", new_string="uno", message="left edits a")
    await lfs.file.write(P, "right", "/right.txt", "from right\n", message="right adds a file")
    merged = await lfs.branch.merge(P, "left", "right", into="merged", message="merge left and right")
    head = (await lfs.branch.get(P, "merged"))["commit"]
    say("branch.merge", "merged is at the merge Commit" if head == merged["commit"] else "?")
    diff = await lfs.commit.diff(P, "merged", from_="main")
    say("commit.diff main..merged", ", ".join(f"{c['change']} {c['path']}" for c in diff["items"]))
    say("file.read merged", json.dumps((await lfs.file.read(P, "merged", "/docs/a.txt"))["text"]))

    # history, and main's next head
    history = await lfs.commit.list(P, "merged")
    say("commit.list merged", " | ".join(c["message"] for c in history["items"]))
    say("branch.list", " ".join(b["branch"] for b in (await lfs.branch.list(P))["items"]))
    await lfs.branch.publish(P, "merged")
    page = await lfs.commit.list(P, "main", limit=1)
    say("commit.list main limit=1", json.dumps({"source_branch": page["items"][0]["source_branch"], "more": "next_cursor" in page}))

    # refusals: LayerFSError with error, message and fix
    await refused("missing Branch", lambda: lfs.file.read(P, "nope", "/README.md"))
    await refused("write to main", lambda: lfs.file.write(P, "main", "/x.txt", "x\n"))
    await refused("unknown option", lambda: lfs.file.read(P, "agent", "/README.md", colour=True))
    scoped = aio.client(root=str(root), scope={"project": P, "folder": "/docs", "branch": "ns"})
    await refused("scope: other call", lambda: scoped.store.list())
    await refused("scope: outside folder", lambda: scoped.file.read(P, "main", "/README.md"))


root = Path(tempfile.mkdtemp(prefix="layerfs-guide-", dir=os.environ.get("TMPDIR")))
root.chmod(0o700)
try:
    asyncio.run(main(root))
finally:
    layerfs.close()
    shutil.rmtree(root, ignore_errors=True)
