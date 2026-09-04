# Phase 1 infrastructure reuse and qualification

Status: implementation guidance for [#22](https://github.com/Ephemeral-AI-Lab/layerfs/issues/22).
This document records inspected reuse points and proposed interfaces. It does
not claim that new commands, adapters, observations or qualification results
already exist. The [testing rules](testing-rules.md) and family specifications
remain authoritative. The coordinator owns numerical resource caps and phase
deadlines in the execution contract.

## Existing implementation to retain

| Concern | Existing file and entry points | Minimal extension |
| --- | --- | --- |
| Host binary | `benchmark/fs-bench-pro/src/main.rs`, `run` | Dispatch new families in the same binary; put shared host lifecycle handling in one module. |
| Family definitions | `benchmark/fs-bench-pro/families/`, included by `workload.rs` | One canonical definition module per family. |
| Deterministic bytes | `families/sdk_edit_common.rs`: `SIZES`, `SIZE_LABELS`, `fixture_block`; `workload.rs`: `Sha256`, `hex` | Bounded Workspace shard generation and nested schedule helpers. Existing flat-payload hashes keep their original meanings. |
| Ordinary filesystem workloads | `benchmark/fs-bench-pro/workload.rs`, `run` | New explicit commands in the existing workload executable. Actual tools retain their ordinary writes. |
| Public lifecycle | `main.rs`: `case_client`, `case_placement`, `lifecycle`, `execute`, `execute_workload`, `visible_head` | Preserve one Store, public initialization, one Branch, real FUSE, public Exec, Commit and End. Extend receipts for trajectories. |
| Prepared inputs | `sdk-edit-custody.py`: `acquire_prepared`, `prepared_key`, `clone_prepared` | Add a compatible fixture-profile branch; reuse locking, atomic publication, quarantine and independent clones. |
| Runner | `lib-edit-sdk-runner.sh`; `run-namespace.sh` | Extract only reusable preparation/runtime/supervision pieces needed by a second family; retain existing SDK-specific route checks. |
| Verification | Namespace manifest checks, `src/sdk_edit_verify.rs`, Store census helpers | One shared complete-tree verifier; family modules supply independent expectations and intermediate checkpoints. |
| Evidence | `sdk-edit-custody.py`: `seal`, `verify_manifest`; current report generators | New family schemas and raw-data adapters within the existing custody/report path. |

`execute` already checks fresh-process completion, exit status, output truncation,
daemon transport, balanced execution timing and zero Docker-engine calls inside
Exec. `WorkspacePlacement::Container` with `WorkspaceProjection::Fuse` supplies
the authentic mounted route. The host owns the Store, SDK and proxy; the managed
container owns the daemon, FUSE helper and workload process.

`sdk_file_edit.rs` and the SDK runner's sealed call-graph validator intentionally
require a singular public range edit and exclude Exec. They are not generic
POSIX adapters. Same-file batch edits use `Client::edit_workspace_file_ranges`;
they must not be presented as a multi-file API.

## Shared interface and ownership

The following is the proposed implementation interface, not a second framework.
The coordinator may settle concrete field names at implementation freeze while
preserving these responsibilities and the canonical family contracts.

```rust
pub(crate) struct Case {
    pub id: String,
    pub family: &'static str,
    pub tier: usize,
    pub kind: &'static str,
}

// Each canonical family module:
pub(crate) fn cases() -> Vec<Case>;
pub(crate) fn fixture(case: &Case, seed: u8) -> Result<Vec<Entry>>;
pub(crate) fn expected(case: &Case, seed: u8, step: usize) -> Result<Vec<Entry>>;
pub(crate) fn apply(case: &Case, seed: u8, step: usize, verify: bool) -> Result<Receipt>;
```

`Entry` contains relative path, kind (`File(Content)`, `Directory`,
`Symlink(target)` or `Hardlink(target)`), mode and controlled timestamp in
seconds/nanoseconds. `Content` is a deterministic streaming descriptor with
seed and length, plus bounded splice, slice, XOR or literal recipes; entries
never materialize full expected file bytes. Root and empty directories are
entries. The shared verifier streams actual regular-file bytes and compares
the complete path set, including unchanged paths. It rejects missing, extra or
duplicate paths and compares required intermediate states. Expected content is
independently constructed from the frozen fixture/schedule, never by executing
`apply` on a reference directory or accepting the candidate's output root.
Symlink metadata uses native no-follow operations. Unknown entry types or
unavailable required metadata fail closed. Hard-link targets define expected
equivalence classes, which the verifier compares with observed device/inode
classes rather than literal inode values.

`Receipt` is a `BTreeMap<String, String>` emitted as `key=value` lines. It contains
inner workload duration and cheap attempted/completed
operation, byte and path counts. It does not contain a full-tree verifier or
Store census. Its fields must declare metric applicability rather than filling
unsupported observations with zero. Tool-specific semantic receipts may be
added by the owning family.

One shared fixture/verifier owner controls `workspace_common.rs` (or the final
shared module name), content recipes, fixture metadata/manifests and the
canonical actual-tree verifier. The coordinator owns `workload.rs` integration,
host dispatch, runner and custody changes. Family workers own only their
explicitly assigned `families/<family>.rs` modules, ordinary/dedup workload
helpers and thin wrappers. They provide fixture,
schedule, workload and independent expected-state logic through the shared
interface. They do not each create a cache, verifier, runtime manager or report
pipeline. `apply` runs with the Workspace as its current directory; `verify`
enables only explicitly declared proof behavior in the separate verifier lane.
Family SDK edit descriptors contain path, start, deletion length and bounded
replacement bytes; the host adapter performs the actual public calls.
Host-only SDK operations live in the host adapter; the standalone
workload remains compilable with the existing direct `rustc` image build.

Selected-case resolution precedes fixture acquisition. The shared shard helper
receives the requested prefix, emits the prescribed 200-file shards and fixed
metadata, and uses the existing byte generator with at most 1 MiB scratch.
Families with a fixed background explicitly request that background. No family
name enters a compatible fixture cache key unless it changes the fixture's
actual bytes, metadata, generator or oracle semantics.

## Observation availability and required seams

Existing receipts provide:

- `CandidateReceipt`: candidate, inserted and reused object/byte totals,
  preexisting reuse, batch/final admission and transaction maxima.
- `WorkspaceCommitReceipt`: lifecycle phases, capture counts, payload bytes,
  snapshot database calls/rows/bytes and admission/publication timing.
- `WorkspaceCommitDiagnostics`: CDC scanned bytes, edit and piece counts,
  piece height/logical charge, spool allocated/peak/live/superseded bytes,
  tree visits and metric scans. The existing binary enables diagnostic capture.
- `FuseWriteReceipt` and `WorkspaceReadReceipt`: actual kernel request/byte
  counts, transport copies, read-ahead and spool-write observations.
- Host process resource snapshots, SQLite ownership observations, daemon cgroup
  sampling and execution/lifecycle receipts through the existing native tools.

`Client::monitor_snapshot` reads retained operations, session summaries and
Store file size. Expensive `analyze_dedup` and object census are distinct
verification work. The monitor retains **512 operations**; long trajectories
must collect incremental receipts before rollover, identify already-collected
operation IDs and retain all required observations. A final snapshot cannot
prove a 500-Commit trajectory's complete operation history.

Existing read validators often assume exactly one receipt and positive
read-ahead activity. They must not be applied unchanged to metadata-only or
multi-Exec cases. Aggregate actual applicable receipts with checked arithmetic.

The inspected receipts do not expose every required readdir-page,
dirty/clean-namespace visitation or failure-boundary observation. Each family
must map its required fields to an existing receipt or an explicit passive
addition before collection. Missing mandatory observations are incomplete
infrastructure. Runtime call counts, errno and helper acknowledgements must
remain distinguishable from source-level route assertions.

Any added observation or verifier-only fault seam must name its source path,
enabled scope, provenance and qualification. It must preserve the measured
algorithm, storage format and normal behavior. Fault injection runs only in
verification. It must prove the intended boundary was reached, retain the
failure and check post-failure state; an arbitrary early error is not an
equivalent boundary proof. Product fixes and performance/storage optimizations
remain Phase 2 work.

## Custody and runtime preparation

The existing prepared-store cache already uses per-key `flock`, staging and
atomic publication, immutable entries, manifest validation, corrupt-entry
quarantine and ignored abandoned staging. `clone_prepared` uses APFS clone when
available, otherwise byte copy, rejects same-inode clones and verifies SHA-256.
Directory fixtures need equivalent independent copy handling. Never hard-link a
mutable sample to a master.

The SDK cache key is specifically a flat-payload Store identity. New Workspace
fixtures need explicit profile, metadata, generator, oracle and initialization
compatibility; they cannot masquerade as a released SDK fixture of the same
size. Reuse pristine Stores only when initialization is outside timing. Measured
creation/import/history still executes all required work.

`cache_self_check` already exercises concurrent publication, hit/miss,
sample-master isolation, pristine next samples, abandoned staging, metadata and
source invalidation, corruption quarantine, missing rebuild and retained failed
preparation logs. Reuse and extend this check for the new profile rather than
building an independent test service.

The runtime is available through Docker, and retained binaries/images/build
evidence exist. Availability is not compatibility. Bind exact source, binary,
image, workload, tool versions and environment before admitting a run. The
Dockerfile currently copies family sources individually and compiles the
workload directly; include new modules in those build inputs. Verify required
Git/search/archive tools before freezing tool-family inputs.

The existing SDK runner requests `/dev/fuse`, `SYS_ADMIN`, unconfined AppArmor
and a capability-authenticated daemon endpoint published only on host loopback.
Its invocation does not impose CPU/memory/PID/disk limits. Record and enforce
the execution contract's limits explicitly. Its old SDK preparation/performance/
verification watchdogs are not implicitly the limits for new history/endurance
cases. Do not widen a frozen limit after observing a failure.

## Preserving the existing worktree

The inspection started from a dirty tree containing the new roadmap work,
modified release documentation, removed obsolete specifications and unrelated
untracked writing/diagram assets. Record that initial status and preserve it.
Only the coordinator stages the task-owned contract/implementation paths; use
explicit path lists, no blanket staging, resets or broad cleanup. Workers edit
only assigned files and communicate shared-file changes to their owner.

The existing custody `require_clean` is deliberately strict and its identity
helper binds the v0.1.2 SDK contract. Extend contract identity for v0.1.3 without
changing old evidence. A task-scoped committed tree or isolated checkout may be
used for sealed builds while unrelated work remains untouched. Record the exact
product baseline independently of harness revisions and passive instrumentation.

Do not remove unrelated running containers or retained evidence. Every attempt
gets a unique output directory; failed preparation, timeout and product failures
are sealed before cleanup. Reports regenerate only derived files from raw
evidence. A changed relevant identity invalidates only affected evidence.

## Issue #22 validation checklist

- [ ] Commit and publish the canonical specifications, shared rules and exact
  product baseline; replace issue planning links with exact committed links.
- [ ] Register exactly 130 new timed IDs and three prescribed seed slots each;
  account separately for 13 proof recipes, reliability subcases and inheritance.
- [ ] Product-free self-check validates IDs, nested prefixes, fixture identities,
  all intermediate byte bounds, authentic routes and metric applicability.
- [ ] One small selected case resolves and prepares only its dependencies, runs
  through public SDK/Exec/FUSE and retains cheap completion/route observations.
- [ ] New cache-profile qualification retains hit/miss, concurrent publication,
  interruption, corruption/invalidation and sample-to-master isolation evidence.
- [ ] First-use preparation and repeated compatible warm command wall are
  recorded separately, including acquisition, validation, copy and cleanup.
- [ ] Resource caps and independent preparation, selected, performance,
  verification and extended deadlines are frozen before collection.
- [ ] Mandatory passive counters and incremental history receipts are available;
  missing values fail validation instead of becoming fabricated zeros.
- [ ] Performance invokes no added digest/tree oracle/census/reopen/injection;
  the verifier has independent expected semantics and complete path coverage.
- [ ] Five capped inherited growth replacements have new definitions, hashes
  and identities; original SDK semantics/evidence remain intact.
- [ ] Source/build/image/custody/cleanup validators reject stale, incomplete,
  failed or wrongly routed evidence; collection completion stays separate from
  product correctness and release admission.
- [ ] Only the smallest justified route/verifier qualification runs during
  development; performance collection precedes complete verification, and the
  evidence ledger prevents rerunning unaffected passes.

Unchecked entries are work remaining, not failures or implied passes. Completed
entries must link retained evidence and the exact producing source identity.
