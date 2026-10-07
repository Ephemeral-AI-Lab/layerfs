# Reviewed R2–R5 ownership: FUSE request service and daemon composition

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Reviewed2026-10-08 at `5be93f6d7f9352eab3cbfa286fedbc861e6e3494`.
> Product remains the verified R1 source: core/crates tree
> `e3a61dfd814a579b58f56b63754ec3dbce83d67e`. This is review/documentation only.
> No R2 implementation, new runtime proof or agent dispatch accompanies it.

The proposed adjustment is accepted with the clarifications below. FUSE owns the
complete kernel connection/request lifecycle; daemon composes it with existing
engine services and owns overall Workspace control and Commit. This supersedes
old planned daemon `native/` and `request/steps/` homes. It changes source ownership,
not readiness, correctness, fairness, resource or failure requirements. The owning
[S8 specification](S8-SPECIFICATION-20261008.md) is reconciled in the same change.

This split keeps each kernel reply, its parked continuation and its native
connection termination under one library owner. It makes FUSE independently
composable without importing application/control state. The cost is a larger
Fuse crate and a real asynchronous engine-service boundary; it does not eliminate
scheduler, wakeup or readiness work. Keeping SQL and Commit in their existing
owners prevents a native-only engine that other producers would bypass.

## 1. What is implemented now

The [exact inventory](checks/r2-r5-ownership-review-20261008/01-current-source-inventory.json)
records12 active crates, including Sandbox and the daemon executable/application.
FUSE remains excluded incompatible predecessor source; activating it is R2 work.
The future replacement makes13 active crates, not the stale11-plus-two plan.
Source totals are170673 combined:105256 core,62382 active,65417 root reference,
37431 excluded predecessors and5443 excluded integration (old FUSE1447/Server3996).
No code or count changes are implied by the proposed move in responsibility.

Relevant source facts:

| Current source | Implemented fact and consequence |
| --- | --- |
| Daemon `overlay/read_port.rs` | `read_job` submits once then calls `Pending::wait`; the current OverlayRead/OverlayJobs implementations block their caller |
| Daemon `service/completion.rs` | `Pending::try_complete` can take the original outcome; the present notifier is a thread waiter used by `wait`, not a complete FUSE continuation/wakeup API |
| Daemon `overlay/{owner,queue,credits}.rs` | One shared fair SQL owner already serves reads, mutations, lifecycle, capture, records and maintenance. Preserve it |
| Daemon `store/{open,ports,operation}.rs` | Shared fixed read set/cache and fresh operation failure scopes exist; idle-reader admission and lock-free pending demand composition remain prerequisites |
| Workspace `mutation/driver.rs` and source-read ports | Existing semantic fact/operation rounds are synchronous; splitting execution into resumable steps must reuse their deciding logic |
| Overlay `lifetime/{lookup,file_owners,captured_reader}.rs` | Independent existing owners are reusable; native mount/serial aggregate count transactions and exact FORGET mapping remain unimplemented |
| Daemon `store/commit.rs` | One real Capture/Save/finish/History/install driver exists; live captured namespace construction is missing |

R1 proof remains unchanged. ControlReady/logical Bound is not mounted Ready, and
this review supplies neither. Full R1 evidence, deferred admin snapshots, protected
resources and known failures remain in the [R1 handoff](R1-COMPLETE-FUSE-HANDOFF-20261008.md).

## 2. Selected ownership and dependency direction

| Owner | Responsibility |
| --- | --- |
| `layerfs-fuse` | Native mount/profile/session/serving/drain, fuser callbacks and reply ownership, bounded admission/runnable queues/fixed workers, parked requests and completion wakeups, kernel operation handlers and cache coherence |
| `layerfs-daemon` | Application assembly, authenticated controls and sole Workspace registry, overall Ready/unmount composition, shared Overlay SQL service, direct Store/read admission and the existing Commit driver |
| Workspace | Filesystem semantics, immutable/effective read plans, mutation decisions, captured namespace adaptation and install continuity |
| Overlay | Atomic mutable/indexed state and native lookup/open/request/captured ownership; bounded physical reclamation |
| Content | Canonical file/namespace construction, validation, topology and formats |
| Storage / Persistence / History | Encoding/packs/Save; concrete database writes; authority and publication, respectively |

