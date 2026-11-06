# Back up a Store to MinIO or Cloudflare R2

> **Status:** Current general guide.

This guide takes a clean Linux home to a Store backed up in a bucket and
restored again with the `layerfs` CLI, on MinIO or on Cloudflare R2. Every
command and output below was run on both providers, except where a step
says otherwise.

## What was tested

| Item | Value |
| --- | --- |
| LayerFS | branch `layerfs_v2_0_0`, commit `c51d04b942` (PR [#579](https://github.com/Ephemeral-AI-Lab/layerfs/pull/579)); the failure matrix ran at `282ea970e8`, and the cases the last commit affects were run again at `c51d04b942` |
| CLI build | `cargo +1.85.1 build --locked -p layerfs-cli --features tls` (debug), Linux arm64 in Docker |
| Host | Linux. Store calls (`store`, `project`, `file`, `backup`) are Linux only |
| MinIO | `cgr.dev/chainguard/minio@sha256:4692462f…`, plain HTTP on a Docker network |
| R2 | a real account, bucket `layerfs-stores`, region `auto` |
| Evidence | [backup-verify report](https://github.com/Ephemeral-AI-Lab/layerfs-benchmark/blob/main/result/2026-11-06/backup-verify/c51d04b9428c4d56df690223ced1e3fe20f73dc2/report.md) in layerfs-benchmark, with its harness `backup-verify/` |

A backup is one object, `<prefix>/<file>/store.sqlite`, holding the whole
sqlite Store (every project, branch, commit and file in it). Its metadata
records the file's sha256, the Store's heads and binding, and the home
that wrote it.

## 1. Build the CLI with TLS

R2 and any `https://` MinIO need the `tls` feature; the tested build had
it. Only plain-HTTP MinIO works without it.

```bash
cargo +1.85.1 build --locked --release -p layerfs-cli --features tls
export PATH="$PWD/target/release:$PATH"
export LAYERFS_HOME="$HOME/.layerfs"    # any empty folder for a clean home
```

## 2. Prepare the bucket

### MinIO

The S3 API listens on port 9000; the admin console (9001) is not the
endpoint. Create the bucket first: LayerFS never creates or deletes buckets.

```bash
# example; the tests used the pinned image above
docker run -d --name minio -p 9000:9000 \
  -e MINIO_ROOT_USER=<admin> -e MINIO_ROOT_PASSWORD=<admin-secret> \
  minio/minio server /data
mc alias set lab http://127.0.0.1:9000 <admin> <admin-secret>
mc mb lab/layerfs-test
```

Give LayerFS its own user, not the root user. The policy below is the
smallest that lets every backup call work (tested; see
[permissions](#permissions)):

```bash
cat > layerfs-backup.json <<'EOF'
{"Version": "2012-10-17", "Statement": [{"Effect": "Allow",
  "Action": ["s3:ListBucket", "s3:GetObject", "s3:PutObject",
             "s3:AbortMultipartUpload", "s3:ListMultipartUploadParts"],
  "Resource": ["arn:aws:s3:::layerfs-test", "arn:aws:s3:::layerfs-test/*"]}]}
EOF
mc admin user add lab <access-key> <secret-key>
mc admin policy create lab layerfs-backup layerfs-backup.json
mc admin policy attach lab layerfs-backup --user <access-key>
```

For HTTPS, give MinIO a certificate (its `certs` folder) and use an
`https://` URL; the CLI checks it against the system's CA store
(`SSL_CERT_FILE` names another bundle).

### Cloudflare R2

1. In the Cloudflare dashboard, create the bucket (R2 › Create bucket).
   Note its jurisdiction if you chose EU or FedRAMP.
2. Create an R2 API token (R2 › Manage API tokens) with **Object Read &
   Write**, limited to that bucket. Copy its Access Key ID and Secret
   Access Key; Cloudflare shows the secret once.
3. Note your account ID (32 hex characters, on the R2 overview page).

The URL is `r2://<account>/<bucket>/<prefix>`; add
`?jurisdiction=eu` or `?jurisdiction=fedramp` for those buckets.

## 3. Give the CLI the key

`--key env:NAME` reads `NAME_ACCESS_KEY` and `NAME_SECRET_KEY` from the
environment of each call; `--key file:/absolute/path` reads a private file.
The home records only the reference, never the secret.

```bash
umask 077
cat > ~/.layerfs-bucket.env <<'EOF'
export LAYERFS_LAB_ACCESS_KEY=<access-key>
export LAYERFS_LAB_SECRET_KEY=<secret-key>
EOF
. ~/.layerfs-bucket.env
```

Every later `backup` call needs those variables set.

## 4. Make a Store to back up

```bash
mkdir -p ~/demo && echo hello > ~/demo/README.md
layerfs store create shop
layerfs project init shop/api ~/demo
layerfs branch fork shop/api dev main
echo "edited on dev" | layerfs file write shop/api dev /README.md
layerfs branch publish shop/api dev
```

`store create` answers
`{"name":"shop","kind":"sqlite","machine":"native","profile":"disposable",…}`.
`main` is read-only; changes go on a branch and `branch publish` moves
them to `main`.

## 5. Connect, back up, list

```bash
layerfs backup connect lab http://127.0.0.1:9000/layerfs-test/backups --key env:LAYERFS_LAB
# R2:  layerfs backup connect lab r2://<account>/<bucket>/backups --key env:LAYERFS_LAB
```

MinIO answers
`{"name":"lab","provider":"minio","endpoint":"http://127.0.0.1:9000","bucket":"layerfs-test","prefix":"backups","region":"us-east-1","key":"env:LAYERFS_LAB","part_size":16777216}`;
R2 answers `"provider":"r2","account":"<account>","jurisdiction":"",…,"region":"auto"`.
Connect lists the bucket once, so a wrong key, endpoint or bucket fails
here (see [troubleshooting](#troubleshooting)).

```bash
layerfs backup create shop lab --progress
layerfs backup list lab
```

`create` answers
`{"store":"shop","bucket":"lab","file":"shop","object":"shop/store.sqlite","bytes":225280,"pieces":0,"sha256":"…","heads":"…"}`
and `list` shows the copy with `"store":"shop","state":"current"`.

## 6. Restore and inspect

Restore under a new name, or into a fresh home to prove the bucket alone is
enough (that is what was tested):

```bash
export LAYERFS_HOME="$HOME/.layerfs-restore"      # empty
layerfs backup connect lab <same URL> --key env:LAYERFS_LAB
layerfs backup restore shop lab shop --progress
layerfs branch list shop/api
layerfs file read shop/api dev /README.md
```

`restore` answers
`{"store":"shop","bucket":"lab","file":"shop","bytes":225280,"sha256":"…","heads":"…","projects":["api"]}`.
The restored Store has the same projects, branches, commits and tree roots
as the source, and takes new commits. Restoring over an existing Store
works only for an idle Store unchanged since its last backup or restore
through that bucket; otherwise it is refused `Conflict` (back it up first
or restore under a new name).

`file read` answers text files only; to check binary files, compare commit
`root` hashes (`commit list`), which cover every byte.

## 7. Disconnect

```bash
layerfs backup disconnect lab
```

It answers `{"disconnected":"lab","unlinked":["shop"]}`: the home forgets
the bucket and the Stores' links to it. **The objects stay in the bucket.**
LayerFS never deletes bucket objects; remove them with the provider's tools.

## Progress

`--progress` prints one JSON line per update on stderr, at most every
250 ms and at the end of each phase:

```text
{"phase":"upload","done":5242880,"total":51105792,"pieces":null}
```

| Call | Phases |
| --- | --- |
| `create` | `check`, `snapshot`, `send` (a Store behind a worker only), `check`, `hash`, `upload` |
| `restore` | `check`, `download`, `verify`, `install` |

A byte phase ends on its exact total. A failed call ends on what it moved
and exits 125; only exit 0 means success. A restore can move every byte
and still be refused at `verify` or `install`.

## Sizes and parts

| | MinIO | R2 |
| --- | --- | --- |
| Largest object (one multipart upload) | 48.83 TiB (10,000 × 5 GiB) | 4.995 TiB |
| Part size (`--part-size`, K/M/G) | 5 MiB – 5 GiB | 5 MiB – 4.995 GiB |
| Above the largest object | pieces plus a manifest | pieces plus a manifest |
| Largest tested | 1 GiB (create 31 s, restore 10 s, local Docker) | 64 MiB (create 19 s, restore 3 s) |

A file above the part size goes up as a multipart upload; each part must
fit the provider's single-request limit, so `--part-size 5G` is refused on
R2. Create needs free local space for about two copies of the Store (the
snapshot and its upload; measured 1–2× the Store size); restore needs one. Peak memory stayed under
30 MiB at every tested size. Pieces were not tested at TiB scale.

## Replacing and keeping backups

`backup create shop lab` replaces `shop/store.sqlite` each time; the
previous copy is gone once the new one completes. To keep older copies,
back up to another file name (`backup create shop lab shop-2026-11-06`)
or turn on bucket versioning. There is no LayerFS retention policy.

Create refuses to overwrite a copy it did not expect: `Exists` when another
home wrote it, `Conflict` when it is not the copy this home last moved.
Two creates of one Store in one home: the second is refused `Busy`.

## Permissions

Tested on MinIO with scoped users:

| Key allows | connect | list | restore | create / replace |
| --- | --- | --- | --- | --- |
| `ListBucket`, `GetObject` | yes | yes | yes | refused: `HTTP 403 AccessDenied` |
| + `PutObject`, `AbortMultipartUpload`, `ListMultipartUploadParts` | yes | yes | yes | yes |
| + `DeleteObject` | yes | yes | yes | yes |

`DeleteObject` is needed only to remove pieces left by an earlier, larger
upload. On R2, an **Object Read only** token corresponds to the first row
and **Object Read & Write** to the others; R2 token scopes were not tested.

**Rotating a key:** create the new key, update the variables or the file
the reference names, and the next call uses it: the key is read on each
call, so no reconnect is needed while the reference stays the same (read
from source; the swap itself was not run). Then revoke the old key. A revoked
key fails with `Failed: remote refused: HTTP 403 InvalidAccessKeyId`; the
secret is never printed.

## Troubleshooting

| Refusal | Meaning |
| --- | --- |
| `Failed: remote refused: HTTP 403 SignatureDoesNotMatch` | wrong secret |
| `Failed: remote refused: HTTP 403 InvalidAccessKeyId` | unknown or revoked access key |
| `Failed: remote refused: HTTP 404 NoSuchBucket` (MinIO), `HTTP 403 AccessDenied` (R2) | bucket missing, or the key cannot see it |
| `Failed: remote unavailable (nothing sent): …` | endpoint not reached (host, port, TLS); nothing was sent |
| `InvalidInput: multipart part size … outside …` / `above the provider's request limit` | `--part-size` out of range |
| `Failed: no Store <file> on the remote: remote file missing` | no backup of that name under the prefix |
| `Exists: … written by another home` | another home owns that file; use another file name |
| `Conflict: …` | the bucket copy or the local Store changed since this home's last transfer |
| `Busy: another backup.create of Store … is running` | wait for it to finish |
| `Failed: remote file integrity: … sha256 mismatch` | the object is damaged; nothing installed, the existing Store is untouched |
| `Unknown: …` (create) | the request was sent and its answer lost; the object may or may not have been replaced. LayerFS never resends. Run `backup list` and restore the copy to check it |

A network that drops mid-transfer is noticed within the request timeout
(60 s); a server that closes the connection fails at once.

## Cleaning up

| Left behind | By | Clean up |
| --- | --- | --- |
| Incomplete multipart uploads | a create killed or cut mid-upload | `mc ls --incomplete lab/layerfs-test/backups/shop/store.sqlite` (MinIO lists them by exact key) and `mc rm --incomplete --recursive --force …`; R2 aborts them after 7 days, or add a lifecycle rule |
| `<store>.backup` beside the Store | a killed create | removed by the next create of that Store |
| `store/.restore-<random>/` | a killed restore | removed by the next restore in the home |
| Piece objects (`<file>/store.sqlite.piece-NNNNNN`) | only objects above the largest single object | remove only when no manifest you keep names them |

Delete test objects with `mc rm --recursive --force lab/layerfs-test/backups/`
(MinIO) or the R2 dashboard. Disconnecting a bucket deletes nothing.

## Known limits

- Two homes creating the same file at the same moment are not serialized:
  the bucket has no compare-and-swap, so the last writer wins. Use one file
  per home.
- A backup copies the Store as of one read transaction while writers carry
  on; the restored history is one consistent point (tested with 148
  concurrent commits).
- Durable Stores are not run; backups were tested on Disposable Stores.
