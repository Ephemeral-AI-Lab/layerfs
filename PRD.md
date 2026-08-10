# EphemeralAI FS product requirements

| Field | Value |
| --- | --- |
| Status | Draft |
| Owner | Ephemeral AI Lab |
| Last updated | 2026-08-10 |

## Summary

EphemeralAI FS is a branchable SQLite filesystem for multi-agent workspaces.
It stores file content in a content-addressed store, uses content-defined
chunking to preserve reuse after insertions and deletions, and records small
private edits as copy-on-write pages. An agent can work in a private branch,
then publish its changes to the durable main workspace in one transaction.
Publication merges changes to independent files and returns explicit conflicts
when another writer has changed the same path.

The project is an independent library. In EphemeralAI Computer, it becomes the
default production filesystem implementation, including the filesystem
facade, namespace and content engine, schema, and virtual filesystem provider.
Computer may retain `@cloudflare/dofs` as an explicitly selected, isolated
comparison engine. EphemeralAI FS does not replace Durable Object SQLite,
which remains the authoritative database behind the Cloudflare adapter. The
core filesystem must also run in a regular Node.js process with a local SQLite
database.

## Current evidence

The starting prototype lives in the Agent Infra Book storage benchmarks. It
implements content-addressed storage, FastCDC chunking, compact manifests,
4 KiB copy-on-write pages, private branches, conflict-aware publication,
recovery, and garbage collection in Durable Object SQLite.

The prototype is evidence for the design, not the first release. It has a
benchmark-shaped API, uses `DurableObjectStorage` directly, and implements a
smaller namespace than a complete filesystem. Product work must extract the
algorithms behind stable interfaces, add filesystem conformance, and support
both Node.js and Cloudflare database adapters.

## Problem

Agent workspaces often combine one durable source of truth with many temporary
execution environments. A conventional filesystem copy gives each agent
isolation, but it duplicates unchanged data and makes publication a separate
merge problem. A single shared filesystem avoids copies, but concurrent agents
can overwrite one another without a clear conflict boundary.

Fixed-size content chunks also behave poorly for structural edits. Inserting
bytes near the start of a file shifts later fixed boundaries and can replace
content that did not logically change. Repeated small edits can move or retain
far more data than the edit itself.

EphemeralAI FS needs to provide cheap private branches while keeping accepted
workspace state durable, transactional, and easy to inspect.

## Product boundary

EphemeralAI FS owns:

- filesystem namespace and metadata;
- content-addressed objects and manifests;
- content-defined chunking;
- private copy-on-write branch state;
- revisions, publication, conflict detection, recovery, and garbage collection;
- a portable transactional database contract;
- adapters for Node.js SQLite and Durable Object SQLite.

EphemeralAI FS does not own:

- containers, process execution, or Linux isolation;
- FUSE mounts;
- network transport or remote procedure calls;
- agent scheduling or model APIs;
- user authentication and authorization;
- semantic source-code merges.

Those concerns belong to hosts such as EphemeralAI Computer.

EphemeralAI Computer owns the migration and compatibility bridges for its
current `WorkspaceFilesystem` and `SQLiteWorkspaceProvider` consumers. Its
default filesystem path must not depend on `@cloudflare/dofs`.
`workspace.fs` uses the EphemeralAI FS public contract. A Computer-owned DOFS
comparison adapter may implement the common subset and report branch-only
capabilities as unsupported. Computer-specific facades may transport or extend
the contract but must not define different filesystem semantics.

EphemeralAI FS does not import, wrap, or configure DOFS. Computer owns the
engine selector, keeps the engines in separate databases, and prevents
automatic fallback or engine changes during a workspace lifetime.

## Target users

The first users are infrastructure engineers building:

- parallel coding-agent workspaces;
- durable sandboxes with temporary execution environments;
- checkpointed build or data-processing workspaces;
- systems that need explicit publication instead of last-writer-wins updates;
- local tools that need cheap filesystem branches without a Git repository.

## Goals

1. Provide normal filesystem operations over a transactional SQLite store.
2. Make a private branch proportional to changed content rather than total
   workspace size.
3. Preserve content reuse after local insertions and deletions.
4. Publish a branch atomically and return deterministic path conflicts.
5. Recover safely after process restarts or interrupted publication.
6. Reclaim unreachable objects without deleting active branch data.
7. Run the same conformance suite against Node.js and Cloudflare adapters.
8. Expose a small host integration surface without Cloudflare Computer types.

## Non-goals for version 0.1

- Distributed transactions across several databases.
- Real-time collaboration on the same file.
- Line, syntax-tree, or conflict-marker merges.
- A networked filesystem server.
- Windows drive-letter or case-insensitive path semantics.
- Replacing Git history or source-control workflows.
- Transparent encryption, compression, or tiered object storage.

