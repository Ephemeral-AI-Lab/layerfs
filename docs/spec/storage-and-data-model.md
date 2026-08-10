# Storage and data model

| Field | Value |
| --- | --- |
| Status | Draft |
| Scope | Storage, integrity, recovery, and collection |
| Last updated | 2026-08-10 |

This document defines the target storage contract for Ephemeral AI FS. It is
normative unless a section is explicitly labeled as prototype evidence or
non-normative rationale. The words MUST, MUST NOT, SHOULD, SHOULD NOT, and MAY
have the meanings established in [`SPEC.md`](../../SPEC.md).

The filesystem API and publication rules are specified separately. This
document defines how those behaviors become durable without depending on a
particular SQLite binding, cloud runtime, container implementation, or mount
protocol.

## 1. Goals and boundaries

The storage layer MUST provide:

- one durable, linear main revision history;
- a revisioned filesystem namespace that supports directories, regular files,
  symbolic links, and hard links;
- immutable, deduplicated file content;
- branch-local copy-on-write pages and ordered structural patches;
- atomic namespace and revision updates;
- restart recovery based on committed database state;
- exact, bounded garbage collection; and
- measurements whose byte boundaries are unambiguous.

The storage layer MUST NOT import host-specific storage, remote procedure call,
FUSE, process, container, or scheduler types. A host integration MAY choose
when to open a filesystem, run maintenance, or expose metrics, but it MUST do
so through the portable contracts in this specification.

## 2. Target behavior and prototype evidence

The target design in this document is not a declaration that the behavior is
already implemented. A requirement becomes implemented only after the shared
conformance suite passes against every supported database adapter.

The source prototype provides the following evidence:

- it stores SHA-256-addressed chunks in SQLite;
- it uses deterministic content-defined chunking with 32 KiB minimum, 128 KiB
  target, and 512 KiB maximum parameters;
- it encodes each manifest entry as 32 raw hash bytes followed by one 32-bit
  little-endian size;
- it upserts complete 4 KiB branch pages by branch, path, and page index;
- it records structural changes in application order and can reconnect local
  chunking to an unchanged manifest boundary;
- it creates a revision and moves the main head in one SQLite transaction;
- it recovers a durable publish result after engine recreation; and
- its tests preserve active branch data while reclaiming unrelated objects.

The prototype is not the persisted format defined here. In particular, the
target uses versioned, self-describing manifests, stable inode identities,
explicit tombstone columns, explicit schema migrations, bounded garbage
collection, and a portable database interface. Prototype table names,
lowercase hexadecimal hash strings, magic string sentinels, flat paths, and
direct runtime storage calls are evidence only and MUST NOT be treated as a
compatibility promise.

## 3. Terms

**Object**
: An immutable byte string addressed by the SHA-256 digest of those exact
  bytes. In version 0.1, an object is one content-defined file chunk.

**Manifest**
: A canonical binary value that identifies a complete regular-file value and
  contains the ordered object hashes and lengths needed to reconstruct it.

**Inode**
: A stable identity for a filesystem object and its metadata. More than one
  directory entry may reference the same inode.

**Directory entry**
: A parent-directory inode and name bound to a child inode.

**Revision**
: An immutable, durable transition in the main namespace. Revisions form one
  parent-linked linear history in version 0.1.

**Head projection**
: The materialized inode and directory-entry state at the current main
  revision. It is an index, not an independent source of truth.

**Branch overlay**
: Durable private changes rooted at one immutable base revision. An overlay
  may reference shared manifests and may own pages, patches, and newly
  materialized manifests.

**Structural patch**
: An ordered edit `(offset, deleteLength, insertBytes)` whose insertion or
  deletion changes file length, or which cannot safely use the page overlay.

**Root mutation generation**
: A monotonically increasing integer changed by every transaction that can
  add, remove, or replace a garbage-collection root.

## 4. Portable transactional database contract

### 4.1 Required interface

The core MUST depend on a semantic interface equivalent to the following. The
names are illustrative; the behavior is normative.

```ts
type SqliteValue = null | string | number | Uint8Array;
type SqliteBindings = readonly SqliteValue[];
type SqliteRow = Readonly<Record<string, SqliteValue>>;

interface SqliteRunResult {
  readonly changes: number;
  readonly lastInsertRowid?: number;
}

interface FilesystemSqlExecutor {
  run(sql: string, bindings?: SqliteBindings): SqliteRunResult;
  all<Row extends SqliteRow = SqliteRow>(
    sql: string,
    bindings?: SqliteBindings,
  ): readonly Row[];
}

type TransactionMode = "read" | "write" | "exclusive";

interface FilesystemDatabaseAdapter extends FilesystemSqlExecutor {
  readonly kind: "sqlite";
  readonly readOnly: boolean;
  readonly capabilities: {
    readonly maxBindings: number;
    readonly maxBlobBytes: number;
  };
  transaction<T>(
    mode: TransactionMode,
    callback: (tx: FilesystemSqlExecutor) => T,
  ): T;
  close(): void | Promise<void>;
}
```

This is the same public adapter interface defined by
[`filesystem-api.md`](./filesystem-api.md). An adapter MAY expose additional
diagnostic properties, but the core MUST NOT require them for correctness.

### 4.2 SQL and value behavior

An adapter:

1. MUST use SQLite semantics and support parameterized statements, BLOBs,
   `NULL`, uniqueness constraints, foreign keys, indexes, transactions,
   savepoints, common table expressions, `ON CONFLICT`, and `RETURNING`;
2. MUST preserve all `Uint8Array` bytes, including a view with a nonzero
   `byteOffset`, and MUST return byte values that the caller can retain after
   the next query;
3. MUST preserve every persisted core integer as an exact JavaScript safe
   integer and MUST reject a value that would require lossy conversion;
4. MUST reject non-finite numbers and integers outside the safe range;
5. MUST report constraint, busy, corruption, and resource-limit failures as
   distinguishable error categories while preserving the underlying message
   for diagnosis;
6. MUST report exact positive safe-integer `maxBindings` and `maxBlobBytes`
   capabilities that apply to core statements; and
7. MUST enable foreign-key enforcement before making the filesystem available.

All dynamic values MUST be passed as bindings. Identifiers and SQL fragments
MUST come from static core code, not from paths, branch identifiers, actor
identifiers, or other caller-controlled values.

### 4.3 Transaction behavior

`transaction` MUST execute against one consistent SQLite snapshot and provide
atomic commit and deterministic rollback when the callback writes. Its mode
has these semantics:

- `read` MUST establish a stable read snapshot and MUST fail with the public
  read-only error if the callback attempts a write;
- `write` MUST serialize conflicting writers before committing; and
- `exclusive` MUST prevent another initializer or migrator from observing and
  acting on the same schema state before the callback commits.

A successful return means every callback write is durable according to the
adapter's documented durability policy. An adapter MUST reject `write` and
`exclusive` modes when `readOnly` is true.

Transaction callbacks MUST be synchronous and MUST NOT return a promise. The
core MUST perform asynchronous hashing, compression experiments, or other
awaited work before entering a write transaction. After such work, the write
transaction MUST re-read and validate every generation, head, or base value on
which the prepared result depends.

If a transaction callback throws, the adapter MUST roll back every statement
executed by that callback and rethrow the failure. Nested calls MAY use
savepoints, or the core MAY guarantee that it never asks the adapter to nest a
transaction. The choice MUST be documented.

Write transactions MUST serialize conflicting writers. Busy retry policy
belongs to the adapter, MUST be bounded, and MUST never rerun a caller callback
after the callback has produced an externally visible side effect. Core
transaction callbacks therefore SHOULD be pure database work.

