# Remote Store files: push and pull of the whole SQLite Store

> **Status:** Current implementation description, #506 area R first phase
> (2026-10-29), from the code on branch `issue506-store-backends-remote`.
> MinIO and Cloudflare R2 are the remote providers (R2 over HTTPS, added
> 2026-10-30). They add no requirement beyond the owner direction recorded
> on #506.
> Companion notes: [global SQLite Store](84-global-sqlite-store.md),
> [Store concurrency model](83-store-concurrency-model.md).

## 1. What a remote is

The unit is a **Store**, with every project (named layer stack) in it; a
LayerFS home can have many Stores. Remotes are defined once per home, and one
remote holds many Stores: Store `NAME` is the file `NAME/store.sqlite` in the
remote's namespace (`<prefix>/NAME/store.sqlite` on MinIO), plus its pieces
when split. **Push** uploads a consistent copy of a live Store; **pull**
downloads it and replaces one local Store as a whole. LayerFS never opens
SQLite over the network: every opener still opens a local file on a local
Linux filesystem, as before.

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
| `cloudflare_r2/` | the Cloudflare R2 provider: account id (and jurisdiction) to its endpoint, region `auto`, R2's limits; connects only with `tls` |

`layerfs-store-service/src/remote/` uses it for Stores: `settings.rs` (a
home's named remotes and `store_object`), `push.rs`, `pull.rs` and `state.rs`
(the last transfer of each Store).

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
R2 limits: 4.995 TiB object, 4.995 GiB single PUT (multipart above the part
size, parts equal-sized but the last), 4.995 TiB pieces above that.
An endpoint is `http://` or `https://` with any host name (a container on a
Docker network, such as the macOS worker, reaches MinIO by container name),
IPv4 or bracketed IPv6 address, and an optional trailing `/`; nothing
assumes localhost. `https://` needs the `tls` feature (off by default; the
worker's sealed build turns it on with `layerfs-node/tls`), which uses the
`native-tls` already pinned for PostgreSQL: OpenSSL on Linux, checking the
server's certificate and name against the system's roots (`SSL_CERT_FILE`
adds one, as the TLS MinIO test does). The handshake comes before the
request, so a refused certificate is `Unavailable` and nothing was sent.
Without `tls`, an `https://` endpoint is `Unsupported` when the remote is
added. An R2 remote is `https://ACCOUNT[.JURISDICTION].r2.cloudflarestorage.com`,
path-style, region `auto`.

## 6. Named remotes and transfer state

A home's remotes live in one text file, `HOME/remotes`: `remote NAME`
lines, each followed by `key value` lines (`provider`, then the provider's
keys). Credentials are a reference (`env:LAYERFS_REMOTE` reads
`LAYERFS_REMOTE_ACCESS_KEY` and `LAYERFS_REMOTE_SECRET_KEY`); any other key,
and so any secret, is refused. The file is written owner-only.
`MinioSettings::from_env` reads `LAYERFS_REMOTE_*`. An R2 remote's keys
are `provider r2`, `account` (32 lowercase hex digits), `jurisdiction`
(empty, `eu` or `fedramp`), `bucket`, `prefix`, `credentials` and
`part-size`.

The last push or pull of each Store (remote name, direction, time, sha256,
size) is kept in `HOME/remote-state/NAME`, outside the Store, because a pull
replaces the Store file. `remote::state::save` and `load` write and read it;
the caller records a transfer after it completes.

## 7. Push

`remote::push(opened, store, driver, name, binding, expected)`:

0. `expected` (`Any`, `Absent` or `Sha256`, from this home's last push or
   pull) must match the remote copy; otherwise `RemoteMoved`, before the
   snapshot and again just before the upload. This is not atomic: a push by
   another machine between that check and the upload is not detected.

1. Check that the Store's filesystem has room for one Store-sized copy (main
   file plus WAL) and the Store's own write headroom; refuse `Space`,
   naming the directory, the bytes needed and the bytes free. On macOS the
   Store, and so the staged copy, is inside the Docker volume.
2. `OpenedStore::snapshot` copies the live Store to `store.sqlite.pushing`
   beside it with SQLite's online backup API (Persistence owns it). Writers
   continue.
3. The size is checked against the driver's limits; a Store above max file
   goes up as pieces, and the manifest must fit what a pull reads (16 MiB).
4. `pieces::upload` sends it as `NAME/store.sqlite` with its sha256, SQLite
   version and binding key (`layerfs-binding`, hex; not a secret) as
   metadata. The cursor key never leaves its holder.
5. The staged snapshot is removed.

## 8. Pull

`remote::pull(driver, name, destination, binding, cursor_key)`, on the same
terms as the sealed-Store install (`install.rs`): the kernel guard passes for
the volume and the process is the daemons' user.

1. Refuse when the local Store is open: its `-wal` or `-shm` exists. On
   Linux both exist exactly while an opener holds the Store: a daemon (and
   so every mounted Workspace) or an SDK process or worker. The macOS worker
   keeps a Store open for a quiet minute after use; the error says so.
2. Download `NAME/store.sqlite` to `store.sqlite.pulling` beside the
   destination (same filesystem, owner-only, created once) and verify every
   sha256.
3. Open it with the Store's binding and cursor key and close it, which
   removes the sidecars it created.
4. Refuse again if the local Store was opened meanwhile; nothing prevents a
   new opener between this check and the rename.
5. Rename the pulled file over the destination: one rename.

Before downloading, a pull also refuses a remote copy whose recorded
binding is another Store's. A failure keeps the temporary file and names it
in the report; nothing is deleted or tried again.

**Clone** (`remote::clone_store(driver, name, destination, cursor_key)`) is
the same download into a new Store: the destination must be absent, the
binding comes from the remote copy and is returned, and `cursor_key` is the
new Store's own authority (the Store checks only the binding). The file is
hard-linked into place, so a Store that appeared meanwhile is never
replaced, then its temporary name is removed.

## 9. Tests

- `layerfs-remote-storage/tests/folder_pieces.rs`: folder driver, pieces,
  manifest and verification.
- `layerfs-remote-storage/tests/minio.rs`: single PUT, ranged GET, multipart
  above the part size, pieces, and failed multiparts (a refused part, a lost
  part reply, a failed abort) through a fault proxy, against a pinned MinIO
  started by `tests/minio_service.sh`. The service tests are `#[ignore]`d.
  `multipart_direct_round_trip` uploads three parts with no proxy. With
  `MINIO_TLS_DIR` the script serves HTTPS with a fresh self-signed
  certificate, and the suite built `--features tls` runs over TLS; the fault
  proxy reads plain HTTP, so its four tests print that they were skipped.
- `layerfs-store-service/tests/remote_settings.rs`: settings round trip and
  refusals, Store names, the transfer state file, and R2 remotes (round
  trip, endpoint per jurisdiction, region `auto`, account refusals).
- `layerfs-api/tests/remote_store.rs` (Linux): a Store with Branches and
  Commits pushed and pulled into a fresh volume has identical history, over
  the folder remote and (ignored) MinIO; two Stores on one remote; an open
  Store refused before any download.

## 10. Not in this phase

The free-space refusal has no automated test (it needs a full filesystem).
The other providers (AWS S3, Google Cloud Storage, Google Drive, Dropbox,
OneDrive, Box); a run against a real R2 account (the TLS path is checked
against a TLS MinIO); SDK methods and a `layerfs` command for remotes; incremental
push (row bundles, #495); a PostgreSQL Store's remote (it pulls a SQLite
Store and imports it).
