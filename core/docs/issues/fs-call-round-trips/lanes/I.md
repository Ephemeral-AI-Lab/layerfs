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

### I2b measured (SQLite; test profile, one sample)

Receipts: `receipts/I2b/` (pins against the I2a pins, the equivalence
test) and `receipts/I2b/check/`.

| Shape | SQLite stmts after I1 → predicted → measured | Within 15% |
|---|---|---|
| `flat_1k` | 2,264 → ~360 → **360** | yes (exact) |
| `dir_600` | 1,416 → ~273 → **273** | yes (exact) |
| `flat_10k` | 21,560 → ~2,499 → **2,502** | yes (+3: 314 windows of up to 32, not 313; the units' 1 MiB byte bound, not the 4,096 rows, decides their size, so the last window of each unit rounds differently) |
| `wide_hardlinks` | 1,702 → ~1,403 → **1,403** | yes (exact) |
| `nested_1k`, `nested_10k`, `hardlink_window`, `mixed` | unchanged → 288, 1,837, 486, 175 | yes |

- Transactions unchanged; roots, logical and canonical bytes identical.
  PostgreSQL unchanged (its pins pass as at I2a).
- init.md expectation (no D17 check): 1k flat ~330 → 360 (+9%), 10k flat
  ~2,330 → 2,502 (+7%): the per-window dependency check adds one statement
  per 32 children.
- New SQL: `placement_dependencies.sql`, `bind_host_files.sql`,
  `place_children.sql` (init.md named only `place_children.sql`; SQLite
  cannot bind and place in one statement, so the upsert is its own text,
  and the D17 check is the third). `bind_host_file.sql` and
  `place_entry.sql` stay (per-row path, `entry_windows.rs`).
- New file `backend/sqlite/acquisition/child_windows.rs` (SQLite's
  statements over the shared `placement.rs`); `writes.rs` delegates.
- Equivalence (`acquisition_placement.rs`) now runs SQLite's set windows
  against its per-row path: pass (19 cases). No SQLite sweep: in-process
  statements have no lost reply; a refused window rolls back with its unit
  (checked by the equivalence cases).
- Check set (once): init pins 1k/10k, `sdk_round_trip_equivalence_seeded`,
  `method_refusal_order`, persistence `acquisition_placement`,
  `postgresql_placement_fault`, `postgresql_acquisition`,
  `acquisition_batching`, `acquisition_reads`, `acquisition_disposal`,
  `sqlite_reference_acquisition`, all `layerfs-project` tests: pass.
- Milestone end: Clippy `-D warnings --all-targets` on layerfs-persistence,
  layerfs-project, layerfs-store-service clean; rustfmt applied to the
  files this lane touched (incl. the I2a tests). `cargo fmt --check` still
  reports files no lane-I item touches (`history/admission.rs`,
  `layerfs-project/src/store/init.rs`, `tests/init_postgresql.rs`,
  store-service `refresh_lookups.rs`, `support/view.rs`, `verify768y_*`):
  left alone.
- `flat_10k` window count: a placed row is name + host path + 160 bytes
  (`WRITE_ROW_BYTES`), about 270 bytes with the test's temporary path, so
  `place_wide` fills a unit at 1 MiB (about 3,880 children), not 4,096:
  three units of 122 + 122 + 70 SQLite windows = 314.
- Deleted tests: none. Production LOC: 227,827 -> 228,098 (delta +271),
  engine layerfs-persistence +271 (Rust and the three shipped SQL texts).

## I3 — fewer waits per acquisition unit and publication (PostgreSQL)

Decisions (coordinator, after M1 merged at `cfc9586d05`): I3a and I3b as
specified; I3c taken (D12) with Init Saves on `publish_flight` behind their
own flag; I3d not taken (D13): **I3d not done**. SQLite is unchanged by I3.

### Per-phase attribution at M1 (written before coding)

From the M1 pins' server templates (`RT_INIT_TEMPLATE`, printed by both pin
binaries since this item; `receipts/I3/before/`). A unit is one
transaction: its `BEGIN` message and `COMMIT` (2 trips), the owner select
(1), its statements (1 each); each first-use statement text costs one
prepare trip. Model: `scratchpad phases.py` over the two logs.

