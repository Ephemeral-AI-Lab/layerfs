# LayerFS Restart Specification

Status: proposed implementation specification for a new Rust repository.

The detailed Phase 4 gate is maintained in
[implementation-detail/phase-4/storage/sqlite/spec.md](implementation-detail/phase-4/storage/sqlite/spec.md).

This specification defines the restart architecture for LayerFS built around
Rust-owned content algorithms and a replaceable durable storage engine. The
first storage engine is SQLite. PostgreSQL and a recovered custom engine are
future backends, not part of the first implementation.

## 1. Purpose

LayerFS provides DeltaGit with one focused workflow:

1. materialize an immutable LayerFS root into a normal directory;
2. let Git or an agent modify that directory directly;
3. capture the filesystem changes into a new immutable root and delta.

LayerFS is not a general database API, a general filesystem API, a Git
implementation, a process runtime, or a public provider-selection framework.

## 2. The ownership boundary

The system has two different concepts that must not be mixed.

### 2.1 Application-level LayerFS core

`layerfs-core` owns the meaning of LayerFS data:

- CDC chunk-boundary algorithms;
- content-addressed identity and canonical hashing;
- canonical object encoding and decoding;
- immutable chunks, files, directories, trees, and deltas;
- typed root handles for published directory objects;
- copy-on-write tree updates;
- authenticated reuse of unchanged chunks and subtrees;
- bounded range update and rejoin logic; and
- canonical paths, ordering, limits, and typed semantic errors.

CDC decides how a byte stream is divided into chunks. CAS is the application
semantic that gives canonical objects stable identities and immutable
`put/get` behavior. Neither algorithm depends on SQLite, PostgreSQL, a file
pack, or a particular filesystem.

### 2.2 Physical storage engine

`layerfs-engine` owns durable persistence:

- storing and reading canonical object bytes;
- storing root/checkpoint metadata, deltas, and indexes;
- range reads without reconstructing an entire file;
- transactions and durability;
- concurrent access and locking;
- no-replace insertion for an already addressed object; and
- backend-specific schema, journals, pages, files, and connection handling.

The engine does not decide chunk boundaries, hashes, canonical bytes, tree
identity, or delta meaning. It persists values already validated by the core.

The engine API must be semantic and backend-neutral. It must not expose SQL,
SQL transactions, ORM objects, table names, or database-specific error types to
the core, projection, or SDK.

### 2.3 Canonical object contract

Phase 1 freezes the smallest useful canonical object contract:

```text
magic[4] = "LFSO"
kind[1]
payload_len[4] = big-endian u32
payload[payload_len]
```

The header is exactly 9 bytes. It has no flags, reserved fields, compatibility
fields, or format-version field. The payload length excludes the header and is
checked before decoding or allocation. Every object has exactly one canonical
encoding and exact end-of-input is required.

Phase 1 has only two object kinds: bounded bytes and directories. Directory
entries contain immediate canonical names, child kind tags, and typed object
references. Names are sorted by unsigned byte order and duplicate names are
invalid. Descendant paths are never stored in a directory object.

`ObjectId` is one typed 32-byte BLAKE3 identity over the supplied canonical
object bytes using the fixed object hash domain. Identity verification hashes
the supplied bytes directly, then validates their grammar; it does not decode,
re-encode, and hash a reconstructed value.

There is no canonical `Root` object and no second root identity. A root is a
typed handle to a directory `ObjectId`. The engine may store a root/checkpoint
record containing that handle, its parent, and publication metadata, but that
record is storage metadata rather than canonical object bytes.

Phase 1 deliberately does not freeze the large-file content layout. Phase 2
benchmarks a flat manifest, a segmented layout, and a fixed-fanout content tree.
The production candidate is:

```text
File → ContentLeaf/ContentBranch tree → Chunk IDs → CAS
```

Leaves hold bounded ordered chunk references and lengths. Branches hold bounded
child references and subtree byte lengths so range reads and small edits can
avoid scanning an entire file. The benchmark selects the final shape before
the `File`, `ContentLeaf`, and `ContentBranch` encodings become part of the
stable format.

## 3. Repository and crate layout

The new repository starts with five crates:

