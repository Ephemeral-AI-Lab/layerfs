# fs-bench-plus optimization history

## Round 001 — baseline-09700086-20260831

- Status: INVALID
- UTC timestamp: 2026-08-30T17:46:17Z through 2026-08-30T17:48:10Z
- Local timestamp and timezone: 2026-08-31 01:46:17–01:48:10 CST (Asia/Shanghai)
- Git commit and tree: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`
- Dirty source seal and diff hashes: source seal `af654b406b6b63fc86fc0c1b3c65c7e2580ce6e2fa2fe6eaed7507be8ebfdf63`; unstaged diff `dd47e871ef6cf26900bab67d981fcd9c57e331165ad1f21e1314cdcb92dc3c2f`; staged diff `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`; status `fde293eee780e8645e33a082cb68605d65b9ae59e69e88697da8c72b1f40304b`
- Benchmark/profile and exact commands: current-source self-check (`benchmark/fs-benchmark-pro/run.sh --self-check`; `cargo test -p fs-benchmark-pro`) followed by legacy one-pair smoke (`benchmark/fs-benchmark-pro/run.sh smoke sha256:d0cccdb237d466405467672ca42ee342987fc6bfd54ccea53f41cf6dfd72306e sha256:927eb47918e2aec981121dcc62a526b29238a4c2083ea430a949411db9a89d3a baseline-09700086-20260831`)
- Candidate order seed and pair count: `b08b945fa3d7f1442efe20c70f31cec7d4ae8c814e64d7b172149b7f3a87858f`; one adjacent pair; LayerFS then Computer
- Host, kernel, Docker, CPU/memory/I/O envelope: Apple arm64 host, Darwin 25.4.0; Docker Desktop 29.5.2, Linux/arm64; one CPU, 1 GiB memory and swap, PID limit 512, 256 MiB `/tmp`; physical I/O unavailable
- Candidate image tags, digests, architectures, and verified OCI labels: LayerFS `layerfs-fs-benchmark-pro:baseline-09700086`, `sha256:927eb47918e2aec981121dcc62a526b29238a4c2083ea430a949411db9a89d3a`, arm64, commit/tree/dirty/source-seal labels matched; Computer diagnostic image `sha256:d0cccdb237d466405467672ca42ee342987fc6bfd54ccea53f41cf6dfd72306e`, arm64, pinned commit/tree labels matched, build mode `diagnostic-prebuilt-dist`
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `7d364e66550369588cd38fd951a6a8e9851fb20d597fa89f92d6d75713818fe0`; recovery `7aa4c08a0b2017239a32583127f484901b782fcf6e9be96d9727f811a1d04467`; mount `/workspace/fs-benchmark-pro-7`; mountinfo not retained (invalid); helper `49ab6d46a9baf0a4586cff75512c47310f7711594225ecaa8209367c05083d50`
- Raw evidence directory and SHA-256 inventory: `runs/baseline-09700086-20260831/raw/`; inventory `runs/baseline-09700086-20260831/raw-inventory.sha256`, SHA-256 `bd48c0d8de7f23fa9ec885631e454fd70e97001e5c305ec5a6884fce84c1e473`
- Previous comparable round: none
- Current best comparable round: none; this run is not protocol-0.2 comparable

### Hypothesis and planned change

Establish an unchanged-source measurement before optimization. The expected first defect was invalid LayerFS workload wiring through the benchmark's `timed_docker` helper.

### Changes since the previous round

None. This is the current-source baseline; user-owned documentation changes were preserved.

### Correctness and validity

Both arms produced the exact 33,554,442-byte final file with SHA-256 `7b86abcd0e9d2016bbb8b16722e1439475feff84e31fe9801a4ec74e99dc74c3`; fresh-container LayerFS reopen passed. Empirical evidence is nevertheless INVALID for protocol 0.2: LayerFS registered commands and recovery used direct `docker exec`; the arms used different workload implementations; matched two-Store checkpoint/fsync evidence, in-container FUSE mountinfo, terminal SDK output receipts, required scenarios, phase equations, cache axes, and protocol custody fields are absent. The old reporter's `VALID` label applies only to its obsolete v1 schema and is superseded by this erratum.

### Comparable E2E results

One diagnostic sample only; no confidence interval or superiority claim is available. Legacy registered total: Computer 7,640,913,297 ns; LayerFS authority-only total 44,198,627,807 ns; paired ratio 5.784469; speedup 0.172877; wins/ties/losses 0/0/1. Q1, median, Q3, min, and max are each the sole sample. The legacy LayerFS number excludes required Workspace create/end lifecycle and is not protocol-0.2 comparable. Its sixteen-edit authority-only total was 39,479,532,763 ns versus Computer 2,255,666,919 ns (ratio 17.502377).

### LayerFS phase decomposition

Legacy aggregate Workspace Commit was 38,278,323,643 ns. Required Commit, Push, durability, FUSE, and unattributed fragments were not emitted, so no balanced protocol-0.2 decomposition exists.

### Algorithm, transfer, storage, memory, and I/O counters

The legacy authority-checkpoint snapshot reported 69,996,544 database bytes, 10,897,464 WAL bytes, 65,536 SHM bytes, and 84,111,360 allocated bytes. Semantic union/intersection, source reuse, dirty/CDC/comparison work, transfer frontier, storage S0–S8, bounded memory, page faults, and process/cgroup I/O were unavailable.

### Comparison with Computer, previous round, and current best

Computer's legacy total was 7.641 s and LayerFS's incomplete boundary was 44.199 s. There is no previous or valid current-best protocol-0.2 round.

### Defects and root causes

The benchmark calls `timed_docker`/`docker exec` from `WorkspaceRun::mutation`, `WorkspaceRun::read`, and recovery instead of routing through `Client::exec_workspace_session`, `Client::workspace_output`, and `OutputReader::read`. This bypasses the frozen public execution path and prevents terminal receipt evidence. The harness also has an obsolete scenario matrix/topology and reporter boundary. Production receipts expose no matched two-Store checkpoint/database-fsync/directory-fsync completion; therefore Push acknowledgment cannot prove the required stable boundary.

### What needs improvement next

Replace direct benchmark workload execution with the existing SDK Exec/Output lifecycle, use one byte-identical sealed helper in both images, move results to the frozen custody root/layout, and add the smallest passive receipt fields needed to prove terminal output, Commit/Push phases, and in-lifecycle two-Store durability before interpreting performance.

### Stable strengths — no improvement currently needed

Fixture generation and final byte oracles are deterministic and passed. The pinned Computer commit/tree and LayerFS image source labels were checked. Fresh measurement and recovery containers were distinct. Preserve these mechanisms unless contrary evidence appears.

### Subagent reviews and reconciled decision

Independent read-only benchmark and SDK/FUSE reviews confirm the direct-exec defect, obsolete result root/boundary, missing Reference-seeded topology, unobserved `OutputReader::read`, and absent two-Store durability evidence. Detailed reviews remain external to this ledger; the primary decision is to repair public-path validity before performance work.

### Next action

Implement the minimum protocol-0.2 self-check/smoke path through existing public SDK operations and passive receipts, then rerun a fresh sealed one-pair campaign and append Round 002.

## Round 002 — sdk-exec-passive-09700086-20260831

- Status: INVALID
- UTC timestamp: 2026-08-30T17:57:19Z through 2026-08-30T17:58:52Z
- Local timestamp and timezone: 2026-08-31 01:57:19–01:58:52 CST (Asia/Shanghai)
- Git commit and tree: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`
- Dirty source seal and diff hashes: source seal `41120c6e5e32598a3604128521dd993f241faf60964fdb3b9fe8cb7bc71372d8`; unstaged diff `553e6e4d4cdc389100a0fffe03d832c39eb5f5ee60b608d49b4f33520b11b913`; staged diff empty; status `470ab14216323e00aec2ee98ac355213c86f57eceb67172ef0d155d8257c1dd8`
- Benchmark/profile and exact commands: focused tests (`cargo test -p layerfs-workspace -p layerfs-monitor -p layerfs-sdk -p fs-benchmark-pro`; matching scoped clippy with `-D warnings`) and one-pair smoke (`benchmark/fs-benchmark-pro/run.sh smoke sha256:d0cccdb237d466405467672ca42ee342987fc6bfd54ccea53f41cf6dfd72306e sha256:8242fd3415c7eb40d7d1ad9069a531d6deb4c233950d831470b57daddc14190a sdk-exec-passive-09700086-20260831`)
- Candidate order seed and pair count: `2e74cbb3ea59bda5fc573395ec21e4cc9b00a2528841c94f36e81acfe82dff43`; one adjacent pair; LayerFS then Computer
- Host, kernel, Docker, CPU/memory/I/O envelope: Apple arm64 host, Darwin 25.4.0; Docker Desktop 29.5.2, Linux/arm64; one CPU, 1 GiB memory and swap, PID limit 512, 256 MiB `/tmp`; physical I/O unavailable
- Candidate image tags, digests, architectures, and verified OCI labels: LayerFS `layerfs-fs-benchmark-pro:sdk-exec-passive-09700086`, `sha256:8242fd3415c7eb40d7d1ad9069a531d6deb4c233950d831470b57daddc14190a`, arm64, commit/tree/dirty/source-seal labels matched; Computer diagnostic image `sha256:d0cccdb237d466405467672ca42ee342987fc6bfd54ccea53f41cf6dfd72306e`, arm64, pinned commit/tree labels matched, build mode `diagnostic-prebuilt-dist`
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `9286b66513fc1db8c079d08b14b0a72f08fd7542dd006efc1f3861eca00f73b1`; recovery `ecb01764c270501eda23a40c2349dbc549871d15c6a8b5769c3dc7423e709651`; mount `/workspace/fs-benchmark-pro-7`; mountinfo not retained (invalid); helper `49ab6d46a9baf0a4586cff75512c47310f7711594225ecaa8209367c05083d50`
- Raw evidence directory and SHA-256 inventory: `runs/sdk-exec-passive-09700086-20260831/`; inventory `raw-inventory.sha256`, SHA-256 `85bb7ab78975f4f1076af977b377ee985d965eed8c480c845d41f55b48f4f586`
- Previous comparable round: none
- Current best comparable round: none; required Computer lifecycle and LayerFS durability evidence remain incomplete

### Hypothesis and planned change

Replacing the benchmark's direct workload `docker exec` with the existing public SDK Exec/Output path and making Monitor Workspace summaries passive should establish trustworthy terminal execution evidence without warming Commit inputs through a hidden full-tree comparison.

### Changes since the previous round

All LayerFS registered mutation, read, and recovery commands now use `Client::exec_workspace_session`, `Client::workspace_output`, and `OutputReader::read` to a terminal receipt. Required output truncation, exit code, stopped state, stdout/stderr byte totals, and SDK Exec/Output receipts are retained. Active Workspace summaries now use the already-maintained mutation generation instead of exact tree comparison. The runner/reporter result root is the frozen `benchmark-results/fs-bench-pro/runs` path.

### Correctness and validity

Every one of the 19 legacy commands emitted `workspace.exec` and `workspace.output` receipts, reached a terminal receipt with exit code 0 and `stopped=false`, and produced untruncated output. The final 33,554,442-byte SHA-256 oracle and fresh-container SDK recovery passed. Evidence remains INVALID for protocol 0.2 because Computer and LayerFS still use different helpers; the edit topology is not Reference-seeded; most required scenarios and FUSE rows are absent; matched two-Store durability and mountinfo are absent; the Computer/LayerFS complete lifecycle boundary is incomplete; and custody is still the legacy flat schema.

### Comparable E2E results

One diagnostic sample only, so Q1/median/Q3/min/max are the same sole value and no CI or superiority claim is available. Legacy Computer registered total was 7,778,851,712 ns. LayerFS authority-only total was 45,876,111,606 ns (ratio 5.897543; speedup 0.169562); LayerFS complete-turn total was 50,477,690,896 ns. Because Computer Workspace create/end are omitted, no protocol-0.2 complete-workflow ratio exists. The sixteen-edit authority-only total was 41,170,526,897 ns versus Computer 2,374,852,919 ns; wins/ties/losses 0/0/1.

### LayerFS phase decomposition

Aggregate Workspace Commit was 38,727,022,018 ns. The public execution path is now split per operation into SDK dispatch and output-to-terminal; for example, `edit-01` was 88,085,084 ns dispatch and 40,420,958 ns output-to-terminal. Required Commit, Push, durability, FUSE, and unattributed subphase equations are still unavailable.

### Algorithm, transfer, storage, memory, and I/O counters

The authority-checkpoint snapshot reported 69,992,448 database bytes, 10,876,864 WAL bytes, 65,536 SHM bytes, and 83,955,712 allocated bytes. Exact dirty/compare/CDC/source-reuse, S0–S8, union/intersection, transfer frontier, memory, page-fault, and process/cgroup I/O evidence remains unavailable.

### Comparison with Computer, previous round, and current best

This round proves a more complete LayerFS public execution route than Round 001, so its longer timing is not a regression conclusion. It is still not comparable under protocol 0.2 and cannot become a current best.

### Defects and root causes

The public-path bypass and Monitor pre-Commit scan were fixed. The dominant measured phase is still Workspace Commit: it ignores normalized dirty intervals, hashes complete base/final files, rebuilds the complete file with CDC, and copies authority-present candidates into BranchStore. Independent review also confirmed Push treats authority root-row presence as a complete closure receipt, which makes interrupted partial transfers unrecoverable, and that Push returns without the required two-Store checkpoint/fsync barrier.

### What needs improvement next

Fix the Push completeness correctness bug, introduce one byte-identical sealed helper for both arms, establish separate cold-create and Reference-seeded topologies, then add in-lifecycle durability evidence. Only after those validity gates should the dirty-range Commit path be optimized and interpreted.

### Stable strengths — no improvement currently needed

Public Container/FUSE Workspace creation, SDK execution dispatch, bounded OutputReader terminal delivery, deterministic oracles, distinct measurement/recovery containers, and image source-label checks passed. Monitor snapshots no longer exact-scan active Workspace contents. Keep these paths stable.

### Subagent reviews and reconciled decision

The benchmark audit found the direct-exec, helper asymmetry, topology, scenario, boundary, custody, and statistics gaps. The SDK/FUSE audit found active Monitor scans, missing terminal Monitor evidence, missing FUSE attachment evidence, and absent two-Store durability. The content/storage audit found full-file Commit amplification, missing Reference candidate classification, and the root-presence-as-completeness Push bug. The reconciled order is correctness and validity first: Push retry correctness, shared helper/topology, durability, then measured Commit optimization.

### Next action

Remove root-object-presence pruning from Push, add a general interrupted-transfer regression test, and rerun focused BranchStore verification before changing benchmark scenarios.

## Round 003 — push-repair-09700086-20260831

- Status: INVALID
- UTC timestamp: 2026-08-30T18:02:17Z through 2026-08-30T18:03:53Z
- Local timestamp and timezone: 2026-08-31 02:02:17–02:03:53 CST (Asia/Shanghai)
- Git commit and tree: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`
- Dirty source seal and diff hashes: source seal `85a4d6219e266e87546d82e6354af5a115e2ceb5d2c3f50cab70d81557e1f616`; unstaged diff `8f2983cb1bd6c0d70bf54939079c47dfad734dd88b9438bf7b0cce9a75f11f26`; staged diff empty; status `d510b2e8a43810c6f4a82717232fcad2a9072b468ef66bc620348ce7cb6cda59`
- Benchmark/profile and exact commands: focused regression and direct dependents (`cargo test -p layerfs-branch-store`; scoped clippy with `-D warnings`) followed by one-pair smoke (`benchmark/fs-benchmark-pro/run.sh smoke sha256:d0cccdb237d466405467672ca42ee342987fc6bfd54ccea53f41cf6dfd72306e sha256:4946653df12805dbedaa2f235815f50d87908a29ac205e649a81c0f5e8814852 push-repair-09700086-20260831`)
- Candidate order seed and pair count: `f951ab25f652f0827802797493963e9374a888cd429705c8f31e737f4d6a6901`; one adjacent pair; LayerFS then Computer
- Host, kernel, Docker, CPU/memory/I/O envelope: Apple arm64 host, Darwin 25.4.0; Docker Desktop 29.5.2, Linux/arm64; one CPU, 1 GiB memory and swap, PID limit 512, 256 MiB `/tmp`; physical I/O unavailable
- Candidate image tags, digests, architectures, and verified OCI labels: LayerFS `layerfs-fs-benchmark-pro:push-repair-09700086`, `sha256:4946653df12805dbedaa2f235815f50d87908a29ac205e649a81c0f5e8814852`, arm64, labels matched; Computer diagnostic image `sha256:d0cccdb237d466405467672ca42ee342987fc6bfd54ccea53f41cf6dfd72306e`, arm64, pinned labels matched
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `79f69e8295f50e45b985f0b128313e84432e426eb3d2f018b9d6344796585953`; recovery `8f2983cb1bd6c0d70bf54939079c47dfad734dd88b9438bf7b0cce9a75f11f26`; mount `/workspace/fs-benchmark-pro-7`; mountinfo unavailable (invalid); helper `49ab6d46a9baf0a4586cff75512c47310f7711594225ecaa8209367c05083d50`
- Raw evidence directory and SHA-256 inventory: `runs/push-repair-09700086-20260831/`; inventory SHA-256 `32091ee059847af9033b60bb0c19d1e20d40930449b970b2a6a5a6a0418bb043`
- Previous comparable round: none
- Current best comparable round: none

### Hypothesis and planned change

Push must traverse every unacknowledged suffix root as potentially incomplete; root-row membership alone cannot safely prune its descendants.

### Changes since the previous round

Removed the authority root-membership completeness shortcut from `PushRootRequests`. Added a regression that pre-admits only the canonical root object at the authority, proves a descendant is absent, then verifies Push repairs the closure and publishes the Branch.

### Correctness and validity

The new regression fails under the old algorithm with a missing descendant and passes with the repair. All BranchStore tests, clippy, exact file oracle, terminal SDK receipts, and fresh-container reopen passed. The smoke remains protocol-0.2 INVALID for the same unimplemented helper/topology/durability/scenario/custody gates recorded in Round 002.

### Comparable E2E results

One diagnostic sample: legacy Computer total 7,893,910,170 ns; LayerFS authority-only total 47,747,671,477 ns (ratio 6.049938; speedup 0.165326); LayerFS complete-turn total 52,388,718,397 ns. Q1/median/Q3/min/max are the sole sample; no CI; wins/ties/losses 0/0/1. No protocol-0.2 comparison is available.

### LayerFS phase decomposition

Aggregate Workspace Commit was 40,384,300,850 ns. The correctness repair adds no new phase and no valid conclusion can be drawn from one noisy smoke.

### Algorithm, transfer, storage, memory, and I/O counters

The focused test proves missing-only closure repair when the root is already present. Required per-root traversal/authentication and authority-verifier counters remain unavailable.

### Comparison with Computer, previous round, and current best

The one-smoke legacy total is slower than Round 002, but profiles are invalid and single samples are noise; this is neither a regression verdict nor a comparable best.

### Defects and root causes

The root-presence-as-completeness correctness defect is fixed at the shared Push root iterator. Remaining highest-priority defects are shared-helper/topology validity and absent two-Store durability, followed by full-file Workspace Commit amplification.

### What needs improvement next

Add matched BranchStore and LayerStackStore checkpoint/database-fsync/directory-fsync work inside existing Push and expose its passive outcome/timing through the existing receipt path.

### Stable strengths — no improvement currently needed

Owned-suffix history traversal, bounded membership/object/fact batches, missing-only payload admission, visibility-last publication, SDK execution/terminal receipts, deterministic oracles, and fresh recovery remain correct.

### Subagent reviews and reconciled decision

The independent content/storage review identified this as a P0 correctness defect. The focused failure reproduced its precise retry mode; the primary agent accepted the minimal shared fix and rejected caller-side guards.

### Next action

Implement the smallest internal two-Store stable barrier in Push with focused failure/outcome tests, then rerun the public smoke.

## Round 004 — durability-09700086-20260831

- Status: INVALID
- UTC timestamp: 2026-08-30T18:13:02Z through 2026-08-30T18:14:41Z
- Local timestamp and timezone: 2026-08-31 02:13:02–02:14:41 CST (Asia/Shanghai)
- Git commit and tree: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`
- Dirty source seal and diff hashes: source seal `cec66d071078c33ccfacd114d8e963ccdd6a78cbd46be4c5dc3e5d7a1e18e59f`; unstaged diff `907824de78f7d723bc563fb49749c1fb2803c040b712d18deb844429421d8d0d`; staged diff empty; status `0f1ba05675643f737deb15c6d63b83acc1c4eaacca450e0585a574566cf6ad49`
- Benchmark/profile and exact commands: focused Store/SDK/Monitor/BranchStore tests and scoped clippy, then `benchmark/fs-benchmark-pro/run.sh smoke sha256:d0cccdb237d466405467672ca42ee342987fc6bfd54ccea53f41cf6dfd72306e sha256:f37fc9dd474cf3105e5871009327431a7e4ebfccf30b074eb1c676b0b0d7a7ba durability-09700086-20260831`
- Candidate order seed and pair count: `7965e35ea975fe673a8b6e151aed651e83037277f48a706cb4f7da8ea543d1a1`; one adjacent pair; Computer then LayerFS
- Host, kernel, Docker, CPU/memory/I/O envelope: Apple arm64 host, Darwin 25.4.0; Docker Desktop 29.5.2, Linux/arm64; frozen one-CPU/1-GiB envelope; physical I/O unavailable
- Candidate image tags, digests, architectures, and verified OCI labels: LayerFS `layerfs-fs-benchmark-pro:durability-09700086`, `sha256:f37fc9dd474cf3105e5871009327431a7e4ebfccf30b074eb1c676b0b0d7a7ba`, arm64, labels matched; Computer diagnostic `sha256:d0cccdb237d466405467672ca42ee342987fc6bfd54ccea53f41cf6dfd72306e`, arm64
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `2dc9d9f587a3f7adf96ad272f406a65a0e33ad49ba075959e1802e6661a050b4`; recovery `b766a83b9263f0d4accb00061b2cc575dcbef49540ed64e5f1d245feebec0630`; mount `/workspace/fs-benchmark-pro-7`; mountinfo unavailable; helper `49ab6d46a9baf0a4586cff75512c47310f7711594225ecaa8209367c05083d50`
- Raw evidence directory and SHA-256 inventory: `runs/durability-09700086-20260831/`; inventory SHA-256 `e729e5d9c5e02ae8b3039f94404ee650be50a8a4fe433a59c1f468bc992809cf`
- Previous comparable round: none
- Current best comparable round: none

### Hypothesis and planned change

Existing WAL/FULL transactions do not prove the matched quiesced boundary. Stabilize the authority inside existing `publish_branch`, then stabilize BranchStore before Push returns, and attach both receipts to the existing BranchPush operation.

### Changes since the previous round

Added a shared Store stable barrier: WAL checkpoint/TRUNCATE, database fsync, and parent-directory fsync with a balanced timing receipt. Existing Push now records the LayerStackStore barrier before authority publication returns and the BranchStore barrier before Push returns. Receipt encoding remains backward-compatible with retained v3 records; SDK tests verify two bound Store IDs and survive Client reopen. The benchmark rejects any mutating checkpoint without exactly those two valid receipts.

### Correctness and validity

All focused durability, persistence, Push, Monitor, benchmark, and clippy checks passed. The live smoke passed exact terminal/oracle/recovery checks and every Push exposed two bound durability receipts. Evidence remains INVALID because helper symmetry, Reference-seeded topology, required scenarios/FUSE rows, mountinfo, complete comparison boundaries, and terminal custody are incomplete.

### Comparable E2E results

One diagnostic sample: Computer legacy total 8,181,662,921 ns; LayerFS authority-to-stable total 50,232,453,063 ns (ratio 6.139638; speedup 0.162876); LayerFS complete-turn total 55,017,762,397 ns. Q1/median/Q3/min/max equal the sole sample; no CI; wins/ties/losses 0/0/1. No protocol-0.2 comparison is available.

### LayerFS phase decomposition

Aggregate Workspace Commit was 41,779,756,270 ns. For `edit-01`, Push was 261,397,541 ns; authority stability was 3,430,042 ns (2,128,792 checkpoint, 1,181,875 database fsync, 119,125 directory fsync, 250 unattributed) and BranchStore stability was 2,712,791 ns (1,344,750 checkpoint, 1,267,541 database fsync, 100,292 directory fsync, 208 unattributed). Both equations balanced exactly.

### Algorithm, transfer, storage, memory, and I/O counters

Durability Store IDs, roles, and all six checkpoint/fsync fragments are now passive production evidence inside BranchPush. Other protocol mechanism/storage/resource counters remain incomplete.

### Comparison with Computer, previous round, and current best

Round 004 adds real required durability work and therefore cannot be compared as a regression against earlier incomplete boundaries. No valid current best exists.

### Defects and root causes

Matched LayerFS two-Store stability is fixed for successful Push. Computer still uses a separate helper and its barrier is not decomposed into required named fragments. The dominant LayerFS defect remains full-file Commit amplification.

