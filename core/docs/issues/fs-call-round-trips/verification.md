# fs call round trips: Verification

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract. Part of [SPEC.md](SPEC.md). Source pinned at `056e920166`; all after-values are estimates until measured. The corrections at the end override earlier text.

Every phase (R, H, W, I) is judged by counted round trips first and time second.
A phase is done only when its counts, its bytes, its answers and its memory are
shown on both backends, under a named cache state, with every new message swept
for faults. These rules apply the repository's measurement workflow
(`docs/general/agent-measurement-policy.md`, AGENTS §1–§4) and the lessons
`store-access/round-trips-follow-depth.md` ("How to verify a change"),
`store-access/rules-to-preserve.md`, `measurement/count-round-trips-above-every-threshold.md`,
`measurement/state-the-cache-behind-a-cap.md` and `testing/one-database-one-test-binary.md`.

## Principles

1. **Count trips by role, with the cache state.** Read `SqlWork` for the writer
   (with its lane connection), the control reader (session 0, which serves every
   SDK call) and the Workspace readers apart, as `mount_round_trips.rs` does
   (`Counts = [connects, round_trips, statements, transactions]`). Write calls
   bind a private Workspace, so their reader trips are counted too. Every cap
   carries the block from `state-the-cache-behind-a-cap.md`: cap, cache state and
   how it was enforced (new Store open per case = cold; repeat call in one open
   Store = warm), harness and source identity, roles, backend and delay, control
   arm, sample count and 1-minute load. A warm count never stands in for a cold
   cap (AGENTS §1).
2. **Cross-check on the server.** Take `pg_stat_statements` deltas per query id,
   counting `toplevel` calls only (or run the server with
   `pg_stat_statements.track=top`). With `track=all`, nested foreign-key trigger
   checks inflate totals (~30k on Init 10k at baseline); they are server work,
   not trips. The in-repo `Server::snapshot` helpers do not filter `toplevel`;
   the new pins must.
3. **Latency arm.** Add delay with `tc netem` inside the PostgreSQL container's
   network namespace only (never the host network; owner rule). Calibrate one
   trip before and during the arm (median of 200 `SELECT 1`). Pass condition:
   added time ≤ trips × measured delay (baseline 11.65 ms per trip at +10 ms,
   range 10.95–12.63). A larger gap means the counter misses something and
   blocks the phase.
4. **Byte identity.** For every Commit, Init and write call: same root, same
   Save `stored_bytes`, `inserted`, `reused`, `packs`, same logical totals, on
   both backends, against the control arm at `056e920166`. Init's `logical`
   must equal `logical_size(reader, root)` (hardlinks, empty, chunked files,
   symlinks counted 0).
5. **Same answers, same refusal order.** Read results, blame owners,
   `coverage` and `comparisonCoarse`, revision pages and continuation tokens,
   diff pages, list/glob `removed_count` must be unchanged.
   `tests/method_refusal_order.rs` stays green unchanged. Any order change the
   design admits (for example a lost reply now surfacing before an invalid
   `path`) is named in the phase and approved, never absorbed silently.
6. **Seeded SQLite–PostgreSQL equivalence.** One seeded driver runs the same
   call sequence (all 15 `fs.*` methods plus Init, including refusals) on both
   backends and compares answers and roots. Pattern: `commit_fused_sizes_seeded.rs`.
7. **Fault sweeps for every new or changed message.** Each new statement,
   pipelined message or fenced flight gets drop and kill sweeps at every server
   message (`support/fault_sweep.rs`, `support/fault_proxy.rs`, as in
   `commit_fault_*`). Expected: one attempt, no retry, no replay; a lost reply
   quarantines the session and `one_call` reopens it at most once; outcome is
   either the old state or the new state, never a partial one; custody of an
   unknown write outcome is kept as today.
