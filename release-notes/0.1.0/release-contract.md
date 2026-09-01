# LayerFS 0.1.0 release contract

> **Status:** Released Developer Preview contract, normative under `v0.1.0`.

This document defines what the LayerFS 0.1.0 Developer Preview represents. The
keywords **MUST**, **MUST NOT**, **SHOULD**, and **MAY** are normative within
this release record.

## 1. Immutable release identity

A distribution MUST identify the exact source and artifacts from which it was
built.

| Identity | Release value |
| --- | --- |
| Git tag | `v0.1.0` |
| Git commit | The commit resolved by `v0.1.0^{commit}` |
| Source archive SHA-256 | Recorded in the release asset `SHA256SUMS` |
| `Cargo.lock` SHA-256 | Recorded in the release asset `SHA256SUMS` |
| Checksum manifest identity | GitHub Release asset digest for `SHA256SUMS` |
| Verification | Required successful GitHub Actions check on the tagged commit |

The Git tag MUST resolve to the recorded commit. Release archives, executables,
helpers, and container images MUST match [artifacts.md](artifacts.md). A local
build from modified source is a derivative development build and MUST NOT be
represented as the verified 0.1.0 release.

## 2. Durable and ephemeral boundaries

One SDK `Client` binds one local `LayerStackStore`, one Monitor, one Workspace
manager, and one worker for each active Workspace UUID. A separate Store uses a
separate Client.

The Store is exactly one SQLite database file containing:

- named LayerStacks;
- immutable Layers;
- named writable Branches;
- immutable Commits; and
- one Store-wide namespace of canonical content-addressed objects.

A Workspace has no database. Its copy-on-write tree, execution state, output
log, spool files, mount, and container attachment are ephemeral. Workspace End
MUST NOT create a Commit. Only an explicit successful Workspace Commit may
publish durable Workspace state.

## 3. Identity and publication invariants

- Canonical object identity is derived from canonical bytes. Readers MUST
  authenticate stored objects before exposing their contents.
- Layer, Commit, Branch, and LayerStack IDs are authoritative. Entity names are
  validated immutable lookup and presentation metadata.
- Layers and Commits are immutable after publication.
- A Branch head/base update MUST occur after every required canonical object
  and immutable record is admitted successfully.
- A Workspace Commit MUST compare its expected Branch position before
  publication. A stale Workspace MUST return the typed stale/reconciliation
  result instead of silently replacing newer Branch state.
- Failed or interrupted publication MUST NOT expose a Branch head whose
  reachable closure is incomplete.
- Canonical objects are deduplicated by identity within the Store. Forking a
  Branch MUST NOT copy canonical objects.
- IDs, canonical encoding, content-defined chunking, and filesystem identity
  MUST NOT depend on entity display names.

The exact records, keys, schema, canonical encodings, and validation rules are
defined by the [storage-format contract](../../docs/versioned/0.1.0/storage-format.md)
and [product specification](../../docs/versioned/0.1.0/specification.md).

## 4. Public operation boundary

The supported public interfaces are the Rust SDK and standalone CLI documented
for 0.1.0:

- [Rust SDK reference](../../docs/versioned/0.1.0/sdk.md)
- [CLI reference](../../docs/versioned/0.1.0/cli.md)

The SDK and CLI MUST preserve the same Store and Workspace semantics. Internal
crate modules, SQL statements, benchmark helpers, test instrumentation, and
hidden diagnostic entry points are not public API.

Every Workspace execution starts a fresh process. A successful execution
receipt reports the exact terminal state and exit status. Output is bounded and
paged; callers MUST follow the documented cursor and truncation behavior.

Materialized and FUSE projections MUST represent the same logical Workspace
tree. FUSE Commit and End MUST quiesce callbacks, flush pending writes, surface
deferred write errors, and cross the publication boundary only after the
projection is fenced.

## 5. Container runtime boundary

Managed-container FUSE uses a prepared Linux container, a loopback-published
LayerFS daemon endpoint, a real `/dev/fuse` device, and `CAP_SYS_ADMIN`. It MUST
NOT require a host bind mount for the Store or Workspace payload. A context may
bind at most one running managed container at a time.

Container creation, image preparation, and daemon startup are distinct from a
Workspace lifecycle. Runtime requirements, helper placement, authentication,
mount readiness, cleanup, and resource limits are defined by the
[container-runtime contract](../../docs/versioned/0.1.0/container-runtime.md).

## 6. Acknowledgement and durability

A successful operation acknowledges that its transaction is committed and the
result is readable from the live local LayerFS process. The release does not
claim survival of sudden host power loss, kernel failure, storage-controller
failure, or forced process termination at every acknowledgement point.

Operators MUST retain an independent copy of important data. Benchmark
acknowledgement settings are documented with the benchmark and MUST NOT be
generalized into a stronger durability guarantee.

## 7. Compatibility and support level

0.1.0 is a Developer Preview. The source version, Rust SDK API, CLI grammar,
container protocol, and SQLite storage format are versioned together. Mixing
components from different minor release lines is unsupported unless a release
explicitly documents that combination.

A `0.1.x` patch release MUST preserve the documented Store schema, canonical
identity, daemon compatibility, CLI grammar, and public SDK behavior. A change
that cannot preserve those contracts targets `0.2.0` or later. Operators MUST
still preserve the original Store before evaluating any new build.

Supported environments and known constraints are recorded in
[limitations.md](limitations.md) and the versioned
[limitations reference](../../docs/versioned/0.1.0/limitations.md).

## 8. Release acceptance

The 0.1.0 release is accepted only when:

1. every mandatory gate in [verification.md](verification.md) passes against
   the exact release commit;
2. the source tree remains unchanged throughout verification;
3. release artifacts are produced from that source identity and their digests
   are recorded in [artifacts.md](artifacts.md);
4. documented CLI, SDK, Store, Workspace, FUSE, container, and failure-path
   examples agree with the released implementation; and
5. benchmark claims cite the immutable evidence summarized in
   [benchmark-results.md](benchmark-results.md).

A benchmark pass cannot substitute for a correctness gate, and a correctness
pass cannot substitute for artifact identity or documented limitations.
