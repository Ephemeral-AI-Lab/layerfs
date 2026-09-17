# Cloudflare Computer: SQLite placement and container↔host transport

> **Status:** Research; informative and not a product contract.

Focused companion to [architecture_overview.md](architecture_overview.md). That
document covers the whole subject; this one answers the two narrowing questions
asked for the v0.1.7 refactor:

1. **Where does SQLite live** when a durable workspace filesystem is projected
   into a container that runs arbitrary processes?
2. **What actually crosses the container↔host boundary**, when, in which
   direction, and with what guarantees?

It then records what LayerFS's v0.1.7 replacement architecture can borrow, given
that the v0.1.6 executed path installs state on the host per operation.

- **Subject pin:** upstream `https://github.com/cloudflare/computer`, local
  snapshot `/Users/yifanxu/Ephemeral-AI-Lab/study/cloudflare-computer` at commit
  `64c462b083860cad29b374b9e4cda1e6a680f902` (2026-08-26). Written 2026-09-17.
  Citations below are `path:line` relative to that snapshot root.
- **LayerFS references:** `docs/roadmap/0.1/0.1.6/sandbox-host-connection-architecture.md`
  (issue #150) for the executed v0.1.6 shape, and
  `docs/roadmap/0.1/0.1.7/component-decoupling/` for the replacement design.
- **Method:** source reads only in both repositories. No builds, no tests, no
  benchmarks, no code changes. Every load-bearing claim was read directly at the
  pin; the subject's own preview caveat applies (its `docs/` are forward-looking,
  so code is the authority where the two disagree).
- **Scope note:** the isolate backends (`sync: "none"`) appear only where they
  explain why the container backend looks the way it does.

## 1. The placement, in one view

```text
        Cloudflare Workers runtime                        Sandbox container
+-------------------------------------------+     +--------------------------------+
| Durable Object  (the authority)           |     | computerd  (Node SEA binary)   |
|                                           |     |                                |
|  Workspace                                |     |  fuse-native + libfuse         |
|    owns one dofs Database                 |     |        |                       |
|        |                                  |     |        v                       |
|        v                                  |     |  @platformatic/vfs             |
|  storage.sql  = platform SQLite (WAL)     |     |        |                       |
|  storage.transactionSync(closure)         |     |        v                       |
|        ^                                  |     |  SQLiteWorkspaceProvider       |
|        |   the SAME dofs code             |     |        |   (same dofs code)    |
|        |                                  |     |        v                       |
|  vfs_nodes / vfs_dirents / vfs_chunks     |     |  node:sqlite  DatabaseSync     |
|  vfs_blobs / vfs_blob_bytes               |     |      (":memory:")              |
+-------------------------------------------+     +--------------------------------+
        ^                                                  |
        |  capnweb over WebSocket: 7 sync methods          |
        +--------------------------------------------------+
             REVERSE-DIAL: the container POSTs /connect
             and opens the WebSocket up to the DO
```

Two SQLite databases, one schema, one implementation. The DO's is durable and
authoritative; the container's is `:memory:`, disposable, and re-baselined from
rev 0 when it is lost.

## 2. SQLite placement

### 2.1 One access path, two providers

dofs never opens a database file. It takes a storage adapter:

```ts
// packages/dofs/src/types.ts:5-16
export interface SQLStorageLike {
  exec<Row extends object = Record<string, unknown>>(
    query: string, ...bindings: unknown[]
  ): SQLCursorLike<Row>;
}
export interface DurableObjectStorageLike {
  sql: SQLStorageLike;
  transaction?<T>(closure: () => T | Promise<T>): T | Promise<T>;
  transactionSync?<T>(closure: () => T): T;
}
```

`Database` (`packages/dofs/src/storage.ts:3-80`) wraps that adapter and is the only
surface the filesystem code touches. Two providers exist at this pin:

