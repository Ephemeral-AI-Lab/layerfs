# Daemon overlay: initial indexed engine

> **Status:** Current general guide.

Implemented source: the S0/S1 checkpoint introducing `layerfs-overlay` on local
main for [#307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307).
Complete Workspace/FUSE/daemon and milestone qualification remain outstanding;
the [design](../issues/303/README.md) and
[progress](../issues/307/PROGRESS.md) retain the broader target and unfinished gates.

The active [crate](../../crates/layerfs-overlay/src/lib.rs) owns one SQLite
connection and SQL/payload/scratch/custody. It creates fresh disposable state
once; `open_workspace` inserts only a root/incarnation/routing row, without
another connection, schema or immutable-root walk/materialization. Root authority
and filesystem semantic checks remain the Workspace/runtime caller's obligation.

The [profile](../../crates/layerfs-overlay/src/database/profile.rs) reads back MEMORY
journal, OFF synchronization, EXCLUSIVE locking, zero mmap/busy timeout, foreign
keys, 4096-byte pages, configured pager/max-page settings, SQLite version and
compile options. Pager size is a suggestion, not a resident ceiling. Temporary
work selects FILE; ordinary indexed statements need no temporary sort. macOS
retains system SQLite; Linux alone enables the existing driver's bundled feature.
Global Store persistence is unchanged. Disposable state has no sync/checkpoint,
crash-survival or restart-recovery claim.

The original schema had eight application tables. Current schema v11 adds
[bounded live composition](32-live-composition.md) and
[independent file/processing custody](33-independent-custody.md), alongside
SQLite's AUTOINCREMENT allocator. Namespace IDs are not reused.
Every job constrains namespace and checks incarnation against its routing row.
Inode/name keys support both current lookup and generation-selective capture;
cell keys include serial, generation and aligned offset. Metadata queries exclude
payload. Names, data and validity are BLOBs. Scratch/owners are independently keyed
backed state, without resident membership collections.

The following S3 checkpoint advances the disposable schema to version 2: portable
mtime uses separate signed seconds and nanoseconds; mode validation preserves
directory sticky bits, restricts regular files and fixes symlink mode. Startup
reads back schema/application identity. The initial version-1 receipts below keep
their source identity; new profiles accompany the version-2 statements.

The S2 checkpoint advances the disposable schema to version 3 with an installed
generation floor and fixed metadata install/retirement enqueue. Current lookups
exclude retired rows; later active rows/replies survive. Physical cleanup and exact
reader/orphan eligibility are still unfinished. New files are created mode 0600.

`publish` is a primitive for one changed inode, optional final name and optional
4096-byte cell/512-byte mask. It is not S4 compound namespace semantics or S5
byte-write normalization. Its transaction resolves active generation, changes
values/counters and records a publication ticket. A later reply-send attempt
removes only that ticket, including when delivery is lost; published bytes remain.
`capture` refuses outstanding send-attempt custody, then advances fixed metadata
fields without copying inode/name/payload populations. Resolution/install and bounded automatic reclamation are now implemented at
the scope of the S6 checkpoint guides; physical admission remains unfinished.

`captured_inodes` uses fixed `(ns,gen,serial)` membership, keyset pagination and
64-row windows, explicitly selecting `inode_capture`. Initial macOS EXPLAIN chose
the primary `(ns,serial,gen)` key and residual-filtered generation: one returned
captured row cost 540/4124/16412 actual VM steps with 128/1024/4096 later active
rows. The [failure](../issues/307/checks/s1-initial/residual-plan-failure.log) and
[corrected profiles](../issues/307/checks/s1-initial/repaired-engine.log) are retained.
An indexed SEARCH/fullscan counter of zero alone was insufficient evidence.

The [profiler](../../crates/layerfs-overlay/src/diagnostics/metrics.rs) observes actual step
executions, VM/fullscan/sort/autoindex/reprepare counters, returned/attempted changed
rows, bound bytes and inclusive statement wall. It resets cached counters before
each invocation and aggregates in fourteen fixed slots. Exclusive owner-job
snapshots provide attribution; shared lifetime counters are not phase/per-Workspace
resource peaks. Bootstrap PRAGMA/schema work is outside job counters. B-tree
internal page visits, journal/dirty/device bytes, copy allocation and process/kernel
residency remain unavailable. `pages()` reports shared page/freelist observations.

Point access costs O(log N) B-tree work. A capture page returns K<=64 rows with
O(log N+K) index visits plus noncovering fetches, which can add O(K log N).
Fixed-cell replacement binds 4608 data/mask bytes plus keys per call and incurs
SQL/index/page/journal work independent of older fragment count. First-touch and
tiny-file amplification require S5/S7 qualification. Scratch paging holds at most
64 records of at most 65536 bytes; total records continue through windows. Capture
has fixed metadata SQL work, but daemon queue/reply drain is not a constant wall
claim. No speed/cache/residency claim follows from these count diagnostics.

Missing acceptance: fair queued service, retained reads across install, complete
metadata operations, cell transitions/truncate/holes, orphan/failure composition,
physical headroom, automatic live/idle reclaim, authenticated runtime, mounted
cache/mmap/coherence/Exec, canonical construction and integrated qualification.
No S0–S13 completion follows from this initial checkpoint.

## Terminal maintenance checkpoint (#307)

The slice after `c6df039e6` adds schema v4, exact close eligibility and bounded
automatic terminal deletion during live/idle daemon periods. See
[terminal reclaim](25-terminal-reclaim.md) for actual indexed scope, original
error retention and remaining live-generation/orphan/pressure/kernel criteria.
Earlier schema/profile evidence retains its original pin.

## S1 closure audit and profile correction

The closure slice after `14c8a7be4` removes the arbitrary default4GiB quota.
`ProfileConfig.max_pages=None` selects/readbacks SQLite’s format ceiling; an
explicit physical quota is separately recorded. Actual device headroom/pressure
remains S6 work, and this setting supplies no large-capacity qualification.
Operational plan diagnostics now cover exact cell/name/scratch/lease accesses
through shared production query templates. [S1 exit audit](../issues/307/S1-EXIT-AUDIT.md)
maps every criterion to source and scoped proof. S2/S4/S5/S6/S7 remain distinct exits.

## S2 exact custody closure

The closure after `7019801f9` advances schema5 with frozen capture revision,
exact ticket generation and route engine affinity. Bounded observations retain
identity after lost completions; captured bytes remain readable during close.
See [capture custody](26-capture-custody.md) and its S2 exit/evidence map.
S1 evidence keeps its schema4 pin; physical live-generation/failure/pressure
acceptance remains S6/S7.

## Base-source prerequisite checkpoint

The slice after `6e84b9181` adds schema6 transient source custody and maintained
install readiness. The owner now has six classes: new source acquisitions wait
behind a finite known-install fence while existing reads/releases, mutations and
other namespaces progress. See [base-source windows](28-base-source-windows.md).
Earlier schema/class/query receipts keep their original pins. Actual prepared
Workspace/actor/native composition and effective merge remain required.

## S3 completion reconciliation

The completion after `bdc6ed4af` adds effective source-qualified read/ordered-name
composition, actual paired actor install, original unattempted-command custody and
owning SDK stat lengths. See [effective base view](29-effective-base-view.md) and
[S3 exit audit](../issues/307/S3-EXIT-AUDIT.md). Earlier limitations/evidence above
retain their source scope; native/logical runtime transport, mutable byte semantics
and aggregate resource acceptance remain unfinished.

## S4 compound namespace job

The checkpoint after `8d691ab8a` advances the disposable schema to version 7:
every inode row carries its creation generation and a directory's exact visible
entry count. `apply` publishes at most four inode finals, two name finals and
one cell atomically with a single ticket; `source_rows` gives one owner job
consistent point reads, including a name's active and latest lower rows. A
removed name keeps a whiteout only where a lower row or the base binds it.
`publish` remains the single-inode primitive and now shares the same checked
write helpers. See [namespace operations](30-namespace-operations.md) for the
semantics, work derivation and paired plan/runtime evidence. Earlier receipts
keep their schema pins. Byte-stream transitions, removed-inode reclamation,
failure composition and pressure remain S5/S6.

## S5 payload checkpoint

The work after `f5558fc22` advances schema v8 with trimmed cells, optional
validity, engine-maintained lower-layer cutoffs and indexed shrink staircases.
Compound jobs carry one bounded byte write. Effective reads compose local layers
and one inherited range; raw cell APIs remain stored observations. See
[payload streams](31-payload-streams.md) for complexity, amplification, custody
and the paired real-owner evidence. Stale garbage, failure depth, orphans and
physical pressure remain S6; earlier receipts retain their schema/source pins.

Initial S6 after `a0dc7da9b`: schema v9 adds bounded failed-capture composition
and indexed live maintenance. [Live composition](32-live-composition.md) records
the implemented slice and remaining orphan/physical-headroom exits; S6 is not
complete. Earlier receipts keep their source/schema/work identities.

S6 completion update after `be651a048`: schema14 now includes backed resource
accounting and indexed source waits; physical reservation, cleanup headroom,
bounded generation wakes and nonduplicating orphan migration are implemented.
See [shared physical capacity](35-shared-physical-capacity.md) and the
[S6 exit audit](../issues/307/S6-EXIT-AUDIT.md) for current scope/evidence. Earlier
checkpoint limitations and numbers above retain their original source identity.
Native/runtime/kernel and integrated qualification remain later milestones.
