# FUSE

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The FUSE route of [Kernel routes](README.md): one Workspace over one `/dev/fuse` connection with the owner-authorized patched fuser 0.18.0.

| | |
| --- | --- |
| Source | [`layerfs-fuse/src`](../../../../engine/crates/layerfs-fuse/src) (`session/`, `dispatch/`, `request/`, `mount/`, `coherence/`), [fuser fork](https://github.com/Ephemeral-AI-Lab/fuser/blob/7393bca050fcbd0ed2a03f7e79bec0f8bd9755c4) |

`attributes`, `mount`, `request` and `session` are `#[cfg(target_os = "linux")]`;
`coherence` and `dispatch` build everywhere
([`lib.rs`](../../../../engine/crates/layerfs-fuse/src/lib.rs)). The daemon
owns one `KernelSession` per attached Workspace; dependencies run daemon ->
fuse -> workspace, never back.

## Session startup (`session/startup.rs`, `mount/profile.rs`)

```text
 KernelSession::attach(config, queue, services, identity)
   Filesystem  KernelFilesystem::new (root serial must match the mount)
   Directory   mkdir <mounts>/<new>, 0755; an existing path fails
   Device      open /dev/fuse read-write
   Mount       mount("layerfs", dir, "fuse", NOSUID|NODEV|NOATIME,
                 "fd=..,rootmode=40000,user_id=..,group_id=..,
                  allow_other,default_permissions,max_read=..")
   Handshake   Session::from_fd -> INIT -> negotiate()
   Spawn       "layerfs-mount-<id>" runs SessionRunner::run
               "layerfs-fence-<id>" waits for the first loop exit
   Serving     monitor.wait_ready: phase Serving, entered == loops
   Receipt     negotiation; /proc/self/mountinfo row; abort control
 Ok(KernelSession)      any failure -> AttachFailure{phase, remainder}
```

| `AttachPhase` | On failure | Source |
| --- | --- | --- |
| `Filesystem`, `Directory`, `Device`, `Mount` | Remove the directory it made, stop the request owner | [`startup.rs`](../../../../engine/crates/layerfs-fuse/src/session/startup.rs) |
| `Handshake`, `Spawn`, `Serving`, `Receipt` | One plain detach, then the normal drain: `Disposed(Some(Drained))` or `Retained(Undrained)` | [`startup.rs`](../../../../engine/crates/layerfs-fuse/src/session/startup.rs) |

| Negotiated value | Value | Source |
| --- | --- | --- |
| Flags | `FUSE_ASYNC_READ`, `FUSE_BIG_WRITES`, plus `FUSE_MAX_PAGES` when offered; a kernel missing either required flag fails INIT `Unsupported`. Nothing else, writeback caching included | [`profile.rs`](../../../../engine/crates/layerfs-fuse/src/mount/profile.rs) |
| `max_write`, `max_readahead` | `READ_WINDOW` (128 KiB) | [`profile.rs`](../../../../engine/crates/layerfs-fuse/src/mount/profile.rs) |
| `max_background`, `congestion_threshold` | 1, 1 | [`profile.rs`](../../../../engine/crates/layerfs-fuse/src/mount/profile.rs) |
| Time granularity; `page_size` | 1 ns; `sysconf(PAGE_SIZE)` for the symlink-target limit | [`profile.rs`](../../../../engine/crates/layerfs-fuse/src/mount/profile.rs) |
| Receive loops | `RECEIVE_LOOPS` = `RECEIVE_SLOTS` = 2 | [`state.rs`](../../../../engine/crates/layerfs-fuse/src/session/state.rs), [`types.rs`](../../../../engine/crates/layerfs-fuse/src/dispatch/types.rs) |

Serving is returned only after the mount exists, INIT completed and every
receive loop entered; a spawn or a probe of the mount is not that evidence.

## Drain, force and invalidation (`session/drain.rs`, `force.rs`, `invalidate.rs`)

```text
 Serving --detach(): umount2(dir, 0)--> EBUSY: Busy (service continues)
    | ok, stop_receive                 abort(): one write "1" to the abort
    v                                  control; Written -> stop_service
 drain(): quiesce (no active request,     (fence stopped, parked woken)
   loops Joined, owner and watcher         |
   joined, nothing retained) ->         force_drain(): quiesce, then one
   release: queue.finish(), rmdir        plain detach if none yet
    v                         \            |
 Drained (receipt)      Undrained{stage}  <+  (owner kept)
```

| Call | One attempt of | Refused before effect when | Source |
| --- | --- | --- | --- |
| `detach` | `umount2(dir, 0)`; no `MNT_DETACH` or `MNT_FORCE` | already detached, or abort written | [`drain.rs`](../../../../engine/crates/layerfs-fuse/src/session/drain.rs) |
| `drain` | join and quiesce, then release | not detached (`DrainStage::Detach`) | [`drain.rs`](../../../../engine/crates/layerfs-fuse/src/session/drain.rs) |
| `abort` | one `write(2)` of one byte on the bound control | detached, aborted, or no control (`ForceRefusal`) | [`force.rs`](../../../../engine/crates/layerfs-fuse/src/session/force.rs) |
| `force_drain` | quiesce, then one plain detach | abort not `Written`, or a recorded failed detach | [`force.rs`](../../../../engine/crates/layerfs-fuse/src/session/force.rs) |
| `Undrained::end_stalled` | `abort_detached`, then `force_drain` | stage is not `Join` | [`force.rs`](../../../../engine/crates/layerfs-fuse/src/session/force.rs) |
| `Invalidator::inode(serial)` | one `inval_inode(inode, 0, 0)`: attributes and cached pages or listing | uncached target: `ENOENT`, reported as success | [`invalidate.rs`](../../../../engine/crates/layerfs-fuse/src/session/invalidate.rs) |
| `Invalidator::entry(parent, name)` | one `inval_entry` | same | [`invalidate.rs`](../../../../engine/crates/layerfs-fuse/src/session/invalidate.rs) |

`DrainStage` is `Detach`, `Join`, `Owner`, `Requests` or `Lane`
([`state.rs`](../../../../engine/crates/layerfs-fuse/src/session/state.rs)).
`end_stalled` covers a detach that succeeded while another mount of the same
superblock kept the connection alive. Dropping a `KernelSession` performs no
detach, abort, join or revocation; `Drained` is necessary for terminal
unmount, not sufficient. The daemon uses the `Invalidator` only after a Commit
installed a root other than the one shown, for each `Stale::Inode` or
`Stale::Entry` that `stale_after_install` returns
([`invalidate.rs`](../../../../engine/crates/layerfs-daemon/src/control/invalidate.rs)).

## Dispatch (`dispatch/`)

```text
 kernel -> receive loop thread (2 per mount)
   MountRequests::receive()   terminal? -> EIO reply, Disposal::Terminal
   Received::request(bytes)   > MAX_INPUT_BYTES -> ENOTCONN
   RequestOwner::serve(future)  poll here; Pending -> park until woken
     Complete -> completed += 1
     Retained(error) or panic -> failure kept, mount terminal
```

| Item | Meaning | Source |
| --- | --- | --- |
| `MountRequests` | Per-mount request owner: counts, retained failures, parked threads, its `Fence`; no daemon-wide dispatcher | [`requests.rs`](../../../../engine/crates/layerfs-fuse/src/dispatch/requests.rs) |
| `MountWork` | `active`, `retained`, `requests`, `completed`, `terminal` | [`diagnostics.rs`](../../../../engine/crates/layerfs-fuse/src/dispatch/diagnostics.rs) |
| `stop_receive`, `stop_service` | Mark terminal; `stop_service` also stops the fence and unparks every running request | [`requests.rs`](../../../../engine/crates/layerfs-fuse/src/dispatch/requests.rs) |
| `finish` | Releases the owner only when terminal with nothing active or retained; else `Busy` | [`requests.rs`](../../../../engine/crates/layerfs-fuse/src/dispatch/requests.rs) |

A receive loop runs its request itself, so a request waiting on a Store
reader holds its loop; two loops let one request proceed while another waits.

## Requests and replies (`request/`, `attributes.rs`)

| Reply case | Errno | Source |
| --- | --- | --- |
| Fenced step (forced teardown) | `ENOTCONN`, counted on the fence | [`terminal.rs`](../../../../engine/crates/layerfs-fuse/src/request/terminal.rs) |
| Failed base demand of this request | `EIO`; the mount keeps serving | [`terminal.rs`](../../../../engine/crates/layerfs-fuse/src/request/terminal.rs) |
| `Refusal` | `EEXIST`, `ENOENT`, `ENOTDIR`, `EISDIR`, `ENOTEMPTY`, `EPERM`, `EOPNOTSUPP`, `EINVAL`, `EMLINK`, `EFBIG` | [`attributes.rs`](../../../../engine/crates/layerfs-fuse/src/attributes.rs) |
| `Declined::NoSpace`, `Contended`, `TargetTooLong` | `ENOSPC`, `EAGAIN`, `ENAMETOOLONG` | [`attributes.rs`](../../../../engine/crates/layerfs-fuse/src/attributes.rs) |
| Missing LOOKUP name | Negative entry (node 0), 60 s entry TTL; every create, link, mkdir, symlink or rename through the mount replaces it | [`reply.rs`](../../../../engine/crates/layerfs-fuse/src/request/reply.rs) |
| FLUSH, FSYNC, FSYNCDIR and the unsupported set | `ENOSYS` | [`callbacks.rs`](../../../../engine/crates/layerfs-fuse/src/request/callbacks.rs) |
| Shared-mapping store | WRITE with `FUSE_WRITE_CACHE`, counted in `store_units` | [`callbacks.rs`](../../../../engine/crates/layerfs-fuse/src/request/callbacks.rs) |
| Reply not composable from the published result | `EIO`, request retained | [`mutate.rs`](../../../../engine/crates/layerfs-fuse/src/request/mutate.rs) |

| Identity | Rule | Source |
| --- | --- | --- |
| Inode number | Root serial is inode 1; any other is serial + 1; 0 is `ESTALE` | [`attributes.rs`](../../../../engine/crates/layerfs-fuse/src/attributes.rs) |
| Generation | The mount's `owner_id` | [`reply.rs`](../../../../engine/crates/layerfs-fuse/src/request/reply.rs) |
| Link count | Directory: 0 once removed, else child directories + 2; others: namespace references | [`attributes.rs`](../../../../engine/crates/layerfs-fuse/src/attributes.rs) |
| Times | One stored mtime shows as atime, mtime, ctime and crtime | [`attributes.rs`](../../../../engine/crates/layerfs-fuse/src/attributes.rs) |

A batched FORGET reaches the adapter as single callbacks, each admitted and
served alone ([`inline.rs`](../../../../engine/crates/layerfs-fuse/src/request/inline.rs)).
`Accounting` keeps per-opcode counts and times, dispositions and the largest
READ/WRITE frames ([`accounting.rs`](../../../../engine/crates/layerfs-fuse/src/request/accounting.rs)).

## Mount syscalls (`mount/syscalls.rs`)

| Function | Syscall | Rule |
| --- | --- | --- |
| `create_directory` | `mkdir` 0755 | An existing directory is never adopted |
| `open_device` | `open("/dev/fuse", O_RDWR)` | The session takes the only descriptor |
| `attach` | `mount` type `fuse`, source `layerfs` | The daemon is mount owner; commands enter through `allow_other` |
| `detach` | `umount2(dir, 0)` | Exactly one plain attempt; no helper binary, no lazy unmount |
| `mount_entry` | read `/proc/self/mountinfo` | Topmost row at exactly this path; sends nothing to the mount |
| `abort_control`, `control_root` | open `<control>/<minor>/abort`; mount `fusectl` | Only a major-0 `fuse`/`layerfs` row; `/sys/fs/fuse/connections`, else a private 0700 copy |
| `abort` | `write(control, "1")` | One write: `Written`, `Short(n)` or `Failed(errno)` |

All in [`syscalls.rs`](../../../../engine/crates/layerfs-fuse/src/mount/syscalls.rs).

## Fuser Git dependency

`layerfs-fuse` uses the organization's [fuser fork](https://github.com/Ephemeral-AI-Lab/fuser)
at fixed revision [`7393bca050fcbd0ed2a03f7e79bec0f8bd9755c4`](https://github.com/Ephemeral-AI-Lab/fuser/blob/7393bca050fcbd0ed2a03f7e79bec0f8bd9755c4),
package version 0.18.0. The dependency is declared directly in
[its manifest](../../../../engine/crates/layerfs-fuse/Cargo.toml); the
[root lockfile](../../../../Cargo.lock) records that Git source and commit.
There is no local vendor, patch application or crates.io override. The branch
name is not the dependency identity and no published package is required.

The fork owns three changes independently of LayerFS:

| Change | Behavior | Used here |
| --- | --- | --- |
| [Signed timestamps](https://github.com/Ephemeral-AI-Lab/fuser/blob/7393bca050fcbd0ed2a03f7e79bec0f8bd9755c4/src/time.rs) | Count nanoseconds forward from a negative whole second and avoid signed-minimum negation overflow. | Signed mtimes; the adapter's own [timestamp conversion](../../../../engine/crates/layerfs-fuse/src/attributes.rs) follows the same rule. |
| [Session lifecycle](https://github.com/Ephemeral-AI-Lab/fuser/blob/7393bca050fcbd0ed2a03f7e79bec0f8bd9755c4/src/session/lifecycle.rs) | Additive `Session::into_runner`, `SessionRunner` and `SessionMonitor`; all-loop startup observation and retained original receiver joins and cleanup outcomes. `Session::run` remains available. | [Attach and the fence watcher](../../../../engine/crates/layerfs-fuse/src/session/startup.rs), [session facts](../../../../engine/crates/layerfs-fuse/src/session/state.rs) and [drain](../../../../engine/crates/layerfs-fuse/src/session/drain.rs). |
| [Receive buffer](https://github.com/Ephemeral-AI-Lab/fuser/blob/7393bca050fcbd0ed2a03f7e79bec0f8bd9755c4/src/read_buf.rs) | Receive loops allocate `max_write + 4096` after INIT; the handshake keeps the maximum buffer. | Two loops, each negotiated to 128 KiB + 4 KiB, rather than 16 MiB + 4 KiB. |

A monitor deadline returns a snapshot; it does not cancel, detach, retry or
release the runner. Mount and connection disposition still belong to the caller.
The smaller receive buffer follows the negotiated write limit, not a total
file or Workspace capacity limit.

The fork's own repository maintains its code and tests. LayerFS uses ordinary
locked dependency builds and its integration tests; no local patch/source checker
is required. A revision change updates the manifest, lock and this page together.

The earlier local-patch mechanism and its source/verification records remain
[available at the deletion-parent snapshot](https://github.com/Ephemeral-AI-Lab/layerfs/tree/8cfb6e273470d0bb2a4ec5a9256fcf4972441eb5/engine/patches/fuser-0.18.0).
Their original verdicts, including the Linux fractional signed-minimum FAIL and
its owner acceptance, are unchanged. Moving the source to the fork does not
re-run or broaden those proofs.

## Related

- [Kernel routes](README.md): ports, operations and coherence rules.
- [Daemon](../daemon/README.md), [Daemon: Control calls](../daemon/control-calls.md): who attaches, detaches, drains and forces.
- [Overlay](../overlay/README.md): open state and lookup references.
