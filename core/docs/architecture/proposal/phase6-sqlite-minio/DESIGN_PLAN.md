# Phase 6 design decisions and delivery plan

> **Owner storage-placement amendment, 2026-10-02:** The strict
> [SP1 specification](implementation/sp1/IMPLEMENTATION.md) supersedes earlier
> physical placement descriptions here. Global SQLite holds all committed
> filesystem metadata and pooled values/groups. MinIO holds regular-file
> whole-payload and CDC chunk packs only. Canonical identities and historical
> evidence remain unchanged; the strict replacement is prospective/unverified.

> **Status: Current planning checklist; no release candidate exists.**
> Source, scope and evidence: [README](README.md).

## Current next action

Implement [S1-SPEC](implementation/S1-SPEC.md): protected trusted daemon publisher,
two-table locator catalog and C5conditional publication with zero service-side
candidate pack reads. Remove displaced certification/index/install machinery.
Prove original128and270initialCommit plus shallow successors. Extreme-depth
successorExecoptimization deferred. Broader streaming/import/families/physical
completion remains open; historical checkpoints below retain their original scope.

## Design gates

All items below are open. Creating this folder does not freeze their contracts.

- [ ] **Source and retirement inventory:** classify accepted Phase 4.5/Phase B
  behavior and unmerged Phase 5 code separately. Identify every data structure,
  persistence path, protocol and lease to retain, replace or retire. Define safe
  owner drain; preserve evidence and unrelated artifacts.
- [ ] **Authority, schema and trust:** assign daemon/global ownership, choose
  namespace/root certification and exact locator/dedup rules, design indexes
  and keyset cursors, and document canonical/v1 compatibility. Decide database
  lifetime and process ownership; avoid per-Commit engine setup where source
  and engine accounting support reuse.
- [ ] **Interfaces and profiles:** freeze bounded input/result grammar, exact
  EOF/seals, resource admission/refund/cancel rules and simultaneous allocation
  arithmetic. Audit real capability/opcode allocations before assigning IDs.
  Define engine/cache/temp-disk/handle/pin limits from the supported provider.
- [ ] **Commit and runtime composition:** specify capture through READY,
  conditional publication, current-successor installation and retirement.
  Resolve Known/Unknown recovery custody, competing upload/Branch outcomes,
  actors/incarnations and independent Exec/mount/Workspace ownership.
- [ ] **Persistence and deployment:** choose local daemon and global service
  persistence separately, with authentication/fencing and process-failure scope.
  Distinguish local WAL/fsync behavior from remote/cloud replication and
  durability. Keep unsupported providers explicit.
- [ ] **Cutover/import:** define source/metadata/pack identity validation,
  cross-process quiescence, v1 compatibility and Unknown migration custody.
  Describe old-owner drain and safe retirement of displaced mutable authority.
- [ ] **Smallest complete implementation:** select one supported ordinary
  mutation through capture, real-provider upload, publication, installation and
  cleanup. Assign non-overlapping files and one coordinated contract owner;
  define dependencies and exit proofs before starting it.

## Standalone metadata experiment checkpoint

[#294](https://github.com/Ephemeral-AI-Lab/layerfs/issues/294) owns the first
[prospective metadata experiment](experiments/METADATA-V1.md) and its
[v1 results](experiments/RESULTS-METADATA-V1.md). All 15 correctness cases passed;
the fragmented overwrite/truncate representation failed the transaction-row
bound, and engine/physical memory qualification remains incomplete. Numerical
observations retain uncontrolled-cache INELIGIBLE status. This checkpoint does
not complete the open design gates above.

The later [six-case metadata adaptation](experiments/COHORT-V2.md) and
[v2 results](experiments/RESULTS-COHORT-V2.md) cover the owner-selected R1 workload
shapes with distinct component timing boundaries. All six semantic/owner checks
passed; one-edit preparation still traverses the whole current extent map.
Full Exec/Commit and physical resource qualification remain unrun.

## Acceptance and performance planning

- [ ] Map current acceptance in #248/#256/#249/#219/#259 and #276's deferred
  ledger to the proposed architecture; retain unresolved and platform gates.
- [ ] Select independent canonical/semantic expected results for retained and
  changed formats. Candidate output cannot supply its own expected roots.
- [ ] Define real-provider bytes/metadata/root, failure/Unknown, concurrency,
  memory/cache/temp-disk, cleanup and accumulated-work proofs. Establish overlap
  through observable provider/kernel/process coordination, not a sleep alone.
- [ ] Declare count diagnostics for source I/O, codec, SQL/query plans, pack I/O,
  path depth and changed-entry scaling. Include engine startup and transactions
  inside any measured operation that needs them; report setup separately.
- [ ] Prospectively define separate matched speed cases, cache contracts,
  complete command limits and evidence identities. Preserve existing failures
  and receipts; do not repeat an unchanged arm for a nicer number.

Commit/capture retain one construction producer. Namespace initialization may
use its existing bounded parallel path; changing that profile needs an explicit
decision. No new worker/quota/timeout/codec/page-size allowance is granted by this
proposal. Physical resource proof is separate from throughput.

## Scope and checkpoints

Each future design or implementation checkpoint records its actual parent/source,
owned changes, decisions still open, checks and gaps, exact production LOC and
published commit in #293. Repository/Core source, dependency, documentation and
measurement rules remain applicable. Product changes require owning Core checks;
docs-only changes require status/link/source review and diff checks.

**Deferred:** mounted daemon/sandbox qualification, cloud replication/failover,
broader file/body/count profiles, writable mmap/complete database locking and
durability beyond the chosen contract. Reconcile the performance handoff with
#288 explicitly before a new campaign; no campaign is part of creating this
folder. No issue closure, remote-main merge, candidate deletion or product revert
is included in this delivery.
