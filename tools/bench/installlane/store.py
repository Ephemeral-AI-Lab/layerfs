#!/usr/bin/env python3
"""Store-shaped model for the install-only-lane contract prototype.

This is **not** product code and is not wired into the persistence crate. It
models the physical tables an SQLite Store compaction mark/build/install
rewrites (pack, object_location, object_ref, root/root_object, kept_node,
high_water) so the contract an install-only lane would need can be exercised:

* an *open* mark that records a high-water tuple and can be extended by later
  commits,
* an incremental re-mark that starts from the tuple and walks the closure of
  roots committed after it (including references to pre-mark packs),
* a delta copy of every newly-live pack/object/metadata row into the staged
  target,
* a catch-up fence that quiesces writers, copies the residual delta and
  verifies source-vs-target closure equality.

The fixture is synthetic: each pack is 1 MiB of ``zeroblob`` and owns eight
128 KiB canonical objects.  A quarter of the packs are dead at the mark; those
objects are what a content-addressed Store can dedupe a later commit onto
(the hazard that makes the delta unbounded).
"""

from __future__ import annotations

import hashlib
import os
import random
import sqlite3
import threading
import time

MIB = 1 << 20
OBJECTS_PER_PACK = 8
CANONICAL = MIB // OBJECTS_PER_PACK

SCHEMA = """
CREATE TABLE store_meta(generation INTEGER NOT NULL);
CREATE TABLE high_water(
    serial INTEGER NOT NULL,
    next_pack_id INTEGER NOT NULL,
    next_ordinal INTEGER NOT NULL,
    pack_bytes INTEGER NOT NULL,
    logical INTEGER NOT NULL
);
CREATE TABLE pack(pack_id INTEGER PRIMARY KEY, body BLOB NOT NULL);
CREATE TABLE object_location(object_id BLOB PRIMARY KEY, pack_id INTEGER NOT NULL);
CREATE TABLE object_ref(object_id BLOB NOT NULL, ref_id BLOB NOT NULL, PRIMARY KEY(object_id, ref_id));
CREATE TABLE root(root_id BLOB PRIMARY KEY, kind TEXT NOT NULL, serial INTEGER NOT NULL);
CREATE TABLE root_object(root_id BLOB NOT NULL, object_id BLOB NOT NULL, PRIMARY KEY(root_id, object_id));
CREATE TABLE kept_node(kind TEXT NOT NULL, node_id BLOB NOT NULL, serial INTEGER NOT NULL, PRIMARY KEY(kind, node_id));
"""


def oid(pack_id: int, index: int) -> bytes:
    return hashlib.blake2b(f"o{pack_id}:{index}".encode(), digest_size=16).digest()


def rid(n: int) -> bytes:
    return hashlib.blake2b(f"r{n}".encode(), digest_size=16).digest()


def new_root_id(serial: int, salt: int = 0) -> bytes:
    return hashlib.blake2b(f"n{serial}:{salt}".encode(), digest_size=16).digest()


def connect(path: str) -> sqlite3.Connection:
    conn = sqlite3.connect(path, timeout=60.0, isolation_level=None)
    conn.execute("PRAGMA busy_timeout=60000")
    # The sealed Store cannot be checkpointed while the build holds its read
    # snapshot (in production the file is attached immutable=1).  No automatic
    # checkpoint means the WAL grows by every committed byte and install pays
    # the whole backlog, which is the cost this prototype measures.
    conn.execute("PRAGMA wal_autocheckpoint=0")
    return conn


def wal_bytes(path: str) -> int:
    try:
        return os.path.getsize(path + "-wal")
    except OSError:
        return 0


def reset(path: str) -> None:
    for suffix in ("", "-wal", "-shm", "-journal"):
        try:
            os.unlink(path + suffix)
        except OSError:
            pass