## Design principles

### The main workspace is durable

Branches, mounts, and execution environments may be temporary. A successful
publication creates a durable revision. Discarding a branch must never change
the main workspace.

### Conflicts are data

Publication must return a stable result that callers can record, retry, and
show to users. The library must not resolve same-path conflicts by silently
choosing the last writer.

### Content identity is separate from namespace identity

Paths and metadata may change while immutable content objects remain reusable.
Renames should not rewrite file content.

### Adapters stay small

The core must depend on a documented transaction and query contract, not on a
specific SQLite library or cloud runtime.

### Measured behavior beats theoretical savings

Benchmarks must report logical bytes, retained payload, database size, bytes
processed, and elapsed time. Storage claims must state which boundary was
measured.

## Target architecture

```text
EphemeralAI Computer workspace.fs or another host
    |
EphemeralAI FS API
    |  default production engine in Computer
    +-- namespace and metadata
    +-- branch views and publication
    +-- content-addressed objects
    +-- FastCDC manifests
    +-- copy-on-write pages
    +-- recovery and garbage collection
    |
Transactional database contract
    |
    +-- Node.js SQLite adapter -> local SQLite
    +-- Durable Object SQLite adapter -> Durable Object SQLite
```

The first package layout is:

```text
packages/
    fs/                  @ephemeralai/fs
    sqlite-node/         @ephemeralai/fs-sqlite-node
    sqlite-cloudflare/   @ephemeralai/fs-sqlite-cloudflare
    testkit/             shared conformance tests
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
docs/
```

## Functional requirements

### FS-1: Portable database contract

The core must use a database interface that supports parameterized queries,
transactions, binary values, schema migrations, and deterministic rollback.
The interface must not expose `DurableObjectStorage` or a Node.js database
class.

### FS-2: Filesystem namespace

Version 0.1 must support absolute POSIX-style paths, regular files,
directories, symbolic links, hard links, and standard metadata needed by
Cloudflare Computer. Required operations include reading, writing, range
updates, truncation, directory listing, `stat`, `mkdir`, rename, linking,
unlinking, and recursive removal.

Search and watch helpers may be built above these primitives, but the
conformance suite must define their behavior before a stable release.

### FS-3: Content-addressed storage

Immutable content objects must use SHA-256 identity. Repeated content must be
stored once within a database. Manifests must preserve ordered object hashes
and lengths without one database row per chunk in the steady state.

### FS-4: Content-defined chunking

The default chunker must use deterministic FastCDC boundaries with documented
minimum, target, and maximum sizes. A local structural edit should rebuild a
bounded dirty region and reconnect to unchanged manifest boundaries when
possible. The implementation may fall back to a full scan when no safe
reconnection exists.

### FS-5: Copy-on-write branches

A branch must reference an immutable base revision and store only private
namespace changes, objects, manifests, and 4 KiB dirty pages. Repeated writes
to the same page must replace branch-local state instead of appending another
full copy.

### FS-6: Branch lifecycle

Callers must be able to create, inspect, publish, and discard branches. Branch
identifiers are unique within a filesystem. Terminal branch states must remain
queryable until retention policy permits collection.

### FS-7: Transactional publication

Publication must run in one database transaction. It must compare each changed
path with the branch base, reuse existing content objects, write the new
revision, record the result, and move main references only after all checks
pass.

Changes to independent paths may merge. A stale change to the same path must
return a conflict. Rename is conflict-checked as a source deletion and target
creation.

### FS-8: Idempotency and recovery

Publication may accept an operation identifier. Retrying an operation
identifier must return the recorded result without creating another revision.
An operation identifier cannot be reused for a different branch. Failed
transactions must leave the main workspace and branch state unchanged.

### FS-9: Garbage collection

Garbage collection must trace main revisions, retained history, active
branches, manifests, and content objects before deleting data. It must report
the number of records and payload bytes reclaimed. Collection may run in
bounded batches.

### FS-10: Schema evolution

The database schema must carry a version. Migrations must be transactional and
covered by fixtures from every released schema version. A newer unsupported
schema must fail with a clear error instead of opening partially.

### FS-11: Observability

The library must expose operation timing and storage counters through hooks or
structured results. It must not require a specific logging or metrics system.
Counters must distinguish logical bytes, retained content payload, branch-only
payload, and reclaimable payload.

## API direction

The exact names may change before version 0.1, but the intended workflow is:

```ts
const fs = await EphemeralFS.open({ database });

await fs.writeFile("/workspace/app.ts", source);

const branch = await fs.branches.create("agent-a");
await branch.writeFile("/workspace/app.ts", changedSource);

const result = await branch.publish({
  operationId: requestId,
});
```

