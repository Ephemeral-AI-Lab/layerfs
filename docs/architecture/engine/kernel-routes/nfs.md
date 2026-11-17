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

## Server (`server.rs`, `kernel.rs`, `read.rs`, `block.rs`)

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
| `ExportConfig` | `path` (normally `/`), `uid`/`gid` shown on every inode, `read_only`, `refuse_apple_double`, `lookups_held`, `companion_spill_dir`, `companion_resident_bytes`, `companions_held`; `writable(uid, gid)` is the default | [`server.rs`](../../../../engine/crates/layerfs-nfs/src/server.rs) |
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
 READDIR / READDIRPLUS (handle, cookie, cookieverf, maxcount)
   cookie 0  -> open_directory: fresh descriptor; verifier = its id
   cookie >0 -> descriptor (serial, verifier) still open? no: BAD_COOKIE
   cookies: 1 = ".", 2 = "..", then engine-published offsets
   one reply = at most one engine window; entries that fit maxcount
   -> publish_cookies(those names); eof only when the window ended and fit
```

| Rule | Source |
| --- | --- |
| The cookie is the engine's kernel offset on one implicit directory descriptor named by `cookieverf`; a retired offset or a listing continued after an install is `NFS3ERR_BAD_COOKIE`, and the client lists again | [`directory.rs`](../../../../engine/crates/layerfs-nfs/src/directory.rs), [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| A READDIRPLUS entry carries the facts its window read — a local inode's current row, or an inherited inode's immutable base stat — and the handle of its serial, so the client needs no LOOKUP per name; both are optional in RFC 1813 and this export sends both for every named entry whose offsets the reply publishes, each handle taking its one lookup reference in the publishing visit (reused offset ranges take none) | [`directory.rs`](../../../../engine/crates/layerfs-nfs/src/directory.rs), [`kernel_directory.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_directory.rs), [`kernel_cookie.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/kernel_cookie.rs) |
| A window's inherited inode values cost one batch inode demand for each level of the inode tree the cache does not already answer (`lookup_many` reads a level's pages in one `read_canonical_batch`, resident levels cost none) and its ordinary file lengths one batch length demand (`provide_lengths`) while the cache can remember them (too small for one entry remembers nothing, so each file asks for its own length); entries memory already answers cost no Store reader. Names and kinds come first, the offer then truncates the window to the names the reply publishes, and only those names' facts are read, so a re-sent reply reads nothing for a name it does not send. Test counts where earlier visits warmed upper levels are not a fresh mount's first window | [`kernel_directory.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_directory.rs), [`client.rs`](../../../../engine/crates/layerfs-workspace/src/base/client.rs) |
| Those batches are not everything an entry costs, and this route changes none of the rest: an inherited inode whose metadata is not inline reports one portable-metadata demand per distinct metadata root, a listed subdirectory one listing of that subdirectory for its child-directory count, and a symlink entry one read of its target; only ordinary files with inline metadata pay only those batches (when the cache remembers lengths) | [`kernel_directory.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_directory.rs), [`view.rs`](../../../../engine/crates/layerfs-workspace/src/base/view.rs), [`links.rs`](../../../../engine/crates/layerfs-workspace/src/base/links.rs) |
| READDIR has one count, which bounds the whole reply; READDIRPLUS sends `dircount` before it, and only `maxcount` bounds this reply | [`nfs.rs`](../../../../engine/crates/layerfs-nfs/src/nfs.rs), [`directory.rs`](../../../../engine/crates/layerfs-nfs/src/directory.rs) |
| A reply's `dir_attributes` are the facts of the listed directory its window read — a local row, fresh from that visit, costing no extra read. An inherited directory has no local row, so its window reads none and the reply carries no attributes; the client then falls back to its own `actimeo` rather than restarting it on facts nothing refreshes | [`directory.rs`](../../../../engine/crates/layerfs-nfs/src/directory.rs), [`kernel_directory.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_directory.rs) |
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
| `wcc_data` pre- and post-op attributes come from the publishing job's own inode state, RENAME included; no reply reads an attribute outside the job. An AppleDouble companion's CREATE, REMOVE or RENAME publishes no engine job and answers both halves from `Table::directory_stats` (or absent when unseen) | [`changing.rs`](../../../../engine/crates/layerfs-nfs/src/changing.rs), [`kernel_visit.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_visit.rs), [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| RENAME looks nothing up first; a detached (`nlink == 0`) destination's reference and descriptor are retired inside that job | [`change.rs`](../../../../engine/crates/layerfs-nfs/src/change.rs) |
| COMMIT answers the attributes last published or observed through the implicit descriptor, so `fsync` costs no owner job | [`change.rs`](../../../../engine/crates/layerfs-nfs/src/change.rs) |
| A client silly rename (`.nfs` + 24 hex on Linux; `.nfs.` + 8 hex + `.` + 4 hex on macOS) is served as an UNLINK; the export keeps the inode until the final REMOVE | [`names.rs`](../../../../engine/crates/layerfs-nfs/src/names.rs) |
| A serial whose last name this export removed gives back its reference and descriptor at once | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs) |
| SETATTR's guard compares a fresh GETATTR ctime, not atomic with the change; access time is dropped | [`change.rs`](../../../../engine/crates/layerfs-nfs/src/change.rs) |
| ACCESS grants owner bits to the inode's uid or uid 0, group bits to its group, else other bits; a read-only export grants no change | [`attributes.rs`](../../../../engine/crates/layerfs-nfs/src/attributes.rs) |

## LOOKUP and GETATTR (`nfs.rs`, `kernel.rs`)

| Rule | Source |
| --- | --- |
| One owner visit answers a LOOKUP: the job that resolves a name has already read the directory's own row, so the reply's `dir_attributes` cost no second visit | [`nfs.rs`](../../../../engine/crates/layerfs-nfs/src/nfs.rs), [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs), [`kernel_read.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_read.rs) |
| A LOOKUP that fails answers the directory's own fresh facts whenever the job that failed read its row, which is every name this export looks up: a name that binds nothing still read its parent's row in the same visit. `.`, `..`, a silly name, an AppleDouble companion and a refusal decided before any visit read no directory row and send no `dir_attributes`, so the client falls back to its own `actimeo`; RFC 1813 makes both fields optional. No LOOKUP answer repeats the export's last observation of the directory, which nothing ages. A failed LOOKUP's directory row is kept as the export's observation, as a successful one's is | [`nfs.rs`](../../../../engine/crates/layerfs-nfs/src/nfs.rs), [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs), [`kernel_read.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_read.rs) |
| `read.rs` holds what a read answers: the entry, its directory and its descriptors, and the refusal with its status (`NFS3ERR_STALE` for an engine `Stale`, else `NFS3ERR_IO`) and the directory row the job read | [`read.rs`](../../../../engine/crates/layerfs-nfs/src/read.rs) |
| An export remembers the facts it last published or observed for a directory (LOOKUP, GETATTR, OPENDIR, a publishing change) and answers from them only where no job reads that directory: an AppleDouble companion's `wcc_data`, whose change never touches it. A directory listing is not such a place: its reply says only what its own window read | [`kernel.rs`](../../../../engine/crates/layerfs-nfs/src/kernel.rs), [`apple_double.rs`](../../../../engine/crates/layerfs-nfs/src/apple_double.rs) |

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

## AppleDouble files (`apple_double.rs`, `apple_double/edit.rs`, `apple_double_codec.rs`, `apple_double_spill.rs`)

On NFSv3 the macOS client keeps extended attributes and resource forks in
`._<name>` files (at least one byte after `._`). With `refuse_apple_double`, a
regular `._` file lives in export state only (`Companions`, serials
`SERIALS` = 2^62 and above): LOOKUP, GETATTR, ACCESS, READ, WRITE, SETATTR,
COMMIT, REMOVE and RENAME to another `._` name are served there, READDIR
never lists it and the engine is never called. Any other kind is
`NFS3ERR_ACCES`. `cp`, `copyfile`, `ditto` and `tar` succeed while nothing
reaches the Workspace, SQLite overlay or a Commit; everything is dropped with
the export.

| Bound | Value | Over the bound | Source |
| --- | --- | --- | --- |
| `HELD` | 65,536 slotted companion entries | `Companions::admits` decides the bound in one place, before anything is committed: CREATE, and a WRITE, SETATTR or RENAME that needs a new slot, first reclaim the orphan slotted companions outside the companion lock, once, and then answer the whole operation `NFS3ERR_NOSPC` with the body, its length and its name as they were (CREATE also increments `names_refused` and logs at most once per 60 s per export). A call whose commit needs I/O in between (`Companions::reserve`) holds the slot from the decision, so the bound holds when calls race for the last one | [`apple_double.rs`](../../../../engine/crates/layerfs-nfs/src/apple_double.rs) |
| `FILE_BYTES` | 1 MiB per companion | WRITE / SETATTR `NFS3ERR_FBIG` | [`apple_double.rs`](../../../../engine/crates/layerfs-nfs/src/apple_double.rs) |
| `RESIDENT_BYTES` | 32 MiB of companion bodies in all: a body kept as written at its length, a decoded one as `Image::cost`; an edit that leaves a body whole decodes it as it records it, so the count may stand one decoded body over until the next edit needs room | Spill the least recently touched bodies, decoded ones included, to `companion_spill_dir` (`<mounts>/.appledouble/<namespace>/<serial>`); host disk I/O errors map to `NFS3ERR_NOSPC` (`ENOSPC`/`EDQUOT`), `NFS3ERR_FBIG` (`EFBIG`), `NFS3ERR_ACCES` (`EACCES`), `NFS3ERR_PERM` (`EPERM`), `NFS3ERR_ROFS` (`EROFS`) or `NFS3ERR_IO` | [`apple_double.rs`](../../../../engine/crates/layerfs-nfs/src/apple_double.rs), [`apple_double_spill.rs`](../../../../engine/crates/layerfs-nfs/src/apple_double_spill.rs), [`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs) |
| `DECODED_ENTRY_BYTES` | 128 KiB counted for one decoded companion: a cap on what one decoded body is charged, not on what a companion may store | A whole image costing more keeps its bytes as written, is charged at their length and still takes no `HELD` slot | [`apple_double.rs`](../../../../engine/crates/layerfs-nfs/src/apple_double.rs) |

