# 06 — Cluster-one library and runtime integration

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Written 2026-10-05 against product `f96d97651` and design `334fc7437`, with
> subsequent owner directions. Public libraries and the later explicitly scoped
> #307 object/Save/native-channel checkpoints exist; complete runtime adapters
> remain unfinished. This design ran no build, workload or measurement.

This is the authoritative integration map for the [operation documents](README.md#primary-design-documents).
The replacement does not restore `layerfs-server`, rename its coordinator or
require its old host construction/prepared-stream service. Application composition
embeds cluster-one libraries and supplies bounded runtime adapters to the sandbox.
Current providers and future distribution are distinguished explicitly.

## 1. The contract in brief

```text
 host application / SDK composition             sandbox daemon
 ----------------------------------             --------------
 cluster-one runtime                              ordinary Bash -> FUSE
   owns selected provider handles                         |
   Storage / Reader / Save                                v
   HistoryCatalog                                    Workspace + overlay
          ^                                               |
          | bounded authenticated adapters                +-> content reads
          +-----------------------------------------------+-> construction
                   immutable objects + typed history

 legacy layerfs-server package: excluded reference / retired target
                         no new dependency or revival
```

The SDK/host application supplies the runtime rather than a new coordinator crate.
The current persistence provider opens only on macOS, so the Linux daemon cannot
open that global SQLite Store. A cross-process boundary still needs request
handlers/framing/authentication; eliminating a named server crate does not remove
this physical boundary. The proposed default composition/handler placement is
`layerfs-api/sdk/src/runtime/`, using bridge-owned framing/authentication and
the current public cluster-one libraries; see the
[file ownership plan](07-implementation-validation.md#44-file-ownership).
Public runtime constructor and Save lifetimes remain S0 integration work, not
existing endpoints. These runtime adapters are included explicitly in the
[planning LOC estimate](07-implementation-validation.md#43-estimated-future-size),
even though no server crate is restored.

Bootstrap the runtime/provider and daemon database once. Each tool call then binds
a complete committed root, runs ordinary Bash, explicitly publishes changed state
and terminally unmounts. Task-level reuse also works. No ignored/dependency/cache
filter or Exec-time materialization is introduced.

## 2. Cluster-one APIs cluster two calls

### 2.1 Runtime side: implemented libraries

| Operation | Existing API | Obligation |
| --- | --- | --- |
| Provider open | `Handles::open_writable(config, binding, cursor_key)` | Host-local SQLite/macOS only today; selected profile and authority validated |
| Storage setup | `Storage::new(handles.storage.clone())`, `policy()`, `capacities()` | Reuse initialized policy/provider; no per-tool-call Store create/open |
| Saved object reads | `Storage::reader()` / `Reader::read_objects(ids)` | Authenticated canonical bytes in demand order; 4,096 IDs / 32 MiB demand window |
| Begin/accept/finish | `Storage::begin_save()` / `Save::accept(object)` / `Save::finish()` | Synchronous backpressure; earlier published waves can survive later failure |
| Pending object reads | `AuthenticatedObjects for Save` | Same-Save visibility, not an ordinary committed Reader |
| Original storage error | `Save::take_failure()` | Preserve error beneath content OutputRejected; respect consuming finish lifetime |
| Open base | `HistoryCatalog::branch_snapshot(branch)` | Coherent root/head/base/scope/profile; root serial read separately |
| Serial range | `reserve_inodes(request)` | Authority-owned reservation, never recycled |
| Stage | `stage_changes(StageRequest)` | Exact captured expectations and saved candidate; API alone does not verify candidate savedness |
| Publish | `commit_staged(CommitStagedRequest)` | Exact token; conditional head transition; Committed or UpToDate |
| Discard | `discard_stage(DiscardRequest)` | Exact owned token with known disposition |
| Inspect history | `stage(workspace)` / `commit(id)` | Reads alone do not fence an in-flight unknown request |

Sources: [provider open](../../../crates/layerfs-persistence/src/open.rs),
[Storage](../../../crates/layerfs-storage/src/storage.rs),
[Save lifecycle](../../../crates/layerfs-storage/src/save/operation.rs),
[history contract](../../../crates/layerfs-history/src/catalog.rs),
[history requests/results](../../../crates/layerfs-history/src/records.rs).

### 2.2 Daemon side: implemented content libraries

| API | Use / current limitation |
| --- | --- |
| `AuthenticatedObjects` | Adapter-backed immutable acquisition; interface alone has no Send/Sync requirement |
| `FilesystemRead` | Child/inode resolution, bounded listing, portable metadata and symlink reads |
| `FileView`, `read_range` | Logical file reads; caller clamps EOF, authenticated base roots retained |
| `construct_stream` | New/full replacement files; bounded prefix then progressive output |
| `apply_edits`, `EditSequence`, `EditSource` | Stable normalized edits; compare/construct replacement passes; deferred-node refusal still present |
| `update_filesystem`, `PreparedRows`, `OrderingBacking` | Ordered replayable namespace inputs; resident directory Vec/new-parent map remain |
| `FinalizedConsumer` | Synchronous finalized-object output to runtime adapter |
| `FilesystemRootId` | Tuple struct: candidate ObjectId is `result.root.0` |

Sources: [content exports](../../../crates/layerfs-content/src/lib.rs),
[file edit](../../../crates/layerfs-content/src/file/edit/input.rs),
[filesystem update](../../../crates/layerfs-content/src/filesystem/update.rs),
[root type](../../../crates/layerfs-content/src/filesystem/root.rs).
The [cluster-one handbook](../../../../cluster_one_handbook.md) and
[CAS/CDC handbook](../../../../cas_cdc_deltaencoding_handbook.md) govern source semantics.

## 3. Bridge operations

These are **conceptual adapter contracts**, not callable product URLs or deployed
endpoints. The #307 native-channel primitive authenticates peers through pinned
KK and binds the SDK; [implemented scope](../../architecture/23-native-bridge.md)
does not establish the logical routes below. The
[old bridge source](../../../crates/layerfs-bridge-legacy/src/adapters/native/connection.rs)
and old history wire are preserved excluded reference code.

| Proposed adapter operation | Result / runtime binding |
| --- | --- |
| `GetBranch` | Captured BranchSnapshot plus checked root serial/binding |
| `GetPolicy` | Persisted construction policy and capacities contract |
| `ReadObjects(ids, save?)` | Canonical bytes in demand order, through Reader or owned Save |
| `Attributes(items)` | Batched logical length/mode/mtime; length-only optimization still P5 |
| `ReserveInodes` | Reserved range; local consumption plus fair refill |
| `SaveBegin` | Authorized logical Save capability |
| `SaveAccept(session, batch)` | Semantic admission then library accept; bounded acknowledgement |
| `SaveFinish(session)` | Library finish result or exact typed unknown/failure |
| `SaveAbort(session)` | Terminate owned operation; no rollback/deletion of prior acknowledged waves |
| `StageChanges` / `CommitStaged` / `DiscardStage` | Typed catalog requests and exact token/outcomes |
| `GetStage` / `GetCommit` | Authorized reads; resolver requires completion fence and policy |

Control flows in the other direction: mount/open, ordinary Exec, explicit Commit,
status and terminal unmount. No separate public close is needed. Exec has no automatic
runtime deadline; an individual RPC deadline must not silently terminate Bash.
The old SaveFile/PreparedChanges/host construction and 256 MiB prepared-stream path
is retired, not the new Commit route.

A transport window limits one frame/batch, not total file/Workspace/Commit size.
Fragment arbitrary operations through bounded bytes and acknowledgements; never
compute or buffer a whole upload first. Session capabilities are independent of
connection lifetime. Preserve demand-read/control capacity and cancel/fence queued
calls before interpreting a disconnect. Exact binary framing/correlation is work
still to implement, not inferred from a list of operation names.

## 4. The cluster-one runtime

One initialized provider owner may serialize bounded persistence jobs without
reviving the retired server. Keep per-Workspace service shares for object reads,
Save batches, finish and history; strict reads-first starvation is forbidden.
An accept may perform a publication wave with authentication/encoding/I/O, so
byte/job limits are not wall-latency guarantees. No lock spans a whole Commit.

`Save<'_>` borrows Storage and mutable indexes; a sound lifetime arrangement for
interleaved session capabilities must be demonstrated against pinned APIs. No
unsafe lifetime extension, dependency patch or silent whole-Commit serialization.

Treat sandbox input as untrusted. Recompute domain-separated identity, decode
role grammar, derive references and validate closure/provenance. `FinalizedObject::new`
only validates a generic envelope and supplied role, not the complete proposed
admission contract. Bind peer, Workspace incarnation, Branch/scope, Store/profile
and Save capability. Enforce count/byte/session bounds before buffering.

Implementation checkpoint #307 after `6a0dbe003`: public
[`FinalizedObject::admit`](../../../crates/layerfs-content/src/object/output.rs)
now supplies domain identity, local role grammar, expected root scope and
role-derived direct references without a payload copy. Runtime adapters must use
this boundary for untrusted output. It does not establish peer/capability authority,
saved closure or contextual child/topology validation; P1 remains incomplete.

The following SDK checkpoint embeds initialized provider/Storage owners and a
serving-scope Save registry at the planned path. Local authority-bound object,
policy, begin/accept/finish/abort/demand and retained-completion APIs are built and
tested through the real host provider; see
[implemented scope](../../architecture/22-sdk-runtime.md). Authenticated native
transport, fair queued service, history adapters, disconnect/restart fences and
complete root/import qualification remain required. A typed local binding is not
a network authentication proof, and S9 remains incomplete.

## 5. Immutable distribution and caller obligations

```text
 CONTENT PLANE                          HISTORY / AUTHORITY PLANE
 immutable canonical IDs               mutable Branch head / stage / serial allocator
      | replication / exact reuse            | conditional atomic transition
      v                                      v
 authenticate bytes + role/refs         compare expected state + exact token
 reference closure before root use     retain outcome if acknowledgement lost
```

Immutability prevents in-place rewriting of another version and permits natural
replication/cache reuse. It does not by itself supply backend availability,
durability, authorization, reference closure, safe garbage collection or atomic
history updates. Current [PersistenceConfig](../../../crates/layerfs-persistence/src/config.rs)
selects host-local SQLite; Postgres is unavailable, and there is no implemented
distributed provider/endpoint. A distributed provider must satisfy PackPersistence
and HistoryCatalog semantics, publish roots only after dependency closure, and
supply declared consistency/leases/outcomes before being claimed supported.

Caller obligations: stable capture and replay; Store-derived policy; no chronological
FUSE log as edits; synchronous output; finish before stage; exact token publication;
no automatic retry/resend/guessed cleanup. UpToDate acknowledges no filesystem change
without a new Commit record. An audit event for every tool invocation is a separate
feature, not assumed history behavior.

## 6. Prerequisites outside cluster two

| ID | Required correction / proof | Current source consequence |
| --- | --- | --- |
| P1 | Semantic admission and runtime authority adapters | Hash-only acceptance is insufficient; endpoints absent |
| P2 | Locked Linux content and bundled overlay build | Not compiled/qualified in this design |
| P3 | Bounded-backed deferred editing without artificial refusal | EDIT_DEFERRED_LIMIT can refuse fragmented files |
| P4 | Hole-aware canonical/read/edit/stream semantics | Sparse Commit currently processes zeros O(logical length) |
| P5 | Cheap file length lookup | Attributes can read complete small file root on runtime side |
| P6 | Stream ordered changes inside a directory | DirectoryUpdate.changes is a resident Vec |
| P7 | Bounded-backed new-parent membership | Resident map grows or refuses via ordering_bytes |
| P8 | Shared-engine payload/capture/orphan/failure algorithms | Replacement bounds not derived/proved by SQLite alone |
| P9 | Qualification hosting policy and prospective specifications | No new measurement is admissible merely from this design |
| P10 | Exact unknown-history resolver policy plus completion fence | Uncertain remains terminal; absence is not a completion proof |
| P11 | Root accessor documentation | `.0` correction made; no new product API needed |
| P12 | Faithful bounded initial import, all files/symlinks, no inherited file cap | Native Init refuses symlinks, retains scan state and limits files to 4 GiB |
| P13 | Backed validation, touched/zero/release state with bounded resident windows | Total row/name/demand checks derive from ordering_bytes/16; demanded/addition/parent collections and touched/zero/release collections remain resident; touched length is checked after collection |
| P14 | Incremental checked parent/reverse-binding evidence preserving alias/cycle checks | Rebinding a stored non-file can trigger a whole-base namespace walk capped by ordering_bytes/1024; streaming that walk alone does not make tiny rename Commit incremental |

Assign engineering ownership; these are not permission to accept artificial caps.
Current source links: [edit bound](../../../crates/layerfs-content/src/file/edit/tree.rs),
[directory changes](../../../crates/layerfs-content/src/filesystem/input.rs),
[new parents](../../../crates/layerfs-content/src/filesystem/update.rs),
[initial scan](../../../crates/layerfs-project/src/scan.rs).

P13/P14 were traced in the 2026-10-05
[per-call investigation](fuse-investigation/03-per-call-lifecycle.md). See
[validation](../../../crates/layerfs-content/src/filesystem/validate.rs),
[touched reduction](../../../crates/layerfs-content/src/filesystem/references/reduce.rs)
and [descendant release](../../../crates/layerfs-content/src/filesystem/references/release.rs).
They extend R3's required work; no product behavior or canonical format changed.
Do not construct a reverse-binding index through a hidden whole-root scan at
each mount or bypass topology validation to remove its cost.

## 7. Failure boundaries

Shared provider quarantine (`backend/sqlite/transaction.rs`) can affect all runtime
users after an unknown operation. Shared daemon overlay failure can affect all local
Workspaces. Separate logical namespaces do not provide independent physical failure.
Preserve phase distinctions: unknown pre-stage Save is not an unknown Branch change;
unknown stage/transition/discard retains custody. Terminal unmount never deletes
shared immutable objects/history. No collector exists today; adding one needs root,
open-orphan and in-flight dependency leases. See [Commit](workspace-api/commit.md)
and [unmount](workspace-api/unmount.md).

## 8. Platform and acceptance

Current persistence opens only on macOS; FUSE/Exec are Linux. ARM64 bridge builds
retain repository AEAD flags; unsupported capabilities fail explicitly. Legacy
server and cluster-two crates remain excluded reference source at the pin: target
retirement is an owner decision, not a claim that tracked files vanished.

Qualification must prove both the authentic full-root per-tool-call path and
per-task/persistent multi-call Workspaces with incremental Commits. Commands can
be short or long-lived in either mode. Include all changed ignored/cache/dependency
data, same-mount coherence, fair concurrent Workspaces and the declared persistence
profile. [Validation](07-implementation-validation.md) preserves historical failures,
source/cache identities, one sample per arm and independent proof budgets.
