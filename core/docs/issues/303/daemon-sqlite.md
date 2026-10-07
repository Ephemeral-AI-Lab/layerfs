# Daemon SQLite engine: bootstrap, indexed state and bounded service

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Product source pin: `f96d97651be5299f153ccde2bc8d921dd58807ad`.
> Replaces relevant engine choices in design `334fc743751b9a181e670d0601a24fb3169208f9`.
> Owner revision: 2026-10-05. No implementation, build, test or measurement is
> claimed. Physical layout candidates below are not a completed payload,
> failed-capture composition or crash-recovery algorithm.

This document specifies the daemon-owned overlay engine. Public lifecycle is in
[operation contracts](README.md#primary-design-documents), kernel behavior in [FUSE](fuse.md), and
canonical object/storage/history integration in [06](06-cluster-one-integration.md).
The [cluster one](../../../../cluster_one_handbook.md) and
[CAS/CDC/delta](../../../../cas_cdc_deltaencoding_handbook.md) handbooks govern
the implemented libraries; SQLite overlay transactions do not replace their APIs.

Owner supersession 2026-10-08 at R0 input `1a6bb53ef`: the SDK exposes
ProjectApi, WorkspaceApi and SandboxApi. Ordinary Sandbox/runtime or an external
executor owns command launch, standard streams, exit status and explicit
cancellation; the filesystem daemon owns no command supervisor, launcher,
per-Exec cgroup, command registration or custom Exec wire. FUSE serves every
permitted visible process. Shell exit/zero registered commands proves no
filesystem drain, and forced filesystem teardown never implicitly kills caller
processes. Current [S8 specification](../307/S8-SPECIFICATION-20261008.md) and
[R0–R9 rollout](../307/ROLLOUT-LEDGER-20261008.md) govern prospective work;
historical baseline pins, receipts and verdicts retain their original scope.

Source-ownership revision2026-10-08 [proposed design, reviewed against verified
R1 at `5be93f6d7`]: [reviewed ownership](../307/R2-R5-SOURCE-OWNERSHIP-REVIEW-20261008.md)
assigns the complete native connection/request service to `layerfs-fuse`.
Daemon assembles it with the existing shared SQL/Store services and retains
registry/control, overall Ready/terminal unmount and the existing Commit driver.
Dependency is daemon -> fuse -> workspace; Fuse imports no daemon. This changes
proposed homes, not filesystem guarantees or proof outcomes. Native implementation
remains R2–R5; optional admin is deferred and R1 is verified.

## 1. Readiness and the complete base

[owner requirement; proposed readiness contract]

One local `overlay.sqlite` belongs to the daemon and is initialized before the
daemon reports readiness. Database creation, schema identity, selected PRAGMAs,
statement preparation and owner/scheduler startup are paid once. There is no
per-Workspace database, connection initialization or operation records database file.

The independently prepared canonical/history base must already exist before a
tool call mounts it. The base includes `.git` and its index, ignored files,
`node_modules`, dependencies, symlinks, caches and build output. A source-only
fixture cannot establish readiness for the full repository. Open/mount binds an
authorized immutable root and allocation scope, verifies necessary root metadata,
inserts bounded logical Workspace state and exposes the FUSE mount. It does not
copy, reconstruct, reinstall dependencies, enumerate the whole namespace or
materialize a regular-file tree. Root metadata/authentication may need I/O; no
zero-I/O or measured mount-time claim follows from this design.

```text
PREPARATION / INITIALIZATION                         TOOL CALL
host Init/seal/install -> shared Store; daemon opens in-process
  canonical full root + history binding ---- authorized root ----+
  active content/storage/history/persistence                      |
                                                                v
daemon startup: overlay.sqlite                         logical Workspace open
  schema + settings + prepared SQL                      ns + incarnation + base
  owner + fair scheduler -> ready                       mount -> ordinary Exec
                                                                |
                                                explicit Commit of affected state
                                                                |
                                                terminal unmount + logical cleanup
```

Every daemon embeds the existing cluster-one libraries and opens the shared
Store directly on a named in-VM volume. Host Init/seal/install is separate and
control-only afterward. No host Store/data runtime or layerfs-server exists.
Immutable reuse does not establish a distributed provider; authority, exact
history transitions, reference closure and retention remain explicit.

Mount, ordinary Exec, explicit Commit and terminal unmount can be sequenced at
one-call granularity. The same Workspace can also remain mounted for many
sequential or concurrent calls and repeated incremental Commits, independent of
task boundaries. Exec does not implicitly Commit, and neither Exec completion
nor Commit success closes it. Known install advances its base and preserves later
active changes; obsolete ownership is reclaimed while it remains live. If a resulting
candidate is unchanged, the history API may return `UpToDate`; this does not prove
that every shell invocation has an immutable invocation record.

Persistent Workspaces keep their namespace/incarnation, native mount and valid
kernel/daemon caches across calls. Their Commit snapshots cover the shared
published filesystem frontier, not per-call attribution. Repeated Commit must
not retain one generation per past operation or orphan capture indefinitely;
automatic reclamation and service fairness apply throughout the live lifetime.

Both per-tool-call and per-task orchestration are required, with per-tool-call
the expected common case. Engine ownership, queue service and backpressure must
support Execs of any duration in either mode. No assumed short command, fixed
Workspace lifetime or command classifier may justify retiring state or leaving
reclamation until a presumed near-term unmount.

### Native request dispatch versus SQL service

The existing daemon Overlay owner is the sole SQL scheduler and remains shared
by filesystem, Commit and cleanup. The proposed Fuse dispatcher schedules bounded
native continuations, not transactions. Daemon application/filesystem.rs assembles
one shared Fuse service; service/filesystem_port.rs maps narrow original-operation
interfaces into the existing engine. It must not contain a second request/session
engine or require Fuse to import daemon types. Engine completion/loss and credit
release notifications resume parked Fuse requests without occupying K workers.
Native lookup/open/request lifetime state remains indexed in Overlay; separate
owners/atomic jobs and automatic bounded reclamation do not move into resident maps.

## 2. Database and connection topology

[owner decision: one database; proposed connection/profile candidate]

One database is not the same decision as one connection. The current candidate
uses one daemon owner connection with `journal_mode=MEMORY`, `synchronous=OFF`,
`locking_mode=EXCLUSIVE`, `mmap_size=0`, and an explicitly read-back pager allowance.
The shared owner serves bounded jobs; no caller holds a Workspace mutex while
waiting for its job. In-process rollback and acknowledged transaction atomicity
require proof. No daemon/VM/power-loss survival is claimed under this profile.

```text
FUSE workers / Commit constructors / lifecycle / maintenance producers
                                  |
                      bounded per-Workspace queues
                                  |
                fair runnable scheduler, with class progress
                                  |
                    ONE overlay owner connection
                   one writer + one shared pager
                                  |
                        overlay.sqlite
              metadata + payload + operation records + ownership + reclaim
```

SQLite still has one writer. Parallel Execs and constructors do not create
parallel overlay write transactions. Fair queues prevent monopolization by one
producer but cannot promise a service rate above the writer/device capacity.
Service-unit size, queue wait, journal/dirty pages and sustained throughput are
unqualified. SQL-cache reuse removes statement preparation; it does not remove
B-tree seeks, page faults, copying, index splits or rollback work.

A WAL writer plus a bounded reader pool is an alternative, **not selected here**.
It would require snapshot lifetime limits, aggregate pager budgets, checkpoint
progress, WAL growth accounting and a restart protocol. WAL alone neither makes
logical Workspace custody crash-resumable nor supplies an acceptable checkpoint
latency bound. Do not introduce reader connections under the MEMORY/exclusive
candidate and assume they can interleave safely.

Schema/PRAGMA failures are startup failures. Unexpected locking or unsafe I/O
outcomes fail explicitly; no busy retry or error-driven profile switch is added.
Bundled Linux SQLite linkage remains a build/feature-isolation prerequisite,
not an implementation or throughput result.

## 3. Typed logical rows and required key layout

[proposed logical schema; not frozen physical DDL]

`ns` is a daemon-local namespace identifier. Its routing record binds it to an
external Workspace identity, incarnation and authority. Do not reuse `ns` while
stale jobs, streams, operation records or reclaim rows can still refer to it. Generation,
inode serial, stream and operation identifiers are separate typed values; a
timestamp is neither a generation allocator nor an ownership token.

| Proposed SQL name / logical relation | Values / meaning | Required access keys |
| --- | --- | --- |
| `workspace` | `ns`, incarnation, lifecycle, active/captured/folded identifiers; base binding and authority context | `ns` primary; incarnation routing unique |
| `inode` | `ns`, serial, generation, kind, mode, mtime, nlink, size, stream, inherited cutoff | `(ns, serial, gen)` primary; `(ns, gen, serial)` capture index |
| `directory_entry` | `ns`, parent serial, binary name, generation, inode binding or whiteout, lower-binding fact | `(ns, parent, name, gen)` primary; `(ns, gen, parent, name)` capture index |
| `payload` | global SQLite rowid, `ns`, stream, offset, actual BLOB bytes and validity representation | rowid primary for incremental BLOB I/O; unique `(ns, stream, offset)` |
| `stream` | `ns`, stream, lifetime/owners, cutoff/visibility required by chosen representation | `(ns, stream)`; explicit owner-reference keys |
| `operation_record` | `ns`, operation, record kind, edit/run index or inode serial, constructed root / ordering rows | `(ns, operation, kind, key)`; typed record agreement required |
| `orphan` | `ns`, inode serial, open references, current content ownership and inheritance | `(ns, serial)`; excludes namespace-generation pin chains |
| `reclaim` | `ns`, monotonic queue key, owned target, deletion cursor and conservative charge | `(ns, queue_key)`; generation-selective retirement index |

These are eight logical table groups and proposed names, not a final eight-table
DDL commitment. Ownership/composition may need additional reference relations;
mask/cutoff/orphan/failure algorithms must be resolved before executable schema is
frozen. Runtime queues/caches/process handles do not each imply a persisted table.
`.git`, dependencies and caches use ordinary filesystem rows, not special storage.
Unchanged base entries are resolved from canonical roots, not inserted at mount.

Names remain byte-ordered BLOBs consistent with cluster-one path rules. Metadata
tables can use `STRICT, WITHOUT ROWID` with composite keys; payload uses a rowid
table because incremental BLOB I/O requires it. Project only metadata/length in
metadata queries. Do not load a payload BLOB to answer an overlay inode seek.

```text
overlay.sqlite
  Workspace(ns) -> authorized BaseBinding + generations + lifecycle
     |
     +-- inode(ns, serial, gen) -------> stream(ns, stream)
     |       capture: (ns, gen, serial)          |
     |                                payload(rowid, ns, stream, offset)
     |                                  BLOB data + validity
     +-- name(ns, parent, name, gen)
     |       capture: (ns, gen, parent, name)
     +-- operation_record(ns, operation, index/serial) -> constructed roots/edit runs
     +-- orphan(ns, serial) -> independent content custody
     +-- reclaim(ns, queue_key) -> bounded retirement/deletion cursor
```

Every statement constrains `ns`, including payload by rowid: confirm the row's
namespace/stream owner before opening its BLOB. Raw rowids supplied by another
caller are not authority. Avoid generation scans that filter a namespace-wide
cursor in application code. A fixed captured generation uses its capture index
and a terminating keyset domain; concurrent active names do not enlarge that
domain. Query-plan checks must confirm the intended index and absence of
unbounded temp sorts.

Capture retains existing G rows/stream ownership rather than bulk INSERT/SELECT
copying the overlay. Later mutations create active G+1 state only for affected
keys/units. Operation records is new metadata in the same database; it is not a
whole-file payload copy. Captured bytes/masks remain immutable, even if active
state shares lower content. Install preserves active rows over equivalent new
base, then retires only unreachable captured/operation records state. See [Commit row
lifetimes](workspace-api/commit.md#51-existing-captured-rows-new-active-rows-and-operation-records).

Mutable mtime is current inode metadata, updated in the mutation transaction.
There is no timestamped event/history row per WRITE. Cluster-one history is
published through explicit Commit; one SQL transaction per mutation does not
mean one canonical/history version per mutation.

## 4. Payload candidates and byte-exact visibility

[proposed candidates; selection and lifetime algorithm still required]

The withdrawn immutable-extent-boundary algorithm allowed a full-window write
to visit every prior one-byte fragment. SQLite cannot cure that by executing the
same loop in fewer SQL statements. Candidate parameters must bound touched cells,
validity work, BLOB copies and dirty/journal pages by request size and B-tree work.

A bounded-cell candidate uses a parameter `C` bytes of logical address space per
cell. Cell offset is `floor(file_offset / C) * C`; at most
`ceil((request_length + C - 1) / C)` cells intersect one request. A validity bitmap
needs `ceil(C / 8)` bytes when it tracks each byte. Candidate `C=4096` gives a
512-byte bitmap; those are arithmetic examples, not selected layout or measured
page costs. Stored data must distinguish written zeros from invalid bytes.

For a resolved cell, valid active bytes override lower content. Invalid bytes
below the inode's inherited cutoff fall through to captured/base content;
invalid bytes at/above it read as zero. Truncate/regrow must change visibility
without exposing discarded active or inherited bytes. Capture must own immutable
data and mask/cutoff state even while active writes continue.

Two physical candidates need comparison:

- Fixed-length data BLOB plus validity BLOB per cell. Incremental writes change
  bounded existing BLOB ranges without base copy-up. First allocation costs the
  cell representation even for a one-byte write; count space, zero initialization
  and index/overflow pages, especially across many tiny files.
- Inline small-tail data/validity rows that grow only to a declared cell bound,
  then transition to fixed-length cells. This can reduce tiny-file overhead but
  requires bounded BLOB replacement and a capture-safe transition. It must not
  copy a whole growing log or introduce a per-file edit threshold.

Pinned `rusqlite` positional BLOB writes modify fixed-length BLOBs; they cannot
resize them. Tail growth therefore binds/replaces a bounded BLOB value, or
allocates a declared fixed-size cell. Preserve binary type explicitly: the old
`data || ?` example yields TEXT and is not valid for the old STRICT BLOB column.
Relevant primary references are [SQLite BLOB writes](https://sqlite.org/c3ref/blob_write.html)
and [STRICT tables](https://sqlite.org/stricttables.html).

The row/key sketch does not finalize mask updates, active/captured sharing,
nonzero-cutoff reclamation, orphan ownership or failed-capture composition.
Those are required algorithm contracts in [04](04-concurrency-commit.md), not
optional details to infer from the table. An alternative normalized-range layout
is acceptable only with equally explicit fragment-visit, copy and page bounds.

Sparse files require metadata representing holes and a hole-aware cluster-one
construction route. Ordinary append of a committed log uses a tail edit, not
whole-log reconstruction. Existing `apply_edits` refusal, resident directory
changes and new-parent membership are mandatory cluster-one corrections in
[06 §6](06-cluster-one-integration.md#9-prerequisites).

## 5. Prepared SQL and service windows

[proposed execution contract]

An indexed point seek costs O(log N) index work; a range/keyset page costs
O(log N + k) for rows visited/returned. BLOB bytes, mask processing, index splits,
page faults and copies are additional costs. General grep/search still reads its
searched bytes. No O(log N) claim applies to returning an arbitrary directory or
constructing an arbitrary Commit.

```text
WRITE request                         READ request
  fair admission                        bounded consistent owner job
  base metadata miss outside SQL        resolve latest inode/name + cells
  owner validates incarnation/view      copy overlay bytes + retain base roots
  BEGIN                                 release owner/logical locks
    bounded data + validity changes      fetch uncovered canonical base ranges
    inode size/mtime + index changes     authenticate and stream bounded reply
  COMMIT -> accepted reply

CAPTURE                               CLEANUP / RECLAIM
  fair owner job                        fence terminal namespace or retired domain
  serialize after accepted mutations    retain capture/orphan/operation ownership
  fix generation + cursor domain        one page/work-bounded owner job
  construct outside owner               save cursor -> yield fairly
  operation_record uses (ns, operation, ...)      next job only when runnable/admitted
```

The owner never waits for network, construction, Exec, or kernel notification
inside a SQLite transaction. Compound metadata/cell reads that form one reply
are one consistent owner job; do not combine independently stale snapshots.
Logical generation/binding updates and SQL job results need an explicit ordering
protocol. A caller must not cache `active` in an open handle or retain a Workspace
lock across queue wait. If metadata acquisition yields before mutation, validate
the current view before applying the one attempted write; this is not a retry of
an operation with uncertain persistence outcome.

Commit constructors use fixed pages over captured membership and operation-keyed
operation records in the same database. Construction releases the owner before chunking,
hashing or sending objects. Input replay uses indexed operation records run lookup rather
than a resident vector growing with edit count. This does not remove cluster-one
source bounds by itself.

Fair admission covers mutation, read, capture, operation records and maintenance across
Workspaces. Busy-inode/resource waiters are parked without occupying all FUSE
dispatch workers. Bound queued request bytes and runnable windows, yield between
service units, and guarantee progress for every class. Strict read priority can
starve Commits; round-robin names alone cannot prove fairness when job work varies.
Report queue wait, service time, resource wait and same-inode wait separately.

## 6. Memory, pressure, retention and teardown

[owner requirements; proposed accounting]

Bound resident windows, aggregate queues and live session state, pager allowance,
transaction journal/dirty pages, immutable caches, reply buffers and guest/host
file-cache residency. `cache_size` is a pager suggestion, not a whole-process or
OS cache bound. Shared kernel dentries/inodes/open descriptors have attributable
memory too. Stream stdout/stderr and payloads with backpressure; no total-flow or
automatic Bash duration cap is introduced by buffer admission.

There is no artificial Workspace file-count, change-count, file-size, edit-count
or Commit-total-size limit. Finite storage/memory/descriptors and SQLite/platform
bounds are real resources. Configured windows/session concurrency are explicit
admission budgets, not excuses to truncate the data model or silently drop output.

`max_page_count` is global to the daemon database. Logical per-Workspace charges
must include metadata/indexes, payload, operation records, captures, orphan state and debt.
Reserve aggregate physical headroom for accepted operations and their lifetime
transitions. Reclaim charges use conservative accounting and actual allocation/
freelist observations: a stream length does not reveal pages exclusively freed
by deleting it because rows/indexes share pages.

No tiny write loops through arbitrary old garbage before replying. Pressure
admission waits fairly or returns a typed resource result before mutation, with
no retry after a failed SQL write. Cleanup after a successful transaction cannot
turn its accepted write into an error. Service capacity must keep up with the
declared sustained rate; a discard burst is debt, not evidence of a fixed bound.

### 6.1 Automatic batched SQL deletion

Logical retirement closes a namespace or stops using an obsolete captured domain;
the underlying rows may still exist. The daemon automatically executes bounded
SQL row-deletion jobs once no reader/capture/orphan/operation owns the target.
This is the meaning of local cleanup, not deletion of separate payload files.

```text
install success / terminal unmount / last-owner release
                             |
                     retire eligible local target
                             |
                 enqueue reclaim cursor + accounted debt
                             |
               fair daemon owner: DELETE bounded batch
                             |
                   cursor advances -> yield -> repeat
                             |
                  target rows gone; bookkeeping released
```

The worker is driven by enqueue/release events and continues while idle between
tool calls. It does not require a next write/mount/Commit/status or manual cleanup
API, and has no intentional expiry delay. Actual service rate/backlog/device
latency determine completion time; no numerical deadline is qualified. Generation
scans and payload deletion must remain work-bounded, with no namespace-sized DELETE
on the mutation/terminal reply path. Account debt and guarantee service share.

At successful terminal unmount, activity/custody is fenced and the namespace is
closed, then all remaining local inode/directory entry/payload/stream/operation records/orphan rows are
owned by automatic cleanup. Keep minimal terminal/reclaim state until that work
finishes; do not reuse its namespace while stale rows/jobs exist. Other Workspaces
and the daemon connection/schema stay alive.

SQL deletion can free cell space or pages for reuse without shrinking overlay.sqlite.
Not every row owns a physical page. Database high-water allocation may remain until
daemon teardown; do not DROP shared tables or unlink the database for one Workspace.
No VACUUM/file-shrink work or global immutable-object/history GC is implied by
local cleanup. Reuse does not make a measurement cache cold.

Unsafe shared SQLite corruption/I/O or unknown state can affect every daemon
Workspace. Logical namespace admission refusals can remain local. The current
host provider also quarantines its shared session after unknown persistence
outcomes; daemon adapters report that session scope honestly.

## 7. Outcome, trust and lifetime prerequisites

[required integration proofs]

Immutable content IDs do not authorize a peer or prove a role/reference graph.
Content constructors produce objects in the same daemon process; bounded root
checks and explicit Content validation retain policy/reference obligations.
Authenticated filesystem controls bind route/incarnation and Branch/scope.
Sandbox setup protects whole-Store authority and daemon credentials. Enforce canonical object length,
batch/queued bytes and live-session budgets before allocation. Reserve demand
read/control capacity; constructor-local Saves cannot monopolize the fixed
reader set or native filesystem service.

Actual Save borrows its independently owned Storage on the Commit thread.
There is no remote Save/session registry. Shared-process/interleaved Save proofs
retain their exact direct-provider scope; mounted construction/service remains
required. No legacy engine or patched dependency is a fallback.

Keep construction acceptance, Save finish, stage creation, history transition,
overlay install and physical retirement distinct. A lost stage/discard/transition
reply preserves the exact uncertain disposition; never fold or resend on a
guess. Force refuses active namespace control producers before effects; stopped
unknowns preserve exact original custody. No host data operation is dispatched. MEMORY/OFF supplies no cross-crash reconstruction procedure. The
bounded orphan/failure-composition algorithm and its sustained progress remain
required before claiming this engine can serve a continuously appended log.

## 8. Full workload evidence and future proof

[historical evidence; no product qualification]

The retained full prepared tree has 130,045 entries: 103,108 regular files,
16,867 directories and 10,070 symlinks, with 3,475,776,149 regular-file bytes.
Copied source HEAD is `639ed015397290b3745d163aafe02ffee4aa3f84`; manifest SHA-256
is `98fd26440b9087bcfd5c23bec9e5497434a2dcb9a27fc85ad0b823ba9c516658`.
The native install oracle creates 95,021 entries and writes 2,126,509,110 bytes.
Source: `codex/phase7-experiment-305` at
`1451b68a720bbe2175a103dd9b35693ad05e2be1`,
`core/docs/issues/305/PREPARATION-REPORT.md`. These are retained observations;
no command was executed in the original deepseek-harness repository for this
document. Passthrough/prototype evidence is not integrated product evidence.

Future proof must retain the complete prepared base and complete affected-state
Commit: git status/build/compiler access, install copy and hard-link replay,
rename/unlink churn, caches/output, sparse files, fragmented edits, concurrent
log appends/tail/rotation and several Workspaces with independent hot activity.
Per-call mount/Exec/Commit/terminal-unmount and eventual reclaim debt are all
observed; a fast Exec alone cannot establish the complete workflow.

Use deterministic correctness interleavings and labeled diagnostics for query
plans/visited rows, BLOB copies, journal/pages, queue service, orphan/version
depth and actual allocation/reclaim progress. Do not manufacture throughput
from statement counts. Qualification follows [07](07-implementation-validation.md):
read the benchmark report rules before each invocation, freeze identities/cache
contracts/budgets prospectively, one sample per case per arm, fresh append-only
outputs, prepared setup clone and matched cold-state enforcement. No benchmark
or workload is run by this documentation change.
