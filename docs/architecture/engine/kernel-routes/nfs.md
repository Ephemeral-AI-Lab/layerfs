# NFS

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The loopback NFSv3 route of [Kernel routes](README.md), used by the macOS native platform.

| | |
| --- | --- |
| Source | [`layerfs-nfs/src`](../../../../engine/crates/layerfs-nfs/src), daemon side [`control/export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs) |

`layerfs-nfs` serves one mounted Workspace as an NFSv3 export on a loopback
TCP port, over the same mount ports FUSE uses. XDR, RPC record marking, NFS
and MOUNT are written over `std::net` with no third-party dependency and no
platform `cfg` ([`lib.rs`](../../../../engine/crates/layerfs-nfs/src/lib.rs)).
The daemon starts it and has the host's own NFS client mount it.

## Server (`server.rs`, `kernel.rs`, `block.rs`)

```text
 host NFS client (mount_nfs on macOS)
      | TCP 127.0.0.1:<port>   MOUNT 100005 v3, NFS 100003 v3
 +----v-----------------------------------------------------------+
 | Server                                                         |
 |  thread layerfs-nfs-accept                                     |
 |  thread layerfs-nfs-connection (one per TCP connection)        |
 |    Records::read -> answer(call) -> write_record -> Tickets::replied
 |  Kernel (synthetic kernel, shared by all connections)          |
 |    lookups, parents, files, directories, silly names,          |
 |    verifiers, names, refreshing;  Companions (AppleDouble)     |
 +----+-----------------------------------------------------------+
      | MountServices::request(&Fence) -> RequestServices
 daemon Mounted -> Workspace owner jobs, Store readers
```

| Item | Meaning | Source |
| --- | --- | --- |
| `Server::start` | Starts the accept thread; one connection serves one call at a time, in arrival order | [`server.rs`](../../../../engine/crates/layerfs-nfs/src/server.rs) |
| `ExportConfig` | `path` (normally `/`), `uid`/`gid` shown on every inode, `read_only`, `refuse_apple_double`, `lookups_held`; `writable(uid, gid)` is the default | [`server.rs`](../../../../engine/crates/layerfs-nfs/src/server.rs) |
| `Server::stop` | Stop accepting, shut down and join every connection, then `Kernel::release`: descriptors first, then lookup references. The engine mount stays with the caller | [`server.rs`](../../../../engine/crates/layerfs-nfs/src/server.rs) |
| `Kernel` | What a FUSE kernel would hold for a stateless client: one lookup reference per handed-out serial, implicit file and directory descriptors with the facts last published through them, a parent index for `..` | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| Verifier | One WRITE/COMMIT verifier per export, from the clock and process id under a random key | [`server.rs`](../../../../engine/crates/layerfs-nfs/src/server.rs) |
| Futures | Run to completion on the connection thread; not ready parks the thread until woken | [`block.rs`](../../../../engine/crates/layerfs-nfs/src/block.rs) |

Every NFSv3 procedure is served; MKNOD answers `NFS3ERR_NOTSUPP`, and a
read-only export answers each changing procedure `NFS3ERR_ROFS`
([`nfs.rs`](../../../../engine/crates/layerfs-nfs/src/nfs.rs)). FSINFO declares
`TRANSFER_WINDOW` = 256 KiB, asserted equal to the overlay `WRITE_WINDOW`.

## RPC and XDR (`rpc.rs`, `xdr.rs`)

```text
 record: [4 bytes: last-bit 31 | len][len bytes] ... total <= RECORD_LIMIT
 call:   xid | CALL | rpcvers | prog | vers | proc | cred | verf | args
```

| Case | Answer | Source |
| --- | --- | --- |
| Not a call, or undecodable header | No reply | [`rpc.rs`](../../../../engine/crates/layerfs-nfs/src/rpc.rs) |
| `rpcvers != 2` | `MSG_DENIED` / `RPC_MISMATCH` 2..2 | [`rpc.rs`](../../../../engine/crates/layerfs-nfs/src/rpc.rs) |
| Flavor not `AUTH_NONE`/`AUTH_UNIX`, bad body, or more than 16 groups | `MSG_DENIED` / `AUTH_ERROR` `AUTH_TOOWEAK` | [`rpc.rs`](../../../../engine/crates/layerfs-nfs/src/rpc.rs) |
| Unknown program; wrong version | `PROG_UNAVAIL`; `PROG_MISMATCH` 3..3 | [`server.rs`](../../../../engine/crates/layerfs-nfs/src/server.rs) |
| Unknown procedure; bad arguments | `PROC_UNAVAIL`; `GARBAGE_ARGS` | [`nfs.rs`](../../../../engine/crates/layerfs-nfs/src/nfs.rs) |
| Record over `RECORD_LIMIT` (2 MiB) | Read error; the connection ends | [`rpc.rs`](../../../../engine/crates/layerfs-nfs/src/rpc.rs) |

`Reader` bounds-checks every read and declared length; `Writer` pads opaque
data to four bytes. A reply is written as one last fragment.

A call is framed from the bytes one `read` delivered: the connection keeps them
in [`rpc::Records`](../../../../engine/crates/layerfs-nfs/src/rpc.rs), so a call
whose fragments arrived together costs one socket read. A record larger than
the 8 KiB read-ahead (a WRITE of a transfer window) is read into a buffer of its
own, so its bytes are not copied twice. Those counts are the case's own and are
reproducible on any platform
([`tests/protocol.rs`](../../../../engine/crates/layerfs-nfs/tests/protocol.rs)):
one read per call that arrived in one piece, two for a record past the
read-ahead. A blocking read a signal interrupts is retried, as `read_exact`
does, and the bytes already received stay buffered.

## Handles (`handle.rs`)

```text
 byte 0     1 ......... 8  9 ........ 16  17 ........ 24
     | ver | mount owner  |   serial     | tag = SipHash(ver,owner,serial)
     |  1  |   u64 BE     |   u64 BE     |   u64 BE
                    HANDLE_BYTES = 25   (NFSv3 limit 64)
```

| Decode result | When | Status |
| --- | --- | --- |
| serial | length 25, version 1, owner matches, serial non-zero, tag matches | — |
| `HandleError::Bad` | wrong length, version or tag; serial 0 | `NFS3ERR_BADHANDLE` |
| `HandleError::Stale` | owner of an earlier engine mount | `NFS3ERR_STALE` |
| collected serial | its remembered name is looked up again, parent first; another serial or none is stale | `NFS3ERR_STALE` |

The same serial always encodes to the same bytes for one mount. The tag key is
drawn once per export; it makes guessing costly and does not authenticate.
`fileid` in every `fattr3` is the serial
([`attributes.rs`](../../../../engine/crates/layerfs-nfs/src/attributes.rs)).

| Synthetic-kernel bound | Value | On reaching it | Source |
| --- | --- | --- | --- |
| `LOOKUPS_HELD` (default `lookups_held`) | 65,536 | Least recently used references forgotten down to three quarters; a serial with a descriptor, silly name or held child is skipped; names kept | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| `RECENT_KEPT` | 64 most recent uses | Never collected | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| `FILES_HELD`, `DIRECTORIES_HELD` | 1,024 files, 256 directories | Least recently used descriptor closed | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| `VERIFIERS_HELD` | 1,024 | EXCLUSIVE CREATE verifiers kept for retransmission | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| `NAMES_HELD` | 4 Mi names | No new name remembered; a collected serial with no name is stale | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |

## Directories (`directory.rs`)

```text
 READDIR / READDIRPLUS (handle, cookie, cookieverf, budget)
   cookie 0  -> open_directory: fresh descriptor; verifier = its id
   cookie >0 -> descriptor (serial, verifier) still open? no: BAD_COOKIE
   cookies: 1 = ".", 2 = "..", then engine-published offsets
   one reply = at most one engine window; entries that fit the budget
   -> publish_cookies(those names); eof only when the window ended and fit
```

| Rule | Source |
| --- | --- |
| The cookie is the engine's kernel offset on one implicit directory descriptor named by `cookieverf`; a retired offset or a listing continued after an install is `NFS3ERR_BAD_COOKIE`, and the client lists again | [`directory.rs`](../../../../engine/crates/layerfs-nfs/src/directory.rs), [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| A READDIRPLUS entry carries the facts its listing already read and its handle; each handle takes its one lookup reference in the publishing visit | [`kernel_directory.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_directory.rs), [`kernel_cookie.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/kernel_cookie.rs) |
| `.` and `..` carry no attributes; `..` comes from the parent recorded at lookup, create, move or listing | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| Silly names and AppleDouble companions are never listed | [`names.rs`](../../../../engine/crates/layerfs-nfs/src/names.rs), [`apple_double.rs`](../../../../engine/crates/layerfs-nfs/src/apple_double.rs) |

## Changes (`change.rs`, `changing.rs`)

```text
 changing procedure -> Kernel::mutate -> KernelMutation::perform
   Declined::Contended -> NFS3ERR_JUKEBOX (client retries later)
   decided             -> ticket pushed to this call's Tickets
   failure             -> fenced/base demand: relinquish; else retained
 reply (wcc_data pre/post from the job) -> write_record -> Tickets::replied
```

| Rule | Source |
| --- | --- |
| A publication ticket is released only after the reply's one send attempt, whether or not it reached the client | [`server.rs`](../../../../engine/crates/layerfs-nfs/src/server.rs) |
| A WRITE is split into `WRITE_WINDOW` (256 KiB) windows, each one publication; a failure after the first is a short write of the windows already published. WRITE answers `FILE_SYNC`: published, not durable | [`change.rs`](../../../../engine/crates/layerfs-nfs/src/change.rs), [`changing.rs`](../../../../engine/crates/layerfs-nfs/src/changing.rs) |
| `wcc_data` pre- and post-op attributes come from the publishing job's own inode state, RENAME included; no reply reads an attribute outside the job | [`changing.rs`](../../../../engine/crates/layerfs-nfs/src/changing.rs), [`kernel_visit.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_visit.rs) |
| RENAME looks nothing up first; a detached (`nlink == 0`) destination's reference and descriptor are retired inside that job | [`change.rs`](../../../../engine/crates/layerfs-nfs/src/change.rs) |
| COMMIT answers the attributes last published or observed through the implicit descriptor, so `fsync` costs no owner job | [`change.rs`](../../../../engine/crates/layerfs-nfs/src/change.rs) |
| A client silly rename (`.nfs` + 24 hex on Linux; `.nfs.` + 8 hex + `.` + 4 hex on macOS) is served as an UNLINK; the export keeps the inode until the final REMOVE | [`names.rs`](../../../../engine/crates/layerfs-nfs/src/names.rs) |
| A serial whose last name this export removed gives back its reference and descriptor at once | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| SETATTR's guard compares a fresh GETATTR ctime, not atomic with the change; access time is dropped | [`change.rs`](../../../../engine/crates/layerfs-nfs/src/change.rs) |
| ACCESS grants owner bits to the inode's uid or uid 0, group bits to its group, else other bits; a read-only export grants no change | [`attributes.rs`](../../../../engine/crates/layerfs-nfs/src/attributes.rs) |

