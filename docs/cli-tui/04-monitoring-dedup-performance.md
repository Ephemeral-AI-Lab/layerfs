# LayerFS Monitoring, Deduplication, and Performance

Status: **binding Phase One backend/Phase Two presentation contract**
Audience: `layerfs-monitor`, `layerfs-sdk`, `layerfs-cli`, `layerfs-tui`,
Store, Workspace, projection, and benchmark implementers

This document defines the observability model for a topology containing one
LayerStore, zero or more StackStores, zero or more BranchStores, and zero or
more ephemeral Workspaces per BranchStore. It covers both a selected direct
route and a selected stacked route:

```text
direct route                         stacked route

BranchStore -> LayerStore            BranchStore -> StackStore -> LayerStore
    2 physical databases                 3 physical databases
```

The monitoring contract has five non-negotiable properties:

1. it measures the current CAS/CDC/missing-only algorithms without putting a
   second algorithm on the hot path;
2. it never presents independent database placements as duplicate rows in one
   imaginary global CAS;
3. it reports every CLI/SDK operation's elapsed time, including queue wait;
4. it adds no metrics, session, transfer, log, retry, recovery, or GC table to
   LayerStore, StackStore, or BranchStore;
5. monitoring is implemented by the in-workspace `layerfs-monitor` crate, not
   inside `layerfs-sdk`, a separate repository, or a generic telemetry daemon.

The Store databases contain product truth. Metrics and execution logs are
bounded local observations outside those databases.

---

## 1. What is measured

```text
CLI request
   |
   +-- parse/context/open
   |
   `-- SDK operation
          |
          +-- queue
          +-- preflight/validation
          +-- membership negotiation
          +-- payload/fact transfer
          +-- authentication/admission
          +-- SQLite transactions/commit-sync
          +-- ref/head CAS
          `-- projection/final-delta/cleanup, when applicable
```

Monitoring has three scopes. They must not be collapsed into one percentage
or one memory number.

| Scope | Examples | Source |
|---|---|---|
| Durable Store | unique CAS bytes, DB/WAL size, inserts, transfers, transactions | Store counters, SQLite aggregate on open/on demand, filesystem metadata |
| Workspace | COW memory, spool, materialized bytes, dirty paths, projection work | `layerfs-workspace` and projection counters |
| Execution/runtime | command output, child CPU, process/container memory, elapsed time | Domain raw receipts plus `layerfs-monitor` and OS/container accounting |

### 1.1 Cardinality and aggregation

```text
1 LayerStore
|-- 0..N StackStores
|     `-- 0..N BranchStores
`-- 0..N direct BranchStores
      `-- each BranchStore has 0..N Branches
             `-- each Branch has 0..N active Workspaces
                    |-- host directory: FUSE/materialized
                    `-- Docker/OCI: thin FUSE projection
```

Database metrics are per physical database first, per selected route second,
and topology-wide only when explicitly requested. Workspace metrics are per
Workspace first and may then be accumulated by Branch. A shared container is
not a physical database and a physical database is not a Workspace.

### 1.2 Implementation ownership

`layerfs-monitor` is a small LayerFS-specific production crate in this Cargo
workspace. It is not a service, plugin system, telemetry framework, or second
source of Store truth.

```text
domain crates                         layerfs-monitor
-------------                         ---------------
raw receipts/snapshots  ----------->  operation envelope and timing spans
                                      exact dedup formulas
                                      route union/placement aggregation
                                      CPU/RSS sampling
                                      bounded collector + JSONL retention
                                      presentation-ready snapshots
                                                  |
                                                  v
layerfs-sdk  -- composes operations and records observations only
                                                  |
                                                  v
layerfs-cli  -- commands/human/JSON output -> layerfs-tui -- visual UI
```

| Owner | Owns | Must not own |
|---|---|---|
| Domain crates (`layerfs-content`, `layerfs-storage`, Stores, `layerfs-workspace`, FUSE/materialization) | Raw content-build counters, object/fact admission receipts, transaction/transport facts, Workspace/session/projection resource snapshots | Percentages, route aggregation, JSONL retention, TUI presentation |
| `layerfs-monitor` | Operation envelope, monotonic span tree, formulas, route analysis, OS/container samples, fixed-memory summaries, bounded retention, presentation-ready snapshots | Store mutation, CAS/CDC, transfer decisions, Workspace lifecycle, CLI parsing, Ratatui |
| `layerfs-sdk` | Compose semantic operations, pass a monitor recorder through the call, attach domain receipts to that recorder | Monitoring formulas, collectors, samplers, persisted monitoring state, monitor snapshots implemented locally |
| `layerfs-cli` | Invoke SDK and monitor queries, render standalone text/JSON | Recalculate dedup/placement formulas |
| `layerfs-tui` | Render monitor snapshots and operation streams | Direct SDK/Store access or monitoring arithmetic |

Foundation and Workspace ownership are fixed: `layerfs-content` emits raw
canonical-build/CDC counters; `layerfs-storage` emits admission/transaction/
transfer receipts; the one `layerfs-workspace` emits COW/spool/session/
projection/execution snapshots. No second Workspace/Core/Overlay package
exists, and Monitor preserves these domains rather than flattening them.

Dependency direction is one-way:

```text
layerfs-cli -> layerfs-sdk -> layerfs-monitor -> raw domain receipt types
layerfs-tui -> layerfs-cli
```

If raw receipt types would create a dependency cycle, keep their minimal value
types in the producing domain crate and let `layerfs-monitor` adapt them.
Do not move Store algorithms upward and do not make a domain crate depend on
`layerfs-monitor` merely to emit a plain receipt.

Minimal crate ownership:

```text
crates/layerfs-monitor/src/
|-- lib.rs           # declarations/re-exports only
|-- operation.rs     # operation envelope, IDs and outcomes
|-- timing.rs        # monotonic span tree and fixed histograms
|-- dedup.rs         # exact covered-byte/set formulas and hero snapshot
|-- route.rs         # streaming DB union/placement analysis
|-- resource.rs      # host/process/container CPU/RSS sampling
|-- collector.rs     # bounded live receipts/gauges/Branch aggregation
|-- retention.rs     # bounded JSONL rotation
`-- snapshot.rs      # presentation-ready frontend-neutral values
```

Do not add exporters, a daemon, networking, plugins, a metrics database, or a
generic instrumentation facade. Extract those only for a real external
consumer later.

---

## 2. Terminology and exact byte domains

The word `bytes` is meaningless unless the domain is named. All public fields
therefore use explicit suffixes.

| Term | Exact definition |
|---|---|
| `canonical_object_bytes` | `length(objects.bytes)` for complete stored canonical objects, including structural objects |
| `payload_object_bytes` | Canonical byte-object payload subset; useful for package/install explanations but not a replacement for total CAS bytes |
| `typed_fact_bytes` | Encoded Commit/Branch/Layer/Stack/history/AddResult fact bytes transported or admitted; never mixed with object bytes |
| `logical_file_bytes` | User-visible file lengths in a selected root/roots; may reference the same canonical object repeatedly |
| `db_file_bytes` | Logical file length of the SQLite main database |
| `db_allocated_bytes` | Filesystem blocks allocated to the SQLite main database |
| `wal_file_bytes` | Logical length of the current SQLite WAL |
| `wal_allocated_bytes` | Filesystem blocks allocated to the WAL |
| `shm_allocated_bytes` | Allocated bytes for SQLite SHM, shown separately because it is runtime coordination state |
| `workspace_spool_bytes` | Allocated bytes in Workspace-owned transient spool files |
| `workspace_materialized_bytes` | Allocated bytes in a host/container materialized or cloned presentation |
| `workspace_owned_memory_bytes` | Memory buffers and indexes exclusively owned and counted by one Workspace |
| `rss_bytes` | OS process/container resident set; includes shared/unattributed memory and is never relabelled as Workspace-owned memory |

