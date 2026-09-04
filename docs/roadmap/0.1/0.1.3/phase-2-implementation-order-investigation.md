# Phase 2 implementation order: shared causes across #39 and #38

Follow-up: the [mechanism-adoption audit](phase-2.1-mechanism-adoption-audit.md)
refines this ordering. Check the actual #40 transfers first: Workspace already
has enlarged checked batches/cache/sorted updates, but not direct owned-slab
delivery; native initialization still uses its older initial-tree builders.
Do not interpret this earlier report as requiring new POSIX work before checking
those existing-mechanism adoption gaps.

Investigation date: 2026-09-05. Read-only code/evidence investigation by the primary
agent and three subagents (live mutations; reads/Git; Workspace/history). No
product changes, builds, new benchmarks or new correctness proofs were performed.
This is a proposed implementation order, not a performance qualification.

## Conclusion

Prioritize the shared live Workspace and Commit causes exposed by #39, and use
#41/#44 as affected-path qualification targets. Do not wait until all fifteen #39
cases pass before checking those targets. Some #38 implementation work may become
unnecessary if the shared changes satisfy its existing criteria.

Do not assume #39 closes all four #38 families. #42 cross-file CAS and #43 CDC
time native initialization from prepared input. They bypass the live POSIX/FUSE
mutation path. Only changes to genuinely shared canonical construction/admission
primitives can benefit both routes directly. They retain a separate native-input
diagnosis and qualification lane.

## Source custody and evidence limits

- Product/harness inspected: `810bb3a589ac58d103483df34bb58ecfe0f0ddf4`, checkout
  `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-p21-integration`. This contains #40's
  product source `95578a5e24ac15f38a07535dfdf1fcc9fee80065` plus subsequent harness
  changes. No #39 family was newly measured at this revision by this investigation.
- Historical checkout: `4c9b14a6b489eb6de08d4bfd0d4a723745013ab4`, with existing
  uncommitted documentation preserved.
