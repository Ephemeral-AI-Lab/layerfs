# Kernel routes

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How the kernel reaches a mounted Workspace: route-neutral mount ports, then FUSE or a loopback NFSv3 export, with the cache-coherence rules both keep.

| | |
| --- | --- |
| Layer | [Engine](../README.md) |
| Crates | `layerfs-mount-ports`, `layerfs-fuse`, `layerfs-nfs` |
| Source | [`layerfs-mount-ports/src`](../../../../engine/crates/layerfs-mount-ports/src), [`layerfs-fuse/src`](../../../../engine/crates/layerfs-fuse/src), [`layerfs-nfs/src`](../../../../engine/crates/layerfs-nfs/src), [`engine/patches/fuser-0.18.0`](../../../../engine/patches/fuser-0.18.0) |
| Uses | [Workspace](../workspace/README.md), [Overlay](../overlay/README.md) |
| Used by | [Daemon](../daemon/README.md) |
| Flows | [A Workspace session](../../flows/workspace.md) |

## Picture

```text
 any process that sees the mount (shell, Exec, external executor)
         | ordinary filesystem calls
 +-------v---------------- kernel ---------------------+
 |  FUSE driver (/dev/fuse)    |  NFSv3 client         |
 +-----------------------------+-----------------------+
         |                         | TCP 127.0.0.1:<port>
 +-------v-----------+     +-------v-------------+
 | layerfs-fuse      |     | layerfs-nfs         |
 |  KernelSession    |     |  Server, Kernel     |
 |  (patched fuser)  |     |  (synthetic kernel) |
 |  KernelFilesystem |     |  RPC/XDR/MOUNT      |
 +-------+-----------+     +-------+-------------+
         +-----------+-------------+
 +-------------------v--------------------------------+
 | layerfs-mount-ports                                |
 |  MountServices -> RequestServices, Fence           |
 |  KernelRead KernelData KernelMutation              |
 |  DirectoryStream            Phase timing           |
 +-------------------+--------------------------------+
                     | impl MountServices for Mounted (daemon)
 +-------------------v--------------------------------+
 | Workspace visits ---> Overlay (own SQLite file)    |
 |      | base reads (never under the Workspace lock) |
 |      v                                             |
 |  Store (committed base)                            |
 +----------------------------------------------------+
```

One engine mount of a Workspace is served by exactly one route. The daemon
chooses it, attaches it and tears it down. The route per platform is the
`kernel` field of [`control/SUPPORT.json`](../../../../control/SUPPORT.json),
generated from [`support.rs`](../../../../control/crates/layerfs-platform/src/support.rs):
`"nfs"` for the macOS native platform only; `"fuse"` for macOS docker,
apple-container and lima and for Linux native and docker; `null` (unsupported)
for every other row, Windows included
([Control: Machines](../../control/machines/README.md)).

## What it does

### Ports (`ports.rs`)

| Item | What it is | Source |
| --- | --- | --- |
| `MountServices` | One mounted Workspace; `request(&Fence)` makes a fresh per-request scope with no provider I/O, no new cache and no whole-Workspace lock | [`ports.rs`](../../../../engine/crates/layerfs-mount-ports/src/ports.rs) |
| `RequestServices` | What one kernel request may ask. Acquiring calls (`observe_visit`, `read_visit`, `mutate_visit`, `directory_visit`, `publish_cookies`, `base`, `recognize`, `reserve_serial`) are refused once the fence stops; disposal calls (`reply_attempted`, `close_file`, `close_directory`, `forget`) never are | [`ports.rs`](../../../../engine/crates/layerfs-mount-ports/src/ports.rs) |
| `Fence` | One mount's terminal stop, set once by forced teardown and never cleared; it keeps the mount's `FailedDemands` (count, first and latest cause) | [`ports.rs`](../../../../engine/crates/layerfs-mount-ports/src/ports.rs) |
| `BaseDemandFailed` | A Store read for one request failed; only `Fence::failed_demand` builds it, so every marker has its record | [`ports.rs`](../../../../engine/crates/layerfs-mount-ports/src/ports.rs) |
| `ServiceReply<T>` | A value plus the completion receipt; the value drops first | [`ports.rs`](../../../../engine/crates/layerfs-mount-ports/src/ports.rs) |
| `Mounted` | The daemon's implementation over a `BoundWorkspace`; it checks the fence on entry to and every poll of the owner-admission and Store-reader waits | [`filesystem_port.rs`](../../../../engine/crates/layerfs-daemon/src/service/filesystem_port.rs) |