```text
layerfs/
├── Cargo.toml
├── crates/
│   ├── layerfs-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── error.rs
│   │       ├── limits.rs
│   │       ├── format/
│   │       ├── identity/
│   │       ├── object/
│   │       ├── cdc/
│   │       ├── cas/
│   │       ├── content/
│   │       ├── cow/
│   │       └── delta/
│   │
│   ├── layerfs-engine/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── error.rs
│   │       ├── transaction.rs
│   │       └── sqlite/
│   │           ├── mod.rs
│   │           ├── connection.rs
│   │           ├── schema.rs
│   │           ├── objects.rs
│   │           ├── roots.rs
│   │           ├── deltas.rs
│   │           └── capture.rs
│   │
│   ├── layerfs-os/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── error.rs
│   │       ├── capabilities.rs
│   │       ├── common.rs
│   │       └── macos.rs
│   │
│   ├── layerfs-vfs/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── materialize.rs
│   │       └── capture.rs
│   │
│   └── layerfs-sdk/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── error.rs
│           ├── materialize.rs
│           └── capture.rs
└── docs/
```

The dependency graph is:

```text
layerfs-sdk
    ↓
layerfs-vfs ─────┬──── layerfs-os
    ↓                   ↓
layerfs-core      layerfs-engine
                         ↓
                      SQLite
```

`layerfs-core` must not depend on `layerfs-engine`. This keeps the canonical
algorithms testable without a database and prevents a database schema from
becoming part of the LayerFS identity format.

## 4. Module ownership

### 4.1 `layerfs-core`

```text
format/       canonical paths, bounds, and serialization rules
identity/     domain-separated hashes and typed IDs
object/       canonical object model and codec
cdc/          chunk scanners, boundaries, anchors, and rejoin verification
cas/          immutable object semantics and authenticated reuse
content/      logical file and directory content
cow/          immutable views and copy-on-write tree mutation
delta/        changed paths, tombstones, parent roots, and new roots
```

`cow/` means copy-on-write. A mutation creates a new root handle and only the changed
nodes plus their affected ancestor spine. Unchanged chunks, files, directories,
and subtrees remain shared by identity.

### 4.2 `layerfs-engine`

The engine contains the durable backend and the narrow port consumed by the
projection. The first implementation is SQLite. No PostgreSQL or custom backend
code is added until a real deployment requires it.

The engine must provide these semantic capabilities:

```rust
pub(crate) trait StorageEngine {
    fn load_root(&self, root: RootId) -> Result<RootRecord>;

    fn read_object_range(
        &self,
        object: ObjectId,
        range: Range<u64>,
    ) -> Result<ReadResult>;

    fn begin_capture(&self, parent: RootId) -> Result<CaptureTransaction>;
}

pub(crate) trait CaptureTransaction {
    fn put_object_if_absent(
        &mut self,
        id: ObjectId,
        canonical_bytes: &[u8],
    ) -> Result<()>;

    fn write_delta(&mut self, delta: &DeltaRecord) -> Result<()>;

    fn commit_root(self, root: RootRecord) -> Result<()>;
}
```

These traits are private implementation boundaries. They are not SDK types.
Their exact names may change, but the following properties are mandatory:

- object insertion is idempotent and no-replace;
- an existing object is authenticated before reuse;
- object reads support bounded range access;
- a delta is not visible until its referenced objects are durable;
- a root is not visible until its complete closure is durable;
- root publication is one atomic engine transition; and
- backend failures are mapped into LayerFS typed errors.

`RootRecord` is engine metadata around a typed root handle. It is not a
canonical `Root` object and does not introduce another object identifier.

The initial SQLite implementation may store chunks and metadata in SQLite
tables. It may later use a file-backed object area behind the same engine port
if benchmarks show that large BLOB rows are the bottleneck. That physical choice
must not change object IDs, canonical bytes, or the engine contract.

### 4.3 `layerfs-os`

`layerfs-os` owns host filesystem mechanics. It contains the narrow platform
boundary used by projection code:

- file and directory open/read/write operations;
- rename, replacement, and synchronization;
- file identity and metadata observations;
- symlink and path behavior;
- native capability detection;
- APFS `clonefile` support when it is later qualified; and
- platform-specific error classification.

The first qualified implementation is macOS/APFS. Linux, Windows, and WASM
are added only after direct qualification. APFS, reflink, POSIX behavior, and
other host details belong here; they must not enter canonical object identity.

### 4.4 `layerfs-vfs`

`layerfs-vfs` owns the two product-neutral filesystem operations:

- materializing a root to a destination directory; and
- observing and capturing changes from a materialized directory.

The initial projection is a normal directory. Later projections may include
Linux OverlayFS or FUSE, but each is a separate implementation with its own
platform qualification. Projection code calls `layerfs-os` for host I/O and
must not implement CDC, object identity, tree hashing, or SQL persistence
itself.

