# Lane I (Init) notes — #818

> **Status:** Working notes for lane I; measured values are one sample each.

Branch `rt818-lane-i`, base `54778271cd` (Phase 0 merge). PostgreSQL database
`rt818_i` in container `lfs818-rt-pg` (port 55818). Pins:
`engine/crates/layerfs-store-service/tests/init_round_trips.rs` and
`init_round_trips_10k.rs`. Counts are test profile; Init timing rows are
release builds (D15) and are run by the orchestrator per milestone.

Counts below are `[connects, round_trips, statements, transactions]` of the
Store's one session (SQLite `[0, 0, statements, transactions]`).

## I1 — logical size from the scan

### Prediction (written before coding)

Mechanism: delete Init's `logical_size` walk (tree open, inode-leaf windows,
one root open per regular file). Saves the per-file locates past the
4,096-entry locator cache at 10k, plus leaf and pooled-value reads.

| Shape | PG trips base → predicted | SQLite stmts base → predicted |
|---|---|---|
| `nested_1k` | 229 → ~205 | 308 → ~290 |
| `flat_1k` | 2,246 → ~2,225 | 2,284 → ~2,270 |
| `hardlink_window` | 705 → ~700 | 487 → ~482 |
| `dir_600` | 1,438 → ~1,425 | 1,428 → ~1,415 |
| `nested_10k` | 10,955 → ~606 | 12,164 → ~2,000 |
| `flat_10k` | 31,039 → ~20,700 | 31,881 → ~21,700 |

Roles: Init uses one Store session; all of the cut is on that session's reads.
Transactions unchanged (the walk's reads add no write transactions; PG reads
may be autocommit, so any change is a recorded finding). Logical, canonical
and root unchanged. Expect the 10k run-to-run spread to vanish (exact pins).

### Measured (test profile, one sample; load 5.7)

Receipts: `receipts/I1/init_round_trips*.log` (first run against the base
pins) and `receipts/I1/check/` (the check set after the pins moved).

| Shape | PG trips base → predicted → measured | SQLite stmts base → predicted → measured | Within 15% |
|---|---|---|---|
| `nested_1k` | 229 → ~205 → **209** | 308 → ~290 → **288** | yes |
| `flat_1k` | 2,246 → ~2,225 → **2,226** | 2,284 → ~2,270 → **2,264** | yes |
| `hardlink_window` | 705 → ~700 → **704** | 487 → ~482 → **486** | yes |
| `dir_600` | 1,438 → ~1,425 → **1,426** | 1,428 → ~1,415 → **1,416** | yes |
| `nested_10k` | 10,955 → ~606 → **624** | 12,164 → ~2,000 → **1,837** | yes (+3% / −8%) |
| `flat_10k` | 31,039 → ~20,700 → **20,720** | 31,881 → ~21,700 → **21,560** | yes |
| `mixed` (new) | — → **255** | — → **175** | new case |

- PG full rows: nested_1k `[0,209,98,33]`, flat_1k `[0,2226,2105,37]`,
  hardlink `[0,704,491,82]`, dir_600 `[0,1426,1305,37]`, mixed
  `[0,255,129,38]`, nested_10k `[0,624,335,120]`, flat_10k
  `[0,20720,20381,144]`. Transactions and prepares unchanged; every cut is
  one statement = one trip (autocommit reads on the one session).
- Logical, canonical bytes and roots identical to base on both backends;
  `logical == logical_size(reader, root)` holds in every case, including
  `mixed` (hard links within and across directories, a hard-linked empty
  file, tiny/whole/chunked files, three symlinks, empty directories:
  1,100,005 bytes = 5 + 2 × 100,000 + 3 × 300,000).
- **10k variance:** gone. The two runs (pin run and check run) gave identical
  10k counts on both backends, so the run-to-run spread was the walk
  (locator-cache hits depending on worker finish order), as inferred. The
  10k pins are exact (`SLACK` = 0).
- Exit (init.md I1): 10k nested 624 ≤ 650, cold; byte identity both
  backends; oracle passes.

