# Ephemeral AI FS technical specification

| Field | Value |
| --- | --- |
| Status | Draft |
| Owner | Ephemeral AI Lab |
| Last updated | 2026-08-10 |

This specification refines the product boundary and acceptance criteria in
[`PRD.md`](./PRD.md) into implementable contracts. The detailed specifications
are split by responsibility so storage, public API, branch behavior,
replication, Node integration, and resource bounds can be reviewed
independently.

## Status and evidence

This repository contains a target specification, not an implementation. The
Agent Infra Book prototype demonstrates FastCDC chunking, compact manifests,
copy-on-write pages, private branches, publication, recovery, and garbage
collection on Durable Object SQLite. It is design evidence only: version 0.1
must implement the contracts here and pass the shared conformance suite on
both required database adapters.

## Specification map

| Contract | Owns |
| --- | --- |
| [Storage and data model](./docs/spec/storage-and-data-model.md) | Database contract, schema, objects, chunking, manifests, revisions, leases, recovery, collection, and accounting |
| [Filesystem API](./docs/spec/filesystem-api.md) | Paths, metadata, namespace operations, I/O, errors, lifecycle, adapters, capabilities, and conformance |
| [Branches and publication](./docs/spec/branches-and-publication.md) | Branch views, overlays, conflicts, atomic publication, replay, retention, and branch limits |
| [Replication](./docs/spec/replication.md) | Host-neutral negotiation, batches, cursors, staging, retry, and import/export |
| [Node virtual filesystem](./docs/spec/node-vfs.md) | Node handles, range I/O, bounded write sessions, flush, errors, and metrics |
| [Performance and resource limits](./docs/spec/performance-and-resource-limits.md) | Aggregate memory, backpressure, workload invariants, metrics, and release gates |

The non-normative
[`design rationale`](./docs/spec/design-rationale.md) records the evidence,
tradeoffs, and rejected alternatives behind these contracts.

When two detailed contracts meet, the public types and error behavior in the
filesystem API define the caller-facing boundary; the storage contract defines
durability; and the branch contract defines lifecycle and publication
semantics. Replication and Node virtual filesystem contracts adapt those
authorities without redefining them. The resource contract places limits on
every package. A contradiction is a specification defect and must be resolved
before implementation.

## Normative language

The words MUST, MUST NOT, SHOULD, SHOULD NOT, and MAY describe requirement
levels. A stable release must satisfy every MUST and MUST NOT requirement.

## Project boundary

`@ephemeralai/fs` contains portable filesystem behavior and depends only on a
documented database contract. Host adapters provide database bindings. A host
such as Ephemeral AI Computer may depend on Ephemeral AI FS, but the filesystem
core must not import host runtime, container, FUSE, or remote procedure call
types.

Ephemeral AI FS is therefore not a plugin to Cloudflare Computer. It is an
independent filesystem library. In Ephemeral AI Computer it is the default
production filesystem facade, namespace and content engine, schema, and
virtual filesystem provider. The Node virtual filesystem provider is a
separate package in this repository. Computer retains its workspace, network
transport, mount, FUSE, and process-execution boundaries as host-owned
compatibility bridges.
Computer may also retain DOFS behind an explicit comparison selector, but that
adapter and its database remain outside Ephemeral AI FS.

Durable Object SQLite is below that replacement boundary. The
`@ephemeralai/fs-sqlite-cloudflare` package adapts it to the portable database
contract; Ephemeral AI FS does not replace the database service.

## Architecture

```text
Ephemeral AI Computer                      another host
        |                                       |
workspace.fs: EphemeralFilesystem     asynchronous filesystem API
        |                                       |
        +---------------+-----------------------+
                        |
                Ephemeral AI FS
         default engine in Computer
                        |
       namespace + branches + publication
                        |
        revisions and content engine
                        |
   SHA-256 + FastCDC v1 + manifests + COW pages
                        |
             portable database contract
                 /                 \
     Cloudflare adapter          Node adapter
              |                       |
  Durable Object SQLite          local SQLite
```