### 4.4 `layerfs-sdk`

The SDK owns the small public facade. It must not expose CAS, CDC, SQLite,
PostgreSQL, object IDs, storage transactions, or internal tree nodes.

## 5. Minimal SDK

The public workflow is:

```rust
let fs = LayerFs::open(config)?;
let workspace = fs.materialize(root, destination)?;
let checkpoint = workspace.capture()?;
```

The public operations are:

```text
LayerFs::open
LayerFs::materialize
MaterializedWorkspace::capture
MaterializedWorkspace::discard
```

`open` constructs a handle. `materialize` creates or refreshes a usable
directory. `capture` freezes the observed changes, stores the changed objects
and delta, publishes a new root, and advances the workspace head. `discard`
closes the materialized workspace without publishing its changes.

There are no public file-by-file mutation methods. Git and agents modify the
normal materialized directory directly. There are no public `read`, `write`,
`list`, `diff`, `publish`, `rollback`, or generic database methods in the first
DeltaGit-facing SDK. A bounded change summary is returned from `capture`; a
separate diff API can be added only if a real caller requires it.

## 6. Materialization

Materialization accepts an immutable root and a destination directory.

The operation automatically handles:

- an empty destination;
- a destination already matching the root;
- an existing materialized view of the parent root; and
- an incrementally refreshable destination with known provenance.

The caller does not select separate cold, warm, or incremental endpoints.

Apple/APFS PoC status: cold construction and exact-live no-op verification are
implemented. A different target root against a nonempty destination currently
fails closed; the PoC does not persist projection provenance, so it does not
claim changed-root incremental materialization yet. Managed edits separately
exercise the qualified clone/same-offset and full-stream fallback routes.

### 6.1 Required behavior

- Cold materialization reads only the authenticated objects required by the
  requested root and writes the destination tree.
- Warm no-op materialization detects an already matching root without rewriting
  unchanged files.
- Incremental materialization updates only changed paths and affected directory
  metadata when provenance is available.
- Unknown or replaced destination state fails closed or performs an explicit
  full materialization according to the destination policy; it must not silently
  treat unknown bytes as a valid prior view.
- Large files are streamed or range-read; the projection must not stage a complete
  workspace in memory.

## 7. Change capture

Capture is the only operation that turns mutable filesystem state into a new
LayerFS root.

The sequence is:

```text
materialized directory
    ↓
mutation recorder / changed-path evidence
    ↓
affected path and range validation
    ↓
core CDC + CAS identity calculation
    ↓
engine transaction
    ├── persist new objects if absent
    ├── persist delta
    └── atomically publish new root
```

### 7.1 Small-edit requirement

An edit to a small region of a large file must not require reading, hashing, or
rechunking the entire file when the mutation recorder provides an exact range
or a bounded update neighborhood.

The core must:

- reuse authenticated unchanged prefix and suffix chunks;
- resynchronize CDC only in the affected neighborhood;
- verify the rejoin before accepting the new chunk sequence; and
- create only changed file metadata, changed directory nodes, and the new root
  delta.

If exact bounded resynchronization cannot be proved, capture returns a typed
failure. It must not silently degrade into a full-file scan and still claim
edit-sized work.

### 7.2 Capture output

Capture returns an opaque checkpoint containing:

- the new root identity;
- the parent root identity;
- changed-path count and bounded change summary; and
- typed status.

Raw storage keys, SQLite row IDs, table names, page offsets, and backend
locators are not returned.

## 8. SQLite engine requirements

SQLite is the first and only required backend.

The initial engine must provide:

- one durable database per LayerFS store;
- transactional object, delta, and root publication;
- parameterized statements only;
- prepared statements for hot lookups;
- indexes on object identity, root identity, parent root, and changed paths;
- bounded range reads for object payloads;
- explicit busy/lock timeout behavior;
- typed mapping of constraint, busy, I/O, corruption, and quota failures; and
- no database-specific values in canonical identity or object encoding.

WAL is disabled in the initial backend. The engine uses SQLite's rollback-journal
mode and must explicitly configure durability and synchronization settings.
WAL is a concurrency mode, not a backup requirement. It may be benchmarked as
a later backend option, but it is not part of the first contract.

The SQLite schema is an implementation detail. A schema migration must not
change canonical object bytes, object IDs, root IDs, or delta meaning.

## 9. Future backends

PostgreSQL is added only when one of these requirements becomes real:

- multiple hosts share one LayerFS store;
- remote service access is required;
- centralized multi-writer coordination is required; or
- SQLite concurrency becomes the measured bottleneck.

The PostgreSQL backend must implement the same semantic engine contract. It must
not introduce SQL-specific behavior into the core or SDK.

The former custom LayerFS engine is treated as a possible backend. Its useful
CDC, identity, object, and COW algorithms move into `layerfs-core`. Its custom
pack, locator, catalog, and filesystem machinery remains backend-specific and
is not allowed to define the new core API.

## 10. Performance contract

The first implementation must measure, separately:

- cold materialization latency and bytes read/written;
- warm no-op materialization latency and bytes rewritten;
- incremental materialization latency by changed-path count once that route is
  implemented (not claimed by the current Apple/APFS PoC);
- range-read latency and throughput for small and large files;
- small edit capture latency as file size grows;
- object reuse ratio;
- CDC bytes scanned and chunks regenerated;
- SQLite transaction latency;
- read/write throughput under the supported concurrency model; and
- process memory and temporary storage high-water marks.

The system must not claim that Rust is faster than TypeScript, or that capture
is independent of file size, without measurements from the same workload,
storage medium, cache state, and concurrency level.

The main target is:

```text
small edit capture work ≈ changed bytes + affected metadata
range read work        ≈ requested range + required chunk metadata
warm materialization   ≈ changed paths, or near-zero for a matching root
```

The implementation must report any unavoidable format-level amplification
honestly. A database lookup improvement cannot hide a core algorithm that still
rescans a complete large file.

## 11. Correctness and safety invariants

- Canonical bytes are determined by `layerfs-core`, never by the backend.
- Object IDs are verified before objects are trusted or reused.
- Installed objects are immutable.
- Existing equal objects are reusable; unequal occupants are integrity errors.
- A root cannot reference a missing or unverified object.
- A delta cannot become visible before its referenced objects are durable.
- A failed capture cannot advance the workspace head.
- A discarded workspace cannot publish changes afterward.
- SQLite row IDs, page layout, transaction IDs, and backend locators are never
  canonical identity inputs.
- No implicit full-file fallback is allowed for a failed bounded small edit.
- No hidden worker, retry loop, background compactor, or provider fallback is
  part of the first implementation.

## 12. Initial implementation order

1. Establish the Phase 1 core contract: canonical paths, the fixed envelope,
   the two initial object kinds, direct identity authentication, and tests that
   do not require a database.
2. Add CDC, CAS, and the benchmark-selected logical content layout in the next
   core phase; then add COW and delta semantics.
3. Create `layerfs-engine` with SQLite schema, object reads/writes,
   root/checkpoint metadata, delta transactions, and direct backend tests.
4. Implement `layerfs-vfs::materialize` using `layerfs-os`.
5. Implement `layerfs-vfs::capture` with exact changed-path/range
   evidence.
6. Expose the four-operation SDK.
7. Add cold/warm/read/edit/write benchmarks and verify small-edit scaling.
8. Consider a PostgreSQL backend only after the SQLite implementation has a
   measured need for it.

The first implementation must not carry forward the old custom engine merely
because it already has code. Preserve useful canonical algorithms and tests;
replace backend-specific persistence machinery with the SQLite engine.

## 13. AppleWorkspaceV1 implementation disposition (2026-08-24)

Current outcome: **PASS for the frozen AppleWorkspaceV1 PoC scope**. This is
not a production or portability qualification.

Implemented and focused-test proven:

- mode-free v3 file state, strict extent codecs, persistent bounded extent
  rope, logarithmic range reads, local overwrite/insert/delete/truncate, and
  retained-root structural counters;
- strict directory/inode/metadata/symlink/Apple ACL value codecs, persistent
  directory and inode-table path copying, 10,000-entry split/merge/root-collapse
  models, recursively derived true child minima and nonoverlapping level-2
  ranges, disk-backed streaming NamespaceRoot graph/link closure for Verified
  authority, root-to-leaf lookup/update reads bounded by tree height, and
  single-pass linear full visitors with pre-descent level checks, bounded inode
  node decoding, and preserved v1/v2 read goldens;