For local files, show both logical and allocated size. On Unix, allocated size
is `st_blocks * 512`; sparse or clone-backed files make logical size an
unreliable disk-cost estimate. A capable remote Store returns its own
DB/WAL/SHM snapshot and sorted paged `(ObjectId, encoded_length)` inventory
through the internal read-only observation capability. If unsupported, the
client reports `unavailable`, disables exact route analysis for that coverage,
and never infers remote physical size from transferred bytes.

### 2.1 Store inventory

For physical Store `d`:

```text
S_d = set of ObjectIds in d.objects
size(x) = length of the canonical bytes for ObjectId x

unique_cas_objects(d) = |S_d|
unique_cas_bytes(d)   = sum(size(x) for x in S_d)

store_allocated_bytes(d)
    = db_allocated_bytes(d)
    + wal_allocated_bytes(d)
    + shm_allocated_bytes(d)
```

`ObjectId` is the primary key, so `unique_cas_bytes(d)` is already deduplicated
inside that physical database. It includes currently unreachable immutable
objects because GC is deliberately out of scope. The UI labels it **stored
unique CAS**, not **reachable CAS**.

Do not calculate `metadata bytes` as `db_file_bytes - unique_cas_bytes`. That
difference also contains SQLite pages, indexes, free space, record headers,
and possibly WAL effects. It may be displayed only as **database envelope
overhead**.

### 2.2 Selected-route placement

For the databases `D` in one selected direct or stacked route:

```text
U_route = sum(unique_cas_bytes(d) for d in D)

S_union = union(S_d for d in D)
U_union = sum(size(x) for x in S_union)

cross_store_placement_bytes = U_route - U_union
placement_factor            = U_route / U_union       when U_union > 0
```

These are placement measurements, not deduplication failures:

```text
placement_factor = 1.00x   each canonical object exists in one route DB
placement_factor = 2.00x   the same object set is placed once in both DBs
placement_factor = 3.00x   the same object set is placed once in all three DBs
```

A BranchStore must own private changed objects before publication; a
StackStore/LayerStore must own the objects it serves. Therefore a required
placement in each independent database is expected. LayerFS does not claim
cross-database physical deduplication.

`S_union` is an expensive, on-demand analysis. Implement it with sorted
`(ObjectId, length(bytes))` cursors and a streaming k-way merge using `O(|D|)`
application memory. Never load every ObjectId into a Rust set and never add a
global CAS registry. A size mismatch for one ObjectId is an integrity error.

Because deferred GC permits unreachable immutable objects, inventory placement
must not be labelled **required copies**. A separate future reachable-roots
analysis may label its result **reachable placement**, but it is not part of
the live monitor or this implementation gate.

---

## 3. CAS/CDC deduplication model

The identity pipeline is unchanged:

```text
new/replacement bytes
    -> FastCDC 8/16/32 KiB
    -> canonical object encoding
    -> ObjectId::for_bytes(canonical bytes)
    -> objects(object_id PRIMARY KEY, bytes)
```

Transfer does not run CDC and does not re-encode. It announces identities,
receives exact missing bitmaps, and sends stored canonical bytes only for the
receiver-missing set.

### 3.1 Exact transfer set equations

Object and typed-fact domains are recorded independently. For objects:

```text
A_o = unique ObjectIds announced at object membership boundaries
E_o = announced ObjectIds already present at the receiver
M_o = receiver-missing ObjectIds
S_o = ObjectIds whose canonical bytes the sender sends
I_o = ObjectIds newly inserted by the receiver
R_o = ObjectIds that became present after negotiation but before insert

A_o = E_o disjoint-union M_o
S_o = M_o
M_o = I_o disjoint-union R_o
```

The same equations apply separately to every typed fact kind. They are never
proven by adding object and fact IDs into one counter.

Exact byte equations are always available for bytes that crossed or reached
admission:

```text
sent_object_bytes
    = missing_object_bytes
    = inserted_object_bytes + raced_existing_object_bytes
```

The current protocol prunes a complete subtree when its root is already known.
It correctly avoids reading the descendants at the sender. Consequently, the
fast path often does not know the encoded byte total beneath the pruned root.
Monitoring must not enumerate that subtree merely to manufacture a percentage.

### 3.2 Three distinct reuse metrics

#### A. Local CAS storage reuse

When Commit/Add already constructs the candidate canonical objects, record:

```text
local_candidate_object_bytes
local_inserted_object_bytes
local_reused_object_bytes
    = local_candidate_object_bytes - local_inserted_object_bytes

local_storage_reuse_rate
    = local_reused_object_bytes / local_candidate_object_bytes
```

This rate is exact only when candidate coverage is complete and the candidate
set is identity-deduplicated before summing. If only changed objects were
constructed, label the scope **changed-set reuse**, not whole-filesystem reuse.

#### B. Transfer payload avoidance

When an exact eligible source baseline `C_o` was measured:

```text
transfer_avoided_object_bytes = C_o - sent_object_bytes
transfer_avoidance_rate       = transfer_avoided_object_bytes / C_o
```

Receipt field `candidate_coverage` is one of:

| Value | Meaning | May show a byte rate? |
|---|---|---:|
| `full_closure` | Complete eligible canonical closure was measured | Yes |
| `changed_set` | Only locally constructed/changed candidates were measured | Yes, explicitly scoped |
| `negotiated_frontier` | Only announced frontier identities are known; pruned descendants omitted | No |
| `not_measured` | No byte baseline | No |

The normal missing-only transfer should use `negotiated_frontier` or
`not_measured`. Exact `full_closure` is for a benchmark or explicit
`monitor dedup --analyze` run; it may scan closures and is never automatic.

Without an exact byte baseline, live monitoring shows:

```text
announced object IDs
preexisting announced IDs = announced - missing
known roots/subtrees pruned
sent object bytes
inserted object bytes
raced object bytes
transfer byte rate = not measured
```

It must not substitute an ID ratio for a byte ratio.

#### C. Receiver admission reuse

```text
receiver_admission_reuse_rate
    = raced_existing_object_bytes / sent_object_bytes
```

This measures concurrent receiver races, not CAS/CDC effectiveness. It is
normally zero and belongs in diagnostics, not the primary dedup card.

### 3.3 Topology storage is not a dedup rate

The route-level values below are shown together but never merged:

| Value | Answers |
|---|---|
| `local_storage_reuse_rate` | How much candidate content reused rows in one physical DB? |
| `transfer_avoidance_rate` | How much eligible source payload did one transfer avoid sending? |
| `unique_cas_bytes(d)` | How many canonical bytes are physically stored once in DB `d`? |
| `placement_factor` | How many independent DB placements exist across the selected route? |
| `store_allocated_bytes(d)` | How much disk is allocated to DB/WAL/SHM? |

The TUI must never display one unlabeled **Dedup 90%** number.

### 3.4 Flagship equivalent-install contract

The primary dedup experience is deliberately concrete:

> Ten equivalent `npm install` results committed through ten Workspaces in one
> BranchStore collapse to one canonical package payload set in that
> BranchStore, then to one independent copy in every downstream Store that is
> required to serve it.

