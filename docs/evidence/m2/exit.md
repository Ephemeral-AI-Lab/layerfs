### Milestone 2 exit

- Candidate commit: `d01651b4ba3a5b9f2e5d02ab48d3d1b519396922`
- Date: 2026-08-11
- Sequential predecessor: accepted M1 candidate
  `ff8cd5a74e3b57392ff232788e6f9244cc447aaf`; this record supersedes the accepted M2
  baseline candidate `2e06a446aa5781102d9c028c62519582ee3b1519` with the M2 improvements
  candidate
- Checklist complete: yes; this record accepts M0, M1, and M2 only
- Primary environment: Microsoft Windows NT `10.0.26200.0`, x64, Node `24.11.1`, pnpm
  `10.32.1`
- Primary commands: `pnpm validate:m2:pre-evidence` (fixtures, docs, style,
  architecture, build, exports, M1 algorithms, workerd parity, M2 storage suite) and
  `pnpm check:evidence`
- Primary result: pass; 99 Node storage tests (86 storage/node-integration plus 13
  maintenance) in 70,919 ms total, 0 failed
- Correctness artifact: [`correctness.json`](./correctness.json)
- Benchmark artifact: the `tests/performance/mini-bench.mjs` matrix (cells A1-A7, B1-B5,
  C1-C3) with raw artifacts under `tests/performance/artifacts/`, `artifacts-r3/`, and
  `artifacts-baseline/`; see
  [`docs/benchmarks/m2-minibench.md`](../../benchmarks/m2-minibench.md)
- Smoke duration and operation counts: 70,919 ms for the M2 suite; the sealed
  100,001-entry closure reconciled with 4,655 statements (0.0465 per manifest entry,
  down from 0.0507), reached 7 unique closure members, and final-validated with one
  statement
- Resource high-water: 128 MiB managed-resident default, 64 MiB byte-weighted cache, 16
  MiB final-transaction ceiling, and a fixed 524,288-byte FastCDC buffer; observed
  streamed-managed peak 12,373,056 bytes and fallback-managed peak 11,182,080 bytes on
  the 100 MiB fixtures
- Known deviations:
  - No hosted GitHub Actions run exists because the branch has not been pushed; only the
    actually executed Windows x64 / Node 24.11.1 cell is claimed.
  - Reopen-after-fault is demonstrated per statement for migrations; content, expiry,
    and cleanup fault tests verify complete rollback on the same connection without a
    reopen leg. The testkit fault-controller capability is declared but not yet
    implemented; both are evidence gaps, not observed defects.
  - R3 hashing seam: the Node adapter injects `node:crypto` SHA-256 through the
    operations storage port; workerd and adapters without the capability fall back to
    the byte-identical pure-JS implementation. No node-only module enters
    `packages/fs/src`, and M1 golden vectors plus the 11 workerd parity checks pass
    unchanged.
  - R5 statement batching: per-chunk staging inserts became multi-row `VALUES` inserts
    and reconciliation leaf edges use one `hash IN (...)` lookup plus one multi-row
    queue insert per leaf; write-path statements measured ~4x fewer on the mini-bench
    (A1: 12,472 -> 3,065). No statement/row/byte budget contract changed.
  - The mini-bench A6 cell (1,000 scattered one-byte edits on the 100 MiB file) records
    `pass: false` by design: default leaves exceed the bounded path-copy window, so the
    edits use the O(file) streamed fallback and the harness caps the loop at 8 s
    (`completedEdits`/`scaledEdits` in the artifact). R1 targets sub-10 ms path-copy
    edits in M3.
- Independent audit: the M2 baseline candidate `2e06a44` was independently approved (all
  25 M2 checklist items and all ten acceptance criteria, `efs_usage` exactness,
  quota-race serialization, sealed-closure constant-row validation, WAL backpressure
  observability, and deterministic restart-safe migrations). The M2 improvements
  candidate `d01651b` re-ran the full gate chain and refreshed this record with its
  measured metrics; no acceptance criterion changed semantics.
- Approved to begin next milestone: yes
