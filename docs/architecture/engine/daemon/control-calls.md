# Control calls

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

What each control request does inside the daemon, within [Daemon](README.md): the registry, Mount and Attach, status, the NFS export, Unmount and forced teardown, and how failures are reported.

| | |
| --- | --- |
| Source | [`engine/crates/layerfs-daemon/src/workspace`](../../../../engine/crates/layerfs-daemon/src/workspace) |

The records themselves (tags, replies, value shapes) are defined once in
[Bridge](../bridge/README.md). `Service::execute_control` runs each request
once on its connection's worker thread; a refusal is
`Reply::Refused(ControlRefusal)` and a stop after an effect is
`Reply::Retained(TeardownCustody)`
([`serve.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/route/serve.rs)).

| `Request` | Daemon action | Source |
| --- | --- | --- |
| `Mount`, `MountNamed` | Bind: overlay file, Store reader, registry entry. `MountNamed` finds the Branch by project and Branch name on the new Workspace's own reader; a miss withdraws the admission | [`dispatch.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/route/dispatch.rs), [`run.rs`](../../../../engine/crates/layerfs-store-service/src/workspace/bind/run.rs) |
| `Attach` | Kernel mount over FUSE or NFS | [`attach.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/fuse/attach.rs), [`export.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/nfs/export.rs) |
| `Commit` | Capture, construct, merge or publish, install ([Commit](../commit/README.md)); after the answer, the released reader is reclaimed on the same thread | [`dispatch.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/route/dispatch.rs) |
| `Status`, `Locate`, `Cleanup` | Observe; settle nothing | [`get.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/status/get.rs), [`cleanup.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/status/cleanup.rs) |
| `Unmount` | Dirty check, then terminal teardown | [`normal.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/teardown/normal.rs) |
| `ForceUnmount` | One abort, then the same teardown | [`forced.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/teardown/forced.rs) |
| `Fork`, `History`, `Branches` | Refused `Invalid`: Store calls run in the caller's process | [`dispatch.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/route/dispatch.rs) |

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
| A token is `(WorkspaceId, namespace)`; a stale namespace is refused, never redirected | [`registry.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/registry.rs) |
| Admission requires `Idle` and marks the activity inside the lock; `Uncertain` or `LocalFailure` answers `Unknown` with the retained publication | [`registry.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/registry.rs) |
| A Commit request on an `Uncertain` or `LocalFailure` Workspace that kept a resolution record first runs its one resolution ([Commit](../commit/README.md#one-resolution-of-retained-custody)): resolved, the entry is `Idle` and admission follows; unresolved, it answers `Unknown` with the reason | [`resolve.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/route/resolve.rs) |
| Mount checks, inside the lock, that no entry exists, capacity remains and no maintenance failure is retained, then inserts `Entry::Binding` | [`dispatch.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/route/dispatch.rs) |
| The registry is the sole authority for which kernel connection an entry owns | [`kernel.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/kernel.rs) |
| A Commit whose captured reader or operation owner could not be released leaves `LocalFailure`; nothing later is admitted past it | [`dispatch.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/route/dispatch.rs) |
| A poisoned registry authorizes no further product operation | [`failure.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/failure.rs) |

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
| Ready | `Kernel::Ready`, `Idle` | [`attach.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/fuse/attach.rs) |
| Failed before any owner remains | Engine mount revoked, `Unattached`, `Idle` | same |
| Engine mount outcome unknown | `Attaching`, `Uncertain`; never a second attachment | same |
| Connection or owner not disposable | `Retained`, `Uncertain`; reply `Retained` | same |

`ReadyMount` carries the token, the mount directory (a new child of the
mount home) and the negotiated `MountReceipt`. The kernel side is in
[Kernel routes](../kernel-routes/README.md).

## Status

| Call | Reads | Settles |
| --- | --- | --- |
| `Status(token)` | Registry copy (binding, activity, epoch, last publication, `MountStatus` from maintained counters: no syscall, SQL job or kernel request) and one engine `observe_state()` job, which drains no queued maintenance (#679) | Nothing; a failed engine read goes in `local_failure` |
| `Locate(workspace)` | Finds the token, then `Status` | Nothing |
| `Cleanup(token)` | `observe_cleanup`: `Live`, `Held`, `Queued` or `Gone` from the overlay roster, also after the registry entry is gone | Nothing |

Source: [`get.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/status/get.rs),
[`kernel.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/kernel.rs),
[`cleanup.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/status/cleanup.rs).

## Export

The NFS route (always on macOS; `route = nfs` on Linux) shares the
registry, Workspace calls and Store steps with FUSE; only the kernel half
differs. `Exported`, `ExportLeaving` and `ExportKept` mirror `Ready`,
`Leaving` and `Retained`.

| Step | NFS route | Source |
| --- | --- | --- |
| Attach | Serve the engine mount as one loopback NFSv3 export on a fresh port; the client mounts it with the mount helper; Ready only after the helper reports the mount, and the host's mount table is read once to record whether the mount is listed. The receipt has `abi_major == 0` and `mount_id` = the port | [`export.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/nfs/export.rs) |
| Commit | Flush first (flush helper, or the export volume's own flush: `sync_volume_np(<dir>, SYNC_VOLUME_WAIT)` on macOS, `sync(2)` elsewhere; a failed flush refuses the Commit as `Failed` at `commit:flush`), close the export's descriptors, run the unchanged Commit; after an install of another root, close them again | [`export.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/nfs/export.rs), [`invalidate.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/fuse/invalidate.rs) |
| Unmount | Helper unmount (busy is reversible), stop the export, revoke, Close. A failed or timed-out helper is settled by the mount table: a mount listed at Attach and gone now completes teardown once the helper has ended, while a still-listed one, an unreadable table or a helper still running keeps `Retained(Detach)` | [`export.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/nfs/export.rs) |
| ForceUnmount | Force helper once, then the same teardown; also finishes an export kept at `Detach` whose client mount stayed | same |
| Helper run | One attempt, at most `HELPER_WAIT` 60 s, then killed and reported; a child still unreaped past `REAP_WAIT` 5 s enters the retained queue and is polled by its one background reaper | [`helper.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/nfs/helper.rs), [`process/retained.rs`](../../../../engine/crates/layerfs-os/src/process/retained.rs) |

## Unmount

```text
Unmount { token, discard }
  Retained(Detach) of a force refused EBUSY ------> detach retry (below)
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
  BoundWorkspace::close(): overlay, Store reader, mount intent
     fails after detach --------------------------> Retained(Close)
  depart: entry removed; retire_namespace:
     checked overlay close, file unlinked --------> reply Unmounted
```

| Rule | Source |
| --- | --- |
| No daemon-side request count refuses admission: the kernel's reversible `Busy` is the fence, because every caller, descriptor, working directory and mapping holds a mount reference | [`kernel_detach.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/teardown/kernel_detach.rs) |
| Only a known detach is terminal; after it every stop keeps the exact owner | same |
| A connection that outlived its mount (another mount namespace holds a copy) is ended by the same one abort write a forced unmount makes | same |
| A forced teardown kept at `Detach` with `abort: Written` and `detach: Busy` (`Kept::retryable`) is asked for its detach again, before the dirty check (that force already dropped the changes): a fresh local drain and one plain detach, never a second abort. Refused again, the same custody stays with this attempt's facts; detached, it goes through Revoke and Close and replies `Unmounted` (#538) | [`detach_retry.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/teardown/detach_retry.rs), [`dispatcher.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/fuse/dispatcher.rs) |
| An Unattached Workspace's Unmount is the logical Close alone | [`normal.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/teardown/normal.rs) |
| Once the overlay owner has closed, the mount intent is released (or marked uncertain) even when the Store reader's close then fails, and the reader's failure is the answer: no later unmount passes the closed owner again, so nothing else would release the row (#776 G5) | [`bind/operation.rs`](../../../../engine/crates/layerfs-store-service/src/workspace/bind/operation.rs) |

### Dirty check

```text
state job: dirty_inodes, dirty_directory_entries, captured,
           consolidating, base_root
clean if all zero, no capture, nothing consolidating,
         base_root == binding root, no pending merge
else stream active entries and inodes (64-row windows):
  changed name -> parent path + name;  changed file -> its path
  base directory whose mode moved -> its path
  base paths: bounded base walk (WALK 4096, DEPTH 256)
reply Dirty(DirtyReport { paths <= 256, inodes, entries,
                          pending_merge, truncated })
```

The check is an observation before admission; a write after it is closed
with the Workspace. No-op rows (an unbound local inode, a base directory
with an unchanged mode) neither refuse nor truncate. A failed Commit's
capture still folding back (`consolidating`) is a change: it refuses and sets
`truncated`, since its paths are not in the active windows. The whole Unmount
runs under a maintenance hold, and a refused one drains the queue after it
(#679, [`normal.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/teardown/normal.rs)).
Force skips it. Source:
[`dirty.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/status/dirty.rs).

## Force

`ForceUnmount { token, relinquish_unknown }` closes one attached Workspace
without waiting for the processes using its mount. It signals no process;
each step is made once.

```text
Retained(Detach) of a force refused EBUSY: taken out (Leaving(Stopping),
  activity Closing); Undrained::retry_detach(): local drain, one plain
  detach; refused -> the same Retained(Detach); detached -> Revoke, Close
  and ForceUnmounted as below (#538)
under the registry lock (a refusal changes nothing):
  1 ExportKept / Retained -> Retained   (a normal unmount stalled at
                                         Join, or a forceable export
                                         kept at Detach, is admitted)
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
| `Held` | Closed, but a kernel mount, file, reader, lookup, capture, base reader, reply ticket or owner still holds it | [`close.rs`](../../../../engine/crates/layerfs-overlay/src/backing/close.rs) |
| `Queued` | Closed and ready for checked connection close | same |
| `Gone` | Connection closed and file deleted by the roster | [`workspaces.rs`](../../../../engine/crates/layerfs-store-service/src/workspace/owner/roster/workspaces.rs) |
| `Unobserved` | The one observation failed, or reported `Live` after a Close | [`forced.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/teardown/forced.rs) |

| Rule | Source |
| --- | --- |
| The abort write uses the connection's own control, bound from the daemon's `fusectl` tree at Attach; without it Force is refused before any effect | [`forced.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/teardown/forced.rs), [`startup/serving.rs`](../../../../engine/crates/layerfs-daemon/src/application/startup/serving.rs) |
| A normal unmount stalled at `Join` with its abort unused is forced from its retained session; its plain detach counts as the one detach | [`forced.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/teardown/forced.rs), [`dispatcher.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/fuse/dispatcher.rs) |
| An export kept at `Detach` whose export and receipt are still held and whose forced facts are unset is forced by the one forced client unmount (`umount -f` on macOS, `umount -f -l` on Linux); a still-listed mount keeps `Retained(Detach)` with forced facts. On Linux that helper detaches lazily, so its failure never counts as a detach even when the table no longer lists the mount | [`forced.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/teardown/forced.rs), [`custody.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/nfs/custody.rs) `ExportKept::forceable` |
| Normal NFS `Unmount` keeps `Retained(Detach)` when Attach never saw the mount listed (`Exported::tracked` false), when the table still lists it or cannot be read (`detached_export`), when the killed helper's exit was not observed (`Ran::ended`: a plain Linux `umount` leaves the table before the superblock's shutdown flushes through the export), or when the helper detaches lazily (`-l`/`--lazy` in its own words: the mount leaves the table before the kernel gives its superblock back, so the table proves nothing); the mount point itself is never resolved (`listed` compares the configured path and the name below canonical mount home) | [`export.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/nfs/export.rs), [`helper.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/nfs/helper.rs) |
| A helper past `KILL` and `REAP_WAIT` enters the retained queue, whose one background thread reaps and records its exit (shared with the macOS driver's `umount -f`). Its bound is one thread and 64 recorded exits, not time: a child whose exit never comes keeps that thread polling for as long as the custody lasts, because a deadline would drop the only handle that can reap it. For tests only, `ExportServingConfig` adds `mount_table`, `reap_wait` and `helper_wait`, and `ExportServing::reaped_helpers` reads observed exits | [`export.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/nfs/export.rs), [`process/retained.rs`](../../../../engine/crates/layerfs-os/src/process/retained.rs) |
| `relinquish_unknown` permits local disposal of stopped unknown Commit custody; the receipt keeps it `Unknown` | [`forced.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/teardown/forced.rs) |
| On the NFS route the force helper is the abort | [`export.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/nfs/export.rs) |
| Outside Linux without NFS the guard runs and the call is refused (`kernel serving requires Linux`) | [`forced.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/teardown/forced.rs) |

## Failure

| `Failure` | Wire form | Source |
| --- | --- | --- |
| `Rejected(code, detail)` | `ControlRefusal`, phase `admission` | [`failure.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/failure.rs) |
| `Kernel(KernelFailure)` | Its code and stable phase (`attach:mount`, `unmount:kernel`, `force:capability`, ...), detail at most 2 048 bytes | [`failure.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/failure.rs), [`receipts.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/receipts.rs) |
| `Retained(TeardownCustody)` | `Reply::Retained`: first `TeardownStage` not established, `detached`, counters, forced facts | [`serve.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/route/serve.rs) |
| `History`, `Commit`, `Bind`, `Owner`, `Workspace` | `Busy`, `HeadMoved` (with `MovedState`), `Missing`, `Capacity`, `Invalid`, `Failed` or `Unknown`; a known publication travels in `published` | [`failure.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/failure.rs) |
| `Poisoned` | `Unknown`, phase `registry` | same |
| `After { cause, original }` | The original success is kept beside the later failure | [`receipts.rs`](../../../../engine/crates/layerfs-daemon/src/workspace/receipts.rs) |

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
