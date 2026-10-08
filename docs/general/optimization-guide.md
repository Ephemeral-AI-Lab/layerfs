# LayerFS optimization and performance-debugging guide

> **Status:** Current general guide.
> Owner-directed performance engineering rules, 2026-10-05. Binding for relevant
> implementation and optimization work; not a claim of product capability,
> benchmark qualification or measured improvement.

Read [root instructions](../../AGENTS.md), [core instructions](../../core/AGENTS.md),
the [cluster-one handbook](../../cluster_one_handbook.md),
[CAS/CDC/delta contracts](../../cas_cdc_deltaencoding_handbook.md) and the
[cluster-two design](../../core/docs/issues/303/README.md) for the owning contract.
Both workstreams implement in `core/`; root `crates/` is the v0.1.6 reference
until cluster two completes. Historical plans and experiments do not select a new
algorithm, dependency, persistence profile or performance gate.

The [optimization handbook](optimization-handbook.md) is the working procedure
built on these rules: which counters exist, how to tell which layer amplifies
work, and which direction each finding points to. These rules govern it.

## 1. Mandatory scope and acceptance

Apply this guide when introducing/changing a hot path, query, index, payload
layout, ownership structure, FUSE/cache/scheduling behavior, construction path or
maintenance algorithm, and when diagnosing a performance finding. Documentation
or unrelated changes do not require an invented benchmark campaign.

1. Identify the authentic operation and the work responsible for its cost.
2. State worst-case, amortized and cumulative complexity with named variables.
3. Reject quadratic or worse scaling/amplification in supported operation paths.
4. Use SQLite's indexed/backed mutable state rather than a second custom mutable
   tree, graph index or growing resident state container.
5. Attribute FUSE requests, SQL work, queue waits, bytes/copies, residency and
   retained/reclaim work. SQLite debugging requires database EXPLAIN evidence
   and runtime database profiling; source inspection or faster wall alone is
   insufficient to support a SQLite optimization claim.
6. Preserve filesystem correctness, authority, capture/publication outcomes and
   resource ownership. Moving work to background or another process does not
   remove it from analysis or accounting.
7. Follow the existing [measurement workflow](agent-measurement-policy.md) and
   [benchmark rules](benchmark_rules.md); debugging does not permit resampling,
   warm credit, reduced workloads or relaxed budgets.

Known current-source violations remain findings with explicit correction scope.
Do not label a new design load-bearing while inherited quadratic work, input-sized
resident collections or artificial total-operation refusal still block it.

## 2. Diagnose mechanisms before tuning

```text
authentic operation + exact source/profile/workload
                     |
             declare cost/work model
                     |
       inspect source + query/request path
                     |
       count actual work and identify amplification
                     |
       propose change + correctness/resource obligations
                     |
       implement on ordinary production path
                     |
       scoped diagnostics and final covering proof
                     |
       qualified timing only under its frozen contract
```

Reuse existing receipts and source evidence first. A new diagnostic must answer
a cause: which requests, rows, pages, bytes, copies or dependencies grew, and why.
Do not repeat a passing unchanged timing arm to obtain a nicer number or infer
the cause from elapsed time alone. Label diagnostics and instrumentation overhead.

Use existing provider/kernel counters, external observers or legitimate product
telemetry. Do not patch third-party code, add test-only product APIs/features,
select a different algorithm for a benchmark or create a second product route.
Unsupported observations are reported unavailable; they are not invented as zero.

## 3. Complexity analysis and rejection of quadratic scaling

### 3.1 Name every growing dimension

| Variable | Meaning |
| --- | --- |
| N | Relevant stored metadata/index population; identify Workspace-local versus shared database scope |
| M | Ordinary filesystem operations requested across the workload |
| K | Affected or returned keys/rows for the operation |
| B | Actual payload bytes requested, changed or emitted; account processed/read bytes separately |
| L | Logical file length, including sparse holes; do not equate it with B |
| F | Payload/edit fragmentation intersecting the operation |
| H | Prior generations, Commits or retained dependency history |
| D | Actual topology/dependency depth traversed |
| W | Concurrent Workspaces/process/service producers |
| Q | Admitted queued jobs/bytes and in-flight work |
| G | Eligible reclamation debt |
| R | Live retained data/owners; distinguish it from reclaimable debt |

State key-size/comparison, tree-depth, page-size and representation assumptions.
Bounded request bytes do not automatically bound F, SQL rows, base demands, journal
pages or native buffers. A batch/window bound is not a total-work complexity proof.

