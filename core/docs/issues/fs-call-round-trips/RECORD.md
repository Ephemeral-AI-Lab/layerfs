# fs call round trips (#818): implementation record

> **Status:** Working record of the rollout on branch `claude/rt818-rollout-c3dbae`.
> Base `layerfs_v2_0_0` `1317ebf7ca`. Measured values replace SPEC estimates only
> where a row below says so.

## Base and citations

- The spec pinned `056e920166`. `git diff --stat 056e920166 1317ebf7ca` touches
  only `AGENTS.md`, `docs/` and `docs/lesson/` (11 files): **no production or
  test source changed**, so every file:line citation in SPEC and the track specs
  stands unchanged at `1317ebf7ca`.
- Target seeded by APFS clone from the idle baseline worktree
  `layerfs-818-rt` (`056e920166`, same `Cargo.lock`).

## Benchmark identity

- Lane and Phase 0 tests: container `lfs818-rt-pg` (postgres:17 17.11), one
  database per lane (`rt818_p0a`, `rt818_p0b`, `rt818_rh`, `rt818_c`,
  `rt818_w`, `rt818_i`).
- Milestone benchmarks: a separate container `lfs818-bench-pg` (postgres:17,
  17.11, same flags as the baseline container: `shared_preload_libraries=
  pg_stat_statements`, `track=all`, `synchronous_commit=off`, trust auth),
  database `rt818_bench`, so its netem windows never slow the lanes' tests.
  Server counts are filtered to `toplevel` and the current database.

## Rules taken for this run (from the session that started it)

- Implement first, verify once. Per item: prediction first, `--no-run` build,
  the item's pinned tests once, drop/kill sweep for each new or changed message,
  byte identity / answers / refusal order through existing tests.
- Per milestone: three-arm benchmark once on the merged tip (SQLite, PG local,
  PG +10 ms; +2 ms where a row is over budget) and one independent review.
- **15% rule:** measured within 15% of the SPEC expectation → record and
  proceed. Beyond 15% → diagnose once from per-role counts, then fix or record
  the cause and continue. No retuned caps, windows or timeouts.
  (This replaces SPEC §6.4's "10% or 3 trips" explain-before-next-item rule
  for this run.)
- The per-item latency arm, memory soak and held-out sweeps of verification.md
  run at milestone level only, not per item (no repeated verification).

## Decisions (SPEC §9 recommendations, taken without asking)

| # | Taken | Note |
|---|---|---|
| D1 | Accept new SQLite statement pins for dedupe fixes | each listed in its item |
| D2 | Deferred: **R7 not done** | owner decision needed |
| D3 | R6 taken after M3, measured on a large-file tree | |
| D4 | No (R5 does not return the root locator) | |
| D5 | Accept windows 16 → 128 and the 128-object roots demand | |
| D6 | **H7 not done** | owner decision needed (large-file attribution scope) |
| D7 | W3 uses a Store slot | no contract change |
| D8 | W4 prepares create statements at Store open (+1 trip per open) | |
| D9 | Write floor 3 + d not attempted | |
| D10 | `PLAN_PATHS` = 1,024 for W6 | |
| D11 | `READ_WINDOW_ROWS` lever deferred | |
| D12 | I3c: Init Saves use `publish_flight` via their own flag, full sweeps | |
| D13 | I3d only if one-shot SDK processes run Init → **not taken** (recommendation is conditional; no such use selected) | |
| D14 | Over-budget +10 ms rows run at +2 ms / smaller shapes until M3 | |
| D15 | Release builds for Init timing; Init baseline re-taken in release at P0 | |
| D16 | **Keep today's refusal order:** keep the eager marker read when the kept markers are absent | no order change |
| D17 | I2a/I2b add a `first_dependent`-style check | no order change |

## Items

Per item: prediction vs measured (trips and time per arm), within 15%,
decisions, deleted tests, Production LOC. Receipts under `receipts/<item>/`.

### Phase 0

#### Phase 0a — probe, pins, W0 attribution, release Init baseline