Check set (once, `receipts/I1/check/`): init pins (1k, 10k), 
`commit_batched_base_files`, `commit_batched_reads`,
`commit_similar_tiny_files`, `postgresql_phase_attribution`,
`mount_round_trips` (all Init in setup), `sdk_round_trip_equivalence_seeded`
(incl. the 4,200-entry Init), `method_refusal_order`: pass.
`postgresql_round_trips` **fails** at `workspace_reader_open`
(`round_trips >= statements`: 11 trips, 12 statements). Not reached by I1:
that scenario runs on a fresh `Opened::open` (new connections, empty
caches) over Store rows I1 does not change (the walk was read-only); the
mount's pipelined admission (#768) sends more statements than trips.
Pre-existing, recorded for the orchestrator; not diagnosed further here.

No sweep: I1 adds no server message (it only removes reads).
Deleted tests: none. Production LOC: 227,595 -> 227,606 (delta +11), engine
layerfs-project +11.

## I2a / I2b — one placement statement per window

### Design (both backends, one algorithm)

`place_children` (wide-directory placement, one unit of at most
`WRITE_WINDOW_ROWS` = 4,096 rows or `WRITE_WINDOW_BYTES` = 1 MiB) cuts its
children into set windows exactly as `independent` cuts entry windows:
before a repeated name, a repeated position, a repeated (device, inode),
and before a row a statement could refuse on its own (host path empty or
over 4,096 bytes, a host child at position 0, a position that does not
convert to signed, which also ends the window as `record_roots` does). A
window of one row runs the unchanged per-row path (`BIND_HOST_FILE` then
`PLACE_ENTRY`), so an all-hardlink directory costs what it costs today. A
longer window first runs a `first_dependent`-style check (D17): the first
slot whose row could hit a stored row's constraint (a stored
`init_host_file` with its canonical position, or a stored directory at its
position) ends the window, and that row runs the per-row path. The window
then binds and places in set statements (PostgreSQL: one statement with
data-modifying CTEs over `unnest`, at most one window per unit plus cuts;
SQLite: a multi-row upsert and a multi-row `UPDATE … FROM` over 32-row
`VALUES` inputs, returned rows matched by identity and by position). The
first slot (in order) with no bound row (host), no placed row, or a
kind/host mismatch is `Changed(position)`, the per-row loop's first
refusal; the unit rolls back as today. A stored identity from an earlier
window or unit still answers its canonical through `DO UPDATE … RETURNING`.

### Prediction (written before coding; after I1)

`wide_hardlinks` is new (300 files + 300 interleaved hard links in one
600-entry directory), measured on the I1 code first (`receipts/I2a/before`):
SQLite `[0,0,1702,24]`, PG `[0,2634,1907,339]`. Windows there are
[a0] alone, then 299 windows [b_k, a_k+1], then [b299] alone.

| Shape | PG trips after I1 → I2a predicted | PG statements | SQLite stmts after I1 → I2b predicted |
|---|---|---|---|
| `flat_1k` | 2,226 → ~228 (−2,000 +2) | 2,105 → ~107 | 2,264 → ~360 (32 windows × 3) |
| `dir_600` | 1,426 → ~228 | 1,305 → ~107 | 1,416 → ~273 (19 × 3) |
| `flat_10k` | 20,720 → ~726 (3 units × 2) | 20,381 → ~387 | 21,560 → ~2,499 (313 × 3) |
| `wide_hardlinks` | 2,634 → ~2,038 (602 + 2 prepares) | 1,907 → ~1,309 | 1,702 → ~1,403 |
| `nested_1k`, `nested_10k`, `hardlink_window`, `mixed` | unchanged (no directory reaches 512) | unchanged | unchanged |

PG per window: dependency check + placement statement (two new prepares).
SQLite per 32-row window: dependency check + host upsert (when the window
has a host child) + placement. Transactions unchanged. Spec expectations
(SPEC §6.3 M1): 1k flat ~226, 10k flat ~706; SQLite init.md ~330 / ~2,330
(those assumed no dependency check).

