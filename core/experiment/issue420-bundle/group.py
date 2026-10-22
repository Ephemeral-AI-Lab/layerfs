"""#420 bundle study: assign every small row of the range-60 fixture to the
Commit whose Save wrote it (exact where the fixture says, proportional where
it does not) and report the mapping quality.

Usage: python3 -I group.py FIXTURE OUT.pickle

Mapping (documented approximation):
  pack            -> Save by its reserved 4096-id block ((pack_id-1)//4096);
                     blocks are aligned in order to Commits ordered by
                     (created_ms, commit_id) where the block's summed pack
                     length equals the Commit's new_stored (exact match);
                     an unmatched block (a Save that published no Commit
                     row) rides with the next matched Commit. Blocks before
                     the first match are project Init (bundle 0).
  object_location -> its pack's Commit (exact).
  file_digest     -> the Commit of its root object (exact for the root).
  content_signature -> the Commit of its object (only the 8,192 surviving
                     ring rows exist; overwritten history is reported).
  commit_lineage  -> its own Commit (exact).
  inode_value     -> ordinals are allocated in Save order, so the ordinal
                     stream is cut into consecutive pieces, one per Commit,
                     sized in proportion to the Commit's new_metadata bytes
                     (largest remainder). Proportional, not exact.
  binding_hint    -> children are serials allocated in time order: rows
                     sorted by (child, parent, name) are cut the same way.
"""
import pickle
import sqlite3
import sys

fixture, out = sys.argv[1], sys.argv[2]
db = sqlite3.connect(f"file:{fixture}?mode=ro&immutable=1", uri=True)

commits = db.execute(
    'SELECT commit_id, created_ms, new_stored, new_metadata FROM "commit" ORDER BY created_ms, commit_id'
).fetchall()
blocks = db.execute(
    "SELECT (pack_id-1)/4096 b, sum(length), group_concat(pack_id) FROM pack GROUP BY b ORDER BY b"
).fetchall()

# Align blocks to commits: Saves reserve blocks in start order, Commits get
# created_ms at publish, so concurrent Commits swap. Each block takes the
# earliest unmatched Commit with equal new_stored whose position is within
# +-2,000 of the block's position (positions drift by the orphan count).
import bisect
SLACK = 2000
by_value = {}
for k, c in enumerate(commits):
    by_value.setdefault(c[2], []).append(k)
pack_commit = {}
save_pos = {}
pending = []
init_blocks = 0
matched = 0
for pos, (b, total, ids) in enumerate(blocks):
    packs = [int(x) for x in ids.split(",")]
    q = by_value.get(total, [])
    j = bisect.bisect_left(q, pos - SLACK)
    hit = q.pop(j) if j < len(q) and q[j] <= pos + SLACK else None
    if hit is not None and b >= 3:
        save_pos[hit] = pos
        for p in pending + packs:
            pack_commit[p] = hit
        pending = []
        matched += 1
    elif b < 3:
        if hit is not None:
            bisect.insort(q, hit)
        init_blocks += 1
        for p in packs:
            pack_commit[p] = -1  # Init bundle
    else:
        pending.extend(packs)
for p in pending:
    pack_commit[p] = len(commits) - 1
i = matched
unmatched_commits = len(commits) - i
print(f"blocks {len(blocks)} matched {matched} init_blocks {init_blocks} orphan_blocks {len(blocks)-matched-init_blocks} unmatched_commits {unmatched_commits}")

n = len(commits)
groups = {k: [[] for _ in range(n + 1)] for k in ["ol", "iv", "bh", "fd", "cs", "cl", "pk"]}
slot = lambda c: c + 1  # bundle 0 = Init

obj_commit = {}
for row in db.execute("SELECT object_id, role, canonical_length, pack_id, group_number, record_number FROM object_location"):
    c = pack_commit[row[3]]
    obj_commit[row[0]] = c
    groups["ol"][slot(c)].append(row)
for row in db.execute("SELECT pack_id, domain, length FROM pack"):
    groups["pk"][slot(pack_commit[row[0]])].append(row)
for row in db.execute("SELECT digest, length, root FROM file_digest"):
    groups["fd"][slot(obj_commit[row[2]])].append(row)
