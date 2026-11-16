# History

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The immutable history records (LayerStacks, Branches, Commits, Layers), their
identities, and the one catalog contract a persistence provider implements.

| | |
| --- | --- |
| Layer | [Core](../README.md) |
| Crates | `layerfs-history` |
| Source | [`contract/`](../../../../core/crates/layerfs-history/src/contract), [`lib.rs`](../../../../core/crates/layerfs-history/src/lib.rs); provider: [`layerfs-persistence/src/history/`](../../../../engine/crates/layerfs-persistence/src/history) |
| Uses | [Objects](../objects/README.md) (`ObjectId` roots); merge records from [Compare and merge](../compare-and-merge/README.md) |
| Used by | [Engine: Store](../../engine/store/README.md), [Engine: Store calls](../../engine/store-calls/README.md), [Engine: Project Init](../../engine/project-init/README.md), [Engine: Daemon](../../engine/daemon/README.md), [Engine: Bridge](../../engine/bridge/README.md), [Engine: Kernel routes](../../engine/kernel-routes/README.md), [Control: Stores and projects](../../control/stores-and-projects/README.md), [Control: Sandboxes](../../control/sandboxes/README.md), [API: Calls](../../api/calls/README.md) |
| Flows | [A Commit](../../flows/commit.md), [Project Init](../../flows/init.md), [Fork and merge](../../flows/merge.md) |

## Picture

```text
 caller (Store calls, Project Init, daemon install)
   | typed request (CommitRequest, ForkRequest, ReserveRequest ...)
   v
 +----------------------- layerfs-history -------------------------+
 |  identity.rs   records.rs   catalog.rs (trait HistoryCatalog)   |
 |  query.rs (cursors, pages)  error.rs (HistoryError)             |
 +----------------------------|------------------------------------+
                              | one named operation per call
 +------------- layerfs-persistence: HistoryProvider --------------+
 |  layerstack, branch, commit, merge, ancestry, lineage,          |
 |  allocation, removal: one short read or write transaction each  |
 +----------------------------|------------------------------------+
                              v
                the one global Store (SQLite or PostgreSQL)

 LayerStack "shop"   L0 (genesis) <-- L1 <-- L2   <== head_layer
                          ^                ^
                          | base_layer     | base_layer
                     Branch "work"    Branch "fix"
                          C1 <-- C2       C5   <== head_commit
```

The crate opens no database, reads no pack and never reads a clock or PID to
invent an identity. A root is an `ObjectId`; history records which root a
Commit or Layer holds and never reads its content. The provider runs on the
Disposable profile only; Durable execution is
`NOT_RUN — disabled by owner until explicit reauthorization`.

## What it does

### Records

| Record | Fields | Notes | Source |
| --- | --- | --- | --- |
| `LayerStackRecord` | `id`, `name`, `scope`, `profile`, `head_layer` | one project's linear Layer timeline; `initialize_layerstack` writes it with its genesis Layer in one transaction | [`records.rs`](../../../../core/crates/layerfs-history/src/contract/records.rs) |
| `LayerRecord` | `parent`, `root`, `logical`, `source_branch`, `source_commit`, `overwrote_count` | one publication on the main line; provenance absent only for genesis; `logical` counts every hardlink path | same |
| `BranchRecord` | `id`, `stack`, `name`, `base_layer`, `head_commit`, `forked_from` | no head when forked from a Layer; `BranchSnapshot` adds `head_root`, `base_root`, `effective_root`, `scope`, `profile` | same |
| `CommitRecord` | `root`, `parent`, `base_layer`, `second_parent` (`NodeId`: Commit or Layer), `meta` | recorded ancestry, not a diff | same |

| `CommitMeta` field | In identity | Meaning |
| --- | --- | --- |
| `branch`, `via` | yes | Branch it was made on; `CommitVia` `Workspace` (1), `Files` (2), `Merge` (3) |
| `message` | yes | ≤ 4,096 bytes; the provider fills `"<via> filesystem change"` when absent |
| `merged_from`, `displaced` | yes | head a merge-on-commit started from; head a forced publication overwrote |
| `lww`, `lww_truncated`, `units` | no | persisted merge decisions and merge units |
| `statistics`, `time_ms` | no | `logical`, `new_canonical`, `new_stored`, `new_metadata`; set on insert |

