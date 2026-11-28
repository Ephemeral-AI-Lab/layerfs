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
    lfs = layerfs.client(root=Path("/data/lfs"), scope={"project": "shop/api", "folder": "/web", "branch": "agent"})
    lfs.store.create("shop")
    lfs.project.init("shop/api", "/src/api", on_progress=on_progress, timeout=60)
    lfs.branch.fork("shop/api", "agent", "main")
    lfs.branch.fork(project="shop/api", name="agent", from_="main", at=None)
    lfs.commit.list("shop/api", "agent", at="32ab9e01", limit=50, cursor="b3A6MQ")
    lfs.commit.get("shop/api", "32ab9e01", branch="main", detail=True)
    lfs.commit.diff("shop/api", "agent", from_="main", path="/README.md", with_=["patch"])
    lfs.home.get(with_=["platforms"])
    lfs.store.get("shop", with_=["live", "stats"], log_tag="run-1")
    lfs.file.history("shop/api", "agent", "/README.md", at="saved-id", limit=10)
    lfs.file.write("shop/api", "agent", "/a.bin", b"\x00\xff", message="bytes")
    lfs.file.write("shop/api", "agent", "/a.txt", "text\n")
    text = lfs.file.read("shop/api", "agent", "/a.txt", line=2)["text"]
    print(text)
    lfs.file.apply("shop/api", "agent", [{"op": "write", "path": "/x", "content": "y"}], message="apply")
    lfs.branch.merge("shop/api", "a", "b", into="m", units=["*.py", {"name": "u", "patterns": ["*"]}])
    lfs.machine.create("box", "docker", memory="2G", cpus=2)
    lfs.workspace.exec("ws", "ls", stdout=on_output, stderr=on_output, stream=False)
    lfs.backup.create("shop", "bucket", on_progress=on_progress)
    lfs.project.export("shop/api", "/exports/api", branch="main", on_progress=on_progress)
    lfs.project.export("shop/api", "/exports/api-default")
    lfs.workspace.cancel("shop:dev", exec="exec-0123456789abcdef")
    try:
        lfs.sandbox.remove("s1", force=True)
    except layerfs.LayerFSError as error:
        code: str = error.code
        message: str = error.message
        print(code, error.error, message, error.fix, error.detail)
    layerfs.project.list()
    layerfs.file.read("shop/api", "main", "/a", at="c1")
    closed: int = layerfs.close()
    print(closed, layerfs.FAMILIES, len(layerfs.TABLE))


async def async_flow() -> None:
    lfs = aio.client(root="/data/lfs")
    await lfs.store.create("shop")
    await lfs.project.init("shop/api", "/src/api", on_progress=on_progress)
    await lfs.commit.list("shop/api", "main", limit=10)
    await lfs.commit.get("shop/api", "32ab9e01", branch="agent")
    await lfs.file.history("shop/api", "agent", "/README.md", at="saved-id", limit=10)
    listed = await lfs.project.list(store="shop", limit=10)
    print(listed["items"])
    await asyncio.gather(lfs.store.create("a"), lfs.store.create("b"))
    await lfs.workspace.exec("ws", "ls", stdout=on_output)
    await lfs.project.export("shop/api", "/exports/api", branch="agent", on_progress=on_progress)
    await lfs.workspace.cancel("shop:dev", exec="exec-0123456789abcdef")
    await aio.store.move("shop", "machine", on_progress=on_progress)
    try:
        await aio.file.stat("shop/api", "main", "/a")
    except aio.LayerFSError as error:
        print(error.code)


if __name__ == "__main__":
    sync_flow()
    asyncio.run(async_flow())