A successful result includes the durable revision and changed paths. A
conflict result includes stable conflicting paths and does not change main.

## Performance requirements

Correctness takes priority over write reduction. Performance gates apply only
after the same workload passes on both compared engines.

Version 0.1 must:

- keep one small private overwrite proportional to copy-on-write page size;
- avoid rewriting an unchanged manifest suffix after a reconnectable insertion;
- prevent database payload from growing with every repeated write to one page;
- publish changes to independent paths without scanning unrelated file bytes;
- complete garbage collection in bounded transactions for large object sets;
- report benchmark distributions rather than a single best run.

The Computer host will preserve its fixed 512 KiB DOFS engine as a benchmark
control, not as a public production engine. Paired runs must use fresh,
isolated databases created from the same engine-neutral logical fixture.
Branch-only workloads may report DOFS as unsupported instead of changing the
workload. This repository does not package or depend on DOFS.

## Security and integrity requirements

- Validate paths before database access.
- Validate manifest lengths, object hashes, and chunk ordering when loading
  untrusted persisted state.
- Bound query batches and materialization sizes.
- Do not treat a content hash as proof that its stored bytes were verified.
- Make database corruption and unsupported schema versions visible.
- Keep adapter-specific native dependencies out of the core package.

## Delivery plan

### Milestone 0: Repository foundation

- Approve this product requirements document.
- Add the workspace package layout, license, provenance, and contribution rules.
- Import benchmark fixtures without generated dependencies or vendored package
  archives.

### Milestone 1: Portable storage core

- Define the database contract.
- Extract chunking, manifests, content storage, and copy-on-write pages.
- Add Node.js and Durable Object adapters.
- Run the same engine tests against both adapters.

### Milestone 2: Filesystem conformance

- Implement namespace and metadata operations.
- Add crash, migration, link, rename, and range-write conformance tests.
- Publish an unstable package for integration testing.

### Milestone 3: Branch publication

- Add complete branch lifecycle, idempotent publication, conflict records,
  recovery, retention, and exact garbage collection.
- Reproduce the existing storage and multi-agent benchmarks.

### Milestone 4: Host integration

- Make EphemeralAI FS the default implementation for Computer's
  `WorkspaceFilesystem`, filesystem primitives, storage schema, and
  `SQLiteWorkspaceProvider` path.
- Keep Computer-owned compatibility bridges for `workspace.fs`, sync, and
  FUSE without retaining `WorkspaceFilesystem` as a second semantic API.
- Retain DOFS only behind Computer's explicit comparison selector, using a
  separate database and the common filesystem benchmark surface.
- Run the full Durable Object, sync, `computerd`, FUSE, shell, and pull path.
- Document compatibility and migration behavior.

## Version 0.1 acceptance criteria

- Node.js and Durable Object adapters pass one shared conformance suite.
- Main and branch operations survive database reopen and process restart tests.
- Same-path concurrent writers produce an explicit conflict with no lost update.
- Independent-path writers publish successfully in either order.
- Idempotent publication returns the original durable result after restart.
- Garbage collection preserves active branches and all retained revisions.
- Benchmark results state the measured boundary and include reproducible inputs.
- EphemeralAI Computer defaults to EphemeralAI FS for its authoritative and
  local mirror filesystem paths without importing Computer-specific code into
  `@ephemeralai/fs`.
- An explicit DOFS comparison run uses an isolated database, never becomes an
  automatic fallback, and does not add a DOFS dependency to EphemeralAI FS.
- Public documentation distinguishes implemented behavior from planned work.

## Risks

- A complete filesystem namespace is broader than the current flat-path
  prototype.
- Content-defined chunking can use more processor time on first writes and full
  rewrites.
- Local rechunking can degrade to a full scan for widely distributed changes.
- SQLite adapters differ in transaction and binary-value APIs.
- File-level conflicts are safe but may reject changes that a semantic merge
  could combine.
- Retention and garbage collection bugs can either leak storage or delete live
  data, so destructive collection needs strong reachability tests.

## Open decisions

- Which Node.js SQLite library should the first adapter use?
- Should the core expose streaming file reads in version 0.1 or add them after
  byte-array conformance?
- Which metadata fields must be stable across Computer and Node.js hosts?
- How long should terminal branch records and publication results be retained?
- Should chunking parameters be fixed per filesystem or versioned per manifest?

## Licensing and provenance

The repository uses the MIT License. Imported or adapted code must retain the
license notices required by its source. `PROVENANCE.md` will identify the
original benchmark commit, the Cloudflare Computer revision used by the full
pipeline tests, and any code that derives from Cloudflare's MIT-licensed DOFS
implementation.