- zero-wait rollback-journal SQLite configuration, one-row/one-auth reads,
  ordered payload batches capped at 64, transactional inode allocation, one
  named-ref publication COMMIT, retained refs, fork/rollback, and
  centralized Verified semantic admission for publish/fork/move plus atomic
  initial and live Verified-after-Trusted scrub transactions; existing SQLite
  stores receive an exact read-only schema-definition/row fingerprint before
  assigning PRAGMAs or DDL, durable refs validate name/generation/membership,
  and fork/move accepts only already-retained roots;
  failed publication COMMIT reconciliation opens the Store read-only, repeats
  exact admission, verifies StoreId, and only then classifies the ref; missing
  and substituted Store paths cannot be created or accepted;
  SQLite internal-object filtering uses an exact `sqlite_*` GLOB prefix, so
  `sqliteX` tables/triggers cannot escape admission or candidate preservation;
- authenticated retained-union marking, exact same-directory generation copy,
  strict 154-byte checksummed `CURRENT`, neutral install port, selected-generation
  reopen, maintenance-locked cleanup of only verified same-StoreId residue,
  fresh requested/prior/different selector reconciliation, and live
  Verified-after-concurrent-Trusted revalidation; generation handles acquire
  SQLite shared lifetime pins before reading `CURRENT`, compaction requires an exclusive pin, and a
  conservative available-space admission runs before candidate creation;
  selector reads use a fixed 154+1-byte probe, directory-sync failure remains
  durability-ambiguous with the prior generation preserved, missing `CURRENT`
  with any generation fails closed, and only exact next-candidate/partial-selector
  residue is recovered without deleting or mutating unknown generations;
- OS-neutral projection handles, FD-relative Apple enumeration/open/link/rename,
  expected-token capture opens, pinned-identity same-volume staging cleanup,
  final file sync, reconciled atomic replace,
  directory sync, hard links, symlinks, negative mtimes, ordered ACLs, exact
  xattr/resource-fork/BSD-flag restore and verification, disk-backed external
  enumeration plus capture/materialization hard-link grouping, terminal
  capture/discard lifecycle, managed spool/replay with dirty fail-closed fallback
  requiring discard or explicit cooperative conversion, committed-cleanup
  outcomes, set-ID rejection before projection, APFS
  case/normalization sibling preflight, fd-only xattr/ACL/flag/root metadata,
  descriptor-relative identity-checked owned-tree/staging cleanup, streaming
  directory/metadata visitors, one capture enumeration scratch connection,
  driver-owned pinned managed spools, no-follow top-level parent/basename
  admission across every parent component, an aggregate 1 MiB xattr memory
  ceiling, identity-shared writer leases, exclusive managed-root creation, and
  metadata-aware incremental replay after successful managed mutations so
  changed file/parent metadata roots reflect native mtimes; capture, mutation,
  rename, and cleanup retain the
  originally pinned workspace handle rather than reopening its pathname, and
  writer/capture admission uses one atomic shared state transition;
  capture revalidates the public parent/basename binding before and after walk
  and immediately before publication, owned cleanup first detaches to an
  exclusive identity-verified tombstone, dirty managed capture fails closed,
  and successful replace/rename replay carries bounded changed-file/affected-
  parent metadata without a full scan;
  materialization also revalidates the public binding at entry and after final
  root sync, recursive cleanup quarantines and post-verifies every child before
  unlink, and descriptor metadata is serialized into the pinned spool so 64
  edits do not multiply the in-memory xattr ceiling;

On this host, new objects under `/tmp`, `/private/tmp`, and the repository all
acquire exact `com.apple.provenance`; the attribute immediately reappears after
a successful removal. The Apple adapter now treats only that exact name as
environmental metadata: it is never canonicalized or restored and is filtered
from native equality. The remaining exclusion list stays fail-closed, with no
prefix filtering. This pre-release correction does not change the canonical
profile because provenance was never encoded by an accepted root and the
profile preimage does not bind native xattr admission policy. Focused metadata
tests and the SDK Bash/capture/reopen workflow pass without deletion hacks.

The Apple/APFS PoC includes runtime APFS clone with typed fallback, same-offset
managed patch, final-reachable mutation emission, test-only legacy writers,
exact constructor/genesis/crash-owned cleanup, StoreId-bound staging custody,
and one evaluator-owned S0-S12 workflow. Concurrent Verified snapshot readers,
1,000 retained revisions, real child-process publication restart faults, and
the frozen three-run terminal campaign pass. The campaign observed gross wall
3.34–5.76 seconds, RSS 18,235,392–20,774,912 bytes, operation-Q current/high
water 0/4,194,304 bytes, FD 4→4, and zero owned residue. Changed-root
incremental materialization is explicitly outside PoC v1; no claims are made
for hardware power loss, production packaging, hostile writers, or non-Apple
performance.
