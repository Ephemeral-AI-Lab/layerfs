# #818 lane RH (reads then history): notes

> **Status:** Working notes of lane RH on branch `rt818-lane-rh`, base
> `54778271cd` (Phase 0 merge). Predictions are written before each item's
> code; measured values come from one run of the item's pins.

Identity: PostgreSQL container `lfs818-rt-pg` (17.11), database `rt818_rh`;
host cargo `+1.85.1`, test profile, private target cloned from the rollout
worktree. Counts are `[connects, round_trips, statements, transactions]` for
the control reader (session 0) unless a role is named; `.cold` is a new Store
open per case, `.warm` the repeat on that open (pins' own definitions).

## M1

### R1 — single-statement Branch-by-name and marker reads (PostgreSQL only)

**Prediction (written before the code).**

- Mechanism: `branch_named` and `path_removals` move from `self.read` (PG:
  BEGIN/LOCK/fence, statement, `SELECT 1; COMMIT` = 3 trips, 1 transaction)
  to `self.read_statement` (one fenced autocommit statement = 1 trip, no
  transaction), with new `{s}`-qualified texts `branch_branch_by_name.sql`
  and `paths_markers.sql`.
- Each Branch-named resolve: −2 trips, −1 transaction. Each marker read
  (`Visibility::for_stack`): −2 trips, −1 transaction.
- SQLite: no change (`read_statement` is the same `run(false)` as `read`).
- Statements (PG column): the transaction wrapper statements leave with the
  transaction; predicted −2 per replaced read (BEGIN/fence and COMMIT), so
  −4 on the read cases. Recorded, not a gate.
- Cold: same −4, plus the read-transaction prepares disappear when no other
  `read` transaction is left in the call (up to −2 more); predicted −4..−6.

| Case (PG control) | baseline warm | pred. warm | baseline cold | pred. cold |
|---|---:|---:|---:|---:|
| every read case (`stat`, `read`, `list`, `glob`, `grep`; Branch `work`) | 18–65 | −4, transactions 2 → 0 | 35–111 | −4..−6, transactions 2 → 0 |
| `hidden.*` (cold only) | — | — | 52–67 | −4..−6 |
| `blame.h10` (full Commit id: no Branch lookup; 1 marker read) | 227 | −2 | 267 | −2..−3 |
| `blame.h100` (Branch `work`) | 2,030 | −4 | 2,270 | −4..−6 |
| `revision.h10` (Commit id) | — | — | 499 | −2..−3 |
| `revision.h100` (Branch) | 3,798 | −4 | 4,026 | −4..−6 |
| `diff_files` (Commit selectors; `for_snap` per side, twice more if extended) | 83 | −4..−8 | 109 | −4..−9 |

- SPEC exit: `stat /` 18 → 14 warm (the probe-equivalent row).
- Writer and Workspace roles unchanged in every read and history pin.
- Outside the lane's pins (reported, not edited): every write call resolves
  its Branch by name and reads markers (`write/files.rs:89`, `:243`), so the
  write pins (lane W's `sdk_write_round_trips.rs`) drop by about 4 too.

**Measured (one run per binary; receipts `receipts/R1/`; 1-minute load 7.8).**

| Case (PG control) | baseline warm → R1 | baseline cold → R1 | predicted | within 15% |
|---|---|---|---|---|
| `stat.root` | 18 → **14** | 35 → 31 | −4 | yes (exact) |
| `stat.depth3` | 37 → 33 | 59 → 55 | −4 | yes |
| `stat.depth7` | 65 → 61 | 91 → 87 | −4 | yes |
| `read.small_depth3` | 38 → 34 | 60 → 56 | −4 | yes |
| `read.2000_lines` / `read.lines_100_200` | 30 → 26 | 50 → 46 | −4 | yes |
| `list.root` | 18 → 14 | 36 → 32 | −4 | yes |
| `list.dir_100` | 31 → 27 | 53 → 49 | −4 | yes |
| `glob.dir_100` / `glob.tree` | 30 → 26 / 20 → 16 | 51 → 47 / 49 → 45 | −4 | yes |
| `grep.one_file_dir` / `grep.tree_1k` | 23 → 19 / 38 → 34 | 43 → 39 / 111 → 107 | −4 | yes |
| `hidden.*` (cold) | — | 59/67/52 → 55/63/48 | −4 | yes |
| `blame.h10` / `blame.h100` | 227 → 225 / 2,030 → 2,026 | 267 → 265 / 2,270 → 2,266 | −2 / −4 | yes |
| `revision.h10` (cold) | — | 499 → 493 | −2..−3 | **no: −6** |
| `revision.h100` | 3,798 → 3,794 | 4,026 → 4,022 | −4 | yes |
| `diff_files` | 83 → 79 | 109 → 105 | −4..−8 | yes |

- Transactions 2 → 0 on every read case (blame.h10 1 → 0, revision.h10 3 → 0).
- PG statements unchanged: the counter counts the read transaction's
  wrapper as trips, not statements, so the predicted −4 statements was
  wrong (trips exact). Cold drops exactly as warm (no extra prepare saved:
  the fence and BEGIN texts are not prepared statements).
- `revision.h10` beyond 15% of its prediction, diagnosed once from the
  transactions column: 3 read transactions before, so its call (Branch
  `work` plus a full Commit id) resolves the Branch by name and reads the
  markers once more than predicted; −2 per read, as everywhere. No fix.
- SQLite: every read and history pin unchanged (both binaries' SQLite
  tests passed unchanged).
- Writer and Workspace roles unchanged in every pin.
- Lane W's write pins (run once, not edited; `receipts/R1/writes-lane-w-pins.log`):
  every case's control reader −4 trips and −2 transactions (warm 10 → 6,
  cold 17 → 13); writer and Workspace unchanged; SQLite unchanged.
- Sweeps (new binaries `sdk_read_fault_rh_{drop,kill}.rs`,
  `support/read_sweep_rh.rs`): the cold `fs.stat` on Branch `work` with a
  marker present, K = 158 server messages (control connect, prepares, every
  statement, both R1 statements included). Drop: 8 exact answers, 145
  `Unknown`, 5 `Failed`; kill: 120 exact answers, 38 `Unknown`; never another
  answer; the same service answered exactly after each fault; no backend
  idle in a transaction. 6.9 s and 8.0 s.
- Checks: `postgresql_statement_schema` (guard + PG runtime, both new
  reads), persistence `branch_removal`, `path_removal`; store-service
  `sdk_path_removal`, `method_refusal_order`,
  `sdk_round_trip_equivalence_seeded` (PG 9.6 s): all green.
- Deleted tests: none.

### R2 — read the Commit row once; serial and markers together (both backends, dedupe)

**Prediction (written before the code).**

- Mechanism:
  1. `snapshot::resolve` keeps the Commit or Layer record its ownership
     check reads and takes the root from it (−1 read per resolve, every
     caller of `resolve`).
  2. The five reads (`stat`, `read`, `list`, `glob`, `grep`) and `blame`
     (through `snapshot_full`) read the node's serial and the project's
     markers in one `node_path_removals` statement at the serial's place;
     `Visibility` takes the markers from it at today's marker-read place, so
     a failure still refuses there (−1).
  3. `paged_reads::pin` builds `at` from the kept record (no Commit or Layer
     row read; a Commit still reads its recorded Branch name) (−1).
  `Snap` keeps its fields (the daemon builds one), so the kept record and
  markers travel beside it from `snapshot_full`.
- PG and SQLite alike (dedupe, D1): −1 statement and −1 trip per removed
  read; SQLite transactions −1 per removed read.

| Case | PG control trips | SQLite control statements (transactions) |
|---|---|---|
| five read cases, cold and warm, `hidden.*` | −3 (`stat.root` 14 → 11 warm) | −3 (−3) |
| `blame.h10`, `blame.h100` | −2 (root, serial+markers; its own `at` is H1's) | −2 (−2) |
| `revision.*` (no `resolve`, no `snapshot_full`) | 0 | 0 |
| `diff_files` (one `resolve` per side) | −2 | −2 (−2) |

- SPEC exit: `stat /` 11 warm.
- Refusal order: the ownership read is where it was; the root read is gone
  (it could only fail where the ownership read succeeded); the combined read
  sits at the serial's place and its failure surfaces at the marker read's
  place. A node with no row, or a provider whose combined read is
  `Unsupported`, falls back to today's serial 0 and the separate marker
  read. One deliberate difference under faults only: today a failed serial
  read became serial 0 and the separate marker read could still succeed;
  now the one read's failure refuses at the marker place (fail closed, as a
  marker failure refuses today).

**Measured (one run per binary; receipts `receipts/R2/`).**

| Case | PG control trips warm (cold) | SQLite control statements warm (cold); transactions | within 15% |
|---|---|---|---|
| `stat.root` | 14 → **11** (31 → 27) | 39 → 30 (47 → 38); 8 → 5 | yes |
| `stat.depth3` | 33 → 30 (55 → 51) | 82 → 73 (95 → 86) | yes |
| `stat.depth7` | 61 → 58 (87 → 83) | 144 → 135 (161 → 152) | yes |
| `read.small_depth3` | 34 → 31 (56 → 52) | 83 → 74 (96 → 87) | yes |
| `list.dir_100` | 27 → 24 (49 → 45) | 67 → 58 (80 → 71) | yes |
| `grep.tree_1k` | 34 → 31 (107 → 103) | 77 → 68 (156 → 147) | yes |
| other read cases | −3 (−4) | −9 (−9); −3 | yes |
| `hidden.stat.removed` (cold) | 48 → 45 | 81 → 75; 6 → 4 | yes (−2 reads: a refusal builds no `at`) |
| `blame.h10` / `blame.h100` | −2 (−2) | −6 (−6); −2 | yes |
| `diff_files` | 79 → 77 (105 → 103) | 193 → 187 (224 → 218); −2 | yes |
| `revision.*` | 0 | 0 | yes |

- Reads removed exactly as predicted (3 per read call, 2 for `blame` and
  `diff_files`). The SQLite statement prediction (−3) counted reads, not
  statements: each SQLite read is a transaction of 3 statements, so −9
  (transactions −3 as predicted). PG cold −4: the merged read replaces two
  statements that each needed a prepare on a new open with one.
- SPEC M1 component: `stat /` 18 → 11 after R1 + R2.
- New SQLite pins (D1, dedupe): every read case −9 statements and −3
  transactions; `hidden.stat.removed` −6/−2; `blame.*` and `diff_files`
  −6/−2. Answers equal (`sdk_round_trip_equivalence_seeded` green).
- Sweeps (rerun: the flight changed): K 158 → 142. Drop: 137 `Unknown`,
  5 `Failed`, **0 answers** (R1: 8); kill: 104 exact answers, 38 `Unknown`.
  The 8 R1 answers came from drops inside the serial read, whose failure
  `resolve` turns into serial 0 while the separate marker read then
  succeeded; now that read carries the markers and its failure refuses at
  the marker step: the fail-closed difference named in the prediction, seen
  only under a fault.
- Checks: `sdk_path_removal`, `method_refusal_order`,
  `sdk_round_trip_equivalence_seeded` green; dependents
  (`layerfs-daemon`, `layerfs-store`, `layerfs-api`, `layerfs-control`,
  `layerfs-bucket`) build.
- Deleted tests: none.

### R3 — one path walk per call (both backends, dedupe)

**Prediction (written before the code).**

- Mechanism: the five methods build the call's `Cached` and one `Memo` over
  it first, walk the path once, strictly, through that memo (at the old
  hidden-path check's place, so a symlink component or a content failure
  refuses where it did), check visibility on the entry found, and pass the
  entry (and the memo) to `reads::{stat,read,list,glob,grep}`, which walked
  again. An absent path is refused `NoSuchPath` where `existing` refused it
  (inside the read, after the arguments read in between: `binary`, the
  glob/grep filters, the page). `hidden_children`/`hidden_matches` look
  their markers up through the same memo. The paged forms (`page`) keep
  their own walk (not in the pins). Applied whether or not markers exist:
  skipping the walk only when the marker set is empty would move the
  symlink-component refusal behind `binary` (read) and the filters
  (glob/grep), an order change; one walk at the early place keeps it.
- Removed: the hidden-path walk on the raw reader (no memo, no call cache).
  Model from the R2 warm counts (both walks together ≈ 6.3–7 trips per
  component): the raw walk ≈ 3 + 5·d trips, the memoized walk that stays
  ≈ 1.5·d. SPEC's −7 per component (−3 at the root) is its upper bound;
  where the two differ the model below is my prediction and SPEC's M1 row
  the expectation.

| Case (PG control) | d | R2 warm | predicted warm | SPEC M1 (probe-like) | R2 cold | predicted cold |
|---|---:|---:|---:|---:|---:|---:|
| `stat.root` | 0 | 11 | 8 | 8 | 27 | 24 |
| `stat.depth3` | 3 | 30 | 12 | 22 | 51 | 33 |
| `stat.depth7` | 7 | 58 | 20 | ~34 | 83 | 45 |
| `read.small_depth3` | 3 | 31 | 13 | 10 | 52 | 34 |
| `read.2000_lines`, `read.lines_100_200` | 2 | 23 | 10 | — | 42 | 29 |
| `list.root` | 0 | 11 | 8 | — | 28 | 25 |
| `list.dir_100` | 2 | 24 | 11 | 12 | 45 | 32 |
| `glob.dir_100` | 2 | 23 | 10 | — | 43 | 30 |
| `glob.tree` | 0 | 13 | 10 | — | 41 | 38 |
| `grep.one_file_dir` | 1 | 16 | 8 | — | 35 | 27 |
| `grep.tree_1k` | 0 | 31 | 28 | 68 (probe 78 → −10) | 103 | 100 |
| `hidden.stat.depth3` (cold) | 3 | — | — | — | 51 | 33 |
| `hidden.list.dir_100` (cold; its marker looked up through the memo) | 2 | — | — | — | 59 | 41 |
| `hidden.stat.removed` (cold; one walk before too, now memoized) | 3 | — | — | — | 45 | 40 |

- SQLite: the raw walk's object reads leave; predicted the same share of the
  object-read statements as PG (about −50% of the walk-bound statements on
  the deep cases); exact counts recorded, new pins listed (D1).
- History pins: unchanged (blame, revision and diff do not use these
  methods' walk).
- Memory: none beyond the call's existing `Cached` (3 MiB bound) and one
  `Memo` (256 objects), both already built per call by every read.

**Measured (one run per binary; receipts `receipts/R3/`).**

| Case (PG control) | R2 warm → R3 | pred. warm | R2 cold → R3 | pred. cold | within 15% of prediction |
|---|---|---:|---|---:|---|
| `stat.root` | 11 → **8** | 8 | 27 → 24 | 24 | yes |
| `stat.depth3` | 30 → **9** | 12 | 51 → 30 | 33 | no (−25%; better) |
| `stat.depth7` | 58 → **9** | 20 | 83 → 34 | 45 | no (−55% / −24%; better) |
| `read.small_depth3` | 31 → **10** | 13 | 52 → 31 | 34 | no (−23%) / yes |
| `read.2000_lines`, `read.lines_100_200` | 23 → 9 | 10 | 42 → 28 | 29 | yes |
| `list.root` | 11 → 8 | 8 | 28 → 25 | 25 | yes |
| `list.dir_100` | 24 → **10** | 11 | 45 → 31 | 32 | yes |
| `glob.dir_100` | 23 → 9 | 10 | 43 → 29 | 30 | yes |
| `glob.tree` | 13 → 10 | 10 | 41 → 38 | 38 | yes |
| `grep.one_file_dir` | 16 → 9 | 8 | 35 → 28 | 27 | yes |
| `grep.tree_1k` | 31 → **28** | 28 | 103 → 100 | 100 | yes |
| `hidden.stat.depth3` | — | — | 51 → 30 | 33 | yes |
| `hidden.list.dir_100` | — | — | 59 → 31 | 41 | no (−24%; better) |
| `hidden.stat.removed` | — | — | 45 → 27 | 40 | no (−33%; better) |

- Diagnosis of the misses (once, from the per-case counts): the savings are
  exactly SPEC's model, −3 at the root and −7 per component, on warm and
  cold alike (depth 3 −21, depth 7 −49, depth 2 −14, depth 1 −7, root −3).
  My split was wrong: the walk that stays costs about nothing per component
  on this tree (`stat` 9 warm at depth 3 and at depth 7), because after its
  first demand the call's `Cached` holds the packs of the upper directory
  and inode pages, while the removed walk on the bare reader paid every
  page. A tree whose pages spread over more packs will still pay per
  component; the held-out depth-12 / 10k-entry shapes at the milestone
  will show it. `hidden.list.dir_100` also saves its marker's lookup (now
  through the memo, −14), `hidden.stat.removed` its strict walk's raw reads.
  No fix: the removed reads are the duplicate ones.
- Against SPEC §6.3 M1 (probe-like rows; the pins' `.warm` equals the probe
  for `stat /` and `read` small): `stat /` 8 (expected 8); `read` small 10
  (10); `list` 100 10 warm (12); `stat` depth 3 9 warm / 30 cold (22);
  depth 7 9 / 34 (~34); `grep` 1k 28 warm / 100 cold (68 probe-like; the
  probe's row sits between, the milestone benchmark measures it).
- SQLite (new pins, D1): warm `stat` 30 → 22 (root), 73 → 23 (depth 3),
  135 → 23 (depth 7); `read.small_depth3` 74 → 24; `list.dir_100` 58 → 24;
  `glob.dir_100` 57 → 23; `glob.tree` 32 → 24; `grep.one_file_dir` 40 → 23;
  `grep.tree_1k` 68 → 60; cold alike (−8 to −112); `hidden.stat.depth3`
  86 → 36, `hidden.list.dir_100` 104 → 37, `hidden.stat.removed` 75 → 32.
  Transactions unchanged (object reads are not transactions).
- History pins unchanged (run once: all equal; formula check passed).
- Refusal order: kept by walking once at the old check's place (decision
  above); `method_refusal_order`, `sdk_path_reads`, `stat_symlink`,
  `sdk_path_hidden_semantics`, `sdk_path_whiteouts`, `sdk_method_golden`,
  `sdk_round_trip_equivalence_seeded` green. One non-observable move: the
  call's `Cached` is built before the walk, not after (its construction
  fails only on allocation).
