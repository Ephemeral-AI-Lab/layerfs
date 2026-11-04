# FUSE

[Architecture](../../README.md) › [Engine](../README.md) › [Kernel routes](README.md) › FUSE

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The FUSE route.

| | |
| --- | --- |
| Component | [Kernel routes](README.md) |
| Source | [`engine/crates/layerfs-fuse/src/session`](../../../../engine/crates/layerfs-fuse/src/session), [`engine/crates/layerfs-fuse/src/dispatch`](../../../../engine/crates/layerfs-fuse/src/dispatch), [`engine/crates/layerfs-fuse/src/request`](../../../../engine/crates/layerfs-fuse/src/request), [`engine/crates/layerfs-fuse/src/mount`](../../../../engine/crates/layerfs-fuse/src/mount) |

`layerfs-fuse` serves one Workspace over one `/dev/fuse` connection with the
patched fuser 0.18.0 library. Everything except `coherence` and `dispatch` is
`#[cfg(target_os = "linux")]`
([`lib.rs`](../../../../engine/crates/layerfs-fuse/src/lib.rs)). The daemon
owns one `KernelSession` per attached Workspace; the dependency runs daemon ->
fuse -> workspace, never back.

## Session startup

```text
 KernelSession::attach(config, queue, services, identity)
   |
   |  Filesystem   KernelFilesystem::new  (root serial must match the mount)
   |  Directory    mkdir <mounts>/<new>, mode 0755; an existing path fails
   |  Device       open /dev/fuse read-write
   |  Mount        mount("layerfs", dir, "fuse", NOSUID|NODEV|NOATIME,
   |                     "fd=..,rootmode=40000,user_id=..,group_id=..,
   |                      allow_other,default_permissions,max_read=..")
   |  Handshake    Session::from_fd -> INIT -> negotiate()
   |  Spawn        thread "layerfs-mount-<id>"  runs SessionRunner::run
   |               thread "layerfs-fence-<id>"  waits for first loop exit
   |  Serving      monitor.wait_ready: phase Serving, entered == loops
   |  Receipt      negotiation recorded; /proc/self/mountinfo row;
   |               abort control <control>/<minor>/abort bound
   v
 Ok(KernelSession)          any failure -> AttachFailure{phase, remainder}
```

| `AttachPhase` | Failure before the mount call | Failure after the mount call | Source |
| --- | --- | --- | --- |
| `Filesystem`, `Directory`, `Device`, `Mount` | Remove the directory it made, stop the request owner: `Disposed` or `Unmounted` | — | [`startup.rs`](../../../../engine/crates/layerfs-fuse/src/session/startup.rs) |
| `Handshake`, `Spawn`, `Serving`, `Receipt` | — | One plain detach, then the normal drain: `Disposed(Some(Drained))` or `Retained(Undrained)` | [`startup.rs`](../../../../engine/crates/layerfs-fuse/src/session/startup.rs) |

| Negotiated value | Value | Source |
| --- | --- | --- |
| Selected flags | `FUSE_ASYNC_READ`, `FUSE_BIG_WRITES`, plus `FUSE_MAX_PAGES` when offered; a kernel that offers neither required flag, or only one, fails INIT with `Unsupported`. Nothing else, including writeback caching, is selected | [`profile.rs`](../../../../engine/crates/layerfs-fuse/src/mount/profile.rs) |
| `max_write`, `max_readahead` | `READ_WINDOW` (128 KiB) | [`profile.rs`](../../../../engine/crates/layerfs-fuse/src/mount/profile.rs), [`types.rs`](../../../../engine/crates/layerfs-overlay/src/contract/types.rs) |
| `max_background`, `congestion_threshold` | 1, 1 | [`profile.rs`](../../../../engine/crates/layerfs-fuse/src/mount/profile.rs) |
| Time granularity | 1 ns | [`profile.rs`](../../../../engine/crates/layerfs-fuse/src/mount/profile.rs) |
| `page_size` | `sysconf(PAGE_SIZE)`, kept for the symlink-target limit | [`profile.rs`](../../../../engine/crates/layerfs-fuse/src/mount/profile.rs) |
| Receive loops | `RECEIVE_LOOPS` = `RECEIVE_SLOTS` = 2 | [`types.rs`](../../../../engine/crates/layerfs-fuse/src/dispatch/types.rs), [`state.rs`](../../../../engine/crates/layerfs-fuse/src/session/state.rs) |
| Library ACL | `SessionACL::All`; the kernel's `default_permissions` checks decide access | [`startup.rs`](../../../../engine/crates/layerfs-fuse/src/session/startup.rs) |

