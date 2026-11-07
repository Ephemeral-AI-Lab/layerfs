"""LayerFS 2.0.0 from Python (synchronous): the mountless flow. Store, project,
files, Branches, diff, merge, history and three refusals. No Sandbox, no
Workspace, no machine.

How it was run (see README.md in this folder for the output): the wheel was
built with `maturin build --release --locked` and installed with `pip install
<wheel>` into a new virtualenv outside the checkout; then, in that
environment,
    TMPDIR=<ext4 dir> python python_sync_flow.py
It makes its own Disposable home and project folder in a temporary directory
and removes them at the end. The temporary directory must be on a filesystem
with SQLite locks (ext4, xfs, btrfs).
"""

import json
import os
import shutil
import tempfile
from pathlib import Path

import layerfs


def say(label, value):
    print(f"{label:<28}{value}")


def refused(label, call):
    try:
        call()
    except layerfs.LayerFSError as e:
        say(label, json.dumps({"code": e.code, "message": e.message, "detail": e.detail}))
    else:
        say(label, "NOT REFUSED (unexpected)")


root = Path(tempfile.mkdtemp(prefix="layerfs-guide-", dir=os.environ.get("TMPDIR")))
root.chmod(0o700)
try:
    # the project is a copy of a host folder
    folder = root / "site"
    (folder / "docs").mkdir(parents=True, mode=0o700)
    (folder / "README.md").write_text("# site\n")
    (folder / "docs" / "a.txt").write_text("one\ntwo\n")

    P = "shop/site"
    home = str(root / "home")
    lfs = layerfs.client(home=home)

    store = lfs.store.create("shop")
    say("store.create", f"{store['name']} {store['kind']} {store['profile']}")
    phases = []
    lfs.project.init(P, str(folder), on_progress=lambda p: phases.append(p["phase"]))
    say("project.init phases", " ".join(phases))

    # mountless file work on a Branch (main changes only through publish)
    lfs.branch.fork(P, "agent", "main")
    lfs.file.write(P, "agent", "/notes.md", "# notes\n", message="add notes")
    say("file.read", json.dumps(lfs.file.read(P, "agent", "/notes.md")["text"]))
    listing = lfs.file.list(P, "agent", "/")
    say("file.list /", " ".join(f"{e['name']}({e['type']})" for e in listing["items"]))
    lfs.file.edit(P, "agent", "/docs/a.txt", old="two", new="three", message="edit a")
    say("file.read after edit", json.dumps(lfs.file.read(P, "agent", "/docs/a.txt")["text"]))

    # two Branches change different files, then merge
    lfs.branch.fork(P, "left", "main")
    lfs.branch.fork(P, "right", "main")
    lfs.file.edit(P, "left", "/docs/a.txt", old="one", new="uno", message="left edits a")
    lfs.file.write(P, "right", "/right.txt", "from right\n", message="right adds a file")
    merged = lfs.branch.merge(P, "merged", "left", "right", message="merge left and right")
    say("branch.merge", merged["kind"])
    diff = lfs.branch.diff(P, "main", "merged", paths_only=True)
    say("branch.diff main..merged", ", ".join(f"{c['kind']} {c['path']}" for c in diff["items"]))
    say("file.read merged", json.dumps(lfs.file.read(P, "merged", "/docs/a.txt")["text"]))

    # history
    history = lfs.commit.list(P, "merged")
    say("commit.list merged", " | ".join(c["message"] for c in history["items"]))
    say("branch.list", " ".join(b["name"] for b in lfs.branch.list(P)["items"]))

    # refusals: the error shape is code, message, detail
    refused("missing Branch", lambda: lfs.file.read(P, "nope", "/README.md"))
    refused("write to main", lambda: lfs.file.write(P, "main", "/x.txt", "x\n"))
    refused("unknown option", lambda: lfs.file.read(P, "agent", "/README.md", colour=True))
    scoped = layerfs.client(home=home, scope={"project": P, "folder": "/docs", "branch": "ns"})
    refused("scope: other call", lambda: scoped.store.list())
    refused("scope: outside folder", lambda: scoped.file.read(P, "main", "/README.md"))
finally:
    layerfs.close()
    shutil.rmtree(root, ignore_errors=True)
