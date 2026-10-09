# LayerFS optimization handbook

> **Status:** Current general guide.
> Working procedure for diagnosing and optimizing the cluster-two product,
> written 2026-10-09 at product identity `2b4dc28a6` (R6 closed). It is not a
> claim of product capability, benchmark qualification or measured improvement.
> No timed result of the mounted product exists yet.

This handbook says how to find out why something is slow and which direction to
take. The [optimization guide](optimization-guide.md) holds the binding rules;
the [measurement workflow](agent-measurement-policy.md) and
[benchmark rules](benchmark_rules.md) hold the rules for any timed sample. Where
this handbook and one of those disagree, they govern.

Numbers quoted here are counts from functional receipts in the
[R4](../../core/docs/issues/307/R4-COMPLETION-20261008.md),
[R5](../../core/docs/issues/307/R5-COMPLETION-20261009.md) and
[R6](../../core/docs/issues/307/R6-COMPLETION-20261009.md) records, or context
from the #305 experiment as summarized in the
[proof plan](../../core/docs/issues/307/S8-PROOF-PLAN-20261008.md). Experiment
numbers are not comparison arms.

## 1. The one idea

Treat every slow case as an amplification to locate. Some layer turns one unit
of work from above into many units below. Find that layer, remove the
multiplication, and prove nothing else changed. Do not tune speed directly.

```text
one tool call
   |
   +-- mount:   bind root -> attach -> Ready
   +-- command: syscall -> kernel cache -> FUSE request -> dispatch
   |                -> owner jobs -> SQL statements
   |                -> Store reader -> object demands (cold only)
   |                -> reply
   +-- Commit:  capture -> construct namespace -> save -> publish -> install
   +-- unmount: detach -> drain -> revoke -> close
   +-- cleanup: maintenance turns until the namespace is gone
```

Each arrow is a place where one unit can become many. The ratios in section 4
measure each arrow.

## 2. Rules in short

The full text is in the optimization guide. These are the ones that decide most
situations.

- **Counts before time.** Make no code change until the cause is one sentence
  of the form "X grew because Y", with numbers.
- **One sample per case and arm.** Never rerun an unchanged case. Keep every
  receipt, including failures.
- **Pin everything.** Release build with the repository ARM flags, image,
  workload and cache class. Cache state is enforced, never assumed.
- **Compare like with like.** Product (`L`) against passthrough (`P`) against
  native (`N`), in the same cache class only.
- **A database claim needs both** the query plan and a runtime profile from the
  engine that executed the statement.
- **Keep a change only if** the counts drop, every affected proof still passes,
  and speed and storage are reported together.
- **Do not trade correctness.** Permissions, stable identity, cache coherence,
  kernel writeback off, one attempted operation and original failures stay.
- **No test hooks in product source.** Use the existing counters. An
  observation that is unavailable is reported unavailable, not zero.
- **One change per commit**, each with its counts and its production LOC line.
- **Stop and write a proposal** for anything that changes an interface between
  crates, the canonical format, a persistence profile, the negotiated FUSE
  profile, a public contract or a threshold. Those belong to the owner.
- **Global Store profile is Disposable / WAL / synchronous=OFF only.** Durable
  is `NOT_RUN — disabled by owner until explicit reauthorization`.

## 3. Instruments that already exist

Start here. None of these needs new product code.

