# PROGRESS — P10 per-call results (area-p/p10, commit 08682b085)

Environment: Docker container `layerfs-build` (node 24.21.0, cargo 1.85.1,
python 3.13.5, gcc 14.2.0), repo at /work/layerfs on ext4. All test commands
under explicit `timeout 120`. No mounted/real-E2E tests in this slice:
P10 is client-side JS only; no Store/daemon/FUSE surface touched.
Durable execution NOT_RUN (owner policy).

- area-p10-results: PASS 11 scenarios (new; also FAIL on old code for the
  intended reason `lf.results is not a function`, verified by stashing the
  src diff).
- area-p9-events: PASS 13 scenarios.
- area-p: PASS 241 cases + 2 P7 validation groups + 1 P8 validation group
  + 1 budget traversal.
- calls: PASS 66 cases.
- trace on/off/failure/layerfs-env/start/ids/no-content/store/daemon-host:
  PASS all 9 cases.
- cli.test: PASS 106/106.
- area_p1_history: PASS 8/0.
- area_p14_paging: PASS 8/0.
- area_p7_attribution: PASS 11/0.
- sdk_method_golden (umask 022): PASS 1/0.
- control_records: PASS 4/0.
- observed_records: PASS 7/0.
- cargo +1.85.1 fmt --all -- --check: clean.
- cargo +1.85.1 clippy --locked --workspace --exclude layerfs-node
  --all-targets -- -D warnings: clean (57.4 s).
- check_product_boundary: PASS 968 files.
- tool self-tests: PASS 155/0 (after installing procps in the container;
  test_host_timeout_cancels_owned_process_group needs `ps`).

Windows-host note (not a product failure): trace on/store/daemon-host fail
on Windows node with ERR_UNSUPPORTED_ESM_URL_SCHEME (`await import(FAKE)`
takes a Windows absolute path); pre-existing, they pass on Linux above.
