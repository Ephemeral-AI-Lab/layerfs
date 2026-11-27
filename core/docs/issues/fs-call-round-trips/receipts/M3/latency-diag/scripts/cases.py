#!/usr/bin/env python3
"""Split decoded exchanges into probe cases using the observer's snapshot
pairs and print per-exchange timing for the selected cases.

usage: cases.py <json> <log> <netem_ms> [case ...]
"""
import json
import re
import sys

js, log, netem = sys.argv[1], sys.argv[2], float(sys.argv[3])
want = sys.argv[4:]
d = json.load(open(js))
ex = d["exchanges"]
obs = set(d["observer_ports"])
cases = [m.group(1) for m in re.finditer(r'"case":"([a-z0-9_.]+)","method"', open(log).read())]
cases = [c for c in cases if not c.startswith("setup.")]
# snapshot = exchange on observer whose sql mentions 'coalesce(queryid'
snaps = [e for e in ex if e["conn"] in obs and any("coalesce(queryid" in s for s in e["sql"]) and "E" in e["msgs"]]
# measured window: after the 'dealloc' reply of the before-snapshot to the req of the after-snapshot.
deallocs = [e for e in ex if e["conn"] in obs and any("pg_stat_statements_info" in s for s in e["sql"]) and "E" in e["msgs"]]
# Each measured case: snapshot(before) [coalesce, dealloc], call, snapshot(after) [coalesce, dealloc]
# the first snapshot pair belongs to Init setup (before/after). Find pairs by order.
pairs = list(zip([s for s in snaps], deallocs))
# case k uses pairs[-(2*len(cases)) + 2k] (before) and +1 (after)
n = len(cases)
pairs = pairs[-2 * n:]
SHORT = {
    "SELECT q.*,f.__layerfs_incarnation FROM (": "",
}


def short(sql):
    s = sql.split("|", 1)[-1]
    s = s.replace("SELECT q.*,f.__layerfs_incarnation FROM (", "")
    s = re.sub(r"\s+", " ", s)
    return s[:90]


summary = {}
for k, case in enumerate(cases):
    before, after = pairs[2 * k], pairs[2 * k + 1]
    t0 = before[1]["reply_last"]
    t1 = after[0]["req_first"]
    mine = [e for e in ex if e["conn"] not in obs and t0 < e["req_first"] < t1]
    tot_srv = 0.0
    tot_gap = 0.0
    rows = []
    prev_end = t0
    for e in mine:
        rf = e["reply_first"] or e["req_last"]
        rl = e["reply_last"] or e["req_last"]
        srv = (rf - e["req_last"]) * 1e3 - netem
        xfer = (rl - rf) * 1e3
        gap = (e["req_first"] - prev_end) * 1e3
        prev_end = max(prev_end, rl)
        rows.append((e, srv, xfer, gap))
        tot_srv += srv
    wall = (mine[-1]["reply_last"] - mine[0]["req_first"]) * 1e3 if mine else 0
    syncs = sum(e["msgs"].count("S") + e["msgs"].count("Q") for e in mine)
    summary[case] = dict(exchanges=len(mine), syncs=syncs, wall_ms=round(wall, 2),
                         server_ms=round(tot_srv, 2),
                         req_bytes=sum(e["req_bytes"] for e in mine),
                         reply_bytes=sum(e["reply_bytes"] for e in mine),
                         gaps_ms=round(sum(r[3] for r in rows[1:]), 2),
                         xfer_ms=round(sum(r[2] for r in rows), 2))
    if want and case not in want:
        continue
    print(f"## {case}: exchanges {len(mine)} syncs {syncs} wire-wall {wall:.2f} ms "
          f"server-sum(excl netem) {tot_srv:.2f} ms gaps-sum {summary[case]['gaps_ms']} ms "
          f"reply-xfer-sum {summary[case]['xfer_ms']} ms reply bytes {summary[case]['reply_bytes']}")
    print(" #  conn  start_ms  gap_ms  srv_ms  xfer_ms  reqB  replyB rows  pkts(c/s) reqspan replyspan maxgap msgs  sql")
    for i, (e, srv, xfer, gap) in enumerate(rows):
        print(f"{i:2d} {e['conn']} {(e['req_first']-mine[0]['req_first'])*1e3:8.2f} {gap:7.2f} {srv:7.2f} {xfer:7.2f} "
              f"{e['req_bytes']:5d} {e['reply_bytes']:7d} {e['rows']:4d} {e['req_pkts']:3d}/{e['reply_pkts']:<3d} "
              f"{e['req_span_ms']:6.2f} {e['reply_span_ms']:7.2f} {e['reply_max_gap_ms']:6.2f} "
              f"{e['msgs'][:14]:14s} {' ; '.join(short(s) for s in e['sql'] if s.startswith(('B','Q')))[:160]}")
print(json.dumps(summary))