Serving is returned only after the mount exists, INIT completed and every
receive loop entered. A spawn or a probe of the mount is not that evidence.

## Drain and invalidation

```text
              attach
                |
                v
 +---------------------------+  detach(): one umount2(dir, 0)
 |         Serving           |--- EBUSY -----> Busy (nothing changed,
 |  requests served on the   |                 service continues)
 |  receive loops            |
 +---------------------------+
   | detach ok                 | abort(): one write "1" to the abort control
   | stop_receive              | Written -> stop_service: fence stopped,
   v                           |            parked requests woken
 +---------------------------+ v
 | drain()                   | +--------------------------------------+
 |  quiesce(deadline):       | | force_drain()                        |
 |   no active request       | |  quiesce(deadline) (same half)       |
 |   loops Joined            | |  then one plain detach (if none yet) |
 |   owner, watcher joined   | +--------------------------------------+
 |   no active or retained   |        |
 |  release: queue.finish(), |        |
 |   rmdir directory         |<-------+
 +---------------------------+
   |                         \
   v                          v
 Drained (receipt)          Undrained{stage, ...}  (owner kept)
```

| Call | One attempt of | Refused before effect when | Source |
| --- | --- | --- | --- |
| `detach` | `umount2(dir, 0)`; `EBUSY` is `Busy`; no `MNT_DETACH` or `MNT_FORCE` | already detached, or the abort write was made | [`drain.rs`](../../../../engine/crates/layerfs-fuse/src/session/drain.rs) |
| `drain` | join and quiesce, then release | not detached (`DrainStage::Detach`) | [`drain.rs`](../../../../engine/crates/layerfs-fuse/src/session/drain.rs) |
| `abort` | one `write(2)` of one byte on the bound control | detached, already aborted, or no control bound (`ForceRefusal`) | [`force.rs`](../../../../engine/crates/layerfs-fuse/src/session/force.rs) |
| `force_drain` | quiesce, then one plain detach | abort not `Written`, or a recorded failed detach | [`force.rs`](../../../../engine/crates/layerfs-fuse/src/session/force.rs) |
| `Undrained::end_stalled` | `abort_detached` then `force_drain` | stage is not `Join` | [`force.rs`](../../../../engine/crates/layerfs-fuse/src/session/force.rs) |

`DrainStage` names where a drain stopped: `Detach`, `Join`, `Owner`,
`Requests` or `Lane`
([`state.rs`](../../../../engine/crates/layerfs-fuse/src/session/state.rs)).
`end_stalled` covers a detach that succeeded while another mount of the same
superblock kept the connection alive, so the loops never returned. Dropping a
`KernelSession` performs no detach, abort, join or revocation. `Drained` is
necessary for terminal unmount, not sufficient: engine, Store and control
owners are released by the daemon.

