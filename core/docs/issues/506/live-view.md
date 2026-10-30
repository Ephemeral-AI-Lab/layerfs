# #506 Live view of a Store in S3: implementation spec

> **Status:** Owner-approved direction (2026-10-30); A–D implemented on PR #507
> (`26eb1b975d`–`59eb6ef8bf`), E in Connect's working tree. Narrowed by the
> owner (2026-10-30, `63ced40427`): a view reads the tree, sizes and history,
> never file contents; content reads (read, grep, blame, diffs) are refused
> `Unsupported`. On `r2-examples-clone` via MinIO, History costs 4 GETs and the
> run 7.26% of the file, but the root folder listing costs 20 GETs (each inode
> lookup misses in a different place and reads 1 MiB ahead). R2 stopped twice
> on a cut reply body (`Unknown`); the cause was the client's
> `Connection: close`, and the S3 client now keeps connections alive instead
> (§4.2 Transport). See the #510 comments. Builds on
> [one link per Store](links.md) (#509). The Connect part is in Connect's own
> repository, `docs/ui/storage-links.md` §8.

## 1. Owner direction (2026-10-30)

- A SQLite Store saved in S3 (MinIO, Cloudflare R2) must be viewable without
  loading it: its folder tree, its history, and the contents of a file.
- Downloading a temporary copy first ("preview") is **rejected**: it does not
  work for large Stores.
- The view reads only what it shows, straight from the remote file, read-only.

## 2. Findings that make it feasible (2026-10-30, diagnostic, one sample)

Measured on `r2-examples-clone` (251 MB, 4 KiB pages, 61,322 pages) with
SQLite's `dbstat`, inside the Connect worker. Exploratory figures, not a
qualified measurement.

| Part | Tables | Size |
| --- | --- | --- |
| File contents | `pack` | 239.4 MB (95%) |
| Tree metadata | `inode_value`, `object_location` | 8.0 MB |
| History | `commit`, `commit_lineage`, `branch`, `commit_runs`, `commit_identity`, `history_meta`, `branch_names`, `branch_identity`, `run_merges`, `layer`, `absorbed*`, `reach`, `merge_record`, `sqlite_schema` | 0.7 MB (176 pages) |

How scattered each part is, counted in blocks the reader would fetch (merged
ranges = requests after joining adjacent blocks):

| Layout | History, 64 KiB blocks | Tree metadata, 64 KiB blocks |
| --- | --- | --- |
| As pushed today (backup API snapshot) | 71 blocks, 43 ranges, 4.7 MB | 450 blocks, 259 ranges, 29.5 MB |
| Rewritten with `VACUUM INTO` | 13 blocks, **3 ranges**, 0.9 MB | 123 blocks, **2 ranges**, 8.1 MB |

`VACUUM INTO` of that Store took 0.41 s against 0.45 s for the backup API, on
a warm page cache. That is diagnostic only; §7 measures it properly.

Already in place:

- `layerfs-remote-storage` `download_file(name, span, out)` sends a ranged GET
  (`bytes=a-b`, expects 206), see `s3_api/client.rs`.
- `layerfs-persistence` opens a SQLite Store read-only (`store/open.rs`
  `open_read_only`; `Session::connect` with `SQLITE_OPEN_READ_ONLY`).
- MinIO pieces at 5 TiB and R2 at about 5 TB, so every realistic Store is
  one whole object. The reader must still handle a manifest (§4.2).
- The PostgreSQL route already opens a Store through a store path
  (`postgresql:<store>`), which the remote view mirrors.

## 3. What gets built

```text
 Connect "Browse" ─► runtime remote view ─► binding opens store path
                                              remote:<connection>/<file>
                                                    │
                     layerfs-persistence            ▼
                     read-only SQLite session ── VFS "layerfs-remote" (immutable)
                                                    │ xRead(offset, len)
                                                    ▼
                                          block cache (64 KiB blocks, bounded)
                                                    │ miss: merged ranged GETs,
                                                    │ If-Match: <ETag>
                                                    ▼
                                    layerfs-remote-storage driver (MinIO, R2)
```

1. **Push layout (A).** Push builds its snapshot with `VACUUM INTO` instead
   of the backup API, so each table and index is one contiguous run.
2. **Remote reader (B).** A read-only SQLite VFS whose reads go to the
   remote file through a bounded block cache.
3. **Remote store path (C).** `remote:<connection>/<file>` opens a Store
   read-only through B; every existing read call works on it, every write is
   refused.
4. **Runtime calls (D)** to open, read and close a view.
5. **Connect (E):** Browse on a remote file opens the project pages
   read-only.

## 4. Engine

### 4.1 A: push layout with `VACUUM INTO`

- The push snapshot step writes `VACUUM INTO '<staged path>'` from a
  read transaction on the live Store, instead of the backup API. Writers keep
  going, as today. Free-space check, hashing, upload, record and the heads
  digest are unchanged; heads are read from the staged file as now.
- **Pre-check before changing it (blocking):** `VACUUM` may renumber the
  rowid of tables without an explicit `INTEGER PRIMARY KEY`. Audit every
  table and every engine query for reliance on such rowids (including
  `rowid` used as a cursor, `last_insert_rowid`, and foreign references).
  If any table relies on them, report it and stop A; do not change the
  schema to make it pass. WITHOUT ROWID tables are unaffected.
- The staged file keeps the Store's page size and `user_version`, and the
  installed (pulled or loaded) file must open exactly as today. Check
  `journal_mode`, `auto_vacuum` and `application_id` survive or are set as
  the open path expects.
- A file pushed before A still opens through B, only with more requests.

### 4.2 B: the remote reader

- A SQLite VFS named `layerfs-remote`, registered once per process through
  `rusqlite::ffi` (libsqlite3-sys, already linked). No new crate or
  dependency. Unsafe FFI stays in one module with its safety comments.
- **Read-only and immutable:** opened with `SQLITE_OPEN_READ_ONLY` and the
  URI parameter `immutable=1`, so SQLite takes no locks and looks for no WAL
  or `-shm`. `xWrite`, `xTruncate`, `xSync` and `xDelete` refuse.
- **Pinned version:** the open HEADs the object once (size, ETag,
  `layerfs-layout`, `layerfs-sha256`). Every ranged GET sends
  `If-Match: <ETag>`. A 412 surfaces as `Changed` ("the remote file changed
  since this view opened; open it again"), never as mixed bytes.
- **Pieces:** for `layerfs-layout: pieces`, read the manifest once and map
  each offset to `(piece, offset in piece)`; a block that spans two pieces
  becomes two GETs.
- **Block cache:** 64 KiB blocks, adjacent missing blocks merged into one
  ranged GET (at most 1 MiB per request), LRU, bounded per view (default
  64 MiB, `LAYERFS_REMOTE_VIEW_CACHE`). A bound, not a cap on file or Store
  size: a view reads any size of Store through it.
- **No retry:** each GET is one attempt. Failure maps to `Unavailable`,
  `Denied` (401/403), `Changed` (412) or `Unknown` (connection lost), and
  fails that SQLite read (`SQLITE_IOERR_READ`), so the call reports it.
- **Transport (#510 fix):** the S3 client sends HTTP/1.1 requests without
  `Connection: close` and keeps a connection whose reply ended exactly at
  its framing for its next request (at most 4 idle, each for at most 10 s;
  `s3_api/connection.rs`). Cause, diagnostic (one sample per arm, Linux
  container on macOS Docker, 1 MiB ranged GETs of an `lfc-test-` object,
  4 at a time): with `Connection: close`, R2's edge ended 6 of 583 bodies
  2–18 KiB early with a clean TLS close (`close_notify`); without it, 0 of
  597 on fresh connections and 0 of 1,035 on reused ones. The transport
  could not tell that close from a complete body: OpenSSL 3 through
  rust-openssl's stream reports both a `close_notify` and a TCP close
  without one as end of file, so `read_body` said "reply body ended
  early". Not a read timeout (that surfaces as `WouldBlock`, never as end
  of file). Taking a kept connection sends nothing: one idle too long, or
  with anything to read (a close, an alert, stray bytes) is closed first
  and the request connects afresh. Once a request's first byte is written,
  a failure is that request's outcome (`Unknown` when sent), never resent
  on this or another connection.
- **Counters:** per view, requests, bytes fetched, cache hits and misses,
  for the trace and for §7.
- **Concurrency:** one view may serve concurrent read calls; the cache is
  shared under one short lock, never held across a GET.

### 4.3 C: the remote store path

- Store path `remote:<connection>/<file>`, resolved like
  `postgresql:<store>`: the connection's settings and credential reference
  from the home, or per-call `credentials`. On Linux in the caller's
  process; on macOS and Windows in the worker, as every Store call.
- Opens `layerfs-persistence` read-only over B. Reads (project list,
  state, branches, Commits, layers, directory listing, file read) work as on
  a local Store. Writes, Workspaces and Sandboxes refuse `ReadOnly`.
- Not registered in `stores.json`, never linked, never counted as a Store
  of the home.

### 4.4 D: runtime calls

| Call | Does |
| --- | --- |
| `runtime.remote.open_view(remote, { file, credentials })` | HEADs the file and opens a view: `{ view, remote, file, bytes, etag, sha256, heads, modified, projects: [names] }` |
| reads with `{ view }` | `runtime.project.list({ view })`, `get_state(name, since, { view })`, and the branch, Commit, directory and file reads Connect's project pages use accept `{ view }` in place of `{ store }` |
| `runtime.remote.close_view(view)` | closes it and drops its cache; views also close after a quiet minute, like the worker's Store handles |
| `runtime.remote.view_stats(view)` | the §4.2 counters |

Exact names may change in the implementation; README §6 records the final
contract.

## 5. E: Connect

- **Browse** beside **Load…** on every file in a connection's file list
  (storage-links §3.3), and on a linked Store's Link card ("Browse remote").
- It opens the project pages of the file's projects, read-only: History,
  Files and Timeline work; Live and Sandboxes are hidden; every action that
  writes is absent. A banner reads "`r2-team / team-examples` · as pushed
  `<time>` · read-only", with **Load…** (or **Open** when linked).
- `Changed` shows "This file changed since you opened it" with **Reopen**.
  Other failures show their message once, with no automatic retry.
- Host routes: `POST /api/remotes/:name/views` `{ file }`,
  `DELETE /api/views/:id`, and the project routes accept `?view=`.

## 6. Rules

- Disposable profile for every local Store. One attempt per operation.
- New production files ≤999 lines; production LOC in every commit
  (`tools/production_loc.py`); delete tests of replaced behaviour rather
  than adapt them.
- Test commands ≤120 s with unfiltered output.

## 7. Verification

1. **Pre-check result (A)** recorded in the issue: the rowid audit, table by
   table.
2. **Unit:** block math (merge, piece spans, cache bound), ETag pinning
   (412 → `Changed`), write refusals, error mapping.
3. **Native, against MinIO:** push with A, open a view, and compare with the
   same Store opened locally: identical `project.list`, branch heads,
   Commit lists, a directory listing and the bytes of several files (small,
   chunked, large). Push over the file mid-view → `Changed`.
4. **End to end through Connect on MinIO and R2:** Browse a file, open
   History, a folder and a file; record request counts and bytes (§4.2
   counters).
5. **Measurement** (agent measurement policy, Disposable, one sample per
   case, cold cache declared and enforced, fresh outputs):
   - push snapshot time and file size, backup API vs `VACUUM INTO`, on the
     largest available Store;
   - first History view and one folder listing through a view, on MinIO and
     R2: wall time, requests, bytes.
6. **Acceptance:** for a Store pushed with A, the first History view takes
   at most 5 ranged GETs, and opening a folder at most 4 per directory
   level; the view never downloads the whole file (bytes fetched under 10%
   of the file for History plus one folder plus one small file on
   `r2-examples-clone`); every read in step 3 is identical to the local
   Store's.
