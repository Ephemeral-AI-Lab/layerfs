# LayerFS fs-bench-plus self-improving implementation handoff

You are the sole implementation owner for completing, instrumenting, measuring,
and optimizing LayerFS fs-bench-plus in:

~~~text
/Users/yifanxu/Ephemeral-AI-Lab/layerfs
~~~

Continue until the implementation is correct, the public-SDK benchmark is
complete, every required verification gate passes, the terminal formal
campaign passes its optimization gates, and an evidence-backed exhaustion
review finds no remaining actionable optimization. A failed test, invalid run,
regression, weak result, missing timer, unexplained phase, audit finding, or
`REVISE` verdict requires diagnosis, replanning, correction, and another
measured iteration. It is not a stopping condition.

Do not commit or push unless the user explicitly requests it.

## 1. Authority and required reading

Read these files completely before editing:

1. `docs/v2/spec.md`
2. `docs/v2/sdk-cli-operation-families.md`
3. `docs/fs-bench-plus/spec.md`
4. this handoff prompt

The V2 specification remains authoritative for product architecture.
`docs/fs-bench-plus/spec.md` remains authoritative for workload, fairness,
evidence, statistics, storage accounting, and performance gates except for one
explicit refinement in this handoff:

> The public operation set is frozen to operations that already exist in the
> current SDK. Do not add or use a new public operation, command, endpoint,
> operation family, benchmark-only API, or active selector. In particular, do
> not add `Client::durability_barrier()`. If matched durability needs stronger
> behavior, implement it internally within the existing Commit/Push lifecycle
> and expose passive timing/outcome evidence through existing receipts and
> Monitor data.

Before changing source, inspect:

~~~text
git status --short
git diff --stat
git diff
git log -1 --oneline --decorate
rg --files
Cargo.toml
Cargo.lock
every affected Cargo.toml
all callers of every changed public type or function
current focused tests
benchmark/fs-bench
benchmark/fs-benchmark-pro
benchmark-results/fs-bench-plus
current Monitor, receipt, tracing, and evidence paths
~~~

The worktree contains user and previous-agent changes. Preserve all unrelated
changes. Never reset, revert, overwrite, stage, commit, push, delete, or absorb
them to simplify this task. Preserve the existing TUI removal. Use FUSE;
OverlayFS remains deferred.

## 2. Required deliverables

Complete all of the following:

1. Implement the complete fs-bench-plus public-SDK benchmark in
   `benchmark/fs-benchmark-pro`.
2. Preserve the frozen base suite in `benchmark/fs-bench`; do not rewrite it to
   make the comparison favorable.
3. Run LayerFS Reference against pinned upstream Computer through the exact
   public product paths in `docs/fs-bench-plus/spec.md`.
4. Add the timers, passive counters, structured traces, and evidence needed to
   attribute execution, Commit, Push, durability, storage, and FUSE
   materialization.
5. Route production work through the existing optimized algorithms in
   `crates/layerfs-content` and `crates/layerfs-storage` instead of rebuilding
   equivalent logic in Workspace, SDK, FUSE, Store, or benchmark code.
6. Fix correctness, wiring, algorithm, transaction, resource, and performance
   defects found by each run.
7. Maintain one append-only optimization history at:

   ~~~text
   /Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-plus/optimization-history.md
   ~~~

8. Retain raw evidence for every run under:

   ~~~text
   /Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-plus/runs/<run-id>/
   ~~~

9. Finish with full repository verification, a valid 30-pair formal campaign,
   storage and recovery proof, and a final optimization-exhaustion audit.

## 3. Frozen public operation allowlist

Measured LayerFS setup may use only these existing public operations and
types:

~~~text
LayerStackStore::create
BranchStore::create
Client::connect
Client::initialize_layerstack
Client::pull_layer with RemotePlacement::Reference
Client::fork_branch with LocalForkSource
~~~

Each measured mutating checkpoint must use only:

~~~text
Client::create_workspace_session with WorkspaceProjection::Fuse
Client::exec_workspace_session
Client::workspace_output
OutputReader::read through a terminal ExecutionReceipt
Client::commit_workspace_session
Client::push_branch
Client::end_workspace_session with EndWorkspaceMode::Clean
~~~