A companion whose main inode the export knows belongs to that inode rather than
to one name: hard links share it, main-file RENAME keeps it under the new name
and existing aliases, and only the inode's last name (or its silly rename)
drops it. A name of the inode that goes does not take its `._` name with it:
after `rm a` of a file that `b` still names, `._a` still answers the inode's
companion, because the `._` name key stays until the inode's last name (a
client whose `actimeo` expired may ask either name). A new file that takes the
freed name reaches the old companion nowhere: `Companions::resolve` takes a
name key only for the inode its companion belongs to, so `._a` answers that new
inode's companion, or `NFS3ERR_NOENT` while it has none. It can hold several of
the inode's `._` names at once: a LINK of a
companion name (`._a` to `._b`, which a client that carries the attributes to
a new name sends) adds one, answering `NFS3ERR_EXIST` for a name already held
and `NFS3ERR_ACCES` for a name of the Workspace, and a REMOVE of one of those
names drops only that name while the companion holds another or its inode has
another name the export knows. The REMOVE of the last name of an inode with one
name is the client clearing the attributes, which takes the companion with it,
as does any name of a companion no main inode owns.

When a write leaves an inode-bound companion holding a complete
AppleDouble image that encodes back to the exact bytes written
([`apple_double_codec.rs`](../../../../engine/crates/layerfs-nfs/src/apple_double_codec.rs)),
it is marked whole and takes no `HELD` slot: decoded in memory (counted as
`Image::cost` against `RESIDENT_BYTES`, at most `DECODED_ENTRY_BYTES` each), or
kept as written bytes, which take the same budget at their length; partial
writes, resource forks, other layouts and unknown-main companions stay as
written bytes and occupy a slot. `companion_rename` re-classifies under the
destination so moving onto an unknown main name takes a slot (a spilled body
keeps its prior class); a decoded body that would be the client's bytes again
and does not fit `RESIDENT_BYTES` is written out first, under the companion's
edit gate and outside the companion lock, so the rename never exceeds the
budget and only records the new name and class.

