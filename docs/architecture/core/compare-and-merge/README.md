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
 |  across   one path, many   |   |                                   |
 |           versions (H5)    |   |                                   |
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
| `branch.diff_commits`, `branch.diff_branches` | `Plan::make`, `ChangeWalk`, `Memo`, `same_file_bytes` | one page of path pairs, resumable by cursor | [`compare/diff.rs`](../../../../engine/crates/layerfs-store-service/src/filesystem/compare/diff.rs) |
| `fs.diff_files` | `line_diff`, `same_file_bytes` | `Equal`, `Text` lines or a binary verdict | same |
| `fs.blame` | `match_lines` | line origins | [`text/blame.rs`](../../../../engine/crates/layerfs-store-service/src/filesystem/text/blame.rs) |
| `branch.merge` | `unit_changes`, `decide_units`, `revert_units`, `merge_roots` | merged root, or `Conflict` and nothing created | [`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs), [`history/merge_units.rs`](../../../../engine/crates/layerfs-store-service/src/history/merge_units.rs) |
| Commit onto a moved head | `merge_roots`, `paths_with_markers`, `root_changes` | merged root; a conflicted root is installed with markers | [`commit/merge.rs`](../../../../engine/crates/layerfs-store-service/src/workspace/commit/merge.rs) |

A conflict is decided, not left open: text files get marker blocks, binary or
type conflicts keep one side, and each path is listed with a `ConflictKind`.
`branch.merge` refuses a conflicted result; merge-on-commit installs it as the
Workspace's base and refuses the next Commit while a listed file still holds a
complete marker block ([Merge](merge.md)). The Workspace's view changes where
the merge changed it; a merge whose changes rows written after the capture
cover is not installed at all, clean or conflicted (`LiveConflict`,
[Commit](../../engine/commit/README.md#what-it-does)).

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
| `difference::<F>` | every differing key in key order; counts `pages_read`, `pages_shared`, `rows_compared` | [`pages.rs`](../../../../core/crates/layerfs-content/src/compare/pages.rs) |
| `TreeTable::open` | one root decoded once: its `FilesystemRoot` and inode table | [`tree.rs`](../../../../core/crates/layerfs-content/src/compare/tree.rs) |
| `inode_changes` / `inode_differences` | inodes whose value differs, ignoring / including reference count and metadata form | same |
| `binding_changes`, `new_bindings` | names whose binding differs; bindings only `right` holds | same |
| `ChangedDirectories`, `directory_bindings` | the directories whose listing an inode difference shows changed; their new bindings, 128 directories a window with both root pages of a window read in one demand (#810) | same |
| `root_changes` | a bounded list of `Added`, `Removed`, `Modified` paths with a `truncated` flag; a hard-linked file at its first breadth-first path | same |

An opened page is read in one demand with the pages after it at the same level
that the walk will certainly open too, leaving out any ID the other frontier
shows; at most `READ_AHEAD_PAGES` = 128 (1 MiB) are held, and each is opened
exactly once. Each frontier page carries the largest key its parent records:
when the other frontier still holds a page above the level, the walk takes
only the following pages whose largest key is at most the key before that
page (its hidden pages hold only larger keys, so none can share an
identity); when both fronts open at one level, both sides' pages go in one
demand (#810). `logical_size_and_directories` counts a root's logical size
and notes its changed directories in the same walk. `same_inode` compares decoded portable metadata, so a root-form
and an inline row of equal metadata are equal.

### One path across versions

A history call resolves the same path in each version it walks.
`PathAcross::resolve` takes a window of version roots and answers each
version exactly as walking it alone would (the store service's strict or
lenient path walk), refusals included: an intermediate symlink is
`Symlink { component }` where a strict walk refuses it, a bound
intermediate without an inode record is `Unbound`.

```text
 window of W roots (newest first), chain predicted by the previous window
   roots                 1 demand (<= 128 objects)
   inode values          all versions x all chain serials: one demand per
                         inode level (lookup_tables; pages shared by
                         versions read once, <= 128 pages a demand)
   directory bindings    DirectoryMemo (dir root, name) -> serial answers
                         unchanged directories; every miss of every version
                         and level: one demand per directory page level
   divergence            a binding != prediction: that version reads its
                         actual serial in one more inode wave
 window cost: 1 + h + m (+ h + m per diverged level); first window, no
 prediction: level by level, all versions together
```

| Function | Answers | Source |
| --- | --- | --- |
| `PathAcross::resolve` | one `PathAnswer` per root: `Entry`, `Absent`, `Symlink`, `Unbound`; with `batched = false` (no round-trip cost, `batched_chain_reads()` false) each root is opened and walked alone with today's reads | [`across.rs`](../../../../core/crates/layerfs-content/src/compare/across.rs) |
| `DirectoryMemo` | a call's `(directory root, name) -> serial` bindings, absence included, at most the caller's capacity; directory contents are immutable, so an equal root gives an equal binding | same |
| `lookup_tables` | inode values from several tables in one descent, one demand per level, each page checked where each table found it, answers as `lookup_many` per table | [`inode/read.rs`](../../../../core/crates/layerfs-content/src/filesystem/tree/inode/read.rs) |

A prediction never answers: a version's answer comes from its own bindings
and values, and a wrong prediction only costs the extra wave. Values and
bindings are dropped when `resolve` returns; what is kept across windows is
the serial chain (path depth + 1) and the caller's memo. An integrity failure
in any version of a batched window fails the window.

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
root pair ([`compare/plans.rs`](../../../../engine/crates/layerfs-store-service/src/filesystem/compare/plans.rs)).

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
| Comparing two sorted trees reads only pages whose IDs differ | `difference` in [`pages.rs`](../../../../core/crates/layerfs-content/src/compare/pages.rs) |
| A planned walk's work follows changed inodes times depth, not the tree | `Plan::make` in [`changes.rs`](../../../../core/crates/layerfs-content/src/compare/changes.rs) |
| A stale or absent hint never changes an answer | [`changes.rs`](../../../../core/crates/layerfs-content/src/compare/changes.rs), [`named.rs`](../../../../core/crates/layerfs-content/src/compare/named.rs) |
| A bounded listing reports truncation, never a smaller complete answer | `RootChanges::truncated` in [`tree.rs`](../../../../core/crates/layerfs-content/src/compare/tree.rs) |
| Unequal file roots prove nothing; equality is decided by bytes | [`bytes.rs`](../../../../core/crates/layerfs-content/src/compare/bytes.rs) |
| A merge walks only the deciding side's changes from the base | [`serial/walk.rs`](../../../../core/crates/layerfs-content/src/merge/serial/walk.rs) |
| A merge result is an ordinary tree, built by the same backed update as a Commit | `finish` in [`plan/build.rs`](../../../../core/crates/layerfs-content/src/merge/plan/build.rs) |
| Conflicts and `.lww` decisions are reported in path order | [`merge/roots.rs`](../../../../core/crates/layerfs-content/src/merge/roots.rs) |
| Merge state beyond its memory hold spills to the caller's records | [`plan/spill.rs`](../../../../core/crates/layerfs-content/src/merge/plan/spill.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Largest file compared or merged line by line | `MAXIMUM_TEXT_BYTES` 1 MiB; larger is binary | [`text.rs`](../../../../core/crates/layerfs-content/src/compare/text.rs) |
| Line-diff table | `MAXIMUM_DIFF_CELLS` 2^20 cells, then unique-line matching | same |
| Changed inodes a plan accepts | `MAXIMUM_CHANGED` 65,536; past it `Plan::make` is `None` and the caller walks fully | [`changes.rs`](../../../../core/crates/layerfs-content/src/compare/changes.rs) |
| Comparison memo | `MEMO_PAGES` 256; a batch size mismatch is `BatchCardinality` | [`memo.rs`](../../../../core/crates/layerfs-content/src/compare/memo.rs) |
| Objects one cross-version demand carries | 128 (the directory window; 1 MiB of pages, one storage `PREPARED_LEAVES` window); a wider wave takes more demands | [`across.rs`](../../../../core/crates/layerfs-content/src/compare/across.rs) |
| `/.lww` files per load | `MAXIMUM_RULE_FILES` 16, includes counted | [`rule/lww.rs`](../../../../core/crates/layerfs-content/src/merge/rule/lww.rs) |
| Changed serials a bounded merge decides | the caller's `bound`, else `BoundedCapacityExceeded { what: "merge.bound" }`; the publication lane passes `LANE_MERGE_PATHS` = 1,024 | [`serial/walk.rs`](../../../../core/crates/layerfs-content/src/merge/serial/walk.rs), [`merge.rs`](../../../../engine/crates/layerfs-store-service/src/workspace/commit/merge.rs) |
| Merge state in memory | `HELD_BYTES` 8 MiB, then the caller's records | [`plan/spill.rs`](../../../../core/crates/layerfs-content/src/merge/plan/spill.rs) |
| Ours and theirs in different scopes | `InvalidRecord("merge scope")` | [`merge/roots.rs`](../../../../core/crates/layerfs-content/src/merge/roots.rs) |
| Malformed merge unit | `Units::new` error naming the unit or pattern | [`rule/units.rs`](../../../../core/crates/layerfs-content/src/merge/rule/units.rs) |

Only `merge.bound` refuses a decision; every other disagreement becomes a
listed conflict with a defined result.

## Demonstration

`layerfs-content` tests use in-memory trees. The `layerfs-store-service` tests
run Project Init into a temporary SQLite Store and call the SDK
method table in process; they need no Docker, root or PostgreSQL server.
Each command gets a wall timeout of at most 120 s.

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`full_resume.rs`](../../../../core/crates/layerfs-content/tests/compare/full_resume.rs) | `cargo +1.85.1 test --locked -p layerfs-content --test compare full_resume` | any | A resumed full walk lists the same pairs as an uninterrupted one; reads before the first pair do not grow with the cursor |
| [`memo.rs`](../../../../core/crates/layerfs-content/tests/compare/memo.rs) | `cargo +1.85.1 test --locked -p layerfs-content --test compare memo` | any | Past 256 objects, shared upper pages are still read once |
| [`across.rs`](../../../../core/crates/layerfs-content/tests/compare/across.rs) | `cargo +1.85.1 test --locked -p layerfs-content --test compare across` | any | Each version's answer equals walking it alone on seeded random histories; per-window demands on depth-12, 10k-entry, symlink, hardlink, created-path and kind-change histories |
| [`merge_units.rs`](../../../../core/crates/layerfs-content/tests/merge_units.rs) | `cargo +1.85.1 test --locked -p layerfs-content --test merge_units` | any | Unit admission and coverage; a relink is no change; ours wins a tie |
| [`units.rs`](../../../../engine/crates/layerfs-store-service/tests/merge/units.rs) | `cargo +1.85.1 test --locked -p layerfs-store-service --test merge units` | any | `branch.merge` with `units`: a unit both sides changed comes whole from the later side |
| [`by_serial.rs`](../../../../engine/crates/layerfs-store-service/tests/merge/by_serial.rs) | `cargo +1.85.1 test --locked -p layerfs-store-service --test merge by_serial` | any | Moves follow their serial; crossed directory moves and delete-against-edit conflict |

## Pages

| Page | What it covers |
| --- | --- |
| [Merge](merge.md) | `merge/`: decisions, `.lww`, places and names, metadata, conflict markers, building the result, merge units and revert. |
