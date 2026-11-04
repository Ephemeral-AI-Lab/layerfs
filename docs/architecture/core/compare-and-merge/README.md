# Compare and merge

[Architecture](../../README.md) › [Core](../README.md) › Compare and merge

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Diffing two trees or files, and merging Branches: decisions, conflict markers and revert.

| | |
| --- | --- |
| Layer | [Core](../README.md) |
| Crates | `layerfs-content` |
| Source | [`core/crates/layerfs-content/src/compare`](../../../../core/crates/layerfs-content/src/compare), [`core/crates/layerfs-content/src/merge`](../../../../core/crates/layerfs-content/src/merge) |
| Uses | [Filesystem trees](../filesystem-trees/README.md), [Files](../files/README.md) |
| Used by | [Engine: Store calls](../../engine/store-calls/README.md), [Engine: Commit](../../engine/commit/README.md) |
| Flows | [A call, end to end](../../flows/call.md), [Fork and merge](../../flows/merge.md), [A Commit](../../flows/commit.md) |

## Topics

| Topic | What it covers |
| --- | --- |
| [Compare](compare.md) | Comparing trees, files and text. |
| [Merge](merge.md) | Merging two lines of history into one tree. |

## The picture

```text
        engine: Store calls                  engine: Commit
   fs.diff_files, branch.diff_*,       merge-on-commit (Workspace
   branch.merge                        Commit onto a moved head)
          |              |                        |
          v              v                        v
 +----------------------------+   +-----------------------------------+
 | Compare                    |   | Merge                             |
 |  sorted   page-skip diff   |<--|  serial   walk one side's changes |
 |  tree     inodes, names,   |   |  decide   base / ours / theirs    |
 |           paths, listings  |   |  text     merge3, marker blocks   |
 |  named    paths from hints |   |  lww      newer side wins         |
 |  changes  ChangeWalk, Plan |   |  units    whole-unit pick, revert |
 |  bytes    same file bytes  |   |  build    plan -> backed update   |
 |  text     line diff        |   |                                   |
 |  memo     bounded page memo|   |                                   |
 +----------------------------+   +-----------------------------------+
          |  reads by ID                    |  emits new objects
          v                                 v
   AuthenticatedObjects               FinalizedConsumer
   (canonical bytes)                  (merged files, then the root)
```

Both modules work only on checked immutable roots. They read canonical
objects through [`AuthenticatedObjects`](../../../../core/crates/layerfs-content/src/object/access.rs)
and write through the caller's `FinalizedConsumer`; neither opens a database
([`lib.rs`](../../../../core/crates/layerfs-content/src/lib.rs)).

## What it does

