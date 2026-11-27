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
- **Cold vs warm targets (parent session, 2026-11-27):** SPEC targets come from
  the probe, which runs each case on the setup's open Store and roughly equals a
  pin's `warm` row. Record rule: **warm target = SPEC target; cold target = SPEC
  target + (cold − probe) from the Phase 0 table**; compare cold pins with the
  cold baseline by delta. The milestone benchmark on the probe harness is the
  absolute check; no extra check is added for this.
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

### Owner deferrals (owner decision 2026-11-27, relayed by the parent session)

Deferred, not built, and **not misses**:
- D2 / R7: the cross-call cache, including a root-keyed path memo;
- D4 / O4: returning the root object's location in the context statement;
- joining locate and fetch per level on the read path;
- preparing all read statements at Store open;
- raising the 8 KiB constructed staging limit so create/mkdir/cp/mv/rm/apply
  take the fused Commit (the Phase 0 W0 finding).

The plan stays: M2, M3, then R6; H7 skipped (D6).

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

## M1 (merged at `cfc9586d05`)

Merges: RH `1cb784d313` (R1–R3) → W `dd62841f38` (W1–W3, W5) → I `bfde99c690`
(I1, I2a, I2b); no textual conflicts. Read and write pins pass on the merged tip
(`receipts/M1/merge-pins-*.log`): W2 took write calls off the control reader, so
R1's −4 control-reader trips do not move the write pins. Production LOC
227,595 -> 228,392 (delta +503+152+142 = +797). Per-item detail: `lanes/RH.md`,
`lanes/W.md`, `lanes/I.md`.

| Item | Prediction | Measured (pins) | 15% | Notes |
|---|---|---|---|---|
| R1 | −4 trips every read | −4 every read; revision.h10 −6 (makes 3 of the replaced reads) | yes | 2 new statements, drop/kill swept |
| R2 | −3 reads per read call, −2 blame/diff | exact; SQLite −9 stmts (3-stmt tx per read) | yes | D1 SQLite pins; **fault-path change:** a failed serial read now refuses instead of silently using serial 0 (base `read/snapshot.rs:94-97`); fail-closed per `visibility.rs`'s own rule; healthy answers equal |
| R3 | SPEC −3 root, −7/component | exact to SPEC; cold stat d7 91 → 34 (−57 vs −56), grep1k 111 → 100 (−11 vs −10) | yes (vs SPEC, cold-delta rule) | walk kept even without markers to keep refusal order |
| W1 | −6 warm / −10 cold | exact | yes | D16 kept; with kept markers present, the base-tree open happens only on the refusal path |
| W2 | −4 warm / −7 cold | exact; control reader 0 on all write cases | yes | new named-bind flight, 26 messages drop+kill swept; cold separate-path Commit +1 writer trip |
| W3 | −1 warm | exact | yes | D7 Store slot |
| W5 | rm.dir ~88/~34 | 92/38 | yes (+4.5%/+11.8%) | `validate`'s 3 Workspace trips not in estimate |
| I1 | 205 / 2,225 / 606 / 20,700 PG | 209 / 2,226 / 624 / 20,720 | yes | 10k pins now exact (variance came from the removed walk) |
| I2a | flat 1k ~226, 10k flat ~706 | 228 / 726 | yes | D17 check adds 1 trip per unit; 84 messages drop+kill swept |
| I2b | SQLite windows of 32 | flat 1k 360, flat 10k 2,502 stmts | yes | 3 SQLite SQL files (place_children, bind_host_files, placement_dependencies) |

Pre-existing, not #818: `postgresql_round_trips` fails at `workspace_reader_open`
(11 round trips < 12 statements) identically at the Phase 0 base `54778271cd`
(`receipts/M1/postgresql_round_trips-base-54778271cd.log`) and the M1 tip.

Findings outside the spec (recorded, not built): `check_aliases` runs one read unit
per hard link (Init `wide_hardlinks` PG ≈1,200 trips, 339 transactions).

Tests deleted in M1: none.

### M1 independent review (fresh subagent, diff `54778271cd..cfc9586d05`)

Healthy-path answers equal on reads, writes and Init; R1, R2, R3, W1, W3, W5, I1,
I2a/I2b checked correct (I2 window cuts mirror `independent`; D17 covers every
stored-row constraint). Findings:
1. **W2 (bug, fault path):** `write/files.rs:273` maps a `BindError::Read` in
   phase `Snapshot` to `published`, but reader acquisition (`store/bind.rs:129`,
   `:148`) also runs in that phase before the lookup → a reader-open failure
   answers `Unknown` (custody) where the parent answered `NoSuchProject` /
   `Failed`. **Sent to lane W to fix with a test** (fix lands with M2).
