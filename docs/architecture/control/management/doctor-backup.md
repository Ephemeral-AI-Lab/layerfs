# Backup connection diagnostics

[Doctor](doctor.md) · [Bucket drivers](../backups/buckets.md)

`doctor backup connection <alias>` reads one saved R2/MinIO alias and its
credential reference, using the same provider configuration, SigV4 signer,
TLS policy and HTTP codec as backup operations. No bucket creation, policy
change, backup enumeration, manifest change or existing-object mutation occurs.
Configuration failures and unavailable TLS support block connection checks.
Resolved credentials and arbitrary provider error bodies are never reported.

| Check | What is established |
| --- | --- |
| `backup.configuration` | Saved identity/version, provider, endpoint, region, bucket and prefix are usable |
| `backup.credentials` | The saved reference resolves; values remain private |
| `backup.transport` | Actual configured HTTP/TLS transport returned an HTTP response, or its observable failure phase |
| `backup.list` | One signed prefix-scoped ListObjectsV2 request with `max-keys=1`; validated returned keys are discarded |

A 403 does not independently prove authentication success or distinguish all
credential/policy causes. HTTP status and allowlisted S3 error codes are retained;
arbitrary response bodies and listed backup keys are not disclosed. Listing
success establishes neither object-read permission nor object-write permission.
The implementation never calls `Driver::list`, which collects all pages.

## Explicit temporary write test

`--write-test` additionally performs:

| Check | Operation and boundary |
| --- | --- |
| `backup.write_capability` | Read-only GetBucketVersioning must positively return `Enabled` before any PUT |
| `backup.put` | One small single-part PUT with `If-None-Match:*`, a cryptographically random 256-bit run token, and token metadata |
| `backup.reconcile` | Only after an uncertain PUT: GET the exact generated key; exact token/bytes plus a non-null version can establish ownership; never resend PUT |
| `backup.get` | GET only the acknowledged/reconciled immutable version, checking returned version, token and exact bytes |
| `backup.delete` | DELETE only that immutable owned version; require the response to identify that version |

Probe keys are `<saved-prefix>/_layerfs_doctor/<random-token>/probe`.
They do not match the backup discovery format `<file>/store.sqlite`, pieces,
or manifests. No bucket or real backup object is manufactured for the probe.
Probe ownership is bound to its endpoint, bucket and prefix; an opaque probe
from one destination cannot authorize cleanup in another destination.

Unversioned, suspended, denied or unsupported versioning means **no PUT**.
R2's S3 API does not provide the immutable versioning contract required by this
cleanup path, so it can complete connection/list checks while the temporary
write test remains unsupported. This does not mean R2 backup writes are broken.
The implementation does not assume conditional DELETE support from a provider
label: older MinIO versions ignored conditional-delete headers. It never uses
HEAD followed by unconditional DELETE or silently falls back to overwrite.

Collision/refused PUTs authorize no cleanup. Unexpected missing version IDs,
uncertain PUTs that cannot be reconciled, cleanup denial, cancellation or lost
DELETE responses leave explicit possible-retained-key/version evidence. Mutation server errors other than a definitive unsupported-operation refusal
remain uncertain: an error does not prove that no object was committed or removed. No mutation is retried. A failed GET does not suppress an independent safe cleanup
attempt. A lost DELETE reply remains uncertain even when the object may already
have been removed. Operator guidance names only that exact key/version.

The small-object path proves neither multipart support, restoration correctness,
database integrity nor blanket access to all bucket keys.

## Deadlines and verification

Default overall probe bounds are ten seconds, or thirty with `--write-test`,
shortened by the shared requested timeout. The absolute deadline covers DNS,
TCP and every socket read/write during TLS, HTTP headers and body, including
trickling peers. Buffered TLS/application reads also check the deadline.
The existing ordinary backup transfer timeout policy remains unchanged.

`layerfs-bridge::network` resolves numeric IPs directly and hostnames through a
bounded system resolver child (`dscacheutil` on macOS, `getent` on Linux), with
bounded nonblocking output and kill/reap cleanup. No abandoned resolver thread
or hostname replacement in TLS verification is used. Unsupported host resolvers
are reported rather than falling back to an unbounded resolver.

Fault peers exercise the exact signer/HTTP codec and provider semantics; they
are not claims of live R2 or MinIO qualification. Source:
`layerfs-control/src/doctor/backup_target.rs`,
`layerfs-bucket/src/bucket/s3_api/diagnostic.rs`, and the shared bridge resolver.

Provider contract references: [R2 S3 compatibility](https://developers.cloudflare.com/r2/api/s3/api/),
[MinIO conditional headers](https://docs.min.io/aistor/developers/s3-api-compatibility/),
[older MinIO conditional-delete behavior](https://github.com/minio/minio/issues/21677).
