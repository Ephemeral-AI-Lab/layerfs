# Pieces and manifest

[Architecture](../../README.md) › [Control](../README.md) › [Backups](README.md) › Pieces and manifest

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How a backup is split and checked.

| | |
| --- | --- |
| Component | [Backups](README.md) |
| Source | [`control/crates/layerfs-bucket/src/bucket/pieces.rs`](../../../../control/crates/layerfs-bucket/src/bucket/pieces.rs), [`control/crates/layerfs-bucket/src/bucket/progress.rs`](../../../../control/crates/layerfs-bucket/src/bucket/progress.rs), [`control/crates/layerfs-bucket/src/bucket/limits.rs`](../../../../control/crates/layerfs-bucket/src/bucket/limits.rs) |

## Pieces

```text
 file of B bytes, driver.limits() = { max_file, piece_size, ... }

 B <= max_file   (pieces(B) = 0)
   <name>                     the whole file
     layerfs-layout = whole, layerfs-sha256, layerfs-bytes, + caller's keys

 B >  max_file   (n = ceil(B / piece_size))
   <name>.piece-000000        piece_size bytes   meta: layerfs-sha256
   <name>.piece-000001        piece_size bytes   meta: layerfs-sha256
   ...
   <name>.piece-<n-1>         the rest           meta: layerfs-sha256
   <name>                     the manifest       (uploaded LAST)
     layerfs-layout = pieces, layerfs-sha256 (whole), layerfs-bytes (B),
     + caller's keys
```

`pieces::upload(driver, name, path, metadata)`:

| Step | What | Progress phase |
| --- | --- | --- |
| 1 | `limits.validate()`; refuse the reserved keys `layerfs-sha256`, `layerfs-layout`, `layerfs-bytes` in the caller's metadata. | — |
| 2 | Hash the whole file once. | `hash` |
| 3a | Fits: one `upload_file(name, whole file)`. | `upload` |
| 3b | Larger: for each piece, hash its span, then `upload_file(<name>.piece-NNNNNN, span)`. | `upload`, pieces `done/of` |
| 4b | Stage the manifest beside the source as `<path>.layerfs-manifest` (removed on drop) and upload it as `name`. | (quiet: not file bytes) |
| 5 | Remove pieces from index `n` on, left by an earlier, larger upload; a failure here is `Moved::cleanup`, the new file is complete. | — |

`pieces::delete(name)` deletes `name`, then its pieces from index 0 while
they exist. Piece names use six digits: `piece_name(name, index)`.

## sha256 manifest

```text
 layerfs-pieces 1
 bytes <B>
 sha256 <64 lowercase hex of the whole file>
 piece-size <piece_size>
 piece <bytes> <64 hex>          one line per piece, in order
 ...
```

`Manifest::decode` refuses (`Integrity("manifest: …")`) a wrong header, a
missing or misordered field, a bad number or digest, an extra word, or
piece sizes that are not `piece_size` for all but the last with the total
equal to `bytes`.

`pieces::download(driver, name, out)` checks, in order:

| # | Check | Failure |
| --- | --- | --- |
| 1 | `stat(name)` exists. | `Missing` |
| 2 | Its metadata has `layerfs-sha256` (64 hex). | `Integrity("<name> has no layerfs-sha256")` |
| 3 | `layerfs-layout` is `whole` or `pieces`. | `Integrity("<name> has no layerfs-layout")` |
| 4w | Whole: bytes received equal the stat's size. | `Integrity` |
| 4p | Pieces: the manifest is at most 16 MiB, valid UTF-8, decodes, and its sha256 equals the metadata's. | `Integrity` |
| 5p | Each piece: bytes and sha256 equal its manifest line, streamed into `out`. | `Integrity("<piece> does not match the manifest")` |
| 6 | The sha256 of everything written equals `layerfs-sha256`. | `Integrity("<name>: sha256 mismatch")` |

Nothing is atomic across requests. Pieces go up first and the manifest
last, so a manifest names only pieces already sent; a download racing an
upload fails a sha256 check instead of returning mixed bytes.
`backup::push` also refuses a Store whose manifest would pass 16 MiB,
counting at most 96 bytes per line (`MANIFEST_LINE` in
[`backup/create.rs`](../../../../control/crates/layerfs-bucket/src/backup/create.rs)).

## Progress

```text
 transfer thread                          reader thread
   progress::observe(Arc<Cell>, work)       cell.sample() -> Sample {
     begin(phase, total?)  ---------------->   phase, bytes, total,
     add(bytes)  (published <= 1 / 250 ms)     pieces: (done, of) }
     pieces(done, of)  (published at once)
```

| Phase | Reported by | Bytes |
| --- | --- | --- |
| `check` | `backup::push`, `backup::pull` | — |
| `snapshot` | `backup::push` | bytes copied |
| `hash` | `pieces::upload` | bytes hashed |
| `upload` | `pieces::upload`, the driver's request body | bytes sent |
| `download` | `pieces::download` | bytes received |
| `verify`, `install` | `backup::pull`, `backup::load` | — |

| Rule | Source |
| --- | --- |
| One transfer runs on one thread; reports go to that thread's `Progress`, if one is installed, else nowhere. No thread is added and nothing is retried. | [`progress.rs`](../../../../control/crates/layerfs-bucket/src/bucket/progress.rs) |
| `Cell` publishes byte counts at most once per `INTERVAL` (250 ms); a new phase or piece count at once. | same |
| `quiet` runs work with no progress (the manifest upload). | same |

`backup.create` and `backup.restore` install their own `Progress` for the
transfer thread instead of a `Cell`: it keeps each phase's exact last count
and every piece count, and the calling thread sends them with the call's
own phases (`check`, `snapshot`, `send`)
([`api/…/backup/progress.rs`](../../../../control/crates/layerfs-control/src/backup/progress.rs);
[SDK: backup progress](../../api/sdk/backup/README.md#progress)).

## Related

- [Buckets](buckets.md): the `Driver` and `Limits` these functions use.
- [Backup and restore](backup-and-restore.md): the Store-level metadata added on top.

---

← [Backups: Buckets](buckets.md) · [Up: Backups](README.md) · [Backups: Backup and restore](backup-and-restore.md) →
