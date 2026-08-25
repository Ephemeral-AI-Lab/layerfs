# Cloudflare Computer — Storage, FUSE, Containers, and Remote Lessons for LayerFS

Status: external-architecture research note only. This document does not alter
LayerFS canonical identities, Stage 1 authority, accepted benchmark evidence,
the Apple/APFS product path, or the scope of any implementation task.

Prepared: 2026-08-25 from the current `cloudflare/computer` `main` sources and
documentation, the public performance report, current issue evidence, and the
accepted LayerFS Stage 1.1 artifacts.

Cloudflare Computer is explicitly preview software. Its repository says that
the APIs are unstable and the package is not suitable for production use. Code
controls when its forward-looking documentation disagrees with shipped
behavior.

## 1. Executive conclusion

Cloudflare Computer and LayerFS overlap at the workspace/filesystem boundary,
but they are different products:

```text
LayerFS
  = immutable, versioned, content-addressed filesystem engine

Cloudflare Computer
  = remote agent workspace with a mutable Durable Object filesystem,
    synchronized container mirror, FUSE execution surface, and RPC shell
```

The useful synthesis is:

```text
Cloudflare Computer
  good product/execution shell
  weak storage foundation for LayerFS's goals

LayerFS
  strong storage foundation
  unfinished container/FUSE/remote product shell
```

LayerFS should borrow Cloudflare's container daemon, direct range-read FUSE
adapter, open-file write coalescing, RPC execution boundary, health/diagnostic
surface, and real npm/git workloads.

LayerFS should reject Cloudflare's fixed 512 KiB chunk boundaries, mutable
revision-only authority, absence of content history, last-write-wins
multi-container conflicts, whole-file buffering fallback, and physical polling
shim as a primary path.

The remote LayerFS design remains strong. Immutable objects plus a small
transactional ref service are a cleaner remote protocol than synchronizing two
mutable path-indexed filesystems.

## 2. Sources and freshness

Primary Cloudflare sources:

- [Cloudflare Computer repository](https://github.com/cloudflare/computer)
- [Injected service and FUSE/container lifecycle](https://github.com/cloudflare/computer/blob/main/docs/07_injected_service.md)
- [Sync protocol](https://github.com/cloudflare/computer/blob/main/docs/02_sync_protocol.md)
- [Filesystem schema](https://github.com/cloudflare/computer/blob/main/docs/03_filesystem_schema.md)
- [Lifecycle](https://github.com/cloudflare/computer/blob/main/docs/11_lifecycle.md)
- [Performance](https://github.com/cloudflare/computer/blob/main/docs/19_performance.md)
- [`computerd` README](https://github.com/cloudflare/computer/blob/main/packages/computerd/README.md)
- [`computerd` FUSE backend selection](https://github.com/cloudflare/computer/blob/main/packages/computerd/src/fuse/backend.ts)
- [`computerd` FUSE driver](https://github.com/cloudflare/computer/blob/main/packages/computerd/src/fuse/driver.ts)
- [`computerd` standalone build targets](https://github.com/cloudflare/computer/blob/main/packages/computerd/scripts/build-bin.mjs)
- [DOFS package](https://github.com/cloudflare/computer/blob/main/packages/dofs/README.md)
- [Restart visibility issue #101](https://github.com/cloudflare/computer/issues/101)

Local LayerFS comparison evidence:

- [Apple/APFS PoC overview](../poc/README.md)
- [Portability boundary](../poc/09-portability-and-apple-completeness.md)
- [Stage 1.1 edge specification](../poc/16-stage1-part1-apple-edge-benchmark.md)
- [Stage 1.2 developer-workspace specification](../poc/15-stage1-workspace-benchmark.md)
- [Accepted Stage 1.1 result](../target/layerfs-stage1-apple-edge-20260825-attempt-007/summary.md)

Refresh the Cloudflare findings before implementation. The repository is under
active development and explicitly labels design documents as potentially ahead
of shipped code.

## 3. What Cloudflare Computer actually ships

Cloudflare Computer keeps durable workspace state in a Durable Object SQLite
database. A sandbox-side daemon named `computerd` runs in a Linux container,
maintains a process-lifetime container VFS, exposes it at `/workspace` through
FUSE, executes tools, and synchronizes with the Durable Object over capnweb on
a WebSocket.

```text
Durable Object
  authoritative mutable SQLite VFS
        |
        | push/pull revisions, chunk hashes, missing objects
        v
Linux container
  transient SQLite/in-memory VFS
        |
        v
  Linux FUSE /workspace
        |
        v
  shell / compiler / git / npm / agent
```

The Durable Object is durable authority across restarts. The current container
database is process-lifetime state; a container restart loses its local state
and must be re-baselined from the Durable Object.

Cloudflare also ships isolate shell and isolate JavaScript execution backends.
Those reach the authoritative Workspace over RPC and do not require the
container's second Store or FUSE mount.

### 3.1 Platform matrix

The current FUSE backend selector supports:

| Runtime | Real mount behavior |
|---|---|
| Linux with accessible `/dev/fuse` | real Linux FUSE |
| macOS with `/Library/Filesystems/macfuse.fs` | real macFUSE |
| Windows | no real backend; `auto` falls back to shim |
| CI/Linux container without `/dev/fuse` | shim |
| `FUSE_MOUNT=none` | no mount |

The real selector is:

```text
FUSE_MOUNT=fuse
  requires platform == linux
  requires accessible /dev/fuse

FUSE_MOUNT=macfuse
  requires platform == darwin
  requires installed macFUSE

FUSE_MOUNT=auto
  Linux /dev/fuse if available
  macOS macFUSE if available
  otherwise shim
```

The standalone build script currently produces Linux x64 and macOS x64
targets. It embeds the relevant `fuse-native` addon and `libfuse` or
`libosxfuse`. It contains no Windows/WinFsp target.

Cloudflare Computer can be used from a Windows client because execution and the
real FUSE mount run remotely in Linux. It does not expose a real local Windows
filesystem.

### 3.2 The non-FUSE shim

When real FUSE is unavailable, the shim:

1. walks the VFS subtree;
2. materializes ordinary files at the workspace path;
3. watches VFS changes;
4. polls the ordinary directory approximately every 250 ms;
5. hashes and diffs physical entries; and
6. applies changed physical entries back to the VFS.

The documented limitations are:

- no process-level coherence guarantee;
- conflicts settle on a later reconciliation tick;
- incomplete symlink, xattr, mode, and ownership behavior; and
- full reads for large changed files.

This is comparable to a reduced Stage 1 physical projection/capture loop, not
to a direct mounted extent-backed LayerFS view.

## 4. Cloudflare storage representation

The DOFS schema is a conventional mutable SQLite filesystem:

```text
vfs_nodes
  live inode metadata

vfs_dirents
  parent inode + name -> inode

vfs_chunks
  inode + fixed chunk index -> content hash

vfs_blobs / vfs_blob_bytes
  SHA-256-addressed data

vfs_changes / watermarks
  incremental synchronization state
```

Each mutation advances a monotonic revision. The revision is a sync cursor and
committed mutation point, not an immutable filesystem root.

### 4.1 Fixed 512 KiB chunks

Files use deterministic fixed boundaries:

```text
chunk_index = floor(byte_offset / 512 KiB)
```

This is simple and useful for same-offset overwrites, appends, tail changes,
missing-object probes, and remote transfer.

It is poor for arbitrary count-changing edits. Inserting one byte near the
front shifts every later fixed boundary:

```text
before
  [A][B][C][D]

after one-byte head insert
  [A'][B'][C'][D']...
```

Cloudflare's schema document explicitly acknowledges that nearly every suffix
chunk receives a different hash in this case.

LayerFS avoids this with small FastCDC chunks referenced through a persistent
extent tree:

```text
before
  [A][B][C][D]

after insert
  [A][B][X][C][D]
```

The unchanged suffix remains logically shared.

### 4.2 No immutable content history

Cloudflare's sync document states that its fetch cursor is not a point-in-time
snapshot handle and that the Store keeps no content history. Change
coalescing materializes each path's current state at stream time.

Cloudflare therefore does not naturally provide:

```text
open exact historical root
fork historical root
rollback ref to historical root
compare two immutable roots
retain multiple roots through compaction
```

LayerFS roots, refs, fork, rollback, retained-root reads, and reachability
compaction are structurally stronger for versioned storage.

### 4.3 Conflict model

The Durable Object serializes its own mutations. Two containers writing the
same path converge by sync order. The last pushed state wins; Cloudflare
documents no merge, conflict error, or writer notification.

LayerFS instead publishes an exact expected `RefState`:

```text
expected A -> proposed B

remote/current still A
  accept B

remote/current C
  return conflict { expected: A, actual: C, proposed: B }
```

DeltaGit, not LayerFS storage, should decide whether to fork, merge, or rebase.

## 5. FUSE write path

Cloudflare contains one particularly useful pattern. The FUSE adapter avoids a
durable transaction for every small syscall:

```text
open/create
  open provider write buffer

write/truncate
  update buffer/ranges

release/flush/fsync
  commit chunks in one transaction
```

The provider exposes direct operations including range read, range write,
truncate, deferred create, open write buffer, and release write buffer.

LayerFS should adopt the lifecycle, not the complete implementation:

```text
open
write
write
truncate
fsync/release
  one LayerFS checkpoint/publication
```

LayerFS must retain bounded extent/range state. Cloudflare's compatibility
fallback may retain one complete growable `Buffer` per regular file, capped at
256 MiB. That violates LayerFS's file-size-independent memory requirement.

## 6. Measured performance

Cloudflare reports results from a standard-2 container with 1 vCPU, 6 GiB RAM,
and 12 GB disk. The FUSE mount is `/workspace`, with tmpfs and container ext4
controls.

### 6.1 Filesystem microbenchmarks

| Scenario | Computer FUSE | tmpfs | ext4 | FUSE/ext4 |
|---|---:|---:|---:|---:|
| create 1,000 files | 560.6 ms | 83.2 ms | 303.2 ms | 1.85x |
| stat 1,000 files | 1,971.9 ms | 1,324.2 ms | 2,659.3 ms | 0.91x |
| remove 1,000 files | 827.7 ms | 322.7 ms | 1,281.8 ms | 0.66x |
| mkdir 10x10x10 tree | 1,597.5 ms | 1,585.7 ms | 3,034.7 ms | 0.74x |
| find tree | 1,813.6 ms | 1,819.9 ms | 4,404.2 ms | 0.72x |
| write 64 MiB | 230.6 ms | 47.3 ms | 16.8 ms | 16.93x |
| copy 64 MiB | 1,037.2 ms | 37.4 ms | 39.8 ms | 40.46x |
| read 64 MiB | 437.5 ms | 22.6 ms | 25.6 ms | 39.72x |
| pure read 64 MiB | 263.1 ms | 8.3 ms | 8.5 ms | 30.26x |
| overwrite 64 MiB | 272.6 ms | 8.3 ms | 8.5 ms | 43.35x |
| git init + commit 100 files | 459.2 ms | 40.3 ms | 635.4 ms | 0.72x |
| shallow git clone, about 1 MiB | 549.1 ms | 421.0 ms | 576.2 ms | 0.84x |
| npm init + tiny install | 598.5 ms | 630.7 ms | 630.7 ms | 0.95x |

Directional throughput calculations:

```text
64 MiB write       about 278 MiB/s
64 MiB pure read   about 243 MiB/s
64 MiB FUSE read   about 146 MiB/s
64 MiB copy        about 61.7 MiB/s
```

Cloudflare's in-memory inode/index path can beat its ext4 control on some
metadata-heavy operations. Its content-addressed payload path is substantially
slower than ext4 for sequential I/O.

### 6.2 Full npm install

The full workload installs 854 packages and creates 36,675 files:

| Target | Duration |
|---|---:|
| tmpfs | 34.3 s |
| ext4 | 63.9 s |
| Computer FUSE | 124.7 s |

Computer FUSE is about 2x slower than ext4 and 3.6x slower than tmpfs in this
workload.

### 6.3 LayerFS comparison limitations

The Cloudflare and LayerFS numbers are not directly comparable:

- different hardware and CPU allocation;
- Linux FUSE versus macOS APFS projection;
- different file sizes and caches;
- different durability and integrity contracts; and
- different benchmark-oracle work.

LayerFS Stage 1.1 nevertheless provides useful directional controls:

```text
direct logical 4 KiB edit       about 1.8 ms p50
canonical reconstruction/read   about 274-281 MiB/s
fresh APFS materialization       about 279-299 MiB/s
```

Cloudflare has stronger evidence for real npm/container behavior. LayerFS has
stronger evidence for canonical edit locality and immutable-root correctness.
Stage 1.2 must close the real-workspace evidence gap.

## 7. Side-by-side architectural assessment

| Area | LayerFS | Cloudflare Computer |
|---|---|---|
| Primary goal | versioned filesystem engine | remote agent workspace |
| Authority | immutable CAS roots + refs | mutable DO SQLite VFS |
| File mapping | persistent B+ extent tree | fixed 512 KiB chunk indexes |
| Chunking | FastCDC 8/16/32 KiB | fixed 512 KiB |
| Namespace | persistent directory/inode trees | mutable node/dirent rows |
| History | retained immutable roots | no content history |
| Conflict | exact expected-head rejection | last-write-wins across containers |
| Integrity | role/identity authentication and scrub | SHA-256 blobs + SQLite transactions |
| Memory rule | bounded buffers independent of file size | whole-file fallback buffers up to 256 MiB |
| Local macOS | ordinary APFS workspace | macFUSE or shim |
| Linux mount | future | shipped container FUSE |
| Windows mount | future WinFsp candidate | absent |
| Remote execution | future | core capability |
| Lifecycle | exact roots and derived workspace authority | DO authority + transient synchronized mirror |
| Status | Apple/APFS PoC | preview remote workspace |

For LayerFS's goals:

```text
arbitrary edit locality       LayerFS stronger
history/fork/rollback         LayerFS stronger
exact root identity           LayerFS stronger
integrity and metadata        LayerFS stronger
bounded storage algorithms    LayerFS stronger

container execution           Cloudflare further ahead
Linux FUSE integration        Cloudflare further ahead
remote RPC/product UX         Cloudflare further ahead
real npm evidence             Cloudflare further ahead until Stage 1.2
```

## 8. Remote LayerFS architecture

LayerFS is naturally remote-friendly because immutable bulk data and mutable
publication state can be separated:

```text
remote immutable object plane
  chunks
  extent nodes
  namespace nodes
  inode records
  metadata nodes
  roots

remote mutable authority plane
  StoreId
  named refs
  generations
  next inode serial
  upload sessions
  authorization and quotas
```

Clients should perform filesystem work locally near a cache and synchronize at
root/checkpoint boundaries. Do not perform each filesystem syscall over a WAN.

### 8.1 Remote read flow

```text
get accepted RefState
  -> fetch root and required tree nodes in bounded batches
  -> verify object identity and canonical role
  -> cache immutable objects locally
  -> serve reads through existing Engine/VFS paths
```

Cold reads are network-bound. Warm reads use the local immutable cache.

### 8.2 Remote write flow

```text
accepted ref A
  -> edit locally
  -> produce root B and new immutable objects
  -> ask which object IDs are missing
  -> upload only missing objects
  -> atomically publish expected A -> proposed B
  -> receive accepted RefState B
```

The server must never silently overwrite a moved ref. A stale expected head is
an explicit conflict.

### 8.3 Minimal protocol

The smallest useful future protocol is:

```text
GetStoreInfo
GetRef
HasObjects(ids[])
GetObjects(ids[])
PutObjects(objects[])
LeaseInodeSerials(count)
PublishRef(expected_ref, proposed_root, upload_session)
```

Optional later calls include ref listing/creation, retained-root listing,
upload-session management, root diff, and explicit compaction.

Do not begin with per-path mutation RPC, live multi-writer collaboration,
remote FUSE, merge, a global event log, or a canonical pack format.

### 8.4 Batching and latency

LayerFS's small chunks and tree nodes cannot use one network request per
object. Start with bounded multi-object operations:

```text
up to 64 object references per batch
byte-bounded streaming frames
largest in-memory buffer <= 1 MiB
per-object identity and role verification
backpressure and idempotent retry
```

Open-file read plans should retain decoded path/inode/extent state and prefetch
adjacent extent objects. Add a transport bundle or pack only after measured WAN
RTT/metadata overhead proves that bounded object batches are insufficient. A
transport bundle must not change canonical object identities.

### 8.5 Inode serial leases

Current LayerFS derives `InodeId` from `StoreId` and a durable serial. Two
offline replicas starting from the same serial could allocate the same
`InodeId` for unrelated files.

The remote authority should lease disjoint serial ranges:

```text
client A  [1000, 1999]
client B  [2000, 2999]
client C  [3000, 3999]
```

This preserves the current canonical formula and allows bounded offline inode
creation. Do not introduce provisional IDs and global remapping unless serial
leases prove inadequate.

### 8.6 Upload sessions and GC safety

Objects are uploaded before the ref that makes them reachable. Concurrent GC
must not remove them during this window.

```text
BeginUpload
  -> lease/session

PutObjects(session)
  -> protect objects

PublishRef(session, expected, proposed)
  -> validate closure
  -> atomically publish
  -> consume session
```

Expired sessions become collectable after a grace period. Initial remote work
should retain unreachable uploads and use explicit offline compaction; online
remote GC is a later measured requirement.

### 8.7 Tenant isolation

An unrestricted global `HasObjects` leaks whether another tenant stores known
content. Initially scope object namespaces, missing-object probes, and
deduplication to a Store/tenant. Cross-tenant deduplication requires an explicit
privacy and encryption decision.

## 9. Remote container execution

The strongest combined design is:

```text
remote LayerFS object/ref service
        |
        v
Linux agent container
  local verified LayerFS cache/working Store
  direct LayerFS FUSE mount at /workspace
  shell/compiler/git/npm/agent
        |
        v
checkpoint
  upload missing immutable objects
  expected-head ref publication
```

Unlike Cloudflare's two-mutable-store path sync, the container holds:

```text
accepted remote root
+ unpublished local roots
+ verified immutable cache
```

After restart it rereads the accepted ref and lazily fetches missing immutable
objects. It does not need to interpret a partially synchronized mutable mirror.

Mac, Windows, and browser clients can use the remote Linux execution service
without a local mount. This can defer Windows WinFsp and Apple FSKit work until
native local access has demonstrated product demand.

## 10. What to borrow

Borrow only these patterns:

- Linux container-local FUSE execution;
- an injected workspace daemon;
- direct range-read callbacks;
- open-file dirty-range buffering;
- commit on release/flush/fsync;
- deferred create plus one commit;
- bounded missing-object probes and transfers;
- resumable idempotent object synchronization;
- RPC shell/execution separation;
- health and diagnostic endpoints;
- real git/npm/directory/large-file workloads; and
- remote clients that do not require local native mounts.

## 11. What to reject

Do not adopt:

- fixed 512 KiB chunk boundaries;
- mutable revision-only filesystem authority;
- absence of immutable historical roots;
- last-write-wins multi-container conflicts;
- two mutable Stores as the default local/remote model;
- whole-file memory buffers;
- physical polling/hash shim as the primary path;
- reduced metadata fidelity;
- a canonical per-path change log;
- remote per-object request fanout; or
- Cloudflare's current FUSE throughput as an acceptable LayerFS target.

## 12. Roadmap implications

The recommended order is:

1. finish current Stage 1.1 Engine/read optimization;
2. execute and close Stage 1.2 on the real APFS developer workspace;
3. freeze the macOS/APFS path;
4. implement direct Linux FUSE LayerFS;
5. implement OCI import/export;
6. implement containerd/Docker container execution;
7. add a remote read-only object/ref client;
8. add remote single-writer expected-head publication;
9. add Linux remote agent containers with local LayerFS cache;
10. add inode-serial leases, upload sessions, authorization, and quotas;
11. expose Mac/Windows/browser RPC clients;
12. implement Windows WinFsp only if native Windows workspace access remains a
    real requirement; and
13. revisit direct FSKit only if a concrete Apple product need justifies it.

macFUSE is not a recommended LayerFS dependency. OverlayFS may remain the
container image/root mechanism or a fallback/control; the LayerFS workspace
should be a separate direct mount because OverlayFS whole-file copy-up defeats
large-file small-edit locality.

## 13. Falsifiable future gates

Before promoting a remote design, require small, fixed-budget evidence:

```text
remote read-only
  cold path fetches only required tree/chunk objects
  warm path performs zero network fetches
  every received object is authenticated

remote 4 KiB edit
  suffix payload upload = 0
  transfer is bounded by changed chunks/tree nodes
  one expected-head publication

conflict
  stale expected head returns exact conflict
  neither candidate root is lost

restart
  fresh container reopens exact accepted root
  no mutable-mirror ambiguity

resources
  object batches <= 64
  product buffers <= 1 MiB
  bounded RSS/Q and terminal cleanup

real workspace
  npm/git/build/search inside Linux FUSE container
  aggregate wall compared with ext4 and Cloudflare's published class
```

Do not create a large remote campaign, pack format, distributed lease system,
or multi-writer merge framework before these gates identify the real bottleneck.

## 14. Final research disposition

Cloudflare Computer validates the product value of a durable agent workspace
paired with Linux container execution and FUSE. It does not validate its
storage representation as a LayerFS replacement.

LayerFS should keep:

```text
immutable authenticated objects
persistent extent/namespace/inode trees
exact roots and refs
expected-head publication
bounded memory
history, fork, rollback, and compaction
```

The future remote LayerFS should synchronize immutable objects and one accepted
root, not mutable path revisions. Cloudflare's execution shell is the useful
reference; LayerFS's canonical Store remains the stronger storage foundation.