- Sweeps rerun (flight changed): K 142 → 102; drop 97 `Unknown`, 5
  `Failed`; kill 64 exact answers, 38 `Unknown`; invariants held.
- Other tests the diff reaches, run once, green: `sdk_fs_reads_replay`,
  `sdk_grep_reads`, `sdk_list_entries`, `sdk_text_scan`,
  `sdk_large_file_budget`, `sdk_reads_during_publish`,
  `sdk_reads_under_publish`, `sdk_path_merge_removal`, `fs_mode`,
  `saved_namespace_pages`, `write_refusals`, `sdk_path_removal`.
- Test adapted (not deleted): `sdk_text_scan.rs` calls `reads::read`
  directly; it now builds the call's `Cached` and its one lookup first, as
  `fs.read` does.
- Deleted tests: none.
- Milestone checks at the R3 identity: Clippy `-D warnings` on
  `layerfs-store-service` and `layerfs-persistence` (all targets) clean after
  boxing `NodeRecord::Commit` (R2's enum; `large_enum_variant`), folded
  into this commit; read pins rerun once after it (`reads-final.log`,
  green). `cargo fmt --check` differs only in files this lane did not touch
  (`persistence/src/history/admission.rs`, store-service
  `tests/refresh_lookups.rs`, `tests/support/view.rs`, `tests/verify768y_*`),
  left as they are.

## M2 (base: merged M1 tip `cfc9586d05`; read and write pins green there)

Targets (RECORD rule): warm = SPEC target; cold = SPEC target + (cold − probe)
from the Phase 0 table.

### R4 — one `Cached` for the whole call (both backends)

**Prediction (written before the code).**

- Mechanism: the five reads (and `blame`) build the call's `Cached` before
  the snapshot resolves and open the root's `TreeTable` through it; `size()`
  of a file in `stat`/`read` and `Cached::roots`' lengths (grep) read through
  the same cache (`Storage::lengths_with` beside `read_with`,
  `ReadSession::lengths_with`, `StorePorts::lengths_with`).
- Saves a demand only where the root object, the file's state object or the
  lengths' objects share a pack the call already holds. Predicted per case:
  0 to −2 PG trips (TreeTable::open's pack is the one the walk's first page
  sits in on this small tree: −1 likely everywhere; a file's length −0/−1
  on `stat`/`read` files); SQLite: the same reads' statements. No case may
  rise. SPEC counts R4 as 0.
- Memory: the existing per-call `Cached` (3 MiB bound) only.

**Measured (receipts `receipts/R4/`).** PG: every read case unchanged (0,
within the predicted 0..−2). SQLite: −1 statement on every read case, cold
and warm (`stat.root` 22 → 21 warm; new pins, D1). History pins unchanged
(`blame` opens its root through a cache of its own until H3). No new
message: no sweep. `method_refusal_order`, `sdk_round_trip_equivalence_seeded`,
`sdk_text_scan`, `sdk_path_reads`, `stat_symlink`, `sdk_grep_reads`,
`sdk_large_file_budget` green. `sdk_text_scan.rs` follows `reads::read`'s
signature (no reader argument). Deleted tests: none.

### R3 review fix — caches made at the first demand (`4cc9974dd3`)

The M1 review found `Cached::new` (decode-arena allocation) ahead of the
hidden-path check. `Cached` now makes its caches at the call's first demand
(the root's open, where the parent's reader made its arena). Read pins
unchanged; `method_refusal_order`, `sdk_text_scan` green. `Cached::new` keeps
its `Result` shape (control's `project/export/walk.rs` calls it).

### R5 — one context statement per read call (PostgreSQL; SQLite default)

**Prediction (written before the code).**

- Mechanism: `HistoryCatalog::read_context(project, target)` with target
  `main`, a Branch name or a full node id. Persistence overrides it: on
  PostgreSQL one fenced `read_statement` (`read_context_named` /
  `read_context_node`) returns the stack, the Branch, the head node's record
  with its `history_serial`, the Commit's recorded Branch (`branch_any`) and
  the stack's markers (UNION ALL rows; PG records carry no arrays). SQLite
  calls `layerfs_history::catalog::read_context_serial`: today's
  `layer_stack_named`, `branch_named`, `commit`/`layer` in today's order, and
  leaves the markers and the recorded Branch unread, so the store service
  reads them where it does today (statement counts and order unchanged,
  refusal cases included). The commit-prefix path, invalid names and
  unparsable ids keep today's flow.
- PostgreSQL warm, every successful read case: −4 trips and −4 statements
  (project, Branch, Commit row, serial+markers, recorded name → 1):
  `stat.root` 8 → 4, `stat.depth3` 9 → 5, `stat.depth7` 9 → 5,
  `read.small_depth3` 10 → 6, `read.2000_lines`/`lines_100_200` 9 → 5,
  `list.root` 8 → 4, `list.dir_100` 10 → 6, `glob.dir_100` 9 → 5,
  `glob.tree` 10 → 6, `grep.one_file_dir` 9 → 5, `grep.tree_1k` 28 → 24.
- PostgreSQL cold: −4 executes and −4 first-use prepares (5 statements
  prepared today, 1 after): −8 trips, −4 statements, e.g. `stat.root`
  24 → 16, `stat.depth3` 30 → 22, `grep.tree_1k` 100 → 92;
  `hidden.stat.depth3`/`hidden.list.dir_100` −8; `hidden.stat.removed`
  (refused, no recorded name read today) −3 executes −3 prepares = −6.
- Targets (RECORD rule): warm `stat /` 4 = SPEC 4; cold `stat /` 16 =
  4 + (24 − 8 − 4 = 12 cold overhead) — on target if measured.
- History pins: `blame` resolves through the same path: −3 executes
  (project, Branch, Commit, markers → 1; blame reads no recorded name there),
  warm −3; cold −3 executes and −2 prepares (the Commit statement stays in
  use) = −5. `revision`, `diff_files` unchanged.
- SQLite: every pin unchanged (read and history).
- Memory: the stack's markers, as `path_removals` loads them today; the row
  count is one context row plus one row per marker of the stack (no cap: the
  marker set is what today's statement already loads).
- New messages `read_context_named`, `read_context_node`: drop and kill sweeps.

**Measured (receipts `receipts/R5/`).**

| Case (PG control) | before warm / cold | predicted | measured warm / cold |
|---|---|---|---|
| `stat.root` | 8 / 24 | 4 / 16 | 4 / 16 |
| `stat.depth3` | 9 / 30 | 5 / 22 | 5 / 22 |
| `stat.depth7` | 9 / 34 | 5 / 26 | 5 / 26 |
| `read.small_depth3` | 10 / 31 | 6 / 23 | 6 / 23 |
| `list.dir_100` | 10 / 31 | 6 / 23 | 6 / 23 |
| `grep.tree_1k` | 28 / 100 | 24 / 92 | 24 / 92 |
| `hidden.stat.removed` | — / 27 | — / 21 | — / 21 |
| `blame.h10` | 223 / 263 | 220 / 258 | 221 / 260 |
| `blame.h100` | 2,024 / 2,264 | 2,021 / 2,259 | 2,021 / 2,259 |

- Every read case −4 warm, −8 cold, as predicted (all 27 pins); statements
  −4 each. `blame.h10` names a full Commit id (no Branch read): −2 warm,
  −3 cold, one fewer than predicted (within 15%). `revision`, `diff_files`
  unchanged. SQLite: every read and history pin unchanged.
- Targets (RECORD rule): warm `stat /` 4 = 4; `stat.depth7` cold 26 ≤ 30 +
  18; `grep.tree_1k` warm 24 ≤ 64, cold 92 ≤ 64 + 33.
- Sweeps (the two new statements): `sdk_read_fault_rh_{drop,kill}` now run
  the cold `fs.stat` twice, by Branch (`read_context_named`) and by the head
  Commit's full id (`read_context_node`): K 102 → 75 each; drop 70
  `Unknown`, 5 `Failed`; kill 46 exact answers, 29 `Unknown`; both shapes
  alike; invariants held.
- Equivalence: `postgresql_statement_schema` (guard pins `read_context`;
  runtime evidence compares the one statement with the default's reads plus
  `node_path_removals` and `branch_any` for main, a Branch, a missing
  Branch, a Commit, a Layer, a missing Commit and a missing project).
  New `sdk_read_context_rh.rs`: 18 `fs.stat` shapes (no project, no Branch,
  identity-shaped names, missing Commit/Layer, path refusals after them,
  prefixes, answers' `at`) give the same outcomes in the same order on
  SQLite and PostgreSQL.
- `method_refusal_order`, `sdk_round_trip_equivalence_seeded` and the tests
  the diff reaches green, one run each.
- Deleted tests: none.

### H1 — carry rows forward; read each reference once (both backends)

**Prediction (written before the code).**

- Mechanism: one per-call `Rows` (store-service `history/rows.rs`) holds the
  last few Commit/Layer records read (a small recent window, 8 rows) and a
  Branch-name memo `HashMap<BranchId, Option<String>>` (bounded by distinct
  Branches met, ≤ `MAXIMUM_QUERY_WORK`). `operator::files::revision`,
  `saved::reference` and `saved::record` become `Rows` methods (the free
  functions keep their reads, with a fresh `Rows`). Blame and revision use one
  `Rows` per call, seeded with the record (and, from R5's context, the
  recorded Branch) the resolve already read; blame builds each owner's `ref`
  from the row it held when it assigned the owner, and carries the older
  version's `Text` to the next comparison (keyed by content root).
- Per walked version today: `revision(node)` and `revision(parent)` each read
  a Commit row and its Branch name (4 reads), + the parent's serial. After:
  the parent's row only (+ its serial): −3 per version. Owners: −2 per
  distinct owner line (Commit row + Branch name). `at`: −2 (seeded). Text
  carry: −1 text read per changed pair (≈ 1–3 object trips each).
- PostgreSQL control, warm (cold alike, ±prepares):
  - `blame.h100`: ~100 versions × −3, 51 owners × −2, at −2, ~50 texts ×
    −1..−3: ≈ −450..−550 → 2,021 → ~1,520 (cold 2,259 → ~1,760).
  - `blame.h10`: 10 × −3, 6 owners × −2, at −2, 5 texts: ≈ −50..−60 →
    221 → ~165 (cold 260 → ~205).
  - `revision.h100`: per node −2 (parent row and root read once; the node's
    row carried), per item (50) −7 (record's parentRef, baseLayer, ref and
    comparedTo from held rows and the name memo), at −2: ≈ −550 → 3,794 →
    ~3,240 (cold 4,022 → ~3,470).
  - `revision.h10` (cold only): 10 × −2, 5 items × −7, −2: ≈ −57 → 493 → ~436.
  - `diff_files`: unchanged (H3).
- SQLite: each saved read is 3 statements (its transaction): ≈ 3× the read
  savings, plus the text statements: `blame.h100` −~1,400, `blame.h10`
  −~160, `revision.h100` −~1,650, `revision.h10` −~170 (new pins, D1/O2).
- Writer role: unchanged.
- Formula check (bounds only) must still hold; it can only get easier.
- No new server message: no sweep. Answers identical (`owner`, `origins`,
  `commit`/`message`, `coverage`, `comparisonCoarse`, revision items).

**Measured (receipts `receipts/H1/`).** PostgreSQL trips, writer + control:

| Case | before warm (cold) | predicted Δ | after warm (cold) | Δ |
|---|---|---|---|---|
| `blame.h100` | 199 + 2,021 (200 + 2,259) | −450..−550 | 0 + 1,768 (0 + 2,005) | −452 (−454) |
| `blame.h10` | 19 + 221 (20 + 260) | −50..−60 | 0 + 193 (0 + 231) | −47 (−49) |
| `revision.h100` | 92 + 3,794 (93 + 4,022) | −550 | 0 + 3,161 (0 + 3,389) | −725 (−726) |
| `revision.h10` (cold) | 11 + 493 | −57 | 0 + 420 | −84 |
| `diff_files` | 0 + 77 (0 + 103) | 0 | unchanged | 0 |

- The Branch-name reads were on the writer's session (`store.history()` in
  `operator::files::revision`), not the control reader, which the
  prediction assumed: the name memo now reads them once on the control
  reader (`published`, as `saved::reference` did), so writer → 0 and the
  control column falls less than the total. Totals: blame within the
  predicted range (h10 −47 vs −50..−60, within 15%); revision −725 / −84
  vs −550 / −57, beyond 15% and better. Diagnosed once: each item's record
  re-read 5 rows (Commit, parent, base Layer, `ref`, `comparedTo`), not the
  4 counted, plus the writer's per-node Branch name. Recorded, no change.
- SQLite (writer + control): `blame.h100` 597 + 6,746 → 0 + 5,829 warm
  (−1,514; cold 597 + 7,102 → 0 + 6,185), `blame.h10` 57 + 621 → 0 + 548,
  `revision.h100` 276 + 12,382 → 0 + 10,483 (−2,175), `revision.h10` cold
  30 + 1,325 → 0 + 1,106; `diff_files` unchanged. New pins (D1, O2).
- Formula check holds. Answers: `sdk_blame_coarse`, `sdk_file_history`,
  `history_surface`, `operator_history`, `sdk_history_depth`,
  `history_wire_codes`, `sdk_method_golden` (blame golden lines),
  `sdk_round_trip_equivalence_seeded`, `method_refusal_order`,
  `verify_history_readback`, `sdk_compare` (attribution),
  `history_ancestry_oracle`, `sdk_file_compare_budget` green.
- No new server message: no sweep. Deleted tests: none.

### H2 — revision walks once (both backends)

**Prediction (written before the code).**

- Mechanism: `fs.revision`'s walk reads node N's `now` and its parent's
  `before` (a path walk from each root, and each one's serial when the
  entry exists); the next node is that parent, so its `now` and serial are
  the carried `before` and serial (kept before the hidden filter). A
  `hidden_before` stop ends the walk, so nothing is carried there. The same
  carry in `fs.list_revisions` (`operator/files.rs`, not pinned).
  `comparedTo` already costs no read since H1 (held row, name memo).
- Per walked node: −1 path walk (root open + two lookups + value through
  the bare reader, ≈ 10–12 trips) and −1 serial: ≈ −11..−13 per node.
  - `revision.h100` warm control 3,161 → ~1,860 (range 1,860–2,060); cold
    3,389 → ~2,090.
  - `revision.h10` cold 420 → ~295.
  - `blame`, `diff_files`: unchanged.
- SQLite: the same walk's statements and serial reads per node: about
  −20..−40 per node: `revision.h100` 10,483 → ~7,000–8,500; `revision.h10`
  1,106 → ~800–900.
- Invariants: iteration order unchanged, so `candidateIndex`, boundaries,
  continuation tokens and `revision_pages` are unchanged;
  `sdk_file_history.rs` pages pass unchanged. No new message: no sweep.

**Measured (receipts `receipts/H2/`).**

| Case | before | predicted | measured |
|---|---|---|---|
| PG `revision.h100` warm (cold) | 3,161 (3,389) | ~1,860–2,060 (~2,090) | 1,705 (1,933) |
| PG `revision.h10` cold | 420 | ~295 | 260 |
| SQLite `revision.h100` warm (cold) | 10,483 (10,827) | ~7,000–8,500 | 5,797 (6,141) |
| SQLite `revision.h10` cold | 1,106 | ~800–900 | 634 |

- −14.6 trips per node on PostgreSQL (−16 at h10), beyond 15% of my
  −11..−13 and better. Diagnosed once: a path walk through the bare reader
  costs ≈ 13.5 trips here (root open, two lookups, value, pooled leaves),
  not 10–12; SPEC's ≈ −13 per node is within 15%. Recorded, no change.
- `blame`, `diff_files`, writer: unchanged. Formula check holds.
- `sdk_file_history` (pages unchanged), `history_surface`,
  `operator_history`, `sdk_history_depth`, `history_wire_codes`,
  `sdk_method_golden`, `sdk_round_trip_equivalence_seeded`,
  `method_refusal_order`, `verify_history_readback` green. No new message:
  no sweep. Deleted tests: none.

### H3 — one per-call cache across versions, plus a directory memo (both backends)

**Prediction (written before the code).**

- Mechanism: `fs.blame`, `fs.revision`, `fs.diff_files` (and the operator's
  `fs.list_revisions`/`fs.get_storage`) read every version's objects
  through one call `Cached` (pack bodies, decoded groups, 3 MiB bound):
  roots, inode pages, directory pages, sizes (`lengths_with`), text windows,
  ranges, metadata and byte comparisons. Version walks bind names through a
  per-call directory memo `(DirectoryRoot, PathName) → Option<serial>`
  (bounded at 4,096 entries; the shape of lane C's `DirectoryMemo`: `get`
  gives unknown / known-unbound / bound, `insert` refuses when full), so an
  unchanged directory's pages are not read again. `diff_files` keeps the
  strict lookup it made (`raw`) instead of walking again when
  `allowMissing` is false. Store reads (rows, serials) unchanged.
- PostgreSQL control, warm (cold alike, plus first-use prepares unchanged):
  - `blame.h100` 1,768 → ~900–1,100 (a version ≈ 17 → ≈ 8: its new root,
    new inode branch and leaf, values, text through shared packs).
  - `blame.h10` 193 → ~110–130.
  - `revision.h100` 1,705 → ~850–1,000; `revision.h10` cold 260 → ~150–180.
  - `diff_files` 77 → ~50–60 warm (cold 103 → ~75–85).
- SQLite: the same page reads go: `blame.h100` 5,829 → ~3,000–3,500,
  `revision.h100` 5,797 → ~3,000–3,500, `diff_files` 187 → ~130 (new pins).
- Read pins (`sdk_read_round_trips`): unchanged (their walk is unchanged).
- Exit check: page reads per Commit on `blame.h100` ≤ 8 (control trips per
  version after the rows: (trips − fixed) / versions).
- Memory: the existing `Cached` bound (3 MiB) and the memo (≤ 4,096 names of
  ≤ 255 bytes), both dropped at call end. No new message: no sweep.

**Measured (receipts `receipts/H3/`).**

| Case | PG warm (cold) before → after | predicted | SQLite warm (cold) before → after |
|---|---|---|---|
| `blame.h100` | 1,768 (2,005) → 254 (491) | ~900–1,100 | 5,829 (6,185) → 779 (1,135) |
| `blame.h10` | 193 (231) → 33 (71) | ~110–130 | 548 (589) → 91 (132) |
| `revision.h100` | 1,705 (1,933) → 253 (481) | ~850–1,000 | 5,797 (6,141) → 769 (1,113) |
| `revision.h10` (cold) | 260 → 82 | ~150–180 | 634 → 147 |
| `diff_files` | 77 (103) → 23 (49) | ~50–60 | 187 (218) → 71 (102) |

- Far beyond 15%, better. Diagnosed once: the prediction priced a version
  at ≈ 8 page reads (SPEC's "new root, inode branch and leaf, values"), but
  a Commit's new objects sit in one pack and its delta chain's bases in
  packs the call already read, so after the first versions a version costs
  its Commit row, its serial and about 0.5 object trips: (254 − 54) / 100 ≈
  2 control trips per version on `blame.h100`. Exit check (≤ 8 page reads
  per Commit) holds. Recorded, no change.
- Answers: the eleven history calls of the pinned scenario (blame h10, h100,
  `/hist/other.txt`; revision h10, h100, page size 7, other.txt; diff_files
  with and without attribution) dumped on SQLite at R5 (`de237565ee`, before
  H1) and at H3, ids and times masked: identical (sha256 prefix
  `70ba09470f703fa3` both). The dump test was temporary, not committed.
- Read pins unchanged (their walk binds through the tree as before).
  `sdk_blame_coarse`, `sdk_file_history`, `history_surface`,
  `operator_history`, `sdk_history_depth`, `history_wire_codes`,
  `sdk_method_golden`, `sdk_round_trip_equivalence_seeded`,
  `method_refusal_order`, `verify_history_readback`, `sdk_compare`,
  `history_ancestry_oracle`, `sdk_file_compare_budget`,
  `history_diff_oracle`, `sdk_diff_size`, `sdk_path_reads`, `sdk_text_scan`,
  `sdk_fs_reads_replay` green.
- Test adapted (not deleted): `sdk_file_compare_budget.rs` calls
  `file_facts::changes_bounded`, which now takes the call's `Cached`.
- The directory memo has lane C's `DirectoryMemo` shape (`get` → unknown /
  known-unbound / bound, `insert` → `false` when full); H5 wiring can swap
  in lane C's type. No new message: no sweep. Deleted tests: none.

### M2 milestone checks

Clippy `-D warnings` (all targets) on `layerfs-history`, `layerfs-storage`,
`layerfs-persistence`, `layerfs-store-service`: clean after two fixes in a
separate commit (needless borrows in `read/file_compare.rs` from H3; a
duplicated `allow(dead_code)` in the new `sdk_read_context_rh.rs`).
`cargo fmt --check` differs only in files this lane did not touch (as at
M1). History pins, `sdk_read_context_rh`, `sdk_file_compare_budget` rerun
green (`receipts/M2-lint/`). Host free space 195 GB.

## M3 (base: merged M2 tip `7a79aaaea5`)

### H4 — windowed first-parent chain read (PostgreSQL; SQLite default)

**Prediction (written before the code).**

- Mechanism: `HistoryCatalog::first_parent_window(head, count)` returns up
  to `count` first-parent Commits from `head`, each a `ChainRow` (full
  `CommitRecord` + `history_serial`), each row its own result so a bad row
  refuses only when a walk reaches it (rows past where blame stops are never
  surfaced). Default `first_parent_window_serial`: `commit()` +
  `node_history_serial` per step (Unsupported → 0). PostgreSQL: one fenced
  statement `lineage_chain_window` (recursive over runs through the
  `commit_runs` (run, depth) index; a run's first Commit continues in its
  parent's run), rows checked in Rust (first row is `head`, each row's id
  is the previous row's parent, else `Integrity` on that row). The store
  service's `Rows` prefetches windows of 16, 32, 64, then 128 rows, clipped
  to the walk's remaining budget, only where the Store batches reads
  (`batched_reads()`: PostgreSQL), and serves blame's and revision's parent
  rows and serials from them. SQLite: unchanged (no window is read).
  Branch names stay H1's per-call memo (one read per distinct Branch; the
  SPEC's `= ANY` names statement is not needed for that).
- PostgreSQL control, warm (cold +1 prepare):
  - `blame.h100` 254 → ~55–65 (−2 per version, +3 windows).
  - `blame.h10` 33 → ~14–16 (one 16-row window).
  - `revision.h100` 253 → ~55–65; `revision.h10` cold 82 → ~62–66.
  - `diff_files`, read pins: unchanged. SQLite: unchanged everywhere.
- Memory: at most 128 Commit rows held ahead per call.
- New message `lineage_chain_window`: drop and kill sweep (a cold
  `fs.blame` on PostgreSQL).

**Measured (receipts `receipts/H4/`).** PostgreSQL control (writer 0):

| Case | before warm (cold) | predicted | after warm (cold) | within 15% |
|---|---|---|---|---|
| `blame.h100` | 254 (491) | ~55–65 | 59 (296) | yes |
| `blame.h10` | 33 (71) | ~14–16 | 16 (54) | yes |
| `revision.h100` | 253 (481) | ~55–65 | 72 (301) | yes (+11% over 65) |
| `revision.h10` (cold) | 82 | ~62–66 | 66 | yes |
| `diff_files` | 23 (49) | unchanged | unchanged | yes |

- `revision.h100` saves −1.8 a node, not −2: `fs.revision` reads a node's
  serial only where the path exists there, so the window saves a serial
  read only at those nodes. Diagnosed once; recorded, no change.
- SQLite: every pin unchanged (no window is read; `walked` and `serial`
  fall back to today's reads). Formula check holds.
- Answers: the eleven calls of the pinned scenario dumped on SQLite and
  PostgreSQL at H4 and on SQLite at H3, ids and times masked: identical
  (sha256 prefix `70ba09470f703fa3`, all three). Temporary dump test, not
  committed.
- `first_parent_window` equals its default on random histories (forks from
  the middle of a run, merges, publications, removed Branches; sizes 0, 1,
  3, 16, 32, 400; an absent Commit), SQLite and PostgreSQL
  (`persistence/tests/first_parent_window.rs`). The first PostgreSQL run
  refused Store creation `Capacity` (Docker guest disk full; reported,
  rerun after the space was restored).
- Sweep (new binaries `sdk_history_fault_rh_{drop,kill}.rs`, the shared
  `support/read_sweep_rh.rs` now takes the call): a cold `fs.blame` over
  18 Commits (two windows), K = 492 messages: drop 95 `Failed` + 397
  `Unknown`; kill 439 ok + 5 `Failed` + 48 `Unknown`; each armed answer is
  the clean one or a failed-read refusal, the same service and a fresh one
  answer exactly, no backend left in a transaction. The stat sweeps rerun
  green after the refactor.
- `postgresql_statement_schema` (READS entry `chain_window`),
  `sdk_round_trip_equivalence_seeded`, `method_refusal_order`,
  `sdk_blame_coarse`, `sdk_file_history`, `history_surface`,
  `operator_history`, `sdk_history_depth`, `history_wire_codes`,
  `sdk_method_golden`, `verify_history_readback`, `sdk_compare`,
  `history_ancestry_oracle`, `sdk_file_compare_budget` green. Clippy
  `-D warnings` (all targets) clean on history, persistence, store-service.
- `blame::blame` no longer takes the Store (its serials come from `Rows`).
- Deleted tests: none.
