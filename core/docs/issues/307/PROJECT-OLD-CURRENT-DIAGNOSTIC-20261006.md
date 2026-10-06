# Older Project versus current acquisition: cause diagnostic

> **Status:** Archived; retained for historical evidence only.
> 2026-10-06. Owner-selected component diagnosis; no new speed admission.

The older and current Project implementations produce the same canonical root.
The current SQLite-backed acquisition executes about2200 additional statements
and393000 additional VM steps for this1000-file input. Indexed access is working;
the remaining cost is repeated entry/identity execution, row-by-row cleanup and
additional transactions. This supports optimizing the SQLite provider while
retaining its ownership and bounded-window contract. It does not support
restoring the custom run-backed importer or declaring the current design wrong.

The earlier statement-lease/root-window correction remains implemented. The
[qualified component checkpoint](ACQUISITION-WINDOW-FIX-RESULTS-20261006.md)
still contains three relative-speed FAILs. These new uncontrolled-cache,
instrumented observations are diagnostic, not replacements for those receipts.

## Selection and controls

The owner explicitly selected an older Project worktree and a side-by-side
diagnostic. The old arm is the last corrected run-backed Project before A3,
`abdb322f42befdbaca9e4fc6d4592f3b39fecbc7`, tree
`49630a9bfd509281cf60e0ac479e56ff4c16f95d`. The current arm is the implemented
execution fix, `4c03b41bf5fcc684df3a707d223ad2b9166396f8`, tree
`6f8770c98ba9648b1af6a2e1e0bfafb5a57d2c8a`.
Primary main is reconciled at38206166e6b5a82946c6df2a407d3d520a63f95e;
its later receipt commit has the same product as the current arm. The issue's
append-only comments and local audits supersede dated earlier body paragraphs;
S7/S8/S9 remain unchecked.

The old worktree is
`/Users/yifanxu/.codex/worktrees/project-init-run-backed-diagnostic/layerfs`;
the current worktree is
`/Users/yifanxu/.codex/worktrees/namespace-init-benchmark/layerfs`.
Both remain attached. Neither has a tracked source change. Each has one
untracked external example whose exact bytes are retained as
[old driver](checks/project-old-current-diagnostic-20261006/drivers/old.rs) and
[current driver](checks/project-old-current-diagnostic-20261006/drivers/current.rs).
They drive the public Init/Timing/diagnostics APIs; no private source include,
test-only product API, third-party edit or substitute implementation is used.

Content, History and product Storage implementations, core Cargo.lock/manifests
and root ARM64 config are unchanged between arms. Storage differs only in its
acquisition port declarations. Persistence differs in the implemented statement
executor correction; that distinction and Project's changed Save order are
part of the comparison, not hidden controls.

Both arms explicitly open Monolithic schema4 with acquisition tables enabled.
The old Project leaves those tables unused. This removes a schema1-versus4
bootstrap difference without changing either Project algorithm. Both Durable
arms read back WAL/FULL/fullfsync; both Disposable arms read back MEMORY/OFF.
SQLite is the same host3.51.0 provider. This comparison is distinct from the
earlier Phase4.5 MEMORY/OFF split-Store reference benchmark.

One run per arm/profile, in registered order: old Durable, current Durable,
old Disposable, current Disposable. All four use
`namespace-1000-compact-v3`,1000 files/10 non-root directories/20 MB, seed1,
closed master manifest SHA256
`e4c484767163117b3c846b6d846cc8157fed052e0b15c70a908ad977f59d881c`.
Prepared source copies are ordinary independent byte copies, not cloned stores
or hard links. Cache is uncontrolled: no cold-content/speed/RSS/rate claim.
`LAYERFS_CONSTRUCTION_WORKERS=1`; Init's supported four constructors remain.
Builds are locked Rust1.85.1 release builds with the required ARM64 AEAD flags
confirmed in each Cargo fingerprint. Builds have60s caps, runs15s caps.
No timeout, retry, replay or background run occurs. Parallel builds encountered
Cargo package-cache locks; neither overlaps any diagnostic run.