| Phase (PG trips) | nested_10k | flat_10k | nested_1k | flat_1k |
|---|---|---|---|---|
| Read units (count) | 87 | 108 | 15 | 18 |
| Read units: trips (2 + owner + 1–2 reads each) | 370 | 454 | 64 | 76 |
| Write units (count: put, place, roots, discard, release) | 15 | 18 | 6 | 7 |
| Write units: trips (2 + owner + statements) | 96 | 111 | 41 | 47 |
| Acquisition prepares (texts) | ~22 | ~24 | ~20 | ~22 |
| Save publications (count; BEGIN, lane, rows, COMMIT, one COPY) | 8: 33 | 8: 33 | 3: 13 | 3: 13 |
| Locates, spans, value blocks (one fenced statement each) | 28 | 29 | 5 | 5 |
| History, begin unit, GC intent, other prepares | rest (~75) | rest (~75) | rest (~66) | rest (~65) |
| **Total** | **624** | **726** | **209** | **228** |

### Predictions (written before coding)

- **I3a** (every read unit one fenced autocommit statement with the owner
  joined in; sizing folded): a unit costs 1 trip instead of
  3 + reads. Saved: 2 × units + read statements, plus one prepare per
  sizing text removed (`JOB_SIZES`, `DIRECTORY_SIZES`). The single-row
  reads (`job`, `directory_path`) are read units too: 3 saved each. This
  is the same mechanism as finding 3 (`check_aliases`' one `job` read unit
  per alias), whose units drop from 4 trips to 1; batching those reads
  into windows is a different mechanism and is not built.