| Side | Provider | Backing | Lifetime |
| --- | --- | --- | --- |
| DO | `storage.sql` from the Durable Object runtime | platform-managed SQLite, WAL | durable; survives DO restarts |
| container | `SQLiteTestStorage` (`packages/dofs/src/testing.ts:33-41`) | `new DatabaseSync(":memory:")` | process lifetime; rebuilt from rev 0 |

The container's provider is named "test" storage because it is the same class the
test suite uses; it is nevertheless what computerd mounts in production
(`packages/computerd/src/fuse/vfs.ts:1-2`). Its `transactionSync` is
`BEGIN`/`COMMIT`/`ROLLBACK` (`testing.ts:60-70`).

The load-bearing consequence: **the identical filesystem implementation runs on
both sides, and the boundary between them is state reconciliation rather than an
operation protocol.** The wire contract states it directly:

```ts
// packages/rpc/src/interface.ts:1-4
// The DO side and the container-side computerd both implement this interface
// against a SQLite-backed VFS; the only thing that differs is which direction
// each method is called from.
```

### 2.2 The transaction closure stays with the owner

`Database.transactionSync` runs a JavaScript *closure*, not a statement list. The
outer call delegates to the adapter's `transactionSync`; nested calls become
`SAVEPOINT`/`RELEASE`, because "SQLite forbids a real BEGIN inside an active
transaction" (`storage.ts:6-11,20-38`). A depth counter also exposes
`inTransaction`, and the resolve cache refuses to populate while it is set, so a
rolled-back mutation cannot leave cached uncommitted state (`storage.ts:56-72`).

This answers a question LayerFS's replacement design leaves open.
`admission-and-persistence.md` §8 requires a remote provider to offer "atomic
pack/locator/catalogue writes" and observes that "a Rust callback is not sent over
a network", so "a remote integrated workflow needs a concrete equivalent atomic
execution mechanism".

Cloudflare's answer: **do not send the closure — put the owner next to the
database and let the closure run there.** The DO owns its composition, the
container owns its own composition against its own database, and the two are joined
by state sync, never by a distributed transaction.

### 2.3 Content rows are immutable and independently addressable

Schema v6 (`packages/dofs/src/schema/core.ts:16`) stores content as:

```sql
vfs_blobs       (hash BLOB PRIMARY KEY, size INTEGER, last_seen INTEGER)
vfs_blob_bytes  (hash BLOB PRIMARY KEY REFERENCES vfs_blobs(hash) ON DELETE CASCADE,
                 bytes BLOB NOT NULL)
vfs_chunks      (inode, idx, hash, size, PRIMARY KEY (inode, idx)) WITHOUT ROWID
```

Content is written as **one row per fixed 512 KiB window**, keyed by sha256
(`CHUNK_SIZE = 512 * 1024`, `packages/dofs/src/fs/writeFile.ts:29`; deterministic
boundaries, `docs/02_sync_protocol.md:179-183`). Bytes are insert-once — only a
small scalar is ever updated:

```ts
// packages/dofs/src/fs/writeFile.ts:514-526
"INSERT INTO vfs_blobs (hash, size, last_seen) VALUES (?, ?, ?)
   ON CONFLICT(hash) DO UPDATE SET last_seen = excluded.last_seen"
"INSERT INTO vfs_blob_bytes (hash, bytes) VALUES (?, ?)
   ON CONFLICT(hash) DO NOTHING"
```

Note the shape: the **mutable** part (`last_seen`, one integer) and the
**immutable** part (`bytes`) live in *different tables*. The mutable row stays
tiny; the immutable row is never rewritten. The only `DO UPDATE` on bytes is the
sync receiver's repair path, `stageBlob`
(`packages/dofs/src/sync/blobs.ts`), which clears the blob cache afterwards.

**No row in this schema is an accumulator.** There is no growing pack BLOB, no
in-place append, no read-modify-write of a content row; growth is new rows. That is
what makes the schema safe to place behind an interface with provider row and
parameter limits, and it is exactly the property LayerFS's current C2 candidate
lacks (§4.2).

