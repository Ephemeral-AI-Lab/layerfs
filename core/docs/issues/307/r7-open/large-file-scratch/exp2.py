"""Scratch: run rows, overwrites and reclamation against copies of the fdc24ef3f schema text."""
import os, random, shutil, sqlite3, sys, time
from exp import (D, SCHEMA, CELL_PUT, INODE_PUT, FRONTIER, MIB, TOTAL, WINDOW, open_db, real_schema, report, run_cells)

RUN_TABLE = """CREATE TABLE payload (
    rowid INTEGER PRIMARY KEY,
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    serial INTEGER NOT NULL CHECK(serial>0),
    gen INTEGER NOT NULL CHECK(gen=-1 OR gen>0),
    cell_offset INTEGER NOT NULL CHECK(cell_offset>=0 AND cell_offset%4096=0),
    epoch INTEGER NOT NULL CHECK(epoch>=0),
    data BLOB NOT NULL CHECK(length(data) BETWEEN 1 AND 1048576),
    validity BLOB CHECK(validity IS NULL OR length(validity)=(length(data)+7)/8),
    UNIQUE(ns,serial,gen,cell_offset)
) STRICT;"""
TWO = ("CREATE INDEX payload_namespace_row ON payload(ns,rowid);",
       "CREATE INDEX payload_generation ON payload(ns,gen,serial,cell_offset);")
PAYLOAD = ("SELECT rowid,length(data)+ifnull(length(validity),0) FROM payload INDEXED BY payload_namespace_row "
           "WHERE ns=?1 ORDER BY rowid LIMIT ?2")