A local SQLite adapter SHOULD map the modes to `BEGIN`, `BEGIN IMMEDIATE`, and
`BEGIN EXCLUSIVE`, or stronger equivalent guarantees. A runtime-owned SQLite
adapter MAY map both `write` and `exclusive` to its synchronous transaction
facility when that facility already serializes every operation for the one
database owner. In particular, a Durable Object adapter maps them to
`transactionSync`; the core MUST NOT issue raw `BEGIN` or `COMMIT` statements
inside that callback. Mode mapping MUST NOT weaken the behavior above.

### 4.4 Read stability and garbage collection

A read operation MUST either:

- read its namespace row, manifest, and required objects in one read
  transaction; or
- acquire a durable read lease that garbage collection treats as a root until
  the operation releases it.

Version 0.1 byte-array reads SHOULD use one read transaction. A streaming read
MUST either copy every selected object in its opening read transaction or hold
the durable read lease defined in section 13. Merely reading a head identifier
and then loading unprotected objects in later transactions is not sufficient,
because a concurrent garbage-collection cycle could reclaim historical
objects between those steps.

## 5. Database identity and schema evolution

### 5.1 Database identity

An initialized database MUST set SQLite `application_id` to hexadecimal
`0x45414653` (ASCII `EAFS`). A database with another nonzero application ID
MUST NOT be opened or modified as an Ephemeral AI FS database. A zero
application ID permits initialization only when `sqlite_schema` contains no
user table, index, view, or trigger. A zero-ID database with any user object
MUST fail as an unsupported schema and MUST remain unchanged.

The database MUST contain one `efs_meta` row with at least:

| Field | Meaning |
| --- | --- |
| `schema_version` | Exact relational schema version |
| `filesystem_id` | Stable, randomly generated filesystem identifier |
| `main_revision` | Current durable main revision |
| `root_inode` | Stable root-directory inode identifier |
| `root_mutation_generation` | Garbage-collection consistency counter |
| `next_allocation_sequence` | Monotonic sequence for objects and manifests |
| `created_at_ms` | Informational creation time |

SQLite `user_version` MUST equal `efs_meta.schema_version`. A mismatch is an
integrity failure, not permission to guess which value is current.

The relational schema version, manifest format version, and chunker algorithm
version are separate values. Changing one MUST NOT silently reinterpret either
of the others.

### 5.2 Opening a database

Opening MUST perform these steps before exposing a filesystem handle:

1. validate adapter capabilities and inspect `application_id`, `user_version`,
   `sqlite_schema`, and metadata in a `read` transaction;
2. when schema work is unnecessary, validate the singleton metadata row, root
   inode, main revision, indexes, constraints, and maintenance state in that
   read snapshot;
3. when initialization or migration is required, reject a read-only adapter;
4. otherwise enter an `exclusive` transaction and recheck every value from
   step 1 before changing schema;
5. initialize a truly empty database or run required migrations in order;
6. validate the resulting schema and commit; and
7. only then make the handle available.

A current-schema read-only database MUST open successfully after read-only
validation. It MUST NOT require a write transaction merely to open.

An implementation MUST reject a schema newer than its maximum supported
version with an `UnsupportedSchemaVersion` error and MUST NOT write to that
database. It MUST reject a version older than its minimum migratable version
without attempting a partial upgrade.

### 5.3 Migration rules

Each released schema version MUST have an ordered migration to the next
released version and a fixture in the conformance suite. A migration:

- MUST be deterministic;
- MUST validate its expected source schema;
- MUST preserve filesystem and retained revision behavior;
- MUST update `efs_meta.schema_version` and `user_version` in the same final
  transaction;
- MUST roll back to the previous usable version on failure; and
- MUST be safe to invoke again after process termination.

A version transition that fits within configured transaction limits SHOULD run
in one transaction. A large data rewrite MAY use a resumable shadow table or
shadow column in bounded transactions, but the old schema MUST remain the
authoritative readable representation until one final transaction validates
the rewrite, switches readers, and advances the version. Opening MUST NOT
return a normal filesystem handle while such a migration is incomplete.

Destructive downgrade is outside version 0.1. Unknown tables or columns MAY be
preserved, but unknown values in a field whose meaning affects correctness
MUST cause an explicit unsupported-format error.

## 6. Logical persisted relations

The exact `CREATE TABLE` statements live with the implementation, but version
0.1 MUST represent the relations and constraints in this section. Physical
indexes, integer enum values, and normalized helper tables MAY change in a
schema migration without changing filesystem behavior.

### 6.1 Content relations

`efs_objects` stores:

- `hash`: 32-byte SHA-256 digest, primary key;
- `size`: byte length, equal to `length(bytes)`;
- `bytes`: exact immutable payload; and
- `allocation_sequence`: unique, monotonically increasing integer.

`efs_manifests` stores:

- `hash`: 32-byte SHA-256 digest of the canonical encoded manifest, primary
  key;
- `file_size`: logical file byte length;
- `entry_count`: number of encoded chunk entries;
- `encoded`: canonical manifest bytes; and
- `allocation_sequence`: unique, monotonically increasing integer.

There MUST NOT be one steady-state database row per manifest entry. Readers
MUST decode entries from the compact BLOB. A migration MAY temporarily retain
legacy entry rows, but new writes MUST use the compact format and garbage
collection MUST understand both representations until the migration completes.

### 6.2 Revisions and namespace

`efs_revisions` stores the revision identifier, nullable parent revision,
creation time, opaque writer identifier, and change count. Revision zero is
the bootstrap revision, has no parent, and contains the root directory.
Revision identifiers MUST increase monotonically. In version 0.1 every
nonzero revision MUST have the immediately preceding main revision as its
parent.

The current main namespace MUST have materialized inode and directory-entry
tables. Historical changes MUST be recorded as immutable revision rows:

- a head inode row contains inode identity, type, mode, `birthtimeMs`,
  `mtimeMs`, `ctimeMs`, link count, regular-file size and manifest hash, or
  symbolic-link target as applicable;
- a head entry-slot row represents a present or absent
  `(parentInode, nameSortKey)` binding and stores the original name;
- an inode revision row stores a complete changed inode value or an explicit
  inode tombstone; and
- an entry-slot revision row stores a complete present or absent slot value.

Ownership fields are not part of schema version 1. All three inode timestamps
MUST be persisted as nonnegative safe-integer Unix epoch milliseconds. A
logical mutation samples the filesystem clock once. Each updated timestamp is
the greater of that sample and the prior stored value, so a clock moving
backward cannot move inode time backward.

The entry name's exact UTF-8 bytes are its `nameSortKey`. The sort key MUST be
stored as a BLOB, MUST decode to the stored name, and MUST be the uniqueness key
within one parent. The head relation MUST have an index ordered by
`(parentInode, nameSortKey)` so lookup, ordering, and `startAfter` pagination do
not depend on SQLite text collation.

#### 6.2.1 Durable conflict tokens

The head projection and revision history MUST persist these independent token
classes. A token is an opaque, never-reused safe integer or byte string. The
creating revision identifier MAY serve as a token when it is unambiguous.

**Entry-slot token**
: Changes whenever a child name is created, deleted, or rebound. An absent
  slot retains its latest token. A slot that never existed has a distinguished
  absent state with no token. Therefore create followed by delete cannot become
  indistinguishable from the earlier absence.

**Inode identity token**
: Created with an inode and never changes or reappears. It is checked with the
  entry token for every traversed ancestor, so replacing, moving, deleting, or
  recreating a path component is observable.

