# History publication

[Architecture](../../README.md) › [Engine](../README.md) › [Store](README.md) › History publication

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Writing Branch and Commit history and publishing it.

| | |
| --- | --- |
| Component | [Store](README.md) |
| Source | [`engine/crates/layerfs-persistence/src/history`](../../../../engine/crates/layerfs-persistence/src/history) |

`HistoryProvider` implements the core `HistoryCatalog` trait. Each catalog
call is one transaction: `read` runs `Session::run(false, ..)`, `write` runs
`Session::run(true, ..)`, on either engine
([`transaction.rs`](../../../../engine/crates/layerfs-persistence/src/history/transaction.rs),
[`catalog.rs`](../../../../engine/crates/layerfs-persistence/src/history/catalog.rs)).
The entities are defined in [Core: History model](../../core/history-model/README.md).

## Branch and Commit rows

```text
 layer_stack (project)                 branch "feature"
   head_layer_id ---> L2                 base_layer_id ---> L1
                      |                  head_commit_id --> C3
                      v parent                              | parent
                      L1 <---- base_layer_id of C1,C2,C3    v
                      |                                     C2 -> C1 -> (L1)
                      v
                      L0 (genesis)
```

| Catalog call | One write transaction does | Refusals | Source |
| --- | --- | --- | --- |
| `initialize_layerstack` | Insert the stack and its genesis Layer; lineage height 0 | identity or name taken: `Integrity` | [`layerstack.rs`](../../../../engine/crates/layerfs-persistence/src/history/layerstack.rs) |
| `fork` | Insert one Branch row at a Layer, or at a Commit on the source Branch's first-parent chain (with that Commit's own base). No content or history is copied. | name taken; Commit not in history: `NotInHistory` | [`branch.rs`](../../../../engine/crates/layerfs-persistence/src/history/branch.rs) |
| `commit_branch` | Check the expected head and base; insert or verify the Commit; advance the Branch head | moved head: `HeadMoved`, unless `force` with the same base; candidate equals current root: `UpToDate` | [`commit.rs`](../../../../engine/crates/layerfs-persistence/src/history/commit.rs) |
| `add_layer` | Publish a Branch head Commit as a new Layer on the stack head; record how many Layers it overwrote | Branch moved: `HeadMoved`; stack moved: `StackMoved`; Commit base differs: `BaseMismatch`; same root: `NoChanges` | [`layerstack.rs`](../../../../engine/crates/layerfs-persistence/src/history/layerstack.rs) |
| `reserve_inodes` | Consume a half-open inode range for a scope; advance `scope_allocator.highwater` | past `i64::MAX`: `Capacity` | [`allocation.rs`](../../../../engine/crates/layerfs-persistence/src/history/allocation.rs) |
| `record_bindings`, `record_file_digests` | Insert name hints and file digests (array form on PostgreSQL) | — | [`hints.rs`](../../../../engine/crates/layerfs-persistence/src/history/hints.rs), [`digests.rs`](../../../../engine/crates/layerfs-persistence/src/history/digests.rs) |

A Commit id is derived from its root, parent, base Layer, second parent and
metadata. A Commit whose identical row exists is verified, not inserted
again. A forced Commit keeps its captured parent and records the head it
displaced (`displaced_id`). LWW decisions and merge units are encoded into
the `lww` column ([`facts.rs`](../../../../engine/crates/layerfs-persistence/src/history/facts.rs)).
Allocated inode serials are never recycled.

## Merge

```text
 merge_branch(request)              one write transaction
   for each side (Main or Branch):  still at the node merged? else
                                      StackMoved (Main) / HeadMoved
   new Branch id and name free?     else Integrity
   head = Node(Layer)   -> Branch at that Layer       (fast-forward)
          Node(Commit)  -> Branch at that Commit      (fast-forward)
          Commit(merge) -> new_commit with second_parent = right side
   insert Branch row; return BranchSnapshot
```

Neither side changes. The merged tree itself is built before this call by
the Store calls layer; see [Store calls: Branches](../store-calls/branches.md).
Source: [`merge.rs`](../../../../engine/crates/layerfs-persistence/src/history/merge.rs).

## Lineage and ancestry

