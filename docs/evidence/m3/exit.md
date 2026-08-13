# Milestone 3 exit

- Candidate commit: `11fc61d2ed0bab979bd8d0cd468e024f35b8bbea`
- Date: 2026-08-13
- Sequential predecessor: accepted M2 candidate
  `d01651b4ba3a5b9f2e5d02ab48d3d1b519396922`
- Checklist complete: yes
- Primary environment: Windows `win32` x64, Node `v24.11.1`, pnpm `10.32.1`, AMD Ryzen
  Threadripper 7960X, 128 GiB RAM, Samsung 980 PRO / Crucial T705 SSD, SQLite `3.50.4`,
  4 KiB pages, WAL, no mmap, and no operating-system cache drop
- Primary command: `pnpm validate:m5:pre-evidence` from the clean detached candidate
  worktree; the retained raw artifacts were then reproduced with the exact commands in
  [`correctness.json`](./correctness.json)
- Primary result: pass; the complete default Node selection finished in 571,294 ms under
  its executable 600,000 ms deadline. The M3 baseline contains 181 correctness checks
  with 0 failures, all eight benchmark gates passed, and the retained benchmark
  reproduction finished in 111,543 ms.
- Node smoke artifact: [`node-smoke.json`](./node-smoke.json); the exact 60-second
  workload completed in 52,332 ms with 9,056 completed operations, exactly 2,000 mixed
  namespace operations, full namespace and payload digests, zero live leases, staging
  certificates, and operation reservations, complete limits and environment, and
  slowest-operation diagnostics
- Raw benchmark artifacts: [`A3-cold-read.json`](./benchmarks/A3-cold-read.json) and its
  seven sibling artifacts; every retained result records candidate `11fc61d`,
  `worktreeDirty: false`, fresh-database-per-trial isolation, full hardware, cache, and
  effective resource limits, the 100 MiB fixture SHA-256, five raw digest-verified
  trials, min/max/mean and percentiles, and `pass: true`
- Gate medians: A3 cold read 300.7 MiB/s with 55 transactions and 84 statements; A4 warm
  read 826.4 MiB/s with a 2.748x warm/cold ratio; A5 canonical three of 100
  guaranteed-different edits in 49.609 ms; A6 500 guaranteed-different edits in
  9,587.797 ms and 0.838 ms per small read
- Workerd evidence: 12 checks passed; write-path hashing measured 391.1 MiB/s versus a
  65.5 MiB/s baseline, a 5.97x result
- Known deviations: no hosted CI cell is claimed because the candidate has not been
  pushed. The finite SQLite benchmark page-cache target is 128 MiB; lower 64 MiB and 96
  MiB tuning profiles truthfully exposed cold-read misses without changing the 250 MiB/s
  threshold. Operating-system cache dropping is unsupported in this local cell, so the
  exact cache state and unsuccessful drop flags are retained per trial.
- Independent audit: approved by the independent correctness, crash/resource, and
  evidence/spec review tracks for the exact candidate and retained artifacts
- Approved to begin next milestone: yes