### What needs improvement next

Use one byte-identical sealed helper and read-only fixture mount in both arms; then create a true Reference-seeded edit topology.

### Stable strengths — no improvement currently needed

Two-Store stable acknowledgment, Push retry repair, transaction boundaries, public SDK terminal execution, deterministic oracles, and fresh LayerFS recovery pass and should remain unchanged.

### Subagent reviews and reconciled decision

The SDK/FUSE and content/storage reviews both located durability after publication and before Push return. The implementation uses existing publish/Push calls rather than a new public operation or endpoint.

### Next action

Admit one shared helper into both images, mount the same fixture read-only for both candidates, verify equal helper SHA-256 before any trial, and rerun smoke.

## Round 005 — shared-helper-09700086-20260831

- Status: FAILED
- UTC timestamp: 2026-08-30T18:26:28Z through 2026-08-30T18:26:29Z
- Local timestamp and timezone: 2026-08-31 02:26:28–02:26:29 CST (Asia/Shanghai)
- Git commit and tree: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`
- Dirty source seal and diff hashes: source seal `53a7e14c164679b9b8606f81ee0d5d973a27b20ee60c2dc3e57b1e3441d1527e`; unstaged diff `e9a802b28cf6b799e67caecdcc445fc0e38cd47d120f15306e3320e5ae8279a0`; staged diff empty; status `c4d45c7d151f346a82b71184345263563b172206a689a2082adf775f97921d00`
- Benchmark/profile and exact commands: `benchmark/fs-benchmark-pro/run.sh smoke sha256:3fc1dee681eb4d29b6108f5c21bf6345a3c6adc28fc309c381d079265912f501 sha256:e02e25055407945fcbb6cf945fb10881ad77e9959c13962793c54048aedcb167 shared-helper-09700086-20260831`
- Candidate order seed and pair count: `d3b11a8de9bba65c40efd1262c8b3bae3ff956f32ca15d6c15ca55ca44a4ee62`; one scheduled pair; Computer first; campaign stopped on the failed arm
- Host, kernel, Docker, CPU/memory/I/O envelope: same frozen arm64 Docker Desktop envelope
- Candidate image tags, digests, architectures, and verified OCI labels: LayerFS `sha256:e02e25055407945fcbb6cf945fb10881ad77e9959c13962793c54048aedcb167`; Computer diagnostic `sha256:3fc1dee681eb4d29b6108f5c21bf6345a3c6adc28fc309c381d079265912f501`; both arm64; helper SHA admission passed
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: LayerFS arm did not start; shared helper SHA-256 `e20271d4c39f3ac974eddfae82bcfaffd9e5b6877131103fffb04fd319a87fe6`
- Raw evidence directory and SHA-256 inventory: `runs/shared-helper-09700086-20260831/`; inventory SHA-256 `8f6c82babd3fb67aeb66a938c96536695edec7390be03c155b0386b70f7ee497`
- Previous comparable round: none
- Current best comparable round: none

### Hypothesis and planned change

The same helper file and read-only fixture path should remove syscall/helper asymmetry without changing candidate product behavior.

### Changes since the previous round

Computer commands were routed through the same `workload.py`; both images admitted the byte-identical file; the runner now verifies both image copies against the host SHA before creating a run. LayerFS now receives the fixture through a read-only `/fixture/payload.bin` bind rather than a candidate-owned hard link.

### Correctness and validity

FAILED_CANDIDATE and INVALID. Helper SHA admission passed, but the chosen diagnostic Computer base lacked `/opt/cloudflare-computer/packages/computer/dist/index.js`; Computer emitted a typed module-not-found failure before its first registered operation. LayerFS was not run and no result was silently retried.

### Comparable E2E results

No complete pair; all statistics and wins/ties/losses are unavailable.

### LayerFS phase decomposition

N/A — LayerFS arm did not start.

### Algorithm, transfer, storage, memory, and I/O counters

N/A — no measured pair.

### Comparison with Computer, previous round, and current best

No comparison. Earlier rounds remain invalid.

### Defects and root causes

The functional diagnostic image was layered from a pinned base that contained computerd but not the built Computer package used by the adapter. Separately, the exact sealed-source build was attempted from a verified upstream archive whose SHA-256 matched the frozen value, but Docker Desktop's HTTP proxy truncated Debian's arm64 package index after five retries.

### What needs improvement next

Layer the diagnostic helper adapter on the prior diagnostic image that contains both Computer and computerd for a smoke-only proof. Retry the exact sealed-source build unchanged when the package proxy is healthy; formal remains unavailable until it succeeds.

### Stable strengths — no improvement currently needed

Helper source equality, host/image SHA admission logic, read-only fixture mounting, source archive/tree verification, and failure custody behaved correctly.

### Subagent reviews and reconciled decision

No new subagent review was needed. The failure is isolated to diagnostic image composition; no product or benchmark result was interpreted.

### Next action

Rebuild the smoke-only adapter from the complete prior diagnostic Computer image, retain the same helper bytes, and start a new run ID.

## Round 006 — shared-helper-2-09700086-20260831

- Status: INVALID
- UTC timestamp: 2026-08-30T18:28:19Z through 2026-08-30T18:29:57Z
- Local timestamp and timezone: 2026-08-31 02:28:19–02:29:57 CST (Asia/Shanghai)
- Git commit and tree: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`
- Dirty source seal and diff hashes: source seal `53a7e14c164679b9b8606f81ee0d5d973a27b20ee60c2dc3e57b1e3441d1527e`; unstaged diff `e9a802b28cf6b799e67caecdcc445fc0e38cd47d120f15306e3320e5ae8279a0`; staged diff empty; status `6668950e180a5e36a7b94cdfea5b65e922ce8b1f2a827ba92624ad91fb23f358`
- Benchmark/profile and exact commands: `benchmark/fs-benchmark-pro/run.sh smoke sha256:9101541ae0b374100430afccedf6b5c941e2b54d6130abf783bbaa6f1ca7353f sha256:e02e25055407945fcbb6cf945fb10881ad77e9959c13962793c54048aedcb167 shared-helper-2-09700086-20260831`
- Candidate order seed and pair count: `1bb28eac37badb6eb8c2e4b2c369d4ac0ff333c3952c85edc46f9c8a12877c5c`; one pair; LayerFS then Computer
- Host, kernel, Docker, CPU/memory/I/O envelope: frozen arm64 Docker Desktop one-CPU/1-GiB envelope
- Candidate image tags, digests, architectures, and verified OCI labels: LayerFS `sha256:e02e25055407945fcbb6cf945fb10881ad77e9959c13962793c54048aedcb167`; Computer diagnostic `sha256:9101541ae0b374100430afccedf6b5c941e2b54d6130abf783bbaa6f1ca7353f`; arm64; source labels and equal helper SHA passed
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `c47427e5f7e2917127ec6dc1161001e0131727fd8c13c10d7a38dcb891724cd2`; recovery `b73d3f89620d1a3736ae5bc19ad4d80c1aa8cd95d3c6223e076a89cae7a9aa3b`; mount `/workspace/fs-benchmark-pro-7`; mountinfo unavailable; helper `e20271d4c39f3ac974eddfae82bcfaffd9e5b6877131103fffb04fd319a87fe6`
- Raw evidence directory and SHA-256 inventory: `runs/shared-helper-2-09700086-20260831/`; inventory SHA-256 `f8c66f3e5db3089609ae301830a954882e7d9efa27ca8c3a635b2d1a5580bf55`
- Previous comparable round: none
- Current best comparable round: none

### Hypothesis and planned change

Using the exact same helper bytes and read-only fixture in both arms removes workload syscall asymmetry.

### Changes since the previous round

The diagnostic Computer adapter was rebuilt from the complete previous diagnostic image. The runner proved the same helper SHA in host, Computer, and LayerFS before creating the run. LayerFS consumed the fixture through the read-only bind.

### Correctness and validity

Both arms completed exact oracles and reopen; LayerFS SDK terminal and two-Store durability evidence passed. Evidence remains INVALID because cold-create/edit topologies are still pooled, Computer is diagnostic rather than sealed-source, required scenarios/FUSE evidence/custody are absent, and the helper is an executable Python file rather than the final native sealed helper binary.

### Comparable E2E results

One diagnostic sample: Computer legacy total 7,812,747,877 ns; LayerFS authority-to-stable 50,229,032,943 ns (ratio 6.429173; speedup 0.155542); LayerFS complete-turn 54,938,123,363 ns. Q1/median/Q3/min/max equal the sample; no CI; wins/ties/losses 0/0/1.

### LayerFS phase decomposition

Workspace Commit totaled 41,796,228,814 ns. Required fine-grained Commit/Push/FUSE decomposition remains incomplete.

### Algorithm, transfer, storage, memory, and I/O counters

Helper/fixture parity is proven; other counters are unchanged.

### Comparison with Computer, previous round, and current best

No valid comparison or current best. The run is a functional fairness proof only.

### Defects and root causes

Helper asymmetry is fixed for smoke. The legacy topology still edits a Branch-owned file created earlier in the same context, so it cannot test Reference base avoidance.

### What needs improvement next

Split cold create and seeded edit contexts for both candidates and prove zero BranchStore objects after Reference Pull/Fork.

### Stable strengths — no improvement currently needed

Shared helper bytes, read-only fixture delivery, public SDK execution, two-Store durability, exact oracles, and recovery are stable.

### Subagent reviews and reconciled decision

The benchmark audit identified helper and fixture asymmetry as a hard invalidity. This round clears that smoke-level issue without claiming formal sealed provenance.

### Next action

Implement separate cold-create and seeded-edit contexts in both arms, including clean close/reopen before timed edits.

## Round 007 — reference-topology-09700086-20260831

- Status: INVALID
- UTC timestamp: 2026-08-30T18:32:37Z through 2026-08-30T18:34:23Z
- Local timestamp and timezone: 2026-08-31 02:32:37–02:34:23 CST (Asia/Shanghai)
- Git commit and tree: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`
- Dirty source seal and diff hashes: source seal `581229f2f7113ece55afb5d5c3f0d3889aeda9d24d4100278582f41e34dbbe69`; unstaged diff `4de14eb2a6c1c12fbf97287cfab1ed5069a54e46772da88659bb684cdb8bd935`; staged diff empty; status `6668950e180a5e36a7b94cdfea5b65e922ce8b1f2a827ba92624ad91fb23f358`
- Benchmark/profile and exact commands: `benchmark/fs-benchmark-pro/run.sh smoke sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:d39599af740ad541dfca90edbd3324bd387b17d6f912ba47f62c6b6cee4f7bb6 reference-topology-09700086-20260831`
- Candidate order seed and pair count: `3666f468b054548e45bfb793f255045f7474255e8ebb91a8e76c6f8b9f9796cf`; one pair; Computer then LayerFS
- Host, kernel, Docker, CPU/memory/I/O envelope: frozen arm64 Docker Desktop one-CPU/1-GiB envelope
- Candidate image tags, digests, architectures, and verified OCI labels: LayerFS `sha256:d39599af740ad541dfca90edbd3324bd387b17d6f912ba47f62c6b6cee4f7bb6`; Computer diagnostic `sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd`; arm64; labels/helper SHA matched
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `84721a6487fc34a3539ad554d26e086affcef4d3f8191313b808d0ccb8fdac50`; recovery `a3817bd2cdd745eafffdc766a2fe7d3af66d95f9e031c6312d3ed4e1a79ff8eb`; mount `/workspace/fs-benchmark-pro-7`; mountinfo unavailable; helper `e20271d4c39f3ac974eddfae82bcfaffd9e5b6877131103fffb04fd319a87fe6`
- Raw evidence directory and SHA-256 inventory: `runs/reference-topology-09700086-20260831/`; inventory SHA-256 `ab3787bade39ba9809a6c8497eebee15d113662ebe3882b9baf57e294f4a37c6`
- Previous comparable round: none
- Current best comparable round: none

### Hypothesis and planned change

True Reference editing requires a distinct authority seeded from the fixture, zero-object Reference Pull/Fork, and clean Store reopen before timing.

### Changes since the previous round

LayerFS cold creation now runs in its own empty context. Editing uses a separate authority initialized from the read-only fixture directory, Reference Pull and zero-copy Fork assertions, then closes and reconnects the exact Store pair before edits. Computer likewise uses separate cold and edit authority DBs, seeds through its public Workspace path, stops/reopens computerd and the DB, then edits.

### Correctness and validity

Zero BranchStore objects/bytes after Reference Pull and Fork were enforced; Store IDs survived reopen; exact helper/oracles, SDK terminal receipts, Push durability, and fresh recovery passed. Evidence remains INVALID because the Computer image is diagnostic, required scenario/FUSE matrices and mount evidence are incomplete, complete lifecycle comparison/reporting and custody are obsolete, and required mechanism counters are missing.

### Comparable E2E results

One diagnostic sample: Computer legacy total 8,789,904,880 ns; LayerFS authority-to-stable 52,053,193,525 ns (ratio 5.922', speedup 0.168864); LayerFS complete-turn 56,800,523,110 ns. Q1/median/Q3/min/max equal the sample; no CI; wins/ties/losses 0/0/1. The legacy reporter still omits Computer lifecycle, so no protocol comparison exists.

### LayerFS phase decomposition

Workspace Commit totaled 43,563,989,394 ns. First edit Commit generated a 1,737-object, 33,661,059-byte candidate; Push then took only the seven authority-missing objects (23,191 bytes) plus one Commit fact. Durability receipts remained balanced.

### Algorithm, transfer, storage, memory, and I/O counters

First edit candidate: 1,737 IDs and 33,661,059 bytes, all inserted into BranchStore. Push announced 1,739 IDs/33,661,300 bytes over 20 membership pages, sent seven IDs/23,191 bytes, and sent zero pulled ancestry facts. This is direct evidence that Workspace Commit rebuilds/copies the full authority-backed file even though transfer payload is missing-only.

### Comparison with Computer, previous round, and current best

This is the first topology-correct diagnostic bottleneck measurement but remains invalid for claims. It is not a current best.

### Defects and root causes

`Workspace::build_candidate` discards exact normalized dirty ranges, hashes complete base/final files, full-CDC rebuilds the final file, and admits authority-present candidates locally. The measured 33.66-MiB candidate for a ten-byte edit confirms the predicted O(file-size) defect.

### What needs improvement next

Route existing-file dirty ranges through `FileMutationBatch`, compare only dirty bytes, preserve namespace identity, and classify parent-present candidates before BranchStore admission. Add passive dirty/comparison/CDC/candidate counters and a focused varied-offset regression.

### Stable strengths — no improvement currently needed

Reference seed/fork zero-copy, Store reopen, missing-only Push payload, zero ancestry fact transfer, durability, public FUSE execution, exact oracles, and recovery are stable and should not be perturbed.

### Subagent reviews and reconciled decision

The content/storage review predicted the exact 1,737-object/33.66-MiB amplification now measured. The primary agent accepts Workspace candidate construction as the dominant root cause; Push payload transfer is not the next optimization target.

### Next action

Implement the minimum exact dirty-range file mutation path in Workspace using existing `FileMutationBatch`, with correctness tests for overwrite, no-op, append, truncate, rename, and unlink/recreate before another smoke.

### Erratum to Round 007

The stray apostrophe in the displayed authority-only ratio is a transcription error. The exact one-sample ratio is `5.921929103401173`.

## Round 008 — dirty-range-09700086-20260831

- Status: INVALID
- UTC timestamp: 2026-08-30T18:45:25Z through 2026-08-30T18:46:35Z
- Local timestamp and timezone: 2026-08-31 02:45:25–02:46:35 CST (Asia/Shanghai)
- Git commit and tree: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`
- Dirty source seal and diff hashes: source seal `eaea3b6126b799ac08fdc825ca98b6efafef27ecb6bbaa8add2a6c9c9ce8674d`; admitted working-tree patch `955f252ada877323e902572c8d434a8641d48d1785c776d276d9ad6a20cfa132`; status `afa379f666ab0804591fc6668e03751d77b18046fa799048417489165e24e2d7`
- Benchmark/profile and exact commands: focused Workspace/Monitor/SDK tests and clippy, then `benchmark/fs-benchmark-pro/run.sh smoke sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:0b95352aa99a302245f8bedcd6a5651039cff53ef2c7d9da0810fd7afb1be1dc dirty-range-09700086-20260831`
- Candidate order seed and pair count: `f819f62e96b056ef2d96b9fe4e4868b8bc20efdd4f1d15dccb8adf5413c4910b`; one pair; Computer then LayerFS
- Host, kernel, Docker, CPU/memory/I/O envelope: frozen arm64 Docker Desktop one-CPU/1-GiB envelope
- Candidate image tags, digests, architectures, and verified OCI labels: LayerFS `sha256:0b95352aa99a302245f8bedcd6a5651039cff53ef2c7d9da0810fd7afb1be1dc`; Computer diagnostic `sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd`; arm64; labels/helper SHA matched
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `e43a0e99701892def881fa16f4542e46f89d51a324dbad4f0b934b863eb62dce`; recovery `d451b625dbf86f14e1112d117b5542199fa874b03ce4e5b1244835388f2726bd`; mount `/workspace/fs-benchmark-pro-7`; mountinfo unavailable; FUSE helper `49ab6d46a9baf0a4586cff75512c47310f7711594225ecaa8209367c05083d50`; shared workload helper `e20271d4c39f3ac974eddfae82bcfaffd9e5b6877131103fffb04fd319a87fe6`
- Raw evidence directory and SHA-256 inventory: `runs/dirty-range-09700086-20260831/`; inventory SHA-256 `3d55c7edc265d2deb5193ca5a447b1545e0068c500e8c79d9bec050da5eebc36`
- Previous comparable round: none
- Current best comparable round: none

### Hypothesis and planned change

Exact dirty ranges plus `FileMutationBatch` should reduce a ten-byte Reference edit from complete-file rebuild/admission to a bounded canonical frontier.

### Changes since the previous round

Existing-file overlays now compare only registered dirty ranges, apply sorted range replacements through one `FileMutationBatch`, update the preserved canonical inode once, and keep a full streaming fallback only for new/opaque files. Clean shrink, grow/zero tails, append, no-op, and metadata are length-aware. Pure rename uses persistent namespace rename and preserves inode identity. Same-path recreation can no longer be mistaken for the base inode during planning. Local admission receipts now retain CDC and encode/hash counts with backward-compatible receipt parsing.

### Correctness and validity

Focused varied-data model tests pass exact overwrite, no-op, append, shrink, grow, and rename oracles, candidate bounds, CDC counts, and rename inode identity. All Workspace/Monitor/SDK tests and clippy passed. The live topology-correct smoke passed oracles, terminal receipts, durability, and recovery. Evidence remains INVALID because Computer is diagnostic, required scenario/FUSE matrices and mountinfo are absent, reporter/custody/statistics are obsolete, and several mandatory counters are missing.

### Comparable E2E results

One diagnostic sample: Computer legacy total 8,561,374,672 ns; LayerFS authority-to-stable 16,728,887,213 ns (ratio 1.953965; speedup 0.511772); LayerFS complete-turn 21,486,864,426 ns. EDIT16 authority total fell to 11,275,341,126 ns from Round 007's 47,027,593,481 ns. Q1/median/Q3/min/max equal the sample; no CI; wins/ties/losses 0/0/1.

### LayerFS phase decomposition

Aggregate Commit fell from 43,563,989,394 ns to 9,897,798,336 ns. `edit-01` complete turn was 897,351,334 ns: Commit 411,666,083 ns and Push 124,041,084 ns. Lifecycle and projection work remain material.

### Algorithm, transfer, storage, memory, and I/O counters

`edit-01` candidate is 10 IDs/16,908 bytes with exactly 10 CDC bytes and 10 encode/hash invocations, versus Round 007's 1,737 IDs/33,661,059 bytes. Push sent the exact ten authority-missing candidate objects (16,908 bytes) and one Commit fact. It still announced 407 IDs/7,664,284 bytes over ten membership pages, showing remaining frontier/verification amplification. Temp-prepend still creates a 1,747-object/33.66-MiB candidate and needs its separate one-pass source-reuse path.

### Comparison with Computer, previous round, and current best

The topology and helper are comparable at smoke method level, but the diagnostic Computer image and incomplete lifecycle reporter prevent a valid comparison. Mechanism and latency improved materially versus Round 007; there is still no formal current best.

### Defects and root causes

The complete-file small-edit defect is fixed. Commit remains ~0.4 s/edit; review and lifecycle tracing identify unnecessary FUSE refresh/remount after successful Commit as the next avoidable phase. Push announcement and authority verification also traverse more of the base than the sent frontier. Temp-prepend/new-file source reuse remains unoptimized.

### What needs improvement next

Avoid FUSE refresh for a successfully reloaded committed Workspace while retaining its existing mount read-only; keep Materialize refresh. Then measure before touching Push traversal.

### Stable strengths — no improvement currently needed

Dirty comparison/CDC/candidate work now meets the ten-byte mechanism limits. Reference seed zero-copy, missing-only Push payload, rename identity, no-op root equality, durability, SDK execution, oracles, and recovery are stable.

### Subagent reviews and reconciled decision

The dirty-range design review confirmed the shared batch mapping and identified length-aware cleanliness and current-length deletion as necessary; both were incorporated. Broader candidate/source-reuse work is deferred to opaque/new-file evidence.

### Next action

Measure the FUSE no-refresh Commit change with the same topology-correct smoke and retain the complete phase/candidate evidence.

## Round 009 — fuse-no-refresh-09700086-20260831