Implementation dependency direction is `daemon -> fuse -> workspace`; FUSE must
not import daemon, its concrete OwnerClient/Pending/Completion types, or application
configuration. Existing domain-type dependencies are reviewed only when used; this
arrow does not invent a new adapter crate or permit circular back-edges. Do not
copy a token/error schema merely to avoid reusing an existing owning contract.
Interfaces belong in Fuse only for real external service boundaries it consumes.
Workspace's semantic ports stay independently reusable outside FUSE.

Daemon `application/filesystem.rs` assembles one Fuse request service using its
configured limits and concrete engine ports. **One fixed K-worker Fuse dispatcher
is shared across that daemon's mounts** and lives through zero-mounted intervals;
per-mount connection/receive/handoff state does not imply a worker pool per mount.
Daemon `service/filesystem_port.rs` implements narrow asynchronous service interfaces;
it contains no second session, kernel reply state machine or request scheduler.

FUSE scheduling and Overlay scheduling are distinct. Fuse dispatch chooses a
runnable native continuation. The existing daemon SQL owner chooses a bounded SQL
job among filesystem, Commit and cleanup producers. Both have explicit credits
and fairness; neither holds the other worker or a global Workspace lock across
waiting, and neither serializes whole commands/Commits.

## 3. Ports must implement real park/resume behavior

Moving the current blocking OwnerClient adapters behind a trait is insufficient.
A native step must submit one bounded original demand, own a pending handle and
its exact inputs/credits, and park without occupying a receiver or service worker.
Completion/credit availability wakes that same continuation, not a replay of the
attempted operation. The interface must cover success, original error, lost
publisher/disconnection, cancellation/disposal and terminal shutdown.

The implementer must solve registration-before/after-completion races, single
outcome consumption, release wakeups and retained attempted work during drain.
`try_complete` plus busy polling, one sleeping thread per pending request or
blocking all K workers is not the mechanism. Extend the real completion/credit
owners and expose a narrow neutral service contract; preserve synchronous callers
where they remain appropriate. No new general executor/service-locator abstraction
is justified by the file layout alone.

Workspace resumable semantic steps reuse its current facts/decisions. Fuse
`operations/` drives those steps; it does not recode filesystem semantics. Immutable
Store I/O is performed only with actually admitted read capacity. Add a separate
`store/read_service.rs` only if existing Store ports cannot supply that bounded
service; it remains an immutable-demand service, not a duplicate kernel dispatcher.
Source failure scopes and SQL/provider locks must not span parks/provider waits.

## 4. Serving, Ready and terminal unmount have separate owners

FUSE reports connection facts: native mount/handshake, every configured receive
loop serving, request/reply/pending-work disposition, loop join and connection
drain. These receipts preserve unknown or partial startup/exit outcomes.

Daemon publishes overall Workspace Ready only after those serving facts **and**
registry/Workspace/service admission prerequisites hold. It owns the control
transition, not a second FUSE readiness engine. Likewise Fuse connection-drained
is necessary but insufficient for terminal unmount: daemon combines it with exact
namespace-bound SQL/completion/Store/control producer disposal, Overlay native
ownership revocation, logical Close and route removal. Physical deletion follows
through the existing bounded owner and remains visible as debt.

Normal detach/Busy service usability and this complete normal drain are R2.
Forced teardown, sustained concurrency and full frozen acceptance remain R6/R8;
R2 must preserve the required owners/interfaces without claiming those later proofs.
No separate control/commit module becomes a second Commit orchestrator.