`HELD` is the admission limit of this export-local shim, not a Workspace or
Sandbox capacity: the companions live for one mount, are deleted at release and
never reach the Workspace, the SQLite overlay or a Commit (owner decision,
2026-11-14, option C, a scoped waiver). Option A, a per-export SQLite index, is
deferred until `NFS3ERR_NOSPC` shows up in real use; it would add a `rusqlite`
dependency edge to `layerfs-nfs`, which the workspace already pins.

`Table::names` and `Table::aliases` index each serial's names so unlink,
`entry_taken` (when `nlink <= 1`), `renamed` (keeping an overwritten recalled
name as an alias) and retirement drop only that serial's keys from `by_name`
without scanning it. At the `HELD` bound, CREATE releases the companion lock to
collect orphans from `Companions::slots` (walking only slotted entries under
the table and companion locks, never whole images or `by_name`), then re-acquires
the lock and re-reads the companion name and main file before inserting, so
concurrent GUARDED creates of one name yield one serial and one
`NFS3ERR_EXIST`. The name index is lossy (its own bound, a name never looked
up, a client-side rename), so a candidate whose main inode the export does not
know costs one read-only lookup of its main file in the engine, outside both
locks - and a companion it holds several names of is reached by any of them, so
one whose name the index holds is no candidate at all: a file that is there binds the companion back to that inode and keeps
its attributes, `NFS3ERR_NOENT` - or `NFS3ERR_STALE`/`NFS3ERR_NOTDIR`, a main
file whose directory the engine no longer has, which cannot hold it either -
makes it an orphan, and a lookup that cannot answer any other way leaves it
alone. The lookup that finds the main file took one
engine reference on it, and the export keeps exactly one per serial, so the
visit registers the name it found the file under (`Kernel::entry_taken`, as
`Kernel::lookup` does) before the companion is bound: that name is what makes
the bound companion live for the next reclamation, and its reference is the one
the export later gives back.

