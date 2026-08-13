# Milestone 5 exit

- Candidate commit: `11fc61d2ed0bab979bd8d0cd468e024f35b8bbea`
- Date: 2026-08-13
- Sequential predecessor: accepted M4 candidate
  `11fc61d2ed0bab979bd8d0cd468e024f35b8bbea`
- Checklist complete: yes
- Primary environment: Windows `win32` x64, Node `v24.11.1`, pnpm `10.32.1`, AMD Ryzen
  Threadripper 7960X, 128 GiB RAM, Samsung 980 PRO / Crucial T705 SSD, SQLite `3.50.4`,
  4 KiB pages, 16 MiB SQLite cache, WAL, and no mmap
- Primary command: `pnpm validate:m5:pre-evidence` from a clean detached candidate
  worktree
- Primary result: pass in 571,294 ms under the executable 600,000 ms deadline. The
  command built once, ran every predecessor static gate, ran M3 and M4 benchmarks and
  the Node smoke uncontended, and ran the independent correctness groups concurrently.
  The accepted milestone topology contains 273 cumulative checks with 0 failures; the
  exact selected run also passed 201 core Node tests, 31 maintenance tests, 3 exhaustive
  fault tests, 12 Workerd checks, the Node smoke, and all 28 retained benchmark cells.
- Correctness artifact: [`correctness.json`](./correctness.json)
- Restart evidence: snapshot covered 110 statement and 42 batch positions; collection
  covered 157 statement and 75 batch positions; abandoned-run cleanup covered 61
  statement and 33 batch positions. Each fault-resume result counter equals its no-fault
  baseline.
- Scale evidence: actual fixture digest
  `c2ff2b167ed8af69ebb7896c9e2a7390906376c7f479fb38ee38687064373eed`; 100,001 namespace
  rows and reachable objects, 100,002 manifest roots, 100,001 manifest nodes, and
  300,003 peak durable marks in both snapshot and collection survived physical reopen
  and exact bounded recount.
- Resource evidence: identical workloads at 10,240 and 100,000 fixture rows measured
  9,274,632 and 9,315,643 bytes of managed-memory high-water, respectively. Absolute
  process peaks were 184,810,696 heap bytes and 380,272,640 RSS bytes. Maximum WAL was
  203,359,112 bytes under an explicit 512 MiB limit; the longest bounded maintenance
  call was 1,187.5 ms under the 5-second limit.
- Quota/corruption evidence: reachable corruption aborts before sweep and remains safe
  after physical reopen; normal root-journal exhaustion is atomic and emergency
  reconciliation compacts it; metadata-only page growth and pinned-WAL pressure return
  without partial filesystem mutation and recover after checkpoint/reopen.
- Known deviations: no hosted CI cell is claimed because the candidate has not been
  pushed; the optional extended 10 GiB and millions-of-rows diagnostics were not run.
  Those profiles are non-gating in the authoritative specifications.
- Independent audit: approved by the independent correctness, crash/resource, and
  evidence/spec review tracks for the exact candidate and retained artifacts
- Approved to begin M6: yes