Content addressing also removes a whole class of cache invalidation: the blob cache
comment notes that because a correct `(hash, bytes)` pair is never overwritten, "the
cache stays valid for it" (`packages/dofs/src/fs/blobCache.ts:9-12`). The cache is
a 16-entry LRU per `Database`, about 8 MiB at the 512 KiB window
(`blobCache.ts:29-30,45-81`).

### 2.4 Provider limits are design inputs, not incidents

Three constants fall out of the provider's published envelope:

| Constant | Value | Where | Stated reason |
| --- | --- | --- | --- |
| content window | 512 KiB | `writeFile.ts:29` | one row per window, well under the row/BLOB cap |
| object probe batch | 100 | `packages/dofs/src/sync/fetch.ts:36-38` | "SQLite accepts at most 100 bound parameters per query" |
| FUSE max read/write | 512 KiB | `packages/computerd/src/fuse/options.ts:34-35` | "so a single FUSE read maps to a single chunk fetch instead of four 128 KiB slices" |

The probe bound is the interesting discipline: it is *derived from the provider and
declared next to the query that needs it*, and every probe loops in that batch size
(`fetch.ts:50-51`).

LayerFS's replacement docs already record these same numbers from the other side
and reach the compatibility conclusion:

> Current published limits include 100 bound parameters and 2-MB row/BLOB limits,
> which are not interchangeable with the local profile's accepted singletons.

— `component-decoupling/implementation-plan.md` §"Radish and Cloudflare"

### 2.5 Durability belongs to the provider, and that provider is WAL

dofs contains no `PRAGMA journal_mode`, no `PRAGMA synchronous`, and no explicit
transaction control outside the adapter; a grep over `packages/dofs/src` and
`packages/computerd/src` finds no journal configuration at all. Journaling,
locking and durability are entirely the Durable Object platform's.

The LayerFS policy consequence is already recorded: the v0.1.7 target profile is
"embedded SQLite's selected MEMORY journal / synchronous OFF profile with zero busy
timeout" with "no WAL or added crash-durability work"
(`docs/roadmap/0.1/0.1.7/README.md`; `admission-and-persistence.md` §1 decision 9),
and the Durable Object provider is therefore "a future study, not a supported
deployment" (`implementation-plan.md` §"Radish and Cloudflare").

**Framing worth correcting:** that exclusion is a *provider* verdict, not an
*architecture* verdict. Everything in §2.1–§2.4 and all of §3 is independent of
which SQLite sits behind the adapter. The two-instance split, the immutable-row
discipline, the metadata/bytes split and the cursor discipline are all available to
LayerFS without adopting WAL, and the WAL question can be re-opened separately when
the provider question is actually on the table.

## 3. Container ↔ host transport

### 3.1 Seven methods, both directions

The sync surface is `SyncRPC` — `push`, `pushObjects`, `fetchChanges`,
`fetchObjects`, `hasObjects`, `watermarks`, `readEntry`
(`packages/rpc/src/interface.ts:22-103`) — carried over capnweb on a WebSocket
alongside a sibling `ShellRPC` (`exec`, `getExec`, `killExec`, `disposeExec`), the
two composing into one `WorkspaceRPC` stub.

Compare LayerFS's live wire: **53 hand-rolled `u8` opcodes**
(`crates/layerfs-fuse/src/live_wire.rs:19-53,578-620`), including `RESERVE`,
`APPEND`, `FACTS_BEGIN/NODE/END`, `INSTALL_BEGIN/NODE/END`, `FREEZE`, `RESUME` and
`EDIT_BEGIN/PART/END`. That protocol describes *edits*. Cloudflare's describes
*states*. The rest of this section is what the state description buys.

### 3.2 Metadata and bytes are separate flows

`ChangeEntry` records never carry bytes. A file entry carries chunk `(hash, size)`
pairs, and `readEntry` documents the follow-up explicitly:

```ts
// packages/rpc/src/interface.ts:84-88
// File entries carry chunk (hash, size) pairs only; the caller follows up with
// hasObjects + fetchObjects for the bytes.
```

