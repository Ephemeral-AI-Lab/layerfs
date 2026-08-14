# Correctness test plan

| Field     | Value                                                                |
| --------- | -------------------------------------------------------------------- |
| Status    | Normative version 0.1 release gate                                   |
| Scope     | Core, SQLite drivers, branches, replication, Node VFS, Computer path |
| Authority | `@ephemeralai/fs-testkit` executable suites                          |

## 1. Purpose

Ephemeral AI FS is eligible for Computer integration only when every mandatory case in
this document passes. Performance never compensates for a correctness, durability,
integrity, isolation, or resource-accounting failure.

The same portable suite MUST run against:

1. a file-backed raw Node.js SQLite driver;
2. Cloudflare Durable Object SQLite;
3. a second concurrent connection when supported;
4. read-only reopen where supported; and
5. physical driver or runtime restart.

Driver-specific setup may differ. Observable filesystem results may not.

### 1.1 Durable Object execution tiers

Durable Object evidence is split into two non-interchangeable tiers:

1. **M6 local conformance:** the complete portable driver suite runs in the pinned
   `@cloudflare/vitest-pool-workers` workerd/Miniflare environment with a real local
   SQLite-backed Durable Object binding. It MUST NOT route SQL through the Node adapter
   or another mirror. This tier is mandatory, fully local, and credential-free.
2. **M9 hosted preview:** the already-accepted M6 Worker bundle runs in a dedicated
   non-production Cloudflare account environment. This tier reruns the 60-second smoke
   profile and the Durable Object release benchmarks against hosted storage. It requires
   explicit user authorization and authenticated Wrangler access.

Passing the local tier does not satisfy the hosted M9 gate. Conversely, a hosted smoke
run does not replace the complete local portable suite. Normal M6 commands MUST NOT read
Cloudflare credentials or create remote resources.

## 2. Fast smoke suite

The first integration gate is a finite high-volume smoke suite, not a long-running soak.
Each Node SQLite, faithful-local Durable Object SQLite, and real-FUSE target MUST finish
its reference profile within 60 seconds. M9 MUST rerun the identical Durable Object
profile in the hosted preview; it may not substitute a smaller remote workload.

- one 16 MiB pseudorandom write, close, reopen, read, and digest check;
- 5,000 same-page, clustered, and scattered one-byte COW edits;
- 2,000 mixed namespace and link operations;
- 16 readers and 16 writers performing 64 bounded operations each;
- three forced close and reopen or runtime restart cycles;
- one interrupted and resumed bounded collection; and
- final namespace, digest, lease, reservation, and usage-counter verification.

The suite MUST NOT sleep to satisfy a duration. A failure prints the completed operation
count, seed, phase, and slowest measured operations. Extended scale tests may run
separately, but do not block initial Computer integration.

The default mandatory correctness selection SHOULD complete within 10 minutes per
target. Individual cases use finite iteration counts and bounded timeouts; they do not
contain soak sleeps. An optional `load-10m` selection may repeatedly run the same real
mutations, restarts, collection, and verification until a hard 10-minute deadline,
reporting completed operations rather than claiming reliability from elapsed time alone.

## 3. Required test organization

```text
packages/testkit/src/
  architecture/
  sqlite-driver/
  cas/
  cdc/
  cow/
  patches/
  manifests/
  namespace/
  revisions/
  branches/
  streams/
  maintenance/
  replication/
  node-vfs/
tests/
  node-integration/
  durable-object-integration/
  computer-integration/
```

Every test result MUST record the implementation commit, schema and format versions,
driver, capabilities, limits, seed, fixture digest, and fault point. Randomized tests
MUST print a replayable seed on failure.

## 4. Architecture boundaries

### CT-ARCH-1: Dependency direction

An automated dependency graph MUST prove:

- `cdc` imports no database, filesystem, COW, branch, or host module;
- `cas` performs identity and verification but imports no SQL repository;
- `cow` imports neither FastCDC execution nor SQLite;
- `manifests` import CAS identities but no branch or host package;
- only `operations` compose CAS, CDC, COW, patches, manifests, namespace, and storage
  ports;
- domain and content modules never import SQLite repositories; and
- adapters and integrations introduce no dependency cycle.

### CT-ARCH-2: Export boundary

Packed-package tests MUST prove that only these surfaces are importable:

```text
@ephemeralai/fs
@ephemeralai/fs/sqlite-driver
@ephemeralai/fs/integrations/replication
@ephemeralai/fs/integrations/node-vfs
```

Deep imports of CAS, CDC, COW, manifests, schema, repositories, transactions, and
resource accounting MUST fail. API extraction MUST detect any accidental new export.

### CT-ARCH-3: Transaction-only SQL

The SQLite driver MUST expose no connection-level statement execution. A transaction
value retained past its callback MUST fail before issuing SQL. Instrumentation MUST
prove every repository statement belongs to the one unit-of-work transaction admitted by
its application operation.

## 5. CAS and CDC

### CT-CAS-1: Identity and deduplication

- Match SHA-256 vectors for empty bytes, `abc`, binary fixtures, and segmented input.
- Store identical content once on both drivers.
- Verify an existing row before treating a digest collision as deduplication.
- Reject missing, truncated, wrong-size, and wrong-digest objects before returning any
  affected bytes.
- Prove cache eviction changes performance only.

### CT-CDC-1: Deterministic FastCDC

- Match checked-in empty, sub-minimum, minimum, average, maximum, and large golden
  fixtures.
- Produce identical boundaries across input buffer sizes and both drivers.
- Cover every byte exactly and never exceed the configured maximum chunk.
- Resume streaming scanner state across bounded staging flushes.
- Use getter-backed configurations to prove validation, capacity selection, and scanning
  consume one immutable scalar snapshot.
- Exercise a valid minimum-to-maximum ratio with many short chunks and require copied
  and scanned work linear in prepared input, including a quick bounded fallback.
- Verify that fixed-capacity pure diagnostic reconnection produces the byte-identical
  full-scan result and that exceeding a cap takes the bounded streamed fallback while
  reporting both phases.

## 6. Segmented Merkle manifests

### CT-MANIFEST-1: Canonical encoding

Golden vectors MUST cover the root envelope, empty leaf, full and partial leaf, internal
node, multiple levels, and content-defined grouping boundaries. The encoded bytes and
SHA-256 hashes MUST match on every runtime.

Encoder ownership cases MUST use getter-backed scalars and mutating/subclassed byte
views for root hashes, entries, and children. Encoding followed by decoding MUST remain
self-consistent and byte ownership MUST not call caller-overridable typed-array methods.

### CT-MANIFEST-2: Integrity

Corrupt every header field, count, span, record, child hash, object hash, reserved byte,
and trailing-length condition. Delete, duplicate, or reorder children. The affected
range MUST fail before returning unverified bytes.

### CT-MANIFEST-3: Bounded lookup

Start, middle, end, EOF, and random ranges in 100 MiB and 1 GiB logical files MUST
traverse at most `maxManifestDepth + 1` manifest values and scan at most one 256-entry
leaf. Run cold, after reopen, after cache eviction, and with a corrupted disposable
derived index.

### CT-MANIFEST-4: Diagnostic and durable local rebuild

Within its documented entry, node, byte, and content-size caps, the pure diagnostic
algorithm MUST make insertion, deletion, truncation, and equal-size overwrite produce
exactly the full rebuild root. Above a diagnostic cap it MUST take the bounded streamed
full-scan fallback and report the attempted-local and fallback work. This helper is not
evidence that arbitrary large-file edits are locally incremental.

The persisted implementation MUST authenticate every located boundary and ancestor path
against the selected root before reuse. Derived offset/group indexes are
non-authoritative. Tests MUST corrupt and stale those indexes, cover empty and
single-leaf height growth, compare every result with a full scan, and edit a manifest
with more than 16,384 entries under explicit per-level and aggregate record-read,
emitted-node, retained-segment, byte, and transaction caps. After authenticated
reconnection, unchanged CAS objects and unchanged manifest subtrees MUST retain their
identifiers.

## 7. COW pages and structural patches

### CT-COW-1: All persisted page sizes

Run every case with 4, 8, and 16 KiB pages. A new filesystem defaults to 8 KiB.
Reopening with a conflicting requested size MUST fail without writes.

### CT-COW-2: Page behavior

- One-byte writes retain one current page when no stream pins the predecessor.
- A boundary-crossing write creates exactly two complete page versions in one
  transaction.
- A final partial page stores only its exact logical bytes.
- Rewriting the current page reads that overlay rather than the base object.
- One thousand same-page writes retain one current page plus only versions explicitly
  pinned by active leases.

