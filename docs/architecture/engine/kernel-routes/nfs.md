# NFS

[Architecture](../../README.md) › [Engine](../README.md) › [Kernel routes](README.md) › NFS

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The loopback NFSv3 route.

| | |
| --- | --- |
| Component | [Kernel routes](README.md) |
| Source | [`engine/crates/layerfs-nfs/src`](../../../../engine/crates/layerfs-nfs/src) |

`layerfs-nfs` serves one mounted Workspace as an NFSv3 export on a loopback
TCP port, over the same [ports](ports.md) and operations FUSE uses. It has no
third-party dependency: XDR, RPC record marking, NFS and MOUNT are written
over `std::net` ([`lib.rs`](../../../../engine/crates/layerfs-nfs/src/lib.rs)).
It has no platform `cfg`. The daemon starts it and has the host's own NFS
client mount it ([`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs)).

## Server

```text
 host NFS client (mount_nfs on macOS)
      |  TCP 127.0.0.1:<port>   MOUNT 100005 v3 and NFS 100003 v3
      v
 +---------------------------------------------------------------+
 | Server                                                        |
 |  thread layerfs-nfs-accept                                    |
 |  thread layerfs-nfs-connection  (one per TCP connection)      |
 |    loop:  read_record -> answer(call) -> write_record         |
 |           -> Tickets::replied   (after the one send attempt)  |
 |                                                               |
 |  Kernel  (synthetic kernel, shared by all connections)        |
 |    SqliteHandles   Table{lookups, parents, files,             |
 |    directories, silly, verifiers, names, used, refreshing}    |
 |    Companions (AppleDouble)                                   |
 +---------------------------------------------------------------+
      |  MountServices::request(&Fence) -> RequestServices
      v
 daemon Mounted  -> Workspace owner jobs, Store readers
```

| Item | Meaning | Source |
| --- | --- | --- |
| `Server::start(listener, services, mount, config)` | Starts the accept thread. One connection serves one call at a time, in arrival order. | [`server.rs`](../../../../engine/crates/layerfs-nfs/src/server.rs) |
| `ExportConfig` | `path` (normally `/`), `uid`/`gid` shown on every inode, `read_only`, `refuse_apple_double`, `lookups_held`. `writable(uid, gid)` is the default. | [`server.rs`](../../../../engine/crates/layerfs-nfs/src/server.rs) |
| `Server::stop` | Stop accepting, shut down and join every connection, then `Kernel::release`: descriptors first, then lookup references. The engine mount stays with the caller. | [`server.rs`](../../../../engine/crates/layerfs-nfs/src/server.rs), [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| `Kernel` | What a FUSE kernel would hold for a stateless client: one lookup reference per handed-out serial, implicit file and directory descriptors, a parent index for `..`. | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| Verifier | One WRITE/COMMIT verifier per export, from the clock and process id under a random key. | [`server.rs`](../../../../engine/crates/layerfs-nfs/src/server.rs) |
| `Counters` | Calls and nanoseconds per NFS and MOUNT procedure. | [`server.rs`](../../../../engine/crates/layerfs-nfs/src/server.rs) |

Port futures run to completion on the connection thread; a future that is
not ready parks the thread until woken
([`block.rs`](../../../../engine/crates/layerfs-nfs/src/block.rs)). Every
NFSv3 procedure is served. A read-only export answers each changing procedure
`NFS3ERR_ROFS` ([`nfs.rs`](../../../../engine/crates/layerfs-nfs/src/nfs.rs)).

## RPC and XDR

```text
 TCP record marking (one record, possibly several fragments)
 +----------------------+------------------------+--------------
 | 4 bytes: L | len     | fragment bytes (len)   | next fragment
 +----------------------+------------------------+--------------
   L = last-fragment bit 31          record total <= RECORD_LIMIT

 call header (XDR, big-endian u32 words)
 +-----+------+---------+------+------+------+------------+--------
 | xid | CALL | rpcvers | prog | vers | proc | cred flavor| cred..
 +-----+------+---------+------+------+------+------------+--------
   then verifier flavor + body, then the procedure arguments
```

| Case | Answer | Source |
| --- | --- | --- |
| Not a call, or undecodable header | No reply | [`rpc.rs`](../../../../engine/crates/layerfs-nfs/src/rpc.rs) |
| `rpcvers != 2` | `MSG_DENIED` / `RPC_MISMATCH` 2..2 | [`rpc.rs`](../../../../engine/crates/layerfs-nfs/src/rpc.rs) |
| Flavor not `AUTH_NONE` or `AUTH_UNIX`, or a bad `AUTH_UNIX` body | `MSG_DENIED` / `AUTH_ERROR` `AUTH_TOOWEAK` | [`rpc.rs`](../../../../engine/crates/layerfs-nfs/src/rpc.rs) |
| `AUTH_UNIX` with more than 16 groups | Rejected as above | [`rpc.rs`](../../../../engine/crates/layerfs-nfs/src/rpc.rs) |
| Unknown program; wrong version | `PROG_UNAVAIL`; `PROG_MISMATCH` 3..3 | [`server.rs`](../../../../engine/crates/layerfs-nfs/src/server.rs) |
| Unknown procedure; bad arguments | `PROC_UNAVAIL`; `GARBAGE_ARGS` | [`nfs.rs`](../../../../engine/crates/layerfs-nfs/src/nfs.rs) |
| Record over `RECORD_LIMIT` (2 MiB) | Read error; the connection ends | [`rpc.rs`](../../../../engine/crates/layerfs-nfs/src/rpc.rs) |

`Reader` bounds-checks every read and every declared length; `Writer` pads
opaque data to four bytes
([`xdr.rs`](../../../../engine/crates/layerfs-nfs/src/xdr.rs)). A reply is
written as one last fragment.

## Handles

```text
 byte  0      1 ............ 8   9 ........... 16  17 ........... 24
      +-----+------------------+-----------------+------------------+
      | ver | mount owner (BE) |   serial (BE)   |   tag (BE)       |
      |  1  |      u64         |      u64        | SipHash(ver,     |
      |     |                  |                 |  owner, serial)  |
      +-----+------------------+-----------------+------------------+
                       HANDLE_BYTES = 25   (NFSv3 limit is 64)
```

| Decode result | When | Status | Source |
| --- | --- | --- | --- |
| serial | length 25, version 1, owner matches, serial non-zero, tag matches | — | [`handle.rs`](../../../../engine/crates/layerfs-nfs/src/handle.rs) |
| `HandleError::Bad` | wrong length, version or tag; serial 0 | `NFS3ERR_BADHANDLE` | [`handle.rs`](../../../../engine/crates/layerfs-nfs/src/handle.rs) |
| `HandleError::Stale` | owner of an earlier engine mount | `NFS3ERR_STALE` | [`handle.rs`](../../../../engine/crates/layerfs-nfs/src/handle.rs) |
| collected serial | its remembered name is looked up again (parent first); the same serial continues, another or none is stale | `NFS3ERR_STALE` if not found | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |

The same serial always encodes to the same bytes for one mount. The tag key
is drawn once per export with std `RandomState`; it makes guessing costly and
does not authenticate the caller. `fileid` in every `fattr3` is the serial
itself ([`attributes.rs`](../../../../engine/crates/layerfs-nfs/src/attributes.rs)).

| Synthetic-kernel bound | Value | On reaching it | Source |
| --- | --- | --- | --- |
| `LOOKUPS_HELD` (default `lookups_held`) | 65,536 | References are forgotten, least recently used first, down to three quarters of the bound; a serial with a descriptor, a silly name or a held child is skipped; names are kept | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| `RECENT_KEPT` | 64 most recent uses | Never collected | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| `FILES_HELD`, `DIRECTORIES_HELD` | 256 each | Least recently used descriptor closed | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| `VERIFIERS_HELD` | 1024 | EXCLUSIVE CREATE verifiers remembered for retransmission | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| `NAMES_HELD` | 4 Mi names | No further new name is remembered; a collected serial with no name is stale | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |

## Directories

```text
 READDIR / READDIRPLUS (handle, cookie, cookieverf, budget)
   cookie 0 -----> open_directory: fresh descriptor; verifier = its id
   cookie > 0 ---> directory(serial, verifier) still open?
                     no -> NFS3ERR_BAD_COOKIE (client lists from 0)
   cookies:  1 = ".", 2 = "..", then engine-published offsets
   one reply = at most one engine window:
     entries that fit the byte budget -> publish_cookies(those names)
     names left -> eof = false;  no names left -> eof = true
```

| Rule | Source |
| --- | --- |
| The NFS cookie is the engine's kernel offset on one implicit directory descriptor named by `cookieverf`. | [`directory.rs`](../../../../engine/crates/layerfs-nfs/src/directory.rs) |
| An offset a rewind retired is `NFS3ERR_BAD_COOKIE`. | [`directory.rs`](../../../../engine/crates/layerfs-nfs/src/directory.rs) |
| READDIRPLUS entries carry no attributes and no handle; the client looks each name up. | [`directory.rs`](../../../../engine/crates/layerfs-nfs/src/directory.rs) |
| `..` comes from the parent recorded when a directory was looked up, created, moved or listed; the engine resolves names only downward. | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| Silly names and AppleDouble companions are never listed. | [`names.rs`](../../../../engine/crates/layerfs-nfs/src/names.rs), [`apple_double.rs`](../../../../engine/crates/layerfs-nfs/src/apple_double.rs) |

## Change and refresh

```text
 connection thread, one call
   changing procedure
     Kernel::mutate -> KernelMutation::perform        (mount ports)
       Declined::Contended  -> NFS3ERR_JUKEBOX (client retries later)
       decided              -> pushed to this call's Tickets
       failure              -> fenced/base demand: relinquish
                               otherwise: retained in the Kernel
   reply bytes built (wcc_data: pre-op and post-op directory attrs)
   write_record(reply)            the single send attempt
   Tickets::replied               reply_attempted for each ticket
```

| Rule | Source |
| --- | --- |
| A publication ticket is released only after the reply's one send attempt, whether or not it reached the client. | [`server.rs`](../../../../engine/crates/layerfs-nfs/src/server.rs), [`change.rs`](../../../../engine/crates/layerfs-nfs/src/change.rs) |
| WRITE splits into `WRITE_WINDOW` (128 KiB) publications; a failure after the first window is a short write of the windows already published. One call carries at most 1 MiB. | [`change.rs`](../../../../engine/crates/layerfs-nfs/src/change.rs), [`changing.rs`](../../../../engine/crates/layerfs-nfs/src/changing.rs) |
| WRITE answers `FILE_SYNC`: every window was published before the reply. That is publication, not durability. | [`changing.rs`](../../../../engine/crates/layerfs-nfs/src/changing.rs) |
| `wcc_data` pre- and post-op reads are separate visits, not atomic with the change. | [`changing.rs`](../../../../engine/crates/layerfs-nfs/src/changing.rs) |
| A client silly rename (`.nfs` + 24 hex digits on Linux, `.nfs.` + 8 hex + `.` + 4 hex on macOS) is served as an UNLINK of the source; the export keeps the inode by its own reference and descriptor until the final REMOVE. | [`names.rs`](../../../../engine/crates/layerfs-nfs/src/names.rs) |
| A serial whose last name this export removed gives back its reference and descriptor at once; its handle is stale afterwards. | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| SETATTR's guard compares a fresh GETATTR ctime first; the check is not atomic with the change. Access time is decoded and dropped. | [`change.rs`](../../../../engine/crates/layerfs-nfs/src/change.rs) |
| ACCESS: a caller with the inode's uid, or uid 0, gets the owner bits; a caller in its group the group bits; others the other bits. A read-only export grants no change. | [`attributes.rs`](../../../../engine/crates/layerfs-nfs/src/attributes.rs) |
| MKNOD answers `NFS3ERR_NOTSUPP`. | [`lib.rs`](../../../../engine/crates/layerfs-nfs/src/lib.rs) |

Refresh after an install is covered in
[Coherence: Invalidation](coherence.md#invalidation).

## AppleDouble files

```text
 CREATE "._name" (refuse_apple_double = true)
   regular file --> Companions (export memory only)
                    serial >= SERIALS = 1 << 62
                    LOOKUP GETATTR ACCESS READ WRITE SETATTR COMMIT
                    REMOVE, RENAME to another "._" name: served here
                    READDIR: not listed;  engine: never called
   any other kind (dir, symlink, link, rename target) --> NFS3ERR_ACCES
```

| Bound | Value | Over the bound | Source |
| --- | --- | --- | --- |
| `HELD` | 4096 companions | CREATE `NFS3ERR_ACCES` | [`apple_double.rs`](../../../../engine/crates/layerfs-nfs/src/apple_double.rs) |
| `FILE_BYTES` | 1 MiB per companion | WRITE `NFS3ERR_FBIG` | [`apple_double.rs`](../../../../engine/crates/layerfs-nfs/src/apple_double.rs) |
| `TOTAL_BYTES` | 64 MiB in all | WRITE `NFS3ERR_NOSPC` | [`apple_double.rs`](../../../../engine/crates/layerfs-nfs/src/apple_double.rs) |

On NFSv3 the macOS client stores extended attributes and resource forks in
`._<name>` files. Keeping them in export memory lets `cp`, `copyfile`,
`ditto` and `tar` succeed while nothing reaches the Workspace or a Commit.
Everything is dropped when the export is released. A name is AppleDouble when
it starts with `._` and has at least one more byte.

## Mount

```text
 daemon Attach (NFS route)
   bind 127.0.0.1:0 -> Server::start(...)        port P
   helper: mount_nfs -o vers=3,tcp,port=P,mountport=P,... 127.0.0.1:/ <dir>
     client --MOUNT MNT "/"--> root handle + AUTH_UNIX
 daemon Commit:   flush helper (sync(2) by default), close_files, Commit
 daemon Unmount:  unmount helper (busy is reversible), Server::stop,
                  revoke the engine mount, Close
```

| MOUNT procedure | Answer | Source |
| --- | --- | --- |
| 0 `NULL`, 4 `UMNTALL` | Empty success | [`mount.rs`](../../../../engine/crates/layerfs-nfs/src/mount.rs) |
| 1 `MNT` | The configured path (trailing slashes ignored): `MNT3_OK`, the root handle, flavor `AUTH_UNIX`; any other path `MNT3ERR_NOENT` | [`mount.rs`](../../../../engine/crates/layerfs-nfs/src/mount.rs) |
| 2 `DUMP` | Empty list | [`mount.rs`](../../../../engine/crates/layerfs-nfs/src/mount.rs) |
| 3 `UMNT` | Path decoded, success | [`mount.rs`](../../../../engine/crates/layerfs-nfs/src/mount.rs) |
| 5 `EXPORT` | One export, the configured path, no groups | [`mount.rs`](../../../../engine/crates/layerfs-nfs/src/mount.rs) |

The client mount options are the daemon's `Mounter::host`. On macOS:
`vers=3,tcp,port={port},mountport={port},locallocks,nfc,noatime,actimeo=60,rsize=131072,wsize=131072,soft,intr,dumbtimer,timeo=300,retrans=6,mutejukebox,nobrowse`;
elsewhere `vers=3,proto=tcp,port={port},mountport={port},mountproto=tcp,nolock,actimeo=60`
([`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs)).
Helper deadlines and teardown belong to the
[daemon](../daemon/README.md).

## Related

- [Ports](ports.md): the operations the export drives.
- [Coherence](coherence.md): client caches, refresh after install, writeback.
- [Daemon: Control calls](../daemon/control-calls.md): Attach, Commit and
  Unmount on the NFS route.
- [Support matrix](../../connect/support-matrix/README.md): which platform
  uses this route.
- [Glossary](../../glossary.md).

---

← [Kernel routes: fuser patches](fuser-patches.md) · [Up: Kernel routes](README.md) · [Kernel routes: Coherence](coherence.md) →