- Status: INVALID
- UTC timestamp: 2026-08-30T18:49:45Z through 2026-08-30T18:50:52Z
- Local timestamp and timezone: 2026-08-31 02:49:45–02:50:52 CST (Asia/Shanghai)
- Git commit and tree: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`
- Dirty source seal and diff hashes: source seal `121f015d6a2c62539e9c80318ad0abc5bc976eba921cd3e28975e046b9923b4d`; patch `4db7d369f474e4ddd1b4681216214feaa777dbb7a3292ea205bbb4bb9c7b8dce`; status `afa379f666ab0804591fc6668e03751d77b18046fa799048417489165e24e2d7`
- Benchmark/profile and exact commands: focused Workspace/SDK tests and clippy, then `benchmark/fs-benchmark-pro/run.sh smoke sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:49fa7b508d97ed875c5e308d61d81a633861cc708188f68175f070a940961ecb fuse-no-refresh-09700086-20260831`
- Candidate order seed and pair count: `8f2c1ff6b16c7ebe3d33bcf62857eb0b928c7b195a70e64332ce1a68099cd027`; one pair; Computer then LayerFS
- Host, kernel, Docker, CPU/memory/I/O envelope: frozen arm64 Docker Desktop one-CPU/1-GiB envelope
- Candidate image tags, digests, architectures, and verified OCI labels: LayerFS `sha256:49fa7b508d97ed875c5e308d61d81a633861cc708188f68175f070a940961ecb`; Computer diagnostic `sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd`; arm64; labels/helper matched
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `91ef65f8d6bb2508b9830b27b99edb3fa9c17ce76a1c41c7f0c29a2ef63cd9a6`; recovery `adbe101ed6821077384f70f21972e5ad29233c8e662ac0e1abb357557aaf0227`; mount `/workspace/fs-benchmark-pro-7`; mountinfo unavailable; helper hashes unchanged
- Raw evidence directory and SHA-256 inventory: `runs/fuse-no-refresh-09700086-20260831/`; inventory SHA-256 `e32cba586c0cc70b74d43d12331bef54bfcb841fd41ee12a39f375fe3fc91c50`
- Previous comparable round: none
- Current best comparable round: none

### Hypothesis and planned change

A reloaded FUSE Workspace can expose its committed root through the existing proxy and be remounted read-only without destroying and recreating the helper/mount.

### Changes since the previous round

Successful reloaded FUSE Commit now keeps the existing projection and only makes it read-only. Materialize retains its required refresh. No public lifecycle or operation changed.

### Correctness and validity

All focused tests/clippy, live smoke oracles, terminal receipts, durability, and fresh recovery passed. Evidence remains INVALID for the previously listed diagnostic-image, scenario, FUSE-evidence, reporter, statistics, and custody gaps.

### Comparable E2E results

One diagnostic sample: Computer legacy total 8,745,503,713 ns; LayerFS authority-to-stable 10,734,750,551 ns (ratio 1.22746; speedup 0.814691); LayerFS complete-turn 18,610,556,303 ns. EDIT16 authority total fell from 11,275,341,126 ns to 5,997,828,965 ns. Q1/median/Q3/min/max equal the sample; no CI; wins/ties/losses 0/0/1.

### LayerFS phase decomposition

Aggregate Commit fell from 9,897,798,336 ns to 3,977,152,751 ns. `edit-01`: 127,896,375 ns Commit, 142,400,500 ns Push, 216,728,292 ns Workspace create, 191,494,334 ns end, and 781,365,376 ns complete turn.

### Algorithm, transfer, storage, memory, and I/O counters

Small-edit mechanism remains 10 CDC bytes and 10 objects/16,908 candidate bytes. No-remount preserves that stable strength.

### Comparison with Computer, previous round, and current best

Authority-only registered total improved substantially, but complete lifecycle remains slower and the evidence is not formal-comparable.

### Defects and root causes

Workspace create/end now dominate each edit. DockerProjection pays several Docker CLI round trips to test/mkdir/copy/chmod/start and later pause/unmount/remove. These are general container-projection costs, not content work.

### What needs improvement next

Batch safe projection setup/cleanup Docker commands and avoid a redundant pause during Clean end of an already committed/paused Workspace. Preserve fresh helper processes and cleanup semantics.

### Stable strengths — no improvement currently needed

Dirty-range mechanism, existing FUSE mount reuse after Commit, Materialize refresh, durability, transfer payload, oracles, and recovery remain stable.

### Subagent reviews and reconciled decision

Lifecycle review identified FUSE projection transition as the measured phase. The primary change reused the live proxy/mount rather than adding a benchmark path.

### Next action

Measure the batched Docker projection lifecycle under the same smoke before considering helper-binary caching.

### Audit correction to Round 009

Round 009 was append-sealed before this correction. Its `raw-inventory.sha256` exists and its SHA-256 was reverified as `e32cba586c0cc70b74d43d12331bef54bfcb841fd41ee12a39f375fe3fc91c50`.

The `5,997,828,965 ns` EDIT16 value above is authority-only and is not public end-to-end latency. Summing the raw per-operation receipts gives the complete EDIT16 lifecycle of `12,622,303,342 ns`: Workspace create `3,787,168,919 ns`; SDK exec dispatch `1,411,666,335 ns`; SDK output to terminal `571,207,459 ns`; Commit `1,416,585,292 ns`; Push/durability `2,598,369,879 ns`; End `2,837,305,458 ns`. The component sum exactly equals the recorded complete-turn sum.

The no-refresh path is not yet correctness-proven. Commit replaced the Workspace node map while retaining a paused FUSE proxy whose attribute, directory, and read caches were not challenged by a live post-Commit read; read-only write rejection was also not tested. Therefore the latency improvement cannot justify retaining this production change until live read-after-Commit, no-stale-bytes, and write-rejection checks pass. The earlier statement that existing FUSE mount reuse was a stable strength is withdrawn.

Reporter totals and future comparisons must use the complete Workspace lifecycle. Authority-only totals remain diagnostic decomposition only.

## Round 010 — projection-batch-09700086-20260831

- Status: INVALID
- UTC timestamp: 2026-08-30T18:54:24Z through 2026-08-30T18:55:27Z
- Local timestamp and timezone: 2026-08-31 02:54:24–02:55:27 CST (Asia/Shanghai)
- Git commit and tree: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`
- Dirty source seal and diff hashes: source seal `f724965ebd1ed60e77e7b54269ad3265a8d2fbe074c0a746385e447aaf01a6d3`; unstaged patch `35fbca065af27aa481072d7dfaebab69fa34fdd7e9a5c74f59bda15d243dfa6b`; staged patch empty; status `1cf3876a680de1b271092c224bb403f3237e3f350ef35b0543e6eee71c6b405b`
- Benchmark/profile and exact commands: focused Workspace/SDK tests and clippy, then `benchmark/fs-benchmark-pro/run.sh smoke sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:ac59993780063d081c1d80fc5cbf8dcd5ed576b3e91e6af4bdbb99b1374af944 projection-batch-09700086-20260831`
- Candidate order seed and pair count: `624a89536cc01e60355d477b91dcacae879c6341c8fd81deaa4de47b53f104ba`; one pair; Computer then LayerFS
- Host, kernel, Docker, CPU/memory/I/O envelope: frozen arm64 Docker Desktop one-CPU/1-GiB envelope
- Candidate image tags, digests, architectures, and verified OCI labels: LayerFS `sha256:ac59993780063d081c1d80fc5cbf8dcd5ed576b3e91e6af4bdbb99b1374af944`; Computer diagnostic `sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd`; arm64; source labels and helper SHA matched
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `7445ae235289ec28e8ff01037f6eaf2494f5b8f5c6f89de2cfb9572fa3752e56`; recovery `769eb70f7b39476f907b0f21b6309e976c27ca4e2cb654c1209624e4f3b85596`; mount `/workspace/fs-benchmark-pro-7`; mountinfo unavailable; FUSE helper `49ab6d46a9baf0a4586cff75512c47310f7711594225ecaa8209367c05083d50`; shared workload helper `e20271d4c39f3ac974eddfae82bcfaffd9e5b6877131103fffb04fd319a87fe6`
- Raw evidence directory and SHA-256 inventory: `runs/projection-batch-09700086-20260831/`; every inventory entry reverified; inventory SHA-256 `90db5bb04726e753bba56441705105bc48def266cc231f8afa338045a0f73d3f`
- Previous comparable round: none
- Current best comparable round: none

### Hypothesis and planned change

Batching independent Docker projection setup/cleanup commands should remove fixed Workspace create/end round trips without changing storage semantics.

### Changes since the previous round

Projection setup combined device validation and directory preparation in one container command, copied the helper once, and combined chmod with helper execution. Cleanup combined control/helper removal. Clean end of an already committed Workspace skipped the redundant projection pause while retaining quiescence.

### Correctness and validity

The diagnostic smoke passed its existing byte/hash oracles, SDK terminal receipts, two-Store durability, and fresh recovery. It is INVALID: the Computer image is diagnostic; the reporter still promotes authority-only values; required scenarios, FUSE evidence, statistics, and custody are incomplete; and the retained no-refresh FUSE path lacks live read-after-Commit, stale-cache, and read-only write-rejection proof. Host Materialize also has a dirty-detection regression because the fast mutation-generation signal cannot detect out-of-band projection changes.

### Comparable E2E results

No public E2E comparison is claimed. Raw LayerFS registered complete lifecycle was `15,289,441,888 ns`; the legacy authority-only total was `10,881,066,263 ns` and must not be used as E2E. The Computer diagnostic registered total was `8,552,857,752 ns`, but its lifecycle model and provenance are not formal-comparable.

EDIT16 complete lifecycle was `9,865,336,177 ns`: Workspace create `1,954,166,835 ns`; SDK exec dispatch `1,516,407,377 ns`; SDK output to terminal `606,481,334 ns`; Commit `1,474,458,254 ns`; Push/durability `2,595,840,378 ns`; End `1,717,981,999 ns`. The component sum exactly equals the complete-turn sum. The reporter's `6,193,187,343 ns` EDIT16 number is authority-only diagnostic decomposition.

### LayerFS phase decomposition

Aggregate Commit was `3,990,219,297 ns`. `edit-01` complete turn was `592,326,542 ns`: Workspace create `117,755,709 ns`; SDK exec dispatch `108,143,875 ns`; SDK output to terminal `37,699,375 ns`; Commit `98,342,875 ns`; Push `127,914,542 ns`; End `102,470,166 ns`. Required passive Commit/capture/transaction subphases were not yet emitted, so no internal attribution is valid.

### Algorithm, transfer, storage, memory, and I/O counters

The ten-byte dirty-range mechanism remained bounded at ten CDC bytes and ten candidate objects/16,908 bytes in the available receipts. Push still announced 407 IDs/7.66 MiB over ten membership pages to transmit ten objects/16.9 KiB. FUSE live capture mode/captured-file/captured-byte proof, SQL transaction wait/statement/row/byte counters, memory, and complete I/O evidence are absent.

### Comparison with Computer, previous round, and current best

No valid comparison or current best exists. Relative to Round 009, raw Workspace create/end components fell, but a single invalid sample and the unproven FUSE path prohibit a performance conclusion.

### Defects and root causes

The public reporter hides Workspace lifecycle by using authority-only time. Commit fixed cost is still unattributed. The live FUSE no-refresh transition may expose stale cached bytes after the node-map reload. Host Materialize no longer checks projection dirtiness. Push performs closure announcement/verification far beyond the ten-object frontier. Workspace exec still uses multiple Docker control round trips. These must be isolated in that order rather than inferred from aggregate latency.

### What needs improvement next

Fix the reporter to make complete lifecycle primary. Restore Host Materialize projection-dirty detection. Add passive Commit subphases, explicit live-capture zero-work evidence, and bounded DB transaction timing/counters. Add live read-after-Commit, no-stale-bytes, and read-only write-rejection checks before retaining no-refresh. Do not optimize durability first.

### Stable strengths — no improvement currently needed

Dirty-range candidate construction, ten-byte candidate bounds, Reference zero-copy seeding, missing-only payload transfer, two-Store durability, public SDK execution, exact final/recovery oracles, and append-sealed evidence remain stable.

### Subagent reviews and reconciled decision

The external audit corrected the E2E boundary and identified the retained FUSE cache hazard, Materialize dirty regression, missing capture/DB evidence, and reusable-checkpoint direction. Those findings supersede the earlier lifecycle interpretation. This round is retained only as an immutable diagnostic record.

### Next action

Repair evidence and correctness first: complete-lifecycle reporting, Materialize dirty detection, passive Commit/capture/SQL attribution, and live post-Commit cache/write proof. Only then choose between reverting no-refresh or implementing a correct in-place reusable checkpoint.

## Round 011 — reusable-v03-09700086-20260831

- Status: FAILED (evidence PASS; optimization gate FAILED)
- UTC timestamp: 2026-08-30T19:28:22Z through 2026-08-30T19:28:39Z
- Local timestamp and timezone: 2026-08-31 03:28:22–03:28:39 CST (Asia/Shanghai)
- Git commit and tree: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`
- Dirty source seal and diff hashes: source seal `4a26460f8ef52aa9476ec807b192e0c4fd9357e9d6f3be9c9d75a4fd52287afc`; unstaged patch `a8b994a5e33a8e2bf3aa373c094c9f85c14076dc17d004af2fa2a93ec9f4adda`; staged patch empty; status `ea1f814ce2f6d9a366579c4780c1d2d697ac7b5da6060a7a4aa5ae4067319399`; untracked inventory `9555d337e06d07cd13d2e053b9381868ba9e884d624376d10cf9768401dca8bb`
- Benchmark/profile and exact commands: full focused package tests and warning-denying Clippy, cached LayerFS image build, then `benchmark/fs-benchmark-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:d7ad2ae59f9ba41050a22f007aac41ed9131668e2b6c5ed671952ea20bf54eef reusable-v03-09700086-20260831`
- Candidate order seed and pair count: `c071b9fe8465e186b866021d9f4323aff0c37a8e040a05df605db2864902a848`; one LayerFS-only focused sample; Computer was neither rebuilt nor executed
- Host, kernel, Docker, CPU/memory/I/O envelope: frozen arm64 Docker Desktop one-CPU/1-GiB prepared-container envelope
- Candidate image tags, digests, architectures, and verified OCI labels: LayerFS `sha256:d7ad2ae59f9ba41050a22f007aac41ed9131668e2b6c5ed671952ea20bf54eef`; arm64; commit/tree/dirty/source-seal labels passed. The pre-existing diagnostic Computer image was admission metadata only and was not scheduled.
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `f44143b072381e08746d57c282575dedf23e0bd339997d491f9cdbbc2149cdb7`; recovery `e2b3de2d109cdf416bb181fce705274a054e99322ebdfb11f9dd20cb1bf5fe0d`; mount `/workspace/fs-benchmark-pro-7`; mountinfo was not yet captured; FUSE helper `49ab6d46a9baf0a4586cff75512c47310f7711594225ecaa8209367c05083d50`; workload helper `e20271d4c39f3ac974eddfae82bcfaffd9e5b6877131103fffb04fd319a87fe6`
- Raw evidence directory and SHA-256 inventory: `runs/reusable-v03-09700086-20260831/`; every inventory entry reverified; inventory SHA-256 `fcae953f41b4f43a76a2618ca9b9c8b78762b14e351062f0d3ebd81037b89894`
- Previous comparable round: none; protocol 0.2 rows used a different Workspace lifecycle
- Current best comparable round: none; this is the first protocol-0.3 focused baseline and is not a paired claim

### Hypothesis and planned change

A safe in-place Commit rebase plus one Workspace/FUSE mount for EDIT16 should eliminate 15 create/end cycles while preserving NodeIds, open handles, lease, CAS, durability, and recovery. Passive named receipts should identify the remaining fixed costs before further optimization.

### Changes since the previous round

Protocol 0.3 makes complete public lifecycle the only comparable boundary. EDIT16 now creates one Workspace, repeats 16 SDK exec/output/fsync/Commit/Push checkpoints on the same active mount, and ends once. Commit rebases visible nodes in place, advances head/base/root/reader, clears committed dirty/spool state, retains the lease, and resumes. Stale CAS preserves the losing view; reconciliation refreshes its active Materialize presentation. Host Materialize dirty summaries again inspect out-of-band files, while live FUSE uses the mutation-generation fast path. Operation receipt v4 adds Commit, live-capture, Push, SQLite transaction, attach, and End phases. Existing-object collision authentication moved before the insert-only writer transaction. Both adapters and the reporter now use complete lifecycle, and Computer setup/container preparation is outside the headline.

### Correctness and validity

Evidence PASS: all focused package tests, diff checks, formatting, and warning-denying Clippy passed. The live focused run completed 16 Commit/Push checkpoints on one Workspace ID, emitted SDK terminal receipts, retained exact two-Store durability, matched the edited and final digests, and passed fresh-container recovery. Every edit reported `capture_mode=live`, zero captured files, zero captured bytes, ten CDC bytes, and at most ten candidate objects. SQL trace tests prohibit payload/recursive reads inside writer transactions. This remains non-formal because it is one LayerFS-only focused sample, mountinfo and several custody axes are missing, and required scenario/statistical matrices are incomplete.

### Comparable E2E results

No Computer comparison is claimed. Protocol-0.3 EDIT16 complete public lifecycle was `6,157,588,163 ns`, or `384,849,260 ns` amortized per durable edit. Components: one Workspace/FUSE create `131,444,292 ns`; SDK exec dispatch `1,419,212,456 ns`; OutputReader first-read-to-terminal `546,585,625 ns`; Commit API `1,366,481,874 ns`; Push/two-Store durability `2,560,015,582 ns`; one End `133,848,334 ns`. The exact component sum equals the complete total. The intermediate `<3.10 s`, hard `<=1.20 s`, and preferred `<=0.80 s` gates all failed.

### LayerFS phase decomposition

Passive Commit receipts summed to `1,355,998,754 ns`: pause/fence `647,780,333`; quiesce `4,959`; live capture `10,541`; candidate plan `11,950,623`; dirty compare `25,209`; persistent content `32,861,457`; namespace `6,771,206`; candidate finish `1,314,209`; local admission `26,804,498`; completeness verification `0`; publication/CAS `12,072,292`; in-place rebase `27,281,500`; resume `587,282,545`; unattributed `1,839,382`. Pause plus resume consumed `1,235,062,878 ns` (91.08% of receipted Commit), proving Docker CLI control round trips are the Commit bottleneck. Live capture was the required measured no-op.

Attach was `128,232,834 ns`: proxy `64,000`; Docker setup `42,433,333`; helper copy `23,023,583`; mount readiness `62,705,166`; three Docker calls. End lifecycle was `79,632,500 ns`: unmount `37,264,625`; wait `8,708`; cleanup `42,358,833`; two Docker calls. Public End API was `133,848,334 ns`, leaving additional controller/registry cleanup outside the Docker lifecycle receipt.

### Algorithm, transfer, storage, memory, and I/O counters

Each edit stayed bounded at ten CDC bytes, ten encode/hash invocations, at most ten candidate IDs, and at most 16,908 candidate bytes. The first Push announced 407 IDs/7,664,284 bytes over ten membership pages to send ten IDs/16,908 bytes; the sixteenth announced 1,761 IDs/33,663,628 bytes over 26 pages to send ten IDs/14,988 bytes. Push receipts summed to `2,552,180,167 ns`: history `1,528,334`; frontier `88,027,591`; membership `364,475,165`; source read/auth `9,177,463`; object admission `33,602,957`; fact admission `7,169,209`; authority transition verification `1,944,947,000`; publication `15,091,583`; two-Store durability `74,619,124`; unattributed `13,541,741`; endpoint calls `437`. SQLite object admission was insert-only after fixed-page pre-authentication; sample Commit CAS transactions used two visibility-last statements and sub-millisecond-to-low-millisecond commit/sync.

### Comparison with Computer, previous round, and current best

No paired comparison is valid. Relative to Round 010's obsolete per-edit-Workspace topology, create/end repetition was removed and the reporter no longer hides lifecycle, but protocol and evidence changes prohibit a numeric speedup claim. There is no formal current best.

### Defects and root causes

Commit is dominated by two Docker CLI control invocations per checkpoint, not content, capture, or SQLite. Execution still pays the multi-Docker PID handshake and polling path. Push is root/history-sized: authority validation rechecks the growing owned suffix/root closure, and membership announces the expanding full root even though payload remains ten objects. Workspace create exceeds its 80 ms hard budget because setup/copy/mount readiness uses three Docker calls. End exceeds its 80 ms lifecycle budget and 133 ms public API budget. Rebase currently materializes only already-visible NodeIds, which is correct for the tested normal path but still needs live old-handle/mountinfo evidence in the campaign artifact. Mountinfo capture and recovery BranchStore ID proof are missing from raw custody.

### What needs improvement next

Replace per-Commit Docker CLI pause/resume with the existing persistent proxy/control connection while preserving flush/fence/error semantics. Then remove the execution PID/control round trips and output polling delay. For Push, move authenticated authority transition verification to the old-published-root→new-root frontier and prune equal subtrees before membership; never infer completeness from root presence. Do not optimize fsync first.

### Stable strengths — no improvement currently needed

Exact dirty-range FileMutationBatch behavior, ten-byte candidate bounds, one active Workspace/mount, in-place NodeId/head/base/reader rebase, retained lease, stale-CAS preservation, reconciliation correctness, live-capture no-op, Reference zero-copy, missing-only payload, two-Store durability, shared helper/fixture, SDK terminal execution, exact final/recovery oracles, complete-lifecycle reporting, and append-sealed custody are stable.

### Subagent reviews and reconciled decision

The consolidated audits predicted Docker control amplification, root-sized Push validation, obsolete authority-only reporting, unsafe reload-under-old-mount, Materialize dirty regression, and writer-transaction payload reads. This round confirms the first two quantitatively and closes the reporting, reusable Workspace, dirty reporting, live-capture, and transaction-boundary issues. The next retained optimization is persistent control, not durability or content mutation.

### Next action

Route Commit fence/pause/resume over the already-open proxy connection, add same-mount mountinfo and old-open-handle evidence to the focused artifact, rebuild only LayerFS, and rerun protocol-0.3 focused EDIT16 before touching Push.

## Round 012 — persistent-control-09700086-20260831

- Status: FAILED (evidence PASS; intermediate optimization gate FAILED)
- UTC timestamp: 2026-08-30T19:35:15Z through 2026-08-30T19:35:31Z
- Local timestamp and timezone: 2026-08-31 03:35:15–03:35:31 CST (Asia/Shanghai)
- Git commit and tree: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`
- Dirty source seal and diff hashes: source seal `61a60cf5d0b214095f01869a3ec182db299ebcca1cc7c71d62e7a92824864e01`; unstaged patch `f00110c72aba0dc70ac3e2f51745c782b69bdcd0aba8ad0c19c13f6a00293320`; staged patch empty; status `7958ac6e4a11bf66b0272cd86ff0752b5bd6cc35e5a271efd50cfdd853b707a2`; untracked inventory `54db5efd725d6981c780df98d1039b16dc2f97c2fc8a806419e9b1a057925a3b`
- Benchmark/profile and exact commands: persistent-control proxy test, full SDK v2 suite, warning-denying Clippy, cached LayerFS build, then `benchmark/fs-benchmark-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:7d8495d2894686a95f14beb110fc3f32334e24777910f8256c729e177a18f69e persistent-control-09700086-20260831`
- Candidate order seed and pair count: `7fc94a3b810c7d53e3a1a7c84674481e778116601d6dabf15a96ec149cf63cd5`; one LayerFS-only focused sample; Computer was neither rebuilt nor executed
- Host, kernel, Docker, CPU/memory/I/O envelope: frozen arm64 Docker Desktop one-CPU/1-GiB prepared-container envelope
- Candidate image tags, digests, architectures, and verified OCI labels: LayerFS `sha256:7d8495d2894686a95f14beb110fc3f32334e24777910f8256c729e177a18f69e`; arm64; exact commit/tree/dirty/source-seal labels passed
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `f97d9e5dad5ed0601c1d8cbe5975c170102f7d7c4b64236ec31bf6627f28fd1a`; recovery `ba0b65408a884ea42b19c3f2bec06a6e43c5ad1860234ca5e38d08f79f4d6b44`; mount `/workspace/fs-benchmark-pro-7`; mountinfo not yet captured; FUSE helper `74c02ac7bd9590db36d5b8e1b13a84e29fb24b64bf8bfccaef88bf4226cd8cb6`; workload helper unchanged
- Raw evidence directory and SHA-256 inventory: `runs/persistent-control-09700086-20260831/`; every inventory entry reverified; inventory SHA-256 `043eed7482a7550730f31727a92175e762e28ed35f03d56b156443572e69b527`
- Previous comparable round: Round 011 protocol-0.3 focused baseline
- Current best comparable round: Round 012 is the best focused protocol-0.3 mechanism result; no paired/formal current best exists

### Hypothesis and planned change

An authenticated persistent helper-to-controller control stream can execute the existing ProxyClient pause/fence/resume logic without spawning Docker CLI for every checkpoint.

### Changes since the previous round

Proxy connections now authenticate a distinct data or control role under the same capability. The helper opens one persistent control stream before mount readiness. `DockerProjection` uses that stream for pause/resume; the existing ProxyClient still flushes pending work, fences the data stream, blocks new requests while paused, and resumes the same mount. The Unix control path remains available. SQL trace state became thread-local so parallel tests cannot contaminate transaction-boundary assertions.

### Correctness and validity

Evidence PASS: the proxy test proved capability isolation, paused request rejection, and successful resume; the full SDK v2 suite, reusable Workspace tests, exact SQL writer-boundary proof, Clippy, final/reopen oracles, SDK terminals, live-capture zeros, and two-Store durability passed. This is still a one-sample LayerFS-only focused run with incomplete mount/custody/scenario/statistical evidence.

### Comparable E2E results

No Computer comparison is claimed. EDIT16 complete lifecycle was `4,820,427,632 ns`, down `1,337,160,531 ns` (21.71%) from Round 011. Components: one create `112,521,084`; SDK exec dispatch `1,421,922,919`; output-to-terminal `528,024,792`; Commit `126,716,918`; Push/two-Store durability `2,527,641,877`; one End `103,600,042`. The exact component sum equals the total. Commit is now `7,919,807 ns/edit`, inside the preferred 8–15 ms/edit budget and hard 20 ms/edit budget. The complete `<3.10 s` intermediate and `<=1.20 s` terminal gates still failed.

### LayerFS phase decomposition

Receipted Commit summed to `120,253,084 ns`: pause/fence `2,216,211`; live capture `7,584`; candidate plan `11,483,167`; dirty compare `25,083`; content `31,945,290`; namespace `7,388,583`; candidate finish `1,271,831`; local admission `22,731,248`; completeness verification `0`; publication/CAS `13,564,125`; in-place rebase `26,747,750`; resume `1,024,291`; unattributed `1,845,005`. Persistent control reduced pause+resume from Round 011's `1,235,062,878 ns` to `3,240,502 ns` (99.74%) and reduced total receipted Commit by 91.13%.

Attach was `109,470,167 ns` across three Docker calls; End lifecycle was `94,660,334 ns` across two Docker calls. Both remain above preferred budgets and public End remained `103,600,042 ns`.

### Algorithm, transfer, storage, memory, and I/O counters

All edits retained ten CDC bytes, at most ten candidate IDs, and at most 16,908 candidate bytes. First/last Push amplification remained 407 IDs/7.66 MiB/10 pages and 1,761 IDs/33.66 MiB/26 pages respectively, while sending ten objects. Push phase totals were `2,520,128,418 ns`: history `1,460,875`; frontier `89,318,813`; membership `368,258,629`; source read/auth `9,407,417`; object admission `33,287,334`; fact admission `6,577,959`; authority transition verification `1,907,282,254`; publication `15,117,501`; durability `75,978,667`; unattributed `13,438,969`; endpoint calls `437`.

### Comparison with Computer, previous round, and current best

The single focused method is comparable to Round 011 and confirms the control optimization. It is not comparable to protocol-0.2 rows or Computer. Round 012 becomes the focused mechanism best, not a publishable performance best.

### Defects and root causes

Commit control amplification is fixed. EDIT16 is now dominated by Push (52.44%), exec dispatch (29.50%), and output wait (10.95%). Execution still performs multiple Docker CLI PID/ready round trips, while terminal collection polls at a 20 ms cadence. Push remains dominated by growing authority transition verification and root-sized membership. Create/End still use five Docker calls in total. Mountinfo/old-handle evidence is present in tests but not copied into this run's raw artifact.

### What needs improvement next