- **I3b** (each write unit one flight from `BEGIN` to `COMMIT`; every
  dependency check, `put_entries`' and placement's, stays one extra wait;
  a release waits once for its "last record" answer before its compaction;
  a discard's key lookups, deletes and credit become one statement): a unit
  costs 1 + its dependency checks + (release) 1 trips. Each unit kind's
  texts are prepared in one trip at its first use.
- **I3c** (Init's Saves on the fenced flight): a publication costs 1 trip
  instead of 4 (5 with packs); the prepared texts change, net 0.

| Shape | M1 | after I3a | after I3a+b | after I3a+b+c |
|---|---|---|---|---|
| nested_10k | 624 | **339** (−285) | ~254 (−85) | **~229** (−25) |
| flat_10k | 726 | **378** (−348) | ~284 (−94) | **~259** (−25) |
| nested_1k | 209 | **158** (−51) | ~119 (−39) | ~109 (−10) |
| flat_1k, dir_600 | 228 | **168** (−60) | ~125 (−43) | ~115 (−10) |
| hardlink_window | 704 | **507** (−197) | predicted before I3b | −10 |
| wide_hardlinks | 2,038 | **1,073** (−965) | predicted before I3b | −10 |
| mixed | 255 | **189** (−66) | predicted before I3b | −10 |

I3a statements fall by each read unit's owner select and the folded
sizing reads (nested_10k 335 → ~226, flat_10k 387 → ~257); transactions
fall by the read units (nested_10k 120 → ~33, flat_10k 144 → ~36; the
counter counts autocommit statements as no transaction).

**Is ≤ 250 per 10k reachable?** For nested_10k yes: ~229 with a+b+c.
For flat_10k **no, by about 9 trips** (~259; the model is ± ~10): flat
has 18 write units with 8 dependency waits and 6 more read units than
nested. The remaining gap is about the prepare trips that I3d (D13, not
taken) would batch (~13 after a+b+c). Proposed revision: flat_10k
≤ 265 with a+b+c, or ≤ 250 only with a narrowed I3d (prepare each Init's
acquisition texts in one flight at `begin`).

Order: I3a, then I3c (small, independent of I3b), then I3b (the largest
and the only one that could meet a stop condition: custody or refusal
order), so a stop in I3b does not hold back I3c.

### I3a measured (PostgreSQL; test profile, one sample; load ~12)

`[0, round_trips, statements, transactions]`, I2a pin → predicted → measured:

| Shape | Pin at M1 | Predicted | Measured | Within 15% |
|---|---|---|---|---|
| `nested_10k` | 624 / 335 / 120 | 339 / ~226 / ~33 | **339 / 226 / 33** | yes (exact) |
| `flat_10k` | 726 / 387 / 144 | 378 / ~257 / ~36 | **378 / 257 / 36** | yes (exact) |
| `nested_1k` | 209 / 98 / 33 | 158 | **158 / 79 / 18** | yes (exact) |
| `flat_1k`, `dir_600` | 228 / 107 / 37 | 168 | **168 / 85 / 19** | yes (exact) |
| `hardlink_window` | 704 / 491 / 82 | 507 | **507 / 424 / 18** | yes (exact) |
| `wide_hardlinks` | 2,038 / 1,309 / 339 | 1,073 | **1,073 / 986 / 19** | yes (exact) |
| `mixed` | 255 / 129 / 38 | 189 | **189 / 105 / 18** | yes (exact) |

- Prepares fall by 2 (the two sizing texts); roots, logical and canonical
  bytes identical; SQLite unchanged (its pins pass as at M1).
- Finding 3 (`check_aliases`, one `job` read unit per alias): the same
  read-unit mechanism, so each alias now costs 1 round trip instead of 4
  (`wide_hardlinks` −903 of its −965). Batching those reads is a different
  mechanism and is not built (recorded only).
- Wall in the test profile is not comparable across runs at this load:
  `nested_1k` 322 ms at M1, 530 then 368 ms (a second, diagnostic sample);
  `flat_10k` 6.2 s → 7.8 s, inside M1's own 6–13 s spread. The new reads'
  server time is 0.7–1.4 ms each (`pg_stat_statements`, about 110 ms of
  `flat_10k`'s 21 × 6 reads), so no server cost explains a difference. The
  release timing rows are the orchestrator's.
- Design: `reads.rs` (the owner join, `LEFT JOIN LATERAL`; NULL marker for
  an empty window; running sum with `row_number() = 1` keeping the first row
  as the sizing loop did, over at most the row limit); `provider.rs` `read`
  for every read unit and `work`; `OWNER` names the schema so `work` runs
  alone. Rows are put in key order client-side, whatever order the joined,
  fenced statement returns. No write, format, answer or refusal-order change.
- New test `postgresql_acquisition_reads.rs`: PG windows equal SQLite's
  (972 compared lines over 7 limits, single reads and `work`); empty,
  released, missing-operation and foreign-epoch owners distinct (every read
  `Stale` but the live empty one); drop at every server message of each of
  the 9 read kinds (78): always `Uncertain`, no row changed, no reconnect;
  kill (78): `Uncertain`, or the clean answer when the whole reply was held
  and passed on before the kill. Its first run failed on the test's own data
  (entries read before the unplaced children were placed: `Integrity`, as
  before I3a; and a kill after the reply that the test had not allowed);
  fixed in the test and run once more.
- Check set (once): init pins 1k/10k, `sdk_round_trip_equivalence_seeded`,
  `method_refusal_order`; persistence `postgresql_acquisition`,
  `acquisition_placement`, `postgresql_placement_fault`,
  `acquisition_batching`, `acquisition_disposal`,
  `postgresql_acquisition_compaction`, `acquisition_reads`,
  `sqlite_reference_acquisition`; every `layerfs-project` test binary: pass.
  `fs_call_round_trips` NOT_RUN (needs `FS818_ARM`; the probe is the
  orchestrator's).

## I2b fix — SQLite `flat_10k` release time (coordinator's M1 note)

M1's release benchmark: SQLite `init.flat_10k` 510.0 → 563.8 ms (+10.5%)
while statements fell 31,882 → 2,502. Diagnosed once, from the plans:

- `EXPLAIN QUERY PLAN` of the three I2b texts on an empty acquisition
  schema (sqlite3 3.51): `placement_dependencies.sql` probes
  `init_host_file`'s key and `init_entry_directory` per input row (fine);
  `bind_host_files.sql` scans its 32 input rows (fine);
  `place_children.sql`'s `UPDATE … FROM` ran
  `SEARCH target USING PRIMARY KEY (operation_id=? AND parent_position=?)`
  with the input probed through a bloom filter and an automatic index:
  **every child of the parent per window**. A flat directory of 10,000
  children placed in 314 windows visits about 3.1 million rows; nested
  directories (100 children, below 512) never place, hence only flat
  regressed.
- Fix within I2b's design (one multi-row update per window, same rows,
  same answers, no constant): the update finds its rows by
  `target.name IN (SELECT child_name FROM input)` and takes the new values
  from correlated lookups in the 32-row input. Plan:
  `SEARCH target USING PRIMARY KEY (operation_id=? AND parent_position=? AND name=?)`,
  one probe per child. Statement counts unchanged (pins pass as they are).
- One release sample each, same load, same probe (`FS818_ARM=sqlite`,
  `init_flat_10k`): the M1 binary (sha256 `db76b87011816739`) 494.0 ms,
  the fixed tree (`a99d36b75b0d2b96`) **298.8 ms** (−40% against 494, −41%
  against the pre-I2b 510.0). Test profile: 1,010.6 ms at M1 → 478.6 ms.
  Receipts `receipts/I2b-fix/`.
- New guard in `acquisition_placement.rs`
  (`sqlite_window_update_probes_each_child_by_key`): the bundled SQLite's
  plan for `place_children.sql` probes the full key; equivalence (19
  cases) passes with the new text.
- Checks: init pins, `sdk_round_trip_equivalence_seeded`,
  `method_refusal_order`, `acquisition_placement`,
  `sqlite_reference_acquisition`, `acquisition_batching`, `init_sqlite`,
  `backed_acquisition`: pass.

## I3c — Init's Saves in fenced flights (D12)

### Prediction (the numbers are the I3 table's, written before coding)

From the per-phase table above: each of the 8 (10k) or 3 (1k) Save
publications costs 1 round trip instead of 4 (5 with packs): −25 at 10k,
−10 at 1k; nested_10k 339 → ~314, flat_10k 378 → ~353, nested_1k 158 →
~148, flat_1k 168 → ~158. Statements +3 per publication (the flight counts
each request). Entry (decided with the implementation): `publish_captured` keeps no payloads by itself
(payload retention is `settle`'s `captured_output && batched_chain_reads`),
so the flag can reuse the flight entries without lane W's provider change.
`publish_captured_tail` is chosen over `publish_captured`: both are the same
flight; `_tail` answers an untrusted completion `Unknown`, which is the
ordinary publication's custody (spec: "failure custody of the ordinary
path"); `publish_captured` would answer a definite refusal there.

### Measured (PostgreSQL; test profile, one sample)

| Shape | after I3a | Predicted | Measured | Within 15% |
|---|---|---|---|---|
| `nested_10k` | 339 / 226 / 33 | ~314 | **303 / 250 / 33** | yes (−3.5%) |
| `flat_10k` | 378 / 257 / 36 | ~353 | **342 / 281 / 36** | yes (−3.1%) |
| `nested_1k` | 158 / 79 / 18 | ~148 | **145 / 88 / 18** | yes |
| `flat_1k`, `dir_600` | 168 / 85 / 19 | ~158 | **155 / 94 / 19** | yes |
| `hardlink_window` | 507 / 424 / 18 | ~497 | **494 / 433 / 18** | yes |
| `wide_hardlinks` | 1,073 / 986 / 19 | ~1,063 | **1,060 / 995 / 19** | yes |
| `mixed` | 189 / 105 / 18 | ~179 | **173 / 116 / 18** | yes |

- The saving per publication was larger than predicted (−36, not −25, at
  10k): diagnosed from the flat_10k templates (nested_10k's window was
  evicted, `evicted: true`, so its templates are not used). The flight
  inserted packs in 6 of the 8 publications, while `pg_stat_statements`
  had shown one `COPY` call for the ordinary path; and a `COPY` costs 2
  round trips (`copy_in`: the start and the finish). Ordinary: 6 × 6 + 2 ×
  4 = 44 trips; flights 8: −36 exactly. 1k: 2 of 3 with packs, −13.
- Statements +3 per publication (8 → +24), transactions unchanged (a flight
  counts its own), prepares unchanged (the fused `PACK`/`ROWS` texts replace
  `publish_rows.sql`). Roots, logical, canonical and stored bytes identical.
- Release, same load, one sample each (`FS818_ARM=pg`, M1 binary
  `db76b87011816739` vs this tree with I3a+I3c): flat_10k 8,920 → 8,668 ms,
  nested_10k 9,187 → 8,593 ms (726 → 342 and 624 → 303 round trips). Local
  PostgreSQL Init time is server work; the trips pay at a network distance.
  The test-profile drift seen at I3a (6.2 → 7.8 → 9.1 s) is host load: the
  M1 binary itself took 8.9 s in this window against 6.4 s at the M1 bench.
- New test `postgresql_save_flight_fault.rs`: a Save with the flag stores
  the same rows as an ordinary Save in 8 round trips instead of 14; drop at
  every server message of its finish (68): always an error, the
  publication atomic; kill (68): 50 errors and 18 successes whose rows equal
  the clean finish's; never a reconnect or replay.
- Check set (once): init pins 1k/10k, `sdk_round_trip_equivalence_seeded`,
  `method_refusal_order`, `postgresql_captured_save_fault` (the shared
  flight), `layerfs-storage` `captured_payload_retention` and
  `kept_ids_pooled_chains`, every `layerfs-project` test binary: pass.

## I3b — stopped before coding (needs a session-layer capability)

Prediction (from the I3 table): nested_10k 303 → ~218, flat_10k 342 →
~248, i.e. both ≤ 250 only with I3b. Not built. Why:

- A `put_entries` or `place_children` unit decides its statements from its
  dependency check (append the independent prefix, then the first
  dependent row per row, then the next window). Keeping that check as "one
  extra wait" inside the same transaction means one write transaction held
  across two or more flights in one turn. `Session::decided_turn` /
  `DecidedTurn::send` can send several flights in a turn, but each flight
  prepares its own uncached texts inside a scoped read transaction
  (`BEGIN … READ ONLY; SET LOCAL search_path; prepares; COMMIT`): inside an
  open write transaction that `COMMIT` would commit the unit half way. A
  prepare before the turn does not help: the statement cache clears when
  full, so a text can be missing again by the second flight. A safe form
  needs the session layer (for example: prepare a unit's every text at the
  start of the turn, before its `BEGIN`, and refuse any prepare while the
  turn's transaction is open). `session/` is not lane I's.
- Without it, the alternatives change behaviour and are not lane I's call:
  (a) the dependency check as a separate fenced read before a one-flight
  write: not atomic with the write (another session's
  `discard_abandoned`, or a concurrent unit of the same owner, could land
  in between and change the answer); (b) an optimistic one-flight unit
  that asserts "no dependency" server-side and, when it fires (nothing
  written), continues on today's path in the same call, like the owner's
  2026-11-25 Commit race fallback: an owner decision for Init.
- What lane I could build alone: the units with no wait (`complete_files`,
  `set_directory_roots`, `advance`, `discard` as one data-modifying CTE
  statement) as single fenced flights with server-side assertions before
  `COMMIT` (a `Changed` position read from the reply, the abort via a
  session-local flag, as `publish_flight`'s nonce check does). About −50 at
  10k (nested ~253, flat ~292): neither reaches 250 without the put and
  place units. Not started, pending the coordinator's choice.
