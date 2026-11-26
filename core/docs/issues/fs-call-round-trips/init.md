# fs call round trips: Init track

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract. Part of [SPEC.md](SPEC.md). Source pinned at `056e920166`; all after-values are estimates until measured. The corrections at the end override earlier text.

**Scope.** `project.init` (Namespace Init: import a host directory into a new
project) on a PostgreSQL Store. SQLite behaviour changes only where noted
(I1, I2b). Init keeps its parallel construction profile (`files.rs` workers), the
shared Content/Storage/Persistence construction, and one encoder. No Store
format or schema change.

Source: v2 `056e920166` (code identical at `c9dfd0eba5`). Paths below are
repository-relative.

## Where the trips go (verified)

For every PostgreSQL Init case, client round trips = statements +
2 × transactions + prepares, to within 2. Nested 10k: 10,666 + 2 × 120 + 47 =
10,953, measured 10,955. Flat 10k: 30,700 + 2 × 144 + 49 = 31,037, measured
31,039. An acquisition unit is one transaction:

- BEGIN with the fence, in one message;
- the `OWNER` select (`engine/crates/layerfs-persistence/src/backend/postgresql/acquisition/provider.rs:49-63`);
- the body;
- `SELECT 1; COMMIT` (`GUARDED_COMMIT`, `.../postgresql/session/transaction.rs:761`).

Three per-file or per-window terms dominate:

1. **Logical size, about 1 trip per regular file above about 4k files.**
   - `init.rs:176-181` calls `statistics::logical_size`
     (`core/crates/layerfs-content/src/filesystem/statistics.rs:32`). That walks
     every inode leaf and opens each regular file's root for its length
     (`bytes`, `:20`).
   - Each root read is a single-object demand. It costs a `locate` once the
     shared locator cache (`READ_OBJECT_LIMIT` = 4,096,
     `core/crates/layerfs-storage/src/store/policy.rs:77`) no longer holds
     the roots.
   - Measured: 10,142 locates at 10k nested and 10,135 at 10k flat; none at 1k.
2. **Wide-directory placement: 2 statements per regular-file child.**
   - It applies to every directory that reaches `WINDOW_ROWS` = 512 entries
     (`engine/crates/layerfs-project/src/import/scan.rs:202`, `backing.rs:22`).
   - `place_children` (`.../postgresql/acquisition/writes.rs:439-478`) sends
     `BIND_HOST_FILE` (`INSERT … ON CONFLICT … RETURNING`, `:35`, via `bind`
     `:151`) and then `PLACE_ENTRY` (`UPDATE … RETURNING kind`, `:37`), one
     child at a time.
   - A directory or symlink child costs only `PLACE_ENTRY`.
   - Measured: exactly 1,000 and 10,000 of each at 1k and 10k flat.
   - SQLite runs the same loop in process (`.../sqlite/acquisition/writes.rs:169-199`).
3. **Windowed work, everything else:**
   - read units of `READ_WINDOW_ROWS` = 512
     (`core/crates/layerfs-storage/src/port/acquisition/contract.rs:12`), 4–5 trips each;
   - write windows of `WRITE_WINDOW_ROWS` = 4,096 or `WRITE_WINDOW_BYTES` = 1 MiB (`:16-18`);
   - Save publications of about 7 trips (`.../postgresql/objects/publish.rs:77-83`);
   - binding hints per 4,096 rows (`engine/crates/layerfs-project/src/import/hints.rs:9`);
   - 47–49 first-use prepares.

   At 10k nested this is about 606 trips (inferred from the totals).

## Targets

> SPEC §4 recomputes these rows with each case's own measured cost per trip and the cold-pin rules; where the two differ, SPEC §4 governs.