def make_fixture(path: str, size_mib: int, dead_ratio: float = 0.25, seed: int = 7) -> dict:
    """Create a store-shaped source: ``size_mib`` packs of 1 MiB.

    ``dead_ratio`` of the packs are unreachable at the mark (no root reaches
    them); their objects stay in the store and are the dedupe targets a later
    commit can reference.
    """
    reset(path)
    conn = connect(path)
    # Fixture creation is not a product step: journal OFF keeps the 10 GiB arm
    # from paying a full WAL copy before the measurement starts.  The model
    # switches to WAL before any measured step.
    conn.execute("PRAGMA journal_mode=OFF")
    conn.execute("PRAGMA synchronous=OFF")
    conn.execute("PRAGMA cache_size=-262144")
    conn.executescript(SCHEMA)
    rng = random.Random(seed)
    packs = size_mib
    dead = set()
    live = []
    for pack_id in range(1, packs + 1):
        if rng.random() < dead_ratio:
            dead.add(pack_id)
        else:
            live.append(pack_id)
    # one root per live pack entry, plus chained refs inside each pack
    loc_rows: list[tuple[bytes, int]] = []
    ref_rows: list[tuple[bytes, bytes]] = []
    root_rows: list[tuple[bytes, str, int]] = []
    root_object_rows: list[tuple[bytes, bytes]] = []
    kept_rows: list[tuple[str, bytes, int]] = []
    serial = 0
    for pack_id in range(1, packs + 1):
        ids = [oid(pack_id, index) for index in range(OBJECTS_PER_PACK)]
        loc_rows.extend((object_id, pack_id) for object_id in ids)
        ref_rows.extend((ids[0], ids[index]) for index in range(1, OBJECTS_PER_PACK))
        if pack_id in live:
            serial += 1
            root_id = rid(serial)
            root_rows.append((root_id, "branch", serial))
            root_object_rows.append((root_id, ids[0]))
            kept_rows.append(("root", root_id, serial))
    conn.execute("BEGIN")
    conn.executemany(
        "INSERT INTO pack(pack_id, body) VALUES(?, zeroblob(?))",
        [(pack_id, MIB) for pack_id in range(1, packs + 1)],
    )
    conn.executemany("INSERT INTO object_location VALUES(?, ?)", loc_rows)
    conn.executemany("INSERT INTO object_ref VALUES(?, ?)", ref_rows)
    conn.executemany("INSERT INTO root VALUES(?, ?, ?)", root_rows)
    conn.executemany("INSERT INTO root_object VALUES(?, ?)", root_object_rows)
    conn.executemany("INSERT INTO kept_node VALUES(?, ?, ?)", kept_rows)
    conn.execute("INSERT INTO store_meta VALUES(1)")
    conn.execute(
        "INSERT INTO high_water VALUES(?, ?, ?, ?, ?)",
        (serial, packs + 1, len(loc_rows), packs * MIB, packs * MIB),
    )
    conn.execute("COMMIT")
    conn.execute("PRAGMA journal_mode=WAL")
    conn.execute("PRAGMA synchronous=NORMAL")
    conn.close()
    return {
        "packs": packs,
        "dead_packs": sorted(dead),
        "live_packs": len(live),
        "objects": packs * OBJECTS_PER_PACK,
        "visited_objects_at_mark": len(live) * OBJECTS_PER_PACK,
        "bytes": packs * MIB,
        "fixture_seconds": None,
        "dead_bytes": len(dead) * MIB,
    }


def read_high_water(conn: sqlite3.Connection) -> tuple:
    return conn.execute("SELECT serial, next_pack_id, next_ordinal, pack_bytes, logical FROM high_water").fetchone()


def _fetch_edges(conn: sqlite3.Connection) -> dict[bytes, tuple[bytes, ...]]:
    edges: dict[bytes, list[bytes]] = {}
    for object_id, ref_id in conn.execute("SELECT object_id, ref_id FROM object_ref"):
        edges.setdefault(object_id, []).append(ref_id)
    return {key: tuple(value) for key, value in edges.items()}


def closure(conn: sqlite3.Connection, roots: list[bytes], edges: dict | None = None) -> set[bytes]:
    """Reachability closure from ``roots`` over ``object_ref``."""
    edges = edges if edges is not None else _fetch_edges(conn)
    visited: set[bytes] = set()
    stack = list(roots)
    while stack:
        object_id = stack.pop()
        if object_id in visited:
            continue
        visited.add(object_id)
        stack.extend(edges.get(object_id, ()))
    return visited


def locations(conn: sqlite3.Connection) -> dict[bytes, int]:
    return dict(conn.execute("SELECT object_id, pack_id FROM object_location"))


def mark(path: str) -> dict:
    """Read the high-water tuple, then walk the closure of roots up to it.

    The tuple is read *first*, in one statement; a commit that lands after it
    is not part of the mark and must be caught by the incremental re-mark.
    """
    started = time.perf_counter()
    conn = connect(path)
    hw = read_high_water(conn)
    roots = [row[0] for row in conn.execute("SELECT root_id FROM root WHERE serial <= ? ORDER BY serial", (hw[0],))]
    root_objects = [
        row[0]
        for row in conn.execute(
            "SELECT o.object_id FROM root_object o JOIN root r USING(root_id) WHERE r.serial <= ?",
            (hw[0],),
        )
    ]
    edges = _fetch_edges(conn)
    visited = closure(conn, root_objects, edges)
    where = locations(conn)
    conn.close()
    live_packs = {where[object_id] for object_id in visited if object_id in where}
    return {
        "hw": hw,
        "roots": roots,
        "visited": visited,
        "live_packs": live_packs,
        "objects": len(visited),
        "packs": len(live_packs),
        "bytes": len(live_packs) * MIB,
        "seconds": time.perf_counter() - started,
    }