8. **Memory.** For every changed case: peak memory (`/usr/bin/time -l` "peak
   memory footprint", Linux `VmHWM` where available) and RSS after the call,
   before and after the phase. A 1,000-call soak in one SDK process (mixed reads
   and writes on one tree) must keep RSS flat after warm-up. On the large shapes
   (Init 10k/100k, blame over 1,000 versions, grep over 10k files) peak memory
   must not grow with input size beyond the named window bounds. Any cross-call
   cache reports its charged bytes and hit/miss counts and stays within its
   named budget. A gain bought with memory that grows with Store, tree or
   history size is rejected or taken to the owner.
9. **Anti-hack checks.**
   - Each phase names, before code, the trips it removes (which wait, which
     duplicate read) and predicts the new counter per case; the commit records
     prediction and result.
   - Gains must hold on held-out shapes the design did not use: deep-narrow
     (depth 12, one entry per level), wide-flat (10k entries in one dir),
     100k files, large files (64 MiB, chunked), hidden paths present
     (`path_removals` markers non-empty), symlinks (including an intermediate
     symlink), hardlinks (including a repeated (device, inode) inside one
     window), long history (1,000 versions), and a delta-chained directory.
   - No tuned constants: windows and caches use existing named policy bounds;
     raising a cap, a window or a timeout to pass a gate is forbidden.
   - No weakened assertions; tests that cannot be adapted are deleted and their
     behaviour re-covered (AGENTS §4), listed in the commit message.
   - The cheap tiers must show their gain with caches off. Cache gains are a
     separate warm arm.
   - The batched path is taken only where the provider reports a round-trip
     cost (`batches_locations`, `batched_chain_reads`); SQLite statement changes
     are allowed only for dedupe fixes and I2b, and are pinned openly.
10. **Independent review.** Each phase's diff gets `/code-review` or a fresh
    subagent given the code and this spec, not the author's conclusions.

## Per-phase gate

Each R, H, W and I phase passes this checklist before the next phase starts.

```text
[ ] Prediction: trips removed per case, by role, written in the phase notes.
[ ] Build first:  cargo +1.85.1 test --locked --offline -j 6 -p <pkg> --test <t> --no-run
[ ] Counts:       one bounded run per test binary (≤120 s wall, killed at expiry;
                  macOS has no `timeout`: use a process-group runner),
                  LAYERFS_TEST_POSTGRES_URL="host=127.0.0.1 port=<p> user=postgres
                  dbname=<own db> sslmode=disable", server with
                  shared_preload_libraries=pg_stat_statements, track=top.
                  One PostgreSQL test binary per database at a time.
[ ] Pins:         new counts pinned per case and role, cache state named;
                  SQLite pins unchanged unless the phase is a dedupe fix.
[ ] Bytes:        Save outcome and roots identical to the control arm.
[ ] Answers:      method_refusal_order.rs and the seeded equivalence driver green.
[ ] Faults:       drop and kill sweeps for each new or changed message.
[ ] Latency arm:  netem +10 ms in the container; added ≤ trips × calibrated delay.
[ ] Memory:       peak and after-call RSS before/after; soak flat where touched.
[ ] Held-out:     the phase's affected held-out shapes counted once.
[ ] Review:       independent review of the diff; findings fixed or recorded.
[ ] Docs:         affected docs/architecture pages updated in the same commit.
[ ] Commit message: Production LOC: <before> -> <after> (delta <±n>),
                  counted with tools/production_loc.py at parent and tree;
                  deleted tests listed; predicted vs measured counts table.
```

A red check is diagnosed from its output and the source, fixed once, and the
covering commands run once (measurement workflow §3.4). No rerun selects a
better number. Disk is checked before builds and at handoff (machine rules).

## Pinned tests

All in `engine/crates/layerfs-store-service/tests/` unless noted. Each pins
`Counts` per role and case for SQLite (statements, transactions) and PostgreSQL
(connects, round trips, statements, transactions), each scenario on a newly
opened Store (`cold`) unless named `warm`. The PostgreSQL halves report
`NOT_RUN` without `LAYERFS_TEST_POSTGRES_URL`.

| File | Pins | Notes |
| --- | --- | --- |
| `fs_call_round_trips.rs` (exists, untracked) | Measurement probe, not a gate | Kept as the benchmark harness; three arms via `FS818_ARM`, groups via `FS818_GROUP` |
| `sdk_read_round_trips.rs` (new) | `stat` `/`, depth 3, depth 7; `read` small and 2,000 lines; `list`; `glob`; `grep` one dir and 1k; cold and warm | Also pins the hidden-path case (markers present) so the skipped walk cannot hide a regression |
| `sdk_history_round_trips.rs` (new) | `blame` at 10 and 100 versions, `revision` 100, `diff_files`; formula check F + ⌈V/W⌉·k at two V | Two history lengths give the per-window slope |
| `sdk_write_round_trips.rs` (new) | `write` new/overwrite, `edit`, `mkdir`, `cp`, `mv`, `rm` file and 100-file dir, `apply` 10 | First-use vs steady state named apart (prepares, serial block) |
| `init_round_trips.rs` (new, `layerfs-project` or store-service tests) | Init 1k and 10k nested and flat, plus hardlink-in-window and 600-entry dir | Pins logical equality and Save outcome; two sizes above every threshold (4,096 locators, 512-entry wide dir, 4,096-row windows) |
| `sdk_round_trip_equivalence_seeded.rs` (new) | Answers and roots equal across backends | Seeded call sequence including refusals |
| `sdk_fault_<track>_{drop,kill}.rs` (new per changed message) | Every server message of each new statement or flight | Reuse `support/fault_sweep.rs` and `fault_proxy.rs` |
| `method_refusal_order.rs` (exists) | Unchanged | Must stay green without edits |

Pins never loosen a guard. A pin that moves is updated with the predicted
counter in the same commit and its comment names the change, as the
`mount_round_trips.rs` comments do.

## Benchmark

**Harness.** The in-tree probe `fs_call_round_trips.rs` is the benchmark during
this effort: it already drives the SDK's own route, splits groups to fit the
command limit, and records trips, statements, time and server top-level calls.
fs-bench-pro is frozen and is not used. When the effort closes, the cases move
to layerfs-benchmark as an `fs-file-calls` family (the benchmark repo's
existing report for this issue) so they run against released builds; that
move is a follow-up, not a gate.

**Matrix.**

| Axis | Values |
| --- | --- |
| Shapes | The probe tree (10 × 100 files, depth-7 file, 2,000-line file, 100-Commit history); held-out shapes from Principle 9 |
| Calls | All 15 `fs.*` methods; Init 1k/10k nested/flat; 100k nested (SQLite and PG local) |
| Arms | SQLite Normal; PostgreSQL local; PostgreSQL +2 ms; PostgreSQL +10 ms (netem in the container) |
| Cache | Cold: new Store open per case. Warm: repeat call in one open Store (only for phases that add a cross-call cache) |
| Metrics | Round trips and statements by role; server top-level calls; ms; peak and after-call RSS; 1-minute load |
| Build | Release for every Init timing row (measurement workflow §3, scoped SDK Init profile); test profile allowed for count-only rows, labelled |

**Sample policy.** One sample per case per arm; identities pinned (source
commit, binary hash, harness hash, PostgreSQL image and version, delay
calibration); fresh output path per run; failures and `NOT_RUN` rows kept.
Spread is never measured by resampling; anomalies are diagnosed from counts.
Time comparisons apply the 5 ms per call floor; counters do not.

**Command budget.** A complete command is ≤15 s, with declared exceptions up
to 25 s; tests stop at 120 s. At baseline several latency rows cannot fit:
`blame` 100 versions (31.2 s at +10 ms), `revision` 100 versions (50.9 s),
Init 1k flat (27.0 s), Init 10k at +10 ms (projected 141 s and 386 s). These
rows stay `NOT_RUN` at +10 ms with their projection (local time + trips ×
11.65 ms) and the reason; their latency arm runs at +2 ms and on smaller shapes
(`blame`/`revision` at 10 and 20 versions, Init 1k nested). After H and I land,
every row is expected to fit and is measured directly.

**Receipts and report.** Baseline receipts at `056e920166` are copied next to
this spec as `receipts/056e920166/` (`sqlite.log`, `pg.log`, `pg10.log`, `measured.json`, plus the first
probe run `first-run-pg-fs-calls.log` and `first-run-postgresql_init_*.log`). Each phase appends new
receipts under `receipts/<phase>/`; old receipts are never rewritten. Reports
use `docs/general/benchmark_agent_report.md`: the identity and custody table,
then one row per case and arm with trips, statements, ms, RSS and verdict.

**Projected vs measured.** Each phase ends with a table: case, baseline trips,
predicted trips, measured trips, predicted and measured ms per arm. A miss of
more than 10% or 3 trips (whichever is larger) is explained from the
per-role, per-phase counts before the next phase starts.

## Exit criteria for the whole effort

The effort is complete when, at one final source identity, measured once:

- **Reads**: every `fs.stat`, `read`, `list`, `glob` and single-dir `grep` is
  ≤ 4 + 5·d round trips cold (d = components below the root); `grep` over 1k
  files ≤ 64, or ≤ 30 if R6 is approved. Warm repeats (only if a cross-call
  cache is approved) ≤ 2.
- **History**: `blame` and `revision` ≤ F + d + 4 + 10 × windows(V), with
  windows of 16, 32, 64, then 128 versions (≤ 18 at V = 10 and ≤ 38 at V = 100
  for a depth-2 file); `diff_files` ≤ 15.
- **Writes**: each single-path write call at or below its SPEC §4 value
  (overwrite ≤ 12 cold, new file ≤ 40, mkdir/cp/mv/rm ≤ 26), with every
  remaining trip attributed by W0; recursive `rm` of a 100-file directory ≤ 50.
- **Init**: ≤ 250 round trips per 10k files nested or flat (≤ 750 after M1;
  the 250 target is confirmed or revised from M1's per-phase counts); logical
  size equals the Store walk.
- **Latency**: +10 ms arm added time within trips × 11.65 ms (±10%) for every
  row; every row fits the command budget.
- **Memory**: no peak growth with input size beyond named bounds; 1,000-call
  soak flat; any cache within its approved budget.
- **Bytes and answers**: identical roots and Save outcomes; equivalence and
  refusal-order tests green; fault sweeps green for every new message.
- **SQLite**: statement pins changed only by dedupe fixes and I2b, each listed.

## Corrections from the code review (2026-11-27)

These override the text above where they differ.

- **Refusal-order changes needing approval:** W1's marker-read refusal (SPEC D16). Any others a phase finds are named before merge.
- **I2a/I2b equivalence shapes:** add a repeated name inside a window, a repeated position, an unconvertible position, a dependency on an earlier window's stored row, and an all-hardlink directory. Each must match the per-row loop's first refusal and its rows exactly.
- **R5/H4 on SQLite:** pins must show SQLite statements unchanged by R5 and H4 (only the dedupe items move them).
