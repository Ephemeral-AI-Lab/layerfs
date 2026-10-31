PRE area_p7_attribution::tiny_commit_records_share_one_encoded_provider_page: 0 passed, 1 failed (16 provider pages; statements=110 rows=78 vm_steps=2072).
PRE area_p7_attribution::mixed_commit_records_pack_by_actual_width_and_resume_exactly_once: 0 passed, 1 failed (page widths [1,1,1,1]; statements=26 rows=18 vm_steps=476).
PRE area_p14_paging::snapshot_carries_multiple_small_commits_from_one_branch: 0 passed, 1 failed (snapshot returned 1 of 4 Commits).
PRE area_p14_paging::snapshot_mid_page_rejection_resumes_at_rejected_commit: 0 passed, 1 failed (provider fixture was not mid-page).
POST area_p7_attribution::tiny_commit_records_share_one_encoded_provider_page: 1 passed, 0 failed (1 provider page; statements=20 rows=18 vm_steps=497).
POST area_p7_attribution::maximal_commit_records_still_take_one_provider_page_each: 1 passed, 0 failed (3 one-record pages).
POST area_p7_attribution::mixed_commit_records_pack_by_actual_width_and_resume_exactly_once: 0 passed, 1 failed (actual exact packing [2,2], saved expectation [2,1,2] was inconsistent with record widths).
POST area_p14_paging::snapshot_carries_multiple_small_commits_from_one_branch: 1 passed, 0 failed.
POST area_p14_paging::snapshot_mid_page_rejection_resumes_at_rejected_commit: 0 passed, 1 failed (resume cursor skipped rejected Commit; must position at its immediate newer record).
POST area_p14_paging::snapshot_rejected_commit_yields_one_restart_continuation: 1 passed, 0 failed (first-record rejection remains Null restart).
POST area_p7_attribution::mixed_commit_records_pack_by_actual_width_and_resume_exactly_once: 1 passed, 0 failed (correct exact widths [2,2]; statements=14 rows=10 vm_steps=266).
POST area_p14_paging::snapshot_mid_page_rejection_resumes_at_rejected_commit: 1 passed, 0 failed (resume returns rejected then old, newest exactly once).
PASS area_p1_history: 4 passed, 0 failed.
FAIL area_p7_attribution: 8 passed, 3 failed (EXPLAIN diagnostic omitted its required bound parameter; production cases otherwise passed).
PASS area_p14_paging: 8 passed, 0 failed.
PASS sdk_method_golden: 1 passed, 0 failed (umask 022).
PASS layerfs-history: 0 passed, 0 failed (lib and doc targets contain no tests).
PASS layerfs-bridge control_records: 4 passed, 0 failed; observed_records: 7 passed, 0 failed.
PASS layerfs-store-service: 6 passed, 0 failed across external suites; 0 lib/doc tests.
BUILD combined History/Persistence/Store-service/Bridge --no-run: two 120 s compile timeouts, zero tests run; per-package builds subsequently completed.
PASS layerfs-persistence: 99 passed, 0 failed, 2 ignored across all package targets.
PASS area_p7_attribution: 11 passed, 0 failed; EXPLAIN QUERY PLAN = SEARCH commit USING PRIMARY KEY (commit_id=?).
PASS area_p14_paging after original-anchor resume tightening: 8 passed, 0 failed.
PASS tools/check_product_boundary.py: scanned 967 production files.
PASS tools Python unittest discovery: 155 passed, 0 failed.
PASS JS area-p: 1 harness passed, 0 failed (239 cases + 2 P7 groups + 1 P8 group + 1 budget traversal).
PASS JS calls: 1 harness passed, 0 failed (64 cases).
PASS JS cli.test: 104 passed, 0 failed.
FAIL JS packaging without selection: 0 passed, 1 failed (script requires explicit selection; rerun each selection).
FAIL cargo fmt --check: style drift in the two saved test files only; rustfmt applied.
PASS cargo +1.85.1 fmt --all -- --check.
PASS JS packaging: surface, lifecycle, options, copy, sandbox, worker-route, transport-order, dispatch each passed (8/8 selections).
PASS cargo +1.85.1 clippy --locked --offline --workspace --exclude layerfs-node --all-targets -- -D warnings.
NOT_RUN mounted/real-E2E: /dev/fuse and Docker unavailable; Durable execution remains disabled.