`equivalent` means the measured payload category resolves to the same
canonical ObjectId set under the declared coverage. Merely running the same
command is not equivalence: platform-specific packages, lockfile changes,
paths, modes, or non-deterministic output may legitimately create different
objects.

For `N` equivalent committed candidates with complete measured payload bytes:

```text
C = sum(candidate_payload_bytes for all N commits)
U = bytes(union of candidate payload ObjectIds across all N commits)

saved_bytes     = C - U
savings_rate    = saved_bytes / C
collapse_factor = C / U
set_collapse    = N candidate sets -> 1 canonical set
```

`U` is cohort content overlap, independent of whether the Store already held
some/all of that union before the selected window. Actual new Store allocation
remains the separate local-admission `inserted_object_bytes`; it may be zero
for a completely preexisting payload without making `collapse_factor`
infinite or changing the cohort's `N -> 1` statement.

For `N = 10` and equal-size candidates, the flagship values are:

```text
90% saved        10 -> 1        9 * one-install bytes saved        10.0x
```

All four hero values must be derived from the same numerator, denominator,
category, and coverage. If coverage is not `full_closure` or a clearly named
complete payload category, the hero card says **not measured** rather than
showing 90%.

Presentation-ready data owned by `layerfs-monitor` contains at least:

```text
EquivalentWorkloadDedupSnapshot
    workload_label                 "npm install"
    candidate_runs                 10
    equivalent_canonical_sets      1
    byte_category                  "package_payload"
    candidate_bytes                1,000 MiB
    unique_bytes_per_required_db   100 MiB
    saved_bytes                    900 MiB
    savings_rate                   0.90
    collapse_factor                10.0
    coverage                       full_closure
    stores[]
        role                       branch | stack | layer
        required_for_selected_route true
        canonical_sets_present     1
        unique_payload_bytes        100 MiB
```

Required physical placement is rendered beside, never inside, the savings
rate:

```text
direct:  10 -> 1 in BranchStore; 1 Branch + 1 Layer placement = 2 copies
stacked: 10 -> 1 in BranchStore; 1 Branch + 1 Stack + 1 Layer = 3 copies
```

The route's total stored payload may therefore be about `2U` or `3U` even
though the within-DB result is `10 -> 1`. The hero view must show both truths.

---

## 4. Worked examples

Let one deterministic installation produce a 100 MiB canonical package
payload set `Q`, excluding the small structural/Commit/Branch facts shown as
`m`. Ten Workspaces/Branches execute the same installation from the same base
through the same edit path.

The visible hero row for the committed payload category is:

```text
EQUIVALENT INSTALLS     10 -> 1
SAVED                   900 MiB / 1,000 MiB = 90.0%
COLLAPSE                10.0x
COVERAGE                full package-payload closure
```

### 4.1 Direct route: BranchStore -> LayerStore

```text
10 logical install candidates = 10 * 100 MiB = 1,000 MiB

BranchStore
    first install inserts Q
    next nine reuse Q
    stored payload ~= 100 MiB + O(10m)

Branch pushes to LayerStore
    first push sends/inserts Q
    next nine send 0 payload bytes

LayerStore
    stored payload ~= 100 MiB + O(10m)
```

| Metric | Result |
|---|---:|
| BranchStore local payload reuse | `(1000 - 100) / 1000 = 90%` |
| Ten Branch pushes transfer avoidance, with full baseline | `(1000 - 100) / 1000 = 90%` |
| Unique payload per DB | about 100 MiB |
| Route payload placement | about 200 MiB |
| Payload placement factor | `2.00x` |

`2.00x` is the two required physical placements, not a claim that BranchStore
contains duplicate CAS rows. Ten installs are approximately one payload set
inside each physical database.

### 4.2 Stacked route: BranchStore -> StackStore -> LayerStore

```text
BranchStore       about Q once
    |
    | ten Branch pushes: first Q, nine payload no-ops
    v
StackStore        about Q once; ten Stack/Branch facts remain small
    |
    | one Stack push of its accepted head/suffix
    v
LayerStore        about Q once
```

| Metric | Result |
|---|---:|
| BranchStore local payload reuse | `90%` |
| BranchStore -> StackStore ten-push avoidance | `90%`, with full baseline |
| First StackStore -> LayerStore push | `0%` if LayerStore lacks all of Q; this is expected placement |
| Repeated identical Stack push | `100%` payload avoidance |
| Unique payload per DB | about 100 MiB |
| Route payload placement | about 300 MiB |
| Payload placement factor | `3.00x` |

If LayerStore already received some or all of `Q` through another accepted
Branch/Stack, Stack Push sends only the missing remainder. Typed facts are
reported separately, because a new Branch/AddResult may be needed even when
object payload is fully reused.

### 4.3 Required test interpretation

The existing ten-install fixtures prove set equality and row uniqueness:

```text
receiver ObjectIds = mathematical union of required ObjectIds
one row per ObjectId in each physical DB
```

They do not by themselves prove a byte percentage unless a full candidate
baseline is recorded. The primary acceptance gate remains the set equation;
the percentage is a presentation derived from covered bytes.

---

## 5. Workspace storage, CPU, and memory

A Workspace is an ephemeral transaction forked from a Branch head. It is not
a database and its placement is not permanently bound to the Branch.

```text
Branch b:demo
|-- Workspace A: host/materialized
|-- Workspace B: host/FUSE
`-- Workspace C: Docker/thin-FUSE, host-controlled COW
```

### 5.1 Active transient storage is not committed Store dedup

An active Workspace may contain a complete materialized directory, dirty
spool, package cache, or container projection. Those bytes have not yet passed
through final-delta canonicalization, CDC, canonical encoding, and Store
admission. They must not be counted as CAS-reused or included in the flagship
`10 -> 1` savings card.

```text
active Workspace bytes
    -> projection/materialization/spool accounting only

workspace commit succeeds
    -> final base-to-view delta + CDC + canonical identities
    -> BranchStore admission receipt
    -> committed Store dedup accounting

branch/stack publication succeeds
    -> downstream independent Store placement accounting
