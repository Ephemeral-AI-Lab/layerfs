# History model

[Architecture](../../README.md) › [Core](../README.md) › History model

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

LayerStacks, Branches, Commits, Layers and stages: the immutable history metadata.

The `layerfs-history` crate defines the history records, their identities and
one semantic contract, [`HistoryCatalog`](../../../../core/crates/layerfs-history/src/contract/catalog.rs),
that a persistence provider implements. It opens no database, reads no pack
and never reads a clock or PID to invent an identity
([`lib.rs`](../../../../core/crates/layerfs-history/src/lib.rs)). Terms are in the
[glossary](../../glossary.md).

| | |
| --- | --- |
| Layer | [Core](../README.md) |
| Crates | `layerfs-history` |
| Source | [`core/crates/layerfs-history/src`](../../../../core/crates/layerfs-history/src); provider: [`engine/crates/layerfs-persistence/src/history`](../../../../engine/crates/layerfs-persistence/src/history) |
| Uses | [Objects](../objects/README.md) |
| Used by | [Engine: Store](../../engine/store/README.md) |
| Flows | [A Commit](../../flows/commit.md), [Project Init](../../flows/init.md) |

`CommitMeta` also carries merge records (`LwwDecision`, `MergeUnit`) from
[`layerfs-content`](../../../../core/crates/layerfs-content). Besides the Store,
these crates depend on `layerfs-history` (from their `Cargo.toml`):
[Store calls](../../engine/store-calls/README.md) (`layerfs-store-service`),
[Project Init](../../engine/project-init/README.md) (`layerfs-project`),
[Daemon](../../engine/daemon/README.md) (`layerfs-daemon`, `layerfs-bridge`),
[Kernel routes](../../engine/kernel-routes/README.md) (`layerfs-nfs`),
[Management](../../control/management/README.md) (`layerfs-control`) and
[Call table](../../api/call-table/README.md) (`layerfs-api`).

## Topics

| Topic | What it covers |
| --- | --- |
| [Entities](entities.md) | LayerStacks, Branches, Commits, Layers and stages. |
| [Catalog and queries](catalog-and-queries.md) | The history catalog and the queries over it. |
| [Allocation](allocation.md) | Scoped allocation of history identities. |

## The picture

```text
 caller (Store calls, Project Init, daemon install)
   | typed request (CommitRequest, ForkRequest, ReserveRequest ...)
   v
 +------------------------- layerfs-history ---------------------------+
 |  Entities              Catalog and queries         Allocation       |
 |  identity.rs           catalog.rs (HistoryCatalog)  ReserveRequest   |
 |  records.rs  <-------- query.rs (cursors, pages)    Reservation      |
 |                        error.rs (HistoryError)                      |
 +-----------------------------|---------------------------------------+
                               | trait HistoryCatalog
                               v
 +--------------- layerfs-persistence: HistoryProvider ----------------+
 |  history/catalog.rs -> layerstack, branch, commit, merge,           |
 |  ancestry, lineage, allocation, removal                             |
 |  one short read or write transaction per call                       |
 +-----------------------------|---------------------------------------+
                               v
                 the one global Store (SQLite or PostgreSQL)
```

Every catalog call is one named operation over typed values; the provider owns
the transaction ([`catalog.rs`](../../../../core/crates/layerfs-history/src/contract/catalog.rs),
[`HistoryProvider`](../../../../engine/crates/layerfs-persistence/src/history/catalog.rs)).

## What it does

```text
 LayerStack "shop" (one project)       main line of Layers
   L0 (genesis) <-- L1 <-- L2   <== head_layer
          ^              ^
          | base_layer   | base_layer
     Branch "work"   Branch "fix"
          |              |
          C1 <-- C2      C5   <== head_commit
```