def _install_schema(conn: sqlite3.Connection) -> None:
    conn.executescript(SCHEMA)


def build(source: str, target: str, marked: dict) -> dict:
    """Fresh-copy build of the mark's live set into ``target`` (no dead rows)."""
    started = time.perf_counter()
    reset(target)
    out = connect(target)
    # The fresh-copy build writes its target with journal OFF (fx-src arm);
    # the target is switched to WAL at install, which is a separate step.
    out.execute("PRAGMA journal_mode=OFF")
    out.execute("PRAGMA synchronous=OFF")
    out.execute("PRAGMA cache_size=-262144")
    _install_schema(out)
    out.execute("BEGIN")
    out.execute("ATTACH DATABASE ? AS src", (source,))
    out.execute("CREATE TEMP TABLE live(pack_id INTEGER PRIMARY KEY)")
    out.executemany("INSERT INTO live VALUES(?)", [(p,) for p in marked["live_packs"]])
    out.execute(
        "INSERT OR IGNORE INTO pack(pack_id, body) SELECT pack_id, body FROM src.pack "
        "WHERE pack_id IN (SELECT pack_id FROM live)"
    )
    out.execute("CREATE TEMP TABLE keep(object_id BLOB PRIMARY KEY)")
    out.executemany("INSERT INTO keep VALUES(?)", [(o,) for o in marked["visited"]])
    out.execute(
        "INSERT OR IGNORE INTO object_location SELECT o.object_id, o.pack_id FROM src.object_location o "
        "JOIN keep k USING(object_id)"
    )
    out.execute(
        "INSERT OR IGNORE INTO object_ref SELECT e.object_id, e.ref_id FROM src.object_ref e "
        "JOIN keep a ON a.object_id = e.object_id JOIN keep b ON b.object_id = e.ref_id"
    )
    out.execute(
        "INSERT INTO root SELECT r.root_id, r.kind, r.serial FROM src.root r WHERE r.serial <= ?",
        (marked["hw"][0],),
    )
    out.execute(
        "INSERT INTO root_object SELECT ro.root_id, ro.object_id FROM src.root_object ro "
        "JOIN src.root r ON r.root_id = ro.root_id WHERE r.serial <= ?",
        (marked["hw"][0],),
    )
    out.execute(
        "INSERT INTO kept_node SELECT kind, node_id, serial FROM src.kept_node WHERE serial <= ?",
        (marked["hw"][0],),
    )
    generation = out.execute("SELECT generation FROM src.store_meta").fetchone()[0] + 1
    out.execute("INSERT INTO store_meta VALUES(?)", (generation,))
    out.execute("INSERT INTO high_water VALUES(?, ?, ?, ?, ?)", marked["hw"])
    out.execute("COMMIT")
    out.close()
    return {
        "seconds": time.perf_counter() - started,
        "packs": len(marked["live_packs"]),
        "bytes": len(marked["live_packs"]) * MIB,
        "objects": len(marked["visited"]),
    }


def targets_visited(path: str) -> set[bytes]:
    conn = connect(path)
    visited = set(row[0] for row in conn.execute("SELECT object_id FROM object_location"))
    conn.close()
    return visited