Bodies kept as written and decoded bodies alike, exceeding `RESIDENT_BYTES`
together, are written to `<serial>.<n>.tmp` (`create_new`, dir `0700`, file
`0600`) and renamed to `<serial>`, all of it outside the companion lock; the
body is committed as spilled under the lock afterwards, so a reader reads the
body in memory until its file is whole. One writer per companion is the edit
gate (`Companion::editing`): an edit holds it, and an eviction takes it with
`try_lock`, skipping a companion someone is editing, one another call removed
and one whose file cannot be written out; only a call that finds no room and
then cannot write out its own body answers its own error. A rename is dropped when
the body changed since the snapshot (`used` tick), and a spilled body is opened
by path per `pread`/`pwrite`/`set_len` and never held open; a `pread` shorter
than the length the lock saw is a short read, not `NFS3ERR_IO`.

One edit per companion runs under that gate
([`edit.rs`](../../../../engine/crates/layerfs-nfs/src/apple_double/edit.rs)),
recording length and bytes together and spilling its own body rather than
failing with `NFS3ERR_IO`. A file operation that fails after the spill put the
length the companion had back, so a refused write reports neither bytes nor the
longer file the spill made for it. Eviction follows `Companions::by_use`, the
`(used tick, serial)` order kept on attach, detach and every use: the victim is
the first entry (O(log n) per round, no walk of the index), and the rounds end
when no companion holding memory is left. `ExportServing::reclaim` and release
delete `<mounts>/.appledouble`.