- POSIX POC: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-posix-exec-poc`, committed
  `6a9bf74075e69d8cb1b3fb622f8e420cf7bab7fb`, plus uncommitted proxy-client edits
  and investigation notes. It is a separate experimental source, not the #40
  implementation. Its projected counters are not family performance results.
- Historical #39 inventory is from clean source `6c54f8d7`, except the incomplete
  directory100 attempt at `30d13dee`. These are not current-candidate baselines.
- The active benchmark audit reported native-verifier handoff/deadline defects
  and runtime mount/placement concerns. Confirm applicable repairs and source
  identities before new qualification. Audit status can change after this report.
- #38/#39 still contain a literal <=2.2-second #40 prerequisite, whereas #40
  reported Approach B completion with that target missed. Reconcile that recorded
  dependency before execution; this report does not silently change acceptance.
- The Phase 1 exhaustive verification withdrawal remains in force. Use bounded
  checks for changed routes; do not reinstate historical exhaustive instructions
  found in older backlog sections.

## Why family names do not define independent engines

Current source references below are relative to the inspected integration checkout.

| Route | Cases | Concrete entry point |
| --- | --- | --- |
| Native prepared-directory import | #42 and #43 | `benchmark/fs-bench-pro/workspace_registry.rs:84`; `src/workspace_bench.rs:918` times `initialize_layerstack` and exits performance before Workspace creation |
| Live FUSE workload followed by Commit | #41, #44, bulk create/delete, dense rewrite, subtree mutation, reads/Git, unrelated history | `src/workspace_bench.rs:1139` executes the workload; `:1186` begins Commit |
| SDK range edit followed by Commit | Distributed history | `src/workspace_bench.rs:1104`; bypasses live FUSE editing but shares Commit/continuation |

Payload creation writes one file of up to 500 MiB and performs metadata/fsync before
close (`ordinary_workloads.rs:610`, `:1692`). Workspace unique creates 1 MiB files
and performs metadata/fsync for every file (`dedup_workloads.rs:370`). Therefore
even these live routes do not necessarily benefit from a small-file optimization
that batches only until close and must drain at fsync.

## Retained #39 statistics and what they establish

Inventory values below are historical three-seed medians unless stated otherwise.
Phase examples are particular retained samples, not a decomposition of those
medians. Do not combine rows from different sources into a matched comparison.

| Family / exact cases | Historical product time | Relevant evidence |
| --- | --- | --- |
| Tiny churn: `tiny-bulk-create-100`, `tiny-bulk-create-500`, `tiny-bulk-delete-500` | 30.09 / 175.04 / 47.17 s | Create500 seed1: 88.67 s Exec + 85.77 s Commit. Both sides matter. |
| Workspace locality: `workspace-dense-rewrite-100`, `workspace-dense-rewrite-500` | 54.53 / 305.17 s | Dense500 seed2: 199.688 s Exec + 112.231 s Commit; its total is 312.183 s, not the 305.17 s median. |
| History: `dedup-history-unrelated-100`, `dedup-history-unrelated-500` | 72.86 / 390.17 s | Unrelated500 seed3: 317.111 s Exec + 73.043 s Commit. Live rewrite dominates. |
| History: `dedup-history-distributed-500` | 17.34 s | Seed3: 1.106 s SDK edits + 16.214 s Commit. Useful discriminator for repeated Commit overhead. |
| Subtree mutation: `namespace-subtree-relocate-delete-500` | 53.29 s, one complete seed | Seed1: 28.50 s Exec + 24.77 s Commit. Later stopped seed2 is not a correctness failure or completed sample. |
| Directory traversal: `directory-content-scan-500` | 65.64 s | Seed2: 65.47 s Exec, approximately 0.002 s clean Commit. Commit optimization cannot resolve this cost. |
| Directory traversal: `directory-content-scan-100` | Incomplete timeout at 15.000758416 s | Source `30d13dee`, seed1. Not a throughput or speedup denominator. |
| Git: `git-tool-1`, `git-tool-10`, `git-tool-100`, `git-tool-500` | 23.19 / 23.97 / 23.80 / 26.53 s | Git10 seed2: 21.171 s Exec + 2.74 s Commit; first status 9.312 s and unstaged diff 11.398 s dominate Exec. |

Sources: [Phase 2 backlog](phase-2-backlog.md), [issue #39](https://github.com/Ephemeral-AI-Lab/layerfs/issues/39),
and its named raw attempt anchors. Git timing is in
`attempts/git-tool-10-s2-performance-6daec80a92bb/raw.jsonl` under the retained
Phase 1 results. The status/diff attribution is measured for that historical
sample; whether current cost lies in Git computation, reads or metadata requires
current per-command and transport counters.

## Current code findings

### Live creation and metadata

`layerfs-fuse/src/proxy_client.rs:257` takes a pending create out of the pending
state on its first nonzero buffered write. Existing closed-create batching
(`:773`) consequently does not batch that ordinary nonempty-file route.
The host closed-create batch in `layerfs-workspace/src/projection.rs:604` still
performs scalar create/pin/write/mtime/unpin for each file. `chmod` and `set_mtime`
at proxy-client lines 955 and 982 flush writes and perform synchronous per-node
requests unless the create remains pending.

The POC tests bounded retention through close with the existing protocol. Its
reported focused proof reduces 1,000 5 KiB creates from about 3,001 frames to
eight batches plus one fence. Reservation and metadata traffic are excluded.
Append-only segment spooling is a proposed next integration, not a qualified
production engine or end-to-end speedup. Preserve per-operation visibility,
error propagation, fsync fences and open-unlinked lifetimes.

### Commit and continuation

#40's sorted tree updates, exact metadata cache, staging and publication exist.
However, `layerfs-workspace/src/changes.rs:402` still iterates the selected
namespace frontier and `:779` iterates content-only updates. Continuing a
Workspace still loops over loaded nodes in `lifecycle.rs:211`, resolves paths
and aliases against the committed root and authenticates attributes.

This identifies remaining work, not proof that it dominates the current sample.
Use existing candidate/content/admission/rebase counters. A distributed-history
control distinguishes repeated Commit cost from POSIX rewrite cost without
discarding required intermediate publications. Preserve exact returned snapshots,
identity/alias checks and sparse-change locality.

### Deletion and subtree mutation

`projection.rs:692` processes an unlink batch using scalar Workspace unlink calls.
`cow_tree.rs:865` checks rmdir emptiness by constructing directory entries while
an existing `directory_is_empty` helper is present at `:631`. This is a concrete
small reuse opportunity whose semantic equivalence and real cost must be checked.
The workload itself deliberately performs traversal, unlink and rmdir; replacing
it with root reset or offline survivor import would change the operation.

Dense final-namespace construction can share #40's tree primitives, but no current
survivor-based performance result establishes recovery. Preserve surviving aliases,
old immutable history roots, open handles and the efficient sparse path.

### Reads and Git

The scan enumerates and opens files and reads them through the live projection.
Existing metadata/read-ahead caches and acknowledged read-only pins must be
accounted for before proposing another cache. Git has a fixed populated background
and six commands at every tier. Its retained first status/diff costs justify
per-command read/metadata diagnosis before Git-specific tuning. Neither pure
read scans nor those command costs are resolved by a faster Commit alone.

Scan500 seed2 recorded 100,000 opens, 200,000 preads and 100,000 closes for
524,288,000 bytes. The current read-only pin acknowledgement is at
`layerfs-fuse/src/proxy_client.rs:760`, and read-cache-miss exchange at `:821`.
Existing read/transport telemetry can separate host storage work from socket
waiting; RPC count alone does not prove which dominates current latency.
Git's fixed background is 32 MiB / 6,400 tracked files. The prior uncached-parent
unlink visibility defect was repaired; preserve its barrier and regression rather
than treating it as a newly reproduced failure or removing it to reduce requests.

### Native import remains an independent concern

In `layerfs-layerstack-store/src/layerstack.rs:975`, flat-directory files are
partitioned in groups of `INITIALIZATION_SLAB_OBJECTS` (512); worker count at
`:1041` is bounded by task count. CAS tiers with at most 500 files therefore form
one construction task when this direct route is selected. CDC fixtures contain
a root reference file and one variants directory; the generic root task split
at `:995` leaves the heavy variants directory in one task. `:1100` processes that
directory as a unit.

These are static scheduling observations, not proof of a current scaling root
cause or a recommendation simply to add workers. Confirm route, task count,
producer CPU, admission waiting and total CPU before a bounded scheduling change.
FUSE batching cannot alter this native task partitioning.

## Recommended implementation order

| Stage | Work and smallest useful discriminator | Families benefiting / qualification checkpoint |
| --- | --- | --- |
| 0 | Finish applicable harness/placement fixes, pin current source and reconcile the prerequisite. Reuse valid evidence. | All families; required before admitting new measurements. |
| 1 | One small create/dense-write selection plus a distributed-history Commit control. Separate live metadata/write/spool costs from content/admission/rebase. Implement the measured shared Workspace cause first, one change at a time. | Tiny creation, dense rewrites, both history routes; strongest shared opportunity for #41/#44. |
| 2 | Address remaining live metadata and eligible nonempty create batching/host ingestion. Use POC evidence, but prove real fsync/visibility/error behavior before promotion. Do not treat dense overwrite as newly created-file batching. | Tiny create and live rewrite/metadata paths. Recheck #41/#44 immediately; retain their distinct file sizes and fences. |
| 3 | Recover bulk delete and subtree relocate/delete together through shared binding/traversal/emptiness and final-namespace work. Start with a small delete/subtree control. | Tiny delete + subtree mutation. Limited direct #38 benefit. |
| 4 | Diagnose/fix content-scan read/metadata costs, then reuse the fix in Git status/diff. Investigate only Git-specific residuals afterward. | Directory scan + all four Git tiers. Commit-only changes are insufficient. |
| 5 | Reassess remaining distributed/unrelated history costs after shared live and Commit fixes; optimize only history-specific residuals. Preserve original cycles and intermediate Commits. | Complete recovery of history coverage; avoid running the full slow unrelated500 during each iteration. |
| Separate native lane | Check #42/#43 on the stable shared-primitives candidate; diagnose their native task/data shape if scaling still fails. This need not wait for delete, reads, Git or all fifteen #39 cases. | CAS/CDC scaling, canonical and admission regressions. |

Stage 1 identifies which shared cost merits implementation; it does not prescribe
an unmeasured rebase or worker-pool rewrite. Stage 5 is residual/final history work,
not postponement of the early distributed-history diagnostic. Move a small proven
fix earlier when it has no dependency; this is an ordering by shared causes rather
than a mandate to complete entire families serially.

## #38 disposition and matrix correction

- Keep #41 and #44 open as qualification targets while implementing shared #39
  fixes. If their complete curves pass, no family-specific patch is required.
- Keep #42 and #43 as native-import work/qualification. Shared Store/content
  improvements may transfer, but live POSIX changes do not establish their pass.
- The CAS registry uses one shared `dedup-cross-file-anchor-1`, then identical,
  mixed and unique at tiers 10/100/500 (`families/dedup_cross_file.rs:5`). There
  are ten unique CAS cases, not twelve: **30 samples**, not 36, for three seeds.
  The four #38 families therefore require **114 unique candidate performance
  samples**, before invalidated regression controls, rather than the previously
  stated 120. The common anchor is reused when evaluating the three curves.
- Preserve #38's normalized median ratio <=1.0, smaller-case absolute performance,
  original seeds and operations. Preserve #39's full definitions, per-sample
  15-second recovery target and scaling criteria. Neither issue closes from code
  reuse, missing cases or suppressed execution.

No new POSIX semantic failure was reproduced in this read-only investigation.
Existing repaired visibility/failure checks remain constraints. If a current
correctness failure appears, fix its shared cause before performance work on that
route and retain the failed evidence under its actual source identity.