## Refresh after install (`refresh.rs`, daemon `export.rs`, `invalidate.rs`)

```text
 Commit installed R' while the export showed R  (R' != R)
   Server::installed(): close file and directory descriptors
   stale_after_install(R, R') -> serials (at most REFRESHED = 4096)
   Server::refresh(serials): mark them, return their paths
   thread layerfs-nfs-refresh: chmod(path, same mode) per path
     client SETATTR -> marked and mode-only? answer current attrs,
     change nothing -> new post-op attrs -> client drops its cache
```

| Rule | Source |
| --- | --- |
| A mark is taken by one mode-only SETATTR and ends after `MARK_LIFE` (30 s); a serial the client never saw is left out; symlinks and vanished paths are skipped | [`refresh.rs`](../../../../engine/crates/layerfs-nfs/src/refresh.rs), [`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs) |
| The refresh never writes into the Workspace and never reverts a mode the install changed | [`refresh.rs`](../../../../engine/crates/layerfs-nfs/src/refresh.rs) |
| `LAYERFS_NFS_REFRESH=0` leaves the client to `actimeo` and close-to-open | [`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs) |

## AppleDouble files (`apple_double.rs`)

On NFSv3 the macOS client keeps extended attributes and resource forks in
`._<name>` files. With `refuse_apple_double`, a regular `._` file (at least
one byte after `._`) lives in export memory only, at serial
`SERIALS` (2^62) and above: LOOKUP, GETATTR, ACCESS, READ, WRITE, SETATTR,
COMMIT, REMOVE and RENAME to another `._` name are served there, READDIR
never lists it and the engine is never called. Any other kind is
`NFS3ERR_ACCES`. `cp`, `copyfile`, `ditto` and `tar` succeed while nothing
reaches the Workspace or a Commit; everything is dropped with the export.