| What it counts | Type and fields | Source | How to read it |
| --- | --- | --- | --- |
| Requests per opcode for one connection | `OpcodeWork`: `opcodes[]`, `handoffs`, `inline`, `refused`, `terminal`, `unadmitted`, `forget_units` | [`request/accounting.rs`](../../core/crates/layerfs-fuse/src/request/accounting.rs) | In the drain receipt after unmount |
| A mount's live request state | `NativeWork`: `received`, `admitted`, `queued`, `running`, `parked`, `retained`, `completed`, loop counts | [`control_native.rs`](../../core/crates/layerfs-bridge/src/control_native.rs) | Control Status while mounted; the unmount receipt afterwards |
| The dispatcher as a whole | `DispatchWork`: workers, mounts, `queued`, `running`, `parked`, `retained` | [`layerfs-fuse/src/dispatch`](../../core/crates/layerfs-fuse/src/dispatch) | Through the native serving assembly |
| Owner jobs per class | `OwnerWork`: `admitted`, `completed[6]`, `queue_wait_ns[6]`, `service_ns[6]`, `outstanding`, `queued`, `peak_queued`, maintenance jobs and rows | [`overlay/queue.rs`](../../core/crates/layerfs-daemon/src/overlay/queue.rs) | The six classes are Read, Mutation, Capture, Lifecycle, OperationRecord and Source ([`commands.rs`](../../core/crates/layerfs-daemon/src/overlay/commands.rs)) |
| SQL work per statement family | `StatementWork`: `executions`, `rows_returned`, `rows_changed`, `vm_steps`, `fullscan_steps`, `sorts`, `autoindex_rows`, `reprepares`, bytes, `elapsed_ns` | [`diagnostics/metrics.rs`](../../core/crates/layerfs-overlay/src/diagnostics/metrics.rs) | Fourteen families (`StatementKind`); foreground and maintenance are separate in `OwnerWork`; each job's completion carries its own receipt |
| Query plans of the overlay | `explain_*` functions per family | [`layerfs-overlay/src`](../../core/crates/layerfs-overlay/src) | Reached through owner commands that return plans |
| Stored rows and pages | `StoredCounts` | [`database/accounting.rs`](../../core/crates/layerfs-overlay/src/database/accounting.rs) | Plateau and leak checks |
| Base demands on the Store | `StoreWork`: `object_batches`, `object_ids`, `length_batches`, `length_ids`, `serial_reservations` | [`store/open.rs`](../../core/crates/layerfs-daemon/src/store/open.rs) | A warm phase must show zero object demands |
| Store reader pool | `ReadServiceWork`: `readers`, `quarantined`, `waiting`, `leased`, `peak_outstanding`, `grants`, `queue_wait_ns`, `maximum_queue_wait_ns` | [`store/read_service.rs`](../../core/crates/layerfs-daemon/src/store/read_service.rs) | Reader contention and quarantine |
| Namespace construction in a Commit | `CapturedNamespaceWork`: rows, pages, `parent_points`, `base_lookups`, headers, values, `files_constructed`, record jobs | [`construction/outcome.rs`](../../core/crates/layerfs-workspace/src/construction/outcome.rs) | In the Commit success or failure |
| Storage read and save path | `Diagnostics`: locate, pack reads and selections, reservations, publish, payload reads, locator and pack cache hits and evictions | [`read/counters.rs`](../../core/crates/layerfs-storage/src/read/counters.rs) | In the Commit outcome |
| Global Store SQL | `SqlWork`: statements, actual VM steps, transactions | [`sqlite/connection.rs`](../../core/crates/layerfs-persistence/src/backend/sqlite/connection.rs) | Separate from the overlay engine |
| Kernel queue of one connection | `waiting`, `max_background`, `congestion_threshold` | the mount's own entry under the fusectl filesystem | Read only your own connection. Nothing writes there except the product's own abort |

Existing count tests show the pattern for asserting work instead of time:
`job_cost`, `startup_cost`, `e2_startup_receipts` and `product_commit_cost` in
[`layerfs-daemon/tests`](../../core/crates/layerfs-daemon/tests).

External observers that need no product change: a syscall count of the command
on native ext4, the mount table, and per-process state under `/proc`. If a
profiler is unavailable in the pinned image, say so; do not patch a dependency
to get one.

## 4. Triage: which group is this?

**Step 1. Split by phase.** Time mount, command, Commit, unmount and later
cleanup separately. Work on the phase that dominates.

**Step 2. Form the ratios** for that phase.

| Ratio | From | Normal | Points to |
| --- | --- | --- | --- |
| FUSE requests per syscall | `OpcodeWork` against a native syscall count | near the passthrough arm's ratio | group B |
| Owner jobs per FUSE request | `OwnerWork.completed` against `handoffs` | about 5 today | group C |
| Statements per owner job | `StatementWork.executions` against jobs | a few | group E |
| VM steps per returned or changed row | `StatementWork` | small and flat | group E |
| Wait against service per class | `queue_wait_ns` against `service_ns` | wait well below service | group D |
| Object demands per cold request | `StoreWork` against requests | about 1, zero when warm | group F |
| Owner jobs per captured row | Commit work against rows | about 31 today | group H |
| Maintenance rows during the phase | `OwnerWork` maintenance | zero in a quiet phase | group J |

**Step 3. Pick the group.**

