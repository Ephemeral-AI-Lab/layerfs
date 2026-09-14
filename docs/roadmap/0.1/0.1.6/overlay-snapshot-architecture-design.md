# Snapshot-Isolated Workspace: architecture and design

Status: pre-specification architecture draft, 2026-09-14. Governed by
[overlay-snapshot-rule.md](overlay-snapshot-rule.md). Related work:
[#123](https://github.com/Ephemeral-AI-Lab/layerfs/issues/123) and the applicable
infrastructure in [#122](https://github.com/Ephemeral-AI-Lab/layerfs/issues/122).

This document supplies diagrams, component boundaries, demonstrations, source
findings, and conditional complexity analysis for the next specification. It is
not an implementation, a selected storage-engine specification, or a performance
qualification. Code review baseline: `0814cc37f1dafb6041930c74489107f4a5035a26`.
The rule document is a local prerequisite and is not part of that source commit.

The subsequent [detailed specification draft](overlay-snapshot-spec.md) selects
concrete draft storage/authority and predecessor-correspondence contracts and
records the resulting removal/addition boundaries. Its
[adversarial review](overlay-snapshot-spec-review.md) adds corrected concurrency,
zero-range locality, retention, and performance requirements. This architecture
document retains the original alternatives and source analysis; neither document
claims full-surface readiness or measured benchmark performance.

Owner review update, 2026-09-14: the
[promoted #130 implementation plan](overlay-minimal-overhead-implementation-plan.md)
prioritizes local reclamation instead of whole-arena sweeps and bounded Index
preparation. Select further density/range/payload changes from measured costs;
no new RAM-first pager is required. The current objective is close performance
on the existing 20 tiny-churn cases. The 25k/two-second and million-file cases
are deferred. Quadratic growth is rejected, including cumulative maintenance
and diagnostic work. This update
supersedes the old after-closure scheduling and unconstrained design alternatives,
while preserving all snapshot/ownership/publication contracts.

## 1. Architectural conclusion and unresolved gates

Separate live filesystem operation from Commit construction/publication. Maintain
one current mutable overlay, acquire a temporary immutable snapshot, construct a
canonical candidate from that snapshot, retain it in `workspace_stages`, and
conditionally publish branch history. Never install the published snapshot back
into live files. Changes made during construction remain in the current overlay.

The existing detached candidate builder, shared file ranges, canonical encoding,
sorted namespace updates, bounded construction journals, and staged publication
are valuable foundations. The existing public lifecycle is incompatible: it
freezes operations, borrows live/fact maps, installs a same-generation checkpoint,
and resets dirty state. Removing the freeze calls without replacing those
assumptions would be incorrect.

Init and snapshot-based Workspace Commit continue sharing the existing content
construction, canonical formats, and bounded output/admission machinery. The
snapshot is a replacement input/ownership boundary for Workspace Commit, not a
second storage engine. Initial namespace assembly and incremental namespace updates
remain appropriate distinct planners over shared formats/primitives (D8, S14).

Three gates remain unresolved. No implementation may claim this architecture
complete until all three are designed and qualified:

| Gate | Required resolution |
| --- | --- |
| G1: FUSE visibility | Capture supported buffered writes and writable mappings consistently without global freeze, whole-cache drainage, or silently weaker semantics. A host root alone cannot capture bytes absent from host-visible state. |
| G2: shared metadata authority | Select one installed-state authority and a bounded snapshot-capable index. Prove acknowledgment, cache coherence, atomic namespace mutations, and ownership across container/host. |
| G3: incremental continuation | Preserve post-snapshot changes and reuse prior canonical output without resetting live state, accumulating historical dirty work, or falling back to whole-file reconstruction after every Commit. |

The recommended candidate is host-owned snapshot-capable metadata backing with a
bounded container adapter/cache. It is a proposal, not a settled migration choice:
the current live owner is in the container, and relocating mutation authority may
increase foreground protocol cost. Section 7 compares alternatives.

## 2. Diagram conventions and reading order

`[E]` denotes an existing mechanism, `[P]` a proposed component/boundary, `[G]`
an unresolved gate. Labels apply to the depicted role, not a promise that an
existing implementation can be reused unchanged. `-->` means operation/data flow;
`-.->` means reference/ownership. Physical placement and lifetime are explicit.

Diagrams D1-D8 are the planned specification diagram set, drawn here as ASCII so
agents can read them without a rendering tool. Each states its scope, invariant,
cost concern, and source/design status. Copy these IDs into the later spec rather
than inventing inconsistent names. Failure/ownership details belong in D4/D6;
do not overload the overview with every protocol frame.

| ID | Question answered | Read with |
| --- | --- | --- |
| D1 | Where do live operation, backing, snapshots, and Commit run? | G1/G2 and interface table |
| D2 | What is shared, and what changes after snapshot acquisition? | Ownership and memory bounds |
| D3 | Which changes belong to successive Commits? | Demonstrations A/B |
| D4 | How do staging, conflict, retry, and cleanup relate to live state? | Failure matrix |
| D5 | How are edits and Commit work localized? | Change-tracking and complexity analysis |
| D6 | Who retains each resource, and when is it reclaimable? | Reclamation/accounting rules |
| D7 | Which interfaces synchronize, perform I/O, or retain state? | Component responsibilities |
| D8 | How do Init and snapshot Commit reuse construction and CAS/FULL-DELTA/CDC formats? | Shared machinery, distinct input/namespace/publication planners |

### D1. System architecture and deployment

```text
CONTAINER                                      HOST
====================================           ====================================
Commands                                       SDK mutation request
   |                                                   |
FUSE mount /workspace/a [E]                     route through SAME authority [P/G2]
   |                                                   |
Linux VFS/page cache [E/G1]                             |
   | /dev/fuse request connection                      |
Live operation adapter [E -> P] -- mutations --> installed-state authority [P/G2]
Bounded active cache [P]       <-- replies ----         |
                                                       v
                                              Workspace private backing [E -> P]
                                              current metadata root [P]
                                              bounded buffers + disk index [P]
                                              spool segments [E, ownership extended]
                                                       |
                                              acquire/retain root [P/G1/G2]
                                                       |
                                              temporary snapshot handle S [P]
                                                       |
                                              snapshot reader [P]
                                                       |
                                              candidate construction [E adapted]
                                                       |
                                              workspace_stages [E]
                                                       |
                                              conditional branch publication [E]
                                                       |
                                              persistent Store / branch history [E]
```

Invariant: one live mount and one authoritative installed state; no copied
snapshot directory or second FUSE connection. Commit reads an internal snapshot,
never live paths. `/dev/fuse` is a device interface, not a directory or storage.
SDK mutations cannot bypass the installed-state/visibility contract.

Cost: authority placement determines protocol round trips and cache-coherence
traffic. Host backing already exists, but host metadata currently consists of
acknowledged facts rather than an independently current snapshot root [S04/S09].

Current SDK backing placement is
`<host-temp>/layerfs-runtime/<pid>-<counter>/workspaces/<id>/spool/`; lower-level
callers can select another runtime root [S10]. Current spool files are opened and
immediately unlinked; retained descriptors keep disk bytes alive [S03]. Proposed
metadata/index backing belongs to this private host lifetime. File names/page
format are undecided. Cached pages and snapshot descriptors may be in host RAM;
snapshot retention must not make all reachable pages resident. The Store path is
configured separately. Temporary storage implies no new crash-recovery promise.

### D2. Overlay and snapshot storage

```text
BEFORE CAPTURE                       AFTER CAPTURE AND ANOTHER EDIT

live -.-> root R                    live -.-> root R2
             |                                   |
             v                                   +-.-> changed metadata pages
       index/page graph                          +-.-> replacement Y
             |                                   |
             +-.-> inode records    snapshot S -.-> root R
             +-.-> namespace keys                |
             +-.-> change index                  +-.-> preserved metadata pages
             +-.-> file ranges                   +-.-> replacement X
                                                 |
                          both roots -.----------+-.-> shared unchanged pages
                                                 +-.-> committed base objects

LOGICAL FILE EXAMPLE
snapshot /a and /alias -.-> inode 42 -.-> [base prefix][X][base suffix]
live     /a and /alias -.-> inode 42 -.-> [base prefix][Y][base suffix]
```

Proposed snapshot representation: one owned descriptor containing a consistent
bundle of namespace/inode/change-index roots, base-reading context, and backing
ownership identity. Installing a filesystem mutation atomically updates the bundle;
snapshot acquisition cannot observe mismatched roots. Internal ordering tokens are
not Workspace checkpoints or retained action history. New logical inode identity
must be stable before a later Commit makes that inode canonical.

The subsequent specification selects immutable published copy-on-write pages,
including pages owned only by the current root. Ordinary updates touch affected
pages and replacement ranges; unowned prepared pages can be filled before
publication. The earlier exclusive-page in-place alternative is not selected.
No whole-map copy on capture or the first subsequent write is allowed. Metadata eviction must preserve
stable references to disk or retained buffers without an unbounded RAM indirection map.

Payload segments can accept non-overlapping appends while older ranges are read.
Length/publication ordering must prevent a snapshot referencing unwritten bytes.
Reusing old offsets requires stronger ownership than a currently absent live piece.
The disk ownership model must cover references no longer represented by `Arc`s.

### D3. Operations overlapping sequential Commits

```text
Time   command / same open fd       snapshot and candidate        branch
----   --------------------------  ----------------------------  ---------
t0     write A completes                                         C0
t1                                 acquire S1 = state A
t2     write x completes           build exclusively from S1
t3     rename completes            stage R1 from S1
t4     read/write still complete   publish R1                     C0 -> C1
t5                                 acquire S2, including x
t6     further operations          build/stage R2 from S2
t7                                 publish R2                     C1 -> C2

Commit 2 requested at t2: queued, NO snapshot retained until it starts at t5.
Publication at t4 does not reset the overlay or install S1 into open inodes.
```

Invariant: C1 excludes x; C2 includes x if its effect remains at S2. A write in
flight at acquisition is ordered by the specified installation/visibility boundary,
not the wall-clock time its application call began. Atomic namespace operations
are wholly before/after the boundary. Open handles follow live identity; owned
read plans/snapshots follow their captured content.

Cost: one unresolved Commit attempt per Workspace bounds candidate multiplicity,
not snapshot size or arbitrary reader retention. Queue memory and wait policy are
explicit. No queue wait holds filesystem-operation admission. Different Workspaces
may construct concurrently under aggregate budgets and existing branch leases.

### D4. Separate filesystem lifetime and Commit-attempt states

```text
FILESYSTEM:   Active ------------------------------------------> Active
                 operations continue, including on Commit failure
              Explicit end/discard has its own coordinated cleanup semantics.

COMMIT:       queued -> acquired -> building -> staged -> publishing
                            |          |                       |
                          failure    failure                   |
                            |          |                       |
                            +-> release owned                  |
                                resources                      |
                                                               |
                 +----------------------+----------------------+
                 |                      |                      |
              published          definite conflict       outcome unknown
                 |                      |                      |
              cleanup            retain/resolve          resolve SAME attempt
                                 candidate                     |
                                              +----------------+----------------+
                                              |                                 |
                                        known published                known unpublished
                                              |                                 |
                                           cleanup                     retry/resolve SAME
                                                                       candidate/context

STAGING / PUBLICATION:
  snapshot S -> canonical objects admitted -> stage(workspace, branch, root R)
                                               |
                            transaction: validate expected head/base
                            + insert Commit + advance branch + delete exact stage
                                               |
                                     success or rollback
```

Existing `workspace_stages` stores a candidate root, not another filesystem copy.
Object admission precedes staging. The successful publication transaction retires
the stage record; committed objects remain referenced by history [S05]. A conflict
retains the stage. This mechanism should be reused; staging is not a second history
Commit and deleting its row is not deleting all admitted objects.

One existing stage per Workspace can remain initially if a retained attempt must
be resolved/abandoned before the next attempt begins. Queue acquisition must not
overwrite it. Do not reinterpret a request to abandon a staged candidate as
discarding live state. Existing APIs/leases and explicit discard behavior require
careful separation in the spec; no automatic conflict merge is introduced here.

Commit-attempt bookkeeping must bind snapshot boundary, expected branch context,
candidate root, outcome, and covered-change receipt. These are logical fields,
not a proposed durable schema. Publication uncertainty must be reconciled before
advancing that receipt or starting the next attempt. Missing stage alone cannot
prove publication succeeded. Recovery must use the exact candidate/expected
parent and authoritative Store state.

| Failure point | Preserved state and required handling |
| --- | --- |
| Mutation admission/backing write before install | Previous live root and payload remain readable; release or charge unused reservation. |
| Snapshot registration | No half-registered reader; live state continues. |
| Construction | Snapshot/current backing remains safe; discard partial construction through existing admission ownership where applicable. |
| Head/base conflict | Branch is not overwritten; staged candidate and attempt context remain identifiable; live mutation remains available. |
| Publication succeeds, reply lost | Resolve exact attempt; do not publish again from newer live state or clear newer changes. |
| Snapshot/stage cleanup fails | Preserve successful publication identity and charges; retry cleanup without a live checkpoint/reset. |

### D5. Localized mutation and localized Commit construction

```text
write/truncate                create/link/unlink/rename             chmod/mtime
      |                                  |                              |
affected file-range paths     affected binding + inode records      inode metadata
      +----------------------------------+------------------------------+
                                         |
                          atomic current state + change-index update
                                         |
                                  acquire snapshot S
                                         |
                      enumerate captured changes since covered boundary
                                         |
             +---------------------------+---------------------------+
             |                           |                           |
       distinct file tasks         sorted name deltas         metadata changes
             |                           |                           |
       file fast paths             directory updates          inode updates
             +---------------------------+---------------------------+
                                         |
                           reuse unaffected canonical structure
```

Snapshot consistency is Workspace-wide; construction locality is change-driven.
Do not acquire independent per-file snapshots while scanning and combine them.
FUSE supplies mutation observations; the dirty frontier and indexed namespace
deltas avoid discovery by full mounted-tree scan. SDK mutations use the same path.

The existing builder already has much of the lower half [S06/S07]. It still has
scans and vector materialization that must be audited rather than labelled O(1)
or uniformly local. A large operation may legitimately affect many bindings;
measured work should track that affected set, not unrelated namespace size.

Proposed continuation bookkeeping: a snapshot-consistent index of latest affected
keys, including removals, plus one covered boundary from the last resolved successful
attempt. Updating a key replaces current tracking, not an action log. An ordered
change index or subtree summaries must permit incremental enumeration; a filter
applied after scanning every historical key does not meet the target. Name length,
hardlink paths, directory ancestors, and newly assigned canonical identities also
need coverage. The mechanism is undecided (G3).

Physical content provenance and branch comparison context are separate: live
ranges may refer to the original immutable base even after C1 advances branch
history. C2 must enumerate only newer effects and reuse C1's output without
installing it into live files. A bounded/disk-backed correspondence mechanism or
equivalent incremental comparison is required. Blindly changing a `base_root`
field is incorrect; retaining the original base forever without incremental
correspondence can be correct but too expensive.

### D6. Ownership, residency, and reclamation

```text
OWNERS                                         RESOURCES
------                                         ---------
current overlay root -.---------------------> current metadata/ranges
Commit snapshot S -.------------------------> captured metadata/ranges
retained read plan -.-----------------------> requested older ranges
prepared operation -.-----------------------> old/new pages + reserved bytes
open-unlinked live inode -.------------------> orphan content, absent from namespace

all above -.-------------------------------> shared backing ownership/index
                                                      |
                                               cache page resident?
                                                /             \
                                              yes             no
                                               |               |
                                          evict to backing    disk-readable
                                               |
                                          ownership unchanged

last relevant owner releases -> reclaim eligible ranges/pages
                              -> release segments only when safe

candidate/stage -.---------------------------> admitted canonical objects
published branch Commit -.------------------> persistent reachable objects
```

Eviction is not reclamation. Snapshot release is not whole-spool deletion. Stage
retirement is not deletion of committed objects. There is no blanket two-version
bound: snapshots, arbitrary retained readers, operations, and shared segments can
retain different content. Account and limit those resources honestly.

Use bounded roots/handles for retained page graphs rather than recursively pinning
one in-memory record per changed inode. Page/range ownership for on-disk references
needs an explicit persistent-within-runtime index or equivalent algorithm. A
coarse snapshot epoch is simple but may retain unrelated overwritten bytes; it is
acceptable only if it meets declared disk/reclamation bounds under fixed-work tests.
Reclamation work itself needs a budget; dropping one root must not trigger an
unbounded synchronous traversal on a foreground operation.

### D7. Component interfaces and synchronization boundaries

```text
FUSE / SDK adapter
    |
    +--> prepare mutation -------> acquire backing / reserve resources
    |                                   |
    +--> install mutation <-------------+   short atomic visibility boundary
    |         |
    |         +-.-> current root and inode/namespace identity
    |
    +--> live reads ----------------------> immutable range/page access

Commit coordinator (one attempt per Workspace)
    |
    +--> acquire snapshot ----------------> root + ownership registration
    +--> build(snapshot reader) ----------> independent reads, bounded workers
    +--> stage/publish -------------------> existing Store conditional transaction
    +--> resolve coverage/outcome --------> Commit bookkeeping only
    +--> release snapshot ----------------> bounded reclamation scheduling

NO build-time arrow to: global operation freeze, live inode checkpoint install,
                       whole dirty reset, live mount traversal, remount/resume.
```

These are conceptual interfaces, not a new trait hierarchy or final API proposal.

| Responsibility | Inputs/output and ownership | Synchronization / I/O rule |
| --- | --- | --- |
| Operation adapter | Kernel/SDK requests, stable live IDs, bounded cache | Preserve normal permissions, errors, handles, and supported cache/mapping semantics; no independent mutable authority. |
| Mutation installer | Prepared affected records and exact backing ranges -> installed current state | Acquire/encode I/O outside short installation synchronization; revalidate revisions and admission before visibility. |
| Backing service | Pages, extents, retained buffers, budgets | Shared ownership; bounded queues and per-request work; no one mutex held for complete candidate construction. |
| Snapshot provider | Consistent root bundle -> owned immutable handle | Atomic registration vs mutation/reclaim; no scans or complete export/drain; G1/G2 prerequisites. |
| Snapshot reader | `lookup_inode`, `lookup_binding`, `read_range`, `iter_changes` concepts | Read captured state only; bounded page/range I/O; immutable base access retains authentication. |
| Candidate builder | Snapshot + expected branch comparison context -> admitted candidate root | Adapt existing builder/journals; separate content provenance from branch predecessor; release Workspace locks. |
| Attempt coordinator | Candidate, snapshot boundary, expected head/base -> resolved outcome | Per-Workspace serialization applies to Commit requests only; idempotent uncertainty handling. |
| Reclaimer | Released ownership and storage eligibility -> bounded cleanup | No data reuse before last owner; keep failed cleanup charged; avoid synchronous graph-wide drop. |

### D8. Data-format foundation: ranges to CAS, FULL/DELTA, and CDC

The input convergence below is part of D8. `[E]` identifies existing shared
machinery; `[P]` identifies the new Workspace input boundary. Init does not acquire
a Workspace snapshot, and Commit does not adopt Init's source-discovery scan.

```text
INIT [E]                                  WORKSPACE COMMIT [E -> P]
========                                  =========================
Source files / namespace                  Current overlay
          |                                     |
Discover initial input                    Acquire snapshot [P]
          |                                     |
Plan initial file tasks                   Enumerate captured changes [P]
          |                                     |
          +-----------------+-------------------+
                            |
                 SHARED CONSTRUCTION [E]
                 bounded producer/output driver
                 file-content encoding/builders
                 CAS/authentication and deduplication
                 existing packing/compression policies
                 bounded object admission
                            |
             +--------------+------------------+
             |                                 |
      initial namespace                 incremental namespace
      and inode assembly                and inode updates
             |                                 |
      existing Init                     workspace_stages
      publication                              |
                                        conditional branch Commit
```

This is shared machinery, not a claim that both entry points execute identical
functions for every file/namespace operation. Init's direct path calls
`run_finalized_output`; Workspace's `construct_workspace_files` calls the same
driver, with different task preparation and admission context [S14]. Preserve
those common internals and common namespace/inode formats/primitives. Retain
Commit-specific unchanged-content and incremental-edit fast paths. Do not force
the two planners through an unnecessary universal framework or fork a new
snapshot-only encoder/admission pipeline.

```text
LIVE TEMPORARY FILE
  metadata + [base ranges][spool replacements][inline/zero ranges]
                                 |
                          snapshot-owned reader
                                 |
           +---------------------+----------------------+
           |                     |                      |
     unchanged content      small-file path        large-file path
     reuse existing root    FULL/DELTA policy       CDC/extents + existing
                            and chain rules        incremental rope updates
           |                     |                      |
           +---------------------+----------------------+
                                 |
                   canonical objects / authentication
                   CAS identity + deduplication
                   existing packing/compression policy
                                 |
                        admitted candidate root
                                 |
                         workspace_stages
                                 |
                       published branch Commit
```

This is a logical representation diagram, not the literal internal call ordering
of hashing, encoding, and compression. Preserve the existing ordering/invariants.
Temporary pending ranges are not canonical objects; ordinary writes need not run
the complete committed pipeline. Small files are not always DELTA. Large-file CDC
does not imply whole-file rechunking. Some legitimate changes cross small/large
format boundaries and require complete construction; measure those separately.

Reuse the existing file builder [S08], including compact/direct spool reads,
unchanged-content checks, incremental updates when base correspondence permits,
predecessor handling, and bounded output admission. Existing sequential-new-file
precomputation is optional [S11]: bind reuse to exact captured identity/content,
and avoid live-state invalidation/join coupling or a blocking producer channel
that makes foreground writes wait for Commit. Do not generalize it to every
write without measured benefit.

## 3. Demonstrations and independent expected results

These are deterministic scenarios for specification and later tests, not results.
Use public FUSE/SDK paths and an independent byte/namespace oracle; inspect the
freshly reopened Store for published results. Barriers below hold the test builder,
not live filesystem operations.

| ID | Scenario | Required result / diagnostic value |
| --- | --- | --- |
| A | Write `A`, acquire S1, hold builder; append `x` on same fd; read `Ax`; release and publish; acquire S2. | C1 is `A`; C2 is `Ax`; fd offset/identity preserved; reads and writes complete while builder is held. |
| B | Queue Commit 2 during held C1; mutate again before C1 resolves. | S2 acquired only when C2 starts; captures final current effects then, no snapshot/queue multiplicity leak. |
| C | Hardlinks `/a` and `/b`; capture; write through `/b`; rename `/a`; unlink last name while fd retained. | S1 retains captured bindings/content; live aliases share identity; orphan fd remains usable; subsequent Commit does not resurrect orphan. |
| D | 1 GiB base; 4 KiB replacement X; capture; replace same range with Y. | Snapshot reads X, live reads Y; unchanged ranges shared. No 1-GiB overlay copy; allocated bytes include page/segment overhead, not just 8 KiB. |
| E | One million changed regular files; Commit; then ten further changed files. | First explicit Commit proves scale; second enumerates relevant newer work and reuses unchanged output. Report namespaces and affected ancestor records separately. |
| F | Fixed-count overwrites during held snapshot, with separate retained readers; release each owner in a known order. | Reclaim intermediate states not visible to any owner under declared policy; disk retention and deferred cleanup explained quantitatively. |
| G | Inject quota/short-I/O/install-race failures; separately lose publication reply and trigger a head conflict where supported. | Previous/current state remains readable; no duplicate publication; staged candidate is distinct from live state; next Commit context is resolved explicitly. |
| H | Supported buffered writes and writable mappings straddle acquisition; combine SDK edits with kernel-cached pages. | Exact documented snapshot visibility and continued operations. Failure is a G1 failure, not permission to flush/freeze globally or weaken the oracle. |

## 4. Complexity vocabulary and assumptions

Owner acceptance rule: no quadratic scaling with namespace, changed-set size or
operation history. Necessary complete scans/input/output processing may be linear;
point operations should follow logarithmic index paths, and snapshot ownership
registration must be constant-sized. Creating N files through O(log N) individual
updates totals O(N log N), not O(log N) or O(N); disclose residual sorting/index
factors and measure them. A bounded per-call maintenance budget does not excuse
quadratic total relocation. The #130 review found exactly that risk in the current
payload arena sweep and requires local free-space/tail-block reclamation instead.

Complexity below is conditional analysis, not measured results. Symbols separate
logical cardinality from resident structures and actual I/O. Byte-linear terms
describe processing volume; compression, hashing, authentication, and storage
latency retain their real implementation costs. Use actual tree heights until the
chosen balancing/page representation establishes stronger bounds.

| Symbol | Definition |
| --- | --- |
| N, Nc | Total namespace entries; materialized/cached live nodes in the current implementation. They are not interchangeable. |
| D, K | Distinct changed inodes relevant to one snapshot/Commit; changed namespace/attribute records to process. Include required reference/ancestor work. |
| A, L | Known alias/path references touched; total bytes of those names/paths examined or copied. |
| P, T | Pieces in an affected file; pieces intersected/removed/processed by the particular edit. |
| Hm, Hp, He | Metadata-index, piece-index, and canonical extent-tree heights. |
| U, R, C | New replacement bytes; all content bytes read (including equality/predecessor reads); bytes processed by CDC. These overlap and must not be summed as disjoint physical I/O. |
| I, Z | Canonical objects processed/admitted/reused; newly allocated persistent object/storage bytes under the existing format. |
| B, E | Metadata page size; number of affected/copy-on-write pages for a particular operation. |
| F, J, b | Registered payload segments; frontier/journal records; bounded pending run batch capacity. |
| M, W, Q | Declared per-component cache/buffer budgets; active construction workers; admitted in-flight requests. M is a budget vector, not total RSS. |
| V, X | Additional physical backing retained exclusively by snapshots/readers; temporary construction/merge scratch. Count shared allocations once. |
| G | Pages/ranges/segments visited by a particular reclamation pass. |

Analysis assumptions to prove in the implementation specification:

1. Root bundles and ownership registration are bounded and snapshot-visible pages
   already have stable read ownership. No implicit whole-set pin walk or flush.
2. Metadata/page indexes, current change tracking, payload ownership, and reference
   bookkeeping are disk-backed or budgeted; the cache can evict retained pages.
3. A bounded cursor replaces full piece-list allocation where fragmentation can
   exceed a transient memory allowance. Traversal stack/page buffers are charged.
4. The change index can skip covered keys without scanning all accumulated keys.
   Current reference-count/path consumers have been audited before removing data.
5. Long-lived readers/attempts count toward explicit storage budgets. No finite
   disk bound independent of arbitrary retained content is promised.
6. Foreground operations do not acquire a build-duration lock. Short synchronization
   contention, RPC latency, disk misses, and quota failure are measured separately.

### Time, I/O, and transient RAM by operation

| Operation | Existing fact or proposed target | RAM and I/O implications |
| --- | --- | --- |
| File mutation | Target work follows Hm + Hp + T + U plus required A/L; not unconditional O(log P). Existing piece split/merge shares paths, but adapters also enumerate backing/pieces in some routes. | New payload U; E copied metadata pages cost E*B; bounded buffers/cursor stack plus admitted old/new paths. Deferred freeing must not add an unbounded synchronous T/G cost. |
| Namespace mutation | Target affected-key work approximately O(K*Hm + L), conditional on parent/name representation. Current rename scans Nc nodes and known paths. | Charge names, atomic old/new records, caches, and any path-rewrite work. Large subtree semantics may legitimately increase K/L. |
| Snapshot acquisition | Target O(1) descriptor/registration work independent of N/D/P/U. Current exported fact capture and detached-map test are proportional to captured records. | O(1) handle memory only if ownership is already indexed; no acquisition-time payload copy. End-to-end latency includes protocol and synchronization, not assumed constant wall time. |
| First mutation after acquisition | Same locality target as an ordinary mutation; no O(N) or O(D) map clone. | Measure E*B and transient reservations separately from baseline writes. |
| Captured-change enumeration | Target O(D + K) yielded records plus actual index traversal; multiple bounded passes must be counted. | Streaming buffers; snapshot-consistent tombstones/identities. An O(Nc) identity-reservation scan is a current gap. |
| Changed-file piece traversal | Existing planning/comparison paths can take O(P) time and O(P) Vec RAM. Streaming target retains O(P) work when all pieces are necessary. | Cursor buffers + traversal stack replace O(P) transient allocation; do not promise O(U) work for every fragmented file. |
| Small-file construction | May read the complete threshold-bounded file and predecessor; codec/selection work is not O(U) merely because output is DELTA. | Per-worker small-file input is threshold-bounded; include aggregate W and predecessor buffers. |
| Large-file construction | Account P traversal, R reads/equality checks, C CDC bytes, and affected extent-tree nodes. Local incremental paths can reuse unchanged extents. | Bounded streaming buffers; a base-root mismatch may currently trigger full comparison/construction. Large/boundary transition cases must expose this. |
| Sorted metadata construction | Work follows D/K and actual canonical tree nodes visited/rewritten, including fallback paths. | Bounded sorted-update scratch; page-level I/O can exceed number of logical changed keys. |
| Frontier run merging | Current geometric sorted runs incur merge work; a conservative cumulative model is O(J log(1 + J/b)) record processing for batch-carry workloads, not O(J) for all patterns. | Bounded merge buffers and logarithmic run descriptors subject to limits; retain originals/carry/output during safe replacement and measure actual peak scratch. |
| Object admission/publication | Object handling scales with I and encoded bytes; existing bounded admission transactions followed by conditional publication. | W/Q/queues and Store writer contention matter. Metadata publication is not the whole Commit cost; no global construction lock. |
| Snapshot release/reclamation | Foreground ownership release can be small; total reclamation work is O(G) over visited resources, not O(1). Existing segment retirement scans F. | Budget/defer traversal; account descriptor/registry memory and physical allocation slack until release succeeds. |

For a simple page-based edit, additional temporary allocation is approximately
`U + E*B + allocation slack`, excluding already shared pages. This is an illustrative
model, not a guarantee for all tree layouts or ranges. Two roots do not imply two
full copies, and they also do not imply a strict two-record retention ceiling.

### Space and process-memory accounting

```text
temporary allocated space = current required payload + metadata backing
                          + V (snapshot/reader-exclusive payload + metadata)
                          + auxiliary ownership/allocation overhead
                          + X (construction and safe-merge scratch)
                          + allocation fragmentation / deferred-cleanup slack

host resident RAM         = metadata + dirty/payload caches
                          + admitted operation reservations and queues
                          + snapshot/reader/registry bookkeeping
                          + Commit worker buffers and journal scratch
                          + allocator/runtime/Store overhead

container resident RAM    = live adapter/kernel-reference metadata and caches
                          + retained append buffers and protocol queues
                          + active operation/runtime overhead

persistent growth         = Z under existing object/pack/encoding policies
                          + Commit/branch metadata and physical DB overhead
```

These temporary-space buckets are mutually exclusive. Required backing means
occupied encoded backing bytes, not logical file length; shared bytes are assigned
to current state once. Metadata/index bytes needed by current or captured views
belong in those first two buckets, with payload/metadata subtotals reported.
Auxiliary overhead excludes every byte already counted there; scratch is separate,
and unused allocation plus unowned bytes awaiting cleanup belong to slack. The
implementation must document its allocation-unit/occupancy measurement rather
than adding whole allocated pages and their internal slack a second time.

Kernel page cache and container cgroup accounting need their own observations;
application cache reservations are not a complete RSS/cgroup measurement. Avoid
double-counting physical bytes shared by live and snapshot roots. Conversely,
temporary spool bytes and their newly canonicalized Store representation may both
exist physically until safe reference substitution/reclamation; show both.

Existing spool descriptors scale with F, not just active writes. Because current
segments are immediately unlinked, closing their last descriptor destroys the
backing; they cannot simply be reopened through an FD cache. Bound retained segment
count under that layout, consolidate addressing into fewer shared backing files,
or explicitly choose reopenable private files with a safe cleanup protocol. This
is a backing-layout decision, not a free catalog optimization. Snapshot identity
alone does not remove descriptor pressure. Similarly, mapped metadata pages are
not free RAM merely because the index is on disk.

Stage insertion is a small root record, not a second payload copy. Admitted
candidate objects may persist while a stage is retained. Their physical cleanup
must follow existing Store ownership rules, not overlay reclamation assumptions.

## 5. Existing-code review: issues, gaps, and opportunities

Three independent read-only subagents reviewed lifecycle/staging, storage/FUSE
ownership, and performance/locality. Findings below were reconciled against the
rule document and source. No subagent changed product code or ran qualification.
This review does not establish benchmark performance or solve G1-G3.

| ID / priority | Existing evidence | Required change or simplification |
| --- | --- | --- |
| R01 / critical | `lifecycle.rs:504,539,550,598` holds lifecycle/workspace locks and freezes/quiesces; `changes.rs:360-389` holds backing lock through build; SDK edit takes lifecycle lock at `:811`. | An owned attempt and snapshot reader must remove every build-duration operation exclusion. Spawning the existing method on another thread is not isolation. |
| R02 / critical | `live_owner.rs:1280,1430-1452` acknowledges some writes while payload remains in container `PendingBytes::Filling`; freeze later flushes append at `:2676`. | Host current-root authority requires changed transfer/acknowledgment, or a bounded owned read path to retained buffers. Never assume acknowledged payload is already host-readable. |
| R03 / critical | `live_owner.rs:2585-2622` enumerates cached nodes, invalidates, and syncfs; `:2654` holds the operation cut. `filesystem.rs:37-47` does not explicitly request FUSE_WRITEBACK_CACHE, while open/create handling at `:721,:763,:1421` distinguishes cached/mmap-capable and direct-I/O behavior. | G1 must address actual supported mappings/cache behavior. Do not infer negotiated writeback mode from callback names, or declare host metadata sufficient. |
| R04 / critical | Core live maps at `lib.rs:421`; host `facts/incoming` at `live_backing.rs:19`; dirty-prefix export at `live_owner.rs:2778`. | Replace growing borrowed/mirrored maps as Commit input with snapshot-capable backing. Root handoff must not hide a later whole-prefix export. |
| R05 / critical | `lifecycle.rs:355` treats pending stage/publication as inactive; retained-stage uncertainty at `:151` freezes mutation; `staging.rs:45-49` rejects a different root for the same Workspace. | Retry the same immutable attempt/candidate; staged conflict may block the next Commit, not filesystem activity. Keep one-stage schema initially. |
| R06 / critical | `lifecycle.rs:133` passes live `base_root` as expected publication source; Store `workspace.rs:510` verifies it; checkpoint currently advances both contexts. | Split live content provenance from last-published root/head/base. Updating expected head alone breaks C2's publication-source validation. |
| R07 / critical | `changes.rs:1843-1882` requires matching file-base root for incremental update; mismatch calls full `file_matches`/construction. | G3 requires canonical/range correspondence outside live inode reset. An inode-to-root table or change token alone does not restore incremental CDC. |
| R08 / critical | Local install checks generation at `lifecycle.rs:276`; core `checkpoint.rs:97-99` resets mutation paths/dirty state; remote installation also requires a cut. | Remove mandatory live checkpoint completion from ordinary Commit. Reuse useful bounded result correspondence, not its obligation to overwrite live state. |
| R09 / locality | `changes.rs:563-593` scans materialized nodes to reserve new inode serials; `namespace.rs:776-794` scans materialized node paths on rename. | Track identity allocation high-water; consider parent/name-based path resolution. Audit reconciliation/path consumers before deleting path data. Current Commit/mutation costs are not uniformly O(D)/local. |
| R10 / allocation | `file_edit.rs:1101` creates all-piece Vecs; backing enumeration and file construction/comparison use them. File edits also touch known aliases at `:309-315`. | Use bounded range/piece cursors where needed; include P/T/A/L in analysis. Retain shared piece semantics rather than rewrite the whole content engine. |
| R11 / ownership | `backing.rs:13` uses Arc; `file_io.rs:734` scans/retains entire segments by uniqueness; spool admission uses high-water accounting. | Disk-resident records need explicit backing ownership. Separate logical, allocated, retained, and reclaimable bytes; bound F/registry/scan cost. |
| R12 / optimization | `capture.rs:129-135` may join a precompute worker during invalidation; producer sends through a bounded channel. | Keep only exact-snapshot-safe useful precompute; remove avoidable live-state coupling rather than generalize a new capture subsystem. |
| R13 / reuse | Detached candidate test at `changes.rs:3529` preserves earlier state after write/unlink but clones maps at `:3548-3564`. | Reuse semantic proof and builder boundary, not the O(D + selected references) snapshot implementation. |
| R14 / reuse | `workspace.rs:453-593` admits objects, stages a root, checks branch context, publishes and retires stage transactionally. | Preserve canonical staging/publication. A new stage database/history system is unnecessary for one unresolved attempt per Workspace. |

These priorities describe architectural risk, not newly reproduced production
bugs. The freeze-related behavior is deliberate in the existing model and must
be replaced coherently rather than patched piecemeal.

## 6. Simplification decisions for the next spec

### Change inventory: remove, replace, add, and preserve

This inventory concerns the ordinary Workspace Commit path. It is not permission
to delete similarly named mechanisms used by other operations. Implementation
must trace callers and validate affected Init and Workspace paths.

| Component or coupling | Disposition | Resulting boundary / qualification requirement |
| --- | --- | --- |
| Build-duration freeze/quiesce/resume | Remove from ordinary Commit | Snapshot acquisition and independent reads replace live exclusion; G1/G2 must be resolved first. |
| Build holding live Workspace/lifecycle/backing-map guards | Replace | Owned snapshot/attempt; only short acquisition/installation synchronization; FUSE and SDK operations must overlap. |
| Full dirty-fact export at Commit acquisition | Replace | Snapshot-addressable backing and bounded handoff; audit other fact consumers before removing protocol paths. |
| Mandatory checkpoint installation into live inodes | Remove from ordinary Commit | Publication does not overwrite live state, reset caches wholesale, or require presentation resume. |
| Global dirty/generation reset | Replace | Cover precisely the successful snapshot boundary; preserve post-snapshot changes and net-zero handling. |
| Pending stage/publication means inactive Workspace | Remove coupling | Attempt state controls Commit ordering/retry, not ordinary filesystem admission. |
| Snapshot-capable metadata/index and disk ownership | Add or adapt | Incremental pages/records with bounded cache/index/FD resources; no map clone or history log. |
| Owned snapshot reader | Add/adapt input boundary | Captured inode/name/range/change cursors feed the existing candidate builder. |
| Independent per-Workspace Commit attempt | Separate existing state into owned context | Expected canonical root/head/base, snapshot identity, candidate/stage, coverage, and uncertainty remain tied to one attempt. |
| Successive-Commit comparison and range correspondence | Add/adapt | Efficient C2 without live rebase; inode-to-root mapping alone is insufficient if range bases differ. |
| Checkpoint-result journal | Repurpose where useful | Retain bounded identity/content output for correspondence; eliminate its mandatory live-install consumer. |
| Shared Init/Commit construction and admission | Preserve | Same existing producer/output driver, file encoders and Store admission where applicable; distinct task and namespace planning remain. |
| CAS, small-file FULL/DELTA, large-file CDC, packing/compression | Preserve | Existing identity, encoding, selection, and incremental reuse; no per-update canonicalization requirement. |
| Staging and conditional publication | Preserve | Existing single-stage ownership and publication transaction; release snapshot and canonical stage under their separate lifetimes. |
| Backing retention and cleanup | Extend | Disk-resident roots/readers can own ranges independently of cached Arcs; cleanup remains accounted and bounded. |
| fsync, SDK cache coherence, reconciliation, explicit End/Discard | Audit and preserve required semantics | Do not delete gates/helpers indiscriminately; these are distinct consumers and do not justify a Commit-wide freeze. |

The completion simplification is:

```text
CURRENT                                   PROPOSED
-------                                   --------
publish                                   publish
  -> validate live generation               -> resolve exact attempt
  -> install committed live backing          -> advance expected branch context
  -> reset dirty state/caches                -> record covered snapshot boundary
  -> resume filesystem operations            -> release unneeded ownership

                                          current filesystem remains untouched
                                          by publication completion
```

The published comparison root/head/base advances even if live content continues
to reference its older immutable source. This is bookkeeping separation, not a
live checkpoint. Conditional canonical correspondence work must not become a
hidden synchronous rewrite of all live records on completion.

### Reuse without unnecessary abstractions

| Keep / simplify | Why | Guard against |
| --- | --- | --- |
| One snapshot reader feeding the existing builder | Preserves tested canonical construction and localized frontier logic. | A parallel diff/replay/encoding engine or FUSE-mounted snapshot. |
| One owned attempt per Workspace | Captures expected context, stage identity, coverage, and retries in one lifetime. | Treating pending stage as live inactivity or rebuilding a retry from current state. |
| Existing single `workspace_stages` row and publication transaction | Already separates admitted candidate from branch history. | Unnecessary multi-stage schema, candidate-byte copying, or dropping unresolved stage silently. |
| Reuse bounded checkpoint-result journal as correspondence where useful | Existing output contains live/canonical inode and content relationships. | Retaining global live installation merely because the journal is named Checkpoint; inode-to-root data alone is insufficient range provenance. |
| Existing prepare/acquire/apply mutation discipline | Revalidation and resource admission already avoid partial logical installation. | Moving all I/O under a root lock or adding a durable transaction per small update without need. |
| Existing range representations, compact forms, shared spool | Already avoid whole-file copy-up and excessive per-file physical ownership. | Serializing all P pieces after each edit or allocating one file/FD per inode. |
| Remove mirrored fact groups as Commit input once replaced | Avoid duplicate growing maps and full export at acquisition. | Deleting facts/control paths still needed by fsync, diagnostics, SDK coherence, or reconciliation without tracing consumers. |
| Remove ordinary Commit presentation checkpoint/resume states | Publication no longer changes live files or mount presentation. | Reusing destructive end/discard paths after Commit error; deleting distinct reconciliation semantics indiscriminately. |
| Track allocation/paths by stable identities where justified | Avoid reservation scans and global materialized-path rewriting. | Speculative new indexes without accounting, or breaking conflict/path reporting consumers. |

No new trait/factory hierarchy is prescribed. Four core responsibilities are
sufficient to start: live operation owner, snapshot provider/reader, backing owner,
and per-Workspace attempt coordinator feeding the existing Store. Diagram boxes
describe responsibilities; they need not become separate crates, services, or threads.

## 7. Authority and backing alternatives

Physical snapshot backing stays on the host under the rule. Logical installation
authority and cache protocol remain G2 decisions.

| Candidate | Simplification | Cost / proof obligation | Status |
| --- | --- | --- | --- |
| Host-authoritative current metadata; container adapter/cache | Single host root, direct snapshot reads, no full host fact mirror. | Mutation RPC/ack cost; adapter cache coherence; transfer/ownership of currently container-buffered bytes; G1 still unresolved. | Leading candidate to prototype, not selected. |
| Live-owner-authoritative metadata with continuously host-backed snapshot-addressable pages | Reuses operation ordering and more current foreground buffering. | Bounded root handoff plus coherent host-readable page/buffer ownership before capture; disconnect lifetime; cannot export mutable maps at Commit. | Viable alternative only with explicit non-draining protocol. |
| Current maps plus clone/export on Commit | Little initial code change. | O(D/P) capture/residency and freeze/visibility dependence violate rules. | Rejected. |
| Per-file copied snapshot directory or second mount | Familiar file layout. | Copies payload/namespace and adds mount/live-path ambiguity. | Rejected. |

For temporary metadata, evaluate a snapshot-capable page graph and existing
disk-index facilities before introducing dependencies. The Store's private
`SpillDiskIndex` already uses SQLite, but its scratch configuration uses
`journal_mode=OFF`, `synchronous=OFF`, and failure-disposable behavior [S12]. It
is not a drop-in transactional mutable overlay that preserves the previous valid
state after I/O failure. Reuse bounded buffers/index lessons, not unsafe assumptions.
A read transaction or copy-on-write root is an implementation option, not a proof
of acknowledgment visibility, bounded retention, or foreground latency by itself.

## 8. Minimal attempt and continuation context

The later spec should show these distinct logical contexts, without prescribing a
public API or durable Workspace version format:

| Context | Information | Lifetime |
| --- | --- | --- |
| Live view | Current inode/namespace/range roots, stable IDs, active handles and backing provenance | Workspace lifetime, modified only by filesystem operations/equivalent backing substitutions. |
| Snapshot | Captured root bundle, readable backing ownership, captured change-boundary identity | One attempt/read lifetime; no branch history entry. |
| Published comparison context | Last resolved successful canonical root, parent/head and base-layer context, covered boundary, bounded correspondence references | One current comparison context, advanced without live checkpoint; not an unbounded list of Workspace versions. |
| Commit attempt | Snapshot, expected branch root/head/base, admission/stage candidate, outcome/uncertainty, result correspondence | Until outcome and retained resources are resolved. |

Publish against the attempt's expected canonical root, not whatever immutable
root happens to remain in a live piece. Bind covered-change advancement to exactly
the published snapshot. Advance on a verified `UpToDate` outcome as appropriate
without fabricating a Created Commit; preserve later changes. Uncertain outcomes
must not advance coverage speculatively.

If a canonical candidate can be retained independently in Store, release raw
snapshot ownership when no retry/correspondence task needs it. Do not keep a large
raw snapshot pinned for the entire lifetime of a staged conflict by default.
Conversely, a published outcome does not authorize releasing backing still used
by live pieces or readers. End/Discard must explicitly coordinate outstanding
attempts, publication uncertainty, readers, and physical ownership; this is separate
from ordinary non-destructive Commit.

## 9. FUSE visibility gate in detail

The source supports kernel-cache coordination and writable mappings, but the
requested FUSE init flags do not explicitly include `FUSE_WRITEBACK_CACHE` [S13].
Do not derive negotiated kernel behavior solely from a `writeback` variable or
callback. Direct-I/O create handles and later cached/mmap-capable opens differ.

There are at least three visibility locations to reconcile:

```text
application-private buffers     not yet filesystem state
kernel cached/mapped changes    may be filesystem-visible without host data
live-owner append buffers       some acknowledged writes not yet sent to host
host-readable installed state   eligible only under the chosen visibility contract
```

The current freeze drains cache/append/facts; that does not establish a permitted
replacement. Root capture on the host proves only host metadata consistency until
the first two filesystem-visible gaps are addressed. Host authority, write-through
for ordinary calls, or direct I/O alone must not be claimed to solve all supported
mapping behavior. Do not silently disable mmap or redefine successful writes as
excluded. Define and prove the supported kernel/adapter boundary before declaring
the fast non-freezing acquisition algorithm implementable for the full surface.

The later specification must include negotiated capability evidence, exact
acknowledgment ordering, dirty-map/page ownership, SDK/kernel coherence, failure
propagation, and an overlap test oracle. If no candidate satisfies the rule, report
the conflict as an unresolved design issue; do not fall back to a global freeze.

## 10. Correctness evidence and existing-benchmark evaluation

Owner decision, 2026-09-14: implement the model correctly, then run the applicable
existing benchmarks and inspect the results. The table below maps evidence needs
to that evaluation, not a new mandatory benchmark campaign or numeric performance
gate. Preserve existing contracts, fixtures, budgets, identities and reporting.
Use focused additions only for demonstrated coverage gaps. Follow
[benchmark rules](../../../general/benchmark_rules.md) and applicable
`benchmark/AGENTS.md` when implementing/running benchmark work.

| Workload | Primary correctness gate | Performance/resource evidence |
| --- | --- | --- |
| Small ordinary edit/read workloads, no snapshot | Existing supported behavior unchanged | Matched baseline/candidate foreground latency, CPU, RPCs, page/spool I/O. |
| Affected Init paths when shared machinery changes | Preserve supported identity/format semantics and existing fast paths | Matched Init construction/admission/locality/resource checks; compare like identity and predecessor contexts, not arbitrary cross-entry-point root equality. |
| Same work while builder held after acquisition | Public operations actually complete before builder release | Acquisition end-to-end and lock durations; foreground p50/p95/p99 through build/stage/publish/cleanup. |
| First mutation after capture | Correct COW separation | Copied pages/bytes, transient RAM; no delayed whole-map copy. |
| Wide unchanged namespace, small changed subset | Exact content/metadata, no unchanged edit descriptors | Nc/N vs D/K visits, path scans, identity reservation visits, index/cache residency. |
| At least 1,000,000 changed regular files, one Workspace, one final Commit | Exact declared verification before and after fresh reopen | Fixed declared RAM/disk/deadline, total names vs changed inodes, RSS, index/owner/FD bounds, edit/read/Commit time. |
| Large C1 then small C2, repeated | Correct captured states/branch parents and post-S1 changes | Covered records skipped; base-match vs full-hash/full-build counts; R/C/I and piece cursor peak. |
| Same-range overwrite, disjoint edits, held readers | Correct final/snapshot bytes and owner lifetimes | Current/shared/snapshot-only physical bytes, reclamation backlog/age, FDs, largest partially live segment. |
| Threshold transitions and large files | Existing FULL/DELTA/CDC selection and exact decoded output | Complete small-file reads, incremental large-file reuse, predecessor worker limits, codec and admission costs. |
| Multiple Workspaces and Store publication | Isolation and existing lease/publication rules | Aggregate host/container budgets, Store writer wait, admission batch time, foreground fairness. |
| Failure and explicit cleanup cases | No partial live install, duplicate history, or premature reclaim | Retained charges, cleanup retry, bounded queues and unsettled attempt lifetime. |

| Metric family | Required counters/observations |
| --- | --- |
| Snapshot | Nodes enumerated, pages/bytes copied, ownership records created, RPCs, lock-held/wait time, total acquisition latency. |
| Locality | Changed inode/binding count, total materialized-node/path visits, sorted-tree visits/fallbacks, aliases touched. |
| Content | New bytes, base/predecessor/equality reads, CDC bytes, object reuse/admission, full-file fallback counts. |
| Memory | Host/container RSS, cache/dirty/index/pin reservations, Q/W buffers, cursor stack, queue peaks; kernel/cgroup observations separately. |
| Storage | Logical/current/shared/snapshot-only bytes, allocated bytes, metadata overhead, temporary merge amplification, descriptors, reclamation backlog and delay. |
| Pipeline | Producer blocking, consumer idle time, actual workers, Store transaction durations/wait, publication and cleanup durations. |

No new numerical performance thresholds are to be selected at this stage.
Architectural requirements (no live freeze, no acquisition walk/copy, exact
snapshots, no overwritten newer state) remain correctness/design obligations.
Existing benchmark results should inform later optimization decisions.
Retain misses/failures. No extrapolated descriptor arithmetic proves million-file
support, and no ordinary fs-bench row with closed writers proves overlap.

Preserve #122's declared regular matrix and deadlines. Add/version the non-pausing
overlap cases explicitly; the million-file test is separately selected. Existing
plans requiring helpers to finish before Commit are historical workload contracts,
not permission to reintroduce forbidden product barriers.

## 11. Rule traceability

| Rule section | Architecture coverage | Remaining proof |
| --- | --- | --- |
| 1: terminology/current state | D1-D4, contexts in section 8 | No per-update/Commit Workspace checkpoint or retained action history. |
| 2: non-pausing lifecycle | D3/D4/D7, R01/R08 | End-to-end FUSE/SDK overlap through publication/cleanup. |
| 3: snapshot and sequential Commit | D3, demonstrations A/B, section 8 | Exact visibility/ordering and queued capture timing. |
| 4: host/container placement | D1, section 7 | G1/G2; root and buffered-byte ownership protocol. |
| 5: storage efficiency | D2/D6/D8, section 4 | Disk reference accounting and bounded fragmentation/reclamation. |
| 6: fast acquisition | D2/D7, complexity assumptions | No hidden full export, pin walk, flush, or first-write clone. |
| 7: locality and canonical format | D5/D8, change inventory, R06/R07/R09/R10 | Shared Init/Commit machinery, affected Init regressions, G3 and successive-Commit reconstruction counters. |
| 8: publication/failure | D4, section 8, stage lifecycle | Immutable-attempt retry, stage independence, expected-root separation, uncertainty resolution. |
| 9: memory/FUSE visibility | D6/D7, sections 4/9 | Full budget/ownership model and mapping visibility. |
| 10: evidence | Demonstrations and section 10 | Existing benchmark evaluation after correctness, not performed here; no new numerical gates. |
| 11: open decisions | G1-G3, sections 7/12 | Explicit decision records before implementation. |
| 12: reuse references | Source register below | Trace actual callers before editing shared mechanisms. |

## 12. Next specification decisions and implementation ordering

1. Resolve G1/G2 together: actual kernel visibility plus metadata/buffer authority.
   Compare candidate protocols with bounded small-operation work; do not start
   by removing freeze or adding a host snapshot of existing stale facts.
2. Select the temporary metadata/index and disk ownership mechanism. Specify page
   identity, atomic root installation, failure-safe spill, bounded cache admission,
   snapshot read access, and reclamation, including descriptor limits.
3. Specify immutable attempt context, publication-source separation, stage retry,
   and explicit End/Discard coordination. Preserve single-stage and branch leases.
4. Resolve G3: captured change enumeration plus range/canonical correspondence.
   Include new-inode canonical identity and rename/path consumers. Keep the
   existing bounded journal output if it covers the necessary mapping.
5. Specify adapters to the existing builder using bounded inode/name/piece cursors.
   Remove mandatory live checkpoint installation only after equivalent snapshot
   and publication correctness paths are defined. Avoid duplicate content engines.
6. Require exact public-path overlap, C1/C2, failure and resource correctness.
   After correct implementation, evaluate with applicable existing benchmarks and
   inspect actual results. Missing coverage may justify focused extensions; no new
   numerical gate or replacement benchmark suite is required now.

Unselected details include page sizes, database/index choice, cache budgets,
transfer batching, kernel visibility mechanism, background replacement/reclamation
policy and queue capacity. New numerical timing targets are not required now. The architecture is useful
as a foundation precisely because these uncertainties are exposed instead of
disguised as completed boxes in D1.

## 13. Source register and review provenance

Links are repository-relative; line labels refer to the reviewed source commit.
Line numbers will move after implementation. Reviewers: `review_commit`
(lifecycle/staging), `review_storage` (ownership/FUSE), and `review_performance`
(locality/complexity). Their convergent findings are incorporated in section 5;
their read-only review is not an independent runtime proof.

| ID | Source evidence |
| --- | --- |
| S01 | [workspace lifecycle](../../../../crates/layerfs-workspace/src/lifecycle.rs): `commit_workspace_session_with_status` at 499, `commit` at 36, `ensure_active` at 355, checkpoint at 260, SDK edits at 798. |
| S02 | [core live state](../../../../crates/layerfs-workspace-core/src/lib.rs): frozen map view at 123, live maps at 421; [checkpoint](../../../../crates/layerfs-workspace-core/src/checkpoint.rs): install at 31, reset at 64. |
| S03 | [piece edits](../../../../crates/layerfs-workspace-core/src/file_edit.rs): shared nodes at 451, compact forms, admission at 321, piece Vec at 1101; [backing](../../../../crates/layerfs-workspace-core/src/backing.rs): Arc ownership at 13; [spool](../../../../crates/layerfs-workspace/src/file_io.rs): segment at 122, reserve/retire at 688/734. |
| S04 | [live owner](../../../../crates/layerfs-fuse/src/live_owner.rs): buffered writes at 1280-1452, cache flush/freeze at 2580/2654, facts at 2687, checkpoint at 3221; [operation gate](../../../../crates/layerfs-fuse/src/live_runtime.rs): 277. |
| S05 | [Store workspace](../../../../crates/layerfs-layerstack-store/src/workspace.rs): candidate staging/publication at 423; [staging](../../../../crates/layerfs-layerstack-store/src/staging.rs): 24; [schema](../../../../crates/layerfs-layerstack-store/sql/schema/v10.sql): workspace_stages at 137. |
| S06 | [candidate construction](../../../../crates/layerfs-workspace/src/changes.rs): remote borrowed maps at 354, detached inputs at 550, identity scan at 563, tasks/workers at 640, dirty frontier at 699, detached test at 3529. |
| S07 | [directory frontier](../../../../crates/layerfs-workspace/src/changes.rs): 850, frontier spill at 2175; [sorted tree engine](../../../../crates/layerfs-content/src/tree/batch.rs): directory update at 1364; [namespace mutation](../../../../crates/layerfs-workspace-core/src/namespace.rs): rename/path scan at 710/776. |
| S08 | [FrozenFile](../../../../crates/layerfs-workspace/src/changes.rs): 1717, build at 1762, correspondence/equality paths at 1843-2014; [rope edit](../../../../crates/layerfs-content/src/file/rope/edit.rs): incremental mutation; [object construction](../../../../crates/layerfs-layerstack-store/src/objects.rs): bounded workspace pipeline at 4473. |
| S09 | [host backing](../../../../crates/layerfs-workspace/src/live_backing.rs): facts/incoming at 19, fact admission at 103, backing service at 1841; [projection](../../../../crates/layerfs-workspace/src/projection.rs): attach at 22, materialized capture at 101, pause/resume at 175/351. |
| S10 | [SDK runtime](../../../../crates/layerfs-sdk/src/client.rs): 68; [Workspace placement](../../../../crates/layerfs-workspace/src/lifecycle.rs): 417; [daemon mount](../../../../crates/layerfs-daemon/src/main.rs): 1390. |
| S11 | [sequential precomputation](../../../../crates/layerfs-workspace/src/capture.rs): write at 44, finish/invalidate/take at 96/127/139; this existing name does not mean constant-time snapshot acquisition. |
| S12 | [temporary object spill](../../../../crates/layerfs-layerstack-store/src/objects/spill.rs): scratch configuration at 695, disk index at 719; not a selected live-overlay implementation. |
| S13 | [FUSE implementation](../../../../crates/layerfs-fuse/src/filesystem.rs): init flags at 17-60, open handling around 721/763, create handling around 1421; [mount setup](../../../../crates/layerfs-fuse/src/host_mount.rs): 88. |
| S14 | [Init construction](../../../../crates/layerfs-layerstack-store/src/layerstack.rs): shared `run_finalized_output` at 1318 and admission at 1434; [shared output driver](../../../../crates/layerfs-layerstack-store/src/objects.rs): 372, Workspace construction/admission wrapper at 4473; [Workspace caller](../../../../crates/layerfs-workspace/src/changes.rs): 661. |

Document validation checks local links, code-fence balance, diagram IDs, rule
traceability, and whitespace. Product tests, live FUSE overlap tests, and performance
measurements are outside this drafting turn and remain required qualification.