Host integration enters through high-level packages rather than storage
internals:

```text
Computer authenticated RPC carrier -> @ephemeralai/fs-replication -> core
Computer FUSE bridge              -> @ephemeralai/fs-node-vfs     -> core
```

Replication owns its protocol, batching, cursors, staging, retry, and
validation. Node VFS owns range operations and bounded write sessions.
Computer supplies transport, FUSE request handling, engine selection, and
process-wide limits.

Main is one durable, linear revision history. A branch freezes a base revision
and records only its namespace changes, content objects, pages, and patches.
Publication validates the branch write set against durable entry and inode
tokens, then either commits one new main revision atomically or returns a
deterministic conflict without changing main.

In Computer, `workspace.fs` MUST use the `EphemeralFilesystem` contract rather
than retain `WorkspaceFilesystem` as a second semantic API. Computer MAY add
transport-safe facades and convenience helpers, but they MUST delegate to this
contract and MUST NOT redefine filesystem behavior or storage.

A Computer-owned DOFS comparison adapter MAY implement the common contract.
It MUST report unsupported branch capabilities rather than emulate them, use a
separate database, and never become an automatic fallback. Ephemeral AI FS
packages MUST NOT import or configure that adapter.

## Repository and package layout

```text
packages/
  fs/                    @ephemeralai/fs
  sqlite-node/           @ephemeralai/fs-sqlite-node
  sqlite-cloudflare/     @ephemeralai/fs-sqlite-cloudflare
  replication/           @ephemeralai/fs-replication
  node-vfs/              @ephemeralai/fs-node-vfs
  testkit/               @ephemeralai/fs-testkit
tests/
  conformance/
  node-integration/
  durable-object-integration/
  replication/
  node-vfs/
benchmarks/
  storage/
  branching/
  multi-agent/
  small-edits/
  sequential-io/
  fuse-materialization/
examples/
  node-workspace/
  durable-object-workspace/
  multi-agent-branches/
docs/spec/
```

Implementation packages do not exist yet. They are created in this order so
the shared testkit, rather than either host, remains the behavioral authority.

## Cross-cutting decisions

### Persistence and content

- The database has a versioned application identity and transactional
  migrations.
- File content uses verified SHA-256 objects and canonical compact manifests.
- FastCDC version 1 has fixed, test-vector-backed boundary behavior. Chunking
  parameters are persisted or encoded with the data they interpret.
- Copy-on-write page size is persisted independently of FastCDC. Version 0.1
  accepts 4, 8, or 16 KiB and defaults new filesystems to 8 KiB. Structural
  edits use ordered patches and deterministic materialization.
- Namespace revisions remain reconstructable from complete checkpoints and
  contiguous deltas while any retained root depends on them.

### Filesystem semantics

- Paths are absolute POSIX-style UTF-8 paths rooted at `/`.
- Version 0.1 supports regular files, directories, symbolic links, hard links,
  metadata, range I/O, range replacement, truncation, and snapshot streams.
- Each public mutation and main publication is atomic. Close is idempotent and
  waits for already-admitted non-stream work.
- Snapshot streams pin their selected data with durable leases. On a read-only
  adapter, `readStream` returns `EROFS`; bounded `readFile` and `readRange`
  remain available.
- Reads never create content, manifest, namespace, or branch rows merely to
  make existing bytes readable. Streams fetch and verify content lazily under
  backpressure.
- Search, watch, persistent file handles, ownership, ACLs, and caller-assigned
  timestamps are deferred from version 0.1.

### Branching and publication

- Branch identifiers are never reused within a filesystem. A branch has a
  durable generation, immutable base revision, and explicit active, merged, or
  discarded state.
- Independent paths may publish in either order. A stale change to the same
  entry, inode, source, destination, ancestor, or recursive subtree conflicts.
  Version 0.1 does not perform semantic or line-based merges.
- Publication is a single final database transaction. Optional operation IDs
  bind durably to one branch generation and make success or conflict results
  replayable after restart.