| What you see | Group |
| --- | --- |
| The command is fast, the call is slow | A: fixed cost |
| More requests than passthrough | B: kernel request volume |
| Same requests, many jobs per request | C: per-request daemon cost |
| Counts normal, wait counters high | D: queueing |
| Few statements, many VM steps each | E: overlay database |
| Slow only on first touch | F: cold base reads |
| Many tiny writes | G: write path |
| Commit dominated by record jobs | H: construction |
| Commit dominated by save or publish | I: Store save |
| Slowness right after an earlier unmount | J: cleanup |
| Fine alone, slow beside another Workspace | K: concurrency |
| Nothing above explains it | L: environment; then CPU and copies |
| Per-unit work rises as size doubles | Section 6, before anything else |

## 5. Diagnostic groups

Each group lists what to count, how to tell causes apart, the direction to
take, what not to do, and the proofs that must still pass afterwards. Test
names are binaries in `layerfs-daemon/tests` unless stated.

### A. Fixed cost per call: mount, unmount

Matters most for one Workspace per tool call.

- **Count.** Jobs and statements from Mount to Ready (`startup_cost`,
  `e2_startup_receipts`); Store demands during bind (a warm bind makes zero
  object demands but still pays a history read); jobs in Revoke and Close; the
  drain wait; maintenance rows after `Unmounted`.
- **Tell apart.** Bind cost that follows the size of the root is a scan and is
  a defect: bind is meant to be bounded. Unmount time that follows the number
  of lookups held is FORGET retirement. Time between `Unmounted` and `Gone` is
  cleanup and is reported separately, never charged to the reply.
- **Direction.** Keep the daemon, Store, readers, immutable cache and overlay
  alive across calls. Remove per-mount work that does not depend on the mount.
  Make unmount wait only for what it owns.
- **Do not.** Prefetch or materialize the tree at mount; recreate the overlay;
  hide cleanup inside the next call.
- **Context.** The experiment's passthrough floor was mount 17–20 ms and
  unmount 5–18 ms on an empty base.
- **Proofs.** `native_mount`, `native_mount_routes`, `mounted_cycles`,
  `mounted_drain`, `startup_cost`.

### B. Kernel request volume

The cheapest request is one the kernel never sends.

- **Count.** `OpcodeWork` per opcode for the command, on a fresh mount and
  again on the same mount. Compare with the passthrough arm under the same
  profile and with native syscalls.
- **Tell apart.**
  - LOOKUP or GETATTR repeating for the same name within 60 s: an
    invalidation or a reply that carried no lifetime.
  - Many LOOKUPs that end `ENOENT`: no negative-entry caching.
  - READDIR calls far above entries divided by 64: one window per reply.
  - One GETATTR per directory change: the cost of kept permissions.
  - FLUSH and RELEASE per open: elision is not negotiated.
  - Everything repeats on each fresh mount: kernel caches start empty per
    connection. Only the daemon's own caches carry over.
- **Direction.** Negative entries, adaptive READDIRPLUS, FLUSH elision, wider
  directory replies. Each needs a coherence proof first. For Git, check that
  inode number, change time and modification time are identical across mounts:
  in the experiment a second-mount `git status` took 0.58 s when identity
  survived and 4.4 s when it did not.
- **Do not.** Lengthen a lifetime, drop permissions, or enable kernel
  writeback to cut requests. Do not treat lifetime expiry as correctness.
- **Current profile.** 60 s entry and attribute lifetime, cached reads,
  writeback off, `default_permissions`, background depth 1
  ([`reply.rs`](../../core/crates/layerfs-fuse/src/request/reply.rs),
  [`profile.rs`](../../core/crates/layerfs-fuse/src/mount/profile.rs)).
  Changing it is an owner decision.
- **Proofs.** `native_coherence`, `native_mutation`, `native_mount`,
  `mounted_install`.

### C. Per-request daemon cost

Same number of requests as passthrough, but each costs more.

- **Count.** Owner jobs per handoff, by class. `inline` against `handoffs`.
  Bytes copied per byte requested.
- **Tell apart.** A request that takes a source, reads facts, takes a read
  ticket and releases is four round trips before any byte moves. Each round
  trip is a submit, a wake of the owner thread and a wake back. The experiment
  measured one FUSE `fstat` round trip at 44 µs unpinned and 5 µs on one CPU
  (a warm smoke, not a record), so wake-up latency can exceed the work.