Remove the execution PID/ready Docker handshakes while preserving process-group stop semantics, replace the 20 ms terminal polling cadence with prompt completion, and rerun focused. Then optimize authenticated Push transition verification/frontier pruning. Do not perturb the now-successful Commit path.

### Stable strengths — no improvement currently needed

Persistent control fencing, sub-8-ms Commit, live-capture no-op, exact dirty-range candidate behavior, safe same-Workspace rebase, Reference zero-copy, missing-only payload, two-Store durability, complete reporting, exact oracles/recovery, and append-sealed custody are stable.

### Subagent reviews and reconciled decision

The audit's control-round-trip prediction is confirmed and resolved without a benchmark-only semantic path. The measured root cause has moved to execution and Push. Persistent control is retained.

### Next action

Optimize the shared Workspace execution transport and terminal wait, rebuild only LayerFS, and require a focused reduction before changing Push.

## Round 013 — execution-fast-09700086-20260831

- Status: FAILED (evidence PASS; intermediate optimization gate FAILED)
- UTC timestamp: 2026-08-30T19:40:51Z through 2026-08-30T19:41:05Z
- Local timestamp and timezone: 2026-08-31 03:40:51–03:41:05 CST (Asia/Shanghai)
- Git commit and tree: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`
- Dirty source seal and diff hashes: source seal `fe90ac9dfdf40fe96bdd07aabce78b7d5e2230709a9d721a17447b5d0bf5783f`; unstaged patch `889c080154e988ea56582f139a4c02bb55ee217402560d7c0da2da45385ccfe0`; staged patch empty; status `9cd9acf327f9334061a73d7fd90c3b2fd2f95501c2bc6a7a3fa78d6f995222f3`; untracked inventory `bea13d61721ddbe63c704daff6a137dc513960e9b24c010d5cd44b2a03a88411`
- Benchmark/profile and exact commands: focused execution command-shape test, full SDK v2 tests, warning-denying Clippy, cached LayerFS build, then `benchmark/fs-benchmark-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:8aef9779a15c884bd1fc1a356065d6eb0ddc6398f082f15d0723ac942ceb3992 execution-fast-09700086-20260831`
- Candidate order seed and pair count: `15a2f14f44c3644e895fb80eb704117cadb962108fe860423f1fc8e3480cffa0`; one LayerFS-only focused sample; Computer was neither rebuilt nor executed
- Host, kernel, Docker, CPU/memory/I/O envelope: frozen arm64 Docker Desktop one-CPU/1-GiB prepared-container envelope
- Candidate image tags, digests, architectures, and verified OCI labels: LayerFS `sha256:8aef9779a15c884bd1fc1a356065d6eb0ddc6398f082f15d0723ac942ceb3992`; arm64; source seal `fe90ac9dfdf40fe96bdd07aabce78b7d5e2230709a9d721a17447b5d0bf5783f`; exact labels passed
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `36508cefaa44b7b2bb3f9df7123eb9772a444144944dbc29e9ccd1494b78960a`; recovery `4fd777fe24c539643136ccb2af3847d7eb567ad463ffd230d168798d54648dac`; mount `/workspace/fs-benchmark-pro-7`; mountinfo not captured; helper hashes unchanged from Round 012
- Raw evidence directory and SHA-256 inventory: `runs/execution-fast-09700086-20260831/`; every inventory entry reverified; inventory SHA-256 `4ab328c351dbeb24f1591b42f39ba0c4418237029278d672896633f23a68e8b1`
- Previous comparable round: Round 012
- Current best comparable round: Round 013 is the focused protocol-0.3 mechanism best; no paired/formal best exists

### Hypothesis and planned change

Starting the container process immediately and resolving its process group lazily only for Stop should delete two Docker CLI handshakes per execution; a 2 ms supervisor cadence should reduce terminal detection delay.

### Changes since the previous round

Container execution now launches with one Docker CLI process. The in-container shell writes its unique group PID and immediately runs the command; it no longer waits for a controller-created ready file. Stop first checks child completion, then uses the PID file to signal the exact process group. The supervisor cadence changed from 20 ms to 2 ms. A command-shape check rejects reintroducing the ready handshake, and the live Docker test includes public Stop/terminal proof when its environment is enabled.

### Correctness and validity

Evidence PASS: all executed tests/Clippy, exact workload oracles, 16 same-Workspace Commit/Push checkpoints, SDK terminal receipts, capture zeros, durability, and recovery passed. Stop is compiled and has a live-environment assertion but was not exercised in this focused run. Formal validity gaps remain unchanged.

### Comparable E2E results

No Computer comparison is claimed. EDIT16 complete lifecycle was `3,792,377,252 ns`, down `1,028,050,380 ns` (21.33%) from Round 012. Components: create `121,273,708`; exec dispatch `14,450,376`; first OutputReader read to terminal `888,857,583`; Commit `123,685,127`; Push/durability `2,526,669,750`; End `117,440,708`. Exec dispatch improved by 98.98%; combined exec/output improved from `1,949,947,711 ns` to `903,307,959 ns` (53.68%). The time moved to Output correctly because public exec now returns before the single Docker process starts and completes. The `<3.10 s` intermediate gate still failed by `692,377,252 ns`; hard/preferred gates also failed.

### LayerFS phase decomposition

Receipted Commit remained stable at `117,908,163 ns`: pause `2,123,207`; live capture `8,541`; planning `12,293,666`; content `30,965,041`; local admission `22,061,663`; publication `12,723,750`; rebase `26,882,376`; resume `1,058,668`; unattributed `1,774,168`. Commit API stayed within the preferred per-edit budget.

### Algorithm, transfer, storage, memory, and I/O counters

Ten-byte candidate/capture/durability bounds remained stable. Push stayed `2,526,669,750 ns` API / `2,519,277,541 ns` receipted: authority transition verification `1,899,658,958`; membership `371,488,524`; frontier `89,102,907`; durability `75,169,253`; unattributed `13,236,381`. Root-sized first/last announcement amplification and 437 endpoint calls remained unchanged.

### Comparison with Computer, previous round, and current best

Round 013 is directly comparable only to Rounds 011–012 focused protocol-0.3 runs. It becomes the focused mechanism best. No paired claim is made.

### Defects and root causes

Handshake amplification is fixed, but a fresh Docker CLI/executor process is still created for every command. Its startup and workload completion are now visible in output-to-terminal time, which exceeds the hard 80 ms total budget by more than 11x. Push remains 66.62% of EDIT16 and is dominated by growing authority transition verification. Create/End remain over budget.

### What needs improvement next

Use a persistent in-container executor transport for repeated Workspace commands while preserving argv boundaries, process-group Stop, stdout/stderr framing, exit status, and public receipts. Independently, the measured Push transition verifier must move from full growing history/root verification to an authenticated one-commit old-root→new-root proof. The latter alone is sufficient to cross the intermediate gate and is the next smallest measured root cause.

### Stable strengths — no improvement currently needed

Immediate exec dispatch, lazy exact-group Stop metadata, persistent FUSE control, sub-8-ms Commit, safe rebase, capture no-op, candidate bounds, missing-only payload, durability, reporting, and recovery are stable.

### Subagent reviews and reconciled decision

The execution audit's three-round-trip diagnosis is confirmed and resolved. The remaining output time is not reporter error; it is real per-command Docker process startup/completion. Round 013 is retained.

### Next action

Optimize authenticated Push authority transition verification/frontier pruning first to cross the intermediate gate, then replace per-command Docker execution with a persistent executor for the hard terminal budget.

## Round 014 — transition-proof-09700086-20260831

- Status: FAILED (evidence PASS; intermediate gate PASS; hard terminal gate FAILED)
- UTC timestamp: 2026-08-30T19:47:58Z through 2026-08-30T19:48:11Z
- Local timestamp and timezone: 2026-08-31 03:47:58–03:48:11 CST (Asia/Shanghai)
- Git commit and tree: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`
- Dirty source seal and diff hashes: source seal `1102c4298326328ab7bb8ed56ef76b55738919063a5051fa447ee03cb282ab6f`; unstaged patch `4df66d83f8df580b310f6146e4b10f6de0b215e791a1433f7b6420eef1ff0fef`; staged patch empty; status `a6251bd50b111a531d4572e11e1d30c91e243ae399b0ac82bff37d1cf5b28c05`; untracked inventory `ccb8d87f0dfadb192e3d8d5948368c6e5e2868d13920f9ba72cf501270599184`
- Benchmark/profile and exact commands: transition missing-frontier and suffix-only tests, BranchStore/LayerStack tests, warning-denying Clippy, cached LayerFS build, then `benchmark/fs-benchmark-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:615e20fe017c3ef9274acadc3733a655c343d0d2bd3e5a3f07ef9c2c38fadb64 transition-proof-09700086-20260831`
- Candidate order seed and pair count: `4c3afa58a46a68539dfc08d9606aa52720b2f81025d9f0777d096f0570d39d6c`; one LayerFS-only focused sample; Computer was neither rebuilt nor executed
- Host, kernel, Docker, CPU/memory/I/O envelope: frozen arm64 Docker Desktop one-CPU/1-GiB prepared-container envelope
- Candidate image tags, digests, architectures, and verified OCI labels: LayerFS `sha256:615e20fe017c3ef9274acadc3733a655c343d0d2bd3e5a3f07ef9c2c38fadb64`; arm64; exact source labels and seal `1102c4298326328ab7bb8ed56ef76b55738919063a5051fa447ee03cb282ab6f` passed
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `0e3a92ce4cab22c2a511d0b87575459a344a3c9ccf1160767ac0385a7b57d37c`; recovery `48c37cd948a927821830d5d3119ea373386999ba12260d78d997cdeacfdc4e03`; mount `/workspace/fs-benchmark-pro-7`; mountinfo not captured; helper hashes unchanged
- Raw evidence directory and SHA-256 inventory: `runs/transition-proof-09700086-20260831/`; every inventory entry reverified; inventory SHA-256 `b6b0967d2aa610bf40d821867a9a4f8a18fb3890849fb94dcbc8b525da1ac300`
- Previous comparable round: Round 013
- Current best comparable round: Round 014 is the focused protocol-0.3 mechanism best; no paired/formal best exists

### Hypothesis and planned change

An already-published authority root is a trusted complete closure. Pairing it with the new root and recursively pruning equal content-addressed subtrees should verify only the authenticated new frontier while still rejecting any missing unequal object.

### Changes since the previous round

Authority validation now collects the exact new owned Commit suffix. First publication still performs full root verification. An advance starts from the observed published Commit root, verifies each new root in chronological order, authenticates unequal new objects, prunes equal old subtrees, and fully traverses unmatched additions. The verifier never treats target root presence as completeness. A focused test withholds a genuinely new frontier object and requires failure; the existing suffix-only test corrupts a historical leaf and proves an advance does not revalidate it.

### Correctness and validity

Evidence PASS: transition correctness tests, all relevant Store tests, Clippy, 16 same-Workspace checkpoints, candidate/capture bounds, durability, exact final digest, and fresh recovery passed. This remains focused, single-arm, and non-formal with the same custody gaps.

### Comparable E2E results

No Computer comparison is claimed. EDIT16 complete lifecycle was `1,925,014,876 ns`, down `1,867,362,376 ns` (49.24%) from Round 013 and down 68.71% from Round 011. Components: create `140,044,250`; exec dispatch `13,250,542`; output-to-terminal `831,432,292`; Commit `123,015,040`; Push/durability `698,215,460`; End `119,057,292`. The intermediate `<3.10 s` gate passed with 1.175 s margin. The hard `<=1.20 s` gate failed by `725,014,876 ns`; preferred `<=0.80 s` failed by 1.125 s.

### LayerFS phase decomposition

Commit stayed stable at `117,117,043 ns` receipted / `123,015,040 ns` API, within the preferred per-edit budget. No capture, candidate, admission, publication, or rebase regression appeared.

### Algorithm, transfer, storage, memory, and I/O counters

Push API fell from `2,526,669,750 ns` to `698,215,460 ns` (72.37%). Receipted Push was `691,964,710 ns`: authority transition verification `280,937,459` versus Round 013's `1,899,658,958` (85.21% lower); membership `199,618,363`; frontier `86,261,613`; source/auth `8,821,880`; object admission `32,764,043`; fact admission `6,511,957`; publication `8,730,415`; durability `57,319,580`; unattributed `9,531,027`; endpoint calls remained `437`. Candidate and missing-only payload bounds remained exact. Transfer still announces the full growing root before membership, so this round fixes authority verification but not transfer traversal.

### Comparison with Computer, previous round, and current best

Round 014 is directly comparable to Round 013 focused protocol-0.3 evidence and becomes the focused mechanism best. It is the first run to pass the intermediate LayerFS-only gate. No paired speedup claim is made.

### Defects and root causes

Authority full-closure verification is fixed. Output/executor startup is now 43.19% of EDIT16 and Push remains 36.27%. Transfer membership/frontier still consumes ~286 ms and 437 endpoint calls because BranchStore walks the full new root and asks authority membership before recognizing equal old subtrees. Create/End together remain ~259 ms.

### What needs improvement next

Apply the same authenticated old-root→new-root equal-subtree proof on the transfer source before authority membership. Buffer only the bounded new frontier using the existing 8-MiB spillable structures, issue one sorted bounded membership page for a ten-object edit, and preserve postorder admission. Then address the persistent executor.

### Stable strengths — no improvement currently needed

Transition completeness proof, historical-suffix isolation, persistent control, Commit budget, immediate dispatch, safe rebase, capture/candidate bounds, missing-only payload, durability, exact reporting, and recovery are stable.

### Subagent reviews and reconciled decision

The audit's requested old-published-root→new-root proof is now implemented for authority verification and quantitatively confirmed. The same proof must move earlier into transfer discovery; root presence remains forbidden as a shortcut.

### Next action

Prune equal old/new subtrees before transfer membership, rebuild only LayerFS, and rerun focused before persistent-executor work.

## Round 015 — transfer-transition-09700086-20260831

- Status: FAILED (evidence PASS; intermediate gate PASS; hard terminal gate FAILED)
- UTC timestamp: 2026-08-30T19:53:35Z through 2026-08-30T19:53:47Z
- Local timestamp and timezone: 2026-08-31 03:53:35–03:53:47 CST (Asia/Shanghai)
- Git commit and tree: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`
- Dirty source seal and diff hashes: source seal `28664061ff5dddb3be5bf0f3104605f50c522d32348db33f7d02604aeeaf5c0b`; unstaged patch `43218bece2613f8aa975a833f9d13794538da05f94f71264c8a4c5ac383c470f`; staged patch empty; status `a6251bd50b111a531d4572e11e1d30c91e243ae399b0ac82bff37d1cf5b28c05`; untracked inventory `bc60a02bcb7bedc9127b90158028fc5695d1b82deedddce8fcb8b5d44d39adfa`
- Benchmark/profile and exact commands: transition-transfer missing-frontier/one-page test, full BranchStore/LayerStack tests, warning-denying Clippy, cached LayerFS build, then `benchmark/fs-benchmark-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:037014f44d16746940d2be4e82c96dcb237236a59fec372e74ede5011ea47d11 transfer-transition-09700086-20260831`
- Candidate order seed and pair count: `756c83f7717b712ec7188880bd1791b83b093bab9419d6c83436df9de7730124`; one LayerFS-only focused sample
- Host, kernel, Docker, CPU/memory/I/O envelope: frozen arm64 Docker Desktop one-CPU/1-GiB prepared-container envelope
- Candidate image tags, digests, architectures, and verified OCI labels: LayerFS `sha256:037014f44d16746940d2be4e82c96dcb237236a59fec372e74ede5011ea47d11`; arm64; source labels/seal passed
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `3b27030a9f42494a7815c5df55500566b1fc83e24af1f6766796657fea4cbc17`; recovery `16d6cf626f92750d47b773ac5f182ce5e8139537f6ec28e7616ac41140ebac76`; mount path unchanged; mountinfo missing; helper hashes unchanged
- Raw evidence directory and SHA-256 inventory: `runs/transfer-transition-09700086-20260831/`; every entry reverified; inventory SHA-256 `ce4a76569335130f22574897eff1c4c107dfeb04d3a64b9cefc62fdb39b35da7`
- Previous comparable round: Round 014
- Current best comparable round: Round 015 is nominally the lowest focused total by 19.5 ms, but Round 014 remains the cleaner transition baseline until pairing is corrected

### Hypothesis and planned change

Collecting the unequal old/new object frontier before membership should reduce root-sized authority queries to bounded frontier pages while retaining postorder admission and the 8-MiB spill ceiling.

### Changes since the previous round

BranchStore advances now pair the observed authority root with each new Commit root, collect unequal new objects into the existing memory-first/spillable deferred store, batch sorted IDs for membership, and stage only missing objects in postorder. First publication still uses full transfer. Tests require missing-frontier failure, at least one trusted-subtree prune, one membership page in the focused model, and complete target reconstruction.

### Correctness and validity

Evidence PASS: all targeted transfer, authority repair, branch history, refinement, Clippy, live focused, final/reopen, and durability checks passed. Formal gaps remain unchanged.

### Comparable E2E results

No Computer comparison is claimed. EDIT16 was `1,905,537,380 ns`, only `19,477,496 ns` (1.01%) below Round 014. Components: create `133,117,417`; exec `15,396,545`; output `864,393,376`; Commit `123,278,581`; Push `673,866,169`; End `95,485,292`. Intermediate gate passed; hard `<=1.20 s` failed by `705,537,380 ns`.

### LayerFS phase decomposition

Commit remained stable and within budget. No content, capture, admission, or rebase regression appeared.

### Algorithm, transfer, storage, memory, and I/O counters

Push endpoint calls fell from 437 to 141 and membership time from `199,618,363 ns` to `17,877,542 ns`. The last edit announced 244 IDs/4,498,692 bytes over two pages and pruned 112 equal subtrees, versus Round 014's 1,761 IDs/33.66 MiB/26 pages. It still sent exactly ten missing objects. However source read/auth rose to `258,460,660 ns`, authority transition verification remained `204,693,541 ns`, and Push unattributed rose to `80,176,504 ns`. First edit remains a full 407-ID/10-page publication because no authority Branch root exists yet.

### Comparison with Computer, previous round, and current best

The small total change is within one-sample noise. Mechanism counters prove fewer membership calls, but the implementation is not yet the intended bounded frontier. No paired claim is made.

### Defects and root causes

The transition collector sorted child object IDs by hash before pairing unmatched children. Persistent tree children are positionally aligned; hash sorting destroys that alignment, forcing traversal of otherwise equal subtrees. This converts saved membership time into source reads and verifier work. The fix is to preserve authenticated codec/reference order, prune common IDs by set membership, and pair remaining children in original order.

### What needs improvement next

Preserve child reference order in both transition transfer and transition completeness verification. Re-run the same focused sample and require frontier announcements near the ten-object candidate, one membership page after the first edit, lower source/auth, and no correctness regression.

### Stable strengths — no improvement currently needed

Spill-bounded frontier buffering, postorder missing-only sends, first-publish full verification, incomplete-authority repair, historical isolation, and all prior stable mechanisms remain correct.

### Subagent reviews and reconciled decision

The pre-membership pruning direction is retained; hash-sorted pairing is rejected as the measured implementation defect.

### Next action

Correct transition pairing order, rebuild only LayerFS, and rerun before persistent executor work.

## Round 016 — ordered-transition-09700086-20260831

- Status: FAILED (evidence PASS; intermediate gate PASS; hard terminal gate FAILED)
- UTC timestamp: 2026-08-30T19:58:38Z through 2026-08-30T19:58:50Z
- Local timestamp and timezone: 2026-08-31 03:58:38–03:58:50 CST (Asia/Shanghai)
- Git commit and tree: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`
- Dirty source seal and diff hashes: source seal `0bfe708258bd60bd230d0e1c414a6261de44cfd4309f6ab6db70c02265c12a8d`; unstaged patch `f926ee6e4f9bae1efa2cd5f44dfb044d814c67ff564ff4e281fd8e6c750f12f5`; staged patch empty; status `a6251bd50b111a531d4572e11e1d30c91e243ae399b0ac82bff37d1cf5b28c05`; untracked inventory `ce3d71e573812d23af57b76b70517fadafa320b8ddec56f382e9ec352a205768`
- Benchmark/profile and exact commands: focused transition-verifier and suffix tests, complete BranchStore/LayerStack suites, warning-denying Clippy, cached LayerFS-only build, then `benchmark/fs-benchmark-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:5dd0f3241e4b6f545c7f2e35c37c5dd3096987177984bf15914afc52b33c7880 ordered-transition-09700086-20260831`
- Candidate order seed and pair count: `17870ede3f1b5b1efad5c29b1b6ca9e83fee2e931c00557f80466eb939a023a7`; one LayerFS-only focused sample; Computer was neither rebuilt nor executed
- Host, kernel, Docker, CPU/memory/I/O envelope: frozen arm64 Docker Desktop one-CPU/1-GiB prepared-container envelope
- Candidate image tags, digests, architectures, and verified OCI labels: LayerFS `sha256:5dd0f3241e4b6f545c7f2e35c37c5dd3096987177984bf15914afc52b33c7880`; arm64; exact commit/tree/dirty/source-seal labels passed
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `81f8cfb5f1ddefe283ae2dc0387ab97277d3d3b044b9115860ecc9da88858749`; recovery `ed7a5c57b4de978d344a8fbcb5177b65699be66c86dfacea05bf84a9d2b9ba21`; mount `/workspace/fs-benchmark-pro-7`; mountinfo not yet captured; FUSE helper `74c02ac7bd9590db36d5b8e1b13a84e29fb24b64bf8bfccaef88bf4226cd8cb6`; workload helper `e20271d4c39f3ac974eddfae82bcfaffd9e5b6877131103fffb04fd319a87fe6`
- Raw evidence directory and SHA-256 inventory: `runs/ordered-transition-09700086-20260831/`; 40 inventory entries reverified; inventory SHA-256 `c6abc6283deb2c3ec02a7496b646c2b3311ff378b7489c66f3f5c0faa8b50843`
- Previous comparable round: Round 015
- Current best comparable round: Round 016 is the focused protocol-0.3 mechanism best; no paired/formal best exists

### Hypothesis and planned change

Preserving authenticated child-reference order while deduplicating should keep positionally aligned old/new subtrees paired, reducing false frontier expansion without weakening exact-ID pruning.

### Changes since the previous round

Both transfer-frontier collection and authority transition verification now deduplicate child references with an order-preserving set instead of sorting hashes before pairing. No protocol, transaction, candidate, or lifecycle code changed.

### Correctness and validity

Evidence PASS: the missing-frontier transition proof, suffix-only authority proof, complete BranchStore/LayerStack suites, warning-denying Clippy, exact 16-checkpoint workload, final/reopen oracles, SDK terminals, live-capture zeros, two-Store durability, and fresh recovery passed. This remains a single focused sample; mountinfo, full scenario coverage, balanced pairs, and formal statistics remain incomplete.

### Comparable E2E results

No Computer comparison is claimed. EDIT16 complete lifecycle was `1,775,183,045 ns`, down `130,354,335 ns` (6.84%) from Round 015 and down 71.17% from Round 011. Components: one create `123,175,709`; SDK exec dispatch `15,310,916`; first OutputReader read to terminal `863,234,293`; Commit `124,856,000`; Push/two-Store durability `530,382,919`; one End `118,223,208`. The exact component sum equals the total. The `<3.10 s` intermediate gate passed; the hard `<=1.20 s` gate failed by `575,183,045 ns`, and the preferred `<=0.80 s` target failed by `975,183,045 ns`.

### LayerFS phase decomposition

Receipted Commit was `118,590,668 ns` (`7,411,917 ns/edit`): pause/fence `2,225,332`; live capture `6,791` with `capture_mode=live`, zero files, and zero bytes; candidate plan `10,422,622`; dirty compare `22,082`; content `31,858,417`; namespace `6,764,791`; candidate finish `1,330,083`; local admission `22,957,042`; completeness verification `0`; publication/CAS `12,631,624`; in-place rebase `27,554,582`; resume `1,066,414`; unattributed `1,745,760`. Commit remains within the preferred and hard per-edit budgets.

### Algorithm, transfer, storage, memory, and I/O counters

Receipted Push was `523,653,001 ns`: history `1,502,957`; frontier `1,509,578`; membership `15,184,540`; source/auth `197,795,626`; object admission `33,218,957`; fact admission `6,327,917`; authority transition verification `159,734,752`; publication `6,595,501`; durability `54,242,754`; unattributed `47,540,419`; endpoint calls `131`. The last edit announced 150 IDs/2,628,108 bytes over two membership pages, pruned 207 equal subtrees, and sent exactly ten missing IDs/14,988 bytes. The first edit still announced the full 407 IDs/7,664,284 bytes over ten pages, while sending ten missing IDs/16,908 bytes.

### Comparison with Computer, previous round, and current best

Round 016 is directly comparable to Round 015 and confirms that reference-order preservation removed part of the false frontier. It becomes the focused mechanism best. It is not a paired Computer result or a formal performance claim.

### Defects and root causes