Release logs one line per export that still holds companions:
`held= slotted= decoded= resident_bytes= unreachable=`. `unreachable` counts the
companions no name reaches - neither the name a companion records nor another
name of its inode finds it - which is the shape a lost attribute takes when the
body is still held but a client cannot ask for it; a zero beside a client that
lost attributes says the loss was a missing companion rather than a lost name.
The same count is in the holdings a status read reports
(`Released::companions_unreachable`), so the two never disagree; it walks the
companion index, like the release line and the reclamation walk, and only those
diagnostics read it.

The requests one real client makes can be read from an optional trace
([`trace.rs`](../../../../engine/crates/layerfs-nfs/src/trace.rs)): with
`LAYERFS_NFS_TRACE=1` the export prints, to the daemon's standard error, one
line for each request that names an AppleDouble companion - the `._` names
themselves and the main file names of the inodes that hold one - carrying the
directory serial, the name, the main inode the export's name index gave the
name, which part of the companion index answered it (`route=name` for the
companion's own name key, `route=inode` for the main inode, `route=none` when
neither did), the companion serial a reply carries as its fileid, and the
status the client is answered. A CREATE that finds the companion already held
says whether it would truncate it (`existing-truncate`), a SETATTR of a `._`
file is traced by handle, `BIND` shows a main name the kernel table
re-attributed to an inode that has a companion (`alias` while that inode keeps
several names, `unbound` when the one it held is all it has), and `UNPAIR`
shows what a main name that went left behind: `name-kept` when the companion
belongs to its inode and keeps the `._` name, `companion-dropped` when the
companion goes with it. It is off unless
the variable is exactly `1`, is read once per process, and changes no state;
the lines are written under the lock the request already holds.

On the `native` route the daemon is started with `env_clear` and an empty
environment (`control/crates/layerfs-sandbox-backend/src/local/create.rs`), so
it does not inherit `LAYERFS_NFS_TRACE=1` from the shell that created the
Sandbox: a trace run there needs `machine update native --daemon` pointed at a
wrapper that exports the variable and `exec`s the real daemon (measured on
macOS 27.2 for #636, where the plain variable gave 0 trace lines and the wrapper
55 and 77 on the two matrices). Other routes start the daemon through a worker
or container, where the environment is passed differently.

## Mount (`mount.rs`, daemon `export.rs`)

```text
 Attach:   bind 127.0.0.1:0 -> Server::start -> port P
           helper mount_nfs -o vers=3,tcp,port=P,mountport=P,... <dir>
           client MOUNT MNT "/" -> root handle, AUTH_UNIX
 Commit:   flush helper (else the export volume's own flush: sync_volume_np
           on macOS, sync(2) elsewhere), close_files, Commit
 Unmount:  unmount helper (busy is reversible), Server::stop,
           revoke the engine mount, Close
```

| MOUNT procedure | Answer |
| --- | --- |
| 0 `NULL`, 4 `UMNTALL` | Empty success |
| 1 `MNT` | The configured path (trailing slashes ignored): `MNT3_OK`, root handle, `AUTH_UNIX`; else `MNT3ERR_NOENT` |
| 2 `DUMP`; 3 `UMNT`; 5 `EXPORT` | Empty list; success; one export with no groups |

Client options are the daemon's `Mounter::host`. macOS:
`vers=3,tcp,port={port},mountport={port},locallocks,nfc,noatime,actimeo=60,rsize=262144,wsize=262144,soft,intr,dumbtimer,timeo=300,retrans=6,mutejukebox,nobrowse,nordirplus`
(`rsize`/`wsize` are `TRANSFER_WINDOW`); elsewhere
`vers=3,proto=tcp,port={port},mountport={port},mountproto=tcp,nolock,actimeo=60`
([`export.rs`](../../../../engine/crates/layerfs-daemon/src/control/export.rs)).
`nordirplus` turns READDIRPLUS off: a macOS client holding a directory's
complete READDIRPLUS listing answers a `._` name absent from it with ENOENT
and never asks the server, and this export serves AppleDouble companions
without listing them, so every `._` name has to arrive as a LOOKUP.
An NFS ready receipt has `abi_major == 0` and its `mount_id` is the port.

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
