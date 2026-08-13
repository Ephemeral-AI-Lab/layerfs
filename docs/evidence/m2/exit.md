### Milestone 2 exit

- Candidate commit: `fc36e2ebcaad555f8d0befce040cf792938e22bf`
- Date: 2026-08-13
- Sequential predecessor: accepted M1 candidate
  `ff8cd5a74e3b57392ff232788e6f9244cc447aaf`; this record supersedes the accepted M2
  improvements candidate `d01651b4ba3a5b9f2e5d02ab48d3d1b519396922` with the CI/evidence
  repair candidate
- Checklist complete: yes; this record accepts M0, M1, and M2 only
- Primary environment: macOS `14.4.1`, arm64, Node `26.5.0`, pnpm `10.32.1`
- Primary commands: `pnpm validate:m2:pre-evidence` (fixtures, docs, style,
  architecture, build, exports, M1 algorithms, workerd parity, M2 storage suite) and
  `pnpm check:evidence`
- Primary result: pass; 105 M2 tests (92 storage/node-integration plus 13 maintenance)
  in 40,662 ms, 0 failed; the complete local pre-evidence chain exited 0 in 87 seconds
- Correctness artifact: [`correctness.json`](./correctness.json)
- Pre-evidence receipt: candidate `fc36e2e`; log SHA-256
  `c05233ae6f987909f89eee0f6526c017e8ebbe422f7325208e7c3a547d7ba967`; 25,414 bytes;
  2026-08-13T10:22:55Z through 2026-08-13T10:24:22Z
- Benchmark artifact: the `tests/performance/mini-bench.mjs` matrix (cells A1-A7, B1-B5,
  C1-C3) with raw artifacts under `tests/performance/artifacts/`, `artifacts-r3/`, and
  `artifacts-baseline/`; see
  [`docs/benchmarks/m2-minibench.md`](../../benchmarks/m2-minibench.md)
- Smoke duration and operation counts: 40,662 ms for the M2 suite; the sealed
  100,001-entry closure reconciled with 1,744 statements (0.017439825601743984 per
  manifest entry), reached 7 unique closure members, and final-validated with one
  statement
- Resource high-water: 128 MiB managed-resident default, 64 MiB byte-weighted cache, 16
  MiB final-transaction ceiling, and a fixed 524,288-byte FastCDC buffer; observed
  streamed-managed peak 12,373,056 bytes and fallback-managed peak 12,759,060 bytes on
  the 100 MiB fixtures
- Known deviations:
  - No hosted GitHub Actions run exists yet for candidate `fc36e2e` because the branch
    has not been pushed; only the actually executed macOS 14.4.1 arm64 / Node 26.5.0 /
    pnpm 10.32.1 cell is claimed.
  - Independent audit approval remains the approval recorded for accepted M2 baseline
    candidate `2e06a44`. Candidate `fc36e2e` re-ran the complete local pre-evidence
    chain (6 architecture tests, 40 M1/workerd tests, and all 105 M2 tests) but this
    record does not claim a new independent audit of that candidate.
  - Candidate `fc36e2e` contains only the mini-benchmark JSON formatting repair, CI
    full- history checkout, and accepted-milestone checker control-flow repair beyond
    the existing accepted M2 implementation. It does not change an M2 storage,
    boundedness, quota, parity, or benchmark acceptance contract.
  - Reopen-after-fault remains demonstrated per statement for migrations; content,
    expiry, and cleanup fault tests verify complete rollback on the same connection
    without a reopen leg. The testkit fault-controller capability is declared but not
    implemented; both are evidence gaps, not observed defects.
  - The Node adapter injects `node:crypto` SHA-256 through the operations storage port;
    workerd and adapters without the capability fall back to the byte-identical pure-JS
    implementation. No node-only module enters `packages/fs/src`.
  - The mini-bench A6 cell remains the documented M3 target: default leaves exceed the
    bounded path-copy window, so scattered edits use the O(file) streamed fallback and
    the harness caps the loop at 8 seconds.
- Independent audit: the M2 baseline candidate `2e06a44` was independently approved (all
  25 M2 checklist items and all ten acceptance criteria, `efs_usage` exactness,
  quota-race serialization, sealed-closure constant-row validation, WAL backpressure
  observability, and deterministic restart-safe migrations). The current candidate
  `fc36e2e` re-ran the complete local gate chain and refreshed this record with its
  measured metrics; no new independent audit of `fc36e2e` is claimed.
- Approved to begin next milestone: yes
