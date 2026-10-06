# Acquisition space reclamation and scaling, 2026-10-06

> **Status:** Owner-selected implementation and prospective qualification.
> No new latency/allocation PASS is established by this plan.

The owner asks to fix retained allocation and improve the larger-case time
slope. Restored Monolithic is the starting product. Canonical objects, acquisition
ownership, four Init constructors, durability and historical failures remain.

Read-only hash-verified inspection finds36,360,192 B of free pages in final
Durable100000 and36,392,960 B in Disposable100000. Both cluster-one controls
have zero free pages; current acquisition tables are empty. Free pages explain
99.876238% of Durable's36,405,248 B growth. Remaining non-free-page overhead
and pointer-map allocation are counted; zero freelist does not imply gate PASS.

The before-change count diagnostic at restored source7878bbbb4 records1614
jobs calls/6456 statements for100000 native files. Every path was budgeted as
4096 B. Entry insertion uses24,317,374 VM steps; deletion11,777,603. Original
removal plans materialize a bounded key list/Bloom filter and seek each key.
This diagnostic is instrumented/uncontrolled, not eligible performance evidence.
Its initial verifier lacked the cursor-key environment and failed `NotPresent`;
retain that failure and any corrected independent proof separately.

## Ordinary implementation and bounds

New acquisition Stores select incremental auto-vacuum before schema/WAL creation.
Existing schemas1–6 and mode0 Stores open without implicit conversion. Canonical
objects, pack layout, Durable WAL/FULL/fullfsync and Disposable MEMORY/OFF remain.
Mode2's additional pointer-map pages are included in physical allocation.

Each nonempty discard and release reclaims at most512 pages inside its existing
short transaction. `Handles::reclaim_space` continues residual debt through
acknowledged bounded jobs, releasing the writer between jobs. No whole-Store
VACUUM, extra database, failed-job replay or guessed uncertain cleanup occurs.
Mode0 maintenance is refused without migration. The public benchmark pays any
residual jobs before final checkpoint/close; automatic maintenance is inside Init.

Jobs/directories inspect at most512 indexed scalar path lengths, then fetch
the prefix fitting the existing256-KiB column budget in the same snapshot.
The first row is allowed at tiny byte budgets. Long paths retain bounds;
short paths no longer force62-row windows. No path bytes are copied for sizing.

Removal walks at most4096 indexed keys to retain one inclusive endpoint, then
deletes that indexed prefix and obtains exact removed-byte charges via RETURNING.
It retains at most one255-byte name and removes the explicit IN-subquery key
list/Bloom filter. Full EXPLAIN still shows SQLite-owned deletion-key and RETURNING
buffers bounded by the job. It keeps
owner fencing, entry-first removal, rollback and uncertain custody. Endpoint selection is O(log N+K); deletion and required index work remain
O(K log N). The endpoint OFFSET is at most4095 within each removed prefix,
never an increasing whole-enumeration offset. Whole enumeration/removal is output-sized through bounded jobs;
reclamation pays bounded work per removed page, without total input/time caps.

## Frozen qualification

New cases, in order: Durable100/1000/10000/100000 then the same Disposable tiers,
named `phase7-sqlite-init-{n}-space-scaling-v1` and
`phase7-sqlite-disposable-init-{n}-space-scaling-v1`.

One fresh cold sample per case at one clean source. Reuse qualified same-profile
cluster-one public Project controls197d2fb7d, unchanged workload manifests/seed1,
prepared sources and native cold helper. Preserve all old failures and unrun rows.
Complete-command30s and independent-proof19s caps, workers, cache and gates stay:
`10*current_ns <= 11*control_ns`; final allocated DB/WAL/SHM <= matching control.
Every physical reclamation job is inside the complete product clock.

Cover actual engine plans and per-unit runtime profiles for sizing/removal.
Public proofs cover window byte/order bounds, exact charges, bounded page jobs,
another live operation, reopen, old-mode compatibility and ordinary immutable
root/content reads. Read-only page accounting confirms remaining free-page debt.
Instrumented diagnostics never replace the cold gate sample.

The tiers change both file count and logical bytes (5/20/300/500 MB). Report raw
deltas and per-entry SQL work; those tiers alone do not establish a pure scaling
exponent or qualify an unmeasured namespace. New source measurements decide
speed/storage acceptance independently; S7/S8/S9 runtime closure remains open.
