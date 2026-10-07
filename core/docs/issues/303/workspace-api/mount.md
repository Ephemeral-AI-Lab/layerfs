# workspace_api.mount — full-tree execution readiness

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Owner revisions: 2026-10-05. Product baseline:
> `f96d97651be5299f153ccde2bc8d921dd58807ad`; supersedes the mount direction
> of design `334fc743751b9a181e670d0601a24fb3169208f9` where stated below.
> No implementation, bootstrap measurement or workload qualification is claimed.

This document owns the observable mount operation. Read the
[daemon/SQLite engine](../daemon-sqlite.md) for local ownership and scheduling,
[FUSE contract](../fuse.md) for attachment/coherence, and
[cluster-one integration](../06-cluster-one-integration.md) for runtime adapters.
The [cluster-one handbook](../../../../../cluster_one_handbook.md) governs the
underlying library APIs and their completion semantics.

Owner supersession 2026-10-08 at R0 input `1a6bb53ef`: the SDK exposes
ProjectApi, WorkspaceApi and SandboxApi. Ordinary Sandbox/runtime or an external
executor owns command launch, standard streams, exit status and explicit
cancellation; the filesystem daemon owns no command supervisor, launcher,
per-Exec cgroup, command registration or custom Exec wire. FUSE serves every
permitted visible process. Shell exit/zero registered commands proves no
filesystem drain, and forced filesystem teardown never implicitly kills caller
processes. Current [S8 specification](../../307/S8-SPECIFICATION-20261008.md) and
[R0–R9 rollout](../../307/ROLLOUT-LEDGER-20261008.md) govern prospective work;
historical baseline pins, receipts and verdicts retain their original scope.

## 1. Purpose and granularity

[owner requirement]

`workspace_api.mount` creates a logical mutable Workspace over an authorized
committed filesystem root and exposes that complete filesystem for execution.
One tool call is the minimum supported orchestration granularity. A Workspace
can also serve many sequential or concurrent calls over a long lifetime and
Commit incrementally. Its lifetime is independent of any one invocation or task;
the complete filesystem cannot depend on the caller's chosen granularity.

Per-tool-call is the expected common mode; per-task is also required. An Exec
can be short or long-lived in either mode. Mount does not impose a duration,
single-command limit or automatic teardown based on the mode.

```text
                  caller-owned tool-call pipeline

  previous history head H0 / full root R0
                     |
                     v
             mount(project, branch, base)
                     |
                     v
             exec(ordinary Bash command) -------- no hidden preparation
                     |
                     v
             commit(captured delta) ------------ exact result required
                     |
                     v
             unmount(terminal cleanup)
                     |
            known new head H1 / full root R1
                     |
            next tool call mounts R1

  Persistent W: keep the mount across calls A/B, Commit C1, calls C/D, Commit C2.
  Only explicit terminal unmount ends W; Exec/Commit do not remount it.
  No-change Commit: UpToDate retains the existing head; it is not an audit event.
```

The caller orchestrates this sequence. Mount, [Exec](exec.md),
[Commit](commit.md), and [terminal unmount](unmount.md) remain explicit operations.
Mount does not import the original repository, install dependencies or publish
history. A no-change tool call need not create a new immutable Commit; a separate
invocation audit is outside the current contract.

A new mount starts from its selected committed root. Further calls using the
same Workspace see the current live union of base and overlay, including changes
not yet committed. Successful incremental install advances the base while
preserving later active changes, stable identities and valid same-mount caches.

## 2. Full namespace is an execution prerequisite

[owner requirement; required integration work]

The selected root includes `.git`, tracked and ignored files, dependencies such
as `node_modules`, caches, build outputs, symlinks and supported hard-link aliases.
`.gitignore` and package-manager rules are ordinary file content, not LayerFS
exclusion policies. Cache/output directories stay inside the Workspace and their
mutations participate in Commit. There is no source-only execution projection.

Initial acquisition must preserve the complete supported namespace before a root
is offered for tool-call use. It is separate from repeated mount. Unsupported
required file kinds or portable metadata need an explicit format/integration
decision; they cannot be silently omitted and later reconstructed by Exec.

```text
   ONE-TIME / EXPLICIT ACQUISITION               REPEATED TOOL CALL

   complete prepared repository                 full immutable root R
     .git / source / dependencies                        |
     symlinks / caches / output                          | bind identity
             |                                           v
       faithful acquisition                       empty logical overlay
       Save + genesis/history                     + FUSE projection of R
             |                                           |
             +------ full immutable root R --------------+

   NOT repeated at mount: scan, copy all bytes, filter, reinstall, warm payload.
```

