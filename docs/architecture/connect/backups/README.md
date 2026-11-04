# Backups

[Architecture](../../README.md) › [Connect](../README.md) › Backups

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Whole-Store backup to a bucket and restore, over bucket drivers that know nothing about Stores.

| | |
| --- | --- |
| Layer | [Connect](../README.md) |
| Crates | `layerfs-store-backup` |
| Source | [`connect/crates/layerfs-store-backup/src/backup`](../../../../connect/crates/layerfs-store-backup/src/backup), [`connect/crates/layerfs-store-backup/src/bucket`](../../../../connect/crates/layerfs-store-backup/src/bucket); the calls in [`api/crates/layerfs-control/src/backup`](../../../../api/crates/layerfs-control/src/backup) |
| Uses | [Engine: Store](../../engine/store/README.md), [Engine: Store calls](../../engine/store-calls/README.md) |
| Used by | [API: Control](../../api/control/README.md) |
| Flows | [Backup and restore](../../flows/backup.md) |

## Topics

| Topic | What it covers |
| --- | --- |
| [Buckets](buckets.md) | Bucket drivers and providers. |
| [Pieces and manifest](pieces-and-manifest.md) | How a backup is split and checked. |
| [Backup and restore](backup-and-restore.md) | Whole-Store backup and restore. |

## The picture

```text
 caller process (holds the bucket key; every bucket request runs here)
+---------------------------------------------------------------------+
| backup/  (knows Stores)              bucket/  (knows only files)    |
|  create  snapshot of a live Store     pieces::upload / download     |
|   (Store's machine stages it) ------>  sha256, whole or pieces +    |
|  restore load: download, verify,       manifest, progress           |
|   open as a Disposable Store <------   Driver (sealed trait)        |
|  copies  stat / list Store files        |  local_folder  (tests)    |
|  heads   history heads digest           |  minio --+                |
|  link    remotes, last transfer         |  r2 -----+-> s3_api       |
+-----------------------------------------|---------------------------+
                                          v  each request sent once
                    bucket:  <prefix>/<file>/store.sqlite
                             [<prefix>/<file>/store.sqlite.piece-000000 …]
```

`bucket/` holds files by name and nothing else; `backup/` turns a Store
into one such file and back. Terms: [glossary](../../glossary.md).

## What it does

| Call | Copies | When | Source |
| --- | --- | --- | --- |
| `backup.create` | One consistent snapshot of a live sqlite Store, every project in it, as `<file>/store.sqlite` with its sha256, size, layout, SQLite version, history heads, binding and `layerfs-exported-from`. | Checked against the expected copy before the snapshot and again before the upload. | [`api/…/backup/create.rs`](../../../../api/crates/layerfs-control/src/backup/create.rs), [`bucket/pieces.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/pieces.rs) |
| `backup.restore` | Downloads the file, verifies every piece and the whole file, opens it as a Disposable Store, then installs it as a sqlite Store on `native`. | New Store: hard link. Replacing: only an idle Store unchanged since its last transfer through that bucket. | [`api/…/backup/restore.rs`](../../../../api/crates/layerfs-control/src/backup/restore.rs), [`backup/restore.rs`](../../../../connect/crates/layerfs-store-backup/src/backup/restore.rs) |
| `backup.list` | Stats and lists Store files without downloading. | Read live. | [`backup/copies.rs`](../../../../connect/crates/layerfs-store-backup/src/backup/copies.rs) |

| Provider | Feature | Transport | Source |
| --- | --- | --- | --- |
| `local_folder` | `local_folder` (default) | files under a directory | [`local_folder/folder.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/local_folder/folder.rs) |
| MinIO | `minio` (default, enables `s3_api`) | S3 API, `http://`, or `https://` with `tls` | [`minio/provider.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/minio/provider.rs) |
| Cloudflare R2 | `s3_api`; needs `tls` to connect | S3 API over `https://` | [`r2/provider.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/r2/provider.rs) |

A PostgreSQL Store is not backed up here: `backup.create` refuses it
`InvalidInput`, "back it up with PostgreSQL's tools".

## Invariants

| Rule | Enforced by |
| --- | --- |
| Each bucket request is attempted once. A sent request with an unknown outcome is `Uncertain` and never resent. | [`bucket/error.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/error.rs), [`s3_api/http.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/s3_api/http.rs) |
| A kept connection is checked before any byte is written, so taking one is not a resend. | [`s3_api/connection.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/s3_api/connection.rs) |
| A restore checks the manifest, every piece and the whole file against their sha256 before the bytes are used. | `pieces::download` ([`pieces.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/pieces.rs)) |
| A restored file must open as a Disposable Store with its binding and leave no `-wal`/`-shm`. | `fetch` ([`restore.rs`](../../../../connect/crates/layerfs-store-backup/src/backup/restore.rs)) |
| A manifest goes up only after all its pieces. | `pieces::upload` |
| Settings and records hold credential references, never secrets; `Credentials` `Debug` is redacted. | [`credentials.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/credentials.rs) |
| Only this crate's providers implement `Driver` (sealed). | `sealed::Sealed` ([`driver.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/driver.rs)) |
| Bucket I/O runs in the caller's process; a worker never sees the key. | [connect guide](../../../../connect/AGENTS.md); [`api/…/backup/create.rs`](../../../../api/crates/layerfs-control/src/backup/create.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| One object / one request / piece size | MinIO 50 TiB / 5 TiB / 5 TiB; R2 4.995 TiB / 4.995 GiB / 4.995 TiB | [Buckets](buckets.md#limits) |
| Multipart part size | 5 MiB to 5 GiB, at most 10,000 parts; raised to whole MiB when needed | [`s3_api/multipart.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/s3_api/multipart.rs) |
| Listing pages | At most 10,000 pages of up to 1,000 names, then refused | `MAX_LIST_PAGES` in [`driver.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/driver.rs) |
| Stats per Store listing | 64 (`MAX_LIST_STATS`); copies beyond show no sha256 or heads | [`copies.rs`](../../../../connect/crates/layerfs-store-backup/src/backup/copies.rs) |
| Manifest | At most 16 MiB (`MAX_MANIFEST`) | [`pieces.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/pieces.rs) |
| Fails before a byte is sent | `Unavailable` | [`s3_api/http.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/s3_api/http.rs) |
| Fails after a byte is sent | `Uncertain`, not resent | same |
| A multipart upload fails | aborted; if the abort fails too, `PartsLeft { upload }` names the parts that may remain | [`multipart.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/s3_api/multipart.rs) |
| A piece upload fails part-way | the pieces sent so far stay; the name still holds the previous file or manifest, and a previous manifest whose pieces were overwritten fails its sha256 check on download | [`pieces.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/pieces.rs) |
| Not atomic across requests | no compare-and-swap; a download racing an upload fails its sha256 check | [`backup/mod.rs`](../../../../connect/crates/layerfs-store-backup/src/backup/mod.rs) |

