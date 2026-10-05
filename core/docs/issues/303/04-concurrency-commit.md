# 04 — Concurrency and Commit: contract routing and review rationale

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Source pin `f96d97651`, reviewed design `334fc7437`, owner revisions 2026-10-05.
> No implementation or measurement. Current contracts live in the primary docs
> below; this file preserves earlier section anchors and the rejected reasoning.

## 1. Concurrency model

See [daemon engine](daemon-sqlite.md) for shared SQL admission, payload ownership
and service windows, [FUSE](fuse.md) for deferred replies and
[Commit](workspace-api/commit.md) for independent captures/Save capabilities.
One shared SQLite database does not provide multiple concurrent write transactions.

## 2. State machine

The [Commit operation](workspace-api/commit.md) owns phase states, exact outcomes
and capture retention. No new state machine is defined here.

## 3. Capture

The captured namespace/data domain is immutable and indexed by Workspace/generation;
cursors must not chase active inserts. See [engine](daemon-sqlite.md) and
[Commit](workspace-api/commit.md). Syscalls split into requests can straddle capture;
only daemon-acknowledged requests are covered, not unflushed mmap stores.

## 4. Construction from the captured state

The [Commit doc](workspace-api/commit.md) and [integration APIs](06-cluster-one-integration.md#2-cluster-one-apis-cluster-two-calls)
map normalized edits, streams and PreparedRows. A caller scratch index does not
remove EDIT_DEFERRED_LIMIT, directory Vec/new-parent memory or sparse-zero work.
Required integration corrections remain visible; full ignored/dependency/cache
state cannot be excluded to make construction fit.

## 5. Save, stage, transition, install, retire

These are separate completions in [Commit](workspace-api/commit.md). Runtime
adapters embed current cluster-one libraries; the retired layerfs-server is not
restored. No Save capability owns a transport connection for its whole lifetime.

## 6. Outcomes

[Commit](workspace-api/commit.md) is authoritative. Unknown pre-stage Save is not
unknown history publication. Unknown stage/transition/required discard retains
exact custody. Absence from two reads does not fence a still-running operation.
UpToDate creates no new filesystem Commit record; invocation audit is separate.

## 7. Open-unlinked files

Repeated capture pins at 334fc7437 could retain N versions for one descriptor.
The [engine](daemon-sqlite.md) must supply independent bounded orphan ownership,
with captured pre-unlink sources retained only for their exact operation lifetime.
This correction remains an implementation prerequisite, not a proven algorithm.

## 8. Fold after a Commit that did not succeed

Foreground replay of the smaller payload stream while an inode is busy is withdrawn:
it can pause a logger for O(payload). Short local resolution plus bounded
consolidation must preserve active precedence/metadata and limit read depth under
continuous writes and repeated failures. An unbounded list of failed generations
is also invalid. See [engine](daemon-sqlite.md) and [Commit](workspace-api/commit.md).

## 9. Required interleavings

[Validation](07-implementation-validation.md) collects fragmentation, growing active
namespace during tiny capture, shrink/install cleanup races, repeated orphan Commit,
failed large-stream resolution, concurrent runtime service and outcome-aware drain.
Operation docs include corresponding workloads and ASCII ownership/timing diagrams.

## 10. Second Commit and terminal unmount

[Terminal unmount](workspace-api/unmount.md) includes logical close and automatic
cleanup, no separate public close. Commit/unmount concurrency and forced unknown
custody have explicit dispositions; no successful teardown guesses NotPublished.
A second Commit for the same Workspace never queues a later implicit capture.
