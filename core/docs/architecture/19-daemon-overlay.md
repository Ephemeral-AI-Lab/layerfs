# Daemon overlay: initial indexed engine

Terminology update2026-10-07: the current API/SQL names below use overlay
schema16. Earlier algorithm and proof pins retain their original scope; see
[the naming checkpoint](../issues/307/PRE-S8-TERMINOLOGY-20261007.md).

> **Status:** Current general guide.

R7 update, 2026-10-09 (directory link counts, overlay schema 22): the `inode`
row gains `subdirs INTEGER NOT NULL CHECK(subdirs>=0 AND (kind=2 OR
subdirs=0))`, the absolute number of child directories bound in a directory
in the row's view, and `Inode` gains the field. Every whole-row statement
carries it (`INODE_PUT`, now fifteen bound values and 120 declared bound
bytes; `INODE_LOOKUP`; `INODE_CAPTURE`; the orphan select; the fold's select
and its `put_inode` copy). `PRAGMA user_version` and its startup readback are
22. No statement was added and no statement count changed. `nlink` keeps its
meaning, the namespace reference count that construction writes back. The
filesystem meaning of the column belongs to Workspace; see
[namespace operations](30-namespace-operations.md).

R7 update, 2026-10-09 (statement diet, schema unchanged at 21): **a job
reads a row once.** A native visit's fence is one statement
(`FENCE_LOOKUP`, `FENCE_FILE`, `FENCE_HANDLE` in
[`statements.rs`](../../crates/layerfs-overlay/src/database/statements.rs)):
the Workspace row, its `native_mount` row and the kernel's own reference on
the named inode, each reached by key, with the earlier outcomes (no row, a
foreign or revoked mount or a missing reference is `Stale`; a closed
Workspace is `Closed` first). The Workspace row it returns is the one the
visit's evaluation, its `apply_checked` and the terminal-cleanup decision of
a RELEASE use; none of them reads it again. The descriptor the fence read is
not re-validated: only its access mode is checked, in memory. The active and
the latest lower row of one name come from one seek (`NAME_LAYERS`), and a
publication computes a name's inheritance once. A file-reference decrement
returns the references that remain (`UPDATE ... RETURNING`), and the kernel
lookup reference and descriptor of a created-and-opened file are one custody
row write. `Changes::created` names the serial a job creates: its kernel
lookup row is inserted without a read, and a row that already exists fails
the whole job. The orphan-domain probe of an inode read runs only after this
engine has attempted its first `INSERT INTO orphan` (`Overlay::orphan_seen`,
set before that statement and never cleared; the database is never
reopened, and a rolled-back insert leaves only a probe that finds nothing).
Per created file the five request jobs now attempt 48 statements (61
executions) instead of 88 (101), exact per job and family in
[`native_visit_cost.rs`](../../crates/layerfs-workspace/tests/native_visit_cost.rs);
plans are `Overlay::explain_native_visit` and the `name-layers` line of
`explain_compound`. Not changed: transaction framing, the `lease` rows, the
accounting triggers, the layer reads of `put_inode_domain` and the second
evaluation of a visit that needed a resident base fact.

R7 update, 2026-10-09 (overlay schema 21): **reply tickets are held in the
engine's memory**, not in a `request` row
([`ReplyTickets`](../../crates/layerfs-overlay/src/lifetime/tickets.rs)). The
job that advances the frontier issues the ticket; a job that fails takes its
ticket back. The one reply attempt returns it through
`ReplyTickets::attempted`, callable from any thread and without an owner
turn; a ticket that is not held is `Stale` and changes nothing, so exact
ticket identity (namespace, revision, generation) is kept. `capture_ready`,
`capture` and the terminal-cleanup hold read the pending set; a job that
finds a namespace's tickets pending marks it watched, and the attempt that
empties a watched namespace reports `Watched`: its caller owes one owner
turn, `Overlay::reply_settled`, which queues a cleanup that waited. The
database is created by its engine and never reopened, so the stored row
outlived nothing this record does not; the set is bounded by the
publications whose replies are in flight. `pending_publications` and the
`reply_tickets` count read the same set.

R7 update, 2026-10-09 (overlay schema 20): an atomic job starts its SQLite
transaction at its first writing statement. Reads before it run in
autocommit, which is the same state here: the connection is the only one,
holds the exclusive lock and runs one job at a time. A job that writes
nothing issues no `BEGIN`, `COMMIT` or freelist read. A nested atomic job is
refused. Owner identities are minted from a counter in the connection, which
creates the database and never reopens it; `workspace.next_owner` is gone,
and an identity minted by a rolled-back job is not reused. The prepared-
statement cache holds 256 statements, above the engine's fixed set of about
224 literal texts, so a steady request cycle prepares nothing twice; the set
is fixed by source and does not grow with files, bytes or operations.
Per-statement observation reads and resets six counters after execution, and
`SQLITE_STMTSTATUS_MEMUSED` is sampled only for statements prepared for one
use.

Implemented source: the S0/S1 checkpoint introducing `layerfs-overlay` on local
main for [#307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307).
Complete Workspace/FUSE/daemon and milestone qualification remain outstanding;
the [design](../issues/303/README.md) and
[progress](../issues/307/PROGRESS.md) retain the broader target and unfinished gates.

The active [crate](../../crates/layerfs-overlay/src/lib.rs) owns one SQLite
connection and SQL/payload/operation records/custody. It creates fresh disposable state
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

The original schema had eight application tables. Schema v11 added
[bounded live composition](32-live-composition.md) and
[independent file/processing custody](33-independent-custody.md), alongside
SQLite's AUTOINCREMENT allocator. Namespace IDs are not reused.
Every job constrains namespace and checks incarnation against its routing row.
Inode/name keys support both current lookup and generation-selective capture;
cell keys include serial, generation and aligned offset. Metadata queries exclude
payload. Names, data and validity are BLOBs. OperationRecord/owners are independently keyed
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
tiny-file amplification require S5/S7 qualification. OperationRecord paging holds at most
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
Operational plan diagnostics now cover exact cell/name/operation records/lease accesses
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

## Read-only terminal cleanup observation

The R7 instrumentation slice adds `Overlay::observe_cleanup(namespace,
incarnation)` without changing schema 19. It performs one fixed query over the
namespace primary key, its existing terminal reclaim key and the existing
AUTOINCREMENT sequence. Live rows require the original incarnation and report
Live, Held or Queued from their actual close/owner/queue state. An absent row
reports Gone only when the namespace lies in this daemon's already allocated
domain. Namespace IDs are not reused. Gone establishes present physical absence;
it does not attest that the supplied absent-row incarnation existed historically.
Invalid, future and mismatched live identities remain errors.

The observation creates no mutable Route, advances no maintenance, adds no
retained lookup/tombstone/index, and is independent of unrelated namespace or
root size. `explain_cleanup_observation` uses the same production query. Host
proof receipt 028 in the R7 stage records one statement, 55 VM steps and zero
full-scan steps beside 64 and 256 unrelated namespaces, with its actual SQLite
profile. Linux proof 095 records 47 VM steps at both sizes with SQLite 3.53.2;
its plan and runtime profile remain separate from the host receipt. Mounted
release lifecycle proof 084 observes Gone after both normal unmounts at its
scoped tree. These are scoped counts and functionality, not qualification.
The authenticated daemon Cleanup control prices one read-only owner job
after terminal routing removal, as described in the native-control guide.