Test files only, under `engine/crates/layerfs-store-service/tests/`
(production delta 0). Receipts in `receipts/P0/`. PostgreSQL ran in
container `lfs818-rt-pg`, database `rt818_p0a`. No container was named for
P0 netem, so the arm runs used `lfs818-rt-pg` too, not `lfs818-bench-pg`.

- `fs_call_round_trips.rs` is the probe. It has arms `FS818_ARM` = `sqlite`,
  `pg`, `pg2` and `pg10`, and groups reads, history, writes, `w0` and Init
  1k/10k. `FS818_CASES` filters the cases. Without `FS818_ARM` the probe
  reports NOT_RUN.
  - Netem runs only inside the PostgreSQL container's namespace:
    `docker run --rm --net container:$FS818_NETEM_CONTAINER --cap-add NET_ADMIN alpine:3 tc ...`.
    It never touches the host network.
  - Removing netem asserts that no `netem` qdisc remains. Each switch
    re-calibrates `SELECT 1`.
  - `revision.h10` is appended to the history group at the end, so earlier
    rows are not shifted.
- `support/round_trips.rs` is the shared harness. It opens the Store the way
  the SDK does (`CALLS` = 64, 64 KiB cache) and splits counts by role:
  - writer;
  - control reader, which is session 0;
  - Workspace readers, which is all readers minus session 0.

  Its server counter reads `pg_stat_statements` for `toplevel` calls of the
  current `dbid` only, excluding its own queries. It checks
  `pg_stat_statements_info.dealloc` for evictions.
- Pin files are self-contained. Each has an editable constants table per
  backend and reports PostgreSQL as NOT_RUN without
  `LAYERFS_TEST_POSTGRES_URL`. Server calls are printed, not pinned.
  - `sdk_read_round_trips.rs`: 27 cases. The 12 probe reads run cold and
    warm, then three hidden-path cases run after `project.remove_file`.
  - `sdk_history_round_trips.rs`: blame at h10 and h100, revision at h10 and
    h100, and `diff_files` h10 vs h99. It includes the `F + ceil(V/W)·K`
    formula check, which at baseline is blame F=37 W=1 K=25 and revision
    F=100 W=1 K=41.
  - `sdk_write_round_trips.rs`: nine write shapes, cold and warm.
  - `init_round_trips.rs`: `nested_1k`, `flat_1k`, `hardlink_window` (50
    files and 50 hard links interleaved) and `dir_600`. Each checks
    `logical == logical_size` after Init and pins logical bytes, canonical
    bytes and the genesis root, which is the same on both backends because
    source times are fixed.
  - `init_round_trips_10k.rs`: `nested_10k` and `flat_10k`, with
    `SLACK = 32` (see below).

  Run times in `pins.log`, both backends:

  | Binary | Time |
  |---|---:|
  | read | 4.7 s |
  | history | 9.7 s |
  | write | 4.1 s |
  | init | 3.7 s |
  | init_10k | 38.8 s |

  The history and init_10k pins were added to the `tools/ci_tests.py` SLOW
  list.

**Probe vs `056e920166`** (`sqlite.log`, `pg.log`, `pg10.log`, `pg2.log`):
- Every `fs.*` row and both Init 1k rows are identical on SQLite, PG and
  PG +10 ms, including the server's top-level calls.
- Init 10k varies run to run by a few statements:
  - PostgreSQL nested: 10,953–10,958 trips.
  - PostgreSQL flat: 31,039–31,044 trips.
  - SQLite nested: 12,163–12,172 statements.
  - SQLite flat: 31,879–31,885 statements.

  The likely cause, not yet confirmed, is Init's own `logical_size` walk
  hitting the locator cache (4,096 entries) depending on the order the
  workers finished in. Connects, transactions and bytes are exact.
- Cost per trip:
  - PG +10 ms: median 13.13 ms per trip, against a `SELECT 1` median of
    about 13.0 ms.
  - PG +2 ms: 3.65 ms per trip, against 3.5–4.0 ms.
