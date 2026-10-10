#!/usr/bin/env python3
"""Count-driven diagnostic of the full comparator's own bookkeeping.

Touches no product, mount or Store. It runs the registered comparator's
expectation preparation unchanged on the sealed inventory, then replays the
retained observed rows of receipt 053 through the same per-entry SELECT/UPDATE
and JSONL emission, without any filesystem observation or hashing of product
bytes. Hash throughput is measured separately over an in-memory buffer.
Exploratory and ineligible: it attributes comparator-side cost only.
"""
import hashlib
import importlib.util
import json
import sqlite3
import sys
import tempfile
import time
from pathlib import Path

oracle_path, inventory, inventory_sha, observed, out = sys.argv[1:6]
spec = importlib.util.spec_from_file_location("full_oracle", oracle_path)
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)

result = dict(schema="r8b-comparator-bookkeeping-diagnostic-v1", eligible=False,
              oracle_sha256=oracle.sha256(oracle_path), phases={})


def timed(name, call):
    start = time.monotonic_ns()
    value = call()
    result["phases"][name] = time.monotonic_ns() - start
    return value


with tempfile.TemporaryDirectory(dir=sys.argv[6] if len(sys.argv) > 6 else None) as scratch:
    scratch = Path(scratch)
    require = oracle.require
    require(timed("inventory_sha256_1", lambda: oracle.sha256(inventory)) == inventory_sha, "inventory differs")
    database = sqlite3.connect(scratch / "expected.sqlite", timeout=0, isolation_level=None)
    totals = timed("load_expected", lambda: oracle.load_expected(database, inventory, 501, 20))
    timed("write_expected", lambda: oracle.write_expected(database, scratch / "expected.jsonl"))
    timed("expected_jsonl_sha256", lambda: oracle.sha256(scratch / "expected.jsonl"))
    timed("inventory_sha256_2", lambda: oracle.sha256(inventory))
    result["expected_totals"] = totals
    result["preparation_ns"] = sum(result["phases"].values())

    rows = []
    with open(observed, "rb") as stream:
        next(stream)
        for line in stream:
            try:
                rows.append(json.loads(line))
            except ValueError:
                result["partial_tail_bytes"] = len(line)
    result["replayed_rows"] = len(rows)

    def replay():
        selects = updates = 0
        with open(scratch / "observed.jsonl", "x", encoding="utf-8") as stream:
            for row in rows:
                relative, value = row["path"], row["observation"]
                expected = database.execute(
                    "SELECT kind,mode,uid,gid,mtime,size,checksum,target,nlink,seen "
                    "FROM expected WHERE path=?", (relative,)).fetchone()
                selects += 1
                require(expected is not None and expected[9] == 0, "replay row state: " + relative)
                database.execute(
                    "UPDATE expected SET seen=1,observed_device=?,observed_inode=?,observed_nlink=? "
                    "WHERE path=?", (str(value["device"]), str(value["inode"]), value["nlink"], relative))
                updates += 1
                oracle.emit(stream, dict(path=relative, match=True, observation=value))
        return selects, updates

    result["statements"] = timed("per_entry_sql_and_emit_autocommit", replay)
    timed("final_alias_queries", lambda: [database.execute(q).fetchone() for q in (
        "SELECT path FROM expected WHERE seen=0 LIMIT 1",
        "SELECT source_device,source_inode FROM expected WHERE kind='file' GROUP BY source_device,source_inode "
        "HAVING COUNT(DISTINCT observed_device||':'||observed_inode)!=1 LIMIT 1",
        "SELECT observed_device,observed_inode FROM expected WHERE kind='file' GROUP BY observed_device,observed_inode "
        "HAVING COUNT(DISTINCT source_device||':'||source_inode)!=1 LIMIT 1")])
    database.close()

block = bytes(oracle.WINDOW)
hashed = sum(row["observation"]["size"] for row in rows if row["observation"]["kind"] == "file")


def hash_only():
    digest, left = hashlib.sha256(), hashed
    while left > 0:
        digest.update(block[:min(left, oracle.WINDOW)])
        left -= oracle.WINDOW
    return digest.hexdigest()


timed("sha256_same_byte_count_from_memory", hash_only)
result["hashed_bytes_equivalent"] = hashed
replay_ns = result["phases"]["per_entry_sql_and_emit_autocommit"]
result["per_entry_bookkeeping_ns"] = replay_ns // max(1, len(rows))
result["projected_full_bookkeeping_ns"] = result["per_entry_bookkeeping_ns"] * totals["entries"]
with open(out, "x") as stream:
    json.dump(result, stream, sort_keys=True, indent=2)
    stream.write("\n")
print(json.dumps(result, sort_keys=True, indent=1))