There is no stage entity: the expected head carried by `CommitRequest` is the
only precondition. No record holds a connection, path, clock, live Workspace,
or any process, memory, socket or stdout/stderr state.

### Operations

| Operation | Call | Source |
| --- | --- | --- |
| Create a project | `initialize_layerstack` | [`layerstack.rs`](../../../../engine/crates/layerfs-persistence/src/history/layerstack.rs) |
| Fork | `fork(ForkRequest)`: from `Layer(id)` (no head) or `Commit { branch, commit }`, which must be in that Branch's ancestry, else `NotInHistory` | [`branch.rs`](../../../../engine/crates/layerfs-persistence/src/history/branch.rs) |
| Commit | `commit_branch(CommitRequest)` against the expected head and base | [`commit.rs`](../../../../engine/crates/layerfs-persistence/src/history/commit.rs) |
| Publish | `add_layer`: the Commit's root becomes the stack head (overwrite-only); Layers it does not contain are counted in `overwrote_count` and paged by `overwritten` | [`layerstack.rs`](../../../../engine/crates/layerfs-persistence/src/history/layerstack.rs) |
| Merge | `merge_branch`: a new Branch whose first state is a node (fast-forward) or a two-parent Commit | [`merge.rs`](../../../../engine/crates/layerfs-persistence/src/history/merge.rs) |
| Remove | `remove_layerstack`: the one removal; packs, objects, file digests and the allocation scope stay | [`removal.rs`](../../../../engine/crates/layerfs-persistence/src/history/removal.rs) |

```text
 Store calls (store/commit.rs)             HistoryProvider
   publish_lane() ------------------------> hold the Store's lane
   branch_snapshot(branch) ---------------> read head inside the lane
     (head moved? merge in lane)
   commit_branch(CommitRequest) ----------> one write transaction:
                                             profile, scope, head, base
                                             candidate == root? UpToDate
                                             insert Commit, advance head
   <-- Committed { record, displaced }
   drop(lane) ----------------------------> release
```

| `commit_branch` case | Result |
| --- | --- |
| Stack profile or scope differs | `Integrity("Commit context")` |
| Base moved, or head moved without `force` | `HeadMoved(MovedState)` with expected and actual head and base |
| Head moved, `force`, base unchanged | new Commit keeps `expected_head` as parent; `displaced` names the old head |
| `candidate_root` equals the effective root | `UpToDate { head, root }` |
| Otherwise | `Committed { record, displaced: None }` |