```

For ten active materialized Workspaces, transient disk may approach ten working
trees even when their eventual commits collapse to one canonical payload set.
Only an authoritative allocated-block measurement may claim a lower cost. `workspace
commit` is the boundary at which committed Store reuse becomes measurable.

Tool calls, FUSE requests, shell commands, and intermediate mutations are not
candidate objects or durable facts. Monitor may retain bounded operation and
execution receipts, but committed candidate bytes are calculated only from the
collapsed final Workspace delta. Ten thousand temporary writes that end in one
final file must be measured as one final candidate, not ten thousand durable
changes.

The TUI presents separate sections:

```text
ACTIVE WORKSPACES                 COMMITTED/PUBLISHED CAS
materialized/spool/owned memory   10 -> 1 canonical payload set
not yet dedup-rated               90% saved under full coverage
```

An ended-with-discard Workspace contributes no candidate to committed Store
dedup. A failed/HeadMoved commit keeps transient Workspace accounting active
and contributes only the immutable rows actually admitted, if any; it does
not count as a successful equivalent committed result.

### 5.2 Workspace storage gauges

| Gauge | Definition |
|---|---|
| `cow_owned_memory_bytes` | Exact Workspace COW buffers/nodes owned on host |
| `spool_allocated_bytes` | Allocated host bytes for dirty payload spool |
| `spool_logical_bytes` | Logical spool file lengths |
| `materialized_allocated_bytes` | Host/container allocated blocks for a materialized presentation |
| `projection_cache_bytes` | Exact cache owned by this Workspace's FUSE/materialization adapter |
| `dirty_logical_bytes` | Logical size of pending changed file content; not physical cost |
| `referenced_base_logical_bytes` | Optional logical size visible from the base; label **referenced**, never **copied** |

If authoritative allocated-block accounting is unavailable, expose
`materialized_allocated_bytes = null` with
`measurement = "unsupported"`; do not report logical file length as disk use.

### 5.3 CPU accounting

CPU and wall elapsed are different:

```text
wall elapsed: operation latency observed by caller
CPU time:      processor time consumed by a process/thread/cgroup
```

Per Workspace, distinguish:

| Counter | Ownership |
|---|---|
| `workspace_controller_cpu_ns` | Host COW, final-delta construction, canonicalization, and projection-server work attributable to the Workspace |
| `execution_cpu_ns` | Host child or container command CPU attributable to the Workspace |
| `projection_client_cpu_ns` | Thin FUSE helper CPU attributable to the Workspace |
| `shared_runtime_cpu_ns` | Process/container CPU not safely attributable; shown separately and never divided heuristically |

Use thread/process CPU clocks or `getrusage`/`wait4` where available. For a
dedicated Workspace container, cgroup/container CPU may be attributed to that
Workspace. For multiple Workspaces in one shared container, only tracked
execution processes and per-Workspace FUSE helpers are attributable; residual
container CPU remains **shared/unattributed**.

### 5.4 Memory accounting

```text
workspace_owned_memory_bytes
    = cow_owned_memory_bytes
    + projection_cache_bytes
    + bounded transfer/final-delta buffers owned by the Workspace
```

This is an exact owned-memory gauge. It is not RSS.

| Memory value | Rule |
|---|---|
| Workspace-owned memory | May be summed across active Workspaces |
| Host process RSS | Process-level gauge; do not split across Workspaces |
| Dedicated container current/peak memory | May be assigned to its one Workspace |
| Shared container RSS/current memory | Container-level only; never divide by Workspace count |
| Tracked child peak RSS | Per execution when the OS/runtime exposes it |

### 5.5 Branch aggregation

For Branch `b` and its active Workspace set `W_b(t)`:

```text
branch_current_owned_memory(t)
    = sum(workspace_owned_memory(w, t) for w in W_b(t))

branch_current_spool(t)
    = sum(spool_allocated(w, t) for w in W_b(t))

branch_peak_concurrent_owned_memory
    = max_t(branch_current_owned_memory(t))
```

Never compute Branch peak as the sum of each Workspace's independent peak;
those peaks may occur at different times.

| Type | Examples | Aggregation |
|---|---|---|
| Current gauge | active memory, spool, dirty paths, active executions | Sum current values across active Workspaces when additive |
| Peak concurrent gauge | Branch owned-memory peak | Maximum of the observed concurrent sum |
| Cumulative counter | CPU ns, read/write bytes, candidate/inserted bytes, executions, commits | Sum completed and active Workspace deltas |
| Non-additive gauge | RSS, shared-container memory, percent CPU | Show at owning process/container; do not sum into an exact Branch value |

When a Workspace ends, remove it from current gauges but retain its cumulative
CPU/I/O/dedup/operation counters in the Branch runtime summary.

---

## 6. Per-operation elapsed-time contract

Every executed semantic operation emits an `OperationReceipt`, whether it
succeeds, returns `UpToDate`/`Conflict`/`HeadMoved`, is interrupted, or fails.
It covers queue and service work and is finalized in `CliEvent::Finished`
before frontend rendering. Missing phases are omitted, not represented as zero.

The standalone CLI additionally owns a `CliInvocationReceipt`. It starts after
capturing argv, covers parse, plan, context-host connection, execution wait,
render and flush, and is finalized/persisted only after output flush. It is not
embedded in the earlier `Finished` event and therefore makes no impossible
claim that pre-render data contains post-render time. Parse/plan failures have
an outer invocation receipt with no semantic-operation child. Phase Two may
measure its own render loop separately; it does not alter the service receipt.

`layerfs-monitor` owns both receipt schemas and span clocks. For a direct SDK
caller, the SDK starts an operation recorder at semantic method entry. The SDK
records the operation key/route, attaches raw receipts returned by domain
crates, and closes the semantic recorder with the typed outcome. It does not
calculate percentiles or persist monitoring state itself.

This ownership does not require passing a Monitor object into every low-level
function. Domain logic returns plain raw receipts for the SDK/host to attach;
instrumentation must not invert dependencies.

### 6.1 Clock rules

1. Measure durations with a monotonic clock (`std::time::Instant`), never
   `SystemTime`.
2. Wall-clock timestamps are optional receipt metadata only; never subtract
   timestamps from different machines.
3. Each process measures its own phase durations. A remote Store may return
   server-side durations, but the caller's end-to-end duration remains the
   authoritative user latency.
4. Store durations as integer nanoseconds. Rendering chooses ns/us/ms/s.
5. A timer stops on every return path, including errors and conflicts.
6. Instrument named coarse phases, not objects, chunks, FUSE calls, or SQL
   rows; per-item spans would change the algorithm being measured.

### 6.2 CLI and SDK boundaries

```text
cli_invocation_total_elapsed
|-- parse_elapsed
|-- plan_elapsed
|-- context_open_elapsed
|-- operation_wait_elapsed
`-- render_elapsed

operation_total_elapsed
|-- queue_elapsed
`-- service_elapsed
    |-- preflight_elapsed
    |-- membership_elapsed
    |-- transfer_elapsed
    |-- validation_elapsed
    |-- transaction_elapsed
    `-- projection/final-delta/cleanup elapsed, if applicable
```

`cli_invocation_total_elapsed` starts when the standalone CLI has captured
argv and ends after final output flush. Interactive time spent typing is not
included. `operation_total_elapsed` starts at accepted semantic execution and
ends when its typed result is available; this is the receipt carried by
`Finished` and direct SDK calls.

For a foreground `workspace exec`, total elapsed ends when the child exits and
its output/receipt is finalized. For `workspace output --follow`, elapsed is
the user's follow-session duration and must not be mixed into LayerFS storage
operation histograms. An interactive `workspace shell` similarly reports shell
session duration separately from filesystem operation performance.

### 6.3 Phase definitions

| Phase | Starts | Ends | Notes |
|---|---|---|---|
| `queue` | Store accepts caller into serialized queue | caller owns operation slot | Never hidden in service p95 |
| `preflight` | operation slot acquired | heads/bases/AddResult and route validation pinned | Includes indexed preflight queries, excludes queue |
| `membership` | first identity page prepared | last missing bitmap received | Record object and typed pages separately |
| `transfer` | first missing payload/fact frame prepared | last admitted transfer batch acknowledged | Includes transport waiting; has nested sender/receiver work |
| `validation` | closure/signature/three-way validation begins | definitive clean/conflict/error result | May precede and follow bounded admission; record actual phase tree |
| `transaction` | SQLite transaction begins | `commit` or rollback returns | One entry per transaction plus class |
| `commit_sync` | call to SQLite transaction `commit()` begins | call returns | Includes SQLite/WAL sync and possible checkpoint work; do not claim raw fsync syscall time |
| `ref_cas` | final head/ref statement begins | row outcome known | Nested in final transaction |
| `projection` | Workspace exposure begins | host/container view is ready | Workspace create only |
| `final_delta` | Workspace quiescence begins | canonical collapsed base-to-final delta is ready | Workspace commit only; excludes tool-operation history |
| `cleanup` | unmount/close starts | owned transient state is removed | Workspace end only |