## Demonstration

The commands were not run while writing this page.

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`minio.rs`](../../../../connect/crates/layerfs-store-backup/tests/minio.rs) | `layerfs-store-backup` | Against a running MinIO server: single PUT, ranged GET, HEAD, DELETE; multipart above the part size; a refused part aborts and leaves no parts; a failed abort reports `PartsLeft`; a lost part reply is `Uncertain`, not resent; a file over the limit goes up as pieces; listing and last-modified. |
| [`copies.rs`](../../../../connect/crates/layerfs-store-backup/tests/copies.rs) | `layerfs-store-backup` | One stat tells a copy's size, sha256 and heads; one listing finds every Store copy, pieced ones at their whole size, and skips other files. |
| [`folder_pieces.rs`](../../../../connect/crates/layerfs-store-backup/tests/folder_pieces.rs) | `layerfs-store-backup` | The folder driver (whole, ranged, stat, delete); whole-file round trip; pieces with a manifest, verified; manifest round trip and refusals; reserved metadata keys refused. |

- `minio`: `part_sizes_and_config_refusals` runs anywhere. The others are
  `#[ignore = "needs MinIO: tests/minio_service.sh"]`: start one with
  `connect/crates/layerfs-store-backup/tests/minio_service.sh start NAME`,
  eval its `LAYERFS_REMOTE_*` exports, then
  `cargo +1.85.1 test --locked -p layerfs-store-backup --test minio -- --ignored`.
  With `--features tls` and an `https://` MinIO the same tests run over TLS,
  except the fault-injection ones.
- `copies`: a `LocalFolder` with a 1000-byte object limit under
  `CARGO_TARGET_TMPDIR`; any host.
  `cargo +1.85.1 test --locked -p layerfs-store-backup --test copies`
- `folder_pieces`: same setup.
  `cargo +1.85.1 test --locked -p layerfs-store-backup --test folder_pieces`

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`docs/versioned/2.0.0/backups.md`](../../../versioned/2.0.0/backups.md):
  the user contract of the `backup` calls stays there; the bucket layer,
  pieces and the checks are covered in [Buckets](buckets.md),
  [Pieces and manifest](pieces-and-manifest.md) and
  [Backup and restore](backup-and-restore.md).

---

← [Sandboxes: Adding a backend](../sandboxes/adding-a-backend.md) · [Up: Connect](../README.md) · [Backups: Buckets](buckets.md) →
