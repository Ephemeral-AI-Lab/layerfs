# Proposed G5-B0 metadata-only shadow contract

Status: **prepared, not authorized or implemented**. The terminal G5-B disposition is `RETAIN_K64_F64`; this contract records the smallest future experiment if a later stage explicitly authorizes it.

## One variable and three arms

Change only persistent grouping over the exact accepted ordered `(raw_length, canonical ObjectId)` reference vector:

1. current K64/F64 control;
2. exact pinned-Xet 3–9 rule as a reference/negative arm, including unary tails;
3. LayerFS CD32–64: role-separated, domain-separated marker after 32, forced at 64, same rule at every level, fixed terminal/root collapse rules.

No payload rechunking, SQLite writes, canonical/profile migration, publication, concurrency, worker pool, or shared-path state is allowed. Run a standalone benchmark-private child sequentially after exact reference extraction.

## Required rows

- fresh build; same-count early/middle/late;
- +1 and -1 early/middle/late; append/truncate;
- insert then delete; branch then revert; two histories reaching the same final sequence;
- no-cut, every-cut, duplicate, gap-2, gap-8, singleton-tail N=4/N=10, parent-level grinding, zero/max-chunk, and forced-cut phase shifts.

For every final sequence, fresh construction and every incremental history must produce identical boundaries, topology, node IDs, and root.

## Direct counters

Record reference sequence digest; objects/bytes by level; path objects/bytes; nodes visited/encoded/new/reused; bytes encoded/new/reused; resynchronization references at each level; natural/forced cuts; frontier and summary nodes/bytes; full-suffix fallback; root equality; SQL work (zero for metadata-only arms); child RSS; logical Q; maximum buffer; and complete wall.

## Corrected gates

- 100-MiB early +1 operation counter `<=19,637` and file-only bytes `<=19,609`.
- 100-MiB middle +1 operation counter `<=10,076` and file-only bytes `<=10,047`.
- Live file mapping `<=205,857`; full-create counter `<=205,982`.
- Worst one-leaf authenticated path `<=5,302`.
- Same-count file rewrite `<=5,050`, or operation counter `<=5,334`, plus at most one separately sized candidate node.
- Node/summary/frontier memory hard bounded; individual buffer `<=1 MiB`; whole-child RSS `<=20,971,520`; terminal Q exactly zero.
- Ordinary/iid and hard/adversarial dispositions are separate. An expected-case pass cannot erase an adversarial failure.

## Promotion boundary

Even a passing shadow does not select Canonical-v3. Promotion requires explicit profile identity, golden vectors, mixed-profile rejection or dual-reader rules, migration/downgrade/rollback authority, crash-safe GC and root pinning, and a new acceptance campaign. None is part of G5-1.
