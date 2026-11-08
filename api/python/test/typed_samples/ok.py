"""Checked by mypy --strict in test_stubs.py (never run): every line is
accepted by the stubs."""

import asyncio
from pathlib import Path

import layerfs
from layerfs import aio


def on_progress(progress: layerfs.Progress) -> None:
    phase: object = progress.get("phase")
    print(phase)


def on_output(data: str | bytes) -> None:
    print(data)


def sync_flow() -> None:
    lfs = layerfs.client(home=Path("/data/lfs"), scope={"project": "shop/api", "folder": "/web", "branch": "agent"})
    lfs.store.create("shop")
    lfs.project.init("shop/api", "/src/api", on_progress=on_progress, timeout=60)
    lfs.branch.fork("shop/api", "agent", "main")
    lfs.branch.fork(project="shop/api", name="agent", from_branch="main", from_commit=None)
    lfs.branch.commits("shop/api", "agent", page=1, page_size=50, at_commit="saved-id")
    lfs.branch.get_commit("shop/api", "main", "layer-id")
    lfs.branch.diff("shop/api", "main", "agent", from_commit="layer-id", to_commit="saved-id", include_attribution=True, page=1)
    lfs.file.revision("shop/api", "agent", "/README.md", at_commit="saved-id", page=1, page_size=10)
    lfs.file.diff("shop/api", "main", "/README.md", "agent", "/README.md", include_attribution=True, allow_missing=True, page=1)
    lfs.file.write("shop/api", "agent", "/a.bin", b"\x00\xff", message="bytes")
    lfs.file.write("shop/api", "agent", "/a.txt", "text\n")
    text = lfs.file.read("shop/api", "agent", "/a.txt", start_line=2)["text"]
    print(text)
    lfs.file.apply("shop/api", "agent", [{"op": "write", "path": "/x", "content": "y"}], message="apply")
    lfs.branch.merge("shop/api", "m", "a", "b", units=["*.py", {"name": "u", "patterns": ["*"]}])
    lfs.machine.create("box", "docker", memory="2G", cpus=2)
    lfs.workspace.exec("ws", "ls", stdout=on_output, stderr=on_output, stream=False)
    lfs.backup.create("shop", "bucket", on_progress=on_progress)
    lfs.project.export("shop/api", "/exports/api", branch="main", on_progress=on_progress)
    lfs.project.export("shop/api", "/exports/api-default")
    lfs.workspace.exec_cancel("ws", "exec-0123456789abcdef", grace_ms=1000)
    try:
        lfs.sandbox.close("s1", force=True)
    except layerfs.LayerFSError as error:
        code: str = error.code
        message: str = error.message
        print(code, message, error.detail)
    report = layerfs.doctor()
    ok: bool = report["ok"]
    severity: str = report["findings"][0]["severity"] if report["findings"] else "none"
    print(ok, severity)
    layerfs.project.list()
    layerfs.file.read("shop/api", "main", "/a", commit="c1")
    closed: int = layerfs.close()
    print(closed, layerfs.FAMILIES, len(layerfs.TABLE))


async def async_flow() -> None:
    lfs = aio.client(home="/data/lfs")
    await lfs.store.create("shop")
    await lfs.project.init("shop/api", "/src/api", on_progress=on_progress)
    await lfs.branch.commits("shop/api", "main", page=1, page_size=10)
    await lfs.branch.get_commit("shop/api", "agent", "saved-id")
    await lfs.file.revision("shop/api", "agent", "/README.md", at_commit="saved-id", page=1)
    listed = await lfs.project.list("shop", page_size=10)
    print(listed["items"])
    await asyncio.gather(lfs.store.create("a"), lfs.store.create("b"))
    await lfs.workspace.exec("ws", "ls", stdout=on_output)
    await lfs.project.export("shop/api", "/exports/api", branch="agent", on_progress=on_progress)
    await lfs.workspace.exec_cancel("ws", "exec-0123456789abcdef")
    await aio.store.push("shop", "machine", on_progress=on_progress)
    try:
        await aio.file.stat("shop/api", "main", "/a")
    except aio.LayerFSError as error:
        print(error.code)


if __name__ == "__main__":
    sync_flow()
    asyncio.run(async_flow())
