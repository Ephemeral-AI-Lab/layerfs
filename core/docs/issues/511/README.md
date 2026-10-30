# #511 Remove, delete, prune and compact: give Store space back on request

> **Status:** proposal, 2026-10-30. Owner decisions D1–D7 (§9) are open, and
> nothing here is implemented. It answers #411 and is #446 A8. It builds on
> the #506 Store model on PR #507, where `runtime.project.remove` was
> renamed `runtime.project.delete` on 2026-10-30. Figures in §2 are
> diagnostic: one read-only sample, not a qualified measurement.

## 1. Owner direction (2026-10-30)

- **No automatic compaction.** Space comes back only through explicit SDK
  calls. Nothing runs compaction on a timer, on close or after a removal.
- **Four calls:**
  - `branch.remove` and `project.delete` hide history at once;
  - `branch.prune` gives up the file trees of intermediate Commits;
  - `store.compact` rebuilds the Store file without what nothing uses.
- **Remove, delete and prune free nothing by themselves.** Compact is the only call
  that returns space.

## 2. Findings

### 2.1 Where the space is

Read-only from the Connect worker's Stores on 2026-10-30, with SQLite
queries over `commit`, `branch`, `layer`, `store_totals`, `pack` and
`dbstat`.

| Store | Every file in every Commit | After dedup | After compression and deltas | File |
| --- | --- | --- | --- | --- |
| `r2-examples-clone` (818 Commits, 410 Branches, 28 Layers) | 260.5 GB | 688 MB | 237.4 MB | 251.2 MB |
| `services` (155 Commits, 111 Branches, 10 Layers) | 6.24 GB | 489 MB | 148.9 MB | 155.5 MB |

`r2-examples-clone` by part (`dbstat`):

| Part | Size |
| --- | --- |
| File contents (payload packs) | 234.5 MB |
| Folder listings and file maps (metadata packs) | 2.9 MB |
| Lookup tables (`inode_value`, `object_location`, `binding_hint`, `file_digest`, `content_signature`) | 10.7 MB |
| History (Commits, Branches, Layers, lineage) | 0.55 MB (about 0.7 KB per Commit) |
| Pack pages and other | the rest |
| Free pages | 0 |

### 2.2 What removal and compact would free

Each Commit records the bytes it stored first (`commit.new_stored`). Summing
that over the Commits compact would drop gives an **upper bound**: remaining
history may still use those objects, directly or as delta bases. Only
compact's mark pass (§6) gives the exact figure.

| `r2-examples-clone`, removed then compacted | Commits dropped | Frees up to |
| --- | --- | --- |
| 384 Branches with no Commits of their own | 30 | 5.2 MB |
| `depot_2.implementation` alone | 4 (+1 shared with the above) | 94.6 MB |
| All 410 Branches (main kept) | 197 | 105.1 MB |
| The whole project | 818 | 237.4 MB |
| Prune all intermediate Commits (§5) | trees of 548 | 176.4 MB (loose) |

What this shows:

- **Forks cost almost nothing.** A fork copies no rows and shares its
  source's Commits.
- **Space concentrates in a few Commits.**
- **Published Commits keep their whole ancestry.** A Layer links to the
  Commit it came from (`layer.source_commit_id`), so its ancestors stay.
- **Intermediate trees are where prune helps.** History rows are under 1 MB.

### 2.3 Why removal cannot simply delete

All in the PR #507 tree.

- **Packs are immutable and shared.**
  - SQLite triggers refuse UPDATE and DELETE of `pack` and `pack_unit`
    (`sql/sqlite/objects.sql`, `objects_units.sql`); PostgreSQL has the same
    triggers.
  - `read_immutable` reads with no transaction, and that is only safe
    because locations are insert-only and packs immutable
    (`backend/sqlite/session/transaction.rs`).
  - One Save writes each pack, and later Commits of any project reuse its
    objects.
- **Delta bases are hidden edges.** A delta record names its base only by
  `ObjectId` (`layerfs-storage/src/encoding/delta/record.rs`). The candidate
  ring is Store-wide, so a base may belong to another project.
- **Pooled inode leaves share `inode_value` ordinals** across leaves and
  projects (`save/pooled.rs`).
- **Branches share Commits.**
  - Fork copies no rows (`history/branch.rs`).
  - Merge and published Layers point at other Branches' Commits.
  - Merge picks a fork point or a Layer as its base
    (`history/ancestry.rs`).
- **A Branch's ID is derived from its name.** `BranchId::named` is
  hash(stack, name), and its comment says names "are never reused"
  (`layerfs-history/src/contract/identity.rs`). `CommitId::derive` includes
  the Branch.
