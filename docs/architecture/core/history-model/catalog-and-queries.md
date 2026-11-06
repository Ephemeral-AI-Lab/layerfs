# Catalog and queries

[Architecture](../../README.md) › [Core](../README.md) › [History model](README.md) › Catalog and queries

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The history catalog and the queries over it.

| | |
| --- | --- |
| Component | [History model](README.md) |
| Source | [`core/crates/layerfs-history/src/contract/catalog.rs`](../../../../core/crates/layerfs-history/src/contract/catalog.rs), [`core/crates/layerfs-history/src/contract/query.rs`](../../../../core/crates/layerfs-history/src/contract/query.rs), [`core/crates/layerfs-history/src/contract/error.rs`](../../../../core/crates/layerfs-history/src/contract/error.rs); provider: [`engine/crates/layerfs-persistence/src/history/catalog.rs`](../../../../engine/crates/layerfs-persistence/src/history/catalog.rs) |

## Catalog

`HistoryCatalog` is a `Send + Sync` trait. Each method is one named history
operation; none hands out SQL, a table, a row cursor or the database. The one
implementation in the product is `HistoryProvider` in `layerfs-persistence`,
which runs each method as one read or one write transaction.

| `HistoryCatalogConfig` field | Check | Meaning |
| --- | --- | --- |
| `binding_key` | 1–128 bytes | Names the content authority; `CatalogId::derive` hashes it |
| `incarnation` | 1 to `i64::MAX` | Catalog generation; cursors bind it |
| `cursor_key` | not all zero | Secret for the cursor MAC; never stored in a cursor |