**How the projections are made:**
- Measured values are one sample each (test profile, `056e920166`).
- Projected PG local = projected SQLite ms + trips × **that case's** measured local cost per trip, (PG local − SQLite) / trips: 1.63 ms (1k nested), 0.58 (1k flat), 1.17 (10k nested), 0.75 (10k flat). Init's statements carry write work, so its per-trip cost differs from the read calls' 0.69 ms.
- Projected +10 ms = projected local + trips × that case's measured extra per trip: 11.68 (1k nested), 11.43 (1k flat), 11.65 where +10 ms was not run.
- The 10k "now" values at +10 ms were not run (over the 2-minute limit).
- The 100k row extrapolates the 10k measurements linearly; it is not a measurement.
- I2b is the SQLite form of I2 (owner decision 5); without it, SQLite's columns after I2 stay as after I1.
- SQLite times after I1 assume the same ~120 ms cut in both 10k shapes (the same ~10,170 statements are removed); I2b's ≤ ~100 ms is an upper bound from flat vs nested.
- The PG columns use the SQLite time with I2b, as SPEC §4 does.

| Shape | Arm | Now: trips / time | After I1 | After I1+I2 | After I1+I2+I3 |
|---|---|---|---|---|---|
| 1k files, 10 dirs | PG trips | 229 | ~205 | ~205 | ~140 (est.) |
| | PG local | 429 ms | ~390 ms | ~390 ms | ~284 ms |
| | PG +10 ms | 3.1 s | ~2.8 s | ~2.8 s | ~1.9 s |
| | SQLite stmts / ms | 308 / 55 ms | ~290 / ~55 ms | same | same |
| 1k files, one dir | PG trips | 2,246 | ~2,225 | ~226 | ~150 (est.) |
| | PG local | 1,367 ms | ~1,355 ms | ~186 ms | ~142 ms |
| | PG +10 ms | 27.0 s | ~26.8 s | ~2.8 s | ~1.9 s |
| | SQLite stmts / ms | 2,284 / 65 ms | ~2,270 / ~65 ms | with I2b ~330 / ~55 ms | same |
| 10k files, 100 dirs | PG trips | 10,955 | **~606** | ~606 | ~210 |
| | PG local | 13.4 s | ~1.2 s | ~1.2 s | ~0.74 s |
| | PG +10 ms | ≈141 s (proj.) | ~8.3 s | ~8.3 s | ~3.2 s |
| | SQLite stmts / ms | 12,164 / 613 ms | **~2,000** / ~490 ms | same | same |
| 10k files, one dir | PG trips | 31,039 | ~20,700 | **~706** | ~240 |
| | PG local | 24.0 s | ~16.1 s | ~1.0 s | ~0.68 s |
| | PG +10 ms | ≈386 s (proj.) | ~257 s | ~9.3 s | ~3.5 s |
| | SQLite stmts / ms | 31,881 / 717 ms | ~21,700 / ~595 ms | with I2b ~2,330 / ~500 ms | same |
| 100k files, 1,000 dirs (proj. only) | PG trips | ~110,000 | ~6,100 | ~6,100 | ~2,100 |
| | PG +10 ms | ~22 min | ~80 s | ~80 s | ~30 s |

How the after-values are derived:
- **I1** removes the logical-size walk: the locates plus about 205 pooled-value reads at 10k, and about 21 leaf reads at 1k. 10,955 − 10,349 ≈ 606.
- **I2** turns about 2 × (regular children) statements into one per placement window: 10k flat has about 3 windows, so 31,039 − 20,000 + 3 ≈ 11,042, then −10,336 with I1 ≈ 706.
- **I3** savings are inferred from the unit counts (about 120–144 transactions). Confirm them with per-phase counters before committing to the target.
- At 100k the windowed terms still grow with the entry count, about 4 trips per 512 entries. That growth is the next lever, an owner decision.

## Phases

**I1. Logical size from data Init already holds (both backends).**

- **Change:**
  - Delete the walk at `init.rs:176-181`.
  - Each worker in `save_files` (`engine/crates/layerfs-project/src/import/files.rs:48-142`) adds `job.stamp.len` for every canonical file it constructs. Use a per-worker sum, merged once at the end.
  - `check_aliases` (`files.rs:163-190`) adds the canonical's `stamp.len` for each later path. It already holds the canonical's stamp (`first`).
  - `init.rs` passes the total where `logical` was used (`:208`).