for row in db.execute("SELECT slot, stamp, object_id, signature FROM content_signature"):
    groups["cs"][slot(obj_commit[row[2]])].append(row)
index = {c[0]: k for k, c in enumerate(commits)}
for row in db.execute("SELECT commit_id, run, depth, newest, merged FROM commit_lineage"):
    groups["cl"][slot(index[row[0]])].append(row)


def cut(rows, weights):
    total = sum(weights)
    want = [len(rows) * w / total for w in weights]
    base = [int(x) for x in want]
    rest = len(rows) - sum(base)
    for k in sorted(range(len(want)), key=lambda k: want[k] - base[k], reverse=True)[:rest]:
        base[k] += 1
    out, at = [], 0
    for size in base:
        out.append(rows[at : at + size])
        at += size
    return out


# Save order: matched Commits by their block position; an unmatched Commit
# right after its created-order predecessor.
last = -1.0
spos = []
for k in range(len(commits)):
    last = save_pos.get(k, last + 1e-6)
    spos.append(last)
save_order = sorted(range(len(commits)), key=lambda k: spos[k])
rank = {c: r for r, c in enumerate(save_order)}
# inode_value: ordinals are reserved in Save order, one contiguous range
# per Save. A value naming an object first stored by Save r marks an ordinal
# of Save r or later (later only through dedup against another Save's
# object). The first ordinal marked by each Save is taken as its range
# start; starts that break monotonicity are dropped (longest increasing
# subsequence over Save rank), and every ordinal goes to the Save whose kept
# start precedes it. A Save with no kept start gets no inode values (they
# go to the Save before it).
import bisect as _b
ivs = db.execute("SELECT ordinal, value FROM inode_value ORDER BY ordinal").fetchall()
first = {}
for o, v in ivs:
    refs = [v[9:41]] + ([] if v[0] & 0x80 else [v[41:73]])
    for r in refs:
        c = obj_commit.get(r)
        if c is not None and c >= 0:
            rk = rank[c]
            if rk not in first:
                first[rk] = o
cand = sorted(first.items())  # by rank
tails, tails_i, prev = [], [], [None] * len(cand)
for i, (rk, o) in enumerate(cand):
    j = _b.bisect_left(tails, o)
    if j == len(tails):
        tails.append(o); tails_i.append(i)
    else:
        tails[j] = o; tails_i[j] = i
    prev[i] = tails_i[j - 1] if j else None
keep = []
i = tails_i[-1] if tails_i else None
while i is not None:
    keep.append(cand[i]); i = prev[i]
keep.reverse()
starts = [o for _, o in keep]
print("iv save starts", len(first), "kept", len(keep))
for o, v in ivs:
    j = _b.bisect_right(starts, o) - 1
    groups["iv"][0 if j < 0 else slot(save_order[keep[j][0]])].append((o, v))
# binding_hint: no reference to anchor on; rows sorted by child (serials are
# allocated in time order) are cut in Save order in proportion to each
# Commit's new_metadata bytes (largest remainder). Proportional, not exact.
init_md = db.execute("SELECT init_metadata FROM store_totals").fetchone()[0]
w_all = [max(init_md, 1)] + [max(commits[k][3], 1) for k in save_order]
slots = [0] + [slot(k) for k in save_order]
bhs = db.execute("SELECT child, parent, name FROM binding_hint ORDER BY child, parent, name").fetchall()
for k, part in zip(slots, cut(bhs, w_all)):
    groups["bh"][k] = part
stamp_max = db.execute("SELECT max(stamp) FROM content_signature").fetchone()[0]
meta = {
    "commits": [c[0] for c in commits],
    "save_order": save_order,
    "blocks": len(blocks),
    "matched": matched,
    "init_blocks": init_blocks,
    "unmatched_commits": unmatched_commits,
    "signature_writes_total": stamp_max,
}
with open(out, "wb") as f:
    pickle.dump({"meta": meta, "groups": groups}, f, protocol=5)
for k, g in groups.items():
    sizes = sorted(len(x) for x in g)
    print(k, "rows", sum(sizes), "p50", sizes[len(sizes) // 2], "p99", sizes[int(len(sizes) * 0.99)], "max", sizes[-1])