| Kind | Methods |
| --- | --- |
| Point reads | `layer_stack`, `layer_stack_named`, `branch`, `branch_named`, `branch_snapshot`, `commit`, `layer` |
| Paged reads | `layer_stacks`, `branches`, `commit_history`, `layer_history`, `overwritten` |
| Ancestry | `merge_base`, `is_ancestor` |
| Writes | `initialize_layerstack`, `fork`, `commit_branch`, `add_layer`, `merge_branch`, `remove_layerstack`, `reserve_inodes` |
| Lane | `publish_lane` returns a `PublishLane` hold, released on drop |
| Facts and hints | `statistics`, `record_init`, `record_bindings`, `bindings_of`, `record_file_digests`, `file_roots` (up to `FILE_DIGEST_WINDOW` = 1,024 pairs, one overlay page; 64 before layerfs-project #69) |

Default trait bodies make some methods optional: `overwritten`, `statistics`,
`record_init` and `remove_layerstack` return `Unsupported`; the binding and
digest methods accept and drop rows. `HistoryProvider` overrides all of them.

### One Commit publication

```text
 Store calls (store/commit.rs)            HistoryProvider
   |                                        |
   |-- publish_lane() --------------------->| hold the Store's lane
   |-- branch_snapshot(branch) ------------>| read head inside the lane
   |   (head moved? merge in lane)          |
   |-- commit_branch(CommitRequest) ------->| one write transaction:
   |                                        |  check profile, scope
   |                                        |  check expected head/base
   |                                        |  candidate == current root?
   |                                        |    -> UpToDate
   |                                        |  insert Commit, advance head
   |<-- Committed { record, displaced } ----|
   |-- drop(lane) ------------------------->| release
```

The lane flow is in
[`store/commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs);
the transaction is `commit_branch` in
[`history/commit.rs`](../../../../engine/crates/layerfs-persistence/src/history/commit.rs).

| `commit_branch` case | Result |
| --- | --- |
| Stack `profile` or `scope` differs from the request | `Integrity("Commit context")` |
| Base moved (with or without `force`), or head moved without `force` | `HeadMoved(MovedState)` with expected and actual head and base |
| Head moved, `force`, base unchanged | New Commit keeps `expected_head` as parent; `displaced` names the old head |
| `candidate_root` equals the current effective root | `UpToDate { head, root }` |
| Otherwise | `Committed { record, displaced: None }` |

## Queries

```text
 cursor: 160 bytes (CURSOR_VERSION = 2)
 0      1      2           34          42           76           110
 +------+------+-----------+-----------+------------+------------+
 | ver  |range | CatalogId | incarn.   | len|subject| len|anchor |
 |  1   |  1   |    32     |  8 (BE)   |   1 + 33   |   1 + 33   |
 +------+------+-----------+-----------+------------+------------+
 110          144         160
 +------------+-----------+
 | len|posit. | MAC       |   MAC = keyed BLAKE3 (cursor_key) over
 |   1 + 33   |    16     |   bytes 0..144, first 16 bytes
 +------------+-----------+
```

| `Range` | Order | Subject |
| --- | --- | --- |
| `StackNames` (1) | name | none |
| `BranchNames` (2) | name | stack |
| `CommitAncestry` (3) | newest first, by parent | Branch |
| `LayerChain` (4) | newest first, by publication | stack |

A cursor from another catalog, incarnation, range or subject is refused, and
a changed byte fails the MAC as `Integrity("page cursor digest")`. The anchor is
an immutable Commit or Layer, so a growing Branch never invalidates a cursor;
a cursor is not a snapshot ([`query.rs`](../../../../core/crates/layerfs-history/src/contract/query.rs)).

Page size is the smaller of the caller's `limit` (1–128) and what fits in
16,208 bytes: `MAXIMUM_RESULT_BYTES` (16,384) minus the cursor (160) and 16.
With the frozen record widths in
[`records.rs`](../../../../core/crates/layerfs-history/src/contract/records.rs):

| Record | `MAXIMUM_ENCODED_BYTES` | Records per page at most |
| --- | --- | --- |
| `LayerStackRecord` | 179 | 90 |
| `BranchRecord` | 200 | 81 |
| `LayerRecord` | 176 | 92 |
| `CommitRecord` | 4510 | 3 |

The continuation names the last delivered record and the next page resumes
strictly after it, so no record repeats or is skipped.

`merge_base` and `is_ancestor` follow Commit parents and second parents, base
Layers, Layer parents and each Layer's source Commit. The answer is exact and
has no history-size bound; among several maximal common ancestors, the one
with the newest Layer in its ancestry wins, then the deepest first parent, then
the smallest identity ([`catalog.rs`](../../../../core/crates/layerfs-history/src/contract/catalog.rs)).
The provider answers from lineage records rather than walking every Commit
([`lineage.rs`](../../../../engine/crates/layerfs-persistence/src/history/lineage.rs)).

## Errors

| `HistoryError` | Meaning |
| --- | --- |
| `InvalidInput(what)` | A checked input is unusable (name, page limit, cursor field) |
| `Missing(Missing)` | No `LayerStack`, `Branch`, `Commit`, `Layer`, `Scope` or `Catalog` row |
| `Unsupported(what)` | The provider lacks the capability |
| `Busy` | Admission contention, not a semantic conflict; never retried silently |
| `OwnershipUnavailable` | The bound content authority is not available |
| `Capacity(what)` | A declared bound or counter is exhausted |
| `Integrity(what)` | A record or relation contradicts another |
| `HeadMoved(MovedState)` | Branch head or base differs from the request |
| `BaseMismatch { commit_base, branch_base }` | The selected Commit's base is not the Branch's base |
| `NotInHistory(what)` | The source is not in the expected ancestry or stack |
| `StackMoved { expected, actual }` | Stack head differs from the request |
| `ContinuityUnavailable` | This process holds no writable authority |
| `UnknownOutcome` | Persistence did not answer; nothing may be replayed, discarded or rolled back on a guess |

Callers match variants; nothing parses the message text
([`error.rs`](../../../../core/crates/layerfs-history/src/contract/error.rs)).

## Related

- [Entities](entities.md): the records these operations read and write.
- [Allocation](allocation.md): `reserve_inodes`.
- [Engine: Store](../../engine/store/README.md): the provider, its transactions and the publication lane.
- [Engine: Store calls](../../engine/store-calls/README.md): the main caller of `commit_branch` and `publish_lane`.

---

← [History model: Entities](entities.md) · [Up: History model](README.md) · [History model: Allocation](allocation.md) →
