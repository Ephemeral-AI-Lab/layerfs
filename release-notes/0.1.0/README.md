# LayerFS 0.1.0 Developer Preview

> **Status:** Developer Preview release record, frozen by the annotated
> `v0.1.0` tag.

LayerFS 0.1.0 is a Developer Preview of a local, content-addressed,
copy-on-write filesystem for branchable agent workspaces. It provides one
SQLite-backed `LayerStackStore`, immutable snapshots, named writable Branches,
ephemeral Workspaces, fresh-process command execution, explicit Commit, and
materialized or real-FUSE projections.

This directory is the release record. The versioned documentation
describes the product contract intended for this release; files outside the
versioned documentation may continue to evolve.

## Release identity

| Field | Value |
| --- | --- |
| Version | `0.1.0` |
| Channel | Developer Preview |
| Git tag | `v0.1.0` |
| Git commit | The commit resolved by `v0.1.0^{commit}` |
| Release date | 2026-09-01 |
| Source and artifact checksums | [Artifact manifest](artifacts.md) |
| Verification result | [Verification record](verification.md) |

## Product surface

The 0.1.0 public surface supports:

- creating or opening one local `LayerStackStore` per SDK Client;
- initializing a named LayerStack from an empty root or native directory;
- creating a named Branch from a Layer or Branch Commit without copying
  canonical objects;
- querying LayerStacks, Layers, Branches, Commits, Workspaces, and Monitor
  receipts through bounded SDK pages; the CLI convenience command drains those
  pages into one response;
- computing bounded path diffs between supported snapshot pairs;
- creating ephemeral Workspaces with materialized or managed-container FUSE
  projections;
- executing each Workspace command in a fresh process and reading bounded,
  paged output;
- committing a changed Workspace frontier to an immutable Commit and advancing
  the Branch head explicitly;
- reconciling an explicitly stale Workspace with typed conflict choices;
- adding a Branch head as the next immutable Layer; and
- inspecting passive operation, storage, lifecycle, and deduplication
  receipts.

Workspace End is explicit and never commits implicitly. Durable content lives
in the Store; Workspace runtime state and projections are ephemeral.

## Versioned documentation

- [Documentation index](../../docs/versioned/0.1.0/README.md)
- [Quickstart](../../docs/versioned/0.1.0/quickstart.md)
- [Product specification](../../docs/versioned/0.1.0/specification.md)
- [CLI reference](../../docs/versioned/0.1.0/cli.md)
- [Rust SDK reference](../../docs/versioned/0.1.0/sdk.md)
- [Container runtime contract](../../docs/versioned/0.1.0/container-runtime.md)
- [Storage-format contract](../../docs/versioned/0.1.0/storage-format.md)
- [Product limitations](../../docs/versioned/0.1.0/limitations.md)

## Release documents

- [Release contract](release-contract.md) defines the identity, behavioral,
  compatibility, and support boundary for 0.1.0.
- [Verification](verification.md) records the required source, correctness,
  integration, and evidence gates.
- [Limitations](limitations.md) summarizes the operational constraints that
  matter before evaluation.
- [Artifacts](artifacts.md) records release files, container images, digests,
  and provenance.
- [Benchmark results](benchmark-results.md) records the final reproducible
  performance and storage campaign without making it part of the correctness
  contract.

## Evaluation guidance

Build and evaluate LayerFS from an immutable 0.1.0 source identity, keep the
Store outside every imported or projected tree, and retain an independent copy
of important data. Managed-container FUSE evaluation requires a Linux runtime
with `/dev/fuse` and `CAP_SYS_ADMIN`; the authority Store remains a single
local SQLite file.

This release is intended for development, integration testing, benchmark
reproduction, and design evaluation. Review the [release limitations](limitations.md)
before using it with valuable or irreplaceable data.
