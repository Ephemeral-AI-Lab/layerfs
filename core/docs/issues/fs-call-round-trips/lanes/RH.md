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
