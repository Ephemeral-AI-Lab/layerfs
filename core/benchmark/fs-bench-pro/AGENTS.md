# v0.1.7 fs-bench-pro agent workflow

Read the repository and `core/` `AGENTS.md` first. The
[#231 first-pass spec](../../docs/benchmark/fs-bench-pro/issue-231/SPEC.md)
controls the new substrate's initial operations. This file does not relabel
legacy Stage 6 or v0.1.6 results.

## Scope and sample

[#235](https://github.com/Ephemeral-AI-Lab/layerfs/issues/235) builds the
small shared substrate. #231 first measures the 100, 1,000 and 10,000-file
native Init cases; 100,000 remains `NOT_RUN` and the original four-tier final
gate remains open. #232 and #233 follow their own frozen contracts. Do not
start remaining families until their prerequisite gates are settled.

Take exactly **one** performance sample per registered case. The first-pass
runner has one `daemon-host` route, seed `1`, no control/candidate arm,
`--perf-samples`, seed sweep, median, percentile, retry loop, sampled oracle or
best-of selection. Multiple saves or Commits inside one workload are micro
events, not more samples. Preserve every failure, `INELIGIBLE`, `INCOMPLETE`
and `NOT_RUN` row. A changed relevant identity requires a new fresh output;
never overwrite a receipt or rerun a passing cell to improve its number.

## Minimal operations

- `list` reads the registry without building or preparing.
- `run --case ID --out NEW` lazily acquires/reuses only that case's sealed
  source, builds/reuses exact product binaries, invokes the one public Init,
  captures telemetry/resources and cleans owned temporary files. It **skips
  the full verifier by default** for the fast lane, writing
  `verification.status=SKIPPED` and `status=DIAGNOSTIC` after a healthy import.
  `run --case ID --verify --out NEW` opts into the separate full verifier child.
  `run --family init_namespace` has the same default and option, calls each
  selected 100/1,000/10,000 case once and builds once.
- `verify --run DIR` re-derives retained raw/performance and verification
  receipts. It does **not** run the full oracle or turn `SKIPPED` into `PASS`.
  `report --run DIR` renders the recorded status. Neither runs the product again.

Keep the Init case declarations and public-operation/full-verifier child in
`families/init_namespace.py`; keep focused case, fixture and oracle-refusal
checks in `tests/test_init_namespace.py`. A default fast-lane row is diagnostic
only. The full benchmark proof requires an explicit `run --verify`, its
separate verifier invocation and `verification.json`, not a unit-test PASS.

Do not add separate first-pass `prepare`, `prune`, `calibrate`, `self-check`,
`build`, comparison or empty mode-adapter commands. Use focused tests for
self-checking and automatic owned-temporary cleanup. The three immutable
source masters remain reusable. No second Cargo workspace or benchmark Rust
binary is needed unless the eventual public Init API proves it necessary.

## Fast development gate

Every invoked Cargo build is **at most 30 s**, including first-use. Build only
changed production binaries with `--locked` in one worktree-owned target and
reuse an exact matching binary/image seal; Python-only edits require no Cargo
invocation. Record first-use, edited-product and unchanged build walls and
compiled units. A miss is `BUILD_SLOW` and must be fixed, not hidden with a
warm no-op, stale binary, `cargo clean`, weaker checks or a changed profile.

Each requested independent verifier child is **at most 5 s**, including reopen,
full oracle and teardown. Never sample/shrink the oracle or extend its deadline
to pass. Preparation, the three case runs, verification and cleanup are
recommended to finish within **30 s per family**; record and investigate a
miss without reducing work. Keep the existing complete-performance-command
15 s budget and prospectively declared small exceptions up to 25 s.

## Authenticity, telemetry and cleanup

This harness uses **no machine-global benchmark lock**. Give each agent a
separate worktree with private target, prepared fixtures, sample Store and
output root. Hold only that worktree's nonblocking run `flock` while its
fixture/output namespace is mutable; builds take no benchmark lock and a
run in another worktree never waits on this one. Give every run unique
ports, container names, credentials and telemetry namespaces. Refuse a
foreign `CARGO_TARGET_DIR` and an existing or escaping output path. Record
observed concurrent work on the shared host and mark an interfered row
diagnostic for an admission claim. One sample per case identity does not
mean one active run globally.

Only a real public native-directory Init can clear #231. Pre-saved roots,
the pathless bootstrap or mounted creates are different operations. Use one
caller timer for the full public operation. M1–M4 are report headings; retain
actual daemon and Service `LFT1` labels and separate clocks, never add
overlapping spans or subtract them to invent transit time. C5 stays within
Init/Commit and needs named spans before claiming a C5 duration.

Follow the [telemetry ingestion guide](../../docs/benchmark/fs-bench-pro/telemetry-ingestion-and-retention.md):
retain one exact `telemetry.lft1` file and parsed receipt per telemetry-on
case. After verified capture, parsing and hashing, remove only owned temporary
stderr/operational segments and record cleanup. Keep original stderr when
ingestion fails; preserve raw events and all nonpassing receipts. Keep process
CPU/RSS, external host/cgroup memory, source cache, Store/history/spool disk
and verification resources in separate scopes. `resource_status=sampled` does
not prove phase coverage.

Prepare a deterministic expected manifest once. Reuse it only outside the
product timer; requested full verification reads actual persisted output. A clone is
setup reuse, never a cold claim. If the source cache cannot be qualified,
report the first-pass number as discovery-only with its actual cache state.
Do not use the old 100,000-only `cold.py` to certify a smaller tier. Run the
smallest relevant focused check after an edit and the owning workspace checks
once at final identity; there is no aggregate preflight or CI claim.
