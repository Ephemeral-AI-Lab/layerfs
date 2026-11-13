# Buckets

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The bucket drivers that hold whole files by name, and how a file is split,
checked and reported, within [Backups](README.md).

| | |
| --- | --- |
| Source | [`control/crates/layerfs-bucket/src/bucket`](../../../../control/crates/layerfs-bucket/src/bucket); features in [`Cargo.toml`](../../../../control/crates/layerfs-bucket/Cargo.toml) |

## Driver

```text
 trait Driver: sealed::Sealed + Send + Sync                driver.rs
   limits()                                -> Limits { max_file, max_request,
                                                       piece_size }
   upload_file(name, path, span?, metadata)   replaces <name>
   download_file(name, span?, out)         -> bytes written
   stat(name)                              -> Option<Stat { bytes, metadata,
                                                            modified }>
   delete(name)                               absent is not an error
   list(prefix)                            -> Vec<Listed>, name order
```

| Rule | Value | Source |
| --- | --- | --- |
| Sealed | Only this crate's providers implement `Driver`; a new provider is a folder under `bucket/`. | [`driver.rs`](../../../../control/crates/layerfs-bucket/src/bucket/driver.rs) |
| One attempt | Each call sends each of its requests once; a sent request with an unknown outcome is `Uncertain`. | same |
| Names | Slash-separated segments of `[A-Za-z0-9._-]`, none empty or starting with `.`, at most 512 bytes (`check_name`). | same |
| Metadata | Keys 1 to 64 of `[a-z0-9-]`; values at most 256 printable ASCII bytes, no space (`check_metadata`). | same |
| Span | `offset + length` within the file, else `Invalid` (`open_span`). | same |
| Listing | One paged pass, at most `MAX_LIST_PAGES` (10,000), then `Invalid`; `Listed.metadata` is `None` where a stat is needed. | same |
| Limits | `validate` refuses a zero value or `piece_size > max_file`; `pieces(bytes)` is 0 when the file fits one object, else `ceil(bytes / piece_size)`. | [`limits.rs`](../../../../control/crates/layerfs-bucket/src/bucket/limits.rs) |
| Time | `Last-Modified` and listing `LastModified` as milliseconds since the epoch, anything else `None`. | [`time.rs`](../../../../control/crates/layerfs-bucket/src/bucket/time.rs) |

`RemoteError` ([`error.rs`](../../../../control/crates/layerfs-bucket/src/bucket/error.rs)):

| Variant | Meaning | Sent? |
| --- | --- | --- |
| `Invalid` | bad input or configuration | no |
| `Unavailable` | provider not reached ("remote unavailable (nothing sent)") | no byte |
| `Refused { status, code }` | the provider answered a failure ("remote refused: HTTP 403 AccessDenied") | yes |
| `Missing` | the named file is absent ("remote file missing") | yes |
| `TooLarge { bytes, max }` | larger than the provider or layout allows | no |
| `Integrity` | bytes do not match their size or sha256 ("remote file integrity: …") | yes |
| `Local` | a local file operation failed | — |
| `Uncertain` | sent, outcome unknown ("remote outcome unknown (not resent)") | yes |
| `PartsLeft { cause, upload, abort }` | a part failed and so did the abort: the object was not replaced, but parts may remain | yes |
| `Unsupported` | not in this build, e.g. an `https://` endpoint without `tls` | no |

## Pieces and manifest

```text
 B <= max_file:  <name>                 whole file
                   layerfs-layout=whole, layerfs-sha256, layerfs-bytes,
                   +caller's
 B >  max_file:  <name>.piece-000000    piece_size bytes   meta layerfs-sha256
                 ...
                 <name>.piece-<n-1>     the rest           meta layerfs-sha256
                 <name>                 manifest, uploaded LAST
                   layerfs-layout=pieces, layerfs-sha256 (whole),
                   layerfs-bytes (B), +caller's

 manifest:  layerfs-pieces 1 / bytes <B> / sha256 <hex> / piece-size <P>
            piece <bytes> <hex>         one line per piece, in order
```

