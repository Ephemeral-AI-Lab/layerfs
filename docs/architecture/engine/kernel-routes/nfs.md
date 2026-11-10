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
| `ExportConfig` | `path` (normally `/`), `uid`/`gid` shown on every inode, `read_only`, `refuse_apple_double`, `lookups_held`, `companion_spill_dir`, `companion_resident_bytes`, `companions_held`. `writable(uid, gid)` is the default. | [`server.rs`](../../../../engine/crates/layerfs-nfs/src/server.rs) |
| `Server::stop` | Stop accepting, shut down and join every connection, then `Kernel::release`: descriptors first, then lookup references. The engine mount stays with the caller. | [`server.rs`](../../../../engine/crates/layerfs-nfs/src/server.rs), [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| `Kernel` | What a FUSE kernel would hold for a stateless client: one lookup reference per handed-out serial, implicit file and directory descriptors with the facts last published through each file descriptor, a parent index for `..`. | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
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
| `FILES_HELD`, `DIRECTORIES_HELD` | 1024 files, 256 directories | Least recently used descriptor closed | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
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
     window listed the last name and all of it fit -> eof = true
     otherwise -> eof = false; the next call resumes after its last cookie
```

| Rule | Source |
| --- | --- |
| The NFS cookie is the engine's kernel offset on one implicit directory descriptor named by `cookieverf`. | [`directory.rs`](../../../../engine/crates/layerfs-nfs/src/directory.rs) |
| An offset a rewind retired is `NFS3ERR_BAD_COOKIE`. | [`directory.rs`](../../../../engine/crates/layerfs-nfs/src/directory.rs) |
| A READDIRPLUS entry carries the facts its listing already read (a local inode's current row, or an inherited inode's immutable base stat) and the handle of its serial, so the client needs no LOOKUP per name. | [`directory.rs`](../../../../engine/crates/layerfs-nfs/src/directory.rs), [`kernel_directory.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_directory.rs) |
| Every handle a READDIRPLUS reply hands out takes its one kernel lookup reference in the publishing visit that makes its offset valid; a reply reusing an already published offset range hands out none. | [`kernel_cookie.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/kernel_cookie.rs) |
| `.` and `..` carry no attributes: both name an object whose handle the client already holds. | [`directory.rs`](../../../../engine/crates/layerfs-nfs/src/directory.rs) |
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
| One WRITE call carries at most one `WRITE_WINDOW` (256 KiB, the transfer window FSINFO declares), so its one window is one publication; a call that still carries more arrives as several windows, and a failure after the first is a short write of the windows already published. | [`change.rs`](../../../../engine/crates/layerfs-nfs/src/change.rs), [`changing.rs`](../../../../engine/crates/layerfs-nfs/src/changing.rs) |
| WRITE answers `FILE_SYNC`: every window was published before the reply. That is publication, not durability. | [`changing.rs`](../../../../engine/crates/layerfs-nfs/src/changing.rs) |
| `wcc_data` pre- and post-op attributes come from the publishing visit's own pre- and post-change inode state, for every changing procedure, `RENAME` included: one directory answers both halves of a same-directory move, and no procedure reads an attribute for its reply outside the job. | [`changing.rs`](../../../../engine/crates/layerfs-nfs/src/changing.rs), [`kernel_visit.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_visit.rs) |
| The one change that publishes no engine job, an AppleDouble companion's CREATE, REMOVE or RENAME, answers both halves from the facts this export last published or observed for that directory: a companion lives in the export alone, so it cannot have changed them. Answering both keeps the client's cached attributes of the directory valid; a missing post-operation half makes it re-GETATTR the directory for a later entry. A directory this export never saw still answers both absent. | [`apple_double.rs`](../../../../engine/crates/layerfs-nfs/src/apple_double.rs), [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| RENAME neither looks its source or destination up first nor needs their attributes: the publishing job reports the moved inode and the destination it detached, and the export retires a detached (`nlink == 0`) destination's own reference and descriptor inside that job, so a replaced file is never an orphan of the export's references. | [`change.rs`](../../../../engine/crates/layerfs-nfs/src/change.rs) |
| COMMIT answers the attributes this export last published or observed for the file through its implicit descriptor, so a client's `fsync` costs no owner job; a file this export holds no descriptor for is read. | [`change.rs`](../../../../engine/crates/layerfs-nfs/src/change.rs) |
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
   regular file --> Companions (export index in memory; body resident, spilled or decoded)
                    serial >= SERIALS = 1 << 62
                    main inode known from Table::by_name --> that inode's companion:
                      any name of the inode reads it, RENAME follows the inode,
                      its last name drops it
                    resident body <= RESIDENT_BYTES (32 MiB) in all;
                    older bodies spill to <mounts>/.appledouble/<namespace>/<serial>
                    (dir 0700, file 0600; written as <serial>.<n>.tmp and
                     renamed on adoption; pread/pwrite outside the companion
                     lock, opened by path per call, never held open)
                    a whole AppleDouble image decodes (apple_double_codec):
                      FinderInfo + attributes kept under the main inode's serial,
                      reads synthesize the client's own bytes, takes no HELD slot;
                      <= DECODED_BYTES in all, each <= DECODED_ENTRY_BYTES
                    LOOKUP GETATTR ACCESS READ WRITE SETATTR COMMIT
                    REMOVE, RENAME to another "._" name: served here
                    READDIR: not listed;  engine: never called
   any other kind (dir, symlink, link, rename target) --> NFS3ERR_ACCES
```

| Bound | Value | Over the bound | Source |
| --- | --- | --- | --- |
| `HELD` | 65,536 companion index entries not holding a whole image | Reclaim orphan companions whose main inode (or, for a companion whose main file the export never saw, whose main file) is no longer known; if still at the bound, CREATE answers `NFS3ERR_NOSPC`, increments `names_refused` and logs the export/namespace, current count and limit at most once per 60 seconds per export | [`apple_double.rs`](../../../../engine/crates/layerfs-nfs/src/apple_double.rs) |
| `FILE_BYTES` | 1 MiB per companion | WRITE / SETATTR `NFS3ERR_FBIG` | [`apple_double.rs`](../../../../engine/crates/layerfs-nfs/src/apple_double.rs) |
| `RESIDENT_BYTES` | 32 MiB resident bodies in all | Spill least recently touched resident bodies to `companion_spill_dir` (`<mounts>/.appledouble/<namespace>/<serial>`); host disk I/O errors map to `NFS3ERR_NOSPC` (`ENOSPC`/`EDQUOT`), `NFS3ERR_FBIG` (`EFBIG`), `NFS3ERR_ACCES` (`EACCES`), `NFS3ERR_PERM` (`EPERM`), `NFS3ERR_ROFS` (`EROFS`) or `NFS3ERR_IO` | [`apple_double.rs`](../../../../engine/crates/layerfs-nfs/src/apple_double.rs), [`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs) |
| `DECODED_BYTES` | 512 MiB counted over all decoded companions | A further whole image keeps its bytes as written, and still takes no slot | [`apple_double.rs`](../../../../engine/crates/layerfs-nfs/src/apple_double.rs) |
| `DECODED_ENTRY_BYTES` | 128 KiB counted per decoded companion | That image keeps its bytes as written, and still takes no slot | [`apple_double.rs`](../../../../engine/crates/layerfs-nfs/src/apple_double.rs) |

