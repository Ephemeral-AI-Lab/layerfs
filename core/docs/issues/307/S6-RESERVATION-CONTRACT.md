# S6 physical reservation and accounting selection

> **Status:** Selected implementation/proof plan after `be651a048`, 2026-10-06,
> before code. Later source-review amendments below are prospective
> selections; exit evidence is recorded separately, not inferred from this plan.

The daemon retains one descriptor for its freshly created disposable database,
checks the descriptor/path identity, and uses the existing locked nix 0.31.3 safe
allocation wrappers. Linux uses `fallocate(KEEP_SIZE)` on a supported filesystem;
macOS uses `F_PREALLOCATE` from physical EOF with `F_ALLOCATEALL`, verifying returned
allocation and unchanged logical length. Unsupported allocation is an explicit
resource/platform result, never a disk-free estimate or zero-writing fallback.
No sync, VACUUM, global Store relocation or third-party source/pin change is allowed.

Reserve before BEGIN/DDL, outside any SQL transaction. Separate ordinary mutable
admission from release/failure/install/cleanup headroom. Proposed conservative
windows: 64 MiB ordinary growth and 32 MiB cleanup/transition reserve, shared once
by the daemon, with exact allocated/logical/page/freelist observations. All real
allocated tail counts in storage reports; it is not exclusive Workspace data or
RSS, and logical SQL deletion need not shrink the file. Enforce auto_vacuum NONE
and read it back, so implicit shrink cannot silently discard the reserve.

The growth arithmetic uses the actual supported SQLite implementation: 20-level
cursor depth, at most three old/five resulting pages in one balancing window, at
most two added pages per level, bounded overflow for the <=65536-byte scratch row,
and each operation's actual bounded record/index changes. Compound jobs have
<=4 inode finals, <=2 names and <=34 cells including optional creation cell;
last-unlink custody/enqueues also count. Cleanup deletes <=14 cells or <=64 small
records, or transfers one cell/name. 64/32 MiB are conservative rounding windows,
not a total file/Workspace/Commit cap. Verify this arithmetic from current source
and every changed DML before qualifying it; a missing bound stays INCOMPLETE.

