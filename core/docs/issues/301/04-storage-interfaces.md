# 04 — Global storage interfaces: S3 object store and PostgreSQL

> **Status:** Current planning checklist; no release candidate exists.
> Part of the [Phase 7 packet](README.md). Proposal; trait and table names are
> sketches for review. Limits quoted from source are the Phase 4.5 values at
> `7edddbdb8` (`layerfs-storage/src/policy.rs`), not new selections.

> **Superseded 2026-10-05.** This document describes PostgreSQL and S3
> interfaces that were not built. Cluster one is host-local SQLite: see
> [`cluster_one_handbook.md`](../../../../cluster_one_handbook.md). The cluster
> two contract is [`../303/06-cluster-one-integration.md`](../303/06-cluster-one-integration.md).
> The text below is unchanged and is kept as context.

## 1. Placement

| Bytes | Store | Addressed by | Written | Read |
| --- | --- | --- | --- | --- |
| File-content packs (CDC chunks, whole-file payloads; FULL/PREFIX/STORED) | Object store | Pack digest | Commit, Init | Workspace read of unchanged ranges; delta base reconstruction |
| Metadata bodies (encoded inode leaves, directory and file-tree nodes, pooled values/groups) | PostgreSQL, `bytea` rows | Body digest | Commit, Init | Namespace lookup, listing, attribute and mapping reads |
| Locators and dependencies (object → body, group, record; delta → base) | PostgreSQL | Canonical object identity | Commit, Init | Every committed read; exact-reuse checks |
| History (LayerStack, Layer, Commit, Branch, inode reservations) | PostgreSQL | History identity | Publish | Open Workspace, fork, history queries |

C2 decides the domain from the object's role; the rest of the system never
chooses a store. Canonical identities are independent of placement, so the same
object has the same identity wherever its bytes live.

**Environment (owner direction, 2026-10-03).** One machine. PostgreSQL and MinIO
each run in a local Docker container and are reached over the local network:
MinIO through its S3 HTTP API, PostgreSQL through its own client protocol over
TCP. LayerFS writes no service of its own. Multi-machine deployment, remote
endpoints and TLS are out of scope for Phase 7 and nothing here is designed for
them.

## 2. Object store port

The trait, key and error types are defined in `layerfs-storage`, because this is
C2's port. `layerfs-s3` implements it and depends on `layerfs-storage` only for
those types, so C2 builds and tests with no HTTP, TLS or signing code.

A deliberately small, synchronous contract. Packs are bounded (§4) and already
assembled in memory by C2, so no streaming body, multipart or listing is needed.

```rust
/// Immutable, digest-keyed bodies. Implemented for any S3-compatible service.
pub trait ObjectStore {
    /// Stores `body` under `key` unless the key already exists.
    fn put_if_absent(&self, key: &ObjectKey, body: &[u8]) -> Result<Put, ObjectError>;
    /// Reads the whole object, or one byte range of it, appending to `out`.
    fn read(&self, key: &ObjectKey, range: Option<ByteRange>, out: &mut Vec<u8>)
        -> Result<(), ObjectError>;
    /// Returns the stored length, or `None` if the key is absent.
    fn head(&self, key: &ObjectKey) -> Result<Option<u64>, ObjectError>;
}

pub enum Put { Created, AlreadyPresent }

pub enum ObjectError {
    Missing,                       // definite: no such key
    Refused { status: u16 },       // definite: provider said no
    Malformed(&'static str),       // definite: response violates the contract
    Uncertain(&'static str),       // the request may or may not have taken effect
}
```

| Port call | S3 operation | Notes |
| --- | --- | --- |
| `put_if_absent` | `PutObject` with `If-None-Match: *` | Conditional create must be verified on the pinned MinIO release in M2; a provider without it is unsupported, not silently downgraded |
| `read` (whole) | `GetObject` | Verified against the key digest |
| `read` (range) | `GetObject` with `Range` | One range per request; response status, `Content-Range` and length are checked |
| `head` | `HeadObject` | Used to resolve an uncertain upload |

**Not used in the product path:** `ListObjects`, multipart upload, versioning,
presigned URLs, object tagging, and `DeleteObject` (reserved for the future
collector behind a separate capability).

