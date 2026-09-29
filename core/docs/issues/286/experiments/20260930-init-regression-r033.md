# #286 round 20260930-init-regression-r033

> **Status: BUILD FAIL; no family-1 sample.** The required one-time Init
> regression check after r031/r032 history gates did not start a timed case.
> Family 2's three candidate PASS receipts remain intact, but its checkpoint
> still awaits this earlier-family check.

At clean source `cdf146ee9`, command
`LAYERFS_CONSTRUCTION_WORKERS=1 python3 core/benchmark/fs-bench-pro/runner.py run --family init_namespace --out benchmark-results/fs-bench-pro/issue286-init-regression-r033`
allocated a fresh append-only output. The runner requested the locked release
SDK driver and separate verifier from `core/target/release/examples/`. Its
worktree-local build exited101 in **2,997,682,084 ns** after compiling three
units, below the unchanged30 s build budget. Rust reported `E0004`:
`core/crates/layerfs-server/src/service/error.rs:22` matches `StorageError`
without the newly added `Io(_)` case. The C2 physical reservation introduced
that typed I/O failure; its service boundary has no mapping yet. This is a
source defect, not an Init timing or cache result.

| Registered Init case | Attempted sample | Result |
| --- | ---: | --- |
| `namespace-100-compact-v3` | 0 | NOT_RUN: build failed |
| `namespace-1000-compact-v3` | 0 | NOT_RUN: build failed |
| `namespace-10000` | 0 | NOT_RUN: explicit tier not selected |
| `namespace-100000` | 0 | NOT_RUN: explicit tier not selected |

There is no SDK duration, independent verifier, Store/history footprint,
cache eligibility observation or cleanup receipt for a timed child: those
fields are unavailable, never zero or PASS. The [raw build log, exact
receipt/report and SHA index](20260930-init-regression-r033/evidence-index.json)
are preserved. The next concrete fix is one source change in the shared
service error mapper: map `StorageError::Io(_)` to the existing wire `Code::Io`,
which already handles other typed file I/O errors. Then rebuild under
`--locked` and run a new family-1 selection once at the changed source; no
unchanged arm is resampled. The previous r001 Init functional receipt retains
its original source/profile/status. Families3–7 remain NOT_RUN; #285 stays
draft and #286 open.

This report-only commit changes no production code: reference65,417→65,417,
Core70,213→70,213, combined135,630→135,630 (delta+0), counted with
`tools/production_loc.py --json --root <snapshot>` on exact first-parent and
staged/committed Git archives, including runtime SQL and excluding tests,
examples, harness and docs (counter SHA-256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`).