Analyze one operation, the complete command and repeated call/Commit lifetimes.
Include SQL inside loops, repeated passes, copying/reallocation, sorting, version
resolution, topology/reference checks and deferred disposal. Separate actual
input/output work from avoidable amplification. Showing O(N) for one operation
does not justify invoking it M=N times and producing O(N^2) work.

### 3.2 Required work shapes

| Operation | Required direction / honest cost |
| --- | --- |
| Mutable point metadata lookup | Indexed O(log N), plus bounded key/page work; no unrelated population scan |
| Suitable covering/keyset range returning K rows | O(log N + K); non-covering row fetches can add O(K log N) work |
| K independent indexed mutations | O(K log N) metadata work plus actual payload/page/index work |
| Read/construct/copy B new bytes | Account B and canonical/read/copy amplification; no metadata-only claim |
| Capture | Stable ownership/domain transition without a K/B-sized copy; admission/drain wait still counted |
| Localized Commit | Affected keys/ranges/tree paths plus necessary verified topology/reference/release work |
| Reclaim K owned records | Output-sized total work through bounded weighted batches, not a whole-state scan per record |
| Repeated incremental Commit | No mandatory growth solely with H in current read depth, live versions or per-mutation work |

These are requirements to derive and demonstrate, not implemented complexity
claims. O(K) output, O(B) content processing and indexed O(K log N) updates are
not quadratic merely because they are nonconstant. Multiplicative dimensions
must be expanded when they can grow together; do not conceal N^2 as M*N or K*F.

Reject full-pair/ancestor materialization or repeated global validation introduced
only to accelerate another lookup. SQL-backed graph relations do not exempt
their storage, traversal or update algorithm from this rule. Required alias/cycle/
authority checks stay intact; redesign their evidence/access instead of skipping
them or raising a work limit.

### 3.3 Patterns that must be rejected

| Pattern | Why it fails | Required direction |
| --- | --- | --- |
| Whole-node collection after every FORGET/RELEASE | M callbacks repeatedly scan N owners | Indexed ownership updates and separately scheduled targeted disposal |
| Rebuild growing file/BLOB after every tiny append | Sum of growing prefixes is quadratic copying | Fixed bounded append units/tails and explicit transitions |
| Repeated pages using increasing OFFSET | Complete enumeration revisits prefixes | Stable keyset cursor and indexed ordering |
| Linear membership search for every changed key | K changes against K resident entries yields K^2 | Indexed backed membership and bounded processing windows |
| Whole-base walk after each small rename/Commit | Repeated small operations pay unchanged N | Incremental checked topology/reverse-binding evidence; no hidden mount scan |
| Captured cursor filters a growing active population | Tiny capture cost follows unrelated inserts | Generation-selective index and fixed captured termination domain |
| Add a retained layer per Commit/failure/orphan capture | Read/consolidation depth grows with H | Bounded ownership/composition and proven reclamation progress |
| One base read per invalid byte/gap | Fragmentation recreates a request/IO storm | Grouped bounded read plan; expose extra bytes/copies |

Source/count analysis can establish rejection without waiting for an enormous
timing failure. For scaling diagnostics, prospectively define distinct size cases
and hold other dimensions controlled. Counts at N, 2N and 4N can test the derived
model; they are not permission to resample the same case or proof from a noisy
time ratio. Record preexisting sizes, affected work and the selected adversarial
shape rather than extrapolating from a favorable small smoke case.

## 4. SQLite is the mutable indexing and backed-state engine

### 4.1 Scope of the SQLite rule

For cluster-two mutable inode/dentry lookup, ordering, captured membership,
dirty/touched sets, parent/reference relations, payload-unit location, operation
scratch and reclamation, use SQLite tables, keys, indexes and bounded cursors.
Do not introduce a competing custom persistent B-tree, interval/page store or
graph index, or an unbounded resident tree/graph mirroring that state.

Bounded standard caches and fixed processing buffers are allowed only with exact
identity/version keys and eviction that cannot lose authoritative/live state or
refuse more files solely because a cache is full. Large ownership/membership state
must remain addressable in backing; do not replace a total-workload cap with a
different resident container.

Cluster one's immutable canonical content/filesystem trees, CAS identities and
dependency grammars are existing public formats. Use their public constructors,
readers and update contracts; this rule does not replace those formats with SQL
or authorize reimplementing their trees in cluster two. Analyze inherited work
and correct genuine core constraints through the owning contract.

