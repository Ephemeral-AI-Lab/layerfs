# M5 completion handoff

Milestone 5 is complete. The normative status and checklist are in section 8 of
`implementation-plan.md`; this note records the final validation boundary for later
milestones.

## Final evidence

- `pnpm validate:m5:pre-evidence` is the default Node correctness-and-benchmark
  selection. It builds once, runs the five-trial M3 and retained M4 benchmarks without
  competing I/O, runs the 60-second smoke without competing I/O, then runs the isolated
  correctness, maintenance, fault, and workerd suites concurrently. This selects every
  mandatory predecessor and M5 check without rerunning the maintenance directory at
  multiple milestone layers; the command enforces the 600-second default-target ceiling.
- `pnpm test:m5`: 34 passed, 0 failed.
- Snapshot restart matrix: every one of 110 durable statement positions and 42 batch
  positions physically reopened and resumed.
- Collection restart matrix: every one of 157 durable statement positions and 75 batch
  positions physically reopened and resumed.
- Abandoned-run restart matrix: every one of 61 durable statement positions and 33 batch
  positions physically reopened and resumed.
- Mandatory scale fixture: 100,001 namespace rows, 100,001 reachable objects, 100,002
  manifest roots, and 100,001 manifest nodes, plus 300,003 peak durable marks in both
  snapshot and collection.
- Resource measurements: identical read/write/snapshot workloads at 10,240 and 100,000
  fixture rows measured 9,274,632 and 9,315,643 bytes of managed-memory high-water,
  respectively. The 41,011-byte difference is non-proportional and both points are below
  the asserted 16 MiB ceiling. Machine-readable exit evidence retains each run's exact
  absolute heap/RSS peaks, maximum WAL, and longest bounded maintenance call; executable
  gates require heap below 512 MiB, RSS below 768 MiB, WAL at or below the explicit 512
  MiB limit, and every bounded call below 5 seconds.
- The scale fixture includes a concurrent writer, snapshot and GC cursors, a mid-GC
  physical reopen, full object/manifest/namespace verification, exact bounded usage
  recount after reopen, an actual-fixture SHA-256 digest, WAL measurement, and
  successful checkpoint truncation.
- File-backed filesystem scenarios cover metadata-quota exhaustion, pinned-WAL
  backpressure, database-page exhaustion, exact accounting, physical reopen, and later
  maintenance progress.

The optional 10 GiB logical-manifest and millions-of-rows diagnostics were not run and
remain optional. Existing untracked performance artifacts under
`tests/performance/artifacts-m31*` belong to the user and must remain untouched.