All runs complete, timing trees are complete and scratch directories are empty.
All four roots equal
`a71b9151bb269cdc3e71d9424e36669edd7d78ed39f9055bc355cebd4d93ec2c`,
entries1011/root serial1. Root equality is not a new independent full verifier:
that verifier was NOT_RUN for this diagnostic selection. Existing source proofs
remain separately qualified. Full ignored/dependency/cache/output/symlink and
large-root runtime acceptance is not inferred from this compact fixture.

Exact selection, commands, source/build/binary/driver/cache identities, raw
outputs and arithmetic are in the
[ledger](checks/project-old-current-diagnostic-20261006/ledger.json) and
[selection](checks/project-old-current-diagnostic-20261006/selection.json).

## Complete Init SQL work

These are counter differences immediately before and after public Init. Store
bootstrap/checkpoint/close are outside this SQL interval and remain in the
complete product clock. A statement count is an execution count, not a prepare
count. VM steps are actual runtime counters, not EXPLAIN instruction counts.

| Init metric | Old Durable | Current Durable | Old Disposable | Current Disposable |
| --- | ---: | ---: | ---: | ---: |
| Statements | 267 | 2468 | 271 | 2465 |
| VM steps | 202332 | 596223 | 202237 | 595297 |
| Returned rows | 2908 | 11022 | 2905 | 10977 |
| Bound bytes | 20447743 | 20819312 | 20447790 | 20818544 |
| Write commits | 16 | 25 | 16 | 25 |
| Fullscan steps | 0 | 1023 | 0 | 1023 |
| Sorts / autoindex rows / reprepares | 1 /0 /3 | 1 /0 /3 | 1 /0 /3 | 1 /0 /3 |

Current minus old is+2201 statements/+393891 VM in Durable and+2194 statements/
+393060 VM in Disposable. Reused current acquisition-unit profiling attributes
2200 statements/394041 VM/eight write commits to acquisition itself. The totals
do not equal that sum exactly: Save ordering, concurrent content packing and
physical reservation/publication paths also vary. The ninth additional complete
Init write commit is not attributed to acquisition. Do not equate statement or
bound-byte counts with device I/O or copies.

The1023 fullscan steps are the bounded32-row constant SQL inputs introduced by
root completion (992 file steps+31 directory steps), not scans of the stored
population. The
[final plans and population profile](checks/init-acquisition-fix-20261006/18-final-acquisition-tests.log)
show indexed target SEARCH, and the same32-root unit has5 statements/2176 VM/
31 constant-input steps at2000 and20000 stored rows. Those earlier unaffected
proofs are reused; they were not rerun here. Cleanup plans still search the
operation-prefixed primary indexes but construct a list subquery and Bloom
filter for the budgeted keys. Plans and runtime counts agree about repeated
indexed work; they do not establish missing indexes or quadratic behavior.

## Where the current work goes

The reused exact current unit counts are from the same product source and
compact1000-file shape. Worktree path lengths differ, so their payload-byte
charges are not silently substituted for this run's charges. Each unit includes
its transaction and ownership/charge statements.

| Acquisition unit group | Calls | Statements | VM steps | Write commits |
| --- | ---: | ---: | ---: | ---: |
| Entry insertion and native binding | 3 | 2023 | 165261 | 3 |
| File-root completion | 1 | 36 | 65105 | 1 |
| Directory-root completion | 1 | 5 | 1217 | 1 |
| Budgeted discard | 1 | 7 | 117946 | 1 |
| Begin/release/read windows | 32 | 129 | 44512 | 2 |
| Total | 38 | 2200 | 394041 | 8 |

Entry insertion is92.0% of acquisition statement executions. Source
[`writes.rs`](../../../crates/layerfs-persistence/src/backend/sqlite/acquisition/writes.rs)
still loops over the write window. A regular file executes a native-identity
upsert followed by its namespace-entry insert. The earlier fix leases these
statements once per unit, but still executes1000 identity upserts+1011 entry
inserts. A prepared statement and a transaction window do not make those
individual executions disappear. They also maintain the current indexes.