def _copy_rows(
    out: sqlite3.Connection,
    source: str,
    delta_objects: set[bytes],
    delta_packs: set[int],
    roots: list[tuple],
    root_objects: list[tuple],
    kept: list[tuple],
    hw: tuple,
) -> None:
    out.execute("ATTACH DATABASE ? AS src", (source,))
    if delta_packs:
        out.execute("CREATE TEMP TABLE dp(pack_id INTEGER PRIMARY KEY)")
        out.executemany("INSERT INTO dp VALUES(?)", [(p,) for p in sorted(delta_packs)])
        out.execute(
            "INSERT OR IGNORE INTO pack(pack_id, body) SELECT pack_id, body FROM src.pack "
            "WHERE pack_id IN (SELECT pack_id FROM dp)"
        )
    if delta_objects:
        out.execute("CREATE TEMP TABLE do(object_id BLOB PRIMARY KEY)")
        out.executemany("INSERT INTO do VALUES(?)", [(o,) for o in delta_objects])
        out.execute(
            "INSERT OR IGNORE INTO object_location SELECT o.object_id, o.pack_id FROM src.object_location o "
            "JOIN do USING(object_id)"
        )
    out.execute("CREATE TEMP TABLE all_o(object_id BLOB PRIMARY KEY)")
    out.execute("INSERT INTO all_o SELECT object_id FROM object_location")
    out.execute(
        "INSERT OR IGNORE INTO object_ref SELECT e.object_id, e.ref_id FROM src.object_ref e "
        "JOIN all_o a ON a.object_id = e.object_id JOIN all_o b ON b.object_id = e.ref_id "
        "WHERE e.object_id NOT IN (SELECT object_id FROM object_ref)"
    )
    if roots:
        out.executemany("INSERT OR IGNORE INTO root VALUES(?, ?, ?)", roots)
    if root_objects:
        out.executemany("INSERT OR IGNORE INTO root_object VALUES(?, ?)", root_objects)
    if kept:
        out.executemany("INSERT OR IGNORE INTO kept_node VALUES(?, ?, ?)", kept)
    out.execute("UPDATE high_water SET serial=?, next_pack_id=?, next_ordinal=?, pack_bytes=?, logical=?", hw)


def remark_delta(source: str, target: str, marked: dict, mode: str = "closure") -> dict:
    """Incremental re-mark from the mark's tuple and delta copy.

    ``mode="closure"`` walks every root committed after the mark and copies
    each newly-live object's locating pack whole, even when that pack is
    pre-mark and the mark skipped it.  ``mode="new_packs_only"`` is the naive
    delta that copies only packs with ``pack_id >= next_pack_id``; it is kept
    to show the loss it causes.
    """
    started = time.perf_counter()
    scan_started = time.perf_counter()
    src = connect(source)
    hw_now = read_high_water(src)
    roots = [
        (row[0], row[1], row[2])
        for row in src.execute(
            "SELECT root_id, kind, serial FROM root WHERE serial > ? ORDER BY serial", (marked["hw"][0],)
        )
    ]
    root_objects = [
        (row[0], row[1])
        for row in src.execute(
            "SELECT ro.root_id, ro.object_id FROM root_object ro JOIN root r USING(root_id) WHERE r.serial > ?",
            (marked["hw"][0],),
        )
    ]
    kept = [
        (row[0], row[1], row[2])
        for row in src.execute("SELECT kind, node_id, serial FROM kept_node WHERE serial > ?", (marked["hw"][0],))
    ]
    edges = _fetch_edges(src)
    where = locations(src)
    new_objects = closure(src, [o for _, o in root_objects], edges)
    src.close()

    in_target = targets_visited(target)
    delta_objects = {o for o in new_objects if o not in in_target}
    if mode == "new_packs_only":
        delta_objects = {o for o in delta_objects if where.get(o, 0) >= marked["hw"][1]}
    delta_packs = {where[o] for o in delta_objects if o in where}
    scan_seconds = time.perf_counter() - scan_started
    copy_started = time.perf_counter()
    out = connect(target)
    out.execute("BEGIN")
    _copy_rows(out, source, delta_objects, delta_packs, roots, root_objects, kept, hw_now)
    out.execute("COMMIT")
    out.close()
    return {
        "mode": mode,
        "mark_serial": marked["hw"][0],
        "install_serial": hw_now[0],
        "new_roots": len(roots),
        "new_objects": len(new_objects),
        "delta_objects": len(delta_objects),
        "delta_packs": len(delta_packs),
        "delta_bytes": len(delta_packs) * MIB,
        "delta_seconds": time.perf_counter() - started,
        "scan_seconds": scan_seconds,
        "copy_seconds": time.perf_counter() - copy_started,
        "target_objects": len(in_target) + len(delta_objects),
    }


def verify(source: str, target: str, fence_serial: int | None = None) -> dict:
    """Source closure at the fence equals the target closure; every object has
    a pack row.  This is the check a catch-up fence must pass before rename."""
    src = connect(source)
    hw = read_high_water(src)
    fence = hw[0] if fence_serial is None else fence_serial
    root_objects = [
        row[0]
        for row in src.execute(
            "SELECT o.object_id FROM root_object o JOIN root r USING(root_id) WHERE r.serial <= ?",
            (fence,),
        )
    ]
    edges = _fetch_edges(src)
    source_visited = closure(src, root_objects, edges)
    source_packs = dict(src.execute("SELECT object_id, pack_id FROM object_location"))
    src.close()

    out = connect(target)
    target_visited = set(row[0] for row in out.execute("SELECT object_id FROM object_location"))
    target_packs = set(row[0] for row in out.execute("SELECT pack_id FROM pack"))
    out.close()
    missing = source_visited - target_visited
    extra = target_visited - source_visited
    absent_pack = {o for o in source_visited & target_visited if source_packs.get(o) not in target_packs}
    return {
        "fence_serial": fence,
        "source_objects": len(source_visited),
        "target_objects": len(target_visited),
        "missing": len(missing),
        "extra": len(extra),
        "objects_without_pack": len(absent_pack),
        "ok": not missing and not extra and not absent_pack,
    }