| Table | One row per | Fields | Answers |
| --- | --- | --- | --- |
| `layer_lineage` | Layer | `height` in its stack chain, `overwrote_count` | Layer order by height |
| `commit_lineage` | Commit | `run` (first-parent path, named by its first Commit), `depth`, `newest` Layer height, `merged` second parent | which run and depth a Commit sits at |
| `absorbed` | run prefix | `(run, depth)`, Layer `height` | the first Layer whose ancestry holds a Commit |
| `reach` | frontier entry | `(key, x)`, `height` | newest Layer published from a source |

`merge_base`, `is_ancestor`, `commit_history` and `ancestry_contains` walk
run segments and merges, never single Commits or the Layers between two
heights. Walks have no budget; answers are exact. A Store written before the
lineage tables gains them at writable open, filled once by
`backfill_lineage` in the migrating transaction. Sources:
[`lineage.rs`](../../../../engine/crates/layerfs-persistence/src/history/lineage.rs),
[`ancestry.rs`](../../../../engine/crates/layerfs-persistence/src/history/ancestry.rs),
[`lineage_backfill.rs`](../../../../engine/crates/layerfs-persistence/src/history/lineage_backfill.rs).

## Removal

`remove_layerstack(stack)` deletes one project in one write transaction, in
this order: `removal_totals`, `removal_absorbed`, `removal_reach`,
`removal_commit_lineage`, `removal_layer_lineage`, `removal_branches`,
`removal_commits`, `removal_layers`, `removal_stack`. Foreign keys are
deferred, so rows may go in any order; the order serves the totals and the
run-keyed lineage. Packs, objects, file digests and the stack's allocator
scope stay. An unknown stack is `Missing(LayerStack)`. Source:
[`removal.rs`](../../../../engine/crates/layerfs-persistence/src/history/removal.rs).

## The publish lane

```text
 Store-half Commit (store-service store/commit.rs)    lane  Store writer
 ---------------------------------------------------  ----  ------------
 capture; begin Save; construct candidate              -
 read head (branch_snapshot)                           -    read
 head moved?  merge candidate with head (two-stage)    -
 Save.finish; logical count; record binding hints      -    short writes
 history.publish_lane()  ---------------------------> HELD
 re-read head (unless forced or conflicted)            |    read
   moved again? merge only what landed in between,     |    short writes
   finish that step's Save; count; hints               |
 validate: prepare the Workspace base install          |
 commit_branch: head check, insert, advance            |    1 write tx
 drop(lane)  <---------------------------------------- free
 install the new base
```

| Rule | Where |
| --- | --- |
| The lane is one held admission turn plus the cross-process writer lock (SQLite) or the `store_lane` row (PostgreSQL). | [`sqlite/lane/lane.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lane/lane.rs), [`postgresql/lane/lane.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/lane/lane.rs) |
| Head check, merge step and publish run as one step under the lane; no other publication of any process interleaves. | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| Work that grows with the Commit (construction, Save finish, full merge, counts, hints) runs before the lane. | same, comment "Everything that grows with the Commit runs before the lane" |
| Reads never take the lane. Daemon and SDK readers use their own read-only sessions. | [`read_service.rs`](../../../../engine/crates/layerfs-store-service/src/store/read_service.rs) |
| A lane wait is readiness, not a retry. PostgreSQL's wait ends at `lock_timeout` with `Busy`. | [`admission.rs`](../../../../engine/crates/layerfs-persistence/src/backend/admission.rs) |

Work inside the lane that grows with tree size, history or Branch count is a
defect by owner rule. `commit_branch`, `fork`, `merge_branch` and `add_layer`
check their expected heads inside their own write transaction, which the
single writer already orders; only the Store-half Commit holds the lane across
several transactions. A provider with no concurrent publishers may return an
empty `PublishLane`
([`catalog.rs`](../../../../core/crates/layerfs-history/src/contract/catalog.rs)).

## Related

- [Backends](backends.md#one-writer): the locks the lane holds.
- [Commit: Publish and merge](../commit/publish-and-merge.md): the full Commit sequence.
- [Store calls: Branches](../store-calls/branches.md): fork, merge and publish as calls.
- [Core: History model](../../core/history-model/README.md): the entities and their ids.
- [Glossary](../../glossary.md).

---

← [Store: PostgreSQL](postgresql.md) · [Up: Store](README.md) · [Store: Metadata and objects](metadata-and-objects.md) →
