# Defer logical diff, conflicts and conflict resolution to v0.2.0

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Owner scope decision, 2026-09-16: omit the proposed Comparison/Reconciliation
component from the v0.1.7 replacement and introduce the redesigned feature family
in [v0.2.0 issue #164](https://github.com/Ephemeral-AI-Lab/layerfs/issues/164).
Related: [seven-component map](cluster-1-2-components.md),
[co-design](content-storage-co-design.md), [source audit](cluster-1-2-source-audit.md).

## What this changes

The reference implementation already contains logical Diff, reconciliation and
conflict APIs. Ordinary Workspace Commit does not automatically run that feature;
it enters reconciliation only when explicit resolution state exists. This decision
reduces the replacement's target scope. Root crates/ stays intact for reference
during migration; no production source is deleted by this design update.

The seven remaining responsibility groups are:

```text
CLUSTER 1                           CLUSTER 2
---------                           ---------
1. Canonical objects                4. Object store / admission
2. File content                     5. Physical encoding
3. Filesystem tree and metadata     6. Packing
                                    7. SQLite persistence

v0.2.0, separately:
  logical diff -> conflict detection / reconciliation -> conflict resolution
```

Package boundaries remain undecided; telemetry is still the only agreed new crate.

## Workflow before and after

```text
REFERENCE
  ordinary Workspace -> edit -> construct -> conditional Commit
                                      |
                                      +-> explicit HeadMoved

  explicit reconciliation Workspace
    -> compare Base / Incoming / Current
    -> provisional candidate + conflicts
    -> conflict choices + fingerprints/path generations
    -> preview -> reconciliation publication -> presentation refresh

  independent Diff -> enumerate -> temporary spool -> pages -> CLI output

v0.1.7 TARGET
  Workspace -> edit -> frozen input -> construct -> admit
                                                 |
                         conditional publication against captured context
                                  /                       \
                            matches                       stale
                               |                            |
                      Commit / UpToDate                  HeadMoved
                               |                      no merge/rebase
                    revision-safe completion          no silent replay
```

The new target makes the ordinary path the sole path. Preserve captured head,
base, root and generation; do not fetch a newer head and silently adopt it as
the candidate's parent. Keep final transactional compare-and-swap even if a cheap
precheck rejects stale inputs early. Add already has an explicit HeadMoved result.

## What to omit from the replacement

Three focused subagent re-reviews traced canonical, storage and runtime callers
at `a8a1ba848429d5f2fbba83c2de22dada8c29def9`. Their findings are source analysis;
no implementation, tests or performance measurements were run for this decision.

| Feature code in the reference | Replacement disposition |
| --- | --- |
| [Filesystem diff](../../../../../crates/layerfs-content/src/filesystem/diff.rs), [filesystem reconciliation](../../../../../crates/layerfs-content/src/filesystem/reconcile.rs) | Omit public diff, three-way merge, conflict records and choice application |
| [Rope diff](../../../../../crates/layerfs-content/src/file/rope/diff.rs), [directory diff](../../../../../crates/layerfs-content/src/tree/directory/diff.rs), comparison-only inode/metadata helpers | Omit feature-specific traversal and merge code after tracing retained callers |
| [Store visit_diff](../../../../../crates/layerfs-layerstack-store/src/query.rs#L206) and [reconciliation preparation/publication](../../../../../crates/layerfs-layerstack-store/src/workspace.rs#L50) | Omit diff orchestration, PreparedReconciliation and reconciliation-specific commit routes |
| [Candidate combination](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L3811), [SnapshotReader overlays](../../../../../crates/layerfs-layerstack-store/src/workspace.rs#L27) | Nonempty overlays and candidate union are reconciliation-specific; omit this reader search/copy path, retain construction scratch separately |
| [Workspace reconciliation](../../../../../crates/layerfs-workspace/src/reconcile.rs) | Omit resolution state, conflict paging, choices, fingerprints, reconciliation workspace creation and refresh-to-merged-root |
| [Conflict path invalidation](../../../../../crates/layerfs-workspace/src/reconcile.rs#L183) | Omit reconciliation-only mutation_paths bookkeeping and its copies after the final consumer check |
| [SDK Diff spool](../../../../../crates/layerfs-sdk/src/request.rs), [public methods](../../../../../crates/layerfs-sdk/src/client.rs#L139) | Omit Diff handles/pages/spool codec and public diff/conflicts/resolve surfaces |
| [CLI dispatch](../../../../../crates/layerfs-cli/src/lib.rs#L459) | Omit layerstack/branch diff and workspace conflicts/resolve commands, variants and output plumbing |

No .layerfs-conflicts virtual directory implementation was found. Do not claim
its removal or create one as a placeholder. The reference reconciliation workspace
is an explicit Host + Materialize route; omitting it removes that feature's
materialization requirement, not every unrelated projection capability.

## What must remain

| Retained logic | Why it is not the deferred feature |
| --- | --- |
| Canonical authentication, ObjectId equality and exact CAS collision/reuse comparison | Content integrity and deduplication |
| Physical delta matching, FULL/PREFIX selection and base validation | Storage encoding, unrelated to logical filesystem diff |
| Ordinary path resolution and lookup | Needed for read/stat/create/rename/delete |
| Local byte equality, no-op checks and structural identity reuse | Avoid unnecessary construction and preserve unchanged results |
| Sorted-update old/new edge accounting | Required hardlink/reference counts and deletion correctness |
| Directory paging and generic bounded tree traversal | Ordinary reads and mutations use these too |
| Node revisions, dirty/frontier sets and mutation generation | Frozen capture and post-capture edit safety |
| Kernel-cache coherence around SDK writes | FUSE consistency, not Branch conflict resolution |
| Head/base/root validation and atomic conditional publication | Prevent stale overwrites even without a resolver |
| Rollback, stage ownership as required, quarantine and exact completion identity | Correct failure handling after partial admission or successful publication |

Names alone are insufficient deletion criteria. In particular:

- [filesystem/resolve.rs](../../../../../crates/layerfs-content/src/filesystem/resolve.rs)
  resolves filesystem paths. The deferred resolver is a **conflict resolver**.
- [directory_page_after](../../../../../crates/layerfs-content/src/tree/directory/read.rs#L277)
  uses a directory cursor that also appears near diff code.
- [inode cursor helpers](../../../../../crates/layerfs-content/src/tree/inode/cursor.rs#L240)
  include shallow loading/shape checks used by COW mutations; omit diff-only types,
  not the whole file by name.
- [WorkspaceDiff](../../../../../crates/layerfs-workspace/src/session.rs#L127)
  currently holds dirty/generation status. Keep the required status semantics
  under a clear status surface; it is not logical two-root diff.
- [FUSE coherence](../../../../../crates/layerfs-fuse/src/live_owner.rs#L1015)
  and [published completion recovery](../../../../../crates/layerfs-workspace/src/remote_commit.rs#L344)
  must not be removed just because their code uses the word reconcile/resolve.

## Publication and staging require an explicit contract

Keep construction tied to its captured context, as the current
[ordinary commit](../../../../../crates/layerfs-workspace/src/lifecycle.rs#L86)
and [final checks](../../../../../crates/layerfs-layerstack-store/src/workspace.rs#L500)
already do. Validate that context before reporting UpToDate under a strict
stale-head rule; the reference direct path has an earlier no-op shortcut that
must not be copied blindly.

Normal Workspace commit also uses staging, independently of reconciliation.
Therefore this deferral alone does not justify deleting workspace_stages or
rollback/retention. A later simplification may choose terminal HeadMoved with
explicit attempted-candidate cleanup while preserving working edits. It must prove
cleanup across already committed private cohorts and cannot replace batching with
one unbounded transaction. Until that policy is agreed, keep the required stage
ownership and failure states.

Published-but-completion-pending is still published. Retain the exact root/head
and completion token; never report it as an unpublished operation safe to repeat.
The later owner decision is [one attempt with no retries](physical-encoding-and-packing.md#one-attempt-no-retries).
The replacement disables the existing SQLite busy timeout and automatic SQL/SDK
retries; BUSY/LOCKED fails. Planned bounded queue backpressure before execution
remains ordinary work. Lost acknowledgement is failure with unknown persistence
outcome, never permission to repeat or delete the write.

## Compatibility and test coverage

The [v0.1.7 release plan](../README.md#owner-scope-decision-diff-and-conflict-features)
records this owner-directed exception to SDK/CLI feature parity. Omit the deferred
surfaces from the replacement; provide no silent legacy fallback or empty-success
stubs. Preserve remaining public behavior, canonical identity, storage integrity
and publication rules unless a separate decision changes them.

Do not rewrite released manuals or historical receipts. Before candidate release,
explicitly classify affected tests/benchmark selections as deferred/unsupported
under this scope decision. Do not silently drop registered cases or mark unrun
feature tests PASS. Retain construction, no-op, reference accounting, physical
encoding and stale-publication tests outside product src/.

## Is the structure greatly simplified?

Yes for workflow/API scope: no comparison result pipeline, three-way candidate
composition, conflict workspace, resolution choices/invalidation, or reconciliation
snapshot overlays. The component map shrinks from eight groups to seven. Removing
the reconciliation-only path-generation map also removes real edit-time bookkeeping
and copies, once its final consumers are excluded.

The core storage/CDC/COW algorithms remain substantial and necessary. Most conflict
caches/trees allocate only when those features run; their absence does not prove
that ordinary Commit or SQL is faster. Measure production LOC when code is changed
and qualify retained-operation time/space/memory behavior. This design update has
no production-code deletion or measured performance reduction to report.