**Node token**
: Changes for regular-file content changes and explicit metadata changes such
  as `chmod`. It does not change for a parent directory's timestamp update that
  is only an implied effect of adding, removing, or renaming a child.

**Row version**
: Changes whenever any persisted inode field changes, including an implied
  parent timestamp. It supports reconstruction and cache invalidation but is
  not by itself a whole-directory conflict key.

**Subtree token**
: Exists for each directory and changes when any namespace, content, or
  explicit metadata state at or below that directory changes. Mutations MUST
  update subtree tokens through the root in the same transaction. Ordinary
  child operations do not expect the parent subtree token; recursive removal
  and directory rename do.

An ancestor anchor is the pair of the expected entry-slot token and expected
child inode identity token for each traversed path component. The root anchor
is its inode identity token. These anchors are durable values, not hashes of a
path string or process-local cache entries.

Implied parent timestamp updates MUST update the parent row version and stored
timestamps without changing its node token. Publication of independent child
slots therefore derives each parent timestamp from the publishing mutation and
current parent value using the monotonic timestamp rule. An explicit metadata
mutation changes the node token and remains conflict checked.

Digest columns MUST contain a digest or `NULL`; they MUST NOT contain magic
strings for missing or deleted content. Deletion MUST use a typed tombstone or
operation field.

The head projection and revision rows written for one revision MUST agree. A
revision lookup MUST be definable solely from immutable revision history, even
if an implementation normally serves the head projection. This includes
absent entry-slot tokens, inode identity, node and row versions, subtree
tokens, and all timestamp values. A verifier MUST be able to rebuild the head
projection and compare it with the stored projection.

The following invariants apply:

- inode identifiers MUST NOT be reused within a filesystem;
- the root inode MUST exist, MUST be a directory, and MUST never have a parent
  directory entry;
- every live directory entry MUST reference a live inode;
- directory parent relationships MUST be acyclic;
- a regular-file inode MUST reference exactly one valid manifest and its size
  MUST equal the manifest file size;
- a directory or symbolic-link inode MUST NOT reference a file manifest;
- a regular-file link count MUST equal its live directory-entry aliases;
- each non-root directory and symbolic link MUST have link count one;
- the root directory MUST have link count one; and
- a rename or new hard link MUST reuse file content identity rather than copy
  object bytes.

Names, metadata field semantics, and namespace operation errors are defined in
[`filesystem-api.md`](./filesystem-api.md).

### 6.3 Branch content overlays

The branch specification owns lifecycle and conflict behavior. The storage
model MUST nevertheless provide durable relations equivalent to the following.

`efs_branch_ids` permanently records every branch identifier ever used. Its
rows MUST survive terminal-record retention and garbage collection, so a
branch identifier cannot be reused during the filesystem lifetime.

`efs_branches` stores identifier, base revision, state, monotonic generation,
creation time, nullable `terminalAt`, and nullable `mergedRevision`. Its state
constraint permits only `active`, `merged`, or `discarded`. Additional checks
MUST enforce:

- `active` has null `terminalAt` and null `mergedRevision`;
- `merged` has non-null `terminalAt` and exactly one `mergedRevision`;
- `discarded` has non-null `terminalAt` and null `mergedRevision`; and
- a terminal branch owns zero mutable overlay and expectation rows.

The mutable relations include:

- branch inode and directory-entry overlay rows;
- one branch-file row containing an explicit base-existence expectation,
  nullable base manifest and size, plus an optional materialized manifest and
  size;
- branch page rows keyed by `(branch, inode, pageIndex)`;
- ordered structural patch rows keyed by `(branch, inode, sequence)`; and
- zero or more insertion segments keyed by patch and segment index;
- durable expectation rows for entry slots, nodes, subtrees, and ancestor
  anchors.

An expectation row MUST store its kind, stable subject identity, expected
token or distinguished never-present value, expected child inode identity when
applicable, base revision, deterministic conflict path and reason, and the
branch generation at which it entered the write set. Once recorded for a
subject, its expected base value MUST NOT be replaced with a later main value.
All expectations added by one mutation and the one generation increment MUST
commit atomically.

All overlay rows MUST be reachable from exactly one branch row and SHOULD use
foreign-key cascade for cleanup. A branch base revision MUST remain a garbage-
collection root while that branch can still be read or published.

Terminal branch metadata is retained for 30 days by default and MUST be
configurable no lower than 7 days. Removing expired terminal metadata MUST NOT
remove its permanent `efs_branch_ids` row or a retained operation result.

### 6.4 Publication operations and replay

`efs_publication_operations` is the permanent used-operation-ID relation. It
is keyed by the exact operation identifier and binds that identifier to one
branch identifier and one captured branch generation for the filesystem
lifetime. It also stores reservation nonce and expiry, creation time, and one
state from `reserved`, `merged`, `conflict`, or `expired`.

The first publish attempt with an operation identifier MUST reserve it in a
short write transaction before expensive preparation. A concurrent attempt
MUST either replay the completed result, wait or retry the same unexpired
reservation, or reclaim an expired reservation for the same branch and
generation. It MUST NOT bind the identifier to another branch or generation.
If the bound branch generation changes before finalization, that identifier
remains used and MUST NOT publish the newer generation.

`efs_publication_results` contains the retained replay payload and has exactly
one row per completed operation. It stores all common result fields and either:

- merged outcome, base revision, parent revision, merged revision, and the
  complete ordered changed-path list; or
- conflict outcome, base revision, observed head revision, and the complete
  ordered list of conflict path, reason, expected revision, and actual
  revision.

Changed paths and conflicts MAY use normalized child rows, but their order and
all public fields MUST be preserved for exact replay. A conflict result MUST
finalize only the operation reservation and result records. It MUST leave main,
branch state, branch generation, and every overlay row unchanged.

Publication results are retained for 30 days by default and MUST be
configurable no lower than 7 days. On expiry, the replay payload MAY be deleted
and the permanent operation row MUST transition to `expired`. A later retry of
the same identifier and branch returns `OperationResultExpired`; another
branch returns `OperationBranchMismatch`. An expired identifier MUST never be
treated as a new operation.

### 6.5 Lease relations

`efs_leases` stores a collision-resistant lease identifier, owner kind, owner
identifier, optional branch and captured generation, creation time, last
renewal time, expiry, and state. Owner kind is one of `read-stream`,
`stream-write`, `publication`, or a later schema-versioned kind.

`efs_lease_manifests` and `efs_lease_objects` store the complete manifest and
object membership protected by each lease. Membership rows MUST reference one
active lease. Section 13 defines acquisition, renewal, release, expiry, and
garbage-collection behavior.

## 7. Content objects

### 7.1 Identity

The object identifier is:

```text
SHA-256(objectBytes)
```

It MUST be stored and compared as 32 raw bytes. User-facing diagnostics MAY
render the digest as 64 lowercase hexadecimal characters, but the text form is
not persisted identity.

Object bytes are immutable. Inserting an already-present digest MUST behave as
deduplication only after the stored row's length and digest have been
verified. If supplied bytes or metadata disagree with an existing row, the
operation MUST fail with `StorageIntegrityError`; it MUST NOT overwrite the
row or silently choose either value.

### 7.2 Verification

Before a new object is inserted, the core MUST compute SHA-256 over the exact
bytes it will bind. On load, the core MUST verify all of the following before
returning bytes to a caller:

1. the row exists;
2. the stored size is a nonnegative safe integer;
3. the stored size equals the BLOB byte length;
4. the requested manifest-entry size equals the stored size; and
5. SHA-256 of the BLOB equals the requested object hash.