| Term | In this model | Operation | Source |
| --- | --- | --- | --- |
| Project | One LayerStack: a name, an allocation scope, a filesystem profile and a head Layer | `initialize_layerstack` writes the stack and its genesis Layer together | [`records.rs`](../../../../core/crates/layerfs-history/src/contract/records.rs) |
| Layer | One immutable publication on the stack's main line; its parent is the previous head | `add_layer` | same |
| Branch | A base Layer plus an optional head Commit | `fork`, `merge_branch` | same |
| Commit | One immutable complete root with parent, base Layer and optional second parent | `commit_branch` against an expected head | same |
| Fork | A new Branch from a Layer (no head) or from a Commit in another Branch's ancestry | `fork(ForkRequest)` | [`branch.rs`](../../../../engine/crates/layerfs-persistence/src/history/branch.rs) |
| Publish | A Branch's head Commit becomes a new Layer on the stack head (overwrite-only) | `add_layer(AddLayerRequest)` | [`layerstack.rs`](../../../../engine/crates/layerfs-persistence/src/history/layerstack.rs) |
| Merge | A new Branch whose first state is a node (fast-forward) or a two-parent Commit | `merge_branch(BranchMergeRequest)` | [`records.rs`](../../../../core/crates/layerfs-history/src/contract/records.rs) |

A root is an [Objects](../objects/README.md) `ObjectId`. History records which
root a Commit or Layer holds; it never reads that root's content.

## Invariants

| Rule | Enforced by |
| --- | --- |
| Commits and Layers are immutable. A record that derives an existing identity must match it on every field, or the call fails `Integrity`. | [`identity.rs`](../../../../core/crates/layerfs-history/src/contract/identity.rs) module docs; `new_commit` in [`commit.rs`](../../../../engine/crates/layerfs-persistence/src/history/commit.rs), `verify_layer` in [`layerstack.rs`](../../../../engine/crates/layerfs-persistence/src/history/layerstack.rs) |
| A Branch head moves only through `commit_branch` against the expected head and base; a moved head is `HeadMoved` unless `force` | [`commit.rs`](../../../../engine/crates/layerfs-persistence/src/history/commit.rs) |
| A stack head moves only through `add_layer` against `expected_stack_head`; otherwise `StackMoved` | [`layerstack.rs`](../../../../engine/crates/layerfs-persistence/src/history/layerstack.rs) |
| History grows by appending; the one removal is a whole LayerStack (`remove_layerstack`) | [`removal.rs`](../../../../engine/crates/layerfs-persistence/src/history/removal.rs) |
| Stack, Branch and Workspace identities come from the caller; the crate never invents one | [`lib.rs`](../../../../core/crates/layerfs-history/src/lib.rs) |
| Inode serials are consumed once and never reused | [`allocation.rs`](../../../../engine/crates/layerfs-persistence/src/history/allocation.rs) |
| A metadata transaction never spans upload, construction or Save completion | [`catalog.rs`](../../../../core/crates/layerfs-history/src/contract/catalog.rs) module docs |
| Filesystem history is not a process, memory, socket or stdout/stderr checkpoint | Owner rule in the [repository guide](../../../../AGENTS.md); no record type holds such state ([`records.rs`](../../../../core/crates/layerfs-history/src/contract/records.rs)) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| History name | 1–63 ASCII bytes: lowercase, digits, `.` `_` `-`, alphanumeric at both ends | [`identity.rs`](../../../../core/crates/layerfs-history/src/contract/identity.rs) |
| Commit message | at most 4096 bytes (`MAXIMUM_MESSAGE_BYTES`), no control character except `\n` and `\t` | [`records.rs`](../../../../core/crates/layerfs-history/src/contract/records.rs) |
| Page size | 1–128 records (`MAXIMUM_PAGE_RECORDS`), at most 16 KiB encoded (`MAXIMUM_RESULT_BYTES`) | same |
| Cursor | fixed 160 bytes (`MAXIMUM_CURSOR_BYTES`) | [`query.rs`](../../../../core/crates/layerfs-history/src/contract/query.rs) |
| Branch count, history depth, lineage walk | no count limit in the contract; walks follow lineage records | [`lineage.rs`](../../../../engine/crates/layerfs-persistence/src/history/lineage.rs) |
| Inode serials per scope | end must stay below `i64::MAX`, else `Capacity("inode serials")` | [`allocation.rs`](../../../../engine/crates/layerfs-persistence/src/history/allocation.rs) |
| Persistence did not answer | `UnknownOutcome`: nothing is replayed or rolled back | [`error.rs`](../../../../core/crates/layerfs-history/src/contract/error.rs) |