Once serving, `KernelSession::invalidator` hands the daemon an `Invalidator`
over the connection's `Notifier`; it is used only after a Commit installed a
different root ([Coherence: Invalidation](coherence.md#invalidation)).

## Dispatch

```text
   kernel --> receive loop thread (2 per mount)
                |
                v
   MountRequests::receive()  -- terminal? -> EIO reply, Disposal::Terminal
                |  active += 1, thread recorded
                v
   Received::request(bytes)  -- > MAX_INPUT_BYTES -> ENOTCONN
                |
                v
   RequestOwner::serve(future)
     poll on this thread; Pending -> thread::park until woken
     Complete           -> completed += 1
     Retained(error)    -> failure kept, retained += 1,
     or panic              mount terminal (later receives refused)
```

| Item | Meaning | Source |
| --- | --- | --- |
| `MountRequests` | Per-mount request owner: counts, retained failures, parked threads and its `Fence`. There is no daemon-wide dispatcher. | [`requests.rs`](../../../../engine/crates/layerfs-fuse/src/dispatch/requests.rs) |
| `MountWork` | `active`, `retained`, `requests`, `completed`, `terminal` | [`diagnostics.rs`](../../../../engine/crates/layerfs-fuse/src/dispatch/diagnostics.rs) |
| `stop_receive` | Marks the mount terminal and wakes drain waiters. | [`requests.rs`](../../../../engine/crates/layerfs-fuse/src/dispatch/requests.rs) |
| `stop_service` | `stop_receive`, stops the fence, unparks every running request so it sees the stop. | [`requests.rs`](../../../../engine/crates/layerfs-fuse/src/dispatch/requests.rs) |
| `finish` | Releases the owner only when terminal with no active and no retained request; else `Busy`. | [`requests.rs`](../../../../engine/crates/layerfs-fuse/src/dispatch/requests.rs) |
| `MAX_INPUT_BYTES` | `WRITE_WINDOW + 2 * 255` | [`types.rs`](../../../../engine/crates/layerfs-fuse/src/dispatch/types.rs) |

A receive loop runs its request itself, so a request waiting on a Store reader
holds its loop. Two loops let one request proceed while another waits. A
retained failure or panic keeps its original future and makes the mount
terminal; nothing is replayed.

## Requests and replies

```text
 kernel opcode
   |  KernelFilesystem callback (Timed: opcode clock + phase scope)
   |  inline (STATFS), refusal (ENOSYS, ENAMETOOLONG, EINVAL) or:
   v
 receive_request / receive_reply ---------------------- Dispatch above
   |  identity.serial(inode)  (ESTALE if not a serial)
   v
 serve(future):
   services = MountServices::request(&fence)
   read class:  KernelRead / KernelData   --> ReadReply::serve
   mutation:    KernelMutation::perform   --> MutationReply::serve
   READDIR:     DirectoryStream           --> enumerate
   |
   |  [ordering] the publishing owner job commits under the Workspace
   |             lock; the reply is composed from its published values
   v
 one reply attempt (fuser consumes the reply; no send result)
   |
   v
 KernelMutation::replied -> reply_attempted(ticket)  [after the reply]
 READDIR: publish_cookies of the accepted prefix     [before the reply]
   |
   v
 RequestDisposition::Complete | Retained(failure)
```

| Reply case | Errno | Source |
| --- | --- | --- |
| Fenced step (forced teardown) | `ENOTCONN` (`STOPPED`), counted on the fence | [`terminal.rs`](../../../../engine/crates/layerfs-fuse/src/request/terminal.rs) |
| Failed base demand of this request | `EIO`; the mount keeps serving | [`terminal.rs`](../../../../engine/crates/layerfs-fuse/src/request/terminal.rs) |
| `Refusal` | `EEXIST`, `ENOENT`, `ENOTDIR`, `EISDIR`, `ENOTEMPTY`, `EPERM`, `EOPNOTSUPP`, `EINVAL`, `EMLINK`, `EFBIG` | [`attributes.rs`](../../../../engine/crates/layerfs-fuse/src/attributes.rs) |
| `Declined::NoSpace`, `Contended`, `TargetTooLong` | `ENOSPC`, `EAGAIN`, `ENAMETOOLONG` | [`attributes.rs`](../../../../engine/crates/layerfs-fuse/src/attributes.rs) |
| Missing LOOKUP name | Negative entry (node 0) with a 60 s entry TTL | [`reply.rs`](../../../../engine/crates/layerfs-fuse/src/request/reply.rs) |
| FLUSH, FSYNC, FSYNCDIR; xattrs, ACCESS, READDIRPLUS, locks, BMAP, IOCTL, POLL, LSEEK, FALLOCATE, COPY_FILE_RANGE | `ENOSYS` | [`callbacks.rs`](../../../../engine/crates/layerfs-fuse/src/request/callbacks.rs) |
| Reply that cannot be composed from the published result | `EIO`, request retained | [`mutate.rs`](../../../../engine/crates/layerfs-fuse/src/request/mutate.rs) |

| Identity | Rule | Source |
| --- | --- | --- |
| Inode number | Root serial is inode 1; any other serial is serial + 1; serial 0 is `ESTALE` | [`attributes.rs`](../../../../engine/crates/layerfs-fuse/src/attributes.rs) |
| Generation | The mount's `owner_id` | [`reply.rs`](../../../../engine/crates/layerfs-fuse/src/request/reply.rs) |
| Owner | Every inode shows the configured command uid and gid | [`attributes.rs`](../../../../engine/crates/layerfs-fuse/src/attributes.rs) |
| Link count | Directory: 0 once removed, else child directories + 2; other kinds: namespace references | [`attributes.rs`](../../../../engine/crates/layerfs-fuse/src/attributes.rs) |
| Times | One stored mtime shows as atime, mtime, ctime and crtime | [`attributes.rs`](../../../../engine/crates/layerfs-fuse/src/attributes.rs) |

A batched FORGET reaches the adapter as repeated single callbacks with one
request id; each unit is admitted, charged and served alone
([`inline.rs`](../../../../engine/crates/layerfs-fuse/src/request/inline.rs)).
`Accounting` keeps per-opcode counts and times, dispositions
(`Served`, `Inline`, `Refused`, `Terminal`) and largest READ/WRITE frames
([`accounting.rs`](../../../../engine/crates/layerfs-fuse/src/request/accounting.rs)).

## Mount syscalls

| Function | Syscall | Rule | Source |
| --- | --- | --- | --- |
| `create_directory` | `mkdir` 0755 | An existing directory is never adopted. | [`syscalls.rs`](../../../../engine/crates/layerfs-fuse/src/mount/syscalls.rs) |
| `open_device` | `open("/dev/fuse", O_RDWR)` | The session takes the only descriptor. | [`syscalls.rs`](../../../../engine/crates/layerfs-fuse/src/mount/syscalls.rs) |
| `attach` | `mount` type `fuse`, source `layerfs` | Daemon is mount owner; commands enter through `allow_other` under `default_permissions`. | [`syscalls.rs`](../../../../engine/crates/layerfs-fuse/src/mount/syscalls.rs) |
| `detach` | `umount2(dir, 0)` | Exactly one plain attempt. | [`syscalls.rs`](../../../../engine/crates/layerfs-fuse/src/mount/syscalls.rs) |
| `mount_entry` | read `/proc/self/mountinfo` | Topmost row at exactly this path; sends no request to the mount. | [`syscalls.rs`](../../../../engine/crates/layerfs-fuse/src/mount/syscalls.rs) |
| `abort_control` | open `<control>/<minor>/abort` | Only a row with device major 0, type `fuse`, source `layerfs`; absence binds nothing. | [`syscalls.rs`](../../../../engine/crates/layerfs-fuse/src/mount/syscalls.rs) |
| `control_root` | `mount` type `fusectl` | Uses `/sys/fs/fuse/connections` if mounted there; else mounts a private copy in a new 0700 directory. | [`syscalls.rs`](../../../../engine/crates/layerfs-fuse/src/mount/syscalls.rs) |
| `abort` | `write(control, "1")` | One write; `Written`, `Short(n)` or `Failed(errno)`. | [`syscalls.rs`](../../../../engine/crates/layerfs-fuse/src/mount/syscalls.rs) |

There is no helper binary and no lazy unmount path.

## Related

- [Ports](ports.md): the operations every callback drives.
- [fuser patches](fuser-patches.md): `SessionRunner`, `SessionMonitor` and the
  receive buffer this route relies on.
- [Coherence](coherence.md): cache rules, reply order and invalidation.
- [Daemon: Forced teardown](../daemon/forced-teardown.md) and
  [Daemon: Control calls](../daemon/control-calls.md): who calls attach,
  detach, drain and force.
- [Overlay: Open state](../overlay/open-state.md).
- [Glossary](../../glossary.md).

---

← [Kernel routes: Ports](ports.md) · [Up: Kernel routes](README.md) · [Kernel routes: fuser patches](fuser-patches.md) →
