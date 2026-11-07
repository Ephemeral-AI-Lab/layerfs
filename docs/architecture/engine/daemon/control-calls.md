# Control calls

[Architecture](../../README.md) › [Engine](../README.md) › [Daemon](README.md) › Control calls

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Mount, status, export and unmount.

| | |
| --- | --- |
| Component | [Daemon](README.md) |
| Source | [`engine/crates/layerfs-daemon/src/control`](../../../../engine/crates/layerfs-daemon/src/control) |

Every control call is one `Request` in one `Call` record; `Service::execute_control`
runs it once on the connection's worker thread.

| `Request` | Wire tag | Daemon action | Reply on success | Source |
| --- | --- | --- | --- | --- |
| `Mount { workspace, branch }` | 1 | Bind: overlay file, Store reader, registry entry | `Bound { token, binding }` | [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |
| `MountNamed { workspace, project, branch }` | 15 | `Mount` of the Branch found by its project's name and its own on the Workspace's reader, with its snapshot in one statement (#64; the SDK's mount) | `Bound`, or `SdkRefused` `NoSuchProject` / `NoSuchBranch` with the admission withdrawn | [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs), [`bind.rs`](../../../../engine/crates/layerfs-store-service/src/store/bind.rs) |
| `Commit(CommitControl)` | 2 | Capture, construct, merge or publish, install; after the answer, the released reader's reclamation on the same thread | `Committed`, `MergeCommitted` or `Conflict` | [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |
| `Status(token)` | 3 | Copy registry fields, one engine `state` job | `Status` | [`status.rs`](../../../../engine/crates/layerfs-daemon/src/control/status.rs) |
| `Unmount(UnmountControl)` | 4 | Dirty check, terminal teardown | `Unmounted` or `Dirty` | [`unmount.rs`](../../../../engine/crates/layerfs-daemon/src/control/unmount.rs) |
| `Fork`, `History`, `Branches` | 5, 6, 14 | Refused: Store calls | — | [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |
| `Hello` | 7 | Startup observation | `Hello(DaemonStatus)` | [Startup](startup.md#serve-and-readiness) |
| `EndSession` | 8 | End this channel; no Workspace closes | `SessionEnded` | [`connection.rs`](../../../../engine/crates/layerfs-daemon/src/application/connection.rs) |
| `Attach(token)` | 9 | Kernel mount over FUSE or NFS | `Ready(ReadyMount)` | [`attach.rs`](../../../../engine/crates/layerfs-daemon/src/control/attach.rs), [`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs) |
| `Locate(workspace)` | 10 | `Status` by incarnation alone | `Located` | [`status.rs`](../../../../engine/crates/layerfs-daemon/src/control/status.rs) |
| `ForceUnmount` | 11 | Abort, then teardown | `ForceUnmounted` | [Forced teardown](forced-teardown.md) |
| `Observed` | 12 / 21 | Wrap one call with diagnostics | inner reply, or `Observed` | [Startup](startup.md#connection) |
| `Cleanup(token)` | 13 | Observe overlay disposal | `Cleanup { state }` | [`cleanup.rs`](../../../../engine/crates/layerfs-daemon/src/control/cleanup.rs) |
| `Sdk(call)` | 20 | `backend.*` and `sandbox.list_workspaces` only | `Sdk` or `SdkRefused` | [Operator](operator.md) |

A refusal is `Reply::Refused(ControlRefusal)`; a stop after an effect is
`Reply::Retained(TeardownCustody)` ([`serve.rs`](../../../../engine/crates/layerfs-daemon/src/control/serve.rs)).

## Attach and detach

```text
Mount                                 Attach (FUSE, Linux)
 registry: absent? capacity? no        admit: Idle, Kernel::Unattached
   stopped maintenance?                  -> Attaching, Activity::Attaching
   insert Entry::Binding               root serial of the bound base
 Store::bind(owner, workspace, branch) prepare_serials (before any
   overlay: copy template ->             kernel request can create)
     <n>.sqlite, open_workspace        job: kernel_mount(root)
   open Store reader                   serving.register(mount)
 Entry::Bound(Binding { Idle,          KernelSession::attach: mount,
   kernel: Unattached, epoch 1 })        handshake, every loop entered
 reply Bound { token, binding }        Kernel::Ready, reply Ready
```

| Attach outcome | Entry afterwards | Source |
| --- | --- | --- |
| Ready | `Kernel::Ready`, `Idle` | [`attach.rs`](../../../../engine/crates/layerfs-daemon/src/control/attach.rs) |
| Failed before any owner remains | Engine mount revoked (`unwind`), `Kernel::Unattached`, `Idle` | [`attach.rs`](../../../../engine/crates/layerfs-daemon/src/control/attach.rs) |
| Engine mount outcome unknown | `Kernel::Attaching`, `Uncertain`; never a second attachment | [`attach.rs`](../../../../engine/crates/layerfs-daemon/src/control/attach.rs) |
| Connection or owner not disposable | `Kernel::Retained(Kept)`, `Uncertain`; reply `Retained` | [`attach.rs`](../../../../engine/crates/layerfs-daemon/src/control/attach.rs) |

`ReadyMount` carries the token, the mount directory (a new child of the
mount home, at most `MOUNT_DIRECTORY_LIMIT` bytes) and a `MountReceipt` of
negotiated facts ([`control_mount.rs`](../../../../engine/crates/layerfs-bridge/src/control_mount.rs)).
Detach is part of [Unmount](#unmount); the kernel side is in
[Kernel routes: FUSE](../kernel-routes/fuse.md).

## Status and inspect

| Call | Reads | Settles |
| --- | --- | --- |
| `Status(token)` | Registry copy (binding, `Activity`, epoch, last published outcome, `MountStatus` from maintained counters) and one engine `state()` job (`LocalObservation`) | Nothing; a failed engine read is reported in `local_failure` |
| `Locate(workspace)` | The registry finds the token, then `Status` | Nothing |
| `Cleanup(token)` | `observe_cleanup`: `Live`, `Held`, `Queued` or `Gone` from the overlay roster | Nothing; works after the registry entry is gone |
| `sandbox.list_workspaces` | `inspect`: every bound entry plus one `state` job each; history from the Store | Nothing |

`MountStatus` is built from maintained counters only: "no syscall, SQL job
or kernel request". Source: [`status.rs`](../../../../engine/crates/layerfs-daemon/src/control/status.rs),
[`kernel.rs`](../../../../engine/crates/layerfs-daemon/src/control/kernel.rs),
[`cleanup.rs`](../../../../engine/crates/layerfs-daemon/src/control/cleanup.rs),
[`inspect.rs`](../../../../engine/crates/layerfs-daemon/src/control/inspect.rs).

## Export

The NFS route (always on macOS; `route = nfs` on Linux) uses the same
registry, Workspace calls and Store steps as FUSE; only the kernel half
differs.

| Step | NFS route | Source |
| --- | --- | --- |
| Attach | Serve the engine mount as one loopback NFSv3 export on a fresh port; the host client mounts it with the mount helper; Ready only after the helper reports the mount. An NFS receipt has `abi_major == 0` and `mount_id` = the port | [`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs) |
| Commit | Flush first (flush helper, or `sync(2)`), close the export's descriptors, run the unchanged Commit; after an install of another root, close them again | [`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs), [`invalidate.rs`](../../../../engine/crates/layerfs-daemon/src/control/invalidate.rs) |
| Unmount | Helper unmount (busy is reversible), stop the export, revoke, Close | [`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs) |
| ForceUnmount | Force helper once, then the same teardown | [`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs) |
| Helper run | One attempt, at most `HELPER_WAIT` (60 s), then killed and reported | [`export_helper.rs`](../../../../engine/crates/layerfs-daemon/src/control/export_helper.rs) |

Kernel states `Exported`, `ExportLeaving` and `ExportKept` mirror `Ready`,
`Leaving` and `Retained`. Details: [Kernel routes: NFS](../kernel-routes/nfs.md).

## Unmount

```text
Unmount { token, discard }
  discard false and dirty? ------------------------> reply Dirty (no effect)
  admit: Idle; Kernel Ready -> Leaving(Probing)      Activity::Closing
  session.detach()
     Busy --------> Kernel::Ready again, Idle ------> refused Busy (reversible)
     error -------> Retained(Detach)
     Detached
  phase Draining; session.drain()
     stalled at Join -> end_stalled(): one abort write, drain again
     still stopped ---> Retained(Join | Abort | ...)
  job: kernel_revoke(mount) ---- fails -------------> Retained(Revoke)
  BoundWorkspace::close(): overlay close(), Store reader closed
     fails after detach ----------------------------> Retained(Close)
  depart: registry entry removed; retire_namespace:
     checked overlay close, file unlinked ---------> reply Unmounted
```

| Rule | Source |
| --- | --- |
| No daemon-side request count refuses admission: the kernel's reversible `Busy` is the fence, because every caller, descriptor, working directory and mapping holds a mount reference. | [`detach.rs`](../../../../engine/crates/layerfs-daemon/src/control/detach.rs) |
| Only a known detach is terminal; after it every stop keeps the exact owner. | [`detach.rs`](../../../../engine/crates/layerfs-daemon/src/control/detach.rs) |
| A connection that outlived its mount (another mount namespace holds a copy) is ended by the same one abort write a forced unmount makes (#426, #460). | [`detach.rs`](../../../../engine/crates/layerfs-daemon/src/control/detach.rs) |
| An Unattached Workspace's Unmount is the logical Close alone. | [`unmount.rs`](../../../../engine/crates/layerfs-daemon/src/control/unmount.rs) |
| The dirty check is an observation before admission; a write after it is closed with the Workspace. | [`dirty.rs`](../../../../engine/crates/layerfs-daemon/src/control/dirty.rs) |

## Registry

```text
Service.entries: Mutex<BTreeMap<WorkspaceId, Entry>>
  Entry::Binding                 Mount in progress (Busy to others)
  Entry::Bound(Binding)
    kernel:    Kernel  (Unattached | Attaching | Ready | Leaving |
                        Retained | Exported | ExportLeaving | ExportKept)
    workspace: Arc<BoundWorkspace>   snapshot: BranchSnapshot
    activity:  Idle | Committing | Closing | Attaching |
               Uncertain | LocalFailure
    epoch:     +1 on every transition      published: Option<CommitOutcome>
```

| Rule | Source |
| --- | --- |
| A token is `(WorkspaceId, namespace)`; a stale namespace is refused, never redirected. | [`registry.rs`](../../../../engine/crates/layerfs-daemon/src/control/registry.rs), [`control_types.rs`](../../../../engine/crates/layerfs-bridge/src/control_types.rs) |
| Admission requires `Idle`; `Uncertain` or `LocalFailure` answers `Unknown` with the retained publication. | [`registry.rs`](../../../../engine/crates/layerfs-daemon/src/control/registry.rs) |
| The registry is the sole authority for which connection an entry owns. | [`kernel.rs`](../../../../engine/crates/layerfs-daemon/src/control/kernel.rs) |
| A Commit whose captured reader or operation owner could not be released leaves `LocalFailure`; nothing later is admitted past it. | [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |
| A poisoned registry authorizes no further product operation. | [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |

## Related

- [Forced teardown](forced-teardown.md): `ForceUnmount`, dirty reports and failure codes.
- [Overlay: Lifetimes and reclamation](../overlay/lifetimes-and-reclamation.md): file disposal after `retire`.
- [Commit](../commit/README.md): what `Commit` runs.
- [Kernel routes: FUSE](../kernel-routes/fuse.md), [Kernel routes: NFS](../kernel-routes/nfs.md).
- [Bridge](bridge.md): request and reply records.
- [Glossary](../../glossary.md).

---

← [Daemon: Startup](startup.md) · [Up: Daemon](README.md) · [Daemon: Forced teardown](forced-teardown.md) →
