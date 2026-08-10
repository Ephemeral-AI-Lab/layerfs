# Ephemeral AI FS technical specification

| Field | Value |
| --- | --- |
| Status | Draft |
| Owner | Ephemeral AI Lab |
| Last updated | 2026-08-10 |

This specification refines the product boundary and acceptance criteria in
[`PRD.md`](./PRD.md) into implementable contracts. The detailed specifications
are split by responsibility so storage, public API, and branch behavior can be
reviewed independently.

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

When two detailed contracts meet, the public types and error behavior in the
filesystem API define the caller-facing boundary; the storage contract defines
durability; and the branch contract defines lifecycle and publication
semantics. A contradiction is a specification defect and must be resolved
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
independent library with a Cloudflare SQLite adapter. Ephemeral AI Computer is
a host and integration consumer that may select this filesystem engine behind
its own workspace, sync, mount, and process-execution boundaries.

## Architecture

```text
Ephemeral AI Computer or another host
                    |
                    v
       asynchronous filesystem API
                    |
      +-------------+-------------+
      |             |             |
  namespace     branch views   maintenance
  and metadata  and publish    and metrics
      |             |             |
      +-------------+-------------+
                    |
       revisions and content engine
                    |
      +-------------+-------------+
      |             |             |
 SHA-256 objects FastCDC v1   4 KiB COW pages
 and manifests   chunking     and patches
      +-------------+-------------+
                    |
       portable SQLite contract
            /                 \
  Node.js SQLite       Durable Object SQLite
```

Main is one durable, linear revision history. A branch freezes a base revision
and records only its namespace changes, content objects, pages, and patches.
Publication validates the branch write set against durable entry and inode
tokens, then either commits one new main revision atomically or returns a
deterministic conflict without changing main.

## Repository and package layout

```text
packages/
  fs/                    @ephemeralai/fs
  sqlite-node/           @ephemeralai/fs-sqlite-node
  sqlite-cloudflare/     @ephemeralai/fs-sqlite-cloudflare
  testkit/               @ephemeralai/fs-testkit
tests/
  conformance/
  node-integration/
  durable-object-integration/
benchmarks/
  storage/
  branching/
  multi-agent/
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
- Small private overwrites use complete 4 KiB copy-on-write pages. Structural
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
bounded collection, and accounting.

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
6. Validate benchmarks, then integrate the adapter into Ephemeral AI Computer.

## Open implementation choices

The contracts intentionally leave implementation latitude for the first
Node.js SQLite binding, identifier representation, concrete schema names,
materialization thresholds, verified-object cache size, and large-migration
mechanics. Any choice that changes a public result, binary format, chunk
boundary, transaction guarantee, reachability rule, or metric definition
requires a specification and conformance-fixture update.

## Compatibility rule

The specification describes target behavior, not the current implementation.
Every behavior becomes implemented only when its conformance test passes
against each supported database adapter.
