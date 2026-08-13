# Milestone 4 exit

- Candidate commit: `11fc61d2ed0bab979bd8d0cd468e024f35b8bbea`
- Date: 2026-08-13
- Sequential predecessor: accepted M3 candidate
  `11fc61d2ed0bab979bd8d0cd468e024f35b8bbea`
- Checklist complete: yes
- Primary environment: Windows `win32` x64, Node `v24.11.1`, pnpm `10.32.1`, AMD Ryzen
  Threadripper 7960X, 128 GiB RAM, Samsung 980 PRO / Crucial T705 SSD, SQLite `3.50.4`,
  4 KiB pages, 16 MiB SQLite cache, WAL, and no mmap
- Primary command: `pnpm validate:m5:pre-evidence` from the clean detached candidate
  worktree; the retained raw artifacts were then reproduced with
  `node tests/performance/branch-bench.mjs --artifacts=C:\Users\yifan\code\Ephemeral-AI-Lab\ephemeral-ai-fs\docs\evidence\m4\benchmarks`
- Primary result: pass; the complete default Node selection finished in 571,294 ms under
  its executable 600,000 ms deadline. The M4 baseline contains 181 predecessor checks
  and 58 branch checks with 0 failures; all 20 retained benchmark cells passed in
  19,740.0 ms.
- Correctness artifact: [`correctness.json`](./correctness.json)
- Concurrency evidence: 50 independent writers formed one parent chain; 50 same-inode
  writers produced exactly one merge and 49 explicit conflicts
- Raw benchmark evidence: [`benchmarks/index.json`](./benchmarks/index.json); all 20
  mandatory cells retain candidate `11fc61d`, `worktreeDirty: false`, the fixture
  SHA-256, full hardware and SQLite configuration, fresh-database isolation, all
  effective filesystem, storage, runtime, and branch limits, counters, and the raw trial
- Known deviation: no hosted CI cell is claimed because the candidate has not been
  pushed. The 20-cell branch matrix is correctness and bounded-scaling evidence with one
  raw trial per required cell; it is not presented as a release-latency claim.
- Independent audit: approved by the independent correctness, crash/resource, and
  evidence/spec review tracks for the exact candidate and retained artifacts
- Approved to begin next milestone: yes