Discard is29.9% of acquisition VM work. Its7 statements return2014 rows:
2011 deleted working rows plus ownership/credit results.
[`cleanup.rs`](../../../crates/layerfs-persistence/src/backend/sqlite/acquisition/cleanup.rs)
materializes each returned payload charge and sums it in Rust. Its shipped
[entry deletion](../../../crates/layerfs-persistence/sql/sqlite/acquisition/queries/discard_entries.sql)
and
[native deletion](../../../crates/layerfs-persistence/sql/sqlite/acquisition/queries/discard_native.sql)
select bounded keys, then locate/delete them again and RETURN their charge.
Provider unit entry and cleanup each check the owner in the same transaction.
This is correct, bounded and indexed, but adds avoidable-looking key/list/row
conversion work to the successful final operation. Removing it requires proof
that the processing budget, exact charges and stale/uncertain fences still hold.

Entry insertion and discard together account for71.9% of acquisition VM work.
The root-window fix already reduced file completion1004→36 statements and total
acquisition3178→2200, while leaving eight acquisition write commits unchanged.
Under Durable, those additional working-state transactions use the declared
synchronized global Store profile. Under Disposable they still incur journal,
transaction and engine work, without a crash-survival guarantee. This is a real
profile cost; changing the persistence profile or holding a whole Init inside
one transaction is not an authorized performance fix.

## What the old path paid instead

The old path uses operation-owned, unsynchronized, length-prefixed file runs for
entries, directory frontier, native identities/aliases, roots and ordering.
Its64 KiB reader/writer buffers,1 MiB/4096-record sort chunks and fan-in16 merges
bound processing windows. It writes ordered runs, re-reads streams to construct
prerequisites/directories/inodes, and unlinks remaining runs before final tree
publication. All scratch cleanup results are checked. Lower SQL work therefore
does not mean zero metadata work, zero I/O or one fewer complete-root obligation.

Both old observations report621490 bytes as the maximum simultaneously owned
scratch runs and1179648 bytes as the largest individual sort capacity diagnostic.
Other reported capacities include131072 entry-stream,118784 job-stream/queue,
65536 frontier-stream and393216 inode-stream bytes. These are overlapping
component capacity observations, not summable total RSS. Cumulative scratch
read/write bytes, number of merge passes, VFS/device bytes and page-cache
residency were not observed and remain unavailable.

The current path removes2011 working rows with306563 logical charged payload
bytes,30 read units,5 write units before cleanup, one discard and one release.
Its largest read window is512 rows and write window1000 rows/340000 charged
input bytes. Charges exclude SQL cells/indexes/journal/pager overhead. Current
logical charge and old simultaneous scratch length have different definitions;
their ratio is not a measured storage or memory improvement.

Source reasoning: for N entries, fixed fan-in/chunk external sorting requires
multiple streaming passes as N grows; indexed insertion remains O(N log N)
with fixed-window keyset traversal and output-sized cleanup. The actual old
sort pass count was not observed, and this one fixture is not a scaling proof
for either complete Init. B-tree asymptotics do not eliminate per-entry index
maintenance, statement execution, byte conversion or commit costs. The current
provider is correctly placed behind the engine-neutral Project port and uses
the Store's existing database; no per-Project second database is introduced.

## Diagnostic clocks and allocation

Raw nanoseconds are retained below. Instrumentation is enabled, cache is
uncontrolled and these values are ineligible for speed admission. They are not
medians, distributions or proof that a particular clock delta is caused entirely
by acquisition. SQL/COMMIT/Save spans overlap and must not be added as exclusive
parts. Statement clock accounting changed to component sums in the new executor;
its time cannot be used as an unchanged observer across versions.