- **Direction.** Fewer round trips: combine acquisitions that always occur
  together into one job; answer from already held state; serve more inline.
  Known candidate: every READ window takes a Store reader even when its bytes
  are wholly local (recorded limit in R6).
- **Do not.** Add workers or receive loops to hide round trips. Do not hold
  the owner across a Store read.
- **Proofs.** `filesystem_port`, `fenced_port`, `native_jobs`, `job_cost`,
  `native_coherence`, `mounted_parking`.

### D. Queueing and admission

Counts are normal but time is spent waiting.

- **Count.** `queue_wait_ns` against `service_ns` per class; `peak_queued`;
  a mount's `parked` and `queued`; reader `waiting` and
  `maximum_queue_wait_ns`; the connection's kernel `waiting`.
- **Tell apart.**
  - High owner wait with low service: one SQL owner thread serves every
    class. A Commit's jobs wait among filesystem requests with no priority.
  - `parked` at 16: the mount's admission is full; later requests wait in the
    kernel.
  - Reader `waiting` above zero: the fixed reader set is leased out.
  - Kernel `waiting` rising while the daemon is idle: background depth 1 is
    throttling readahead, not the daemon.
- **Direction.** Shorter jobs, fewer jobs, and an explicit ordering rule where
  a starved class is shown. Size the reader set from evidence.
- **Do not.** Raise a limit to make a wait disappear without naming the
  dependency that caused it. A wait is never turned into a retry.
- **Limits today.** Per namespace: 16 ordinary, 16 Source and 2 Lifecycle
  slots. Per mount: 16 admitted and 2 received.
  See [the owner](../../core/docs/architecture/21-daemon-owner.md) and
  [queue scheduling](workspace-queue-scheduling.md).
- **Proofs.** `owner`, `install_slots`, `admission_future`,
  `mounted_concurrency`, `mounted_parking`, `finite_service`.

### E. Overlay database

- **Count.** Rank statement families by `executions` and by `vm_steps` over
  the slow phase.
- **Tell apart.**
  - Many executions, few steps each: the caller asks too often. This is a
    batching problem (groups C and H), not a SQL problem.
  - Few executions, many steps each: a plan problem. Continue below.
- **Plan and profile together.** Take the family's plan from its `explain_*`
  function and the runtime counters from the same run. Bad signs: a `SCAN`
  where a `SEARCH` on an index was expected, `USE TEMP B-TREE`, an automatic
  index, `OFFSET` paging, and non-zero `fullscan_steps`, `sorts`,
  `autoindex_rows` or `reprepares`.
- **Other checks.** VM steps per row must stay flat as tables grow. Begin and
  Commit counts should match job counts: one short transaction per job.
  Returned blob bytes show payload copied through SQL.
- **Direction.** A covering index or keyset cursor for a real plan defect;
  otherwise reduce the number of jobs.
- **Do not.** Replace SQLite-backed state with a private in-memory tree. Do
  not hold a transaction across a file, a command or a Commit. The overlay
  stays MEMORY / OFF / EXCLUSIVE.
- **Proofs.** The `layerfs-overlay` tests, `job_cost`, `completion_storage`,
  `indexed_operation_record`.

### F. Cold base reads

Slow on first touch, fast afterwards.

- **Count.** `StoreWork` object and length demands per request; storage
  `Diagnostics` for pack reads, locate, locator and pack cache hits, misses
  and evictions; reader grants and wait.
- **Tell apart.** One demand per miss with neighbours missing separately is no
  coalescing. Evictions during a single command mean the cache allowance is
  too small for the working set. Demands on a warm repeat mean the cache key
  or authority differs, which is a defect.
- **Direction.** Group neighbouring misses into one bounded read plan;
  prefetch inside the same demand only where the format already stores
  objects together; keep identical objects shared across Workspaces.
- **Do not.** Credit warmth to a cold phase. Do not key the cache by path or
  branch name. Do not retry a failed read: a failed cold read ends its own
  request with `EIO`.
- **Proofs.** `store_read_service`, `cold_failure_scope`,
  `mounted_failure_scope`, `native_mount_routes`, the `layerfs-storage` tests.

### G. Write path

- **Count.** WRITE requests against bytes written; Mutation jobs; payload
  statements and `PayloadWork`; `AllocationWork`.
