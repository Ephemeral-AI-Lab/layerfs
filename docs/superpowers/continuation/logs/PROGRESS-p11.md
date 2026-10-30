# PROGRESS — P11 store statistics (area-p/p11, base bc3036162)

Environment: WSL2 Ubuntu on E: (cargo 1.85.1, node 22.22.1, python 3.12),
repo at /mnt/e/layerfs-connect/layerfs. All test commands under explicit
`timeout 120`. No mounted/real-E2E tests in this slice: /dev/fuse and Docker
do not exist here. Disposable Store fixtures only; Durable NOT_RUN (owner
policy).

- area_p11_statistics: PASS 3/0 (new; all 3 FAIL on old code for the intended
  reason, verified: `disk` absent, `commits` absent, refusals accepted as
  unknown option/ignored args).
- area_p1_history: PASS 8/0.
- area_p14_paging: PASS 8/0.
- area_p7_attribution: PASS 11/0.
- sdk_method_golden (umask 022): PASS 1/0 (get_storage reply shape unchanged
  for existing golden callers: new `disk` is additive).
- layerfs-bridge control_records: PASS 4/0.
- layerfs-bridge observed_records: PASS 7/0.
- layerfs-bridge other suites (commands/replies/duplex/terminal): PASS 25/0.
- JS area-p11-storage: PASS 5/5 scenarios (new).
- JS area-p: PASS 241 cases + 2 P7 validation groups + 1 P8 validation group
  + 1 budget traversal.
- JS area-p10-results: PASS 11 scenarios.
- JS calls: PASS 66 cases.
- cargo +1.85.1 fmt --all -- --check: clean.
- cargo +1.85.1 clippy --locked --workspace --exclude layerfs-node
  --all-targets -- -D warnings: clean.
- check_product_boundary: PASS 969 files.
- tool self-tests: PASS 155/0.