### CT-COW-3: Immutable snapshots

Open a branch stream, then overwrite its page, add patches, materialize, publish or
discard, collect garbage, restart, and renew or expire leases. The stream returns
exactly its initially selected bytes until consumption, cancellation, error, or handle
close. A new stream observes the new view.

### CT-PATCH-1: Ordered structural edits

Cover insertion, deletion, replacement, truncation, zero-fill growth, segment limits,
gaps, duplicate sequences, invalid offsets, and mixtures with pages. Acknowledged
operation order MUST reconstruct the exact branch value.

## 8. Filesystem and namespace semantics

The suite MUST cover:

- absolute POSIX path normalization and UTF-8 byte limits;
- invalid, empty, relative, dot, dot-dot, NUL, and over-limit paths;
- create, read, write, range write, range replacement, and truncate;
- directories, deterministic UTF-8 listing, recursive creation and removal;
- `stat`, `lstat`, mode changes, timestamps, and link counts;
- symbolic-link traversal, dangling links, loops, and final-link rules;
- hard links, aliases, inode identity, unlink, and last-link deletion;
- atomic file and directory rename, including replacement and ancestry checks;
- exact stable error codes and precedence;
- handle, stream, branch, and filesystem close behavior; and
- no durable mutation from ordinary reads except a required bounded lease.

Every mutating operation MUST be tested with a failure injected after each SQL
statement. Reopen MUST show the complete old state or complete new state, never a
partial namespace or incorrect link count.

## 9. Revisions, branches, and publication

Mandatory cases include:

- revision-zero bootstrap and exact reconstruction from checkpoints and deltas;
- independent branches from the same immutable base;
- independent sibling publications in both orders;
- 50 independent writers producing one valid parent chain;
- 50 same-inode writers producing one merge and 49 explicit conflicts;
- create/delete and node ABA conflicts even when final bytes match;
- hard-link alias conflicts and deterministic parent timestamp merging;
- exact sorted changed paths and deterministic conflict records;
- publication with and without an operation identifier;
- lost response, restart, exact replay, result expiry, and branch mismatch;
- publication reservation expiry and generation change;
- successful empty publication producing one auditable revision;
- discard, terminal retention, identifier non-reuse, and bounded cleanup; and
- independent branch handles and idempotent close.

No conflict or interrupted publication may change main or clear the active branch
overlay.

## 10. Staging, recovery, and leases

### CT-STAGE-1: Closure certificate

Stage and finalize a manifest containing more than 100,000 CAS entries. The final
visible transaction MUST validate constant-row certificate state rather than rescan
membership. Crash after every batch, alter every certificate field, delete membership,
race expiry, and race garbage collection.

### CT-LEASE-1: Lifecycle

Acquire, renew, release, expire, cancel, close, and restart read, write, publication,
replication, and export leases. A protected value is never collected. An expired or
released lease is never revived.

## 11. Accounting, quotas, and garbage collection

### CT-QUOTA-1: Atomic durable usage

Race two connections at exact CAS, manifest, COW, staging, result, metadata,
database-page, and journal boundaries. After commit, rollback, deduplication,
replacement, expiry, and collection, `efs_usage` MUST equal bounded direct
recalculation. Normal work may not consume the maintenance reserve.

### CT-GC-1: Bounded collection

- Interrupt and resume every mark and sweep batch.
- Add and remove roots while marking.
- Fill and compact the root-change journal.
- Preserve main, retained revisions, branch bases, operation results, leases, staging
  certificates, and explicit holds.
- Detect reachable corruption and perform no unsafe sweep.
- Prove eventual progress under root changes below reconciliation capacity.

### CT-SCALE-1: Bounded cursor scale

Beginning at M5, enumerate, account, verify, and collect 100,000 CAS, namespace,
manifest-node, and mark rows under tiny query and memory limits. Managed-memory
high-water MUST not grow with total row count. Bounded storage accounting MUST not hold
a database-wide read transaction or pin WAL for the complete scan.

Beginning at M8, the unchanged accepted 100,000-row fixture MUST additionally replicate
Node-to-Node and Node-to-Durable-Object through the bounded host-neutral protocol before
collection. This later replication phase does not reduce, replace, or defer the M5/M6
enumeration, accounting, verification, or collection gate.

