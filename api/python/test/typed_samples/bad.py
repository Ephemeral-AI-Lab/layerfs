"""Checked by mypy --strict in test_stubs.py (never run): every line marked
`# E` is an error the stubs must report, and no other line is."""

import layerfs
from layerfs import aio

lfs = layerfs.client()


async def run() -> None:
    aio.store.create("shop")  # E: a coroutine nobody awaits
    await aio.store.create(1)  # E: a name is text
    await aio.file.read("shop/api", "main", "/a", start_line="2")  # E: a line is a number


layerfs.store.create(1)  # E: a name is text
layerfs.file.read("shop/api", "main", "/a", bogus=1)  # E: no such option
layerfs.file.read("shop/api", "main", "/a", "c1", "extra")  # E: too many positionals
layerfs.file.read("shop/api", "main")  # E: path is required
layerfs.project.init("shop/api", "/src", on_progress=3)  # E: a function option takes a function
layerfs.file.write("shop/api", "main", "/a", 12)  # E: content is text or bytes
layerfs.machine.create("m", "docker", memory=1.5)  # E: a size is a number or text
lfs.nosuch.list()  # E: no such family
layerfs.doctor()["nosuch"]  # E: not a field of the report
layerfs.client(scope={"project": 1, "folder": "/"})  # E: scope.project is text