The sort-order defect is fixed, but positional child pairing still expands to 150 IDs on edit 16 because object-reference lists are not a semantic child-alignment API. First publication remains root-sized because it does not transition from the trusted fork layer. Output remains `863,234,293 ns` because every public exec starts a fresh Docker CLI/container process; create and End remain above hard budget with five lifecycle Docker calls total.

### What needs improvement next

Use the trusted fork-layer root for the first Branch publication so its transfer and authority verification can use the same exact transition proof. Then replace per-command Docker execution with one persistent Workspace executor, preserving argv, stdout/stderr, terminal status, Stop semantics, and the single active-execution rule. Do not perturb the healthy Commit path.

### Stable strengths — no improvement currently needed

Order-preserving transition verification, bounded spillable transfer, missing-only payload, safe reusable Workspace rebase, persistent FUSE control, sub-8-ms Commit, live-capture no-op, exact candidate behavior, two-Store durability, exact oracles/recovery, and complete-lifecycle reporting remain stable.

### Subagent reviews and reconciled decision

The audit's root-sized Push and Docker execution diagnoses remain confirmed. The smallest next Push fix reuses existing trusted fork-layer state; the persistent executor is required for the hard terminal target.

### Next action

Prune first publication against the trusted fork layer, seal and rerun, then implement the persistent executor against that new measured baseline.

## Round 017 — origin-transition-09700086-20260831

- Status: FAILED (evidence PASS; intermediate gate PASS; hard terminal gate FAILED)
- UTC timestamp: 2026-08-30T20:04:57Z through 2026-08-30T20:05:09Z
- Local timestamp and timezone: 2026-08-31 04:04:57–04:05:09 CST (Asia/Shanghai)
- Git commit and tree: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`
- Dirty source seal and diff hashes: source seal `a68257ae59f8666232bac8d58f2f2734592783012710af5ad813202fbc44786f`; unstaged patch `f03a871a9305724dd5a7d8104020338c1bad657018a5e4e15680fdce3fb9c40a`; staged patch empty; status `a6251bd50b111a531d4572e11e1d30c91e243ae399b0ac82bff37d1cf5b28c05`; untracked inventory `9038dec5cb994c1a3dac8aef048f160b5cd721f7d02acc8dfcf65943cda53ef0`
- Benchmark/profile and exact commands: first-Reference-Push pruning contract, corrupted-local-closure rejection, inherited-history transition test, complete BranchStore/LayerStack suites, warning-denying Clippy, cached LayerFS-only build, then `benchmark/fs-benchmark-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:96a86b87c949e215bf04b04013984c79d19185fae6f699af66c30052639bc861 origin-transition-09700086-20260831`
- Candidate order seed and pair count: `3849e5d4fc134cfc44e586689c8e316ce5023572c17a6c4e532075fd81126c97`; one LayerFS-only focused sample; Computer was neither rebuilt nor executed
- Host, kernel, Docker, CPU/memory/I/O envelope: frozen arm64 Docker Desktop one-CPU/1-GiB prepared-container envelope
- Candidate image tags, digests, architectures, and verified OCI labels: LayerFS `sha256:96a86b87c949e215bf04b04013984c79d19185fae6f699af66c30052639bc861`; arm64; exact commit/tree/dirty/source-seal labels passed
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `97edd3e5a3178dae284815d8834929d6ba21bbaaa8e103ef470721f02fa9627f`; recovery `f52d54616eefbb85f59d318ccc259057c8abb588871f1b0db38291576a277287`; mount `/workspace/fs-benchmark-pro-7`; mountinfo not yet captured; FUSE helper `74c02ac7bd9590db36d5b8e1b13a84e29fb24b64bf8bfccaef88bf4226cd8cb6`; Python workload helper `e20271d4c39f3ac974eddfae82bcfaffd9e5b6877131103fffb04fd319a87fe6`
- Raw evidence directory and SHA-256 inventory: `runs/origin-transition-09700086-20260831/`; 40 entries reverified; inventory SHA-256 `cacb65e2a3b4a2f8fcd278972e16336b6d45c364871e90e04ad98f2abab9b86f`
- Previous comparable round: Round 016
- Current best comparable round: Round 017 is the focused protocol-0.3 mechanism best; no paired/formal best exists

### Hypothesis and planned change

The authority-complete immutable fork origin is the correct trusted old boundary before a Branch has an observed authority head. Reusing it should remove first-publish full-root traversal while preserving missing-local-closure rejection for locally complete snapshots.

### Changes since the previous round

Reference Push now starts its chronological transitions from the observed authority Commit, fork-origin Commit, or fork-origin Layer root. Authority validation independently starts from the same admitted origin. A locally complete source still takes the full local-verification path so a stale completeness receipt cannot mask a deleted object through authority fallback. Tests cover both origin pruning and corrupted Replica rejection.

### Correctness and validity

Evidence PASS: first-Reference-Push pruning, exact authority origin ownership, corrupted locally receipted closure rejection, inherited Commit suffixes, full store suites, Clippy, 16 reusable checkpoints, exact final/reopen, capture zeros, durability, and fresh recovery passed. Sender and receiver still independently traverse the unequal frontier; a bounded receiver-validated transition-proof format is not yet implemented. Formal and mountinfo gaps remain.

### Comparable E2E results

No Computer comparison is claimed. EDIT16 complete lifecycle was `1,647,091,790 ns`, down `128,091,255 ns` (7.22%) from Round 016 and down 73.25% from Round 011. Components: create `122,766,833`; exec dispatch `13,900,290`; OutputReader wait `852,080,624`; Commit `119,981,210`; Push/two-Store durability `436,647,083`; End `101,715,750`. The exact component sum equals the total. Intermediate `<3.10 s` passed. Hard `<=1.20 s` failed by `447,091,790 ns`; preferred `<=0.80 s` failed by `847,091,790 ns`.

### LayerFS phase decomposition

Receipted Commit was `113,819,749 ns` (`7,113,734 ns/edit`): pause/fence `2,027,875`; live capture `7,166` with zero files/bytes; plan `9,976,084`; dirty compare `23,125`; content `30,205,125`; namespace `6,615,330`; candidate finish `1,295,957`; local admission `22,453,289`; completeness verification `0`; publication/CAS `12,367,247`; in-place rebase `26,171,290`; resume `932,919`; unattributed `1,740,093`. Commit/capture/DB remain healthy.

### Algorithm, transfer, storage, memory, and I/O counters

Receipted Push was `430,192,126 ns`: history `1,474,710`; membership `7,663,500`; source/auth `212,282,121`; object admission `33,310,958`; fact admission `7,228,336`; authority transition verification `60,844,496`; publication `5,862,959`; durability `50,524,420`; unattributed `51,000,626`; endpoint calls `123`. First Push fell from Round 016's `123,919,750 ns` receipted to `42,932,417 ns`, announced 217 IDs/4,144,728 bytes over two pages, pruned 191 equal-ID subtrees, and sent ten missing objects/16,908 bytes. Last Push remained 150 IDs/2,628,108 bytes over two pages, 207 prunes, and ten missing objects/14,988 bytes.

### Comparison with Computer, previous round, and current best

Round 017 is directly comparable to Round 016 and confirms the fork-origin boundary. It becomes the focused mechanism best. It is not a paired Computer or formal performance claim.

### Defects and root causes

First publication no longer scans the full root, but the sender's reference-list pairing is not an exact semantic positional frontier: first/last still announce 217/150 IDs for ten new objects. The receiver repeats frontier traversal across the trust boundary rather than validating a bounded proof. Push is 36.65 ms over its hard 400 ms total budget. The largest remaining headline is 852 ms of fresh Python interpreter plus Docker exec process startup/runtime/drain; create and End also exceed hard budgets.

### What needs improvement next

Per the binding user correction, do not add a persistent or resident executor. Replace Python with one sealed native helper copied byte-identically into both candidates, add prepared-container `/bin/true` and native pwrite+fsync SDK diagnostics, retain one exact short-lived process per Exec, and add spawn/runtime/drain/terminal receipts. Then remove redundant shell/CLI/handshake work on the existing path. Separately, replace heuristic reference pairing with a bounded receiver-validated positional transition proof.

### Stable strengths — no improvement currently needed

Trusted origin selection, locally complete corruption rejection, missing-only payload, bounded spill, reusable Workspace/FUSE rebase, capture no-op, sub-8-ms Commit, two-Store durability, SDK output/Stop semantics, complete reporting, and recovery remain stable.

### Subagent reviews and reconciled decision

The audit's first-Push diagnosis is fixed and measured. The user correction supersedes the earlier persistent-executor idea: no daemon, worker, resident queue, or generalized FUSE command channel will be built.

### Next action

Implement and seal the native shared workload plus `/bin/true` and native pwrite+fsync split diagnostics before changing execution transport.

## Round 018 — native-bash-baseline-09700086-20260831

- Status: INVALID (workload/oracle/recovery PASS; raw mountinfo custody FAILED; no baseline accepted)
- UTC timestamp: 2026-08-30T20:33:45Z through 2026-08-30T20:34:50Z
- Local timestamp and timezone: 2026-08-31 04:33:45–04:34:50 CST (Asia/Shanghai)
- Git commit and tree: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`
- Dirty source seal and diff hashes: source seal `605cc398ffe96a3280207e35806acce051a4b807c8802e11a80e39bc11fc195e`; unstaged patch `6a5e5622b07d481bb1f3785db740ff1d9e54af5ecaa0ef9b5f80e8e2fab7e7ce`; staged patch empty; status `783f32028283ed3e3cb5df9db10e13d510f5275ca262e716c899c4e79690a0d3`; untracked inventory `43078098703a968a7edc5ad5307effd1f3a52da3af993e09ad99ebea830bf648`
- Benchmark/profile and exact command: `benchmark/fs-benchmark-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:60ae13e68a303a7a0849d8ae2ac08100683e36da18d95b90c85e09c7641a30d2 native-bash-baseline-09700086-20260831`
- Candidate order seed and pair count: `ae0ba07762577f7b24292d3d75d111174bf402cc3d8d4841246da2bd61fefdc2`; one LayerFS-only focused EDIT16 sample; Computer was neither rebuilt nor executed
- Image/provenance: LayerFS `sha256:60ae13e68a303a7a0849d8ae2ac08100683e36da18d95b90c85e09c7641a30d2`; arm64; source/helper OCI labels passed without pre-schedule helper execution
- Measurement/recovery containers: `486fa3d1a553dbb62c7b44a527b66ddb76392fb34dadb0c1c80cd8f308c1bff3`; `6c7f068676980b90b095e7839094ff5bb3c7ebb615b0d78d6c509294d8ee4e07`; exact BranchStore recovery identity and final oracle passed
- Native helper: source SHA-256 `6bee3425acd3de6ea2e9e0bb9b0e3f7dc10301663d691c9a96261883a34d0e4d`; binary SHA-256 `61f6454a7a1f982b4fc342b05d1a883e7b692f359e4c4de8cfc0cc22f86ed737`; Bash 5.2.15 `/bin/bash` SHA-256 `a901747ce15177efd627bd265f64cf18e3079f607d7168432158e172e6d8649b`; OS page cache uncontrolled
- Raw evidence directory and SHA-256 inventory: `runs/native-bash-baseline-09700086-20260831/`; 263 entries; inventory SHA-256 `b5680995b328b239c10802dbba603279aff2c4c31f0db80c22c936c9a1b0f11c`
- Previous comparable round: none; the neutral native+Bash correction creates a new baseline family
- Current best comparable round: none because this run is invalid

### Hypothesis and changes

The Python workload was replaced by one dependency-free native binary with O_RDWR, exact ten-byte positional write, `sync_all`, identical oracles, and admitted binary identity. Each edit used the frozen ordinary Bash argv `/bin/bash -lc '"$@"' fs-bench-shell <helper> edit ...`; no worker, daemon, resident process, batching, or FUSE bypass was added. PID-file umask became private. Execution receipts now balance pre-spawn total wall into spawn, supervisor queue, runtime, drain, terminal publication, and unattributed time. EDIT16 ran before any other Bash/helper invocation in its prepared measurement container. Recovery preceded twenty isolated post-recovery diagnostics.

### Correctness and validity

Exact final digest, 16 Commit/Push durability checkpoints, fresh recovery, exact BranchStore identity, live capture zeros, helper identity, Bash argv, O_RDWR semantics, process-group Stop, and balanced receipts passed. A separate live Docker gate proved the user argv retained the container's baseline umask/file mode. Every ordinary benchmark Commit required a live in-place rebase receipt with resume and no lifecycle/remount receipt.

The run is invalid because mountinfo was written inside the per-Workspace runtime directory and removed by Clean before raw inventory. The helper did capture it, but custody did not retain it; the stated mountinfo gate therefore failed.

### Non-comparable diagnostic timing

Complete EDIT16 was `1,509,032,375 ns`: create `134,941,875`; dispatch `14,979,168`; OutputReader wait `628,917,335`; Commit `135,807,873`; Push `477,148,041`; End `117,238,083`. Edit-01 complete was `236,023,666 ns`; edits 2–16 are retained individually in raw evidence. Execution total wall was `730,564,916 ns`, exactly balanced: spawn `2,428,499`; supervisor queue `2,028,707`; runtime `725,981,627`; drain `35,375`; terminal publication `81,333`; unattributed `9,375`.

Commit receipts totaled `129,374,251 ns`, including live capture `8,000` with zero files/bytes, publication `16,210,416`, in-place rebase `29,245,167`, and resume `1,219,248`. Push receipts totaled `469,848,710 ns`: sender source/auth `233,421,701`, authority transition verification `62,225,791`, durability `57,591,292`, unattributed `53,562,958`, 123 endpoint calls.

Five fresh-container samples per post-recovery diagnostic reported public medians: `/bin/true` `37,734,750 ns`; Bash `:` `36,628,416`; Bash→helper noop `35,335,166`; Bash→native pwrite+fsync `39,838,834`. Raw values and Q1/Q3 are in `execution-diagnostics-summary.json`. These explain overhead only and are not subtracted.

### Decision and next action

No speedup or baseline claim is made. Retain the neutral native+Bash direction, copy the already-captured mountinfo to append-only raw custody before Workspace End without invoking Docker/Bash/helper, make missing custody fail the run, rebuild with a new source seal, and repeat this baseline unchanged. Do not start Commit→Push frontier work until that valid baseline is sealed.

## Round 019 — native-bash-valid-09700086-20260831

- Status: INVALID (failed before edit-01; no performance sample)
- UTC timestamp: 2026-08-30T20:37:05Z through 2026-08-30T20:37:26Z
- Git commit/tree/source seal: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`; `2a41337e8b0fd080b6553df091b1570d0f358481d04dcd14e088cd703e98640d`
- Image: LayerFS `sha256:cbad723bb21faeae00c7af9333eeb64d2c0c1af37770c61fe5a9f04f1050bc16`; Computer was not executed
- Candidate order seed/pairs: `297937a4bf4e93c154365e38c138f40df03c549308d0949433785a329d2dd975`; one focused LayerFS arm
- Exact command: `benchmark/fs-benchmark-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:cbad723bb21faeae00c7af9333eeb64d2c0c1af37770c61fe5a9f04f1050bc16 native-bash-valid-09700086-20260831`
- Dirty evidence hashes: unstaged patch `979fe7a95a2fc56364c9221089696a85728c4f15576c8d07a7cfdcd2d2e7aec9`; staged patch empty; status `783f32028283ed3e3cb5df9db10e13d510f5275ca262e716c899c4e79690a0d3`; untracked inventory `f0da8e3b08a99b8d6374ed5fb7dea8a484653f16932b9ce42594f0554cb65f4a`
- Raw evidence directory/inventory: `runs/native-bash-valid-09700086-20260831/`; 26 entries; inventory SHA-256 `8fc43ce31c80eeaa150f059a6d3eeb3e0345182d49b5f7b990124ad229d41705`

### Failure and custody correction

The run intentionally failed before the first Bash/helper edit because mandatory mountinfo custody could not open its assumed path. The captured file existed at `branch.sqlite.runtime/workspaces/workspaces/<WorkspaceId>/mountinfo.txt`; the benchmark expected one fewer `workspaces` component. No edit, Commit, Push, recovery, or diagnostic timing ran. The harness had not yet auto-written failure custody, so `terminal.json` and `raw-inventory.sha256` were added immediately before any source edit.

### Next action

Correct the exact nested runtime path, add a test for it, regenerate the source seal/image, and repeat the unchanged native+Bash baseline. No product optimization is authorized from this failed run.

## Round 020 — native-bash-valid-v2-09700086-20260831

- Status: FAILED (evidence PASS; valid native+Bash baseline; hard terminal gate FAILED)
- UTC timestamp: 2026-08-30T20:41:49Z through 2026-08-30T20:42:54Z
- Local timestamp and timezone: 2026-08-31 04:41:49–04:42:54 CST (Asia/Shanghai)
- Git commit/tree/source seal: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`; `df09ae45fabe1a8f286690629dae2633f8bf3c4cdcc7892c1d618791196dd3b5`
- Dirty evidence hashes: unstaged patch `a57cd28e5f9594131be7c2c6fb9296453157393c2ab2967071a4b857dcd50937`; staged patch empty; status `783f32028283ed3e3cb5df9db10e13d510f5275ca262e716c899c4e79690a0d3`; untracked inventory `3a9dbc594e68910bbfc223148cf3ef956fe93fc4cd4ed34b7e34144dfef668d3`
- Exact command: `benchmark/fs-benchmark-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:8a6b24e3d6a9161c401194ee3c19af878993f5cde85928b06fe2d5b18c091358 native-bash-valid-v2-09700086-20260831`
- Candidate order seed/pairs: `2c8c2536aa73ae890d14b9927619e41f950dbcba93a35d8b578b551fc773139e`; one focused LayerFS arm; Computer was neither rebuilt nor executed
- Image/provenance: LayerFS `sha256:8a6b24e3d6a9161c401194ee3c19af878993f5cde85928b06fe2d5b18c091358`; arm64; admitted source/helper labels passed without executing Bash/helper before the schedule
- Measurement/recovery containers: `fcbb8e53511e9f089422900c9916fc2d6425a60fdb752d7dd41383f62276701d`; `5df0fc68b54851278d17f798ffe060591e15efef3dc5c3cb47939e401b7ceefa`; exact authority and BranchStore identities passed
- Helper/Bash: source `6bee3425acd3de6ea2e9e0bb9b0e3f7dc10301663d691c9a96261883a34d0e4d`; binary `61f6454a7a1f982b4fc342b05d1a883e7b692f359e4c4de8cfc0cc22f86ed737`; Bash 5.2.15 `/bin/bash` `a901747ce15177efd627bd265f64cf18e3079f607d7168432158e172e6d8649b`; OS page cache uncontrolled
- Raw evidence directory/inventory: `runs/native-bash-valid-v2-09700086-20260831/`; 264 entries; inventory SHA-256 `ff9f4f24349633868dee7c599be4181940798aa44279a76aae2da06c08012e3c`
- Previous comparable round: none; Rounds 018–019 were invalid and Python rounds are a different workload family
- Current best comparable round: Round 020 is the valid focused native+Bash baseline; no paired/formal best exists

### Frozen baseline and validity

EDIT16 used one real FUSE Workspace and sixteen public `Client::exec_workspace_session → OutputReader terminal → Commit → Push/two-Store durability` checkpoints. Every edit ran a fresh `/bin/bash -lc '"$@"' fs-bench-shell /usr/local/bin/fs-benchmark-workload edit ...`; Bash used ordinary child/wait behavior. The native helper used O_RDWR, one exact ten-byte positional write, `sync_all`, and close. Edit-01 was the prepared measurement container's first Bash/helper invocation. No worker, daemon, resident process, batching, target pre-read, internal mutation, or FUSE bypass existed.

All SDK and internal execution receipts balanced exactly. The headline uses one outer monotonic SDK Exec-to-terminal timer from before Exec dispatch through observed terminal receipt. Dispatch, output-handle acquisition, follow/read, and unattributed time are passive subphases. `sdk_output_follow_ns` starts at the first `OutputReader.read`. Missing or unbalanced receipts fail.

Exact final/reopen digests, exact authority/BranchStore recovery identities, two-Store durability, live capture zero files/bytes, same-mount in-place rebase/resume with no lifecycle/remount receipt, lease/CAS tests, and helper identity passed. Raw `edit-mountinfo.txt` proves `/workspace/fs-benchmark-pro-7` was a live `fuse layerfs` mount. A separate live gate proved exact argv, Stop, and baseline umask/file-mode preservation.

### Complete EDIT16 result

Complete public EDIT16 was `1,547,330,961 ns`, failing the `<=1.20 s` hard gate by `347,330,961 ns`. Components: one Workspace create/FUSE attach `136,032,292`; full SDK Exec-to-terminal `730,325,419`; Commit `128,038,333`; Push/two-Store durability `442,328,333`; one End `110,606,584`. The exact component sum equals the total.

SDK execution subphases: dispatch `14,591,293`; output-handle acquisition `5,200,042`; first-read-to-terminal follow `601,321,333`; unattributed intervals `109,212,751`. Every outer equation balanced. Edit-01 complete was `236,780,667 ns`, with `46,696,250 ns` Exec-to-terminal; edit-16 Exec-to-terminal was `46,732,917 ns`. Edits 2–15 are retained individually.

### Commit and Push

Receipted Commit was `122,120,209 ns`: pause `2,049,333`; live capture `7,293` and zero files/bytes; plan `10,577,125`; content `32,540,083`; local admission `25,318,461`; publication `12,364,459`; in-place rebase `27,545,750`; resume `1,168,955`; unattributed `1,939,711`. API Commit remained about `8.00 ms/edit`, at the preferred boundary.

Receipted Push was `435,640,876 ns`: history `1,549,377`; membership `7,869,793`; sender source/auth `217,647,383`; object admission `31,891,541`; fact admission `6,589,294`; authority transition verification `60,922,085`; publication `6,543,792`; durability `51,274,502`; unattributed `51,353,109`; 123 endpoint calls. Push remains above the 400 ms hard budget.

### Post-recovery diagnostics and decision

Five isolated fresh-container/Store samples each, after recovery and never subtracted, reported public medians: `/bin/true` `40,915,209 ns` (Q1 `39,923,584`, Q3 `44,394,896`); Bash `:` `49,269,583` (Q1 `48,029,292`, Q3 `52,267,917`); Bash→helper noop `49,018,542` (Q1 `42,413,792`, Q3 `57,025,896`); Bash→pwrite+fsync `49,444,500` (Q1 `47,309,813`, Q3 `53,598,105`). Raw totals and exact argv are retained. Organic Bash is frozen; the ~41 ms `/bin/true` floor proves LayerFS's one-short-lived-process path is dominated by Docker/wrapper/notification overhead. Bash/helper/pwrite add little beyond that floor in these samples.

### Next action

Round 020 is the baseline, not an engine speedup over Round 017. Because Bash-inclusive execution is above the council's ~540 ms threshold and `/bin/true` exposes the transport floor, optimize only redundant existing execution wrapper/CLI/polling work while preserving one fresh Bash/helper process per edit. Independently stage create Round A (blocking accept, lazy node reservation, no eager root Readdir) as its own later source seal. Do not mix Commit→Push frontier or create changes into the next execution-focused round.

## Round 021 — native-bash-direct-receipts-09700086-20260831