- Over-budget rows (D14):
  - At +10 ms, history ran only `blame.h10` and `revision.h10`, and Init ran
    only `nested_1k`.
  - At +2 ms, history ran in two filtered commands. `revision.h100` read
    3,999 trips there because it ran first in its command; it is labelled.
  - Init 10k at +2 ms and +10 ms is NOT_RUN (projected from trips × measured
    per-trip cost: about 40 s and 145 s for nested).
- Commands over 15 s: pg10 reads 19.5 s, writes 16.9 s, history 20.0 s;
  pg2 history 21.3 s and 32.3 s; PG flat 10k about 20 s.

**Pins vs probe.** Every pin equals the measured baseline on both backends.
The probe runs each case once on the open that wrote the setup, so its row
lies between `.cold` (a new Store open: control-reader connect, prepares,
empty caches) and `.warm` (a repeat on that open). In most cases the probe
equals `.warm`.

| Case | PG trips: probe / cold / warm | SQLite statements: probe / cold / warm |
|---|---|---|
| stat.depth7 | 73 / 91 / 65 | 152 / 161 / 144 |
| grep.tree_1k | 78 / 111 / 38 | 131 / 156 / 77 |
| blame.h100 | 2,325 / 2,470 / 2,229 | 7,575 / 7,705 / 7,349 |
| revision.h100 | 3,890 / 4,119 / 3,890 | 12,658 / 13,002 / 12,658 |
| write.overwrite | 24 / 75 / 22 | 54 / 148 / 62 |
| write.new_file | 66 / 112 / 44 | 77 / 152 / 75 |
| mkdir | 45 / 113 / 46 | 69 / 158 / 82 |
| rm.dir_100_recursive | 151 / 207 / 148 | 179 / 251 / 177 |

Init pins in `[connects, round_trips, statements, transactions]`, with
logical and canonical bytes. The 1k rows equal the probe.

| Shape | SQLite | PostgreSQL | Logical | Canonical |
|---|---|---|---:|---:|
| `nested_1k` | [0,0,308,24] | [0,229,118,33] | 9,900 | 137,268 |
| `flat_1k` | [0,0,2284,25] | [0,2246,2125,37] | 10,890 | 137,359 |
| `hardlink_window` | [0,0,487,23] | [0,705,492,82] | 780 | 7,524 |
| `dir_600` | [0,0,1428,25] | [0,1438,1317,37] | 6,490 | 82,511 |
| `nested_10k` (±32) | [0,0,12164,57] | [0,10955,10666,120] | 108,000 | 1,379,418 |
| `flat_10k` (±32) | [0,0,31881,60] | [0,31039,30700,144] | 118,890 | 1,380,691 |

Pack bytes (`stored`) vary by a few bytes from run to run at baseline. They
are printed, not pinned.

**W0 attribution** (`pg-w0.log`, `sqlite-w0.log`, template listings
`w0-*-phases.txt`; no product change).
- Per-phase `SqlWork` comes from `telemetry::phase_sql`, split by role.
  Root spans are not marked, so gaps between phases are labelled by position:
  - `files.resolve` before `store.bind`;
  - `files.change` after it;
  - `commit.between` before `begin`;
  - `files.close` after `publish`.
- The W0 totals equal the write pins exactly on both backends.

PG trips per phase, warm:

| Phase | overwrite (fused) | new_file | mkdir | rm.dir_100 |
|---|---:|---:|---:|---:|
| `files.resolve` (control) | 4 | 4 | 4 | 4 |
| `store.bind` (writer) | 2 | 2 | 2 | 2 |
| `files.change` (control + Workspace) | 6 | 8 | 6 | 112 |
| `commit.between` (writer ROLLBACK) | — | 1 | 1 | 1 |
| `construct` | 3 | 1 | 2 | 3 |
| `head` | — | 1 | 1 | 1 |
| `logical.known` + `logical.count` | 3 | 3 | 5 | 4 |
| `finish` | 2 | 7 | 7 | 7 |
| `bindings` | — | 3 | 3 | — |
| `validate` (Workspace) | — | 3 | 4 | 3 |
| `lane` | — | 1 | 1 | 1 |
| `publish` / `publish.fused` | 1 | 9 | 9 | 9 |
| `files.close` | 1 | 1 | 1 | 1 |
| **total** | **22** | **44** | **46** | **148** |