- Successful empty publication still creates one auditable revision. Discard
  never changes main.
- Terminal branch metadata and publication results default to 30-day
  retention, with lifetime tombstones preventing identifier reuse.

### Recovery, collection, and limits

- Active branches, reconstructable revisions, retained publication results,
  read streams, staging work, and administrative holds are garbage-collection
  roots.
- Collection is generation-safe, resumable, and bounded. It never sweeps from
  an incomplete or stale mark.
- Filesystem, storage, branch, format, and adapter limits are resolved at open
  and exposed through immutable capabilities. Persisted limits must agree
  across writers.
- Corruption, schema mismatch, resource exhaustion, read-only access, ordinary
  filesystem errors, and branch lifecycle errors have stable, non-overlapping
  error contracts.

### Runtime efficiency

- SQLite remains the authoritative store for metadata, content objects,
  manifests, overlays, revisions, leases, replication cursors, and maintenance
  state in version 0.1.
- Every implementation-owned cache, prefetch buffer, rechunking buffer,
  pending write, and prepared result participates in aggregate byte accounting.
- Contiguous Node writes may be coalesced only within per-session and global
  budgets. Discontinuity, flush, close, or pressure forces bounded staging.
- A small eligible overwrite performs work proportional to affected pages and
  intersecting content objects, never to the logical file size.
- A large sequential read performs no content copy-up or branch
  materialization and does not retain already emitted bytes.
- Replication and Node virtual filesystem packages use bounded batches and
  backpressure. They do not move their algorithms into Computer.

## Key terminology

| Term | Meaning |
| --- | --- |
| Main | The current durable workspace and head revision |
| Revision | An immutable, reconstructable namespace state in main history |
| Branch | A durable private overlay rooted at one base revision |
| Entry token | Durable version of a directory-name binding, including absence |
| Inode token | Durable version of a file or metadata identity |
| Object | Immutable content bytes addressed by SHA-256 |
| Manifest | Canonical ordered object references and lengths for one file |
| Publication | Atomic validation and application of a branch to main |
| Operation result | Durable replay record for one publication attempt |
| Lease | Temporary root protecting stream or staging data from collection |

## Conformance and release gate

`@ephemeralai/fs-testkit` is the executable form of these contracts. The same
suite must run against Node.js SQLite and Durable Object SQLite and must cover
transactions, migration, namespace semantics, binary formats, restart,
fault injection, corruption, concurrent publication, replay, lease races,
bounded collection, accounting, aggregate memory pressure, replication retry,
Node handle semantics, and the small-edit, large-read, and materialization
workloads.

Version 0.1 is not stable until every normative MUST and MUST NOT has a passing
test on both adapters. Adapter-specific tests may exercise setup and physical
restart mechanisms, but may not weaken portable outcomes.

## Implementation sequence

1. Create the monorepo packages, shared types, fixtures, and adapter test
   harness.
2. Implement the transactional database contract, schema, objects, FastCDC,
   manifests, pages, and revision history.
3. Implement namespace and I/O semantics plus both SQLite adapters.
4. Implement durable branches, conflict tokens, publication, replay, and
   retention.
5. Implement verification, accounting, and bounded garbage collection.
6. Implement host-neutral replication and the Node virtual filesystem package.
7. Pass the aggregate-resource and end-to-end performance release gates.
8. Make Ephemeral AI FS Computer's default runtime path and wire its
   Computer-owned compatibility bridges. Keep any optional DOFS comparison
   adapter outside Ephemeral AI FS.

## Open implementation choices

The contracts intentionally leave implementation latitude for the first
Node.js SQLite binding, identifier representation, concrete schema names, and
large-migration mechanics. Any choice that changes a public result, binary
format, chunk boundary, transaction guarantee, reachability rule, resource
boundary, or metric definition requires a specification and
conformance-fixture update.

## Compatibility rule

The specification describes target behavior, not the current implementation.
Every behavior becomes implemented only when its conformance test passes
against each supported database adapter.
