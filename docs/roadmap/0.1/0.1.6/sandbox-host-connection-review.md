# Sandbox/host connection simplification: independent review record

Date: 2026-09-15. Tracking: [#150](https://github.com/Ephemeral-AI-Lab/layerfs/issues/150).
Design: [reviewed connection architecture](sandbox-host-connection-architecture.md).
Parent: [#149](https://github.com/Ephemeral-AI-Lab/layerfs/issues/149).

The owner explicitly requested subagent review, dedicated documents, and a new issue, then requested ASCII before/after diagrams, an explicit removal list, and the justification for transferring mutable changes at snapshot Commit time.

Three subagents completed independent read-only reviews while the parent consolidated the documents. No reviewer changed code, ran benchmarks, checked out a branch, or posted an issue. The parent authored the resulting design and issue. Existing source remains main `7b73c4b33c950c3ce3cd192ea7b571398bda3f8f`; the implementation starting point is the peeled v0.1.5 commit `6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13`.

| Reviewer | Responsibility |
|---|---|
| `connection_boundary_review` | Transport and payload ownership, remaining RPCs, service fairness, lifecycle/SDK locking, snapshot cancellation |
| `v015_flow` | Atomic database publication, admission rollback, host outcome ownership, C1/C2 correctness and lost replies |
| `benchmark_audit` | Resource/performance claim boundaries, physical dependency closure, durability/cloud extensibility and scope discipline |

## Findings and dispositions

| ID | Priority | Finding | Disposition in the connection design |
|---|---|---|---|
| R1 | P1 | v0.1.5 local metadata still uses host payload RESERVE/APPEND and dirty-fact mirroring | Localize replacement backing too; reuse only the validated immutable-base/read behavior |
| R2 | P1 | One whole-stream control transaction can starve SDK/status/cold reads | Reuse fixed role separation and bounded live/control reservations; test progress with bulk transfer held |
| R3 | P1 | Restoring the legacy SDK route can restore a whole-Commit lifecycle lock | Route SDK mutations through local owner ordering; no lifecycle lock across remote edit or construction |
| R4 | P1 | Old checkpoint completion clears all dirty/generation state and can erase G+1 | Exact incarnation/generation coverage and newer-key guards; preserve compact canonical provenance |
| R5 | P1 | Persistent receipt/history recovery is broader than the live-host failure model needs | Default to one bounded host-memory attempt/result; infallible synchronous success handoff; SQL marker only for a demonstrated ambiguity gap |
| R6 | P1 | One Commit worker does not protect rollback from unrelated Store operations | Preserve existing Store admission/operation permit and rollback ownership |
| R7 | P1 | Existing root-row/foreign-key presence does not establish all decoding dependencies | Preserve checked admission plus logical and transitive physical DELTA-base closure; no new full-tree scan |
| R8 | P1 | Sandbox/caller cancellation can race database publication | Abort before publication; once publication begins, host finishes/resolves the exact transaction independently |
| R9 | P2 | “Only communicates at Commit” hides remaining control, immutable fetch and reverse canonical-result traffic | Scope the claim to local hot mutations and Commit-time mutable-state transfer; show all remaining flows |
| R10 | P2 | Generic public exactly-once retry is not provided by current workspace-id-only SDK API | Scope supported exact outcomes to Store/coordinator and sandbox coverage; no automatic new capture masquerading as retry |
| R11 | P2 | One snapshot and one worker do not prove bounded bytes, lifetime or total CPU | Bound retention/queues; account service CPU, base caches, provenance and actual transfer copies |
| R12 | P2 | Durable/cloud extension can reintroduce speculative logs/frameworks | Preserve object/publication boundaries; defer machinery to #69/#82/#52 |

All findings were incorporated as design requirements or explicit limitations. This is a source/design review disposition, not an implementation PASS or performance result.

## Source evidence

**R1 — incomplete locality of the v0.1.5 base.** The tagged local owner reserves host spool windows and emits APPEND frames. Host `BackingOwner` owns spool segments and mutable facts. A branch based on v0.1.5 must change both metadata snapshot behavior and payload placement.

- [v0.1.5 buffering/reservation](https://github.com/Ephemeral-AI-Lab/layerfs/blob/6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13/crates/layerfs-fuse/src/live_owner.rs#L1280-L1354)
- [v0.1.5 host spool/facts](https://github.com/Ephemeral-AI-Lab/layerfs/blob/6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13/crates/layerfs-workspace/src/live_backing.rs#L19-L42)
- [validated immutable seed/read context](https://github.com/Ephemeral-AI-Lab/layerfs/blob/6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13/crates/layerfs-workspace/src/live_backing.rs#L230-L268)

**R2/R3 — transport and lifecycle starvation.** Current control grouping retains a mutex across a frame group; the existing transport already distinguishes authenticated service roles. The legacy SDK branch holds `worker.lifecycle` before remote editing. A held-builder test alone will not detect a bulk-stream/control deadlock.

- [control grouping](https://github.com/Ephemeral-AI-Lab/layerfs/blob/7b73c4b33c950c3ce3cd192ea7b571398bda3f8f/crates/layerfs-fuse/src/live_transport.rs#L181-L237)
- [connection roles](https://github.com/Ephemeral-AI-Lab/layerfs/blob/7b73c4b33c950c3ce3cd192ea7b571398bda3f8f/crates/layerfs-fuse/src/live_transport.rs#L296-L330)
- [existing admission reservations and thread dependencies](https://github.com/Ephemeral-AI-Lab/layerfs/blob/7b73c4b33c950c3ce3cd192ea7b571398bda3f8f/crates/layerfs-fuse/src/live_runtime.rs#L57-L203)
- [host versus legacy SDK locking](https://github.com/Ephemeral-AI-Lab/layerfs/blob/7b73c4b33c950c3ce3cd192ea7b571398bda3f8f/crates/layerfs-workspace/src/lifecycle.rs#L944-L962)

**R4 — checkpoint completion is not a live-generation acknowledgment.** v0.1.5 checkpoint completion resets mutation paths/generation/dirty state. It must not run on a successor that has accepted later writes. Compact correspondence semantics remain necessary even though their disk-heavy implementation is removed.

- [v0.1.5 checkpoint installation/reset](https://github.com/Ephemeral-AI-Lab/layerfs/blob/6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13/crates/layerfs-workspace-core/src/checkpoint.rs#L31-L99)
- [current newer-key guard](https://github.com/Ephemeral-AI-Lab/layerfs/blob/7b73c4b33c950c3ce3cd192ea7b571398bda3f8f/crates/layerfs-workspace/src/overlay.rs#L690)

**R5/R8/R10 — narrow outcome ownership.** The host must not lose a known successful transaction result on caller cancellation or sandbox failure. A synchronous result assignment immediately after successful SQL completion is the minimum for a live-host profile. Persistent receipts are an optional response to a demonstrated wider failure requirement. Current public Commit takes a Workspace ID rather than a caller idempotency token; its semantics must not be overstated.

- [v0.1.5 publication and precomputed outcome](https://github.com/Ephemeral-AI-Lab/layerfs/blob/6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13/crates/layerfs-layerstack-store/src/workspace.rs#L490-L603)
- [current persistent outcome types/resolution](https://github.com/Ephemeral-AI-Lab/layerfs/blob/7b73c4b33c950c3ce3cd192ea7b571398bda3f8f/crates/layerfs-layerstack-store/src/staging.rs#L97)
- [current outcome acknowledgment before public return](https://github.com/Ephemeral-AI-Lab/layerfs/blob/7b73c4b33c950c3ce3cd192ea7b571398bda3f8f/crates/layerfs-workspace/src/commit_attempt.rs#L278-L295)
- [public SDK Commit](https://github.com/Ephemeral-AI-Lab/layerfs/blob/7b73c4b33c950c3ce3cd192ea7b571398bda3f8f/crates/layerfs-sdk/src/client.rs#L213-L232)
- [SQLite transaction errors](https://www.sqlite.org/lang_transaction.html), [autocommit state API](https://www.sqlite.org/c3ref/get_autocommit.html)

**R6/R7 — preserve real Store safety.** Admission rollback owns a pack baseline; removing its operation permit can let rollback affect another operation. A complete candidate must retain all dependencies necessary for reading, including physical delta bases not directly named by the logical root graph.

- [v0.1.5 admission/permit](https://github.com/Ephemeral-AI-Lab/layerfs/blob/6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13/crates/layerfs-layerstack-store/src/objects.rs#L2241)
- [v0.1.5 rollback ownership](https://github.com/Ephemeral-AI-Lab/layerfs/blob/6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13/crates/layerfs-layerstack-store/src/objects.rs#L2508)
- [existing dependency closure](https://github.com/Ephemeral-AI-Lab/layerfs/blob/6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13/crates/layerfs-layerstack-store/src/objects.rs#L4071)
- [logical/physical closure contract](https://github.com/Ephemeral-AI-Lab/layerfs/blob/6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13/docs/roadmap/0.1/0.1.5/spec.md#L273-L277)

**R9/R11/R12 — precise claims and future compatibility.** The budgets are selected targets, not demonstrated capacity. One worker does not eliminate concurrent service CPU. Durable Commit can be added without durable live writes, and remote pack placement must preserve dependency availability before transactional head publication. No cross-object or object-store/SQLite transaction is supplied by S3.

- [#69 durability boundaries](https://github.com/Ephemeral-AI-Lab/layerfs/issues/69)
- [#82 SQLite/immutable-pack hybrid](https://github.com/Ephemeral-AI-Lab/layerfs/issues/82)
- [#52 cloud scope](https://github.com/Ephemeral-AI-Lab/layerfs/issues/52)
- [SQLite journal-mode limitations](https://www.sqlite.org/pragma.html#pragma_journal_mode)
- [S3 consistency/atomicity](https://docs.aws.amazon.com/AmazonS3/latest/userguide/Welcome.html#ConsistencyModel)

## Remaining unproved items

Compact snapshot data structure and actual 500/25k resource feasibility; independent live service progress during a stalled transfer; the host-memory publication handoff under the selected runtime; generation-safe canonical results and incremental C2; and the separately proposed writable shared-mmap scope change. None was implemented or measured by this review.

The resulting connection issue defines these architecture refinements and focused proof requirements. #149 remains the requirements/gate contract. The subsequently created [execution issue #151](https://github.com/Ephemeral-AI-Lab/layerfs/issues/151) tracks their combined implementation and evidence through the three benchmark phases; no independent duplicate implementation campaign is required here.

## Post-review owner clarification: multiple Workspaces in one sandbox

The owner subsequently clarified that multiple Workspaces must coexist in one sandbox. The parent updated the design with `/workspaces/<workspace-id>/` visible mounts, per-ID private backing and snapshot state, a shared daemon registry, sibling-preserving teardown, aggregate resource admission and one focused two-Workspace proof. This is a documented owner requirement and source inspection, not an additional subagent or runtime verification pass. The existing daemon already keys mount state by WorkspaceId and rejects duplicate IDs/roots; the new mutable owners must retain that separation.

The owner then selected `/snapshots/<workspace-id>/` as the private counterpart to the live mount, with one concurrent Commit per Workspace. The design uses one active in-memory snapshot slot with shared payload backing in that directory; it does not materialize a second file tree. Stable paths still require internal incarnation/generation/attempt checks, and live-owned backing is retained across Commit rather than deleted with the snapshot handle.

The owner subsequently selected the fast-lane performance order: `tiny-create-500-mixed-v4`, then `tiny-bulk-create-500-mixed-v3`, then the retained 25k case. The bulk fixture genuinely creates 5,000 files / 500 MiB; its transfer and encoding costs are included in Commit and its payload is not subject to the 25k-only 32 MiB backing ceiling. All existing time/CPU/memory gates and the one-sample-per-case/arm rule remain in force. This is a pre-measurement plan update, not an additional review or performance result.
