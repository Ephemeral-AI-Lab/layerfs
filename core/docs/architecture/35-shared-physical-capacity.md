# Shared physical capacity and resource snapshots

> **Status:** Current implementation guide, S6 completion slice after `be651a048`.
> Qualification and retained outcomes are in the [S6 audit](../issues/307/S6-EXIT-AUDIT.md).

The [allocation owner](../../crates/layerfs-overlay/src/database/allocation.rs)
retains the descriptor for the fresh private disposable SQLite file, checks its
path/device/inode/link identity and establishes physical capacity before startup
DDL or each BEGIN. It uses existing locked nix0.31.3 safe allocation calls. Linux
uses one `fallocate(KEEP_SIZE)` for the precise nonzero required tail range on
every admission. Darwin uses `F_PREALLOCATE`, physical EOF and `F_ALLOCATEALL`,
checks returned allocation and logical length, and tracks only established dense
allocation. It conservatively discards credit on a length/block decrease. Allocated
block totals do not establish range position. No zero-fill fallback, retry, sync,
VACUUM, shrink or second database is introduced.

The S7 E04 continuation adds a narrow Linux startup compatibility refusal.
After creating the fresh owned descriptor, Linux startup unconditionally opens
the same path read-only with O_NOFOLLOW. Both descriptors must identify the same
regular file with one link. One safe `fstatfs` observation on the verified reopened
descriptor precedes the allocation owner, bulk reservation and SQLite open.
The original create descriptor on this host share reports generic FUSE instead
of the facade filesystem; its insufficient first guard is retained as failed
receipt 11 in `checks/e04-native-backing-20261007`. The same-inode read-only
reopen is a selected startup step, not a retry or fallback after allocation. Observed filesystem magic `0x6a656a63` is refused as
`UnsupportedFilesystem { linux_magic }`. On the actual Docker `fakeowner` host
share, two successful identical KEEP_SIZE requests each added allocation; three
failed E04 runs and the small independent primitive diagnostic are retained in
[the E04 checkpoint](../issues/307/E04-ORIGINAL-RECEIPTS-20261007.md). The created
empty artifact and original probe/refusal remain in caller custody. A failed
probe returns its original I/O cause. No retry or alternate backing is selected.

This refusal does not qualify every other filesystem. Guest overlayfs has only
the separately scoped two-call idempotence observation; full-device qualification
remains ext4. The exact Linux range primitive and 128/256 MiB resource windows
are unchanged. `CreationWork` reports the actual reopen, identity-read and probe attempts
and returned Linux type; macOS and pre-probe failures contain no invented Linux observation.

Ordinary jobs require 128 MiB growth plus 128 MiB cleanup capacity. Lifecycle,
reply release, capture, known install, definite failure and maintenance use the
128 MiB cleanup class. This costs 256 MiB once per daemon at startup, plus SQL
high-water data and allocation rounding/metadata; all of it counts as storage.
It imposes no total Workspace/file/edit/Commit limit. One fixed pre-BEGIN PRAGMA reads committed freelist_count. Owned
file length must be nonzero and page aligned, and bounds the reusable page count. Freelist pages are already allocated reusable capacity, so only
`max(0, budget - freelist_count*4096)` tail must be established. Cleanup can use
freed pages when the filesystem has no new blocks. Tail credit is observed after
transactions, including known rollback truncation.

The [selected derivation](../issues/307/S6-RESERVATION-CONTRACT.md#source-review-amendments-before-final-proof-2026-10-06)
counts up to 700 persistent tree mutations, all physical indexes and accounting
triggers, at most 43 added pages per tree mutation and 100 bounded overflow pages:
123699200 bytes, below one 134217728-byte window. It is pinned to actual SQLite
3.51.0 and 3.53.2 source/build profiles; another version is explicitly unqualified.
This includes simultaneous shrink boundaries, independent orphan metadata,
processing scratch, bounded source wake and cursor work. It is a conservative
format/source bound, not measured page amplification or process residency.

[Accounting SQL](../../crates/layerfs-overlay/sql/accounting.sql) maintains one
namespace row and namespace0 aggregate in the same transaction as product state.
Insert/delete/count/byte triggers use two primary-key updates. They include
namespace/inode/name/payload/mask/shrink/scratch/orphan rows, sources, leases,
file/lookup/captured/operation and reference detail rows, reply tickets, source
waits and ready/retirement/maintenance targets. Resource observations do not sweep
or COUNT the live namespace. [Resources](../../crates/layerfs-overlay/src/database/accounting.rs)
returns logical counts, real shared allocation, guaranteed tail, page/freelist
counts, allocation attempts/refusals and a declared logical debt upper bound.
Debt includes all stored data and <=1024-byte metadata per counted row while
retirement/maintenance is pending. It does not estimate exclusively owned pages;
held targets may be pending without being garbage. SQL deletion can free reusable
pages while file high-water allocation remains unchanged.

[Source waits](../../crates/layerfs-overlay/src/maintenance/source_wait.rs) park
orphan migration while main capture or independent snapshot readers retain its
lower generation. Once fenced/released, a compatible payload row moves to the
orphan domain without duplication; overlap composes and deletes one cell atomically.
The independent orphan remains at most three layers while waiting and one after
migration. A generation reader release enqueues fixed metadata work, then wakes
at most64 target keys per background turn using `maintenance_generation`. It no
longer updates an arbitrarily large generation population in the release job.
Orphan wake pages are likewise bounded. The daemon's existing weighted service
continues live and idle work; failed attempted maintenance retains its original
error/cursor/custody and stops without replay.

`Command::Resources` exposes credited, boxed snapshots through the real owner.
Ordinary physical refusal preserves the exact allocation cause, enters no BEGIN
and leaves publication/capture/other namespaces unchanged. An unknown file identity
or unsafe SQLite read/mutation/COMMIT outcome quarantines the connection; callers
retain exact owners/captures, and no inferred failed-capture resolution or deletion
is authorized. Disposable MEMORY journal/OFF sync/auto_vacuum NONE does not promise
crash durability or recovery after external corruption. No external clone, hole
punch, truncation, mutator, filesystem snapshot/COW interference or device failure
is part of the exclusive backing assumption.

Owned Linux ext4 device-full proof covers actual refusal and cleanup headroom,
including real owner idle last-release cleanup. macOS qualifies allocation and
functional behavior on the current host; full-device/COW behavior there is not
qualified. Native kernel/request/output residency, authenticated history uncertainty
and integrated FUSE/Exec/Commit service remain S8–S12. These resource/correctness
checks do not establish cold-cache speed, RSS, sustained numerical throughput,
universal filesystem support or release qualification.

Operator page-count observations use a fresh statement once: its SQLite Expire
program must not remain cached and induce automatic reprepare. Nonexpiring
freelist admission remains cached. Preparation and execution stay in recorded
diagnostics; eight repeated public snapshots have zero reprepare.
