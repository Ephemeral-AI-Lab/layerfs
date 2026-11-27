# Lane W (writes): notes

Branch `rt818-lane-w`, base `54778271cd` (Phase 0 merge). PostgreSQL database
`rt818_w` in `lfs818-rt-pg`. Pin file
`engine/crates/layerfs-store-service/tests/sdk_write_round_trips.rs`.
Counts are `[connects, round_trips, statements, transactions]` per role
(writer, control reader, Workspace reader); SQLite rows count statements.
Baseline = the Phase 0 pins (RECORD.md Phase 0a, W0 attribution
`receipts/P0/w0-*-phases.txt`).

## W1 — hidden-path check from the kept markers

Prediction (written before coding). The eager `Visibility::for_stack` +
`node_serial` + `TreeTable::open` in `write/files.rs` `change` run on the
control reader. With the bind's kept markers (`KnownRemovals`; both backends
keep them under a mount admission) the call builds `Visibility::from_markers`
and opens the base tree only on the refusal path. W0 attributes the control
reader's `files.change` work on every write case to exactly these reads.

| Case | PG control before → after | PG total before → after | SQLite control stmts before → after |
|---|---|---|---|
| every `.warm` | [0,10,6,2] → [0,4,2,1] | −6 (overwrite 22 → 16, rm.dir 148 → 142) | 14 → 6 (tx 4 → 2) |
| every `.cold` | [1,17,7,2] → [1,7,3,1] | −10 (its 4 prepares go too; overwrite 75 → 65) | 14 → 6 |

Writer and Workspace reader unchanged. Hand-off expectation: −5 on an
overwrite (4 verified + 1 inferred `TreeTable::open`); W0 shows the tree open
costs 2 (locate + span) on the uncached control reader, so −6 warm.

**Measured** (`receipts/W1/pins-measured.log`, then `sdk_write_round_trips.log`
green on the new pins). Exactly as predicted on every case and role, both
backends: PostgreSQL control [1,17,7,2] → [1,7,3,1] cold and [0,10,6,2] →
[0,4,2,1] warm; SQLite control 14 → 6 statements, 4 → 2 transactions. Writer
and Workspace reader unchanged.

| Case | PG trips before → predicted → measured | SQLite stmts before → measured |
|---|---|---|
| `write.overwrite.cold` / `.warm` | 75 / 22 → 65 / 16 → 65 / 16 | 148 / 62 → 140 / 54 |
| `write.new_file.cold` / `.warm` | 112 / 44 → 102 / 38 → 102 / 38 | 152 / 75 → 144 / 67 |
| `mkdir.cold` / `.warm` | 113 / 46 → 103 / 40 → 103 / 40 | 158 / 82 → 150 / 74 |
| `rm.dir_100_recursive.cold` / `.warm` | 207 / 148 → 197 / 142 → 197 / 142 | 251 / 177 → 243 / 169 |

Within 15%: yes (0% off). Against the hand-off's −5: −6, the tree open is 2
trips (locate + span), as W0 attributed.

- Decisions: D16 kept. Kept markers present → `Visibility::from_markers`, no
  marker or serial read, base tree opened lazily (control reader) only when
  `edit`/`cp`-from met `NoSuchPath`. Kept markers absent → today's eager
  `for_stack` + `node_serial` + `TreeTable::open`, unchanged. Narrowing on
  the kept path only: a transient failure of the base-tree open no longer
  refuses a change that never asks for it (the Workspace reads the same root
  itself); no answer or refusal on a healthy Store changes.
- Accessor `BoundWorkspace::kept_markers` (removal.rs) checks `gc_op`, the
  bound node and maps a node kept as absent (or one this Workspace
  published) to (0, no markers); `commit_removals` shares it.
- New test `tests/sdk_write_kept_markers.rs` (both backends): one kept marker
  and then 71 markers over `KEPT_REMOVAL_BYTES` (fallback); `edit` of the
  hidden path `Removed`, of an absent one `NoSuchPath`, a write commits; the
  fallback's control reader does strictly more (PG kept [13, 2] vs fallback
  [15, 6] control statements; SQLite [17, 6] vs [23, 16]).
- Checks (once): `sdk_round_trip_equivalence_seeded` (186 + 17 steps equal on
  both backends), `method_refusal_order`, `sdk_path_removal`,
  `sdk_path_whiteouts`, `sdk_path_reads`, `sdk_path_hidden_semantics`: green.