The pinned-fuser readiness/join limitations remain unresolved engineering work:
constructor/one stat/thread spawn is not all-loop Ready; run errors may leave
unjoined loops. File movement, callback-entry witnesses and proc-directory metadata
do not establish missing liveness/exit evidence. No wider dependency patch is
selected. Maintain I-3/I-8/I-9/I-14 and the original proof obligations.

## 5. Changes and consolidations to the proposed tree

- Adopt Fuse `session/`, `dispatch/`, `operations/` and `coherence/`; remove planned
  daemon `native/` and kernel `request/steps/`. Mount configuration belongs to
  Fuse; daemon only validates/maps its application configuration into it.
- Keep existing files in their owning homes. Do not move Overlay `file_owners.rs`
  to `native_open.rs` merely for symmetry; only split for a real distinct native
  association/atomic job. Existing lookup methods remain the engine foundation.
- Start lookup/forget and open/release together in their operation homes. Split
  truncate/write or each namespace handler only when responsibility/size warrants;
  no empty scaffolds or rigid file-count target. `flush.rs` defines actual supported
  semantics, not backing durability or a new automatic Commit.
- Directory handles/cookies are indexed/backed with live last-owner cleanup.
  Native lookup aggregates and open owners remain distinct Overlay lifetimes;
  Fuse owns orchestration, never a whole-namespace resident map.
- Prefer the existing raw-record/backed construction facilities before adding
  scratch codecs/tables. `captured_namespace.rs` ports adapt existing reader jobs
  where sufficient; schema/query modules are conditional on missing indexed work.
- Existing Content `validate.rs`, `validate/{cycles,entries}.rs` and indexed
  reference reducers retain their owners. Add backed/incremental helpers for
  missing bounded validation; do not create a second validator or move correct
  code just to match the diagram. Directory changes reuse the current stream APIs.
- Put FUSE device/capability Create wiring in actual Sandbox
  `backend/docker/container.rs`/`container_types.rs`, inspection in
  `endpoint.rs`/`topology.rs`. Its `request.rs` is the ordinary Exec request owner;
  the proposal's device-setup annotation there would be misplaced.
- Fuse tests own real callbacks/session/coherence, including kernel mmap behavior.
  Daemon tests own external execution visibility and the aggregate control/drain/
  mounted-Commit route. Reuse existing test homes; no duplicate mmap engine or
  mandatory one-test-file-per-operation scheme.

The [reviewed destination](FINAL-CLUSTER-TWO-FILE-LAYOUT-20261008.md),
[rollout](CLUSTER-TWO-LOC-AND-ROLLOUT-20261008.md) and
[next-agent prompt](HANDOFF-R2-R5-IMPLEMENTATION-20261008.md) incorporate these choices.
R4 component work can overlap R2/R3; R5 integrated proof depends on all three.
No old numerical LOC projection is mechanically reassigned across crates. New
estimates require a fresh deepest-file analysis; source count is not a design gate.

All unchanged owner requirements apply: Disposable/WAL/OFF only; separate Overlay
profile; direct shared-volume Store; host control-only after Init/install; writeback
off; one Commit construction producer; locked/bounded checks; authorized fuser
patch integrity only; no replay; exact per-commit LOC and no early retirement or
remote publication. This documentation review completes no R2–R5 proof row.

## 6. Review verification and accounting

The [document/source check](checks/r2-r5-ownership-review-20261008/02-document-check.json)
and [review verification](checks/r2-r5-ownership-review-20261008/03-review-verification.json)
record local links/anchors, exact unchanged R1 source, current membership and
preserved evidence/notes. No Rust build, runtime proof, new agent or container
was needed for this documentation-only review; R1 evidence retains its scope.

Production LOC:170673 ->170673 (delta +0). Core105256, active62382, reference65417,
excluded predecessors37431 and excluded integration5443 are unchanged. The
[exact parent/staged count](checks/r2-r5-ownership-review-20261008/04-exact-production-loc.json)
uses the same pinned production counter and unchanged member/per-file classification.
Proposed responsibility changes are not implemented relocations or retirement.
The full objective remains unfinished and work stops after this review/handoff.