An implementation MAY cache a successful digest verification for an immutable
object during one process lifetime. It MUST invalidate that cache when the
database is reopened, restored, or mutated outside the core. A size check alone
is never proof that bytes match a content hash.

The core MUST copy or otherwise freeze caller-owned byte arrays before an
asynchronous hash can race with caller mutation.

## 8. Deterministic FastCDC version 1

### 8.1 Parameters

New regular-file manifests MUST use these defaults:

| Parameter | Bytes |
| --- | ---: |
| Minimum | 32,768 |
| Target average | 131,072 |
| Maximum | 524,288 |

The chunker configuration MUST satisfy `0 < minimum <= average <= maximum`,
and the average MUST be a power of two. Configuration is embedded in every
manifest. A filesystem MAY choose different valid parameters for new writes,
but readers MUST honor the parameters stored in each manifest. Changing the
filesystem default MUST NOT reinterpret existing manifests.

### 8.2 Gear table

FastCDC version 1 uses a 256-entry unsigned 32-bit Gear table. It MUST be
generated identically on every platform:

```text
seed = 0x9e3779b9
for i from 0 through 255:
    seed = uint32(seed XOR uint32(seed << 13))
    seed = uint32(seed XOR (seed >>> 17))
    seed = uint32(seed XOR uint32(seed << 5))
    gear[i] = seed
```

All shifts, additions, and masks in this section use unsigned 32-bit wraparound.

### 8.3 Boundary algorithm

For a chunk beginning at `start` in a byte array:

```text
minimumEnd = min(start + minimum, inputLength)
normalEnd  = min(start + average, inputLength)
maximumEnd = min(start + maximum, inputLength)

if minimumEnd >= inputLength:
    return inputLength

bits      = log2(average)
earlyMask = uint32(2^min(30, bits + 1) - 1)
lateMask  = uint32(2^max(1, bits - 1) - 1)
gearHash  = 0

for cursor from minimumEnd while cursor < maximumEnd:
    gearHash = uint32(uint32(gearHash << 1) + gear[input[cursor]])
    mask = earlyMask if cursor < normalEnd else lateMask
    if (gearHash AND mask) == 0:
        return cursor + 1

return maximumEnd
```

Chunking begins at offset zero and repeatedly invokes the boundary algorithm
until the input is covered. An empty input produces no chunks. Every nonempty
input MUST be covered exactly once, without gaps, overlap, or zero-length
chunks. No chunk may exceed the configured maximum.

This exact algorithm, including table generation, scan indices, masks, and
32-bit overflow, defines `fastcdc-v1`. An optimization MUST produce identical
boundaries. Any intentional boundary change requires a new chunker identifier;
it MUST NOT be released as a silent implementation change.

### 8.4 Local rechunking

After a local edit, an implementation SHOULD begin rechunking at the start of
the old chunk that intersects the earliest dirty byte. It MAY expand the dirty
window and reconnect to an old suffix only when a newly produced boundary maps
exactly to an unchanged old boundary after accounting for the edit's length
delta.

The spliced result MUST be byte-for-byte the same canonical manifest that a
full `fastcdc-v1` scan of the resulting file would produce. Matching content
bytes without matching canonical boundaries is insufficient. If safe
reconnection cannot be proved, the implementation MUST expand the window and
MAY fall back to a full-file scan. Widely distributed pages and multiple
structural patches are expected fallback cases.

## 9. Compact manifest version 1

### 9.1 Canonical encoding

A version 1 manifest is one binary value. Multibyte integers are unsigned
little-endian. The 32-byte header is:

| Offset | Size | Field | Required value |
| ---: | ---: | --- | --- |
| 0 | 4 | Magic | ASCII `EAFM` |
| 4 | 2 | Manifest version | `1` |
| 6 | 1 | Hash algorithm | `1` for SHA-256 |
| 7 | 1 | Chunker algorithm | `1` for `fastcdc-v1` |
| 8 | 4 | Minimum chunk size | Manifest parameter |
| 12 | 4 | Average chunk size | Manifest parameter |
| 16 | 4 | Maximum chunk size | Manifest parameter |
| 20 | 8 | Logical file size | Unsigned byte length |
| 28 | 4 | Entry count | Number of entries |

The header is followed immediately by `entryCount` entries of 36 bytes:

| Entry offset | Size | Field |
| ---: | ---: | --- |
| 0 | 32 | Raw SHA-256 object hash |
| 32 | 4 | Object byte length |

The encoded length MUST equal `32 + entryCount * 36`. There is no padding,
trailer, map ordering, text conversion, or platform-dependent value. The
manifest identifier is `SHA-256(encodedManifestBytes)` and MUST be stored as 32
raw bytes.

### 9.2 Validation

Before using a manifest, a reader MUST validate:

- exact magic and supported version and algorithm identifiers;
- valid chunking parameters;
- exact encoded length with no trailing bytes;
- entry count within configured limits;
- every entry size is positive and does not exceed the manifest maximum chunk
  size;
- the sum of entry sizes equals the logical file size without integer
  overflow;
- zero entries occur if and only if file size is zero;
- SHA-256 of the complete encoding equals the requested manifest hash; and
- every referenced object passes object verification.

The core MUST use checked integer arithmetic. It MUST reject a decoded file
size greater than `Number.MAX_SAFE_INTEGER` in APIs that use JavaScript numeric
offsets, even though the binary field can represent a larger unsigned value.

Two identical file byte strings written with the same chunker identifier and
parameters MUST produce identical manifest encodings and hashes. Timestamps,
paths, inode identifiers, revisions, branches, and host information MUST NOT
appear in a manifest.

### 9.3 Prototype compatibility

The prototype encodes only repeated 36-byte entries and hashes a decimal file
size prefix plus those entries. That format MAY be accepted by a one-time
importer identified as a legacy format. New writes MUST use the self-describing
version 1 format above. A legacy value MUST be validated and rewritten before
it is reported as migrated; it MUST NOT be relabeled without re-encoding and
rehashing.

## 10. Copy-on-write pages and structural patches

### 10.1 Page overlays

The page size is exactly 4,096 bytes. A page overlay is keyed by branch, inode,
and zero-based page index. Its logical offset is `pageIndex * 4096`.

An equal-length, nonempty overwrite MAY use page overlays when:

- the file currently has no structural patches after its materialized base;
- the overwrite does not change file size; and
- the write size is within the implementation's configured page-overlay
  eligibility limit, whose version 0.1 default is 65,536 bytes.

For each touched page, the stored bytes MUST represent the complete page after
the write. A non-final page MUST contain exactly 4,096 bytes. The final page
MUST contain exactly `min(4096, fileSize - pageOffset)` bytes. Page indexes and
lengths outside the file are corruption.

Writing a page that already exists MUST update the same row; it MUST NOT append
another version. One write crossing a page boundary MUST update both page rows
atomically. Reads apply the complete set of page overlays to the materialized
base before applying any later structural patches.

### 10.2 Structural patches

A structural patch records:

```text
(sequence, offset, deleteLength, insertLength, insertionSegments)
```

`offset` and `deleteLength` are interpreted against the branch file value
produced by all lower sequence numbers. The values MUST be nonnegative safe
integers, `offset` MUST not exceed that value's size, and
`offset + deleteLength` MUST not exceed it. The sum of insertion segment
lengths MUST equal `insertLength`.

Patch sequence numbers MUST be unique and strictly increasing for one branch
file. All rows and segments for one logical patch MUST be added atomically.
Insertion segments MUST be ordered, gap-free, and no larger than 524,288 bytes.
An implementation MAY materialize the current branch value into a manifest
instead of retaining a large or long patch chain. Materialization MUST
atomically install the new manifest and clear only the pages and patches it
supersedes.