2. **R3 (nit):** `Cached::new` now precedes the hidden-path check, so a
   decompression-workspace failure would precede `Removed`. **Sent to lane RH**
   (fix lands with M2).
3. **W1 (nit, recorded):** with kept markers, an `open_tree` failure surfaces
   inside `removed()` as `Unknown`, as the parent's lazy `snapshot::entry` did.

### M1 benchmark (`receipts/M1/REPORT.md`, one sample per case and arm)

22 of 32 rows exactly at expectation; every outcome equals P0. Headline PG trips
P0 → M1 (PG +10 ms ms): stat `/` 18 → 8 (256 → 107); stat d7 73 → 17 (1,029 →
234); read small 38 → 10 (514 → 145); list100 33 → 12; grep1k 78 → 67 (1,120 →
912); overwrite 24 → 13 (318 → 188); new file 66 → 56; rm dir 151 → 41 (2,142 →
638); Init 10k nested 10,952 → 624 (+10 ms 14.2 s, was ≈141 s projected); Init
10k flat 31,042 → 726 (15.2 s, was ≈386 s). All Init rows now run directly at
+10 ms.

Beyond 15%, diagnosed once:
- stat.depth3 28 vs 22 (+27%): first call of the group pays R1/R2's prepares on
  a control reader the setup no longer warms (W2); within its cold target 31.
  Recorded.