Raw filesystem `fsync` syscall time is available only with explicit OS/VFS
profiling. The normal field is `commit_sync_elapsed_ns`, because SQLite's
`commit()` may include WAL writes, syncs, locks, and automatic checkpoint
work. Labelling it simply `fsync_ns` would be false.

### 6.4 Nested timing and sums

Phases form a tree. Each span records:

```text
name
inclusive_elapsed_ns
self_elapsed_ns
parent_span_index, optional
endpoint: client | branch_store | stack_store | layer_store | projection
```

`inclusive_elapsed_ns` may overlap child work. UI stacked totals use
`self_elapsed_ns`; they must not add every inclusive span and exceed total.
Parallel stdout/stderr drain and child execution are also overlapping work.

Required identities, allowing timer overhead/rounding epsilon:

```text
operation_total = queue + service
span.inclusive = span.self + sum(non-overlapping direct child inclusive spans)
cli_invocation_total >= parse + plan + context_open + operation_wait + render
```

CLI, context host, and remote Store processes each use their own monotonic
clock. They share one opaque `OperationId`, not `Instant` values or aligned
span starts. CLI socket wait contains host service and host transport wait may
contain remote service; these fragments are overlapping evidence and are never
added as sequential siblings. The `inclusive = self + children` identity is
valid only inside one process/clock. Remote fragments render as independently
measured durations associated with the same operation.

### 6.5 Operations covered

The common wrapper instruments every command automatically. The operation key
is a finite semantic class, not a user-supplied ID:

| Group | Operation keys |
|---|---|
| DB | `db.create`, `db.connect`, `db.use`, `db.disconnect`, `db.list` |
| Layer | `layer.init`, `layer.pull`, `layer.add`, `layer.list`, `layer.show` |
| Stack | `stack.create`, `stack.pull`, `stack.add`, `stack.push`, `stack.list`, `stack.show` |
| Branch | `branch.create`, `branch.merge`, `branch.pull`, `branch.push`, `branch.pull_commits`, `branch.list`, `branch.show`, `branch.diff` |
| Workspace | `workspace.create`, `workspace.shell`, `workspace.exec`, `workspace.output`, `workspace.stop`, `workspace.commit`, `workspace.end`, `workspace.list`, `workspace.show`, `workspace.diff` |
| Monitor | `monitor.db`, `monitor.dedup`, `monitor.workspace`, `monitor.branch`, `monitor.operation`, `monitor.process` |

Breakdown fields are operation-specific. Examples:

| Operation | Required relevant phases/counters |
|---|---|
| `branch.push` | queue, preflight, object/typed membership, transfer, validation, transactions/commit-sync, CAS, protocol turns |
| `stack.push` | queue, attestation/provenance preparation, membership, transfer, validation, transactions/commit-sync, copied-head CAS |
| `layer.add` / `stack.add` | queue, preflight, three-way, candidate build, transactions/commit-sync, head CAS |
| `workspace.create` | Branch head pin, Workspace allocation, projection ready; host/container subphases |
| `workspace.commit` | busy/quiesce, final delta, CDC/encode, local admission, Branch transaction/commit-sync/CAS, read-only transition |
| `workspace.end` | busy/dirty check, projection unmount, helper stop, spool/materialization cleanup, receipt close |
| read-only list/show/log/diff | query/service total; no fictional transaction or transfer phase |

### 6.6 Transaction and round-trip counters

Each operation receipt records:

```text
sqlite_read_statements
sqlite_write_transactions
sqlite_rollback_transactions
visibility_transactions
object_admission_transactions
fact_admission_transactions
transaction_commit_sync_elapsed_ns[] by class

object_membership_pages
typed_membership_pages
request_reply_turns
one_way_payload_batches
command_frames
payload_frames
reply_frames
wire_bytes_sent
wire_bytes_received
```

Counters are incremented at the actual SQL/transport boundary, never inferred
from pages or payload batches. The existing algorithmic gates remain:

```text
transfer request/reply turns <= P + 1
P <= object_membership_pages + typed_membership_pages

direct publication <= P_branch + 2 request/reply turns
stacked publication <= P_branch + P_stack + 4 request/reply turns
```

One-way data frames/batches do not become request/reply turns. Push and Add are
two explicit semantic operations; the Add request/result is not an avoidable
transfer turn.

---

## 7. Receipt v2

The current internal receipts must be destructively reshaped and returned to
`layerfs-monitor` before their values are exposed to CLI/TUI monitoring.

### 7.1 Current-code audit

The current code has three independent observability losses:

1. `TransferReceipt` merges objects and facts into the same
   `announced/missing/sent/inserted/raced` counters. Object payload reuse and
   metadata-fact reuse therefore cannot be reconstructed afterward.
2. `AdmissionReceipt` also merges locally admitted object/fact counts at
   aggregate call sites. Local Commit/Add reuse versus receiver races is not
   preserved as a typed domain result.
3. production transfer paths call `finish()` and discard `TransferReceipt`;
   `test_receipt()` is test-only. Several local admission calls similarly drop
   their returned receipt. Correct internal work is performed, but the facts
   disappear before `layerfs-monitor` can correlate and expose them.

The fix is not a metrics query after the fact. Domain methods return compact
raw receipts on the same successful/error outcome path, and
`layerfs-monitor` attaches them to one operation envelope. No extra existence
query, closure walk, transaction, frame, hash, or Store table is allowed.

Raw receipts belong with the domain operation that knows the truth:

```text
layerfs-content          candidate/CDC/encode raw counters
layerfs-storage/Stores   object, fact, transaction, frame/turn raw receipt
layerfs-workspace        COW/spool/quiesce/session/placement/execution/output raw receipt
FUSE/materialization     projection/import raw receipt
layerfs-monitor          one correlated OperationReceiptV2 + aggregates
```

### 7.2 Current fields that are misleading

| Current field/behavior | Problem | Replacement |
|---|---|---|
| `announced_ids` | Adds object IDs and typed fact IDs | `objects.announced_ids` plus `facts[kind].announced_ids` |
| `announced_bytes` | Currently increases while staging missing objects, not for all announced objects | Remove; use covered `candidate_object_bytes` or `null` |
| `missing_ids` / `missing_bytes` | Mix objects and typed facts | Separate object and typed-fact domains |
| `sent_ids` / `sent_bytes` | Mix payload CAS with metadata facts | `sent_object_*` and `sent_fact_*` |
| `inserted_*` / `raced_existing_*` | Admission receipt combines objects/facts | Split at the corresponding admission path |
| `wire_turns` | Can be accidentally derived from page/control branches | Count actual request/reply cycles in transport |
| `transactions` | Does not explain object/fact/visibility classes | Count actual transactions by class and measure commit-sync |

Do not preserve these ambiguous public aliases for compatibility. There is no
released external metrics contract to protect, and carrying both versions
would create two sources of truth.

### 7.3 Compact operation receipt

The Rust types may be split by SRP, but serialized output follows this logical
shape:

```json
{
  "schema": "layerfs-operation-receipt-v2",
  "operation_id": "op:...",
  "operation": "branch.push",
  "route": "branch_to_stack",
  "outcome": "fast_forwarded",
  "timing": {
    "cli_total_elapsed_ns": 12800000,
    "sdk_total_elapsed_ns": 12100000,
    "queue_elapsed_ns": 130000,
    "service_elapsed_ns": 11970000,
    "spans": []
  },
  "objects": {
    "candidate_coverage": "negotiated_frontier",
    "candidate_object_bytes": null,
    "announced_ids": 513,
    "preexisting_announced_ids": 500,
    "missing_ids": 13,
    "sent_ids": 13,
    "sent_bytes": 4194304,
    "inserted_ids": 13,
    "inserted_bytes": 4194304,
    "raced_existing_ids": 0,
    "raced_existing_bytes": 0,
    "known_subtrees_pruned": 17
  },
  "facts": {
    "announced_ids": 4,
    "missing_ids": 1,
    "sent_ids": 1,
    "sent_bytes": 130,
    "inserted_ids": 1,
    "inserted_bytes": 130,
    "raced_existing_ids": 0,
    "raced_existing_bytes": 0
  },
  "database": {
    "read_statements": 5,
    "write_transactions": 2,
    "object_admission_transactions": 1,
    "fact_admission_transactions": 1,
    "visibility_transactions": 0,
    "commit_sync_elapsed_ns": 2410000
  },
  "transport": {
    "object_membership_pages": 2,
    "typed_membership_pages": 1,
    "request_reply_turns": 4,
    "one_way_payload_batches": 2,
    "wire_bytes_sent": 4202000,
    "wire_bytes_received": 1024,
    "peak_buffer_bytes": 4194304
  }
}
```

Fields that do not apply are absent. Values not measured are `null` with a
coverage/status field; they are never silently zero.

### 7.4 Runtime counter ownership

| Counter | Owning layer |
|---|---|
| CDC input bytes, encoded candidates | `layerfs-content` build instrumentation |
| Missing membership, object/fact admission, transactions | Store implementation through `layerfs-storage` hooks |
| Actual frames/turns/wire bytes | byte transport boundary |
| COW/spool/quiesce state | `layerfs-workspace` |
| Session placement/execution/output state | `layerfs-workspace` |
| Host/container projection counters | FUSE/materialization adapter |
| Operation IDs, envelope, phase spans, CLI/SDK totals | `layerfs-monitor` recorder |
| Exact formulas, route placement, Branch aggregate | `layerfs-monitor` collector |
| Fixed histograms, JSONL rotation, presentation snapshots | `layerfs-monitor` |

`layerfs-monitor` composes the receipt when the SDK closes the operation
recorder. The SDK only records/attaches owned subreceipts. The CLI renders
human/JSON output and the TUI renders presentation snapshots; neither
recalculates Store semantics.

---

## 8. CLI presentation

Every mutating/transfer command ends with elapsed time and the relevant exact
work. Default output is compact:

```text
BRANCH PUSHED

Branch          b:91e
Route           BranchStore -> StackStore
Objects         13 sent / 500 announced-known
Payload         4.0 MiB sent · 4.0 MiB inserted · 0 B raced
Facts           1 sent / 3 announced-known
DB writes       2 transactions · commit-sync 2.4 ms
Transport       4 request/reply turns · 2 one-way payload batches
Elapsed         12.1 ms service + 0.1 ms queue = 12.2 ms SDK
```

When byte baseline coverage is unavailable:

```text
Transfer saved  not measured (known subtrees were pruned)
```

When an explicit full analysis supplied it:

```text
Transfer saved  96.7 MiB / 100.7 MiB eligible = 96.0%
```

Commands:

```text
layerfs monitor db
layerfs monitor dedup
layerfs monitor dedup --route <branch-store-id>
layerfs monitor dedup --route <branch-store-id> --analyze
layerfs monitor operation [operation-id]
layerfs monitor workspace [uuid]
layerfs monitor branch <branch-id>
layerfs monitor process
```

`--analyze` is explicit because it may stream full ObjectId/length inventories
or selected closures. It runs outside a Store mutation and does not hold the
writer gate.

Machine output uses the receipt directly:

```text
layerfs --json branch push b:91e
layerfs --json monitor dedup --route branch:local
```

JSON uses integer bytes/nanoseconds and `null` for unavailable measurements.
It never emits pre-rounded percentages as the only source value; rates are
derived from their numerator and denominator and may be included as display
convenience.

---

## 9. TUI presentation

The TUI must make scope visible before showing a number.

### 9.1 Route monitor

```text
+ STORAGE ROUTE ------------------------------------------------------------+
| BranchStore local-1  ->  StackStore build-1  ->  LayerStore central       |
+-----------------------+------------------------+---------------------------+
| BRANCH DB             | STACK DB               | LAYER DB                  |
| CAS       102 MiB     | CAS       103 MiB      | CAS       104 MiB         |
| DB+WAL    109 MiB     | DB+WAL    111 MiB      | DB+WAL    113 MiB         |
| objects   8,412       | objects   8,438        | objects   8,451           |
+-----------------------+------------------------+---------------------------+
| ROUTE PLACEMENT: union CAS 104 MiB · stored placements 309 MiB · 2.97x    |
| Expected independent DB placement; not duplicate rows inside one DB.       |
+---------------------------------------------------------------------------+
```

DB/WAL and CAS measurements carry freshness markers such as `live`, `5s ago`,
or `analyzed 3m ago`.

### 9.2 Dedup panel

```text
+ DEDUP · EQUIVALENT NODE INSTALLS -----------------------------------------+
|                                                                           |
|   90.0% SAVED        10 -> 1          900 MiB SAVED        10.0x COLLAPSE |
|                                                                           |
|   coverage: full package-payload closure · committed Workspaces only       |
+---------------------------------------------------------------------------+
| REQUIRED INDEPENDENT PLACEMENTS                                            |
|   BranchStore   1 canonical set   100 MiB                                  |
|   StackStore    1 canonical set   100 MiB                                  |
|   LayerStore    1 canonical set   100 MiB                                  |
|   route total   3 placements      300 MiB · expected, not failed dedup     |
+---------------------------------------------------------------------------+
| TRANSFERS                                                                  |
|   Branch -> Stack     900 / 1000 MiB avoided        90.0% [full]           |
|   Stack -> Layer      not measured · 24 known roots pruned                 |
|   Receiver races      0 B                                                  |
+---------------------------------------------------------------------------+
| ACTIVE TRANSIENT WORKSPACES                                                 |
|   materialized/spool       1.7 GiB · not yet committed/dedup-rated         |
+---------------------------------------------------------------------------+
```

The four hero values appear only when their byte category and coverage are
complete. Rate color is secondary to coverage. `full`, `changed set`, or `not
measured` must be visible on the same line. Active Workspace storage is never
subtracted from committed CAS savings.

### 9.3 Live operation timeline

```text
branch.push b:91e                                    RUNNING  12.2 ms

queue       [#]                                      0.1 ms
preflight    [##]                                    0.5 ms
membership     [####]                                1.8 ms
transfer            [###########]                   6.7 ms
  remote admit          [######]                    3.8 ms  (overlaps wait)
commit-sync                    [###]                 2.4 ms
finish                              [#]              0.7 ms

P_o 2 · H 1 · request/reply 4 · payload batches 2 · peak buffer 4.0 MiB
```

Nested remote spans use a second lane. The UI never sums overlapping bars.

### 9.4 Workspace/Branch resources

