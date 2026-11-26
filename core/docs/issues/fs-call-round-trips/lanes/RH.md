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