def run_rows(label, name, extent, mode, drop=(), window=WINDOW):
    c, path = open_db(name)
    real_schema(c, RUN_TABLE, drop)
    buf = os.urandom(window)
    parts = [bytes(buf[i:i + extent]) for i in range(0, window, extent)]
    zero = ("INSERT INTO payload(ns,serial,gen,cell_offset,epoch,data,validity) VALUES(?1,?2,?3,?4,?5,zeroblob(?6),NULL) "
            "ON CONFLICT(ns,serial,gen,cell_offset) DO UPDATE SET epoch=excluded.epoch,data=excluded.data,validity=excluded.validity")
    growth, last = [], os.path.getsize(path)
    start = time.perf_counter()
    for request in range(TOTAL // window):
        base = request * window
        c.execute("BEGIN IMMEDIATE")
        c.execute(INODE_PUT, (1, 5, 1, 1, 0o644, 0, 0, 1, base + window, 0, 1, 0, 0, 0))
        for i, part in enumerate(parts):
            if mode == "bind":
                c.execute(CELL_PUT, (1, 5, 1, base + i * extent, 0, part, None))
            else:
                row = c.execute(zero, (1, 5, 1, base + i * extent, 0, len(part))).lastrowid
                with c.blobopen("payload", "data", row) as blob:
                    blob[0:len(part)] = part
        c.execute(FRONTIER, (1, request + 1, 0, 0))
        c.execute("COMMIT")
        now = os.path.getsize(path)
        growth.append(now - last)
        last = now
    report(c, path, label, time.perf_counter() - start, growth)
    return c, path


def reclaim(label, source, limit, single=False):
    path = os.path.join(D, "reclaim.sqlite")
    shutil.copyfile(os.path.join(D, source), path)
    c = sqlite3.connect(path, isolation_level=None)
    c.executescript("PRAGMA journal_mode=MEMORY; PRAGMA synchronous=OFF; PRAGMA locking_mode=EXCLUSIVE;"
                    "PRAGMA foreign_keys=ON; PRAGMA cache_size=-2048;")
    before = (os.path.getsize(path), c.execute("PRAGMA freelist_count").fetchone()[0])
    steps = rows = 0
    start = time.perf_counter()
    while True:
        c.execute("BEGIN IMMEDIATE")
        if single:
            n = c.execute("DELETE FROM payload WHERE rowid IN (SELECT rowid FROM payload INDEXED BY payload_namespace_row "
                          "WHERE ns=?1 ORDER BY rowid LIMIT ?2)", (1, limit)).rowcount
        else:
            page = c.execute(PAYLOAD, (1, limit)).fetchall()
            c.executemany("DELETE FROM payload WHERE rowid=?1 AND ns=?2", [(r, 1) for r, _ in page])
            n = len(page)
        c.execute("COMMIT")
        steps += 1
        rows += n
        if n == 0:
            break
    seconds = time.perf_counter() - start
    after = (os.path.getsize(path), c.execute("PRAGMA freelist_count").fetchone()[0],
             c.execute("PRAGMA page_count").fetchone()[0])
    print(f"## reclaim {label}: source={source} limit={limit} single_statement={single}: {seconds*1000:.1f} ms, "
          f"transactions={steps}, rows={rows}, file before={before[0]} after={after[0]}, freelist {before[1]} -> {after[1]} of {after[2]} pages")
    c.close()


def overwrite(label, source, extent, mode, count=2000):
    path = os.path.join(D, "overwrite.sqlite")
    shutil.copyfile(os.path.join(D, source), path)
    c = sqlite3.connect(path, isolation_level=None)
    c.executescript("PRAGMA journal_mode=MEMORY; PRAGMA synchronous=OFF; PRAGMA locking_mode=EXCLUSIVE;"
                    "PRAGMA foreign_keys=ON; PRAGMA cache_size=-2048;")
    rng = random.Random(7)
    new = os.urandom(4096)
    size0 = os.path.getsize(path)
    start = time.perf_counter()
    for _ in range(count):
        at = rng.randrange(TOTAL // 4096) * 4096
        c.execute("BEGIN IMMEDIATE")
        if mode == "cell":
            c.execute(CELL_PUT, (1, 5, 1, at, 0, new, None))
        else:
            row_at = at - at % extent
            if mode == "blob":
                (row,) = c.execute("SELECT rowid FROM payload WHERE ns=1 AND serial=5 AND gen=1 AND cell_offset=?1", (row_at,)).fetchone()
                with c.blobopen("payload", "data", row) as blob:
                    blob[at - row_at:at - row_at + 4096] = new
            else:
                (old,) = c.execute("SELECT data FROM payload WHERE ns=1 AND serial=5 AND gen=1 AND cell_offset=?1", (row_at,)).fetchone()
                merged = old[:at - row_at] + new + old[at - row_at + 4096:]
                c.execute(CELL_PUT, (1, 5, 1, row_at, 0, merged, None))
        c.execute("COMMIT")
    seconds = time.perf_counter() - start
    print(f"## overwrite {label}: {count} random 4 KiB overwrites, one transaction each: {seconds*1e6/count:.1f} us each, "
          f"file growth {os.path.getsize(path)-size0} bytes, freelist {c.execute('PRAGMA freelist_count').fetchone()[0]}")
    c.close()


if __name__ == "__main__":
    which = sys.argv[1:]
    if "rows" in which:
        run_rows("X64 run rows 64 KiB, bound blob, three b-trees kept", "x64.sqlite", 65536, "bind")
        run_rows("X64z run rows 64 KiB, zeroblob + incremental blob write", "x64z.sqlite", 65536, "blob")
        run_rows("X128 run rows 128 KiB, bound blob", "x128.sqlite", 131072, "bind")
        run_rows("X128z run rows 128 KiB, zeroblob + incremental blob write, no secondary indexes", "x128z.sqlite", 131072, "blob", drop=TWO)
        run_rows("X1M 1 MiB window as sixteen 64 KiB rows, one transaction per MiB", "x1m.sqlite", 65536, "bind", window=MIB)
        run_rows("X16 run rows 16 KiB, bound blob", "x16.sqlite", 16384, "bind")
    if "reclaim" in which:
        reclaim("current statements", "v0.sqlite", 14)
        reclaim("row window 64", "v0.sqlite", 64)
        reclaim("row window 512", "v0.sqlite", 512)
        reclaim("one statement per step", "v0.sqlite", 512, single=True)
        reclaim("64 KiB run rows, current statements", "x64.sqlite", 14)
        reclaim("128 KiB run rows, current statements", "x128.sqlite", 14)
    if "overwrite" in which:
        overwrite("V0 4 KiB cells, upsert", "v0.sqlite", 4096, "cell")
        overwrite("X64 64 KiB rows, read+merge+upsert whole row", "x64.sqlite", 65536, "row")
        overwrite("X64 64 KiB rows, incremental blob write", "x64.sqlite", 65536, "blob")
        overwrite("X128 128 KiB rows, incremental blob write", "x128.sqlite", 131072, "blob")