The sealed helper may use ordinary filesystem operations through the real FUSE
mount, including open, create, read, write, pwrite, append, truncate, rename,
unlink, mkdir, fsync, close, and temp-copy-fsync-rename.

Read-only evidence after the relevant timer stops may use only:

~~~text
Client::monitor_snapshot
Client::analyze_dedup
Client::query
LayerStackStore::inventory_page
BranchStore::inventory_page
BranchStore::root_complete
read-only filesystem metadata for DB/WAL/SHM allocation
~~~

`Client::add_layer` may be used only as a separately labeled diagnostic after
the Computer-comparable boundary. It must not enter headline latency or
storage.

Every materialization row means FUSE materialization: make the selected
immutable root available in the admitted container through
`WorkspacePlacement::Container` and `WorkspaceProjection::Fuse`, then measure
mount readiness and exact first access. Host placement and
`WorkspaceProjection::Materialize` are out of scope.

Implement and report the FUSE rows frozen by the specification, including cold
mount, proven-warm fresh mount, same-mount no-change, next checkpoint after a
ten-byte edit, append, truncate, rename, temp-prepend, full rewrite, first read,
repeat read, and fresh-process remount/recovery. At the current SDK lifecycle,
a successful Commit makes the prior Workspace non-executable, so next-root rows
must include the required new Workspace/helper/mount instead of pretending the
old mount can continue.

Do not add or use:

~~~text
Client::durability_barrier or any other new public operation
new CLI commands or operation families
WorkspacePlacement::Host or WorkspaceProjection::Materialize in this benchmark
docker exec for the registered filesystem workload
BranchStore::commit_changes from the benchmark
ContentChange::Splice as a user-facing benchmark operation
ObjectBuffer or StoreDb from the benchmark
direct SQLite reads or writes
private mutation/admission helpers
test-only hooks
benchmark-only fast paths
fixture-specific paths, digests, offsets, markers, roots, or ObjectIds
~~~

The public operation vocabulary is frozen; internal implementation,
instrumentation, receipts, and algorithms may be optimized.

## 4. Frozen Docker environment

Use the dedicated fs-bench-plus images. Do not substitute the simpler
`containers/layerfs-fuse/Dockerfile` product/base-fs-bench image for the
public-SDK controller image.

LayerFS image:

~~~text
Dockerfile: benchmark/fs-benchmark-pro/Dockerfile.layerfs
tag: layerfs-fs-benchmark-pro:local
build base: rust:1.85.1-bookworm
runtime base: rust:1.85.1-bookworm
required binaries:
  /usr/local/bin/fs-benchmark-pro
  /usr/local/bin/layerfs-fuse
  /usr/local/bin/docker
~~~

Build it from the exact measured checkout. OCI labels must match the current
commit, tree, dirty state, and source-seal SHA-256. Rebuild after every source,
benchmark, manifest, lockfile, or container-definition change. The runner must
reject a stale or mismatched image.

Computer image:

~~~text
Dockerfile: benchmark/fs-benchmark-pro/Dockerfile.computer
tag: layerfs-fs-benchmark-pro-computer:de87919a
build base: node:22.22.0-bookworm
runtime base: node:22.22.0-bookworm-slim
upstream commit: de87919a4fd37242e960e13b7b3ba802d1eef0a0
upstream tree: 4fb409d7e1356e1098439293d77d2fdc2dbf2190
~~~

Build Computer from the exact sealed upstream `git archive` and admitted
adapter files only. Do not patch the product source. Both image architectures
must match.

Every arm uses a fresh container with exactly:

~~~text
--privileged
--device /dev/fuse:rwm
--cap-add SYS_ADMIN
--security-opt apparmor=unconfined
--security-opt seccomp=unconfined
--network none
--cpus 1
--memory 1g
--memory-swap 1g
--pids-limit 512
--tmpfs /tmp:rw,nosuid,nodev,size=256m
~~~

The LayerFS controller may mount `/var/run/docker.sock` only because the
production Container-placement path uses Docker to copy, start, pause, resume,
and remove the FUSE helper in the target container. The measured Workspace must
target that fresh admitted container at a registered `/workspace/...` mount
path.

An outer `docker exec` may start the sealed `fs-benchmark-pro` controller in
the admitted container. It may not execute a registered read or mutation. The
controller must launch every registered filesystem command through:

~~~text
Client::exec_workspace_session
Client::workspace_output
OutputReader::read through a terminal receipt
~~~