- **Why this is exact:**
  - `construct_file` refuses unless `built.logical_len == job.stamp.len` and the stamp still matches (`files.rs:216-221`).
  - `logical_size` sums length × `namespace_ref_count` over regular files only (`statistics.rs:20-30`).
  - `namespace_ref_count` is 1 + aliases for a canonical file (`namespace.rs:190-208`).
  - So the sum equals the walk's result; symlinks and directories count 0 in both.
- **Choice:** sum in `files.rs`, not by adding `length` to `FileRoot` through `substring(evidence,1,8)`.
  - That alternative would change the storage port row (`core/crates/layerfs-storage/src/port/acquisition/rows.rs:155`) and both backends' `FILE_ROOTS` reads (`.../postgresql/acquisition/reads.rs:32`).
  - It would also put Init's evidence layout (`backing.rs:53-66`: length in bytes 0–8) into Persistence.
  - The `files.rs` sum touches one crate and no port.
  - Fallback, if a Store-side sum is required: call batched `file_lengths` (`core/crates/layerfs-storage/src/read/provider.rs:49`) per inode-leaf window, about 10 trips plus the leaf walk (about 350) at 10k.
- **Depends on:** nothing.
- **Trips saved:** about 10,350 at 10k (nested and flat) and about 20–25 at 1k.
- **SQLite:** intentionally changes. It stops reading the 10k roots, about 12,164 → about 2,000 statements. The SQLite pins in `engine/crates/layerfs-project/tests/init_counters.rs` and `init_sqlite.rs` must be updated in the same commit (owner decision below).
- **Memory:** one `u128` per worker. Nothing is read.
- **Invariants:**
  - Stored objects, roots and Save counts are byte-identical.
  - The recorded `logical` is identical.
  - `Changed` and `Deadline` refusals are unchanged.
- **Tests:**
  - On both backends, Init's `logical` must equal `statistics::logical_size(reader, root)`. Shapes: hardlinks inside the root (aliases), empty files, chunked files above the whole-file bound, symlinks, empty directories. Extend `core/crates/layerfs-storage/tests/logical_size_lengths.rs` and `engine/crates/layerfs-project/tests/init_postgresql.rs`.
  - Keep `statistics::logical_size` as the oracle: tests use it and it has no other production caller.
- **Exit:** 10k nested is at or below 650 trips, cold; byte identity holds on both backends; the oracle test passes.

**I2. One placement statement per window (I2a PostgreSQL; I2b SQLite, owner decision 5).**

- **Change:** `place_children` (`writes.rs:439-478`) sends one statement per window instead of the per-child loop:
  - a `WITH s AS (unnest(slot,name,position,device,inode,evidence,path))`;
  - `b` = `BIND_HOST_FILE` over the rows of `s` that have a host, `RETURNING device,inode,canonical_position`;
  - `p` = `PLACE_ENTRY` over `s LEFT JOIN b`, `WHERE position IS NULL … RETURNING slot,kind,canonical_position`;
  - a final `SELECT … ORDER BY slot`.
- **The same checks, client side:**
  - The first slot with no `b` row (a host child), no `p` row, or `regular != host.is_some()` gives `Failure::Changed(that position)`. That is the same child the loop stops at, and the unit rolls back as today.
  - "created" (canonical = own position) drives the same `charge` rows and bytes (`:455-459`, `charge` `:96`).
  - The canonical list stays in input order for regular children (`Bound::count`, `backing.rs:240`).
- **Hardlinks:** PostgreSQL refuses `ON CONFLICT DO UPDATE` touching one row twice in one statement (SQLSTATE 21000).
  - Cut the window before a repeated (device, inode) inside it, as `independent` (`writes.rs:219`) does for entry windows.
  - The next statement then sees the first path's row, which is today's sequential result.
  - An identity first seen in an earlier window returns its stored canonical via `DO UPDATE … RETURNING`, as now.