| Step | Upload (`pieces::upload`) | Download (`pieces::download`) |
| --- | --- | --- |
| 1 | `limits.validate()`; refuse the reserved keys `layerfs-sha256`, `layerfs-layout`, `layerfs-bytes` (`Invalid`). | `stat`: absent is `Missing`; no 64-hex `layerfs-sha256` or no `layerfs-layout` is `Integrity`. |
| 2 | Hash the whole file once (`hash`). | Whole: bytes received must equal the stat's size. |
| 3 | Fits: one `upload_file`. Larger: hash each span, upload it as `<name>.piece-NNNNNN` with its sha256. | Pieces: manifest at most 16 MiB (`MAX_MANIFEST`), UTF-8, decodes, its sha256 equals the metadata's. |
| 4 | Stage the manifest as `<path>.layerfs-manifest` (removed on drop) and upload it as `<name>`, without progress. | Each piece's bytes and sha256 equal its manifest line, streamed into the output. |
| 5 | Delete pieces from index `n` on while they exist (left by an earlier, larger upload); a failure is `Moved::cleanup`, the new file is complete. | The sha256 of everything written equals `layerfs-sha256`. |

`Manifest::decode` refuses a wrong header, a missing or misordered field, a
bad number or digest, an extra word, or piece sizes that are not `piece_size`
for all but the last with the total equal to `bytes`. Nothing is atomic across
requests: because the manifest goes last it names only pieces already sent, so
a download racing an upload fails a sha256 check instead of returning mixed
bytes; a previous manifest whose pieces were overwritten fails the same way.
`pieces::delete` removes the name, then its pieces from index 0. Source:
[`pieces.rs`](../../../../control/crates/layerfs-bucket/src/bucket/pieces.rs).

## Progress

```text
 transfer thread                          reader thread
   observe(Arc<dyn Progress>, work)         Cell::sample() -> Sample {
     begin(phase, total?) ----------------->  phase, bytes, total,
     add(bytes)    (<= 1 per 250 ms)          pieces: (done, of) }
     pieces(done, of)   (at once)
```

