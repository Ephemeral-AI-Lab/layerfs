# Compare and merge

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Diffing two trees, files or texts, and merging two lines of history into one
tree with decided conflicts.

| | |
| --- | --- |
| Layer | [Core](../README.md) |
| Crates | `layerfs-content` |
| Source | [`compare/`](../../../../core/crates/layerfs-content/src/compare), [`merge/`](../../../../core/crates/layerfs-content/src/merge) |
| Uses | [Filesystem trees](../filesystem-trees/README.md), [Files](../files/README.md), [Objects](../objects/README.md) |
| Used by | [Engine: Store calls](../../engine/store-calls/README.md), [Engine: Commit](../../engine/commit/README.md) |
| Flows | [Fork and merge](../../flows/merge.md), [A Commit](../../flows/commit.md) |

## Picture

```text
        engine: Store calls                   engine: Commit
   fs.diff_files, fs.blame,             merge-on-commit (Workspace
   branch.diff_*, branch.merge          Commit onto a moved head)
          |               |                        |
          v               v                        v
 +----------------------------+   +-----------------------------------+
 | compare/                   |   | merge/ (merge.md)                 |
 |  sorted   page-skip diff   |<--|  serial   walk one side's changes |
 |  tree     inodes, names,   |   |  decide   base / ours / theirs    |
 |           paths, listings  |   |  text     merge3, marker blocks   |
 |  named    paths from hints |   |  lww      newer side wins         |
 |  changes  ChangeWalk, Plan |   |  units    whole-unit pick, revert |
 |  bytes    same file bytes  |   |  build    plan -> backed update   |
 |  text     line diff        |   |                                   |
 |  memo     bounded page memo|   |                                   |
 +----------------------------+   +-----------------------------------+
          | reads by ID                      | emits new objects
          v                                  v
   AuthenticatedObjects               FinalizedConsumer
```

Both modules work on checked immutable roots and open no database
([`lib.rs`](../../../../core/crates/layerfs-content/src/lib.rs)).

## What it does

