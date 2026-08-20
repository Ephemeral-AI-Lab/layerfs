# Mem9/Drive9 Layered-Filesystem Research — Distilled for LayerFS

Status: future architecture note only. Read this document after Phase 4 has a
terminal disposition and final read-only audit. It does not modify, extend, or
reinterpret the active Phase 4 program; authorize a new storage carrier; select
a workspace or VFS implementation; or claim production readiness.

Prepared: 2026-08-20 from Drive9 `main` at
`ffe6663c97e0fc1c8ac2b1dafe03a54d32aee77e`, the primary sibling sources below,
and read-only comparison with the current `layerfs-empty` architecture and
evidence.

Scope: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` on
`codex/empty-worktree`. Never modify the sibling `layerfs` repository. Preserve
all Phase 4 artifacts and the terminal accepted implementation. Do not commit
unless the user explicitly asks.

Primary sources:

- [Drive9 layered filesystem research](https://github.com/mem9-ai/drive9/blob/ffe6663c97e0fc1c8ac2b1dafe03a54d32aee77e/docs/design/layered-filesystem-research.md)
- [Drive9 Layer FS V1 design](https://github.com/mem9-ai/drive9/blob/ffe6663c97e0fc1c8ac2b1dafe03a54d32aee77e/docs/design/layered-filesystem-v1-design.md)
- [Drive9 LayerFS feature matrix](https://github.com/mem9-ai/drive9/blob/ffe6663c97e0fc1c8ac2b1dafe03a54d32aee77e/docs/design/layered-filesystem-feature-matrix.md)
- [Drive9 CoW fork design](https://github.com/mem9-ai/drive9/blob/ffe6663c97e0fc1c8ac2b1dafe03a54d32aee77e/docs/design/layered-filesystem-cow-fork-design.md)
- [AgentFS overlay guide](https://docs.turso.tech/agentfs/guides/overlay)
- [AgentFS specification](https://github.com/tursodatabase/agentfs/blob/main/SPEC.md)
- [Cloudflare ArtifactFS](https://github.com/cloudflare/artifact-fs)
- [Linux OverlayFS](https://docs.kernel.org/filesystems/overlayfs.html)
- [DeltaBox / DeltaFS](https://arxiv.org/html/2605.22781)
- [YoloFS](https://arxiv.org/html/2604.13536)
- [OSTree repository model](https://ostreedev.github.io/ostree/repo/)
- [casync](https://github.com/systemd/casync)
- [restic repository references](https://restic.readthedocs.io/en/stable/100_references.html)

## 1. Entry gate

Before using this note:

1. Read the final Phase 4 report, accepted source, raw evidence, independent
   audit, manifest verification, and terminal decision.
2. Record the accepted engine format, SQLite profile, publication contract,
   full-create and changed-spine baselines, CPU/RSS/Q/storage observations, and
   every explicitly unavailable physical measurement.
3. Preserve all `FAIL`, `REVISE`, `revert`, and rejected-carrier results. Do not
   reopen them because an external system uses a superficially similar design.
4. Confirm whether the next real product caller requires parallel workspaces,
   live FUSE staging, command checkpoints, or only the existing
   materialize/edit/capture workflow.
5. Refresh the external sources. Their implementation status may have changed
   after 2026-08-20.

If Phase 4 still has an active diagnostic, candidate, audit, custody question,
or unresolved accepted implementation, stop. Finish Phase 4 first.

## 2. Executive conclusion

Drive9 and `layerfs-empty` solve different halves of an agent filesystem:

```text
Drive9
  durable, queryable session operation log over a live mutable base

layerfs-empty
  immutable content-addressed filesystem roots and canonical deltas
```

Borrow Drive9's **workspace control plane**, not its authoritative state model.

The target composition is:

```text
ephemeral native workspace / shadow files
  -> one writable stage over one immutable base root
  -> CDC changed ranges
  -> immutable CAS objects and changed tree spine
  -> new root + canonical delta + audit event
  -> atomic compare-and-swap of a named workspace ref
```

Filesystem reads use the resolved immutable root. Audit and agent explanation
use an append-only event stream. Never replay the event stream to reconstruct
ordinary reads.

## 3. Drive9 distilled

The four Drive9 documents define three state planes:

```text
local runtime overlay
  local shadow, pending writes, local-only dependencies/build output

durable session overlay
  fs_layers, append-only fs_layer_entries, events, checkpoints

published main
  file_nodes, inodes, contents, semantic state