If page writes precede the first structural patch, the resulting view is:

```text
materialized or base manifest
    -> page overlays in page-index order
    -> structural patches in sequence order
```

After structural patches exist, a later overwrite MUST be recorded in patch
coordinates or the file MUST first be materialized. It MUST NOT create a page
whose coordinate is ambiguously relative to the old base. Truncation is a
patch with an empty insertion; insertion is a patch with zero deletion.

### 10.3 Overlay invariants

At every committed branch state:

- the materialized manifest, if present, is a valid complete file value;
- the base manifest is immutable and remains associated with the branch's base
  revision;
- page and patch rows belong to an active, readable branch file;
- applying the documented overlay order yields exactly one file byte string;
- reads after reopen yield the same byte string; and
- discarding overlay rows cannot change the main namespace or content.

## 11. Revisions and atomic updates

### 11.1 Revision record

A durable main mutation MUST create one revision containing:

- a new monotonic revision identifier;
- the prior main revision as parent;
- an informational creation timestamp;
- an opaque writer or source identifier;
- the complete set of inode and directory-entry changes; and
- enough full changed values, token transitions, and tombstones to reconstruct
  any retained revision.

Timestamps and writer identifiers are metadata and MUST NOT affect manifest or
object identity. Except for a successful branch publication, a transaction
with no observable namespace or metadata change SHOULD return the existing
main revision instead of adding an empty revision. A successful branch
publication MUST create its one auditable revision even when its net change set
is empty, as required by the branch specification.

### 11.2 Atomic commit boundary

One main revision transaction MUST atomically:

1. revalidate the expected main head and every baseline applicable to the
   operation;
2. verify or insert required immutable manifests and objects;
3. insert the revision record and immutable change rows;
4. update the head inode and directory-entry projections;
5. record an operation result when the operation has a replay identifier;
6. transition or clear a source overlay when the operation is a branch
   publication;
7. set `efs_meta.main_revision` to the new revision; and
8. increment `root_mutation_generation`.

The main revision MUST be updated last within the logical transaction. SQLite
may execute statements in another safe order, but no committed database state
may expose a new head without all of its content and namespace records.

Hashing and canonical manifest preparation MAY occur before this transaction.
If immutable objects are staged in earlier bounded transactions, each staged
allocation MUST receive a new allocation sequence. Staging for a prepared
publication or streamed write MUST have a durable, unexpired staging lease that
garbage collection treats as a root. Unleased staging is an unreachable orphan
and MAY be collected. The final transaction MUST verify that every referenced
row still exists and matches its digest.

Rename MUST update source and destination directory entries in one transaction.
A hard-link operation MUST add the directory entry and update the shared
inode's link count in one transaction. No operation may leave a half-rename,
dangling entry, or incorrect link count after failure.

### 11.3 Final publication transaction

Publication preparation MUST capture the branch generation and, when present,
the operation reservation nonce. The final publication write transaction MUST
perform these checks and effects without yielding:

1. Re-read the permanent operation row. Replay a completed result. Otherwise,
   require the same operation ID, branch, captured generation, unexpired
   reservation, and reservation nonce.
2. Require the branch to remain `active` at the captured generation.
3. Read the current main head. That head becomes the candidate revision's
   parent. It need not equal the branch base or a head seen during preflight.
4. Compare every entry-slot, node, subtree, and ancestor expectation with the
   token in current main, including expected absent slots.
5. If any expectation differs, construct the complete deterministic conflict
   list. With an operation ID, insert its replay payload and finalize the
   operation as `conflict`. Commit no main, revision, branch, generation, or
   overlay change.
6. Otherwise verify or insert every candidate object and manifest, allocate one
   revision whose parent is the head from step 3, and apply the complete change
   set to that parent.
7. Derive implied parent timestamps from current parent rows and the publishing
   mutation without changing their node conflict tokens.
8. Advance main, set the branch to `merged`, set `terminalAt` and
   `mergedRevision`, remove all mutable overlay and expectation rows, and
   release publication staging.
9. With an operation ID, insert the complete merged replay payload and finalize
   the permanent operation row as `merged`.
10. Increment root mutation generation and commit once.

The preflight head is only a preparation hint. Requiring current head to equal
the branch base would incorrectly reject independent writers. When 50 branches
from one base change independent slots or nodes, all 50 MUST be able to merge
into a 50-revision parent chain. When they all change the same node token,
exactly one merges and 49 return conflicts.

A conflict without an operation identifier MAY return after the same
serialized token check without storing a replay row. It still MUST leave main
and branch state unchanged. A conflict with an operation identifier commits
only result finalization and any staging release; it does not make the branch
terminal.

### 11.4 Reconstructable revision retention

Revision pruning MUST preserve a proof path for every retained revision,
including current main, every active branch base, held results, and explicit
administrative retention. An implementation MUST use one or both of:

- immutable checkpoint snapshots containing every inode, present and retained
  absent entry slot, manifest reference, timestamp, link count, and conflict
  token visible at the checkpoint revision; or
- the complete contiguous ancestor delta chain back to an earlier retained
  checkpoint or revision-zero snapshot.

For every retained target revision, applying its complete ordered deltas to
the nearest retained checkpoint MUST reconstruct exactly that target. A
checkpoint is authoritative only after one transaction stores all of its rows,
validates counts and a canonical checksum, and marks it complete. Incomplete
checkpoint staging is never a reconstruction root.

Every retained nonzero revision header MUST reference an existing parent
header, and parent links MUST never be rewritten to skip pruned history. Header
and delta pruning MUST therefore retain required ancestors, or retain immutable
headers while replacing old state deltas with a verified checkpoint. An active
branch based on an old revision prevents collection of every checkpoint and
delta needed to reconstruct its complete base, including paths it never
changed.

Pruning and checkpoint installation MUST increment root mutation generation.
They MUST run in bounded transactions without exposing a revision that cannot
be reconstructed between batches.

## 12. Integrity failures and corruption handling

Persisted data is not trusted merely because it came from SQLite. Invalid
application metadata, namespace relations, revision ancestry, manifests,
objects, overlays, or maintenance state MUST produce a typed
`StorageIntegrityError` containing the entity kind, a printable entity
identifier, and a reason. Raw file bytes, insertion bytes, secrets, and full
paths not already requested by the caller SHOULD NOT be included in error
telemetry.

SQLite corruption errors MUST be surfaced as `DatabaseCorruptionError` with
the adapter error attached as its cause. Unsupported schema, manifest, hash,
or chunker versions MUST use an unsupported-version error, not a generic
missing-file result.

On an integrity failure, the operation:

- MUST NOT return partial file content as a successful read;
- MUST NOT advance main, publish a branch, or delete suspected rows;
- MUST roll back an active write transaction; and
- SHOULD emit a structured integrity event through the optional observer.

The core MUST NOT automatically repair corruption by deleting an object,
rewriting a digest, dropping history, or selecting one of two inconsistent
values. An explicit verifier MAY rebuild derived head projections after proving
that immutable revision history and content are intact. Irreversible repair is
outside this specification.

An explicit verification operation SHOULD support bounded checks of metadata,
namespace invariants, manifest hashes, object hashes, and head reconstruction.
Database-wide SQLite integrity checks MAY be exposed separately because their
cost and locking behavior are adapter-specific.

Before an error crosses the public filesystem API, the core MUST map internal
categories as follows and retain the internal error as `cause`:

| Internal category | `FilesystemError` code |
| --- | --- |
| Persisted or SQLite corruption | `ECORRUPT` |
| Unsupported schema, manifest, hash, or chunker version | `ESCHEMA` |
| Configured content or operation limit | `EFBIG` |
| SQLite capacity exhaustion | `ENOSPC` |
| Unexpected storage, hashing, or verification failure | `EIO` |

Normal branch conflicts remain result values, and branch lifecycle or replay
errors remain the `BranchError` values defined by the branch specification.

## 13. Durable leases and restart recovery

### 13.1 Lease lifecycle

Each lease identifier MUST have at least 128 bits of randomness or equivalent
collision resistance. A lease also carries a secret owner nonce. Renewal,
membership changes, release, and conversion into a durable result MUST match
both values; knowing a public stream, branch, or operation identifier is not
enough to mutate a lease.

A read-stream lease MUST bind to the selected revision, inode identity, node
token, manifest, and stream identifier. A streamed-write lease MUST bind to a
write-session identifier and its target baseline. A publication lease MUST
bind to branch identifier, captured branch generation, and operation
reservation when present. Reusing a lease for another owner or generation is
an integrity error.

Lease acquisition, renewal, release, expiration, or membership change is a
garbage-collection root mutation and MUST increment
`root_mutation_generation`. Lease times are safe-integer Unix epoch
milliseconds. The effective lease clock MUST have a persisted nondecreasing
floor; a backward wall-clock jump MAY delay collection but MUST NOT expire a
lease early.

#### Read-stream acquisition

The first write transaction selects the stream snapshot and creates a
`preparing` lease with its manifest membership. A preparing lease is already a
root, and a reachable manifest protects its transitive objects. The core MAY
then enumerate and add exact object-membership rows in bounded transactions.
It MUST validate the manifest and object set before one transaction changes the
lease to `active`. `readStream` MUST NOT resolve before activation.

The stream owner MUST renew before expiry while it remains readable. Full
consumption, cancellation, stream error, or filesystem close MUST release the
lease. If renewal observes expiry or owner mismatch, the stream MUST stop
before reading another object and surface the corresponding public error.

#### Staging acquisition

A streamed write or publication MAY create one staging lease before storing
prepared content. Each object or manifest insertion and its membership row
MUST commit together. A final write or publication transaction MUST recheck
the active, unexpired lease, exact owner binding, and complete membership
before referencing staged content.

Successful finalization MUST make the content reachable from main or a branch
and release staging in the same transaction. Abort or failed preparation MUST
release the lease when possible; a process crash leaves it protected only until
expiry. If a streamed-write lease expires, the operation MUST re-stage from
retained input or fail without changing namespace state.

#### Renewal, expiry, and collection races

Renewal MUST compare owner nonce and current expiry in one transaction, extend
from `max(effectiveNow, priorExpiry)`, and increment root mutation generation.
It MUST NOT revive an expired or released lease. Release MUST make membership
non-rooting atomically and be idempotent for the same owner.

The collector MAY expire a lease only in a write transaction that rechecks its
expiry against the effective lease clock and increments root mutation
generation. A GC run whose captured generation predates an acquisition,
renewal, release, expiry, or membership change MUST stop before its next sweep
batch. Allocation high-water marks additionally protect newly staged rows.

Unexpired `preparing` and `active` leases are roots after restart. Expired
leases are not silently deleted during open; bounded maintenance expires them
using the transaction above. Temporary membership without a valid lease is
reclaimable but MUST never authorize deletion by itself.

### 13.2 Restart recovery

SQLite rollback or write-ahead-log recovery is the authority for interrupted
transactions. The core MUST NOT infer a committed revision from partially
prepared in-memory work.

After a restart:

- the metadata head MUST identify the last fully committed revision;
- an interrupted revision transaction MUST have no visible namespace,
  revision, operation-result, or branch-state effects;
- active branch pages and patches from earlier committed edits MUST remain
  readable;
- a completed operation result MUST be replayable without creating a revision
  or rerunning conflict detection;
- a reserved operation MUST remain bound to its original branch and generation
  and MAY be reclaimed only under its reservation-expiry rules;
- unexpired read and staging leases MUST retain their complete membership;
- immutable objects prepared but never referenced MAY remain as orphans; and
- incomplete garbage-collection or migration work MUST be resumed or safely
  abandoned according to its durable state.

Opening MUST validate that the main revision row and root inode exist. It
SHOULD perform inexpensive structural checks. It MUST NOT scan or hash every
content object during normal open; complete validation belongs to the bounded
verifier.

Maintenance state MUST use explicit phases such as `marking`, `sweeping`,
`complete`, and `abandoned`. Temporary mark rows without an active run MAY be
deleted. A restart MUST never interpret the presence of temporary rows alone as
permission to delete content.

## 14. Garbage collection

### 14.1 Roots

Garbage collection MUST preserve every manifest and object reachable from:

1. the current main namespace;
2. every revision retained by policy, interpreted as its complete namespace
   snapshot rather than only the paths changed in that revision;
3. every base revision of an active or otherwise readable branch;
4. every materialized manifest referenced by such a branch;
5. content required by branch pages and patches through their base or
   materialized file;
6. every active read lease;
7. every revision referenced by a retained successful operation result;
8. every unexpired preparing or active staging lease;
9. every complete checkpoint and delta required to reconstruct another root;
   and
10. any explicit administrative hold.

Terminal branch rows do not retain content unless retention policy explicitly
says they do. A retained successful operation result roots its revision; a
conflict result does not independently root content because its branch remains
active. Revision pruning MUST preserve all rows needed to reconstruct every
root snapshot. In particular, retaining the last `N` revision identifiers
without retaining the state visible at the oldest such revision is incorrect.

### 14.2 Mark phase

A collection cycle MUST begin in one short transaction that:

- creates a unique run identifier;
- records `root_mutation_generation`;
- records the greatest object and manifest allocation sequence eligible for
  the cycle; and
- records the selected retention policy.

The collector MUST enumerate root snapshots and decode manifests in bounded
batches. It MUST durably mark manifest hashes and object hashes keyed by run
identifier. It MUST trace both manifest and object membership of every
unexpired preparing or active lease. Leases MUST be checked again before
sweep. A missing or invalid reachable manifest or object MUST fail the cycle
as an integrity error; it MUST NOT be treated as already collected.

If root mutation generation changes before sweeping begins, the run MUST be
abandoned or restarted without deleting content. Marks from an abandoned run
MAY be removed in bounded batches.

### 14.3 Sweep phase

Each sweep batch MUST run in its own write transaction and MUST:

1. verify that the run is still active and root mutation generation still
   equals the captured value;
2. select at most the configured batch limit;
3. delete only unmarked manifests or objects whose allocation sequence is no
   greater than the run's captured high-water value;
4. record progress and exact deleted payload bytes; and
5. commit before starting another batch.

If generation changed, that and all later batches MUST stop. Data already
deleted by earlier batches was unreachable at the captured generation. A new
write that wants to reuse such a digest MUST verify that the row still exists
and reinsert verified bytes atomically before creating the new reference.

Objects MUST NOT be swept until all eligible manifest rows for that batch's
consistent mark set have been considered. Revision and terminal-branch pruning
MUST also be bounded and MUST increment root mutation generation when it
changes roots.

The default delete batch MUST be finite and configurable. Tests SHOULD use a
small batch to force interruption boundaries. Implementations MUST bind values
in groups no larger than the configured safe binding batch and MUST keep every
BLOB within `adapter.capabilities.maxBlobBytes`.

### 14.4 Completion and results

A completed run MUST remove its temporary marks in bounded batches and report:

- run identifier and completion state;
- examined and deleted manifest counts;
- examined and deleted object counts;
- reclaimed object payload bytes;
- reclaimed manifest payload bytes;
- reclaimed branch-overlay payload bytes, if pruning included overlays;
- number of committed batches; and
- elapsed time measured with a monotonic clock.

Reclaimed payload MUST be computed from deleted row lengths, not from change in
database file size. SQLite page allocation, free-list behavior, WAL size, and
vacuum behavior are separate physical metrics.

## 15. Accounting and observability

### 15.1 Snapshot counters

An accounting snapshot MUST use one consistent read transaction and MUST
define at least:

`mainLogicalBytes`
: Sum of sizes of distinct live regular-file inodes at main head. Hard links
  count once.

`storedObjectPayloadBytes`
: Sum of `size` for all object rows.

`storedManifestPayloadBytes`
: Sum of encoded manifest BLOB lengths.

`reachableObjectPayloadBytes`
: Unique object bytes reachable from all selected roots.

`reachableManifestPayloadBytes`
: Unique encoded manifest bytes reachable from all selected roots.

`reclaimablePayloadBytes`
: Stored object, manifest, and overlay payload not reachable or retained by
  policy.

`branchPageBytes`
: Sum of branch page BLOB lengths.

`branchPatchBytes`
: Sum of insertion segment BLOB lengths.

`branchExclusiveObjectBytes`
: Unique object bytes reachable from branches but not selected main revisions.

`branchExclusiveManifestBytes`
: Unique manifest bytes reachable from branches but not selected main
  revisions.

`objectCount`
: Number of object rows.

`manifestCount`
: Number of manifest rows.

`revisionCount`
: Number of retained revision rows.

`branchExclusivePayloadBytes` MUST equal the sum of page, patch, exclusive
object, and exclusive manifest payload under the snapshot's stated root and
retention policy. Shared values MUST be counted once in each set-based metric.

The snapshot MUST state whether namespace metadata and operation-result BLOBs
are included. The default counters above exclude relational row overhead,
indexes, rollback journals, and WAL files.

### 15.2 Physical and operation metrics

An adapter SHOULD expose database main-file bytes, WAL bytes, and free-list
bytes when those values are meaningful. The core MUST label these as physical
storage metrics and MUST NOT compare them directly with logical or payload
bytes without stating the boundary.

The core SHOULD emit structured operation observations for bytes read, bytes
hashed, BLOB bytes submitted, BLOB bytes retained, query count, transaction
count, fallback full scans, local rechunk windows, and elapsed time. Elapsed
time MUST use a monotonic clock. Observability hooks MUST be optional, MUST NOT
select a logging or metrics vendor, and MUST NOT change transaction outcomes.

### 15.3 Maintenance surface

The filesystem API MUST expose a host-neutral logical surface equivalent to:

```ts
interface FilesystemMaintenance {
  snapshotStorage(options?: StorageSnapshotOptions): Promise<StorageSnapshot>;
  collectGarbage(
    options?: GarbageCollectionOptions,
  ): Promise<GarbageCollectionResult>;
  verify(options?: VerificationOptions): Promise<VerificationResult>;
}
```

`snapshotStorage` implements section 15.1. `collectGarbage` implements section
14 and MUST reject on a read-only adapter. `verify` MUST accept a finite work
limit or cursor and report whether more work remains. Its default invocation
MUST be bounded. The exact exported result fields are owned by the filesystem
API, but they MUST include every counter or result required by this document.

Filesystem capabilities MUST report effective filesystem, storage, and branch
limits together with adapter BLOB and binding capabilities. An optional
observer supplied at open MAY receive the operation observations above.

## 16. Limits and resource safety

The binary formats impose these absolute limits:

- an object and manifest entry are at most `2^32 - 1` bytes;
- a manifest has at most `2^32 - 1` entries;
- a manifest file-size field is an unsigned 64-bit integer; and
- public APIs using JavaScript numeric offsets are limited to
  `Number.MAX_SAFE_INTEGER` bytes.

The storage configuration MUST expose exactly these core limits, in addition
to filesystem and branch configuration defined by the companion specs:

```ts
interface StorageLimits {
  readonly maxManifestEntries: number;
  readonly maxFileBytes: number;
  readonly maxWriteBytes: number;
  readonly maxPatchesPerFile: number;
  readonly maxQueryBatchSize: number;
  readonly maxGcBatchSize: number;
  readonly maxRetainedRevisions: number;
  readonly readLeaseMs: number;
  readonly stagingLeaseMs: number;
}
```

All values MUST be positive safe integers. The effective values and adapter
capabilities MUST be reported through filesystem capabilities. Directory
result limits belong to `FilesystemLimits`; active branch, changed-path,
conflict, and retention limits belong to branch configuration.

`maxWriteBytes` limits one buffered non-streaming input or staged insertion,
not the total of a bounded streamed file. `maxRetainedRevisions` bounds the
discretionary history window; mandatory main, branch-base, result, checkpoint,
and ancestor roots override it and MUST NOT be deleted to enforce the number.

For compact manifest version 1, let:

```text
blobEntryCapacity = floor((adapter.capabilities.maxBlobBytes - 32) / 36)
maxManifestEntries = min(configuredEntryCap, 2^32 - 1, blobEntryCapacity)
formatFileCapacity = maxManifestEntries * (fastCdcMinimum + 1)
maxFileBytes = min(configuredFileCap, Number.MAX_SAFE_INTEGER,
                   formatFileCapacity)
```

The products MUST use checked arithmetic. The defaults for
`configuredEntryCap` and `configuredFileCap` are their respective calculated
capacities, not `Number.MAX_SAFE_INTEGER`. The `minimum + 1` bound follows the
exact version 1 scan index and guarantees that every possible canonical
manifest for an accepted file fits in one adapter BLOB.

Opening MUST reject a default chunk maximum greater than
`adapter.capabilities.maxBlobBytes`, fewer than eight available bindings, or a
manifest capacity below one entry. Query builders MUST additionally divide
the binding capability by bindings per row. No statement or BLOB MAY exceed
the reported adapter capability.

The core MUST validate lengths with checked arithmetic before allocation,
hashing, binding, or mutation. Limit failures MUST leave state unchanged and
MUST use a resource-limit error distinct from corruption. Reads larger than the
materialization limit MUST require a bounded range or streaming API. No query
may create a placeholder list from an unbounded caller collection.

The following version 0.1 storage defaults are normative unless a filesystem
is explicitly created with another valid configuration:

- 4,096-byte copy-on-write pages;
- 65,536-byte page-overlay eligibility limit;
- 524,288-byte maximum patch insertion segment;
- 32,768/131,072/524,288-byte FastCDC parameters; and
- 64 MiB `maxWriteBytes`;
- 10,000 `maxPatchesPerFile`;
- 256 `maxQueryBatchSize`;
- 1,000 `maxGcBatchSize` and `maxRetainedRevisions`;
- 5-minute read leases and 15-minute staging leases; and
- batches no larger than both the configured batch limit and adapter binding
  capability.

Configuration affecting persisted interpretation MUST be stored in metadata
or the value it interprets. Process-local defaults MUST NOT reinterpret
existing content.

## 17. Required invariants

Every stable implementation MUST continuously preserve these invariants:

1. The metadata head references one existing revision and one existing root
   directory inode.
2. A successful main mutation advances the namespace and immutable revision
   history atomically. When it is a publication, branch terminal state and a
   requested replay result advance in that same transaction.