An extended non-gating job SHOULD repeat this with millions of rows when CI capacity
permits. It is not part of the 60-second integration smoke suite.

## 12. Node VFS and real FUSE

The Node suite MUST cover pinned read sessions, `readIntoSync`, direct range reads,
irregular sequential callbacks, discontinuous writes, overlapping writes, truncation,
read-after-write, and exact close behavior.

Three same-inode sessions MUST run in every commit and close order. Monotonic provider
admission must prevent stale-base lost updates. Rename and unlink must coordinate open
sessions by inode identity.

Hidden `stagePrefixSync` MUST release resident memory without satisfying `fsync`. Only
`commitVisibleSync` may acknowledge flush or fsync. Successful commit, close, engine
recreation, unmount, and remount MUST preserve the exact fixture digest.

At least one privileged Linux suite MUST use the real FUSE kernel path. The development
shim cannot satisfy the release gate alone.

## 13. Replication

Test authenticated empty-replica provisioning, exact genesis adoption, handshake
compatibility, segmented manifest negotiation, bounded graph frontiers, missing-object
negotiation, deduplication, operation and cursor replay, dropped responses in every
phase, staging certificates, one global-flow role matrix, active-branch transfer,
authority-to-replica main catch-up, generation-guarded publication, retry exhaustion,
and abandoned-session cleanup.

Envelope decoding MUST be incremental and must not copy a complete envelope. All
sessions share the filesystem admission controller. The replication bridge must expose
no SQL, schema, repository, raw CAS insertion, or raw COW mutation.

The suite MUST resume the exact recognized durable unbound bootstrap state after every
accepted batch and reject unrelated nonempty state, a wrong engine, wrong workspace,
unauthorized scope, unsupported logical filesystem schema, unsupported storage user
version, and unsupported protocol before any further mutation. It MUST resume by stable
operation ID and opaque resume key after physical process restart, without resetting the
durable attempt or elapsed-time budget.

## 14. Computer integration

The release candidate MUST pass this real path:

```text
workspace.fs
  -> authenticated replication
  -> computerd
  -> real FUSE
  -> shell, Git, and filesystem tools
  -> pull
  -> branch publication
  -> restart and reconnect
  -> garbage collection and verification
```

Run host reads and writes, push, pull, shell, Git, mounts, read-only enforcement,
`find`, `grep`, `ls`, streaming reads, branch conflict reporting, Durable Object
restart, container restart, and reconnect to the same branch. Omitted engine
configuration selects Ephemeral AI FS. DOFS runs only when selected explicitly and uses
an isolated database.

This path MUST use the pinned Computer fork's actual Cap'n Web text carrier and a real
kernel FUSE mount. Authenticate and bind the workspace, filesystem, peer, global flow,
host profile, and branch before the first exchange. Bound raw and decompressed frames
before JSON/base64 decoding, then independently bound the decoded protocol envelope.
Stable replication errors MUST survive the carrier without relying on JavaScript
thrown-error properties.

Provision a truly empty persistent Node SQLite replica, transfer main, transfer one
active private branch, and mount exactly that branch. The mount MUST see base-main
content; branch-private mutations MUST remain invisible to main and siblings, and
sibling-private mutations MUST remain invisible to it. Replica main writes, missing
branches, and terminal branches MUST never become writable main fallback. Return the
exact branch generation and digest, publish with both expectations and an operation
identifier, lose the response, and prove replay creates neither a second activation nor
a second revision. Return the authority's terminal state and result to the replica, then
prove reconnect rejects the branch without main fallback. Run incoming activation with
pinned readers and dirty writers and prove cache invalidation, snapshot behavior, and no
lost update. Delete the local database, provision a replacement from empty, retransmit
main and the active branch, and verify exact identity and digest without another
authority activation.

## 15. Release exit criteria

All of the following are required:

- every mandatory portable case passes on Node and faithful-local Durable Object SQLite;
- the 60-second smoke profile passes on Node, faithful-local Durable Object SQLite,
  hosted-preview Durable Object SQLite, and real FUSE;
- zero digest mismatch, partial commit, lost update, incorrect replay, unsafe
  collection, leaked lease, leaked reservation, or quota-accounting mismatch;
- every normative `MUST` and `MUST NOT` maps to a test identifier;
- all architecture and packed-export checks pass;
- every failure is reproducible from its recorded seed and fault point; and
- the benchmark plan passes after correctness succeeds.