```

The later CoW design forks a layer by recording `parent_layer_id` and the
parent's current `MAX(entry_seq)`. No entries or object bodies are copied at
fork. A child reads its own operations, pinned ancestor prefixes, and then live
main. Commit folds the effective log, diffs it against live main, and applies
the result to main.

Useful properties:

- O(1) fork metadata;
- explicit `active`, `sealed`, `committing`, `committed`, `conflicted`, and
  `abandoned` lifecycle;
- whiteouts and ordered rename/chmod/upsert semantics;
- checkpoint, diff, status, rollback, actor, tag, and event surfaces;
- local fast state separated from durable session state;
- base revision conflict checks;
- child pins prevent premature ancestor GC;
- no-layer behavior remains on its existing path.

Costs and limitations:

- a fork freezes the parent overlay, not main;
- exact-path reads currently materialize and replay the visible chain log;
- modifying an inherited regular file copies the complete file into the upper;
- layer objects are not content-address deduplicated;
- repeated operations retain payload-bearing append-only entries;
- physical GC, retention, quota, and orphan cleanup remain incomplete;
- commit is preflight plus ordered apply plus best-effort rollback, not one
  atomic filesystem transaction;
- deep chains require depth limits, caching, and eventual compaction;
- the feature matrix can drift: its older baseline still labels fork as a gap
  even though current code contains the CoW fork implementation.

## 4. Sibling techniques and the applicable lesson

### 4.1 Linux OverlayFS

Borrow these live-namespace primitives:

- `whiteout`: hide one lower name;
- `opaque directory`: hide the complete matching lower directory without one
  tombstone per descendant;
- metadata-only copy-up (`metacopy`): change mode/metadata while continuing to
  reference lower data;
- origin identity: remember which lower object a copied-up entry came from;
- generation/cache invalidation rules after layer changes;
- conservative directory rename behavior when atomic semantics are unavailable.

Do not make kernel xattrs, host inode numbers, native file handles, mount
options, or redirect paths canonical LayerFS identity inputs.

### 4.2 AgentFS

Borrow:

- a portable session/export concept;
- queryable filesystem changes;
- tool-call and actor attribution;
- namespace/data separation;
- fixed and explicit consistency rules;
- origin mapping for copied-up objects.

Do not make one SQLite session file the authoritative server format. AgentFS
stores file data in fixed-size blocks and uses whole-file overlay copy-up; it
does not replace LayerFS CDC/CAS identity and immutable-root semantics.

### 4.3 ArtifactFS and Drive9 Git workspace

Borrow:

- expose tree metadata before downloading every content blob;
- hydrate content on demand by verified object identity;
- key rebuildable caches by immutable base generation/root;
- deduplicate concurrent hydration waiters;
- publish a base generation only after verifying the requested source identity;
- keep clean-source caches rebuildable and outside durable dirty state;
- keep specialized Git semantics outside the generic LayerFS format.

LayerFS can do this without Git-specific reconciliation: `stat` and `readdir`
read tree/mapping objects, while file reads fetch only the required CAS chunks.

### 4.4 DeltaFS

Borrow the checkpoint generation rule, not its kernel stack:

```text
fd opened at generation N
checkpoint publishes root RN and advances workspace to N+1
later write through old fd detects N != N+1
old fd re-resolves and writes into a new N+1 shadow
```

This prevents open handles from modifying a frozen checkpoint. It belongs in
`layerfs-vfs` and the local workspace state. DeltaFS's reflink-backed copy-up
also supports optional APFS `clonefile` or Linux reflink for local shadow files,
but this is a projection optimization, not canonical storage.

### 4.5 YoloFS

Borrow its strongest separation:

```text
resolved current override state
  !=
