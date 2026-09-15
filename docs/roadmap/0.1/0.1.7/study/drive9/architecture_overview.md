# Drive9 architecture overview

> **Status:** Research; informative and not a product contract.

Pin: upstream `https://github.com/mem9-ai/drive9` (Apache-2.0, Go 1.25+);
local snapshot `/Users/yifanxu/Ephemeral-AI-Lab/study/drive9` at commit
`cff6d294b452b54a964de2217c137af303602be9` (2026-09-05, "feat(fuse): add S3
Express append-log WAL support (#883)"). Written 2026-09-16 for
[#157](https://github.com/Ephemeral-AI-Lab/layerfs/issues/157), one of the
three v0.1.7 external studies (Drive9, Cloudflare Computer, AgentFS). This
study reads only the pinned snapshot; every load-bearing claim carries a
citation (`path:line` for code, `doc#section` for docs). Performance figures
are quoted only from Drive9's own material and are marked as such.

**Naming collision, stated once.** Drive9 has its own feature it calls
"LayerFS" / "layered filesystem" — the `drive9 fs layer` commands and
`docs/design/layered-filesystem-*.md`. Throughout this document that feature
is called **Drive9 layers** and is *unrelated* to the LayerFS product this
study serves. Drive9 was also renamed from "dat9"; legacy `Dat9` type names
and `X-Dat9-*` wire headers survive the rename deliberately
(README.md:269-273, docs/guides/go-sdk-integration.md:21-31).

## The system in one paragraph

Drive9 is a server-side workspace kernel for AI agents: a fleet of Go HTTP
servers that keeps agent *working state* — the files, logs, and artifacts a
sandbox is currently producing — as a first-class runtime object, mounted
into any sandbox via FUSE (or WebDAV) and manipulated through a CLI, five
SDKs, and a plain HTTP API. Its own framing (README.md:16-50): "Sandboxes are
disposable. Workspaces should not be." Local sandbox disks run the process,
Git stores the final result, and Drive9 keeps everything in between —
durable across sandbox replacement, forkable for parallel attempts,
checkpointed and rollback-safe, with server-side conflict detection when two
attempts from the same base both commit. The server owns all authority: a
central MySQL/TiDB control-plane database plus one database per tenant plus
S3-compatible object storage for large content; clients never see database
credentials (pkg/meta/meta.go:545-853, pkg/tenant/pool.go:273-370). The
early design doc (docs/design-overview.md, 2026-03-26 "Proposal (Draft v2)")
framed the product as a "network drive with built-in semantic search" built
on db9; the code has moved well past that framing — db9 is one of four
tenant-database providers, and the center of gravity is now the workspace
kernel (layers, git workspaces, vault, journal, mounts). Where doc and code
disagree, code wins; the divergences are noted where they matter.

## Component map

```text
drive9/ @ cff6d29
├── cmd/
│   ├── drive9/            CLI (hand-rolled switch dispatch, stdlib flag; main.go:96)
│   ├── drive9-server/     the server binary (single HTTP listener, :9009 default; main.go:46)
│   ├── drive9-migration/  EBS→Drive9 bulk data migration worker (K8s Jobs)
│   ├── kubectl-drive9-migration/  operator status plugin for the migration
│   ├── e2e-aggregate, feishu-notify  CI helpers
├── pkg/
│   ├── server/            HTTP handlers, auth, SSE, event bus, fork (tenant), fs layers,
│   │                      git workspaces, journal, vault, quota, admin, object GC,
│   │                      pod registry, semantic sharding        (~50 files)
│   ├── datastore/         per-tenant metadata store over MySQL/TiDB (or PG for db9):
│   │                      file_nodes/inodes/contents/semantic, uploads, fs_events,
│   │                      fs_layer_*, git_workspace_*, journals, vault_*
│   ├── backend/           Dat9Backend: AGFS FileSystem over datastore + S3; uploads,
│   │                      PATCH, batch write, quota, mutation log, file GC, layer content
│   ├── s3client/          S3-compatible object clients (AWS/Aliyun/Tencent/local dev)
│   ├── meta/              control-plane schema (tenants, keys, quota, pods, outbox)
│   ├── tenant/            tenant lifecycle: 4 providers (tidb_zero, tidb_cloud_native,
│   │                      db9, local), connection pool, schema init, tokens
│   ├── fuse/              Dat9FS: go-fuse RawFileSystem bridging kernel ⇄ HTTP API;
│   │                      write buffers, shadow store, WAL journal, commit queue,
│   │                      caches, append-log, layer/git-workspace integration
│   ├── mountsupervisor/   supervisor process managing the FUSE worker (flock, restarts)
│   ├── mountstate/        pidfiles, control sockets, process identity (PID+creation time)
│   ├── mountcontrol/      drain/status/ping control-socket protocol
│   ├── mountpath/         remote-path parsing (":/repo" syntax)
│   ├── objectfs/          rclone-backed object-store mounts (s3/cos/tos/oss/gcs/azure)
│   ├── gitcache/          rebuildable clean-tree/blob caches for fast-clone workspaces
│   ├── gitwsindex/        wire type for the git-workspace existence index
│   ├── gitws (in fuse)    git-workspace FUSE runtime (manifest + overlay + .git checkpoints)
│   ├── journal/           audit-journal envelope: canonical JSON, SHA-256 hash chain
│   ├── vault/             per-tenant scoped secrets (DEK envelope, grants, audit)
│   ├── encrypt/           control-plane envelope encryption (local AES / AWS / Aliyun / Tencent KMS)
│   ├── webdav/            client-side WebDAV mount adapter over the Go client
│   ├── leader/            MySQL GET_LOCK-based leader election for background workers
│   ├── tenantctx/         tenant-id context plumbing (getter currently has no consumer)
│   ├── client/            the Go SDK — also the CLI's and FUSE's own transport
│   ├── semantic/, embedding/  embedding/extract task types and OpenAI-compatible client
│   ├── parser/, treebuilder/  interface-only stubs (P9 smart-parser concept)
│   ├── metrics/, logger/, traceid/, mysqlutil/, pathutil/, pathfilter/, ...
├── clients/               drive9-js (near-complete), drive9-py, drive9-rs,
│                          drive9-kotlin, drive9-swift (core-only tiers)
├── internal/migration/    the EBS→Drive9 migration engine (sync/dual-write/fence phases)
├── internal/schemaspec, testtidb, e2ereport, feishu
├── e2e/                   bash-driven live-server suites (FUSE gates, git gates, layers)
├── blackbox/, examples/, obsidian-plugin/, scripts/, tools/
```

Stack and deployment shape: one language (Go 1.25.1, go.mod:3); FUSE via a
pinned fork `github.com/mornyx/go-fuse/v2` ("inode/nlookup behavior. Pin the
replace; do not drop it", go.mod:164-166); rclone for object-store mounts
(pkg/objectfs/doc.go:1-12); AGFS's `FileSystem` interface implemented by the
backend (pkg/backend/dat9.go:83-181). What runs where: **server** — one or
more `drive9-server` pods behind nothing exotic (plain HTTP/1.1 + JSON +
SSE; no gRPC), each pod able to serve all requests, coordinating only
through control-plane tables; **sandbox/host** — the CLI, the supervised
FUSE worker, the WebDAV loopback adapter, all talking HTTP to the server;
nothing of Drive9's server runs inside the sandbox. Storage dependencies:
one central meta DB (MySQL/TiDB), one user DB per tenant (provider-specific,
pkg/tenant/provider.go:6-24), S3-compatible object storage (local-directory
dev mode served on `/s3/`, pkg/server/server.go:543-552).

## Authority and data flow

### Where state lives, and who may write it

Three planes, all server-written (pkg/meta/meta.go:545-853;
pkg/tenant/schema/tidb_app.go:68-224):

1. **Control plane (central meta DB)**: tenants (DB coordinates, encrypted
   password, provider, kind live/fork, parent), API keys (envelope-encrypted
   JWT ciphertext + SHA-256 hash, token_version, fs scopes), storage
   namespaces, quota config/usage, the durable `quota_mutation_log`,
   `object_gc_candidates`, `tenant_delete_jobs`, `pod_registry`,
   `tenant_notify_outbox`, admin pools. Tenant DB credentials are stored
   encrypted and never handed to clients (pkg/meta/meta.go:553-557).
2. **Per-tenant user DB**: the filesystem itself — `file_nodes` (dentry:
   path → inode, unique `path_hash`), `inodes` (size, `revision`, mode,
   status PENDING/CONFIRMED/DELETED), `contents` (`storage_type`
   db9|s3, `storage_ref`, inline `content_blob`, checksum),
   `semantic` (text + VECTOR embeddings + FTS), `uploads`, `fs_events`
   (monotonic change log), plus the feature tables: `fs_layer_*` (Drive9
   layers), `git_workspace_*`, `journals*`, `vault_*`,
   `semantic_tasks`, `file_gc_tasks`. The legacy monolithic `files` table
   is still detected and migrated lazily on pool acquire
   (pkg/datastore/store.go:183-201, pkg/migrate/split_tables.go:17-31,
   docs/design/metadata-schema-refactor.md#1).
3. **S3-compatible object storage**: content ≥ the inline threshold as
   `blobs/<ULID>`; layer payloads as `layers/<layerID>/<ULID>`
   (pkg/backend/dat9.go:917-928, pkg/backend/fs_layer_content.go:39).
   `storage_ref_hash` is a hash of the *location*, not the content — there
   is no content-addressed dedup; every create/overwrite mints a fresh ULID
   object (pkg/datastore/storage_ref_hash.go:9-14, pkg/backend/dat9.go:903,
   1207).

The one authority exception: an owner token can run arbitrary SQL against
its own tenant DB via `POST /v1/sql` (pkg/server/server.go:6107-6146);
scoped tokens are denied (pkg/server/server.go:1673-1674).

### Concurrency model

Client concurrency is mediated optimistically, not with locks: every
overwrite carries `X-Dat9-Expected-Revision` and the store CASes on the
inode's `revision` inside a transaction (`SELECT revision … FOR UPDATE`,
compare, bump; pkg/datastore/file_tx.go:108-146; header at
pkg/server/server.go:2427-2429). Layer writes serialize on the layer row
(`SELECT … FOR UPDATE` in `UpsertFSLayerEntry`, pkg/datastore/fs_layer.go:591-680),
which also serializes fork vs. parent writes
(pkg/datastore/fs_layer_chain.go:201-203). Quota admission is deliberately
optimistic across pods — writes commit, then a durable `quota_mutation_log`
is replayed by a leader-gated worker, with a documented crash window of
possible undercount (docs/quota-meta-runtime-cutover.md#correctness-tradeoff).
Fleet-wide coordination is tables-only: pods heartbeat into `pod_registry`
(10 s, stale after 30 s; pkg/server/pod_registry.go:13-54); a MySQL
`GET_LOCK`-based leader (`SELECT GET_LOCK(?,0)` on a pinned connection, with
`CONNECTION_ID()` capture and `IS_USED_LOCK` heartbeat as split-brain
defense, pkg/leader/leader.go:277-358) gates singleton background work
(object GC, tenant-delete cleanup, quota replay, reconcilers) — leadership
never gates request serving; per-tenant sharded work (semantic, file GC) is
routed by jump-consistent hash over the pod ring (pkg/server/semantic_shard.go:15-43);
cross-pod notification runs through `tenant_notify_outbox` with 200 ms
coalescing (motivated by "46M inserts/12h under stress" — Drive9's own code
comment, pkg/server/notify_coalescer.go:15-45).

### Write path (plain files, no layer)

Small file (< `DefaultInlineThreshold` = 50,000 bytes; one threshold
deliberately governs both server inline-vs-S3 *and* client PUT-vs-multipart,
so the server never becomes a data-plane proxy — pkg/backend/dat9.go:37-55):
checksum + text extraction inline, blob written into `contents.content_blob`
(`storage_ref="inline"`), then one tenant-DB transaction inserts inode +
content + dentry + tags + semantic task; on transaction failure the blob is
deleted (pkg/backend/dat9.go:847-1082). Overwrite writes a *new* blob
`blobs/<genID>` then CASes metadata; the old S3 ref becomes a GC candidate,
never an immediate delete (pkg/backend/dat9.go:1084-1352, 1905-1953).
Large file: V2 multipart — server reserves quota centrally first, creates
the multipart upload, presigns part URLs; the client uploads parts directly
to S3 (no data through the server); complete runs `CompleteMultipartUpload`
then one metadata transaction (pkg/backend/upload.go:430-638, 989-1118).
PATCH assembles clean parts server-side via `UploadPartCopy` on a new key
(pkg/backend/patch.go:151-330). GC is a two-stage deferred pipeline: durable
`file_gc_tasks` (lease, retries), then a leader-gated object-GC worker that
deletes only after a 7-day `NotBefore` grace, checks that no confirmed
metadata row still references the object, and postpones while forks exist
(pkg/backend/file_gc_worker.go:17-99, pkg/server/object_gc_worker.go:102-176).

### Read path and the mount lifecycle

A mount is a purely client-side session over a remote subtree — there is no
server-side mount entity (verified: no `mounts` DDL or struct in `pkg/`).
`drive9 mount --mode=fuse :/repo ./work` spawns a supervised FUSE worker
that bridges kernel ops to the HTTP API. Reads resolve through a long local
ladder before hitting the network: local-only overlay files → git-workspace
local handles → open-unlinked snapshots → shadow-spilled buffers → dirty
write buffers → same-path dirty handles → flush-staged shadow → write-back
cache snapshots ("close-to-open consistency", pkg/fuse/dat9fs.go:11886-11905)
→ prefetcher → in-memory LRU ReadCache (revision-validated, 128 MB/30 s
defaults) → persistent disk read cache (CRC-verified, keyed by
FileID+Revision+Offset+Length, credential-hashed so different credentials
never share bytes, pkg/fuse/mount.go:441-452) → remote range reads, split
into parallel 1 MiB fetches (pkg/fuse/dat9fs.go:452-458). An SSE watcher on
`/v1/events` (`file_changed`, `reset`, `heartbeat`; monotonic `seq` with
explicit reset semantics, pkg/server/sse.go:51-56, 819-859) invalidates
caches and pushes kernel entry notifications, self-filtering events that
carry the mount's own random actor ID (pkg/fuse/sse.go:25-131).

Writes stage locally before reaching the server, under an explicit
durability profile `--durability auto|interactive|fsync|close-sync|write-sync`
(docs/design/fuse-durability-policy.md#cli): `write(2)` lands in an
in-memory sparse `WriteBuffer` (8 MB parts, 64 MB per-file cap) with
best-effort spill to a host-durable per-path shadow file; `Flush` (in the
default writeback profile) stages shadow + appends a WAL journal frame and
returns — "The actual HTTP upload happens in Release (async). This reduces
Flush latency from ~100-300ms to ~1-5ms" (Drive9's own comment,
pkg/fuse/dat9fs.go:13010-13013); a commit queue (bounded, per-path
serialized, backpressure at 500 pending) performs the conditional upload
with the revision observed at open as the CAS base. fsync means
remote-durable in strict profiles and local-shadow-durable in the
interactive profile — with two hard exceptions that always force remote
sync: SQLite `*-wal`/`*-journal` sidecars, and append-log-configured paths
(pkg/fuse/dat9fs.go:13422-13442). Crash recovery replays the local journal
into a pending index at next mount and re-enqueues persisted pending
commits (pkg/fuse/journal.go:240-286, pkg/fuse/commit_queue.go:662-663).
There is no in-place "reconnection": a supervisor restart is a cold remount,
made data-safe by the stable cache-dir-keyed staging stores
(docs/design/fuse-mount-supervision.md#49).

Supervision: `drive9 mount` → SUPERVISOR (long-lived, flock-serialized) →
WORKER ("Worker must NOT setsid — remain child of supervisor",
pkg/mountsupervisor/supervisor.go:613-614). The worker has an honest typed
exit contract (0 external-umount/signal … 3 serve-abnormal, 4 panic,
5 permanent startup failure, 6 transient; pkg/fuse/mount_exit.go:8-96); the
supervisor restarts with exponential backoff and a circuit breaker (5
restarts/10 min → force-unmount and idle resident), cleans stale mounts
before respawn ("after SIGKILL the kernel keeps a dead FUSE superblock",
pkg/mountsupervisor/supervisor.go:414-439), and health-checks *locally only*
— "never readdir/remote List. Backend outage while mounted is degraded
status, not FUSE death" (pkg/mountsupervisor/supervisor.go:573-599). An
external `drive9 mount ensure` reconciles orphaned trees (adopt or remount).
FUSE platforms: Linux + macOS; Windows falls back to WebDAV
(cmd/drive9/cli/fuse_bridge_windows.go). Drive9's own POSIX report claims
8,941/8,941 passes across pjdfstest (8,798), LTP (133), flock, pyxattr, fsx
(-N 5000), fio (3 profiles), and mdtest (docs/posix-compatibility-report.md#summary)
— a vendor-reported figure; the report lists no environment details and no
explicit unsupported-behavior section.

## Implementation details

### The layered-filesystem (Drive9 layers) mechanism

The V1 design (2026-06-03) and the CoW-fork spec (2026-08-13, "Canonical")
are the two governing documents
(docs/design/layered-filesystem-v1-design.md,
docs/design/layered-filesystem-cow-fork-design.md). The model:

- **Layer** = one agent session/attempt: an `fs_layers` row (state
  `active | sealed | committing | committed | abandoned | conflicted`;
  `durability_mode`; `base_root_path`; chain columns `parent_layer_id`,
  `origin_seq`, `origin_checkpoint_id`, `root_layer_id`, `depth`)
  (pkg/tenant/schema/fs_layer.go:12-108, pkg/datastore/fs_layer.go:14-87).
- **Content** = an append-only op log, `fs_layer_entries`, PK
  `(layer_id, path_hash, entry_seq)`, `op ∈ upsert|whiteout|mkdir|symlink|chmod|rename`,
  carrying `base_inode_id`/`base_revision` claims for CAS, inline
  `content_blob` or S3 `storage_ref` above a 95 MiB client inline threshold
  (pkg/client/fs_layer.go:18), and `entry_seq` allocated `MAX+1` inside the
  upsert transaction (pkg/datastore/fs_layer.go:644-653). Views use
  latest-per-path; commit/restore use ordered replay — so `upsert → chmod`
  and `upsert → rename` sequences never collapse
  (docs/design/layered-filesystem-v1-design.md:328).
- **Resolver**: local dirty → layer entry (chain-folded for children) →
  base/main fallback; whiteout hides; readdir merges main children + layer
  children (docs/design/layered-filesystem-v1-design.md#resolver-semantics;
  pkg/datastore/fs_layer_overlay.go:28-34).
- **Checkpoint** = a named pointer to an `entry_seq` boundary plus
  `durable_seq` advancement — no data is copied
  (pkg/datastore/fs_layer.go:801-829). Mounting `--checkpoint` is read-only;
  writable history is `fork --checkpoint` (rule D10).
- **Fork** = O(1) metadata child: one transaction locks the parent row,
  requires `active|sealed`, and pins `origin_seq = MAX(parent.entry_seq)` —
  deliberately *not* `durable_seq` (rule D1) — or pins at a checkpoint's
  `durable_seq` (D2); depth soft-capped at 8, hard 16 (D15)
  (pkg/datastore/fs_layer_chain.go:43-166). The pin freezes the parent
  *overlay*, not main: sibling commits into main remain visible to the
  child through the main fallback (D7, §5.5). Chain reads fold
  root→tip on the server, never client-side multi-hop (D8, §5.6).
- **Delete/GC**: logical `abandoned` only; physical deletion is a future
  GC worker gated on the transitive `still_pins` predicate (D17, §5.8).

The separate **tenant fork** (`drive9 ctx fork` / `POST /v1/fork`)
provisions an entire new tenant whose database is a branch of the source
(TiDB Cloud branch API) — the "heavy" fork. On share-mode tenants it
returns 409 pointing at layer fork (rule D9, pkg/server/fork.go:317-332).

### Commit and conflict detection

`drive9 fs layer commit <ref>` → `POST /v1/layers/{ref}/commit`
(pkg/server/fs_layer.go:873-977), in order:

1. Idempotent fast path if already `committed`; state must be
   `active|sealed`; a conditional `UPDATE … WHERE state IN (active,sealed)`
   CASes into `committing` — the fence against duplicate concurrent commits
   (pkg/datastore/fs_layer.go:440-465).
2. Build the apply set: a child layer materializes the folded chain and
   diffs against **live main** (rules D18/D19 — "apply set ≡ effective view
   − live main", claims recomputed from live main at commit time); a root
   replays its own ordered log (pkg/server/fs_layer.go:890-904,
   pkg/datastore/fs_layer_overlay.go:340-412).
3. Scope validation (paths inside `base_root_path`), then preflight.
4. **Preflight conflict rules** (pkg/server/fs_layer.go:1623-1716), per
   entry, against live main: rename — source must exist, target must *not*
   (no-replace); directory whiteout — directory must be empty;
   new-file claim (`base_revision==0 ∧ base_inode_id==""`) — path must not
   exist ("base path exists"); inode claim — inode/file id must match;
   revision claim — `main.revision == entry.base_revision`, else
   `"base revision changed"` with both current and wanted values in the 409
   body (`fsLayerCommitConflict{Path, Reason, BaseRevision, WantRevision}`,
   pkg/server/fs_layer.go:116-121). Detection is per-path and
   revision/identity-based — *not* whole-tree and *not* content-based
   (checksums are used only to *skip* already-matching entries in child
   flattening, pkg/server/fs_layer.go:1513-1552); failure granularity is the
   whole commit (all-or-nothing).
5. Pre-commit snapshot of every touched main path; ordered apply
   (creates/renames shallow-first, whiteouts deep-first), each upsert a
   revision-CAS write; on any failure, snapshot-based best-effort rollback
   of the already-applied entries, layer → `conflicted`.
6. Final CAS `committing → committed`.

Two honest limitations, stated in Drive9's own docs: the commit "does not
claim single-transaction atomicity across DB/filesystem mutations" and
needs a deferred apply-ledger/owner-lease for crash-safe resumption
(docs/design/layered-filesystem-v1-design.md:494, cow-fork rule D11). A
layer left in `committing` by a crash has **no public API escape** — commit,
rollback, and delete all reject that state; a fully implemented
`recoveringCommit` mode exists but no production caller passes `true`
(pkg/server/fs_layer.go:921, tests at fs_layer_test.go:518-545).

A **conflicted** layer is terminal for writing but preserved: status, diff
(entry list), rollback (→ `abandoned`), and delete all work; there is no
merge/rebase/resolve API anywhere — deliberately (spec non-goal,
cow-fork-design.md:33). The client-side twin: layer upload conflicts in the
FUSE commit queue are terminal failures, never last-writer-win — "Keep
layer conflicts terminal" (pkg/fuse/commit_queue.go:2416-2423), and the
non-layer LWW auto-resolve is explicitly bypassed for layer mounts.

There is also a **write-time synchronous CAS**: an entry upsert with
`base_revision > 0` re-stats live main inside the insert transaction and
rejects on mismatch, "while the writer can still retry" — so a stale
copy-up fails immediately instead of escalating the whole layer at commit
preflight (pkg/datastore/fs_layer.go:619-643).

### The S3 Express append-log WAL (commit #883, newest feature)

Scope: this repo holds only the **client/FUSE half**; the server contract
and S3 Express lifecycle live in an external `tidbcloud/fs` repo
(docs/design/s3-express-append-log-fuse.md:13) — verified absent from this
snapshot's server (no `?append-log` handler in
pkg/server/server.go:2113-2135). The problem: a strict fsync of a growing
file (the SQLite WAL workload) previously re-uploaded the entire file; the
goal is "only the newly appended tail … without weakening SQLite
`synchronous=FULL` durability" (design doc:15-17). Drive9's own
diagnostics: a pre-optimization fsync rewrote a 20,632-byte WAL; checkpoint
reads at a first boundary served from local shadow in 22-38 µs per 4 KiB
pread while a second boundary fell back to remote reads costing 44 ms to
1.12 s each (design doc §3.3-3.4).

Mechanism: operator-declared path patterns (`--append-log <pattern>`,
reusing the `--local-only` pathfilter) plus a cached server capability
`append_log_v1` from `/v1/status` — both required (§2.1-2.2). Routing for
a configured file is fixed: proven SQLite-WAL generation reset → strict
tail append (`POST /v1/fs/{path}?append-log`, typed errors
`append_log_rebased|conflict|unsupported|too_large`, exactly one
re-stat retry on `rebased`) → layout-aware conditional full-body PUT for
resets and non-tail rewrites; one-way fallback, never a retry loop (§4).
The generation reset validates a 32-byte SQLite WAL header (magic, format
version 3007000, page size, salts, checksum — explicitly "not a WAL frame
parser", pkg/fuse/sqlite_wal.go:5-54) and publishes a new 32-byte
generation by conditional PUT, after which the first frame write is a tail
append again; a fresh local shadow of exactly those 32 bytes keeps
checkpoint reads local ("a current-process read cache, not
crash-recovery data", §3.2). No persistent schema, no background worker,
no second commit state machine (§1). Append-log is mutually exclusive with
layer mounts (pkg/fuse/append_log.go:1062-1064).

### Git fast-clone workspaces

A Git-native content model: the clean tree is a **Git manifest**
(`git_workspace_tree_nodes`: path, kind, mode, `object_sha`, size) — clean
blobs are *not* stored in Drive9; dirty state is a Drive9 overlay
(`git_workspace_overlay`); `.git` stays local, checkpointed to the server
as sanitized `tar.gz-no-objects` state plus inline packs of local-only
objects (SHA-256-verified, 5 MiB/blob, 256 MiB/pack caps)
(pkg/datastore/git_workspace.go:47-123, pkg/fuse/git_workspace.go:3353-3453;
docs/design/git-fast-clone-workspace.md#data-model). `drive9 git clone
--fast --blobless` clones with `--filter=blob:none --no-checkout`, uploads
the manifest, then hydrates locally: GitHub repos fetch the codeload
tarball and re-import blobs via batched `git hash-object` verified against
manifest SHAs (pkg/gitcache/cache.go:613-894). Mount-side discovery is a
DORMANT→ARMED state machine that eliminates idle API polling entirely —
armed by local markers or the remote index, refreshed only on events
(docs/design/git-workspace-fuse-poll-optimization.md#1-#7).

### Journal, vault, semantic, migration (brief)

- **Journal**: a per-tenant hash-chained append-only audit log — each entry
  hashes canonical JSON *including* `prev_hash`; appends are idempotent by
  `Idempotency-Key` with stored-response replay under a row lock; verify
  recomputes the whole chain (pkg/journal/journal.go:612-643,
  pkg/datastore/journal.go:213-299, 390-483). Seals/artifacts are declared
  but unimplemented in this snapshot.
- **Vault**: per-tenant scoped secrets; envelope encryption (32-byte master
  key wrapping per-tenant DEKs, AES-GCM), grants as HMAC-SHA256 JWTs signed
  with a tenant-derived key, explicit scope lists, TTL, audit
  (pkg/vault/crypto.go:15-51, grant.go:10-27, scope.go:7-39); surfaced as an
  API and as a FUSE mount where a secret is a directory and a key a file.
- **Semantic/embedding**: durable `semantic_tasks` (embed, L0/L1 generation,
  image/audio/video extraction) drained by sharded workers; or TiDB
  database-managed auto-embedding via `EMBED_TEXT` generated columns
  (pkg/semantic/task.go:30-87, pkg/tenant/schema/tidb_auto.go:761-824).
- **Migration (a different thing entirely)**: a bulk EBS→Drive9 data
  migration tool with exact-convergence sync, dual-write repair, and an
  irreversible fence checkpoint (docs/design/drive9-migration-v1.md#1, #6).

## Interfaces

**CLI** (`cmd/drive9`): hand-rolled switch dispatch on stdlib `flag`
(cmd/drive9/main.go:96). Groups: `ctx` (server profiles: owner /
fs_scoped / delegated), `mount`/`umount` (+ `drain`, `status`, `ensure`,
`systemd-unit`; hidden `supervise`), `fs` (cp cat ls stat mv rm mkdir chmod
setmeta symlink hardlink sh grep find archive **layer**), `fs layer`
(create list status diff checkpoint rollback commit fork chain delete),
`git` (clone --fast/--blobless/--hydrate, worktree, hydrate), `token`
(issue/revoke), `vault`, `journal` (new append cat find verify seal),
`pack`/`unpack`, `admin`, `doctor`, `region`, `profile`, `update`
(cmd/drive9/cli/layer.go:29-44, token.go:17-61, journal.go:36-50). One
dead verb: `db sql` usage strings exist but the dispatch has no `db` case
(cmd/drive9/cli/sql.go:33,38).

**HTTP API**: registered in one block (pkg/server/server.go:487-556);
`/v1/fs/{path}` multiplexed by method and query actions
(`?stat ?grep ?find ?list ?append ?copy ?rename ?mkdir ?chmod ?setmeta
?create ?symlink ?hardlink`), batch endpoints, `/v1/uploads` + `/v2/uploads`
(multipart, presigned), `/v1/tokens`, `/v1/fork`, `/v1/sql`, `/v1/events`
(SSE), `/v1/journals*`, `/v1/git-workspaces*`, `/v1/layers*` +
`/v1/layer-checkpoints/{id}`, `/v1/object-credentials` (STS minting),
`/v1/vault/{secrets,tokens,grants,audit,read}`, `/v1/status`, `/v1/provision`,
`/v1/quota`, `/v1/admin/*`, `/v1/auth/slock/*`, `/healthz`, `/metrics`,
`/s3/`. Wire conventions: JSON bodies; octet-stream for fs read/write;
large reads 302 to presigned URLs; `Idempotency-Key` for journal appends;
the legacy `X-Dat9-*` header family is the working vocabulary
(`X-Dat9-Expected-Revision`, `X-Dat9-Content-Layout`, `X-Dat9-Actor`,
stat response headers, ~43 uses).

**AuthN/AuthZ**: an API key *is* a server-issued JWT (`drive9_`-prefixed)
verified by hash lookup + constant-time compare + ciphertext match + JWT
claims (pkg/tenant/token/token.go:24-75, pkg/server/auth.go:220-300).
Scoped tokens (`fs_scoped`) are path/ops/TTL-bounded API keys enforced in
two layers — a default-deny dispatcher allowlist (chmod permanently
owner-only; SQL/fork/events/journals/vault default-deny) plus per-request
`AuthorizeFS(op, path)` prefix checks, with upload sessions re-authorized
per step (pkg/server/server.go:1620-1675,
pkg/server/fs_authorization.go:36-127, 229-265). Admin auth is TiDB Cloud
IAM, not bearer. Vault reads authenticate by capability token.

**SDKs**: the Go client (`pkg/client`) is the reference surface (~50 files:
full transfer engine, layers, git workspaces, journals, vault, SSE, admin).
External SDKs re-implement natively with sharply uneven coverage: **TS**
near-parity (layers, git, journals, vault grants, SSE, scoped tokens);
**Python/Rust** core fs + transfer + basic vault only; **Kotlin/Swift**
mobile-tier equivalents of the Python set. JS-only among external SDKs:
layers, git workspaces, journals, scoped tokens, SSE. Go-only: admin,
provisioning, STS minting, batch-write, setmeta, append-log. No MCP server
exists (roadmap P5 only). Consistency matrix in the source report:
Marcus, Interfaces (2026-09-16), spot-checked.

## What LayerFS can learn

These are recommendations handed to the v0.1.7 checklist, not decisions.
LayerFS constraints used in the verdicts: a local Store today (single
SQLite file, one writer via `locking_mode=EXCLUSIVE`), canonical bytes and
identities frozen through 0.1.x, and the 0.2 topology (LayerStack = main,
Branch = pod, Workspace = one tool-call attempt) as the next destination.

### Concept mapping

| Drive9 concept | Closest LayerFS 0.1 concept | Notes |
| --- | --- | --- |
| tenant (own DB + S3 namespace) | — (no equivalent) | LayerFS has no multi-tenancy; nearest future analogue is the hosted-Store question (cloud-sqlite-vfs Stage 3) |
| main / base (`file_nodes`/`inodes`/`contents`) | Branch head (as a tree) | Drive9 main is *mutable current state* with per-path `revision` counters; LayerFS Branches sit on immutable snapshots |
| layer (`fs_layers`, active) | Workspace + Branch hybrid | One long-lived attempt line; LayerFS splits "attempt" (Workspace) from "writable line" (Branch) |
| `fs layer fork` (tip-pinned, O(1)) | Branch creation from a Layer (root reuse, no copy) | Same O(1) idea; Drive9 adds pin depth caps + transitive pin GC |
| checkpoint (`fs_layer_checkpoints`) | Workspace snapshot / 0.2 Proposal pin | A named pointer to a log boundary; no payload copied |
| commit (effective view → main, per-path CAS) | Commit (capture → CAS on Branch head) | Both CAS; Drive9 checks per-path claims, LayerFS checks one head pointer |
| conflicted layer (terminal, preserved) | `HeadMoved` CAS loss today; 0.2 resolution ticket | Drive9 parks whole-layer; LayerFS 0.2 wants structured resumable conflicts |
| rollback (layer → abandoned) | Discard End | Base/Branch untouched in both |
| durability barriers (close/fsync/checkpoint/unmount) | Commit-time capture | Drive9 pushes incrementally; LayerFS captures at Commit only |
| FUSE staging (buffer/shadow/journal/commit queue) | layerfs-fuse local spool + live proxy | Same problem, different transport |
| mount supervision (supervisor/worker/typed exits) | layerfs-daemon's per-Workspace FUSE helper | LayerFS helper is daemon-owned and Workspace-scoped |
| SSE events + reset semantics | — (no equivalent) | LayerFS Workspaces are private views; no cross-client coherence needed yet |
| scoped fs tokens (`fs_scoped`) | capability-authenticated daemon protocol (partial) | LayerFS authenticates the daemon, not paths/ops |
| journal (hash-chained audit) | Monitor receipts | Receipts are typed outcomes, not a tamper-evident chain |
| git fast-clone workspace | — (no equivalent) | LayerFS owns no Git integration (explicitly out of ownership) |
| append-log WAL | — (no equivalent) | Relevant only to a future LayerFS-as-VFS-provider product |
| pack/unpack two-tier state | — (no equivalent) | LayerFS projections capture the whole visible tree |
| — | LayerStack (integrated immutable history) | Drive9 main has no history — revisions only |
| — | Layer (immutable published snapshot) | No equivalent; Drive9's `committed` is a state, not an object |
| — | canonical objects, global dedup, authenticated reads | No equivalent — Drive9 mints fresh ULID blobs; checksums recorded, not verified on read |

The deepest structural difference: Drive9's durable model is **mutable
current state plus overlay op logs**; LayerFS's is **immutable
content-addressed snapshots**. Drive9 buys cheap per-path claims and
server-side chain resolution at the cost of no content authentication and
no dedup; LayerFS buys exact identity and reuse at the cost of a heavier
commit. The mappings above inherit that asymmetry.

### Observations and verdicts

1. **Per-path revision claims as the conflict-detection primitive.**
   Drive9 records `base_revision`/`base_inode_id` at copy-up and compares
   against live main at commit; two attempts touching disjoint paths both
   commit (rule D16), and the conflict response is machine-readable
   (`{path, reason, current, expected}`, pkg/server/fs_layer.go:116-121,
   1706-1713). LayerFS 0.2 needs exactly this input for "automatically
   combine independent, identical, and otherwise commuting changes" — you
   cannot reconcile what you cannot attribute. Verdict: **adopt** the
   per-path claim record as Proposal metadata (LayerFS already has the
   better primitive to build it from: canonical identity per path, not just
   a revision counter). Note what Drive9 does *not* do: it never reconciles;
   identical-content writes still conflict on revision. The reconciliation
   itself is LayerFS 0.2's own work.

2. **Write-time synchronous CAS before commit-time preflight.** Drive9
   rejects a stale-claim upsert inside the entry-insert transaction, "while
   the writer can still retry", so a stale copy-up fails at write time
   instead of escalating the whole attempt at commit
   (pkg/datastore/fs_layer.go:619-643). This is the cheap, early half of
   LayerFS 0.2's "stale-head CAS loss becomes an internal retry" goal.
   Verdict: **adopt** — LayerFS Proposal admission should validate claims
   against the Branch head at admission, not only at integration time.

3. **Detect-and-park as the whole conflict story.** Drive9's conflicted
   layer is terminal: preserved for review/diff/rollback, no merge, no
   resolve API, and a crash mid-commit leaves a layer stuck in `committing`
   with no public escape (pkg/server/fs_layer.go:882-885;
   docs/design/layered-filesystem-v1-design.md:494). LayerFS's 0.2 roadmap
   explicitly rejects parking conflicts as manual work ("A routine stale
   head must not become a Git-style manual pull/rebase loop") and requires
   resumable, structured resolution that survives Workspace teardown.
   Verdict: **avoid** the park-only model; **adopt** two of its pieces —
   never silently overwrite or discard a conflicted attempt, and keep the
   attempt durably inspectable after its process dies. Also **avoid**
   Drive9's deferred-atomicity position: LayerFS's Commit is a single
   transaction plus one CAS, and 0.2 should keep that property rather than
   inheriting an apply-ledger debt.

4. **Flatten-then-diff commit planning (rules D18/D19).** A child commit
   materializes the ordered chain into a tree and diffs against *live main*,
   skipping paths that already match — so a child committing after its
   parent landed the same content is a clean empty apply, and raw
   latest-per-path rows are never applied (a chmod-only latest would drop
   content) (docs/design/layered-filesystem-cow-fork-design.md:82-83,
   pkg/server/fs_layer.go:1365-1468). This is precisely the shape of
   LayerFS 0.2's "reconcile an older Workspace result against the latest
   Branch head without replaying every intervening Commit": compute the
   candidate's effective tree, diff it against the current head, and let
   already-satisfied paths vanish from the plan. Verdict: **borrow-with-
   changes** — LayerFS diffs canonical trees rather than op logs, but the
   diff-against-live-head rule and the already-matching skip are directly
   transferable.

5. **Tip-pinned O(1) fork with explicit pin semantics.** `origin_seq =
   MAX(parent.entry_seq)` under the parent row lock (never `durable_seq`),
   the pin freezes the parent overlay rather than main, depth-capped
   (8/16), with a transitive `still_pins` GC predicate
   (docs/design/layered-filesystem-cow-fork-design.md:65, 71, 79-83).
   LayerFS 0.1 already pins a Workspace to one exact Branch head — the same
   design decision, and Drive9 demonstrates it holding at server scale.
   Verdict: **adopt** the framing (LayerFS already has it); **borrow** the
   two hardening details for 0.2's concurrent-Workspace work if Proposals
   ever chain: a pin-depth cap and an explicit retention predicate for
   pinned ancestry.

6. **Server-side authority as a product property.** All writes flow through
   the server; clients hold scoped, revocable, TTL-bounded credentials;
   concurrency is mediated by the authority, not by client locks
   (pkg/server/auth.go:220-300, pkg/server/fs_authorization.go:36-127).
   This is a live implementation of what cloud-sqlite-vfs.md calls D1-A
   (hosted Store endpoint) and its Stage 3 requirement that "the one-writer
   invariant must be re-established as a server property". Verdict:
   **not-applicable** to 0.1.x (the local Store is the product), but
   **borrow-with-changes** as evidence for the 0.2/cloud line: Drive9 shows
   the hosted shape is mostly *authorization engineering* (scopes,
   sessions, tenant isolation), with storage itself staying boring. Note
   also Drive9's isolation choice — one database per tenant, physical not
   row-level — which matches the LayerFS open question 3 answer "a Store
   per tenant is simple and probably right".

7. **Mount supervision as a first-class subsystem.** Typed exit codes,
   local-only health ("backend outage ≠ FUSE death"), a restart circuit
   breaker, stale-mount cleanup that distinguishes transport-broken from
   permission errors, PID+creation-time identity against PID reuse, and an
   external `ensure` reconcile (pkg/fuse/mount_exit.go:8-96,
   pkg/mountsupervisor/supervisor.go:266-439, 573-599, 1056-1096). LayerFS's
   analogue is layerfs-daemon's one fresh FUSE helper per Workspace —
   shorter-lived and Workspace-scoped, so the problem is smaller, but the
   failure classes (helper death leaving a stale mount, permission noise
   vs. real death, PID reuse) are identical. Verdict: **borrow-with-
   changes** — the typed exit contract and the transport-broken-vs-degraded
   distinction are directly applicable to the daemon's helper lifecycle.

8. **Lost-update fences in the client staging stack.** Drive9's commit
   queue separates `PayloadBaseRev` (what the bytes derive from) from the
   CAS `BaseRev` ("a stale writeback entry must never pair rev2-era bytes
   with a later BaseRev just because a sibling handle committed rev3"),
   fences entries below a durable watermark revision, and binds every
   staged artifact to the exact staging generation it was created in
   (pkg/fuse/commit_queue.go:50-101). Verdict: **adopt** the invariants as
   a review checklist for layerfs-fuse's local spool and live proxy
   transport — the three silent-corruption classes Drive9 fences against
   (old bytes + new base; stale entry overwriting a newer durable state;
   cleanup racing a later write) are generic to any write-back design.

9. **Explicit durability profiles with hard exceptions.** Interactive
   (fsync = local-durable) vs. strict (fsync = remote-durable), `O_SYNC`
   open-time promotion, and two always-strict path classes — SQLite
   sidecars and append-log files — that keep `synchronous=FULL` honest
   regardless of profile (docs/design/fuse-durability-policy.md#cli;
   pkg/fuse/dat9fs.go:13422-13442). Also the FUSE-era placement rule:
   close-time errors must surface in `Flush`, because `Release` has no
   status return. Verdict: **borrow-with-changes** for any LayerFS
   writeable projection: LayerFS's contract is capture-at-Commit, so the
   profiles are not needed, but the SQLite-sidecar lesson (a database file
   inside a write-back mount silently violates its own durability
   expectations unless specially routed) is exactly the hazard a future
   `layerfs` VFS projection (cloud-sqlite-vfs §5b) must design for.

10. **The append-log WAL as a design precedent, not a mechanism.**
    Operator-declared eligibility + negotiated capability + tail-only
    conditional appends + a 32-byte generation reset + one local shadow —
    900-1,330 net LoC by Drive9's own estimate, no new schema, no second
    state machine (docs/design/s3-express-append-log-fuse.md:49-57).
    Verdict: **not-applicable** to the 0.1.x Store (LayerFS's own SQLite is
    local; there is no remote to amortize), but **record** for the
    LayerFS-as-VFS-provider product: hosting a `synchronous=FULL` SQLite
    database inside a network-backed projection costs full-file re-uploads
    per fsync (Drive9 measured 44 ms-1.12 s remote reads where local shadow
    served 22-38 µs), and the mitigation pattern — content-layout metadata
    on the server, conditional tail append, generation reset on header
    change — is now proven in production code by a peer system.

11. **Multi-tenancy: gap or out of scope?** Drive9's answer is physical
    isolation (DB + S3 namespace per tenant), JWT-wrapped keys, path-scoped
    tokens with default-deny dispatch, and IAM-based admin — a large,
    permanently maintained surface (pkg/meta/meta.go:545-853,
    pkg/server/server.go:1620-1675). LayerFS today has none, and its 0.2
    ownership boundary explicitly excludes network policy. Verdict:
    **out of scope** for 0.1.x/0.2 — but when the hosted question is asked
    (cloud-sqlite-vfs Stage 3), Drive9 is the reference for what the
    non-storage engineering actually consists of, and "Store per tenant"
    is the shape that keeps LayerFS's one-writer invariant intact per
    tenant.

12. **Evidence discipline worth copying.** Drive9's POSIX report runs
    pjdfstest + LTP + flock + pyxattr + fsx + fio + mdtest and reports
    8,941/8,941 (its own claim; docs/posix-compatibility-report.md#summary)
    — and its design docs state limitations plainly ("does not claim
    single-transaction atomicity", "Conflict explain — Gap",
    feature-matrix.md:220-222). The suite menu is a good input for LayerFS
    0.2's projection-conformance contract (whiteout, rename, open-unlink,
    and metadata behavior are all covered by pjdfstest families Drive9
    enumerates). Verdict: **borrow** the suite menu; per LayerFS
    benchmark rules, any LayerFS numbers would be independently measured,
    never inherited from a vendor report — and note that Drive9's own
    report omits environment details and an unsupported-behavior list,
    which is exactly the omission LayerFS's documentation policy forbids.

## Unknowns

What this research could not verify, and why:

1. **The server half of the append-log WAL** — the `?append-log` handler,
   the S3 Express object lifecycle, and the append semantics live in the
   external `tidbcloud/fs` repo, outside the pinned snapshot
   (docs/design/s3-express-append-log-fuse.md:13). What "S3 Express One
   Zone" provides as an AWS product was not researched (no web access for
   the subject).
2. **Production topology of the reference deployment** (api.drive9.ai) —
   fleet size, pod count, live provider, sticky routing. The
   cache-invalidation spec concedes the event stream is trusted only for
   single-server/sticky deployments (docs/specs/cache-invalidation.md#2);
   which regime is live is unknown.
3. **Drive9's measured performance beyond its own quoted figures** — this
   study ran no measurements (per the study rules) and quotes only
   Drive9's own numbers with citations; no independent throughput or
   latency characterization exists.
4. **`ReexecPreflight`'s future** — the reexec gate has no non-test caller;
   whether the fd-handoff upgrade path will ship is unknowable from one
   snapshot (docs/design/fuse-clean-state-reexec-audit.md,
   pkg/fuse/reexec_gate.go:54-165).
5. **`tenantctx.TenantIDFromContext` has no non-test consumer** — dead
   plumbing, compatibility surface, or groundwork cannot be distinguished
   from a single commit.
6. **The go-fuse fork delta** — `mornyx/go-fuse` is pinned for
   "inode/nlookup behavior" (go.mod:164-166) but the fork source is not in
   the repo and could not be inspected.
7. **Scope limits of Drive9's POSIX report** — no environment/spec details
   and no explicit unsupported-behavior section; areas outside the listed
   suites (mmap coherence, atime, O_DIRECT, large xattrs) are unverified by
   that document. Code comments do disclose deliberate divergences
   (per-path xattrs for hardlinks, pkg/fuse/xattr_store.go:19-23; chmod
   transmits only the 0o777 mask, pkg/fuse/mode.go:10-20).
8. **Design-doc dates vs. git history** — the pinned snapshot is a single
   shallow commit, so the design evolution timeline (2026-06-01 research →
   2026-06-03 V1 → 2026-08-13 CoW fork) rests on the documents' own dates,
   not on history.
9. **Whether the npm `drive9` package is actually published** — declared in
   package.json; registry state not verifiable offline.
10. **L0/L1/L2 tiered context and cross-tenant sharing** — the early
    design-overview's flagship features; their current status beyond the
    semantic_tasks types could not be fully traced and they appear to be
    subsumed by the semantic pipeline rather than a distinct subsystem.

## Source index

Docs: README.md; docs/design-overview.md (early proposal, superseded in
framing); docs/posix-compatibility-report.md; docs/quota-meta-runtime-cutover.md;
docs/auto-embedding-mode.md; docs/openclaw-drive9-fuse.md; docs/grafana/README.md;
docs/design/{layered-filesystem-v1-design.md, layered-filesystem-cow-fork-design.md,
layered-filesystem-feature-matrix.md, layered-filesystem-research.md,
s3-express-append-log-fuse.md, fuse-mount-supervision.md,
fuse-clean-state-reexec-audit.md, fuse-durability-policy.md,
git-fast-clone-workspace.md, git-workspace-fuse-poll-optimization.md,
pack-unpack-profile-spec.md, directory-archive-spec.md,
drive9-agent-artifact-archive-proposal.md, metadata-schema-refactor.md,
drive9-migration-v1.md, agent-vault-phase0.md};
docs/specs/{cache-invalidation.md, pr-a-jwt-implementation.md,
pr-b-ctx-implementation.md, vault-interaction-end-state.md};
docs/guides/{go-sdk-integration.md, go-sdk-cli-parity.md, quota.md,
vault-quickstart.md, typescript-sdk-integration.md, sse-notifications.md};
e2e/README.md and e2e/layer-fs-smoke-test.sh.

Code, principal files: cmd/drive9-server/{main.go, local_provider.go,
startup_retry.go}; cmd/drive9/main.go plus cmd/drive9/cli/ (mount, layer,
layer_write, git, token, ctx, journal, pack, archive, profile, credentials,
admin, sql); pkg/server/{server.go, auth.go, fs_authorization.go, tokens.go,
fs_layer.go, fork.go, slock.go, pod_registry.go, eventbus.go, sse.go,
notify_coalescer.go, git_workspace.go, journal.go, vault.go, object_sts.go,
object_gc_worker.go, tenant_delete.go, admin_tenant_pool.go,
semantic_shard.go, tidbcloud_iam.go}; pkg/leader/leader.go; pkg/meta/meta.go;
pkg/tenant/{provider.go, pool.go, dsn.go, token/, quota_adapter.go,
provisioner.go, db9/, local/, tidbzero/, tidbcloudnative/, starter/,
schema/, schemadump/}; pkg/datastore/{store.go, scope.go, file_tx.go,
inode.go, content.go, storage_ref_hash.go, fs_layer.go, fs_layer_chain.go,
fs_layer_overlay.go, git_workspace.go, journal.go, fs_events.go};
pkg/backend/{dat9.go, upload.go, upload_reservation.go, patch.go,
batch_write.go, quota.go, mutation_dispatcher.go, mutation_replay.go,
file_gc_worker.go, expiry_sweep.go, fs_layer_content.go, options.go};
pkg/s3client/s3client.go; pkg/fuse/{dat9fs.go, mount.go, mount_exit.go,
mount_profile.go, inode.go, handle.go, read.go, write.go, dir.go, locks.go,
xattr_store.go, special_node.go, rename_posix.go, disk_read_cache.go,
prefetch.go, singleflight.go, kernel_cache_bypass.go, sse.go, writeback.go,
writeback_uploader.go, stream_upload.go, shadow.go, pending_index.go,
journal.go, commit_queue.go, drain.go, reexec_gate.go, layer_events.go,
local_overlay.go, local_policy.go, git_workspace.go, append_log.go,
append_log_snapshot.go, sqlite_wal.go, exports.go};
pkg/mountsupervisor/supervisor.go; pkg/mountcontrol/control.go; pkg/mountstate/;
pkg/mountpath/; pkg/objectfs/; pkg/gitcache/; pkg/gitwsindex/index.go;
pkg/journal/journal.go; pkg/vault/; pkg/encrypt/; pkg/webdav/;
pkg/tenantctx/context.go; pkg/semantic/task.go; pkg/migrate/split_tables.go;
internal/migration/fence.go; pkg/client/ (client.go, tokens.go, vault.go,
fs_layer.go, git_workspace.go, journal.go, append_log.go, events.go,
archive.go, git_workspace_index.go); clients/drive9-{js,py,rs,kotlin,swift};
go.mod.

LayerFS grounding read alongside (all read-only, in the LayerFS repo):
docs/general/concepts.md; docs/roadmap/0.2/README.md;
docs/roadmap/0.2/agent-branch-reconciliation/README.md;
docs/roadmap/0.1/0.1.7/README.md; docs/roadmap/0.2/cloud-sqlite-vfs.md.
