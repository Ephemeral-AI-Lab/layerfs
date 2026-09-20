#!/usr/bin/env python3
"""Is the depth term a cache *scope* term? Read it from counters that already exist.

Nothing here is a new measurement and nothing is instrumented. Three retained
artifacts are read as they lie on disk:

* the retained campaign's per-state counter traces
  (`stage-6-history-190-corpus-phase-20260920T054215Z/runs/*/raw/trace.jsonl`),
  which publish the pooled read path's own counters per state;
* the retained campaign's saved Stores (`.../raw/sample.sqlite`), which give the
  pack inventory a state's read work could possibly touch;
* the retained timing trees, for the per-state elapsed the counters are compared
  against.

The claim under test is the reading the scaling round left open: the read path
makes *fewer but far larger* authenticated reads as the chain deepens. Two
mechanisms produce that signature and they are separable with these counters:

* **scope**: the pooled metadata reader is constructed per resolved inode leaf, so
  its pack cache is discarded after every leaf. A pack demanded again by the next
  leaf is copied again, and as the Store matures each copy is larger.
* **structure**: a late-chain state genuinely needs more distinct records, more
  chain steps, or more value groups per leaf.

`physical_group_cache_hits` settles it in one direction: the decoded-group cache
*is* operation-scoped already (`ReadSession::groups`), so a hit is proof that the
same `(pack, group)` was demanded again inside one operation while the pooled pack
cache that served it had already been thrown away.

Custody: the Stores are excluded from Git (`runs/*/raw/sample.sqlite`), so they are
read by absolute path from the retained worktree and are named in the output.
"""
import json
import os
import sqlite3
import sys
from pathlib import Path

# The retained campaign, as it lies in the worktree that produced it. The traces
# are committed in this tree too (under the same relative path), so only the saved
# Stores need the retained worktree; `H190_RETAINED` overrides the path, and a
# missing Store is reported as a custody gap rather than substituted.
RETAINED = Path(os.environ.get(
    "H190_RETAINED",
    "/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-qual/docs/roadmap/0.1/"
    "0.1.7/evidence/stage-6-history-190-corpus-phase-20260920T054215Z"))
LOCAL = (Path(__file__).resolve().parent.parent
         / "stage-6-history-190-corpus-phase-20260920T054215Z")
CASES = ("history-stride10", "history-stride3")
BANDS = ((4_000_000, 7_000_000), (7_000_000, 10_000_000),
         (10_000_000, 14_000_000), (14_000_000, 22_000_000))
P = "filesystem.provider.pooled."


def campaign_root():
    """The retained campaign directory: the retained worktree, else this tree."""
    return RETAINED if (RETAINED / "runs").exists() else LOCAL


def load(case):
    raw = campaign_root() / "runs" / f"diagnostic-{case}" / "raw"
    tree = json.loads((raw / "timing.json").read_text())
    elapsed = {int(c["name"].rsplit(".", 1)[-1]): int(c["elapsed_ns"])
               for c in tree["children"]}
    values = {}
    for line in (raw / "trace.jsonl").read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        record = json.loads(line)
        key = str(record.get("key", ""))
        if not key.startswith("history.state.") or not isinstance(record.get("value"), int):
            continue
        parts = key.split(".")
        if parts[2].isdigit():
            values.setdefault(int(parts[2]), {})[".".join(parts[3:])] = int(record["value"])
    return elapsed, values


def inventory(case):
    """Pack inventory of the run's own saved Store: the ceiling on distinct packs."""
    store = campaign_root() / "runs" / f"diagnostic-{case}" / "raw" / "sample.sqlite"
    if not store.exists():
        return None
    uri = f"file:{store}?mode=ro"
    with sqlite3.connect(uri, uri=True) as connection:
        packs, pack_bytes, smallest, largest = connection.execute(
            "SELECT COUNT(*), COALESCE(SUM(LENGTH(data)), 0), COALESCE(MIN(LENGTH(data)), 0),"
            " COALESCE(MAX(LENGTH(data)), 0) FROM object_packs").fetchone()
        groups, group_values = connection.execute(
            "SELECT COUNT(*), COALESCE(SUM(count), 0) FROM metadata_value_groups").fetchone()
        objects = connection.execute("SELECT COUNT(*) FROM objects").fetchone()[0]
    return {"path": str(store), "packs": packs, "pack_bytes": pack_bytes,
            "smallest": smallest, "largest": largest, "groups": groups,
            "group_values": group_values, "objects": objects}


