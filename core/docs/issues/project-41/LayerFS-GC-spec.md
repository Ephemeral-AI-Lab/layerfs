# LayerFS garbage collection: soft removal and Store compaction

> **Status:** **Frozen for layerfs-project#41 (LayerFS 2.0.0) on 2026-11-15**, approved by the owner on layerfs-project#42. An amendment needs a new #42 decision entry.
>
> **Baseline.** Commit `ed14ed53b` (LayerFS project main layer
> `32770a67…`) is the frozen **2.0 GC implementation baseline**. Implementation
> starts here; residual review items are tracked as implementation/acceptance
> gaps on layerfs-project#41, not as contract changes.

> **Authority.** This is the only GC design document in the tree. It replaces the
> six-file set merged by #658 (removed by #671; last text in the #677 branch).
> The contract it states is the frozen set of layerfs-project#42: S1–S11
> and items 1–42 (2026-11-13 to 2026-11-15). Items 43–44 are recorded on
> layerfs-project#42 and are not contract text. Where this file and an older
> document disagree, this file wins; a change to a contract sentence needs a new
> decision entry on #42 first.
>
> **Sign-off.** S1–S11, items 1–42 and the amendments below were approved by
> the owner on layerfs-project#42; this file is the frozen text. The decision
> table indexes every decision that carries contract text here; the complete
> log is layerfs-project#42.
>
> **Scope.** layerfs-project#41 and its sub-issues #42–#52. The durability
> profile (#155 §7) is outside this contract (#42 item 26). GC is developed
> and measured under the Disposable profile (#42 item 42).
>
> **Source.** Code references are to `layerfs_v2_0_0` at `e8fa0a7e00`; those
> added by items 27–42 were checked at `8cfb6e2734`.

## 1. Requirements (owner policy, #41)

- Four operations, on SQLite and PostgreSQL: `branch.remove(project, branch)`,
  `project.delete(project)`, `project.remove_file(project, path)` and
  `store.compact(name)`. Each takes only a resource identifier: no confirm,
  force, dry-run, stop-Sandboxes, Store, URL or volume option.
- The three removals are logical. They change visibility and retention and
  physically reclaim nothing. Even deleting the sole project keeps the Store
  and its volume addressable. No undo is promised.
- `store.compact(name)` alone physically reclaims rows and data that no
  retained reference needs. Shared content, delta bases and pooled
  dependencies survive.
- Existing immutable IDs never silently resolve to changed contents.
- Admission (§5): mounts in scope → `MountsActive`; other conflicting users →
  `Busy`. The caller prepares inactivity: no implicit unmount, no waiting, one
  attempt, no retry or replay; a lost reply stays uncertain.
- No automatic collection, no online sweep, no compaction of remote
  object-storage copies. PostgreSQL autovacuum stays on and is not counted.
- Report logical bytes discarded, backend-reusable space and physical shrink
  separately.

## 2. Decisions

| # | Decision | Section |
|---|---|---|
| S1 | Reachability is computed per Store, over every project in it | §6 |
| S2 | Exclusion reuses the Store lock, lease and op record; PostgreSQL adds advisory locks (#42 items 20, 33, 34) | §5 |
| S3 | Superseded by S5/§7–§8: the rebuild is its own module, not the transfer `Stage`; no transfer-intent sidecar (the rebuild artifacts of §7/§9 are separate) | §7, §8 |
| S4 | No object age; collection depends only on reachability | §6 |
| S5 | The unit that frees bytes is the pack. A rebuild copies only retained rows into a fresh Store; a published `pack` row is never updated or deleted in place | §7, §8 |
| S6 | A SQLite Store compacts on the machine where it lives (`place::plan`, `control/crates/layerfs-store/src/place.rs:40-49`) | §7 |
| S7 | Report the three byte definitions, peak temporary space and residue; estimates carry `estimate: true`; a capped report names its cap | §11 |
| S8 | One fence: `(dev, ino)` per transaction on SQLite, the incarnation join on PostgreSQL; both `StaleStore` | §10 |
| S9 | Install is binary; `live_heads_digest` matches before and after | §7, §8 |
| S10 | Logical removal never rewrites history or changes an ID | §4 |
| S11 | Compaction is exclusive against mounts and long-lived users; on PostgreSQL one-shot reads/writes may run until the swap, and a write that spans it makes the swap abandon; SQLite excludes all readers | §5 |
| 6 | Marker and position tables are added to 2.0 Stores at open by an additive step; 1.0 Stores are converted offline | §4.1 |
| 7 | Removal markers live in marker tables; `branch` and `layer_stack` stay append-only | §4 |
| 12a | PostgreSQL advisory key is `(kind, id)` over the Store, project and Branch scopes | §5.2 |
| 14 | Compaction collects history rows no retained root reaches; Commits of a removed Branch that nothing retains become `NoSuchCommit` | §6.1 |
| 15 | A name held by a removed Branch or deleted project returns `NameTaken` until compaction removes the row; then it is free. `NameRetired` is not added | §4.4 |
| 16 | `remove_file` hides a path across the project's history at once; 2.0 is visibility-only and frees no bytes for it (details: item 35) | §4.3, §6.2 |
| 17 | `store.compact` has two paths, `none` and `rebuild`; there is no light path. C3 has no caller in 2.0 | §3 |
| 18 | A leftover is judged by the Store generation; no `OrphanArtifact`, no leftover cap | §9 |
| 19 | API codes follow #41 (`MountsActive`, `Busy`); one `StaleStore` for both backends | §10 |
| 20 | PostgreSQL presence is a session advisory lock; the fence is the generation check carried in each statement (who holds presence: item 34) | §5.2 |
| 21 | PostgreSQL compaction builds in batches, then swaps two tables in one short transaction | §8 |
| 22 | SQLite compaction is its own module: the walk is fused with the copy, the visited set is a temp-file table, verification is cheap by default | §6.3, §7 |
| 23 | Removed: the `pinned` set, the per-transaction SQLite generation check, the capacity pre-check, the extra report fields | §6, §7, §11 |
| 26 | The durability profile is outside this contract | header |
| 27 | One visit level: anything reached by a logical or base edge is kept with all its references | §6.2 |
| 28 | `merged_from_id` and `displaced_id` are history edges | §6.1 |
| 29 | A retired Branch is collected as a unit; the history closure is a fixpoint | §6.1 |
| 30 | Compaction verifies with `live_heads_digest`; `heads_digest` is unchanged | §7, §8 |
| 31 | No views: filtered statements plus a guard test; PostgreSQL gets an additive step at open | §4.1, §4.2 |
| 32 | `project.remove` is retired; compaction reuses its removal statements | §4, §7, §8 |
| 33 | Admission at project and Branch scope (project op, intent record before bind, PostgreSQL keys at three scopes) | §5 |
| 34 | PostgreSQL presence locks only on long-lived sessions; SDK calls keep working through transaction poolers | §5.2 |
| 35 | `remove_file` marker with a Store history-serial frontier; every tree-building operation materializes the whiteout, not only mounts; a republished Layer keeps its source Commit's serial; object-ID reads not path-aware; 2.0 frees no bytes (visibility only) | §4.3 |
| 36 | No format change for the PostgreSQL mark until #52 measures it | §6.2 |
| 37 | All `inode_value_block` rows are kept in 2.0 | §6.4 |
| 38 | The rebuild clears `binding_hint_block` | §6.4 |
| 39 | All four calls are admin-profile and refused under a folder scope | §10 |
| 40 | Compaction writes its phase into the op; `store.status` shows it | §11 |
| 41 | A started rebuild completes | §3 |
| 42 | GC is developed and measured under the Disposable profile | header |

Out of this release: object age (Q3), repacking partly dead packs (reported,
not performed), in-place deletion of packs on either backend, a read-available
compaction window (A'), pruning (#46).

## 3. `store.compact(name)` paths

| Path | When | Work | Generation |
|---|---|---|---|
| `none` | Bytes in wholly dead packs are below **10%** of the Store's bytes | Mark only; nothing is written; the report gives the reclaimable bytes | unchanged |
| `rebuild` | 10% or more | SQLite §7, PostgreSQL §8 | `G → G+1` |

The numerator is `COALESCE(SUM(pack.length), 0)` over wholly dead packs and
the denominator `COALESCE(SUM(pack.length), 0)` over all packs, on both
backends; the test is `rebuild iff total > 0 AND dead * 10 >= total` (no
truncating integer division). The 10% value is frozen at 10%; a change needs a
#42 entry. #52 measures it. A Store with no packs, or packs summing to 0, is
`none` with `deadBytesBound = 0` and `zeroReason: "empty-store"`.
Below it a rebuild would copy at least nine live bytes for each byte freed.
The choice is made on the bound known after the metadata walk; a rebuild that
has started completes even if the refined share falls below 10%, and the
report gives both numbers as `deadBytesBound` and `deadBytesRefined` (§11;
#42 item 41).
There is no path that deletes rows in place: it would free no payload, touch
most index pages of the hash-keyed tables and need its own deletion contract.

## 4. Soft removal

All three removals write one marker row in one short write transaction. No
pack, object, Commit or Layer row changes, and no ID changes. They do not
bump the Store generation, but they do bump the project's `epoch` in
`project_epoch` (§5), which a concurrent writer checks before it commits.

| Call | Marker table | Effect |
|---|---|---|
| `branch.remove(project, branch)` | `branch_retired(branch_id BLOB NOT NULL PRIMARY KEY REFERENCES branch(branch_id) ON DELETE CASCADE, removed_ms INTEGER NOT NULL) STRICT` | Hidden from listings and name resolution; fork, merge, publish and mount refused; `main` → `ReservedName` |
| `project.delete(project)` | `layer_stack_retired(layer_stack_id, deleted_ms)` | Project hidden; its home entry is removed; its data stays until compaction; the Store stays addressable |
| `project.remove_file(project, path)` | `path_removed(layer_stack_id BLOB NOT NULL, path_key BLOB/bytea NOT NULL, path BLOB/bytea NOT NULL, frontier_serial INTEGER NOT NULL, removed_ms INTEGER NOT NULL) STRICT`, `PRIMARY KEY(layer_stack_id, path_key)`, `REFERENCES layer_stack ON DELETE CASCADE`; `path_key = BLAKE3(normalized path)` (a 4096-byte `bytea` key cannot fit a PG btree), the stored `path` is compared on collision; `frontier_serial` is the marker frontier (§4.3) | §4.3 |

`project.delete` is the only project removal: the hard-deleting
`project.remove` is removed from the call table (an old client that sends it
gets `InvalidInput` "no call named `project.remove`", never a silent hard
delete), and its module is deleted; compaction reuses its `removal_*`
statements (`engine/crates/layerfs-persistence/src/history/removal.rs`) to
drop a deleted project's history (#42 item 32). Tests that call
`project.remove` migrate to `project.delete`. Retirement also covers the
worker op/route for `project.remove`, the remote-SQLite route for
`project.delete`, the legacy own-project identity save (project name == Store
name) before the home entry is removed, the call-count and MCP-profile
generators, and the PostgreSQL path. The worker (the Store's home) is named
`project.delete`; the frame is `{store, stack_id, deleted_ms?}` (the caller
writes `deleted_ms` to the sidecar before the call and resends it on
retries). The worker saves the legacy own-project identity, writes the
marker, removes its own entry and replies
`{kind: "Removed", project, stack_id, deleted_ms}`; the caller removes only
its own entry and echoes `home_entry_removed`. A lost reply is `Unknown`,
keeps the sidecar, and the retry resolves retired/live/absent; the old
`project.remove` worker arm is deleted.

`project.delete` commits the Store marker first and removes the caller's home
entry after it. The marker is the commit point. If the home removal fails the
call returns success with `home_entry_removed: false` and the stored
`deleted_ms`; a retry from the same home is idempotent: it may read the
retired stack allow-listed, reuses `deleted_ms`, finishes the entry removal
and returns `home_entry_removed: true`. The durable delete intent
`{stack_id, deleted_ms, attempt_ms}` lives outside the removed entry (or the
entry is **not** renamed; one sidecar file at
`<home>/store/<store>/delete/<project>.json` with
`{store, project, stack_id, deleted_ms, attempt_ms, state}` is written
(temp + fsync + rename + dir fsync) after the identity save and before the
marker, with `pending|marked|done` updated atomically and unlinked after the
entry removal; the retry
re-reads the Store by the recorded stack id and compares it with the home
record, discarding a mismatch without touching a re-created project's entry;
a retired row reuses `deleted_ms` and finishes the entry; a live row with no
marker redoes the marker with the same `deleted_ms` and does **not** remove
the entry; an absent stack row finishes the entry. It never resolves a name. If the marker fails nothing is
removed. The legacy own-project identity save (project name == Store name)
runs before the marker transaction, is idempotent, and a failure refuses with
nothing removed. A different home's stale entry is not rewritten; the
`project.list`/`project.get` intersection in §4.2 hides it.

### 4.1 Representation

The marker and intent tables are added at open by an additive step under
the current schema identity (#42 item 6). On SQLite this is one more `add_*`
step, as `add_file_digests` was; the existing steps, the table list and
`tools/store_blocks.py` are updated with it. PostgreSQL has no such step today
(its open check requires the exact table set and recorded definition,
`backend/postgresql/schema.rs:158-252`), so new Stores get the tables in
`schema.sql` and an additive step at open creates them and rewrites the
recorded definition for older Stores (#42 item 31). 1.0 Stores are converted
offline. The same step adds the position state: `history_meta.next_history_serial`
(the next serial to assign; never lowered), a `history_serial` column on
`commit` and `layer` (a Layer that republishes a Commit carries that
Commit's serial; every other Commit and Layer takes the next value in its
creation transaction), `project_epoch(layer_stack_id PRIMARY KEY, epoch)`
(one row created with the project, bumped in the same transaction as every
marker write, copied by a rebuild, collected with the project), and two
admission tables: `gc_intent(...)` (§5.1) and `gc_op(...)`. The same step
adds `store_policy.trace_salt` last, written as 32 random bytes by the
Store-creation INSERT (`randomblob(32)`/OS RNG) and backfilled for existing
rows; `store_policy` copies use an explicit column list. `gc_intent` is
the shared Store-side admission table (created empty, never copied); `gc_op`
is PostgreSQL's single compaction row (created by PG's additive step,
untouched by the swap, cleared on settle) with columns `generation`,
`phase`, `scratch_schema`, `mark_serial`, `mark_pack_id`, `mark_digest`,
`mark_totals`, `mark_stamp` (`MAX(content_signature.stamp)`), `started_ms`,
`dead_canonical`, `dead_stored`, `dead_metadata`,
`dead_logical`, `totals_applied`, `artifact`, `pid`, `backend_start`,
`incarnation`. The `mark_*` columns are the first-statement tuple; the raw
head rows are hashed by the client. A GC-only `gc_dead(kind, id)` table (same additive step, never
copied, truncated on settle) carries the full dead set; a single `gc_op` row
cannot. SQLite keeps its phase in the
home/sidecar op and never writes the sealed DB, relying on the deterministic
artifact names for cross-machine recovery. The backfill runs on **every**
Store the step upgrades, including an existing 2.0 Store: Commits first by
`(created_ms, stack, id)`; then per Stack the genesis Layer first, a
republishing Layer takes its source Commit's serial, the rest by
`(source Commit created_ms, stack, id)`; `project_epoch(layer_stack_id, 0)`
for every existing stack; and
`next_history_serial = MAX(existing next_history_serial,
COALESCE(MAX(history_serial), 0) + 1)`; a Store with no stack/history keeps
its existing value. `branch` and `layer_stack` keep
their full unique name indexes.

### 4.2 Visibility filter

No views: views count as schema on both backends. Every statement that lists
or resolves a Branch or project carries a `NOT EXISTS` filter against the
marker tables, and a guard test fails on any statement that reads `branch` or
`layer_stack` without it unless the statement is on an allow-list: the
name-collision check (it must read the retired holder to return `NameTaken`
with its state), the fork/merge identity checks, compaction's mark/rebuild
statements, the `project.delete` retry, and two named by-id statements:
`branch_branch_by_id_any` (render/identity for `layer.list`/`layer.get`/
`saved`/operator inspect) and `layerstack_stack_by_id_retired` (the retry's
live/absent read by the caller's recorded stack id). The ordinary
`branch_branch_by_id`/`layerstack_stack_by_id` stay filtered. Every
other statement that reads `branch` or `layer_stack` carries the filter; a new
allow-list entry needs a comment and a test. A statement that bypasses the filter would resurrect a removed Branch. The
minimum is the Branch and project listings and name lookups
(`branch_snapshot`, `branch_snapshot_named`, `layerstack_stack_by_name`),
fork, merge, publish and mount resolution, and the project listing
`layerfs_project::store::projects`. That listing also drives
`identity::list_projects` and, since #678, the record rebuild in
`backup.restore` (`control/crates/layerfs-store/src/backup/restore.rs`), so an
unfiltered listing would write records for deleted projects back into a
home. `project.list` and `project.get` intersect the home entries with this
filtered Store listing before paging: a stale entry (a different home, or a
failed home-entry removal) is absent from `project.list` and answers
`NoSuchProject` from `project.get`. The join is on the stack id recorded in
the home record: `manifest.stack` for a local SQLite record,
`record.stack` for PG/worker records; a missing stack is filled from the
filtered listing by name. The all-Stores listing omits an unreadable Store;
`project.list(store)` propagates `NoSuchStore`/`Busy`/`Unknown`. a Store that cannot be listed is omitted by `project.list`
and returns its own `NoSuchStore`/`Busy` from `project.get`, never the
unfiltered record. A hidden project is `NoSuchProject` for
the four calls, except `project.delete`'s same-home retry, which resolves the
caller's record by stack id and allow-lists its retired stack.

### 4.3 `remove_file`

- The marker names a path in one project and a **frontier**: the current
  Store high-water, `next_history_serial - 1`, read under the Store write
  lane in the same transaction as the marker; a node created concurrently is
  ordered by its serial against that value, never by a later read. The first project's genesis Layer takes serial 1, so its first frontier
  is 1; a Store with no Stack has none and `remove_file` is `NoSuchProject`. Every
  Commit gets the next Store serial; a Layer that republishes a Commit
  carries that Commit's serial, so `branch.publish` does not move a tree's
  position, and every other Layer gets the next serial. A Commit or Layer is
  *at or below* the frontier when its serial is at most the frontier; the
  serial also covers Commits on unmerged Branches (#42 item 35). It applies
  to the Commits and Layers at or below the frontier, before and after any
  compaction:
  - a read whose resolved tree actually contains the path (or a hidden
    directory ancestor for an existing descendant) returns `Removed`; an
    at-or-below node without the entry is `NotFound`/absent, and a
    never-existing path hides nothing;
  - a listing omits the entry and reports one top-level `removed_count` over
    the queried subtree (present at 0, filtered before paging, no page slot or
    admission cost);
  - a mount of a Branch whose effective tree (its head Commit, else its base
    Layer) is at or below the frontier applies a whiteout for the path, so
    the next Commit records a real deletion.
- **Every operation that builds a new tree from a node at or below the
  frontier materializes the whiteout**: a workspace Commit, a
  non-fast-forward `branch.merge`, every one-shot tree creator (`file.write`,
  `edit`, `apply`, `rm`, `mkdir`, `mv`, `cp`), and any server-side
  construction. This tree/whiteout walk is mandatory in 2.0 and
  distinct from the byte-freeing GC walk (§6.2). A fast-forward merge creates
  no tree and is covered by the node it points at. `branch.publish` of a
  Commit at or below the frontier republishes that tree as a Layer at the
  same position: the marker hides the path and a mount of the Branch that
  carries it whiteouts, so publish needs no whiteout of its own. A merge that
  cannot materialize the whiteout is refused `Removed` with
  `detail {paths: [...]}`.
- A path explicitly re-created after the frontier is not a whiteout: the
  marker hides only content inherited from nodes at or below the frontier.
  In a merge of a head at or below the frontier with a head whose path was
  re-created after it, the re-creation wins; the whiteout removes the
  inherited side and suppresses any conflict from that side. An ordinary edit
  conflict is reported only when both sides are post-frontier (the marker does
  not apply) or on other paths.
- One marker per project and path: a repeat `remove_file` writes the current
  Store high-water (`next_history_serial - 1`) as the new frontier, so
  everything at or below is hidden again. It is `UpToDate` iff no project
  node has a serial greater than **this path's** marker frontier (a missing
  marker is -1); a commit in another project changes
  `next_history_serial - 1` but neither rewrites the marker nor advances its
  frontier. A path that never existed is accepted and
  hides nothing. The newest frontier subsumes the older ones.
- Every marker write that advances the frontier bumps the project's `epoch`
  in the same transaction; an `UpToDate` reply does not.
- Commits after the frontier never inherit the path; creating a file at the
  same path later is an ordinary visible write.
- A path is **hidden at node N** only when the entry exists at N and the
  marker's frontier (the maximum over the path and its `/`-boundary
  ancestors) is at least N's `history_serial`; a marked-but-absent entry is
  absent/`NotFound`, and a marker on a never-existing path hides nothing.
- Reads by object ID are not path-aware: bytes while retained, then
  `NotFound` once compaction collects it, never different bytes.
- The result is `{kind: "Removed", project, path, frontier_serial,
  removed_ms}` or `{kind: "UpToDate", project, path, frontier_serial,
  removed_ms}` (the stored values). Every refusal carries one detail shape:
  `{paths: [{path, frontier_serial}]}` (one element for read/stat; the
  governing maximum frontier and the request path). `file.read`/`file.stat`
  refuse `Removed` with it. `file.list`/`file.glob` omit hidden entries and
  report one top-level `removed_count` over the queried subtree (a hidden
  target path is `Removed`; a marked-but-absent path is not counted).
  `file.diff` has three states per side (visible/hidden/absent); hidden
  sides never refuse: left visible + right hidden → `changeKind` `Removed`;
  left hidden + right visible → `Added`; both hidden → `Removed` with both
  side objects; a hidden side object is
  `{removed: true, path, frontier_serial}`. A hidden side beats
  `allow_missing`; only a genuinely absent side is `NoSuchPath` (with
  `allow_missing: false`). `branch.diff`/`commit.diff` apply the same
  per-side rule to their historical trees, so a removed path is absent, not a
  spurious change. `file.revision` refuses `Removed`
  when the anchor is hidden; otherwise it walks and **stops** at the first
  hidden at-or-below version (never skips past it), returning only visible
  versions. `file.blame` stops at the hidden version and attributes the
  remaining lines at the tip. `file.grep`/`file.blame` omit hidden
  hits. `project.export` omits it; a cp-from source is `Removed`; `file.edit`
  is `Removed`; `file.mv` from a hidden path is `NotFound` and to a hidden
  path is a visible re-creation; `mkdir`/`cp`/`apply` follow the same
  construction rule; `file.rm` of a hidden path is `NotFound`; a
  post-frontier write is the visible re-creation.
- The call accepts an absolute POSIX path and converts it to the Store's
  normalized relative components (strip one leading `/`; `/a` and `a` are the
  same). It refuses `InvalidInput` for empty, `/`, `//a`, `a//b`, `a/`, `./a`,
  `a/./b`, `.`/`..` components, `\`, NUL, >4096 bytes or >256 components. A
  directory argument hides its subtree only at a `/` boundary
  (`p = m OR p begins m ++ b'/'`, computed as bytes in Rust, never `||` or
  `LIKE` on a BLOB). A child of a removed **file** is `NotFound`; a read of
  an existing descendant under a hidden directory is `Removed` with the
  governing maximum frontier over the path and its ancestors (using
  `history_serial`); an absent descendant is `NotFound`. An intermediate symlink component is `InvalidInput` (not followed);
  a symlink leaf is hidden as an entry. Other hardlink names stay visible and
  nothing is freed.
- History rows and IDs are unchanged. The trace key is BLAKE3(per-Store salt,
  layer_stack_id, normalized relative path); the salt is a 32-byte
  `store_policy.trace_salt` (random per Store, created by the additive step,
  backfilled for converted Stores, copied by a rebuild). Every path argument
  (`path`, `left_path`, `right_path`, `from_path`, `to_path`) is redacted
  per call: the log records the normalized length as `<N bytes>`, never the
  text; the daemon (native/worker) or the PG server-side call writes the keyed
  hash, and the path argument names (`path`, `left_path`, `right_path`,
  `from_path`, `to_path`, `path_prefix`, `pattern`) are in the per-call
  redaction set.
  Refusal details/messages are not traced; the `Removed` detail echoes the
  request path, and raw paths never enter traces.
- The call frees no bytes in 2.0 (visibility only, §6.2); the path is scoped
  to one project, so bytes reached elsewhere or as a live delta base stay.
- Physical removal, a path-aware GC walk and a guaranteed purge are follow-ups
  that amend this contract before they ship.

### 4.4 Names

`BranchId::named = blake3(stack ‖ name)`
(`core/crates/layerfs-history/src/contract/identity.rs:266-275`) assumes a
name is never reused while its old rows exist. So:

- A create or fork whose name a live row holds returns `NameTaken` (existing:
  `engine/crates/layerfs-store-service/src/history/branch.rs:232,397`) with
  `detail {holder: {kind: "branch"|"project", name, state: "live"},
  source: "store"}`. The detail rides
  `HistoryError::NameTaken(Box<NameTakenState>)` (Holder/Source defined in
  `layerfs-history`, boxed like `HeadMoved`); `lookup.rs` has an explicit arm
  that projects `{holder, source}`. `source` is `store` only; a home-record
  collision is a service `refuse("NameTaken", ..., {holder, source:"home"})`.
  The `branch.rs:230-234,395-399` Null pre-checks are deleted, so every site
  reaches the typed error. The branch query joins `branch_retired`; the
  project query joins `layer_stack_retired`.
- A name held by a removed Branch or deleted project also returns
  `NameTaken`, with `detail {holder: {kind: "branch"|"project", name,
  state: "live"|"retired"|"deleted"}, source: "store"}`; `source: "home"` is
  used only for a project's home record. The `BranchId::named` identity check
  and the name-collision statements both return this detail; the
  `_ => "Failed"` arm is not allowed to swallow them.
- `branch_retired(branch_id BLOB NOT NULL PRIMARY KEY REFERENCES
  branch(branch_id) ON DELETE CASCADE, removed_ms INTEGER NOT NULL) STRICT`
  (same for `layer_stack_retired`), so a second remove cannot add a second
  marker and a
  collected base row takes its marker with it, on both backends. The write is
  `INSERT ... ON CONFLICT(branch_id) DO NOTHING` with an affected-row check;
  only an insert bumps `epoch`. A concurrent remove is `Busy` (phase
  `"retire"`, no op), a repeat is `NoSuchBranch`; `project.delete`'s repeat is
  the idempotent success in §4.
- `branch.fork` disambiguates by syntax: `main`, a valid HistoryName (a
  Branch or `NoSuchBranch`, checked before `args::node`), or 66-hex (a Commit
  or a Layer; a missing Layer is `NotFound`, a collected Commit
  `NoSuchCommit`). A valid name is never retried as a node id; a Commit fork
  keeps the retired row/name through `forked_from_id` and `commit.branch_id`,
  and a Layer fork through `forked_from_id` and `layer.source_branch_id`.
  A mount of a retired Branch is `NoSuchBranch`. For `merge`, a retired
  **target** name is `NameTaken` while a retired source is `NoSuchBranch`; for
  `publish`, a retired Branch is `NoSuchBranch` and a collected node is
  `NoSuchCommit`.
- Compaction removes a retired `branch` or `layer_stack` row once no retained
  row references it (`layer.source_branch_id`, `commit.branch_id` and the
  other history edges, `engine/crates/layerfs-persistence/sql/sqlite/history.sql:62-65`).
  A Branch published by a retained Layer is pinned by that foreign key, and a
  Branch named by a retained `commit.branch_id` is kept by the history
  closure; only a Branch with neither reference is freed.
  The name is then free, and no retained row carries the old ID. The comment
  on `BranchId::named` ("never reused") is updated by #44 to say "never
  reused while its rows exist".
- A Branch whose old Commits stay retained (for example through a live
  Layer's `source_commit_id`) keeps its row, so its name stays taken.

## 5. Admission

| Operation | Refused while | Code |
|---|---|---|
| `branch.remove` | a mount or Sandbox of that Branch, an unfinished Init/merge row names it, or another GC op at Branch scope | `MountsActive`, `Busy` |
| `project.delete`, `project.remove_file` | any mount of the project, another user of the project, an unfinished Init/merge row names it, or another GC op at project scope | `MountsActive`, `Busy` |
| `store.compact` | any mount, long-lived client or operation on the Store, or unfinished Init/merge rows | `MountsActive`, `Busy` |

Mounts take a Branch; there is no mount of a historical Commit. All four
calls resolve their resource through the filtered statements (§4.2); a hidden
Branch is `NoSuchBranch`, a hidden project `NoSuchProject`, and a collected
Commit `NoSuchCommit` (the latter only on merge/publish), never a second
marker. The one exception is `project.delete`'s same-home retry, which
resolves the caller's recorded stack id under §4.2.

`branch.merge` and `branch.publish` take no mount and are not in the table. A
non-fast-forward merge materializes the `remove_file` whiteout in its result
(§4.3) or is refused `Removed`; a publish of a Commit at or below the frontier
yields a Layer at the same position, which the marker hides.

Every operation that creates a Commit or Layer, or writes under a project,
reads `project_epoch.epoch` at start and re-checks it before commit:
- SQLite: inside the one `BEGIN IMMEDIATE` transaction that also writes the
  Commit/Layer, so a removal cannot interleave.
- PostgreSQL: the writer's last statement before commit is
  `SELECT epoch FROM project_epoch WHERE layer_stack_id = $1 FOR SHARE`
  (it blocks the marker's non-key `UPDATE`); a plain unlocked read is not
  enough under READ COMMITTED. The marker transaction itself runs
  `UPDATE project_epoch SET epoch = epoch + 1`, then reads the frontier
  `next_history_serial - 1`, then inserts the marker. A missing epoch row
  (post-cleanup project delete) is `Conflict`, never "unchanged".

### 5.1 SQLite

Admission is one Store-DB table and two `BEGIN IMMEDIATE` transactions
(SQLite 3.53.2 `lang_transaction`; a locked writer is `Busy`).

`gc_intent(op_id PRIMARY KEY, kind, scope_kind, scope_id, workspace, sandbox,
machine, project, branch, owner_runtime, owner_boot, state, uncertain, phase,
generation, artifact, high_water, evidence, updated_ms)`, `STRICT`, with
`UNIQUE(scope_kind, scope_id, owner_boot)`; `kind` is `mount`, `init`,
`merge`, `removal` or `compaction`, `state` is `live`, `uncertain` or `done`,
`scope_kind` is `store`/`project`/`branch`, and the ids are typed per kind
(17-byte project/Branch, 33-byte commit/layer, 32-byte catalog id). The
migration step that adds it is the same additive step as §4.1.

- A **mount** transaction checks that no operation op is running at every
  scope it names (Store, project, Branch), inserts its intent and commits. A
  **removal/compaction** transaction scans the intents in its scope, writes
  its marker or op row, and commits. A live mount → `MountsActive`; a live
  operation → `Busy`; a provably dead, uncertain op → `Unresolved`; a
  provably dead, certain op is adopted or ignored per its `kind`.
- Compaction writes its `gc_intent` operation row **before** its first
  checkpoint; the row lives in `<store>.compact` during build/verify, moves
  to the live Store after install, and is cleared at settle. `none` writes
  and clears the same row (its only write); "nothing is written" in §3 means
  no pack/object/history/generation change.
- Intent lifetime: the row is deleted in the `workspace.unmount`
  transaction, on `sandbox.close`, daemon exit, machine stop, and in the
  mount's error path if bind fails after the intent commit. A failed delete
  is reported; a provably dead owner's row is ignored, never trusted.
- `machine_stopped` is provider evidence written into the intent/op; a live
  but unreachable Sandbox is `Busy`; a removed machine record alone is not
  proof.
- **Project scope:** `project.delete`/`remove_file` take a project op and
  scan the intents naming the project. A mount is excluded by its own
  intent, so several mounts of one project proceed. `project.delete` holds
  its op through the marker transaction only; the retired-row entry-removal
  retry needs no op, the live-row retry takes one and re-scans.
- **Branch scope:** the intent carries the Branch; a removal checks the
  intents and writes its marker in the same transaction, so a mount and a
  removal can never both succeed; a bind after the marker is refused by the
  filtered statements (§4.2).
- `Unresolved` carries `{entity: {kind, name}, phase, op}`; a non-owning call
  observes it, the owning call adopts/clears it. A `kind`-by-owner-state
  matrix governs {mount, removal, compaction} × {live, unreachable,
  dead-certain, dead-uncertain}.

### 5.2 PostgreSQL

Leases are files in one home and cannot see another machine. So (#42 items
33, 34):

- **Presence.** Advisory keys at three scopes, `key(kind, id) =
  blake3("layerfs/advisory/v1\0" ‖ catalog_id ‖ kind ‖ id)[..8]` with kind 1
  the Store (32-byte `catalog_id`), 2 a project (17) and 3 a Branch (17); the
  8-byte truncation can cause a false `Busy` or a misclassified holder, never
  a missed exclusion. A mount holds shared kind 1/2/3 on one dedicated
  session for the daemon's life, refcounted per `(session, key)` and acquired
  all-or-none with `pg_try_advisory_lock[_shared]`; a failed acquisition
  unlocks what it took. Compaction holds only the exclusive kind 1 key, taken
  before Phase M and held through Phase C/settle on that same pinned session.
- **Removals** take transaction-scoped locks in the marker transaction:
  `branch.remove` = shared kind 1 + shared kind 2 + exclusive kind 3;
  `project.delete`/`remove_file` = shared kind 1 + exclusive kind 2; order
  Store → project → Branch. A failed shared attempt is `MountsActive`, a
  failed exclusive attempt `Busy`; `pg_locks.mode` names the holder.
- **One-shot SDK calls** hold no presence lock. They rely on the
  `project_epoch` `FOR SHARE` check (§5), the fence below, and the swap's
  `ACCESS EXCLUSIVE NOWAIT` (§8). The four GC calls are not one-shot.
- **Fence.** The generation check rides in the statements a client already
  sends: in the `BEGIN` batch of a transaction, and as a joined column of
  `history_meta.catalog_incarnation` in reads without a transaction.
  No extra round trip. A mismatch is `StaleStore`.
- **Connections.** Mounts and compaction need a direct or session-pooled
  connection; transaction pooling cannot hold a session lock and is
  `Unsupported` for them. One-shot SDK calls keep working through transaction
  poolers.
- **Dead clients.** A client whose TCP connection died holds its lock until
  the server notices. The guide documents `tcp_keepalives_idle`,
  `client_connection_check_interval` and `pg_terminate_backend`; a GC call's
  lost reply is resolved on a second connection before the exclusive key is
  taken.

## 6. Mark

Inside the exclusive window, at the frozen generation `G`. No caching across
operations; a missing locator or corrupt dependency aborts with no writes.

### 6.1 History closure

A recursive query from the retained roots over the history edges:

- Branch → `head_commit_id`, `base_layer_id`, `forked_from_id` (followed for
  every retained Branch; the **roots** are every non-retired Stack and every
  Branch with no own or stack marker)
- Stack (not retired) → `head_layer_id`, published Layers
- Commit → `parent_commit_id`, `second_parent_id`, `base_layer_id`,
  `merged_from_id`, `displaced_id`, `branch_id`
- Layer → `parent_layer_id`, `source_commit_id`, `source_branch_id`

`merged_from_id` and `displaced_id` are followed because doctor requires
their targets (`diagnostic/history.rs:40,146`; #42 item 28). A retired Branch
is collected as a unit: if a retained row references it (`commit.branch_id`,
`layer.source_branch_id`), its row stays and so do `head_commit_id`,
`base_layer_id` and `forked_from_id`, and the head commit enters the Commit
edge set above; the closure is the fixpoint of these rules (#42 item 29). A
Layer's `source_branch_id` is a foreign key, so a published Branch and its
head chain stay retained while the Layer lives; that Branch keeps its name
(`NameTaken`) and compaction does not free it. "Free after compaction" (§4.4)
applies only to a Branch that nothing retains: no retained
`layer.source_branch_id` and no retained `commit.branch_id` (a fork's
`forked_from_id` is such a Commit). Every history edge is
stack-local, so a live stack keeps all its Layers and only Commits and
retired Branch rows can go; a deleted project goes whole. A fork keeps its source's history up to the fork point.
`store.history.branches` validates every retained `commit.branch_id`: it must
name a Branch in the same stack (SQLite cannot add the FK, so this is a doctor
edge). History rows outside the closure are collected by the rebuild (§7, §8);
this
is required because the full doctor qualifies every Commit and Layer root it
finds (`engine/crates/layerfs-persistence/src/diagnostic/history.rs:155-198`).

### 6.2 Object closure

- Start from the retained Commits' and Layers' roots.
- Follow logical references with `FinalizedObject::admit(..).references()`
  (`core/crates/layerfs-content/src/object/output.rs:121,212`) and delta bases
  with `ChainBases::base_of` (`core/crates/layerfs-storage/src/encoding/delta/read.rs`).
  Do not reuse `qualify`: it skips file mappings, symlink targets and
  attribute trees and ignores delta bases.
- Transitive closure (#42 item 27): each object is admitted once; admitting
  it enqueues its logical references (`references()`) and its delta base
  (`base_of`), and the queue runs to empty. An object reached only as a base
  still keeps all of its references, because the full doctor checks the
  references of every stored object (`diagnostic/scan.rs:129-224`). Objects
  kept only because they are bases are reported in `keptWaste`.
- `remove_file` is **visibility-only in 2.0.0**: the marker hides the path in
  reads, listings, mounts, merges and publishes, and compaction reclaims
  nothing that only the removed path reached. A retained root's inode page
  must stay (qualification refuses a missing inode,
  `filesystem/qualify/walk.rs:183`), and rewriting the tree would change the
  root and the Commit/Layer id (S10). A path-aware walk that frees bytes, and
  per-entry page pruning, are a follow-up; `keptWaste` has no page component
  in 2.0.
- On PostgreSQL the base edges live inside pack groups, so the mark (§8
  Phase M) reads that payload. A remote caller transfers it; `markBytes` is
  the distinct live-pack body bytes read once, and #52 measures them. The
  table copy moves no pack bytes. 2.0 keeps the format unchanged; adopting a
  nullable `object_location.base_id` is a separate #52 decision with a schema
  version and a defined NULL/backfilled distinction, not a runtime switch
  (#42 item 36).
- Window by window, sort lookups by object ID before locating them.
- Whole files and chunks have no logical references; only their delta base
  is needed. Reading it still loads and authenticates the whole group
  (`delta/read.rs:561-571`); on SQLite the payload part of the walk runs
  inside the copy (§7 step 4), and the PostgreSQL mark reads it in Phase M.

### 6.3 Visited set

A scratch table `visited(object_id PRIMARY KEY, pack_id) WITHOUT ROWID` in a
named scratch database beside the Store (`<store>.compact-visited`), attached
to the compaction connection: it is in the op/artifact set and in
`peakTempBytes`. Its own journal (`<store>.compact-visited-journal`) and any
multi-database super-journal (`-mj*`) are in the artifact set too; or the
visited DB runs `journal_mode=OFF` and the copy is not atomic with it. Session
connections keep `temp_store=2`
(`engine/crates/layerfs-persistence/src/backend/sqlite/session/profile.rs:88`);
both paths spill the closure to `<store>.compact-visited` and choose the
path from the refined live set; the `none` path deletes the scratch before
replying, so it leaves no residue and never answers `Capacity` for a cap. On PostgreSQL it is a temporary table.

`kept_node(kind, id)` is the history side of the same walk: `kind` is
`commit`, `layer`, `branch` or `stack`, and it holds the §6.1 closure. A
rebuild copies `layer_lineage.layer_id ∈ K_layer`, `commit_lineage.commit_id
∈ K_commit`, `absorbed.run ∈ R`, `reach.key ∈ K_stack ∪ R` (R := runs of kept
`commit_lineage` rows), and the history/catalog rows by `kept_node`; it
deletes the complement, and a deleted project goes whole.

### 6.4 Decision

- `live packs` = packs holding at least one live object; the rest are dead.
- `none` or `rebuild` from §3, using the bytes of packs that hold no live
  object after the metadata walk (an upper bound that §7 step 4 refines).
- Dead rows: `object_location`, `content_signature`, `file_digest` (keep a
  row only if its root is in `visited`; a digest or signature root is not a
  mark root, so an uncommitted saved file's row is dead), history rows
  outside §6.1, retired `branch`/`layer_stack` rows (§4.4), a `branch_retired` row whose
  branch row is collected, a `layer_stack_retired` row whose stack is
  collected, and `path_removed`/`project_epoch` rows of a deleted project (a
  project goes whole). A live project's markers stay: `path_removed` is one
  per path, and a serial allocated later never falls below a frontier.
- Kept in 2.0: every `inode_value_block` row; the ordinals of pooled leaves
  are private to core, and the blocks are small. The report gives their bytes
  (#42 item 37).
- Cleared: `binding_hint_block`. The hints are advisory and not per project,
  and they hold the file names of deleted projects and removed paths (#42
  item 38). Clearing all of them costs only advisory lookup speed, never
  correctness; a later Save repopulates hints for live paths. A per-project
  clear is a follow-up, not 2.0.0.

`live_heads_digest` (both backends) is BLAKE3 over
`"layerfs/live-heads/v1\0"`, then the non-retired stacks in id order as
`(stack_id, head_layer_id)`, then the Branches with no own or stack marker in
id order as
`(branch_id, head_commit_id or 33 zero bytes, base_layer_id)`, each
fixed-width. Commit ids are 33 bytes (tag `0x12` + 32), Layer ids 33 (tag
`0x32`), Branch and stack ids 17; a golden vector pins the bytes. A retired
row is absent, not zeroed; timestamps are not hashed, so SQLite and
PostgreSQL agree.

## 7. SQLite rebuild

Runs as one operation on the Store's machine (S6).

1. Take the persistence writer guard and hold it through step 7's rename.
   Checkpoint the source with `PRAGMA wal_checkpoint(TRUNCATE)` and require
   `busy == 0` (never unlink a WAL file); checked-close the last source
   session and check for `-wal`/`-shm` while the guard is held; pass the
   guard into the seal (no second acquire); use
   `seal_holding() -> (size, mtime, db-header change counter, Writer)`
   captured after the sidecar check and before release. The current
   `Session::seal` releases the lane, so a returned Writer is required; there
   is no re-acquire alternative. A busy checkpoint is
   `Busy`. Record the sealed `(size, mtime, db-header change counter)`.
1.5 After the seal, `ATTACH` the old file `immutable=1` (it stays
   byte-identical), create `<store>.compact-visited`, run the complete
   §6.1+§6.2 closure into it, and choose `none`/`rebuild` from the refined
   live set. On `none`, delete the scratch and stop: no `.compact`, no
   `.compact-visited`, no residue. On `rebuild`, keep the scratch and reuse
   `visited` in step 4; step 2 creates the new file only on this path.
2. If the path is `rebuild`, create `<store>.compact` beside the Store with a
   raw connection: attach the sealed Store `immutable=1` as `src`, give the new
   file the source's `page_size`, `auto_vacuum`, `user_version` and
   `application_id`, and copy the schema objects verbatim from
   `src.sqlite_master` (`type, name, sql`, tables first) so the recorded
   `schema_source`/`schema_definition` stay byte-identical. The bulk load runs
   with `journal_mode=OFF`, `synchronous=OFF`, `foreign_keys=OFF`,
   `temp_store=MEMORY` and `cache_size=-262144` (256 MiB). The new file has the
   source's `(layout, acquisition)` because its schema is the source's:
   `Monolithic` has `pack.body` and no `pack_unit`; `GroupRows` has
   `pack_unit`; `GroupRowsIndexed` also has its index; the acquisition tables
   exist only for a Store that recorded them. `schema_source`/`schema_definition`
   must be those of that pair, or step 6's `validate` refuses. The session
   defaults are a 2 MiB cache and no mmap (`session/profile.rs:86-87`).
3. The old file is already attached `immutable=1` in step 1.5; it stays
   byte-identical.
4. One `journal_mode=OFF` transaction copies the retained rows in foreign-key
   order; `foreign_keys=OFF` and step 6's `foreign_key_check` are the guard
   (D1). The T7.2 mark already resolved every logical reference and delta base,
   so `visited` is the whole retained object set and `live_pack` the whole
   retained pack set: the build copies exactly those, with no second payload
   sweep.
   - `pack` rows whose `pack_id` is in `live_pack`, whole (pack-as-unit, body
     and all), then `pack_unit` for the same packs; a `Monolithic` Store has no
     `pack_unit`.
   - `object_location` whose `object_id` is in `visited`.
   - `content_signature` whose object is in `visited`; `file_digest` rows whose
     root is in `visited`; all `inode_value_block` rows.
   - History and lineage copied by `kept_node`: `layer_stack`, `layer`,
     `"commit"`, `branch`, `layer_lineage`, `commit_lineage`, `absorbed` by
     `run`, and `reach` by its kind-mapped key; the marker tables
     (`branch_retired`, `layer_stack_retired`, `path_removed`) and
     `project_epoch` for the rows the closure keeps.
   - `kept_node` itself (the settle clears it after the install),
     `scope_allocator` and `store_policy` verbatim (allocators and salt,
     `INSERT ... SELECT *`), and this run's `gc_intent` row only.
   - `sqlite_sequence` when the source has it (the acquisition operation
     counter never falls, §4.1); transient `init_*`/acquisition working rows,
     `merge_operation`, `merge_record`, `gc_op`, `gc_dead` and the dead packs
     are **not** copied, and a live Init op is admitted as `Busy`.
   - `binding_hint_block` stays empty; `history_meta` keeps
     `next_history_serial` and records `catalog_incarnation = G + 1`.
   - Assert the source's `store_totals.logical` equals
     `SUM(commit.logical) + SUM(layer.logical)` before the copy, copy
     `store_totals` as a whole row (keeps `init_*`), then one UPDATE sets
     `logical` to the triggers' value and `canonical`/`stored`/`metadata` to
     the new sums.
   - After the bulk load, create the secondary indexes and the
     immutability/logical triggers (the load then fires no trigger and writes
     no index page), and check the copied `schema_definition` against the file's
     own definition.
5. Switch the new file to WAL on the same raw connection after detaching
   `src` and `scratch` (the mode switch writes no rollback journal under
   `journal_mode=OFF`), set the source's `synchronous`, then checkpoint with
   `wal_checkpoint(TRUNCATE) == 0` and close it. On macOS call
   `disable_persistent_wal` before the checkpoint
   (`snapshot.rs:116`, `seal.rs:31-32`). No Session is opened.
6. Verify with an `immutable=1` open (`READ_ONLY|URI`,
   `file:...?immutable=1`, percent-encoded path bytes so `?`/`#`/`%` do not
   misdirect the URI, no profile apply/check, no lane) and run `validate`
   there before the rename:
   `schema::check`,
   `foreign_key_check`, a cheap `quick_check` (a page scan with no payload
   readback), row counts against the mark, both `live_heads_digest`
   values and the catalog allocator/totals checks. No sha256 of either file.
   Close the reader; the full `integrity_check` and the doctor stay separate
   commands.

The existing transfer pipeline is not reused for this: its `Install` refuses
an existing Store file and its `Retire` deletes one
(`control/crates/layerfs-store/src/sqlite/transfer/steps.rs:262-288`).

Cost: read about the live bytes plus the metadata, write about the live
bytes; peak disk = old Store + the new Store + the visited scratch file. The
fresh path stages no whole-Store copy, writes no prune journal and opens no
rewrite temp database.

## 8. PostgreSQL rebuild

Only `object_location.pack_id` references `pack`, so the payload lives in two
tables. `content_signature` (at most 8192 rows, no foreign key:
`sql/postgresql/schema.sql:45-50`) is cleaned in place.

**Phase M — mark (before Phase A).** Take the session-level exclusive Store
key before this phase and hold it through Phase C. The mark's **first
statement** reads `(catalog_incarnation, live_heads_digest,
history_meta.next_history_serial, next_pack_id, next_ordinal,
store_totals canonical/stored/metadata/logical)` and records that tuple;
any later publication makes Phase B `Conflict`, so a commit that lands during
the walk cannot be recorded while its objects are absent from `visited`. (Or
run the whole mark in one REPEATABLE READ transaction and record the tuple
inside it; state the vacuum cost.) One session, kept through Phase B, builds
`visited(object_id PRIMARY KEY, pack_id)` as a temporary table: start from the
retained roots, read pack groups in batches, follow
`FinalizedObject::admit(..).references()` and `ChainBases::base_of` as a
worklist to a fixpoint (each object once, bases of late objects included), and
record `live_heads_digest` and the live pack/object sets. After the walk,
write `gc_op` and `gc_dead` in batches (phase `mark`); a crash leaves both,
and the next `store.compact` adopts the op, truncates `gc_dead` and re-marks.
A `none` decision writes `gc_op` phase `mark`, truncates `gc_dead`, removes
the named artifact and clears `gc_op` before replying.
`gc_dead` carries `mark_stamp` for signature slots. The payload read is
reported as `markBytes`; the Phase A table copy itself moves no pack bytes.

**Phase A — build (session-level exclusive Store advisory key, no table lock).**

1. Load the live object and pack IDs into temporary tables; `ANALYZE` them.
2. Create `pack` and `object_location` in a scratch schema with
   `CREATE TABLE ... (LIKE store.pack INCLUDING STORAGE, COMPRESSION,
   CONSTRAINTS)` and the same for `object_location` (PK explicit; do not copy
   `object_location_prefix`), plus the old `TABLESPACE` and `WITH
   (reloptions)` read from `pg_class`. `pack.body` stays STORAGE EXTERNAL and
   `object_location.object_id` stays STORAGE PLAIN; the trigger and foreign
   key are added later.
3. Copy live rows with `INSERT … SELECT` and an `EXISTS` semi-join
   (`object_location` has no unique key, `schema.sql:29-38`, so a plain join
   can duplicate rows): `pack` rows with at least one visited object and
   `object_location` rows whose `object_id` is visited, in batches of about
   1 GB by `pack_id`, each batch its own transaction. A retried batch first
   deletes its own `pack_id` range from the scratch tables (idempotent), then
   inserts; assert `count(*) = count(DISTINCT object_id) = |visited|`. No pack
   bytes cross the network. Batches may pause for standby lag.
4. Build `object_location_prefix` and the foreign key, then `ANALYZE`.

**Phase B — swap (one short transaction).**

5. `LOCK TABLE` every base table of the Store schema (from `pg_class`,
   including `store_lane`, `store_totals`, `history_meta`,
   `content_signature`, the history tables and `gc_intent`/`gc_op`) in one
   `ACCESS EXCLUSIVE NOWAIT`, with `store.*`/`scratch.*` qualified names. A
   `55P03` abort is `Busy` with holder detail (not `Conflict`); `ROLLBACK`
   first, then read holders and drop the scratch schema in a new
   transaction.
6. Re-check `catalog_incarnation = G`, `live_heads_digest`,
   `store_policy.next_pack_id`/the ordinal allocator,
   `history_meta.next_history_serial` and `store_totals`
   (`canonical`/`stored`/`metadata`/`logical`) against the values the mark
   recorded. A
   publication that reused a pre-reserved pack/ordinal may move none of the
   first three but always bumps `store_totals`; any change abandons the round
   with no writes (`Conflict`). No delta copy is attempted, so the new pack
   stays in the old Store.
7. Copy the old tables' owner and ACL onto the new ones.
8. `DROP TABLE` the old `object_location` and `pack`; `ALTER TABLE … SET SCHEMA`
   the new ones into the Store's schema; create the `pack_immutable` trigger
   there, bound to the Store schema's `pack_immutable()` so doctor sees
   `EXECUTE FUNCTION pack_immutable()`
   (`diagnostic/postgresql_contract_expected.rs:213`).
9. Verify inside the transaction: row counts against the mark;
   `SUM(new) = mark_totals - dead_*` with `logical` unchanged; the
   `pg_attribute`/`attstorage`/`reloptions`/`tablespace` fingerprint recorded
   in `gc_op` at mark equals the scratch tables' fingerprint; persist the
   `dead_*` totals **before** the swap COMMIT. The settle transaction then
   subtracts once and asserts the four `store_totals` sums.
   `live_heads_digest` unchanged, and the bounded checks on the moved tables
   (the `pack_immutable` trigger/function and `pg_attribute`/storage). Run only
   the C/I contract entries for `pack`/`object_location` on the scratch tables
   in Phase A (the trigger/function entries need the Store-schema function and
   stay in the swap's bounded check), with `search_path=scratch`. A failure
   rolls back.
10. `catalog_incarnation = G + 1`; the effective `synchronous_commit` is the
    stronger of the Store's and the server's in the order
    `off < local < remote_write < on < remote_apply`, set with `SET LOCAL`
    around the swap and never lowered. It waits iff the level is at least
    `remote_write` **and** `current_setting('synchronous_standby_names')` is
    non-empty; then require the named FIRST/ANY standbys to appear in
    `pg_stat_replication` with `state='streaming'` and the matching
    `sync_state` (`sync`/`quorum`). Check before Phase M and again inside the
    swap before COMMIT; a missing standby abandons with no writes (`Busy`,
    standby list). A standby that dies after the check blocks the COMMIT;
    `SET LOCAL` cannot bound that wait.
    `DROP` the empty scratch schema before `COMMIT` (it is transactional).

**Phase C — cleanup (batched, after the swap).**

11. Delete only ids in the mark's **persisted dead set** in `gc_op`
    (membership, not a predicate: exact `content_signature` slots,
    `file_digest` roots, history/lineage ids, a deleted project's
    `path_removed`/`project_epoch` rows, and marker/base row pairs). Per-table
    watermarks (`commit`/`layer` serial, signature stamp) are only guards; a
    row created after the mark is never a member, so a Commit/Layer/save
    created during cleanup survives. Assert each batch's own `pack_id` range
    and, after the last batch, `count(*) = count(DISTINCT object_id) =
    |visited|`. Delete child-first (a project's closure
    in one transaction; a marker with its base row) so the deferred FKs hold.
    Clear `binding_hint_block` wholesale (it is not id-addressable, §6.4).
    The swap already dropped dead packs and locations, so persist
    `dead_canonical` (dropped `object_location.canonical_length`),
    `dead_stored`/`dead_metadata` (dropped pack `length`; `stored` is all
    domains, `metadata` only domain 0) and `dead_logical` in `gc_op`
    **before** the swap COMMIT; `content_signature` deletes are by
    `(slot, stamp)` with `stamp <= mark_stamp`; reduce
    `store_totals` by those totals in the **same transaction that settles the
    op**, or gate on `totals_applied`; never re-sum. Assert the new tables'
    sums equal old totals minus dead totals. The op phase is `cleanup`; a crash resumes the
    persisted set — there is no "re-derive from the current Store" path. The
    Store is usable at `G + 1` throughout; the full doctor runs after cleanup.

- **Scratch schema:** one deterministic, length-safe name per Store derived
  from `catalog_id` (`lfs_gc_<hex>`, truncated to 63 bytes), recorded in the
  Store-side `gc_op` row (§4.1) before `CREATE SCHEMA`; a crash names it from
  the Store, not only from the caller's home.
- **Crash:** a crash in Phase M/A leaves only the scratch schema, which the
  Store's op names and the next attempt removes. Phase B is atomic; a crash
  after COMMIT resumes Phase C.
- **Lost `COMMIT` reply:** record `pid`, `backend_start`, `usename` and the
  incarnation in `gc_op` in a transaction **before** the swap. Recovery on a
  second connection (before taking the exclusive key) is one statement:
  `SELECT pg_terminate_backend(a.pid, <ms>) FROM pg_stat_activity a WHERE
  a.pid = $1 AND a.backend_start = $2 AND a.usename = $3 AND
  a.datname = current_database() AND a.pid <> pg_backend_pid()`, checking the
  boolean and polling until the pair is gone, then reading the incarnation.
  Terminating another role's backend needs `pg_signal_backend` or membership;
  the precheck reports `Unsupported` when neither is available.
- **Lock time:** `ACCESS EXCLUSIVE` is held for the swap only, seconds rather
  than the length of the copy or the cleanup. No long transaction holds back
  vacuum for other Stores in the same database.
- **Space:** old tables + new heap/TOAST + indexes + up to the live bytes of
  WAL. The guide states the WAL headroom requirement; a full `pg_wal` stops
  the whole server.
- **Privileges:** `CREATE` on the database (for the scratch schema), `TEMP`
  (the visited tables), `USAGE` and `CREATE` on the Store schema, ownership
  of the Store's tables, and `pg_read_all_stats` for `client_addr`/
  `backend_start` (which may be NULL); check all before Phase M, otherwise
  `Unsupported`.

## 9. Failure and leftovers

- The op record names `G` and the staged artifact. The SQLite artifact is
  the set `<store>.compact{,-journal,-wal,-shm,-visited,-visited-journal,-mj*}`; all of it is removed for the
  reserved name, and the op is settled only after removal. The PostgreSQL
  artifact is the scratch schema, whose deterministic name is recorded in the
  Store's op row before it is created, so a crash on another machine can still
  name and remove it.
- The next `store.compact` judges it: the Store at `G` means not installed, so
  the named artifact is removed; `G+1` means installed, so the outcome is
  recorded. An op left uncertain by a dead owner is adopted by that
  `store.compact` (its artifact and high-water are in `gc_op`); only a live
  owner blocks it. A machine that cannot be reached reports `unknown`, never
  clean.
- A fresh target is built with `journal_mode=OFF`: a crash leaves a partial
  `<store>.compact` with no rollback journal, and the settle **deletes** it as
  the recorded artifact; it never tries to recover it.
- A file or schema at the staged name that no record names is refused with
  the existing `Exists` and is not deleted.
- Install is the only irreversible step: the Store is wholly old or wholly new.
- `backup.create` before compaction is the recovery path for a mark bug; no
  undo copy is kept (#155 D-24).

## 10. Fences after install and error codes

- **SQLite:** at the start of every transaction, compare the Store path's
  `(dev, ino)` with the one recorded at open; a mismatch is `StaleStore`
  before anything is read or written. This covers both a reader and a writer
  still holding the old file (formerly C8 and C8b). Compaction holds
  `lane::writer_admission` from the seal through the rename, so an admitted
  writer cannot be in flight at install; a connection that bypassed admission
  is caught at its next transaction. Install also requires the sealed
  `(size, mtime, db-header change counter)` unchanged; a change is `Conflict`
  with no writes.
- **PostgreSQL:** the generation check of §5.2.
- **`lfq1` tokens** of an older generation already fail, because their
  authenticator is keyed by the incarnation.

| Code | Status |
|---|---|
| `MountsActive`, `Busy`, `Unresolved`, `NotFound`, `NoSuchBranch`, `NoSuchCommit`, `NoSuchProject`, `NoSuchStore`, `Unsupported`, `ReservedName`, `Conflict`, `NameTaken`, `Capacity`, `Exists`, `InvalidInput` | existing (`Unresolved` now used by the four calls) |
| `StaleStore` | new; one `HistoryError` variant with an explicit mapping arm, because the `_ => "Failed"` wildcard (`engine/crates/layerfs-store-service/src/history/lookup.rs:54,64`) would hide it |
| `Removed` | new; reading a path removed by `remove_file`; an explicit `HistoryError::Removed` arm in `lookup.rs` before the `_ => "Failed"` wildcard; detail `{paths: [{path, frontier_serial}]}` |


- `project.init` whose name is held returns `NameTaken` (today it returns
  `Exists`), the same as a Branch create or fork; both the home record and the
  Store name collision map to it.
- The new `HistoryCatalog` methods are enumerated in the #42 list
  (`mark_*`, `retained_roots`, `get_retained_roots`, `remove_unreachable`);
  both production providers implement them; the default body is `Unsupported`,
  and `lookup.rs` maps `HistoryError::Unsupported` explicitly (C9).
- All four calls belong to the MCP admin profile and are refused under a
  folder scope (`api/crates/layerfs-api/src/calls/scope.rs:33`, `out of
  scope: <call>`; #42 item 39). A folder scope guards one folder and one
  Branch; none of the four is a folder operation (`branch.remove` retires the
  whole Branch, `project.*` the project, `store.compact` the Store), so they
  are refused rather than reinterpreted at a wider scope.
- The four calls take `branch.remove(project, branch)`,
  `project.delete(project)`, `project.remove_file(project, path)` and
  `store.compact(name)`; no options.
- MCP: the admin classifier checks its admin list before the work families;
  the four calls join it and `project.remove` leaves both the call table and
  that list. `whyOf`, the profile prose and `--profile admin` usage name
  the four calls; an old MCP `project_remove` is `InvalidParams "unknown
  tool"`, a CLI usage error exits 2, and the library reports `InvalidInput`;
  the precedence test covers `branch.remove` under `work`. The table becomes
  76 calls (38 work/38 admin); regenerate `gen-profiles`/`gen-coverage` and
  the hard-coded 73/38/35 counts, and give `whyOf` one line per call.
- Results: `branch.remove` → `{kind: "Removed", project, branch, removed_ms}`;
  `project.delete` → `{kind: "Removed", project, deleted_ms,
  home_entry_removed}`; `project.remove_file` → §4.3's `Removed`/`UpToDate`;
  `store.compact` → the §11 report. Success results and refusal details are
  snake_case; refusal `detail` is snaked in the dispatcher (or built snake at
  the refusal site), and a camelCase detail is a bug; the CLI prints the
  rendered map; §11 names are logical.
- `MountsActive` carries `detail.holders` (mounts/Sandboxes); `Busy` carries
  `{entity, phase, op}` plus the PostgreSQL holders (`pid`, `client_addr`,
  `backend_start`). `InUse` stays for non-GC calls only.
- `store.compact`'s open-cache refusal for a call still running on the Store
  in the same process is therefore `Busy`, naming the Store as `entity` with
  `phase` and `op` null: no operation holds it, a call does. The cache's own
  `InUse` is reported by the non-GC calls that close a Store (`store.move`,
  `store.remove`, `store.disconnect`).
- `Unresolved` carries `{entity: {kind, name}, phase, op}` and is observed by
  a non-owning call and settled by the owning one: `store.compact` adopts a
  dead compaction op; `project.delete`'s settlement reads the Store by the
  recorded stack id (retired/absent finishes the entry, live redoes the
  marker under the project op); `sandbox.close`/`machine.remove`/
  `store.remove` clear their own ops. `logs.reset` only truncates `calls.jsonl`; it cannot clear an op.
  A live owner stays `Busy`; a provably dead, certain op is ignored.
- Capacity is the real `PersistenceError::Refused { status: "Capacity" }`
  path (`StorageError::is_capacity_refusal`); add `StorageError::CapacityExceeded`
  to that predicate, keep PG 53100/53200/53400/54000 (53300
  `too_many_connections` is transient and maps to `Busy`; 54001 is out), and
  name the SQLite codes (`SQLITE_FULL`, `SQLITE_TOOBIG`, `SQLITE_NOMEM`).
  `CapacityExceeded` (a codec/canonical bound) is documented as
  `InvalidInput`/`ProviderFailure`, not disk exhaustion. The `_ => Refused`
  arm gets an explicit Capacity arm.

## 11. Report

While compaction runs, it writes its phase into the op (`mark`, `build`,
`verify`, `install` on SQLite; `mark`, `build`, `swap`, `cleanup` on
PostgreSQL), and `store.status` shows it with the op's other fields
(`control/crates/layerfs-home/src/op.rs:189-201`); there is no `PROGRESS`
option (#42 item 40). On PostgreSQL `store.status` reads the Store-side
`gc_op`; on SQLite it reads the home op. The `op` projection is
`{id: op_id, name, phase, started: started_ms, uncertain}` from
`gc_op`/`gc_intent`, and `state` is `busy`/`unresolved` from it; a remote
SQLite Store's op is read from its home/sidecar via `step_on`. `store.compact`
returns:

| Field | Meaning |
|---|---|
| `path` | `none` or `rebuild` |
| `generation` | `{ before, after }` |
| `deadBytesBound` | bytes in wholly dead packs after the metadata walk; the value that chose the path |
| `deadBytesRefined` | the same bytes after the payload sweep (§7 step 4) |
| `logicalDiscarded` | the collected history rows' `logical` bytes (the `store_totals.logical` delta) |
| `backendReusable` | SQLite: rebuild = the new artifact's `freelist_count * page_size` before the rename (normally 0), none = the live Store's; PostgreSQL: 0 (no in-place reclaim) |
| `physicalShrink` | SQLite: old file size minus new; PostgreSQL: `pg_total_relation_size` delta |
| `keptWaste` | object: `{deadInRetainedPacksBytes, deltaBases: {bytes, references: [id]}, inodeBlocksBytes}` |
| `peakTempBytes` | SQLite: the new Store + the `visited` scratch file (the fresh path stages no whole-Store copy and writes no rollback journal); PostgreSQL: the scratch schema + WAL |
| `residue` | object `{bytes, artifacts}`; a nonzero residue is explained |

The result is `{kind: "Compacted", ...}`. `keptWaste` is
`{deadInRetainedPacksBytes, deltaBases: {bytes, references: [id]},
inodeBlocksBytes}`; `residue` is `{bytes, artifacts: [string], reason?}`;
top-level `estimate: bool`; a capped field is `{value, cap}`; `zeroReason`
is per zero field with the closed vocabulary `empty-store`, `none-path`,
`no-history`, `no-waste`, `postgresql-in-place`; `markBytes` is a #52 metric,
not a result field. The SDK/CLI render every key
snake_case. On the `none` path `deadBytesRefined` equals `deadBytesBound` (no
payload sweep runs below the threshold), and the report says so. `physicalShrink`
reads the old size before the rename; `backendReusable` reads the SQLite
free-page count × page size before it; `logicalDiscarded` is the summed
`logical` of the collected history rows; PostgreSQL reports 0 for rebuilt tables and does
not count in-place free space. `physicalShrink` samples `pg_total_relation_size`
before and after the swap; on PostgreSQL `deadBytesRefined` equals
`deadBytesBound` (there is no payload sweep to refine it). A zero is always
explained; a capped field carries its cap (S7).

## 12. Acceptance (#52)

Each bullet is one case; its label is its case ID and must be unique.

- **Crash injection** (`kill -9`, per backend) at mark, build, verify, before
  and after install: wholly old or wholly new; the leftover is judged by §9.
- **Backend matrix:** every removal, admission, report and name item runs on
  SQLite and PostgreSQL; a converted 1.0 Store opens with the additive step on
  both.
- **Crash matrix:** named failpoints at every phase (SQLite: mark, build,
  verify, install, WAL switch, artifact removal, op settle; PostgreSQL: mark,
  build, swap, cleanup, scratch drop), plus lost COMMIT, power-loss
  (fsync/rename) and unresolved op/lease takeover; assert §9 state,
  generation, digest and closure.
- **No live object lost:** property tests over random fork / commit / merge /
  publish / `branch.remove` / `project.delete` / `remove_file` histories;
  closure sets compared before and after.
- **Independent walk:** after compaction, walk the new Store with the
  existing reference walk (`diagnostic/scan.rs`) and run the full doctor on
  the fixtures (#155 D-23).
- **Regression:** after compaction, a Save of bytes whose `file_digest` row
  named a collected root succeeds.
- **Fences:** a connection that bypassed admission gets `StaleStore`; a second
  compaction gets `Busy`; a remote PostgreSQL daemon with a mount blocks
  compaction and the matching removal, naming itself; a one-shot SDK
  transaction open at swap time makes the swap abandon.
- **Admission races:** a mount and a removal of the same Branch started
  together never both succeed.
- **Names:** `NameTaken` after removal (Branch and project) and on create
  collision (Branch and project; project create was `Exists`), a free name
  after a rebuild collects the row, `ReservedName` for `main`.
- **`remove_file`:** `Removed` for Commits at or below the frontier, before and
  after compaction; the next Commit of a mounted older head records the
  deletion; a file created later at the path is visible; 2.0 frees no bytes
  for the removed path (visibility only).
- **`remove_file` and derived nodes:** a non-fast-forward `branch.merge` of
  two heads at or below the frontier keeps the path hidden (the whiteout is
  materialized), and a file created after the frontier is visible; a
  fast-forward merge is the control (no new tree); a `branch.publish` of a
  Commit at or below the frontier yields a hidden Layer, a mount of it
  whiteouts, and its next Commit records the deletion; a merge that cannot
  apply the whiteout is refused `Removed`.
- **Removed path in a merge:** a non-fast-forward merge of two heads at or
  below the frontier reports no conflict for the removed path and omits it; a
  merge of an at-or-below head with a post-frontier re-creation keeps the
  re-created file, and only an ordinary edit conflict on it is reported.
- **Unmerged Branches:** `remove_file` hides the path in a Commit on an
  unmerged Branch created before the frontier; a Commit on that Branch after
  the frontier does not inherit it.
- **Layer-headed mount:** a Branch whose head is empty and whose base Layer is
  at or below the frontier whiteouts the path at mount.
- **Visibility only:** `remove_file` frees no bytes; an object reachable only
  through the removed path is retained; the marker hides reads/listings and a
  mount/merge/publish whiteouts the path.
- **Published Branch:** a Branch published by a retained Layer, or named by
  a retained `commit.branch_id`, keeps its name after `branch.remove`; only an
  unreferenced Branch's name is freed by a rebuild (not by a `none` pass).
- **Late base:** a delta base in an already-passed pack and a depth-2 chain
  are copied by the worklist; `pack_id` is preserved.
- **Digest roots:** an uncommitted saved file's `file_digest` row is collected
  unless another retained root reaches its content.
- **Path grammar:** `/a` and `a` are accepted; empty, `/`, `//a`, `a/`,
  `a//b`, `./a`, `a/./b`, `.`/`..` components, `\`, NUL, >4096 bytes and >256
  components are refused `InvalidInput`; a directory hides its subtree; a
  child of a removed non-directory is `NotFound`; an intermediate symlink is
  `InvalidInput` and a leaf is hidden; a hardlink survives.
- **Repeated removal:** a second `remove_file` after a re-creation hides the
  re-created file; a repeat with no newer project node is `UpToDate`, while a
  commit in another project neither rewrites the marker nor advances it; a
  never-existing path is accepted.
- **Marker epoch:** a one-shot write racing `branch.remove` or
  `project.delete` gets `Conflict` and leaves no Commit; a merge racing
  `remove_file` gets `Conflict`, and the retry applies the whiteout.
- **Concurrent merge and removal (property):** a merge and a `remove_file`
  started together never leave the path visible in a derived tree: one side
  wins, the other either sees the marker or gets `Conflict` and retries with
  the whiteout.
- **Marker cleanup:** a rebuild of a Store with a deleted project leaves no
  `path_removed` row for it and keeps a live project's markers; a `none` pass
  writes nothing and retains them.
- **Position state:** a converted Store backfills serials;
  `next_history_serial` never falls after compaction; `project_epoch` copies
  and bumps only on a marker write.
- **Marker survival:** a retained `branch_retired`/`layer_stack_retired`/
  `path_removed` survives a rebuild; a collected row takes its marker with it.
- **project.delete retry:** the marker commits, the home removal fails →
  hidden, entry present, `home_entry_removed: false`; a same-home retry
  finishes and returns the stored `deleted_ms`, with no second marker; the
  own-Init identity is saved before the marker and a save failure removes
  nothing.
- **Home listing:** a second home's stale entry is absent from `project.list`
  and `project.get` answers `NoSuchProject`; Store calls agree.
- **PG cleanup:** a Commit/save during Phase C survives the high-water bound;
  a crash resumes from `gc_op`; a marker and its base row are deleted in one
  batch (no resurrection/orphan), with a failpoint at the boundary.
- **Admission:** a mount from another home/machine is visible in the Store-side
  intent; `machine_stopped` is ignored; a second mount of one project
  proceeds; an uncertain dead op is `Unresolved` and the owning call clears it.
- **NameTaken:** `detail {holder: {kind, name, state: live|retired|deleted},
  source}`; a forced two-reference fixture (Layer FK and `commit.branch_id`)
  keeps the name; one reference and neither are the controls. The error
  matrix covers `MountsActive`, `Busy`, `NoSuchBranch`/`NoSuchCommit`/
  `NoSuchProject`, `NameTaken`, `ReservedName`, `Removed`, `InvalidInput`.
- **Trace salt:** the call log stores `<N bytes>` for the path; the trace key
  is the keyed hash; `Removed` echoes the path; a rebuild copies the salt.
- **Admission (liveness):** a live-but-unreachable Sandbox is `Busy`;
  `machine_stopped` is ignored; a `gc_intent` from another home is visible;
  `Unresolved` is observed by a non-owning call and settled by the owner.
- **Checkpoint/install:** a live reader makes `wal_checkpoint(TRUNCATE)`
  return `busy>0` → `Busy`, no artifact, no op; a sidecar or `-mj*` at install
  is `Conflict` with no writes.
- **Bind failure:** a committed intent whose bind fails is deleted in the
  error path; a failed delete is reported; no `MountsActive` remains.
- **PG refcount:** two mounts keep the Store/project keys after one unmount;
  a failed all-or-none acquire unlocks the taken subset.
- **Worker/lost reply:** an unreachable worker op is `unknown, never clean`;
  a lost reply keeps the sidecar and the retry resolves retired/live/absent.
- **Trace:** the six path names are redacted; refusal details are not traced;
  the salt is backfilled for an upgraded Store.
- **Artifacts (cross-backend):** SQLite leaves/removes
  `.compact{,-journal,-wal,-shm,-visited,-visited-journal,-mj*}`;
  PostgreSQL's `lfs_gc_<hex>` is recorded before CREATE, removable from
  another machine, and a stray schema is `Exists`.
- **Position state (2.0 backfill):** every upgraded Store (2.0 included)
  gets one `project_epoch` row per stack; `gc_op` is cleared on settle and
  not copied; `next_history_serial` never lowered.
- **Digest agreement:** SQLite and PostgreSQL report the same
  `live_heads_digest` for the same fixture; a retired head changes it.
- **PostgreSQL mark:** the mark transfers only `markBytes`; the table copy
  moves no bytes; a base object that only a dead pack holds is kept.
- **Reserved pack:** a Save that reserves a pack/ordinal before the mark and
  publishes during Phase A (or Phase C) moves `store_totals`; the swap
  `Conflict`s, the pack stays in the old Store, and the cleanup failpoint
  keeps it; no dangling location.
- **PG cleanup (resume):** after the swap the Store is usable at `G + 1`;
  Phase C resumes after a crash; totals/ACL/trigger/hints are asserted (no
  `inode` on PG; that assertion belongs to the SQLite install).
- **PG privileges and reply:** missing `CREATE`/`TEMP`/`pg_read_all_stats` is
  `Unsupported`; a lost `COMMIT` reply is resolved by proving the backend
  ended, then reading the incarnation.
- **SQLite layouts:** schema 36 (`Monolithic`), 37 (`GroupRows`), 38
  (`GroupRowsIndexed`) and their 42/43/44 acquisition variants rebuild;
  `Monolithic` copies `pack` (body) and no `pack_unit`; `GroupRows` copies
  both; `GroupRowsIndexed` copies `pack_unit` and the schema creates its
  index (not "copied"); `sqlite_sequence` is kept when present and
  `init_*`/acquisition tables are empty.
- **Install fence:** a connection that bypassed the writer lane and opens a
  transaction after install gets `StaleStore`; an in-flight transaction is
  excluded by the lane (a documented limit); a sealed-state or sidecar change
  makes install `Conflict` with no writes.
- **Artifacts (SQLite exact):** a crash leaves
  `.compact{,-journal,-wal,-shm,-visited,-visited-journal,-mj*}`; the next
  compact removes all of it; `none` leaves no `.compact`; a stray name with
  no op is `Exists`.
- **Threshold and failures:** 9% wholly dead → `none`; 10% and 30% →
  `rebuild`; an empty Store (0/0, COALESCEd sums) → `none` with a
  `zeroReason`; a started rebuild completes when the refined share drops
  below 10%; `Capacity` and a verify failure abort, delete the artifact set
  and leave the old Store unchanged apart from step 1's checkpoint.
- **Mount/compact race:** a mount during compact is `Busy` with the compact
  op; compaction returns `MountsActive` when a mount wins; a Commit during the
  rebuild is not lost (PG: a write during Phase A makes the swap `Conflict`;
  during Phase C it survives the high-water bound).
- **Profile and scope:** all four calls are refused under a folder scope
  (`out of scope`) and allowed only in the MCP admin profile.
- **Progress:** while `store.compact` runs, `store.status` shows the op phase
  (`mark`, `build`, `verify`, `install` on SQLite; `mark`, `build`, `swap`,
  `cleanup` on PostgreSQL).
- **Report:** both paths; `kind:"Compacted"`, snake_case keys, every §11
  field with its unit and sampling point (`freelist_count * page_size`, old
  size before rename, history `logical` delta, peak temp = new file +
  journal/WAL + visited, `physicalShrink`, `keptWaste` components,
  `residue {bytes,artifacts}`, `estimate`, capped `cap`, a `zeroReason`);
  `markBytes` is a #52 metric (PG-only, not a result field);
  `keptWaste` is `{deadInRetainedPacksBytes, deltaBases:{bytes,references},
  inodeBlocksBytes}`; bound≥refined; PG `backendReusable = 0` and PG/none
  `deadBytesRefined == deadBytesBound`.
- **Read surface:** `file.read`/`stat`/`list`/`glob`/`diff`/`revision`/
  `export`/cp-from before and after compaction, with the §4.3 shapes; add
  `edit`/`grep`/`blame`/`rm`/`mv`/`mkdir`/`cp`/`apply`, the
  visible/hidden/absent diff table, `removed_count` (0 and >0),
  `branch.diff`/`commit.diff`, and one whiteout case per one-shot tree
  creator (`write`/`edit`/`apply`/`rm`/`mkdir`/`mv`/`cp`).
- **Restore and listing:** `backup.restore` before and after a removal; a
  second home's stale entry; the §4.2 guard test and allow-list.
- **Error matrix:** exact code and message per case, including `StaleStore`,
  `Removed`, `InvalidInput` for `project.remove`, `Exists`, `Unsupported` and
  `Capacity`.
- **Digest golden vector:** a fixed-id fixture (out-of-order ids, a retired
  Branch and stack, differing timestamps) hashes to one literal vector on both
  backends; an absent head is 33 zero bytes, not 32, and widths are fixed at
  17/33.
- **Speed and space** (at least three runs per arm, median and range,
  declared caches, generated fixtures): downtime per GB of live data and per
  GB freed, bytes written, peak disk, `ACCESS EXCLUSIVE` duration on
  PostgreSQL. Downtime per GB freed is absolute bytes when nothing is freed
  (the `none` path and removal-only runs). The rebuild's build phase is
  compared with plain `VACUUM INTO` of the same Store; the mark is reported
  separately. A storage option that costs about 50% more time for about 5%
  less space is rejected.

## 13. Estimates, not measurements

For a 50 GB SQLite Store with 30% in wholly dead packs (35 GB live) on NVMe,
the review of 2026-11-14 estimated about 2–3 minutes of downtime for §7, and
hours if the full doctor ran before and after. For PostgreSQL with the same
Store, about 9 minutes in total with `ACCESS EXCLUSIVE` held for about two
seconds. Nothing has been measured; #52 owns the numbers.

#52 measures, per backend, these arms: `none` at 9% wholly dead; `rebuild` at
10% and 30%; a 50 GB total / 35 GB live (30% dead) Store, plus a 50 GB
live-store arm; and each removal type (branch, project, remove_file) as a
marker-only arm, plus removal-then-compact at a declared share. A
`remove_file`/`remove_file`-then-compact arm is 0-freed and reports absolute
bytes. At least three runs per arm, median and range; declare the hardware,
SQLite 3.53.2 with `page_size`, `journal_mode`, caches, `synchronous_commit`
and PostgreSQL version. Record the
§11 fields, `markBytes`, downtime per GB of live data and absolute bytes when
nothing is freed (0/0 or removal-only), bytes written, peak disk,
`ACCESS EXCLUSIVE` duration, WAL bytes and the `VACUUM INTO` baseline; use
`store.status` for the phase. A storage option that costs about 50% more time
for about 5% less space is rejected, naming the option and the measured
numbers.
