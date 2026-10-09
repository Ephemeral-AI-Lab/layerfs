"""Scratch page arithmetic for the overlay payload table. Read-only w.r.t. the repo:
schema.sql/accounting.sql here are `git show fdc24ef3f:` copies."""
import os, sqlite3, sys, time

D = os.path.dirname(os.path.abspath(__file__))
SCHEMA = open(os.path.join(D, "schema.sql")).read()
ACCOUNTING = open(os.path.join(D, "accounting.sql")).read()
CELL_PUT = ("INSERT INTO payload(ns,serial,gen,cell_offset,epoch,data,validity) VALUES(?1,?2,?3,?4,?5,?6,?7) "
            "ON CONFLICT(ns,serial,gen,cell_offset) DO UPDATE SET epoch=excluded.epoch,"
            "data=excluded.data,validity=excluded.validity")
INODE_PUT = ("INSERT INTO inode VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14) "
             "ON CONFLICT(ns,serial,gen) DO UPDATE SET kind=excluded.kind,mode=excluded.mode,"
             "mtime_seconds=excluded.mtime_seconds,mtime_nanoseconds=excluded.mtime_nanoseconds,"
             "nlink=excluded.nlink,size=excluded.size,inherited_cutoff=excluded.inherited_cutoff,"
             "born=excluded.born,entries=excluded.entries,epoch=excluded.epoch,height=excluded.height")
FRONTIER = "UPDATE workspace SET revision=?2,dirty_inodes=dirty_inodes+?3,dirty_directory_entries=dirty_directory_entries+?4 WHERE ns=?1"
MIB = 1 << 20
TOTAL = 64 * MIB
WINDOW = 128 * 1024


def open_db(name, page_size=4096, cache_kib=2048):
    path = os.path.join(D, name)
    for suffix in ("", "-journal"):
        try:
            os.remove(path + suffix)
        except FileNotFoundError:
            pass
    c = sqlite3.connect(path, isolation_level=None)
    c.executescript(
        f"PRAGMA page_size={page_size}; PRAGMA auto_vacuum=NONE; PRAGMA journal_mode=MEMORY;"
        f"PRAGMA synchronous=OFF; PRAGMA locking_mode=EXCLUSIVE; PRAGMA mmap_size=0;"
        f"PRAGMA foreign_keys=ON; PRAGMA temp_store=FILE; PRAGMA cache_size=-{cache_kib};")
    return c, path


def real_schema(c, payload_sql=None, drop=()):
    schema = SCHEMA
    if payload_sql is not None:
        start = schema.index("CREATE TABLE payload (")
        end = schema.index("CREATE TABLE shrink (")
        schema = schema[:start] + payload_sql + "\n" + schema[end:]
    for line in drop:
        assert line in schema, line
        schema = schema.replace(line, "")
    c.executescript(schema)
    c.executescript(ACCOUNTING)
    c.execute("INSERT INTO workspace(incarnation,base_root,active) VALUES(?1,?2,1)", (b"\x01" * 32, b"\x02" * 32))
    c.execute(INODE_PUT, (1, 5, 1, 1, 0o644, 0, 0, 1, 0, 0, 1, 0, 0, 0))