def row(name, ordinal, values, elapsed):
    get = lambda suffix, default=0: values[ordinal].get(P + suffix, default)  # noqa: E731
    return {
        "ordinal": ordinal,
        "changed_bytes": values[ordinal].get("changed_bytes", 0),
        "elapsed_ns": elapsed[ordinal],
        "leaves": get("leaf_requests"),
        "edges": get("chain_edges"),
        "records": get("physical_record_calls"),
        "decodes": get("physical_group_decodes"),
        "hits": get("physical_group_cache_hits"),
        "values": get("value_group_decodes"),
        "fetches": get("pack_fetches"),
        "bytes": get("pack_bytes"),
    }


def main() -> int:
    out = []
    write = out.append
    write("# The depth term: scope, structure, or neither")
    write("")
    write("Retained artifacts only. No new sample, no instrumentation, no product change.")
    write("")
    for case in CASES:
        elapsed, values = load(case)
        store = inventory(case)
        ordinals = sorted(elapsed)
        rows = [row(case, o, values, elapsed) for o in ordinals]
        total = {key: sum(r[key] for r in rows)
                 for key in ("leaves", "edges", "records", "decodes", "hits", "values",
                             "fetches", "bytes")}
        write(f"## {case}: {len(ordinals)} states")
        write("")
        if store is None:
            write("  custody gap: the run's saved Store is not on disk; the pack ceiling is")
            write("  reported as unavailable rather than substituted.")
        else:
            write(f"  saved Store: {store['path']}")
            write(f"    packs {store['packs']:,}  pack bytes {store['pack_bytes']:,}"
                  f"  ({store['smallest']:,}..{store['largest']:,} per pack)")
            write(f"    value groups {store['groups']:,} holding {store['group_values']:,}"
                  f" values; objects {store['objects']:,}")
        write("")
        write("  whole-row totals over the measured states")
        write(f"    pooled leaf requests        {total['leaves']:>12,}")
        write(f"    physical record calls       {total['records']:>12,}")
        write(f"    chain edges                 {total['edges']:>12,}")
        write(f"    decoded-group cache hits    {total['hits']:>12,}")
        write(f"    decoded-group decodes       {total['decodes']:>12,}")
        write(f"    decoded-group hit rate      {total['hits'] / max(1, total['hits'] + total['decodes']):>12.3f}")
        write(f"    value-group decodes         {total['values']:>12,}")
        write(f"    pack fetches                {total['fetches']:>12,}")
        write(f"    pack bytes copied           {total['bytes']:>12,}"
              f"  ({total['bytes'] / 1e9:.2f} GB)")
        if store and store["packs"]:
            write(f"    copies of the whole pack space  {total['bytes'] / store['pack_bytes']:>8.1f}x")
            write(f"    fetches per stored pack         {total['fetches'] / store['packs']:>8.1f}")
            write(f"    repeat fetches (fetches - packs, >=)  "
                  f"{max(0, total['fetches'] - store['packs']):>8,}"
                  f"  ({max(0, total['fetches'] - store['packs']) / total['fetches']:.1%} of fetches)")
        write("")
        write("  per state, and the matched-volume early/late split")
        write(f"  {'st':>3} {'chg MB':>7} {'leaf':>6} {'rec/lf':>7} {'edg/rec':>8} {'val/lf':>7}"
              f" {'fetch/lf':>9} {'KiB/f':>7} {'MB copied':>10} {'MB/chgMB':>9}")
        half = max(ordinals) // 2
        for r in rows:
            leaves = max(1, r["leaves"])
            write(f"  {r['ordinal']:>3} {r['changed_bytes'] / 1e6:>7.2f} {r['leaves']:>6}"
                  f" {r['records'] / leaves:>7.2f}"
                  f" {r['edges'] / max(1, r['records']):>8.2f}"
                  f" {r['values'] / leaves:>7.2f}"
                  f" {r['fetches'] / leaves:>9.2f}"
                  f" {r['bytes'] / max(1, r['fetches']) / 1024:>7.0f}"
                  f" {r['bytes'] / 1e6:>10.1f}"
                  f" {r['bytes'] / max(1, r['changed_bytes']):>9.2f}")
        write("")
        write(f"  {'band MB':>10} {'n e/l':>7} {'fetch/lf e':>11} {'l':>8} {'x':>6}"
              f" {'KiB/f e':>8} {'l':>7} {'x':>6} {'val/lf e':>9} {'l':>8} {'x':>6}"
              f" {'rec/lf e':>9} {'l':>8} {'x':>6}")
        for lo, hi in BANDS:
            early = [r for r in rows if lo <= r["changed_bytes"] < hi and r["ordinal"] <= half
                     and r["leaves"]]
            late = [r for r in rows if lo <= r["changed_bytes"] < hi and r["ordinal"] > half
                    and r["leaves"]]
            if not early or not late:
                continue

            def mean(group, numerator, denominator):
                return (sum(r[numerator] for r in group) / len(group)
                        / max(1e-9, sum(r[denominator] for r in group) / len(group)))

            cells = []
            for numerator, denominator in (("fetches", "leaves"), ("bytes", "fetches"),
                                           ("values", "leaves"), ("records", "leaves")):
                e, l = mean(early, numerator, denominator), mean(late, numerator, denominator)
                if denominator == "fetches":
                    e, l = e / 1024, l / 1024
                cells.append((e, l, l / e if e else float("nan")))
            write(f"  {f'{lo/1e6:.0f}-{hi/1e6:.0f}':>10} {f'{len(early)}/{len(late)}':>7}"
                  + "".join(f" {e:>11.2f} {l:>8.2f} {x:>6.2f}" for e, l, x in cells))
        write("")
    write("## Reading")
    write("")
    write("**Source fact.** The pooled reader is constructed once per resolved inode leaf")
    write("(`encoding/delta/read.rs`, `Resolver::resolve_charged`: `PoolReader::new()`), so")
    write("its pack cache - bounded at `DEPENDENCY_PACK_CACHE_BYTES` - never survives a leaf.")
    write("The decoded-group cache beside it is **operation**-scoped (`cas/read.rs`,")
    write("`ReadSession::groups`), and the two caches have the same job for the same access")
    write("stream: one holds decoded group bodies, the other the pack bytes those bodies")
    write("were cut from.")
    write("")
    write("**Measured.** Two independent readings agree that the demands repeat:")
    write("")
    write("1. The decoded-group cache is consulted *after* the pack body is acquired, so")
    write("   every hit is a demand the operation had already served for another record -")
    write("   at 0.951 (stride10) and 0.916 (stride3) of all record calls. Each hit still")
    write("   paid for the pack BLOB unless the same leaf had already fetched that pack.")
    write("2. The number of distinct packs a state can fetch is at most the number of packs")
    write("   its Store holds, so `fetches - packs` is a **floor** on fetches that re-read a")
    write("   pack already read inside the same state: 79,529 of 79,784 (99.7%) on stride10")
    write("   and 426,947 of 427,384 (99.9%) on stride3. The whole pack space is copied")
    write("   176x (stride10) and 444x (stride3) over within one run.")
    write("")
    write("**Not a deeper-chain algorithm.** Chain edges per record call rise once, from 0")
    write("(the first states still write FULL records) to 0.28-0.39, and then stay there:")
    write("state 17 of stride10 reads 4.46 records per leaf against state 2's 2.00 - a 2.2x")
    write("structural rise - while bytes copied per leaf rise 54x (20 KiB to 1,085 KiB),")
    write("because fetches per leaf rise 3.8x and the bytes each fetch copies rise 14.2x")
    write("(10 KiB to 142 KiB) as packs mature toward `PACK_LIMIT`.")
    write("")
    write("**What this does and does not establish.** It establishes that the term is")
    write("repeated acquisition of whole committed packs, discarded at the per-leaf")
    write("boundary, and not more algorithm per byte. It does **not** establish how much of")
    write("that repetition an operation-scoped cache can hold at the *existing*")
    write("`DEPENDENCY_PACK_CACHE_BYTES` bound: the repeats are many but their reuse")
    write("distance is not published by any counter. That is the treatment's question, and")
    write("the treatment is the experiment that separates the two readings: if the term is")
    write("scope, the same bound over a wider lifetime removes it; if the repetition is")
    write("wider than any bounded window, the counters move little and the reading is")
    write("refuted rather than confirmed.")
    write("")
    print("\n".join(out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
