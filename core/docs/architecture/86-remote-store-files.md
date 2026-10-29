# Remote Store files: push and pull of the whole SQLite Store

> **Status:** Current implementation description, #506 area R first phase
> (2026-10-29), from the code on branch `issue506-store-backends-remote`.
> MinIO is the only remote provider in this phase. It adds no requirement
> beyond the owner direction recorded on #506.
> Companion notes: [global SQLite Store](84-global-sqlite-store.md),
> [Store concurrency model](83-store-concurrency-model.md).

## 1. What a remote is

A remote holds a project's whole SQLite Store file under one name
(`store.sqlite`) in its own namespace. **Push** uploads a consistent copy of
the live Store; **pull** downloads it and replaces the local Store. LayerFS
never opens SQLite over the network: every opener still opens a local file on
a local Linux filesystem, as before.

There is no compare-and-swap on the remote (owner, 2026-10-29). Two pushes
race last-writer-wins. A pull racing a push fails its sha256 check instead of
installing mixed bytes.

## 2. Crates and files

`engine/crates/layerfs-remote-storage` knows nothing about Stores. It moves
named files and has no first-party dependency; its only dependency is `sha2`.

| File | Role |
| --- | --- |
| `driver.rs` | `Driver`: `limits`, `upload_file` (a file or a span of it), `download_file` (whole or ranged, into a writer), `stat`, `delete`; name and metadata rules |
| `limits.rs` | max file, max single request, piece size |
| `pieces.rs` | the whole-file layout: one object, or fixed-size pieces plus a manifest |
| `credentials.rs` | credential references: `env:LAYERFS_*` or an owner-only `file:` |
| `error.rs` | `RemoteError`, including `Uncertain` and `PartsLeft` |
| `local_folder/` | a folder remote (feature `local_folder`), for tests |
| `s3_api/` | the S3 API client (feature `s3_api`): `config`, `sign` (SigV4), `http`, `multipart`, `client` |
| `minio/` | the MinIO provider (feature `minio`) |

`layerfs-store-service/src/remote/` uses it for Stores: `settings.rs` (named
remotes), `push.rs` and `pull.rs`.

## 3. One attempt per request

Every request is sent once. A failure before the first byte is written is
`Unavailable`; after it, a failure to complete the exchange is `Uncertain`
and the request is never resent. The S3 client opens one TCP connection per
request (`Connection: close`), so no connection state carries a failure into
the next request. Socket reads and writes each have a timeout; a long
transfer that keeps moving does not time out.

## 4. Layout on the remote

A file of at most the provider's max file goes up as one object. Its metadata
carries `layerfs-layout: whole` and `layerfs-sha256`, the whole file's sha256.

A larger file goes up as pieces `NAME.piece-000000`, `…-000001`, … of the
provider's piece size (the last smaller), each with its own sha256, then a
manifest under `NAME` with `layerfs-layout: pieces`. The manifest lists the
total size, the whole file's sha256, the piece size and each piece's size and
sha256. Pieces go up first and the manifest last. After an upload, pieces an
earlier, larger upload left beyond the new count are removed.

A download reads the metadata, then the file or each piece in order, and
verifies every piece and the whole file. A mismatch is `Integrity`.

## 5. S3 API and MinIO

The client is the retired one-attempt SigV4 client (`layerfs-s3`, last at
`da0fa3a120^`) over hand-written HTTP/1.1, now with streamed file and reply
bodies and canonical query strings. Each body's sha256 is computed before its
request and signed, so the server rejects bytes that differ.

A body above the configured part size (MinIO default 64 MiB) goes up as a
multipart upload: create, equal-sized parts (the last smaller; 5 MiB to
5 GiB, at most 10,000, raised to whole MiB when a file needs it), complete.
Any failure, including an uncertain part, sends one abort. If the abort fails
too, the error is `PartsLeft` with the upload id; `S3Client::pending_uploads`
lists such uploads and `abort_upload` removes one.

MinIO limits: 50 TiB object, 5 TiB single PUT, 5 TiB pieces above 50 TiB.
Endpoints are plain `http://`; an `https://` endpoint is `Unsupported`. TLS,
which Cloudflare R2 and AWS S3 need, is a later phase.

## 6. Named remotes

A project's remotes live in one text file its caller places: `remote NAME`
lines, each followed by `key value` lines (`provider`, then the provider's
keys). Credentials are a reference (`env:LAYERFS_REMOTE` reads
`LAYERFS_REMOTE_ACCESS_KEY` and `LAYERFS_REMOTE_SECRET_KEY`); any other key,
and so any secret, is refused. The file is written owner-only.
`MinioSettings::from_env` reads `LAYERFS_REMOTE_*`.

## 7. Push

`remote::push(opened, driver, scratch)`:

1. `OpenedStore::snapshot` copies the live Store to a new file in `scratch`
   with SQLite's online backup API (Persistence owns it). Writers continue.
2. The size is checked against the driver's limits; a Store above max file
   goes up as pieces, and the manifest must fit what a pull reads (16 MiB).
3. `pieces::upload` sends it with its sha256 and SQLite version as metadata.
4. The temporary snapshot is removed.

## 8. Pull

`remote::pull(driver, destination, binding, cursor_key)`, on the same terms
as the sealed-Store install (`install.rs`): the kernel guard passes for the
volume and the process is the daemons' user.

1. Download to `store.sqlite.pulling` beside the destination (owner-only,
   created once) and verify every sha256.
2. Open it as this project's Store (binding and cursor key) and close it,
   which removes the sidecars it created.
3. If a local Store exists, refuse when its `-wal` or `-shm` exists: on
   Linux both exist exactly while an opener holds the Store. The caller stops
   the project's daemons and SDK processes first; nothing prevents a new
   opener between this check and the rename.
4. Rename the pulled file over the destination.

A failure keeps the temporary file and names it in the report; nothing is
deleted or tried again.

## 9. Tests

- `layerfs-remote-storage/tests/folder_pieces.rs`: folder driver, pieces,
  manifest and verification.
- `layerfs-remote-storage/tests/minio.rs`: single PUT, ranged GET, multipart
  above the part size, pieces, and failed multiparts (a refused part, a lost
  part reply, a failed abort) through a fault proxy, against a pinned MinIO
  started by `tests/minio_service.sh`. The service tests are `#[ignore]`d.
- `layerfs-store-service/tests/remote_settings.rs`: settings round trip and
  refusals.
- `layerfs-api/tests/remote_store.rs` (Linux): a Store with Branches and
  Commits pushed and pulled into a fresh volume has identical history, over
  the folder remote and (ignored) MinIO.

## 10. Not in this phase

TLS and the other providers (AWS S3, Cloudflare R2, Google Drive, Dropbox,
OneDrive, Box); SDK methods and a `layerfs` command for remotes; incremental
push (row bundles, #495); a PostgreSQL Store's remote (it pulls a SQLite
Store and imports it).