| Rule | Source |
| --- | --- |
| A transfer runs on one thread; reports go to that thread's installed `Progress`, else nowhere. No thread is added and nothing is retried. | [`progress.rs`](../../../../control/crates/layerfs-bucket/src/bucket/progress.rs) |
| `Cell` publishes byte counts at most once per `INTERVAL` (250 ms); a new phase or piece count at once. | same |
| Phases: `hash` and `upload` from an upload (bytes sent by the driver's request body), `download` from a download; `quiet` runs the manifest upload unreported. | same; [`pieces.rs`](../../../../control/crates/layerfs-bucket/src/bucket/pieces.rs) |

The `backup` calls install their own `Progress` that keeps each phase's exact
last count ([Backups](README.md)).

## Credentials

```text
 record:  "env:LAYERFS_…"                 "file:/absolute/path"
               |                                   |
 resolve()     v                                   v
          <PREFIX>_ACCESS_KEY             lines access_key=…, secret_key=…
          <PREFIX>_SECRET_KEY             mode & 0o077 must be 0
               \______________ _______________/
                              v
             Credentials { access_key, secret_key }
             Debug: "Credentials([redacted])"
```

| Rule | Source |
| --- | --- |
| An `env:` prefix starts with `LAYERFS_` and holds only `[A-Z0-9_]`: LayerFS reads only its own settings. | [`credentials.rs`](../../../../control/crates/layerfs-bucket/src/bucket/credentials.rs) |
| A `file:` path is absolute; a file readable by group or others is refused `Invalid`. | same |
| Keys must be non-empty printable ASCII; a missing variable is `Invalid` "… is not set". | same |
| `connect` resolves the reference and sends nothing. | [`minio/provider.rs`](../../../../control/crates/layerfs-bucket/src/bucket/minio/provider.rs), [`r2/provider.rs`](../../../../control/crates/layerfs-bucket/src/bucket/r2/provider.rs) |

## Local folder

```text
 <root>/objects/<name>      the file's bytes
 <root>/metadata/<name>     its metadata
 upload: write <name>.uploading (create-new), then rename into place
```

`LocalFolder::new(root, limits)` takes an existing directory and any valid
`Limits`, so tests force pieces with a small `max_file`; its listing carries
metadata. As a `Provider::LocalFolder` its limits are `u64::MAX`. Feature
`local_folder` (default). Source:
[`local_folder/folder.rs`](../../../../control/crates/layerfs-bucket/src/bucket/local_folder/folder.rs),
[`link.rs`](../../../../control/crates/layerfs-bucket/src/backup/link.rs).

## MinIO and R2

| | MinIO | R2 |
| --- | --- | --- |
| Feature | `minio` (default; enables `s3_api`) | `s3_api`; connects only with `tls` |
| Endpoint | `http://host:port`, or `https://` with `tls`; path-style | `https://<account>[.<jurisdiction>].r2.cloudflarestorage.com` |
| Settings | `endpoint`, `bucket`, `prefix`, `region`, `credentials`, `part_size` | `account` (32 lowercase hex), `jurisdiction` (empty, `eu`, `fedramp`), `bucket`, `prefix`, `credentials`, `part_size` |
| Region | as configured (`us-east-1` by default) | always `auto` |
| Timeouts | connect 5 s, socket I/O 60 s | connect 10 s, socket I/O 60 s |
| `max_file` (one object) | 48.83 TiB: one multipart upload, `MAX_PARTS * MAX_PART`, below MinIO's 50 TiB | 5 TiB − 5 GiB (4.995 TiB) |
| `max_request` (one PUT or part) | 5 TiB | 5 GiB − 5 MiB (4.995 GiB) |
| `piece_size` | 5 TiB | 4.995 TiB |
| Part size | 5 MiB to 5 GiB; default 64 MiB (`DEFAULT_PART_SIZE`) | 5 MiB to 4.995 GiB; above is `Invalid` before anything is sent |
| Source | [`minio/provider.rs`](../../../../control/crates/layerfs-bucket/src/bucket/minio/provider.rs) | [`r2/provider.rs`](../../../../control/crates/layerfs-bucket/src/bucket/r2/provider.rs) |

## S3 API

```text
 S3Client::upload_file(name, path, span, metadata)          client.rs
   span > limits.max_file ...........................  TooLarge
   span > part_size or > max_request -> multipart       multipart.rs
     create -> parts 1..n (equal, last smaller) -> complete
     any failure -> abort (a new request) -> the cause, or
       PartsLeft (a part failed), or Uncertain (completion unknown)
   else one PUT, SHA-256 payload hash, x-amz-meta-* headers

 every request                                              http.rs
   take a kept connection (idle < 10 s and quiet) or connect once
   failure before the first byte written ...........  Unavailable
   failure after it ................................  Uncertain
   4xx sent before the server stopped reading body ..  Refused
   reply read to its exact end -> connection kept (at most 4)
```

| Part | What | Source |
| --- | --- | --- |
| Config | Bucket 3 to 63 of `[a-z0-9.-]`, alphanumeric at both ends; prefix a valid name; endpoint `http://` or `https://` with no path or credentials; part size within `MIN_PART`..`MAX_PART` and at most `max_request`. All refused before anything is sent. | [`config.rs`](../../../../control/crates/layerfs-bucket/src/bucket/s3_api/config.rs) |
| Signing | Header SigV4 (`AWS4-HMAC-SHA256`) with `x-amz-content-sha256` of the payload. | [`sign.rs`](../../../../control/crates/layerfs-bucket/src/bucket/s3_api/sign.rs) |
| Requests | Hand-written HTTP/1.1; streamed file and reply bodies in 256 KiB buffers; headers at most 16 KiB; error and XML replies read up to 4 MiB; a body buffer that takes longer than the I/O timeout to send is `Uncertain`. | [`http.rs`](../../../../control/crates/layerfs-bucket/src/bucket/s3_api/http.rs) |
| Keep-alive | No `Connection: close` is sent. A kept connection older than 10 s or with anything to read is closed before any byte is written, so taking one is never a resend. TLS is `native-tls` against the system roots. | [`connection.rs`](../../../../control/crates/layerfs-bucket/src/bucket/s3_api/connection.rs) |
| Parts | 5 MiB (`MIN_PART`) to 5 GiB (`MAX_PART`), at most 10,000 (`MAX_PARTS`); `part_size` raises the configured size to whole MiB when a file would need more parts. | [`multipart.rs`](../../../../control/crates/layerfs-bucket/src/bucket/s3_api/multipart.rs) |
| Reads and lists | Ranged GET with `content-range` checked, HEAD, DELETE, ListObjectsV2 with continuation tokens; `pending_uploads` and `abort_upload` for an operator. | [`client.rs`](../../../../control/crates/layerfs-bucket/src/bucket/s3_api/client.rs) |
| Diagnostics | For [Doctor](../doctor/README.md): one authenticated list page (`max-keys=1`); with versioning positively enabled, conditional create, read and version-owned delete of probes under `_layerfs_doctor`; never a full enumeration, never a resend. `diagnostic_within` bounds them by one absolute deadline. | [`diagnostic.rs`](../../../../control/crates/layerfs-bucket/src/bucket/s3_api/diagnostic.rs), [`deadline.rs`](../../../../control/crates/layerfs-bucket/src/bucket/s3_api/deadline.rs) |

## Tested providers

| Fact | MinIO | R2 |
| --- | --- | --- |
| Tested against | the pinned Chainguard image (by digest) that [`minio_service.sh`](../../../../control/crates/layerfs-bucket/tests/minio_service.sh) starts, over HTTP; HTTPS with `MINIO_TLS_DIR` set (self-signed, exported as `SSL_CERT_FILE`) and `--features tls` | a real account, region `auto` |
| Build | plain HTTP needs no feature | `cargo +1.85.1 build --locked -p layerfs-cli --features tls` |
| The bucket | must exist; LayerFS never creates or deletes a bucket | same |
| Bucket missing or not visible | `HTTP 404 NoSuchBucket` | `HTTP 403 AccessDenied` |
| Pieces | exercised only through the local folder with small limits, not at provider scale | same |

Whatever the provider, a wrong secret answers `HTTP 403 SignatureDoesNotMatch`
and an unknown or revoked key `HTTP 403 InvalidAccessKeyId`, both as `Failed`;
the secret is never printed. A network that drops mid-transfer is noticed within
the 60 s I/O timeout. The S3 actions each call needs follow from the requests
it sends:

| Call | Requests | Actions |
| --- | --- | --- |
| `backup.connect`, `backup.list` | ListObjectsV2, HEAD | `s3:ListBucket`, `s3:GetObject` |
| `backup.restore` | HEAD, GET | `s3:GetObject` |
| `backup.create` | HEAD, PUT or multipart create, part, complete; abort on failure | `s3:PutObject`, `s3:AbortMultipartUpload` |
| stale-piece removal after a smaller upload | HEAD, DELETE | `s3:DeleteObject` |

A key with read access only connects, lists and restores; its create is
refused `Failed: remote refused: HTTP 403 AccessDenied`.

## Related

- [Backups](README.md): the Store-level calls that use these drivers.
- [Home](../home/README.md): where bucket records and credential files live.
- [Doctor](../doctor/README.md): the bucket diagnostics.
- [SDK: backup](../../api/sdk/backup.md): the call contract.