Docker calls made internally by the production `DockerProjection` to attach or
control the FUSE helper are legitimate timed product work. The current
`timed_docker` workload shortcut in `benchmark/fs-benchmark-pro/src/main.rs` is
a known invalid wiring path and must be removed before the first valid result.

The current runner also writes to `benchmark-results/fs-benchmark-pro`. Change
its result root to the user-frozen `benchmark-results/fs-bench-plus`, where the
single append-only history and all per-run raw directories live.

Each LayerFS trial uses a fresh measurement container. Recovery uses a second
fresh container over retained Store files. Each Computer trial also uses a
fresh container. No container, mount, Workspace, Store, or application cache
crosses trial boundaries.

## 5. Fairness and anti-cheating rules

Use exactly two candidates:

~~~text
Computer upstream
LayerFS Reference
~~~

Do not add C3, Replica, OverlayFS, multi-agent execution, concurrent writers,
or an engine-only candidate.

Both candidates must use the same sealed high-entropy fixture and the same
byte-identical helper, syscalls, flags, edit order, buffer sizes, fsyncs, closes,
and rename sequence. Mount the fixture and helper read-only outside both
measured namespaces. They must never be synchronized, chunked, canonicalized,
or counted as candidate storage.

Never:

- hardcode benchmark names, paths, data, offsets, markers, roots, or expected
  results in production code;
- precompute final files, chunks, extents, ObjectIds, changed-object lists, or
  expected roots;
- disable or delay durability;
- use sparse holes, reflinks, hard links, compression-friendly zero data, a
  hidden surviving mount, or a pre-read target unless the scenario explicitly
  requires that mechanism for both candidates;
- bypass FUSE or the public SDK;
- warm only one candidate;
- retain candidate state across trials;
- label an uncontrolled OS page cache as cold;
- remove outliers, silently rerun a failed arm, select a best run, or tune after
  seeing a formal result;
- move expensive work outside a timer, into setup, into an unmeasured thread,
  or after acknowledgement;
- add a counter that performs a full scan not already required by production
  work.

Natural state created by earlier registered operations in the same scenario is
product behavior. Explicit warm and cold scenarios remain separate and are
never pooled.

## 6. Reuse the existing optimized algorithms

Before optimizing a slow phase, trace the complete call path and all callers.
Prefer fixing the shared production primitive once.

Existing-file edits must route to the persistent content algorithms in
`crates/layerfs-content` and storage/admission primitives in
`crates/layerfs-storage`:

~~~text
exact normalized dirty ranges
  -> compare only the registered dirty bytes
  -> FileMutationBatch
  -> persistent extent split/join/replacement
  -> persistent inode/directory/namespace mutation
  -> bounded canonical admission
~~~

For a small base-backed edit, do not hash, CDC-scan, copy, eagerly reconstruct,
or rebuild the complete file. Do not copy authority-owned Reference objects
into BranchStore. Candidate admission must distinguish:

~~~text
inserted locally
reused locally
reused through the immutable parent route
~~~

Push must use the locally owned suffix, known-root pruning, bounded membership
batches, missing-only payload transfer, and visibility-last publication. It
must send zero pulled-ancestry facts and zero authority-owned base payload.

Full new files and opaque temp-copy-rename files may scan their input once.
Keep their construction streaming and bounded; avoid a second file-sized
scratch copy, delayed full-candidate buffering, or copying parent-present
canonical objects into BranchStore.

You may optimize inefficient algorithms anywhere in the in-scope production
path, including content, storage, BranchStore, LayerStackStore, Workspace,
FUSE, SDK internals, Monitor, FUSE projection, and the benchmark harness.
Preserve V2 identities, canonical encodings, CDC behavior, transaction
boundaries, failure atomicity, and the public operation set.

No SQLite write transaction may contain network I/O, complete history or
closure enumeration, CDC, hashing, or unbounded work. Preserve bounded memory,
bounded batches, exact missing-only transfer, and immutable publication.

## 7. Timers, traces, counters, and logging

Add enough instrumentation to explain every material phase. Instrumentation
must be passive, bounded, structured, low-overhead, and independent of fixture
contents.

At the harness boundary, time at least:

