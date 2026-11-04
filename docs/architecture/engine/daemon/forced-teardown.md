# Forced teardown

[Architecture](../../README.md) › [Engine](../README.md) › [Daemon](README.md) › Forced teardown

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Forced unmount and what it reports.

| | |
| --- | --- |
| Component | [Daemon](README.md) |
| Source | [`engine/crates/layerfs-daemon/src/control/force.rs`](../../../../engine/crates/layerfs-daemon/src/control/force.rs), [`engine/crates/layerfs-daemon/src/control/cleanup.rs`](../../../../engine/crates/layerfs-daemon/src/control/cleanup.rs), [`engine/crates/layerfs-daemon/src/control/dirty.rs`](../../../../engine/crates/layerfs-daemon/src/control/dirty.rs), [`engine/crates/layerfs-daemon/src/control/failure.rs`](../../../../engine/crates/layerfs-daemon/src/control/failure.rs) |

`ForceUnmount { token, relinquish_unknown }` closes one attached Workspace
without waiting for the processes that use its mount. It signals no
process. Each step is made once.

## Force

```text
registry lock: guard (in this order; a refusal changes nothing)
  1 ExportKept / Retained custody -> Retained (except a forceable
    normal unmount stalled at Join, which is admitted)
  2 Committing | Attaching | Closing   -> Busy   (force:admission)
  3 no serving connection              -> Invalid / Unknown / Busy
  4 Commit knowledge: Idle -> Absent; LocalFailure + published ->
    Published; Uncertain + relinquish_unknown -> Unknown; else Unknown
    refusal (force:custody)
  5 abort control not bound           -> Failed  (force:capability)
admit: Kernel -> Leaving(Stopping), Activity::Closing
-----------------------------------------------------------------
session.abort()          one 1-byte write to the connection's abort
  refused before write --> Kernel::Ready again, prior activity
  Short / Failed -------> Retained(Abort)
session.force_drain()    the local drain with its one plain detach
  stopped --------------> Retained(stage), facts.detach recorded
phase Draining; job: kernel_revoke(mount) -- fails --> Retained(Revoke)
closed(): logical Close            -- fails --> Retained(Close)
observe_cleanup()        Held | Queued | Gone (or Unobserved)
depart: entry removed, retire_namespace (file deletion)
reply ForceUnmounted { token, ForcedOutcome { facts, work, cleanup } }
```

| `ForcedFacts` field | Values | Meaning |
| --- | --- | --- |
| `abort` | `Written`, `Short`, `Failed` | The one abort write |
| `detach` | `NotAttempted`, `Detached`, `Busy`, `Failed` | The one plain detach; `Busy` here means aborted and still mounted |
| `commit` | `Absent`, `Published(outcome)`, `Unknown` | Commit custody copied at admission, never settled later |
| `fenced` | count | Terminal replies the drain made |