```text
BRANCH b:demo
active Workspaces 3     current owned memory 84 MiB     spool 1.2 GiB
peak concurrent owned memory 119 MiB                    CPU total 42.8 s

WORKSPACE     PLACEMENT          MEMORY OWNED   SHARED/RSS       CPU
w:host-a     host/materialized  18 MiB         host RSS 211 MiB  8.1 s
w:docker-a   Docker/FUSE        41 MiB         shared ctr 382 MiB 21.3 s*
w:docker-b   dedicated Docker   25 MiB         ctr 144 MiB       13.4 s

* tracked execution/helper CPU; shared-container residual is unattributed
```

The TUI must not sum the `SHARED/RSS` column into the exact Branch-owned
memory total.

### 9.5 Operation summaries

The monitor groups by finite key:

```text
(operation kind, direct|stacked, local|remote, outcome, projection kind)
```

Never key histograms by Branch, Workspace, path, command string, or ID; that
would create unbounded cardinality.

```text
OPERATION          N    p50      p95      max      queue p95   tx p95
branch.push       82   8.1 ms   18.4 ms  31.0 ms   0.8 ms      4.2 ms
stack.push        14  12.8 ms   44.1 ms  51.7 ms   0.2 ms      7.9 ms
workspace.commit 106  31.4 ms  121.0 ms 231.2 ms   1.1 ms     14.0 ms
```

---

## 10. Aggregation and statistics

`layerfs-monitor` owns all aggregation in this section. Each individual
receipt retains exact elapsed nanoseconds. Runtime summaries use a
fixed-memory histogram per finite operation key:

```text
count
sum_elapsed_ns
min_elapsed_ns
max_elapsed_ns
64 logarithmic ns buckets (base 2)
```

The buckets span sub-microsecond through multi-hour work without dependencies
or retained sample arrays. Percentiles are bucket estimates and render with
`~`; individual operation details remain exact. Tests and benchmarks that
need exact p50/p95 retain their bounded raw sample set and use nearest-rank
p95.

Report at least:

```text
n, min, approximate p50, approximate p95, approximate p99, max
queue p50/p95
service p50/p95
transaction commit-sync p50/p95 by transaction class
throughput operations/s for an explicitly selected window
```

Do not publish p95 with `n < 20`; show `insufficient samples (n=...)`. Do not
mix cold-open and warm-connected operations, local and remote routes, direct
and stacked routes, or success and Conflict/HeadMoved outcomes.

Checkpoint spikes remain in transaction/service distributions. No result may
delete a slow sample merely because it is considered noise. A gate may use an
explicit tolerance, but evidence retains the raw sample.

---

## 11. Sampling and overhead

### 11.1 Cadence

| Measurement | Default refresh | Cost policy |
|---|---:|---|
| Operation counters/timing | Event-driven | Always on; coarse spans only |
| Workspace-owned gauges | 500 ms while visible, 2 s otherwise | Read atomics/counters only |
| Process/container CPU and memory | 1 s | OS/cgroup/Docker sample; coalesce stale UI updates |
| DB/WAL/SHM file allocation | 5 s and after Store mutation | Filesystem metadata only |
| `page_count`, `page_size`, row counters | On connect and operation completion | Reuse Store owner; no writer wait in TUI thread |
| `sum(length(objects.bytes))` | Once on connect, then maintained by exact insert deltas | Full scan only at open or explicit refresh |
| Cross-DB ObjectId union/placement | Explicit `--analyze` | Streaming sorted cursors, outside writer gate |
| Reachable logical bytes/full closure baseline | Explicit benchmark/analyze only | Never a live polling task |

The single Store owner means the initial CAS aggregate plus exact inserted-byte
deltas remains accurate while that owner is active. On reopen, recalculate it
once. A second writer is forbidden by `StoreBusy`, so monitoring does not need
a persistent counter table.

### 11.2 Hot-path limits

Instrumentation may add:

```text
one operation receipt accumulator
coarse Instant timestamps
scalar/atomic counters at existing batch/transaction/frame boundaries
one bounded event notification per phase or display interval
```

It may not add:

```text
per-object logs or spans
per-FUSE-call persistent records
extra existence queries
full closure scans
duplicate object hashing
network calls under a writer transaction
unbounded channel queues
```

When the TUI cannot consume events quickly enough, retain the latest gauge and
coalesce progress. Never block a child stdout/stderr drain or Store pipeline
because the UI missed a frame.

Phase One also records and enforces the domain working-set ceilings:

```text
transfer object buffers                       <34 MiB
three-way + candidate/deferred memory          <=8 MiB before scratch spill
one active Store operation                     <42 MiB
Workspace final-delta memory                   <=configured cap (default 8 MiB)
live output tail per execution                 <=configured cap (default 1 MiB)
```

The Store ceiling excludes the separately frozen SQLite page cache and fixed
SQLite/runtime overhead. One Store admits one active operation working set;
queued callers allocate theirs only after admission. Tests use a 64 MiB changed
file, large provenance/DAG, and output larger than the tail to prove the bounds
are independent of total input/history/output size.

---

## 12. Persistence, retention, and privacy

Operation receipts and command output are local runtime artifacts, not Store
facts.

`layerfs-monitor` owns the bounded operation-receipt collector, fixed
histograms, and rotating operation JSONL. `layerfs-workspace` owns bounded
Workspace execution receipts/stdout/stderr and exposes their raw status for
monitor snapshots. The SDK owns neither retention implementation.

```text
state root/
|-- receipts/
|     `-- operations-000123.jsonl
`-- workspaces/
      `-- <workspace-uuid>/executions/<execution-id>/
            |-- receipt.json
            |-- stdout.log
            `-- stderr.log
```

Default bounded policy:

```text
operation receipts: keep newest 10,000 and at most 64 MiB or 30 days,
                    whichever limit is reached first
execution output:   separately byte/age bounded; receipt marks truncation
in-memory events:   bounded channel and fixed histogram cardinality
```

Rotation uses complete files and atomic rename. A partial final JSONL line is
ignored on read. This is observation durability, not Store recovery logic.

Default metrics receipts include:

```text
operation kind, route kind, outcome
Store/Branch/Workspace opaque IDs when needed
counts, bytes, durations, resource measurements
coverage/freshness/measurement status
```

They exclude by default:

```text
canonical object payloads
file contents and changed path names
environment variables
credentials/capability tokens
remote authentication material
full interactive-shell transcript
stdout/stderr (stored in separately governed execution logs)
```

Execution receipts may retain argv because command history is an explicit
feature, but never persist the full environment. UI and CLI must show the log
retention/truncation status. Store Push/Pull never transfers these artifacts.

---

## 13. Verification plan

Ownership gate:

```text
Phase One:
cargo test -p layerfs-monitor
cargo test -p layerfs-cli monitor

Phase Two:
cargo test -p layerfs-tui monitor
```

`layerfs-sdk` integration tests prove that every semantic outcome attaches and
closes the monitor recorder, but monitoring formulas, sampling, aggregation,
histograms, retention, and presentation snapshots are tested in
`layerfs-monitor`. A source/dependency audit rejects a new SDK `monitor.rs`,
SDK JSONL collector, or Store metric table.

### 13.1 Receipt and equation tests

| Test | Required proof |
|---|---|
| Object/fact separation | One mixed transfer produces independent exact counters; no combined public alias exists |
| Set equations | `A_o = E_o union M_o`, `S_o = M_o`, `M_o = I_o union R_o`, with disjoint sets and exact missing/sent/inserted/raced bytes |
| Known root | Descendants are not read; byte baseline is null/not-measured rather than a fake 100% |
| Concurrent insert | Two receiver connections partition inserted and raced IDs/bytes exactly |
| Repeated Push | Second identical Push sends zero object payload, runs zero CDC/encode, and creates zero duplicate object rows |
| Payload/fact distinction | New Branch/AddResult facts remain visible when object payload is fully reused |
| Transport counts | Actual proxy-observed request/reply cycles match receipt; payload-only frames do not increment RTT |
| Transaction counts | SQL trace counts actual transactions/classes including final folded visibility |