Read-only primary references for the derivation:
[SQLite cursor depth](https://github.com/sqlite/sqlite/blob/version-3.51.0/src/btreeInt.h),
[SQLite balancing](https://github.com/sqlite/sqlite/blob/version-3.51.0/src/btree.c),
[Darwin allocation syscall](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/kern_descrip.c),
[Linux KEEP_SIZE allocation](https://www.man7.org/linux/man-pages/man2/fallocate.2.html).
The locked bundled 3.53.2 source has the same depth/balance constants; macOS uses
system 3.51.0. These are implementation/primitive inputs, not cross-version or
filesystem qualification by inference.

Reservations must remain valid after rollback/truncation; a cached capacity alone
is insufficient. Preserve exact failed/partial allocation custody and refuse before
SQL mutation. Known SQL FULL rollback stays one attempted operation; unsafe I/O or
COMMIT uncertainty keeps/quarantines original state. Releases and automatic cleanup
must remain possible after ordinary admission closes. No operation is retried or
replayed to obtain success. Any allocation failure during maintenance must be
classified explicitly and retain its cursor/custody, without guessed cleanup.

Add backed namespace/global accounting for actual stored row/BLOB counts, ownership
and maintenance readiness/debt targets, maintained in the same transactions. Expose
indexed snapshots and actual file allocation, page/freelist and reserve observations.
Do not COUNT/sweep the live namespace on tiny operations or claim exclusive pages
from stream lengths. Exact obsolete-byte debt can be unavailable; report a declared
conservative upper bound and actual reclaimed rows/data separately. Scope physical
capacity and filesystem semantics honestly, including external/COW interference.

Proof selection is component correctness/resource verification, not a performance
benchmark or integrated FUSE/Commit qualification. Retain fresh append-only outputs,
source/toolchain/binary/image/workload identities, all outcomes and actual counts.
No cold/latency/RSS/speedup claim is selected. Keep existing page-quota FULL proof,
add actual owned filesystem/device ENOSPC with prior publication/capture preservation,
ordinary refusal before SQL, cleanup headroom/last-owner progress and unchanged
unrelated namespace state. Exercise host allocation and Linux ext4 in an exclusively
created loop image with exact loop/mount ownership and fenced teardown; never touch
unrelated containers/devices. Build first, each test <=120s; separate selected proof
should fit the default <10s scope. Preparation/build is separate and reusable only
by exact identities; failed/unrun cases remain visible. No raw SQL test substitute
for the product mutation/admission path, no workload/timeout changes to pass a miss.

A pre-existing capture/reader may block source deletion. Avoid payload duplication
under retained snapshot custody: a bounded orphan turn should move/reuse lower
physical cells once independent snapshot consumers release, while keeping its
fixed <=3-layer view valid. Held source readiness/wakes must remain indexed and
bounded. Physical resources can refuse new state; they cannot silently lose data,
turn uncertainty into release authority or hide a growing resident/debt chain.
This part must be resolved and proved before the S6 physical row is complete.

## Source-review amendments before final proof (2026-10-06)

The provisional 64/32 MiB windows did not include every simultaneous compound
shrink boundary, orphan metadata row, index operation and accounting trigger.
They are superseded by **128 MiB mutation growth plus 128 MiB cleanup reserve**.
This is a 256 MiB daemon-wide physical cost at startup, plus database growth and
filesystem allocation rounding/metadata. It is not per-Workspace, exclusive data,
RSS, a large-workload timing result or a storage improvement. The initial 128 MiB
ext4 fixture is correspondingly superseded by an owned 512 MiB image so setup can
contain the selected reservation. No frozen performance/cache gate was selected,
changed or passed by this change; all earlier outcomes remain retained.

For the qualified SQLite 3.51.0/3.53.2 builds, one physical B-tree mutation has a
conservative 43-page growth allowance: two additional pages at each of at most
20 levels, plus three root/freelist bookkeeping pages. Page size is 4096; add
bounded BLOB overflow separately. Count individual persistent trees and index
insertion/deletion, including the two accounting point updates per affected row.
The public job has <=4 inode finals and <=2 name finals. To cover all branch
combinations conservatively, use twice those maxima in this derivation; this does
not enlarge the public processing windows. Its conservative count is:

| Group | Persistent tree mutation allowance |
| --- | ---: |
| Eight inode finals (two inode trees plus two counts) | 32 |
| Eight shrink boundary cell deletions/changes (four payload trees plus two counts) | 48 |
| Eight staircase records (two trees plus two counts) | 32 |
| Sixteen shrink maintenance enqueues/changes (three trees plus two counts) | 80 |
| At most 33 write cells and one optional creation cell | 204 |
| Four name finals (two trees plus two counts) | 16 |
| Eight serial-retirement enqueues | 40 |
| Eight orphan records | 24 |
| Eight independent inode rows | 32 |
| Eight orphan maintenance enqueues | 40 |
| Workspace frontier and publication ticket | 4 |
| **Total** | **652** |

Use a rounded 700-tree allowance for every bounded job, including acquisition,
release, startup, install, composition and maintenance. The heaviest wake page is
64 times (five maintenance tree changes plus three wait-row/count changes), plus
its fixed cursor bookkeeping: less than 700. Scratch cleanup is bounded by both
64 records and 65536 bytes, including per-record cursor changes; terminal cleanup
has the same byte bound. Other deletion pages have at most 14 payload cells or
64 metadata records, and composition transfers one cell/name. All paths fit this
allowance; release must enqueue, never wake an unbounded target population.

Allow another 100 overflow pages, covering 42 cell changes (including every
shrink boundary), optional validity, and a 65536-byte scratch row conservatively,
even though these maxima do not occur together. The growth bound is
`(700 * 43 + 100) * 4096 = 123699200 bytes < 134217728 bytes`.
Each ordinary admission covers that job and leaves one cleanup window. Cleanup
can use its own window without accepting new mutable input. A different SQLite
version is explicitly unqualified until its growth rules are reviewed; startup
refuses it rather than silently extending this derivation.

Freelist capacity is real committed database capacity, not file-cache credit.
Before each BEGIN, one fixed nonexpiring PRAGMA reads freelist_count; descriptor length
must be nonzero and page aligned, with freelist_count within its physical prefix. Required tail is the selected budget minus
freelist_count*4096, floored at zero. This lets deletion replenish cleanup capacity
without needing free filesystem blocks. Linux establishes that exact nonzero tail
range on every admission with one KEEP_SIZE call. Darwin credits only previously
established dense-file allocation, capped conservatively by observed blocks; a
length/allocation decrease discards tail credit, and the next job establishes its
own capacity. Post-transaction observations include rollback truncation. Block
sums do not establish range position, and metadata blocks never enlarge the
tracked guaranteed end.

The exclusive fresh private backing must not be independently cloned, punched,
truncated or mutated. No reservation promises survival of external corruption,
filesystem snapshots/COW interference, metadata I/O faults or device failure.
Owning device-full qualification is Linux ext4; macOS qualifies allocation and
correctness on this host, without a full-device/COW qualification. Exact original
unsafe errors quarantine custody. MEMORY journal, OFF sync and auto_vacuum NONE
remain disposable guarantees; no crash durability is added.

The source review also replaces an unbounded generation-ready UPDATE with one
indexed wake item and <=64-key turns. Orphan source waits are backed and wake in
<=64-key turns. A retained captured domain/reader parks orphan migration. Once
released, physical rows move to the independent domain without payload duplication;
only overlapping bytes require one-cell composition. Accounting counts every
namespace, stored payload/mask/scratch byte, row, owner/reference detail, ticket,
wait and debt target in the same transaction, using two exact primary-key updates.