- **Write and edit take the fused Commit. Create, mkdir, cp, mv, rm and apply
  do not.** The non-fused path costs about 20 more trips warm:
  - `head`, `bindings`, `validate` and `lane`;
  - `finish` at 7 trips instead of 2;
  - `publish` at 9 trips instead of 1.

  Inferred cause: the SDK's constructed staging limit is the 64 KiB cache ÷
  `CONSTRUCTED_SHARE` (8) = 8 KiB. After a directory rewrite,
  `staged().covers(root)` is then false. This lever is outside the W plan and
  needs an owner decision.
- The writer `ROLLBACK` in `commit.between` (1 trip) on the non-fused paths
  has a source that was not identified.
- Cold costs:
  - Every cold case adds the control reader's connect and prepares (17 vs
    10 trips) and the Workspace reader's open in `store.bind` (+13–15).
  - The first create on a Store open adds a serial reservation in
    `files.change` (writer [0,6,2,1]) and about 12 create-path prepares:
    `finish` 3, `logical.count` 1, `bindings` 1, `publish` 7.

**W4/W6/W7 re-estimates:**
- **W4** (prepare create statements at open): saves about 17 trips on the
  first create per Store open, and only there. The warm second `new_file`
  (44) already equals `mkdir` (46) within 2.
- **W6** (plan paths): `files.change` costs 0–4 Workspace-reader trips warm
  (0–14 cold; `cp` cold 14), so W6 saves about 0–3 trips warm. The large
  `files.change` cost is the recursive remove: 106 Workspace-reader trips
  warm for 100 files, which is W5's target.
- **W7** (fuse locate and dependent read): `construct` (Workspace reader)
  and `finish` (writer) each run a locate (`WITH o AS MATERIALIZED`) and a
  span read (`substring(s.span …)`) in the same phase. Statement order within
  a phase is not recorded. W7 is therefore worth at most 2 trips per call and
  is conditionally warranted. Confirm the sequential dependency with a
  per-statement trace before building it.

**Release Init baseline (D15)** (`release-init.log`): release profile,
`fs_call_round_trips` binary sha256 prefix `88470101c59c64e4`, one sample
each, load 8–9.6, PostgreSQL 17.11.

| Shape | SQLite | PG local | PG +2 ms |
|---|---|---|---|
| `nested_1k` | 308 statements, 50.5 ms | 229 trips, 409.5 ms | 1,239.4 ms (calibrated median 3.0 ms) |
| `flat_1k` | 2,284 statements, 52.7 ms | 2,246 trips, 1,016.3 ms | 9,457.4 ms (median 3.75 ms) |
| `nested_10k` | 12,169 statements, 491.5 ms | 10,952 trips, 9,152.9 ms | NOT_RUN (over budget) |
| `flat_10k` | 31,882 statements, 510.0 ms | 31,042 trips, 16,202.2 ms | NOT_RUN (over budget) |

The `+2 ms flat_1k` command took 27.9 s, mostly Docker and `apk` overhead
for netem.

**Deviations and problems:**
- The first PG `w0` run panicked in the harness: a closure still held the
  server handle. It was fixed, the run was repeated, and both runs are kept
  in `pg.log`. The `W0` lines in `sqlite.log` and `pg.log` use an earlier
  case order and are superseded by `*-w0.log`.
- One `pg2` netem removal failed silently because of an `apk` fetch flake.
  The end-of-test removal cleared it. The removal check is now asserted, and
  a later `pg2` run verified it. Removal was checked after every arm.
- Ruff is not installed on this host, so the `tools/ci_tests.py` change was
  not Ruff-checked. Its unit tests pass.

### Lane C — H5 core (branch `rt818-lane-c`, `7647c0479f`, parent `3a904ac225`; merges at M2)

- New `layerfs-content` `compare/across.rs` (`PathAcross`, `DirectoryMemo`,
  `AcrossWork`, `VersionPath`/`PathAnswer`) and `filesystem::inode::read::lookup_tables`
  beside an unchanged `lookup_many`; `TreeTable::from_root`. Batched path only when
  the caller passes `batched = true`; `batched = false` with `DirectoryMemo::new(0)`
  reads exactly what today's per-version walk reads (SQLite unchanged).