- stat.depth7 17 vs ~34: better (R3's walk costs ~0 per cached component).

Recorded, within 15% but noted:
- blame.h100 probe 2,428 vs P0 2,325 (+4.4%): probe starts colder (setup writes
  no longer read through the control reader); pins at the tip show the predicted
  −6. Not a code regression.
- **SQLite Init flat_10k 510 → 564 ms (+10.5%)** with statements −92%: against
  the "no SQLite call slower" guardrail; sent to lane I to diagnose once with I3.
- +10 ms latency band (≤1.10×) exceeded on mkdir 1.17, rm.dir 1.11, Init flat_1k
  1.18, Init nested_10k 1.15 (+2 ms: blame 1.11–1.21). Client statements equal
  server top-level calls in every flagged row, so no trip is uncounted; the
  excess is time without statements (P0 spread reached 1.18 at +2 ms).
  Needs a per-statement trace to attribute; recorded.
- Peak memory 13.5–15.7 MiB per fs command, Init 12 MiB (1k) / 23–29 MiB (10k);
  no P0 memory receipt existed to compare.

## M2 (merged at `7a79aaaea5`)

Merges (each with its LOC block): lane C `7647c0479f` → `fef2f4ccc8` (+567, core
layerfs-content); lane RH R4, R3 review fix, R5, H1, H2, H3 → `642c3ddbab`
(+634); lane W W2 fix, W6, W6 SQLite guardrail → `7a79aaaea5` (+279). Production
LOC 228,392 -> 229,872 (delta +1,480). Pins, equivalence and refusal order green
on the merged tip (`receipts/M2/merge-*.log`).

| Item | Prediction | Measured (pins) | 15% | Notes |
|---|---|---|---|---|
| R4 | 0 PG (SPEC) | 0 PG; SQLite −1 per read | yes | one `Cached` per call |
| R3 fix | — | — | — | caches made at first demand: review nit fixed |
| R5 | −4 warm / −8 cold reads | exact; stat `/` warm 4 | yes | `read_context_named/node` swept (75 msgs); SQLite unchanged (corrections rule) |
| H1 | blame h100 −450..−550 | −452 (writer 0); revision −725 | yes / better | Branch names read once on the control reader |
| H2 | revision h100 ~1,860–2,060 | 1,705 warm | better (>15%) | a path walk cost ~13.5, not 10–12 |
| H3 | blame h100 ~900–1,100 | 254 warm (491 cold); revision 253; diff 23 | better (>15%) | a version's objects sit in one held pack (~2 trips/version) |
| W2 fix | parent refusals restored | reader-capacity test now `NoSuchProject` | — | `looked_up` flag; SQLite-only test (64 readers) |
| W6 | apply.10_dirs ~36 / ~9 | 34 / 8 Workspace reader | yes | PG only; SQLite keeps M1 full-stat walk (every SQLite pin = M1) |

Better-than-predicted H2/H3 were diagnosed once from per-role counts (above);
recorded, nothing to fix. SQLite pins (D1): blame.h100 6,746+597 → 779, revision.h100
12,382 → 769, diff_files 187 → 71.

### M2 benchmark (`receipts/M2/REPORT.md`)

All 28 `fs.*` rows within 15% of the lane expectation (22 exact). Every history
row now runs directly at +10 ms: blame h100 2,428 → 459 trips (31.2 s at P0 →
5.4 s), revision h100 3,887 → 256 (50.9 s → 3.1 s), diff 87 → 35 (423 ms), stat `/`
4 (50 ms), read small 6 (75 ms). Writes unchanged (cp 33 → 32). Init counts
unchanged (no-change check). Flags: SQLite Init flat_10k time (I2b scan, fixed on
lane I's branch, merges at M3); read.lines_100_200 latency 1.13 on 5 trips; load
8.5–11.5; peak memory +1.5–4.3 MiB per command (history 18.1 MiB: H3's 3 MiB-bounded
per-call cache plus memo; writes moved too without code change).

## M3 in progress

### Lane I — I3 (branch `rt818-lane-i`; merges at M3)

- `24c69d577b` I3a: acquisition read units as one fenced autocommit statement
  (owner joined in; sizing reads folded). Exact to prediction on every shape.
- `cef20dfd63` I2b SQLite fix (M1 benchmark flag): the window `UPDATE … FROM`
  scanned every child of the parent per window (~3.1M rows / 314 windows at
  flat 10k); now a primary-key probe per child. Release flat_10k 494 → 299 ms
  (pre-I2b 510). Same statements, rows, answers; plan-guard test added.
- `e49ef69187` I3c (D12): Init's three Saves publish through `publish_flight`
  behind `Save::publishing_in_flights`, via `publish_captured_tail` (no payloads
  kept; untrusted completion answers Unknown as the ordinary path). −36 at 10k
  (predicted −25: a pack COPY is 2 trips). Sweeps 68 + 68.
- I3d: **not done** (D13 not taken).
- I3b: **decision (orchestrator): C now, then A; not B.** The spec's one-message
  write unit with the dependency check as its one wait cannot be built on
  today's session layer: `DecidedTurn::send` prepares uncached statements
  inside a READ ONLY BEGIN…COMMIT, which inside an open write unit would commit
  it half-way. Options were (A) a session-layer capability (lane W's files),
  (B) an optimistic one-flight unit that continues on today's path after a
  dependency refusal — extends the owner's Commit-only fallback exception, so
  owner decision, not taken; (C) build only the no-wait units.
  - `8f5c180c6c` I3b part C: `complete_files`, `set_directory_roots`,
    `advance`, `discard`, `dispose` as one fenced flight each (new
    `acquisition/flights.rs`). nested_10k 303 → 245, flat_10k 342 → 284
    (predicted ~245 / ~284); 1k shapes −26 (predicted −22; the 1k `dispose`
    falls back to two tail lookups, diagnosed). Sweeps 327 + 327 over 7 unit
    kinds. Production LOC +312.
  - Part A: lane W implements the session capability (note
    `I3b-session-capability.md` in the shared notes folder), then lane I
    builds `put_entries` / `place_children` waiting units on it.
- Production LOC through part C: 228,392 -> 228,755 (+363 on lane I's branch).

### M2 independent review (fresh subagent, diff `ba5386f3ae..7a79aaaea5`)

No healthy-path bug. R5 SQL/decoding and order, H1 rows, H2 carry, H3 memo
(keyed by directory content root: exact), W2 fix, W6 (SQLite unchanged, PG walk
order) and `across.rs` checked correct. Findings, sent to the owning lanes:
1. `history/rows.rs` per-call Branch-name map unbounded (operator history/stats)
   → RH: bound it.
2. `diff_files` with attribution holds two `Cached` (≈2 × 3 MiB) → RH: share one.
3. `across.rs:151` `DirectoryMemo` outer map grows when full → RH.
4. Fault/corruption-only refusal order: R5 statement failure before the `path`
   argument refusal; corrupt marker row before "belongs to another project";
   W6 plan-read failure before that change's own argument refusal → RH and W
   restore where no trip is needed; any residue goes to the owner as a named
   change (Principle 5).
5. Nit: `operator/files.rs` Branch-name read error text differs only on a fault.

### Disk episode (2026-11-27)

Host free fell to 87.6 GB and the Docker guest to 0 B (PostgreSQL refused Store
creation with `Capacity`; lane RH paused H4). One cleanup episode via
`tools/cleanup.py` plan/apply (pass folders `~/.cache/layerfs-cleanup/2026-11-27/`):
host 87.60 → 166.72 GB (net +79.12 GB; **300 GB target not met, short 220.9 GB**);
guest 0 → 82.6 GB free. Most of the gain came from another session shrinking its
own target volume; this pass removed ~2.9 GB (6 idle volumes, 101 orphan
anonymous volumes, the two finished #818 agent worktrees, archived under
`refs/cleanup/20261127T011107Z-5c90d6ee436a/*`). Protected/uncertain allocations
(other sessions' #819 targets and volumes, #816 measurement data, dsh, BuildKit
cache, launcher records) were left alone.

### M3 merges so far

- Lane W `c3906bd027` (I3b session capability: `prepare_unit` / `send_in_unit` /
  `count_unit`; 31-message two-flight unit drop+kill swept; existing session,
  mount and commit fault tests green) and `2bd35e981a` (W6 order fix: a refusal
  that needs no read comes before the plan; 131-message drop sweep) → merge
  `34dcec810c`, Production LOC 229,872 -> 230,065 (+193).
  - **Residual fault-only order difference (W6), for the owner:** a failed plan
    read refuses at the window's first change that reads; without the plan that
    change might have answered and the failure surfaced at a later change.
    Matching it would need a re-read after the failure (a retry), so not done.
    Documented in store-calls `README.md`.
  - Pre-existing flake noted by lane W: `fused_commit`'s
    `postgresql_cancelled_barrier_is_released` and
    `postgresql_lost_backend_without_start_is_not_terminated` pick the paused
    backend server-wide, so they can interfere when run in parallel (passed alone).
- Lane I `24c69d577b`…`8f5c180c6c` (I3a, I2b SQLite fix, I3c, I3b part C) →
  merge `0ef4c6526b`, Production LOC 230,065 -> 230,428 (+363); doc conflict in
  `docs/architecture/engine/store/postgresql.md` resolved keeping both lanes'
  Statement-mode uses. Init pins, write pins and equivalence green on the tip
  (`receipts/M3/merge-WI-*.log`).
- Lane RH `bd564456cf` H4 + `55f5d4fedc` M2 review fixes → merge `f10b477d40`,
  Production LOC 230,428 -> 230,600 (+172). History/read pins, equivalence and
  `method_refusal_order` green on the tip (`receipts/M3/merge-RH-*.log`).
  - H4 (PG warm, cold): blame.h100 254 → 59 (491 → 296), blame.h10 33 → 16,
    revision.h100 253 → 72 (11% over the ~55–65 range: `fs.revision` reads a
    serial only where the path exists, so the window saves less; diagnosed,
    recorded), revision.h10 cold 82 → 66. SQLite unchanged. Sweep
    `lineage_chain_window` 492 messages drop + kill.
  - Review fixes: Branch-name memo ≤ 1,024; one `Cached` for diff_files with
    attribution; `DirectoryMemo` bounded when full; corrupt-Store refusal order
    restored (a corrupt recorded Branch costs one extra read, corrupt Store only).
  - **Residual fault-only order difference (R5), for the owner:** a failure of
    the context statement itself refuses before an invalid `path` argument
    refusal. (Before R5 this already held when the fault hit the project read;
    only a fault exactly on the old marker read refused after `path`.) Removing
    it needs a second statement per read or checking `path` first, which would
    reorder healthy `NoSuchProject`/`NoSuchBranch` vs `InvalidInput`; neither done.
  - Ownership of persistence `history/catalog.rs` and
    `sql/postgresql/queries/history/` passed to lane W at this merge.
- **H5/H6 refusal-order blocker and decision (orchestrator).**
  `AuthenticatedObjects::read_canonical_batch` returns one result per demand, so
  a damaged object in any version of a batched window fails the whole window,
  including versions past where today's blame/revision walk stops (and blame's
  batched text fetch / diff_files' two-sided planning likewise): a corrupt-Store
  order change. Options: (1) per-object read results, an additive read-port
  method with a default; (2) accept the corruption-only order change (owner);
  (3) bounded windows for revision only (partial). **Taken: (1)** — no answer,
  order, format or custody change; additive core method per SPEC §5.
  - RH `2a9a328930`: `AuthenticatedObjects::read_canonical_each` +
    `reads_each_at_once()` (default false; one-by-one default, no re-read).
    LOC +6 (layerfs-content). The batched read lives in core storage's shared
    read path (`layerfs-storage` `read/objects.rs`, `fetch.rs`, `prefetch.rs`,
    `length.rs`, `encoding/pack_cache.rs`, `encoding/pool/read.rs`; both
    engines), not in lane W's PG provider. Exact per-object errors there are a
    sizeable refactor: new **lane S** (orchestrator decision) implements it in
    parallel; RH builds the H5/H6 wiring on the port, gated on
    `reads_each_at_once()`, with a test-only reader to force batching for the
    corruption-order tests.
  - Lane S constraints (parent session): SPEC §5 core rules — additive only
    (existing read paths and signatures unchanged), no format change, SQLite
    statement pins unchanged unless a listed pure dedupe, memory bounded by the
    existing `ReadCache`/`KeptReads` limits, one attempt. Lane S's added core
    files and LOC go into RECORD and the layout listing; its diff is part of the
    M3 independent review.
- Lane I `fc914d59e0` I3b part A on lane W's session capability: `put_entries`
  and `place_children` as one transaction over several flights of one writer
  turn (new `acquisition/unit.rs`); waits = the D17 dependency check (kept, same
  order) and a dependent row's `BIND_HOST_FILE`. **Init 10k: nested 215, flat
  247 PG trips (≤ 250 target met;** baseline 10,955 / 31,039). 1k nested 99, flat
  106; hardlink_window 298; wide_hardlinks 709. SQLite unchanged. First
  measurement 218 / 251 vs predicted ~213 / ~243 (16–20% smaller reductions):
  diagnosed once (the old generation fence rides in the BEGIN batch, not its own
  trip); then a revised prediction for a shared prepare (the first unit prepares
  every acquisition write text) ~215 / ~247, measured exactly. **Note:** that
  shared prepare is close to I3d (not taken, D13) but runs only when Init runs,
  not at every Store open, so D13's per-open trip does not apply; recorded.
  Sweeps: `postgresql_acquisition_units` 156 + 156; placement 72 + 72; flights
  251 + 251. Production LOC +375 (layerfs-persistence).
- Lane S `dec2216e23` (on RH's `2a9a328930`): exact per-object results for one
  batched demand in `layerfs-storage` (additive: `Reader::read_canonical_each`,
  `reads_each_at_once() = true`, `Storage::read_each_with`, `lengths_each_with`,
  `read_each_kept`; `read`, `chains`, `prepare_leaves`, `length::read`
  unchanged). Healthy demands make the same provider calls; transport/session
  and multi-unit failures fail the call; anything else is the error of exactly
  the dependent ids, equal to each id's point read; a failed pack is not
  requested again. 9 new tests; storage suite 191 passed; read/history pins
  unchanged on both backends; equivalence green. Production LOC 230,606 ->
  231,175 (+569, core layerfs-storage). No new core files; per file:
  `read/objects.rs` 199→428, `read/prefetch.rs` 85→215, `encoding/pool/read.rs`
  586→677, `read/provider.rs` 101→160, `read/length.rs` 66→119, `read/fetch.rs`
  873→880. Limit being closed: a pack refused inside a multi-pack group read
  (batched PG) still failed the whole call → `read/hints.rs` follow-up (lane S).
- Lane W `b7dd8bfd48` W4 (D8): one SDK-open prepare flight of the writer
  statements a first create sends (no fenced read statements: owner deferral;
  `commit_snapshot_epoch` and `lineage_parent` included as SELECTs inside the
  Commit write transaction), re-prepared after reopen (`sdk_writes` flag);
  `reserve_inodes` on PostgreSQL one pipelined message (`allocation_reserve.sql`,
  `history/reserve.rs`, dispatched from `catalog.rs`; authority then Capacity
  checked on the server). new_file cold 96 → 80, mkdir cold 90 → 74 (predicted =
  measured); warm unchanged; SDK open +1 trip. Sweeps: reservation 27 + 27,
  open prepare flight 53 + 53. Init −6 trips / +1 statement (W4 edited lane I's
  Init pins). Production LOC +215.
  - **Met under the cold rule** (parent session correction): `write.new_file`
    cold target = SPEC ~40 + (cold 112 − probe 66 = 46) ≈ 86, measured 80;
    `mkdir` cold target = SPEC ~26 + (113 − 45 = 68) ≈ 94, measured 74. Note
    (not a miss): the cold construct alone is 18 trips, and the read-statement
    prepares that would lower it further are an owner deferral.
  - Asked lane W to **revert** its loosening of `postgresql_round_trips`'
    `round_trips >= statements` check (AGENTS: never weaken an assertion); that
    test's failure is pre-existing (recorded at M1).
  - Guard failure found by W: `postgresql_statement_schema::every_one_statement_read_names_the_schema`
    trips on I3a's `read_statement(body)` → lane I to fix.
- Lane W `d78f3f85a0` W7: **not built.** Per-statement trace of a cold
  `apply.10_writes`: the membership locate is not followed by a read of what it
  returned; the construct's base-path locates pair with dependent reads but each
  locates a single object (core `read/fetch.rs` turns the join off for
  single-object demands); upper bound 1 trip of 80. Follow-up recorded (not
  built): hand the bind's Workspace-reader base-path locates to the Save (up to
  6 cold construct locates).
- Lane RH `ea3516d884` H5 wiring (`read/versions.rs`, windows 16/32/64/128 via
  `PathAcross`; `Cached` records each object's own failure and never re-reads
  it; a window with a data failure walks its versions alone; transport failure
  refuses at once; `Batching::Provider` only on PostgreSQL with
  `reads_each_at_once`; `Batching::Forced` test-only), LOC +244; `3d377ea174` H6
  (`lengths_each_with`; read-ahead lengths and blame text roots ≤ 64 / 4 MiB, or
  revision attribute roots; `diff_files` both sides together; `equal_edges` per
  pair; `CONTENT_QUERY_BYTES` charged in today's order), LOC +217. Not
  measurable until lane S merges and `store/ports.rs` forwards to
  `Storage::read_each_with` / `lengths_each_with`. Predicted PG warm after S:
  blame.h100 ~18–28 (SPEC ~29), blame.h10 ~8–12, revision.h100 ~22–32 (SPEC ~30),
  diff_files ~20–21 (SPEC ~12: per-side resolve, `for_snap` and attribution trips
  not touched by H6 — open, to diagnose from the merge-time trace). Corruption
  tests (forced batching): a damaged tree root / text refuses only where the
  walk reaches it.
  - **Residual fault-only order difference (H6, batched mode), for the owner:**
    `diff_files` locates the right side before reading the left text, so a
    right-side locate failure comes before the left text's demands.
- Lane W `bd1ee54013`: `postgresql_round_trips` check reverted to the original `round_trips >= statements` (no replacement); the test fails as recorded at Phase 0, and W4's pipelined reservation now also trips it. Lane W complete for M3.
- Merge `1e77af6757` (lane W M3): Init pin conflict (I3b A vs W4) resolved to
  part A's counts with W4's −6 trips / +1 statement, predicted and measured
  exactly: **Init 10k PG nested 209, flat 241** (`receipts/M3/merge-W4-*.log`).
  Production LOC 230,975 -> 231,190 (+215).
- Lane I `428fc626f3` → merge `71bf02bffc`: I3a acquisition reads now call
  `read_statement` per site with qualified keys from 9 new
  `sql/postgresql/queries/history/acquisition_*.sql` files (SQL unchanged but
  whitespace); `postgresql_statement_schema` green, guard unchanged. Production
  LOC 231,190 -> 231,247 (+57, SQL text moved into shipped .sql files).
- `visibility_guard` fails on R5's `read_context_node.sql` (reads `branch` /
  `layer_stack` without the §4.2 filter) → lane RH to fix (match the one-by-one
  statements it replaced; never weaken the guard).
- Lane S `0ad54a7401` (hints follow-up): per-object demands read pages through
  one `read_groups_classified` call and whole packs through
  `Fetch::whole_packs_each`, so on the batched PostgreSQL route a refused pack
  inside a multi-pack group read is the error of exactly the dependent ids, each
  equal to its point read, requested once; existing callers keep `read_groups`
  and their counts (no `Fetch::per_object()` guard). Production LOC 231,175 ->
  231,300 (+125, core layerfs-storage; `read/hints.rs` 684→802,
  `read/objects.rs` 428→434, `read/fetch.rs` 880→882, `read/prefetch.rs`
  215→214; no new files). `read/fetch.rs` is at 998 physical lines (ceiling 999).
  **Recorded edge:** a pack first refused by a reconstruction's own acquisition
  after the walk (only when the pack cache evicted a group the walk read within
  one cohort) is acquired again by that reconstruction; the root still gets its
  point-read error. Corrupt Store + eviction only.
- Lane S merged → `eb0d8c6b69` (with RH's port `2a9a328930`): Production LOC
  231,247 -> 231,947 (+700, core: layerfs-content +6, layerfs-storage +694).
- Lane RH `ef2159594c`: `visibility_guard` — `read_context_node.sql` registered
  on the guard's allow-list like the `branch_branch_by_id_any.sql` read it
  replaced (a removed Branch's own row still shows in a ref; adding the filter
  would change answers); the entry requires `layer_stack_retired`, so the rest
  stays filtered. Guard not weakened; test-only.
- Lane RH `029bd57d68` switch-on (after merging `eb0d8c6b69` as `5f8a7145ee`):
  `StorePorts::objects_each_with` / `lengths_each_with` → `Storage::read_each_with`
  / `lengths_each_with` via new `ReadSession` entry points; `reads_each_at_once`
  true; `Batching::Forced` replaced by test-only `Batching::Alone`. LOC +20.
  PG control, warm (cold): blame.h100 59 → **22** (296 → 65; SPEC ~29 met);
  blame.h10 16 → **12** (54 → 38; SPEC ~17 met); revision.h100 72 → **48**
  (301 → 92; predicted ~22–32, SPEC ~30: **miss**); revision.h10 cold 66 → 52;
  diff_files 23 → **22** (49 → 48; SPEC ~12: **miss**). SQLite unchanged.
  Diagnosed once (PG statement log, own database):
  - revision.h100: the walk passes the 100 Commits into the Branch's base
    Layers: Commit part ≈ 26 trips (in prediction); Layer part ≈ 22 (12 Layer
    rows one at a time + ~10 object reads of Layer versions walked alone). H4's
    row windows follow Commits only. Fix = a row window along the Layer chain:
    **new item, not built, recorded.**
  - diff_files: H6 already plans both sides' lengths and texts in one demand;
    the rest is per side (Commit row read 4×: resolve, `belongs`, `for_snap`,
    reference; tree root; path; serial; markers; a Branch row per reference).
    Reaching ~12 needs one row read per side and markers once per call (−6 to
    −9): the R5 read-context route applied to diff_files' sides. **Follow-up,
    not built, recorded.**
  Sweeps `sdk_history_fault_rh_{drop,kill}` now cover the live per-object path
  (green). Equivalence, refusal order, both guards green.

## M3 (merged at `5af53d4667`) — benchmark `receipts/M3/REPORT.md`

Production LOC from the M2 record `2c440193d5` (229,872; core 46,302, engine
108,420) to M3 `5af53d4667` (232,428; core 47,056, engine 110,222): delta
+2,556 (core +754, engine +1,802, control 0, api 0); per-merge crate deltas are
in each merge commit's LOC block. Every row ran directly at +10 ms.
Headline PG trips baseline → M3 (+10 ms): blame h100 2,325 → 51 (31.2 s →
632 ms), revision h100 3,890 → 52 (50.9 s → 831 ms), blame h10 267 → 37, diff
88 → 35, Init 10k nested 10,952 → 209 (≈141 s → 8.4 s), flat 31,042 → 241 (≈386 s
→ 9.0 s), Init 1k flat 2,246 → 100.

SPEC §3 exit at M3: reads, grep 1k, recursive rm, Init ≤ 250, rm file, budget,
SQLite rule and outcomes **met**; blame met on warm pins. **Not met:**
revision (Layer-chain rows not windowed), diff_files ≤ 15 (per-side row
re-reads), mkdir/cp/mv ≤ 26 (non-fused Commit path; the staging-limit lever is
an owner deferral), and the ±10% latency band on 7 rows at +10 ms (new at M3:
statements equal server top-level calls, so time without statements;
unattributed — needs a per-statement timing trace). PG local Init 10k time
(5.9 / 6.0 s) is server foreign-key work, not trips.

## R6 (D3) — lane RH `773de194be`

Unpaged `fs.grep` / `fs.glob` directory pages up to 256 names (8,192-byte page
bound unchanged); grep windows up to 256 files, root demands ≤ 256 roots and the
existing 4 MiB; PostgreSQL only (`batched_reads`); SQLite unchanged. Production
LOC 232,428 -> 232,476 (+48, layerfs-store-service).

| Case (PG pins) | Before cold / warm | After | Predicted | Target (cold rule) | Verdict |
|---|---|---|---|---|---|
| grep.tree_1k | 92 / 24 | **62 / 15** | ~45–60 / ~12–20 | warm ≤ ~29 (SPEC), cold 29 + 33 = 62 | met (cold 3% over prediction) |
| grep.one_file_dir | 20 / 5 | 20 / 5 | unchanged | — | — |
| glob.tree | 30 / 6 | 30 / 6 | ~25–30 / ~4–6 | — | within |
| glob.dir_100 | 21 / 5 | 21 / 5 | ~19–21 / ~3–5 | — | within |

Large-file shape (new `sdk_read_large_rh`, 1/4/16/64 MiB texts): PG grep.large
321/280 → 291/255; glob unchanged; peak RSS PG 242.24 → 242.22 MB, SQLite 232.65 →
233.00 MB (setup alone 238.9 MB): no regression. **Open:** `grep.large` counts vary
run to run on the same code and Store (SQLite cold 289–293, warm 257–267; PG warm
248/249); not diagnosed (bounded reader caches suspected); its pin holds round
trips and statements within ±8 (other cases and fields exact). Recorded as a
weak pin, not a guard loosening of an existing pin.

### M3 independent review (fresh subagent + a sub-review of Init/session; diff `2c440193d5..5af53d4667`)

Healthy-Store answers hold; existing storage callers unchanged (no-ops without
the per-object guard); H4 CTE correct and re-checked in Rust; W2/W6
`refused_unread` correct; Init/session: lost reply quarantines before judge,
open unit rolls back, no implicit COMMIT, no resend; server-side assertions
cannot fire on valid data; W4 authority then Capacity, refusal writes nothing;
every production file ≤ 999 lines. Findings, sent to the owning lanes:
1. **Bug (damaged Store, order):** `store/ports.rs` `object_owned` rejects the
   per-object kinds lane S reports (refused pack `Io(Missing/Malformed)`,
   `MissingDependency`, …), so a refused pack fails a whole history window at a
   version the lone walk never reaches → RH (match storage's `call_failure`
   complement; SDK test deleting a pack row).
2. `Cached::read_canonical_each` duplicate ids misattribute errors (latent) → RH.
3. `Cached.failed` unbounded by a named bound (damaged Store only) → RH.
4. `layerfs-history/src/lib.rs:34` fails rustfmt → RH.
5. Init/session nits: uncounted ROLLBACK (`unit.rs:342`) → lane I / W;
   `send_in_unit` lacks a post-rollback guard → W; empty first flight still
   sends ROLLBACK (+1 trip) → W; I3a joined reads stopped decoding the owner row
   (corrupt owner no longer `Integrity`) → lane I restores the decode.
Recorded, no change: a whole-pack cohort read refuses every object of a pack on
a frame failure where a lone point read might take a ranged read; `chains_each`
base dedupe can let an over-depth chain escape the depth refusal (as batched
`read` did before).

### M3 latency diagnosis (receipts/M3/latency-diag/)

Diagnosed once, as the 15% rule asks. The 7 rows over the +10 ms latency band
are **not extra round trips**: wire exchanges equal counted trips in every
case; no slow start, no flight waiting twice. The excess is Docker Desktop
netem releasing replies 3–4.6 ms late per trip, plus longer client gaps under
host load (7 → 16), against a `SELECT 1` median calibration that understates
the mean. Same binary, same trips, ratios moved run to run (overwrite 1.36× →
1.07×, apply.10_writes 0.95× → 1.97×). Recorded as an environment cause; no
code change. Open: the M2 → M3 per-trip wall difference on revision.h100
(12.05 vs 16–17 ms) is unexplained (no M2 capture). Suggested harness change
(not done): calibrate by the mean of a pause-matched loop or the in-container
delay, and treat local netem as indicative.

### M3 review fixes merged, and S3

| Merge | Lane commit | Change | LOC |
|---|---|---|---|
| `f7dac666ad` | W `ffd5a9a426` | `send_in_unit` refuses after a failed flight; no ROLLBACK for an unsent first flight | +2 |
| `c2e00dd9c0` | I `857431518d` | joined acquisition reads decode the owner row (corrupt → Integrity); ROLLBACK counted after a failed check | +10 |
| `f4936e01c8` | RH `645d0aedea` | `object_owned` = `!call_failure` (shared); `read_canonical_each` dedupe; `Cached.failed` ≤ MEMO; rustfmt | +20 |
| `3a15eba8f4` | S `8c1b6b7466` | S3: `locate_chains_each`; a deleted pack row (PG) fails only its objects and dependents; relocation into `read/values.rs`, `read/per_object.rs` | +104 |

No tests deleted. No healthy-Store trip changed (pins unchanged on both
backends). Fixed: the damaged-Store refusal-order bug (corrupt pack and deleted
pack row both refuse only where the lone walk reaches them).

### Final check at `3a15eba8f4` (receipts/final/)

All 19 affected binaries pass once. Probe at SQLite and PG local: every count
equals M3 except PG grep.tree_1k 63 → 35 (R6). Final table: `receipts/M3/REPORT.md`
"Final comparison" with grep.tree_1k PG trips 78 → 35 and PG local 79.4 → 28.8 ms
(2.8×); its +10 ms was not re-run.

Range LOC:
```text
Production LOC: 227,592 -> 232,612 (delta +5,020)
  core     45,655 ->  47,144 (+1,489)  layerfs-content +575, layerfs-history +123, layerfs-storage +791
  engine  106,787 -> 110,318 (+3,531)  layerfs-persistence +1,920, layerfs-project +11, layerfs-store-service +1,490, layerfs-workspace +110
  control  64,634 ->  64,634 (0)
  api      10,516 ->  10,516 (0)
  reference absent (retired R9c; last counted 65,417 at tree 498dd1917812)
  Method: tools/production_loc.py --detail on base 1317ebf7ca and the tip 3a15eba8f4
```