On NFSv3 the macOS client stores extended attributes and resource forks in
`._<name>` files. Keeping their index in export memory and spilling excess
bodies to the export's scratch directory lets `cp`, `copyfile`, `ditto` and
`tar` succeed without losing companion bytes during the mount, while nothing
reaches the Workspace, SQLite overlay or a Commit.

A companion whose main inode the export knows belongs to that inode rather than
to one name: every hard-link alias reaches it, RENAME of the main file keeps it
reachable under the new name and leaves the inode's other names reaching it,
unlink or a replacing RENAME of the main file only
unbinds the paired `._` name, and the inode's last name (or its silly rename)
drops it. Its FinderInfo and extended attributes are therefore kept once per
inode, not once per name. When a write leaves such a companion holding a whole
AppleDouble image the client laid out itself (`apple_double_codec::Image`), the
body is decoded: the FinderInfo, every attribute name and value, the client's
alignment padding after a name, the tag and the resource-fork entry are kept,
and each READ encodes the image again. Decoding requires that the decoded form
encode back to the very same bytes, so a synthesized read is byte for byte what
the client wrote; a state the client is still writing (a header, a truncation,
an edit that leaves the layout incomplete) stays its own bytes, and an image
with a resource fork, another layout or a main file the export does not know
keeps the stored-as-written path, and a body that becomes one of those is
written back out as the client's own bytes. A
companion that is a whole image takes no slot of `HELD`, which is what keeps a
large tree's attribute copies from exhausting the index bound; the bound, its
`NFS3ERR_NOSPC`, its rate-limited log and orphan reclamation remain the fallback
for everything that is not a whole image. No attribute is dropped:
`com.apple.provenance`, `com.apple.quarantine`, `com.apple.FinderInfo` and user
attributes are all kept, and nothing here reaches the Store or a Commit.