Bytes then move only for what the peer *lacks*, and only in the direction needed:

- DO → container: probe `hasObjects`, then `pushObjects` streams the missing
  subset (`interface.ts:100-103`).
- container → DO: probe `hasObjects`, then `fetchObjects` streams by hash in
  request order — "Throws EUNKNOWN_HASH if any hash is unknown — callers must
  dedupe and probe first" (`interface.ts:95-98`).

The protocol negotiates **identity**, not the transfer. A chunk the receiver
already holds — a delta base, a deduplicated copy, a retransmit after reconnect —
costs one probe entry and no bytes.

### 3.3 Ordering: bytes before the entry that references them

The push path probes, transfers objects, and **only then** sends the entries whose
chunks all fit the byte budget (`packages/rpc/src/sync-driver.ts:584-650`). If the
budget cannot cover every missing chunk of an entry, the batch returns `pending`
and the entries are withheld.

The container enforces the other half. FUSE reads are served from the container's
own SQLite with **no read-through**: a missing blob is a hard error, not a fetch.

```ts
// packages/dofs/src/fs/readFile.ts:185-188
const bytes = getBlobBytes(db, chunk.hash);
if (bytes === undefined) {
  throw createWorkspaceError("EIO", `missing blob bytes for ${path}`, path);
}
```

So a receiver can never observe an entry whose bytes are absent, and a miss is a
bug rather than a slow path. The pull direction asserts the same invariant from the
other side: the puller throws `pullBatch: remote is missing object <hash>` when the
sender lacks content it referenced (`sync-driver.ts:425-428`).

### 3.4 Cursors, watermarks, and a wire-checkable invariant

`ChangeCursor = {rev, path|null}` is a *resume point*, not a snapshot handle. On
every push and every `fetchChanges` response the receiver echoes its
`appliedPushCursor`, and the sender asserts that the echoed cursor covers its own
`pushRev`; a violation throws and rebuilds the connection
(`packages/dofs/src/sync/invariant.ts:18-28`; assertion sites
`packages/rpc/src/sync-driver.ts:204,387,573,675`).

The point is not the cursor. The point is that **"the peer is caught up" is
inspectable on the wire**, so a regression trips an assertion instead of corrupting
state silently, and a sender never has to trust its own in-process belief about the
peer.

Reconnect re-derives rather than trusts (`sync-driver.ts:735-775`): if the remote's
`currentRev` is below the local fetch cursor, the fetch cursor resets to rev 0 and
the next push re-baselines the whole container VFS. The function deliberately does
*not* compare against the remote's `pushRev`, because that field is the remote's own
outbound progress and stays 0 on the container side — comparing it would force a
full re-push on every reconnect.

### 3.5 Bracketing an execution

```text
exec(source)
  [1] push      DO -> container: objects, then entries.
                A failed push ABORTS before spawn.
  [2] spawn     /bin/sh -c "cd '<cwd>' && cmd"
  [3] events    {id, seq} stdout/stderr/exit, streamed with backpressure
                propagating to the kernel pipe
  [4] pull      container -> DO: fetchChanges(after cursor), 256-entry /
                4 MiB batches
  [5] result    {status, exitCode, pushed, pulled, skipped, sync}
```

(`packages/computer/src/shell.ts:150-192`; `sync-driver.ts:400-480`.)

"Running with stale or incomplete workspace contents is not safe" is the stated
reason the push is a gate rather than an optimization
(`shell.ts:152-154`). The post-drain pull is the part that cannot be synchronous: a
failed pull becomes a durable `SyncRetryIntent` fenced to the container's runtime
UUID, with terminal outcome `lost` if the retry reaches a replacement container
(`packages/computer/src/workspace.ts:64-84,714-797`).

### 3.6 The container is a disposable replica, re-baselined from zero