3. Object and manifest identifiers match verified canonical bytes.
4. A regular-file inode size equals the sum of its ordered manifest entries.
5. The current namespace has no dangling entries, directory cycles, reused
   inode identifiers, or incorrect hard-link counts.
6. Reconstructing any retained revision from a complete checkpoint and
   contiguous deltas yields its exact namespace, tokens, and content roots.
7. A branch read is the base revision plus its namespace overlay, page overlay,
   and ordered patches, in that order.
8. Only active branches own overlay or expectation rows; terminal branches own
   none, and used branch identifiers remain permanently reserved.
9. One publication operation ID binds to at most one branch, generation, and
   immutable result or lifetime-expired tombstone.
10. Replaying a completed or expired operation cannot change main, branch, or
    result state.
11. A conflict finalizes only its replay result when requested and changes
    neither main nor the active branch view or generation.
12. An absent entry slot retains enough version history to detect ABA changes,
    and same-node concurrent writers from one base token cannot both merge.
13. Independent entry slots may merge while implied parent timestamps remain
    monotonic and do not become whole-directory conflict tokens.
14. A failed or interrupted transaction leaves the previously committed main
    and branch views unchanged.
15. Garbage collection never deletes data reachable from current main,
    reconstructable revisions, active branches, operation results, leases, or
    administrative holds.
16. Metrics with the same name use the same documented byte boundary on every
    adapter.

An implementation MAY keep additional derived indexes and caches. It MUST be
possible to discard and rebuild them without changing these authoritative
values.

## 18. Conformance cases

The shared testkit MUST run the following storage cases against every supported
adapter. Tests MAY add cases; passing only this list is not evidence that the
complete filesystem specification is satisfied.

The conformance factory MUST declare and provide capability-driven hooks for a
second concurrent connection, read-only reopen, physical restart, migration
fixture installation, fault injection, controlled corruption, adapter limit
overrides, and the maintenance surface. Capability declarations select the
adapter-specific mechanism; they MUST NOT silently skip a normative case for a
required adapter.

### 18.1 Adapter and transactions

1. Round-trip empty, binary, and nonzero-`byteOffset` BLOB views exactly.
2. Round-trip `0`, `Number.MAX_SAFE_INTEGER - 1`, and
   `Number.MAX_SAFE_INTEGER` exactly, and reject unsafe integers without
   truncation.
3. Inject a failure after each statement of a multi-row write and observe full
   rollback.
4. Serialize two writers that start from the same expected head; exactly one
   succeeds or the second revalidates against the new head.
5. Enforce foreign keys and classify unique, busy, limit, and corruption
   errors.
6. Execute batched reads and writes with a deliberately small configured
   binding limit.
7. Verify that a read sees one consistent snapshot while another connection
   commits.

### 18.2 Schema and migration

1. Initialize an empty database with revision zero and a valid root inode.
2. Reopen without changing schema or filesystem identity.
3. Migrate a fixture from every released schema version and compare namespace,
   content, branches, revisions, and accounting.
4. Inject failure at every migration checkpoint and reopen the prior usable
   schema or resume the declared shadow migration.
5. Reject newer, too-old, wrong-application, and mismatched `user_version`
   databases without writes.
6. Initialize a zero-application-ID database only when it has no user objects;
   reject a zero-ID database containing any user schema object without writes.

### 18.3 Objects, chunking, and manifests

1. Match standard SHA-256 vectors, including empty bytes and `abc`.
2. Store identical chunks once and detect an injected same-key size or byte
   mismatch.
3. Detect missing, truncated, length-mismatched, and digest-mismatched objects
   without returning partial content.
4. Match checked-in `fastcdc-v1` golden boundary vectors for empty, sub-minimum,
   minimum, average, maximum, and multi-megabyte fixtures.
5. Produce identical boundaries on repeated runs and on every adapter.
6. Cover every input byte exactly with no chunk above the configured maximum.
7. Preserve most unchanged boundaries after a small front insertion as a
   regression measurement; correctness MUST NOT depend on a reuse percentage.
8. Match checked-in binary manifest golden vectors and their SHA-256 hashes.
9. Reject bad magic, unknown algorithms, trailing bytes, zero-sized entries,
   overflow, wrong total size, and wrong manifest digest.
10. Prove that local rechunking after insertion, deletion, truncation, and
    sparse page edits yields the exact full-scan canonical manifest.

### 18.4 Pages, patches, namespace, and revisions

1. Repeated overwrites of one byte in one page retain one 4 KiB page row and
   produce the latest bytes after reopen.
2. A write crossing a page boundary updates exactly two complete page rows in
   one transaction.
3. A final partial page stores its exact logical length.
4. Page writes followed by insertion, deletion, overwrite, and truncation obey
   the specified overlay order.
5. Patch segments reconstruct their exact insertion and reject gaps,
   duplicates, over-limit segments, and invalid ranges.
6. Materialization installs one canonical manifest and clears superseded
   overlays atomically.
7. Rename changes namespace bindings without rewriting unchanged content.
8. Hard links share an inode and manifest, update link counts, and count logical
   bytes once.
9. Inject failure into each stage of revision commit and observe the old head,
   complete branch overlay, and no partial revision after reopen.
10. Rebuild the head projection from retained immutable history and compare
    every inode and directory entry.
11. Verify regular-file aliases, directory, symlink, and root link counts.
12. Preserve exact UTF-8 byte ordering and pagination independently of SQLite
    text collation.
13. Detect entry-slot create-delete ABA, node ABA with identical final bytes,
    ancestor replacement, and recursive subtree changes.
14. Publish independent sibling entries while merging implied parent
    timestamps monotonically and preserving explicit metadata conflicts.

### 18.5 Recovery, collection, and metrics

1. Recreate the engine after a committed operation whose response was lost and
   return its original durable result without another revision.
2. Recreate after a rolled-back main update and successfully retry from the
   unchanged active branch.
3. Preserve an active branch whose base revision is older than the normal
   history window while reclaiming unrelated orphan content.
4. Preserve every complete retained snapshot, including content visible at the
   oldest retention boundary but changed by a later revision.
5. Interrupt and resume each mark and sweep batch without deleting a live
   object or double-counting reclaimed bytes.
6. Mutate a branch or main root during marking and verify that sweeping aborts
   or restarts.
7. Allocate new unreferenced content after the collection high-water mark and
   verify that the current run does not delete it.
8. Discard a branch, expire its retention, collect its exclusive manifests and
   objects, and leave main unchanged.
9. Detect reachable corruption during marking and perform no sweep.
10. Compare accounting counters with direct fixture byte sums, including
    deduplicated objects, hard links, shared branch content, pages, patches,
    manifests, and reclaimable rows.
11. Expire, renew, release, crash, and race read and staging leases against each
    mark and sweep boundary without deleting protected content.
12. Prune around a complete checkpoint while an old active branch retains its
    base, then reconstruct every retained revision and validate parent links.
13. Replay complete merged and conflict results, expire their payloads, reject
    branch mismatch, and prove lifetime operation and branch IDs are not reused.
14. Publish 50 independent writers into one parent chain, then publish 50
    same-node writers and observe exactly one merge and 49 conflicts.

## 19. Open implementation choices

The following choices do not change this storage contract and may be resolved
in implementation proposals:

- concrete table and index names after schema version 1;
- the Node.js SQLite binding;
- identifier representation for inodes and filesystems;
- thresholds for automatic branch materialization;
- verified-object cache size; and
- whether large migrations use shadow tables or shadow columns.

Any choice that changes canonical manifest bytes, chunk boundaries, root
reachability, transaction semantics, or metric definitions requires an update
to this specification and its conformance fixtures before release.