- **Tell apart.** Writeback is off by contract, so every `write(2)` is one
  publishing job and one transaction. Ten thousand 100-byte appends are ten
  thousand jobs. Bytes copied above bytes written point to partial-cell
  rewrite. Statement work that grows with file length for a fixed append is a
  scaling defect (section 6).
- **Direction.** Cheaper single mutation: fewer statements per write, no
  rewrite of unchanged cell bytes, fixed append tails.
- **Do not.** Enable kernel writeback or acknowledge a write before it is
  published.
- **Proofs.** `native_mutation`, `native_coherence`, `edit_backing`, the
  `layerfs-workspace` write and truncate tests.

### H. Commit: capture and namespace construction

The largest known cost.

- **Count.** `CapturedNamespaceWork`; owner jobs by class during the Commit,
  especially OperationRecord; statements; `product_commit_cost`.
- **Known today.**
  - A small Commit of 24 captured rows: 564 owner jobs, 485 of them record
    jobs, and 2,366 statements.
  - A large one of 672 rows: 21,182 jobs, 19,076 of them record jobs, and
    87,578 statements. That is about 31 jobs and 130 statements per row.
  - About 35 record point reads and 4 writes per changed row, each its own
    job. Roughly half answer "is this serial new" and "does this directory
    have a header".
  - A one-file update reads 70 to 86 inode pages in the sorted merge.
  - Base inode metadata is patched for every captured row with no compare
    first. A mode-only change goes through the changed-file constructor.
  - A directory moved across parents under a stored non-root directory lists
    the moved subtree once.
- **Direction.** One job that returns many sealed records; grouped point
  questions per window instead of per serial; skip rows whose metadata is
  unchanged. The first two change an interface between crates and need the
  owner.
- **Do not.** Walk the base; add a second construction producer; change the
  canonical format to carry a parent pointer without a ruling.
  `LAYERFS_CONSTRUCTION_WORKERS=1`.
- **Proofs.** `captured_namespace`, `captured_commit`, `product_commit`,
  `mounted_commit`, `mounted_install`, `mounted_commit_failures`,
  `product_commit_cost`, and the Init proofs when shared Content code changes.

### I. Commit: Store save, publication, install

- **Count.** Storage `Diagnostics` (reservations, publish, pack writes,
  forced seals); `SqlWork` statements, VM steps and transactions on the
  global Store; History statements; the install job.
- **Tell apart.** Save cost that follows unchanged content means
  deduplication is not reached before encoding. Repeated pack searches during
  publication show as locate counts above objects published. A contended
  Store writer is one exact `Busy`, never a wait.
- **Direction.** Lower save transaction and batch cost; avoid repeated pack
  searches; keep the Store open across Commits.
- **Do not.** Seal per file or per Commit; call any `fsync`-family function;
  add a retry on `Busy`.
- **Proofs.** `store_commit`, `product_commit`, the `layerfs-storage`,
  `layerfs-persistence` and `layerfs-history` tests.

### J. Cleanup and reclamation

- **Count.** Maintenance jobs and rows; `StoredCounts` before and after;
  database pages; the cleanup state of a closed namespace.
- **Tell apart.** Maintenance rows rising during a foreground phase mean an
  earlier unmount's cleanup is competing with it. Counts that do not return
  to baseline after a cycle are a leak. Pages that grow across identical
  cycles are a plateau failure.
- **Direction.** Output-sized work in bounded turns (64 rows today), indexed
  ownership updates, no whole-state scan per record.
- **Do not.** Report cleanup as part of the unmount reply, or skip it to make
  a later call look faster.
- **Proofs.** `mounted_cycles`, `mounted_drain`, `native_custody`, the
  `layerfs-overlay` lifetime tests.

### K. Concurrency across Workspaces and processes

- **Count.** Per-mount `completed` on each Workspace over the same interval;
  every owner class advancing; reader leases per Workspace; `peak_queued`.
- **Tell apart.** One Workspace's progress stopping while another runs is a
  shared wait: the owner thread, the reader set or the two receive loops. A
  Commit on A that slows B is owner contention; B must still advance.
- **Direction.** Keep jobs short; never hold a shared resource across a base
  read; make sure a parked request does not occupy a receive loop.
- **Do not.** Add a global lock. Do not claim a fairness share: none is
  declared.