| Rule | Source |
| --- | --- |
| The abort write uses the connection's own control, bound from the daemon's `fusectl` tree at Attach. Without it, Force is refused before any effect. | [`force.rs`](../../../../engine/crates/layerfs-daemon/src/control/force.rs), [`application/filesystem.rs`](../../../../engine/crates/layerfs-daemon/src/application/filesystem.rs) |
| A normal unmount stalled at `Join` with its abort control unused is forced from its retained session; its plain detach counts as the one detach. | [`force.rs`](../../../../engine/crates/layerfs-daemon/src/control/force.rs), [`serving.rs`](../../../../engine/crates/layerfs-daemon/src/control/serving.rs) `Kept::forceable` |
| `relinquish_unknown` allows local disposal of stopped unknown Commit custody; the receipt keeps it `Unknown`. | [`force.rs`](../../../../engine/crates/layerfs-daemon/src/control/force.rs), [`control_request.rs`](../../../../engine/crates/layerfs-bridge/src/control_request.rs) |
| On the NFS route the force helper is the abort; the same teardown follows. | [`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs) |
| Outside Linux without NFS, the guard runs and the call is refused (`kernel serving requires Linux`). | [`force.rs`](../../../../engine/crates/layerfs-daemon/src/control/force.rs) |
| No daemon-wide graceful drain exists. | [engine guide](../../../../engine/AGENTS.md) |

## Cleanup

| `CleanupState` / `ForcedCleanup` | Meaning | Source |
| --- | --- | --- |
| `Live` | Not closed. After a Close this is reported `Unobserved` with an observation failure | [`close.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/close.rs), [`force.rs`](../../../../engine/crates/layerfs-daemon/src/control/force.rs) |
| `Held` | Closed, but a kernel mount, file, reader, lookup, capture, base reader, reply ticket or owner still holds it | [`close.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/close.rs) |
| `Queued` | Closed and ready for checked connection close | [`close.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/close.rs) |
| `Gone` | Connection closed and file deleted by the roster | [`workspaces.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/workspaces.rs) |
| `Unobserved` | The one observation failed; no state is claimed | [`force.rs`](../../../../engine/crates/layerfs-daemon/src/control/force.rs) |

The forced receipt reads cleanup once, after Close and before `depart`.
`Request::Cleanup(token)` reads it on demand later, also after the
registry entry is gone ([`cleanup.rs`](../../../../engine/crates/layerfs-daemon/src/control/cleanup.rs)).

## Dirty state

A normal `Unmount` without `discard` first asks whether the Workspace holds
changes a Commit would keep. Force does not run this check.

```text
state job: dirty_inodes, dirty_directory_entries, captured, base_root
clean if all zero, no capture, base_root == binding root, no pending merge
else stream active_directory_entries and active_inodes (64-row windows):
  a changed name                    -> path of parent + name
  a changed file (not a local file
    that no name binds any more)    -> its path
  a base directory whose mode moved -> its path
  base paths: bounded walk of the base tree (WALK 4096, DEPTH 256)
reply Dirty(DirtyReport { paths <= 256, inodes, entries,
                          pending_merge, truncated })
```

Source: [`dirty.rs`](../../../../engine/crates/layerfs-daemon/src/control/dirty.rs),
[`control_unmount.rs`](../../../../engine/crates/layerfs-bridge/src/control_unmount.rs).
No-op rows (an unbound local inode, a base directory with an unchanged mode)
neither refuse the Unmount nor truncate the report.

## Failure reporting

| `Failure` | Wire `ControlRefusal` | Source |
| --- | --- | --- |
| `Rejected(code, detail)` | `code`, phase `admission` | [`failure.rs`](../../../../engine/crates/layerfs-daemon/src/control/failure.rs) |
| `Kernel(KernelFailure)` | Its `code` and stable `phase` (for example `attach:mount`, `unmount:kernel`, `force:capability`), detail bounded to 2048 bytes | [`failure.rs`](../../../../engine/crates/layerfs-daemon/src/control/failure.rs), [`kernel.rs`](../../../../engine/crates/layerfs-daemon/src/control/kernel.rs) |
| `Retained(TeardownCustody)` | Sent as `Reply::Retained`: token, first `TeardownStage` not established, `detached`, counters, forced facts | [`serve.rs`](../../../../engine/crates/layerfs-daemon/src/control/serve.rs), [`control_mount.rs`](../../../../engine/crates/layerfs-bridge/src/control_mount.rs) |
| `History`, `Commit`, `Bind`, `Owner`, `Workspace` | Mapped to `Busy`, `HeadMoved`, `Missing`, `Capacity`, `Invalid`, `Failed` or `Unknown`; `HeadMoved` carries `MovedState`; a known publication travels in `published` | [`failure.rs`](../../../../engine/crates/layerfs-daemon/src/control/failure.rs) |
| `Poisoned` | `Unknown`, phase `registry` | [`failure.rs`](../../../../engine/crates/layerfs-daemon/src/control/failure.rs) |
| `After { cause, original }` | The original success is kept with the later failure | [`types.rs`](../../../../engine/crates/layerfs-daemon/src/control/types.rs) |

| `TeardownStage` | First boundary not established |
| --- | --- |
| `Detach` (1) | Kernel detach was neither success nor the reversible `Busy` |
| `Join` (2) | Not every receive loop is known joined |
| `Owner` (3) | The connection owner or its exit watcher did not return |
| `Requests` (4) | Request ownership remains |
| `Lane` (5) | The mount's request owner was not released |
| `Revoke` (6) | Engine mount revocation refused or unknown |
| `Close` (7) | Logical Close refused or unknown |
| `Registry` (8) | The registry could not record the transition |
| `Abort` (9) | The forced abort write was short or failed |

A failure whose code is `Unknown` (and is not `Retained`) is `uncertain()`:
its connection keeps the receipt in its slot after the reply is sent. The
`detail` text is never parsed to infer the category. Source:
[`failure.rs`](../../../../engine/crates/layerfs-daemon/src/control/failure.rs),
[`connection.rs`](../../../../engine/crates/layerfs-daemon/src/application/connection.rs).

## Related

- [Control calls](control-calls.md#unmount): normal unmount, which shares the drain and Close.
- [Overlay: Lifetimes and reclamation](../overlay/lifetimes-and-reclamation.md): what `Held`, `Queued` and `Gone` observe.
- [Kernel routes: FUSE](../kernel-routes/fuse.md): abort control, detach and drain.
- [Bridge](bridge.md#records): `ForceUnmounted` and `TeardownCustody` records.
- [Glossary](../../glossary.md).

---

← [Daemon: Control calls](control-calls.md) · [Up: Daemon](README.md) · [Daemon: Bridge](bridge.md) →
