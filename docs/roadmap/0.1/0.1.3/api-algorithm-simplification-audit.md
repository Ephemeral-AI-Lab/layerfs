# API, endpoint and algorithm simplification audit

Date: 2026-09-05. Three subagents audited public SDK/CLI, FUSE/daemon operations,
and construction/admission algorithms. Pinned product source:
`810bb3a589ac58d103483df34bb58ecfe0f0ddf4`, checkout
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs-p21-integration`.

Read-only investigation; no code changes, builds, tests or measurements. #45 is
actively changing benchmark infrastructure; its files were not modified by this
audit. Proposed product changes follow its handoff. Existing public compatibility
and POSIX/publication semantics remain constraints.

## Main finding

Reduce independent implementations and contradictory validation/observation first.
Many public methods and wire variants are already thin adapters to one algorithm.
Replacing them with a catch-all endpoint or a bag of policy flags would hide the
same branches rather than simplify behavior.

Concrete target: common per-file construction, one bounded admission accumulator,
one native task-scheduling implementation, and content-owned tree-update fallback.
A single bounded Workspace dirty-state planner is a later goal after all current
planner domains transfer. Keep native versus Workspace input adapters and their
different publication/lifetime ownership explicit.

## Reduction counts: methods versus implementations

These are proposed targets, not completed deletions. The SDK inventory was
checked against the pinned source: 28 public Client methods, of which two are
feature-gated verification helpers and 26 are production methods.

| Surface | Current | Initial compatibility-preserving change | Possible later reduction |
| --- | ---: | --- | --- |
| Production SDK methods | 26 | **0 removed; 26 remain**. Unify implementation/observation behind existing methods. | **One Commit variant** may be removed through a versioned migration: 26 -> 25 if no other API changes occur. |
| Feature-gated verification helpers | 2 | No removal proposed by this audit | Separate test surface; do not include them in production SDK counts. |
| FUSE wire requests | One confirmed stale candidate: `PinRead` | Preserve acknowledged production readonly-open behavior; decide compatibility before pruning | **One identified request for removal**, not a guaranteed net protocol-count reduction if another change adds a combined attribute request. |
| Internal Rust methods | No validated net deletion count | Extract shared helpers and remove duplicated bodies | Count the actual implementation diff; a new helper can reduce duplicated code without reducing method count. |

The current Commit methods already delegate to one implementation. Making the
full-status API canonical is an internal/caller migration first; retaining the
legacy signature is not retaining another Commit algorithm. Do not break its
return type or remove either signature in a compatibility-preserving patch.

| Algorithm implementation area | Current independent implementations | Target | Qualification |
| --- | ---: | ---: | --- |
| Native task scheduling/orchestration | 3 | 1 | Preserve direct/private-candidate sinks and unsupported-shape handling until domains transfer. |
| Admission accumulation/orchestration | 3 | 1 shared accumulator | Operation-specific final flush/publication remains explicit; real planned-admission callers must migrate. |
| Native regular-file construction bodies | 2 | 1 | Pass already-read metadata; preserve canonical identity and hard links. |
| Workspace inode-record construction bodies | 2 | 1 | Preserve capture, incremental extents, metadata and counters. |
| Workspace candidate planners | 3 | Eventually 1 | Long-term target only after alias/noncanonical/fallback domains and bounded sparse behavior transfer. |

These rows are not additive method-deletion counts. Some are repeated bodies,
some orchestration paths, and some retained wrappers around shared algorithms.
Report algorithm consolidation, public API removals, wire changes and actual
method/line deltas separately. No deletion quota is a family performance gate.

## Public SDK findings

The inspected Client has 28 public methods, including two feature-gated
verification methods. This is one Rust SDK, not 28 HTTP endpoints. Method count
alone is not a useful deletion target.

| Finding | Proposed simplification | Current source reference |
| --- | --- | --- |
| Legacy Commit wrapper discards presentation health; CLI uses it | Make full status canonical for actual CLI/internal callers; preserve result-only forwarding for released compatibility, consider signature reduction in a versioned change | `layerfs-sdk/src/client.rs:211`; `layerfs-cli/src/lib.rs:529` |
| Singular and batch file edits share atomic core but differ in SDK observation/validation | One batch-level observation/validation boundary; singular delegates once; retain failure-atomic same-file batching and one projection refresh | `layerfs-sdk/src/client.rs:250`; `layerfs-workspace/src/lifecycle.rs:714`, `:748` |
| Generic Query accepts filters some kinds silently ignore | One validation point rejects unsupported filter/kind combinations; keep typed QueryKind and paging | `layerfs-sdk/src/query.rs:41`; `client.rs:388`; CLI `lib.rs:799` |
| SDK reexports Store with both old and staged commit APIs | Consolidate lower admission/publication primitives first; migrate real reconciliation callers before deprecating advanced mutation APIs | `layerfs-sdk/src/lib.rs:16`; Store `workspace.rs:211`, `:276`, `:437` |

The old Commit method is already a wrapper over `commit_workspace_session_with_status`,
not a second Commit algorithm. Do not silently change its return type in a
compatibility-preserving refactor. Full status must distinguish durable publication
from failed presentation; callers must not retry a published Commit as though it
rolled back. Coherent access to the existing recovery operation belongs in CLI
follow-up, not a new Commit policy framework.

The single-edit Workspace method already delegates to the atomic batch core.
Do not replace a batch with a loop of singular calls: it changes failure atomicity
and repeats projection work. If observation is unified, preserve one operation
receipt per public batch and account for any added monitoring overhead.

Already unified and worth retaining:

- `connect` / `connect_with_container` both use `connect_inner`.
- Exec and Shell both use `spawn`; Shell has interactive/PTY and shell-selection
  semantics. Output and Stop have different reader/process lifecycle effects.
- Recovery restores an already-published presentation; End releases/discards
  resources. They are not interchangeable operations.
- Monitor snapshots/counts are cheap direct reads; routing them through an
  observing catch-all query can change observability semantics.

## FUSE and daemon endpoints

Wire requests are not separate algorithms. Most already dispatch into common
Workspace methods. The daemon's process/mount lifecycle protocol is also distinct
from filesystem operations and should not be merged into one opaque envelope.

| Target | Consolidation | Required semantic distinction |
| --- | --- | --- |
| Ordinary/reserved creation | Share node creation, namespace binding and mutation bookkeeping behind thin allocation/reservation adapters | Host-allocated acknowledged create versus reserved/deferred create |
| Create-open handlers | Share create+pin/writer-accounting/error-cleanup implementation | Creating an inode alone versus retaining a live writable handle |
| Closed-create batch lifecycle | Share per-file closed-create operation; retain one production batch callback/lock | Batch partial progress, deferred errors and bounded input |
| One kernel `setattr` split into multiple calls | One canonical apply-attributes operation for that callback; old field methods can remain thin adapters | Preserve existing validation/order/partial-error behavior; do not combine separate syscalls |
| `PinRead` | Candidate for versioned wire pruning: no current production sender found | Keep the current acknowledged readonly Pin and its lifetime proof |

Creation duplicates are in Workspace `cow_tree.rs:404` and `:787`, and projection
open handlers `projection.rs:554` and `:581`. Physical spool allocation already
shares `file_io.rs::new_spool_node_reserved_inner`; do not create another spool
engine. The trait default closed-create loop is in FUSE `port.rs:104`, with a
production one-lock equivalent in Workspace `projection.rs:604`.

`UnlinkBatch` already loops over the same `workspace.unlink` under a shared lock
(`projection.rs:692`). It is transport amortization, not another deletion
algorithm. Removing the production override in favor of per-item trait calls can
increase lock/callback overhead despite reducing source lines.

`filesystem.rs:92` decomposes `setattr`; `proxy_client.rs:955` contains individual
metadata request behavior. A combined operation can reduce repeated flushes and
round trips for one kernel callback, but changing atomicity or merging separate
acknowledged calls is a different API contract.

`PinRead` references remain in codec/classification/dispatch/tests, while the
production readonly client sends acknowledged `Pin(node,false,false)` at
`proxy_client.rs:762`. Prune encoding/decoding only with an explicit wire
compatibility disposition; never renumber tags or resurrect asynchronous pinning
to remove an acknowledgement.

Retain meaningful distinctions: Fence versus fsync, Write versus compact
WriteZero, pin/unpin versus read, readdir versus readdirplus, and live-open versus
closed-create installation. Buffer limits and decoder acceptance limits protect
different boundaries. Keep validation and deferred-error acknowledgement in one
place each, but do not merge limits merely because their names look similar.

## Algorithm routes and concrete deletion candidates

| Area | Current runtime routes | Intended simpler shape |
| --- | --- | --- |
| Native import | Direct streaming; buffered parallel fallback; serial import | One discovery/task scheduler feeding existing direct/private-candidate sinks; serial becomes one-worker execution once its domain transfers |
| Workspace candidates | Localized content-only; frontier; full-manifest fallback | Shared per-inode construction first; eventually one bounded dirty/frontier planner if all fallbacks can transfer |
| Admission orchestration | Direct accumulator; checked complete candidate; planned missing-only admission | One owned-object bounded accumulator and checked insertion; operation caller owns final flush/publication |
| Tree updates | Initial specialized builders; sorted updates; incremental fallbacks | One content-owned update/fallback entry where valid; retain specialized streamed initial builder until canonical/memory parity is proved |

Native direct and buffered loops both use `NativeImport` but repeat discovery,
task assignment, producer setup, result merge and cross-task hard-link handling
(`layerstack.rs:1076`, `:1701`). First share the duplicated regular-file record
construction (`:2022`, `:2160`) and pass already-read metadata so reuse does not
introduce another stat call. Then consolidate task orchestration. Test-only
`legacy_directory_root` and append-only construction are differential oracles,
not extra runtime policies.

Admission duplication is in `InitializationSegmentAdmission` (`objects.rs:2787`),
`admit_checked_objects` (`:2985`) and `admit_planned_objects_with_limits` (`:3104`).
Share fixed byte/count accounting, owned pending-object handling, flush/receipt
logic and the existing checked row inserter. Callers may explicitly flush all or
take the remaining batch for their publication transaction. Do not add a
`Policy { direct, bulk, staged, final, ... }` object.

The old planned path is still used by reconciliation (`workspace.rs:211`) and
native fallback. Migrate those callers while preserving their transaction/head
rules; then delete unused membership-plan/bulk-insert plumbing. Do not manufacture
a Workspace identity or stage to force unrelated publication through the staged
Workspace API. Likewise, `finish(root)` cannot globally become
`finish_all_reachable`: mutation candidates can contain discarded objects.

Workspace localized/frontier construction duplicates existing-record resolution,
capture/incremental/full-content selection, metadata, inode record emission and
CDC accounting (`changes.rs:445`, `:775`). Share this operation before deleting
planners. The full-manifest fallback covers unmapped/noncanonical/alias and other
states and also participates in reconciliation fingerprints; removing one caller
does not make every manifest reader dead.

Workspace callers currently implement sorted-update fallback themselves
(`changes.rs:626`, `:1650`). Put that algorithm choice beside the content helpers
once, rather than repeating it in future native/SDK adapters. Existing
initial-inode construction accepts streamed pairs without first sorting an entire
manifest; replacing it must preserve its memory advantage or use bounded sorting.

## Implementation sequence and checks

1. Full Commit status in actual callers, shared edit observation/validation and
   rejection of ignored Query filters. Keep compatible forwarding APIs.
2. Shared native regular-file and Workspace inode-record construction: remove
   duplicated algorithms without changing their selection domains.
3. One bounded checked-admission accumulator; migrate real old callers before
   deleting planned/bulk wrappers. This aligns with #38's admission optimization.
4. Consolidate native scheduling and content-level sorted fallback; delete old
   orchestration only after its supported shapes transfer.
5. Consolidate create/open/closed lifecycle handling and per-callback attributes,
   retaining transport acknowledgements and batching boundaries.
6. Evaluate removal of Workspace planners and versioned public/wire reductions
   only after caller/domain and compatibility checks establish they are redundant.

Use existing small checks for post-publication presentation failure/recovery,
atomic failed batch edits, Query paging/filter validation, duplicate objects
within/across admission batches, collision failure, stale-head/stage retention,
canonical split boundaries, mixed-root hard links, sparse/frontier alias behavior,
capture invalidation and acknowledged readonly/unlink lifetime. Do not run the
withdrawn full verification suite or make a large benchmark matrix a refactor loop.

Success is fewer independent algorithms, fewer repeated validations and fewer
places making the same fallback decision, with unchanged observable semantics and
measured work reduction where performance is claimed. Public method count can
remain similar while the implementation becomes substantially simpler.