| Call (engine) | Content functions it uses | Answer | Source |
| --- | --- | --- | --- |
| `branch.diff_commits`, `branch.diff_branches` | `Plan::make`, `ChangeWalk`, `Memo`, `same_file_bytes` | one page of path pairs, resumable by cursor | [`read/compare.rs`](../../../../engine/crates/layerfs-store-service/src/read/compare.rs) |
| `fs.diff_files` | `line_diff`, `same_file_bytes` | `Equal`, `Text` lines or a binary verdict | [`read/compare.rs`](../../../../engine/crates/layerfs-store-service/src/read/compare.rs) |
| `fs.blame` | `match_lines` | line origins | [`read/blame.rs`](../../../../engine/crates/layerfs-store-service/src/read/blame.rs) |
| `branch.merge` | `unit_changes`, `decide_units`, `revert_units`, `merge_roots` | merged root, or `Conflict` and nothing created | [`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs), [`history/units.rs`](../../../../engine/crates/layerfs-store-service/src/history/units.rs) |
| Workspace Commit onto a moved head | `merge_roots`, `paths_with_markers`, `root_changes` | merged root; a conflicted root is installed with markers | [`store/commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |

A **conflict** is decided, not left open: the merged tree holds marker blocks
(`<<<<<<<`, `=======`, `>>>>>>>`) in text files, keeps one side for binary
or type conflicts, and lists each path with a `ConflictKind`. `branch.merge`
refuses a conflicted result and creates nothing; merge-on-commit installs it
as the Workspace's local base and refuses the next Commit while a listed file
still holds a complete marker block. Details are in [Merge](merge.md).

## Invariants

| Rule | Where it is enforced |
| --- | --- |
| Comparing two sorted trees reads only pages whose IDs differ; equal IDs are skipped whole. | [`compare/sorted.rs`](../../../../core/crates/layerfs-content/src/compare/sorted.rs) `difference` |
| A planned change walk does work that follows the changed inodes times their depth, not the tree. | [`compare/changes.rs`](../../../../core/crates/layerfs-content/src/compare/changes.rs) `Plan::make` |
| A stale or absent binding hint never changes an answer; it is used only when the tree binds that name to that inode. | [`changes.rs`](../../../../core/crates/layerfs-content/src/compare/changes.rs), [`named.rs`](../../../../core/crates/layerfs-content/src/compare/named.rs) |
| A bounded listing reports truncation, never a smaller complete answer. | [`compare/tree.rs`](../../../../core/crates/layerfs-content/src/compare/tree.rs) `RootChanges::truncated` |
| Unequal file roots prove nothing; file equality is decided by bytes. | [`compare/bytes.rs`](../../../../core/crates/layerfs-content/src/compare/bytes.rs) |
| A merge walks only the deciding side's changes from the base. | [`merge/serial.rs`](../../../../core/crates/layerfs-content/src/merge/serial.rs) |
| A merge result is an ordinary tree, built by the same backed update route as a Commit. | [`merge/build.rs`](../../../../core/crates/layerfs-content/src/merge/build.rs) `finish` |
| Conflicts and `.lww` decisions are reported in path order. | [`merge/entry.rs`](../../../../core/crates/layerfs-content/src/merge/entry.rs) |
| A merge state larger than its memory hold spills to the caller's records. | [`merge/spill.rs`](../../../../core/crates/layerfs-content/src/merge/spill.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Largest file compared or merged line by line | `MAXIMUM_TEXT_BYTES` = 1 MiB; larger is binary | [`compare/text.rs`](../../../../core/crates/layerfs-content/src/compare/text.rs) |
| Largest line-diff table | `MAXIMUM_DIFF_CELLS` = 2^20 cells; past it the middle is one removed-then-added block | [`compare/text.rs`](../../../../core/crates/layerfs-content/src/compare/text.rs) |
| Changed inodes a planned walk accepts | `MAXIMUM_CHANGED` = 65,536; past it `Plan::make` answers `None` and the caller walks the full trees | [`compare/changes.rs`](../../../../core/crates/layerfs-content/src/compare/changes.rs) |
| Objects one comparison memo holds | `MEMO_PAGES` = 256, least recently used evicted | [`compare/memo.rs`](../../../../core/crates/layerfs-content/src/compare/memo.rs) |
| Naming from hints | depth 4,096 components, 1,024 paths per inode; past either, breadth-first `locate` names the inode | [`compare/named.rs`](../../../../core/crates/layerfs-content/src/compare/named.rs) |
| `/.lww` files read per load | `MAXIMUM_RULE_FILES` = 16, includes counted | [`merge/lww.rs`](../../../../core/crates/layerfs-content/src/merge/lww.rs) |
| Changed serials a bounded merge decides | caller's `bound`; past it `BoundedCapacityExceeded { what: "merge.bound" }` before more decisions. The publication lane passes `LANE_MERGE_PATHS` = 1,024 | [`merge/serial.rs`](../../../../core/crates/layerfs-content/src/merge/serial.rs), [`commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |
| Merge state held in memory | `HELD_BYTES` = 8 MiB, then the caller's records | [`merge/spill.rs`](../../../../core/crates/layerfs-content/src/merge/spill.rs) |
| Ours and theirs in different inode scopes | `InvalidRecord("merge scope")` | [`merge/entry.rs`](../../../../core/crates/layerfs-content/src/merge/entry.rs) |
| Malformed merge unit | `Units::new` returns an error naming the unit or pattern | [`merge/units.rs`](../../../../core/crates/layerfs-content/src/merge/units.rs) |

A merge refuses a decision only through `merge.bound`; every other
disagreement becomes a listed conflict with a defined result.

## Demonstration

Commands are written from source and were not run while writing this page.
None of these tests carries `#[ignore]` or a `target_os` gate.

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`compare_full_resume.rs`](../../../../core/crates/layerfs-content/tests/compare_full_resume.rs) | `layerfs-content` | A resumed full walk lists the same pairs as an uninterrupted one, and its reads before the first pair do not grow with the cursor. |
| [`compare_memo.rs`](../../../../core/crates/layerfs-content/tests/compare_memo.rs) | `layerfs-content` | Past `MEMO_PAGES`, shared upper pages are still read once; evicted leaves are read again with the same bytes. |
| [`merge_units.rs`](../../../../core/crates/layerfs-content/tests/merge_units.rs) | `layerfs-content` | Unit admission, which unit covers a path, a relink is no change, and ours wins a tie. |
| [`merge_units.rs`](../../../../engine/crates/layerfs-store-service/tests/merge_units.rs) | `layerfs-store-service` | `branch.merge` with `units` on an installed Store: a unit both sides changed comes whole from the later side. |
| [`merge_by_serial.rs`](../../../../engine/crates/layerfs-store-service/tests/merge_by_serial.rs) | `layerfs-store-service` | Moves follow their serial; crossed directory moves and delete-against-edit conflict. |

```text
compare_full_resume              merge_units (store-service)
  left tree   right tree           base --fork--> l: edit /nm/a, /src/s
  /a /d/... /z  (600 files)             \-fork--> r: edit /nm/a, rm /nm/c
  page(None)  -> all pairs         merge(no units) -> Conflict
  page(cursor)-> next 50 pairs     merge(units=["nm"]) -> /nm from r whole,
  reads before first pair <= +4                         /src/s merged
```

The `layerfs-content` tests use in-memory trees from `tests/support`:

    cargo +1.85.1 test --locked -p layerfs-content --test compare_full_resume
    cargo +1.85.1 test --locked -p layerfs-content --test compare_memo
    cargo +1.85.1 test --locked -p layerfs-content --test merge_units

The `layerfs-store-service` tests run Project Init into a temporary
Disposable SQLite Store and call the SDK method table in process
([`support/installed_store.rs`](../../../../engine/crates/layerfs-store-service/tests/support/installed_store.rs)).
They need no Docker, root or `postgresql` feature:

    cargo +1.85.1 test --locked -p layerfs-store-service --test merge_units
    cargo +1.85.1 test --locked -p layerfs-store-service --test merge_by_serial

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`core/docs/architecture/81-compare-and-merge.md`](../../../../core/docs/architecture/81-compare-and-merge.md):
  compare and merge are covered in [Compare](compare.md) and [Merge](merge.md);
  its merge-on-commit section is covered in
  [Engine: Commit, publish and merge](../../engine/commit/publish-and-merge.md).

---

← [Filesystem trees: Validation](../filesystem-trees/validation.md) · [Up: Core](../README.md) · [Compare and merge: Compare](compare.md) →
