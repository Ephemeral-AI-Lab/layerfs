# Buckets

[Architecture](../../README.md) › [Connect](../README.md) › [Backups](README.md) › Buckets

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Bucket drivers and providers.

| | |
| --- | --- |
| Component | [Backups](README.md) |
| Source | [`connect/crates/layerfs-store-backup/src/bucket`](../../../../connect/crates/layerfs-store-backup/src/bucket) |

A bucket driver holds files by name. It knows nothing about Stores.

## Driver contract

```text
 trait Driver: sealed::Sealed + Send + Sync          driver.rs
   limits()                                -> Limits
   upload_file(name, path, span?, metadata)          replaces <name>
   download_file(name, span?, out)         -> bytes written
   stat(name)                              -> Option<Stat{bytes,metadata,
                                                          modified}>
   delete(name)                                      absent is not an error
   list(prefix)                            -> Vec<Listed> in name order
```

| Rule | Value | Source |
| --- | --- | --- |
| Sealed | Only this crate's providers implement it; a new provider is a folder under `bucket/` (#535 G2). | [`driver.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/driver.rs) |
| One attempt | Each call sends each of its requests once; a sent request with an unknown outcome is `Uncertain`. | same |
| Names | Slash-separated segments of `[A-Za-z0-9._-]`, none empty or starting with `.`, at most 512 bytes. | `check_name` |
| Metadata | Keys 1 to 64 of `[a-z0-9-]`; values at most 256 printable ASCII bytes (no space). | `check_metadata` |
| Span | `offset + length` within the file, else `Invalid`. | `open_span` |
| Listing | One paged pass, at most `MAX_LIST_PAGES` (10,000); a remote that cannot list refuses `Unsupported`. | same |