- No new or changed server message: no sweep. Deleted tests: none.
- Production LOC: 227,595 -> 227,620 (delta +25), engine
  layerfs-store-service +25.

## W2 — bind by name

Prediction (before coding). The project lookup (1 statement) and
`branch_named` (a 3-trip read transaction) on the control reader go: the
private bind reads project, Branch and snapshot in one named statement
(`branch_snapshot_named`) where it read the snapshot by id (writer on
PostgreSQL, Workspace reader on SQLite). After W1 they were the control
reader's only work on a write call, so the control reader is idle.

| Case | PG control | PG writer | PG total | SQLite |
|---|---|---|---|---|
| every `.warm` | [0,4,2,1] → [0,0,0,0] | unchanged | −4 (overwrite 16 → 12) | control 6 → 0 (tx 2 → 0); Workspace reader unchanged |
| fused `.cold` (overwrite, edit) | [1,7,3,1] → [0,0,0,0] | unchanged | −7 (overwrite 65 → 58) | control 6 → 0 |
| non-fused `.cold` (new_file, mkdir, cp, mv, rm, apply, rm.dir) | [1,7,3,1] → [0,0,0,0] | +1 trip | −6 (new_file 102 → 96) | control 6 → 0 |

The +1 cold writer trip: the bind's by-id snapshot statement, which the
separate Commit path's `head` phase reuses, is then first prepared in `head`
while the bind prepares the named one. Hand-off expectation: W2 saves 4
before R1 (warm).