A job already admitted is awaited to its original result even after the fence
stops: a stopped mount starts nothing new and still gives back what it holds.

### Operations (`operations/`)

```text
 LOOKUP/GETATTR/OPEN/OPENDIR   KernelRead::prepare
   loop: observe_visit -> Value | Refused | Needs(n): base(); supply; repeat
 READ/READLINK                 KernelData::read
   read_visit -> local window; base() only for inherited bytes
 mutation                      KernelMutation::perform
   reserve_serial (creates) -> recognize (WRITE) -> mutate_visit loop
   Applied{publication} -> reply attempt -> reply_attempted, exactly once
 READDIR                       DirectoryStream
   directory_visit -> window -> kernel takes k -> publish_cookies(k names)
```

| Type or module | Kernel requests | Source |
| --- | --- | --- |
| `KernelRead` | LOOKUP, GETATTR, OPEN, OPENDIR; holds nothing after its visits | [`lookup.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/lookup.rs) |
| `KernelData` | READ, READLINK | [`read.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/read.rs) |
| `KernelMutation` | Every changing request; holds one `Publication` ticket until `replied` | [`mutation.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/mutation.rs) |
| `DirectoryStream` | READDIR; only accepted names become valid offsets | [`directory.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/directory.rs) |
| `create` | CREATE, MKNOD (regular only), MKDIR, SYMLINK; mode masked to `0o7777` | [`create.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/create.rs) |
| `write` | WRITE at the kernel's offset (at most `WRITE_WINDOW`); SETATTR of mode, size, mtime | [`write.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/write.rs) |
| `link`, `remove`, `rename` | Rename flag `0` replaces, `1` (no-replace) does not, any other is `Invalid` | [`rename.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/rename.rs) |
| `flush` | FLUSH, FSYNC, FSYNCDIR: `Synchronize::NotImplemented`; nothing is buffered in the daemon | [`flush.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/flush.rs) |
| `unsupported` | xattrs, ACCESS, READDIRPLUS, locks, BMAP, IOCTL, POLL, LSEEK, FALLOCATE, COPY_FILE_RANGE: `Absent::NotImplemented`; FIFOs, sockets, devices and foreign uid/gid: `NotPermitted` | [`unsupported.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/unsupported.rs) |

`Declined` is a definite answer with no publication: `Refused(Refusal)`,
`NoSpace`, `Contended` or `TargetTooLong`. Each failure type keeps the
request's identity and what it still owns; `relinquish` ends only a fenced
request or one whose base demand failed. Base reads run between owner
visits, after the visit's receipt dropped, so no request holds the Workspace
lock while it waits on the Store.

### Phase timing (`phase.rs`)

`Phases` keeps one count and one nanosecond total per `Phase`: `ReadVisit`,
`ReadSupply`, `MutateVisit`, `MutateSupply`, `MutateRelease`,
`DirectoryVisit`, `DirectoryFinish`, `Forget`, `CloseDirectory`
([`phase.rs`](../../../../engine/crates/layerfs-mount-ports/src/phase.rs)).
The running callback's `Phases` sits in a thread-local that only the FUSE
`Timed` guard sets
([`accounting.rs`](../../../../engine/crates/layerfs-fuse/src/request/accounting.rs)),
so NFS calls charge nothing.

### Coherence

```text
 request A (mutation)                 request B (read)
   mutate_visit -> owner job commits publication P_A (ticket T_A)
   reply A composed from P_A          observe_visit after B arrived:
   reply A attempted                  sees P_A if it ran after it
   reply_attempted(T_A)  <- only after A's reply attempt
 kernel drops an attribute reply sampled before a newer inode version
```

| Rule | FUSE | NFS | Source |
| --- | --- | --- | --- |
| Replies come from published state | `published(value, expected)`; `Incoherent::Unpublished` or `Foreign` refuses the reply | Reply only when the published stat names the expected serial; else `NFS3ERR_IO`, mutation retained | [`coherence/attributes.rs`](../../../../engine/crates/layerfs-fuse/src/coherence/attributes.rs), [`change.rs`](../../../../engine/crates/layerfs-nfs/src/change.rs) |
| Entry and attribute lifetime | `TTL` 60 s; bounds how long the kernel answers alone, never makes an answer correct | `actimeo=60` and close-to-open | [`reply.rs`](../../../../engine/crates/layerfs-fuse/src/request/reply.rs), [`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs) |
| Page cache on open | `FOPEN_KEEP_CACHE` on, `FOPEN_DIRECT_IO` off | client cache | [`coherence/pages.rs`](../../../../engine/crates/layerfs-fuse/src/coherence/pages.rs) |
| WRITE count | Full count or an error, never short | Short only after a later window failed; earlier windows stay published | [`coherence/pages.rs`](../../../../engine/crates/layerfs-fuse/src/coherence/pages.rs), [`change.rs`](../../../../engine/crates/layerfs-nfs/src/change.rs) |
| Hard links | Aliases share one serial, so one kernel inode | `fileid` is the serial | [`link.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/link.rs), [`attributes.rs`](../../../../engine/crates/layerfs-nfs/src/attributes.rs) |
| Ticket release | After the one reply attempt (fuser reports no send result) | After `write_record` returns | [`reply_order.rs`](../../../../engine/crates/layerfs-fuse/src/coherence/reply_order.rs), [`server.rs`](../../../../engine/crates/layerfs-nfs/src/server.rs) |
| After a Commit installs a root other than the one shown | `inval_inode` / `inval_entry` per stale serial or name before the answer | Mode-preserving SETATTR refresh through the client | [`invalidate.rs`](../../../../engine/crates/layerfs-daemon/src/control/invalidate.rs); [FUSE](fuse.md), [NFS](nfs.md) |
| Durability of a reply | `FLUSH`/`FSYNC`/`FSYNCDIR` answer `ENOSYS` once; the kernel then reports success and stops sending them | WRITE answers `FILE_SYNC`, meaning published; the daemon flushes the client before each Commit | [`flush.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/flush.rs), [`changing.rs`](../../../../engine/crates/layerfs-nfs/src/changing.rs) |