Left to persistence: storing heads, the publication lane (`publish_lane`),
transactions, lineage tables and Store totals. Reclaiming packs that no stack
reaches is outside this contract: `remove_layerstack` leaves packs, objects,
file digests and the allocation scope in place
([`catalog.rs`](../../../../core/crates/layerfs-history/src/contract/catalog.rs)).

## Demonstration

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`heads.rs`](../../../../engine/crates/layerfs-persistence/tests/heads.rs) | `layerfs-persistence` | Stack creation, fork and Commit move the recorded heads exactly; a snapshot has the same heads |
| [`layerstack_removal.rs`](../../../../engine/crates/layerfs-persistence/tests/layerstack_removal.rs) | `layerfs-persistence` | Removing one of two projects leaves the other whole and frees the name |
| [`history_allocation.rs`](../../../../engine/crates/layerfs-persistence/tests/history_allocation.rs) | `layerfs-persistence` | Reservations are half-open, per scope, consumed on failure and refused at the end |

```text
 heads.rs:  init "shop" --> fork "work" from L0 --> commit_branch (Files)
            read_heads after each step; snapshot copy has equal heads
```

- **`heads.rs`** creates a Disposable SQLite Store (`SqliteHandles::create`),
  initializes stack `shop`, forks Branch `work` and commits once. It checks
  `read_heads` after each step, that a re-read changes nothing, and that
  `snapshot_observed` reports its bytes and copies equal heads. The file is
  gated `#![cfg(any(target_os = "macos", target_os = "linux"))]`; the
  no-sidecar check runs on Linux only.
  `cargo +1.85.1 test --locked -p layerfs-persistence --test heads`
- **`layerstack_removal.rs`** builds projects `shop` and `api` on a Disposable
  SQLite Store, removes `shop`, and checks that its records are gone, the
  logical total drops to `api` alone, a second removal is
  `Missing(LayerStack)`, `api` still commits and the name `shop` can be reused.
  `cargo +1.85.1 test --locked -p layerfs-persistence --test layerstack_removal`
  The PostgreSQL case needs `--features postgresql` and
  `LAYERFS_TEST_POSTGRES_URL`; without the variable it prints `NOT_RUN` and returns.
- **`history_allocation.rs`** reserves ranges in two scopes and checks the
  starts, a reservation surviving a failed stack creation, a zero count and the
  terminal endpoint. The file has no platform gate.
  `cargo +1.85.1 test --locked -p layerfs-persistence --test history_allocation`

These commands were not run while writing this page. `layerfs-history` has no
tests of its own; its contract is exercised through the provider.

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`docs/general/cluster_one_handbook.md`](../../../general/cluster_one_handbook.md) — its history parts (§4.3 candidate to Commit, inode reservations) are covered in [Catalog and queries](catalog-and-queries.md) and [Allocation](allocation.md); the rest belongs to other core components.
- [`core/docs/architecture/16-history.md`](../../../../core/docs/architecture/16-history.md) — identity and records covered in [Entities](entities.md), cursors and pages in [Catalog and queries](catalog-and-queries.md), allocation in [Allocation](allocation.md); its stage, `layerfs-server` and in-crate SQLite provider sections are historical only.

---

← [Physical storage: Ports](../physical-storage/ports.md) · [Up: Core](../README.md) · [History model: Entities](entities.md) →