The lane flow is in
[`store/commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs).

### Catalog and queries

`HistoryCatalog` ([`catalog.rs`](../../../../core/crates/layerfs-history/src/contract/catalog.rs))
is `Send + Sync`; no method hands out SQL, a table, a row cursor or the
database. `HistoryCatalogConfig` holds `binding_key` (1–128 bytes, hashed into
`CatalogId`), `incarnation` (1 to `i64::MAX`) and a nonzero `cursor_key`.

| Kind | Methods |
| --- | --- |
| Point reads | `layer_stack[_named]`, `branch[_named]`, `branch_snapshot`, `commit`, `layer` |
| Paged reads | `layer_stacks`, `branches`, `commit_history`, `layer_history`, `overwritten` |
| Ancestry | `merge_base`, `is_ancestor` |
| Writes | `initialize_layerstack`, `fork`, `commit_branch`, `add_layer`, `merge_branch`, `remove_layerstack`, `reserve_inodes` |
| Lane | `publish_lane`: a `PublishLane` hold released on drop |
| Facts and hints | `statistics`, `record_init`, `record_bindings`, `bindings_of`, `record_file_digests`, `file_roots` (≤ `FILE_DIGEST_WINDOW` 1,024 pairs) |
| Saved queries | `query_authenticator`: keyed BLAKE3 in `layerfs/saved-query/v1`, binding `CatalogId` and incarnation, payload ≤ 16 KiB ([`query.rs`](../../../../core/crates/layerfs-history/src/contract/query.rs)) |

Trait defaults return `Unsupported` (or drop hint rows);
`HistoryProvider` overrides all of them.

```text
 cursor: 160 bytes, CURSOR_VERSION = 2
 ver 1 | range 1 | CatalogId 32 | incarnation 8 | subject 1+33
 | anchor 1+33 | position 1+33 | MAC 16 (keyed BLAKE3 over bytes 0..144)
```

| `Range` | Order | Subject |
| --- | --- | --- |
| `StackNames` (1), `BranchNames` (2) | name | none, stack |
| `CommitAncestry` (3) | newest first, by parent | Branch |
| `LayerChain` (4) | newest first, by publication | stack |

A cursor from another catalog, incarnation, range or subject is refused; a
changed byte fails as `Integrity("page cursor digest")`. The anchor is an
immutable Commit or Layer, so a growing Branch never invalidates a cursor; a
cursor is not a snapshot. A page holds the smaller of `limit` (1–128) and what
fits 16,208 bytes: at most 90 stacks, 81 Branches, 92 Layers or 3 Commits
(`MAXIMUM_ENCODED_BYTES` 179, 200, 176, 4,510). `merge_base` and `is_ancestor`
follow Commit parents and second parents, base Layers, Layer parents and
source Commits, exactly and with no history-size bound; among several best
common ancestors, the newest Layer ancestry wins, then the deepest first
parent, then the smallest identity. The provider answers from lineage records
([`lineage.rs`](../../../../engine/crates/layerfs-persistence/src/history/lineage.rs)).

### Identities

| Type | Bytes | Made by |
| --- | --- | --- |
| `BranchId` | 17, tag `0x11` | `from_authority(body)`, or `named(stack, name)`: BLAKE3 of `layerfs/sdk-branch/v1`, stack and name, first 16 bytes |
| `CommitId` | 33, tag `0x12` | BLAKE3 over `layerfs/commit/v3`, root, parent, base Layer, second parent, identity fields of `CommitMeta` |
| `LayerStackId` | 17, tag `0x31` | `from_authority(body)` |
| `LayerId` | 33, tag `0x32` | BLAKE3 over `layerfs/layer/v2`, stack, parent, root; provenance outside it |
| `WorkspaceId` | 32 | `from_authority`; all-zero is `InvalidInput` |
| `CatalogId` | 32 | BLAKE3 of `layerfs/history/catalog/v1` and the binding key |
| `NodeId` | 33 | `Commit` or `Layer`, told apart by the tag |

Source: [`identity.rs`](../../../../core/crates/layerfs-history/src/contract/identity.rs).
A wrong tag or width is `Integrity`. Two records deriving one identity must
agree on every stored field; a mismatch is an integrity failure, never an
overwrite.

### Inode-serial allocation

```text
 scope_allocator row: scope_id 32 | highwater | authority_id
 reserve_inodes(scope, count)   one write transaction
   absent row -> insert highwater 0;  authority != CatalogId -> Integrity
   end = highwater + count; end >= i64::MAX -> Capacity("inode serials")
   UPDATE highwater = end WHERE highwater = old
   --> Reservation { scope, start: old + 1, count }   [start, start+count)
```

| Caller | Reservation | Source |
| --- | --- | --- |
| Project Init | one range of the scanned entry count; its start seeds the tree | [`import/init.rs`](../../../../engine/crates/layerfs-project/src/import/init.rs) |
| Mounted Workspace | refill windows of 1,024 through `InodeSerials` | [`workspace/serials.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/serials.rs) |
| SDK file calls | at least 4,096, the rest kept in a per-Store, per-scope pool | [`store/ports.rs`](../../../../engine/crates/layerfs-store-service/src/store/ports.rs) |

A range is spent when handed out, used or not; serials are never recycled,
and `remove_layerstack` keeps the scope row
([`allocation.rs`](../../../../engine/crates/layerfs-persistence/src/history/allocation.rs)).

