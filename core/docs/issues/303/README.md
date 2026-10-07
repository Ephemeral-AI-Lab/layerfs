# Phase 7 cluster two — architecture and implementation design

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Issue [#303](https://github.com/Ephemeral-AI-Lab/layerfs/issues/303). Written
> 2026-10-05 against `main` `f96d97651be5299f153ccde2bc8d921dd58807ad`.
> This set is a design. It contains no product code and claims no
> implementation, measurement, qualification or release admission. No build,
> test or benchmark was run to produce it.

This is the single entry point for the cluster two design: Workspace, FUSE,
daemon, sandbox and Commit integration. It reconciles the prepared #301, #303
and #304 documents, the #305 and #306 experiments, the current source and the
completed cluster one work. It supersedes the untracked
`IMPLEMENTATION-PLAN.md` of 2026-10-03, which assumed PostgreSQL and MinIO.

"Cluster one" and "cluster two" are workstreams. C1, C2 and C5 in older
documents mean `layerfs-content`, `layerfs-storage` and `layerfs-history`.

Owner migration direction: both workstreams implement in `core/`. Root `crates/`
is the v0.1.6 reference retained for explicit inspection/baseline comparisons;
remove it once cluster two is complete. It is not a core dependency or fallback,
and that future retirement does not authorize deleting it before completion.
Current [agent routing](../../../../AGENTS.md) replaces closed-stage assignments
with these contracts; source and measurement identities remain unchanged.

Review revision 2026-10-05: supersedes the algorithms and bounds of design
`334fc743751b9a181e670d0601a24fb3169208f9` where identified below. Product
source remains pinned to `f96d97651`; no implementation or new measurement
accompanies this revision. Required corrections and proof obligations are
tracked in [README](README.md#required-corrections-before-implementation).

Owner update 2026-10-05: one local overlay SQLite database per daemon, initialized
once before readiness; Workspace rows are namespaced within it. Bash Exec has
no automatic runtime timeout. This supersedes the per-Workspace-file proposal;
shared writer/pager/failure accounting and fair admission apply below.

## Primary design documents

Seven primary contracts own the current operation and implementation design.
They include workloads, exact outcomes, source gaps and ASCII workflows; none
claims implementation or load-bearing qualification.

| Document | Authoritative subject |
| --- | --- |
| [mount](workspace-api/mount.md) | Complete-root readiness, fast logical bootstrap, FUSE attach and partial startup custody |
| [Exec](workspace-api/exec.md) | Ordinary Bash, no automatic timeout, streaming process I/O and concurrent filesystem activity |
| [Commit](workspace-api/commit.md) | Full affected-state capture, construction/publication, concurrent Saves, outcomes and retention |
| [terminal unmount](workspace-api/unmount.md) | Detach, logical close, activity fences and automatic physical cleanup, no separate close |
| [status](workspace-api/status.md) | Bounded read-only observation, maintained counters and outcome knowledge |
| [daemon + SQLite](daemon-sqlite.md) | Metadata and physical payload, indexes/caches, ownership, service windows and shared-database scheduling |
| [FUSE](fuse.md) | Kernel profile, callbacks, coherence, enumeration, identity and optimization disposition |

Supporting documents: [architecture](01-architecture.md),
[cluster-one runtime integration](06-cluster-one-integration.md),
[implementation/validation](07-implementation-validation.md),
[decisions/provenance](08-decisions-provenance.md).
[02](02-base-overlay.md), [03](03-mutation-hot-path.md) and
[04](04-concurrency-commit.md) preserve source inventories/rationale and route to
primary contracts. [05](05-fuse-assessment.md) preserves experiment/source assessment;
its original targets are not a second implementation authority.

The [current implementation sequence](07-implementation-validation.md#3-slices)
uses S0–S13: profiled/indexed engine and ownership first, parallel runtime and
cluster-one constraint corrections, mounted execution, incremental Commit,
integrated qualification, then root-v0.1.6 retirement. EXPLAIN/runtime profiling
and quadratic-work rejection apply from the first engine slice.

The [proposed file layout](07-implementation-validation.md#44-file-ownership)
separates SQLite overlay, filesystem semantics, FUSE adaptation and daemon service
ownership. Its host runtime adapters are superseded: since the owner direction of
2026-10-07 the daemon opens the Store itself, and the
[serverless file plan](../307/SERVERLESS-STORE-PLAN-20261007.md) owns the changed
layout and size. The earlier
[size estimate](07-implementation-validation.md#43-estimated-future-size)
is 19,500–27,000 production LOC for the replacement cluster-two scope, including
runtime SQL and adapters, against the unchanged 43,165-line excluded source.
It is a planning range, not achieved size, a performance result or a size gate;
cluster-one prerequisite changes and later root-reference retirement are accounted
for separately.

The [implementation environment](07-implementation-validation.md#21-checkout-and-execution-environment)
uses the existing primary checkout on local `main`: macOS ARM64 for current global
provider/SDK work and Linux ARM64 Docker for daemon/FUSE/Bash integration. Runtime
and daemon database initialization are reused across Workspace opens. Native
Docker/FUSE/build qualification remains S0 work; branch preparation is not an
implementation or measurement result.

[Implementation tracker #307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307)
records S0–S13 progress, checkpoint commits, milestone exit evidence and remaining
work. The [handoff prompt](09-implementation-handoff.md) directs continuous
iteration, exact per-commit LOC accounting and a tracker update after every
completed milestone. Several checkpoint commits may belong to one milestone;
preparing this tracker/prompt does not complete any implementation slice.

The [per-tool-call FUSE investigation](fuse-optimization-investigation.md)
synthesizes three parallel source/primary-document studies. It records request,
mutation and lifecycle alternatives plus newly traced integration constraints;
it is research, not a changed mount profile or performance qualification.

Read this review first, then the architecture, the seven primary contracts and
integration/validation. An evidence reviewer also reads 05 and 08.

## Pre-write review of the owner's seven questions

Three subagents reviewed load-bearing readiness before producing their assigned
operation/engine documents. The consolidated findings are:

| Question | Required design / current finding |
| --- | --- |
| Is it load-bearing? | Required, not achieved or measured. R1–R8 and cluster-one/source prerequisites remain; topology/bounded queues alone do not qualify correctness or throughput |
| Fast Workspace per tool call? | Prepare full canonical base and initialize daemon/runtime once; support mount/execute/Commit/unmount at one-call granularity, while a Workspace can also serve many calls and incremental Commits over a long lifetime. No scan/copy/schema/import in repeated mount |
| All files immediately ready? | Include `.git/index`, ignored files, dependencies, symlinks, caches and output. Demand faults are real I/O, but no dependency install/restore/reconstruction is part of readiness |
| Why no filtering? | Task-granularity source-only projections cannot substitute for the per-call complete filesystem. Ignore rules are command semantics, not LayerFS membership; task mode uses the same complete-root contract |
| No data-structure size/count/time caps? | Remove inherited metadata/edit/file/Commit/whole-stream ceilings through backed/streamed structures; no automatic Bash duration cap. Explicit windows/resources/platform formats remain, no silent omission |
| Large amounts of files/data without Phase-4.5 limits? | Indexed metadata, generation-selective enumeration, bounded payload/namespace construction and transport backpressure; deferred edits/directory Vec/new-parent map/sparse/import constraints must change, not merely be relabelled |
| Removed server and safe immutable distribution? | No layerfs-server revival and no host runtime. Since 2026-10-07 every daemon opens one shared SQLite Store directly ([06](06-cluster-one-integration.md)); immutable content supports verified reuse, while reference closure, GC, durability and mutable history CAS still require exact protocols. Implemented source still opens the Store only on macOS |

## Smallest supported granularity: one tool call

Owner clarification 2026-10-05: one tool call is the smallest supported
orchestration granularity, not a one-call ownership limit or required teardown
boundary. One Workspace may serve sequential or concurrent calls for a long
lifetime, with repeated explicit incremental Commits. Its lifetime is independent
of any particular call or task. Exec completion and Commit success keep it mounted;
only explicit terminal unmount ends that Workspace.

Both per-tool-call and per-task orchestration are required. Per-tool-call is the
expected common case, so fresh-root readiness and native lifecycle overhead stay
important. Exec duration is independent of orchestration granularity: a command
may finish quickly or remain long-lived in either mode. Do not infer its duration,
auto-Commit or close its Workspace from command type or call completion.

```text
 one-time: acquire COMPLETE initial root + history; initialize runtime + daemon DB
                                      |
                                      v
 call n:   mount Rn -> ordinary exec -> explicit Commit -> terminal unmount
                           |                 |
                   all files available       +--> exact known root/head Rn+1
                   no hidden preparation     +--> UpToDate: same head/root
                                             +--> uncertain/failure: retain/report

 call n+1: mount complete known Rn+1 -> real execution immediately
 persistent W: mount R0 -> calls A,B,... -> Commit C1 -> more calls -> Commit C2
                           same mount, live view and ownership until final unmount
```

The controller sequences these explicit APIs. Exec does not auto-Commit. All
acknowledged changed files participate, including a command's ignored cache/output
and git index refresh; a command exiting nonzero can still have changed state.
No-change UpToDate does not create a new Commit record; invocation audit even
without state change is a separate feature, not assumed history semantics.

Calls sharing a Workspace observe one mutable filesystem. Commit captures the
Workspace-wide published frontier, which can contain changes from several calls;
it does not provide per-call mutation isolation or attribution. Known install
advances that Workspace's base and preserves later active changes, so further
calls use the current live view rather than restarting from the old committed root.

Tool-call wall includes mount, actual command/output drain, Commit and terminal
unmount; physical reclaim/debt is also accounted. Initial acquisition is separate
and must not be repeated per call. Cold/warm cache states stay declared; no hidden
prefetch credits a timed phase. No bootstrap time/throughput number is claimed.

## Claim labels

Important claims carry one of these, in square brackets:

| Label | Meaning |
| --- | --- |
| implemented and source-verified | Read in source on `main` at the cited path and line. "Source-verified" alone is the short form |
| measured diagnostic evidence | A number from a retained report. It keeps that report's status; none is a `PASS` |
| owner requirement | Stated by the owner in the task brief or on an issue |
| proposed design | Decided in this set; not implemented |
| unresolved question | No decision or no evidence; usually an owner question in [08 §7](08-decisions-provenance.md#7-questions-only-the-owner-can-answer) |

## The recommended architecture

[proposed design; see primary engine/integration contracts]

```text
 host: Project Init, seal, install, control only      Linux sandbox daemon (one of several)
                                                      FUSE + Workspace + content
            control: mount / Exec / Commit /          Storage / Reader / Save + HistoryCatalog
            status / unmount  ------------------->         |                    |
                                                    overlay.sqlite        store.sqlite
                                                    one per daemon        one shared volume
                                                    WS-keyed local state  immutable objects;
                                                                          mutable history
```

One shared overlay database is an owner decision; its initial MEMORY/OFF owner
connection is a candidate, not a claim of parallel SQL writes or measured capacity.
A WAL reader pool is unselected and would require its own resource/profile proofs.
Construction, Save and history publication are in the daemon over a directly
opened shared Store (owner direction 2026-10-07, K28–K33); the host is
control-only after install. The retired layerfs-server package is not restored
or renamed as a coordinator, and no host runtime stands in for it. Another
database behind the same ports is future integration, not current capability.

Live state uses active/captured changes over an immutable base. R1–R8 require
bounded payload mutation, fixed captured domains, independent orphan custody,
short failed-capture resolution, fair service and aggregate residency accounting.
These algorithms are not completed by writing a schema or drawing this topology.

## Required corrections before implementation

[owner requirement; proposed design corrections, not implemented]

The review verdict is **architecture must change** for the full repository,
continuous file logging and concurrent Workspaces. The changes below are required
by the existing owner requirements; no new approval of those requirements is
needed. Candidate algorithms are not considered complete merely because they
are listed here.

| ID | Correction | Specification / exit proof |
| --- | --- | --- |
| R1 | Bound WRITE work after spatial fragmentation; logical cutoff for nonzero truncate | [02 §5](02-base-overlay.md#5-payload-replacement-required), [03](03-mutation-hot-path.md); alternating-byte overwrite and shrink/capture proof |
| R2 | Generation-selective, terminating captured cursors | [02 §9](02-base-overlay.md#9-bounded-queries); tiny capture during growing full install, visited-row counts |
| R3 | Remove fragmented construction refusal; back directory/touched/validation/release state; avoid mandatory whole-base alias walks; represent sparse holes | [06 §6](06-cluster-one-integration.md#9-prerequisites); full affected-state Commit with fixed processing windows and tiny-rename visited-work proof |
| R4 | Bounded orphan state; failure resolution without foreground payload merge | [04 §7–§8](04-concurrency-commit.md#7-open-unlinked-files); repeated successful and failed Commits with a retained log descriptor |
| R5 | Deferred busy-request replies, fair Workspace and Store scheduling, demand transport capacity | [01 §5](01-architecture.md#5-inside-the-daemon), [06 §4](06-cluster-one-integration.md#4-the-daemon-adapter); independent-request progress under contention |
| R6 | Reserved pressure headroom, actual page accounting and shared-disk admission | [03 §7](03-mutation-hot-path.md#7-maintenance); no unbounded cleanup charged to a tiny write |
| R7 | **Withdrawn 2026-10-07** with the host-mediated wire: constructed objects no longer cross a trust boundary. Bounded root checks and Store-volume confinement replace it | [06 §4](06-cluster-one-integration.md#4-the-daemon-adapter), [§6](06-cluster-one-integration.md#6-store-visibility), [§7](06-cluster-one-integration.md#7-authority) |
| R8 | Whole-system memory/residency bounds and outcome-aware cancellation | [02 §11–§13](02-base-overlay.md#11-what-grows-with-what), [04 §10](04-concurrency-commit.md#10-second-commit-and-terminal-unmount) |

The historical 334fc7437 design remains available through Git. This revision
withdraws its small-statement guarantee for arbitrary extent overlaps, the
factor-of-two capture scan argument, two-version orphan claim, one-step wait,
strict read priority and foreground failed-Commit folding.

Acceptance status is recorded in [07 §5](07-implementation-validation.md#5-validation).
The source constraints in R3 are mandatory integration work, not acceptable
permanent exceptions to the no-cap and bounded-memory requirements.

## Capture, cleanup and efficiency clarification

Capture keeps existing changed rows as stable input; it does not create a full
snapshot copy. New active rows appear only for later mutations, while operation
scratch stores construction metadata. Known history success plus safe install/
last-owner release makes obsolete local rows reclaimable. Cleanup automatically
runs bounded SQL deletes even between tool calls; no next operation/manual call
or intentional TTL is needed. Terminal unmount retires all remaining owned local
state without DROPping shared tables or shrinking the SQLite file.

See [row lifetimes and cleanup](workspace-api/commit.md#91-automatic-sql-row-deletion-and-retention-gates),
[engine cleanup](daemon-sqlite.md#61-automatic-batched-sql-deletion) and
[efficiency model](workspace-api/commit.md#21-efficiency-model-and-limits-of-the-claim).
Capture/incremental construction are efficient directions, not measured throughput;
full per-call latency and sustained reclaim capacity remain required proofs.
Persistent-Workspace acceptance also covers repeated calls and Commit cycles,
same-mount cache coherence, retained descriptors/orphans and reclamation during
continued activity. There is no automatic Workspace lifetime or call-count cap.

## Public Workspace lifecycle

[owner decision, 2026-10-05]

| Operation | Contract |
| --- | --- |
| `mount` | Open logical Workspace on committed base and expose its mount; reuse daemon database |
| `exec` | Ordinary Bash process, no automatic timeout or implicit capture/Commit/lifecycle |
| `commit` | Explicit captured-state publication; later writes continue |
| `unmount` | Terminal detach, logical close and automatic cleanup; discard uncommitted local state, no implicit Commit |
| `status` | Read-only observation |

A separate public close is not required. Busy/uncertain normal unmount leaves the
Workspace intact; explicit forced teardown preserves outcome reporting. Unmount
success establishes logical cleanup/fencing, while owned physical reclamation may
complete in bounded background jobs. See [04 §10](04-concurrency-commit.md#10-second-commit-and-terminal-unmount).

## Consequential decisions

Each reverses or replaces something a prepared document marked decided. Detail
and evidence: [08 §3](08-decisions-provenance.md#3-decisions-of-this-design).

| Decision | Replaces |
| --- | --- |
| One initialized overlay file per daemon, Workspace-prefixed keys (K1, owner update) | Per-Workspace-file proposal at 334fc7437; separate writer/pager isolation withdrawn |
| Construction, storage and history in the daemon over a directly opened shared Store; control-only host and bridge (K2 revised, K28–K33, owner direction 2026-10-07) | Storage and history on the host, joined by bridge data operations |
| In-memory journal, exclusive locking, one daemon-owned connection, no sync (K3) | WAL with reader connections and explicit checkpoints |
| Bounded payload updates with byte-exact visibility; no base-payload copy-up (K5, revised) | A fixed 4 KiB block grid with copy-up |
| Bounded failed-capture resolution without foreground payload merge (K8, replacement required) | One extra layer per failed attempt |
| Orphan content independent of namespace capture (K9, replacement required) | A single retire floor |
| Save finish, stage and transition as separate calls; the stage is discarded by exact token on conflict (K15) | "One conditional transaction" |
| `Uncertain` defines no resolution until the owner rules (K16) | A lookup rule that was never adopted |
| Mutations wait; nothing returns `EBUSY` for contention (K13) | Refusal-based coherence |

## Old limitations targeted for removal

These are removal goals, not implemented removals. Traced to source in
[02 §10](02-base-overlay.md#10-limitation-inventory).

| Removed | Where it was |
| --- | --- |
| Per-file edit and piece counts (4,096; 8,193) | Root reference engine |
| The emergent edit cap between 8,192 and 10,240 writes, which refused a Commit after it was published | Phase 4.5 reconcile budget |
| Page-file creation, read-back and hashing on every mutation | Phase 4.5 private backing |
| The WAL check and inline checkpoint after every write | #305 prototype |
| One 8 MiB memory budget shared by all Workspaces | Phase 4.5 |
| 128 open handles; 32 captures; index depth 7 | Phase 4.5 |
| The 256 MiB cap on Commit input | Bridge prepared stream |
| The 4 GiB file constant on the Workspace path | Bridge contract; removal already required by the owner |
| `EBUSY` for overlapping callbacks; one base read at a time per daemon | Phase 4.5 |
| One Workspace, one Exec, one control session per daemon | Daemon |

## Limits that remain

Real limits, listed with their kind in
[02 §11](02-base-overlay.md#11-what-grows-with-what).

- **Cluster one format:** names of 255 UTF-8 bytes; file, directory and symlink
  only; mode and mtime only (no ownership, no ctime); hard links on regular
  files only; 16 MiB per canonical object; inode serials up to `i64::MAX`.
- **Required integration corrections still present in source:** a heavily fragmented edit of one file
  can exceed `EDIT_DEFERRED_LIMIT` at Commit; one directory's changed names
  must fit in memory at Commit; a hole is committed as zeros.
- **Resources:** daemon database/physical-disk budgets, logical Workspace admission budgets, configured counts
  (Workspaces, Execs, upstream connections), cache budgets, the commands'
  descriptor limit.
- **Engine and platform:** SQLite's database size, 64-bit offsets, 128 KiB per
  FUSE request.

## Proposed performance targets

Proposals for the owner to freeze prospectively; none is a result. Full list
and qualification shape:
[03 §8](03-mutation-hot-path.md#8-proposed-targets) and [engine](daemon-sqlite.md).

- **T1** A mutating request is acknowledged after exactly one overlay
  transaction and zero checkpoints, sync calls, file creates, bridge calls,
  kernel notifications or maintenance steps.
- **T2** Freeze operation/page ceilings from the replacement algorithms. Old
  sequential-write counts were illustrative and do not cover fragmented writes.
- **T3** Statements and pages per write are flat against the write index up to
  100,000 writes to one file.
- **T4** Zero `EBUSY` with four Execs writing and with a writer running through
  a whole Commit.
- **T5** Bound individual service units and prove starvation-free admission.
  Report queue wait, service time, same-inode waits and pressure delays separately;
  the old one-step end-to-end wait claim is withdrawn.
- **T6** Exec with the overlay within a frozen factor (proposed 1.25) of a
  passthrough under the same mount profile, in matched arms.
- **T7** Fixed cost of a tool call against the 100 ms reporting line.

## What the evidence does and does not support

[measured diagnostic evidence]

- No retained cell of #305 or #306 exercised product code or this design.
- The owner promoted the cached mount profile as a candidate; it is not
  implemented or qualified.
- Stage B of #305 failed its required verifier and Stage C was not run.
- #306 compares different architectures under different caps and is not a
  ranking.
- Every number is one observation with no residency proof: `INELIGIBLE`.
- The evidence supports removing requests. It attributes no time to SQLite,
  base acquisition or construction.

## Questions for the owner

Product/policy choices are in [08 §7](08-decisions-provenance.md#7-questions-only-the-owner-can-answer):
One daemon database and unlimited Bash runtime are now owner decisions. Remaining
choices concern crash survival, the shared Store's Disposable profile (O-21), host access
after install (O-18), command identity isolation, SDK view and
ctime policy, conflict behavior, and exact uncertain-history resolution.
Terminal unmount includes logical close/cleanup and discards uncommitted local
changes; O-14 no longer requires a separate close decision.
The permanent benchmark hosting rule and permitted residency mechanisms need
policy alignment before qualification. Implementation choices such as indexes,
normalization, bundled Linux linkage and prerequisite ownership are engineering
work, not additional approval gates.

## Not done

- No implementation was started.
- No benchmark was run and none is admissible until O-1 is answered.
- Nothing was compiled: the Linux build of `layerfs-content` and of a bundled
  SQLite is unverified.
- The untracked #303 plan and #304 study in the cluster two worktree were read
  and not modified.