Immediate readiness means every preserved path can be used through ordinary
filesystem access without reconstruction or dependency restoration. Metadata and
bytes can be demand-loaded after attachment. Cold access still pays authentication,
object-tree navigation, transport and storage reads; immediate readiness does not
mean zero I/O or universal residency.

The historical 2026-10-05 Init baseline refused symlinks/large files and used
resident acquisition collections. Current [backed acquisition](../../../architecture/43-backed-initial-acquisition.md)
and [complete-root direct proof](../../307/PRE-S8-F11-20261007.md) supersede those
source limitations at their exact scope. Native full-fixture acquisition and
mounted full-root proofs remain required; no historical receipt is relabelled.

## 3. Inputs, outputs and readiness

[proposed design; final Rust/wire types remain integration work]

| Item | Contract |
| --- | --- |
| Route | Authorized daemon/sandbox instance and its incarnation |
| Base selection | Project, Branch and optional explicit Commit selection; select an exact coherent filesystem root |
| New identity | Fresh Workspace ID/incarnation and daemon-local namespace key |
| Profile | Compatible filesystem/construction policy and command identity; validated, not inferred from caller paths |
| Success | Workspace identity, mounted directory, exact selected base binding and readiness |
| Readiness | Registry routing, logical overlay ownership and FUSE service exist; ordinary operations may enter |
| Definite failure | No usable Workspace; return the actual phase and any retained cleanup custody |
| Unknown reply | Attachment may exist; retain exact identity/context, do not blindly create a replacement |

The base binding carries Branch, selected head/Commit, base Layer, effective root,
allocation scope/profile and root serial. `BranchSnapshot` does not itself contain
the root serial; the runtime adapter must obtain it from the checked filesystem
root. Selection of an explicit older Commit must preserve its construction base
and define its publication target without silently treating it as the latest head.

## 4. Workflow and owners

[proposed design]

One local overlay SQLite database is initialized before daemon readiness. Mount
uses this existing engine. SQL connection topology is an engine decision;
mount must neither create a per-Workspace database nor imply simultaneous SQLite
writers. Every daemon opens the shared global Store directly in-process. Host Init/seal/
install is separate and control-only afterward; no host object/history service.

```text
 Caller / SDK        Daemon registry       Overlay engine       Cluster-one runtime / FUSE
 ------------        ---------------       --------------       --------------------------
 mount(base) ------> authorize route
                         |
                         +------------------------------------> coherent base selection
                         | <----------------------------------- binding + policy
                         |
                         +-------------> admit namespace job
                         |               create small state
                         | <------------- exact local result
                         |
                         +------------------------------------> attach FUSE at mount path
                         |                                      start runnable dispatch
                         | <----------------------------------- attachment readiness
                         |
                         v
                    publish Ready identity
 mount result <----- identity + path + base
       |
       +------------> exec: no scan/install/restore between these operations

 Uncovered lookup/read ---> local content APIs + authorized object adapter
 Cached immutable object -> authenticate/use within cache/resource contract
```

The implementation must define the publication point so another control call
cannot use a partially attached Workspace. Registry locks cover lookup/reference
transfer only, not base acquisition, kernel attachment or SQL-owner queue waits.
No database transaction spans those external operations.

Mount-local state is bounded binding/generation/lifecycle state. It does not
contain a resident entry for every base inode. Allocator ranges are reserved through
history and refilled independently; unavailable reservation must not trigger a
repository-wide scan or change inode identity.

## 5. Partial startup, cancellation and custody

[proposed design; exact ownership required]

```text
 request admitted
       |
       +-- base selection fails --------> report failure; no new namespace
       |
 namespace allocated
       |
       +-- attachment fails ------------> fence namespace; retain native owner if uncertain
       |                                  reclaim unreachable rows in bounded jobs
       |
 FUSE attached + Ready recorded
       |
       +-- result delivered ------------> caller owns exact mount identity
       |
       +-- reply lost ------------------> identity retained; outcome unknown to caller
                                          no automatic resend / second attachment
```

Cancellation must fence queued/in-flight work before discarding its ownership.
A failed system call or lost response does not prove the native mount is absent.
Retain the Workspace/native owner and expose its known lifecycle disposition
through [status](status.md). Exact reconciliation needs completion fencing, not
an unfenced absence check. Cleanup never deletes shared immutable objects on a
guess. If startup has already attached a mount, report retained custody until
detach is established.

## 6. Large-load cost and concurrency

[proposed resource contract; not a performance result]

Let N be base namespace entries and B its regular-file bytes. Required mount
work must not contain an O(N) enumeration, O(B) copy, dependency install or fixed
resident dirty frontier. A small binding/root acquisition can still depend on
queue wait, provider service and canonical root format. Kernel mount and process
startup costs remain real and separately attributable.