class Writer(threading.Thread):
    """Commit loop: each commit writes one 1 MiB pack (8 objects), a root and
    metadata; optionally references a pre-mark dead object (content dedupe)."""

    def __init__(
        self,
        path: str,
        stop: threading.Event,
        deadline: float = float("inf"),
        dedupe_every: int = 0,
        dead_pool: list[bytes] | None = None,
        salt: int = 0,
        max_commits: int = 0,
    ):
        super().__init__(daemon=True)
        self.path = path
        self.stop = stop
        self.deadline = deadline
        self.dedupe_every = dedupe_every
        self.dead_pool = dead_pool or []
        self.salt = salt
        self.latencies: list[float] = []
        self.bytes = 0
        self.commits = 0
        self.errors: list[str] = []
        self.dedupe_refs = 0
        self.max_commits = max_commits

    def run(self) -> None:
        conn = connect(self.path)
        conn.execute("PRAGMA synchronous=NORMAL")
        conn.execute("PRAGMA cache_size=-65536")
        seq = 0
        while (
            not self.stop.is_set()
            and time.monotonic() < self.deadline
            and (not self.max_commits or self.commits < self.max_commits)
        ):
            started = time.perf_counter()
            try:
                conn.execute("BEGIN IMMEDIATE")
                row = conn.execute(
                    "UPDATE high_water SET serial=serial+1, next_pack_id=next_pack_id+1, "
                    "pack_bytes=pack_bytes+?, logical=logical+? "
                    "RETURNING serial, next_pack_id-1",
                    (MIB, MIB),
                ).fetchone()
                serial, pack_id = row
                ids = [oid(pack_id, index) for index in range(OBJECTS_PER_PACK)]
                conn.execute("INSERT INTO pack VALUES(?, zeroblob(?))", (pack_id, MIB))
                conn.executemany(
                    "INSERT INTO object_location VALUES(?, ?)", [(object_id, pack_id) for object_id in ids]
                )
                conn.executemany(
                    "INSERT INTO object_ref VALUES(?, ?)",
                    [(ids[0], ids[index]) for index in range(1, OBJECTS_PER_PACK)],
                )
                root_id = new_root_id(serial, self.salt)
                conn.execute("INSERT INTO root VALUES(?, 'commit', ?)", (root_id, serial))
                conn.executemany("INSERT INTO root_object VALUES(?, ?)", [(root_id, object_id) for object_id in ids])
                conn.execute("INSERT INTO kept_node VALUES('commit', ?, ?)", (root_id, serial))
                seq += 1
                if self.dedupe_every and self.dead_pool and seq % self.dedupe_every == 0:
                    object_id = self.dead_pool[(seq // self.dedupe_every) % len(self.dead_pool)]
                    # content dedupe: the same object the mark saw as dead is
                    # referenced again by a fresh commit root.
                    conn.execute("INSERT OR IGNORE INTO root_object VALUES(?, ?)", (root_id, object_id))
                    self.dedupe_refs += 1
                conn.execute("COMMIT")
                elapsed = time.perf_counter() - started
                self.latencies.append(elapsed)
                self.bytes += MIB
                self.commits += 1
            except sqlite3.Error as error:  # a refusal is a finding, not a crash
                self.errors.append(str(error))
                try:
                    conn.execute("ROLLBACK")
                except sqlite3.Error:
                    pass
                time.sleep(0.001)
        conn.close()


def dead_object_pool(path: str, dead_packs: list[int], per_pack: int = 2) -> list[bytes]:
    return [oid(pack_id, index) for pack_id in dead_packs for index in range(per_pack)]


def start_writer(path: str, **kwargs) -> Writer:
    writer = Writer(path, **kwargs)
    writer.start()
    return writer


def summarise_latencies(latencies: list[float]) -> dict:
    if not latencies:
        return {"p50_ms": 0.0, "p99_ms": 0.0, "max_ms": 0.0}
    ordered = sorted(latencies)
    return {
        "p50_ms": ordered[len(ordered) // 2] * 1000,
        "p99_ms": ordered[min(len(ordered) - 1, int(len(ordered) * 0.99))] * 1000,
        "max_ms": ordered[-1] * 1000,
    }
