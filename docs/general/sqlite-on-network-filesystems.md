# SQLite databases in macOS NFS Workspaces

> **Status:** Current general guide.

A SQLite database imported from a local filesystem can remain in WAL mode even
when its `-wal` and `-shm` files are absent. With macOS's system SQLite and its
default VFS, querying that database in an NFS Workspace can fail with
`unable to open database file` (`SQLITE_CANTOPEN`, SQLite result code 14).
Opening the connection alone can succeed; the first query exposes the failure.
This concerns application databases inside the Workspace. The LayerFS Store
stays on its supported local filesystem and keeps its configured profile.

## Cause and scope

SQLite's macOS automatic locking selection uses an NFS-specific I/O method table
for an `nfs` filesystem. That table has version 1 and no `xShmMap` callback, so it
cannot provide the shared-memory WAL index through the default route. This is a
SQLite VFS policy; a null callback does not establish a failed kernel `mmap`
system call. Result code 14 is not POSIX errno 14.

A diagnostic on macOS 27.2 with system SQLite 3.54.0 and the native LayerFS NFSv3
backend observed:

| Application database | Local APFS | Imported or created in NFS Workspace |
| --- | --- | --- |
| Closed WAL database, querying a saved row | Success | `SQLITE_CANTOPEN` (14) |
| WAL database converted to DELETE before import | Success | Same saved row returned |
| New database requesting `journal_mode=WAL` | Returns `wal` | Returns `delete`; simple read/write succeeds |

The LayerFS mount already used `locallocks`. Changing an NFS server's handling
of `-shm` files does not provide the callback disabled by the client's VFS.
SQLite builds or explicitly selected VFS implementations can behave differently;
this observation does not qualify every SQLite application or concurrent workload.
A separate macOS system-nfsd export was not tested because starting it required
administrator authentication unavailable to the diagnostic session.

## Preparing application databases

For a database the application permits in rollback-journal mode, stop every
process using it, keep a recoverable backup, then use SQLite on the original
local filesystem before import:

```sh
sqlite3 -bail /absolute/local/path/application.db \
  'PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode=DELETE;'
```

Proceed only after the checkpoint reports no busy readers/writers and the mode
change returns `delete`. Keep the application stopped until import finishes.
The mode change persists in the database header. Do not edit that header by hand
or delete a `-wal` file: committed data may still be in the WAL. Copying an active
database and its sidecars separately does not guarantee a consistent snapshot.

For disposable caches, use the application's supported cache reset while it is
stopped, or configure its cache on a local APFS directory outside the mount.
The application may request WAL again on startup; check its actual returned mode
and its own requirements. Simple DELETE-mode success does not establish general
network-filesystem database safety or multi-host lock correctness.

Import preserves application bytes and does not convert databases automatically.
Checking only sidecar names misses closed WAL databases. A future advisory check
could inspect the SQLite signature and header bytes 18/19 (value 2 denotes WAL
read/write format), without opening or modifying application databases. It would
need an explicit diagnostic surface and a bounded read strategy; it must not
exclude files, rewrite content or claim that a live import is consistent.

## References

- [SQLite WAL overview and network-filesystem restrictions](https://www.sqlite.org/wal.html)
- [SQLite Unix VFS source (`nfsIoMethods` and `autolockIoFinderImpl`)](https://www.sqlite.org/src/doc/trunk/src/os_unix.c)
- [SQLite database header format](https://www.sqlite.org/fileformat2.html#the_database_header)
- [SQLite result codes](https://www.sqlite.org/rescode.html#cantopen)
