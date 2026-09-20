#!/usr/bin/env python3
"""SYNTHETIC sizing of one write pattern - not a product measurement.

The Store this round's saves wrote shows the pattern: `object_packs` is one BLOB
per pack, and every group placement rewrites that whole BLOB
(`sqlite/write.rs::append_pack`: `UPDATE object_packs SET data = ?2`). The retained
stride10 Store holds 44,309 ordinary-lane groups in 255 packs (median 245 groups
per pack) and `save.pack_appends` counts 45,791 of those rewrites for 45.3 MB of
final pack bytes.

This script asks SQLite what that pattern costs **on its own**: it replays the same
sequence of BLOB updates - the same pack ids, the same group counts, the same
intermediate sizes - against a scratch copy of the retained Store, under the same
pragma profile the product declares (`journal_mode = MEMORY`, `synchronous = OFF`,
`temp_store = MEMORY`).

**What it is not.** It is not the product's cost: the real save interleaves five
open lanes, compression, reads and transaction commits between these updates, so
this is an upper bound on the append pattern's share of `storage.accept_loop`, not
an attribution of it. It sizes a hypothesis; it does not replace the treatment's
own measurement.
"""
import os
import shutil
import sqlite3
import sys
import time
from pathlib import Path

RETAINED = Path(os.environ.get(
    "H190_RETAINED",
    "/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-qual/docs/roadmap/0.1/"
    "0.1.7/evidence/stage-6-history-190-corpus-phase-20260920T054215Z"))
CAMPAIGN = Path(__file__).resolve().parent
SCRATCH = CAMPAIGN / "runs" / "synthetic-append" / "copy.sqlite"


def main() -> int:
    source = RETAINED / "runs" / "diagnostic-history-stride10" / "raw" / "sample.sqlite"
    if not source.exists():
        print(f"custody gap: {source} is not on disk; nothing to size")
        return 2
    SCRATCH.parent.mkdir(parents=True, exist_ok=True)
    if SCRATCH.exists():
        print(f"REFUSED: {SCRATCH} exists; a run never overwrites evidence")
        return 2
    shutil.copy2(source, SCRATCH)
    connection = sqlite3.connect(SCRATCH)
    for pragma in ("PRAGMA journal_mode = MEMORY", "PRAGMA synchronous = OFF",
                   "PRAGMA temp_store = MEMORY"):
        connection.execute(pragma)
    packs = connection.execute(
        "SELECT p.pack_id, LENGTH(p.data), COUNT(DISTINCT g.group_number) "
        "FROM object_packs p JOIN ("
        "  SELECT pack_id, group_number FROM objects"
        "  UNION ALL SELECT pack_id, group_number FROM metadata_value_groups"
        ") g ON g.pack_id = p.pack_id GROUP BY p.pack_id ORDER BY p.pack_id").fetchall()
    appends = sum(count - 1 for _, _, count in packs)
    bytes_written = sum(size * (count - 1) / 2 for _, size, count in packs)
    # The two halves are timed apart, because they are different costs and only one
    # of them is SQLite's: the in-memory rebuild of the pack the placement returns
    # (`select_many` -> `SelectedWrite::bytes`), and the BLOB update that stores it.
    # The rebuild here is a Python bytes concatenation, so it is an *upper* bound on
    # the Rust memcpy it stands for; the update is the same statement the product
    # issues, against the same file, under the same pragmas.
    group_of = {pack_id: size / count for pack_id, size, count in packs}
    rebuilt = {}
    started = time.monotonic_ns()
    for pack_id, size, count in packs:
        data = b""
        for _ in range(count):
            data = data + b"\0" * max(1, int(group_of[pack_id]))
        rebuilt[pack_id] = data
    rebuild_wall = time.monotonic_ns() - started
    # The real save does not commit once at the end. `write_pack` charges the
    # **whole rewritten pack** to the transaction's byte budget, so a 4 MiB budget
    # is consumed every ~32 group placements: the measured cadence is 1,149 commits
    # for 45,791 appends. That charge is a second consequence of the same pattern,
    # so the replay models it: one COMMIT every `appends // commits` updates.
    cadence = max(1, appends // 1149)
    started = time.monotonic_ns()
    updates = 0
    commits = 0
    for pack_id, size, count in packs:
        data = rebuilt[pack_id]
        step = max(1, len(data) // max(1, count))
        for index in range(count):
            connection.execute("UPDATE object_packs SET data = ?2 WHERE pack_id = ?1",
                               (pack_id, data[: step * (index + 1)]))
            updates += 1
            if updates % cadence == 0:
                connection.commit()
                commits += 1
    connection.commit()
    commits += 1
    wall = time.monotonic_ns() - started
    connection.close()
    SCRATCH.unlink()
    print("SYNTHETIC append-pattern sizing (a copy of the retained stride10 Store)")
    print(f"  packs {len(packs):,}, appends {appends:,}, "
          f"BLOB bytes rewritten (half-final approximation) {bytes_written / 1e9:.2f} GB")
    print(f"  replay: {updates:,} updates in {wall / 1e9:.3f} s "
          f"= {wall / max(1, updates) / 1e3:.1f} us per append")
    print(f"  of which the in-memory pack rebuild (Python concat, an upper bound on the")
    print(f"  Rust copy it stands for): {rebuild_wall / 1e9:.3f} s")
    print(f"  commits modelled: {commits:,} (the measured 1,149-commit cadence, forced by")
    print(f"  charging each append the whole pack against the 4 MiB transaction budget)")
    print(f"  => the same pattern inside one save's accept loop: "
          f"{wall / 1e9:.3f} s of SQLite time for stride10")
    print("  NOT the product's cost: the real save interleaves five lanes, compression,")
    print("  reads and commits between these updates. It bounds the pattern's share.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
