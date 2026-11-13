# FUSE

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The FUSE route of [Kernel routes](README.md): one Workspace over one `/dev/fuse` connection with the owner-authorized patched fuser 0.18.0.

| | |
| --- | --- |
| Source | [`layerfs-fuse/src`](../../../../engine/crates/layerfs-fuse/src) (`session/`, `dispatch/`, `request/`, `mount/`, `coherence/`), [`engine/patches/fuser-0.18.0`](../../../../engine/patches/fuser-0.18.0), [`engine/vendor/fuser-0.18.0`](../../../../engine/vendor/fuser-0.18.0) |

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

## Authorized fuser patches (`engine/patches/fuser-0.18.0`)

```text
 fuser-0.18.0.crate (crates.io, sha256 b82b6597...cfd, 85 files)
   1 signed-timestamps.patch  src/time.rs
   2 session-lifecycle.patch  src/lib.rs, src/session.rs, CHANGELOG.md,
                              + src/session/lifecycle.rs     (86 files)
   3 receive-buffer.patch     src/read_buf.rs, src/session.rs,
                              src/session/lifecycle.rs
 -> engine/vendor/fuser-0.18.0 <- [patch.crates-io] in the root Cargo.toml
```

`layerfs-fuse` depends on `fuser =0.18.0`
([`Cargo.toml`](../../../../engine/crates/layerfs-fuse/Cargo.toml)); the root
manifest redirects it to the checked-in vendor copy and excludes that copy
from the workspace ([`Cargo.toml`](../../../../Cargo.toml)). No other
dependency is patched. The binding rules are in the
[engine guide](../../../../engine/AGENTS.md); the patch record is
[`engine/patches/fuser-0.18.0/README.md`](../../../../engine/patches/fuser-0.18.0/README.md).

| Patch | What it changes | Used by LayerFS | Diff, provenance |
| --- | --- | --- | --- |
| Signed timestamps | `system_time_from_time` counts nanoseconds forward from a negative whole second, as a `timespec` does (`(-1, 500_000_000)` is epoch − 0.5 s); nanoseconds clamp to `999_999_999`; from upstream commit `e48279f` | Signed mtimes; the adapter's own `timestamp` follows the same rule ([`attributes.rs`](../../../../engine/crates/layerfs-fuse/src/attributes.rs)) | [`signed-timestamps.patch`](../../../../engine/patches/fuser-0.18.0/signed-timestamps.patch), [`provenance.json`](../../../../engine/patches/fuser-0.18.0/provenance.json) |
| Session lifecycle | Additive `Session::into_runner` -> `SessionRunner` + `SessionMonitor`; phases `Prepared`, `Starting`, `Serving` (every loop created and entered), `Stopping`, `Joined`; `SessionOutcome` keeps the startup error, every receiver's join result (panics included) and cleanup. `Session::run` is unchanged | `attach` (`wait_ready`), the fence watcher and the drain `Join` wait (`wait_for_change`), `SessionFacts` ([`startup.rs`](../../../../engine/crates/layerfs-fuse/src/session/startup.rs), [`state.rs`](../../../../engine/crates/layerfs-fuse/src/session/state.rs)) | [`session-lifecycle.patch`](../../../../engine/patches/fuser-0.18.0/session-lifecycle.patch), [`lifecycle-provenance.json`](../../../../engine/patches/fuser-0.18.0/lifecycle-provenance.json) |
| Receive buffer | `negotiated_size(max_write) = max_write + 4096`; each receive loop allocates that instead of `MAX_WRITE_SIZE + 4096` (16 MiB + 4 KiB); the INIT buffer keeps fuser's size | 2 loops × (128 KiB + 4 KiB) per mount | [`receive-buffer.patch`](../../../../engine/patches/fuser-0.18.0/receive-buffer.patch), [`receive-buffer-provenance.json`](../../../../engine/patches/fuser-0.18.0/receive-buffer-provenance.json) |

No `SessionMonitor` deadline cancels, detaches, retries or disposes the
runner; a deadline only returns the current snapshot. The kernel never sends
a write larger than the negotiated `max_write`, so the smaller buffer holds
every request.

[`tools/check_fuser_integrity.py`](../../../../tools/check_fuser_integrity.py)
(`python3 -B tools/check_fuser_integrity.py`, run before a native fuser build)
reads files only: only the root manifest may redirect fuser, only its locks
hold the patched entry, each provenance record and patch matches its pinned
SHA-256, and every vendor file matches the archive except the pinned changed
or added files. The provenance JSON files are byte-pinned by that script, so
they change only together with its constants. Rebuilding the vendor copy is:
extract the archive, then apply the three patches in order with
`patch --batch --forward -p1`. Third-party source is outside first-party
production LOC.

## Related

- [Kernel routes](README.md): ports, operations and coherence rules.
- [Daemon](../daemon/README.md), [Daemon: Control calls](../daemon/control-calls.md): who attaches, detaches, drains and forces.
- [Overlay](../overlay/README.md): open state and lookup references.