| Phase | Work charged to the per-tool-call pipeline |
| --- | --- |
| Route/base binding | Authority checks, coherent selection and necessary root/policy reads |
| Logical bootstrap | Namespace/generation rows and small in-memory ownership |
| FUSE attachment | Kernel/session readiness and dispatch creation |
| First useful Exec access | Cold/warm object lookup and actual demanded payload; not moved into setup |
| Commit | Actual affected metadata/payload and publication; see [Commit](commit.md) |
| Unmount | Activity/native fences and logical cleanup; reclaim debt reported separately |

Concurrent mounts, ongoing mutations and concurrent Commits enter fair bounded
engine/runtime jobs. One large Workspace must not monopolize bootstrap through
cleanup, capture scans or a Save-held connection. The design must retain demand
capacity and account aggregate queues, mount bookkeeping and unreclaimed rows.
No inherited file-count, Workspace-byte, edit-count, accumulated-flow or runtime
limit may substitute for bounded windows and actual resource admission.

Immutable objects allow unchanged content/tree identities to be reused across
tool calls and copied/replicated without in-place payload mutation. This helps
distribution but does not establish a distributed backend, availability, authority,
reference closure, crash durability, garbage-collection leases or atomic history
publication. Those remain explicit runtime/provider responsibilities in [06](../06-cluster-one-integration.md).

## 7. Workloads and future proofs

[proposed validation; IDs are documentation cases, not frozen benchmark selections]

| Case | Workload | Required observation |
| --- | --- | --- |
| M1 | Full prepared development repository, then immediate `git status`/build | No omitted ignored/dependency/cache/output path; no Exec preparation; demand-load costs counted |
| M2 | Many tiny existing files and a wide directory | No mount-time inode/name materialization or base walk; listing remains paged |
| M3 | Large files and scattered old edits already committed | Bind roots without payload copy; immediate range reads correct |
| M4 | Repeated tool-call mount → Exec → Commit → unmount | Each next mount sees prior exact history result; no per-call schema creation/import |
| M5 | Simultaneous mounts while other Workspaces write/Commit/reclaim | Fair bootstrap progress and namespace isolation; demand capacity retained |
| M6 | Partial FUSE startup, lost reply and explicit cancellation | Exact retained owner/incarnation; no duplicate attachment or guessed cleanup |
| M7 | Many completed Workspaces awaiting physical cleanup | Bounded logical bootstrap; namespace keys not reused; debt eventually reclaimed |

The historical full shape is 103,108 files + 16,867 directories + 10,070 symlinks
= 130,045 entries and 3,475,776,149 regular-file bytes. Copy/hard-link dependency
replay is 95,021 entries and 2,126,509,110 bytes. Source HEAD
`639ed015397290b3745d163aafe02ffee4aa3f84`, manifest
`98fd26440b9087bcfd5c23bec9e5497434a2dcb9a27fc85ad0b823ba9c516658`, retained on
experiment branch `codex/phase7-experiment-305` at
`1451b68a720bbe2175a103dd9b35693ad05e2be1` in
`core/docs/issues/305/PREPARATION-REPORT.md`. These are historical manifest
observations, not new measurements or evidence that this mount is qualified.

First prove deterministic readiness/full-tree/custody behavior. Future timing must
follow [validation](../07-implementation-validation.md) and root benchmark rules:
one sample per case/arm, declared equal cache states and cold residency where
required, independent prepared copies, separate verifier and append-only receipts.
Do not pre-read the paths Exec will measure. Report complete pipeline costs and
every nonpassing/unrun case; no benchmark or original-repository command was run
to write this document.

## 8. Current API versus target

[source-verified baseline; proposed replacement]

The original 2026-10-05 dormant SDK/daemon/FUSE baseline used WorkspaceOpen,
a selected slot, legacy cluster-one bindings and polling mount custody. Those
observations retain their historical source pin and do not identify current
active file paths. Current [native control](../../../architecture/68-native-workspace-control.md)
implements authenticated bounded Store/engine binding and returns Bound.
Current [sealed install](../../../architecture/67-native-store-install.md) provides
one-time provisioning; S8/R2 adds real kernel attachment/Ready, exact native
custody and independently proved read/service/permissions/drain. Partial native
owners remain retained until exact disposition. Existing public Control is not
a real WorkspaceApi facade; R1 supplies the actual organization/backend.

Load-bearing acceptance requires the complete pipeline and applicable integration
corrections, not merely logical row binding. [Validation](../07-implementation-validation.md)
and the current R0–R9 rollout retain exact evidence and remaining scope.