- **Depends on:** nothing; it can land before or after I1.
- **Trips saved:** 2 × (regular children) − (windows). About 19,997 at 10k flat and about 1,999 at 1k flat.
- **SQLite (I2b):** SQLite runs the same per-row loop (`sqlite/acquisition/writes.rs:169-206`), in-process. Its cost is bounded by flat vs nested at baseline: about 10 ms at 1k (65 vs 55 ms) and about 100 ms at 10k (717 vs 613 ms), about 5 µs per statement in the test profile. SQLite has no `unnest`; I2b uses the multi-row window pattern SQLite already uses for entries and roots (`sqlite/acquisition/entry_windows.rs`, `root_windows.rs`, `INPUT_ROWS`), at its existing 32-row width, with the same hardlink window cut and the same client-side checks. It turns about 20,000 statements into about 630 at 10k flat (with I1: 31,881 → about 2,330). The main reason to take it is that both backends then keep one placement algorithm instead of a batched one and a per-row one.
- **Bound:** one statement per existing write window, at most 4,096 rows or 1 MiB (`provider.rs:123-137`, `bounded`). That is no larger than `PUT_ENTRIES` today, so the connection's buffer high-water does not rise (#822).
- **Invariants:** custody (one unit = one transaction), `Stale` before any write is kept, identical charges, identical `aliases` counts, canonical order.
- **Tests:**
  - Equivalence: the same `init_entry`/`init_host_file` rows and the same Init roots as the per-row path. Shapes: hardlinks within and across windows, an evidence mismatch on a later path (`Changed`), mixed file/dir/symlink children, a 4,096-row window boundary.
  - Drop and kill sweeps on the new statement (pattern: `engine/crates/layerfs-persistence/tests/postgresql_acquisition.rs`, `acquisition_batching.rs`).
- **Exit:** 10k flat is at or below 1,200 trips with I1; equivalence and sweeps pass.

**I3. Fewer waits per acquisition unit and publication (required for the ≤ 250 target; c and d by owner decision).**

- **Change:**
  - **(a) Read windows (jobs, entries, bindings, file roots, directories):** make each one fenced autocommit statement, using the existing statement mode (`read_statement`), with the owner check joined in as a sentinel row so a stale owner is still refused. Fold the two-statement sizing reads (`.../acquisition/reads.rs`) into one with a running sum.
  - **(b) Write units:** send them as one message from BEGIN to COMMIT, keeping `put_entries`' dependency check as its one extra wait.
  - **(c) Init's Saves:** let them use `publish_flight` (eligibility: at most 4 MiB, generation set, no lane held; `.../postgresql/objects/publish_flight.rs:20-29`) through a flag of their own. Do not use `captured_output` (`core/crates/layerfs-storage/src/save/publication.rs:286-293`): it also keeps payloads (`:351`).
  - **(d) Optional:** prepare Init's statements in one message at open. The 47 prepares are 20% of the 1k nested trips.
- **Depends on:** I1 and I2, so the remaining terms are measured first.
- **Trips saved:** about 395 at 10k nested and about 470 at 10k flat (inferred).
- **SQLite:** unchanged.
- **Invariants:** the owner fence, the lane, acknowledgement order and failure custody of the ordinary path (rules-to-preserve); one attempt; a lost reply quarantines.
- **Tests:** drop and kill sweeps at every message of the new flights; identical rows; per-phase trip pins.
- **Exit:** 10k nested ≤ 250 and 10k flat ≤ 250 trips, cold; time at +10 ms ≤ trips × 11.65 ms + SQLite time.

## Layout

Scope: commit `056e920166`, `tools/production_loc.py --files`, product files the Init track touches only (partial listings). Tests live outside `src/`:
- `engine/crates/layerfs-project/tests/` (`init_postgresql.rs`, `init_sqlite.rs`, `init_counters.rs`);
- `engine/crates/layerfs-persistence/tests/` (`postgresql_acquisition.rs`, `acquisition_batching.rs`);
- `core/crates/layerfs-storage/tests/logical_size_lengths.rs`.

`layerfs-project` (`engine/crates/layerfs-project/src`):