An install of the Workspace's own captured root sends nothing: the tree the
mount shows is unchanged. The overlay is disposable and never synchronized to
stable media.

## Invariants

| Rule | Enforced by |
| --- | --- |
| The kernel decides access: `allow_other,default_permissions`; the library uid filter is off (`SessionACL::All`) | [`syscalls.rs`](../../../../engine/crates/layerfs-fuse/src/mount/syscalls.rs), [`startup.rs`](../../../../engine/crates/layerfs-fuse/src/session/startup.rs) |
| Every inode shows the configured command uid and gid; a chown to anything else is `EPERM`; mode bits beyond `0o7777` and special files are refused | [`unsupported.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/unsupported.rs), [`create.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/create.rs) |
| Stable identity: one serial per inode for the mount's life, across rename, Commit and install; NFS handles are stable per mount and stale across mounts | [`attributes.rs`](../../../../engine/crates/layerfs-fuse/src/attributes.rs), [`handle.rs`](../../../../engine/crates/layerfs-nfs/src/handle.rs) |
| Exact request ownership: each received unit has one disposition; a failure is retained with its future and inputs, never replayed | [`requests.rs`](../../../../engine/crates/layerfs-fuse/src/dispatch/requests.rs), [`failure.rs`](../../../../engine/crates/layerfs-fuse/src/request/failure.rs) |
| Exact open and lookup ownership: OPEN/OPENDIR record a descriptor in their deciding job, RELEASE/RELEASEDIR/FORGET give it back; a stopped fence never refuses disposal | [`ports.rs`](../../../../engine/crates/layerfs-mount-ports/src/ports.rs), [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| A mutation takes effect at the commit of its one owner job; the reply is built from that job's values and a published mutation is never undone | [`reply_order.rs`](../../../../engine/crates/layerfs-fuse/src/coherence/reply_order.rs) |
| READDIR publishes exactly the accepted names as offsets before its reply | [`directory.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/directory.rs) |
| Kernel writeback stays off: `FUSE_WRITEBACK_CACHE` is never selected; every `write(2)` is published before it returns | [`profile.rs`](../../../../engine/crates/layerfs-fuse/src/mount/profile.rs), [engine guide](../../../../engine/AGENTS.md) |
| Only the owner-authorized fuser 0.18.0 patches | [`check_fuser_integrity.py`](../../../../tools/check_fuser_integrity.py); see [FUSE](fuse.md) |
| Filesystem access needs no Exec registration: any process that sees the mount is served the same way | [`ports.rs`](../../../../engine/crates/layerfs-mount-ports/src/ports.rs) (no command crosses `RequestServices`), [engine guide](../../../../engine/AGENTS.md) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| READ and WRITE window | `READ_WINDOW` 128 KiB, `WRITE_WINDOW` 256 KiB; FUSE negotiates `READ_WINDOW` as `max_write`; larger is `EINVAL` | [`types.rs`](../../../../engine/crates/layerfs-overlay/src/contract/types.rs), [`callbacks.rs`](../../../../engine/crates/layerfs-fuse/src/request/callbacks.rs) |
| FUSE input per request | `MAX_INPUT_BYTES` = `READ_WINDOW` + 510; more is `ENOTCONN` | [`types.rs`](../../../../engine/crates/layerfs-fuse/src/dispatch/types.rs) |
| Name length | 255 bytes; longer is `ENAMETOOLONG` / `NFS3ERR_NAMETOOLONG` | [`callbacks.rs`](../../../../engine/crates/layerfs-fuse/src/request/callbacks.rs), [`change.rs`](../../../../engine/crates/layerfs-nfs/src/change.rs) |
| Symlink target | Shorter than one page (FUSE: negotiated page size; NFS: `LINK_PAGE` 4096); else `ENAMETOOLONG` | [`create.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/create.rs), [`change.rs`](../../../../engine/crates/layerfs-nfs/src/change.rs) |
| Serial allocator contended | `EAGAIN` / `NFS3ERR_JUKEBOX`; nothing created | [`attributes.rs`](../../../../engine/crates/layerfs-fuse/src/attributes.rs), [`change.rs`](../../../../engine/crates/layerfs-nfs/src/change.rs) |
| Overlay full | `ENOSPC` / `NFS3ERR_NOSPC`, only for a known rollback | [`mutation.rs`](../../../../engine/crates/layerfs-mount-ports/src/operations/mutation.rs) |
| Failed base read of one request | `EIO` for that request; the mount keeps serving; recorded in `FailedDemands` | [`terminal.rs`](../../../../engine/crates/layerfs-fuse/src/request/terminal.rs) |
| Forced teardown | Waiting requests answer `ENOTCONN` | [`terminal.rs`](../../../../engine/crates/layerfs-fuse/src/request/terminal.rs) |
| STATFS / FSSTAT | Fixed declared values (2^30 blocks and files), not capacity | [`inline.rs`](../../../../engine/crates/layerfs-fuse/src/request/inline.rs), [`nfs.rs`](../../../../engine/crates/layerfs-nfs/src/nfs.rs) |
| NFS record and WRITE | `RECORD_LIMIT` 2 MiB per RPC record; `WRITE_LIMIT` 1 MiB per WRITE call | [`rpc.rs`](../../../../engine/crates/layerfs-nfs/src/rpc.rs), [`changing.rs`](../../../../engine/crates/layerfs-nfs/src/changing.rs) |
| NFS synthetic kernel | 65,536 lookup references (default), 1,024 file and 256 directory descriptors | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| Application WAL SQLite database in an NFS Workspace (macOS) | First query fails `SQLITE_CANTOPEN` (14); convert to `journal_mode=DELETE` before import | [NFS](nfs.md) |
| FUSE route outside Linux | `attributes`, `mount`, `request` and `session` build on Linux only | [`lib.rs`](../../../../engine/crates/layerfs-fuse/src/lib.rs) |

## Demonstration

```text
 kernel_mount / kernel_coherence   (Linux, /dev/fuse, SYS_ADMIN)
   fixture Store + Branch -> daemon Service -> Mount -> Attach (real FUSE)
   -> ordinary fs calls by child commands (uid 65534), never registered
   -> Unmount (detach, drain) -> checks
```

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`kernel_mount.rs`](../../../../engine/crates/layerfs-daemon/tests/kernel_mount.rs) | `cargo +1.85.1 test --locked -p layerfs-daemon --test kernel_mount -- --test-threads=1` | Linux, `/dev/fuse`, `SYS_ADMIN` | The complete root read through ordinary calls with exact modes, times, owners, listings, bytes and link targets; refusals; reversible Busy; full drain; two Workspaces mounted independently |
| [`kernel_coherence.rs`](../../../../engine/crates/layerfs-daemon/tests/kernel_coherence.rs) | `cargo +1.85.1 test --locked -p layerfs-daemon --test kernel_coherence -- --test-threads=1` | Linux, `/dev/fuse`, `SYS_ADMIN` | Shared-mapping stores are published; a LOOKUP racing create, unlink or rename never binds a stale inode; size never shrinks under concurrent append |
| [`requests.rs`](../../../../engine/crates/layerfs-fuse/tests/requests.rs) | `cargo +1.85.1 test --locked -p layerfs-fuse --test requests` | any (no kernel) | A parked request resumes on its own thread and drain waits for it; a panic keeps its future and another mount stays usable |
| [`protocol.rs`](../../../../engine/crates/layerfs-nfs/tests/protocol.rs) | `cargo +1.85.1 test --locked -p layerfs-nfs --test protocol` | any (no network) | XDR round trips and bounds, fragment joining and `RECORD_LIMIT`, `AUTH_UNIX` headers, handles stable per mount and stale after it |
| [`write.rs`](../../../../engine/crates/layerfs-nfs/tests/write.rs) | `cargo +1.85.1 test --locked -p layerfs-nfs --test write` | any (in-process loopback export, no kernel mount) | Every changing NFS procedure; tickets released only after the reply; silly renames and AppleDouble names |

Run each under a wall timeout of at most 120 s. The nightly lane runs each
kernel binary alone in a container with `--device /dev/fuse --cap-add
SYS_ADMIN --security-opt apparmor=unconfined`
([`nightly.yml`](../../../../.github/workflows/nightly.yml)).

## Pages

| Page | What it covers |
| --- | --- |
| [FUSE](fuse.md) | Session startup, drain and forced stop, dispatch, requests and replies, mount syscalls, invalidation, and the authorized fuser 0.18.0 patches. |
| [NFS](nfs.md) | The loopback NFSv3 export: server, RPC/XDR, handles, directories, changes, refresh after install, AppleDouble files, mount options and application SQLite on NFS. |