Removing, silly-renaming or
replacing a main file drops its paired `._<name>` companion and unlinks any
spilled file at once. Companions remain mount-session state: releasing the
export removes the entire `<mounts>/.appledouble/<namespace>` directory
(and a fresh Attach cleans any leftover directory from a crashed daemon), so
remounts, daemon restarts and Commits do not retain them. A name is
AppleDouble when it starts with `._` and has at least one more byte.

The image layout, the decoded form, and the requirement that a decoded image
encode back to exactly the bytes the client wrote, are in
[`apple_double_codec.rs`](../../../../engine/crates/layerfs-nfs/src/apple_double_codec.rs);
the spill helpers are in
[`apple_double_spill.rs`](../../../../engine/crates/layerfs-nfs/src/apple_double_spill.rs).

CREATE checks an existing companion name, checks the entry limit, reclaims
orphans and inserts a new serial under one companion-lock hold, so concurrent
GUARDED creates of one name yield one serial and one `NFS3ERR_EXIST`. An
existing name is still usable when the table is full. `Table::by_name` keeps
every observed main-file name, including all seen hard-link aliases, rather
than only the latest name of an inode. Unlink and rename remove known vanished
names; retiring an inode removes all keys for its serial. Reclamation therefore
keeps a companion when a second name for its main file was observed. The index
remains an observation cache: retained names can delay reclamation. Its existing
`Table::names` admission bound is unchanged; a separate bound on `by_name`
(alias keys) remains future work. The companion entry limit stays 65,536; NOSPC
reports exhaustion of this export resource, not a permission failure or a
per-user quota. The first full-table refusal logs immediately, with later logs
at least 60 seconds apart; every refusal still increments `names_refused`.

A body is written out from a snapshot taken under the companion lock, as a new
`<serial>.<n>.tmp` opened with `create_new`, and is switched to that file only
if the companion is still the one the snapshot saw (same `used` tick, still
resident): the adopting call renames its temporary file to `<serial>`, so two
calls spilling the same companion never write to or unlink each other's file, a
body another call wrote to while the file was being written wins, and the file
just written is dropped. Disk I/O therefore never holds the lock, and no write
of one call is overwritten by the older snapshot of another. No descriptor is
kept for a spilled body: READ, WRITE and SETATTR open `<serial>` for their own
`pread`/`pwrite`/`set_len` and close it again, so descriptors grow with
concurrent operations and not with the number of spilled companions, and a
companion count above the process's own `RLIMIT_NOFILE` still reads and writes.
An edit records a body's bytes, length, recency tick and time under one lock, so
no call sees a resident body and a length that disagree, and a reply never
slices past the bytes it holds. The scratch directory is created `0700` and each
spilled file `0600`, set explicitly rather than left to the process umask, so a
caller embedding this crate without the daemon's `umask(0o077)` still keeps
companion bytes to its own user. The daemon's startup `reclaim` removes whatever
`<mounts>/.appledouble` a daemon that ended without unmounting left behind,
before it serves any export, and touches nothing else under the mount home.

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
- [Support matrix](../../control/support-matrix/README.md): which platform
  uses this route.
- [Glossary](../../glossary.md).

---

← [Kernel routes: fuser patches](fuser-patches.md) · [Up: Kernel routes](README.md) · [Kernel routes: Coherence](coherence.md) →