### 4.2 Require the actual access plan and work bound

For each hot statement document the predicate, expected result/window size,
supporting key/index, ordering and transaction boundary. Both database EXPLAIN
and runtime database profiling are mandatory evidence for SQLite performance
debugging and optimization acceptance. Retain `EXPLAIN QUERY PLAN` for the exact
statement/schema/indexes and representative bound-input shape at the pinned
SQLite build. Use full `EXPLAIN` when the high-level plan does not resolve the
VM loop, temporary-state or execution mechanism under investigation. Pair that
plan with the scoped runtime profile defined below.

An indexed SEARCH is not proof that residual filtering is bounded. An index SCAN
can still visit the complete population. Nested loops, correlated subqueries,
temporary sorting and recursion can multiply work. Inspect the whole statement
and its caller loop. EXPLAIN's human-readable format is not a stable production
interface; do not add a runtime parser depending on its wording.

Required practices:

- Lead indexes with the actual namespace/predicate/order needed; use separate
  generation-selective access when lookup order cannot serve capture efficiently.
- Use prepared statements for repeated operations and keyset pagination rather
  than growing OFFSET. Inspect covering access where it removes unnecessary
  lookups without unacceptable write/index amplification.
- Project metadata/length without payload BLOBs for metadata operations. Binary
  payload remains BLOB; string concatenation is not a safe append/layout strategy.
- Bound actual touched rows, cells, pages and bytes. A set-based DELETE/UPDATE or
  LIMIT does not prove that its underlying scan or overflow-page work is small.
- Document every additional index's insert/delete/split/storage cost. No blanket
  rule to add indexes without measuring the whole access/mutation tradeoff.
- Keep transactions short and release owner/Workspace locks before base fetch,
  construction, remote delivery or process waits. No whole-Exec/Commit checkout.
- Reads must be internally consistent, but should not add unnecessary explicit
  BEGIN/COMMIT framing. Use a deliberate bounded snapshot/critical section where
  multiple statements require it; do not remove consistency for a statement count.
- Close/sequence incremental BLOB handles correctly. Updating their row can expire
  them; writable handles must not prevent COMMIT. Fixed-length BLOB I/O does not
  resize a value or make first-touch allocation/page work zero.
- No per-mutation checkpoint, VACUUM, ANALYZE or unbounded reclamation. Profile and
  maintenance changes are deliberate, prospective decisions, not error fallbacks.

Track statements, transactions, visited/returned rows, relevant VM/scan/sort work,
BLOB calls/bytes, copied/allocated bytes, dirty/journal/overflow/index pages and
owner wait/service. Identify unavailable counters and measurement scope. Do not
equate logical bytes, representation bytes, pager bytes and device bytes.

SQLite has one writer per database. Prepared statements, MEMORY/OFF or WAL do
not eliminate that capacity boundary. Fair bounded jobs and backpressure need
sustained service/debt evidence; more callers do not establish more throughput.