def report(c, path, label, seconds, growth):
    size = os.path.getsize(path)
    pages = c.execute("PRAGMA page_count").fetchone()[0]
    psize = c.execute("PRAGMA page_size").fetchone()[0]
    free = c.execute("PRAGMA freelist_count").fetchone()[0]
    print(f"## {label}: {seconds*1000:.1f} ms wall (python driver, macOS host), file {size} bytes = {size/MIB:.2f} MiB, "
          f"page_size {psize}, pages {pages}, freelist {free}, overhead {100*(size-TOTAL)/TOTAL:.2f}% over 64 MiB")
    rows = c.execute(
        "SELECT name, count(*), sum(pagetype='leaf'), sum(pagetype='internal'), sum(pagetype='overflow'),"
        " sum(payload), sum(unused), max(length(path)-length(replace(path,'/',''))) FROM dbstat"
        " GROUP BY name HAVING count(*)>2 ORDER BY 2 DESC").fetchall()
    for name, n, leaf, internal, overflow, payload, unused, depth in rows:
        print(f"   {name:34s} pages={n:6d} leaf={leaf:5d} interior={internal:3d} overflow={overflow:6d} "
              f"payload={payload:9d} unused={unused:8d} depth={depth}")
    if growth:
        mid = growth[len(growth) // 2 - 8:len(growth) // 2 + 8]
        print(f"   file growth per transaction, 16 mid-run samples (pages): {[g // psize for g in mid]}"
              f" mean={sum(growth)/len(growth)/psize:.2f}")
    sys.stdout.flush()


def run_cells(label, name, cell=4096, data=None, payload_sql=None, drop=(), page_size=4096, per_txn=WINDOW):
    """Current shape: one upsert per `cell` bytes, one transaction per 128 KiB window."""
    c, path = open_db(name, page_size)
    real_schema(c, payload_sql, drop)
    put = CELL_PUT
    buf = data if data is not None else os.urandom(WINDOW)
    views = [bytes(buf[i:i + cell]) for i in range(0, WINDOW, cell)]
    growth, last = [], os.path.getsize(path)
    start = time.perf_counter()
    for request in range(TOTAL // per_txn):
        base = request * per_txn
        c.execute("BEGIN IMMEDIATE")
        c.execute(INODE_PUT, (1, 5, 1, 1, 0o644, 0, 0, 1, base + per_txn, 0, 1, 0, 0, 0))
        c.executemany(put, [(1, 5, 1, base + i * cell, 0, views[i % len(views)], None) for i in range(per_txn // cell)])
        c.execute(FRONTIER, (1, request + 1, 0, 0))
        c.execute("COMMIT")
        now = os.path.getsize(path)
        growth.append(now - last)
        last = now
    seconds = time.perf_counter() - start
    report(c, path, label, seconds, growth)
    return c, path


if __name__ == "__main__":
    which = sys.argv[1:] or ["v0"]
    if "v0" in which:
        c, p = run_cells("V0 real schema v21, 4096-byte cells, random data", "v0.sqlite")
        print("   accounting:", c.execute("SELECT payload_cells,payload_bytes FROM accounting WHERE ns=1").fetchone())
        for (line,) in c.execute("EXPLAIN QUERY PLAN SELECT cell_offset,epoch,data,validity FROM payload WHERE ns=1 AND serial=5 AND gen=1 AND cell_offset>=0 AND cell_offset<131072 ORDER BY cell_offset").fetchall() if False else []:
            print(line)
    if "v0z" in which:
        run_cells("V0z real schema, 4096-byte cells, zero data", "v0z.sqlite", data=bytes(WINDOW))
    if "v1" in which:
        run_cells("V1 real schema minus payload_namespace_row and payload_generation", "v1.sqlite",
                  drop=("CREATE INDEX payload_namespace_row ON payload(ns,rowid);",
                        "CREATE INDEX payload_generation ON payload(ns,gen,serial,cell_offset);"))
    if "v2" in which:
        run_cells("V2 WITHOUT ROWID payload keyed (ns,serial,gen,cell_offset), no secondary index", "v2.sqlite",
                  payload_sql="""CREATE TABLE payload (
    ns INTEGER NOT NULL REFERENCES workspace(ns),
    serial INTEGER NOT NULL CHECK(serial>0),
    gen INTEGER NOT NULL CHECK(gen=-1 OR gen>0),
    cell_offset INTEGER NOT NULL CHECK(cell_offset>=0 AND cell_offset%4096=0),
    epoch INTEGER NOT NULL CHECK(epoch>=0),
    data BLOB NOT NULL CHECK(length(data) BETWEEN 1 AND 4096),
    validity BLOB CHECK(validity IS NULL OR length(validity)=(length(data)+7)/8),
    PRIMARY KEY(ns,serial,gen,cell_offset)
) STRICT, WITHOUT ROWID;""",
                  drop=("CREATE INDEX payload_namespace_row ON payload(ns,rowid);",
                        "CREATE INDEX payload_generation ON payload(ns,gen,serial,cell_offset);"))
