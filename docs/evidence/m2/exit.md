### Milestone 2 exit

- Candidate commit: `2e06a446aa5781102d9c028c62519582ee3b1519`
- Date: 2026-08-11
- Sequential predecessor: accepted M1 candidate
  `ff8cd5a74e3b57392ff232788e6f9244cc447aaf`
- Checklist complete: yes; this record accepts M0, M1, and M2 only
- Primary environment: Microsoft Windows NT `10.0.26200.0`, x64, Node `24.11.1`, pnpm
  `10.32.1`
- Primary commands: `pnpm validate:m2:pre-evidence` (fixtures, docs, style,
  architecture, build, exports, M1 algorithms, workerd parity, M2 storage suite) and
  `pnpm check:evidence`
- Primary result: pass; 99 Node storage tests (86 storage/node-integration plus 13
  maintenance) in 68,306 ms total, 0 failed
- Correctness artifact: [`correctness.json`](./correctness.json)
- Benchmark artifact: storage microbenchmarks are represented by the bounded
  100,001-member staged closure and the 100,000-row maintenance cases; end-to-end
  B01/B05 evidence is gated at M3/M9
- Smoke duration and operation counts: 68,306 ms for the M2 suite; the sealed
  100,001-entry closure reconciled 100,401 grouping records, reached 7 unique closure
  members, and final-validated with one statement
- Resource high-water: 128 MiB managed-resident default, 64 MiB byte-weighted cache,
  16 MiB final-transaction ceiling, and a fixed 524,288-byte FastCDC buffer;
  observed streamed-managed peak 12,373,056 bytes and fallback-managed peak
  11,182,080 bytes on the 100 MiB fixtures
- Known deviations:
  - No hosted GitHub Actions run exists because the branch has not been pushed; only
    the actually executed Windows x64 / Node 24.11.1 cell is claimed.
  - Reopen-after-fault is demonstrated per statement for migrations; content, expiry,
    and cleanup fault tests verify complete rollback on the same connection without a
    reopen leg. The testkit fault-controller capability is declared but not yet
    implemented; both are evidence gaps, not observed defects.
  - The Node driver prepared-statement cache (256-entry bound) is a performance
    change measured at roughly 14-20 percent faster suite wall time; it does not
    change the bounded-statement contract.
- Independent audit: approved after the retracted `ba64ed9` attempt; the audit
  verified all 25 M2 checklist items and all ten acceptance criteria, closed the
  prior P0 blockers, and confirmed `efs_usage` exactness, quota-race serialization,
  sealed-closure constant-row validation, WAL backpressure observability, and
  deterministic restart-safe migrations
- Approved to begin next milestone: yes