- Status: FAILED (evidence PASS; authoritative public native+Bash baseline; hard gate FAILED)
- UTC/local time: 2026-08-30T20:46:05Z through 2026-08-30T20:47:10Z; 2026-08-31 04:46:05–04:47:10 CST
- Git commit/tree/source seal: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`; `e15ea6a25d4c38543464fcb30d80e46c5520a837eb2fff2e82faecf82c1735e1`
- Dirty hashes: unstaged patch `eaa6c5729904e95903a4e25d1ab135bc4ceefca90d68672a0514ae52865bf95a`; staged empty; status `783f32028283ed3e3cb5df9db10e13d510f5275ca262e716c899c4e79690a0d3`; untracked inventory `d166a7142a3bc2d3f0d698b65aaac008b5b2ba06ff34d02f704ebfbaf9145a9e`
- Exact command: `benchmark/fs-benchmark-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:b6ac19976f054c87b30d305a01552866f348578a1af7b287d1c8e64bb5a1518b native-bash-direct-receipts-09700086-20260831`
- Seed/pairs/image: `882b9dacd20a7c163a388d1dbfa10b9d2ff081dcaeb8d20a5ca3b74ddaf6b6f1`; one LayerFS-only focused sample; image `sha256:b6ac19976f054c87b30d305a01552866f348578a1af7b287d1c8e64bb5a1518b`; Computer not executed
- Measurement/recovery containers: `0c14ba3db185bf8dbeb233470d92aae81e9f97b7aea3fcefcf59efd7134b242d`; `85f0a4204148595e3024095ae62cc2b51b3b2bedb80d186fce755ef66eacc851`
- Helper/Bash identity unchanged from Round 020; actual bytes reverified only after all headline/recovery/diagnostic samples
- Raw evidence: `runs/native-bash-direct-receipts-09700086-20260831/`; 264 entries; inventory SHA-256 `00a983f6db1e731a998e2eb43e79caa0b5156c1e483ea06ba6d98eb7fa94626b`
- Previous comparable round: Round 020, whose outer timer conservatively included monitor evidence collection
- Current best comparable round: Round 021 is the authoritative public baseline; no paired/formal best exists

### Reporter correction and validity

Exec dispatch and output-handle acquisition are now timed by direct public SDK calls. One outer clock starts immediately before `Client::exec_workspace_session` and stops when `OutputReader` supplies the terminal receipt. Only after that clock stops does one monitor snapshot retrieve Exec/Output operation receipts by exact ExecutionId. Thus evidence scans are neither hidden nor charged to public latency. The equation `sdk_exec_to_terminal = dispatch + output_handle + output_follow + unattributed` is mandatory for every operation; all 16 passed. Internal execution receipts also balanced.

All Round 020 workload, mountinfo, no-prewarm, exact argv, O_RDWR/fsync, rebase, durability, final/reopen, exact Store IDs, and post-recovery diagnostic gates passed unchanged.

### Complete EDIT16 result

Complete public EDIT16 was `1,620,339,956 ns`: create `144,103,000`; Exec-to-terminal `787,974,291`; Commit `128,798,792`; Push `458,192,206`; End `101,271,667`. It failed the hard gate by `420,339,956 ns`.

Exec subphases were dispatch `15,415,544`; output-handle `3,029,584`; follow/read `769,525,334`; unattributed `3,829`. Edit-01 Exec-to-terminal was `51,662,708 ns`. The near-zero unattributed total proves the public boundary is now complete and balanced.

Commit receipts remained stable at `122,595,167 ns`, including live capture `8,122`, publication `12,823,626`, in-place rebase `29,129,750`, and resume `1,099,460`. Push receipts were `451,169,127 ns`: sender source/auth `224,623,516`, authority transition verification `62,335,166`, durability `55,183,539`, unattributed `52,151,067`, 123 endpoint calls.

### Diagnostics and decision

Fresh post-recovery public medians: `/bin/true` `38,742,167 ns`; Bash `:` `41,176,542`; Bash→helper noop `42,073,125`; Bash→pwrite+fsync `43,594,708`. The organic Bash/helper/FUSE-write increment is small relative to the transport floor. These values are explanatory only and are not subtracted.

Because Exec-to-terminal is well above the ~540 ms decision threshold, the next source seal may optimize only redundant LayerFS execution wrapper/CLI/PID/polling work, preserving one fresh ordinary Bash/helper process and all Stop/output/exit semantics. Create Round A and Commit→Push frontier remain separate future seals.

## Round 022 — direct-engine-09700086-20260831

- Status: FAILED (evidence PASS; execution optimization retained; hard terminal gate FAILED)
- UTC/local time: 2026-08-30T20:57:59Z through 2026-08-30T20:59:04Z; 2026-08-31 04:57:59–04:59:04 CST
- Git commit/tree/source seal: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`; `c43bfc31b6f506e98269644861cbf0c5cdfb4a61b9cc023aa6df428f8f6d1dc1`
- Dirty hashes: unstaged patch `13a011876e260ce14aa88a164fdcc3fe1c640d9dc3823a7fb991d1201cf77c68`; staged empty; status `82d9207679e3b3282929f6408c355b209238fd28f589d7817c0ef6bacc4c86ee`; untracked inventory `5882bd2fff47cb232e917f3a8068aaeab44768e43b225f1f8e9a953fc0730a15`
- Exact command: `benchmark/fs-benchmark-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:274fd2e8df88f8eee2f4f940180c9f3fa8758b824865496a2d02f7eb9aaa39be direct-engine-09700086-20260831`
- Seed/pairs/image: `f370a31aca7a8e5853fd8757dd52a94b94b0747a79f92c5820cb27802988b1ad`; one LayerFS-only focused sample; image `sha256:274fd2e8df88f8eee2f4f940180c9f3fa8758b824865496a2d02f7eb9aaa39be`; Computer not executed
- Measurement/recovery containers: `25072e1ac1e17ab5788467080a0f978eeda2c5926e8565c18810eff537ebc91e`; `d9e65410daa0142976415771d2d5d2c34bda7c13db10c8b66ffb7c8a97169ca5`
- Raw evidence: `runs/direct-engine-09700086-20260831/`; 264 entries; inventory SHA-256 `c1a1d154a3c6dcb968b478ec856283eb633c46409c50131afb22485c837d8059`
- Previous/current best: Round 021 was the public baseline; Round 022 is the new focused native+Bash mechanism best; no paired/formal best exists

### Hypothesis and change

The isolated `/bin/true` floor proved Docker CLI process/connection startup dominated the frozen one-process-per-Exec user path. LayerFS now talks directly to the existing Docker Engine Unix socket for noninteractive container Exec: it creates one Engine exec, launches the existing short-lived private PID wrapper and exact user argv, drains Docker's multiplexed stdout/stderr stream, and inspects exact exit status. It adds no daemon, worker, queue, resident shell, batch, or FUSE command channel. The normal user command still creates a fresh ordinary Bash and native helper.

The existing CLI path remains a compatibility fallback only when the Engine socket is absent. A socket-present direct failure returns an error. Focused evidence requires `direct_engine=true` for every command. Stop retains the container-namespace private PID file and launches a separate direct Engine signal Exec only on Stop; live tests prove process-group termination. The PID-file umask remains scoped and exact user umask/argv semantics remain unchanged.

### Correctness and validity

The full live Docker gate passed direct normal execution, stdout/exit, long-running child Stop, exact argv, baseline umask/file mode, two real FUSE mounts, same-mount commits, and cleanup. All headline and isolated diagnostics report `direct_engine=true`. Receipt equations, mountinfo, capture zeros, exact final/reopen, exact Store IDs, two-Store durability, in-place rebase, and no-prewarm gates passed.

### Results

Complete EDIT16 was `1,337,127,708 ns`, down `283,212,248 ns` (17.48%) from Round 021, but still `137,127,708 ns` above the hard gate. Components: create `113,952,916`; Exec-to-terminal `535,264,957`; Commit `124,425,291`; Push `447,409,836`; End `116,074,708`.

Execution fell from `787,974,291` to `535,264,957 ns` (32.07%). Subphases: Engine create/start dispatch `82,541,918`; output-handle `3,774,500`; stream follow/terminal `448,944,750`; unattributed remained balanced. Edit-01 Exec was `41,459,375 ns`. Fresh diagnostics improved `/bin/true` median from `38,742,167` to `28,245,792 ns`; Bash `:` was `35,417,417`; Bash→helper noop `38,737,959`; Bash→pwrite+fsync `34,141,958`. Raw distributions remain uncontrolled for OS page cache and are never subtracted.

Commit receipts stayed stable at `118,324,083 ns`, including live capture `6,249`, publication `12,299,336`, and in-place rebase `27,123,417`. Push receipts stayed dominant at `440,852,958 ns`: sender source/auth `217,678,982`; authority transition verification `61,924,040`; durability `53,806,083`; unattributed `51,529,067`; 123 endpoint calls.

### Next action

Retain the direct Engine path. Exec-to-terminal is now at the council's ~540 ms decision threshold; implement the separately scoped bounded ID-only Commit→Push plan next. Do not change Bash/helper/process count, execution transport, create/End orchestration, or Commit semantics in that source seal.

## Round 023 — commit-push-plan-09700086-20260831

- Status: PASSED (evidence PASS; focused hard terminal gate PASS; optimization retained; no paired/formal claim)
- UTC/local time: 2026-08-30T21:06:06Z through 2026-08-30T21:07:10Z; 2026-08-31 05:06:06–05:07:10 CST
- Git commit/tree/source seal: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`; `9833f05bd664f55fb4f79ee369b8c4512f561485eb05c6f393fa88abcd8136e4`
- Dirty hashes: unstaged patch `b56bd1ce2a44a96f632108b813907792290a5e4d8cdb0cabc41f7f62adb6e443`; staged empty; status `f2ef9ed7e703da629282a08d227668f51247123bba0133307e0140bf8d5a62ca`; untracked inventory `880df7fe1ea938a4d10dae523e8753365175f82c821dd098cd029fabc9738c0f`
- Exact command: `benchmark/fs-benchmark-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:7153713e1dc51f003c735aab9a891ed9df6ca72fb892679a481867ed26d6a6e7 commit-push-plan-09700086-20260831`
- Seed/pairs/image: `744ebe28b12b2b25c0a11fa32eb8d71b22536d0c9e1187539f9d18528d4492dc`; one LayerFS-only focused sample; image `sha256:7153713e1dc51f003c735aab9a891ed9df6ca72fb892679a481867ed26d6a6e7`; Computer was neither rebuilt nor executed
- Measurement/recovery containers: `e32bdfb3583f630fc17e58bcde4e5b40891ab2df6119b0b0900a8932a02e5427`; `585a7ae44d6d6c39236c722005f6919524c1b8874b552c61da1f56593548cfec`
- Raw evidence: `runs/commit-push-plan-09700086-20260831/`; 264 inventoried entries; `raw-inventory.sha256` SHA-256 `f13992388d0cb55a444089b69ed8586d2a48dc0a66aea957cf5719d62a95713f`
- Previous best comparable round: Round 022 at `1,337,127,708 ns`
- Current best comparable round: Round 023 at `1,150,704,124 ns`; no balanced-pair or formal best exists

### Hypothesis and change

Normal Commit now retains a bounded, process-local, ID-only Push plan containing the exact Branch/Commit/base/new-root identity plus candidate ObjectIds in postorder. The plan carries no payload bytes, is capped at 512 IDs/4 MiB encoded candidate per entry and 64 entries/32,768 IDs globally, and is accepted only when local head, observed authority boundary, candidate root, uniqueness, and size all match exactly. A valid hit performs one bounded membership query and reads/stages only missing canonical objects. Reopen, eviction, mismatch, locally-complete corruption, multi-Commit gaps, or uncertainty uses the existing generic authenticated transition path. The authority's independent old-complete-root-to-new-root verification and visibility-last publication remain unchanged.

### Correctness and validity

Focused and contract tests proved plan-hit and reopen-fallback root/receipt equivalence, incomplete-authority repair before visibility, corrupted locally complete Replica rejection, safe head/base mismatch fallback, bounded membership, exact missing-only transfer, K1/K4/K16 behavior, unchanged durability, and fresh recovery. The public EDIT16 lifecycle, exact native+Bash argv, one fresh Bash/helper per Exec, direct Engine transport, real FUSE mount, no-prewarm boundary, live capture zeros, in-place rebase, exact final/reopen digests, exact Store identities, balanced receipts, mountinfo, and two-Store durability gates all passed.

Every ten-byte edit announced exactly 10 candidate ObjectIds, used one membership page and one payload batch, and sent exactly 10 missing objects. Edit-01 sent `16,908` bytes and edit-16 sent `14,988` bytes. No authority-owned base payload or pulled ancestry was transferred.

### Results

Complete public EDIT16 was `1,150,704,124 ns`, improving `186,423,584 ns` (13.94%) over Round 022 and passing the `<=1.20 s` hard gate by `49,295,876 ns`. It remains `100,704,124 ns` above the upper preferred `1.05 s` direction and `350,704,124 ns` above `0.80 s`. Components: one Workspace create/FUSE attach `124,512,791`; Exec-to-terminal `513,461,084`; Commit `125,010,542`; Push/two-Store durability `278,615,124`; one End `109,104,583`. The exact component sum equals the complete total, and all executions used the direct Engine path.

Receipted Commit was `119,079,958 ns`, including live capture `13,000` with zero captured files/bytes, publication `12,218,582`, and same-mount in-place rebase `27,282,168`. Commit behavior stayed stable at about `7.44 ms/edit`.

Receipted Push fell to `271,960,457 ns`: history `1,540,834`; frontier `3,876`; membership `4,450,878`; sender source/read/auth `2,220,417`; object admission `28,907,627`; fact admission `5,947,003`; independent authority transition verification `163,370,333`; publication `7,428,749`; durability `54,225,914`; unattributed `3,864,826`; 113 endpoint calls. Sender source/read/auth fell from Round 022's `217,678,982 ns` to `2,220,417 ns`. Authority verification rose from `61,924,040 ns` to `163,370,333 ns` because the sender fast path no longer traversed and incidentally warmed the receiver's Store; the full independent proof is retained and charged.

### Decision and next action

Retain the bounded Commit-to-Push plan: it passes the focused hard gate without weakening receiver verification, durability, recovery, process count, or the frozen user command. This is a new focused mechanism best, not a formal speedup claim.

The next source seal is Create Round A only: make ProxyHost accept blocking with Drop wakeup, make ProxyClient node-ID reservation lazy and race-safe, remove eager root `Readdir`, and prove a 100k-entry root performs zero enumeration during Workspace create with bounded RPCs. Do not mix one-call helper streaming, End redesign, Commit/Push changes, helper caching, prewarming, or a daemon into that round.

## Round 024 — create-round-a-09700086-20260831

- Status: PASSED (evidence PASS; focused hard terminal gate PASS; Create Round A retained; no paired/formal claim)
- UTC/local time: 2026-08-30T21:16:55Z through 2026-08-30T21:17:59Z; 2026-08-31 05:16:55–05:17:59 CST
- Git commit/tree/source seal: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`; `5a08137ed180adfe9687ec985a68a47b94cb192826c0340209708803f412618d`
- Dirty hashes: unstaged patch `706c4894dd5364c1a7a3470b868a6b8a1781c657080d88e5f418d69aae6f68d5`; staged empty; status `f2ef9ed7e703da629282a08d227668f51247123bba0133307e0140bf8d5a62ca`; untracked inventory `a6c0b769ea2c7a7ab61cc19ccc9aea913bcb352c15e7fd97a8eddcd1311932d4`
- Exact command: `benchmark/fs-benchmark-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:ed34a460816ab5e2b9323e1de25613f3cc8d1b299b341018ac15b153bf3fe64e create-round-a-09700086-20260831`
- Seed/pairs/image: `6ef5dffb7e6c8a0cfaa060d4d6dc2eceb503fd7635d623d6c0aed985ce9c0bf9`; one LayerFS-only focused sample; image `sha256:ed34a460816ab5e2b9323e1de25613f3cc8d1b299b341018ac15b153bf3fe64e`; Computer was neither rebuilt nor executed
- Measurement/recovery containers: `19800cb1930dcce8efbe98477849067413c242b313b5afcebb4ab7ce30fda69c`; `20a7dd346062c77e7d8c290c6c5e2e0709599914ac1999a2f62767d10b101645`
- Raw evidence: `runs/create-round-a-09700086-20260831/`; 264 inventoried entries; `raw-inventory.sha256` SHA-256 `d6c5e42bec314bda63f6b3b75613de6422a58750b0072c4c47fab0ba49095941`
- Previous best comparable round: Round 023 at `1,150,704,124 ns`
- Current best comparable round: Round 024 at `1,088,705,461 ns`; no balanced-pair or formal best exists

### Hypothesis and change

ProxyHost now blocks in `accept`; Drop retains the existing stopped flag plus loopback self-connect wakeup, removing the 10 ms polling interval between the authenticated data and control streams. ProxyClient connect now performs only the authenticated data connection: it neither reserves 65,536 NodeIds nor enumerates/caches the root. A reservation batch is fetched only by the first actually reservable create, with the existing reservation mutex held across the refill RPC so concurrent refills cannot race or discard ranges. Directory contents are fetched only by an actual `readdir`; a negative lookup no longer performs a hidden full-directory snapshot. The superseded Unix control socket, listener thread, helper CLI mode, argument, and cleanup path were deleted. Production Commit pause/resume continues to use the already-authenticated persistent reverse TCP control stream.

### Correctness and validity

A focused proxy test with a synthetic 100,000-entry root proved connect performs zero `Readdir` and zero `ReserveNodes` calls; the first reservable create performs exactly one lazy reservation. Capability rejection, authenticated remote pause/resume, cached directory behavior, deferred error fencing, and Drop wakeup passed. Full layerfs-fuse, workspace, SDK V2, benchmark unit suites, and workspace-wide all-target/all-feature Clippy passed.

A Linux-helper live gate proved two simultaneous real `fuse layerfs` mounts, authenticated pause/resume, exact argv and inherited umask, functional reads/writes/links, held-open-handle read after Commit rebase, a subsequent write/Commit/Push, and strict End for both Reference and Replica. Its later legacy throughput subtest could not run because the fs-benchmark-pro image intentionally lacks `/usr/local/bin/fs-bench.sh`; no product failure was hidden. The registered focused run independently passed real mountinfo custody, all 16 same-mount in-place rebase/resume checkpoints, live capture zeros, direct execution receipts, exact final/reopen digests, exact authority and BranchStore identities, two-Store durability, and fresh recovery.

### Results

Complete public EDIT16 was `1,088,705,461 ns`, improving `61,998,663 ns` (5.39%) over Round 023 and passing the `<=1.20 s` hard gate by `111,294,539 ns`. It remains `38,705,461 ns` above the upper preferred `1.05 s` direction and `288,705,461 ns` above `0.80 s`. Components: one Workspace create/FUSE attach `99,398,417`; Exec-to-terminal `518,023,336`; Commit `120,379,458`; Push/two-Store durability `276,945,000`; one End `73,959,250`. The exact component sum equals the complete total.

Public create improved `25,114,374 ns` (20.17%) from Round 023. Its attach receipt improved from `121,492,375` to `96,124,584 ns`: proxy `145,042`; Docker setup `33,095,791`; helper copy `21,048,208`; helper start/real-FUSE readiness `41,702,792`; unattributed `132,751`; three Docker calls. Create still misses the `<=80 ms` hard budget by `19,398,417 ns`; Store/root/registry work remains only about 3 ms.

Public End improved from `109,104,583` to `73,959,250 ns` and now passes the `<=80 ms` hard budget. Its lifecycle receipt was `72,217,583 ns`: unmount `32,156,084`; child wait `76,292`; helper cleanup `39,984,875`; unattributed `332`; two Docker calls. The old retained 20 ms polling tail disappeared because the helper exited before the first poll.

Commit stayed stable: receipt total `114,588,003 ns`, live capture `10,124` with zero files/bytes, publication `11,003,707`, in-place rebase `27,016,289`, resume `1,033,961`. Push stayed stable: receipt total `270,673,373 ns`, membership `4,171,962`, sender source/read/auth `1,949,832`, object admission `29,024,793`, authority verification `163,741,290`, durability `53,207,792`, 113 endpoint calls. Every edit retained the exact 10-ID, one-page, missing-only fast plan.

Post-recovery isolated public medians, never subtracted, were `/bin/true` `39,241,208 ns`; Bash `:` `32,672,667`; Bash→helper noop `31,642,167`; Bash→pwrite+fsync `34,114,667`. All raw distributions remain marked `diagnostic_prewarm=false` and `os_page_cache=uncontrolled`.

### Decision and next action

Retain Create Round A: it is a valid focused mechanism improvement and preserves the hard terminal pass. The next source seal is Create Round B only: replace setup exec + helper copy + start exec with one fresh `docker exec -i` that validates `/dev/fuse`, creates only owned paths, streams the exact helper through stdin with bounded `std::io::copy`, sets mode, publishes cleanup identity, execs the fresh helper, and waits for authenticated data/control plus real-FUSE READY/mountinfo. Add an armed AttachGuard immediately after spawn so any partial failure performs authenticated shutdown or checked fallback cleanup and proves mount absence. Do not mix zero-call End, direct Engine attach, Commit/Push, helper caching, preinstallation, prewarming, or a daemon into that source seal.

## Round 025 — create-round-b-09700086-20260831

- Status: PASSED (evidence PASS; hard gate PASS; focused upper preferred direction PASS; Create Round B retained; no paired/formal claim)
- UTC/local time: 2026-08-30T21:25:43Z through 2026-08-30T21:26:44Z; 2026-08-31 05:25:43–05:26:44 CST
- Git commit/tree/source seal: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`; `b4ff87693e1750d2624a4a888e780391afd78ce11a8fcc3d3e6a2f1d5f883fbd`
- Dirty hashes: unstaged patch `f502c4c1f2da2d898bb053e23f43ec382e51837c2822d8c077cf0903019a3632`; staged empty; status `f2ef9ed7e703da629282a08d227668f51247123bba0133307e0140bf8d5a62ca`; untracked inventory `d85d3f0a49d9c4dc600e90bb7bf023e4008073b76dbd846620c5affab76a90c9`
- Exact command: `benchmark/fs-benchmark-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:fa959790f7ade0b9473706371f6103f621dc9634e2b356b1605e74b208d91f47 create-round-b-09700086-20260831`
- Seed/pairs/image: `7b116db31e4e55b8e626687e029e87062b047871c531af630196ba37b1c55fb0`; one LayerFS-only focused sample; image `sha256:fa959790f7ade0b9473706371f6103f621dc9634e2b356b1605e74b208d91f47`; Computer was neither rebuilt nor executed
- Measurement/recovery containers: `194e217a31631139de6c0ae89afe48915303a369885ea5055aecf7e43fa9f552`; `436df4f36f6e159f6c8b296462eadf4876be733a820ba232d323bf78f27ed6a4`
- Raw evidence: `runs/create-round-b-09700086-20260831/`; 264 inventoried entries; `raw-inventory.sha256` SHA-256 `746b9619d812af7a5613fc40335bba3c6ba8ebaf730e153e5236943d156093d0`
- Previous best comparable round: Round 024 at `1,088,705,461 ns`
- Current best comparable round: Round 025 at `1,034,894,668 ns`; no balanced-pair or formal best exists

### Hypothesis and change

The three-call attach sequence—setup `docker exec`, `docker cp`, helper-start `docker exec`—was replaced by one fresh `docker exec -i`. Its private shell validates `/dev/fuse`, creates the requested mount path and the root-owned 0700 helper directory, receives the exact production helper through stdin, sets mode 0555, records helper PID plus `/proc` start identity, exports exact helper/root/capability ownership markers, and execs that fresh helper in the same process. The host streams from the admitted helper file with bounded `std::io::copy`, verifies the exact byte count, closes stdin, then waits for authenticated data/control connections, real-FUSE READY, and retained mountinfo. No helper is preinstalled into the target container, cached, reused, or executed before edit-01.

An AttachGuard is armed immediately after Docker Exec spawn and remains armed through mountinfo capture and lifecycle-receipt publication. Any early return invokes a checked fallback which validates PID reuse protection, executable path, root, and capability through `/proc`, unmounts when present, kills only the exact owned helper, removes helper/identity and an empty owned mount path, and proves mount absence. It retains cleanup stdout/stderr/status plus helper stderr. Killing the host Docker CLI is only the final post-cleanup reap and is never treated as container-helper cleanup.

### Correctness and validity

The live Linux-helper gate first streamed an invalid helper and proved no failed mount path, owned helper, or identity file remained. It then created two simultaneous real FUSE Workspaces in `57.9–60.0 ms`, passed authenticated pause/resume, exact argv and inherited umask, functional reads/writes/links, held-open-handle reads after same-mount rebase, subsequent write/Commit/Push, and strict End for Reference and Replica, with zero remaining FUSE mounts or owned helper files. As in Round 024, only the later unrelated legacy throughput subtest was unavailable because this benchmark image does not ship `fs-bench.sh`.

The registered run passed all 16 public Exec/Output/fsync/Commit/Push/durability checkpoints, capture_mode=live with zero captured files/bytes, same-mount in-place rebase/resume, exact mountinfo, direct execution, complete timing equations, exact final and fresh-recovery digests, exact authority/BranchStore identities, two-Store durability, and post-recovery-only diagnostics. No attach fallback artifact exists in the valid raw arm because no headline attach failed.

### Results

Complete public EDIT16 was `1,034,894,668 ns`, improving `53,810,793 ns` (4.94%) over Round 024 and `115,809,456 ns` (10.06%) over Round 023. It passes the `<=1.20 s` hard gate by `165,105,332 ns` and the focused upper preferred `1.05 s` direction by `15,105,332 ns`; it remains `234,894,668 ns` above `0.80 s`. Components: one Workspace create/FUSE attach `52,050,917`; Exec-to-terminal `502,309,541`; Commit `121,339,252`; Push/two-Store durability `279,515,458`; one End `79,679,500`. The exact component sum equals the complete total.

Public create improved `47,347,500 ns` (47.63%) from Round 024. Its single-call attach receipt was `48,930,416 ns`: proxy `81,209`; Docker Exec spawn `110,875`; container setup plus bounded helper stream `24,906,958`; authenticated helper/FUSE READY `23,705,375`; unattributed `125,999`; exactly one Docker call. Public SDK root pin/Workspace/lease/registry remained about 3.12 ms. Create passes the 80 ms hard budget and misses the 50 ms preferred edge by only `2,050,917 ns`.

Public End was `79,679,500 ns`, passing the 80 ms hard budget by `320,500 ns`. Its unchanged two-call lifecycle receipt was `77,862,208 ns`: unmount `35,201,625`; child wait `35,750`; cleanup `42,624,625`; unattributed `208`; two Docker calls.

Commit and Push remained stable. Commit receipt total was `115,679,291 ns`, live capture `6,998` with zero files/bytes, publication `11,893,417`, in-place rebase `26,447,248`, resume `1,026,084`. Push receipt total was `272,930,045 ns`, membership `4,103,958`, sender source/read/auth `1,864,792`, object admission `28,967,002`, authority verification `162,279,835`, durability `54,807,916`, 113 endpoint calls. Each edit announced 8–10 candidate objects in one object-membership page; sent IDs equaled exact missing IDs, including edit-01 10 objects/16,908 bytes and edit-16 10 objects/14,988 bytes.

Post-recovery isolated public medians, never subtracted, were `/bin/true` `37,206,833 ns`; Bash `:` `33,676,459`; Bash→helper noop `35,973,958`; Bash→pwrite+fsync `39,169,666`. Raw distributions remain `diagnostic_prewarm=false` and `os_page_cache=uncontrolled`.

### Decision and next action

Retain Create Round B. It achieves the expected one-Docker-call, fresh-helper, real-FUSE envelope without a daemon and establishes the first honest focused result inside the 0.80–1.05 s preferred direction.

The next source seal is Round C End only: extend the existing authenticated per-Workspace reverse control stream with shutdown; have the helper-owned HostMount unmount and join, verify mount absence, remove only its exact helper/identity/mount paths, acknowledge, and exit; have the controller wait for the already-attached Docker Exec child with no normal Docker call or polling. Keep a checked identity-validated Docker fallback for abnormal failure, return an error, and preserve evidence. Make public End two-phase so Workspace state/spool/lease are finalized only after checked shutdown succeeds. Do not mix direct Engine attach, Commit/Push, content, helper caching, prewarming, or any worker/daemon into that seal.

## Round 026 — end-round-c-09700086-20260831

- Status: PASSED (evidence PASS; hard gate PASS; targeted End mechanism PASS; complete timing regressed under unrelated noise; no new-best or paired/formal claim)
- UTC/local time: 2026-08-30T21:40:10Z through 2026-08-30T21:41:11Z; 2026-08-31 05:40:10–05:41:11 CST
- Git commit/tree/source seal: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`; `964127ad4d9cbe9db2ad812c53c5e00f5791958704245d87269667b8b88356e4`
- Dirty hashes: unstaged patch `32daf0ecfc652328c62f000ecd53eb82ae8c01b7b375272197b487d59bcb3657`; staged empty; status `30ce6548a7322d54f6d6e0f5814d2c8da4feef4b251aa7afd84165294232f923`; untracked inventory `4e674a95cec6366ee6093d2e4185498422551c5b6d07795d3e97ce67add09e7d`
- Exact command: `benchmark/fs-benchmark-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:356cd9a8fd1c26b8c04d333ad590d6732dd4bf6335d6e3e654261eddea70b1ef end-round-c-09700086-20260831`
- Seed/pairs/image: `36c937a8848680f89924dd5a56d08f215f32b2a990d3018abb740a78babe553a`; one LayerFS-only focused sample; image `sha256:356cd9a8fd1c26b8c04d333ad590d6732dd4bf6335d6e3e654261eddea70b1ef`; Computer was neither rebuilt nor executed
- Measurement/recovery containers: `865198acfe68a72cedd82a4c7a117aa41ae5bdc12e7350c972c395e2e41459b5`; `1f2f6d36039a8d68ec0f1120e2614b6086b0e33d726ab4e6940cb20a2ad457cd`
- Raw evidence: `runs/end-round-c-09700086-20260831/`; 264 inventoried entries; `raw-inventory.sha256` SHA-256 `89363cbee32481b508491bf409b46203e77d6302e1d6760fa16c8e45cc94ebad`
- Previous/current best comparable round: Round 025 at `1,034,894,668 ns`; Round 026 is not a new best

