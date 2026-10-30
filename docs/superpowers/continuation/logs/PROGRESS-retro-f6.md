PRE-FIX area_p8_reads huge-next-line regressions: 0 passed, 2 failed, 1 filtered; intended unbounded-scan failures (terminated 603916 bytes/3 reads, unterminated 603909 bytes/3 reads).
INTERMEDIATE area_p8_reads after scanner predicate: 1 passed, 2 failed; replies/nextLine correct but single-pack 600000-byte fixtures still fetched whole (603916/603909 bytes, 3 reads each); multi-pack fixtures required.
PRE-FIX FINAL area_p8_reads 4MB huge-next-line regressions: 0 passed, 2 failed, 1 filtered; intended unbounded-scan failures (terminated 4079997 bytes/17 reads, unterminated 4079990 bytes/17 reads).
FINAL area_p8_reads: 3 passed, 0 failed; huge terminated=753896 bytes/3 reads, huge unterminated=753896 bytes/3 reads (old=4079997/17 and 4079990/17).
FINAL sdk_text_scan: 1 passed, 0 failed; text Binary classification, truncation, nextLine and grep reference agreement green.
FINAL area_p1_history: 8 passed, 0 failed.
FINAL area_p14_paging: 8 passed, 0 failed.
FINAL area_p7_attribution: 11 passed, 0 failed.
FINAL sdk_method_golden (umask 022): 1 passed, 0 failed.
FINAL layerfs-bridge control_records: 4 passed, 0 failed.
FINAL layerfs-bridge observed_records: 7 passed, 0 failed.
FINAL product boundary guard: PASS, 967 production files scanned; tool self-tests: 155 passed, 0 failed.
FINAL cargo +1.85.1 fmt --all -- --check: PASS.
FINAL cargo +1.85.1 clippy --locked --offline --workspace --exclude layerfs-node --all-targets -- -D warnings: PASS, 0 warnings.
JS area-p/calls/cli.test: NOT_RUN; no JS, JS fixtures, API wire shape, or CLI files touched.
NOT RUN mounted/real-E2E: /dev/fuse and Docker unavailable. Durable execution NOT_RUN, disabled by owner; tests used existing Disposable fixtures.