- **Proofs.** `mounted_concurrency`, `mounted_parking`, `mounted_cycles`,
  `finite_service`, `install_slots`.

### L. Environment and build

Rule these out before blaming the product.

- A debug build. Every functional test so far ran one; a timed run needs a
  release build with the repository ARM flags.
- Store, overlay or scratch under the repository bind mount instead of the
  container's own filesystem.
- Other containers on the shared VM. Declare interference; never interrupt
  another owner's containers.
- Cache state that was assumed. A VM-wide cache drop proves nothing about
  residency and touches other owners.
- CPU placement. Unpinned round trips cost several times pinned ones.

Only after groups A to L are excluded, look at CPU and copies inside one
request: allocations per request, boxed futures, buffer copies between SQLite,
the payload cell and the reply.

## 6. Detecting worse-than-linear scaling

Do this with counts. Timing on a shared VM is too noisy to show an exponent.

1. **Name the dimensions.** Base size, changed rows, number of operations,
   file length, fragmentation, Commit history, concurrent Workspaces, queued
   work, cleanup debt. The guide's table gives the letters.
2. **Double one, hold the rest.** Run at 1×, 2× and 4×. For each counter
   compute the growth factor per doubling.

   | Factor per doubling | Shape | Verdict |
   | --- | --- | --- |
   | about 1 | constant | good |
   | about 2 | linear | good if the dimension is the output |
   | slightly above 2 | n log n | acceptable for indexed work |
   | about 4 | quadratic | reject |

3. **Check per-unit work.** Statements per changed row, jobs per request and
   VM steps per row must stay flat as the count grows.
4. **Check the dimensions that should not matter.** Work for a fixed change
   must not move when the base or unrelated rows grow. R4 showed identical
   validation counts at 1,103 and 69,647 inodes, and identical statement work
   beside 128 and 4,096 unrelated rows.
5. **Check across time.** The cost of cycle 10 must equal cycle 2. A
   per-operation cost that grows with operations already done is quadratic in
   total.
6. **Read the plan and the runtime flags** as in group E.
7. **Use adversarial shapes.** One huge directory, a deep tree, many hard
   links, a heavily fragmented file, thousands of tiny appends, many small
   Commits, many Workspaces. A small friendly case hides the problem.
8. **Turn the finding into a test** that asserts equal or bounded counts at
   two sizes, as the existing cost tests do.

Patterns the guide rejects on sight: a whole-state scan after every FORGET or
RELEASE; rebuilding a growing value after every append; `OFFSET` paging; a
linear membership search per changed key; a base walk after each small change;
a retained layer per Commit.

## 7. Procedure

1. **Prepare.** Harness arms for `L`, `N` and `P`; a sealed Store of the
   fixture; the per-file residency check. These are harness code under
   `core/benchmark/`, not product code. Read the
   [core harness guide](../../core/benchmark/fs-bench-pro/AGENTS.md) and the
   [report template](../../benchmark_agent_report.md) before each run.
2. **Baseline.** Every cell once, labelled exploratory and ineligible.
3. **Triage.** Split each cell by phase. Rank by gap to passthrough and
   native, weighted by how often a tool call pays it.
4. **Diagnose the top cell.** Counts, ratios, group, then that group's steps.
5. **Write the cause sentence.** If it cannot be written with numbers, go
   back to step 4.