### Hypothesis and change

The authenticated reverse control stream now supports a shutdown command. The per-Workspace helper retains ownership of HostMount, waits for shutdown after READY/mountinfo, calls `umount_and_join` with checked fallback inside HostMount, validates its exact PID/start/helper/root/capability ownership, removes only the streamed helper, identity record, and a mount directory that attach itself created, acknowledges only after cleanup succeeds, then exits. The controller waits directly for the already-attached Docker Exec child; healthy End performs no Docker command and no polling.

Abnormal End and Drop reuse one checked Docker fallback. It validates PID reuse protection, executable, root and capability, reads exact mount-table membership with `findmnt --mountpoint` even for disconnected FUSE, unmounts, kills only the owned helper, removes exact owned paths, and proves the mount is absent. A `created_root` identity bit prevents removal of preexisting empty placement roots.

Public End is now two-phase. Dirty/quiesce validation occurs while the projection is still resumable. Projection shutdown must succeed before Workspace state changes, spool deletion, runtime-directory deletion, retention, or lease release. A failure leaves the spool and lease intact, marks the active record `BrokenCleanup`, rejects repeated End/Exec/Commit, remains queryable as dirty, and never resumes a consumed/broken projection. Only successful shutdown finalizes Ended/Discarded state, retains the session, and releases the lease.

### Correctness and validity

Proxy tests proved shutdown request/ack ordering. SDK lease tests forced projection cleanup failure and proved `BrokenCleanup`, retry rejection, queryability, and lease retention. The Linux live gate proved two real Workspaces self-unmounted cleanly after held-handle rebase and second Commit/Push. Its intentional later legacy panic forced Drop on another live mount; final inspection proved zero FUSE mount-table entries, zero owned helper/identity files, and preservation of the preexisting `/workspace` directory. Full SDK V2, workspace, FUSE, lease, benchmark build, and strict Clippy gates passed.

The registered run passed all public lifecycle, exact argv, mountinfo, capture-zero, same-mount rebase, missing-only Push, two-Store durability, exact final/reopen, exact Store identity, fresh recovery, direct execution, and post-recovery diagnostic gates. Healthy End emitted no fallback artifacts.

### Results

Targeted End succeeded: public End fell from Round 025's `79,679,500` to `6,374,792 ns`, a `73,304,708 ns` (92.00%) reduction. Its lifecycle receipt was `4,546,917 ns`: authenticated helper unmount/cleanup/ack `353,208`; attached child exit `4,193,542`; cleanup `0`; unattributed `167`; `docker_calls=0`. This beats the 20–50 ms preferred budget and the 5–15 ms plausible zero-call target.

Complete public EDIT16 was `1,142,731,630 ns`, still passing the `<=1.20 s` hard gate by `57,268,370 ns` but regressing `107,836,962 ns` (10.42%) from Round 025 and missing 1.05 s by `92,731,630 ns`. Components: create `63,028,250`; Exec-to-terminal `646,641,169`; Commit `132,020,419`; Push/two-Store durability `294,667,000`; End `6,374,792`. The exact component sum equals the total.

The regression is not in End. Versus Round 025, create rose `10,977,333 ns`, Exec rose `144,331,628`, Commit rose `10,681,167`, and Push rose `15,151,542`, totaling `181,141,670 ns` of unrelated increase against the `73,304,708 ns` End gain. Receipts likewise show stable mechanisms: attach one call `59,717,250 ns`; Commit `125,618,167` with live capture `7,915` and zero files/bytes; Push `287,457,501` with sender source/read/auth `1,933,457`, authority verify `170,427,086`, durability `58,159,081`, 113 calls.

Post-recovery isolated medians, never subtracted, were `/bin/true` `30,910,833 ns`; Bash `:` `37,939,292`; Bash→helper noop `33,395,208`; Bash→pwrite+fsync `39,906,791`. These do not explain the headline Exec outlier and remain `diagnostic_prewarm=false`, `os_page_cache=uncontrolled`.

### Decision and next action

The zero-call End mechanism passes its targeted proof, but Round 026 alone is not accepted as a complete-performance improvement and Round 025 remains the focused best. Before any source edit, repeat the identical sealed LayerFS-only candidate once under a new append-only run ID. Retain Round C only if the repeat preserves zero-call strict End and the complete result returns to the established noise envelope; otherwise investigate the phase receipts without changing source. No Computer rebuild or run is needed.

## Round 027 — end-round-c-repeat-09700086-20260831

- Status: PASSED (evidence PASS; hard gate PASS; focused upper preferred direction PASS; Round C retained; new focused best; no paired/formal claim)
- UTC/local time: 2026-08-30T21:42:15Z through 2026-08-30T21:43:15Z; 2026-08-31 05:42:15–05:43:15 CST
- Git commit/tree/source seal: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`; `964127ad4d9cbe9db2ad812c53c5e00f5791958704245d87269667b8b88356e4`
- Dirty hashes: unstaged patch `32daf0ecfc652328c62f000ecd53eb82ae8c01b7b375272197b487d59bcb3657`; staged empty; status `30ce6548a7322d54f6d6e0f5814d2c8da4feef4b251aa7afd84165294232f923`; untracked inventory `f3fde2a205f948f0594dc6fb0a3d7edca1e8415bcb23f67b90c2159b9de4827c`
- Exact command: `benchmark/fs-benchmark-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:356cd9a8fd1c26b8c04d333ad590d6732dd4bf6335d6e3e654261eddea70b1ef end-round-c-repeat-09700086-20260831`
- Seed/pairs/image: `1a8bd2e2c325d6d4b01d5b045e5738904ea727b540bb782b46befdef78de1655`; one LayerFS-only focused sample; image `sha256:356cd9a8fd1c26b8c04d333ad590d6732dd4bf6335d6e3e654261eddea70b1ef`; Computer was neither rebuilt nor executed
- Measurement/recovery containers: `5f697d2c5cfaffba77e5ea9d4ff2865bedccc6c4f8377d201806556da0aa0cbe`; `bfb50bbf54f47a42a7544c93a20d7d206b035f2042fa66c721bcf24306ae0e7c`
- Raw evidence: `runs/end-round-c-repeat-09700086-20260831/`; 264 inventoried entries; `raw-inventory.sha256` SHA-256 `6acd6cc675a8477b00de974b4042cd05e5f57211d538c1095077cd3dea131dd3`
- Previous best comparable round: Round 025 at `1,034,894,668 ns`
- Current best comparable round: Round 027 at `1,020,207,162 ns`; no balanced-pair or formal best exists

### Identical-repeat validity

Round 027 used the exact Round 026 source seal, image, public protocol, helper/Bash bytes, one-call fresh-helper attach, direct Engine Exec, Commit-to-Push plan, authenticated End, and evidence gates. Only the run ID/seed and fresh container/Workspace/Store state changed. Terminal and all 264 raw artifacts were sealed before interpretation. No fallback artifact exists in the valid arm.

The repeat passed mountinfo, 16 exact public Exec/Output/fsync/Commit/Push/two-Store durability checkpoints, live capture zeros, same-mount in-place rebase/resume, exact missing-only transfer, exact final/reopen digests, exact authority/BranchStore identities, fresh recovery, balanced receipts, and isolated post-recovery diagnostics. Healthy End again used the authenticated helper path with no Docker command.

### Results and reconciled verdict

Complete public EDIT16 was `1,020,207,162 ns`, improving `14,687,506 ns` (1.42%) over Round 025 and passing the `<=1.20 s` hard gate by `179,792,838 ns`. It passes the focused upper preferred `1.05 s` direction by `29,792,838 ns` and remains `220,207,162 ns` above `0.80 s`. Components: one Workspace create/FUSE attach `44,644,291`; Exec-to-terminal `550,740,375`; Commit `127,647,373`; Push/two-Store durability `290,647,623`; one End `6,527,500`. The exact component sum equals the total.

Create is now inside the 30–50 ms preferred band. Its attach receipt was `41,621,583 ns`: proxy `91,000`; Docker spawn `135,875`; setup plus bounded helper stream `32,920,667`; authenticated/FUSE READY `8,289,083`; unattributed `184,958`; one Docker call.

End repeated the mechanism gain: public `6,527,500 ns`, `73,152,000 ns` (91.81%) below Round 025. Its lifecycle receipt was `4,524,500 ns`: authenticated unmount/cleanup/ack `446,333`; attached child exit `4,078,083`; cleanup `0`; unattributed `84`; `docker_calls=0`. Combined public Workspace create+End lifecycle is `51,171,791 ns`, down from Round 023's `233,617,374 ns`.

Commit receipt was `121,368,626 ns`: live capture `7,706` with zero files/bytes, candidate plan `10,223,835`, content `33,078,666`, namespace `7,296,833`, local admission `23,446,208`, publication `12,652,915`, in-place rebase `28,520,460`, resume `1,036,124`, unattributed `1,837,295`. API Commit remains `7.98 ms/edit`.

Push receipt was `282,411,083 ns`: history `1,487,585`; frontier `4,043`; membership `4,277,873`; sender source/read/auth `1,916,126`; object admission `32,179,123`; fact admission `6,523,169`; independent authority verification `164,420,128`; publication `7,477,998`; durability `60,367,126`; unattributed `3,757,912`; 113 endpoint calls. Push remains inside the 250–300 ms focused target while retaining independent verification.

Post-recovery isolated medians, never subtracted, were `/bin/true` `35,975,916 ns`; Bash `:` `34,949,834`; Bash→helper noop `34,729,042`; Bash→pwrite+fsync `42,380,208`. All remain `diagnostic_prewarm=false`, `os_page_cache=uncontrolled`.

Round 026's non-End slowdown is classified as environmental noise: the identical repeat recovered the established envelope while reproducing the zero-call End gain. Retain Round C and use Round 027 as the current focused best. No formal speedup claim is made from these LayerFS-only samples.

### Next action

Before another edit, inspect Round 025–027 execution receipts against the post-recovery `/bin/true`/Bash/helper/edit distributions and the current direct Docker Engine request path. The remaining EDIT16 majority is 16 fresh Bash/helper executions (`550.7 ms`, 54.0%); organic Bash/process startup stays frozen and no worker/daemon/resident process is allowed. Change execution transport only if passive evidence identifies redundant LayerFS connection/inspect/notification work. Otherwise stop edit-specific changes at the honest `<=1.05 s` result and move to the already-ranked parent-aware bounded admission work for prepend/rewrite/storage rows. Seal that work separately from EDIT16 mechanisms.

## Round 028 — parent-aware-admission-09700086-20260831

- Status: PASSED (evidence PASS; parent-aware storage mechanism PASS; EDIT16 hard and upper preferred direction PASS; new focused best; no paired/formal claim)
- UTC/local time: 2026-08-30T21:52:58Z through 2026-08-30T21:53:58Z; 2026-08-31 05:52:58–05:53:58 CST
- Git commit/tree/source seal: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`; `1a1701d736d235cb33222207f0bb4f7f64672b9496de4e98a08f22f7f0b7b16b`
- Dirty hashes: unstaged patch `56041936af2f7bebf2e5329c340e3f3f491e0f58e824bb8b8a8644d0fa5d8dcf`; staged empty; status `30ce6548a7322d54f6d6e0f5814d2c8da4feef4b251aa7afd84165294232f923`; untracked inventory `1e107b370c95d361fb5271664dadcf2e9588a2473838322ad3449ff71a340124`
- Exact command: `benchmark/fs-benchmark-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:6623b3d19d79dab886d1f1966160253275a71ceb45c89c0392b9856d169af3b0 parent-aware-admission-09700086-20260831`
- Seed/pairs/image: `8d16d967da464921148df24f1ff5a59eee6610e3f7425d353c71d38d761beb6f`; one LayerFS-only focused sample; image `sha256:6623b3d19d79dab886d1f1966160253275a71ceb45c89c0392b9856d169af3b0`; Computer was neither rebuilt nor executed
- Measurement/recovery containers: `a0c11b816c34f73ce1824d4e59d84963fa60f7f97bbae435e19ecf178be11484`; `9c3d9112f3121eaf7309d4ff2e9e028b97c080c25cd1f3c9cc844db44d43d1d6`
- Raw evidence: `runs/parent-aware-admission-09700086-20260831/`; 264 inventoried entries; `raw-inventory.sha256` SHA-256 `37b62784f5871df94d0840e9b90bfb13ee61b3e0d2162b75e25d8ca7c77e0d8a`
- Previous best comparable round: Round 027 at `1,020,207,162 ns`
- Current best comparable round: Round 028 at `1,012,593,378 ns`; no balanced-pair or formal best exists

### Hypothesis and change

Normal incomplete Reference Commit admission now classifies each deferred candidate batch against the union of the BranchStore and its exact admitted parent LayerStack endpoint before opening a writer transaction. Candidate IDs are sorted and checked once per page; local and parent membership are each batched; known lengths must match the canonical candidate. Locally present objects remain ordinary `reused`; parent-only objects increment both `reused` and explicit `source_reused_ids/source_reused_bytes`. Only union-missing canonical objects enter the BranchStore.

Missing objects are accumulated across visitor pages and admitted in at most 128 objects or 4 MiB per writer transaction. The deferred visitor itself remains bounded at 128 objects/4 MiB, so candidate visitation plus admission staging stays below the requested 8 MiB without another full payload buffer. Empty admission transactions are skipped. Membership, parent access, hashing, CDC, and traversal remain outside writer transactions. Complete/Replica roots do not use parent omission and retain full local admission plus closure verification.

The append-only operation receipt format gained a backward-compatible `c` local-admission record with explicit source-reuse counts; existing `l`/`b` records still decode with zero source reuse. JSON raw evidence includes the new fields and validates source reuse as a subset of total reuse.

### Correctness and validity

A focused 8 MiB pseudorandom prepend contract proved batched parent membership (`<=128` IDs per call), substantial source reuse, incomplete Reference root semantics, exact final bytes through the hybrid reader, and bounded local admission. Full BranchStore contracts passed incomplete-authority repair, corrupted-local rejection, Push fallback/hit equivalence, Reference/Replica scope behavior, ancestry boundaries, and reconciliation. Monitor backward-compatible round trips, workspace, SDK V2, storage, benchmark, and workspace-wide strict Clippy gates passed.

The registered run passed all EDIT16 lifecycle/recovery gates plus the 32 MiB prepend oracle and fresh recovery. It retained real FUSE, zero-call strict End, live capture zeros, same-mount rebase, exact missing-only Push, two-Store durability, balanced execution/DB receipts, exact Store identities, and exact final/reopen digests.

### Prepend/storage result

The 32 MiB prepend candidate remained exactly `1,747 objects / 33,661,935 bytes`; no candidate work was hidden. Local BranchStore admission changed from Round 027's `1,747 objects / 33,661,935 bytes` to only `44 objects / 377,723 bytes`. The other `1,703 objects / 33,284,212 bytes` are explicitly `source_reused`. Candidate equations and final root/recovery passed.

Local admission fell from `858,387,000` to `29,592,750 ns` (96.55%). The only BranchStore object writer transaction admitted 44 rows/377,723 bytes: total `9,574,834 ns`, writer acquire `7,584`, statements `455,957`, SQLite commit/sync `9,110,875`, unattributed `126`, 45 statements. CDC, hashing, membership, parent reads, and traversal were outside it. Commit API fell from `1,130,734,584` to `298,642,792 ns` (73.59%).

Push remained missing-only: 1,754 announced candidate IDs, 44 authority-missing IDs, exactly 44 sent/inserted objects totaling `377,723` bytes, 14 membership pages, and one object payload batch with `380,539` peak buffered bytes. Authority object admission was one 44-row/377,723-byte transaction. Push API was stable at `271,811,625 ns` versus `272,276,958` in Round 027.

Complete public prepend fell from `1,817,413,417` to `999,477,376 ns`, improving `817,936,041 ns` (45.00%). Components: create `46,946,959`; native Exec/fsync `374,455,542`; Commit `298,642,792`; Push/durability `271,811,625`; End `7,620,458`. No work was shifted beyond End or recovery.

Final durable Store allocation fell from `72,159,232` to `36,745,216 bytes`, a `35,414,016-byte` (49.08%) reduction, while authority checkpoint and final snapshots match and fresh recovery passed.

### EDIT16 regression guard and decision

Complete public EDIT16 was `1,012,593,378 ns`, improving `7,613,784 ns` (0.75%) over Round 027 while primarily serving as a no-regression guard. It passes the `<=1.20 s` hard gate by `187,406,622 ns` and 1.05 s by `37,406,622 ns`. Components: create `51,565,708`; Exec `552,974,336`; Commit `126,403,251`; Push `276,285,167`; End `5,364,916`. The exact sum balances.

Post-recovery isolated medians, never subtracted, were `/bin/true` `30,606,500 ns`; Bash `:` `31,695,291`; Bash→helper noop `38,933,167`; Bash→pwrite+fsync `39,250,334`. All remain no-prewarm with uncontrolled OS page cache.

Retain parent-aware bounded admission. It removes the measured cross-row storage/write amplification without weakening complete Replica semantics, integrity, durability, or recovery. Round 028 is the current focused best, but not a formal LayerFS-versus-Computer claim.

### Next action

The requested focused optimization loop has reached a stable stopping point: EDIT16 is inside the 0.80–1.05 s preferred direction; create is one-call and preferred-band; End is zero-call and below 10 ms; Commit is about 7.9 ms/edit; Push is inside 250–300 ms; prepend no longer copies the 32 MiB parent into BranchStore. Do not make another speculative algorithm change from a single focused run. Preserve this source seal and use the next run budget for the council-defined adjacent smoke/pilot only if a paired Computer image with formal provenance is prepared; otherwise keep Computer unexecuted and report this as LayerFS-only focused evidence.

## Round 029 — parent-aware-final-09700086-20260831