## Invariants

| Rule | Enforced by |
| --- | --- |
| Commits and Layers are immutable; a re-derived identity must match every field | `new_commit` in [`commit.rs`](../../../../engine/crates/layerfs-persistence/src/history/commit.rs), `verify_layer` in [`layerstack.rs`](../../../../engine/crates/layerfs-persistence/src/history/layerstack.rs) |
| A Branch head moves only through `commit_branch` against the expected head and base | [`commit.rs`](../../../../engine/crates/layerfs-persistence/src/history/commit.rs) |
| A stack head moves only through `add_layer` against `expected_stack_head`, else `StackMoved` | [`layerstack.rs`](../../../../engine/crates/layerfs-persistence/src/history/layerstack.rs) |
| History grows by appending; the one removal is a whole LayerStack | [`removal.rs`](../../../../engine/crates/layerfs-persistence/src/history/removal.rs) |
| Stack, Branch and Workspace identities come from the caller | [`lib.rs`](../../../../core/crates/layerfs-history/src/lib.rs) |
| Inode serials are consumed once and never reused | [`allocation.rs`](../../../../engine/crates/layerfs-persistence/src/history/allocation.rs) |
| A metadata transaction never spans upload, construction or Save completion | [`catalog.rs`](../../../../core/crates/layerfs-history/src/contract/catalog.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| History name | 1–63 ASCII bytes: lowercase, digits, `.` `_` `-`, alphanumeric at both ends | [`identity.rs`](../../../../core/crates/layerfs-history/src/contract/identity.rs) |
| Commit message | ≤ 4,096 bytes, no control character except `\n`, `\t` | [`records.rs`](../../../../core/crates/layerfs-history/src/contract/records.rs) |
| Page; cursor | 1–128 records, ≤ 16 KiB; 160 bytes | same, [`query.rs`](../../../../core/crates/layerfs-history/src/contract/query.rs) |
| Branches, depth, lineage walk | no count limit | [`lineage.rs`](../../../../engine/crates/layerfs-persistence/src/history/lineage.rs) |
| `HistoryError` | `InvalidInput`, `Missing(..)`, `Unsupported`, `Busy` (never retried silently), `OwnershipUnavailable`, `Capacity`, `Integrity`, `HeadMoved`, `BaseMismatch`, `NotInHistory`, `StackMoved`, `ContinuityUnavailable` | [`error.rs`](../../../../core/crates/layerfs-history/src/contract/error.rs) |
| Persistence did not answer | `UnknownOutcome`: nothing is replayed, discarded or rolled back on a guess | same |

Callers match variants; nothing parses message text.

## Demonstration

`layerfs-history` has one test of its own; the provider tests exercise the
contract on Disposable SQLite Stores. Each command gets a wall timeout of at
most 120 s.

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`heads.rs`](../../../../engine/crates/layerfs-persistence/tests/heads.rs) | `cargo +1.85.1 test --locked -p layerfs-persistence --test heads` | Linux, macOS (no-sidecar check Linux only) | Init, fork and Commit move the recorded heads exactly; a snapshot copy has equal heads |
| [`layerstack_removal.rs`](../../../../engine/crates/layerfs-persistence/tests/layerstack_removal.rs) | `cargo +1.85.1 test --locked -p layerfs-persistence --test layerstack_removal` | any; the PostgreSQL case needs `--features postgresql` and `LAYERFS_TEST_POSTGRES_URL`, else prints `NOT_RUN` | Removing one project leaves the other whole and frees the name |
| [`history_allocation.rs`](../../../../engine/crates/layerfs-persistence/tests/history_allocation.rs) | `cargo +1.85.1 test --locked -p layerfs-persistence --test history_allocation` | any | Reservations are half-open, per scope, consumed on failure and refused at the end |
| [`query_authenticator.rs`](../../../../core/crates/layerfs-history/tests/query_authenticator.rs) | `cargo +1.85.1 test --locked -p layerfs-history --test query_authenticator` | any | Every bound identity (catalog, incarnation) and payload byte changes the authenticator |