append-only command/snapshot/travel history
```

YoloFS records command-boundary snapshots, lets agents inspect each command's
changes, and appends travel markers instead of deleting abandoned history.
LayerFS can implement this more directly with immutable roots:

```text
before command: R10
after command:  R11 + delta D10_11
travel to R10:  CAS workspace ref R11 -> R10; append travel event
```

Progressive per-path policies (`ask`, `allow`, `write-ask`, `read-only`,
`deny`) are useful later. Enforce irreversible read/execute decisions before
the syscall succeeds; rollback cannot undo a secret read.

### 4.6 OSTree

OSTree confirms the resolved content-addressed root model and the value of
separating directory topology from directory metadata. The current LayerFS
directory metadata/page/index/wrapper mapping already follows this direction.
Retain it.

### 4.7 casync and restic

Borrow only physical-storage invariants unless post-Phase-4 evidence authorizes
a carrier:

1. canonical object identity is independent of physical placement;
2. published packs are immutable;
3. indexes are reconstructible from packs;
4. write objects/packs before their index;
5. publish the root/ref only after its complete closure exists;
6. remove an old index reference before deleting an old pack;
7. GC starts from reachable roots and retained refs.

Do not start a pack/carrier implementation merely because CDC backup systems
use one. The retained LayerFS carrier evidence and the final Phase 4 bottleneck
attribution control that decision.

### 4.8 Runtime snapshots and shared volumes

E2B, Modal, Cloudflare sandbox backups, CRIU, VM snapshots, and persistent
volumes are execution acceleration or shared-data mechanisms. They may carry a
LayerFS root/workspace ID, but they must not become the authoritative
file-version model.

## 5. Adopt, adapt, reject

| Technique | Decision | LayerFS form |
|---|---|---|
| Explicit workspace lifecycle | Adopt | Named workspace ref and typed state |
| Diff/status/checkpoint/rollback UX | Adopt | Root/delta/ref operations |
| Local-only dependency/build policy | Adopt | Explicit rebuildable path policy in VFS |
| Local shadow before durable capture | Adopt | Native workspace implementation detail |
| Whiteout | Adopt for staging | Removed node in published root |
| Opaque directory | Adopt for staging | Removed subtree in published root |
| Metadata-only copy-up | Adopt | New metadata node reusing content root ID |
| Origin/base identity | Adopt | Expected base `NodeId`/content root ID |
| Append-only audit events | Adopt | Separate from the read model |
| Command-boundary checkpoint/travel | Adapt later | Immutable root refs and events |
| Lazy hydration | Adopt when materialization starts | CAS cache keyed by `ObjectId` |
| Fork by parent operation sequence | Reject | Fork by immutable root ID |
| Live-main fallback after fork | Reject | Frozen complete base root |
| Full log replay on ordinary reads | Reject | Traverse resolved root DAG |
| Whole-file durable copy-up | Reject | CDC chunks plus changed mapping spine |
| Payload bytes in path event log | Reject | Reference CAS content/shadow locator |
| Deep layer stacks plus compaction | Reject | Each checkpoint publishes a resolved root |
| Best-effort apply to mutable base | Reject | Atomic root/ref publication |
| Kernel/host identity in canonical format | Reject | Keep in OS/VFS adapter only |
| New pack carrier without evidence | Reject for now | Reconsider only after Phase 4 gate |

## 6. Minimum post-Phase-4 architecture

Do not build a general branch, merge, policy, audit, search, and FUSE framework
at once. The first useful slice has three additions.

### 6.1 Named immutable refs

Generalize the single `visible_root` into named refs only when a real parallel
workspace caller is ready:

```text
workspace_id
base_root_id
head_root_id
generation
state
actor_id
```

Update a ref with expected `head_root_id`, expected `generation`, and expected
active state. The immutable parent/root IDs replace Drive9 inode revisions and
sequence pins.

### 6.2 Transactional root events

Publish these together:

```text
new immutable objects
canonical delta
new root
workspace ref update
committed event: before_root, after_root, delta_id, actor/tool-call
```

An event cannot claim a committed transition before the root/ref transaction
commits. Failed/rejected attempt evidence is separate and must not masquerade as
filesystem history.

### 6.3 One writable stage over one immutable base

The initial stage vocabulary is:

```text
upsert
whiteout
opaque_dir
metadata
rename
```

An entry references an expected base node and either an immutable content root
or local shadow locator. It does not embed complete canonical file history.

At checkpoint/capture:

1. freeze change evidence;
2. validate path and base identities;
3. CDC only the proven changed neighborhood where possible;
4. store new chunks and mapping/tree spine;
5. build the new immutable root and delta;
6. atomically update the named ref and event;
7. advance the workspace generation;
8. re-resolve pre-checkpoint open handles on their next write.

## 7. Crate mapping

### `layerfs-core`

Retain ownership of:

- canonical identities and bytes;
- CDC and authenticated rejoin;
- immutable CAS semantics;
- immutable file/directory trees;
- copy-on-write mutation;
- canonical deltas and before identities.

Do not add workspace states, FUSE handles, policy prompts, audit actors, or
physical pack locators to canonical core identity.

### `layerfs-engine`

After Phase 4 closure, the smallest new responsibilities are:

- named root/workspace refs;
- compare-and-swap ref generation;
- transactional ref + event publication;
- checkpoint/root retention metadata;
- later reachability enumeration for GC.

Do not replace the accepted storage layout until a separate measured gate
authorizes it.

### `layerfs-os`

Own native directory mechanics, file identity observations, atomic replacement,
sync, and optional APFS clonefile/Linux reflink qualification. Native IDs remain
noncanonical.

### `layerfs-vfs`

Own materialize/capture, local shadows, one-upper merged view, whiteout/opaque
semantics, lazy hydration, open-handle generations, path policy, and cache
invalidation.

### `layerfs-sdk`

Keep the first public workflow small:

```text
open
materialize
capture
discard
```

Add workspace fork/checkpoint/travel/diff only when a real agent or DeltaGit
caller requires them. Internal engine capabilities need not all become public.

## 8. Correctness gates before adoption

Any post-Phase-4 workspace design must prove:

1. Forking a root is O(1) metadata and freezes the complete base view.
2. Ordinary reads do not grow with workspace history or checkpoint count.
3. A metadata-only change writes no new file-content chunks.
4. A bounded large-file edit preserves unchanged chunk and mapping identities.
5. Directory rename reuses the subtree identity and handles destination
   conflicts atomically.
6. Whiteout and opaque-directory behavior is exact for stat, read, and readdir.
7. A checkpoint with open handles cannot receive later writes through an old
   generation backing.
8. Objects, delta, root, ref, and committed event publish atomically.
9. A failed capture leaves the old ref authoritative and produces no reachable
   partial root.
10. Concurrent ref updates produce one winner and a typed conflict; no retry
    silently changes the intended base.
11. Rebuildable local-only state never enters a durable root unless explicitly
    requested.
12. Lazy hydration verifies the requested object identity before cache publish.
13. Retained refs/checkpoints protect their complete object closure from GC.
14. No-layer or non-workspace behavior pays no workspace query cost.

## 9. Measurement gates

Before claiming the combined design is space- or performance-efficient, run at
least:

- fresh 100-MiB capture: canonical, apparent, allocated, and temporary bytes;
- one bounded middle edit: CDC bytes, chunks reused/created, mapping/tree spine,
  apparent and allocated growth;
- 50-edit development loop and 500-edit checkpoint loop;
- many identical files and related versions to prove deduplication;
- metadata-only changes to prove no content writes;
- 10,000-file tree materialize, warm no-op, and three-path incremental update;
- root fork count scaling with zero data copies;
- read/stat/readdir latency as checkpoint history grows;
- crash at object, delta, root, ref, and event publication boundaries;
- reachability/retention accounting including abandoned and traveled-from roots.

Do not treat the retained benchmark campaign directory size as product store
amplification. Evidence custody intentionally keeps many complete control and
candidate database copies and must be reported separately.

## 10. Explicit non-goals for the first post-Phase-4 slice

- no deep layer chain;
- no merge/rebase framework;
- no commit into another active workspace;
- no general POSIX implementation;
- no kernel filesystem or custom ioctl;
- no progressive-permission UI before a sandbox caller exists;
- no semantic/vector search before root-aware basic search exists;
- no portable bundle before root/ref/object closure is stable;
- no PostgreSQL or remote multi-host engine without a deployment requirement;
- no new carrier, pack index, compactor, or GC worker mixed into the accepted
  Phase 4 implementation without a separate specification and evidence gate.

## 11. Recommended order after Phase 4

```text
seal Phase 4 and freeze the accepted engine
  -> confirm the first real materialize/capture caller
  -> implement native-directory materialize/capture end to end
  -> generalize visible_root to named generation-CAS refs if parallel workspaces
     are required
  -> publish canonical delta + ref event atomically
  -> add one writable stage over one immutable base root
  -> add whiteout, opaque_dir, metadata reuse, and subtree rename
  -> add open-handle generation checks and checkpoint --wait
  -> run repeated-edit storage and history-scaling benchmarks
  -> add reachability GC before automatic command checkpoints
  -> add diff/travel/audit UX
  -> add lazy hydration, root-aware search, policy, or portable bundles only in
     response to measured callers
```

Do not skip directly from the storage engine to a broad agent-filesystem API.
The first proof remains:

```text
open -> materialize root -> modify native directory -> capture atomically
     -> materialize new root -> exact byte/tree equality
```

## 12. Final decision rule

Adopt a sibling technique only when it preserves all of these:

- immutable LayerFS roots remain the authoritative filesystem state;
- CDC/CAS and canonical identities remain independent of physical storage;
- ordinary reads use a resolved root rather than replaying history;
- workspace publication is one atomic root/ref transition;
- local performance state remains rebuildable;
- the added mechanism has a real caller and a runnable correctness/performance
  gate.

Otherwise, do not build it. The reusable synthesis is:

> Borrow Drive9's workflow, OverlayFS's live-namespace semantics, ArtifactFS's
> lazy hydration, DeltaFS's handle generations, YoloFS's resolved-head/history
> split and command travel, AgentFS's auditability, OSTree's resolved roots,
> and casync/restic's physical invariants—while keeping LayerFS's immutable
> CDC/CAS root as the sole authoritative filesystem state.