When computerd restarts, its in-memory database is empty and
`reconcileWatermarks` resets the divergent cursor to rev 0, so the next push
re-baselines the entire VFS. Container-local writes that were never pulled before
the process died are unrecoverable — documented as such rather than implied
(`docs/07_injected_service.md:210`).

There is no container-side journal, no write-ahead spool, and no "uncertain
mutation replay" database. The recovery story is: the DO has the state, and the
projection is rebuilt from it.

### 3.7 The container dials out

The DO cannot reach the container, so the container reaches the DO: computerd POSTs
`/connect` with `{base, health, api}`, receives a durable bearer secret, and opens
the WebSocket upstream (`computerd.ts:232,446-536`). The `/api` upgrade uses
`server.accept()` rather than `ctx.acceptWebSocket()`
(`packages/computer/src/backends/container/cloudflare-container.ts:423-426`), so
there is no hibernation and the DO stays resident for the session's lifetime. Container reuse is keyed on an
exact launch-record digest (env + internet flag), and each launch mints a runtime
UUID so reconnect and replacement remain distinguishable.

LayerFS inverts this: the sandbox binds a TCP listener and the **host dials in**,
presenting a 32-byte capability and then claiming one of four roles — control,
observer, snapshot, data (`crates/layerfs-fuse/src/live_transport.rs:40-60,288-340`).

## 4. The v0.1.6 shape, and where it is awkward

The repository has already diagnosed this; this study's job is to say which parts
Cloudflare independently confirms.

`sandbox-host-connection-architecture.md` §2 states the executed path as
"syscall -> host installation -> reply -> next syscall", with the host owning
"inode / namespace state, dirty and range indexes, payload arenas and catalogs,
mutation replay / ownership", reached through `RESERVE`/`APPEND` and
`FACTS_*`/`INSTALL_*` frames. §4 lists what the replacement removes: "Per-FUSE-
mutation host installation and acknowledgment", "Host RESERVE/APPEND backing for
ordinary writes", "Host dirty-facts/inode mirror", and "One serialized control
transaction carrying the entire snapshot stream".

### 4.1 The protocol encodes edits where Cloudflare encodes states

The live wire's vocabulary is POSIX-edit-shaped: reserve capacity, append payload,
push resolved facts, install nodes. Each frame carries authority *about an
operation* across the boundary, so both sides must agree on operation semantics,
ordering and failure atomicity. Cloudflare's seven methods carry only *state*: what
changed (entries), what you have (probe), what you need (bytes), and how far you
have agreed (cursors). Operation semantics never cross.

### 4.2 The Store's payload row is an accumulator

The replacement C2 candidate stores object bytes as a pack BLOB and grows it by
rewriting the row:

```rust
// core/crates/layerfs-storage/src/sqlite/write.rs:70-89
pub fn insert_pack(connection: &Connection, pack_id: i64, bytes: &[u8]) -> StorageResult<()> {
    // "INSERT INTO object_packs (pack_id, data) VALUES (?1, ?2)"
}
pub fn append_pack(connection: &Connection, pack_id: i64, bytes: &[u8]) -> StorageResult<()> {
    // "UPDATE object_packs SET data = ?2 WHERE pack_id = ?1"
}
```