| Bound | Value | Over the bound |
| --- | --- | --- |
| `HELD` | 4,096 companions | CREATE `NFS3ERR_ACCES` |
| `FILE_BYTES` | 1 MiB per companion | WRITE `NFS3ERR_FBIG` |
| `TOTAL_BYTES` | 64 MiB in all | WRITE `NFS3ERR_NOSPC` |

## Mount (`mount.rs`, daemon `export.rs`)

```text
 Attach:       bind 127.0.0.1:0 -> Server::start -> port P
               helper mount_nfs -o vers=3,tcp,port=P,mountport=P,... <dir>
               client MOUNT MNT "/" -> root handle, AUTH_UNIX
               read host mount table once -> tracked: is <dir> listed?
 Commit:       flush helper (else the export volume's own flush: sync_volume_np
               on macOS, sync(2) elsewhere), close_files, Commit
 Unmount:      unmount helper (busy is reversible); failure or deadline
               settled by the mount table; Server::stop, revoke, Close
 ForceUnmount: one forced client unmount (umount -f, or umount -f -l on
               Linux), then the same teardown
```

| Mount-table and helper rule | Source |
| --- | --- |
| Every step above is one attempt; the mount table is the only witness for a failed or timed-out unmount helper. | [`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs) |
| Attach reads the host's mount table once after the mount helper returns: one `/sbin/mount` run (macOS) or `/proc/self/mountinfo` read (Linux) per Attach; unmount timing is unaffected. | `Exported::tracked` in [`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs) |
| The mount point itself is never resolved (a lookup through a wedged mount can block or fail); `listed` compares the configured path and the same name below the canonical mount home. | [`export_helper.rs`](../../../../engine/crates/layerfs-daemon/src/control/export_helper.rs) `listed` |
| A failed or timed-out unmount helper completes teardown only when Attach saw it listed and the table no longer does; a still-listed mount or unreadable table keeps `Retained(Detach)` and the export serving. | `detached_export` in [`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs) |
| A helper past `KILL` and `REAP_WAIT` is reaped in the background by the shared retained queue, with nothing waiting for it or signalling twice. | [`process/retained.rs`](../../../../engine/crates/layerfs-bridge/src/process/retained.rs) |

| MOUNT procedure | Answer |
| --- | --- |
| 0 `NULL`, 4 `UMNTALL` | Empty success |
| 1 `MNT` | The configured path (trailing slashes ignored): `MNT3_OK`, root handle, `AUTH_UNIX`; else `MNT3ERR_NOENT` |
| 2 `DUMP`; 3 `UMNT`; 5 `EXPORT` | Empty list; success; one export with no groups |

Client options are the daemon's `Mounter::host`. macOS:
`vers=3,tcp,port={port},mountport={port},locallocks,nfc,noatime,actimeo=60,rsize=262144,wsize=262144,soft,intr,dumbtimer,timeo=300,retrans=6,mutejukebox,nobrowse`
(`rsize`/`wsize` are `TRANSFER_WINDOW`); elsewhere
`vers=3,proto=tcp,port={port},mountport={port},mountproto=tcp,nolock,actimeo=60`
([`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs)).
An NFS ready receipt has `abi_major == 0` and its `mount_id` is the port.
Forced teardown on this route is in
[Daemon: Control calls](../daemon/control-calls.md#force).

## Application SQLite databases on the NFS route

```text
 local APFS: app.db (WAL, closed) --import--> Workspace (bytes kept)
 macOS SQLite in the NFS mount: VFS picks nfsIoMethods (no xShmMap)
   first query -> SQLITE_CANTOPEN (14)       open alone may succeed
```

| Case in an NFS Workspace | Result |
| --- | --- |
| A WAL database imported closed (no `-wal`/`-shm` needed) | First query fails `SQLITE_CANTOPEN` (14); not POSIX errno 14 |
| The same database converted to rollback journal before import | Works |
| A new database asking `journal_mode=WAL` | SQLite answers `delete`; simple reads and writes work |

macOS system SQLite selects an NFS I/O method table without a shared-memory
WAL index callback for an `nfs` filesystem; `locallocks` on the mount and the
export's handling of `-shm` files cannot supply it. Import preserves
application bytes and converts nothing. Before import, with every process
using the database stopped and a backup kept, run on the original local
filesystem and check that it reports no busy readers or writers and returns
`delete`:

```sh
sqlite3 -bail /absolute/local/path/application.db \
  'PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode=DELETE;'
```

Never delete a `-wal` file or edit the header by hand. This concerns
application databases inside the Workspace only; the Store stays on its
supported local filesystem ([Store](../store/README.md)).

## Related

- [Kernel routes](README.md): ports, operations and coherence rules.
- [Daemon: Control calls](../daemon/control-calls.md): Attach, Commit and Unmount on the NFS route.
- [Control: Machines](../../control/machines/README.md): which platform uses this route.