6. **Write the candidate list** before changing anything.
7. **Apply one candidate per commit.** Recount; rerun the group's proofs;
   take one timed sample at the new identity; keep or revert. Owner direction
   2026-10-09: several ranked candidates, each its own commit with its own
   predicted counter, may share one final proof and one sample at the tip of
   the batch, as the
   [benchmark instruction](benchmark_instruction.md#one-batch) describes.
8. **Check scaling** on the changed path and add a count test.
9. **Record** what was kept, rejected and proposed.
10. **End when the list is exhausted.** Bring the measured gaps to the owner,
    who sets the targets. No numerical target is invented here.

## 8. Direction order when several apply

1. Fewer round trips (groups C and H).
2. Fewer requests (group B), each with its coherence proof.
3. Less waiting (groups D and K).
4. Fewer copies and smaller constants (groups E, F, G, I).
5. Lower fixed cost per call (groups A and J).

A dependency install followed by a Commit is dominated by the first. A walk of
a large tree is dominated by the second. Let the counts decide between them.

## 9. Current candidate list

Status at `2b4dc28a6`. Nothing here is built or selected.

| Candidate | Group | Evidence | Needs the owner |
| --- | --- | --- | --- |
| Batched sealed-record read | H | 19,076 of 21,182 Commit jobs are record point jobs | yes: crate interface |
| Grouped point questions in Content's streamed row source | H | about 16 "is new" and 11 "has header" questions per new inode | yes: crate interface |
| Answer "is new" and "has header" from sorted key windows inside the producer | H | estimated to halve record reads | no |
| Read-compare before patching base inode metadata | H | patched for every captured row | no |
| Sibling page reads in the sorted merge | H | 70–86 inode pages per one-file update | shared with Init: rerun Init proofs |
| Narrow the territory gate to a moved directory's own chain | H | R4 item 11 | no |
| Local READ without a Store reader | C | every READ window takes a read ticket | no, if the read contract is unchanged |
| Combine source, facts and release round trips | C | about 5 owner jobs per request | no |
| Status that waits for, or does not need, a Lifecycle slot | D | 297 of 310 samples had no engine fields under load | yes: control behaviour |
| Ordering between Commit jobs and filesystem jobs | D | 30,327 jobs admitted during one Commit | yes |
| Negative entries, adaptive READDIRPLUS, FLUSH elision | B | request reductions seen in the experiment | yes: negotiated profile |
| Wider READDIR replies | B | one 64-name window per reply | check the cookie contract first |
| Save transaction and batch cost; repeated pack searches | I | #313 candidates 1 and 4 | no |
| Partial-cell copies; cache-hit cost under concurrency | G, F | #313 candidates 2 and 3 | no |
| Parent pointer or ancestry evidence | H | cross-parent directory move lists a subtree | yes: canonical format, proposal only |

## 10. What to write down

**Cause sentence.**

```text
<phase> of <case> spends <count> <unit> because <mechanism>;
expected <count> from <model>. Evidence: <receipt>, <counter>.
```

**Candidate entry.**

```text
Candidate:      <one line>
Group:          <A–L>
Counter:        <which count must drop, from what to what>
Risk:           <what could break>
Proofs to keep: <test binaries>
Owner decision: <none | which contract>
Outcome:        <kept | rejected | proposed>, receipt <path>
```

Receipts are append-only under the stage's `checks/` directory. A failed
attempt keeps its name and is never relabelled. Every commit carries the
production LOC comparison.

## 11. What never counts as a fix

- More threads, receive loops or background depth, without the named
  dependency they relieve.
- A larger limit, a longer cache lifetime, or a longer timeout.
- A path that recognises a benchmark command.
- Dropping permissions, coherence, identity or a validation.
- A retry, a busy handler or a replay of a failed operation.
- A second route, a private mutable mirror, or a helper producer.
- Rerunning an unchanged case until it looks better.

## 12. References

- Rules: [optimization guide](optimization-guide.md),
  [measurement workflow](agent-measurement-policy.md),
  [benchmark rules](benchmark_rules.md).
- Design: [FUSE contract](../../core/docs/issues/303/fuse.md),
  [FUSE optimization investigation](../../core/docs/issues/303/fuse-optimization-investigation.md)
  (research), [engine](../../core/docs/issues/303/daemon-sqlite.md),
  [cache design](sandbox-cache-design.md).
- Architecture: [owner](../../core/docs/architecture/21-daemon-owner.md),
  [request service](../../core/docs/architecture/75-native-request-service.md),
  [mount session](../../core/docs/architecture/76-native-mount-session.md),
  [mutation coherence](../../core/docs/architecture/77-native-mutation-coherence.md),
  [namespace construction](../../core/docs/architecture/78-captured-namespace-construction.md),
  [product Commit](../../core/docs/architecture/79-product-commit.md),
  [Store Commit](../../core/docs/architecture/65-store-commit-composition.md),
  [forced teardown](../../core/docs/architecture/80-forced-teardown.md).
- Scenario matrix, cache classes and owner rulings:
  [proof plan](../../core/docs/issues/307/S8-PROOF-PLAN-20261008.md).
- Priorities and component candidates: issues
  [#314](https://github.com/Ephemeral-AI-Lab/layerfs/issues/314) and
  [#313](https://github.com/Ephemeral-AI-Lab/layerfs/issues/313).