Primary mechanism references: [query planning](https://www.sqlite.org/queryplanner.html),
[EXPLAIN QUERY PLAN](https://www.sqlite.org/eqp.html),
[transactions](https://www.sqlite.org/lang_transaction.html) and
[incremental BLOB handles](https://www.sqlite.org/c3ref/blob_open.html).
Website text is not a deployed binary seal; implementation uses the pinned build.

### 4.3 Require database profiling alongside EXPLAIN

EXPLAIN describes a strategy/program; it does not measure the work that actually
executed. Static opcode count is not runtime VM-step count. Overall Bash/FUSE/
Commit wall, connection settings called a persistence "profile", or an EXPLAIN
artifact alone do not satisfy the database-profiling requirement.

Collect a scoped runtime database profile from the actual owning engine. Use
SQLite's supported profiling facilities, such as `sqlite3_trace_v2` with
`SQLITE_TRACE_PROFILE` and `sqlite3_stmt_status`, or equivalent validated provider
statement/transaction/BLOB profiling that observes actual SQLite execution.
Record which mechanism and build features were used. Do not assume a trace hook
or optional scan-status facility is exposed by the pinned driver/build.

| Evidence | Required content |
| --- | --- |
| Statement identity | SQL/template identity, operation/Workspace scope, invocation counts, input cardinality/size/selectivity and result count |
| EXPLAIN artifacts | Selected indexes, scan/search/nesting/order/temporary-state analysis; full program where needed; schema/statistics/configuration and SQLite identity |
| Runtime statement profile | Actual execution count, elapsed observations and VM-step counts; returned/visited, full-scan, sort and reprepare observations when supported |
| Provider/transaction profile | Prepare/cache checkout, bind, step, row mapping, COMMIT/rollback and BLOB work as applicable; owner/queue wait separately attributed |
| Resource profile | Pager/journal/IO/copy/allocation work needed by the claim; unavailable observations labeled, not treated as zero |
| Correlation | Explain how the chosen plan, measured work and caller multiplicity support the cause and proposed change |

Statement profiler times may be approximate and inclusive; identify clock units,
resolution and boundaries. They are not automatically exclusive database CPU.
Never sum nested statement/COMMIT/transaction spans into a fabricated total.
Reset/snapshot counters through their owning scope and account for cached
statement reuse; cumulative connection statistics shared by several operations
are not automatically a profile of one Workspace or phase.

The current global provider already records actual VM steps and inclusive
statement/transaction/BLOB observations in
[SqlWork](../../core/crates/layerfs-persistence/src/backend/sqlite/connection.rs)
and its [query execution](../../core/crates/layerfs-persistence/src/backend/sqlite/query.rs).
Confirm observation coverage and aggregation before using it for a statement
claim. Global-provider profiling does not automatically profile the daemon's
separate overlay engine.

Retain the paired plan and profile with exact commands, source/build/workload/
schema/configuration identity, observation scope and instrumentation overhead.
Use bounded aggregation rather than an unbounded trace log. Profiling follows
the existing one-sample/cold-state/receipt rules; it does not authorize replay of
mutations, warm reruns or changes to the measured production path.

If a required explain/profile capability is unavailable, report the concrete
build/API/access limitation and keep the SQLite performance claim unqualified.
Continue independent analysis, but do not silently replace mandatory database
profiling with a stopwatch or mark the optimization accepted. Use supported
interfaces; no third-party patches or test-only product APIs.

Primary references: [SQLite tracing](https://www.sqlite.org/c3ref/trace_v2.html),
[PROFILE event semantics](https://www.sqlite.org/c3ref/c_trace.html) and
[runtime statement counters](https://www.sqlite.org/c3ref/stmt_status.html).

## 5. FUSE performance debugging

Attribute the complete syscall path rather than callback wall alone:

```text
ordinary process syscall
          |
kernel cache / FUSE request
          |
receive + validate + dispatch
          |
admitted job / queue / inode-resource wait
          |
Workspace plan -> short SQL service -> required immutable-base acquisition
          |
local mutation publication / bounded result
          |
reply-send attempt -> kernel/process observation

related lifecycle work: native attach, capture, Commit, detach and reclamation
```

Count request classes and payload/entry sizes: LOOKUP, GETATTR, READ, WRITE,
READDIR/PLUS, CREATE, LINK, RENAME, UNLINK, FLUSH, RELEASE and FORGET. Explain
which requests are necessary and which repeats could be removed safely.

Separate receive/dispatch, runnable service, blocked wait, base/transport demand
and reply work. Do not sum overlapping callback spans into a fabricated command
decomposition. Background-request settings do not bound all foreground requests,
and negotiated request bytes do not define every native receive allocation.

Inspect cache hits/misses, invalidations, lookup/open references, queued replies,
copies, disabled-trace allocations, receive buffers/stacks and actual residency.
Gate expensive formatting before disabled tracing. Use bounded aggregate
observations rather than an unbounded log or a printf for every callback.

Deferred replies must own borrowed arguments through bounded credits and exact
incarnation/attempt state. Queue saturation must not block all receivers while
waiting for a callback needed for progress. Do not add workers to disguise that
dependency cycle or a global lock. Lifecycle/release/demand work stays runnable.

No optimization removes permissions, changes stable identity, loses alias/page/
attribute coherence or allows writes to bypass capture. Keep kernel writeback off.
Mapped-write requests are a distinct supported-kernel route, not proof that
mount-wide writeback was enabled. Reply-send attempts are not kernel delivery
receipts; preserve locally published state after lost replies.

Cache/request/capability changes require a prospective profile and the owning
correctness/resource proof. Increasing TTL, request size, background depth or
thread count is not an explanation of the original cost. Copy acceleration must
use ordinary filesystem semantics with independent destination ownership, not
recognize a Bash command or bypass content/history contracts.

Use the pinned kernel/library APIs and available external observers. Report a
dependency capability gap rather than patching the crate or asserting that a
named flag implements an unavailable transport/callback.

References: [kernel FUSE controls](https://www.kernel.org/doc/html/v6.12/filesystems/fuse.html),
[I/O modes](https://www.kernel.org/doc/html/v6.12/filesystems/fuse-io.html),
[primary FUSE contract](../../core/docs/issues/303/fuse.md) and
[source-qualified request investigation](../../core/docs/issues/303/fuse-investigation/01-kernel-and-request-path.md).
Linux v6.12 is a mechanism reference, not an assumed deployment identity.

## 6. Workloads, lifetime and resource amplification

Cover the relevant dimensions, not just a convenient small-file/read-only case:

| Shape | Required analysis |
| --- | --- |
| Many tiny files/wide directories | Requests per name; indexed visits; row/cell/index overhead; enumeration and release work |
| Large streams and tiny appends | Bytes/copies/page work; bounded tail growth; first-touch and boundary amplification |
| Dense scattered edits/full-window overwrite | Dependence on F and prior history; bounded WRITE and READ/base-demand work |
| Sparse/shrink/regrow | Holes/cutoffs; logical versus processed bytes; stale cleanup and boundary visibility |
| Small Commit on a large base | Captured membership; unaffected base visits; validation/topology/reference work |
| Persistent logger/orphan across Commits | Live version count, read depth, failed-capture resolution and last-owner reclamation |
| Concurrent Workspaces/calls/Saves | Per-class service/wait, queue bytes, writer capacity, demand/finish/cleanup fairness |
| Fresh and retained mounts | Actual attach/detach costs versus valid same-mount caches; no forced remount or hidden prefetch |

Per-tool-call and per-task modes are both required; per-tool-call is common.
Either can execute short or long-lived commands. Do not infer a near-term exit,
Commit or unmount to justify deferred resource cleanup. Account repeated calls,
incremental Commits, failures and sustained activity over actual lifetime.

Bound whole-system residency: process heap, SQL pager/journal, scratch, object/
Save caches, native buffers, queues, kernel inode/dentry/page cache and backing
file cache. A buffer allowance or allocation length is not a measured resident
bound; a process/cgroup lifetime peak is not a phase-local peak.

Reclamation debt must have service capacity and reserved headroom. Count eligible
versus live retained state, bytes/pages/owners released and work remaining. Do not
hide unbounded deletion behind an early terminal reply or perform arbitrary old
cleanup in a tiny write. Preserve exact captured/read/orphan/uncertain custody.

## 7. Evidence, correctness and review record

Performance changes retain the ordinary production path and owning correctness
contract. Source reasoning and narrowly scoped public-behavior proof cover the
changed mechanism; final required checks follow [core instructions](../../core/AGENTS.md#checks-and-completion).
Do not write implementation-mirroring tests or rerun unchanged passing suites.

Freeze diagnostic size variants, identities, cache states, limits, worker/profile
settings and instrumentation prospectively. Reuse setup and unaffected evidence;
collect one sample per selected case/arm. Keep every nonpassing/unavailable line,
and separate diagnostics, independent proof and eligible performance claims.
No larger budget, reduced root, warm rerun or extra construction producer to pass.

The review record must state:

1. Authentic operation, source/tree/binary/profile/workload and observation scope.
2. Growing variables, worst-case/amortized/cumulative bound and assumptions.
3. Actual query/request path, supporting index and visited-versus-useful work;
   for SQLite, both EXPLAIN artifacts and correlated runtime database profile.
4. Counts of requests/statements/transactions, bytes/copies/pages, queues,
   residency, retained depth and reclamation; unknown observations labeled.
5. Proposed mechanism, expected work removed and speed/storage/memory tradeoffs.
6. Correctness, coherence, authority, publication/cancellation and custody proof.
7. Exact commands/evidence, outcomes, remaining findings and unrun work.

Use the [report template](../../benchmark_agent_report.md) for actual invocations;
this record does not replace a frozen benchmark contract or receipt. A quadratic
finding, hidden global scan, unsupported plan or unbounded retained structure
blocks optimization acceptance until corrected; a small timing win cannot waive it.

Supporting research: [combined investigation](../../core/docs/issues/303/fuse-optimization-investigation.md),
[mutation engine](../../core/docs/issues/303/fuse-investigation/02-mutation-engine.md)
and [lifecycle](../../core/docs/issues/303/fuse-investigation/03-per-call-lifecycle.md).
Their proposed algorithms and historical observations retain their qualifications.