### 13.2 Two-/three-database dedup gates

Run ten deterministic installs through both routes and retain raw per-Store
inventories and per-boundary receipts:

```text
BranchStore: object count, unique CAS bytes, Commit/Branch rows
StackStore:  object count, unique CAS bytes, typed rows
LayerStore:  object count, unique CAS bytes, typed rows

per transfer:
    announced/preexisting/missing/sent/inserted/raced object sets and bytes
    same counters by typed fact kind
```

Gates:

1. each physical DB has exactly one row per ObjectId;
2. each receiver's final ObjectId set equals the mathematical required union;
3. ten same-path installs approach one package payload set plus `O(10)` small
   refs/structural metadata per physical DB;
4. direct route reports about two placements when both DBs need the same set;
5. stacked route reports about three placements when all DBs need the same
   set, without calling that a dedup failure;
6. repeated transfers send zero known object bytes;
7. StackStore -> LayerStore removes objects already present from any earlier
   Branch Push, Stack Push, or LayerHistory Pull.
8. the flagship snapshot exposes `candidate_runs=10`,
   `equivalent_canonical_sets=1`, exact candidate/unique/saved bytes,
   `savings_rate=0.90`, `collapse_factor=10.0`, and complete coverage;
9. CLI and TUI visibly render **90% saved**, **10 -> 1**, **saved bytes**, and
   **10.0x collapse** from those raw values rather than hard-coded text;
10. direct and stacked variants separately render 2 and 3 required independent
    Store placements without changing the 90% within-DB result;
11. ten active but uncommitted materialized Workspaces appear only under
    transient storage and never under committed Store dedup.

### 13.3 Timing tests

| Test | Required proof |
|---|---|
| Monotonic clock | Wall-clock adjustment cannot create negative/huge duration |
| All outcomes | Created/FastForwarded/UpToDate/Conflict/HeadMoved/error each close a receipt |
| Phase tree | Inclusive/self relationships do not double count; remote overlap is represented |
| Queue separation | Ten callers report queue and service independently; queue is not hidden in service p95 |
| Transaction timing | Commit-sync duration is measured around SQLite `commit()`, checkpoint spikes retained |
| CLI vs SDK | `cli_total >= sdk_total`; parse/context/render reported separately |
| Exact operation detail | Individual elapsed is integer ns; summary percentiles are marked approximate |
| Small samples | p95 withheld below 20 observations |

### 13.4 Resource tests

| Test | Required proof |
|---|---|
| Bounded monitor | Metrics memory stays constant over 100,000 operations |
| Bounded logs | Rotation obeys count/byte/age policy; receipt records truncation |
| Workspace end | Current Branch gauges decrease; cumulative CPU/I/O remain |
| Concurrent peak | Branch peak is maximum concurrent sum, not sum of individual peaks |
| Shared container | Residual RSS/CPU remains shared/unattributed |
| Dedicated container | cgroup current/peak and CPU map to its sole Workspace |
| Materialized directory | Unsupported allocated-block accounting reports null, not logical bytes |
| No Store schema change | Exact Branch 3/9 and Full 8/24 manifests remain unchanged |

### 13.5 Performance benchmarks

Use the existing `fs-bench` scenarios plus operation-level measurements for:

```text
direct and stacked topology
host projection and Docker thin-FUSE projection
warm and cold Store connection
first and repeated identical Push
10 identical installs
64 MiB write/read/copy/overwrite
1,000-file create/stat/remove
workspace create/commit/end
```

Every benchmark receipt includes environment, route, projection, `n`, raw
samples, p50/max, queue/service split, transaction classes, frames/turns, CAS
bytes, and peak Workspace/Store-operation memory. It includes p95 only when
`n >= 20`; otherwise it reports `insufficient samples`. `fs-bench` filesystem
latency and SDK publication latency are separate measurements; neither is used
as a proxy for the other.

Instrumentation acceptance requires:

```text
same functional result and set equations with monitoring on/off
passive semantic-operation instrumentation adds no SQL query or Store request
no additional ObjectId hash/CDC/encode
no unbounded memory/log growth
measured overhead reported, not assumed
```

An explicit `monitor` snapshot or `--analyze` command may execute its documented
read-only observation queries. They are separately timed, remain outside the
writer gate, and are never attributed to the semantic operation being studied.

---

## 14. Explicit non-goals

This design intentionally does not add:

- metrics tables in any Store;
- a global cross-database CAS or registry;
- per-object/per-syscall trace persistence;
- a Prometheus/OpenTelemetry dependency before an external consumer exists;
- a generic telemetry daemon/framework or a separate monitoring repository;
- automatic full-closure scans to improve a dashboard percentage;
- retry, reconnect, crash-recovery, GC, or rollback policy;
- heuristic allocation of shared process/container RSS;
- command output inside Layer/Stack/Commit roots;
- a second transfer or deduplication path for monitoring.

The minimal product is exact counters at existing boundaries, bounded external
receipts, an honest UI, and explicit expensive analysis only when requested.

---

## 15. Terminal acceptance checklist

- [ ] `layerfs-monitor` owns operation envelopes/spans, formulas, route
      aggregation, resource sampling, bounded collection/retention, and
      presentation snapshots; `layerfs-sdk` only composes and records.
- [ ] Raw Content build/CDC counters come from `layerfs-content`, Storage
      admission/transfer receipts from `layerfs-storage`/role Stores, and all
      COW/spool/session/placement/execution/output snapshots from the one
      `layerfs-workspace`.
- [ ] Every CLI and public SDK operation emits exact total elapsed ns.
- [ ] Queue, service, transaction commit-sync, transport, final-delta/projection,
      and cleanup timings are separated where applicable.
- [ ] Receipt v2 separates CAS objects from every typed fact domain.
- [ ] Current misleading combined receipt fields are removed, not aliased.
- [ ] Request/reply turns are observed at the transport, not inferred.
- [ ] Fast-path byte rates are omitted when known-subtree pruning makes the
      baseline unknown.
- [ ] `monitor dedup --analyze` computes route union/placement by a streaming
      sorted merge and holds no writer gate.
- [ ] Direct and stacked ten-install tests prove one payload set per physical
      DB plus small metadata and explain the two/three placement copies.
- [ ] The flagship CLI/TUI view derives and displays **90% saved**, **10 -> 1**,
      saved bytes, **10.0x collapse**, coverage, and 2/3-DB placements.
- [ ] Active transient materialized/spool bytes remain separate from
      committed Store dedup and are never described as already deduplicated.
- [ ] Physical DB, WAL, SHM, unique CAS, Workspace spool/materialization, and
      process/container memory appear as separate values.
- [ ] Per-Branch current gauges and cumulative counters follow the aggregation
      rules; shared RSS is never divided heuristically.
- [ ] TUI operation timelines do not double count nested/remote spans.
- [ ] Receipt/output retention is bounded and privacy defaults exclude
      contents, paths, environments, credentials, and tokens.
- [ ] Exact Branch 3/9 and Full 8/24 Store schemas remain unchanged.
- [ ] Monitoring adds no extra object query, transfer turn, transaction,
      hash, CDC invocation, or unbounded collection to the product path.