- **No Store table records mounts.** Mounts live in daemon memory; the only
  check is asking each Sandbox (`sandbox.list_workspaces`, `MountsActive`).
- **Some roots live outside the Store** (jiangkoumo on #411):
  - a mounted Workspace's base root after its Branch moved on;
  - Saves, Commits and merges in flight;
  - reads in flight, including the macOS NFS export until the daemon
    releases it.
- **Writes serialize.** The writer turn and the Linux directory lock cover
  every write in every process, and history-sized work inside the lane is
  a defect. The current `project.delete` on a shared Store deletes rows in
  proportion to the project's history inside one write.
- **`store_totals` only grows,** except `logical` on removal.

## 3. The calls

| Call | Surface | What it does | Writer hold | Frees space |
| --- | --- | --- | --- | --- |
| `branch.remove(name, { confirm })` | agent SDK, project client | Hides a Branch | one row update | no |
| `project.delete(name, { confirm, store, stopSandboxes, volume, url })` | runtime | Hides a project in a shared Store; deletes a project alone in its Store with its volume, as today | one row update | only in the alone case |
| `branch.prune(name, { confirm })` | agent SDK | Marks the Branch's own intermediate Commits as pruned | one batched update | no |
| `store.compact(name, { confirm, dryRun, stopSandboxes })` | runtime | Rebuilds the Store file with only what remains | whole run, Store closed | yes |

The CLI gets the same verbs (`layerfs branch remove`, `layerfs branch
prune`, `layerfs project delete`, `layerfs store compact`). Each call is one
attempt, with no retry.

## 4. Remove and delete

### 4.1 `branch.remove`

- **Refusals:**
  - `InvalidInput` unless `confirm` equals the name;
  - `NoSuchBranch` if the Branch is absent or already removed;
  - `ReservedName` for `main`, which is the stack, not a Branch row;
  - `MountsActive` while a Workspace is mounted on the Branch. The check
    asks each running Sandbox of the project, as `project.delete` does.
- **Effect:** one write sets `branch.removed_ms`. The Branch leaves
  `project.list_branches` and `branch.list_commits` at once, and fork,
  merge, publish and mount refuse it with `NoSuchBranch`. Its rows and data
  stay until compact.
- **A Commit already in flight** on the Branch fails `NoSuchBranch` at its
  publish step. The lane's head check reads the Branch row and must also
  require `removed_ms IS NULL`.
- **Mount race:** a mount that slips in between the check and the write
  keeps reading (nothing is freed before compact), and its Commit fails
  `NoSuchBranch`.
- **A Branch that is another Branch's base is not refused (D7).** The fork
  shares Commits, and they stay reachable through the fork, so removing the
  source frees only what the fork does not use.
- **Remote link:** the heads digest covers visible Branches only, so a
  removal moves the Store to **ahead** and a push carries it. A pull of an
  older copy brings the Branch back; that is the one-link model.
- **Name reuse:** see D1.

### 4.2 `project.delete` (renamed from `project.remove` on 2026-10-30)

- **A project alone in its own Store** (made by its own Init): unchanged.
  It deletes the volume and the home entry, so the space returns at once.
- **A project in a shared Store:**
  - **Today** it deletes the project's history rows in one write, and its
    packs stay.
  - **Proposed (D2):** one write sets `layer_stack.deleted_ms`. The project
    and its Branches leave every listing, and compact drops the rows and
    the data. Sandboxes are handled as today: `Busy` while one runs, unless
    `stopSandboxes`.
- **Name reuse:** stack IDs are random, so a deleted project's name is free
  at once. Its unique index becomes partial (`WHERE deleted_ms IS NULL`).

## 5. Prune: `branch.prune`

Like git's squash, prune keeps the final state and gives up the
intermediate steps. Unlike a squash, it keeps every Commit's record and ID.
A shallow clone is the closer model.

- **What it prunes:** Commits reachable from the Branch's head and from no
  other visible Branch head, except:
  1. the Branch head itself;
  2. any Layer's source Commit;
  3. fork points: Commits with two or more children, or that are another
     Branch's fork origin. Merge picks these as bases and needs their trees
     (`ancestry.rs::merge_base`);
  4. any Commit that is a merge's second parent.
- **Effect:**
  - One batched write sets `commit.pruned_ms` and saves a change summary
    per pruned Commit (D5): files added, changed and removed, and bytes.
  - The Commit stays in `list_commits` with `pruned: true`, with its ID,
    message, time, Branch and lineage.
  - Reading its files (`fs.*` at that Commit), `diff_commits` against it
    and mounting it fail with a new `Pruned` code.
  - Its root stops being a mark root (§6), so compact drops data that only
    pruned trees used.
- **Refusals:** `InvalidInput` without `confirm`, `NoSuchBranch`, and
  `MountsActive` while a Workspace is mounted at a Commit it would prune.
- **Prune does not change heads.** The digest covers the pruned set, so a
  prune moves the Store to ahead and a push carries it.
- **Later, not in this issue:** pruning main's old Layers before a cutoff,
  and a project-wide prune.

## 6. Compact: `store.compact`

Compact copies what remains into a new file and swaps it in. This matches
`git gc` repacking reachable objects, and the swap matches pull's install.
Nothing is updated or deleted in place, so the immutability triggers and
`read_immutable` stay valid and the schema does not change for compact.

### 6.1 Preconditions

- **Store closed.**
  - `Busy` while any Sandbox of any project in the Store runs, unless
    `stopSandboxes`. Stopping Sandboxes also ends every mount and every
    in-flight Save and read, which covers the roots outside the Store
    (§2.3).
  - The runtime closes its own clients of the Store.
  - On macOS and Windows the worker runs compact. It closes its handle of
    the Store first.
- **Writer fence.** Compact holds the writer turn and the directory lock for
  the whole run. That is declared here as an offline maintenance exception
  to the short-hold rule, since nothing else may write meanwhile.
  - Every writer checks at admission that the file it has open is still the
    file at the Store's path (inode match). If not, it fails `Replaced`
    (D3).
  - This closes the opener race that pull documents
    (`remote/pull.rs`), for pull as well.
- **No unfinished Init** (`init_operation` rows) and no merge scratch rows,
  else `Busy`.
- **Free space:** at least the current file size, checked before the run,
  as push does.
- **SQLite only.** A PostgreSQL Store refuses `Unsupported` (D4).

### 6.2 Mark

1. **History roots:**
   - visible stacks' Layers, all of them;
   - visible Branches' heads;
   - Layer source Commits.

   Kept history is everything those reach through parent, second parent and
   Layer source (`ancestry.rs` semantics). Commits reachable only from
   removed Branches or deleted stacks are dropped. Lineage rows (`runs`,
   `absorbed`, `reach`) are filtered to kept Commits. A run with no kept
   Commit is dropped, and the lineage test cases (§8) check that ancestry
   and merge-base answers are unchanged for every kept pair.
2. **Content roots:**
   - `root_id` of every kept, unpruned Commit;
   - `root_id` of every kept Layer.
3. **Content closure:**
   - From each root, follow `decode_references`
     (`layerfs-content/src/object/admission.rs`). This is the
     `SnapshotClosure` proposed under A1 on #411. Compact reuses it rather
     than adding a second walker.
   - Add physical dependencies: each delta record's base, transitively
     (`stored_base`), and the `inode_value` ordinals of live pooled leaves.
4. **Visited set:** the new file's own `object_location` table. It is
   indexed, on disk and insert-only, so memory does not grow with the Store
   and no second database is introduced. This answers A1's backing
   question.

### 6.3 Copy

Into `<store>.compact` in the Store's directory, with the same schema and
page size, Disposable profile:

- **History:** kept rows, unchanged, with the same IDs.
- **Packs:**
  - A pack whose records are all live is copied byte for byte, with its
    locations.
  - A pack with some live records has those records rewritten into new
    packs, keeping their encodings. Delta records keep their base IDs; the
    bases are live by construction.
  - A pack with no live records is not copied.
- **Lookup tables:**
  - `inode_value` rows for live ordinals;
  - `file_digest` rows whose root is live;
  - `content_signature` rows whose object is live;
  - `binding_hint` copied whole (small, Store-wide).
- **Policy and totals:** `store_policy` is copied with its next IDs.
  `store_totals` is recomputed from the copied locations, packs and kept
  history.

### 6.4 Verify, swap, report

- **Verify:** re-walk every kept root's closure in the new file. Every object
  must resolve and authenticate. Any failure deletes the new file and
  leaves the Store untouched.
- **Swap:** checkpoint and close, then rename the new file over the Store
  file, as pull installs. The old file is deleted.
- **Report:**
  `{ before: { fileBytes, pages }, after: { fileBytes, pages }, dropped: { branches, projects, commits, prunedTrees, objects, packs }, rewritten: { packs }, storedBytesFreed, millis }`.
  - Rows dropped, stored bytes dropped and file bytes are reported
    separately.
  - On macOS and Windows the Docker disk does not shrink on its own; the
    report says so.
- **`dryRun: true`** runs mark and copy into the scratch file, reports the
  same numbers, then deletes the file. These are exact figures, not upper
  bounds.
- **Remote link:** compact keeps visible heads, removals, deletes and prunes, so the
  digest is unchanged and the link stays in sync. A push uploads the smaller
  file (push does not refuse an in-sync Store).

## 7. Connect

- **Branch menu:** Remove… (typed name) and Prune history…, each showing
  what will be hidden.
- **Project menu:** Delete… (already present as remove; it calls
  `project.delete` since 2026-10-30).
- **Storage page:**
  - Compact… opens with a dry run: "Compact would free 94.6 MB (37.7%).
    Stop 2 running Sandboxes first."
  - A per-Branch cost column, "Frees if removed alone (up to)", from the
    history attribution in §2.2.
- **History:** pruned Commits show their change summary and "Files pruned".

## 8. Verification and measurement

1. **Unit tests:**
   - tombstones and partial unique indexes;
   - each refusal (`ReservedName`, `MountsActive`, `NoSuchBranch`,
     `Pruned`, `Busy`, `Replaced`, `Unsupported`);
   - the heads digest after remove, delete, prune and compact;
   - the prune selection rules on fork, merge and published-Layer graphs.
2. **Compact correctness**, on a fixture Store built to hold:
   - two projects sharing chunks;
   - a delta chain whose base sits in a deleted project and is used by a
     kept one;
   - a fork sharing Commits with a removed source;
   - a merge;
   - published Layers;
   - pruned Commits;
   - pooled inode leaves.

   After compact:
   - every kept Commit and Layer lists and reads byte-identically to before
     (recursive `fs.list`, and a hash of every file);
   - `merge_base` answers for every kept pair are unchanged;
   - removed Branches and deleted projects are gone;
   - pruned Commits answer `Pruned`;
   - the dry run's numbers equal the real run's.
3. **Failure tests:**
   - a writer with the old file open after the swap fails `Replaced`;
   - a failed verify leaves the Store untouched;
   - compact refuses while a Sandbox runs.
4. **Real Stores**, on copies only, never the owner's data:
   - on `r2-examples-clone` and `services`, run the §2.2 sequences;
   - record exact freed bytes against the upper bounds, the file size
     before and after, and compact's wall time.

   Measurement follows the agent measurement policy: Disposable, one sample
   per case, cold cache declared, fresh outputs, each command under 2
   minutes.
5. **#411's reporters** offered to run the same steps on their workloads on
   Linux and macOS native. Invite them once a build exists.

**Acceptance:**

- every read of every kept Commit and Layer is identical after compact;
- the dry run is exact;
- remove, delete and prune each take one short write;
- compact of `r2-examples-clone` with every Branch removed leaves a file
  within 2% of what the copied data, lookup rows and history require.

## 9. Owner decisions

| # | Question | Recommendation |
| --- | --- | --- |
| D1 | Reusing a removed Branch's name | **(a)** New Branches get an ID with a generation (hash of stack, name and generation), while existing Branches keep theirs. This is a format change. **(b)** Refuse reuse while the tombstone exists, which is forever if the Branch published Layers, because `layer.source_branch_id` keeps its row. Recommend (a). |
| D2 | `project.delete` in a shared Store hides instead of deleting rows | Yes: one short write, rows go at compact |
| D3 | Writers check the file is still the Store's (inode) at admission, failing `Replaced` | Yes; it also fixes pull's opener race |
| D4 | PostgreSQL compact | Later: it needs in-place batched deletes and changed triggers. Remove, delete and prune work on PostgreSQL now |
| D5 | What a pruned Commit keeps for History | Change counts and bytes; a full changed-path list can be large (`node_modules`) |
| D6 | Schema change for `removed_ms`, `deleted_ms` and `pruned_ms`, partial unique indexes and the change summary | A schema version bump with a one-time upgrade on open |
| D7 | Remove a Branch that another Branch forked from | Allowed (cadorn on #411 asked for a refusal; the fork keeps what it uses) |

## 10. Out of scope

- automatic or scheduled collection;
- in-place sweep with a GC epoch;
- PostgreSQL compaction;
- remote-side compaction (push the compacted file instead);
- pruning main's Layers;
- a Store-wide mount registry (the closed-Store precondition stands in for
  it).

## 11. Rules

- Disposable profile for every local Store. One attempt per operation, no
  retry or replay.
- Production LOC in every commit (`tools/production_loc.py`). New production
  files stay at or under 999 lines.
- Test commands at or under 120 s, with unfiltered output.
- Delete tests of replaced behaviour rather than adapt them.
- No new dependency.