| Engine call | Content functions | Answer | Source |
| --- | --- | --- | --- |
| `branch.diff_commits`, `branch.diff_branches` | `Plan::make`, `ChangeWalk`, `Memo`, `same_file_bytes` | one page of path pairs, resumable by cursor | [`read/compare.rs`](../../../../engine/crates/layerfs-store-service/src/read/compare.rs) |
| `fs.diff_files` | `line_diff`, `same_file_bytes` | `Equal`, `Text` lines or a binary verdict | same |
| `fs.blame` | `match_lines` | line origins | [`read/blame.rs`](../../../../engine/crates/layerfs-store-service/src/read/blame.rs) |
| `branch.merge` | `unit_changes`, `decide_units`, `revert_units`, `merge_roots` | merged root, or `Conflict` and nothing created | [`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs), [`history/units.rs`](../../../../engine/crates/layerfs-store-service/src/history/units.rs) |
| Commit onto a moved head | `merge_roots`, `paths_with_markers`, `root_changes` | merged root; a conflicted root is installed with markers | [`store/commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |

A conflict is decided, not left open: text files get marker blocks, binary or
type conflicts keep one side, and each path is listed with a `ConflictKind`.
`branch.merge` refuses a conflicted result; merge-on-commit installs it as the
Workspace's base and refuses the next Commit while a listed file still holds a
complete marker block ([Merge](merge.md)).

### Tree comparison

```text
 difference(left, right): one ordered frontier per side
   left  [P a1][P a2][P a3]     right [P a1][P b2][P a3]    P = page (ID, level)
     same ID: pop both, pages_shared += 1, nothing read
     IDs differ: open the higher-level page (both if equal level)
     rows: equal key and value -> skip; key on one side -> visit(k, ..)
```

| Function | Answers | Source |
| --- | --- | --- |
| `difference::<F>` | every differing key in key order; counts `pages_read`, `pages_shared`, `rows_compared` | [`sorted.rs`](../../../../core/crates/layerfs-content/src/compare/sorted.rs) |
| `TreeTable::open` | one root decoded once: its `FilesystemRoot` and inode table | [`tree.rs`](../../../../core/crates/layerfs-content/src/compare/tree.rs) |
| `inode_changes` / `inode_differences` | inodes whose value differs, ignoring / including reference count and metadata form | same |
| `binding_changes`, `new_bindings` | names whose binding differs; bindings only `right` holds | same |
| `root_changes` | a bounded list of `Added`, `Removed`, `Modified` paths with a `truncated` flag; a hard-linked file at its first breadth-first path | same |

An opened page is read in one demand with the pages after it at the same level
that the walk will certainly open too, leaving out any ID the other frontier
shows; at most `READ_AHEAD_PAGES` = 128 (1 MiB) are held, and each is opened
exactly once. `same_inode` compares decoded portable metadata, so a root-form
and an inline row of equal metadata are equal.

### Naming changed inodes

Paths are not stored in the tree, so a changed inode must be named.

| Way | Used when | Gives up when | Source |
| --- | --- | --- | --- |
| `locate` | no hints, or hints could not name every inode | `limit` directories listed (breadth first); unnamed serials are absent | [`tree.rs`](../../../../core/crates/layerfs-content/src/compare/tree.rs) |
| `name_from_hints` | caller passes `BindingHints` (the engine's history catalog) | fewer checked bindings than `namespace_ref_count`, depth 4,096, or more than 1,024 paths per inode | [`named.rs`](../../../../core/crates/layerfs-content/src/compare/named.rs) |

A hint counts only when the tree binds that name to that serial, so a stale
hint can make naming give up, never give a wrong path.

### Change lists

```text
 Plan::make(hints, left, right)
   inode_differences --> changed serials (<= 65,536)
   per side, per serial: climb checked bindings to /  --> dirty names
   any binding missing --> None (use a full walk)
 ChangeWalk: stack of frames; next_pair() -> (path, L, R)
   enter(pair): same serial with a plan -> dirty + changed names only
                otherwise merge both listings, 64 names / 8 KiB per window
```

| Mode | Reads | Source |
| --- | --- | --- |
| `ChangeWalk::planned` | only directories on a path to a changed inode, or whose listing or ID differs | [`changes.rs`](../../../../core/crates/layerfs-content/src/compare/changes.rs) |
| `ChangeWalk::full` | both trees, listings merged by name in windows | same |
| `enter_from(pair, from)` | a resumed page: names at or after `from` | same |

A plan depends only on the two immutable roots, so the engine keeps plans by
root pair ([`read/plans.rs`](../../../../engine/crates/layerfs-store-service/src/read/plans.rs)).

### Bytes, text and memo

| Function | Rule | Source |
| --- | --- | --- |
| `same_file_bytes` | equal bytes whatever the roots: shared extents are skipped, unshared bytes compared in 256 KiB windows up to the first difference; a file edited away and back has its old bytes under another root | [`bytes.rs`](../../../../core/crates/layerfs-content/src/compare/bytes.rs) |
| `same_file_bytes_within`, `same_content` | the same within `limit` bytes; larger files with different roots answer `false` | same |
| `as_text`, `lines` | UTF-8 with no NUL, else binary; split on every `\n` | [`text.rs`](../../../../core/crates/layerfs-content/src/compare/text.rs) |
| `line_diff`, `match_lines` | longest common subsequence, removals before additions on ties; `match_lines` serves merge and blame | same |
| `Memo` | wraps a reader for one comparison; least-recently-used, 256 objects; upper pages stay because every lookup uses them | [`memo.rs`](../../../../core/crates/layerfs-content/src/compare/memo.rs) |

A diff whose `(m + 1)(n + 1)` table exceeds 2^20 cells keeps the common
prefix and suffix, matches the middle's lines that occur once on each side,
and diffs each gap alone within the same budget. A gap past it is one removed
block then one added block: a merge conflicts there, and blame gives those
lines to the newer version.

## Invariants

| Rule | Enforced by |
| --- | --- |
| Comparing two sorted trees reads only pages whose IDs differ | `difference` in [`sorted.rs`](../../../../core/crates/layerfs-content/src/compare/sorted.rs) |
| A planned walk's work follows changed inodes times depth, not the tree | `Plan::make` in [`changes.rs`](../../../../core/crates/layerfs-content/src/compare/changes.rs) |
| A stale or absent hint never changes an answer | [`changes.rs`](../../../../core/crates/layerfs-content/src/compare/changes.rs), [`named.rs`](../../../../core/crates/layerfs-content/src/compare/named.rs) |
| A bounded listing reports truncation, never a smaller complete answer | `RootChanges::truncated` in [`tree.rs`](../../../../core/crates/layerfs-content/src/compare/tree.rs) |
| Unequal file roots prove nothing; equality is decided by bytes | [`bytes.rs`](../../../../core/crates/layerfs-content/src/compare/bytes.rs) |
| A merge walks only the deciding side's changes from the base | [`merge/serial.rs`](../../../../core/crates/layerfs-content/src/merge/serial.rs) |
| A merge result is an ordinary tree, built by the same backed update as a Commit | `finish` in [`merge/build.rs`](../../../../core/crates/layerfs-content/src/merge/build.rs) |
| Conflicts and `.lww` decisions are reported in path order | [`merge/entry.rs`](../../../../core/crates/layerfs-content/src/merge/entry.rs) |
| Merge state beyond its memory hold spills to the caller's records | [`merge/spill.rs`](../../../../core/crates/layerfs-content/src/merge/spill.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Largest file compared or merged line by line | `MAXIMUM_TEXT_BYTES` 1 MiB; larger is binary | [`text.rs`](../../../../core/crates/layerfs-content/src/compare/text.rs) |
| Line-diff table | `MAXIMUM_DIFF_CELLS` 2^20 cells, then unique-line matching | same |
| Changed inodes a plan accepts | `MAXIMUM_CHANGED` 65,536; past it `Plan::make` is `None` and the caller walks fully | [`changes.rs`](../../../../core/crates/layerfs-content/src/compare/changes.rs) |
| Comparison memo | `MEMO_PAGES` 256; a batch size mismatch is `BatchCardinality` | [`memo.rs`](../../../../core/crates/layerfs-content/src/compare/memo.rs) |
| `/.lww` files per load | `MAXIMUM_RULE_FILES` 16, includes counted | [`merge/lww.rs`](../../../../core/crates/layerfs-content/src/merge/lww.rs) |
| Changed serials a bounded merge decides | the caller's `bound`, else `BoundedCapacityExceeded { what: "merge.bound" }`; the publication lane passes `LANE_MERGE_PATHS` = 1,024 | [`merge/serial.rs`](../../../../core/crates/layerfs-content/src/merge/serial.rs), [`commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |
| Merge state in memory | `HELD_BYTES` 8 MiB, then the caller's records | [`merge/spill.rs`](../../../../core/crates/layerfs-content/src/merge/spill.rs) |
| Ours and theirs in different scopes | `InvalidRecord("merge scope")` | [`merge/entry.rs`](../../../../core/crates/layerfs-content/src/merge/entry.rs) |
| Malformed merge unit | `Units::new` error naming the unit or pattern | [`merge/units.rs`](../../../../core/crates/layerfs-content/src/merge/units.rs) |

Only `merge.bound` refuses a decision; every other disagreement becomes a
listed conflict with a defined result.

## Demonstration

`layerfs-content` tests use in-memory trees. The `layerfs-store-service` tests
run Project Init into a temporary Disposable SQLite Store and call the SDK
method table in process; they need no Docker, root or `postgresql` feature.
Each command gets a wall timeout of at most 120 s.

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`compare_full_resume.rs`](../../../../core/crates/layerfs-content/tests/compare_full_resume.rs) | `cargo +1.85.1 test --locked -p layerfs-content --test compare_full_resume` | any | A resumed full walk lists the same pairs as an uninterrupted one; reads before the first pair do not grow with the cursor |
| [`compare_memo.rs`](../../../../core/crates/layerfs-content/tests/compare_memo.rs) | `cargo +1.85.1 test --locked -p layerfs-content --test compare_memo` | any | Past 256 objects, shared upper pages are still read once |
| [`merge_units.rs`](../../../../core/crates/layerfs-content/tests/merge_units.rs) | `cargo +1.85.1 test --locked -p layerfs-content --test merge_units` | any | Unit admission and coverage; a relink is no change; ours wins a tie |
| [`merge_units.rs`](../../../../engine/crates/layerfs-store-service/tests/merge_units.rs) | `cargo +1.85.1 test --locked -p layerfs-store-service --test merge_units` | any | `branch.merge` with `units`: a unit both sides changed comes whole from the later side |
| [`merge_by_serial.rs`](../../../../engine/crates/layerfs-store-service/tests/merge_by_serial.rs) | `cargo +1.85.1 test --locked -p layerfs-store-service --test merge_by_serial` | any | Moves follow their serial; crossed directory moves and delete-against-edit conflict |

## Pages

| Page | What it covers |
| --- | --- |
| [Merge](merge.md) | `merge/`: decisions, `.lww`, places and names, metadata, conflict markers, building the result, merge units and revert. |