| Interval or allocation | Old Durable | Current Durable | Old Disposable | Current Disposable |
| --- | ---: | ---: | ---: | ---: |
| Complete product ns | 201684583 | 259698792 | 135887041 | 149143916 |
| Public Init diagnostic ns | 187216125 | 243530917 | 130419208 | 136092084 |
| Bootstrap ns | 9222041 | 11140459 | 4730333 | 3984625 |
| Checkpoint ns | 4825875 | 4656125 | 307125 | 221417 |
| Close ns | 372125 | 318333 | 401167 | 8787709 |
| Observed DB/WAL/SHM allocation before checkpoint B | 26247168 | 25198592 | 23068672 | 23068672 |
| Observed DB/WAL/SHM allocation after close B | 20586496 | 20946944 | 20557824 | 20905984 |

The current Disposable close observation is8787709 ns; retain it rather than
remove it from the complete operation or replace it with another sample.
Allocation observations are not maximum disk/journal usage and exclude old
scratch peaks. Process/phase/whole-system RSS was not collected.

The named scan intervals are not identical work across versions: the current
scan constructs attributes, while the old path constructs prerequisites later.
File intervals also differ: the current path records roots through SQLite;
the old path orders roots through scratch. Exact timing trees are retained, but
comparing one identically named child as the whole cause would be misleading.
The complete diagnostic clock includes open/bootstrap, Init, checkpoint and
checked close, including allocation/diagnostic collection; post-clock report
serialization is outside it. Outer process clocks are separately retained.

## Concrete next work and stopping boundary

1. Investigate bounded batch entry/identity execution in Persistence, preserving
   input-order first refusal, canonical first identity, exact native evidence,
   aliases, charge updates and full-unit rollback. Use a prospective count
   observer before changing production code; prove ordered errors for mixed
   valid/invalid/duplicate entries. No unbounded resident map or total-root unit.
2. Investigate a budget-safe final discard path that avoids redundant key-list
   construction and returned-row materialization. Preserve exact row/byte
   credit, remaining-row checks, stale epoch/owner refusal, partial-unit budgets,
   abandoned custody and quarantine after unknown execution. A ledger-only
   deletion that bypasses table/charge consistency is not an accepted shortcut.
3. Attribute immutable publication/reservation/COMMIT/page/copy work before
   changing it. It remains about202000 VM steps even in the old arm. Pack body
   INSERTs already borrow their first-party body; a hypothetical whole-body
   clone is not established as the cause. Preserve Monolithic/Durable settings.
4. After an actual source correction, run the covering public-API proofs once,
   then register only the necessary changed-source checkpoint. Reuse unaffected
   reference evidence. Keep all existing speed FAIL and larger-tier NOT_RUN
   outcomes; this diagnostic does not authorize an unchanged benchmark replay.

This comparison is complete. No new product change, SQLite rollback, larger-tier
campaign, push/release/deployment or reference retirement occurred. R1–R4,
E1–E4,Q1/C1 and full S7/S9 acceptance remain open; R1 and E1 are independently
ready. S8 remains incomplete. P3/P6/P7/P13/P14 remain later Commit prerequisites;
S10–S13 are outside this batch. Four unrelated containers, both owner notes,
both side-conversation documents and the S5/S6 stopping record remain preserved.

Closed stores, binary files and receipts are additionally retained under primary
`benchmark-results/fs-bench-pro/project-old-current-diagnostic-retained-20261006/`.
[Closed-copy hashes](checks/project-old-current-diagnostic-20261006/closed-copy-manifest.json)
authenticate each copy. Original outputs remain in both attached worktrees.
No store or generated binary is staged into Git.

This evidence/docs checkpoint has unchanged production LOC: core94185,
reference65417, combined159602, delta+0. The exact first-parent/final staged
comparison uses unchanged `tools/production_loc.py` SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`,
core/crates+crates including shipped SQL, excluding tests/inline tests/examples/
harnesses/docs/tools/manifests/third-party/builds. Final tree verification and
commit identity are recorded in the tracker receipt and next handoff.