Finding before coding (not I2's lever): `wide_hardlinks` on PG runs 339
transactions, about one read unit per alias in `check_aliases`
(`backing.job(canonical)` for each later path whose canonical differs from
the previous one). A per-alias loop like I1's; recorded for I3/owner.

### I2a measured (PostgreSQL; test profile, one sample; load ~5.7)

Receipts: `receipts/I2a/` (pins against the I1 pins, the equivalence test,
the sweeps) and `receipts/I2a/check/`.

| Shape | PG trips after I1 → predicted → measured | PG statements predicted → measured | Within 15% |
|---|---|---|---|
| `flat_1k` | 2,226 → ~228 → **228** | ~107 → **107** | yes (exact) |
| `dir_600` | 1,426 → ~228 → **228** | ~107 → **107** | yes (exact) |
| `flat_10k` | 20,720 → ~726 → **726** | ~387 → **387** | yes (exact) |
| `wide_hardlinks` | 2,634 → ~2,038 → **2,038** | ~1,309 → **1,309** | yes (exact) |
| `nested_1k`, `nested_10k`, `hardlink_window`, `mixed` | unchanged → 209, 624, 704, 255 | unchanged | yes |

- Full PG rows: flat_1k `[0,228,107,37]`, dir_600 `[0,228,107,37]`,
  flat_10k `[0,726,387,144]`, wide_hardlinks `[0,2038,1309,339]`.
  Transactions and prepares unchanged (the two per-row statements are no
  longer prepared on these shapes; the check and the placement statement
  are). SQLite unchanged (`wide_hardlinks` SQLite pin `[0,0,1702,24]` is
  the I1 value until I2b). Roots, logical and canonical bytes identical.
- SPEC §6.3 M1: 1k flat ~226 → 228 (+1%); 10k flat ~706 → 726 (+3%,
  the D17 check adds one trip per unit; the spec assumed none); 10k
  nested ~606 → 624 (from I1). All ≤ 750 per 10k (M1 goal).
- Equivalence (`layerfs-persistence/tests/acquisition_placement.rs`, both
  backends, 8.5 s): 19 cases, each a set-window unit against the same
  children placed one per unit (the per-row path in the loop's order).
  Equal first refusal; on success equal rows (`init_entry`,
  `init_host_file`) and held charge; on refusal rows unchanged. Shapes:
  repeated name, repeated position (directories: constraint; symlinks: no
  constraint), unsigned position, a stored directory and a stored identity
  position (dependencies on an earlier unit), a stored alias, a dependency
  on an earlier window of the same unit, each with and without an earlier
  `Changed` in the same window; kind mismatches; an all-hardlink directory
  (with and without a changed path); 1,000 mixed children. Peak charges are
  not compared: the reference charges child by child.
- Sweeps (`postgresql_placement_fault.rs`): a unit with two set windows
  (two checks, two placement statements, first-use prepares) and a
  one-child window; 84 server messages; drop 84/84 and kill 84/84 pass
  (atomic: rows as before or as the clean run; drop always `Uncertain`;
  kill `Uncertain` or, after the commit reply was sent, the clean answer;
  no reconnect or replay).
- Check set (once): init pins 1k/10k, `sdk_round_trip_equivalence_seeded`,
  `method_refusal_order`, persistence `postgresql_acquisition`,
  `acquisition_batching`, `acquisition_reads`, `acquisition_disposal`,
  `sqlite_reference_acquisition`, all `layerfs-project` tests: pass.
- New files: `backend/postgresql/acquisition/placement.rs` (the PG
  statements; `place_children` moved out of `writes.rs`, 552 → 506 physical
  lines), `backend/sqlite/acquisition/placement.rs` (the shared window rule
  and outcome decoding, used by SQLite in I2b).
- Not done here: `tools/ci_tests.py` (shared) does not list the new
  binaries; without `LAYERFS_TEST_POSTGRES_URL` both run in under 5 s, so
  they stay in the fast lane.
- Deleted tests: none. Production LOC: 227,606 -> 227,827 (delta +221),
  engine layerfs-persistence +221.
