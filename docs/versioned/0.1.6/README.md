# LayerFS 0.1.6 manual

> **Status:** LayerFS 0.1.6 Developer Preview manual.

This is the versioned manual for the 0.1.6 Developer Preview. The release is
source-only: the [GitHub release](https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v0.1.6)
carries the tagged source and its checksums, and no package, prebuilt executable
or public runtime image is published.

LayerFS provides local versioned Workspaces, shared FUSE/SDK editing and
explicit snapshot publication. **v0.1.6 moves the live mutable state into the
sandbox.** The container-side owner holds the mutable namespace, file data and
private packed payload backing for a sandbox-owned Workspace, and the host
receives only Commit-time mutable-state transfer; the pause/quiesce path is gone,
the workspace is continuous, and canonical construction runs with **one
construction worker**. Memory accounting is honest: the sandbox spool keeps a
bounded resident window instead of holding a payload in page cache, so a
transfer pays a storage read instead of being credited by the writes that
produced it.

The Store format does **not** move in this release: `SCHEMA_VERSION` stays 10,
no schema or SQL change landed since v0.1.5, and canonical identity and Commit
derivation are untouched — the crates that define them
(`layerfs-content`, `layerfs-layerstack-store`) are byte-identical to v0.1.5.
Explicit compaction stays removed by the earlier owner decision.

## What is new in 0.1.6

1. Sandbox-local snapshots replace host authority (see
   [the product specification](specification.md#shared-live-workspace) and
   [the container runtime](container-runtime.md)).
2. One construction worker, by rule and by default.
3. Bounded sandbox spool memory with a declared cache stance.
4. Two correctness fixes and one reliability repair found by the campaign
   (`truncate` route migration, Commit `previous_head`, failed-publication retry).
5. Three new benchmark families and three extended ones — multi-branch graphs,
   longer histories, concurrent workspaces and retained-state access.
   [Full changelog](../../../docs/releases/v0.1.6/CHANGELOG.md).

## Read this manual

1. [Quickstart](quickstart.md)
2. [Product specification](specification.md)
3. [CLI reference](cli.md)
4. [Rust SDK reference](sdk.md)
5. [Container runtime](container-runtime.md)
6. [Storage format](storage-format.md)
7. [Limitations](limitations.md)

## Compatibility boundary

New Stores use **schema 10** with 4 KiB pages and pooled metadata tables.
Supported schema-6 (legacy), schema-7, schema-8 and schema-9 Stores connect
without promotion and keep the construction/encoding policy of their own
schema. Published v0.1.3/schema-5 Stores and other unsupported versions are
rejected without mutation; there is no in-place promotion to schema 10, no
downgrade and no retained-history transfer command. Keep existing Stores with
their matching binaries. Importing a directory into a new Store copies the
selected filesystem state and starts new history. See
[storage compatibility](storage-format.md#compatibility-and-migration).

**Explicit compaction is removed.** New Stores and every ordinary write use the
schema-10 ordinary path; there is no `LayerStackStore::compact_into`, no
compaction options/receipt export and no `layerfs-store-compact` binary in this
release. Stores that were compacted by an earlier build remain readable through
the retained authenticated LFCNT1/version-107 read path, and no old data is
deleted or reinterpreted. The removal decision and its historical measurements
are recorded in the [compaction removal decision](../../roadmap/0.1/0.1.5/compaction-removal.md);
v0.1.6 keeps the removal unchanged.

Use matching SDK, CLI owner, daemon and runtime components. Mixed-version live
sessions are unsupported: a v0.1.6 sandbox owner and a v0.1.5 host do not share a
mutable workspace. The [0.1.5 manual](../0.1.5/README.md) remains the historical
0.1.5 contract.

## Qualification and acceptance

The [v0.1.6 release record](../../../release-notes/0.1.6/README.md) records the
measured product source, the identity chain, every declared exception and every
non-passing row. The registered benchmark requirement is 33 regular cases plus 3
extensions, one performance and one separate verification invocation each: 28
performance PASS with 8 declared `N/A`, **36 verification PASS**, and no
`FAIL`/`TIMEOUT`/`NOT_RUN` on the final identity. One owner-declared verification
exception (a 30 s ceiling for one L500 K100 branch row) is reported with its
measured wall. See [acceptance](../../../release-notes/0.1.6/acceptance.md),
[waivers](../../../release-notes/0.1.6/waivers.md) and
[evidence limits](limitations.md#scale-and-evidence).