Refusal order kept: the project lookup still runs first (then today's
refusal) whenever the `branch` argument is an error, the Branch is `main`,
`expected` is given (today's path, it needs the stack record), the Workspace
key cannot be made, or the Store refuses the bind before reading (overlay
maintenance stopped, call capacity full). `BindError::NoSuchProject` /
`NoSuchBranch` map to today's `NoSuchProject` / `NoSuchBranch` texts, a
snapshot read failure to today's lookup-failure refusal.

**Measured** (`receipts/W2/pins-measured.log`; `sdk_write_round_trips.log`
green on the new pins). As predicted on every case and role, both backends,
after one fix during the item: the first measurement still showed a cold
control reader `[1, 1, 1, 0]` (its connect and schema check), opened by the
W1 code's eager `Store::ports` for the base tree; the reader is now made with
the tree, lazily. Control is `[0, 0, 0, 0]` on every case.

| Case | PG trips W1 → predicted → measured | SQLite stmts W1 → measured |
|---|---|---|
| `write.overwrite.cold` / `.warm` | 65 / 16 → 58 / 12 → 58 / 12 | 140 / 54 → 134 / 48 |
| `edit.2000_lines.cold` / `.warm` | 57 / 18 → 50 / 14 → 50 / 14 | 143 / 52 → 137 / 46 |
| `write.new_file.cold` / `.warm` | 102 / 38 → 96 / 34 → 96 / 34 | 144 / 67 → 138 / 61 |
| `mkdir.cold` / `.warm` | 103 / 40 → 97 / 36 → 97 / 36 | 150 / 74 → 144 / 68 |
| `rm.dir_100_recursive.cold` / `.warm` | 197 / 142 → 191 / 138 → 191 / 138 | 243 / 169 → 237 / 163 |

Within 15%: yes (0% off). Hand-off expectation "W2 saves 4 before R1": 4
warm. Cold separate-path Commits pay +1 writer trip (the `head` phase now
prepares the by-id snapshot statement the bind used to prepare).

- Refusal order: `tests/sdk_write_named_refusals.rs` (both backends)
  checks `NoSuchProject` before every other fault (missing/`main`/unknown/
  invalid Branch, bad content), then the missing `branch` argument, `ReadOnly`,
  `NoSuchBranch` for unknown and invalid names, then the change's refusal;
  it passes on W2 and, run once against the W1 source, on the order before W2
  (`scratchpad`, not kept). `method_refusal_order`, `commit_expected`
  (`expected` takes today's path), equivalence driver: green.
- Not testable cheaply and kept by construction: a refusal before the bind
  reads (Workspace key, overlay maintenance stopped, capacity full) runs the
  lookups first and returns their refusal if any. A transient failure of the
  named snapshot read answers as a lookup read failure (`published`);
  `BindError::Cleanup` / reader-mount failures answer `Failed` with the bind's
  text, as a bind failure did before.
- Sweeps (new flight: the private named bind, writer named snapshot +
  admission message): `tests/sdk_bind_named_fault.rs`, drop and kill at all
  26 messages, mount-sweep invariants (rows, session keys, reopen, rebind,
  fresh bind, no open transaction): drop 22 "unknown, row released at
  reopen" + 4 "unknown, no row"; kill 26 "bound, then killed". 6.4 s.
- `W1_KEPT` now shows the kept regime's write with 0 control statements
  (PG kept [11, 0] vs fallback [13, 4]; SQLite [11, 0] vs [17, 10]).
- Deleted tests: none.
- Production LOC: 227,620 -> 227,712 (delta +92), engine
  layerfs-store-service +92.

## W3 — the Store remembers the last logical size

Prediction (before coding). A Store-level slot `(NodeId, u64)` (D7: no
`BranchSnapshot` change) is set wherever a Workspace remembers a node's
recorded logical size (the one it read, or the Commit it published), and
`known_logical` checks it after the Workspace's own slot. A call whose bound
head is the Commit the previous call on this Store open published skips the
`logical.known` read. In the pins every `.warm` case is such a call (the
`.cold` call before it published on the same open and Branch); `.cold` cases
open a new Store, so they miss.

| Case | PG writer | SQLite writer |
|---|---|---|
| every `.warm` | −1 trip, −1 statement (overwrite 12 → 11, new_file 34 → 33, rm.dir 138 → 137) | −3 statements, −1 transaction (W0: `logical.known` 3 statements in 1 transaction) |
| every `.cold` | unchanged | unchanged |

Memory: one `(NodeId, u64)` per Store (about 40 bytes).

**Measured** (`receipts/W3/pins-measured.log`; `sdk_write_round_trips.log`
green on the new pins). Exactly as predicted: every `.warm` writer −1 PG
trip and statement, SQLite −3 statements and −1 transaction; `.cold`
unchanged.

| Case | PG trips W2 → predicted → measured | SQLite stmts W2 → measured |
|---|---|---|
| `write.overwrite.cold` / `.warm` | 58 / 12 → 58 / 11 → 58 / 11 | 134 / 48 → 134 / 45 |
| `write.new_file.cold` / `.warm` | 96 / 34 → 96 / 33 → 96 / 33 | 138 / 61 → 138 / 58 |
| `mkdir.cold` / `.warm` | 97 / 36 → 97 / 35 → 97 / 35 | 144 / 68 → 144 / 65 |
| `rm.dir_100_recursive.cold` / `.warm` | 191 / 138 → 191 / 137 → 191 / 137 | 237 / 163 → 237 / 160 |

Within 15%: yes (0% off). Decision D7: Store slot, no `BranchSnapshot`
change; the slot is set in `remember_logical` (a read or a publication, both
immutable facts of that node), so `store/commit.rs` does not grow.
- Checks: equivalence driver, `method_refusal_order`, `mount_round_trips`
  (its pins unchanged), `postgresql_phase_attribution`, `commit_random_model`,
  `commit_batched_base_files`: green. No new message, no sweep. Deleted
  tests: none.
- Production LOC: 227,712 -> 227,717 (delta +5), engine layerfs-store-service +5.

## W5 — the kinds of a listed page in one demand

Prediction (before coding). Recursive `rm` and `cp` stat every listed
child (`SourceView::stat`: inode value, portable metadata, and a file's
length or a directory's sub-directory count). `rm` needs only the kind, and
`cp` needs the kind to descend (its `copy_one` keeps its own stat). The new
`SourceView::kinds` answers a listed page's kinds from the overlay, then the
rest with one `BaseView::values` (`lookup_many`, signature unchanged): the
inode-table waves only, no metadata or length. W0: the 100-file remove's
`files.change` Workspace reader is 106 single-object locates warm, about one
per child (its metadata object or length). The listing is one page
(`PAGE_ROWS` = 1,024), now with one batched inode lookup whose leaves the
remove's own path resolution mostly already read.

| Case | PG Workspace reader W3 → predicted | PG total W3 → predicted | SQLite Workspace reader W3 → predicted |
|---|---|---|---|
| `rm.dir_100_recursive.warm` | [0,109,109,0] → ~[0,6,6,0] (5–7) | 137 → ~34 | 116 → ~13 (12–14) |
| `rm.dir_100_recursive.cold` | [1,123,121,1] → ~[1,20,18,1] (19–21) | 191 → ~88 | 169 → ~66 |
| every other case | unchanged (no recursive walk) | unchanged | unchanged |

Writer and control reader unchanged. Hand-off/SPEC expectation: "106
locates → about 2–6 demands"; SPEC M1 "rm dir ~42 cold" is the probe's warm
open, so it compares with `.warm` here (~34; R1's −2 is not on this base).

**Measured** (`receipts/W5/pins-measured.log`; `sdk_write_round_trips.log`
green on the new pins). Only `rm.dir_100_recursive` moves, only its
Workspace reader; every other case and role is unchanged.

| Case | PG trips W3 → predicted → measured | SQLite stmts W3 → predicted → measured |
|---|---|---|
| `rm.dir_100_recursive.cold` | 191 → ~88 → 92 (+4.5%) | 237 → ~134 → 138 (+3%) |
| `rm.dir_100_recursive.warm` | 137 → ~34 → 38 (+11.8%) | 160 → ~57 → 61 (+7%) |

PG Workspace reader: [1,123,121,1] → [1,24,22,1] cold, [0,109,109,0] →
[0,10,10,0] warm; SQLite 169 → 70 and 116 → 17.

Within 15% on every case total. The Workspace-reader role alone is over
(warm 10 vs ~6), diagnosed once from the per-phase counts (probe `w0`
group, PG, `receipts/W5/w0-rm-dir.log`): `files.change` is 7 (4 locates,
2 span reads, 1 pack-block read: the path walk to `/src/d07`, its listing
page and the one inode-table wave), and `validate` keeps its 3 Workspace
trips from before. The prediction's residual left `validate`'s 3 out
(W0 attributes 106 of the 109 to `files.change`); corrected, 7 + 3 = 10.
The per-child cost is gone: 106 → 7 in `files.change`. No fix needed.

- `cp` takes the page kinds too (it needs them to descend); `copy_one` keeps
  its own stat for mode, length and link target. Its trips are not pinned
  (`cp.file` copies one file, no walk) and its answers are checked below.
- New `tests/sdk_write_recursive_kinds.rs` (both backends): a mixed tree
  (files, nested and empty directories, symlinks to a file and to a
  directory, two hard links, an executable mode, a path a marker hides) is
  copied and removed: the copy equals the visible original by name, type,
  mode, size, content and hard-link grouping, the hidden path is not
  copied; a directory of 1,100 files (over the 1,024-row page) is copied and
  then removed in one `fs.apply` after adding a file and nested directories
  in the overlay (kinds the overlay answers).
- Checks: equivalence driver, `method_refusal_order`, `sdk_method_golden`,
  `write_refusals`, `sdk_path_whiteouts`, `sdk_path_reads`,
  `sdk_path_hidden_semantics`, `history_diff_oracle`, workspace
  `namespace_view`, `namespace`: green. No new server message (fewer of the
  same demands), no sweep. Deleted tests: none.
- Production LOC: 227,717 -> 227,748 (delta +31), engine
  layerfs-store-service −2, layerfs-workspace +33.

## M1 close

- Clippy `-D warnings` (`--all-targets`) on `layerfs-store-service` and
  `layerfs-workspace`: one finding in W2 code (`map_err` returning its
  argument in `StoreService::bound_or_released`), fixed with `inspect_err` in
  its own commit; pins and `sdk_write_named_refusals` rerun green
  (`receipts/M1-lint/`). rustfmt clean on every file lane W touched
  (pre-existing `cargo fmt` diffs in other tests of the crate left alone).

## W2 fix — a bind that fails before its lookup (M1 review)

Finding (independent M1 review, forwarded by the coordinator): `bind_named`
mapped every `BindError::Read` in phase `Snapshot` to the lookup's read
failure, but `Store::bind_once` is also in phase `Snapshot` when the
Workspace reader opens (`readers.mount`, and `self.reader` on SQLite),
before the named lookup. A reader that does not open (factory failure,
reader set stopping or at capacity) answered `Unknown` (custody: the
caller is told the write may have landed) for a misspelled project, where
the parent answered `NoSuchProject` from the control-reader lookup, and an
existing Branch answered `Unknown` where the parent answered `Failed`.
Confirmed against source: the new test on the W2 source fails with
`("Unknown", "Store port: ReadAdmission(Capacity)")` for a missing project
(`receipts/W2-fix/before-fix.log`).

Fix: the named private bind reports whether it read the project and
Branch (`Store::bind_named_private` takes a `looked_up` flag the bind sets
just before the named lookup, reset on the one reopen `one_call` allows;
`NamedBindFailure::Bind { refusal, looked_up }`). Not looked up: the
lookups run first on the control reader, as for an admission failure, then
`Failed` with the bind's text. Looked up: `NoSuchProject`, `NoSuchBranch`
and a `Snapshot`-phase read (after the lookup began, the only `Read` in that
phase is the lookup's) keep the lookups' answers, also under a
`BindError::Cleanup` from the reader's close that followed them (the parent
never opened that reader). No new server message, no trip change.

- Test: `sdk_write_named_refusals::sqlite_reader_refusal_keeps_the_lookups_first`
  holds every Workspace reader (`mount_reader` × `CALLS`), then a missing
  project answers `NoSuchProject`, a missing Branch `NoSuchBranch`, the
  existing Branch `Failed` naming the reader capacity; after release the
  write commits. SQLite only: holding every reader on PostgreSQL would hold
  64 connections of the shared test server. The cleanup-failure case has no
  reachable trigger in the test harness (a reader close does not fail on
  demand); it is covered by the unwrapping in `lookup_answer`.
- Checks (`receipts/W2-fix/`): `sdk_write_named_refusals` (3, both
  backends), `sdk_bind_named_fault` (drop and kill sweeps, PG),
  `sdk_write_round_trips` (pins unchanged), `method_refusal_order`: green.
  Clippy `-D warnings` on `layerfs-store-service` clean; rustfmt clean on
  the touched files. Deleted tests: none.

## W6 — plan the parent-path reads (M2)

Base `cfc9586d05` (M1 merged). Re-estimate first (W0 attribution at the M1
tip, PG, `receipts/W6/w0-pg-m1.log`): `files.change` Workspace reader is
warm 0–4 on every pinned case (overwrite 0, new file 2, edit 2, mkdir 0,
cp 3, mv 4, rm 2, apply 2, rm.dir 7), so W6 can save at most ~0–3 warm
there. The mechanism has a real case: `fs.apply` across directories. New
pinned case `apply.10_dirs` (one new file in each of `/src/d00`–`d06`,
`/src/d09`, `/deep/a/b/c/d/e` and `/hist`; 16 directories on 7 levels),
measured at the M1 tip (`receipts/W6/pins-before.log`): PG Workspace
reader [1,58,56,1] cold, [0,16,16,0] warm (`apply.10_writes`, one
directory: 25 and 2). W0 attribution of the same calls on the probe
(temporary local case, not committed; `receipts/W6/w0-dirs-before.log`):
`files.change` Workspace reader 34 cold (26 locates, 8 pack-block reads)
and 12 warm (10 locates, 2 span reads) against 4 and 2 for one directory.
The writer's Save base reads (`construct`) are 18 cold / 1 warm in both,
so they do not grow with directories: nothing to hand them, and
`store/commit.rs`/`store/pipeline.rs` stay untouched.

Today each path component costs a full `SourceView::stat` (inode value,
portable metadata, and for a directory its sub-directory count: a listing
plus one inode wave of its children), and `parent()` stats the parent
directory too, though only its serial is used. W6 does two things:

1. Kind-only walk (both backends, a logic change like W5):
   `SourceView::lookup_kind` binds a name to its serial and kind (overlay
   first, then the base child, as `lookup` does) with no stat; `resolve`
   uses it for every component but the last, and `parent()` for all of
   them. The checks keep their order (name grammar, then `NotDirectory` at
   the entry before it, then `NoSuchPath`).
2. Planned levels (PostgreSQL only, where the writer reports
   `batched_chain_reads`): `write/plan.rs` collects the change set's
   distinct parent directories (in windows of `PLAN_PATHS` = 1,024 changes,
   at most 128 directories or 1 MiB per level demand) and, level by level,
   reads the level's directory pages in one batch and the found children's
   inode values in one `values` wave, into the call's client cache;
   `resolve` then answers from memory. Base reads only: the overlay still
   answers first, so answers do not change.

| Case (PG Workspace reader) | M1 tip | predicted |
|---|---|---|
| `apply.10_dirs.cold` | [1,58,56,1] | ~[1,36,34,1] (32–40) |
| `apply.10_dirs.warm` | [0,16,16,0] | ~[0,9,9,0] (8–11) |
| pinned single-path cases | as pinned | unchanged to −3 (kind-only parent) |

SQLite: kind-only only; `apply.10_dirs` Workspace statements 113 → ~90
cold, 27 → ~20 warm; other cases unchanged or a few fewer. Writer and
control reader unchanged everywhere.

### W6 measured

Built as predicted, with one change found while measuring: a level's inode
wave also takes every child of each directory whose root page holds its
entries and whose child-directory count is not remembered, and remembers
that count (`BaseView::plan_level`). A first cut asked only the walked
names' values and pushed `write.overwrite.cold` from 21 to 23 and
`cp.file.cold` from 29 to 30 (PG). The full stat of the file's parent had
read its siblings' values in the count wave, and the file lookup's
sibling-length step then reused them. A separate count wave for the paths'
parents did not recover them (still 23), because the walked name's own
value wave came first. Folding the children into the level's one wave
did. Bound: `BaseView::PLAN_SERIALS` = 1,024 serials per wave (a listed
page's rows); a directory's names are split into groups of at most that
many.

| Case (PG Workspace reader) | M1 tip | predicted | measured | vs prediction |
|---|---|---|---|---|
| `apply.10_dirs.cold` | [1,58,56,1] | ~[1,36,34,1] (32–40) | [1,34,32,1] | −6% |
| `apply.10_dirs.warm` | [0,16,16,0] | ~[0,9,9,0] (8–11) | [0,8,8,0] | −11% |
| `cp.file.cold` | [1,29,27,1] | unchanged to −3 | [1,28,26,1] | in range |
| every other pinned case | as pinned | unchanged to −3 | unchanged | in range |

PG case totals: `apply.10_dirs` 138 → 114 cold, 48 → 40 warm. Writer and
control reader are unchanged everywhere. W0 at W6 (`receipts/W6/w0-pg-after.log`,
the whole `w0` group plus the temporary `apply.10_dirs` case) matches the M1
tip on every case except `cp.file` cold (104 → 103). `apply.10_dirs` (run
in group order, after the other cases) has `files.change` Workspace reader
17 cold and 5 warm, against 34 and 12 for the same calls run alone at the
M1 tip.

SQLite (kind-only walk; nothing planned), Workspace statements:
`apply.10_dirs` 113 → 109 cold, 27 → 27 warm; `apply.10_writes.cold`
77 → 74; `rm.dir_100_recursive.cold` 70 → 65; `write.overwrite.cold`
73 → 75; `cp.file.cold` 86 → 88. Other cases are unchanged. The
`apply.10_dirs` prediction (~90 / ~20) missed by more than 15%. Diagnosed
once from the W0 demand trace (temporary instrumentation, not committed):
kind-only drops only the metadata object and child-directory count of each
directory a walk passes. Most of those were already shared or remembered.
Each directory still costs its page and its inode value, which the walk
reads either way. The +2 on overwrite and cp is the same sibling-values
effect as on PG, with no plan to recover it. Net over the pinned SQLite
cases: −8 statements.

- Sweep: no new or changed server message. The plan sends the existing
  object-demand templates (locate, span and pack-block reads) on the
  Workspace reader, grouped, so there is no sweep (as for W5). A failed
  plan read refuses at the first change of its window as `Failed` (a read,
  nothing changed), as that change's own walk would have.
- Answers: new `tests/sdk_write_planned_paths.rs` (both backends) covers:
  - writes across depths, `mkdir -p`, `cp` and `mv` across directories and
    an overwrite in one `fs.apply`, each read back;
  - six refusals with exact code, text and index: the name grammar before
    a file parent, a file parent at shallow and deep levels, and a missing
    parent at three depths;
  - 1,100 changes over four directories, past one `PLAN_PATHS` window.

  It passes on the M1 source (`receipts/W6/before/`) and on W6.
- Checks, once at the final source (`receipts/W6/*.log`), all green:
  - write pins (repinned);
  - `sdk_write_planned_paths`, `sdk_round_trip_equivalence_seeded` and
    `method_refusal_order`;
  - `sdk_write_kept_markers`, `sdk_write_named_refusals` and
    `sdk_write_recursive_kinds`;
  - `sdk_method_golden` and `write_refusals`;
  - `sdk_path_whiteouts`, `sdk_path_hidden_semantics`, `sdk_path_reads`,
    `sdk_path_removal` and `sdk_path_merge_removal`;
  - `history_diff_oracle`;
  - workspace `namespace` and `namespace_view`.

  Clippy `-D warnings` (`--all-targets`, both packages) needed one type
  alias (`base::view::Planned`), added after the test run, signature only.
  rustfmt is clean on the touched files. Deleted tests: none.
- Not done: the spec's hand-off to the Save's base reads. W0 shows the
  writer's `construct` at 18 cold and 1 warm, the same with one directory as
  with ten, so there is nothing to hand over. `store/commit.rs` and
  `store/pipeline.rs` are untouched.
- Production LOC: 228,436 -> 228,641 (delta +205), engine
  layerfs-store-service +128 (new `write/plan.rs` 107), layerfs-workspace +77.

### W6 follow-up: the SQLite guardrail (coordinator review)

SPEC §3 SQLite guardrail and verification Principle 9 say SQLite statement
counts change only through dedupe fixes, and no call gets slower. W6's
kind-only walk raised `write.overwrite` cold from 73 to 75 and `cp.file`
cold from 86 to 88. The cause was the one already seen on PG: the parent's
full stat had read the siblings' values, and the file lookup reused them.

Fix: the kind-only walk runs only on a Store that plans its reads
(`batched_reads`, PostgreSQL), where the plan's level wave reads those
values. A Store that plans nothing (SQLite) keeps the M1 walk unchanged
(`Mutation::stat_walk`: full stat of the root and of every component,
including a parent's). No PG count moves.

SQLite Workspace reader statements (`receipts/W6-sqlite/`):

| Case | M1 tip | W6 | after this fix |
|---|---|---|---|
| `write.overwrite.cold` | 73 | 75 | 73 |
| `cp.file.cold` | 86 | 88 | 86 |
| `apply.10_writes.cold` | 77 | 74 | 77 |
| `rm.dir_100_recursive.cold` | 70 | 65 | 70 |
| `apply.10_dirs.cold` (new) | 113 | 109 | 113 |
| `apply.10_dirs.warm` (new) | 27 | 27 | 27 |
| every other case | as pinned | unchanged | unchanged |

Every SQLite pin now equals the M1 tip, and writer and control reader are
unchanged. PG is as in W6 (`apply.10_dirs` [1,34,32,1] / [0,8,8,0],
`cp.file.cold` 28).

- Ran once: `sdk_write_round_trips` (its PG half passed unchanged; SQLite
  repinned from this run), `sdk_write_planned_paths` and
  `method_refusal_order`, all green. Clippy `-D warnings` on
  `layerfs-store-service` is clean, and so is rustfmt. Deleted tests: none.
- Production LOC: 228,641 -> 228,671 (delta +30), engine layerfs-store-service
  +30 (`stat_walk` and the shared `missing` refusal). W6 with this fix:
  228,436 -> 228,671 (+235).

## I3b — a write unit in a writer turn (session capability for lane I)

This change is additive. A `DecidedTurn` (`Session::decided_turn`) can now
carry one write unit: one transaction spread over several flights of the
turn. Each flight is sent once the previous flight's replies are read
(`session/pipeline.rs`). No product path opens a unit yet. Lane I builds on
these entry points:

```rust
pub struct UnitStatements { /* text -> Statement, held for the turn */ }
impl DecidedTurn<'_> {
    pub(crate) fn prepare_unit(&mut self, texts: &[&str]) -> Result<UnitStatements, BackendError>;
    pub(crate) fn send_in_unit<T>(&mut self, prepared: &UnitStatements, requests: &[Request<'_>],
        open: bool, release: &str,
        judge: impl FnOnce(Vec<Result<Reply, Refusal>>, bool) -> (T, bool)) -> Result<T, BackendError>;
    pub(crate) fn count_unit(&mut self, committed: bool);
}
```

- **`prepare_unit`** prepares the unit's texts before `BEGIN`, unscoped,
  in one flight. It costs one round trip, or none when every text is
  cached. A refused prepare is the caller's definite failure, and a lost
  reply quarantines the session. The handles are held for the turn, so a
  cache clear cannot drop one between flights.
- **`send_in_unit`** binds only `prepared` handles. An unknown text or a
  parameter-count mismatch returns `Integrity` before anything is sent.
  - The first flight starts with `write_begin(true)`, the lane row and the
    barrier; the caller sends it as `Request::Simple`.
  - `open` leaves the transaction open after the flight. The last flight
    ends with the unit's own `COMMIT`.
  - On failure, when no reply was lost: `ROLLBACK` plus the release in one
    more round trip, or the release alone after a sent `COMMIT` that ended
    the transaction.
  - A lost reply quarantines the session. Nothing is resent, and no
    `COMMIT` is ever implicit.
- **`count_unit`** counts the unit's one write transaction. Flights are
  counted as ordinary flights.
- **The turn's end** rolls back a unit still open: `ROLLBACK` plus the
  release, one round trip, counted as a rolled-back write. A panic between
  flights quarantines the session (`Drop`).
- **Guards**, all `Integrity` with nothing sent:
  - the turn's ordinary `send` while a unit is open (that is the only
    behaviour change to an existing entry point, and no current caller
    opens a unit);
  - `prepare_unit` while a unit is open;
  - either call inside this thread's held lane.
- **Refactor:** `Session::flight`'s post-send code became `Session::fly`,
  with the failure ending passed in. This is the same ending for every
  existing flight (lost reply or nothing sent: none; partial: `ROLLBACK`
  plus the release; failed: the release). `count_fused` shares
  `count_transaction`.
- **Test hook:** `internals::postgresql_unit` / `UnitTurn`.

Predicted (from the design) vs measured on PG, test
`postgresql_session_unit` (a two-flight unit: `BEGIN` + insert + count with
the transaction left open, then an insert of the count + `COMMIT`):

| Unit | predicted round trips | measured |
|---|---|---|
| first unit (2 texts to prepare) | 1 prepare + 2 flights = 3 | 3 (2 prepares, 5 statements, 1 write commit) |
| same texts again | 2 | 2 (no prepare) |
| second flight refused (duplicate key) | 2 + 1 release = 3 | 3, rows untouched, rollbacks +1, no barrier key left |
| unit left open at turn end | 1 flight + 1 `ROLLBACK` = 2 | 2 (2 statements), rollbacks +1, no commit, no barrier key left |

Sweeps across the two-flight unit, 31 client messages:
- **Drop:** 31 of 31 answer `Unknown`. The rows are the committed result at
  messages 26–30 (after the `COMMIT` was sent) and empty before that.
- **HoldKill:** `Unknown` with empty rows at 0–25, and `Ok` with the
  committed rows at 26–30.
- **Both:** no reconnect, no replay, and every outcome is atomic.

Existing tests the diff reaches, run once each:
- **Green:** `postgresql_pipeline`, `postgresql_mount_pipeline`,
  `mount_fault_drop`, `mount_fault_kill`, `commit_fault_drop`,
  `commit_fault_kill`, `verify768y_release_faults`.
- **`fused_commit` 9/10:** `postgresql_cancelled_barrier_is_released`
  failed (`bindings_of` answered `UnknownOutcome`). Diagnosed once: run
  alone, it passes (`fused_commit_cancel_alone.log`). Two tests in that file
  pick their target backend server-wide (`wait_event = 'PgSleep' AND query
  LIKE '%pg_advisory_lock(%'`): this one cancels it, and
  `postgresql_lost_backend_without_start_is_not_terminated` cuts its proxy
  when it sees one. Run in parallel, either can act on the other's paused
  backend. Those paths are unchanged by this diff (same ending), so this is
  a pre-existing test-isolation follow-up, not fixed here.

Other checks:
- Clippy `-D warnings` on `layerfs-persistence` (all targets) is clean.
  rustfmt is clean on the touched files. `history/admission.rs` has a
  pre-existing fmt diff from the merged base, left alone.
- Deleted tests: none.
- Receipts: `receipts/I3b-session/`.
- Production LOC: 229,872 -> 230,048 (delta +176), engine
  layerfs-persistence +176. This includes the test hook in
  `src/internals.rs`, which the counter counts as product source.