```diff
  import/                              [554]  (partial: 2 of 13 files)
    files.rs                             203  (I1: sum canonical and alias lengths)
    init.rs                              351  (I1: remove the logical_size walk, :176-181)
```

`layerfs-persistence` (`engine/crates/layerfs-persistence/src`):

```diff
  backend/                            [1120]  (partial)
    postgresql/                       [1120]  (partial)
      acquisition/                     [979]  (partial: 3 of 5 files)
        provider.rs                      212  (I3a/b: statement-mode reads, one-flight units)
        reads.rs                         268  (I3a: sizing folded into window reads)
        writes.rs                        499  (I2: one placement statement per window)
+       placement.rs                       —  cond. (I2 statement and reply decoding, if writes.rs would pass 999 or mix duties)
      objects/                         [141]  (partial)
        publish_flight.rs                141  (I3c: Init Saves eligible behind their own flag)
```

`layerfs-storage` (`core/crates/layerfs-storage/src`):

```diff
  save/                                [366]  (partial)
    publication.rs                       366  (I3c: flight flag apart from captured_output)
```

`layerfs-content` is unchanged: `statistics.rs` (265) keeps `logical_size` as the test oracle. The SQLite acquisition files change only for I2b (`sqlite/acquisition/writes.rs`, `statements.rs`, `sql/sqlite/acquisition/queries/place_children.sql`).

## Owner decisions

1. **SQLite statement counts drop with I1** (12,164 → about 2,000 at 10k nested) because SQLite also stops re-reading roots. Accept this with updated SQLite pins, or gate I1 to PostgreSQL (not recommended: it is the same less-work path with the same result).
2. **`READ_WINDOW_ROWS` (512) is the next lever after I3**, because windowed terms still grow about 4 trips per 512 entries (100k ≈ 2,100 trips, about 30 s at +10 ms). It is a contract constant.
3. **I3c gives Init's Saves the flight path**, which changes Init's publication shape on PostgreSQL. Take it only with sweeps.
4. **I3d (preparing at open)** is in scope only if one-shot SDK processes run Init.
5. **I2b (SQLite multi-row placement). Owner decided 2026-11-27: take I2a and I2b.** Saves at most about 100 ms at 10k flat (test profile) and about 20,000 statements. Recommendation: take it with I2a so both backends keep one placement algorithm; it costs a second SQL text and its own equivalence and fault tests.

## Corrections from the code review (2026-11-27)

These override the text above where they differ.

- **I2a/I2b window rule.** Mirror `independent` (`backend/postgresql/acquisition/writes.rs:215-246`) in full, not just the hardlink cut. Cut the window before:
  - a repeated name (a PostgreSQL `UPDATE` matches a row once, so a duplicate would be absorbed where the loop answers `Changed`);
  - a repeated position (`init_host_file` PK and `init_entry_directory`, `sql/postgresql/acquisition.sql:29,39`);
  - a repeated (device, inode);
  - a position that does not convert to signed. That must end the window, as `record_roots` does, so it is not reported ahead of an earlier `Changed`.

  Dependencies on rows stored by earlier windows get a `first_dependent`-style check (SPEC D17), so a later row's constraint error cannot replace an earlier `Changed`. An all-hardlink directory degrades to 1-row windows: correct, and the stated bound.
- **I2b SQL.** SQLite's `UPDATE … FROM` cannot return columns of the FROM table, so match the returned rows by name or position. `bind_host_file.sql` and `place_entry.sql` stay: `entry_windows.rs:5,68` uses `BIND_HOST_FILE`. I2b adds `place_children.sql`.
- **I3c** needs core `save/state.rs:164` (flag field) and `save/operation.rs:61` (setter), both lane I. It reuses the existing `publish_captured` entry if that keeps no payloads. Otherwise it waits for lane W's merge of PG `storage/provider.rs:378`.
- **I3 status.** I3 is required for the ≤ 250 target. Its savings are inferred, so M1's per-phase counts confirm or revise the target.
- The 999 ceiling counts physical lines; PG acquisition `writes.rs` is 499 production and 552 physical.