- Demands per window (content reader, windows 16/32/64/128), predicted → measured:
  depth-12 28, 2 → 28, 2, 2, 2; 10k-entry dir changed every version 10, 5 → 13, 6
  (**+30%/+20% — diagnosed once:** inode height is 3, not 2; counts fit
  1 + h + Σ(h + dir height) cold, then 1 + h + m; recorded, no change);
  intermediate symlink 6, 2 → 6, 2; path created in a window later 3 → 6, 3
  (cold first window as formula); hardlinks later 2 → 2; kind change later 4 → 4.
- Tests: `core/crates/layerfs-content/tests/compare_across.rs` (9 tests, seeded
  random histories batched/fallback = per-version walk; exact per-window counts;
  `lookup_tables` = `lookup_many`), 0.9 s. Clippy clean.
- Docs: `docs/architecture/core/compare-and-merge/README.md` "One path across versions".
- Production LOC: 227,592 -> 228,159 (delta +567); core +567 (layerfs-content).
- **Open for H5 wiring (RH):** an integrity/demand failure in any version of a
  batched window fails the whole window, including versions past the point where
  blame would stop today. On a healthy Store answers are equal; on a damaged
  Store this could surface an integrity refusal today's walk never reaches. RH's
  wiring must keep today's refusal (e.g. on a window failure, decide whether the
  failing version is past the stop point) or this is raised as an order change.
- Pre-existing `cargo fmt` diff in `filesystem/state/mod.rs` left alone (not lane C's).
- Notes: shared notes folder `lane-c-notes.md`.

### Phase 0b — seam and equivalence driver (branch `rt818-p0b-equivalence`: `dd37798d55`, `d3428bd7b6`)

- `Visibility::from_markers(serial: u64, markers: Vec<PathRemovalRecord>) -> Visibility`
  in `read/visibility.rs`; no caller; unit test `tests/visibility_units.rs`.
  Production LOC: 227,592 -> 227,595 (delta +3), engine layerfs-store-service +3.
- `tests/sdk_round_trip_equivalence_seeded.rs` (test only, delta 0): all 15 `fs.*`
  methods + `project.remove_file`, `branch.fork`, `project.stats`,
  `branch.list_commits`; 186 steps / 44 Commits plus a 17-step 4,200-entry Init
  window test; refusals incl. the W1 Removed/NoSuchPath paths; markers, symlinks,
  repeated-inode and all-hardlink directories, 600-entry dir, depth 8/10.
  PG (`rt818_p0b`) 2 passed 11.0 s; SQLite-only 1.5 s; `method_refusal_order` green.
- Not compared (by design, varies run to run on one backend too): Commit ids/roots
  of SDK writes (wall-clock stamps; compared by position), `newStored`/`newMetadata`
  (compression of timestamped metadata); Init roots and facts compared raw.

### Phase 0 — orchestrator notes

- Phase 0a (`8a8fcea616`) + 0b merged; lanes fork from the merge.
- **Init 10k pin slack 32** (round trips and statements only): Init 10k varies
  ±8 run to run at the base, likely the locator-cache order in Init's
  `logical_size` walk, which I1 removes. Lane I must make the 10k pins exact
  after I1 (or diagnose the remaining variance); slack stays only if a
  diagnosed nondeterministic source remains.
- **Open owner item from W0 (not in the W plan, not done):** create, mkdir, cp,
  mv, rm and apply never take the fused one-message Commit (39–46 trips warm vs
  22 for an overwrite) — inferred cause: SDK staging limit 64 KiB / 8 = 8 KiB, so
  `covers()` is false after a directory rewrite. Raising that limit trades RAM
  and is an owner decision; recorded, not acted on.
- W0 re-estimates: W4 ≈ 17 trips on the first create per Store open only; W6 ≈
  0–3 warm; W7 ≤ 2 per call and needs a per-statement trace before building
  (W0 does not record statement order within a phase); recursive rm 100: 106
  Workspace-reader trips warm (W5's target).
