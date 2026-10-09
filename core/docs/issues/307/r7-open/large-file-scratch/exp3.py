"""Engine-only CPU comparison on an in-memory database (no VFS writes). Relative numbers only."""
import os, sqlite3, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from exp import CELL_PUT, INODE_PUT, FRONTIER, TOTAL, WINDOW, real_schema
from exp2 import RUN_TABLE, TWO
ZERO = ("INSERT INTO payload(ns,serial,gen,cell_offset,epoch,data,validity) VALUES(?1,?2,?3,?4,?5,zeroblob(?6),NULL) "
        "ON CONFLICT(ns,serial,gen,cell_offset) DO UPDATE SET epoch=excluded.epoch,data=excluded.data,validity=excluded.validity")
def once(extent, mode, drop=(), window=WINDOW):
    c = sqlite3.connect(":memory:", isolation_level=None)
    c.executescript("PRAGMA page_size=4096; PRAGMA foreign_keys=ON; PRAGMA cache_size=-2048;")
    real_schema(c, RUN_TABLE, drop)
    buf = os.urandom(window)
    parts = [bytes(buf[i:i + extent]) for i in range(0, window, extent)]
    start = time.perf_counter()
    for request in range(TOTAL // window):
        base = request * window
        c.execute("BEGIN IMMEDIATE")
        c.execute(INODE_PUT, (1, 5, 1, 1, 0o644, 0, 0, 1, base + window, 0, 1, 0, 0, 0))
        if mode == "bind":
            c.executemany(CELL_PUT, [(1, 5, 1, base + i * extent, 0, p, None) for i, p in enumerate(parts)])
        else:
            for i, p in enumerate(parts):
                row = c.execute(ZERO, (1, 5, 1, base + i * extent, 0, len(p))).lastrowid
                with c.blobopen("payload", "data", row) as blob:
                    blob[0:len(p)] = p
        c.execute(FRONTIER, (1, request + 1, 0, 0))
        c.execute("COMMIT")
    return (time.perf_counter() - start) * 1000
for label, args in [
    ("4 KiB cells, bound blob, 3 b-trees + table (current shape)", (4096, "bind")),
    ("4 KiB cells, bound blob, UNIQUE index only", (4096, "bind", TWO)),
    ("16 KiB rows, bound blob", (16384, "bind")),
    ("64 KiB rows, bound blob", (65536, "bind")),
    ("64 KiB rows, zeroblob + blob write", (65536, "blob")),
    ("128 KiB rows, bound blob", (131072, "bind")),
    ("128 KiB rows, zeroblob + blob write", (131072, "blob")),
    ("64 KiB rows, bound blob, 1 MiB transactions", (65536, "bind", (), 1 << 20)),
]:
    print(f"{label:62s}", " ".join(f"{once(*args):7.1f}" for _ in range(4)), "ms per 64 MiB (4 runs)")
