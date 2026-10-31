# P12 progress log

- RED area_p12_summary (old production code): 0 passed, 4 failed; all four reached `backend.store.project.get_summary` and failed `UnknownMethod` as intended.
- RED area-p12-summary (old production code): 0/3 scenarios passed; `backend.store.project.get_summary is not a function` as intended.
- EXPLAIN QUERY PLAN summary_main_branch: `SEARCH s USING COVERING INDEX layer_stack_names (name=?)`; `SEARCH b USING INDEX branch_names (layer_stack_id=? AND name=?) LEFT-JOIN`.
- EXPLAIN QUERY PLAN summary_branch_count: `SEARCH branch USING COVERING INDEX branch_names (layer_stack_id=?)` — index-only count, no table scan.
- EXPLAIN QUERY PLAN summary_commit_count: `SEARCH commit USING COVERING INDEX commit_identity (layer_stack_id=?)` — index-only count, no table scan.
- EXPLAIN QUERY PLAN summary_main_commit: `SEARCH commit USING INDEX commit_identity (layer_stack_id=? AND commit_id=?)` — point lookup.
- EXPLAIN QUERY PLAN statistics_totals: `SEARCH store_totals USING INTEGER PRIMARY KEY (rowid=?)` plus three one-row pragma virtual-table reads; no object/history catalog scan.
- area-p12-summary after implementation: PASS 3/3 scenarios, 0 failed.
- area_p12_summary first implementation run: 2 passed, 2 failed; independent-logical test used the wrong public route (corrected to operator dispatch), and Fixture::work measured 7 statements/5 rows at both sizes because the five queries have a real BEGIN/COMMIT snapshot; contract ruling requested.
- area_p12_summary independent-values rerun: 1 passed, 0 failed, 3 filtered out.