~~~text
sdk_workspace_create_ns
sdk_exec_dispatch_ns
sdk_output_to_terminal_ns
sdk_workspace_commit_ns
sdk_branch_push_ns
sdk_workspace_end_ns
complete_checkpoint_ns
complete_scenario_ns
reopen_ns
~~~

Because no new durability operation is allowed, include required two-Store
checkpoint/fsync work inside the existing Commit/Push lifecycle, attribute it
inside those existing operation timers, and expose its fragments through the
existing receipt/Monitor path:

~~~text
branch_store_checkpoint_ns
branch_store_database_fsync_ns
branch_store_directory_fsync_ns
layerstack_store_checkpoint_ns
layerstack_store_database_fsync_ns
layerstack_store_directory_fsync_ns
durability_unattributed_ns
~~~

Attribute Workspace creation:

~~~text
root_pin_ns
projection_attach_ns
workspace_create_unattributed_ns
~~~

Attribute Commit:

~~~text
pause_quiesce_ns
capture_ns
candidate_plan_ns
dirty_compare_ns
content_mutation_ns
namespace_mutation_ns
local_admission_ns
completeness_verify_ns
commit_publish_ns
workspace_reload_ns
projection_transition_ns
commit_unattributed_ns
~~~

Attribute Push:

~~~text
push_history_ns
push_frontier_ns
push_membership_ns
push_object_admission_ns
push_fact_admission_ns
push_authority_verify_ns
push_publish_ns
push_durability_ns
push_unattributed_ns
~~~

`push_durability_ns` contains the two-Store checkpoint/fsync fragments listed
above. `Client::push_branch` must not return before that durability phase
completes. The benchmark must not add a separate public durability call or
double-count the same fragment outside `sdk_branch_push_ns`.

Attribute FUSE materialization and first access:

~~~text
container_start_ns
client_connect_ns
root_pin_ns
workspace_registry_ns
proxy_start_ns
helper_copy_ns
helper_start_ns
mount_ready_ns
projection_attach_ns
first_lookup_ns
first_stat_ns
first_read_ns
complete_read_ns
projection_pause_ns
projection_unmount_ns
projection_cleanup_ns
fuse_materialization_unattributed_ns
~~~

Record the exact image digest, container ID, mount path, immutable root, helper
binary SHA-256/PID, proxy identity, in-container mountinfo, eager reconstructed
bytes, attach-time payload reads, attach-time complete-file hash/CDC bytes,
first/repeated-read bytes, page faults, process/cgroup I/O, and lifecycle
residue. FUSE attach must not eagerly reconstruct or copy the root.

Expand counters when existing counters cannot explain a phase. Include exact
work quantities such as compared bytes, CDC bytes, dirty bytes, payload reads,
node reads/creates, candidate inserted/local-reused/source-reused bytes,
scratch, membership pages, announced/sent/avoided bytes, verifier work, and
peak bounded buffers.

Each structured event should contain, where applicable:

~~~text
schema version
run ID
operation ID
operation family
Workspace ID
LayerStack/Branch/Commit IDs
phase
monotonic start/end or elapsed_ns
relevant byte/object/fact counters
outcome and typed error
~~~

Do not log payload bytes, secrets, environment credentials, or unbounded
per-object event streams. Do not synchronously format or flush verbose text in
the hot path. Buffer structured events and attach them to the existing
operation/Monitor evidence. Every phase equation must balance; keep an explicit
`*_unattributed_ns` until it is understood rather than forcing totals to match.

## 8. Append-only optimization history

The only cumulative run report is:

~~~text
benchmark-results/fs-bench-plus/optimization-history.md
~~~

Create it if absent. Only the primary handoff agent may append to it. Subagents
return findings to the primary agent and must not edit the ledger.

Never rewrite, reorder, delete, or silently correct an earlier round. Record a
later erratum when needed. Every benchmark campaign, including an invalid,
failed, interrupted, or regressed run, gets one appended round.

Use this exact per-round structure:

~~~markdown
## Round NNN — <run-id>

- Status: BASELINE | INVALID | FAILED | REGRESSED | IMPROVED | TERMINAL-CANDIDATE
- UTC timestamp:
- Local timestamp and timezone:
- Git commit and tree:
- Dirty source seal and diff hashes:
- Benchmark/profile and exact commands:
- Candidate order seed and pair count:
- Host, kernel, Docker, CPU/memory/I/O envelope:
- Candidate image tags, digests, architectures, and verified OCI labels:
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256:
- Raw evidence directory and SHA-256 inventory:
- Previous comparable round:
- Current best comparable round:

