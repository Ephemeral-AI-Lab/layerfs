# Control calls

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

What each control request does inside the daemon, within [Daemon](README.md): the registry, Mount and Attach, status, the NFS export, Unmount and forced teardown, and how failures are reported.

| | |
| --- | --- |
| Source | [`engine/crates/layerfs-daemon/src/control`](../../../../engine/crates/layerfs-daemon/src/control) |

The records themselves (tags, replies, value shapes) are defined once in
[Bridge](../bridge/README.md). `Service::execute_control` runs each request
once on its connection's worker thread; a refusal is
`Reply::Refused(ControlRefusal)` and a stop after an effect is
`Reply::Retained(TeardownCustody)`
([`serve.rs`](../../../../engine/crates/layerfs-daemon/src/control/serve.rs)).

| `Request` | Daemon action | Source |
| --- | --- | --- |
| `Mount`, `MountNamed` | Bind: overlay file, Store reader, registry entry. `MountNamed` finds the Branch by project and Branch name on the new Workspace's own reader; a miss withdraws the admission | [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs), [`bind.rs`](../../../../engine/crates/layerfs-store-service/src/store/bind.rs) |
| `Attach` | Kernel mount over FUSE or NFS | [`attach.rs`](../../../../engine/crates/layerfs-daemon/src/control/attach.rs), [`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs) |
| `Commit` | Capture, construct, merge or publish, install ([Commit](../commit/README.md)); after the answer, the released reader is reclaimed on the same thread | [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |
| `Status`, `Locate`, `Cleanup` | Observe; settle nothing | [`status.rs`](../../../../engine/crates/layerfs-daemon/src/control/status.rs), [`cleanup.rs`](../../../../engine/crates/layerfs-daemon/src/control/cleanup.rs) |
| `Unmount` | Dirty check, then terminal teardown | [`unmount.rs`](../../../../engine/crates/layerfs-daemon/src/control/unmount.rs) |
| `ForceUnmount` | One abort, then the same teardown | [`force.rs`](../../../../engine/crates/layerfs-daemon/src/control/force.rs) |
| `Fork`, `History`, `Branches` | Refused `Invalid`: Store calls run in the caller's process | [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |

## Registry

```text
Service.entries: Mutex<BTreeMap<WorkspaceId, Entry>>
  Entry::Binding                 a Mount in progress (Busy to others)
  Entry::Bound(Binding)
    kernel:    Unattached | Attaching | Ready | Leaving | Retained
               | Exported | ExportLeaving | ExportKept      (NFS)
    workspace: Arc<BoundWorkspace>    snapshot: BranchSnapshot
    activity:  Idle | Committing | Closing | Attaching
               | Uncertain | LocalFailure
    epoch:     +1 on every transition   published: Option<CommitOutcome>
```

| Rule | Source |
| --- | --- |
| A token is `(WorkspaceId, namespace)`; a stale namespace is refused, never redirected | [`registry.rs`](../../../../engine/crates/layerfs-daemon/src/control/registry.rs) |
| Admission requires `Idle` and marks the activity inside the lock; `Uncertain` or `LocalFailure` answers `Unknown` with the retained publication | [`registry.rs`](../../../../engine/crates/layerfs-daemon/src/control/registry.rs) |
| Mount checks, inside the lock, that no entry exists, capacity remains and no maintenance failure is retained, then inserts `Entry::Binding` | [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |
| The registry is the sole authority for which kernel connection an entry owns | [`kernel.rs`](../../../../engine/crates/layerfs-daemon/src/control/kernel.rs) |
| A Commit whose captured reader or operation owner could not be released leaves `LocalFailure`; nothing later is admitted past it | [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |
| A poisoned registry authorizes no further product operation | [`failure.rs`](../../../../engine/crates/layerfs-daemon/src/control/failure.rs) |

## Mount and Attach

```text
Mount                                  Attach (FUSE, Linux)
 registry: absent? capacity?            admit: Idle + Unattached
   no stopped maintenance?                -> Attaching
   insert Entry::Binding                root serial of the bound base
 Store::bind(owner, workspace, branch)  prepare_serials (before any
   copy template -> <n>.sqlite,           kernel request can create)
   open Store reader                    job: kernel_mount(root)
 Entry::Bound { Idle, Unattached,       serving.register(mount)
   epoch 1 }                            KernelSession::attach: mount,
 reply Bound { token, binding }           handshake, every loop entered
                                        Kernel::Ready, reply Ready
```

| Attach outcome | Entry afterwards | Source |
| --- | --- | --- |
| Ready | `Kernel::Ready`, `Idle` | [`attach.rs`](../../../../engine/crates/layerfs-daemon/src/control/attach.rs) |
| Failed before any owner remains | Engine mount revoked, `Unattached`, `Idle` | same |
| Engine mount outcome unknown | `Attaching`, `Uncertain`; never a second attachment | same |
| Connection or owner not disposable | `Retained`, `Uncertain`; reply `Retained` | same |

`ReadyMount` carries the token, the mount directory (a new child of the
mount home) and the negotiated `MountReceipt`. The kernel side is in
[Kernel routes](../kernel-routes/README.md).

## Status

| Call | Reads | Settles |
| --- | --- | --- |
| `Status(token)` | Registry copy (binding, activity, epoch, last publication, `MountStatus` from maintained counters: no syscall, SQL job or kernel request) and one engine `state()` job | Nothing; a failed engine read goes in `local_failure` |
| `Locate(workspace)` | Finds the token, then `Status` | Nothing |
| `Cleanup(token)` | `observe_cleanup`: `Live`, `Held`, `Queued` or `Gone` from the overlay roster, also after the registry entry is gone | Nothing |

Source: [`status.rs`](../../../../engine/crates/layerfs-daemon/src/control/status.rs),
[`kernel.rs`](../../../../engine/crates/layerfs-daemon/src/control/kernel.rs),
[`cleanup.rs`](../../../../engine/crates/layerfs-daemon/src/control/cleanup.rs).

## Export

The NFS route (always on macOS; `route = nfs` on Linux) shares the
registry, Workspace calls and Store steps with FUSE; only the kernel half
differs. `Exported`, `ExportLeaving` and `ExportKept` mirror `Ready`,
`Leaving` and `Retained`.

| Step | NFS route | Source |
| --- | --- | --- |
| Attach | Serve the engine mount as one loopback NFSv3 export on a fresh port; the client mounts it with the mount helper; Ready only after the helper reports the mount. The receipt has `abi_major == 0` and `mount_id` = the port | [`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs) |
| Commit | Flush first (flush helper, or the export volume's own flush: `sync_volume_np(<dir>, SYNC_VOLUME_WAIT)` on macOS, `sync(2)` elsewhere), close the export's descriptors, run the unchanged Commit; after an install of another root, close them again | [`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs), [`invalidate.rs`](../../../../engine/crates/layerfs-daemon/src/control/invalidate.rs) |
| Unmount | Helper unmount (busy is reversible), stop the export, revoke, Close | [`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs) |
| ForceUnmount | Force helper once, then the same teardown | same |
| Helper run | One attempt, at most `HELPER_WAIT` 60 s, then killed and reported | [`export_helper.rs`](../../../../engine/crates/layerfs-daemon/src/control/export_helper.rs) |

## Unmount

```text
Unmount { token, discard }
  discard false and dirty? -----------------------> reply Dirty (no effect)
  admit: Idle; Ready -> Leaving(Probing)          activity Closing
  session.detach()
     Busy ------> Ready again, Idle --------------> refused Busy (reversible)
     error -----> Retained(Detach)
     Detached
  Draining; session.drain()
     stalled at Join -> one abort write, drain again
     still stopped ---> Retained(Join | Abort | ...)
  job: kernel_revoke(mount) --- fails ------------> Retained(Revoke)
  BoundWorkspace::close(): overlay, Store reader
     fails after detach --------------------------> Retained(Close)
  depart: entry removed; retire_namespace:
     checked overlay close, file unlinked --------> reply Unmounted
```

| Rule | Source |
| --- | --- |
| No daemon-side request count refuses admission: the kernel's reversible `Busy` is the fence, because every caller, descriptor, working directory and mapping holds a mount reference | [`detach.rs`](../../../../engine/crates/layerfs-daemon/src/control/detach.rs) |
| Only a known detach is terminal; after it every stop keeps the exact owner | same |
| A connection that outlived its mount (another mount namespace holds a copy) is ended by the same one abort write a forced unmount makes | same |
| An Unattached Workspace's Unmount is the logical Close alone | [`unmount.rs`](../../../../engine/crates/layerfs-daemon/src/control/unmount.rs) |

### Dirty check

```text
state job: dirty_inodes, dirty_directory_entries, captured, base_root
clean if all zero, no capture, base_root == binding root, no pending merge
else stream active entries and inodes (64-row windows):
  changed name -> parent path + name;  changed file -> its path
  base directory whose mode moved -> its path
  base paths: bounded base walk (WALK 4096, DEPTH 256)
reply Dirty(DirtyReport { paths <= 256, inodes, entries,
                          pending_merge, truncated })
```

The check is an observation before admission; a write after it is closed
with the Workspace. No-op rows (an unbound local inode, a base directory
with an unchanged mode) neither refuse nor truncate. Force skips it. Source:
[`dirty.rs`](../../../../engine/crates/layerfs-daemon/src/control/dirty.rs).

## Force

`ForceUnmount { token, relinquish_unknown }` closes one attached Workspace
without waiting for the processes using its mount. It signals no process;
each step is made once.

```text
under the registry lock (a refusal changes nothing):
  1 ExportKept / Retained -> Retained   (a normal unmount stalled at
                                         Join is admitted instead)
  2 Committing | Attaching | Closing   -> Busy     force:admission
  3 no serving connection              -> Invalid / Unknown / Busy
  4 Commit knowledge: Idle -> Absent; LocalFailure + published ->
    Published; Uncertain + relinquish_unknown -> Unknown;
    else refused Unknown                            force:custody
  5 abort control not bound            -> Failed   force:capability
admit: Leaving(Stopping), activity Closing
session.abort()       one 1-byte write to the connection's abort
  refused before write -> Ready again;  Short/Failed -> Retained(Abort)
session.force_drain() local drain + its one plain detach
  stopped -> Retained(stage)
kernel_revoke -> Retained(Revoke);  closed() -> Retained(Close)
observe_cleanup once; depart; retire_namespace
reply ForceUnmounted { token, ForcedOutcome { facts, work, cleanup } }
```

| `ForcedFacts` field | Values | Meaning |
| --- | --- | --- |
| `abort` | `Written`, `Short`, `Failed` | The one abort write |
| `detach` | `NotAttempted`, `Detached`, `Busy`, `Failed` | The one plain detach; `Busy` means aborted and still mounted |
| `commit` | `Absent`, `Published(outcome)`, `Unknown` | Commit custody copied at admission, never settled later |
| `fenced` | count | Terminal replies the drain made |

| `ForcedCleanup` | Meaning | Source |
| --- | --- | --- |
| `Held` | Closed, but a kernel mount, file, reader, lookup, capture, base reader, reply ticket or owner still holds it | [`close.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/close.rs) |
| `Queued` | Closed and ready for checked connection close | same |
| `Gone` | Connection closed and file deleted by the roster | [`workspaces.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/workspaces.rs) |
| `Unobserved` | The one observation failed, or reported `Live` after a Close | [`force.rs`](../../../../engine/crates/layerfs-daemon/src/control/force.rs) |

| Rule | Source |
| --- | --- |
| The abort write uses the connection's own control, bound from the daemon's `fusectl` tree at Attach; without it Force is refused before any effect | [`force.rs`](../../../../engine/crates/layerfs-daemon/src/control/force.rs), [`application/filesystem.rs`](../../../../engine/crates/layerfs-daemon/src/application/filesystem.rs) |
| A normal unmount stalled at `Join` with its abort unused is forced from its retained session; its plain detach counts as the one detach | [`force.rs`](../../../../engine/crates/layerfs-daemon/src/control/force.rs), [`serving.rs`](../../../../engine/crates/layerfs-daemon/src/control/serving.rs) |
| `relinquish_unknown` permits local disposal of stopped unknown Commit custody; the receipt keeps it `Unknown` | [`force.rs`](../../../../engine/crates/layerfs-daemon/src/control/force.rs) |
| On the NFS route the force helper is the abort | [`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs) |
| Outside Linux without NFS the guard runs and the call is refused (`kernel serving requires Linux`) | [`force.rs`](../../../../engine/crates/layerfs-daemon/src/control/force.rs) |

## Failure

| `Failure` | Wire form | Source |
| --- | --- | --- |
| `Rejected(code, detail)` | `ControlRefusal`, phase `admission` | [`failure.rs`](../../../../engine/crates/layerfs-daemon/src/control/failure.rs) |
| `Kernel(KernelFailure)` | Its code and stable phase (`attach:mount`, `unmount:kernel`, `force:capability`, ...), detail at most 2 048 bytes | [`failure.rs`](../../../../engine/crates/layerfs-daemon/src/control/failure.rs), [`types.rs`](../../../../engine/crates/layerfs-daemon/src/control/types.rs) |
| `Retained(TeardownCustody)` | `Reply::Retained`: first `TeardownStage` not established, `detached`, counters, forced facts | [`serve.rs`](../../../../engine/crates/layerfs-daemon/src/control/serve.rs) |
| `History`, `Commit`, `Bind`, `Owner`, `Workspace` | `Busy`, `HeadMoved` (with `MovedState`), `Missing`, `Capacity`, `Invalid`, `Failed` or `Unknown`; a known publication travels in `published` | [`failure.rs`](../../../../engine/crates/layerfs-daemon/src/control/failure.rs) |
| `Poisoned` | `Unknown`, phase `registry` | same |
| `After { cause, original }` | The original success is kept beside the later failure | [`types.rs`](../../../../engine/crates/layerfs-daemon/src/control/types.rs) |

| `TeardownStage` | First boundary not established |
| --- | --- |
| `Detach` 1 | Kernel detach was neither success nor the reversible `Busy` |
| `Join` 2 | Not every receive loop is known joined |
| `Owner` 3 | The connection owner or its exit watcher did not return |
| `Requests` 4 | Request ownership remains |
| `Lane` 5 | The mount's request owner was not released |
| `Revoke` 6 | Engine mount revocation refused or unknown |
| `Close` 7 | Logical Close refused or unknown |
| `Registry` 8 | The registry could not record the transition |
| `Abort` 9 | The forced abort write was short or failed |

A failure whose code is `Unknown` (and not `Retained`) is `uncertain()`:
its connection ends after the reply and the daemon keeps a retained
failure record ([Daemon](README.md)). The detail text is never parsed to
infer the category.

## Related

- [Daemon](README.md): connections, phases and the operator methods.
- [Bridge](../bridge/README.md): every request and reply record.
- [Commit](../commit/README.md): what `Commit` runs.
- [Overlay](../overlay/README.md) and [Overlay lifetimes](../overlay/lifetimes.md): what `Held`, `Queued` and `Gone` observe.
- [Kernel routes](../kernel-routes/README.md), [FUSE](../kernel-routes/fuse.md), [NFS](../kernel-routes/nfs.md): attach, detach, drain and abort.
- [Control: Sandboxes](../../control/sandboxes/README.md): the caller of these requests.