- Status: PASSED (final source-seal confirmation; evidence PASS; parent-aware prepend PASS; EDIT16 hard gate PASS under execution noise; no new-best or paired/formal claim)
- UTC/local time: 2026-08-30T22:00:58Z through 2026-08-30T22:01:57Z; 2026-08-31 06:00:58–06:01:57 CST
- Git commit/tree/source seal: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`; `02b7816c80be8f6e34669204121958041c0f637c3e6c21b89c8518531dd0779c`
- Dirty hashes: unstaged patch `3182eabee9851a2e4d2c48a1f522b0e839d71cc39aac89c325ce3ae4a8cd03d8`; staged empty; status `9539aa8139152a21aa5cc012811b25982e337749b2134182f8fd1c32c5339b42`; untracked inventory `ed62342805b59218e723550c21778047697b08fc9ad9bda5668653e8a8a54845`
- Exact command: `benchmark/fs-benchmark-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:d20292596ed581ab3c0aa6af822801821def10859aee8c361338595b83948254 parent-aware-final-09700086-20260831`
- Seed/pairs/image: `234b6c4ce3cae50de2d92ad9097fc184878b5598160b3c3c6548457b7bbbd32e`; one LayerFS-only focused sample; image `sha256:d20292596ed581ab3c0aa6af822801821def10859aee8c361338595b83948254`; Computer was neither rebuilt nor executed
- Measurement/recovery containers: `6a19c3c6289d4de1265766b07689ce3ab65d54551b47f1098d1a32a3795f7cc0`; `20a3e43f551f4dc1339dce89579f63ec0c779f082daf8da8534926498dce399c`
- Raw evidence: `runs/parent-aware-final-09700086-20260831/`; 264 inventoried entries; `raw-inventory.sha256` SHA-256 `32ad1a849ace8af0a123dc5bdbd4d77431547b9866929ad9e338a35d2ce56433`
- Current best comparable round remains Round 028 at `1,012,593,378 ns`; Round 029 confirms the final handed-off source seal

### Final-seal correction and validity

The only change after Round 028 was a test-only correction: the 270-Commit authority suffix test used identical roots and still expected the superseded full-root verifier's 12 point object reads. The positional transition verifier correctly prunes `old_root == new_root`, so the test now asserts zero payload reads while successful publication still proves the full paged suffix was consumed. No production code or release binary behavior changed.

Round 029 aligns the benchmark image label and raw evidence with that final test-corrected source seal. Terminal, inventory, real mountinfo, 16 checkpoints, two-Store durability, capture zeros, same-mount rebase, zero-call End, exact final/recovery, exact Store identities, native/Bash boundary, and post-recovery diagnostic gates all passed. Full parent-aware counts reproduced exactly.

### Results

Parent-aware prepend reproduced: candidate `1,747 objects / 33,661,935 bytes`; local insert `44 objects / 377,723 bytes`; source reuse `1,703 objects / 33,284,212 bytes`; exact Push `44 objects / 377,723 bytes` in one payload; exact final and recovery passed. Commit API was `309,355,459 ns`, Push `253,629,292`, and complete public prepend `990,205,293 ns`, slightly faster than Round 028's `999,477,376 ns`.

Complete public EDIT16 was `1,151,741,298 ns`, passing the `<=1.20 s` hard gate by `48,258,702 ns` but not replacing Round 028 as best. Components: create `50,567,875`; Exec `690,254,086`; Commit `128,786,294`; Push `277,454,585`; End `4,678,458`. The regression is again isolated to fresh-process execution noise; storage, Commit, Push, one-call create, and zero-call End mechanisms remain stable.

### Final decision

Retain all Round 028 production mechanisms and use Round 028 as the focused performance best; use Round 029 as the exact final-source custody confirmation. The hard EDIT16 target and at least one honest focused 0.80–1.05 s result are proven, as are the cross-row prepend/storage gains. Stop source changes here. A future performance claim against Computer requires the council-defined paired pilot/formal schedule and a formal-provenance Computer image; none was run or claimed in this loop.

## Round 030 — paged-push-plan-09700086-20260831

- Status: IMPROVED (evidence PASS; paged PushPlan mechanism and prepend latency PASS; EDIT16 no-regression PASS; physical-allocation delta repeat required; no paired/formal claim)
- UTC timestamp: 2026-08-30T22:28:33Z through 2026-08-30T22:29:33Z
- Local timestamp and timezone: 2026-08-31 06:28:33–06:29:33 CST (Asia/Shanghai)
- Git commit and tree: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`
- Dirty source seal and diff hashes: source seal `c58f4787eae91f6886d682cb7e88e8bcd625861c1995c030398067288f0b4061`; working-tree patch `92d28bab5ed27a2c49d0a862ed4b87759b6c45b43649bdc9634a7bb3f89ea5ca`; index patch empty `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`; status `9539aa8139152a21aa5cc012811b25982e337749b2134182f8fd1c32c5339b42`; untracked inventory `d182ef652f463c6c4e041f448c17d95aab59b96e2ba721c5407b9a378ff5a4b1`
- Benchmark/profile and exact commands: focused tests listed below; image build `docker build --build-arg LAYERFS_SOURCE_COMMIT=0970008668f54bae841797dafd57acab191fba7f --build-arg LAYERFS_SOURCE_TREE=f81dad341dff677b82c91f31a4beee0de2f1cc9f --build-arg LAYERFS_SOURCE_DIRTY=true --build-arg LAYERFS_SOURCE_SEAL=c58f4787eae91f6886d682cb7e88e8bcd625861c1995c030398067288f0b4061 --build-arg WORKLOAD_SOURCE_SHA256=6bee3425acd3de6ea2e9e0bb9b0e3f7dc10301663d691c9a96261883a34d0e4d --build-arg WORKLOAD_BINARY_SHA256=61f6454a7a1f982b4fc342b05d1a883e7b692f359e4c4de8cfc0cc22f86ed737 -f benchmark/fs-benchmark-pro/Dockerfile.layerfs -t layerfs-fs-benchmark-pro:paged-push-plan-09700086 .`; run `benchmark/fs-benchmark-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd layerfs-fs-benchmark-pro:paged-push-plan-09700086 paged-push-plan-09700086-20260831`
- Candidate order seed and pair count: `e4e1363c93abc3c4e7aa4b869caac0c991704b54316d0eeb4bcfeb077ab334d5`; one LayerFS-only focused sample; Computer was neither rebuilt nor executed
- Host, kernel, Docker, CPU/memory/I/O envelope: Apple arm64; Darwin 25.4.0 host with Docker Desktop 4.76.0 / Engine 29.5.2 linux/arm64; one CPU; 1 GiB memory and swap; PID limit 512; 256 MiB `/tmp` tmpfs; same Docker daemon; OS page cache uncontrolled
- Candidate image tags, digests, architectures, and verified OCI labels: `layerfs-fs-benchmark-pro:paged-push-plan-09700086`; `sha256:fb0e1c57946f41903ee682d2f2517cf2145d796bc3666991b9c11c16aba5b79c`; arm64; source commit/tree/dirty/seal and native helper source/binary labels matched
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `fc9e4661bf50036068d9671dc2cc2f163f2a23aaab269360a0343209df0d3bba`; recovery `a03853a19d06654d8c52bb028d737c2961742477a269ad17c602f4a7ef911e07`; `/workspace/fs-benchmark-pro-7`; `fuse layerfs rw,user_id=0,group_id=0,default_permissions`; native helper `61f6454a7a1f982b4fc342b05d1a883e7b692f359e4c4de8cfc0cc22f86ed737`
- Raw evidence directory and SHA-256 inventory: `runs/paged-push-plan-09700086-20260831/`; 264 entries; `raw-inventory.sha256` SHA-256 `63462994858ff702b31e868817436a69a7026d864eaaba9cfdf6d27c4f9954ae`; every entry reverified and no symlink exists
- Previous comparable round: Round 029 for exact prior source custody; prepend `990,205,293 ns`, Push `253,629,292 ns`; Round 028 remains the prior EDIT16 performance best at `1,012,593,378 ns`
- Current best comparable round: Round 030 for prepend at `840,386,292 ns`; Round 028 remains the EDIT16 best

### Hypothesis and planned change

The immediate Commit retained candidate ObjectIds but discarded plans above 512 IDs or 4 MiB candidate bytes, forcing a generic positional transition walk for a 1,747-object prepend even though only 44 objects were authority-missing. The minimal change was to keep the plan payload-free up to the existing global 32,768-ID cache cap, page sorted receiver membership at 512 IDs, preserve candidate postorder for missing payloads, and reuse the existing 128-object/4-MiB transfer pipeline. Any cap, head, base-root, new-root, root-membership, or uniqueness mismatch still selects the existing generic transition fallback.

### Changes since the previous round

`PushPlan` no longer stores or gates on candidate byte count. Commit calls the existing spill-capable `ids_in_order` with the 32,768-ID safety cap. Push validates that cap and exact head/base/new-root identity, then issues `ceil(candidate_ids/512)` membership pages. It retains only sorted missing IDs and streams those canonical objects in postorder through the unchanged bounded pipeline. No public API, protocol operation, daemon, execution path, Store schema, canonical encoding, object identity, authority verifier, publication transaction, or durability behavior changed.

Focused proof added one case just below 512 IDs, one just above, and the full 32 MiB sparse prepend; exact candidate announcement and sparse missing transfer; 512-ID membership maximum; 128-object/4-MiB authority admission maximum; zero sender parent object reads on a valid plan hit; a 32,768-ID accepted plan and 32,769-ID rejection; reopen/cache-miss fallback; multi-Commit base-boundary mismatch fallback; omitted-local-object rejection; interrupted pre-publication invisibility and successful retry; incomplete authority closure repair; and existing Replica corruption policy.

### Correctness and validity

The run passed exact final bytes/hash, real retained FUSE mountinfo, one public Workspace for 16 fresh Bash/native-helper EDIT16 checkpoints, exact Commit/Push/two-Store durability per edit, clean same-mount rebase, clean End, exact Store IDs and parent identity, fresh-container recovery, and post-recovery diagnostics. Prepend reproduced the exact `33,554,442`-byte final file and `7b86abcd0e9d2016bbb8b16722e1439475feff84e31fe9801a4ec74e99dc74c3` digest. Live capture remained `capture_mode=live`, zero files, and zero bytes.

Focused commands passed: `cargo test -p layerfs-branch-store --all-features`; `cargo test -p layerfs-storage --all-features`; `cargo test -p layerfs-layerstack-store --all-features`; `cargo test -p layerfs-workspace --all-features`; `cargo test -p layerfs-sdk --all-features`; `cargo test -p fs-benchmark-pro`. Full gates passed: `cargo fmt --all -- --check`; `cargo test --workspace --all-targets --all-features`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `git diff --check`.

### Comparable E2E results

One focused sample only; median, Q1, Q3, minimum, and maximum are the same value; no CI, paired ratio, or Computer superiority claim is available; wins/ties/losses are N/A for a LayerFS-only arm.

Prepend complete public lifecycle was `840,386,292 ns`, improving `149,819,001 ns` (15.13%) over Round 029 and reaching the requested 0.82–0.87 s opportunity band. Components were Workspace create `44,119,167`; fresh Bash/native helper copy+fsync `384,077,625`; Commit `313,932,667`; Push/two-Store durability `91,506,958`; strict End `6,749,875`. The component sum is exact.

EDIT16 complete public lifecycle was `1,026,404,593 ns`, 1.36% above Round 028's best and 10.88% below Round 029's execution-noisy confirmation. It remains below the 1.20 s hard gate and inside the established 0.80–1.05 s focused direction. The small immediate plan behavior did not regress.

### LayerFS phase decomposition

Prepend Commit was `313,505,875 ns`: pause/fence `115,750`; quiesce `125`; live no-op capture `375`; candidate plan `603,583`; dirty compare `750`; content `211,708,584`; namespace `587,792`; candidate finish `64,028,250`; local admission `29,386,500`; completeness `0`; publication `1,032,083`; in-place rebase `1,679,250`; resume `85,708`; unattributed `4,277,125`.

Prepend Push was `91,031,167 ns`: history `144,334`; frontier `51,042`; membership `2,280,998`; sender source/read/auth `582,334`; object admission `10,547,333`; fact admission `463,750`; independent authority transition verification `61,657,917`; visibility-last publication `550,334`; durability `13,490,875`; unattributed `1,262,250`; 10 endpoint calls. This replaces Round 029's roughly 254 ms Push without moving work past acknowledgement.

### Algorithm, transfer, storage, memory, and I/O counters

The prepend candidate remained `1,747 IDs / 33,661,935 bytes`: local insert `44 / 377,723`; local/parent reuse `1,703 / 33,284,212`; all reuse was explicit source reuse. Push announced exactly the same 1,747 IDs/33,661,935 bytes in four 512-ID-or-smaller object membership pages, found/sent/inserted exactly 44 IDs/377,723 bytes, used one object payload batch, peaked at 380,539 buffered bytes, and recorded one trusted-boundary prune. One 133-byte Commit fact was admitted separately. Candidate, transfer, raced-existing, and missing-only equations all balance.

Authority object admission was one 44-row/377,723-byte writer transaction: total `10,547,333 ns`; writer acquire `3,917`; statements `526,625`; commit/sync `10,016,375`; unattributed `375`; 45 statements. Fact admission was `463,750 ns`; authority publication/CAS was `550,334 ns`. LayerStack durability was `8,025,000 ns`; BranchStore durability was `5,465,875 ns`; checkpoint, database fsync, directory fsync, and unattributed fragments are present and balanced.

The semantic and SQL work is unchanged. The final durable allocated snapshot was `37,277,696 bytes`, only `196,608 bytes` above Round 029's `37,081,088`, but this sample's registered prepend before/after allocated delta was `1,441,792 bytes` versus `393,216 bytes` in Round 029. Apparent database growth was essentially unchanged (`786,432` versus `798,720` bytes), BranchStore final allocation was identical, and exact inserted object/fact rows and bytes were identical. The differing host `st_blocks` allocation state is therefore retained as an explicit diagnostic regression requiring an identical-source repeat; it is not hidden or relabeled semantic growth.

### Comparison with Computer, previous round, and current best

Computer was not executed and its diagnostic image remains non-formal. Against Round 029's exact prior focused source, Push improved 63.92% (`253,629,292` to `91,506,958 ns`) and complete prepend improved 15.13%. The result meets the requested Push and complete-prepend ranges. Round 030 becomes the focused prepend best; Round 028 remains the focused EDIT16 best.

### Defects and root causes

The obsolete eligibility check conflated payload buffering with an ID-only process-local cache: `built.objects.ids_in_order(512)` plus a 4-MiB candidate-byte condition rejected a safe 1,747-ID plan. The plan transfer then assumed one membership page. Both restrictions were at the sender orchestration boundary; receiver transactions and buffers were already bounded and required no redesign.

The first focused test compile exposed two test-only field/type mistakes: an ambiguous synthetic integer width and use of `InventoryEntry.id` instead of `object_id`. The next run exposed a passive-counter precondition: Push's zero parent-read assertion still included Commit's 15 parent reads. Each was corrected at the test source/reset boundary; production code was unchanged by those corrections.

The remaining allocation discrepancy is isolated to host allocated-block accounting: candidate IDs/bytes, inserted rows/bytes, apparent DB growth, final DB sizes, final BranchStore allocation, durability, and recovery are stable. No evidence supports changing Store or transfer code from this one filesystem-allocation sample.

### What needs improvement next

Run one identical sealed-source focused repeat before any source edit. Accept the mechanism if Push/prepend and exact transfer counts reproduce and the registered allocation delta returns near the prior envelope or repeated evidence explains the allocator variance. If the allocation delta reproduces materially high while logical work remains identical, inspect per-Store S3–S8 block snapshots before changing algorithms.

After that repeat, stop speculative EDIT tuning. Use paired/public evidence to choose between sequential FUSE throughput and dense rewrite; do not infer the next bottleneck from this prepend-only sample.

### Stable strengths — no improvement currently needed

Preserve exact dirty-range/FileMutationBatch editing, 10-byte CDC/candidate behavior, reusable same-mount checkpoints, live capture no-op, one-call fresh-helper FUSE attach, zero-call strict End, fresh Bash/helper per SDK Exec, direct Engine transport, Reference zero-copy reads, parent-aware local admission, missing-only canonical payload, independent authority transition proof, visibility-last publication, short SQLite transactions, two-Store durability, exact recovery, and append-only evidence custody.

### Subagent reviews and reconciled decision

No subagent was used in this continuation. The implementation was deliberately confined to the already identified eligibility and paging points, and the complete direct-dependent/full gates plus raw benchmark receipts provide the independent correctness, transaction, boundedness, and fairness checks for this focused round.

### Next action

Repeat the identical `c58f4787…b4061` source seal and `sha256:fb0e1c…b79c` image under a new append-only run ID. Make no source change before that repeat is sealed and appended.

## Round 031 — paged-push-plan-repeat-09700086-20260831

- Status: PASSED (identical-source repeat; paged PushPlan mechanism PASS; requested Push/prepend bands PASS; EDIT16 hard gate PASS; allocation discrepancy diagnosed; no paired/formal claim)
- UTC timestamp: 2026-08-30T22:31:51Z through 2026-08-30T22:32:53Z
- Local timestamp and timezone: 2026-08-31 06:31:51–06:32:53 CST (Asia/Shanghai)
- Git commit and tree: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`
- Dirty source seal and diff hashes: unchanged source seal `c58f4787eae91f6886d682cb7e88e8bcd625861c1995c030398067288f0b4061`; working-tree patch `92d28bab5ed27a2c49d0a862ed4b87759b6c45b43649bdc9634a7bb3f89ea5ca`; empty index `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`; status `9539aa8139152a21aa5cc012811b25982e337749b2134182f8fd1c32c5339b42`; untracked inventory `b73b197ff508d8e455d5e51f6f9cdac56598a4e4081d37712e983fe7c76d9c30`
- Benchmark/profile and exact commands: no rebuild and no source/test change; `benchmark/fs-benchmark-pro/run.sh --source-seal`; `benchmark/fs-benchmark-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:fb0e1c57946f41903ee682d2f2517cf2145d796bc3666991b9c11c16aba5b79c paged-push-plan-repeat-09700086-20260831`
- Candidate order seed and pair count: `75a951a5ffde89e3e905effa30ed58ef6d7691ca015f57985c89167cff66130d`; one LayerFS-only focused sample; Computer was neither rebuilt nor executed
- Host, kernel, Docker, CPU/memory/I/O envelope: identical Round 030 Apple arm64 / Darwin 25.4.0 / Docker Desktop 4.76.0 and Engine 29.5.2 linux/arm64; one CPU; 1 GiB memory and swap; PID limit 512; 256 MiB `/tmp`; OS page cache uncontrolled
- Candidate image tags, digests, architectures, and verified OCI labels: exact Round 030 image `sha256:fb0e1c57946f41903ee682d2f2517cf2145d796bc3666991b9c11c16aba5b79c`; arm64; all source/helper labels re-admitted
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `0ea29b67c77898180350b3d75ea770e19154d85e545725048f2b1219daa64dd5`; recovery `9ad18c773b30235ad69c891f0a2ef0a41a13d0ddd1d563d0732de8b20751a3ad`; `/workspace/fs-benchmark-pro-7`; real `fuse layerfs rw,user_id=0,group_id=0,default_permissions`; helper `61f6454a7a1f982b4fc342b05d1a883e7b692f359e4c4de8cfc0cc22f86ed737`
- Raw evidence directory and SHA-256 inventory: `runs/paged-push-plan-repeat-09700086-20260831/`; 264 entries; `raw-inventory.sha256` SHA-256 `cd3b206f0d24daf9616bc1b7f944cae02fcc3d918436acdd0541139e9f8b5a91`; inventory reverified and no symlink exists
- Previous comparable round: Round 030, exact same source/image, prepend `840,386,292 ns`, Push `91,506,958 ns`, EDIT16 `1,026,404,593 ns`
- Current best comparable round: Round 030 remains prepend best; Round 028 remains EDIT16 best; Round 031 confirms the mechanism and expected performance band

### Hypothesis and planned change

No change was planned. This was the exact Round 030 repeat required to distinguish a real PushPlan gain from sample noise and to determine whether the unexpected prepend `st_blocks` delta was transient.

### Changes since the previous round

None in admitted source, binary, image, helper, protocol, configuration, public path, durability, or tests. Only run ID/seed and fresh containers/Stores/identities changed. The append-only Round 030 ledger entry is outside the admitted source-seal scope.

### Correctness and validity

All Round 030 gates reproduced: exact fixture/final/reopen hashes; real FUSE mountinfo; one EDIT16 Workspace with 16 fresh Bash/helper processes and durable checkpoints; live capture zeros; same-mount rebase; strict clean End; exact Store identities; two durability receipts per Push; exact missing-only equations; clean fresh-container recovery; and isolated post-recovery diagnostics. The sealed raw inventory verifies all 264 entries.

The full repository test/Clippy/format/diff gates were already executed against this exact source before the Round 030 image build; the repeat used that byte-identical image and did not change source.

### Comparable E2E results

One repeat sample; median/Q1/Q3/min/max are the sample; no paired Computer ratio or CI is claimed. Complete prepend was `863,766,000 ns`, inside the requested 0.82–0.87 s band and 12.77% below Round 029. Components: create `43,635,083`; fresh Bash/native helper `395,158,792`; Commit `326,259,959`; Push/durability `92,230,750`; End `6,481,416`; exact sum.

EDIT16 was `1,057,523,916 ns`: below the 1.20 s hard gate, `7,523,916 ns` above the 1.05 s focused direction in this sample, and within 4.44% of Round 028's best. The two identical-source paged-plan samples bracket EDIT16 at `1.026–1.058 s`; no material small-plan regression is present.

Across the two identical-source focused runs, the midpoint is `852,076,146 ns` for prepend, `91,868,854 ns` for prepend Push, and `1,041,964,255 ns` for EDIT16. These are descriptive two-run values, not formal statistics.

### LayerFS phase decomposition

Prepend Commit receipt `325,564,583 ns`: pause/fence `115,708`; quiesce `209`; capture `334` with live zero-file/zero-byte proof; candidate plan `616,000`; compare `542`; content `216,951,875`; namespace `739,709`; candidate finish `67,100,875`; local admission `33,714,583`; completeness `0`; publication `715,250`; in-place rebase `1,588,042`; resume `88,875`; unattributed `3,932,581`.

Prepend Push receipt `91,758,000 ns`: history `141,709`; frontier `50,667`; membership `2,171,250`; source/read/auth `588,125`; object admission `10,322,458`; fact admission `403,166`; independent authority verification `63,122,000`; publication `552,917`; durability `13,123,959`; unattributed `1,281,749`; 10 endpoint calls.

### Algorithm, transfer, storage, memory, and I/O counters

The exact mechanism counters reproduced byte-for-byte: candidate `1,747 IDs / 33,661,935 bytes`; local insert `44 / 377,723`; source reuse `1,703 / 33,284,212`; Push announcement `1,747 / 33,661,935`; four object membership pages; exact missing/sent/inserted `44 / 377,723`; one object payload batch; 380,539-byte peak; one trusted boundary prune; one 133-byte Commit fact. All object/fact/transfer equations balance.

Authority object admission again used 44 rows/377,723 bytes and 45 statements in one bounded writer transaction: `10,322,458 ns` total; writer acquire `4,708`; statements `482,292`; commit/sync `9,835,042`; unattributed `375`. LayerStack and Branch durability were `7,364,250` and `5,759,709 ns`, with checkpoint/database/directory fsync fragments intact.

The registered prepend allocated-block delta reproduced exactly at `1,441,792 bytes`, while apparent DB growth was `790,528 bytes`. Final total allocated bytes were `37,310,464`, only 32 KiB above Round 030 and 229,376 bytes above Round 029; BranchStore final size/allocation is identical across the compared runs.

Sealed post-run read-only diagnosis found no storage amplification: current Round 030 and 031 authority databases each have 8,708 4-KiB pages and zero freelist pages, compared with 8,710 pages in Round 029; the Branch database is 199 pages with zero freelist in all cases. Current authority apparent size is therefore 8 KiB smaller, not larger. The two current trials also have different trial-derived metadata ObjectIds despite identical aggregate candidate/inserted bytes, which changes SQLite key layout and host APFS `st_blocks` allocation. The operation delta is a real reported physical-layout outcome, but it is not caused by extra LayerFS objects, payload, SQLite pages, duplicate placement, WAL, or deferred durability. Optimizing insertion order or punching holes merely to reproduce a favorable APFS block count would violate the evidence discipline.

### Comparison with Computer, previous round, and current best

Computer remained unexecuted. Round 031 Push is 0.79% above Round 030 and 63.64% below Round 029; complete prepend is 2.78% above Round 030 and 12.77% below Round 029. Both exact-source runs meet the requested Push 90–130 ms and complete prepend 0.82–0.87 s opportunity ranges.

### Defects and root causes

No production defect appeared. The identical repeat establishes that the prior allocation delta is not transient, but page census and exact semantic counters disprove extra storage work. Its root is trial-specific SQLite key/file extent layout reported by host `st_blocks`, not the paged plan's candidate retention or transfer. The raw value remains disclosed; no gate or timer was changed.

### What needs improvement next

No further PushPlan edit is justified. For this workload the remaining Push floor is authority transition verification (`61.7–63.1 ms`) plus durable admission/fsync (`23–24 ms` together), both required trust/durability work. The candidate plan now spends about 2.2 ms on four receiver membership pages and 0.6 ms reading exact missing payloads.

The next source decision requires paired public-path evidence. The existing focused harness cannot produce a valid Computer comparison because its Computer image is diagnostic-prebuilt and the native+Bash runner remains LayerFS-only until the paired scenario isolation/formal provenance work is complete. Do not infer a sequential FUSE/dense-rewrite change from LayerFS-only prepend data.

### Stable strengths — no improvement currently needed

Preserve every stable strength named in Round 030, especially ID-only bounded plans, exact candidate/postorder identity, missing-only transfer, parent-aware admission, authority-side independent verification, short visibility-last transactions, two-Store durability, same-mount Workspace semantics, and complete recovery.

### Subagent reviews and reconciled decision

No subagent was used. The byte-identical repeat plus page-census/storage-set diagnosis reconciles the only open Round 030 issue without an algorithm or benchmark change.

### Next action

Stop speculative EDIT and Push changes. Prepare a sealed-source, same-helper Computer arm and the already specified paired scenario isolation before choosing the next bottleneck. If that prerequisite is not available, report the paged PushPlan as proven and leave sequential FUSE/dense rewrite unchanged.

## Round 032 — custody-namespace-correction-09700086-20260831

- Status: CUSTODY-ERRATUM (no candidate execution; no performance interpretation)
- UTC/local timestamp: 2026-08-30T22:39:55Z; 2026-08-31 06:39:55 CST (Asia/Shanghai)
- Git commit/tree: `0970008668f54bae841797dafd57acab191fba7f`; `f81dad341dff677b82c91f31a4beee0de2f1cc9f`
- Corrected source seal: `867a9ef49cc5f682405d1c061fdf1aefe3b958f0d14af1211bbcdffecd071137`
- Canonical history: `benchmark-results/fs-bench-plus/optimization-history.md`
- Canonical raw runs: `benchmark-results/fs-bench-plus/runs/<run-id>/`
- Previous measured round: Round 031; its source, image, raw inventory, findings, and verdict are unchanged

### Correction

The user corrected the fs-bench-plus custody namespace from the implementation-derived `benchmark-results/fs-bench-pro` name to the product/protocol name `benchmark-results/fs-bench-plus`. The runner, README, binding specification, optimization handoff, and related experiment-handoff references now use the corrected root.

The complete custody directory was renamed in place on the same filesystem. Before and after the rename it contained exactly 31 run directories and 31 `raw-inventory.sha256` files. The append-only history SHA-256 remained exactly `64d306fb21ced02db6ba491c3d505b3d798996e7e3a58cf3b0892e862c9649c2` across the move. No raw or derived artifact was rewritten, no run ID changed, and every inventory remains relative to its unchanged run root.

Historical round prose that accurately recorded the old path at the time was not rewritten. This appended erratum supersedes those path references for all future commands and links.

### Edit optimization disposition

EDIT optimization is closed. Round 028 remains the focused EDIT16 latency best; Rounds 030–031 prove the paged large-candidate PushPlan and requested prepend/Push bands. No remaining EDIT or Push change is evidence-backed without a valid paired Computer campaign.

### Next action

Start the next fs-bench-plus stage at paired benchmark preparation: sealed-source Computer build, identical native-helper and standard-Bash contract, symmetric one-Workspace lifecycle, adjacent smoke, then 10-pair pilot. Use that paired evidence to choose the next product bottleneck; do not reopen EDIT tuning absent contrary evidence.