`RemoteError` ([`error.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/error.rs)):

| Variant | Meaning | Sent? |
| --- | --- | --- |
| `Invalid` | bad input or configuration | no |
| `Unavailable` | provider not reached | no byte |
| `Refused { status, code }` | the provider answered a failure | yes |
| `Missing` | the named file is absent | yes |
| `TooLarge { bytes, max }` | larger than the provider or layout allows | no |
| `Integrity` | bytes do not match their recorded size or sha256 | yes |
| `Local` | a local file operation failed | — |
| `Uncertain` | sent, outcome unknown; never resent | yes |
| `PartsLeft { cause, upload, abort }` | a part failed and so did the abort: the object was not replaced, but parts may remain (a completion of unknown outcome whose abort fails is `Uncertain` instead) | yes |
| `Unsupported` | not in this build (for example TLS) | no |

## Local folder

```text
 <root>/objects/<name>             the file's bytes
 <root>/metadata/<name>            its metadata
 upload: write <name>.uploading, then rename into place
```

`LocalFolder::new(root, limits)` takes an existing directory and any valid
`Limits`, so tests can force pieces with a small `max_file`. As a home's
`Provider::LocalFolder` its limits are `u64::MAX`
([`link.rs`](../../../../connect/crates/layerfs-store-backup/src/backup/link.rs)).
Source: [`local_folder/folder.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/local_folder/folder.rs).

## MinIO

| Setting | Meaning |
| --- | --- |
| `endpoint` | `http://host:port`; `https://` with the `tls` feature |
| `bucket`, `prefix` | an existing bucket; an optional namespace in it |
| `region` | SigV4 region |
| `credentials` | a `CredentialRef` |
| `part_size` | multipart part size; `DEFAULT_PART_SIZE` is 64 MiB |

Path-style requests; connect timeout 5 s, socket I/O timeout 60 s. Limits:
one PUT up to 5 TiB; parts of 5 MiB–5 GiB, at most 10,000, so the largest
single object is 48.83 TiB (`MAX_PARTS * MAX_PART`, below MinIO's 50 TiB
object limit); a larger file goes up as 5 TiB pieces.
Source: [`minio/provider.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/minio/provider.rs).

## R2

| Setting | Meaning |
| --- | --- |
| `account` | 32 lowercase hex digits |
| `jurisdiction` | empty, `eu` or `fedramp` |
| endpoint | `https://<account>.r2.cloudflarestorage.com`, or `https://<account>.<jurisdiction>.r2.cloudflarestorage.com` |
| region | always `auto` |
| `bucket`, `prefix`, `credentials`, `part_size` | as MinIO |

Connect timeout 10 s, socket I/O timeout 60 s. R2 needs the `tls` feature
to connect. Limits: objects up to 4.995 TiB (a larger file goes up as
4.995 TiB pieces); one request, and so one multipart part, up to 4.995 GiB:
a `part_size` above that is refused `Invalid` before anything is sent. Source: [`r2/provider.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/r2/provider.rs).

## S3 API

```text
 S3Client::upload_file(name, path, span, metadata)        client.rs
   span > limits.max_file ----------------------------> TooLarge
   span > part_size or > max_request -----> multipart:     multipart.rs
        create -> part 1..n (equal size, last smaller) -> complete
        part_size above limits.max_request -------------> Invalid
        any failure -> abort (a new request) -> cause, or
          PartsLeft (a part failed), or Uncertain (the completion's
          outcome is unknown: the object may have been replaced)
   else -> one PUT, SHA-256 payload hash, x-amz-meta-* headers

 every request                                             http.rs
   take a kept connection (idle <= 10 s and quiet) or connect once
   failure before the first byte written ............ Unavailable
   failure after it ................................. Uncertain (no resend)
   body send reset or closed by the server, with a 4xx
   reply already sent (a key without write access) ... Refused, definite
   each read or write: io_timeout (60 s); a request body buffer
   (256 KiB) that takes longer than io_timeout to send ... Uncertain
   reply read to its exact end -> connection kept (at most 4)
```

| Part | What | Source |
| --- | --- | --- |
| Signing | Header-based SigV4 with a SHA-256 payload hash; canonical query for multipart. | [`sign.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/s3_api/sign.rs) |
| Requests | Hand-written HTTP/1.1; streamed file body and reply body; headers at most 16 KiB; error and XML replies read up to 4 MiB. | [`http.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/s3_api/http.rs) |
| Keep-alive | No `Connection: close` (on R2 it ended about one ranged reply in a hundred early). A kept connection is closed instead of used when older than 10 s or when it has anything to read. | [`connection.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/s3_api/connection.rs) |
| Parts | 5 MiB (`MIN_PART`) to 5 GiB (`MAX_PART`), at most 10,000 (`MAX_PARTS`); `part_size` raises the configured size to whole MiB when a file would need more parts. | [`multipart.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/s3_api/multipart.rs) |
| Reads and lists | Ranged GET, HEAD, DELETE, ListObjectsV2 with continuation tokens. | [`client.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/s3_api/client.rs) |
| Config checks | Bucket 3 to 63 of `[a-z0-9.-]`, alphanumeric at both ends; endpoint has no path or credentials; refused before anything is sent. | [`config.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/s3_api/config.rs) |
| Time | `Last-Modified` and listing `LastModified` as milliseconds since the epoch, else `None`. | [`time.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/time.rs) |

## Credentials

```text
 settings / records:  "env:LAYERFS_…"   or   "file:/absolute/path"
                              |                       |
 CredentialRef::resolve       v                       v
                  <PREFIX>_ACCESS_KEY      lines access_key=… secret_key=…
                  <PREFIX>_SECRET_KEY      file mode & 0o077 must be 0
                              \__________ _____________/
                                         v
                    Credentials { access_key, secret_key }
                    Debug: "Credentials([redacted])"
```

| Rule | Source |
| --- | --- |
| An `env:` prefix starts with `LAYERFS_` and holds only `[A-Z0-9_]`: LayerFS reads only its own settings. | [`credentials.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/credentials.rs) |
| A `file:` path is absolute; a file readable by group or others is refused. | same |
| Keys must be non-empty printable ASCII. | same |
| Settings keep the reference; `connect` resolves it and sends nothing. | `connect` in [`minio/provider.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/minio/provider.rs), [`r2/provider.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/r2/provider.rs) |

The API resolves the reference in the caller on each call and records only
the reference ([2.0.0 manual: Keys](../../../versioned/2.0.0/backups.md#keys)).

## Limits

| Provider | `max_file` (one object) | `max_request` (one PUT) | `piece_size` | Source |
| --- | --- | --- | --- | --- |
| MinIO | 50 TiB | 5 TiB | 5 TiB | [`minio/provider.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/minio/provider.rs) |
| R2 | 5 TiB − 5 GiB (4.995 TiB) | 5 GiB − 5 MiB | 5 TiB − 5 GiB | [`r2/provider.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/r2/provider.rs) |
| `local_folder` | caller's (`u64::MAX` as a home provider) | same | same | [`link.rs`](../../../../connect/crates/layerfs-store-backup/src/backup/link.rs) |

`Limits::validate` refuses a zero value or `piece_size > max_file`;
`Limits::pieces(bytes)` is 0 when the file fits one object, else
`ceil(bytes / piece_size)` ([`limits.rs`](../../../../connect/crates/layerfs-store-backup/src/bucket/limits.rs)).

## Related

- [Pieces and manifest](pieces-and-manifest.md): what goes over a driver for a Store file.
- [Backup and restore](backup-and-restore.md): who calls the drivers.
- [Network: Credentials](../network/credentials.md): the PostgreSQL counterpart of the reference rule.

---

← [Connect: Backups](README.md) · [Up: Backups](README.md) · [Backups: Pieces and manifest](pieces-and-manifest.md) →
