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