### Hypothesis and planned change

### Changes since the previous round

### Correctness and validity

### Comparable E2E results

### LayerFS phase decomposition

### Algorithm, transfer, storage, memory, and I/O counters

### Comparison with Computer, previous round, and current best

### Defects and root causes

### What needs improvement next

### Stable strengths — no improvement currently needed

### Subagent reviews and reconciled decision

### Next action
~~~

The results section must contain raw sample count, median, Q1/Q3, min/max,
paired ratio/speedup, confidence interval when the profile permits it, and
wins/ties/losses. Never manufacture an aggregate by summing independent
medians.

The defects section must identify the responsible phase, production call path,
evidence, expected complexity, observed complexity, and root-cause hypothesis.
The stable-strengths section is mandatory: state which components already meet
their mechanism/performance/resource gates, remain stable across comparable
runs, and should not be changed without new contrary evidence.

Raw evidence belongs in the round's `runs/<run-id>/` directory, not inline in
the ledger. Link it from the ledger and record its source seal and hashes.

## 9. Self-improving iteration loop

Begin with a current-source baseline before making optimization changes. Then
repeat this loop:

1. **Measure.** Run the smallest benchmark profile that reproduces the current
   bottleneck. Preserve raw output even when it fails.
2. **Validate.** Reject bad provenance, wrong public path, oracle failure,
   incomplete durability, cache asymmetry, missing receipt, or broken equation
   before interpreting performance.
3. **Attribute.** Use timers, traces, counters, system resources, and SQL/transfer
   evidence to locate the dominant phase and distinguish useful work from
   amplification.
4. **Review.** Before the next implementation round, use independent subagents
   when capacity permits to review: algorithm/wiring, fairness/no-cheating,
   correctness/transactions, and performance/statistics. Give them disjoint
   scopes. Tell them others share the worktree, to preserve unrelated changes,
   never revert another contributor, and return evidence-backed findings.
5. **Replan.** Reconcile subagent findings yourself. Rank candidate changes by
   correctness risk, expected E2E effect, storage effect, memory effect, and
   reversibility. Do not optimize a phase already below noise while a larger
   defect is unexplained.
6. **Implement.** Make the smallest root-cause production change that improves
   the public path. Reuse existing content/storage primitives. Do not implement
   benchmark specialization.
7. **Verify narrowly.** Run formatting and the smallest focused tests that fail
   without the change, followed by direct dependents.
8. **Rerun comparably.** Use the same fixture, profile, resource envelope, cache
   classification, boundaries, and evidence schema. Do not compare unlike
   profiles as if paired.
9. **Append.** Add the complete round to `optimization-history.md`, including
   failures, regressions, stable strengths, and the next decision.
10. **Escalate evidence.** Move from self-check to smoke, focused repeated run,
    pilot, and formal only as the implementation stabilizes.

If a change improves an internal micro-timer but regresses complete public E2E,
storage, correctness, or resource safety, it is not an improvement. Diagnose
the interaction and continue.

Do not repeatedly perturb stable code to satisfy “keep optimizing.” An area is
exhausted when its work equations are optimal for the registered operation,
its contribution is at or below measurement noise or unavoidable public-path
cost, and independent review finds no safe evidence-backed improvement.

## 10. Benchmark progression

Use this progression:

~~~text
self-check:
  schema, sealed helper, oracles, receipts, equations, report generation

smoke:
  one adjacent pair; functional and attribution proof only

focused iteration:
  repeated affected scenarios sufficient to distinguish signal from noise;
  no broad superiority claim

pilot:
  10 complete adjacent randomized pairs; variance and direction qualification

formal terminal:
  30 complete adjacent randomized pairs; registered headline verdict
~~~

Do not run the 30-pair campaign after every small edit. Do not declare victory
from smoke or a favorable single run. Use focused profiles for iteration and
the frozen formal profile only after correctness, attribution, and pilots pass.

Every formal attempt is retained and disclosed. A source, configuration,
fixture, helper, evidence-schema, or benchmark change starts a new campaign and
source seal.

## 11. Required optimization order

Let evidence choose the exact patch, but normally inspect bottlenecks in this
order:

1. correctness and durability;
2. public Workspace creation and projection attach;
3. exact dirty-range propagation into persistent extent mutation;
4. removal of full-file hashing, CDC, comparison, rebuild, or scratch from
   small existing-file edits;
5. Reference-aware local/parent membership and admission;
6. Commit closure verification and publication;
7. Push owned-suffix traversal, known-root pruning, membership batching,
   missing-only payload, and authority verification;
8. SQLite transaction duration, checkpointing, fsync grouping, and connection
   reuse inside existing operation semantics;
9. one-pass bounded construction for new files and opaque replacements;
10. reads, dense writes, FUSE materialization/first access, memory, and physical
    I/O.

Do not assume this list identifies the current bottleneck. Confirm every choice
with current-round evidence.

## 12. Verification after each change

After each semantic change:

1. run the smallest focused failing test;
2. run direct dependent tests;
3. run the affected benchmark scenario;
4. verify exact final bytes and namespace;
5. verify retained roots and fresh-process reopen;
6. verify candidate/transfer/storage equations;
7. verify memory and transaction bounds;
8. append the run report.

Add focused regression tests for every root cause. Tests must prove behavior,
not the benchmark fixture. Include varied paths, offsets, data, file sizes, and
operation orders so fixture-specific shortcuts cannot pass.

## 13. Terminal gates

Do not stop at compilation, unit tests, a faster microbenchmark, a favorable
smoke run, a passing pilot, or plausible prose.

Terminal requires all of the following together:

1. Every required fs-bench-plus scenario is implemented through the frozen
   existing public SDK operation set and real FUSE.
   Cold, warm, incremental, first-read, repeat-read, and process-cold
   materialization all use Container placement plus `WorkspaceProjection::Fuse`;
   none use Host directory export.
2. All independent oracles, namespace checks, retained-root checks,
   fresh-process recovery, transaction rules, bounded-memory rules, and
   missing-only equations pass.
3. No benchmark specialization, hidden cache, direct internal mutation,
   durability weakening, lifecycle exclusion, or favorable rerun exists.
4. Complete public E2E—not only Commit or an internal phase—passes every
   registered terminal performance gate in `docs/fs-bench-plus/spec.md`.
5. Reference Fork copies zero authority base objects, small edits remain
   independent of file size, Push sends no pulled ancestry payload, and
   incremental semantic storage passes its registered gates.
6. All timers balance with bounded, explained unattributed time. Every dominant
   phase has an evidence-backed explanation.
7. The final 30-pair campaign is valid, immutable, fully reported, and linked
   from the append-only ledger.
   Its image digests/labels, architectures, Dockerfiles, resource envelope,
   container lifecycle, `/dev/fuse`, in-container mountinfo, and fresh recovery
   container match the frozen environment section.
8. Full repository verification passes:

   ~~~text
   cargo fmt --all -- --check
   cargo test --workspace --all-features
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   git diff --check
   ~~~

9. Independent final audits cover algorithm correctness, Reference placement,
   deduplication/storage, transfer, Workspace/FUSE wiring, transaction safety,
   fairness/no-cheating, statistics, and raw evidence custody. No actionable
   P0/P1 issue remains.
10. The final exhaustion review lists every remaining nonzero or unattributed
    phase and every rejected optimization idea with evidence. There is no known
    safe in-scope change expected to improve a failed gate or materially improve
    complete E2E, storage, or resource use without violating correctness or the
    frozen public operation set.

“Nothing to optimize” means no remaining evidence-backed actionable
optimization, not theoretical proof of a global optimum. Stable components
that already meet their gates should be recorded and left alone.

## 14. External blockers

Stop only for a genuine external blocker after exhausting safe in-repository
alternatives and all independent work. A difficult implementation, regression,
missing timer, failed test, weak benchmark, or architectural defect is not an
external blocker.

If genuinely blocked, append the blocked round to the ledger and report:

1. the exact external action required;
2. the evidence proving the blocker is external;
3. every attempted in-repository alternative;
4. all completed independent work;
5. the exact command and expected result that will resume the loop.

At completion, report the final source seal, operation allowlist, implemented
timers/counters, optimization history path, raw final evidence path, formal
Computer comparison, storage/deduplication results, recovery proof, full test
gates, final audit conclusions, stable strengths, and rejected optimization
ideas.