`bytes` is the *entire re-assembled pack image*, not a delta
(`core/crates/layerfs-storage/src/pack/placement.rs:22-30`, "Exact pack bytes to
write"; the caller is `cas/owner.rs:817-828`). Embedded on one host with a MEMORY
journal, that is a bounded page-cache rewrite of at most
`PACK_LIMIT = 256 KiB` (`policy.rs:47`). Under *any* non-embedded SQLite it becomes
a full-row read-modify-write per append on the network path, with a row that grows
toward the provider's cap.

The singleton lane is worse against a provider with a row cap:
`SINGLETON_PACK_LIMIT = CANONICAL_LIMIT + SINGLETON_FRAMING_SLACK` is **16 MiB +
4 KiB** (`policy.rs:73-75`), eight times Cloudflare's published 2 MB row/BLOB
limit. The repository already flags that mismatch; this study supplies the number.

### 4.3 Bound-parameter paging exceeds a 100-parameter provider

`LOOKUP_PAGE_IDS = 128` (`policy.rs:55`) pages membership queries: `lookup.rs:37`
chunks by it and `lookup.rs:62` builds `IN ({})` with that many placeholders. 128 is
above the 100-parameter envelope Cloudflare declares, so that query shape cannot be
placed unchanged.

### 4.4 There is no wire-checkable cross-side invariant

LayerFS's wire acknowledges boundaries — `BATCH` carries "ordered no-result backing
requests with one acknowledged completed prefix" (`live_wire.rs:36-38`), and
`FACTS_END`/`CHECK`/`RELEASE` bracket phases — but nothing echoes a receiver cursor
that the sender asserts coverage against. The "peer is caught up" belief lives in
process state on each side.

## 5. What v0.1.7 can borrow

Verdicts are this study's recommendations. They bind nothing and select nothing.

| # | Borrowing | Cloudflare evidence | LayerFS today | Verdict |
| --- | --- | --- | --- | --- |
| B1 | Two database instances, one access path, reconciliation at the boundary | `types.ts:5-16`; `testing.ts:33-41`; `interface.ts:1-4` | sandbox has no database; host owns SQLite | **Adopt the principle**; artifact conditional (§5.1) |
| B2 | Immutable, independently addressable content rows; never an accumulator | `core.ts:58-80`; `writeFile.ts:514-526` | `append_pack` rewrites the whole pack BLOB | **Adopt** — highest-value change (§5.2) |
| B3 | Provider limits declared as operation bounds and checked | `fetch.ts:36-38`; `writeFile.ts:29`; `options.ts:34-35` | 128-param pages; 16 MiB singleton rows | **Adopt** (§5.4) |
| B4 | Metadata and bytes are separate flows joined by identity | `interface.ts:84-88,95-103` | `APPEND` inlines payload; `RESERVE` pre-allocates | **Adopt** |
| B5 | Bytes transfer before the entry that references them | `sync-driver.ts:584-650`; `readFile.ts:185-188` | facts and payloads interleaved per phase | **Adopt as an invariant** |
| B6 | Receiver-echoed cursor asserted by the sender | `invariant.ts:18-28` | boundary acknowledgments only | **Adopt** |
| B7 | Owner-local transaction closure; never a distributed transaction | `storage.ts:20-38` | remote atomic composition open in the save design §8 | **Adopt as the closing answer** |
| B8 | Bracket the execution; never run on unestablished state | `shell.ts:150-192` | Commit publication ladder (§7 of the connection design) | **Borrow-with-changes** |
| B9 | Projection is disposable and re-baselines from zero | `sync-driver.ts:735-775` | already the contract ("no Workspace durability flush") | **Confirm; state the data loss explicitly** |
| B10 | One-bit "does this surface own a store" declaration | `backend.ts:92`, `sync?: "remote" \| "none"` | not explicit | **Adopt as an internal declaration** |
| B11 | Reverse-dial when the execution surface is unreachable | `computerd.ts:232,446-536` | host dials the sandbox | **Flag as a deployment constraint** |

### 5.1 B1 — the projection should own a complete local representation

Cloudflare's container is not a thin RPC client. It runs the whole filesystem
against its own SQLite and reconciles. LayerFS's replacement already moves mutable
ownership into the sandbox, but represents it as an in-memory piece tree plus
packed segment files (`crates/layerfs-fuse/src/local_spool.rs:1-33`).

Whether the sandbox's mutable state should itself be a SQLite instance is a genuine
question with costs on both sides:

- **For:** ordered namespace and dirty-index queries, bounded transactional
  updates, one uniform access path with the host side, and the option of the same
  code running on either side.
- **Against:** a second SQLite engine in the container image; payload bytes would
  take an extra copy through SQLite instead of today's positioned writes into packed
  segments; and the workspace is deliberately disposable, so SQLite's durability and
  crash-recovery machinery is dead weight under the no-flush rule.

The decidable part is narrow, and this study recommends deciding only that part:
**adopt the two-instance/reconciliation principle for the mutable metadata index
(namespace, dirty set, generation bookkeeping), and keep payload bytes in packed
segments with positioned writes.** The deciding test is whether that index outgrows
a bounded resident structure at the 25,000-file gate — if it does, a local SQLite is
the natural answer; if it does not, adding an engine is unjustified.

### 5.2 B2 — stop accumulating rows before anything is placed remotely

Today: `object_packs(pack_id, data BLOB)`, grown by rewriting `data` with the whole
re-assembled pack. The minimal change that preserves the design splits the pack
into a small mutable descriptor and immutable group rows:

```text
object_packs       (pack_id, lane, group_count, digest, declared_length)  -- small, rewriteable
object_pack_groups (pack_id, group_number, bytes BLOB)                    -- immutable, append-only
```

`group_number` is the existing group ordinal, and `bytes` is bounded by the existing
`GROUP_LIMIT = 64 KiB` (`policy.rs:45`) — comfortably inside any plausible provider
row cap, unlike the 16 MiB singleton. This is Cloudflare's own split, applied to
LayerFS's unit of storage: the mutable scalar (`last_seen`) and the immutable bytes
live in different tables (`writeFile.ts:514-526`).

Why this is the highest-value borrowing: it converts pack growth from
read-modify-write into append-only insertion, which is the only shape that survives a
non-embedded database; it keeps every read addressable at group granularity, so a
remote adapter can fetch one group instead of a whole pack; and it aligns the
persistence layer with what LayerFS's own `docs/roadmap/0.2/cloud-sqlite-vfs.md` §3
already claims about its data ("a logical append-only log keyed by dense integer pack
IDs"). The singleton lane needs its own decision, because its 16 MiB body cannot be
one row under a 2 MB cap.

Note what is **not** being borrowed: Cloudflare's fixed 512 KiB windowing is coarser
than LayerFS's CDC, and this study does not propose adopting it. Only the row-shape
discipline transfers.

### 5.3 What B4 and B5 replace in the live wire

If the mutable boundary becomes state reconciliation, `RESERVE` and `APPEND` stop
being the ordinary write path — the sandbox owns its payloads, as the v0.1.6
connection design already decides. What remains at Commit is a bounded transfer of a
frozen generation, which is exactly where B4 and B5 apply: send the frozen records
first (identity, length, offsets), let the receiver probe what it already has, and
transfer only the missing bytes, before any record that references them is admitted.
The existing snapshot lane (`SNAP_RECORDS`, `SNAP_READ`, `live_wire.rs:597-603`) is
already a separate service lane; this changes what it carries, not that it exists.

B4 also preserves deduplication across the boundary: a Commit that reuses an extent
the host already holds transfers identity and no bytes.

### 5.4 B3 — write the provider envelope down as a checked profile

The concrete items, all measurable today:

| Bound | LayerFS today | Cloudflare's declared envelope |
| --- | --- | --- |
| bound parameters per statement | 128 (`LOOKUP_PAGE_IDS`) | 100 |
| bytes per row | 16 MiB + 4 KiB (singleton pack) | 2 MB |
| bytes per transaction | 4 MiB − 1 (`TRANSACTION_CANONICAL_BYTES_LIMIT`) | not declared per-transaction |
| rows per transaction | 8,191 (`TRANSACTION_ROW_LIMIT`) | not declared |

The borrowing is not "use 100 and 2 MB". It is: **declare the bounds the operation
requires, and fail explicitly when a selected provider's declared limits cannot carry
them** — already the repository's rule for unsupported required capabilities
(`docs/roadmap/0.1/0.1.7/README.md`, `core/AGENTS.md`).

## 6. What not to borrow

- **The WAL-backed provider.** Excluded by current policy. As §2.5 notes, that is a
  provider verdict; do not let it silently become an architecture verdict that rules
  out the placement.
- **Last-writer-wins conflicts.** Cloudflare converges silently with no detection or
  merge. LayerFS's 0.2 direction is the opposite commitment; the sibling study §6.2.5
  already records this as not-applicable.
- **The history-less change stream.** Cloudflare's stream is derived from current
  state and the store keeps no content history; LayerFS's canonical history is the
  product.
- **Eager full materialization before execution.** Cloudflare *must* have every
  changed byte resident before spawn, because a container miss is `EIO`. LayerFS's
  live projection reads the immutable base on demand, which is strictly better for
  large trees. Borrow the metadata/bytes split; keep lazy base reads.
- **Retry on sync failure.** Cloudflare reconnects and retries sync operations
  freely, treating them as idempotent. LayerFS's owner decision is one attempt with
  no retry; the borrowable part is the cursor design that makes resume *possible*,
  not a retry policy.
- **The mutable in-place inode graph.** Already recorded as not-applicable in the
  sibling study §6.3.
- **`transactionSync` as a synchronous-closure convention.** The transferable
  property is owner co-location, not the JavaScript calling convention.

## 7. Open questions

- **Whether a container-side SQLite is worth an engine.** §5.1 states the deciding
  test; it has not been run, and this study measures nothing.
- **The singleton lane under a row cap.** LayerFS's 16 MiB singleton body must either
  split across rows, move out of the database into a separate byte store, or have the
  provider excluded. Not decided here.
- **Whether the frozen-generation transfer should be pull or push, and what bounds
  it.** Cloudflare pull is host-driven with byte budgets; LayerFS's snapshot lane is
  also host-pull (`SNAP_READ`). The two agree on direction and neither settles the
  budget.
- **Provider semantics behind the platform.** Durable Object input-gate
  serialization, `transactionSync` atomicity, and enforcement of the ~10 GB storage
  cap are asserted by Cloudflare's documentation and code comments and are not
  provable from the snapshot; the sibling study §7 records the same unknowns.
- **Whether reverse-dial is forced by architecture or by the platform.** The
  container is not directly addressable in Workers, which makes reverse-dial necessary
  there. Whether LayerFS's future placements share that property is a deployment
  question this study cannot answer.

## 8. Source index

**Cloudflare Computer** at `64c462b` (paths relative to the snapshot root):
`packages/dofs/src/{types,storage,testing,provider,rev}.ts`;
`packages/dofs/src/schema/{core,migrations}.ts`;
`packages/dofs/src/fs/{readFile,writeFile,blobCache,writeBuffer,resolveCache}.ts`;
`packages/dofs/src/sync/{blobs,fetch,invariant,apply,coalesce,watermarks}.ts`;
`packages/rpc/src/{interface,server,sync-driver}.ts`;
`packages/computerd/src/cli/computerd.ts`;
`packages/computerd/src/fuse/{vfs,options,backend,driver}.ts`;
`packages/computerd/src/{exec/runner,shim/shim}.ts`;
`packages/computer/src/{workspace,shell,backend}.ts`;
`docs/{02_sync_protocol,03_filesystem_schema,07_injected_service}.md`.

**LayerFS** (paths relative to this repository root):
`docs/roadmap/0.1/0.1.6/sandbox-host-connection-architecture.md`;
`docs/roadmap/0.1/0.1.7/README.md`;
`docs/roadmap/0.1/0.1.7/component-decoupling/{admission-and-persistence,content-io,implementation-plan,content-storage-design}.md`;
`docs/roadmap/0.2/cloud-sqlite-vfs.md`;
`docs/roadmap/0.1/0.1.7/study/cloudflare-computer/architecture_overview.md`;
`crates/layerfs-fuse/src/{live_wire,live_transport,local_spool}.rs`;
`core/crates/layerfs-storage/src/{policy.rs,sqlite/write.rs,sqlite/lookup.rs,pack/placement.rs,cas/owner.rs}`;
`core/crates/layerfs-storage/sql/schema.sql`.