**Key layout:** `<prefix>/packs/<first two hex digits>/<pack digest hex>`. The
prefix selects a deployment; the fan-out directory is cosmetic for S3 and helps
filesystem-backed MinIO.

`Uncertain` is a first-class result, so the Commit workflow can keep custody
instead of guessing ([03 §5](03-commit-workflow.md#5-outcomes)). The port has no
retry policy of its own.

## 3. Global metadata store: PostgreSQL

**Selection (owner, 2026-10-03).** The global
metadata store is a PostgreSQL server in a Docker container. It replaces the
earlier proposal of a global SQLite file opened by every daemon.

**Why a server.** The global store is by definition shared: by several daemons,
by the host-side SDK, by Init/import and by the independent verifier. A SQLite
file can only be shared by processes on one kernel (file locks and the WAL
shared-memory index), which excluded the macOS host and forced host tools to go through a
daemon. A server removes that constraint and takes concurrent writers. The daemon's
overlay stays embedded SQLite: it is private to one process.

**One engine crate.** `layerfs-metadata` owns connections, the schema and its
versioning, and every statement sent to PostgreSQL. It implements two ports and
keeps their tables separate, with no foreign key between them (the Phase 4.5
separation is kept):

| Port (defined by) | Tables (role) |
| --- | --- |
| Locator and metadata-body port (C2 `layerfs-storage`) | Object identities, locators, delta dependencies, metadata bodies, pooled groups |
| `HistoryCatalog` (C5 `layerfs-history`) | LayerStacks, Layers, Commits, Branch heads, inode reservations |

Both providers are **written new** in the PostgreSQL dialect. C5's
`HistoryCatalog` trait and records stay; its SQLite provider is not moved. C2's
SQL, today woven into `cas/` and `sqlite/` (lookup, write, reservation,
ownership, pool, cleanup), is replaced by a port of a few **transactional
units** — look up these identities, register this batch atomically, read these
metadata bodies — never row-level calls, because each call is a network round
trip.

**Open (D10).** The exact shape of C2's port is fixed in M0 by reading `cas/` and
`sqlite/` at the base pin: which operations today rely on sharing one SQL
transaction, and whether each can become one unit.

**Transaction classes.** All short; none is open across construction or an
object-store request.

| Transaction | Form | Size bound (existing constants) | Frequency |
| --- | --- | --- | --- |
| Locator lookup | one read statement per page | `LOOKUP_PAGE_IDS = 128` identities per page | Per read wave and per reuse check |
| Register bodies + locators | `INSERT … ON CONFLICT DO NOTHING`, one transaction per batch | `TRANSACTION_ROW_LIMIT = 8,191` rows, `TRANSACTION_CANONICAL_BYTES_LIMIT = 4 MiB − 1` | Per pack group during Commit |
| Reserve inode serials | one counter update | — | Rare |
| Publish | one conditional `UPDATE … WHERE head = expected` plus Commit/Layer inserts | — | Once per Commit |

Registration is insert-if-absent: the first valid locator for an identity is
kept and a concurrent duplicate from another daemon is ignored. Selection never
changes afterwards, so a reader's answer is stable. Writers do not queue behind
one database-wide lock; they contend only on the rows they touch.

**What the network adds.**

- *Round trips.* Every lookup, listing and locator query for unchanged files
  crosses the network, where an embedded database would be a local call. That is
  the same class as Phase 4.5 (bridge calls to the host) and it is bounded by
  batch-shaped port calls and a bounded daemon-side cache of immutable objects.
  **Round trips per operation** is the diagnostic to record from M3 onward.
- *Uncertain outcomes.* A reply can be lost after the server committed. The
  port's error type therefore has the same `Uncertain` class as `ObjectError`,
  and resolving it by exact identity lookup (D7) is required, not optional.
- *No reconnect-and-retry.* A broken connection ends the operation with a
  definite or uncertain result; the engine does not resend.

**To freeze in M0.**

| Item | Proposal |
| --- | --- |
| Server pin | One PostgreSQL image, pinned by digest and recorded in every identity set |
| Durability | PostgreSQL's default settings: it writes a WAL and syncs at commit. `core/AGENTS.md` permits this since the owner amendment of 2026-10-03. State exactly what is and is not claimed with every identity set |
| Isolation | Default `READ COMMITTED`; publish is a single conditional statement, so no higher level is needed |
| Connections | A small fixed number per daemon; no pool that grows with load |
| Timeouts | A statement timeout maps to `Uncertain` for writes and to a definite failure for reads |
| Bootstrap | `layerfs-sandbox` starts and health-checks the PostgreSQL and MinIO containers once per deployment, not per sandbox; daemons connect over the Docker network and host tools through a published local port |
| Measurement | The server keeps its own buffer cache across runs. Root `AGENTS.md` §1 requires cache state to be declared and equal across arms, so the cold contract for a server (for example a restart outside the timers) is defined before any timed row. Container start is setup and never inside a timer |

**Overlay database.** The daemon's embedded SQLite keeps its own profile (WAL,
`synchronous = OFF`, one writer); the full settings table is in
[02 §10](02-workspace-overlay.md#10-sqlite-settings).

## 4. Transfer units

All existing C2 limits, unchanged for the first slice.

| Unit | Limit | Constant |
| --- | --- | --- |
| Ordinary pack | 256 KiB | `PACK_LIMIT` |
| Singleton pack (one large object) | 16 MiB + 4 KiB | `SINGLETON_PACK_LIMIT` |
| Compression group target | 48 KiB | `GROUP_TARGET` |
| Pooled metadata pack | 64 KiB (128 KiB for v2.2) | `POOLED_PACK_LIMIT`, `POOLED_V22_PACK_LIMIT` |
| Objects per save batch | 512 | `BATCH_OBJECT_LIMIT` |
| Read wave | 4,096 objects, 32 MiB canonical | `READ_OBJECT_LIMIT`, `READ_CANONICAL_BYTES_LIMIT` |

Because the largest pack is about 16 MiB, every upload is a single `PutObject`.

**Open (D6).** 256 KiB was sized for BLOB rows in a local Store. On an object
store, each pack is one HTTP request, so small packs mean many requests on
upload, while large packs mean more unrelated bytes per read until ranged reads
exist (#300). Keep the current limits first; change them only with request-count
and byte-count evidence from M2 and M4.

## 5. Sequences

```text
Commit / Init (write)
  C1 finalized object ──> C2
     ├─ exact-reuse lookup (PostgreSQL, read)           → already stored: emit nothing
     ├─ choose FULL / PREFIX / STORED, add to a group
     └─ pack sealed
          ├─ payload pack ──> put_if_absent ──ack──┐
          └─ metadata body ────────────────────────┤
                                                   v
                              register bodies + locators (PostgreSQL, one batch)
  … all batches registered …
  publish: Branch head expected → new  (PostgreSQL, one transaction)

Workspace read of an unchanged range
  read plan gap ──> C1 range read ──> needs object ids
     ├─ bounded cache hit (immutable objects), else:
     ├─ locator lookup (PostgreSQL, ≤128 ids per round trip)
     ├─ metadata objects: body from PostgreSQL
     ├─ payload objects: object-store read
     └─ decode (resolve delta bases) ──> authenticate ──> bytes
```

Backpressure runs right to left: a bounded upload window stalls pack assembly,
which stalls C1, which stops paging the overlay. Nothing accumulates without a
bound, and no complete object list or second send pass exists.

## 6. Integrity and idempotency

- **Whole pack:** digest of the received bytes must equal the key.
- **Ranged read:** the pack digest cannot be checked on a fragment. Bytes are
  accepted only after the reconstructed canonical object hashes to the requested
  identity, which C1's `AuthenticatedObjects` contract already requires. The
  first reader uses whole-pack reads; ranged reads are a nice-to-have (§10), and
  the port already carries the range parameter so the interface does not change.
- **Cache:** objects are immutable, so a cache entry never becomes wrong; the
  only policy is a byte bound and eviction.
- **Idempotent writes:** put-if-absent by digest and insert-if-absent by identity
  make a repeated or racing write a no-op. That is a safety property; whether the
  product *automatically* repeats a request is a separate rule, and the
  recommendation remains one attempt per operation.
- **Credentials:** only the daemon and trusted host tools hold object-store and
  database credentials. Both endpoints are network-reachable in principle, so a
  command inside the sandbox is kept out by not having credentials; restricting
  its network path as well is a hardening item for the trust-boundary proof.

## 7. Client dependencies

Two new external clients are needed, one per engine crate. Neither exists in the
workspace today, so each is an explicit dependency decision under root
`AGENTS.md`, pinned, `--locked` and unpatched. No other crate gains either
dependency.

**S3 client, in `layerfs-s3` (D5).** The Phase 6 prototype used a hand-written
HTTP/1.1 + SigV4 client over `TcpStream`: full-object GET only, one TCP
connection per request, no TLS.

| Option | For | Against |
| --- | --- | --- |
| Own minimal client (SigV4 over a persistent plain-HTTP connection) | No new dependency; exact control of bounds and counters; TLS is not needed in the declared environment | HTTP correctness against one known server becomes product code |
| Small synchronous stack (request signing + blocking HTTP) | Fits the synchronous codebase; modest surface | New pinned dependencies to approve and audit |
| Full async SDK | Complete S3 coverage | Brings an async runtime into a synchronous product; large surface |

Recommendation: the own minimal client. With a local MinIO over plain HTTP the
surface is four request shapes against one known server, and it adds no
dependency. Revisit only if a remote or TLS endpoint is ever brought into scope.

**PostgreSQL client, in `layerfs-metadata` (D11).**

| Option | For | Against |
| --- | --- | --- |
| Synchronous Rust client | Fits the synchronous codebase; no C library | Believed to wrap an async client and bring a runtime with it — to be verified against the pinned release |
| Binding to the C client library | Reference implementation of the protocol | Native library in every build and image, including the musl daemon image |
| Own wire-protocol client | No dependency | Protocol, authentication and type handling become product code; not recommended |

Recommendation: decide in M0 after checking the candidates' actual dependency
trees; provider types never cross a port either way.

## 8. Deferred

- **Collection of unreferenced packs and locators.** Refused or conflicting
  Commits leave immutable garbage. Safe removal needs reachability from published
  roots, delta-base custody and a grace rule for in-flight Commits. Not in the
  first slices; nothing is deleted from the object store until it is designed.
- **Ranged reads** — #300. Nice to have, not required for Phase 7; see §10.
- **Multi-machine deployment, remote endpoints and TLS.** Out of scope by owner
  direction (2026-10-03).
- **Import, export, backup and restart of the global stores** — separate
  capabilities.

## 9. MinIO objects and bucket settings

**One kind of object.** The bucket holds sealed file-content packs and nothing
else: no metadata, no index, no manifest, no per-file object.

| Aspect | Proposal |
| --- | --- |
| Bucket | One bucket per deployment; a key prefix selects a store |
| Key | `<prefix>/packs/<first two hex digits>/<digest hex>` |
| Digest | SHA-256 of the object's bytes. S3 request signing already hashes the body with SHA-256, so one pass gives both the key and the signature, and MinIO checks the body against it on upload. Canonical object identities stay BLAKE3 |
| Content | The existing pack format, unchanged: control area (`LFPACK` magic, framing version, group count, assembled length), directory, group bodies. Records are FULL, PREFIX or STORED |
| Which packs | Packs of file content: CDC chunk records, whole-file payloads and singleton packs. Packs of metadata objects and pooled metadata go to PostgreSQL. The exact framing-to-store mapping is fixed in M0 |
| Size | Up to 256 KiB, or about 16 MiB for a singleton; always one `PutObject` |
| Object metadata, tags | None. Everything known about an object is in PostgreSQL (`pack`, `object`) |
| Mutability | Written once with put-if-absent; never overwritten, appended or renamed |

**Finding a byte.** `object_id` → `object` row (pack, group, record) → `pack`
row (digest) → key → GET → check digest → decode the group → take the record →
resolve a PREFIX base the same way → check the canonical hash.

**Packs must be sealed before upload.** The Phase 4.5 format was designed for
packs that live in SQLite BLOB rows and can be appended in place: the directory
region is reserved, and the pooled-metadata lane keeps one pack open across
flushes. An object in MinIO cannot be appended. File-content lanes already close
their pack at each flush, so they fit as they are. Two consequences to settle in
M0:

- *Pooled metadata (D12).* It goes to PostgreSQL, where appending in place is
  possible but breaks "a pack is immutable and named by its digest" and lets two
  daemons race on one open pack. Proposal: seal it per Commit as well. The effect
  on metadata pack fill is unknown and must be counted.
- *Reserved directory bytes.* Each sealed pack carries its unused reserved
  directory entries as zeros. Keep the format unchanged first; trimming them is a
  new framing version, considered only with byte-count evidence.

**Bucket and server settings.**

| Setting | Value | Why |
| --- | --- | --- |
| Versioning | Off | Keys are content digests; a second version of a key has no meaning |
| Object lock, retention | Off | Not needed; immutability comes from put-if-absent and the access policy |
| Lifecycle / expiry rules | None | An expiry rule would delete live data |
| Server-side compression, encryption | Off | Packs are already compressed; local single-machine scope |
| Topology | Single node, single drive, on a named Docker volume | The declared environment. No redundancy is claimed |
| Access policy for daemons | `GetObject`, `PutObject`, `HeadObject` on the bucket; no `DeleteObject`, no `ListBucket` | Least privilege makes the bucket append-only for every writer; the future collector gets its own credential |
| Image | Pinned by digest, recorded in every identity set | Conditional create and behaviour must not drift |

**Orphans.** A refused Commit can leave an uploaded pack that no `pack` row
names, or a `pack` row no published root reaches. Both are harmless and nothing
deletes them in the first slices (§8).

**Object count.** Every pack is one object, and each object costs MinIO at least
one file on its volume. Pack size therefore sets request count on upload and
file count on the server; that is decision D6 and is measured in M2 and M4.

## 10. Ranged reads (#300) — nice to have

**Status: nice to have. Not required for Phase 7 and not on the milestone path.**
The first reader fetches whole packs. Ranged reads are added only if the M8
measurement shows real amplification for a pattern that matters.

**The problem #300 describes.** A small read of committed data fetches the whole
pack that holds it. A 4 KiB read from a 256 KiB pack downloads 64 times what it
returns (illustrative arithmetic, not a measurement).

**Why it is not first here.**

- *Requests cost more than bytes.* MinIO is on the same machine, so the fixed
  cost of a request dominates the cost of 256 KiB. A ranged read saves bytes, not
  requests, and can add requests.
- *The smallest useful range is a compression group.* Records are compressed in
  groups of about 48 KiB (`GROUP_TARGET`) and a record cannot be decoded without
  its group. A 4 KiB read improves from about 64× to about 12× at best, before
  any PREFIX base it depends on.
- *Sequential reads prefer whole packs.* A file's chunks are written next to each
  other, so one whole-pack fetch serves the following reads from the cache.
  Ranged reads only clearly win for small random reads in large files.
- *A singleton pack is one object* and must be fetched whole to decode.

**What comes before it** (all on the milestone path):

1. Fetch only read-plan gaps — ranges the Workspace has overwritten never reach
   the object store (#300 item 1).
2. One persistent connection in `layerfs-s3`.
3. A bounded, byte-budgeted cache of immutable packs with ordinary eviction. The
   mount uses direct I/O, so the kernel caches nothing on the daemon's behalf.

**What keeps it possible, at no cost now.**

- The object-store port already takes a byte range.
- Integrity needs no new format: a fragment cannot be checked against the pack
  digest, but every object is authenticated by its own canonical hash after
  decoding.
- Group offsets and lengths are recorded in PostgreSQL at registration
  ([06 §2](06-tables.md#2-postgresql--storage-side-layerfs-metadata-c2s-port)), so
  a later ranged read is one request and existing packs need no rewrite.

**When to revisit.** At M8, record response bytes per returned byte and GET count
for sequential reads, small random reads and reads that cross pack boundaries.
If random reads show real amplification, add ranged reads as a per-read choice:
a range when the needed groups are a small part of the pack, the whole pack
otherwise.

**Link to pack size (D6).** Ranged reads are what would make larger packs
affordable. If the pack size is ever raised to cut object and request counts,
ranged reads stop being optional, so the two are decided together.
