# #818 final check (tip `3a15eba8f4`)

> **Status:** Verification receipt at the final identity, one run per binary; the probe at SQLite and PG local once per group. Load 7.3. No +10 ms run (trips at +10 ms equal PG local in every earlier run; latency is environment-bound, see `receipts/M3/latency-diag/`).

Includes the M3 review fixes (lanes RH `645d0aedea`, I `857431518d`, W `ffd5a9a426`) and lane S3 `8c1b6b7466` (deleted pack row fails only its objects).

**Tests** (`run_bounded.py --limit 120`, PG `lfs818-rt-pg` db `rt818_final`), all exit 0:
per_object_reads 10, postgresql_locate_each 1, postgresql_session_unit 3,
postgresql_acquisition_units 5, postgresql_acquisition_reads 5,
postgresql_statement_schema 2, visibility_guard 4, sdk_read_round_trips 2,
sdk_history_round_trips 2, sdk_write_round_trips 2, sdk_history_versions_rh 5
(the missing-pack test no longer ignored), sdk_cached_each_rh 1,
visibility_units 2, sdk_round_trip_equivalence_seeded 2, method_refusal_order 2,
sdk_history_fault_rh_drop 1, sdk_history_fault_rh_kill 1, init_round_trips 2,
init_round_trips_10k 4. Logs: `<test>.log`.

**Probe** (`probe-<arm>-<group>.log`, binary `fs_call_round_trips-9eab8e660a198019`, PG `lfs818-bench-pg`): every `fs.*` count equals M3 on both arms except PG `grep.tree_1k` 63 → **35** trips (R6; SQLite 113 unchanged, R6 is PostgreSQL only). Times vary with load at equal counts.
