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

## Round 033 — custody-namespace-restoration-07b1fc2a-20260831

- Status: CUSTODY-ERRATUM (no candidate execution; supersedes Round 032 path naming)
- UTC/local timestamp: 2026-08-30T22:43:09Z; 2026-08-31 06:43:09 CST (Asia/Shanghai)
- Git base: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`
- Corrected source seal: `ba7b3fc3463973561d4d16232e5b9b74ec58c8bef3fb3feb3a96e44bdb491a07`
- Canonical history: `benchmark-results/fs-benchmark-pro/optimization-history.md`
- Canonical raw runs: `benchmark-results/fs-benchmark-pro/runs/<run-id>/`
- Previous measured round: Round 031; all measured source, image, inventory, and performance findings remain unchanged

### Correction

The user superseded Round 032's `fs-bench-plus` custody namespace and restored the implementation namespace `benchmark-results/fs-benchmark-pro`. The runner, README, binding specification, optimization handoff, related experiment handoff, and Git-ignore exception now use `fs-benchmark-pro` consistently.

The destination already contained one legacy baseline at `benchmark-results/fs-benchmark-pro/baseline-09700086-20260831/`. It had no `runs/` directory or optimization history, so the move was additive and overwrote nothing. The 31-run campaign moved intact to `fs-benchmark-pro/runs/`; the append-only history moved beside it. Before and after the move there were exactly 31 run directories and 31 raw inventories, and the pre-append history SHA-256 remained `6a13708389be7423bf5776fb71b41f9e3ca5d352254b51bbaaeefb595711b006`.

No sealed raw/derived artifact or historical round was rewritten. Round 032 remains as an immutable record of the briefly selected path; this erratum controls every future command and link.

### Edit optimization disposition and next action

EDIT and Push optimization remain closed with the same evidence-backed disposition. Continue with sealed-source paired Computer preparation and use adjacent/pilot evidence to choose the next fs-bench-plus bottleneck. Do not reopen EDIT tuning without contrary paired evidence.

## Round 034 — final-custody-path-07b1fc2a-20260831

- Status: CUSTODY-ERRATUM (no candidate execution; supersedes Rounds 032–033 path naming)
- UTC/local timestamp: 2026-08-30T22:46:16Z; 2026-08-31 06:46:16 CST (Asia/Shanghai)
- Git base: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`
- Corrected source seal: `b6fee8b487fd78f5106858bd492989035a743b9dd276b307a5aaf9b9c8cf2197`
- Final canonical history: `benchmark-results/fs-bench-pro/optimization-history.md`
- Final canonical raw runs: `benchmark-results/fs-bench-pro/runs/<run-id>/`
- Previous measured round: Round 031; measured results remain unchanged

### Final namespace correction

The user clarified that the intended implementation namespace is `fs-bench-pro`, not `fs-bench-plus` or `fs-benchmark-pro`. All live runner, README, specification, handoff, analysis-link, related experiment, and Git-ignore references now use `benchmark-results/fs-bench-pro`.

The complete `fs-benchmark-pro` custody tree—including its pre-existing legacy baseline, 31-run campaign, append-only history, and Finder metadata—was renamed intact to `fs-bench-pro`. The pre-append history SHA-256 was identical before and after the move: `0c3cf9fe12bbb4f6d91e0929d999ca6c736d8c0f935abd18b76a73d1f1888a44`. No artifact was overwritten or rewritten. Rounds 032–033 remain as immutable correction history, while this round controls future paths.

### Next action

Run the full current test gates, prepare a sealed-source Computer candidate with the identical native-helper/Bash and complete public Workspace boundary, then collect adjacent paired evidence for the current LayerFS source. Use only that current complete-lifecycle evidence to compare LayerFS with Computer and choose the next optimization target.

## Round 035 — existing-images-diagnostic-07b1fc2a-20260831

- Status: INVALID (partial manually orchestrated diagnostic; Computer PASS; LayerFS fresh-recovery failure; no paired claim)
- UTC timestamp: 2026-08-30T22:59:06Z through 2026-08-30T23:00:40Z; custody sealed 2026-08-30T23:13:31Z
- Local timestamp and timezone: 2026-08-31 06:59:06–07:00:40 CST (Asia/Shanghai); custody sealed 07:13:31 CST
- Git commit and tree: host base `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; LayerFS image retained admitted source commit/tree `0970008668f54bae841797dafd57acab191fba7f` / `f81dad341dff677b82c91f31a4beee0de2f1cc9f`
- Dirty source seal and diff hashes: this manual diagnostic did not freeze the host adapter/source manifest and is invalid for that reason; retained LayerFS image label source seal `c58f4787eae91f6886d682cb7e88e8bcd625861c1995c030398067288f0b4061`; no source claim is inferred from the later host state
- Benchmark/profile and exact commands: manually orchestrated reuse of the two existing local images, with the current Computer adapter and extracted sealed helper mounted read-only; the complete command manifest was not retained, which independently invalidates publication
- Candidate order seed and pair count: no frozen schedule; one Computer diagnostic followed by one LayerFS diagnostic; not an adjacent registered pair
- Host, kernel, Docker, CPU/memory/I/O envelope: same Apple arm64 / Darwin 25.4.0 / Docker Desktop environment as Rounds 030–031; this manual run did not freeze a complete environment manifest
- Candidate image tags, digests, architectures, and verified OCI labels: LayerFS `sha256:fb0e1c57946f41903ee682d2f2517cf2145d796bc3666991b9c11c16aba5b79c`, arm64, sealed helper/source labels retained; Computer `sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd`, arm64, pinned upstream commit/tree but build mode `diagnostic-prebuilt-dist`
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: LayerFS measurement `d860702927f76a632290da2d33857543634016081f830f9d9671fb74421f4487`; recovery `3d7fa8253406160bb3ca42e0e7db8e823159fbbdd4100e231c3af1efcd654466`; mount `/workspace/fs-benchmark-pro-7`; measurement mountinfo retained; helper `61f6454a7a1f982b4fc342b05d1a883e7b692f359e4c4de8cfc0cc22f86ed737`; Computer container IDs were not retained
- Raw evidence directory and SHA-256 inventory: `runs/existing-images-diagnostic-07b1fc2a-20260831/`; 34 entries; `raw-inventory.sha256` SHA-256 `40d32a48ec448d36af1bdb0dfb1369e54f71c99a763bc3f0accb4b5262659421`; inventory reverified; no symlink exists
- Previous comparable round: Rounds 030–031 are the latest sealed LayerFS-only focused runs; no current paired round exists
- Current best comparable round: unchanged; Round 028 remains the historical focused EDIT16 best and is not a formal headline

### Hypothesis and planned change

Reuse the already-available images to obtain a current Computer diagnostic without another image build, and attempt a fresh LayerFS arm before selecting the next bottleneck.

### Changes since the previous round

No production image changed. The Computer adapter and helper were mounted into the existing diagnostic image. The LayerFS arm used the exact Round 030–031 image. This diagnostic occurred while the host benchmark paths and adapter were being corrected and therefore was never eligible to become sealed-source paired evidence.

### Correctness and validity

Computer measurement and external fresh-container recovery passed the exact 33,554,442-byte final oracle with SHA-256 `7b86abcd0e9d2016bbb8b16722e1439475feff84e31fe9801a4ec74e99dc74c3`. LayerFS produced measurement state but fresh-container verification failed with `Workspace(Io(Os { code: 32, kind: BrokenPipe, message: "Broken pipe" }))`; no LayerFS summary was admitted. Missing frozen manifest/schedule/commands, diagnostic Computer provenance, non-adjacent execution, and the failed LayerFS recovery make the round INVALID. The failure was preserved rather than rerun or hidden.

### Comparable E2E results

No complete pair exists, so median/Q1/Q3/min/max, paired ratio, CI, and wins/ties/losses are unavailable. Computer diagnostic complete times were: COLD-CREATE-32M `2,667,522,668 ns`; EDIT16-K1 `2,694,079,918 ns`; PREPEND-TEMP-RENAME `2,773,801,709 ns`; READ-SYNC-32M `255,294,958 ns`. These values are diagnostic only and support no publishable superiority claim.

### LayerFS phase decomposition

Unavailable as admitted evidence because the recovery arm failed and no LayerFS summary was produced. The retained state/JSONL is debugging input only; its timing and storage counters must not be compared with Rounds 030–031.

### Algorithm, transfer, storage, memory, and I/O counters

Computer retained its exact fixture/edit/prepend/final oracles and external recovery proof. The failed LayerFS files retain the exact Store IDs, mount root, operation records, and error. No cross-candidate storage or physical-I/O equation is valid.

### Comparison with Computer, previous round, and current best

None. Descriptive comparison of this Computer diagnostic to sealed Rounds 030–031 may guide investigation, but it is neither adjacent nor formally admitted and cannot change a current best or optimization verdict.

### Defects and root causes

The diagnostic procedure lacked the runner's manifest/custody chain and used a non-formal Computer image. The LayerFS recovery controller encountered a broken proxy/control connection. Because the failed LayerFS measurement also showed unexpected parent-reuse behavior, all of its measurements are quarantined rather than interpreted.

### What needs improvement next

Use the two sealed LayerFS runs to attribute the already-repeated primary bottlenecks. COLD-CREATE-32M spends roughly two thirds of complete time in per-batch SQLite commit/sync across bounded object-admission transactions. READ-SYNC-32M spends roughly 408–416 ms in public SDK Exec-to-terminal. Add only passive evidence needed to balance those paths, then change one shared production primitive at a time.

### Stable strengths — no improvement currently needed

Freeze the exact dirty-range/FileMutationBatch path, live capture no-op, same-mount in-place rebase, small Commit behavior, ID-only paged PushPlan, parent-aware prepend admission, missing-only transfer, authority verification, visibility-last publication, two-Store durability, exact oracles, and fresh short-lived Bash/helper execution contract.

### Subagent reviews and reconciled decision

No subagent was used. The invalidity is mechanical and the failed LayerFS arm is not interpreted. Current repeated evidence, rather than this failed run, selects create durability amplification and sequential read as the next targets.

### Next action

Instrument and test operation-scoped bounded object admission so intermediate orphan-safe batches do not each pay a durable synchronous commit while final Commit/Push acknowledgment retains existing visibility-last and two-Store durability. In parallel only at the diagnostic level, trace exact read request, buffer-copy, Store fallback, and authentication work; do not alter read semantics until the trace identifies the redundant layer.

## Round 036 — full-sync-contract-no-go-07b1fc2a-20260831

- Status: INVALID (audit No-Go before candidate execution; rejected implementation removed; no performance result)
- UTC timestamp: 2026-08-30T23:22:48Z
- Local timestamp and timezone: 2026-08-31 07:22:48 CST (Asia/Shanghai)
- Git commit and tree: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`
- Dirty source seal and diff hashes: no retained production-crate diff from the rejected attempt (`git diff -- crates` SHA-256 `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`); unrelated path/harness/document changes remain preserved
- Benchmark/profile and exact commands: no candidate execution; focused compile check after removal: `cargo check -p layerfs-storage -p layerfs-branch-store -p layerfs-layerstack-store -p layerfs-monitor --all-features`
- Candidate order seed and pair count: N/A; zero candidates and zero pairs
- Host, kernel, Docker, CPU/memory/I/O envelope: N/A; no benchmark execution
- Candidate image tags, digests, architectures, and verified OCI labels: unchanged existing images; neither was executed for this round
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: N/A
- Raw evidence directory and SHA-256 inventory: no candidate raw directory; this append-only audit entry is the complete custody record
- Previous comparable round: Rounds 030–031 remain the latest sealed LayerFS-only focused runs
- Current best comparable round: unchanged; no performance measurement occurred

### Hypothesis and planned change

An initially considered second SQLite connection configured `synchronous=NORMAL` would have kept bounded transactions while deferring per-batch sync until visibility publication and the final Push barriers.

### Changes since the previous round

The approach was implemented only transiently in the shared worktree and never admitted into an image or benchmark. Audit identified a literal conflict with the binding V2 requirement that both databases use WAL and `synchronous=FULL`. The new connection, configuration, admission method, endpoint routing, receipt-schema field, and focused tests were removed in full. No unrelated change was reverted.

### Correctness and validity

No candidate evidence exists. The rejected approach is a No-Go regardless of potential latency because it weakens a binding durability invariant. A repository search confirms that no new `unpublished_connection`, `configure_unpublished`, `admit_unpublished_objects`, `sync_deferred`, or `synchronous=NORMAL` remains under production crates. The affected crates compile with all features after removal.

### Comparable E2E results

N/A. No sample, ratio, confidence interval, or performance claim exists.

### LayerFS phase decomposition

Unchanged from Rounds 030–031. Their repeated COLD-CREATE-32M receipts remain the authority for investigation: BranchStore Commit and authority Push each perform approximately fifteen bounded object-admission transactions whose FULL commit/sync dominates the complete scenario.

### Algorithm, transfer, storage, memory, and I/O counters

Unchanged. Existing batches remain at most 128 objects / 4 MiB; both Store connections remain WAL + `synchronous=FULL`; Push retains its explicit LayerStackStore and BranchStore checkpoint/database-fsync/directory-fsync barriers.

### Comparison with Computer, previous round, and current best

None. No image or candidate ran.

### Defects and root causes

The rejected hypothesis incorrectly treated unpublished reachability as permission to weaken a Store-wide binding durability mode. The narrower compliant hypothesis is checkpoint cadence under FULL: current `wal_autocheckpoint=1,000` pages is approximately 4 MiB, while object batches are approximately 2.4–2.6 MiB, and sealed receipts show alternating commit/sync cost consistent with periodic automatic checkpoints. Exact per-batch WAL-page and checkpoint evidence is not yet emitted, so no replacement patch is selected.

### What needs improvement next

Wait for the requested root-cause/safety synthesis. Then add passive WAL-page/autocheckpoint occurrence evidence if still required and evaluate only FULL-compliant bounded mechanisms, including a higher fixed bounded checkpoint threshold. Prove WAL bounds for 4/32/256 MiB and repeated unpushed Commits before retention.

### Stable strengths — no improvement currently needed

Preserve WAL + `synchronous=FULL`, bounded transactions, visibility-last publication, standalone Commit crash durability, explicit final two-Store stability barriers, exact candidate/transfer equations, EDIT16, paged prepend, Reference integrity, and recovery.

### Subagent reviews and reconciled decision

The external audit issued an immediate No-Go. The primary agent accepted it, stopped the running test, removed only the rejected implementation, and made no replacement durability change.

### Next action

Pause implementation. Read sealed per-batch receipts and existing FULL/autocheckpoint code only; do not select or benchmark a checkpoint-cadence patch until the three-agent safety/root-cause synthesis provides the exact Go/No-Go tests and read-path direction.

## Round 037 — p0-cdc-scratch-07b1fc2a-20260831

- Status: IMPROVED (P0 evidence correctness PASS; exact recovery PASS; EDIT16/prepend regression guards PASS; no paired/formal claim)
- UTC timestamp: 2026-08-30T23:44:43Z through 2026-08-30T23:45:46Z
- Local timestamp and timezone: 2026-08-31 07:44:43–07:45:46 CST (Asia/Shanghai)
- Git commit and tree: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`
- Dirty source seal and diff hashes: source seal `071280446ec537753a58cf2722edb7df5a394314242a95a94254af6ba9077ed7`; unstaged binary diff `65e70e64c137b733ffd15f7a339bfff3e8e33c69d3d1132b659574743f50dcff`; staged diff empty; status `f7e11c21f7727378267743bb085512aed135b408a618e4aad27649a401e3ec66`
- Benchmark/profile and exact commands: focused content/storage/Monitor/BranchStore/LayerStackStore/Workspace/SDK tests and scoped warning-denying Clippy; cached `docker build --pull=false ... -f benchmark/fs-bench-pro/Dockerfile.layerfs -t layerfs-fs-benchmark-pro:p0-counters-07b1fc2a .`; `benchmark/fs-bench-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd layerfs-fs-benchmark-pro:p0-counters-07b1fc2a p0-cdc-scratch-07b1fc2a-20260831`
- Candidate order seed and pair count: `33b21c0ab14b7e7dbaeeffb489d83dcb47a9adcb4de369c6a1b2df8328e6c5d8`; one LayerFS-only focused sample
- Host, kernel, Docker, CPU/memory/I/O envelope: same Apple arm64 / Darwin 25.4.0 / Docker Desktop envelope as Rounds 030–031; one CPU, 1 GiB memory and swap, PID limit 512, 256 MiB `/tmp`; OS page cache uncontrolled
- Candidate image tags, digests, architectures, and verified OCI labels: `layerfs-fs-benchmark-pro:p0-counters-07b1fc2a`; `sha256:4b148d08b16ec76c235da45a3ad887c15d26ccb58e191b1410ba1f9baff009f9`; arm64; commit/tree/dirty/source seal/helper labels matched; every base/frontend image was a local cache hit and Computer was not rebuilt or executed
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `ba87896f9673867d0941e93c6665e4aacd4aaee14067524e485cbcb652de5b80`; recovery `013206e6eb6e90c4e6c4d3d707b7559f1d7af61c1465618cabfc47614f716fa3`; `/workspace/fs-benchmark-pro-7`; real FUSE mountinfo retained; helper `61f6454a7a1f982b4fc342b05d1a883e7b692f359e4c4de8cfc0cc22f86ed737`
- Raw evidence directory and SHA-256 inventory: `runs/p0-cdc-scratch-07b1fc2a-20260831/`; 264 entries; `raw-inventory.sha256` SHA-256 `0e1c1d264f84c736f2c809fd8f7004a27dee0dfc5155524a9046be09b1d578ca`; inventory reverified from the run root; no symlink exists
- Previous comparable round: Rounds 030–031, identical pre-P0 paged-plan mechanism, create `2,440,690,627` / `2,511,619,626 ns`, EDIT16 `1,026,404,593` / `1,057,523,916`, prepend `840,386,292` / `863,766,000`, read `465,855,751` / `458,156,208`
- Current best comparable round: Round 028 remains the historical focused EDIT16 best; Round 037 is the first mechanism-valid CDC/scratch evidence round and does not replace a formal best

### Hypothesis and planned change

New-file and opaque-replacement CDC work was performed but dropped when `filesystem::write_file` returned the later mode-only candidate. `ObjectBuffer::finish` also copied the reachable closure into a second spill store without counters. Preserve and merge the replacement candidate counters, then passively report first-store writes, reachable-copy writes, combined spill peak, and spill count before changing FULL-WAL cadence.

### Changes since the previous round

`CandidateRoot::after` now merges namespace, inode, rope, and structural counters while retaining the original parent root. Both `write_file` call forms combine replacement and mode candidates. `BuildCounters` and the backward-compatible Monitor local-admission record now retain four bounded aggregate scratch counters. `DeferredObjectStore` distinguishes the initial candidate store from the reachable-copy store and records logical bytes written to each, spill transitions, and the maximum simultaneous logical spill footprint. No payload is logged and no scan/hash was added.

### Correctness and validity

Focused tests proved exact CDC accounting for independent 4 MiB and 32 MiB new files and exact candidate scratch accounting above the 8 MiB spill threshold. Full direct-dependent content/storage/BranchStore/LayerStackStore/Workspace/Monitor/SDK suites and scoped `-D warnings` Clippy passed. The live run passed exact fixture, post-edit, final, real-FUSE, same-mount rebase, two-Store durability, mountinfo, Store identity, clean End, raw inventory, and fresh-container recovery gates.

### Comparable E2E results

One LayerFS-only sample; median/Q1/Q3/min/max equal the sample; no paired ratio, CI, or wins/ties/losses are claimed. Complete COLD-CREATE-32M was `2,215,884,418 ns`; EDIT16-K1 `1,068,121,132`; PREPEND-TEMP-RENAME `842,596,834`; READ-SYNC-32M `457,730,751`; focused aggregate `4,584,333,135`. Recovery was `492,680,334 ns` and passed.

EDIT16 components were one Workspace/FUSE create `57,776,834`; 16 Exec-to-terminal `586,059,917`; 16 Commit `131,987,335`; 16 Push/two-Store durability `286,877,962`; one End `5,419,084`; exact sum `1,068,121,132` (`66.76 ms/edit`). The hard 1.20 s / 75 ms gates pass; no preferred/formal claim is made.

### LayerFS phase decomposition

COLD-CREATE-32M: Workspace create `46,528,333`; public native create+fsync `101,567,959`; Commit `1,041,040,417`; Push/two-Store durability `1,016,425,917`; End `10,321,792`. Commit receipt: content `182,777,125`; candidate finish `55,168,625`; local admission `795,420,250`; publication `644,417`; live capture `833` with zero files/bytes; total receipt `1,040,513,584`. Push receipt: authority verification `110,515,709`; source read/auth `59,343,625`; object admission `720,511,335`; durability `58,080,375`; unattributed `64,252,540`; total `1,015,702,792`.

### Algorithm, transfer, storage, memory, and I/O counters

Create now reports exact CDC `33,554,432 bytes`; candidate `1,747 IDs / 33,661,925 bytes`; local insert `1,744 / 33,661,702`; Reference source reuse `3 / 223`. First candidate store wrote `33,662,033` logical canonical bytes; `reachable_from` copied `33,661,925`; both stores spilled once; maximum simultaneous logical spill footprint was `67,323,958 bytes`. The extra 108 first-store bytes are unreachable intermediate candidates and remain explicitly visible.

Prepend reports exact CDC `33,554,442 bytes`, no more than final size; candidate `1,747 / 33,661,935`; insert `44 / 377,723`; source reuse `1,703 / 33,284,212`; first-store writes `33,662,684`; reachable copy `33,661,935`; two spills; `67,324,619-byte` peak. Every edit reports exactly `10` CDC bytes; representative edit candidates remain 8–10 IDs and below 17 KiB, with no spill.

Create BranchStore used 14 bounded object transactions plus one Commit CAS: combined DB `741,262,460 ns`, commit/sync `724,395,918`, statements `16,433,792`. Authority used 14 bounded object transactions plus one fact and one publication: DB `722,005,210`, commit/sync `703,377,167`, statements `18,147,915`. Batch caps remain 128 objects / 4 MiB. FULL-WAL sync times alternate from approximately 30–51 ms to 64–118 ms, confirming checkpoint-cadence amplification without changing the database policy.

### Comparison with Computer, previous round, and current best

Computer was not executed. The current diagnostic Computer create remains `2,667,522,668 ns` and is not formal evidence. Round 037 create is descriptively 9.21% below Round 030 and 11.78% below Round 031, but P0 did not target performance and one sample cannot establish a gain. EDIT16 is 1.03% above Round 031 and 5.48% above the historical Round 028 best, within its guard band. Prepend matches Rounds 030–031; read is unchanged.

### Defects and root causes

The dropped-CDC evidence defect is fixed. Scratch evidence confirms `reachable_from` performs a second complete candidate copy and sustains approximately twice candidate bytes across simultaneous spill stores. The largest create cost remains FULL commit/sync, not SQL statements. With the binding 1,000-page autocheckpoint and 2.3–2.6 MiB object transactions, approximately every second transaction incurs automatic checkpoint work. No per-batch WAL-frame/auto-checkpoint receipt exists yet, so the next change must add that evidence while testing the council-approved fixed FULL-compliant threshold.

### What needs improvement next

Test one generic production `WAL_AUTOCHECKPOINT_PAGES=16,384` hypothesis while keeping WAL, `synchronous=FULL`, one connection policy, 128-object/4-MiB transactions, standalone Commit durability, visibility-last publication, and explicit final Push barriers. Add passive per-transaction WAL apparent bytes/pages before/after, checkpoint inference/duration where observable, configured threshold, maximum WAL bytes, and transaction maximum. Prove bounded behavior at 4/32/256 MiB and repeated unpushed Commit/failure before retention.

### Stable strengths — no improvement currently needed

Freeze EDIT16 exact dirty-range/FileMutationBatch, live capture no-op, same-mount in-place rebase, paged PushPlan, parent-aware prepend admission, missing-only payload, independent authority verification, two-Store durability, real FUSE, fresh short-lived execution, exact oracles, and recovery. The rejected NORMAL/unpublished writer remains absent.

### Subagent reviews and reconciled decision

The three-agent read-only council required the CDC correction and scratch proof before FULL-WAL optimization. Round 037 satisfies those P0 gates and empirically confirms both the doubled candidate scratch and alternating FULL checkpoint cadence. The reconciled next step is the minimum fixed-threshold FULL-WAL experiment only.

### Next action

Implement and test the 16,384-page fixed autocheckpoint policy plus passive WAL transaction counters as one sealed create-focused round. Do not combine reachable-copy removal or read-path optimization into that source seal.

## Round 038 — full-wal-16384-07b1fc2a-20260831

- Status: REGRESSED (mechanism/correctness PASS; exact recovery PASS; 16,384-page policy rejected as a performance win on the current host-bind envelope; no paired/formal claim)
- UTC timestamp: 2026-08-31T00:03:09Z through 2026-08-31T00:04:15Z; custody verified and appended 2026-08-31T00:05:35Z
- Local timestamp and timezone: 2026-08-31 08:03:09–08:04:15 CST (Asia/Shanghai); custody verified and appended 08:05:35 CST
- Git commit and tree: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`
- Dirty source seal and diff hashes: source seal `933ac0c81ee5deab73fcedcc983b48ce00896f5d1e91c7d51489ec088c71201f`; captured working-tree patch `17cad3ffd13e4eeaa6acd53fd3f88e9b14366c27ed12fa11a78e06b81ef03731`; staged patch empty; captured status `afe45f28940bbe504462d7d32b86e9a7a35c2bc8e90252428c8c46f269e00070`
- Benchmark/profile and exact commands: direct dependent Storage/BranchStore/LayerStackStore/Monitor/Workspace/SDK/fs-bench-pro tests; 4/32/256 MiB and repeated-unpushed-Commit FULL-WAL recovery proof; `cargo fmt --all --check`; scoped all-target/all-feature Clippy with `-D warnings`; cached `docker build --pull=false --platform=linux/arm64 ... -f benchmark/fs-bench-pro/Dockerfile.layerfs -t layerfs-fs-benchmark-pro:full-wal-16384-07b1fc2a .`; `benchmark/fs-bench-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd layerfs-fs-benchmark-pro:full-wal-16384-07b1fc2a full-wal-16384-07b1fc2a-20260831`
- Candidate order seed and pair count: `20a345ad97972ec4931dc3cfe3c13ac40b4e4831d4d6017d26e981076e4276bd`; one LayerFS-only focused sample
- Host, kernel, Docker, CPU/memory/I/O envelope: same Apple arm64 / Darwin 25.4.0 / Docker Desktop host-bind envelope as Round 037; one CPU, 1 GiB memory and swap, PID limit 512, 256 MiB `/tmp`; OS page cache uncontrolled
- Candidate image tags, digests, architectures, and verified OCI labels: `layerfs-fs-benchmark-pro:full-wal-16384-07b1fc2a`; `sha256:6adb85125c31dade2fc0a0ab628f0cdd84e0d15253669a59bf903aca26d4501c`; arm64; commit/tree/dirty/source-seal/helper labels matched; production build used cached bases with `--pull=false`; Computer was neither rebuilt nor executed
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `870da23e38f24b099249b2f5ef5b234d11f46a742a35fcbe307d7534412cb3e4`; recovery `d9416a1602ad12939304d468eb46c8443594eb9c572c5ab758698041ae51c016`; real FUSE `/workspace/fs-benchmark-pro-7` mountinfo retained; helper `61f6454a7a1f982b4fc342b05d1a883e7b692f359e4c4de8cfc0cc22f86ed737`
- Raw evidence directory and SHA-256 inventory: `runs/full-wal-16384-07b1fc2a-20260831/`; 264 entries; `raw-inventory.sha256` SHA-256 `84a6b2904b18f0a1731c886e0c6998d0101fb86846297f9378cdc0d18306c13e`; every entry reverified from the run root
- Previous comparable round: Round 037, complete create `2,215,884,418 ns`, EDIT16 `1,068,121,132`, prepend `842,596,834`, read `457,730,751`
- Current best comparable round: unchanged; Round 038 is a negative focused mechanism result and does not replace any best

### Hypothesis and planned change

Keep the single Store connection policy, WAL, and `synchronous=FULL`, but raise the fixed generic autocheckpoint threshold from 1,000 to 16,384 pages (approximately 64 MiB). The hypothesis was that fewer automatic checkpoints across fourteen bounded 2.3–2.6 MiB object transactions would reduce create latency while final Push retained both explicit Store stability barriers. Add passive WAL file/page and transaction counters without scans or payload logging.

### Changes since the previous round

`WAL_AUTOCHECKPOINT_PAGES` changed to 16,384 for every Store connection; no second writer or synchronous downgrade exists. Each database receipt now balances passive WAL observation time and reports configured threshold, page size, apparent/allocated WAL bytes, derived file frames before/after, process-local maxima, and observed threshold crossing. Monitor encoding advanced compatibly while retaining the previous decoder. `auto_checkpoint_ns` remains explicitly unavailable rather than inventing a duration the built-in SQLite hook does not expose. Object batches remain at most 128 objects / 4 MiB, and all existing final durability calls remain inside public timers.

### Correctness and validity

The heavy proof committed independent deterministic 4, 32, and 256 MiB files without Push, dropped and reopened the exact BranchStore, and authenticated the exact head/root/bytes. It also recovered the final exact state after twenty repeated unpushed 4 MiB Commits. Transactions stayed within 128 objects / 4 MiB; every connection remained WAL + `synchronous=FULL`; bounded WAL allocation stayed under the asserted 96 MiB generic ceiling. Direct dependent tests, formatting, warning-denying Clippy, diff checks, exact registered oracles, mountinfo, Store identities, visibility-last publication, clean End, inventory, and fresh-container recovery all passed. The result is mechanism-valid even though it regressed latency.

The first attempted image command omitted the two frozen workload hash build arguments and was stopped by the Dockerfile provenance guard before compilation or candidate execution. The successful build supplied the exact already-admitted source and binary hashes. This setup failure is outside candidate timers and did not alter the measured image or raw run.

### Comparable E2E results

One LayerFS-only sample; median/Q1/Q3/min/max equal the sample; no paired ratio, confidence interval, or wins/ties/losses are claimed. Complete COLD-CREATE-32M regressed to `2,923,640,209 ns` from Round 037's `2,215,884,418` (+31.94%). EDIT16 improved to `975,519,874 ns` (`60.97 ms/edit`) and remains below the 1.20 s guard. Prepend was `833,440,502`; read was `471,288,918`; focused aggregate was `5,203,889,503`. Fresh recovery passed in `507,161,584 ns`.

The three required create boundaries remain separate. AFTER-READY FUSE EXECUTION was `113,480,167 ns`, or `281.99 MiB/s`; it is below Round 037's `315.1 MiB/s` single sample and below the terminal 500 MiB/s gate. AFTER-READY AUTHORITY DURABLE was exact public Exec + Commit + Push `2,868,643,418 ns`, or `11.16 MiB/s`. COMPLETE was `2,923,640,209 ns`, or `10.95 MiB/s`. No execution-only number is relabeled as durable, and none of these single-sample figures is a comparative claim.

### LayerFS phase decomposition

COLD-CREATE-32M: Workspace create `48,424,666`; public native create+fsync Exec `113,480,167`; Commit `1,015,762,251`; Push/two-Store durability `1,739,401,000`; End `6,572,125`. Commit receipt: content `192,543,709`; candidate finish `57,413,875`; local admission `757,588,875`; publication `891,625`; capture `542` with live mode and zero files/bytes; total `1,015,119,376`. Push receipt: source read/auth `249,651,416`; object admission `690,672,210`; authority transition verification `254,649,333`; publication `1,293,417`; durability `475,798,332`; unattributed `64,414,421`; total `1,738,541,376`.

### Algorithm, transfer, storage, memory, and I/O counters

Create preserved exact P0 accounting: CDC `33,554,432 bytes`; candidate `1,747 IDs / 33,661,925 bytes`; insert `1,744 / 33,661,702`; source reuse `3 / 223`; first scratch writes `33,662,033`; reachable-copy writes `33,661,925`; two spills; `67,323,958-byte` simultaneous logical spill peak. Push announced the same 1,747 candidate IDs across four membership pages, sent exactly the 1,744 missing canonical objects in fourteen bounded payload batches, and peaked at `2,584,837 bytes` buffered.

BranchStore still used fourteen object transactions plus one Commit CAS. Their combined DB time was `703,048,670 ns`: commit/sync `684,605,457`, statements `17,159,625`, passive WAL observation `786,998`. Authority used fourteen object transactions plus fact and publication transactions: combined DB `692,494,210`, commit/sync `673,508,707`, statements `17,639,126`, WAL observation `905,623`. Every transaction balanced. Branch WAL reached `36,054,152` apparent / `37,011,456` allocated bytes; authority reached `36,000,592` / `36,720,640`. Neither reached the 16,384-page threshold, so zero threshold crossings were observed in this 32 MiB scenario.

### Comparison with Computer, previous round, and current best

Computer was not executed. Its current diagnostic-prebuilt create remains `2,667,522,668 ns`; Round 038 is descriptively 9.60% slower, but this is not adjacent or formal evidence. Relative to Round 037, the two Stores' combined DB commit/sync fell by only `69,658,921 ns` (4.88%). Push nevertheless grew by `722,975,083 ns`: final two-Store durability grew from `58,080,375` to `475,798,332` (+417.718 ms), source read/auth from `59,343,625` to `249,651,416` (+190.308 ms), and independent authority verification from `110,515,709` to `254,649,333` (+144.134 ms). The large live WALs therefore moved and amplified work instead of removing it.

### Defects and root causes

The 64 MiB threshold is too high for this 32 MiB candidate: each Store accumulated approximately 36 MiB of WAL without an automatic checkpoint. FULL transaction commits still cost approximately 25–53 ms each, while later reads and verification operated against the large WAL and the explicit final TRUNCATE barriers paid approximately 237 ms per Store. The hypothesis reduced neither the required FULL WAL-frame sync nor total authority-durable latency; it merely changed checkpoint placement. The passive counters correctly show no automatic threshold crossing, and `auto_checkpoint_ns` remains unavailable rather than guessed.

### What needs improvement next

Reject 16,384 pages as the retained performance policy on this envelope. The council's alternative 8,192-page threshold (approximately 32 MiB) is now evidence-backed as the smallest next experiment: it should force at most one late automatic checkpoint per Store for this candidate instead of checkpoints every roughly two batches or only at the final barrier. Keep the same FULL policy, bounded transactions, crash proof, WAL counters, visibility-last publication, and final barriers. Seal that one-line cadence change separately before proceeding to candidate scratch, FUSE write, or read work.

### Stable strengths — no improvement currently needed

Freeze P0 CDC/scratch counters, EDIT16 exact dirty range, live capture no-op, same-mount rebase, paged PushPlan, parent-aware prepend admission, missing-only canonical transfer, independent authority verification, exact oracles, real FUSE, fresh execution, and recovery. Do not reintroduce the rejected NORMAL/unpublished writer or weaken either durability barrier.

### Subagent reviews and reconciled decision

No new subagent was used in this round. The earlier read-only council explicitly allowed 8,192 pages if resource evidence disfavored 16,384. Round 038 provides that evidence: the 64 MiB threshold remains bounded and correct but is slower because both 36 MiB WALs survive until read/verification and the final barriers.

### Next action

Change only the fixed threshold and matching resource assertions from 16,384 to 8,192 pages, rerun the FULL-WAL recovery/direct-dependent gates, then collect and append one focused 32 MiB create round. Diagnose any regression from raw phases; do not mix FUSE streaming, content construction, scratch removal, or read optimization into that seal.

## Round 039 — full-wal-8192-07b1fc2a-20260831

- Status: REGRESSED (mechanism/correctness PASS; exact recovery PASS; 8,192-page policy rejected as a performance win on the current host-bind envelope; no paired/formal claim)
- UTC timestamp: 2026-08-31T00:12:50Z through 2026-08-31T00:14:01Z; custody verified and appended 2026-08-31T00:14:37Z
- Local timestamp and timezone: 2026-08-31 08:12:50–08:14:01 CST (Asia/Shanghai); custody verified and appended 08:14:37 CST
- Git commit and tree: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`
- Dirty source seal and diff hashes: source seal `4cb0d2744c5140e888d35f375e220acf13ec375d5a7fba296ebc3d8d70978e13`; captured working-tree patch `1ec3fc3f8f93b24b6b9b0b1a7ff9f9f9579fd311d6522b5a239721cb87feb5de`; staged patch empty; captured status `afe45f28940bbe504462d7d32b86e9a7a35c2bc8e90252428c8c46f269e00070`
- Benchmark/profile and exact commands: FULL-WAL storage suite; 4/32/256 MiB plus repeated-unpushed-Commit recovery test with a 64 MiB WAL-allocation ceiling; direct LayerStackStore/Monitor/Workspace/SDK/fs-bench-pro tests; `cargo fmt --all --check`; scoped all-target/all-feature Clippy with `-D warnings`; cached `docker build --pull=false --platform=linux/arm64 ... -f benchmark/fs-bench-pro/Dockerfile.layerfs -t layerfs-fs-benchmark-pro:full-wal-8192-07b1fc2a .`; `benchmark/fs-bench-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd layerfs-fs-benchmark-pro:full-wal-8192-07b1fc2a full-wal-8192-07b1fc2a-20260831`
- Candidate order seed and pair count: `bc00cc497a73da59c29082039bcf444d11927bc91e7fe17d2d87d568f8bd423e`; one LayerFS-only focused sample
- Host, kernel, Docker, CPU/memory/I/O envelope: same Apple arm64 / Darwin 25.4.0 / Docker Desktop host-bind envelope as Rounds 037–038; one CPU, 1 GiB memory and swap, PID limit 512, 256 MiB `/tmp`; OS page cache uncontrolled
- Candidate image tags, digests, architectures, and verified OCI labels: `layerfs-fs-benchmark-pro:full-wal-8192-07b1fc2a`; `sha256:4b45761f861ac038c5d89626ff276d4396885c44a335a33e05b2f9164f295e60`; arm64; commit/tree/dirty/source-seal/helper labels matched; cached bases and `--pull=false`; Computer was neither rebuilt nor executed
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `d8df2205e9cb170e3f0cb707ca112c284d0b098866a6b40efa120204f3e90bb6`; recovery `d5f55e3fb53d899fce0e8e78d892197d28484535e6abbce65e79e3ebf76d8f61`; real FUSE `/workspace/fs-benchmark-pro-7` mountinfo retained; helper `61f6454a7a1f982b4fc342b05d1a883e7b692f359e4c4de8cfc0cc22f86ed737`
- Raw evidence directory and SHA-256 inventory: `runs/full-wal-8192-07b1fc2a-20260831/`; 264 entries; `raw-inventory.sha256` SHA-256 `73e67d0182777fa5c403467b5405f593b900db2453af3b981b0311ccae54c3f4`; every entry reverified from the run root
- Previous comparable round: Round 038, complete create `2,923,640,209 ns`, EDIT16 `975,519,874`, prepend `833,440,502`, read `471,288,918`
- Current best comparable round: unchanged; Round 039 is a second negative focused cadence result and replaces no best

### Hypothesis and planned change

Round 038 proved that 16,384 pages let both approximately 36 MiB WALs survive until later reads and the explicit final barriers. Change only the fixed threshold to 8,192 pages (approximately 32 MiB), expecting one late bounded automatic checkpoint per Store: fewer checkpoints than the original 1,000-page policy, but no large WAL left entirely to Push stability. Keep the same FULL connections, bounded transactions, visibility-last publication, and final barriers.

### Changes since the previous round

Only `WAL_AUTOCHECKPOINT_PAGES` and its exact assertions changed from 16,384 to 8,192; the generic crash/resource proof tightened its maximum apparent/allocated WAL assertion from 96 to 64 MiB. No receipt, transaction, execution, FUSE, candidate-construction, transfer, verification, or durability path changed.

### Correctness and validity

All FULL-WAL, 4/32/256 MiB, repeated-unpushed-Commit, exact reopen, bounded transaction, Monitor, Workspace, SDK, harness, formatting, Clippy, mountinfo, Store identity, exact oracle, clean End, inventory, and fresh-container recovery gates passed. The 32 MiB create receipt observed exactly one threshold crossing in BranchStore and exactly one in LayerStackStore, proving the intended mechanism fired. WAL allocation remained below the tighter 64 MiB ceiling in the heavy resource test. The negative performance verdict does not invalidate the correctness evidence.

### Comparable E2E results

One LayerFS-only sample; median/Q1/Q3/min/max equal the sample; no paired ratio, confidence interval, or wins/ties/losses are claimed. Complete COLD-CREATE-32M regressed to `3,106,211,752 ns`, 6.24% slower than Round 038 and 40.18% slower than Round 037's original-policy/P0 baseline. EDIT16 was `1,115,179,917 ns` (`69.70 ms/edit`) and remains inside the 1.20 s guard; prepend was `905,379,250`; read `482,866,209`; focused aggregate `5,609,637,128`. Fresh recovery passed in `508,864,125 ns`.

The named boundaries remain explicit. AFTER-READY FUSE EXECUTION was `128,189,792 ns`, or `249.63 MiB/s`. AFTER-READY AUTHORITY DURABLE was `3,045,538,710 ns`, or `10.51 MiB/s`. COMPLETE was `3,106,211,752 ns`, or `10.30 MiB/s`. Natural execution variance is retained in full; none of these figures is subtracted, relabeled, or a paired performance claim.

### LayerFS phase decomposition

COLD-CREATE-32M: Workspace create `52,539,083`; public native create+fsync Exec `128,189,792`; Commit `1,496,236,167`; Push/two-Store durability `1,421,112,751`; End `8,133,959`. Commit receipt: content `204,464,834`; candidate finish `59,778,667`; local admission `1,223,989,292`; publication `695,042`; live capture `625` with zero files/bytes; total `1,495,590,084`. Push receipt: source read/auth `60,856,542`; object admission `1,140,381,501`; authority verification `120,378,334`; durability `31,114,125`; unattributed `64,621,583`; total `1,420,270,459`.

### Algorithm, transfer, storage, memory, and I/O counters

Create invariants stayed exact: CDC `33,554,432`; candidate `1,747 IDs / 33,661,925 bytes`; insert and missing-only transfer `1,744 / 33,661,702`; source reuse `3 / 223`; four membership pages; fourteen bounded payload batches; `2,584,837-byte` peak transfer buffer; first scratch `33,662,033`; reachable copy `33,661,925`; two spills and `67,323,958-byte` simultaneous logical peak.

BranchStore's fifteen DB transactions totaled `1,168,391,127 ns`, including `1,149,973,294` commit/sync and `16,973,916` statements. Authority's sixteen transactions totaled `1,141,797,668`, including `1,122,044,916` commit/sync and `18,361,666` statements. Passive WAL observation cost remained below 1 ms per Store across the complete create. Branch WAL reached `34,636,872` apparent / `35,663,872` allocated bytes; authority `34,579,192` / `34,623,488`. Each Store recorded exactly one threshold crossing.

The crossing Branch object transaction grew from normal approximately 50–63 ms commits to `486,388,625 ns` total / `484,945,459` commit-sync as frames crossed 7,764 to 8,407. The corresponding authority transaction was `481,430,375` / `480,068,791` while frames crossed 7,750 to 8,393. The final two-Store stability barriers then fell to `31,114,125 ns`, proving the expensive work moved into those two automatic checkpoints.

### Comparison with Computer, previous round, and current best

Computer was not executed. Its non-formal diagnostic create remains `2,667,522,668 ns`; Round 039 is descriptively 16.45% slower and supports no public comparison. Against Round 038, final barriers improved by `444,684,207 ns` and Push improved by `318,288,249`, but Commit worsened by `480,473,916`. Combined DB commit/sync worsened from `1,358,114,164` to `2,272,018,210` (+913.904 ms). Against Round 037's 1,000-page policy, combined DB commit/sync was approximately 844.245 ms worse even though final barriers were approximately 26.966 ms faster.

### Defects and root causes

The intended single late checkpoint is itself too expensive on the current macOS host bind through Docker Desktop: each approximately 32 MiB checkpoint consumed about 0.48 s. The 64 MiB policy deferred two approximately 36 MiB checkpoints to final barriers; the 32 MiB policy performs two similarly large automatic checkpoints inside admission. Neither reduces the required FULL WAL-frame commit sync, and both make larger checkpoint units more expensive than the original smaller cadence in this environment. The passive counters now directly establish the crossing location and cost.

### What needs improvement next

Restore the binding pre-experiment 1,000-page threshold. Retain the P0 CDC/scratch fix and passive WAL receipt fields, but do not retain either raised threshold as a performance optimization. Run one corrected focused seal to prove the counters themselves do not materially perturb the original cadence and that complete create returns to the Round 037 distribution. Only then proceed to the next measured code target or a separately registered symmetric named-volume campaign; never pool storage environments.

### Stable strengths — no improvement currently needed

Freeze EDIT16, paged PushPlan, parent-aware prepend, live capture no-op, same-mount rebase, exact missing-only transfer, independent verification, FULL durability, exact recovery, and the newly proven passive WAL evidence. Do not weaken `synchronous=FULL`, remove barriers, enlarge transactions, or hide automatic checkpoint time.

### Subagent reviews and reconciled decision

No new subagent was used. Round 039 resolves the earlier council's 16,384-versus-8,192 branch empirically: both are correctness-safe and resource-bounded, but both regress the current host-bind campaign. The correct action is restoration, not selecting the less-bad raised threshold.

### Next action

Change only the fixed threshold and exact assertions back to 1,000 pages, restore the original generic 32 MiB WAL ceiling, rerun the same recovery/direct-dependent gates, and append the correction round. Do not start FUSE streaming, content scratch, read copies, or named-volume work until the restored baseline is sealed.

## Round 040 — full-wal-restored-1000-07b1fc2a-20260831

- Status: RESTORED (original FULL-WAL cadence restored; passive WAL/P0 evidence retained; exact recovery PASS; no paired/formal claim)
- UTC timestamp: 2026-08-31T00:19:03Z through 2026-08-31T00:20:02Z; custody verified and appended 2026-08-31T00:20:35Z
- Local timestamp and timezone: 2026-08-31 08:19:03–08:20:02 CST (Asia/Shanghai); custody verified and appended 08:20:35 CST
- Git commit and tree: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`
- Dirty source seal and diff hashes: source seal `1c4c91c44619bacd9a5106251b973a1d51b78b4b08209f387f73c981ae394577`; captured working-tree patch `9f011420ee37a434ddabc3f91ef4da73b12d80e5ab277a8493732a69abf01038`; staged patch empty; captured status `afe45f28940bbe504462d7d32b86e9a7a35c2bc8e90252428c8c46f269e00070`
- Benchmark/profile and exact commands: FULL-WAL storage suite; 4/32/256 MiB plus repeated-unpushed-Commit recovery proof with a 32 MiB WAL ceiling; relevant Storage/BranchStore formatting and warning-denying Clippy gates; cached `docker build --pull=false --platform=linux/arm64 ... -f benchmark/fs-bench-pro/Dockerfile.layerfs -t layerfs-fs-benchmark-pro:full-wal-restored-1000-07b1fc2a .`; `benchmark/fs-bench-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd layerfs-fs-benchmark-pro:full-wal-restored-1000-07b1fc2a full-wal-restored-1000-07b1fc2a-20260831`
- Candidate order seed and pair count: `a46454f018f529c42a050cc12aa59f1632c45b459510756ccfee0b9f5b402862`; one LayerFS-only focused sample
- Host, kernel, Docker, CPU/memory/I/O envelope: authoritative Apple arm64 / Darwin 25.4.0 / Docker Desktop macOS host-bind envelope; one CPU, 1 GiB memory and swap, PID limit 512, 256 MiB `/tmp`; OS page cache uncontrolled
- Candidate image tags, digests, architectures, and verified OCI labels: `layerfs-fs-benchmark-pro:full-wal-restored-1000-07b1fc2a`; `sha256:96d72ba9fbfb72de509dbc8cd18f9c86a77e8e0487ef1dcae1b98bcce90db3ad`; arm64; commit/tree/dirty/source-seal/helper labels matched; cached bases and `--pull=false`; Computer was neither rebuilt nor executed
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `7a5066b10915c3c4d62f9379733ece121f8ec5cb22d868a740301b97215fdc83`; recovery `7953079a82e4c6f42bc18b2d56079df1c8cb857e23f96f8659254c28d093d859`; real FUSE `/workspace/fs-benchmark-pro-7` mountinfo retained; helper `61f6454a7a1f982b4fc342b05d1a883e7b692f359e4c4de8cfc0cc22f86ed737`
- Raw evidence directory and SHA-256 inventory: `runs/full-wal-restored-1000-07b1fc2a-20260831/`; 264 entries; `raw-inventory.sha256` SHA-256 `a9ee8cfd0017300e218372275cb622669ddd8cb8f705e6c83d9a2ced38d54fe3`; every entry reverified from the run root
- Previous comparable round: Round 039's rejected 8,192-page policy, create `3,106,211,752 ns`; original-policy evidence baseline Round 037, create `2,215,884,418`
- Current best comparable round: unchanged; Round 040 restores the current mechanism baseline and creates no formal best

### Hypothesis and planned change

Rounds 038–039 proved that larger FULL-WAL checkpoint units regress the authoritative host bind. Restore the exact pre-experiment 1,000-page threshold while retaining only the passive per-transaction WAL evidence and P0 CDC/scratch correction. Confirm that the added counters do not materially perturb Round 037 and close threshold tuning.

### Changes since the previous round

Only the fixed threshold and exact assertions returned from 8,192 to 1,000 pages; the generic heavy-test WAL ceiling returned from 64 to 32 MiB. No durability mode, transaction shape, acknowledgment boundary, public API, benchmark boundary, FUSE path, candidate algorithm, verification, or final barrier changed.

### Correctness and validity

The 4/32/256 MiB and repeated-unpushed-Commit test again recovered exact standalone public Commit state after Store reopen, with all object transactions at or below 128 objects / 4 MiB and WAL allocation below 32 MiB. Relevant tests, formatting, Clippy, exact registered oracles, real mountinfo, Store identities, clean End, inventory, and fresh-container recovery passed. Live capture remained a zero-file/zero-byte no-op. The complete result returned to the Round 037 distribution, so the passive receipt additions are retained and the raised-threshold mechanisms are not.

### Comparable E2E results

One LayerFS-only sample; median/Q1/Q3/min/max equal the sample; no paired ratio, confidence interval, or wins/ties/losses are claimed. Complete COLD-CREATE-32M was `2,254,385,919 ns`, 1.74% above Round 037 and 27.41% below rejected Round 039. EDIT16 was `1,059,300,462 ns` (`66.21 ms/edit`); prepend `857,589,959`; read `469,608,043`; focused aggregate `4,640,884,383`. Fresh recovery passed in `497,092,875 ns`.

The separate create boundaries are: AFTER-READY FUSE EXECUTION `107,604,042 ns`, or `297.39 MiB/s`; AFTER-READY AUTHORITY DURABLE `2,200,805,168 ns`, or `14.54 MiB/s`; COMPLETE `2,254,385,919 ns`, or `14.19 MiB/s`. The execution sample is near but below 300 MiB/s and remains far below the terminal 500 MiB/s requirement. No execution-only throughput is relabeled as durable.

### LayerFS phase decomposition

COLD-CREATE-32M: Workspace create `46,458,834`; public native create+fsync Exec `107,604,042`; Commit `1,061,833,834`; Push/two-Store durability `1,031,367,292`; End `7,121,917`. Commit receipt: content `193,582,042`; candidate finish `58,485,042`; local admission `802,084,958`; publication `671,041`; live capture `459` and zero files/bytes; total `1,061,205,668`. Push receipt: source read/auth `60,558,376`; object admission `731,377,749`; authority verification `112,757,583`; durability `58,339,166`; unattributed `64,342,000`; total `1,030,407,084`.

### Algorithm, transfer, storage, memory, and I/O counters

Create remained exact: CDC `33,554,432`; candidate `1,747 IDs / 33,661,925 bytes`; insert and missing-only send `1,744 / 33,661,702`; source reuse `3 / 223`; four membership pages; fourteen bounded payload batches; `2,584,837-byte` peak transfer buffer; first scratch `33,662,033`; reachable copy `33,661,925`; two spills; `67,323,958-byte` simultaneous logical peak.

BranchStore's fifteen DB transactions totaled `746,585,835 ns`, with `728,867,459` commit/sync, `16,562,502` statements, and `726,623` passive WAL observation. Authority's sixteen transactions totaled `732,918,041`, with `713,732,834` commit/sync, `17,946,955` statements, and `750,250` WAL observation. Maximum WAL apparent/allocated bytes were approximately `5.46 / 6.46 MiB` for BranchStore and `5.46 / 6.31 MiB` for authority; one initial threshold crossing per Store was observed, after which SQLite reused the bounded WAL. Final two-Store barriers totaled `58,339,166 ns`, matching Round 037.

### Comparison with Computer, previous round, and current best

Computer was not executed. Its diagnostic-prebuilt create remains `2,667,522,668 ns`; Round 040 is descriptively 15.49% lower, but that non-adjacent figure is not a publishable comparison. Versus Round 037, create differed by +38.502 ms: Exec +6.036, Commit +20.793, Push +14.941, while lifecycle noise offset approximately 3.268. DB commit/sync, source read/auth, authority verification, final barriers, EDIT16, prepend, and read all returned to their original-policy distributions.

### Defects and root causes

The threshold experiment is resolved: binding `synchronous=FULL` makes every object transaction persist WAL frames, and larger checkpoint units are expensive on the authoritative macOS host bind. At 16,384 pages the cost moved to later reads and final barriers; at 8,192 pages two approximately 0.48 s automatic checkpoints dominated. The original 1,000-page cadence is the least-bad measured current contract. This is a measured host-bind lower-bound constraint, not permission to change durability semantics.

### What needs improvement next

Stop WAL-threshold tuning. Keep the macOS host bind as the authoritative substrate for both LayerFS and Computer; do not add or project named-volume/native-volume campaigns. Optimize software under unchanged public and durability boundaries in this order: passive FUSE write/copy attribution, borrowed streaming Write framing, retained active-overlay spool descriptor; direct full-spool CDC and single-allocation canonical encoding; removal of the second reachable candidate copy; proven same-boundary read clone/auth removal; batched/cached authority verification with independent receiver proof.

The user has not yet supplied a replacement durability contract. Do not introduce `synchronous=NORMAL` or `OFF`, deferred acknowledgment, or operation-scoped group durability without explicit authority defining restart persistence and whether local Commit must survive before Push. If FULL remains dominant, report the measured lower bound and prepare a separate specification proposal only.

### Stable strengths — no improvement currently needed

Freeze the restored 1,000-page FULL policy, passive WAL counters, P0 CDC/scratch evidence, EDIT16, paged PushPlan, parent-aware prepend, live capture no-op, same-mount rebase, missing-only canonical transfer, independent verification, exact recovery, real FUSE, fresh execution, and complete timers.

### Subagent reviews and reconciled decision

No new subagent was used. The authoritative environment decision removes named-volume/native-volume performance work from the roadmap. The durability clarification blocks semantic weakening but does not block passive FUSE/data-plane work under the existing contract.

### Next action

Add only bounded aggregate FUSE write-path counters for negotiated maximum write, kernel request-size buckets, client/frame/host/spool bytes, owned copy bytes, and encode/socket/decode/spool/fsync time. No counter may add a scan, hash, payload log, or timer-boundary change. Seal that instrumentation round before implementing borrowed framing or retained spool descriptors.

## Round 041 — fuse-write-counters-07b1fc2a-20260831

- Status: EVIDENCE-IMPROVED (passive FUSE write attribution PASS; exact recovery PASS; no product algorithm change and no paired/formal claim)
- UTC timestamp: 2026-08-31T00:35:45Z through 2026-08-31T00:36:44Z; custody verified and appended 2026-08-31T00:37:31Z
- Local timestamp and timezone: 2026-08-31 08:35:45–08:36:44 CST (Asia/Shanghai); custody verified and appended 08:37:31 CST
- Git commit and tree: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`
- Dirty source seal and diff hashes: source seal `94b6f9457a066a7dcbc2baa4867f038971cfa17a9b29884b15c8d819137b6263`; captured working-tree patch `f0d1dbe236dd6be259f8f5779f3f82a6ffa672aa9290de93a801de0ab2e09698`; staged patch empty; captured status `03576f2abe7dfa1004a61676e69582ea2841c6b73394c43c4a55adba12cc0dfd`
- Benchmark/profile and exact commands: focused FUSE protocol/proxy/control tests, Workspace spool metric/reset test, Storage/Monitor/SDK/harness direct-dependent suites, formatting, scoped all-target/all-feature Clippy with `-D warnings`, diff check; cached `docker build --pull=false --platform=linux/arm64 ... -f benchmark/fs-bench-pro/Dockerfile.layerfs -t layerfs-fs-benchmark-pro:fuse-write-counters-07b1fc2a .`; `benchmark/fs-bench-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd layerfs-fs-benchmark-pro:fuse-write-counters-07b1fc2a fuse-write-counters-07b1fc2a-20260831`
- Candidate order seed and pair count: `e33ec77553880ad758451a9995744e3025504b0f7a9135379004c05b23d060f7`; one LayerFS-only focused sample
- Host, kernel, Docker, CPU/memory/I/O envelope: authoritative Apple arm64 / Darwin 25.4.0 / Docker Desktop macOS host-bind envelope; one CPU, 1 GiB memory and swap, PID limit 512, 256 MiB `/tmp`; OS page cache uncontrolled
- Candidate image tags, digests, architectures, and verified OCI labels: `layerfs-fs-benchmark-pro:fuse-write-counters-07b1fc2a`; `sha256:8a06293c54fc22d450e89e6ed55d5107156a0427b8d5e505a53dd271917006cb`; arm64; commit/tree/dirty/source-seal/helper labels matched; cached bases and `--pull=false`; Computer was neither rebuilt nor executed
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `2f9e5172b014cd226cbbca09e9eeaef5b72e5fa4dc5a099eccf35a3c5023981b`; recovery `76765c547a9e86d724a95d4a48d4b92350ab8cd050ddaf246fb3b7534d0ac3f4`; real FUSE `/workspace/fs-benchmark-pro-7` mountinfo retained; helper `61f6454a7a1f982b4fc342b05d1a883e7b692f359e4c4de8cfc0cc22f86ed737`
- Raw evidence directory and SHA-256 inventory: `runs/fuse-write-counters-07b1fc2a-20260831/`; 264 entries; `raw-inventory.sha256` SHA-256 `fff45376e8108ae7735261ba4a6f0f8b1956cb03a26c7fa9589f554bea22bad4`; every entry reverified from the run root
- Previous comparable round: Round 040 restored baseline, create `2,254,385,919 ns`, EDIT16 `1,059,300,462`, prepend `857,589,959`, read `469,608,043`
- Current best comparable round: unchanged; this is an attribution round rather than an optimization claim

### Hypothesis and planned change

The 32 MiB public FUSE write path was believed to copy each kernel write into a client Request Vec, again into the protocol frame, and again from the host frame into the host Request Vec, while reopening the Workspace spool for every request. Add bounded aggregate counters and phase timers to prove exact request size/count, copy bytes, frame bytes, and client/host/spool time before changing framing or descriptor lifetime.

### Changes since the previous round

The existing `FilesystemPort` reports the FUSE maximum write configured in the INIT reply and the ProxyClient aggregates kernel callback size buckets. Existing request encode/decode was mechanically split into measured encode versus socket-write and socket-read versus decode phases without changing wire bytes. Client and host atomics count only write frames and payload copy bytes. The authenticated lifecycle control stream gains one internal metrics snapshot/reset command, used only after pause/barrier has drained all writes. ProxyHost joins helper-side and host-side aggregates; Workspace reports actual spool write/open and fsync/open counts and time. A new validated `FuseWriteReceipt` is persisted through Monitor and emitted as JSON.

No payload is logged; no extra scan, hash, filesystem read, public operation, process, worker, daemon, storage backend, durability mode, or timer exclusion exists. The benchmark now fails every ordinary mutating Commit unless exactly one balanced live FUSE receipt is present with nonzero kernel and spool work.

### Correctness and validity

Protocol tests proved measured framing retains exact wire bytes and payload length. Proxy tests proved capability rejection, pause/resume/fence/error semantics, remote snapshot/reset, equal client/host frame bytes, and equal encode/decode payload copies. Workspace tests proved aggregate spool bytes/open/fsync counts and exact reset. Monitor round-trip preserved the new receipt. All direct dependent suites, formatting, warning-denying Clippy, exact registered oracles, real mountinfo, Store identities, clean End, inventory, and fresh-container recovery passed.

### Comparable E2E results

One LayerFS-only sample; median/Q1/Q3/min/max equal the sample; no paired ratio, confidence interval, or wins/ties/losses are claimed. Complete COLD-CREATE-32M was `2,289,431,585 ns`, 1.55% above Round 040; EDIT16 `1,040,244,128` (`65.02 ms/edit`); prepend `828,952,709`; read `459,402,625`; focused aggregate `4,618,031,047`. Fresh recovery passed in `466,677,292 ns`. These cross-row changes are within the single-run host-bind distribution and do not establish an instrumentation performance effect.

The named create boundaries were AFTER-READY FUSE EXECUTION `112,477,625 ns`, or `284.50 MiB/s`; AFTER-READY AUTHORITY DURABLE `2,237,241,376 ns`, or `14.30 MiB/s`; COMPLETE `2,289,431,585 ns`, or `13.98 MiB/s`. All setup, Commit, Push, barriers, and End treatment remains unchanged.

### LayerFS phase decomposition

COLD-CREATE-32M: Workspace create `44,753,792`; public native create+fsync Exec `112,477,625`; Commit `1,100,090,042`; Push/two-Store durability `1,024,673,709`; End `7,436,417`. Commit receipt: content `196,088,042`; candidate finish `56,047,292`; local admission `813,548,542`; publication `27,120,458`; live capture `458` with zero files/bytes; total `1,099,444,793`. Push receipt: source read/auth `60,889,834`; object admission `745,789,833`; authority verification `119,764,125`; durability `3,705,667`; unattributed `64,789,082`; total `1,023,900,792`. SQLite subphase variation is retained and is unrelated to the FUSE instrumentation hypothesis.

### FUSE write counters and root cause

The configured INIT maximum was `1,048,576 bytes`, but actual create traffic was exactly `512` kernel write callbacks of `65,536 bytes` each: `33,554,432 bytes` total, with all 512 in the `<=64 KiB` bucket. The data path copied exactly `33,554,432 bytes` at each of three owned boundaries—kernel callback slice to client Request, client Request to protocol frame, and host frame to host Request—so aggregate owned payload copying was exactly `100,663,296 bytes` (96 MiB). Client and host write-frame totals matched exactly at `33,567,232 bytes`.

Measured create write phases were: encode `1,350,917 ns`; socket write/flush `22,490,420`; host socket read `4,738,545`; decode `1,589,682`; host dispatch `64,132,045`. Workspace wrote exactly `33,554,432` spool bytes but opened the same active-overlay spool `512` times; spool open+write consumed `62,793,630 ns`, accounting for almost all host dispatch. The single required fsync opened once and took only `76,709 ns`. Metrics collection over the already-authenticated control stream cost `133,042 ns` inside Commit.

PREPEND produced 1,025 callbacks for `33,554,442 bytes`: 512 requests at 64 KiB plus 513 at 4 KiB or smaller. It likewise copied `100,663,326` owned bytes and reopened the spool 1,025 times; spool open+write consumed `120,363,538 ns` of `122,469,612 ns` host dispatch. This independently confirms descriptor churn scales with request count. EDIT16 produced exactly one 10-byte callback, one spool open/write, and one fsync per edit; across sixteen edits, spool write time was `2,920,126 ns`, fsync `24,586,455`, and metrics collection `1,083,874`.

### Comparison with Computer, previous round, and current best

Computer was not executed; its diagnostic values remain non-formal. Against Round 040, create differed by +35.046 ms, while EDIT16, prepend, read, and recovery were faster in this single run. The new receipt collection itself was 0.133 ms for create and 0.058–0.068 ms for representative small/prepend Commits. There is no evidence of a material counter-induced regression.

### Defects and root causes

The actual kernel request ceiling is 64 KiB in this container/FUSE environment despite the 1 MiB value accepted for the INIT reply, so prior reasoning that assumed 32 one-MiB requests was incorrect. The dominant measured software cost is reopening the active-overlay spool once per 64 KiB callback. The 96 MiB owned-copy amplification is real, but measured encode+decode totals are only about 2.94 ms; client slice-to-Request copy time is not separately timed and remains bounded by the public Exec total. Socket write/flush is the second largest attributed data-plane phase at 22.49 ms.

### What needs improvement next

Retain one spool `File` descriptor per active overlay node instead of reopening it for every write and fsync. Invalidate/close it on truncate paths that require replacement, unpin/rebase/discard/End, and any node removal; retain exact open-handle, hard-link, dirty-range, spool accounting, fsync-before-close, and failure semantics. Use the receipt to require create spool write-open count to fall from 512 to one (or another exact lifecycle-bounded minimum) and prepend from 1,025 to its node-count minimum. Seal this descriptor-only round before borrowed streaming framing.

Then remove the three-copy path with borrowed Write framing: write the fixed header and caller slice directly to TCP and decode the validated payload directly into the final host Request Vec. Preserve exact wire bytes, ordered no-reply stream, 17 MiB bound, errors, and capability/fence behavior. Do not combine it with the descriptor round.

### Stable strengths — no improvement currently needed

Freeze host-bind storage, restored 1,000-page FULL policy pending an explicit replacement durability contract, passive WAL/P0 evidence, public fresh execution, real FUSE, exact dirty edits, capture no-op, same-mount rebase, paged/parent-aware Push, missing-only transfer, independent authority proof, barriers, and recovery. Do not change durability or benchmark semantics while optimizing the data plane.

### Subagent reviews and reconciled decision

No subagent was used. The measured counters supersede the earlier 1 MiB-request assumption and select retained spool descriptors ahead of streaming because 62.79–120.36 ms of open+write time scales directly with actual request count, whereas measured frame encode+decode is approximately 2.94 ms for create.

### Next action

Implement only lifecycle-bounded retained active-overlay spool descriptors with exact close/invalidation proof and rerun the FUSE/Workspace/SDK/harness gates. Build and seal one focused host-bind round; do not change framing, request size, durability, public API, or benchmark boundary in the same source seal.

## Round 042 — retained-spool-07b1fc2a-20260831

- Status: IMPROVED (retained active-overlay descriptor PASS; exact recovery PASS; FUSE execution improved; no paired/formal claim)
- UTC timestamp: 2026-08-31T00:44:48Z through 2026-08-31T00:45:46Z; custody verified and appended 2026-08-31T00:46:31Z
- Local timestamp and timezone: 2026-08-31 08:44:48–08:45:46 CST (Asia/Shanghai); custody verified and appended 08:46:31 CST
- Git commit and tree: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`
- Dirty source seal and diff hashes: source seal `927f55c5d7eda50dead51f9fe965a6b3bbbb3c5db9a15032a1ca297ee6d15363`; captured working-tree patch `898a343f09eff2f0c1193637c397b77a41dcde9aa72128d53647bb6b6bc23645`; staged patch empty; captured status `03576f2abe7dfa1004a61676e69582ea2841c6b73394c43c4a55adba12cc0dfd`
- Benchmark/profile and exact commands: full Workspace tests including retained-descriptor reset/reclaim/out-of-band-deletion tests; SDK lifecycle/rebase suite; fs-bench-pro harness tests; formatting, scoped all-target/all-feature Clippy with `-D warnings`, diff check; cached `docker build --pull=false --platform=linux/arm64 ... -f benchmark/fs-bench-pro/Dockerfile.layerfs -t layerfs-fs-benchmark-pro:retained-spool-07b1fc2a .`; `benchmark/fs-bench-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd layerfs-fs-benchmark-pro:retained-spool-07b1fc2a retained-spool-07b1fc2a-20260831`
- Candidate order seed and pair count: `2c81f63fb9d348fdf5077931b109f42d5fc54ccba66b52046ebe088b78323909`; one LayerFS-only focused sample
- Host, kernel, Docker, CPU/memory/I/O envelope: authoritative Apple arm64 / Darwin 25.4.0 / Docker Desktop macOS host-bind envelope; one CPU, 1 GiB memory and swap, PID limit 512, 256 MiB `/tmp`; OS page cache uncontrolled
- Candidate image tags, digests, architectures, and verified OCI labels: `layerfs-fs-benchmark-pro:retained-spool-07b1fc2a`; `sha256:a226c5bf8e44c38e7c96ead862903a2c2f4aa46860519a05abf0c4b060489469`; arm64; commit/tree/dirty/source-seal/helper labels matched; cached bases and `--pull=false`; Computer was neither rebuilt nor executed
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `99ec9c9d137e467df94e610883541c7fb194748e26589f2152ec08d810070748`; recovery `3dc9695c7b57f75b9864f73fba231441335490bbb532ba656c0278c9ceeaea49`; real FUSE `/workspace/fs-benchmark-pro-7` mountinfo retained; helper `61f6454a7a1f982b4fc342b05d1a883e7b692f359e4c4de8cfc0cc22f86ed737`
- Raw evidence directory and SHA-256 inventory: `runs/retained-spool-07b1fc2a-20260831/`; 264 entries; `raw-inventory.sha256` SHA-256 `561520257380529d1f3156a880d163b51d0b389ff0b6f37730b8f68138341972`; every entry reverified from the run root
- Previous comparable round: Round 041 instrumentation baseline, create `2,289,431,585 ns`, EDIT16 `1,040,244,128`, prepend `828,952,709`, read `459,402,625`
- Current best comparable round: Round 042 is the current measured after-ready FUSE create/write best under the corrected public boundary; formal best remains unavailable

### Hypothesis and planned change

Round 041 showed that 512 create and 1,025 prepend kernel callbacks reopened their active-overlay spool every time, consuming 62.79 and 120.36 ms respectively. Retain one descriptor per overlay NodeId and reuse it for write, truncate, and fsync, while keeping exact path/inode identity validation and lifecycle closure.

### Changes since the previous round

Workspace now owns a node-keyed map of active overlay `File` descriptors. New files and first mutations of base files create and retain exactly one descriptor. Writes, truncate, and fsync reuse it. Reclaim removes the descriptor before unlinking the spool. Clean/Discard/Drop close all descriptors before deleting spool files. In-place rebase retains descriptors only for unlinked-but-still-pinned old handles, closes visible committed overlay descriptors before removing obsolete spools, and rejects any overlay/descriptor mismatch.

Every retained-FD use compares the open descriptor's device/inode with current path metadata. Out-of-band unlink or replacement therefore remains a typed write/fsync failure instead of silently writing an unreachable inode. No request framing, kernel size, copy, durability, public API, benchmark boundary, or storage substrate changed.

### Correctness and validity

Focused tests proved one descriptor survives write+fsync, metrics reset independently, reclaim closes it, and externally deleting the spool still prevents mutation state from advancing. Full Workspace and SDK tests covered exact rebase, old-handle, lease, stale-CAS, cleanup, and recovery behavior. Harness, formatting, warning-denying Clippy, exact registered oracles, real mountinfo, Store identities, raw inventory, clean End, and fresh-container recovery passed.

### Comparable E2E results

One LayerFS-only sample; median/Q1/Q3/min/max equal the sample; no paired ratio, confidence interval, or wins/ties/losses are claimed. Complete COLD-CREATE-32M was `2,275,817,751 ns`, 0.59% below Round 041 because Commit/Push remain dominant and noisy. EDIT16 was `1,010,009,835` (`63.13 ms/edit`); prepend `796,254,709`; read `464,311,708`; focused aggregate `4,546,394,003`. Fresh recovery passed in `460,145,250 ns`.

AFTER-READY FUSE EXECUTION improved from `112,477,625` to `93,309,875 ns` (-17.04%), raising effective public throughput from `284.50` to `342.94 MiB/s` (+20.54%). AFTER-READY AUTHORITY DURABLE was `2,216,647,751 ns`, or `14.44 MiB/s`; COMPLETE was `2,275,817,751`, or `14.06 MiB/s`. The result clears the 300 MiB/s intermediate execution gate but not the 500 MiB/s terminal gate.

PREPEND public Exec improved from `372,648,167` to `348,824,667 ns` (-6.39%) and complete time improved 3.94%. EDIT16 and read remained within their distributions; the descriptor optimization does not change their registered semantics.

### LayerFS phase decomposition

COLD-CREATE-32M: Workspace create `52,619,625`; public native create+fsync Exec `93,309,875`; Commit `1,082,167,792`; Push/two-Store durability `1,041,170,084`; End `6,550,375`. Commit receipt: content `200,192,583`; candidate finish `56,815,417`; local admission `817,388,459`; publication `837,250`; capture `333` with live mode and zero files/bytes; total `1,081,412,459`. Push receipt: source read/auth `59,069,207`; object admission `740,236,541`; authority verification `112,292,250`; durability `61,105,084`; unattributed `64,584,712`; total `1,040,341,751`.

### FUSE write counters and measured effect

Create retained identical traffic: configured maximum `1,048,576`, actual `512 x 65,536-byte` requests, `33,554,432` kernel/spool bytes, exactly `100,663,296` owned copy bytes, and equal `33,567,232-byte` client/host frames. Spool write opens fell exactly `512 -> 1`; fsync opens `1 -> 0`. Spool create+identity-check+write time fell `62,793,630 -> 54,925,751 ns` (-12.53%), host dispatch `64,132,045 -> 55,704,332` (-13.14%), and socket-write time was `16,628,206` in this sample. The reused descriptor's single fsync took `1,433,750 ns` and remains inside Exec.

Prepend traffic also stayed exact: 1,025 callbacks, `33,554,442` kernel/spool bytes, `100,663,326` copies, and equal frames. Spool opens fell `1,025 -> 1`, fsync opens `1 -> 0`; spool time fell `120,363,538 -> 91,972,271 ns` (-23.59%) and host dispatch `122,469,612 -> 93,867,671` (-23.35%). These independent request-count reductions validate the mechanism.

### Comparison with Computer, previous round, and current best

Computer was not executed. Its diagnostic values remain non-formal and do not establish superiority. Round 042 materially improves the intended after-ready FUSE boundary while complete create changes little because current FULL Store admission remains approximately 1.56 s across Commit and Push. The result is retained as a product improvement under unchanged integrity and durability.

### Defects and root causes

Descriptor open churn was real but not the only host-dispatch cost. Exact per-write path/descriptor identity checks intentionally remain and require metadata work for every callback; this preserves out-of-band deletion/replacement failure semantics. The protocol still copies 96 MiB and socket write/flush remains 16.63 ms. Therefore descriptor reuse alone cannot reach the 64 ms / 500 MiB/s terminal boundary.

### What needs improvement next

Implement borrowed streaming Write framing as the next independent seal: encode the fixed frame length/tag/node/offset/payload length without constructing a second payload-sized frame Vec, then write the caller's owned Request payload slice directly to the ordered TCP stream. On the host, read and validate the fixed Write header and payload directly into the final Request Vec, avoiding the frame-to-Request payload copy. Preserve byte-identical protocol, MAX_FRAME/MAX_BYTES, truncation/oversize errors, no-reply ordering, capability/fence semantics, and the current receipt.

Acceptance counters: create kernel/spool bytes and request histogram unchanged; `client_request_copy_bytes` remains 32 MiB in this first streaming design because ProxyClient still owns the no-reply payload; `frame_payload_copy_bytes` and `host_decode_copy_bytes` fall to zero; owned copies fall from 96 to 32 MiB; client/host frame bytes remain exactly equal. Seal before considering single-metadata identity caching or any kernel request-size change.

### Stable strengths — no improvement currently needed

Freeze retained descriptors, exact identity checks, authoritative host bind, current public/durability contract, restored FULL policy, passive evidence, real FUSE, fresh process, dirty edits, same-mount rebase, Push algorithms, exact barriers, and recovery.

### Subagent reviews and reconciled decision

No subagent was used. Round 042 satisfies the exact mechanistic gate and preserves failure behavior. The remaining evidence selects borrowed framing; identity-check reduction would require a separate proof because it touches corruption detection rather than only owned-copy amplification.

### Next action

Implement and test only borrowed streaming Write request encoding plus direct final-Vec host decode, using the existing connection and Request type. Build and append one focused round before any identity-cache, kernel-size, content-construction, or durability work.

## Round 043 — streaming-write-07b1fc2a-20260831

- Status: MECHANISM-IMPROVED / LATENCY-INCONCLUSIVE (two payload copies removed; exact recovery PASS; one-sample public Exec did not improve; no paired/formal claim)
- UTC timestamp: 2026-08-31T00:53:32Z through 2026-08-31T00:54:31Z; custody verified and appended 2026-08-31T00:55:09Z
- Local timestamp and timezone: 2026-08-31 08:53:32–08:54:31 CST (Asia/Shanghai); custody verified and appended 08:55:09 CST
- Git commit and tree: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`
- Dirty source seal and diff hashes: source seal `37a32eedd68ea3b7f79db86532685af18e041815a8623d4812884b723c25a5e7`; captured working-tree patch `906b80b8fcd0bd1639f3eb060a6cdcaaeb10dcc9a3e7ffeb829830535a4be717`; staged patch empty; captured status `03576f2abe7dfa1004a61676e69582ea2841c6b73394c43c4a55adba12cc0dfd`
- Benchmark/profile and exact commands: exact protocol round-trip/truncation tests, capability/proxy/deferred-error tests, Workspace/Monitor/SDK/harness suites, formatting, scoped all-target/all-feature Clippy with `-D warnings`, diff check; cached `docker build --pull=false --platform=linux/arm64 ... -f benchmark/fs-bench-pro/Dockerfile.layerfs -t layerfs-fs-benchmark-pro:streaming-write-07b1fc2a .`; `benchmark/fs-bench-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd layerfs-fs-benchmark-pro:streaming-write-07b1fc2a streaming-write-07b1fc2a-20260831`
- Candidate order seed and pair count: `08dc66ee8b14cde6d9b5d698c72682680755e1afe5ca67465c37e97b5aa7ee1a`; one LayerFS-only focused sample
- Host, kernel, Docker, CPU/memory/I/O envelope: authoritative Apple arm64 / Darwin 25.4.0 / Docker Desktop macOS host bind; one CPU, 1 GiB memory and swap, PID limit 512, 256 MiB `/tmp`; OS page cache uncontrolled
- Candidate image tags, digests, architectures, and verified OCI labels: `layerfs-fs-benchmark-pro:streaming-write-07b1fc2a`; `sha256:1294d7ffb6afe787771ba8ad109320122888b876e7b6fb6aa2074a0c714d0c41`; arm64; commit/tree/dirty/source-seal/helper labels matched; cached bases and `--pull=false`; Computer was neither rebuilt nor executed
- Measurement/recovery container IDs, FUSE mount paths, mountinfo, and helper SHA-256: measurement `dbec2b88d74260c0b3ac4d3f772dae6a66e99a071b360114d77f8f1afc935b32`; recovery `f48a88ba96a10ebb1fdd532251f8ebb905056336f9b6947afcc081211f6f543d`; real FUSE `/workspace/fs-benchmark-pro-7` mountinfo retained; helper `61f6454a7a1f982b4fc342b05d1a883e7b692f359e4c4de8cfc0cc22f86ed737`
- Raw evidence directory and SHA-256 inventory: `runs/streaming-write-07b1fc2a-20260831/`; 264 entries; `raw-inventory.sha256` SHA-256 `c81e2801955c5d36f6d7443c6ed8f96aa2ad61dd03777cc3af4e4d4ae7996d5c`; every entry reverified from the run root
- Previous comparable round: Round 042 retained-spool candidate, create `2,275,817,751 ns`, EDIT16 `1,010,009,835`, prepend `796,254,709`, read `464,311,708`
- Current best comparable round: Round 042 remains the single-sample public FUSE execution best; Round 043 requires a source-identical repeat before a latency verdict

### Hypothesis and planned change

Round 042 retained descriptors but still copied 32 MiB from each Request Vec into a second frame Vec and then copied 32 MiB from the received frame into the host Request Vec. Stream the byte-identical fixed Write header plus the already-owned no-reply payload directly to TCP, and decode the validated payload directly into the final host Request allocation.

### Changes since the previous round

Only ordinary protocol `Write` uses the borrowed path. The client builds a fixed 25-byte frame/header, writes it and the existing Request payload slice to the same ordered stream, and flushes exactly as before. The host reads and validates frame length/tag/node/offset/payload length, allocates the final Request payload once, and reads TCP bytes directly into it. All other requests use the existing generic codec. The wire frame, Request type, first client-owned no-reply Vec, limits, control connection, error handling, and public behavior are unchanged.

### Correctness and validity

Tests assert the exact 30-byte sample frame, successful round-trip, direct-copy counters, truncated-payload rejection, oversized/trailing validation, capability scoping, no-reply deferred errors, pause/fence ordering, and lifecycle behavior. Full dependent tests, formatting, warning-denying Clippy, exact workload oracles, real mountinfo, Store identities, clean End, inventory, and fresh recovery passed.

### Comparable E2E results

One LayerFS-only sample; no paired or formal statistics. Complete COLD-CREATE-32M was `2,359,891,085 ns`, 3.69% above Round 042; EDIT16 `1,025,283,377`; prepend `819,396,375`; read `464,913,166`; focused aggregate `4,669,484,003`. Recovery passed in `457,749,625 ns`.

AFTER-READY FUSE EXECUTION was `98,334,666 ns`, or `325.42 MiB/s`, versus Round 042's `93,309,875` / `342.94 MiB/s`. AFTER-READY AUTHORITY DURABLE was `2,292,278,543 ns`, or `13.96 MiB/s`; COMPLETE `2,359,891,085`, or `13.56 MiB/s`. The public Exec moved in the wrong direction by 5.025 ms even though its internal target phases improved, so no latency win is claimed from this sample.

### LayerFS phase decomposition

COLD-CREATE-32M: Workspace create `60,781,125`; Exec `98,334,666`; Commit `1,144,289,751`; Push `1,049,654,126`; End `6,831,417`. Commit receipt: content `191,149,167`; candidate finish `59,822,750`; local admission `858,024,417`; publication `28,088,583`; live capture `375` and zero files/bytes; total `1,143,584,209`. Push receipt: source read/auth `62,325,167`; object admission `771,321,294`; authority verification `112,886,625`; publication `32,704,834`; durability `3,071,292`; unattributed `64,520,458`; total `1,048,801,876`. Store phase variance dominates the complete-row regression and is unrelated to framing.

### FUSE write counters and measured effect

Create request shape remained exact: 512 x 64 KiB, 32 MiB kernel/spool bytes, one spool open, no fsync open, and equal client/host frame totals `33,567,232`. `frame_payload_copy_bytes` fell `33,554,432 -> 0`; `host_decode_copy_bytes` fell `33,554,432 -> 0`; total owned copying fell `100,663,296 -> 33,554,432 bytes`. Encode fell `1,403,411 -> 18,869 ns`; decode `1,493,205 -> 522,457`; socket write `16,628,206 -> 14,529,872`. These targeted phases improved by approximately 4.454 ms combined. Host dispatch and spool work remained effectively unchanged at `55,524,501` and `54,597,497 ns`; fsync was `1,891,167`.

Prepend likewise retained 1,025 callbacks, exact bytes/frames and one spool descriptor while its two copy counters fell to zero. Encode was `38,317 ns`, socket write `15,472,735`, socket read `12,010,691`, decode `596,633`, host dispatch `93,202,468`, and spool `91,497,596`. Public prepend Exec nevertheless rose from `348,824,667` to `372,747,917 ns`, showing run-level noise beyond the improved framing phases.

### Comparison with Computer, previous round, and current best

Computer was not executed; no superiority claim exists. The mechanism is retained provisionally because it removes 64 MiB of owned copies, preserves all bytes and semantics, and improves every targeted internal phase. The public-latency sample does not establish a speedup, so a source-identical repeat is required before choosing the next optimization from timing rather than topology.

### Defects and root causes

The first FUSE write hypothesis is now exhausted: retained descriptor reuse materially improved public execution, while eliminating two payload copies saves only about 4.45 ms internally and is hidden by current fresh-process/FUSE/host noise in one sample. The remaining create Exec is dominated by approximately 54.6 ms of spool descriptor identity checks plus writes, approximately 14.5 ms socket writes, fresh-process cost, and uninstrumented client Request-copy/runtime overhead.

### What needs improvement next

Run one source-identical focused repeat with no code, image, setup, or boundary change. Require the exact same zero-copy/frame equations and compare the two streaming samples plus Round 042. If public Exec remains statistically inconclusive, keep the topology improvement but do not claim latency. Then follow the user-ordered software roadmap to direct full-spool CDC reading and single-allocation canonical chunk encoding; do not tune WAL, change durability, or change storage substrate.

### Stable strengths — no improvement currently needed

Freeze retained spool descriptors and identity checks, borrowed wire-exact framing, current host bind and durability contract, public process/FUSE boundaries, all Push mechanisms, exact recovery, and append-only custody.

### Subagent reviews and reconciled decision

No subagent was used. Counter equations, not the noisy headline, validate the copy-removal mechanism. A source-identical repeat is the smallest honest next step.

### Next action

Run and append one source-identical focused repeat of `sha256:1294d7ffb6afe787771ba8ad109320122888b876e7b6fb6aa2074a0c714d0c41`. Make no production change before sealing it.

## Round 044 — streaming-write-repeat-07b1fc2a-20260831

- Status: REPEATED / LATENCY-NO-WIN (source-identical mechanism PASS; exact recovery PASS; public Exec repeats near Round 043 and above Round 042)
- UTC timestamp: 2026-08-31T00:56:04Z through 2026-08-31T00:57:02Z; custody verified and appended 2026-08-31T00:57:53Z
- Local timestamp and timezone: 2026-08-31 08:56:04–08:57:02 CST (Asia/Shanghai); custody verified and appended 08:57:53 CST
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `37a32eedd68ea3b7f79db86532685af18e041815a8623d4812884b723c25a5e7`
- Captured working-tree/index/status hashes: `906b80b8fcd0bd1639f3eb060a6cdcaaeb10dcc9a3e7ffeb829830535a4be717`; empty; `03576f2abe7dfa1004a61676e69582ea2841c6b73394c43c4a55adba12cc0dfd`
- Exact candidate image: source-identical `sha256:1294d7ffb6afe787771ba8ad109320122888b876e7b6fb6aa2074a0c714d0c41`, arm64; no build and no Computer execution
- Exact command: `benchmark/fs-bench-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd sha256:1294d7ffb6afe787771ba8ad109320122888b876e7b6fb6aa2074a0c714d0c41 streaming-write-repeat-07b1fc2a-20260831`
- Schedule seed/sample count: `2a37a717df3115ef25b0aa1501229eadfc3f3d7323d35e6c5a1da03c92611483`; one LayerFS-only focused repeat
- Measurement/recovery containers: `775afbfa5d8342c6c416287e2619a6402e756550d14a77c90e3c0f0610d59598`; `80d0e7d11edf4b12e9005180f7496ec512850aa10ed4baf5ef94d04b26d23c90`
- Raw evidence: `runs/streaming-write-repeat-07b1fc2a-20260831/`; 264 verified entries; inventory SHA-256 `b0d79706c7a28abdf4391098efdb8be49b777b25db61bfc601f0c9c609c99488`

### Repeated result

Complete create was `2,322,003,834 ns`; EDIT16 `1,074,780,585`; prepend `807,257,044`; read `462,838,416`; recovery `454,153,626`, all exact and recovered. Create AFTER-READY FUSE EXECUTION was `98,846,791 ns`, or `323.73 MiB/s`; AFTER-READY AUTHORITY DURABLE `2,264,897,709`, or `14.13 MiB/s`; COMPLETE `2,322,003,834`, or `13.78 MiB/s`.

The two streaming create Exec samples are `98.335` and `98.847 ms`, mean `98.591 ms`. Round 042's retained-descriptor/non-streaming sample was `93.310 ms`. Streaming therefore does not establish a public-latency win and repeats approximately 5.28 ms slower than that single baseline, even though both streaming receipts prove the targeted internal copy/encode/decode reductions.

Mechanism equations repeated exactly: 512 x 64 KiB; one spool descriptor; 32 MiB kernel and spool bytes; 32 MiB owned copies; zero frame-payload and host-decode copy bytes; equal `33,567,232-byte` frames. Encode was `17,752 ns`, decode `578,793`, socket write `19,286,346`, socket read `5,088,445`, host dispatch `55,501,174`, spool `54,592,327`, and fsync `1,766,542`. Prepend independently repeated zero targeted copies and exact frame/request counts.

### Decision and next action

Do not claim streaming latency improvement. Before accepting or reverting it, replace the two sequential header/payload socket writes with one standard-library vectored-write loop, which is the originally intended scatter-write and removes one write syscall per callback without changing bytes, ownership, limits, or semantics. Seal that as one final FUSE transport round. If public Exec still fails to improve while internal phases remain bounded, retain or reject based on the measured CPU/memory topology benefit without further speculative FUSE tuning, then move to direct full-spool CDC and single-allocation encoding.

## Round 045 — vectored-write-07b1fc2a-20260831

- Status: MIXED / PREPEND-REGRESSED (create FUSE best; prepend catastrophic latency regression; exact correctness/recovery PASS; not retainable yet)
- UTC timestamp: 2026-08-31T01:00:10Z through 2026-08-31T01:01:11Z; custody verified and appended 2026-08-31T01:02:23Z
- Local timestamp and timezone: 2026-08-31 09:00:10–09:01:11 CST; custody verified and appended 09:02:23 CST
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `0aadf65ce6235df778b84c3bdae76e234faaae8b8f245dd10b25a535be019b45`
- Image/digest: `layerfs-fs-benchmark-pro:vectored-write-07b1fc2a`; `sha256:249e5a493aa01e17d2fa803978c8ddce1f8ffcdf07250b1417aa79a6f486974a`; arm64; cached bases, no pull, no Computer execution
- Schedule seed/sample: `b5d682cd81d77421782549b77ff2a4eda2a7604cc44118cc0d7168dd5a848b36`; one focused LayerFS sample
- Measurement/recovery containers: `1d11bb5b0badf6620d750b4957f567ffc7d2c19174b98ae9af710918cbddaa2f`; `d99f87d1897b63538edde161fa0ce3df328e6348ae44b3b1edd662852800d55f`
- Captured working-tree/index/status hashes: `76e177386bdc5b298c11f2feeda0a7242636aac1bac7479d74ef9597559502fb`; empty; `03576f2abe7dfa1004a61676e69582ea2841c6b73394c43c4a55adba12cc0dfd`
- Raw evidence: `runs/vectored-write-07b1fc2a-20260831/`; 264 verified entries; inventory SHA-256 `340a49a59460b7bb53988e444215bfe730b330047bfa9bc5f5063f7232d0c28c`

### Change and correctness

Only the borrowed Write socket emission changed from sequential `write_all(header)` plus `write_all(payload)` to a bounded standard-library `write_vectored` loop that handles partial writes and preserves the exact same header, payload, flush, frame, limits, ordering, and errors. Protocol/proxy/clippy gates, all registered byte oracles, real mountinfo, Store identities, clean End, raw custody, and fresh recovery passed.

### Results

Create AFTER-READY FUSE EXECUTION improved to `88,468,375 ns`, or `361.71 MiB/s`, the current single-sample best. Complete create was `2,247,781,544`; authority-durable `2,185,158,085` (`14.64 MiB/s`). Create retained exact 512 x 64 KiB requests, 32 MiB owned copies, equal frames, one spool descriptor, zero fsync opens, `13,460 ns` encode, `15,741,329` socket write, `4,892,461` socket read, `671,931` decode, `53,301,632` host dispatch, and `52,595,763` spool time.

However PREPEND public Exec regressed from the prior 349–373 ms range to `957,528,792 ns`; complete prepend was `1,468,134,167`. Its receipt remained byte-exact but socket write rose to `311,563,741 ns`, host dispatch to `684,559,146`, and spool time to `679,933,702`. This regression dominates the focused aggregate `5,294,656,628`. EDIT16 was `1,112,273,792`; read `466,467,125`; recovery `510,398,584`, all exact.

### Verdict and next action

The mixed result is not retainable as a product performance win until the prepend spike is classified. Because correctness and byte equations pass, run one source-identical repeat of `sha256:249e5a493aa01e17d2fa803978c8ddce1f8ffcdf07250b1417aa79a6f486974a` without code or rebuild. If the 1,025-request prepend spike repeats, reject vectored write and restore sequential borrowed framing. If it disappears, record Round 045 as host noise and use the repeat plus create result to decide. Do not modify content, durability, or identity checks before sealing the repeat.

## Round 046 — vectored-write-repeat-07b1fc2a-20260831

- Status: REPEATED / NO PUBLIC WIN (Round 045 prepend spike classified as noise; vectored emission adds complexity without stable create benefit)
- UTC timestamp: 2026-08-31T01:02:51Z through 2026-08-31T01:03:52Z; custody verified and appended 2026-08-31T01:04:26Z
- Local timestamp and timezone: 2026-08-31 09:02:51–09:03:52 CST; custody verified and appended 09:04:26 CST
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `0aadf65ce6235df778b84c3bdae76e234faaae8b8f245dd10b25a535be019b45`
- Exact repeated image: `sha256:249e5a493aa01e17d2fa803978c8ddce1f8ffcdf07250b1417aa79a6f486974a`; no rebuild and no Computer execution
- Schedule seed: `599ebc958d2c4044390c47b0a13e02c4e96735fe2ed55afb468bf4b371860e55`
- Measurement/recovery containers: `bfe10b64bba3d83a24656db4d250c6dab93d911efabd1e42e58f8d543f8853ad`; `3ad11a64a35d5d783ef491e689e39c3eb7ec266a9b284b3ea39c6ac197236084`
- Captured working-tree/index/status hashes: `76e177386bdc5b298c11f2feeda0a7242636aac1bac7479d74ef9597559502fb`; empty; `03576f2abe7dfa1004a61676e69582ea2841c6b73394c43c4a55adba12cc0dfd`
- Raw evidence: `runs/vectored-write-repeat-07b1fc2a-20260831/`; 264 verified entries; inventory SHA-256 `16b116fbee849b0f0fb377f383b03ff7303792e3119e511b3e8f20fed4a844c2`

### Repeated results and diagnosis

PREPEND returned to `352,280,167 ns` Exec and `806,301,667` complete. Its exact receipt showed socket write `15,382,602`, host dispatch `101,210,268`, and spool `99,399,328`, so Round 045's 957.5 ms Exec / 311.6 ms socket / 684.6 ms dispatch was a transient host event, not a reproducible vectored-write defect.

Create Exec, however, was `111,649,459 ns` (`286.61 MiB/s`), versus Round 045's `88,468,375`. The two vectored samples average `100.059 ms`; the two sequential borrowed-stream samples average `98.591 ms`. Vectored emission therefore shows no stable public benefit and greater spread. Its internal repeat was also ordinary: `14,635 ns` encode, `16,905,084` socket write, `5,238,628` socket read, `534,625` decode, `62,351,166` dispatch, and `61,566,744` spool.

Complete create was `2,383,895,293`; EDIT16 `1,011,649,581`; prepend `806,301,667`; read `437,840,583`; recovery `468,972,042`, all exact. Create authority-durable was `2,333,389,834` (`13.71 MiB/s`) and complete throughput `13.42 MiB/s`; Store variation is not attributed to the socket change.

### Decision and next action

Reject the vectored helper as unnecessary complexity: it does not improve the repeated public boundary or internal socket distribution. Restore the simpler sequential borrowed header/payload writes, which is byte-identical to sealed Rounds 043–044 and still removes 64 MiB of owned copies. No new performance run is needed merely to re-prove an already twice-sealed identical source mechanism; run exact protocol/clippy checks and append a no-execution restoration note before the next production change.

Then close FUSE transport tuning. Retain descriptors plus sequential borrowed framing and move to the user-ordered next target: direct fully-materialized spool reading for FastCDC, followed separately by single-allocation canonical chunk encoding. Do not change durability, host-bind substrate, public execution, identity checks, or benchmark boundaries.

## Round 047 — sequential-stream-restoration-07b1fc2a-20260831

- Status: SOURCE-RESTORED (no candidate execution; vectored helper removed; source byte-equivalent to twice-sealed Rounds 043–044)
- UTC/local timestamp: 2026-08-31T01:05:55Z; 2026-08-31 09:05:55 CST (Asia/Shanghai)
- Git commit/tree: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`
- Restored source seal: `37a32eedd68ea3b7f79db86532685af18e041815a8623d4812884b723c25a5e7`, exactly matching Rounds 043–044
- Commands: `cargo fmt --all --check`; `cargo test -p layerfs-fuse --all-features`; `cargo clippy --all-targets --all-features -p layerfs-fuse -- -D warnings`; `git diff --check`; `benchmark/fs-bench-pro/run.sh --source-seal`
- Candidate/image/raw custody: no new image or run. Exact restored candidate behavior already has two complete inventories and fresh recoveries: Round 043 `c81e2801955c5d36f6d7443c6ed8f96aa2ad61dd03777cc3af4e4d4ae7996d5c`; Round 044 `b0d79706c7a28abdf4391098efdb8be49b777b25db61bfc601f0c9c609c99488`

### Restoration and verdict

The standard-library vectored-write loop was deleted. Ordinary Write again sends the fixed borrowed header and borrowed payload with the simpler two `write_all` calls, while the host still decodes directly into the final Request Vec. Exact wire bytes, zero targeted frame/decode copies, retained descriptors, capability/fence/error semantics, and all public boundaries remain unchanged.

No benchmark was rerun because the resulting production source seal is exactly identical to the already-built and twice-recovered streaming image. Rounds 043–044 remain its performance evidence. This avoids manufacturing a redundant run merely for custody.

FUSE transport tuning is closed: retained descriptors are a measured public win; sequential borrowed framing is retained for its 64 MiB copy reduction but carries no public-latency claim; vectored emission is rejected. The next production change is direct fully-materialized active-overlay spool reading for FastCDC only.

## Round 048 — direct-spool-cdc-07b1fc2a-20260831

- Status: CONTENT-IMPROVED / COMPLETE-NOISY (direct fully charged spool reader PASS; content approximately 2x faster; exact recovery PASS; complete create regressed from Store noise)
- UTC timestamp: 2026-08-31T01:11:48Z through 2026-08-31T01:12:49Z; custody verified and appended 2026-08-31T01:13:20Z
- Local timestamp and timezone: 2026-08-31 09:11:48–09:12:49 CST; custody verified and appended 09:13:20 CST
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `428a1e612e59015dfb908dd1776807098daf929cc4029c18a8efeb4513b96e10`
- Image/digest: `layerfs-fs-benchmark-pro:direct-spool-cdc-07b1fc2a`; `sha256:4918e05f60be1ecfec50cd2e4096b3b676a56f370ea4e43a328314c797216bac`; arm64; cached bases and no Computer execution
- Schedule seed/sample: `9750eafb271b9926218b000f40c750da7305c763b834bc0ac7aafaccfccd716a`; one focused LayerFS sample
- Measurement/recovery containers: `25239d2f6975444002e8b70bd06926fec55569a9eefbc71b1351a1c8399b3fb7`; `2869c8f8f965535fce9faee8874b02225c82c823243db4cb8acce30063ba8717`
- Captured working-tree/index/status hashes: `5229bfa27dc8d5b5f269e4e8969d83a6c520494db3e5f613247e47f51cf72402`; empty; `7a8e75b68794d024c2954b81ef2777389a22727126c8cb630b182da66fc38a8d`
- Raw evidence: `runs/direct-spool-cdc-07b1fc2a-20260831/`; 264 verified entries; inventory SHA-256 `be2acd3b24e170ed89455faec0a7d48aaebf7035ece04703ee3545836e85d3c7`

### Hypothesis and change

FastCDC requests 32 KiB at a time. The previous `WorkspaceFileReader` called general `Workspace::read` for each request, which rebuilt a ReadPlan, opened the spool, allocated and filled a temporary Vec, then copied that Vec into FastCDC's caller buffer roughly 1,024 times for a 32 MiB fully materialized new file.

`WorkspaceFileReader` now selects a direct source only when the node is an overlay with `base=None` and charged ranges exactly cover `0..len`. It validates the retained spool descriptor identity and exact length once, then fills FastCDC's caller buffer directly with bounded `read_at` calls. Sparse, mixed, base-backed, partial-range, and malformed spools keep the prior path or fail with typed integrity. Owned spools are now opened read-write at their single shared creation helper with unchanged create/truncate semantics; the first focused test caught and fixed the prior write-only descriptor `EBADF` before any image existed.

### Correctness and gates

A focused test proves full charged data selects direct mode and returns exact bytes, while a sparse base-less file selects mixed fallback. Full Workspace, SDK lifecycle/rebase, exact mutation, harness, formatting, Clippy, mountinfo, Store identity, raw custody, final oracle, and fresh recovery gates passed. Create still reports exact CDC `33,554,432`, candidate `1,747 / 33,661,925`, insert `1,744 / 33,661,702`, first scratch `33,662,033`, reachable copy `33,661,925`, two spills, and `67,323,958-byte` peak. Prepend remains exact at `33,554,442` CDC and the same missing/reuse/storage equations.

### Measured result

Create content fell to `97,259,625 ns` from the three immediately preceding non-direct values `200,192,583`, `191,149,167`, and `197,661,833` (about 50.5% lower than their mean; `329.02 MiB/s` content throughput). Prepend content fell to `111,222,876 ns` from `208,387,584`, `197,350,958`, and `197,020,917` (about 44.6% lower than their mean). This is a large phase-local software win with unchanged canonical outputs.

Complete create was nevertheless `3,052,678,084 ns`: Exec `111,678,041`, Commit `1,386,146,834`, Push `1,499,096,668`, lifecycle `55,756,541`. Commit local admission spiked to `1,182,960,834`; Push object admission to `1,132,497,375`, source read/auth `88,084,126`, verification `160,200,917`. These FULL host-bind Store phases are independent of the reader and explain the complete-row regression. Complete prepend was `925,800,084`, including Exec `500,902,209` noise, Commit `225,524,250`, Push `135,000,875`. EDIT16 was `1,160,703,127`; read `579,726,709`; recovery `515,185,750`, all exact.

AFTER-READY FUSE EXECUTION was `111,678,041 ns` (`286.54 MiB/s`); AFTER-READY AUTHORITY DURABLE `2,996,921,543` (`10.68 MiB/s`); COMPLETE `3,052,678,084` (`10.48 MiB/s`). No complete improvement or Computer comparison is claimed from this noisy sample.

### Decision and next action

Retain the direct reader: the intended content phase improved by approximately 2x and exact candidate/recovery equations did not move. The next separate content change is single-allocation canonical chunk encoding. Inspect `encode_chunk_object`: remove only the intermediate payload Vec so each byte-identical canonical chunk is constructed once in a pre-sized buffer. Keep IDs, encoding, CDC boundaries, Store insertion, scratch, and all public semantics exact. Seal before second-scratch removal.

## Round 049 — single-chunk-encoding-07b1fc2a-20260831

- Status: CONTENT-IMPROVED (single-allocation canonical chunk encoding PASS; exact IDs/recovery PASS; no paired/formal claim)
- UTC timestamp: 2026-08-31T01:24:05Z through 2026-08-31T01:25:04Z; custody verified and appended 2026-08-31T01:25:52Z
- Local timestamp and timezone: 2026-08-31 09:24:05–09:25:04 CST; custody verified and appended 09:25:52 CST
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `ab9caeab9bd740646312574b5a63a05ec1801c3103f59a393b60a597a3b7bf42`
- Image/digest: `layerfs-fs-benchmark-pro:single-chunk-encoding-07b1fc2a`; `sha256:06974331f79fc957d5b3af8148629d80fcc9314bd164b636063ec814b3c164b1`; arm64; cached bases and no Computer execution
- Schedule seed/sample: `1c379236ca15111b7b15ee2299a913f7a7ab20c82d2fbf369c4796f8dda3afc9`; one focused LayerFS sample
- Measurement/recovery containers: `43c3921aeef84f9fd5175e5fb2afbe873dd7a168f116ac67ec906c4671f53b00`; `497a80f61ceb11305f707b733a415a9f09eff13d4addec32eebb8d3d6893e836`
- Captured working-tree/index/status hashes: `322e105702099c171e8b78e6ca552df92263a19049721d9871af55eabb670144`; empty; `0337d60b6ba681b65f2ee3da147ec131b18b7a15be49ded3bb292a4895e09ccf`
- Raw evidence: `runs/single-chunk-encoding-07b1fc2a-20260831/`; 264 verified entries; inventory SHA-256 `432a0c07c7e0204eb49f23ee41e2cbc6d651eae694fdf4da85648ecd5420f181`

### Hypothesis and change

`encode_chunk_object` previously allocated an intermediate `CHUNK_MAGIC + chunk` Vec, then `encode_bytes_object` allocated the canonical Vec and copied the full value again. It now computes checked value/payload/total lengths and writes the canonical object magic, Bytes tag, payload length, value length, chunk magic, and input bytes directly into one exact-capacity Vec.

A focused oracle compares empty, ordinary, and maximum-size chunks byte-for-byte and ObjectId-for-ObjectId with the old composition. The complete canonical fixture corpus, literal extent IDs, randomized splice history, FastCDC, logical namespace, Workspace, SDK, harness, formatting, and warning-denying Clippy gates passed. No codec, boundary, ObjectId, chunking, scratch, Store, FUSE, durability, or public behavior changed.

### Results

Create content fell `97,259,625 -> 89,214,250 ns` (-8.27%), raising content throughput from `329.02` to `358.69 MiB/s`. Prepend content was effectively flat/slightly better at `110,312,459` versus `111,222,876`. Exact create counters remained CDC `33,554,432`, candidate `1,747 / 33,661,925`, insert `1,744 / 33,661,702`, encode/hash `1,748`, first scratch `33,662,033`, reachable copy `33,661,925`, two spills and `67,323,958-byte` peak. Prepend counters and final identities were likewise unchanged.

Complete create improved to `2,220,212,335 ns`: Workspace create `45,855,500`; Exec `109,988,625`; Commit `1,024,289,918`; Push `1,031,893,084`; End `8,185,208`. AFTER-READY AUTHORITY DURABLE was `2,166,171,627` (`14.77 MiB/s`) and COMPLETE `14.41 MiB/s`. Complete prepend improved to `748,440,084`; EDIT16 `1,017,224,122`; read `507,018,668`; recovery `499,153,250`, all exact. These whole-row values remain single-sample diagnostics; only the intended content change is claimed.

Create candidate finish remains `63,203,667 ns`, and the receipt still proves a complete second `33,661,925-byte` reachable scratch copy with `67,323,958-byte` simultaneous spill peak. This is now the next measured content/candidate target.

### Decision and next action

Retain single-allocation encoding. Next eliminate only the second reachable candidate store/copy while preserving exclusion of 108 unreachable first-store bytes, authenticated child-before-parent order, bounded admission pages, collision byte comparison, and all exact counters. Inspect `ObjectBuffer::finish`/`DeferredObjectStore::reachable_from`; reuse the first spill as immutable canonical backing and derive a bounded reachable postorder ID view rather than copying payloads into another spill. Seal that change independently before authority verification or read-path work.

## Round 050 — in-place-reachable-07b1fc2a-20260831

- Status: RESOURCE-IMPROVED / FINISH-REGRESSED (second payload scratch removed; exact order/transfer/recovery PASS; candidate finish slower and requires correction)
- UTC timestamp: 2026-08-31T01:37:23Z through 2026-08-31T01:38:22Z; custody verified and appended 2026-08-31T01:39:21Z
- Local timestamp and timezone: 2026-08-31 09:37:23–09:38:22 CST; custody verified and appended 09:39:21 CST
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `bfeb671ddb7fe35db9cb6236b1d64146cb0ddd831c540bc3120b8962e5d883ec`
- Image/digest: `layerfs-fs-benchmark-pro:in-place-reachable-07b1fc2a`; `sha256:4d0088dec7c951e8bec1f870fb72dbb75b3953fa380d2ca4e71e830179ebc13c`; arm64; cached bases and no Computer execution
- Schedule seed/sample: `6e9029ba6effb732385889e4a8c9acbd9924e5f7abd784a0061cf41494715c72`; one focused LayerFS sample
- Measurement/recovery containers: `766a56269744b0ff1306ca1e73e56c5cc8f617bf7cae4224d393e21674154cb3`; `99f635ba5affac9fbed3416c864193f27c53d2201c24984f4fd5b57ca4ad62c9`
- Captured working-tree/index/status hashes: `8966b568af62b03656bd9565e9a142532b942dcd93dbd51bec43a8c4720d3917`; empty; `0337d60b6ba681b65f2ee3da147ec131b18b7a15be49ded3bb292a4895e09ccf`
- Raw evidence: `runs/in-place-reachable-07b1fc2a-20260831/`; 264 verified entries; inventory SHA-256 `3292a69d3ef297a0298998b7207aec23cb73ef0f5d473167cfb19242a00587ba`

### Hypothesis and change

Replace `reachable_from`'s second `DeferredObjectStore` payload copy with authenticated postorder marking on the first store. Memory candidates replace their order vector with reachable IDs. Spilled candidates add an ID-only reachable sequence marker to the existing scratch table; admission joins that ordering to the first canonical payload rows. No candidate payload is duplicated.

The focused scratch oracle stages an explicit unreachable object, proves it is excluded, proves root-last postorder, exact count/encoded bytes, one spill, zero reachable-copy bytes, and first-store-only peak. Full Storage, BranchStore, LayerStackStore, omission/corruption/interruption, paged PushPlan, 4/32/256 MiB recovery, Workspace, SDK, harness, formatting, and Clippy gates passed.

### Mechanism results

Create exact counters changed only as intended: first-store writes `33,662,033`; reachable-copy writes `33,661,925 -> 0`; spill count `2 -> 1`; peak `67,323,958 -> 33,662,033`. Reachable candidate remains `1,747 / 33,661,925`, local insert and Push exact missing send remain `1,744 / 33,661,702`, four membership pages, fourteen payload batches. The 108 unreachable first-store bytes remain excluded.

Prepend similarly reports first-store `33,662,684`, reachable-copy `0`, one spill, `33,662,684` peak, exact candidate `1,747 / 33,661,935`, exact missing/send `44 / 377,723`, source reuse `1,703 / 33,284,212`, and one payload batch. Final oracle and recovery passed.

### Performance result and defect

Candidate finish regressed instead of improving: create `63,203,667 -> 116,579,208 ns`; prepend `71,658,959 -> 106,867,750`. The new traversal authenticates 33.66 MiB, reads each spilled object once for expansion and again at postorder completion, then performs 1,747 single-row `UPDATE reachable_sequence` statements in one scratch transaction. The removed 33.66 MiB payload write is therefore outweighed by repeated reads and statement count.

Complete create was `2,217,474,874`: Exec `104,857,291`, Commit `1,018,049,250`, Push `1,045,972,667`, lifecycle `48,595,666`. Complete prepend `749,371,375`; EDIT16 `1,057,275,667`; read `452,680,334`; recovery `484,647,208`. Content was `79,680,125` (`401.61 MiB/s`), but that phase was not changed. Whole rows remain single-sample diagnostics.

### Decision and next action

Do not accept the current finish latency as optimized. Preserve the one-store resource design, but correct its implementation in the next seal: carry each canonical Vec on the depth-bounded DFS expansion stack so every object is read once, and replace 1,747 individual marker UPDATEs with at most fourteen 128-ID bulk inserts into a separate ID-only reachable-order table. Admission then joins reachable IDs to first-store payload rows in exact sequence. Keep authentication, cycle detection, no payload copy, one spill, bounded memory, exact postorder, and all visibility/integrity gates.

## Round 051 — reachable-bulk-order-07b1fc2a-20260831

- Status: RESOURCE-IMPROVED / FINISH-REGRESSION-CORRECTED (one payload spill retained; exact reachable postorder PASS; candidate finish restored to the pre-copy-removal baseline; no paired/formal claim)
- UTC timestamp: 2026-08-31T01:48:33Z through 2026-08-31T01:49:32Z; custody verified and appended 2026-08-31T01:51:54Z
- Local timestamp and timezone: 2026-08-31 09:48:33–09:49:32 CST; custody verified and appended 09:51:54 CST
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `4258056b4e2053437aa1a8898812a89ea6b1ee53715f61822c77e0de4c47297a`
- Image/digest: `layerfs-fs-benchmark-pro:reachable-bulk-order-07b1fc2a`; `sha256:ae3a538b064ec59be9d50c150db4b603f6abbdd36b1705a8d1984ce069bac603`; arm64; cached bases, `--pull=false`, and no Computer build or execution
- Schedule seed/sample: `634c098c3456627d22986cfba475abfbef348313940d7bc3f67b8a8100862c73`; one focused LayerFS sample
- Measurement/recovery containers: `e1a4514197a77c68548c416e6921b103fb57b1d9205a1d3db13055ded6ec0adf`; `7ce59cf1933bd9bc48cc1187ae874d20619f1c2618e84e563b73cab901a8916c`
- Captured working-tree/index/status hashes: `55de2ef49e2d58a7bea6a9d3b5bac5479a6740feb2cd439d098f40cfebbd0f84`; `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`; `0337d60b6ba681b65f2ee3da147ec131b18b7a15be49ded3bb292a4895e09ccf`
- Raw evidence: `runs/reachable-bulk-order-07b1fc2a-20260831/`; 264 verified entries; inventory SHA-256 `485393c8a195a6bcc654d4f704670c8425e917b03c6b06b1e87eccecf8cfad94`

### Hypothesis and correction

Round 050 removed the second 33.66 MiB reachable payload spill but read every spilled canonical object twice and issued 1,747 individual scratch-table marker updates. This round keeps each authenticated canonical Vec on the depth-bounded DFS expansion stack until its postorder completion, so each object is read once. A separate ID-only `reachable` table receives exact sequence/ObjectId pairs in at most 128-ID fixed SQL pages inside one scratch transaction; the measured 1,747-object candidates therefore require fourteen bulk insert statements rather than 1,747 updates. Admission joins those IDs to the immutable first payload store in sequence.

The implementation changes only `crates/layerfs-storage/src/admission.rs`, uses the existing object-batch bound, adds no dependency or public abstraction, and retains identity authentication, active-cycle rejection, unreachable-object exclusion, child-before-parent ordering, collision comparison, bounded payload admission, and the existing Store durability/publication paths. The first source correction also completed the already-selected separate reachable-table schema and added the explicit DFS stack type required by Rust inference.

### Correctness and commands

Focused scratch oracles proved one spill, zero reachable payload-copy bytes, exact byte/count balance, root-last order, and exclusion of an explicitly staged unreachable object. Full `layerfs-storage` and all fifteen V2 contract tests passed. BranchStore, LayerStackStore, Workspace, SDK, and fs-bench-pro suites passed, including paged PushPlan, omitted/corrupt-object rejection, boundary mismatch, interrupted publication, 4/32/256 MiB FULL-WAL recovery, same-mount rebase, execution receipt equations, mountinfo custody, and fresh recovery. Formatting, diff check, and warning-denying Clippy passed.

Exact commands were `cargo test -p layerfs-storage`; `cargo test -p layerfs-branch-store -p layerfs-layerstack-store -p layerfs-workspace -p layerfs-sdk -p fs-benchmark-pro`; `cargo clippy -p layerfs-storage -p layerfs-branch-store -p layerfs-layerstack-store -p layerfs-workspace -p layerfs-sdk -p fs-benchmark-pro --all-targets -- -D warnings`; `cargo fmt --all -- --check`; `git diff --check`; cached `docker build --pull=false --platform=linux/arm64 ... -f benchmark/fs-bench-pro/Dockerfile.layerfs -t layerfs-fs-benchmark-pro:reachable-bulk-order-07b1fc2a .`; and `benchmark/fs-bench-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd layerfs-fs-benchmark-pro:reachable-bulk-order-07b1fc2a reachable-bulk-order-07b1fc2a-20260831`.

### Mechanism result

Create retained exact counters: CDC `33,554,432`, reachable candidate `1,747 / 33,661,925`, local insert `1,744 / 33,661,702`, reuse `3 / 223`, first-store writes `33,662,033`, reachable-copy writes `0`, one spill, and `33,662,033-byte` peak. Push announced `1,747` IDs over four membership pages and sent the exact `1,744 / 33,661,702` missing objects in fourteen bounded payload batches. Live capture remained a proved no-op: `capture_mode=live`, zero files, zero bytes, `500 ns`.

Create candidate finish fell from Round 050's `116,579,208` to `62,968,458 ns` (-45.99%), effectively restoring Round 049's `63,203,667 ns` pre-removal baseline (-0.37%) while retaining the 33.66 MiB scratch reduction. Prepend candidate finish fell `106,867,750 -> 61,107,292 ns` (-42.82%) and is 14.72% below Round 049's `71,658,959 ns`. Prepend retained exact candidate `1,747 / 33,661,935`, missing/send `44 / 377,723`, source reuse `1,703 / 33,284,212`, one payload batch, one spill, zero reachable copy, and `33,662,684-byte` peak. EDIT16 remained exact at ten CDC bytes and ten objects per edit; edit-16 candidate finish was `100,875 ns`, and capture remained zero-copy/no-op.

### Complete result and decision

Complete COLD-CREATE-32M was `2,219,538,793 ns`: Workspace create `49,647,833`, AFTER-READY FUSE Exec `102,293,875`, Commit `999,654,626`, Push plus two-Store durability `1,061,153,334`, and End `6,789,125`. The named execution boundary delivered `312.82 MiB/s`; AFTER-READY AUTHORITY DURABLE was `2,163,101,835 ns` (`14.79 MiB/s`); COMPLETE was `14.42 MiB/s`. Commit content was `82,722,500 ns`, candidate finish `62,968,458`, and FULL host-bind local admission `845,961,000`. Push remained dominated by object admission `758,325,708`, authority verification `115,302,583`, durability `59,706,500`, source read/auth `59,636,710`, and `64,073,541 ns` unattributed.

Complete PREPEND-32M improved to `703,659,335 ns`; EDIT16 was `1,018,181,293 ns` (`63.64 ms/edit`); READ-SYNC-32M was `465,484,876 ns`; focused aggregate `4,406,864,297 ns`. Exact final bytes/digest, retained mountinfo, exact BranchStore recovery identity, and fresh-container reopen passed in `481,865,750 ns`. These whole rows are one-sample host-bind diagnostics; no paired ratio or formal Computer superiority is claimed.

Retain the corrected one-store reachable design: its resource gain no longer costs finish latency. The next source round must follow the frozen software roadmap rather than revisit EDIT16, FUSE framing, or durability semantics. Attribute and then remove only proven same-boundary payload clones/duplicate authentication on READ-SYNC-32M, while preserving Reference local-first corruption detection and fresh public SDK/FUSE execution. Append that round before any later authority-verification batching.

## Round 052 — read-counters-07b1fc2a-20260831 (invalid)

- Status: INVALID / EVIDENCE-GATE-PASS (new passive read receipt rejected its own equation at strict Workspace End; no performance row or optimization claim)
- UTC timestamp: 2026-08-31T02:23:46Z through 2026-08-31T02:23:51Z; manually sealed and appended 2026-08-31T02:24:21Z before correction
- Local timestamp and timezone: 2026-08-31 10:23:46–10:23:51 CST; manually sealed and appended 10:24:21 CST
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `bb6acb98f1d3d1d6ab2dfe098d5017a11a7a37d1c097b3079151ea7a1f1c15b3`
- Image/digest: `layerfs-fs-benchmark-pro:read-counters-07b1fc2a`; `sha256:c8310667d6154a397f90bf72931b41259598e7f472e9341cec8f1d359b729a09`; arm64; cached bases, `--pull=false`, no Computer build or execution
- Schedule seed/sample: `bbbb299ddf4bf78d8870b8152398f193bde4367882ad25bc532bd6de499236f5`; focused LayerFS attempt aborted before any complete sample
- Measurement container: `17beb97ebd466be2d419c2b9d3c356b272bb562036a8e11f780ee4c10244e636`; no recovery container was started
- Captured working-tree/index/status hashes: `91f83bddcfb8a8cf8d31207328b4e1b5ae48c439b67e0e89a41f7cf06489aac0`; `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`; `419f87ce2421e38b7ab615f29fbcff696dfce105c058a2065e4837e41eaec626`
- Raw evidence: `runs/read-counters-07b1fc2a-20260831/`; 30 verified entries; inventory SHA-256 `07087ffa1af92fb7404b713ae242120f87964cbb4ccbfb7c6c2b7508b6ff40df`

### Attempt and failure

This instrumentation-only source added a validated aggregate Workspace-read receipt spanning FUSE negotiation/kernel request buckets, 16 MiB read-ahead hits/misses/fetch/served/unused bytes, proxy response frames/copies/timing, Workspace rope-plan and payload counters, BranchStore local/parent rows/bytes/timing, Reference-boundary authentication, Core authentication, and ordered clone/move bytes. It did not remove any clone/hash, change batching, alter read-ahead, or change the public benchmark path.

All focused and dependent source gates passed before the image: FUSE protocol/proxy tests, Monitor round-trip, BranchStore Reference/Replica integrity, Storage V2, LayerStackStore, Workspace, SDK, harness, formatting, diff check, and all-target/all-feature warning-denying Clippy. The cached LayerFS build and self-check also passed.

The focused run completed the registered one-Workspace EDIT16 path through all sixteen Exec/Commit/Push durability checkpoints. Monitor custody contains 17 Commit, 17 Push, 17 Exec, 17 Output, two Workspace Create, and two Workspace End receipts. The final strict End failed with `Workspace(Storage(Integrity("Workspace read counter equation")))`; the Workspace End receipt is failed and checked Drop fallback evidence is retained. Therefore no complete EDIT16, COLD-CREATE, PREPEND, READ, storage, or fresh-recovery row exists, and the run is inadmissible for performance interpretation.

`run.sh` exited before its normal failure-seal epilogue, so raw custody and terminal-invalid status were created manually before any code change. The inventory includes the source seal, image/environment evidence, mountinfo, Monitor log, both Store files, fallback status, exact stderr, and terminal record. The next action is strictly to expose or unit-reproduce the rejected equation, correct the passive accounting without relaxing validation, rerun focused gates, and append a new round. No read optimization may be combined with that correction.

## Round 053 — read-counters-v2-07b1fc2a-20260831 (invalid)

- Status: INVALID / ROOT-CAUSE-NARROWED (empty-read receipts suppressed correctly, but EDIT16 performs real kernel reads and a nonempty equation still failed; no performance row)
- UTC timestamp: 2026-08-31T02:28:10Z through 2026-08-31T02:28:15Z; manually sealed and appended 2026-08-31T02:29:16Z before the next correction
- Local timestamp and timezone: 2026-08-31 10:28:10–10:28:15 CST; manually sealed and appended 10:29:16 CST
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `f9ee37e02df4e1a2dab0a6b618161700747997251faa6973f6cdd50af9b6f7a8`
- Image/digest: `layerfs-fs-benchmark-pro:read-counters-v2-07b1fc2a`; `sha256:23ae047179e16d00f4538764409dd29264d2b6c7c4ce64f0f137c0ebedebc342`; arm64; cached bases, `--pull=false`, no Computer build or execution
- Schedule seed/sample: `09b0df1accdc8b98033ace2af50b05749dfed50b1ef48b76ba63587f7cb6d9fb`; focused LayerFS attempt aborted before a complete sample
- Measurement container: `19d9897849327f97bccf08b1f7d16a5967f699c0c24d41e2e2037eeeb06eb608`; no recovery container
- Captured working-tree/index/status hashes: `a11706da85b187c392bd55887981ef46bfa756ffdfcc5e0e8b4e5431e0a2e27a`; `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`; `419f87ce2421e38b7ab615f29fbcff696dfce105c058a2065e4837e41eaec626`
- Raw evidence: `runs/read-counters-v2-07b1fc2a-20260831/`; 30 verified entries; inventory SHA-256 `ee671e721f9da0da2d89c2e843b98964831c5e944a87d96a9b855717aed13384`

### Correction tested and result

The only source correction after Round 052 made `record_read_metrics` omit a receipt when `kernel_read_requests == 0`. The registered READ row still requires exactly one nonempty validated receipt, so this did not weaken evidence. Focused FUSE/Monitor/Workspace/harness tests, warning-denying Clippy, cached build, and image self-check passed.

The rerun again completed all sixteen edit Exec/Commit/Push checkpoints and failed at final strict Workspace End with the same typed counter-equation error. Therefore the edit Workspace was not empty: ordinary 10-byte O_RDWR pwrite/FUSE page-cache behavior generated one or more real kernel reads. Suppressing empty receipts was correct but did not address the nonempty mismatch. No later scenario or recovery ran.

The next correction must make `WorkspaceReadReceipt::validate` report distinct static equation failures, then add a focused live read/edit oracle that preserves the values needed for diagnosis. Do not drop equality checks, relabel bytes, or combine clone/auth optimization with this evidence correction. The source-identical failure is sealed with exact mountinfo, Monitor/Store files, failed End, checked fallback, and terminal-invalid custody.

## Round 054 — read-counters-v3-07b1fc2a-20260831 (invalid)

- Status: INVALID / EXACT-DEFECT-IDENTIFIED (`Workspace read served bytes`; no performance row)
- UTC timestamp: 2026-08-31T02:32:22Z through 2026-08-31T02:32:27Z; manually sealed before correction
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `888f5b64ad233f1b2a856edc05cb33f07cfb9d2b433f299df2458337993726c0`
- Image/digest: `layerfs-fs-benchmark-pro:read-counters-v3-07b1fc2a`; `sha256:2d109ba8b0b85e9a24ef62bee58325c70b384b9a9064db9023dfb88e5ea6219a`; arm64; cached bases, `--pull=false`, no Computer build or execution
- Schedule seed/sample: `fc1c315c016e677a53219b20ce618f6065b596b9ca21caa3c65e42a050a492f4`; focused LayerFS attempt aborted before a complete sample
- Measurement container: `5ea9fd018a5cd0ea4afcf61515bd24bb3b45d0b1581c03ffa9a58980070a036a`; no recovery container
- Captured working-tree/index/status hashes: `16ac12d9071c62d37762f2f3ec1a62409de66165ecfe44a1ad9c3db765a05a1a`; empty index; status hash unchanged from Rounds 052–053
- Raw evidence: `runs/read-counters-v3-07b1fc2a-20260831/`; 30 verified entries; inventory SHA-256 `fdbf5037a70ac5497b46ad9bc0881ad4d7062a24f17c46326cc5622b098495e3`

### Finding and next correction

The source changed only the validator's static error classification; every equality check remained intact. The cached rerun again completed all EDIT16 checkpoints and failed at final strict End, now specifically at `read_ahead_served_bytes != workspace_output_bytes`.

Transport metrics correctly accumulate for the lifetime of the one retained FUSE mount. Workspace-side metrics were stored inside `SnapshotReader`; each successful in-place Commit rebase replaced that reader with the committed snapshot reader and therefore discarded prior read totals while the transport totals continued accumulating. Partial 10-byte FUSE writes can legitimately trigger kernel read/page-fill activity, so EDIT16 exposed the mismatch even before READ-SYNC-32M.

The minimal correction is to preserve only the shared passive metrics accumulator when installing the new authenticated SnapshotReader during in-place rebase. Root/head/base/reader data, caches, NodeIds, and all product semantics remain unchanged. Add a focused unit oracle that proves accumulator continuity across reader replacement; rerun the same gates and focused profile before any clone/hash change.

## Round 055 — read-counters-v4-07b1fc2a-20260831

- Status: VALID PASSIVE BASELINE (balanced end-to-end read attribution; exact recovery PASS; no read optimization yet)
- UTC timestamp: 2026-08-31T02:37:57Z through 2026-08-31T02:38:55Z; custody verified and appended 2026-08-31T02:39:45Z
- Local timestamp and timezone: 2026-08-31 10:37:57–10:38:55 CST; custody verified and appended 10:39:45 CST
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `c29520bd960230514dff40083f98a08036fe1f6fde013e70d1406136f66f9c42`
- Image/digest: `layerfs-fs-benchmark-pro:read-counters-v4-07b1fc2a`; `sha256:8065f65a428a65e6a7b12298a14030bacbd6ec123e4ed9bba2fa084b50cfa5bc`; arm64; cached bases, `--pull=false`, no Computer build or execution
- Schedule seed/sample: `b517f14881bd2a18c1ab969b5a9941b8dcbe2888f97b06185d52b009ba103ce9`; one focused LayerFS sample
- Measurement/recovery containers: `5b9f9c8d73e82c1737eb76148751f036a97b1bca2d461cf932c36037e7acfad5`; `d1fe2cef2b4bd309dc04e7ffaf36633dc19f9e13a5cdfc27ae2649b33abcd453`
- Captured working-tree/index/status hashes: `694cb4ff421212b856b3fd4ac4b3177f0377847063dd9b410f864cccf317d7c5`; `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`; `419f87ce2421e38b7ab615f29fbcff696dfce105c058a2065e4837e41eaec626`
- Raw evidence: `runs/read-counters-v4-07b1fc2a-20260831/`; 264 verified entries; inventory SHA-256 `46e73bd9109628ccc1cb0cab179d38417a5eea5a45a6393a5670e53e795dc5ed`

### Instrumentation and correctness

The final passive design emits one `WorkspaceReadReceipt` only for a Workspace with actual kernel FUSE reads. The receipt records negotiated readahead/capabilities; kernel count/size buckets; read-ahead hits, misses, fetches, requested/fetched/served/unused/cache-copy bytes; exact host/client response frame and payload-copy equations; response encoding/socket/decode/dispatch timing; Workspace read/rope-plan/payload totals; local and parent query rows/bytes/timing; Reference-boundary and Core authentication counts/bytes/timing; ordered clone/move bytes; and collection overhead. Counters are aggregate only and trigger no additional scan, query, copy, or hash.

In-place rebase now installs the new authenticated SnapshotReader while preserving only the prior Workspace's shared metrics accumulator. A focused oracle proves metrics continuity across reader replacement; object data, head/root/base, parent route, caches, and product semantics are not retained by that helper. Empty mutation-only Workspaces do not emit a false read receipt, while the registered READ scenario fails unless exactly one nonempty receipt validates.

FUSE protocol/proxy, Monitor round-trip, BranchStore Reference/Replica/corruption, Storage V2, LayerStackStore, Workspace rebase, SDK, harness, formatting, diff, and warning-denying Clippy gates passed. The full focused run retained exact mountinfo, final bytes/digest, Store identities, and fresh-container recovery.

### READ-SYNC-32M attribution

Complete READ-SYNC-32M was `444,584,584 ns`: Workspace create `42,903,542`, public SDK Exec-to-terminal `393,676,334`, and strict End `8,004,708`. This remains a primary loss versus the current diagnostic Computer reference and is not formal paired evidence.

The kernel issued 257 reads totaling `33,558,528` requested bytes: 256 requests in the <=256 KiB bucket plus one <=4 KiB request. The proxy served the exact `33,554,442` file bytes using 254 read-ahead hits and three misses/fetches. It requested 48 MiB of 16 MiB read-ahead windows, fetched/served exactly `33,554,442`, left zero unused bytes, and copied the served bytes once from the read-ahead cache.

Host/client response custody balanced at three frames and `33,554,469` frame bytes. Current framing copies all `33,554,442` payload bytes on the host and again during client decode. Host response encode was `1,818,875 ns`, socket write `7,517,709`, client socket read/wait `202,630,210`, client decode `4,141,499`, and host dispatch `189,468,375`. Client socket time includes waiting for host dispatch and must not be added to it as an independent phase.

Workspace dispatch built three rope ReadPlans, read 22 mapping/state nodes, selected 1,720 payload IDs in 29 batches with exact maximum 64, and output `33,554,442` bytes in `189,444,834 ns`. SnapshotReader made 64 local calls for 1,755 requested IDs, returning 50 local rows / `388,245` bytes in `4,816,628 ns`; it made 31 parent calls for 1,705 rows / `33,319,292` bytes in `115,931,746 ns`.

The independent Reference boundary authenticated all 1,705 parent objects / `33,319,292` bytes in `32,883,388 ns`. Core authenticated 1,755 canonical objects / `33,707,537` bytes again in `31,863,375 ns`. Ordered reconstruction cloned `33,625,658` bytes and moved zero. Current authority `StoreDb::read_object_rows` additionally performs an at-rest hash in `existing_object_rows_on`, then clones and hashes identical returned bytes a second time before this receipt's Reference/Core checks; that internal portion remains included in the measured parent-call time.

### Other rows and decision

Complete COLD-CREATE-32M was `2,126,213,919 ns`; EDIT16 `1,085,979,211`; PREPEND `688,915,666`; focused aggregate `4,345,693,380`; fresh recovery `470,530,501`, all exact. These are single-sample regression guards, not paired claims.

The next isolated P1 change is now justified: move unique payload Vecs out of StoreDb/SnapshotReader maps instead of cloning, preserve repeated-ID fallback/order/cardinality/missing behavior, and remove only `read_object_rows`' second same-Store authentication after `existing_object_rows_on` already authenticated those exact bytes. Keep one at-rest Store authentication, the independent Reference authentication, and Core's current consumer authentication. The receipt must show ordered clone bytes near zero for this unique payload and lower parent time without corruption or identity regressions.

## Round 056 — read-move-auth-07b1fc2a-20260831

- Status: READ-IMPROVED (unique Vec moves and same-Store duplicate hash removal PASS; trust boundaries/recovery PASS; no paired/formal claim)
- UTC timestamp: 2026-08-31T02:52:39Z through 2026-08-31T02:53:37Z; custody verified and appended 2026-08-31T02:54:15Z
- Local timestamp and timezone: 2026-08-31 10:52:39–10:53:37 CST; custody verified and appended 10:54:15 CST
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `a1fc33df3d5f7ee19c16bd07e4c6eea0884da2d528dbd19ab6327c5d83c41660`
- Image/digest: `layerfs-fs-benchmark-pro:read-move-auth-07b1fc2a`; `sha256:697b6c8aec1a262ba9cf4c61d1f4a56f7b4e2e2c0bb94b3d818972b3dedd205c`; arm64; cached bases, `--pull=false`, no Computer build or execution
- Schedule seed/sample: `2b41094bedd36b0c1394a6f48539dfdef7ea0230a65fc36634d8a059d16b3ef5`; one focused LayerFS sample
- Measurement/recovery containers: `dae176e0bcd97cb320d8fdb6dd67d4799b1b6156f336c3bbceb2b8d25495a007`; `f2b5063f9513250536419d600310a14d4b011ef658ddf62b313a29377b014545`
- Captured working-tree/index/status hashes: `bbcd8d648ae18f0379b837e51682cdd13ef250e744bee8927db40103b691bb07`; `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`; `419f87ce2421e38b7ab615f29fbcff696dfce105c058a2065e4837e41eaec626`
- Raw evidence: `runs/read-move-auth-07b1fc2a-20260831/`; 264 verified entries; inventory SHA-256 `4a695bd3ac10d1a6dedeec84b2587b9d610365b4e94f1097de780aecfdcc6310`

### Change and proof

`StoreDb::read_object_rows` now reconstructs requested order by moving each unique authenticated Vec from the result map. Repeated request IDs clone only until their final occurrence, which moves the retained Vec. It no longer immediately re-hashes bytes already authenticated by `existing_object_rows_on` on the same Store connection. `SnapshotReader::ordered` uses the same move-last rule and records exact cloned versus moved bytes.

Focused oracles cover unique and repeated IDs, exact order, byte identity, final-occurrence moves, and missing-row errors in both StoreDb and SnapshotReader. Full Storage V2, BranchStore Reference/Replica and corruption tests, LayerStackStore, Workspace, Monitor, SDK, harness, formatting, diff, and warning-denying Clippy gates passed. At-rest Store authentication in `existing_object_rows_on`, independent Reference-boundary authentication, Core consumer authentication, local-first behavior, parent-unavailable errors, exact missing/cardinality behavior, real FUSE, and recovery were retained.

### Measured result

READ-SYNC-32M complete improved `444,584,584 -> 431,861,916 ns` (-2.86%, -12.72 ms). Public SDK Exec-to-terminal improved `393,676,334 -> 375,854,000 ns` (-4.53%, -17.82 ms). Host dispatch fell `189,468,375 -> 160,154,333 ns` (-15.47%, -29.31 ms), and parent read/auth call time fell `115,931,746 -> 89,437,959 ns` (-22.85%, -26.49 ms).

The topology remained exact: 257 kernel reads, 254 hits, three 16 MiB fetches, exact `33,554,442` fetched/served bytes, three balanced response frames, three ReadPlans, 22 rope nodes, 1,720 payload IDs, 29 batches of at most 64, 64 local calls, and 31 parent calls. Ordered clone bytes fell `33,625,658 -> 0`; ordered move bytes rose `0 -> 33,625,658`, proving the intended ownership transfer rather than hidden copy. Reference authentication remained `1,705 / 33,319,292` and Core authentication `1,755 / 33,707,537`; their measured times remained approximately 31.6 and 31.9 ms, respectively.

Complete COLD-CREATE-32M was `2,158,018,710 ns`; EDIT16 `1,097,155,708`; PREPEND `662,365,626`; focused aggregate `4,349,401,960`; fresh recovery `430,106,417`, all exact. Cross-row movement is within single-sample host-bind noise; only the clone/auth boundary and associated read phases are claimed.

Retain P1. The next isolated P2 change is to formalize a narrow authenticated-batch contract: SnapshotReader must return bytes already authenticated once at the local Store boundary or once again at the Reference receiver boundary, allowing CoreReader's batch path to validate order/cardinality while decoding without a third hash. Single-object Core reads and untrusted/default ObjectSource implementations retain authentication. Corrupt local, corrupt parent, wrong ID/order/cardinality, missing parent, and unavailable parent tests must remain hard gates. Seal before response framing or batch-size changes.

## Round 057 — read-auth-contract-07b1fc2a-20260831

- Status: READ-IMPROVED (narrow authenticated-batch contract PASS; independent Store/Reference integrity retained; exact recovery PASS)
- UTC timestamp: 2026-08-31T03:05:48Z through 2026-08-31T03:06:47Z; custody verified and appended 2026-08-31T03:07:09Z
- Local timestamp and timezone: 2026-08-31 11:05:48–11:06:47 CST; custody verified and appended 11:07:09 CST
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `920b006d5c02ddbe882704a496d603533c99fc118aad8d49c49126ea7fdc9754`
- Image/digest: `layerfs-fs-benchmark-pro:read-auth-contract-07b1fc2a`; `sha256:382316f4cafad95f535dcc60a9684371b277cd431a14d310e2f1f1e4d9531a6e`; arm64; cached bases, `--pull=false`, no Computer build or execution
- Schedule seed/sample: `72deb3cd1981ef66fb1622d5673d1b8781a41229ed7ff95207e9009a998bb0e6`; one focused LayerFS sample
- Measurement/recovery containers: `ecd93fcdc95f4b772a9930a8bd16aca1df66e29e8059e500e75d7e087e89868e`; `c577acad52a5c4b150b217ec0ed8f81423835460a6cf8d5e15c954adef736c1c`
- Captured working-tree/index/status hashes: `63b3eeb368df667bd14275154d2014a5900908fed13acf14d702314144e59f11`; `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`; `419f87ce2421e38b7ab615f29fbcff696dfce105c058a2065e4837e41eaec626`
- Raw evidence: `runs/read-auth-contract-07b1fc2a-20260831/`; 264 verified entries; inventory SHA-256 `af597b5eee302720ee39f2eba7ee5942f65895feefdaa2ce8496e3b9b62a4ed4`

### Contract and proof

`ObjectSource::read_authenticated_objects` now defines the narrow batch contract. Its default implementation remains safe for arbitrary sources: it requires exact cardinality/order and hashes every returned canonical object. SnapshotReader overrides only after `existing_object_rows_on` authenticated local Store bytes and after it independently authenticated every parent-returned object at the Reference receiver boundary. CoreReader still checks exact cardinality and ID order before decoding, but does not hash that authenticated batch a third time. Single-object Core reads retain their existing authentication.

SnapshotReader now also rejects a valid but reordered parent batch before insertion/decoding. Focused tests prove generic wrong-ID and corrupt-byte rejection, Reference parent order rejection, duplicate order/cardinality, local corruption, parent corruption, missing/unavailable parent, and no fallback for protected local roots. Full Storage/BranchStore/LayerStackStore/Workspace/Monitor/SDK/harness, 4/32/256 MiB recovery, formatting, diff, and warning-denying Clippy gates passed.

### Measured result

READ-SYNC-32M complete improved `431,861,916 -> 394,839,374 ns` (-8.57%, -37.02 ms). Public SDK Exec-to-terminal improved `375,854,000 -> 340,798,041 ns` (-9.33%, -35.06 ms). Host dispatch fell `160,154,333 -> 124,181,458 ns` (-22.46%, -35.97 ms).

Core authentication collapsed from `1,755 / 33,707,537 bytes / 31,939,585 ns` to only the 33 single-object metadata reads / `81,879 bytes / 121,294 ns` (-99.62% time). Independent Reference authentication remained `1,705 / 33,319,292 bytes / 31,587,129 ns`, proving the trust boundary was retained. Parent read time was `85,616,751 ns`; ordered clone stayed zero and move stayed exact at `33,625,658` bytes.

All topology/custody equations stayed fixed: 257 kernel requests, three 16 MiB fetches, exact `33,554,442` bytes, three balanced frames, three ReadPlans, 22 rope nodes, 1,720 payload IDs, 29 batches at maximum 64, 64 local calls, and 31 parent calls. Current response handling still copies all `33,554,442` bytes once into the host frame and once from the client frame; host encode was `1,610,375 ns`, client decode `2,559,792`, and cache slicing copied another complete file.

Complete COLD-CREATE-32M was `2,180,982,251 ns`; EDIT16 `1,010,785,458`; PREPEND `614,260,085`; focused aggregate `4,200,867,168`; fresh recovery `396,771,875`, all exact. These remain one-sample regression guards.

Retain P2. Next isolate P3: special-case `Response::Bytes` to scatter-write the exact existing frame header/tag/length plus borrowed payload, and decode the payload directly into the final Vec. Preserve byte-identical wire format, the 17 MiB frame/16 MiB payload caps, truncation/oversize/error handling, ordering, and the validated host/client frame equation. Seal before changing the 64-object rope batch or ReadPlan lifetime.

## Round 058 — read-response-stream-07b1fc2a-20260831

- Status: READ-IMPROVED (borrowed Bytes response framing/direct final-Vec decode PASS; wire/integrity/recovery PASS)
- UTC timestamp: 2026-08-31T03:16:50Z through 2026-08-31T03:17:49Z; custody verified and appended 2026-08-31T03:18:14Z
- Local timestamp and timezone: 2026-08-31 11:16:50–11:17:49 CST; custody verified and appended 11:18:14 CST
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `e26aa2e789d8eda78d04d4791dfe891b0cea300b1ffa7ddad911036567765d34`
- Image/digest: `layerfs-fs-benchmark-pro:read-response-stream-07b1fc2a`; `sha256:5d1977623cbe319071a8b00407afd8d4573e3e2690b3d1c49567fc9342716f8e`; arm64; cached bases, `--pull=false`, no Computer build or execution
- Schedule seed/sample: `4a7697753cda75efba62399c036fa479581cbc7fe8669843b75fab82a89ef0c6`; one focused LayerFS sample
- Measurement/recovery containers: `db344d42420145bf807fcd3995075896e480578de5c875e14e7833ed386788e0`; `431026e3a3971a6a254adab0aefa9eb649249eb469a6b5cf3825872b079688b0`
- Captured working-tree/index/status hashes: `cfb35377b53bab3abbf3ccdf257e1a1315efd94480b7ff82433eb88cd72f87b0`; `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`; `419f87ce2421e38b7ab615f29fbcff696dfce105c058a2065e4837e41eaec626`
- Raw evidence: `runs/read-response-stream-07b1fc2a-20260831/`; 264 verified entries; inventory SHA-256 `b844a46675ca10029d9b593a3a2d85c1c0801738c6a1e7dd34c4408fb81bdcc7`

### Change and proof

`Response::Bytes` retains its exact wire layout: four-byte outer body length, one-byte Bytes tag, four-byte payload length, and payload. The host now constructs only the nine-byte frame header and writes that plus the borrowed response slice to the existing ordered TCP stream. The client validates the outer length/tag/payload length and reads directly into the final response Vec. Other response variants retain the generic codec.

Focused protocol tests require exact header/payload bytes, measured frame/logical/copy counters, truncation rejection, and round-trip identity. Live proxy tests require host/client frame count/byte equality and matching zero-copy counters. The 17 MiB frame and 16 MiB payload caps, error variants, one authenticated connection, read-ahead behavior, FUSE semantics, and public SDK path are unchanged. FUSE, Storage, Workspace, Monitor, harness, formatting, diff, warning-denying Clippy, mountinfo, exact final oracle, and fresh recovery passed.

### Measured result

READ-SYNC-32M complete improved `394,839,374 -> 382,722,583 ns` (-3.07%, -12.12 ms). Public SDK Exec-to-terminal improved `340,798,041 -> 327,945,458 ns` (-3.77%, -12.85 ms).

Both host response-copy and client decode-copy counters fell `33,554,442 -> 0`. Exact frame custody remained three frames / `33,554,469` bytes on both sides. Host encode fell `1,610,375 -> 125 ns`; client decode fell `2,559,792 -> 72,709 ns`; host dispatch was `122,224,167 ns`; client socket read/wait `130,370,833`; socket write `6,524,042`. The full 33.55 MiB cache-slice copy remains and is not relabeled or subtracted.

Read/storage topology and trust checks stayed stable: three 16 MiB fetches, three ReadPlans, 22 nodes, 1,720 payload IDs, 29 batches capped at 64, 64 local calls, 31 parent calls, `1,705 / 33,319,292` Reference authentications, 33 metadata Core authentications, zero ordered clone, and exact ordered move.

Complete COLD-CREATE-32M was `2,251,951,501 ns`; EDIT16 `1,046,276,376`; PREPEND `603,311,042`; focused aggregate `4,284,261,502`; fresh recovery `395,639,291`, all exact. These remain single-sample regression guards.

Retain P3. Next isolate P4: raise rope payload batching from the fixed 64-object trigger to a byte-bounded maximum below the 128-object/4 MiB Store limits. Preserve exact order, duplicates, payload authentication, chunk length checks, and memory bounds. Expected evidence is approximately 29 -> 15 payload batches and fewer local/parent calls; do not change ReadPlan caching or read-ahead in the same seal.

## Round 059 — read-batch-127-07b1fc2a-20260831

- Status: READ-IMPROVED (127-object / <=4 MiB authenticated payload batches PASS; exact read/recovery PASS)
- UTC timestamp: 2026-08-31T03:40:36Z through 2026-08-31T03:41:35Z; custody verified and appended 2026-08-31T03:42:13Z
- Local timestamp and timezone: 2026-08-31 11:40:36–11:41:35 CST; custody verified and appended 11:42:13 CST
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `bc587c673dbbf57ffb266a859f2bbe3e52b75c137f9ef2e982bee03cf33fb7d1`
- Image/digest: `layerfs-fs-benchmark-pro:read-batch-127-07b1fc2a`; `sha256:105f536fe46656da36ece465eafa25713e5c282fbbca2cdcf454708526b5ad4a`; arm64; cached bases, `--pull=false`, no Computer build or execution
- Schedule seed/sample: `d94c725944fc2d06b73c4acb635014fc0eaff90522382ed9ca62950a7993066c`; one focused LayerFS sample
- Measurement/recovery containers: `9e6684c5a3d947103f9eadd579dfd551fad616cf9283299e1efef771adef665a`; `9b63bad0537d69af503aeac20281eeb4e8bc8f637449968c7f384194b2d044d3`
- Captured working-tree/index/status hashes: `9d946300954d27b502a4cba340027da586c533cf5d8b48d42ac944095f232899`; `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`; `cbe891c7cd808dded6c3bfb9c26d3a1424b50ba18970733d0a94860119af791c`
- Raw evidence: `runs/read-batch-127-07b1fc2a-20260831/`; 264 verified entries; inventory SHA-256 `04ede13eca812091bd7c0b63f3c0c612c77e22424a8626781520c98c341c9c03`

### Change and proof

The rope reader's fixed payload batch capacity/flush trigger changed only from 64 to 127 IDs. A canonical chunk object is hard-bounded at 32,789 bytes including framing, so a compile-time assertion proves 127 maximum-sized objects remain below 4 MiB. No runtime payload pre-read, hash, query, or scan was added.

The cross-mapping-leaf oracle now requires a 195-payload read to use two batches (127 + 68), records exactly 195 payload IDs, maximum batch 127, and asserts actual canonical bytes for every batch remain <=4 MiB. The complete content corpus, randomized splices, literal IDs, Storage/BranchStore/Workspace/SDK/harness, corruption/order/cardinality, formatting, diff, warning-denying Clippy, exact final oracle, mountinfo, and recovery gates passed. A pre-benchmark cached build was canceled when release rustc exposed two constants used only in a const assertion as dead code; the constants were folded into the assertion, a new source seal was built warning-free, and no run existed for the canceled image.

### Measured result

READ-SYNC-32M complete improved `382,722,583 -> 368,483,668 ns` (-3.72%, -14.24 ms). Public SDK Exec-to-terminal improved `327,945,458 -> 302,915,792 ns` (-7.63%, -25.03 ms). Host dispatch fell `122,224,167 -> 102,550,459 ns` (-16.10%, -19.67 ms).

Payload batches fell exactly `29 -> 15`, maximum batch rose `64 -> 127`, and payload IDs/bytes remained exact at `1,720 / 33,554,442`. Parent calls fell `31 -> 17` and parent time `84,229,753 -> 65,338,750 ns` (-22.43%, -18.89 ms). Local calls fell `64 -> 50` and local read/auth time `4,575,922 -> 3,569,414 ns`. Reference authentication remained exactly `1,705 / 33,319,292`; Core remained 33 metadata objects / 81,879 bytes; ordered clone stayed zero and move exact.

The FUSE/transport topology remained 257 kernel reads, three 16 MiB fetches, 254 hits, exact served bytes, three balanced zero-copy frames, and one complete cache-slice copy. Complete COLD-CREATE-32M was `2,288,822,000 ns`; EDIT16 `1,072,944,416`; PREPEND `607,485,959`; focused aggregate `4,337,736,043`; fresh recovery `364,115,750`, all exact. Only the intended read/query phases are claimed.

Retain P4. Next isolate P5: cache the immutable rope ReadPlan for an exact `(root, file state, reader/rebase generation)` within the live Workspace and reuse it across contiguous proxy fetches. Invalidate on mutation, root/rebase change, truncate, and End; never carry the plan across a different root/reader. Add first/repeat, mutation, and rebase invalidation oracles. Do not change read-ahead size, cache-slice copies, or parallelism in the same seal.

## Round 060 — read-plan-cache-07b1fc2a-20260831

- Status: MECHANISM-PASS / PERFORMANCE-REGRESSED (exact keyed plan reuse and invalidation PASS; retain/reject pending one source-identical repeat)
- UTC timestamp: 2026-08-31T03:59:13Z through 2026-08-31T04:00:13Z; custody verified and appended 2026-08-31T04:00:50Z
- Local timestamp and timezone: 2026-08-31 11:59:13–12:00:13 CST; custody verified and appended 12:00:50 CST
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `2786c3e019161c2f8b94bc7cb8e556413487e4052f54b0f532044b1f0886f524`
- Image/digest: `layerfs-fs-benchmark-pro:read-plan-cache-07b1fc2a`; `sha256:c345f8d2b257cb162f773b5e4252cf7fc6875ffee12d022870131e932f6c86dc`; arm64; cached bases, `--pull=false`, no Computer build or execution
- Schedule seed/sample: `65df5cb952846aa3c92f1052166c19dfe258b10e62bf73b21db855dbe188ffe6`; one focused LayerFS sample
- Measurement/recovery containers: `1d319b9519bc1e7a09ade3d1daf8979772de56306d47237fd8031c6c4caf7390`; `6f8e14ebc5bb7a2933863182ebea5b6810664309e66fe6560c6c64efb7f28e76`
- Captured working-tree/index/status hashes: `12f367100d52cba8ad06e0a8faf325268376cec10899af6e20b3ff33ab432f77`; `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`; `cbe891c7cd808dded6c3bfb9c26d3a1424b50ba18970733d0a94860119af791c`
- Raw evidence: `runs/read-plan-cache-07b1fc2a-20260831/`; 264 verified entries; inventory SHA-256 `c6816ec948229ba48622181a0e16e48452b416240648225c9fac37533afe72f4`

### Mechanism and proof

Workspace now retains one immutable rope ReadPlan as an `Arc`, keyed by exact snapshot root and file-state root. The common mutation-generation hook clears it for every write/namespace mutation; successful in-place rebase clears it after installing the new reader/root; Workspace destruction drops it. Overlay reads may reuse only the plan for their exact base root. No cache crosses a different snapshot/reader identity.

A focused oracle proves two reads build once, mutation invalidates, the next read rebuilds, and successful Rebased Commit invalidates again. Workspace/SDK/harness, integrity/recovery, formatting, diff, and warning-denying Clippy gates passed.

The live receipt proves the mechanism: ReadPlan builds `3 -> 1`, rope nodes `22 -> 18`, local calls `50 -> 46`, local IDs/rows `1,755/50 -> 1,751/46`, and Core metadata authentications `33 -> 29`. Payload batches, parent calls, Reference authentication, FUSE requests/fetches, frames, and exact bytes stayed unchanged.

### Performance and decision gate

This sample regressed READ complete `368,483,668 -> 387,469,834 ns` (+5.15%, +18.99 ms) and public Exec `302,915,792 -> 326,182,667 ns` (+7.68%, +23.27 ms). Host dispatch rose `102,550,459 -> 104,877,709 ns`; parent time was `66,615,793 ns`. Complete create was `2,374,778,125`; EDIT16 `1,105,269,501`; PREPEND `635,644,667`; aggregate `4,503,162,127`; recovery `373,950,584`, all exact.

The intended saved work is small, and this is one noisy host-bind sample. Make no source change yet. Run and append one source-identical focused repeat of `sha256:c345f8d2b257cb162f773b5e4252cf7fc6875ffee12d022870131e932f6c86dc`. Retain P5 only if the repeat preserves the exact 1-build/18-node/46-local-call mechanism and the two-run read distribution is not worse than Round 059; otherwise revert only the plan cache and seal the restoration before further P6 work.

## Round 061 — read-plan-cache-repeat-07b1fc2a-20260831

- Status: SOURCE-IDENTICAL REPEAT / P5 REJECT (mechanism repeats exactly; no phase-local performance benefit over Round 059)
- UTC timestamp: 2026-08-31T04:01:16Z through 2026-08-31T04:02:16Z; custody verified and appended 2026-08-31T04:02:48Z
- Local timestamp and timezone: 2026-08-31 12:01:16–12:02:16 CST; custody verified and appended 12:02:48 CST
- Source/image: exact Round 060 source seal `2786c3e019161c2f8b94bc7cb8e556413487e4052f54b0f532044b1f0886f524`; image digest `sha256:c345f8d2b257cb162f773b5e4252cf7fc6875ffee12d022870131e932f6c86dc`; no rebuild and no Computer execution
- Schedule seed/sample: `93850f1c5184b4d3d272839f4dee81300e1bd2a9a700aa789f358368829244c2`; one focused repeat
- Measurement/recovery containers: `a9d9fe3fe5b2549fc1df8f999d66701bfb8f569418ae164d09e6e8c083a54c06`; `0aeb1ff35dc213e04614ed016d1471c66b94692d7cc91d97c0cb1712c39cb8a5`
- Captured working-tree/index/status hashes: `12f367100d52cba8ad06e0a8faf325268376cec10899af6e20b3ff33ab432f77`; empty; `cbe891c7cd808dded6c3bfb9c26d3a1424b50ba18970733d0a94860119af791c`
- Raw evidence: `runs/read-plan-cache-repeat-07b1fc2a-20260831/`; 264 verified entries; inventory SHA-256 `a15b8d45b4f1ed9e8f11b1839060724abe6fa0012233054a41082b56998de026`

The repeat produced READ complete `349,748,876 ns`, public Exec `301,714,292`, host dispatch `106,671,958`, and parent time `69,458,458`. It repeated the exact mechanism: one plan build, 18 rope nodes, 46 local calls, 29 Core metadata authentications, 15 payload batches, 17 parent calls, and every byte/trust/frame equation unchanged.

Across the two P5 samples, mean complete READ is `368,609,355 ns`, effectively flat/slightly worse than Round 059's `368,483,668`. Mean public Exec is `313,948,480 ns` versus `302,915,792`; mean host dispatch `105,774,834` versus `102,550,459`; mean parent time `68,037,126` versus `65,338,750`. The saved four local metadata reads do not produce a demonstrated phase-local benefit and the cache adds mutable state/invalidation burden.

Reject P5 under the minimum-complexity rule. Revert only the Workspace rope-plan cache, its accumulator split, and its oracle; restore Round 059's per-fetch plan construction while retaining P1–P4. Verify the source seal returns to the Round 059 mechanism, run a source-identical restoration sample, and append it before considering P6 cache-slice work.

## Round 062 — read-plan-restored-07b1fc2a-20260831

- Status: RESTORED / P5 REJECTION CONFIRMED (P1–P4 retained; plan cache removed; exact recovery PASS)
- UTC timestamp: 2026-08-31T04:07:12Z through 2026-08-31T04:08:11Z; custody verified and appended 2026-08-31T04:08:36Z
- Local timestamp and timezone: 2026-08-31 12:07:12–12:08:11 CST; custody verified and appended 12:08:36 CST
- Source/image: source seal returned exactly to Round 059 `bc587c673dbbf57ffb266a859f2bbe3e52b75c137f9ef2e982bee03cf33fb7d1`; reused image digest `sha256:105f536fe46656da36ece465eafa25713e5c282fbbca2cdcf454708526b5ad4a`; no rebuild and no Computer execution
- Schedule seed/sample: `9291100df9186416705b91c0718ac526adb61432a2b1b8ce1060b889836c86e4`; one focused restoration sample
- Measurement/recovery containers: `b85f347544b65f8e43ee420acb794889819ee75009c51fd29194dfcbad7f3e43`; `0093a7d15548c9c49b38ee6ab21e1c505fee509e7af5cda8067c9068a9eb9d25`
- Captured working-tree/index/status hashes: `9d946300954d27b502a4cba340027da586c533cf5d8b48d42ac944095f232899`; empty; `cbe891c7cd808dded6c3bfb9c26d3a1424b50ba18970733d0a94860119af791c`
- Raw evidence: `runs/read-plan-restored-07b1fc2a-20260831/`; 264 verified entries; inventory SHA-256 `985f7633f02c20fdfe4cea36817b388e8a3134e0ee76a2858fbbc39b6609e453`

The plan-cache field, keyed Arc, cache lookup/build path, mutation/rebase invalidation code, metrics split, and dedicated cache oracle were removed. P1 Vec moves/same-Store auth cleanup, P2 authenticated-batch contract, P3 zero-copy Bytes response, P4 127-ID/<=4 MiB batches, and all passive read evidence remain. Workspace and harness tests, warning-denying Clippy, source-seal equality, exact final oracle, mountinfo, Store identities, and recovery passed.

Restored READ complete was `361,647,625 ns`; public Exec `313,653,375`; host dispatch `103,379,166`; parent time `66,153,791`. The mechanism returned exactly to three plan builds, 22 rope nodes, 50 local calls, 33 Core metadata authentications, 15 payload batches, and 17 parent calls.

Across Round 059 plus this restoration, no-cache mean complete READ is `365,065,647 ns`. Across the two P5 samples it is `368,609,355 ns`; phase-local host and parent work also showed no cache benefit. P5 is rejected and removed.

Current read gains versus the passive Round 055 baseline are substantial but incomplete: complete READ improved from `444,584,584` to the `361.65–368.48 ms` restored distribution; public Exec improved from `393,676,334` to `302.92–313.65 ms`. This still misses Computer parity and the registered 1.25x/500 MiB/s gates. The remaining measured full-file copy is the 33.55 MiB ProxyClient cache-slice copy; P6 should proceed only with a design that preserves FUSE reply lifetime, read-ahead semantics, bounded memory, and exact error behavior. Do not retain another mutable cache merely for speculative savings.

## Round 063 — single-full-tx-r063-20260831

- Status: VALID TRANSACTION MECHANISM / PERFORMANCE REGRESSION (binding one-`FULL`-transaction design PASS; exact recovery PASS; no superiority claim)
- UTC timestamp: 2026-08-31T06:40:05Z through 2026-08-31T06:41:16Z; local timestamp 2026-08-31 14:40:05–14:41:16 CST
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `a981ae3a57912a26bcfa60a8008d8d68b4bd9b8f7d7252bbf2c11e1485f6a58f`
- Image/digest: `layerfs-fs-benchmark-pro:single-full-tx-r063`; `sha256:0bd73822b5322e31b22aefb35751c06d0bed4979f2e73cb4df4de6185c5a8771`; arm64; cached bases, `--pull=false`, no Computer execution
- Schedule seed/sample: `9cadc582edd792384f55744f4acdaf3a3e3e70f6406f591cdde75f41833910cf`; one focused LayerFS sample
- Measurement/recovery containers: `a5090d19b55900eb64fa3c25784f6c4c6d011ce5fc65f06d000080257edeaee6`; `bd6298f2c73b387354142062ca3ba1edd6de2dbca63fd9a9bb49da3d7dad57c0`
- Captured working-tree/index/status hashes: `908d0ffd95c46ff8c3b382c17a01914dc2ba736b1e58615759bf3d07304feffa`; empty; `3c42922d70b861ce21773a774b8205be506c726beef23f864167ea66d19d6405`
- Raw evidence: `runs/single-full-tx-r063-20260831/`; 265 verified entries after the explicit decision record; inventory SHA-256 `1d6997227bc25718cb34c2ad6fb7eefa4913c83977699f1993713f6bebb83a3e`
- Exact run command: `benchmark/fs-bench-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd layerfs-fs-benchmark-pro:single-full-tx-r063 single-full-tx-r063-20260831`

### Binding transaction change and proof

BranchStore Commit now authenticates, proves closure, and selects exact missing objects outside SQLite, then uses one `BEGIN IMMEDIATE`/`COMMIT` transaction to stream 1,744 objects from the existing spill, insert the immutable Commit and truthful complete-root receipt, and CAS the Branch head/base last. Authority Push owns immutable spillable receiver spools, independently authenticates and validates exact objects/facts, ownership, suffix, transition, and closure outside SQLite, then uses one authority transaction to stream the exact missing objects, insert Commit facts oldest-to-newest, recheck name/head, and publish the authority Branch last. Both Stores remain permanent WAL, `synchronous=FULL`, `wal_autocheckpoint=0`; no sync switching, explicit checkpoint, stable barrier, database/directory fsync, background checkpoint, or hidden post-ack work remains in this profile.

The public Commit receipt contains exactly one `CommitCas` database receipt and Push exactly one `AuthorityPublish` receipt. There are zero `ObjectAdmission`, `FactAdmission`, or durability receipts. Fault-injection tests prove first, middle, and visibility-last failures roll back new object/fact/receipt/head state, and the existing incomplete/missing closure test proves rejection before `BEGIN IMMEDIATE`. The 4/32/256 MiB and repeated-unpushed-WAL recovery proof, full 18-test BranchStore contracts, LayerStackStore, Storage, SDK lifecycle, Workspace, Monitor, harness self-checks, formatting, diff check, and warning-denying Clippy all passed. The real run retained exact host-bind FUSE mountinfo, fresh Bash/helper execution, final bytes/digest, clean End, missing-only equations, and fresh-container reopen.

At the T3-equivalent local boundary, BranchStore contained exactly 1,744 objects / 33,661,702 bytes, the immutable Commit was readable, the Branch head equaled that Commit, and its WAL was 35,333,152 bytes. At T4, authority contained 1,756 objects / 33,662,752 bytes including inherited genesis data, the same Commit and authority Branch head were readable, and its WAL was 35,279,592 bytes. These are transaction-visible WAL diagnostics only: neither boundary claims checkpoint, database-file, or power-loss durability.

### Required same-sequence visibility equations

The exact public sequence produced Workspace Create `55,656,667 ns`, fresh-process Exec-to-terminal `94,592,333 ns`, Workspace Commit API `1,189,933,000 ns`, Push API `1,197,993,292 ns`, and Workspace End `7,424,500 ns`.

- `branchstore_visible_ns = T3 - T0 = 55,656,667 + 94,592,333 + 1,189,933,000 = 1,340,182,000 ns`
- `push_delta_ns = T4 - T3 = push_api_ns = 1,197,993,292 ns`
- `layerstackstore_visible_ns = T4 - T0 = 1,340,182,000 + 1,197,993,292 = 2,538,175,292 ns`
- `complete_lifecycle_ns = T5 - T0 = 2,538,175,292 + 7,424,500 = 2,545,599,792 ns`

The authority comparison boundary is `layerstackstore_visible_ns`; the Push-only delta is not relabeled as full authority time. No fixture reset, cache reset, separate BranchStore sample, or reconstruction occurred between the local Commit and Push in this sample.

### Measured cause and decision

Against Round 062's source-restored diagnostic sample, BranchStore-visible time regressed `1,173,994,418 -> 1,340,182,000 ns` (+166.19 ms), authority-visible time regressed `2,238,190,210 -> 2,538,175,292 ns` (+299.99 ms), and complete lifecycle regressed `2,244,939,793 -> 2,545,599,792 ns` (+300.66 ms). EDIT16 also regressed `1,063,518,628 -> 3,053,362,544 ns`, so this is not accepted as a performance win. PREPEND was `707,467,959 ns`, READ `326,952,626 ns`, and fresh reopen `374,356,376 ns`; exact identity and recovery passed.

The cause is localized. BranchStore's one database transaction was `906,153,750 ns`: 1,746 streamed statements took `574,476,292 ns`, final commit sync `330,335,375 ns`, and visibility-last publication `1,276,625 ns`. Authority's one database transaction was `886,826,250 ns`: statements `556,469,001 ns`, final commit sync `329,495,000 ns`, and publication `768,959 ns`. The former fourteen-transaction path spent only about 1.2–1.6 ms of statement work per 128-object page; the large transaction therefore exposed SQLite dirty-page cache/spill behavior on the host bind rather than application hashing, traversal, or network work inside the transaction.

Retain the binding single-transaction semantics and reporting, but reject any performance claim for Round 063. The next isolated hypothesis is a bounded SQLite page-cache/spill refinement, with measured RSS/WAL/writer-hold proof under the 1 GiB benchmark envelope. It must preserve `synchronous=FULL`, `wal_autocheckpoint=0`, one transaction, streaming from spill, and all visibility/integrity behavior; only after this statement cost is addressed should immediate-Commit candidate payload reuse be measured.

## Round 064 — cache64-r064-20260831

- Status: RETAINED CREATE WIN (bounded 64 MiB SQLite page cache PASS; one-transaction/integrity/recovery proof PASS)
- UTC timestamp: 2026-08-31T06:47:27Z through 2026-08-31T06:48:37Z; local timestamp 2026-08-31 14:47:27–14:48:37 CST
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `268bc674cebb106d2da9263e400ce143db04242fa743509ba9cfc23f514329b8`
- Image/digest: `layerfs-fs-benchmark-pro:cache64-r064`; `sha256:dfe0bd75c7f67db01af94834ccf6a2f00b530296de99e06818394aa562a21fa1`; arm64; cached bases, `--pull=false`, no Computer execution
- Schedule seed/sample: `0769b3f4a90cad1134632bf789bbdbc57c2bf9744deb80dc85007919382adef2`; one focused LayerFS sample
- Measurement/recovery containers: `207a36fb49988cabd6a6ee7551389f0284c25898c3645b17d81d9a6bbdd98b28`; `c70eb59bf66b35a9c48b588d576ca1feab94fbd2dd0a89e93070741ceed52464`
- Captured working-tree/index/status hashes: `ce2bfc60f9180ae5e3de10e3f298b6a9c4e8704194956398db9f0bd70a62226d`; empty; `3c42922d70b861ce21773a774b8205be506c726beef23f864167ea66d19d6405`
- Raw evidence: `runs/cache64-r064-20260831/`; 265 verified entries after the explicit decision record; inventory SHA-256 `18f729fdb76769fdbe561e6afd1da344f7acf8314cbcb074a639b4463e74bde5`
- Exact run command: `benchmark/fs-bench-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd layerfs-fs-benchmark-pro:cache64-r064 cache64-r064-20260831`

### One mechanism and proof

The sole production change raises the already-existing per-connection SQLite page cache from 8 MiB to 64 MiB. Spill remains enabled, so larger 256 MiB transactions remain application-memory bounded rather than forcing all dirty pages into RAM; there is no transaction-size-specific branch or benchmark recognition. The 64 MiB ceiling holds this create sample's 35.3 MiB dirty-WAL working set and stays well inside the measured container's fixed 1 GiB limit.

The exact 4/32/256 MiB plus repeated-unpushed-WAL/reopen test passed, as did schema pragma validation, formatting, diff check, and scoped warning-denying Clippy. The public run repeated one `CommitCas` and one `AuthorityPublish`, zero granular admission/durability transactions, exactly 1,744 transferred/inserted objects / 33,661,702 bytes, 14 application payload pages with a 2,584,837-byte peak, exact final bytes/digest, clean FUSE End, and fresh-container recovery. Branch and authority WAL remained 35,333,152 and 35,279,592 bytes respectively, proving the change removed cache spill cost rather than omitting WAL work.

### Same-sequence visibility equations

The exact public sequence produced Workspace Create `39,752,208 ns`, fresh-process Exec-to-terminal `90,533,500 ns`, Workspace Commit API `979,963,501 ns`, Push API `884,725,334 ns`, and Workspace End `7,763,958 ns`.

- `branchstore_visible_ns = 39,752,208 + 90,533,500 + 979,963,501 = 1,110,249,209 ns`
- `push_delta_ns = push_api_ns = 884,725,334 ns`
- `layerstackstore_visible_ns = 1,110,249,209 + 884,725,334 = 1,994,974,543 ns`
- `complete_lifecycle_ns = 1,994,974,543 + 7,763,958 = 2,002,738,501 ns`

At T3-equivalent visibility, BranchStore again contained exactly 1,744 objects / 33,661,702 bytes, the Commit and head were readable, and WAL was 35,333,152 bytes. At T4, authority contained 1,756 objects / 33,662,752 bytes including genesis, with the same readable Commit/head and a 35,279,592-byte WAL. No fixture/cache reset or separate sample occurred between Commit and Push, and these remain transaction-visible diagnostics rather than checkpoint/power-loss claims.

### Measured result and next cause

Versus Round 063, BranchStore-visible improved `1,340,182,000 -> 1,110,249,209 ns` (-229.93 ms, -17.16%), Push delta `1,197,993,292 -> 884,725,334 ns` (-313.27 ms, -26.15%), LayerStackStore-visible `2,538,175,292 -> 1,994,974,543 ns` (-543.20 ms, -21.40%), and complete lifecycle `2,545,599,792 -> 2,002,738,501 ns` (-542.86 ms, -21.33%). It also beats Round 062's historical fully-checkpointed lifecycle diagnostic by 242.20 ms, but the profiles are distinct and no pooled/formal comparison is claimed.

The intended mechanism is directly visible. Branch transaction statement time collapsed `574,476,292 -> 30,474,875 ns`; authority statement time `556,469,001 -> 31,376,000 ns`. Push source read/auth also fell `152,153,667 -> 39,244,248 ns` because the committed BranchStore pages remained in its bounded cache. The final native `FULL` commit now dominates: `660,785,917 ns` locally and `650,902,750 ns` at authority. Branch transaction total is `691,460,542 ns`; authority is `682,499,750 ns`.

EDIT16 was `2,838,455,841 ns`, PREPEND `696,965,793 ns`, READ `336,008,833 ns`, and fresh reopen `385,848,917 ns`. The edit regression began with the binding uncheckpointed single-transaction profile and remains an explicit guard failure to investigate at its database-sync component; no edit-specific mechanism is introduced here.

Retain the 64 MiB bounded cache. The next isolated create hypothesis is a larger Store page size selected only at creation, before WAL mode, to reduce the number of WAL frames and host-bind write syscalls while preserving canonical IDs, exact bytes, one `FULL` transaction, and the same total WAL durability boundary. Reject it if statement/commit sync, WAL/storage, 4/32/256 memory, reopen, or edit guards regress.

## Round 065 — page64-r065-20260831

- Status: RETAINED CREATE WIN (64 KiB new-Store SQLite pages PASS; one-transaction/integrity/recovery proof PASS)
- UTC timestamp: 2026-08-31T06:58:06Z through 2026-08-31T06:58:53Z; local timestamp 2026-08-31 14:58:06–14:58:53 CST
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `14aa22944d5c7aa6d4252c34c0ec290eec227446ed0a656d5e91f4e87eef9bfd`
- Image/digest: `layerfs-fs-benchmark-pro:page64-r065`; `sha256:96b8def9631a1ffaaf6ad173f2fb1347f0056b65cf61ccb3bbdc982cd13b9b07`; arm64; cached bases, `--pull=false`, no Computer execution
- Schedule seed/sample: `26e2d05c636a7de781c5de7482bd3fbbe8d92dd991c763addd769a306ac42315`; one focused LayerFS sample
- Measurement/recovery containers: `1e663120fd792a0096f59dc8e6fbbe0aa9efce521c70bd4837fb07a13f3badf5`; `47d8e8b1b967469e068d861620432c5aae9dc5614a62abdd3371d000ff132d45`
- Captured working-tree/index/status hashes: `25998ef71e9076e688d29b00057697de67711c52f0e9d2c2c696c1eee7e249af`; empty; `3c42922d70b861ce21773a774b8205be506c726beef23f864167ea66d19d6405`
- Raw evidence: `runs/page64-r065-20260831/`; 265 verified entries after the explicit decision record; inventory SHA-256 `07dced5e8f2c82f35d096ea9296aadd2f1044c3f2b5b958115138ac593183c2b`
- Exact run command: `benchmark/fs-bench-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd layerfs-fs-benchmark-pro:page64-r065 page64-r065-20260831`

### One mechanism and proof

New Store creation now selects SQLite's supported 64 KiB page size before enabling WAL. Connect leaves existing Store page size unchanged; no migration, VACUUM, or rewrite is hidden in open or acknowledgement. The retained 64 MiB cache and normal spill policy remain, so 256 MiB admission stays bounded. Canonical object bytes/ObjectIds, SQL schema, transaction contents/order, and public APIs are unchanged.

An exact schema oracle requires newly created Stores to report 64 KiB pages. Full Storage and BranchStore suites passed, including all 18 contracts, first/middle/visibility-last rollback, corruption/closure/history paths, and 4/32/256 MiB plus repeated-unpushed-WAL reopen. Formatting, diff check, warning-denying Clippy, image self-check, real host-bind FUSE, fresh Bash/helper process, exact final bytes/digest, clean End, no residual mount/helper, and fresh-container recovery passed.

The application transaction contents stayed exact: 1,746 statements/rows and 33,661,802 receipt bytes per Store, 1,744 missing canonical objects / 33,661,702 payload bytes, one Commit, zero granular object/fact transactions, and visibility-last Branch publication. Branch WAL grew from 35,333,152 to 44,056,352 bytes and authority WAL from 35,279,592 to 43,204,072 bytes; this extra physical space is reported, not hidden.

### Same-sequence visibility equations

The exact public sequence produced Workspace Create `54,304,500 ns`, fresh-process Exec-to-terminal `122,308,833 ns`, Workspace Commit API `395,562,709 ns`, Push API `311,818,209 ns`, and Workspace End `7,390,042 ns`.

- `branchstore_visible_ns = 54,304,500 + 122,308,833 + 395,562,709 = 572,176,042 ns`
- `push_delta_ns = push_api_ns = 311,818,209 ns`
- `layerstackstore_visible_ns = 572,176,042 + 311,818,209 = 883,994,251 ns`
- `complete_lifecycle_ns = 883,994,251 + 7,390,042 = 891,384,293 ns`

At T3-equivalent visibility, BranchStore contained exactly 1,744 objects / 33,661,702 bytes, the Commit and head were readable, and WAL was 44,056,352 bytes. At T4, authority contained 1,756 objects / 33,662,752 bytes including genesis, the same Commit/head were readable, and WAL was 43,204,072 bytes. The BranchStore database file at T3 and authority database file at T4 were each 65,536 bytes; their acknowledged data remained WAL-resident and uncheckpointed. No separate sample/reset occurred between Commit and Push.

### Measured result and decision

Versus Round 064, BranchStore-visible improved `1,110,249,209 -> 572,176,042 ns` (-538.07 ms, -48.46%), Push delta `884,725,334 -> 311,818,209 ns` (-572.91 ms, -64.75%), LayerStackStore-visible `1,994,974,543 -> 883,994,251 ns` (-1,110.98 ms, -55.69%), and complete lifecycle `2,002,738,501 -> 891,384,293 ns` (-1,111.35 ms, -55.49%). This nearly reaches the original 550–850 ms first ladder but remains well above the later V2 local/authority targets; no target claim is fabricated.

The proof matches the hypothesis. Branch WAL frames fell `8,576 -> 672` and authority frames `8,563 -> 659`. Branch final `FULL` sync fell `660,785,917 -> 73,587,583 ns`; authority `650,902,750 -> 76,876,292 ns`. Complete Store transactions fell `691,460,542 -> 102,376,917 ns` and `682,499,750 -> 107,559,666 ns`. Statement work remained approximately 28–30 ms, so no validation/traversal was moved into SQLite.

Commit is now `395,562,709 ns`, dominated by local admission planning `143,094,833`, database publication `102,376,917`, content `81,804,501`, and candidate finalization `62,037,333`. Push is `311,818,209 ns`, with source read/auth `39,123,875`, authority transition proof `59,158,833`, database publication `107,559,666`, and unattributed endpoint/receiver work `103,932,128`.

EDIT16 was `2,800,706,293 ns`, PREPEND `729,486,333 ns`, READ `362,657,792 ns`, and fresh reopen `385,864,916 ns`. READ is inside the restored Round 059/062 distribution; PREPEND remains slower than Round 062 and the binding uncheckpointed profile's EDIT regression remains explicit. The page-size change did not create the EDIT regression (`2,838,455,841 ns` in Round 064), but those guards must be solved at their measured sync component before terminal completion.

Retain 64 KiB pages and reconcile both binding specs to the exact fixed values. Per the ordered optimization contract, the next isolated mechanism is immediate-Commit candidate reuse for Push when BranchId, CommitId, old/new roots, and generation match, with exact BranchStore fallback after restart, eviction, or mismatch. It must reuse the authenticated bounded spill/reachable order without a second payload Vec or sender closure trust.

## Round 066 — candidate-reuse-r066-20260831

- Status: RETAINED CREATE WIN (immediate Commit candidate Push source PASS; exact fallback/receiver trust/recovery PASS)
- UTC timestamp: 2026-08-31T07:32:09Z through 2026-08-31T07:32:55Z; local timestamp 2026-08-31 15:32:09–15:32:55 CST
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `ebd9f72635a9fa0961e85f506f0b4af617d4bef1300ef20f0c294d4a8e040836`
- Image/digest: `layerfs-fs-benchmark-pro:candidate-reuse-r066`; `sha256:08c88b100d237ededcaa715c24376455d43b9f22b6f9cb76d4f6206b78277b24`; arm64; cached bases, `--pull=false`, no Computer execution
- Schedule seed/sample: `dcf7e9664234dbaf932e11621e664fc3bb5e90636c66aabb35dcf0f9a54d7df7`; one focused LayerFS sample
- Measurement/recovery containers: `a7e6484d13dcbc0923b0b5066ff5e9a8040248e7e809073dc11a30fde3f2e6dc`; `95257fdb86cb9778d944bb7c153b323c3a29ac2c427ecf40b68b6991b0046b25`
- Captured working-tree/index/status hashes: `a5e48a1b23df950a2e72c94134e1d798eafb14fb23a9425ef57345f3d51ff4c0`; empty; `135c3e02ed433aed29a80467bcc009a5ad9c567a64aa54d3988463c3cd95beea`
- Raw evidence: `runs/candidate-reuse-r066-20260831/`; 265 verified entries after the explicit decision record; inventory SHA-256 `4fecb21d88b8ce3ea00a99c6c29bf45c1ed1f1726e1028cca6a60e3b93946c05`
- Exact run command: `benchmark/fs-bench-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd layerfs-fs-benchmark-pro:candidate-reuse-r066 candidate-reuse-r066-20260831`

### One mechanism and safety proof

After the one BranchStore transaction commits, the existing authenticated `DeferredObjectStore` and reachable postorder move into the existing process-local PushPlan instead of being dropped. The Store is retained once behind `Arc<Mutex<_>>` because its private SQLite connection is `Send` but not `Sync`; no payload copy or second full Vec is created. Aggregate retained payload is capped at 64 MiB and existing 64-entry/32,768-ID caps remain. Sources above the payload ceiling are demoted to the prior ID-only plan.

The source is selected only when the exact Branch map key, CommitId, base root, new root, unique reachable IDs, and BranchStore SQLite `data_version` match. Generation capture occurs before the visibility transaction so candidate retention cannot introduce a fallible query after Commit publication. Restart has no plan. External same-length corruption or deletion changes `data_version`, forces the durable BranchStore reader, fails authentication/missing-object proof before authority `BEGIN IMMEDIATE`, and never allows correct candidate bytes to mask corrupt local durable state.

Spilled candidates use one ordinal/ID `VALUES` join per <=128-ID missing page and return exact order; they do not perform 1,744 point queries. The immutable candidate was authenticated during construction and again during local prevalidation, so the sender skips only the third same-boundary hash. `TransferPipeline` retains object-size, <=128-object/<4-MiB, buffer, and receipt checks. Authority `stage_received` independently hashes every byte, rejects collisions/duplicates, and the authority independently proves facts, ownership, suffix, transition, missing children, cycles, and closure before its transaction.

Focused proof balances immediate-source IDs/bytes exactly with transfer sent IDs/bytes for the spilled 32 MiB path. Restart and external corruption/deletion prove zero immediate-source reads and safe fallback. Boundary mismatch traverses the full durable transition; oversized source is ID-only; first/middle/visibility-last authority failures roll back all objects/facts/head while retaining the same source for retry. Full 18-test BranchStore contracts passed in parallel after making test-only counters thread-local, along with Storage, LayerStackStore, SDK lifecycle, warning-denying Clippy, formatting, diff check, image/harness self-checks, real host-bind FUSE, exact final bytes/digest, clean End, and fresh reopen.

### Same-sequence visibility equations

The exact public sequence produced Workspace Create `37,924,084 ns`, fresh-process Exec-to-terminal `127,027,375 ns`, Workspace Commit API `388,520,584 ns`, Push API `260,356,917 ns`, and Workspace End `8,460,583 ns`.

- `branchstore_visible_ns = 37,924,084 + 127,027,375 + 388,520,584 = 553,472,043 ns`
- `push_delta_ns = push_api_ns = 260,356,917 ns`
- `layerstackstore_visible_ns = 553,472,043 + 260,356,917 = 813,828,960 ns`
- `complete_lifecycle_ns = 813,828,960 + 8,460,583 = 822,289,543 ns`

T3 BranchStore state remained exact at 1,744 objects / 33,661,702 bytes, readable Commit/head, 65,536-byte DB, and 44,056,352-byte WAL. T4 authority remained 1,756 objects / 33,662,752 bytes including genesis, the same readable Commit/head, 65,536-byte DB, and 43,204,072-byte WAL. There was no reset or separate sample between boundaries, and neither boundary is relabeled as checkpoint/power-loss durability.

### Measured result and next cause

Versus Round 065, BranchStore-visible improved `572,176,042 -> 553,472,043 ns` (-18.70 ms), Push delta `311,818,209 -> 260,356,917 ns` (-51.46 ms), LayerStackStore-visible `883,994,251 -> 813,828,960 ns` (-70.17 ms), and lifecycle `891,384,293 -> 822,289,543 ns` (-69.09 ms). This is the first measured lifecycle inside the original 550–850 ms ladder, but it remains far above the later local/authority targets.

Push source read/auth fell `39,123,875 -> 27,219,041 ns`; the retained spill uses 14 bounded reads and no redundant sender hash. Push unattributed receiver/endpoint work fell `103,932,128 -> 70,382,711 ns`; transition proof was `56,268,124 ns`; authority transaction `104,576,625 ns`. Commit was `388,520,584 ns`, still dominated by local admission prevalidation `139,511,916`, Store transaction `105,086,000`, content `77,272,541`, and candidate finalization `62,169,250`.

EDIT16 improved slightly `2,800,706,293 -> 2,766,320,499 ns`; PREPEND was `732,761,083 ns`; READ improved `362,657,792 -> 349,553,791 ns`; fresh reopen was `370,953,625 ns`. The binding profile's EDIT regression remains unresolved but did not worsen in this round.

Retain candidate reuse. The next isolated mechanism is receiver-derived incremental authority closure proof: preserve immutable receiver spooling and receiver authentication, but retain decoded canonical references/roles while receiving so the authority does not immediately reread and rehash the complete 32 MiB closure. Sender postorder remains only a hint; missing children, cycles, conflicting bytes, wrong identity/role/fact order, interruption, and incomplete roots must still fail before the authority transaction.

## Round 067 — receiver-proof-r067-20260831

- Status: RETAINED AUTHORITY WIN (receiver-derived spillable reference proof PASS; independent trust/rollback/recovery PASS)
- UTC timestamp: 2026-08-31T07:53:40Z through 2026-08-31T07:54:26Z; local timestamp 2026-08-31 15:53:40–15:54:26 CST
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `ef2d0734bff493600c84d4cfbc3c1ec138f2533c24a9547786b6ea5bfbaa4112`
- Image/digest: `layerfs-fs-benchmark-pro:receiver-proof-r067`; `sha256:08026871dffee50f76a82d7a21b13b6f31cadc01d1bbd7ffbb0203b0eebf3346`; arm64; cached bases, `--pull=false`, no Computer execution
- Schedule seed/sample: `bfa7b9f4d21a4cfde1ed1fb8ede8765386ee5145ff679fab759f324e78a9845a`; one focused LayerFS sample
- Measurement/recovery containers: `89c2f491a23cfb15ac34133aa8194fb2c748951d50764be9532d51a2a7ad6ace`; `db8dc41686643f9bc91c3055f16e502e4d064063965e4bf733f61d924dbf5f6e`
- Captured working-tree/index/status hashes: `4ba6b85a74a1dc2bbd8c056549d30617d89453d5ef4bda7ea7224547a74c59ba`; empty; `2e2822a7dedd0034c3605436135689adbff39e873b9e786cfa414e779236327b`
- Raw evidence: `runs/receiver-proof-r067-20260831/`; 265 verified entries after the explicit decision record; inventory SHA-256 `390435ec5a1e753c44a6f02124cca3406e0c7ee2f5ac537704627682b0cc6ae4`
- Exact run command: `benchmark/fs-bench-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd layerfs-fs-benchmark-pro:receiver-proof-r067 receiver-proof-r067-20260831`

### One mechanism and receiver trust proof

After `stage_received` independently authenticates each canonical ObjectId, authority parses its canonical child references once and records them in a receiver-owned `ReceivedReferenceIndex`. The index charges 64 bytes per node plus 32 bytes per child, remains in memory through 8 MiB, and spills to private SQLite `received_nodes`/`received_edges` tables with explicit ordinal order. A no-large-allocation oracle forces the spill boundary and proves leaf presence, ordered children, and missing lookup.

Authority transition proof now traverses these immutable receiver-derived references for received nodes and authenticated authority Store bytes for pre-existing nodes. It preserves the prior exact transition algorithm, Seen spill, active-cycle detection, old/new subtree pruning, missing-child failure, Commit order/base ownership, suffix identity, and final root/base checks. Sender postorder remains a transfer hint only and never enters the index. Duplicate/conflicting object bytes fail during receiver staging; wrong ObjectId and malformed canonical role fail before indexing; interruption drops both payload and reference scratch before any authority transaction.

The common transition verifier was refactored to accept authenticated reference lookup without changing the existing Store-backed behavior; its complete-transition/missing-frontier oracle passed. Test-only receiver/database reference counters prove the valid Push uses receiver-derived references. Incomplete authority closure repair, external corruption/deletion fallback, first/middle/visibility-last rollback and retry, full 18-test BranchStore contracts, Storage, LayerStackStore, SDK, formatting, diff, warning-denying Clippy, image/harness self-checks, exact real-FUSE oracle, clean End, and fresh recovery all passed.

### Same-sequence visibility equations

The exact public sequence produced Workspace Create `38,536,292 ns`, fresh-process Exec-to-terminal `102,679,375 ns`, Workspace Commit API `423,583,584 ns`, Push API `234,232,875 ns`, and Workspace End `8,705,042 ns`.

- `branchstore_visible_ns = 38,536,292 + 102,679,375 + 423,583,584 = 564,799,251 ns`
- `push_delta_ns = push_api_ns = 234,232,875 ns`
- `layerstackstore_visible_ns = 564,799,251 + 234,232,875 = 799,032,126 ns`
- `complete_lifecycle_ns = 799,032,126 + 8,705,042 = 807,737,168 ns`

T3 BranchStore state remained 1,744 objects / 33,661,702 bytes, exact readable Commit/head, 65,536-byte DB, and 44,056,352-byte WAL. T4 authority remained 1,756 objects / 33,662,752 bytes including genesis, the same readable Commit/head, 65,536-byte DB, and 43,204,072-byte WAL. Both are transaction-visible uncheckpointed diagnostics; no state reset or separately prepared sample occurred.

### Measured result and decision

The targeted authority proof collapsed `56,268,124 -> 866,292 ns` (-55.40 ms, -98.46%) while authority still performed receiver authentication and independent closure validation. Push delta improved `260,356,917 -> 234,232,875 ns` (-26.12 ms), LayerStackStore-visible `813,828,960 -> 799,032,126 ns` (-14.80 ms), and lifecycle `822,289,543 -> 807,737,168 ns` (-14.55 ms). BranchStore-visible moved `553,472,043 -> 564,799,251 ns` under host noise because content, admission, and local sync were slower in this sample; no local-path improvement is claimed.

Push retained-source read was `30,588,709 ns`, authority transaction `118,542,042 ns`, and unattributed receive/endpoint work `82,005,291 ns`. Commit was `423,583,584 ns`: content `85,091,625`, candidate finalization `65,567,750`, local admission prevalidation `150,084,709`, and Store publication `118,169,833`. WAL frames/bytes, 1,746 rows/statements, transfer batches, and exact missing-only equations did not change.

EDIT16 was `2,871,598,709 ns`, PREPEND improved `732,761,083 -> 678,239,750 ns`, READ was `368,341,625 ns`, and fresh reopen `406,288,250 ns`. EDIT remains an unresolved binding-profile guard; READ is near the retained distribution and no read-path mechanism changed.

Retain receiver-derived proof. The next measured local mechanism is removal of redundant candidate authentication at the same trust boundary: candidate finalization and the complete admission traversal already authenticate immutable candidate bytes, so the subsequent sequential membership/selection pass must not hash those same bytes a third time. Preserve parent/BranchStore authentication, collision checks, exact closure, and pretransaction failure behavior.

## Round 068 — local-auth-r068-20260831

- Status: RETAINED PHASE WIN / LIFECYCLE NOISE-REGRESSED (third candidate hash removed; integrity/recovery PASS; no total win claim)
- UTC timestamp: 2026-08-31T08:00:41Z through 2026-08-31T08:01:27Z; local timestamp 2026-08-31 16:00:41–16:01:27 CST
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `32d1ebf90b0fc7bf52dcd63dccb8d0a2a6714f4dcaab2c1fd17ca3617b36a194`
- Image/digest: `layerfs-fs-benchmark-pro:local-auth-r068`; `sha256:79dda9ed31fa8005906e1e44376726623a84c98807f18772ccc7886d0bd8ba84`; arm64; cached bases, `--pull=false`, no Computer execution
- Schedule seed/sample: `a907b6be92fc4f2c2239c38634b76270156a3b041cf70ba3a6763749f8364b43`; one focused LayerFS sample
- Measurement/recovery containers: `ae083bb3aaf4f7531874b57ed76d18b9e351ec9d1435bf1dc1ec9c190a94df7e`; `5ab13af29934fdb2ffc1dc1e4315a70c7b3c5dd9d909bc21dfe8599f81331874`
- Captured working-tree/index/status hashes: `c86fc02da36edc7f42c514efcb7343aae2fdec197201078cc73cd55c9eb92de5`; empty; `2e2822a7dedd0034c3605436135689adbff39e873b9e786cfa414e779236327b`
- Raw evidence: `runs/local-auth-r068-20260831/`; 265 verified entries after the explicit decision record; inventory SHA-256 `9b84804eb91276a7c7c5b5917441d700138f78a23bed57766bb4dbc0e7eadd84`
- Exact run command: `benchmark/fs-bench-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd layerfs-fs-benchmark-pro:local-auth-r068 local-auth-r068-20260831`

### One mechanism and proof

`plan_commit_admission` retains its complete dependency traversal and authentication of the immutable candidate plus authenticated BranchStore/parent fallback. The following sequential `visit_batches` pass still checks duplicate IDs, exact local membership and bytes, local collision equality, exact parent membership/lengths, and missing insert selection, but no longer hashes every candidate payload a third time. There is no skipped receiver or Store trust boundary and no mutable access to the candidate between the authenticated traversal and membership pass.

External same-length corruption and deletion still force generation mismatch/fallback and fail before authority SQLite. Candidate identity/order, local collision, rollback first/middle/final, 4/32/256 MiB reopen, full 18 BranchStore contracts, Storage/LayerStackStore/SDK, formatting, diff, warning-denying Clippy, real FUSE, exact transfer/final bytes, WAL state, and fresh recovery passed.

### Same-sequence visibility equations

The exact public sequence produced Workspace Create `54,305,292 ns`, fresh-process Exec-to-terminal `111,798,041 ns`, Workspace Commit API `378,890,917 ns`, Push API `261,894,584 ns`, and Workspace End `8,052,584 ns`.

- `branchstore_visible_ns = 54,305,292 + 111,798,041 + 378,890,917 = 544,994,250 ns`
- `push_delta_ns = push_api_ns = 261,894,584 ns`
- `layerstackstore_visible_ns = 544,994,250 + 261,894,584 = 806,888,834 ns`
- `complete_lifecycle_ns = 806,888,834 + 8,052,584 = 814,941,418 ns`

T3/T4 states stayed exact: BranchStore 1,744 / 33,661,702 with readable Commit/head and 44,056,352-byte WAL; authority 1,756 / 33,662,752 including genesis with the same readable Commit/head and 43,204,072-byte WAL. Both DB files were 65,536 bytes and both boundaries remain uncheckpointed transaction-visible diagnostics.

### Measured result and decision

The intended local phase improved directly: local admission `150,084,709 -> 113,657,542 ns` (-36.43 ms) and Commit `423,583,584 -> 378,890,917 ns` (-44.69 ms). BranchStore-visible improved `564,799,251 -> 544,994,250 ns` despite slower Create/Exec setup in this sample.

Authority transaction sync independently regressed: authority publication `118,542,042 -> 147,582,791 ns`, including `FULL` commit sync `85,113,250 -> 112,304,375 ns`. Consequently Push delta moved `234,232,875 -> 261,894,584 ns`, authority-visible `799,032,126 -> 806,888,834 ns`, and lifecycle `807,737,168 -> 814,941,418 ns` (+7.20 ms). The trust-boundary simplification is retained on its isolated 36.4 ms phase proof, but no lifecycle improvement is claimed.

EDIT16 was `2,824,205,209 ns`, PREPEND improved to `654,884,542 ns`, READ `354,436,001 ns`, and fresh reopen `399,677,250 ns`. No regression mechanism was added to these paths.

Next remove the remaining duplicate candidate hash inside the complete admission traversal itself: `CandidateClosureSource` currently authenticates each returned candidate/Store/parent object and `collect_dependency_set` immediately authenticates the identical bytes again. Keep authentication in the generic traversal, remove only the private source wrapper's duplicate, and retain exact missing-child/collision behavior.

## Round 069 — closure-auth-r069-20260831

- Status: RETAINED CREATE WIN (duplicate closure-source hash removed; generic authentication/integrity/recovery PASS)
- UTC timestamp: 2026-08-31T08:06:49Z through 2026-08-31T08:07:35Z; local timestamp 2026-08-31 16:06:49–16:07:35 CST
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `8392060f61d61e14b9eb7901109eae239cf0b9df715296c48afb8dc6d1682e8d`
- Image/digest: `layerfs-fs-benchmark-pro:closure-auth-r069`; `sha256:cd3d7e1cd41d3d69180971e69cd5c8d32e314113b2e570108950a0d8ad9f5dd4`; arm64; cached bases, `--pull=false`, no Computer execution
- Schedule seed/sample: `016233e4314e95b1b6e0d2eba00bdd46411d2ef815f3a19b960b9de1a067a635`; one focused LayerFS sample
- Measurement/recovery containers: `c096af5d4a991d9057b00589471012416f70423772126c24400b5ba42d76a860`; `abdd777018bec179e77231add869aef6d8610982799fa660b901d3faf77baa0c`
- Captured working-tree/index/status hashes: `e09ab2ff967ec5553ffd6fffec90a23dbfaf3ce0700fe72b4f7b9f071125864f`; empty; `2e2822a7dedd0034c3605436135689adbff39e873b9e786cfa414e779236327b`
- Raw evidence: `runs/closure-auth-r069-20260831/`; 265 verified entries after the explicit decision record; inventory SHA-256 `68b6bb9950aa610640b7748362a61c5c83fdd41ed408529eeeecbdff4d418e8d`
- Exact run command: `benchmark/fs-bench-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd layerfs-fs-benchmark-pro:closure-auth-r069 closure-auth-r069-20260831`

### One mechanism and proof

The private `CandidateClosureSource::read_object` still selects exact bytes candidate-first, then BranchStore, then parent. It no longer hashes those bytes because its sole caller, generic `collect_dependency_set`, immediately authenticates the identical `(ObjectId, bytes)` before decoding child references. This removes one same-boundary hash without changing any source order, fallback, graph traversal, cycle/missing-child failure, or trust boundary.

The following membership pass remains protected by candidate finalization plus this authenticated closure traversal. Parent/BranchStore bytes are authenticated in the generic traversal; local collision reads and byte equality remain. Focused candidate, same-length corruption, deletion, missing-child, first/middle/final rollback, 4/32/256 MiB recovery, full BranchStore contracts, formatting, diff, warning-denying Clippy, real FUSE, exact final oracle, WAL state, and fresh recovery passed.

### Same-sequence visibility equations

The exact public sequence produced Workspace Create `36,276,708 ns`, fresh-process Exec-to-terminal `110,952,542 ns`, Workspace Commit API `341,432,542 ns`, Push API `222,736,250 ns`, and Workspace End `7,886,667 ns`.

- `branchstore_visible_ns = 36,276,708 + 110,952,542 + 341,432,542 = 488,661,792 ns`
- `push_delta_ns = push_api_ns = 222,736,250 ns`
- `layerstackstore_visible_ns = 488,661,792 + 222,736,250 = 711,398,042 ns`
- `complete_lifecycle_ns = 711,398,042 + 7,886,667 = 719,284,709 ns`

T3 BranchStore remained exact at 1,744 objects / 33,661,702 bytes, readable Commit/head, 65,536-byte DB, and 44,056,352-byte WAL. T4 authority remained 1,756 objects / 33,662,752 bytes including genesis, same readable Commit/head, 65,536-byte DB, and 43,204,072-byte WAL. No checkpoint, reset, or separate sample occurred.

### Measured result and decision

Local admission improved `113,657,542 -> 71,208,416 ns` (-42.45 ms), Commit `378,890,917 -> 341,432,542 ns` (-37.46 ms), and BranchStore-visible `544,994,250 -> 488,661,792 ns` (-56.33 ms). Authority `FULL` sync returned to its prior range, so Push delta also improved `261,894,584 -> 222,736,250 ns`; authority-visible fell `806,888,834 -> 711,398,042 ns` and lifecycle `814,941,418 -> 719,284,709 ns` (-95.66 ms).

Current Commit phases are content `84,050,417 ns`, candidate finalization `64,231,584`, local admission `71,208,416`, and Store transaction `117,743,584`. Push phases are retained-source read `31,172,166`, receiver proof `803,125`, authority transaction `110,452,500`, and unattributed receiver/endpoint work `78,296,584`.

EDIT16 improved `2,824,205,209 -> 2,371,740,623 ns`; PREPEND `619,926,375 ns`; READ `358,225,833 ns`; fresh reopen `400,407,166 ns`. EDIT remains slower than the Round 062 guard but is improving at the measured Store-sync component.

Retain the duplicate-hash removal. The next local cause is the remaining complete candidate traversal: candidate finalization already authenticates and parses every reachable candidate object to create the exact postorder, then local admission rereads that full spill to reconstruct the same graph. Add an optional <=8 MiB bounded reference index during candidate finalization; use it to prove candidate closure plus authenticated BranchStore/parent frontier, and fall back to the existing full traversal when the index ceiling is exceeded. Never trust caller-supplied closure or skip external-frontier authentication.

## Round 070 — candidate-refs-r070-20260831

- Status: RETAINED CREATE WIN (bounded candidate reference proof PASS; exact full-traversal fallback/integrity/recovery PASS)
- UTC timestamp: 2026-08-31T08:18:50Z through 2026-08-31T08:19:36Z; local timestamp 2026-08-31 16:18:50–16:19:36 CST
- Git commit/tree/source seal: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`; `8768d3a1a71c86753667ef3e73726ae924589099`; `932f7cdceac8af0443f8fdde0f9d13e87834e7dbf399029e2c5a2095c8914f56`
- Image/digest: `layerfs-fs-benchmark-pro:candidate-refs-r070`; `sha256:1db4535671ea4cba96205b96423a609fedca51b081145084ee508aa756319749`; arm64; cached bases, `--pull=false`, no Computer execution
- Schedule seed/sample: `001bb41879be7010d47dffd9ef96af08bfc38c68e32a9aa7d90003366888975f`; one focused LayerFS sample
- Measurement/recovery containers: `dab29f39ee97f5e6fa42c17dad9617d1680c7645a0761ac0e7212667aed19918`; `7158651d50ad0767c601ade718b56c49171a709a138ebca292d2b4834fb8c7a1`
- Captured working-tree/index/status hashes: `fd452bcdc0e3f0cfa3f5d4bb6848a97f8f57a9ba593073b7928ccc6a5bd5ae82`; empty; `2e2822a7dedd0034c3605436135689adbff39e873b9e786cfa414e779236327b`
- Raw evidence: `runs/candidate-refs-r070-20260831/`; 265 verified entries after the explicit decision record; inventory SHA-256 `9c5a4b23e0b38bdbd25c42d23fb8e9e8a84d6d7172b5bb3347cbfc38da75ad36`
- Exact run command: `benchmark/fs-bench-pro/run.sh focused sha256:6807f06d5332818c0b4bbc026ea0b1ed439051cbc924dd48a0679ec5a10a08dd layerfs-fs-benchmark-pro:candidate-refs-r070 candidate-refs-r070-20260831`

### One mechanism and proof

During existing candidate finalization, after authenticating and decoding each reachable candidate object, `DeferredObjectStore` records its sorted unique child IDs in an optional reference map. The map charges 64 bytes per object plus 32 bytes per child and disables itself above 8 MiB. It never contains sender/caller claims. A forced-ceiling oracle proves disabling the index selects the existing full authenticated traversal; every retained candidate ID has a cached entry in the normal spilled-candidate oracle.

Local admission uses generic Seen/active closure verification over cached candidate references. An ID absent from the candidate index is an external frontier: its bytes are still read candidate-first/local/parent according to the exact prior source policy, authenticated, decoded, and recursively proved. Missing children, cycles, local collision equality, parent membership, duplicate IDs, exact insert selection, and transaction-free proof remain unchanged. Receiver/authority proof remains independently derived.

Full Storage and BranchStore suites passed, including forced index fallback, same-length corruption/deletion, incomplete authority repair, first/middle/final rollback, restart/eviction fallback, 4/32/256 MiB reopen, formatting, diff, warning-denying Clippy, harness/image self-checks, real FUSE, exact final bytes/digest, WAL state, clean End, and fresh recovery.

### Same-sequence visibility equations

The exact public sequence produced Workspace Create `41,923,208 ns`, fresh-process Exec-to-terminal `96,788,750 ns`, Workspace Commit API `268,434,125 ns`, Push API `244,010,667 ns`, and Workspace End `8,421,458 ns`.

- `branchstore_visible_ns = 41,923,208 + 96,788,750 + 268,434,125 = 407,146,083 ns`
- `push_delta_ns = push_api_ns = 244,010,667 ns`
- `layerstackstore_visible_ns = 407,146,083 + 244,010,667 = 651,156,750 ns`
- `complete_lifecycle_ns = 651,156,750 + 8,421,458 = 659,578,208 ns`

T3 BranchStore remained 1,744 objects / 33,661,702 bytes, exact readable Commit/head, 65,536-byte DB, and 44,056,352-byte WAL. T4 authority remained 1,756 objects / 33,662,752 bytes including genesis, same Commit/head, 65,536-byte DB, and 43,204,072-byte WAL. The sample used one uninterrupted public sequence and neither boundary claims checkpoint/power-loss durability.

### Measured result and decision

Local admission improved `71,208,416 -> 18,644,875 ns` (-52.56 ms), Commit `341,432,542 -> 268,434,125 ns` (-73.00 ms), and BranchStore-visible `488,661,792 -> 407,146,083 ns` (-81.52 ms). Authority transaction noise made Push slower `222,736,250 -> 244,010,667 ns`, but authority-visible still improved `711,398,042 -> 651,156,750 ns` and lifecycle `719,284,709 -> 659,578,208 ns` (-59.71 ms).

Current Commit is content `79,257,333 ns`, candidate finalization `59,935,416`, local admission `18,644,875`, and Store transaction `105,294,375`. Push is retained-source read `32,499,376`, receiver proof `804,001`, authority transaction `128,516,416`, and unattributed receiver/endpoint work `80,157,292`. Object/fact rows, transfer pages/batches, and WAL frames/bytes remain exact.

EDIT16 improved `2,371,740,623 -> 2,239,906,249 ns`; PREPEND `550,756,668 ns`; READ `367,557,083 ns`; fresh reopen `392,457,125 ns`. PREPEND is now inside the historical guard band; EDIT remains the explicit remaining regression.

Retain bounded candidate references. The next measured ownership cost is the borrowed transfer batch: the retained source moves each bounded payload Vec into `TransferPipeline`, but `TransferTarget::admit_objects(&[CanonicalObject])` forces authority receiver spooling to clone every byte. Add one internal owned-batch method with a safe borrowed default and an authority override that authenticates then moves each Vec into its immutable receiver spool. Preserve exact buffer/page receipts, receiver hashing, retries, and all non-authority targets.

## v4-public-sdk-smoke3-01a0572b — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v4-public-sdk-smoke3-01a0572b/raw/layerfs.jsonl`
- Lifecycle samples: 3
- Workspace Create median: 72649709 ns
- Small-edit Commit median: 8449125 ns
- Small-edit complete median: 139734583 ns
- Cold-create-32m Commit median: 2180293250 ns
- Cold-create-32m complete median: 2393691709 ns
- EDIT16 median: 1912982250 ns
- Inner 32 MiB write throughput: 392336499.575 bytes/s

- Source seal: `ec3eca11d09ba075f53c36abf1e173e63a9ca7dcbc5f910520b297430e8818d6`
- Exit status: `1`

## v4-public-sdk-diagnostic-01a0572b — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v4-public-sdk-diagnostic-01a0572b/raw/layerfs.jsonl`
- Lifecycle samples: 3
- Workspace Create median: 77204583 ns
- Small-edit Commit median: 9128834 ns
- Small-edit complete median: 126429667 ns
- Cold-create-32m Commit median: 2323280917 ns
- Cold-create-32m complete median: 2548823583 ns
- EDIT16 median: 1877717125 ns
- Inner 32 MiB write throughput: 356580012.591 bytes/s

- Source seal: `43ec17d28b7d572226e38e330199c22bcade85058fe9312165d66abbad7710fe`
- Exit status: `1`

## v4-public-sdk-spill-fix-01a0572b — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v4-public-sdk-spill-fix-01a0572b/raw/layerfs.jsonl`
- Lifecycle samples: 3
- Workspace Create median: 62577584 ns
- Small-edit Commit median: 8669209 ns
- Small-edit complete median: 117567584 ns
- Cold-create-32m Commit median: 295186083 ns
- Cold-create-32m complete median: 484675708 ns
- EDIT16 median: 1904332208 ns
- Inner 32 MiB write throughput: 395250533.594 bytes/s

- Source seal: `5265a23710dbfd38e831a3fc7122e6e39b79f3d86d20d7be278e87fa58902b30`
- Exit status: `1`

## v4-public-sdk-id-membership-01a0572b — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v4-public-sdk-id-membership-01a0572b/raw/layerfs.jsonl`
- Lifecycle samples: 3
- Workspace Create median: 72607625 ns
- Small-edit Commit median: 9207958 ns
- Small-edit complete median: 126820667 ns
- Cold-create-32m Commit median: 297460208 ns
- Cold-create-32m complete median: 512332750 ns
- EDIT16 median: 2034663667 ns
- Inner 32 MiB write throughput: 364902721.736 bytes/s

- Source seal: `8ff534f61f14d7efd4d16644ba29310efbe9cbde5876329fb468822f6179444d`
- Exit status: `1`

## v4-public-sdk-sorted-insert-01a0572b — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v4-public-sdk-sorted-insert-01a0572b/raw/layerfs.jsonl`
- Lifecycle samples: 3
- Workspace Create median: 76435792 ns
- Small-edit Commit median: 9333584 ns
- Small-edit complete median: 103851875 ns
- Cold-create-32m Commit median: 299306584 ns
- Cold-create-32m complete median: 520981833 ns
- EDIT16 median: 1985783375 ns
- Inner 32 MiB write throughput: 376425545.878 bytes/s

- Source seal: `c9927db9595845834d5b4a7fe39b56599e4dc71508aefae1a57f1c8121461311`
- Exit status: `1`

## v4-public-sdk-edit16-fix-01a0572b — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v4-public-sdk-edit16-fix-01a0572b/raw/layerfs.jsonl`
- Lifecycle samples: 3
- Workspace Create median: 68267750 ns
- Small-edit Commit median: 9397708 ns
- Small-edit complete median: 135254750 ns
- Cold-create-32m Commit median: 292984667 ns
- Cold-create-32m complete median: 492265709 ns
- EDIT16 median: 866152666 ns
- Inner 32 MiB write throughput: 373069687.473 bytes/s

- Source seal: `481e8284e03618c4370163c2b1c972bf99ffed3fc1b14750e13c512160a548ac`
- Exit status: `1`

## v4-public-sdk-host-bind-01a0572b — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v4-public-sdk-host-bind-01a0572b/raw/layerfs.jsonl`
- Lifecycle samples: 3
- Workspace Create median: 75042250 ns
- Small-edit Commit median: 8654000 ns
- Small-edit complete median: 135411000 ns
- Cold-create-32m Commit median: 294075292 ns
- Cold-create-32m complete median: 568696333 ns
- EDIT16 median: 933312625 ns
- Inner 32 MiB write throughput: 231099219.541 bytes/s

- Source seal: `bfbf32fcae3f7f335439347e116a57a9cddc77ead733eb17fa454bfad64bad9c`
- Exit status: `1`

## v4-public-sdk-container-local-01a0572b — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v4-public-sdk-container-local-01a0572b/raw/layerfs.jsonl`
- Lifecycle samples: 3
- Workspace Create median: 61659291 ns
- Small-edit Commit median: 9095417 ns
- Small-edit complete median: 124570875 ns
- Cold-create-32m Commit median: 279844333 ns
- Cold-create-32m complete median: 491093625 ns
- EDIT16 median: 875318333 ns
- Inner 32 MiB write throughput: 364510777.681 bytes/s

- Source seal: `bfbf32fcae3f7f335439347e116a57a9cddc77ead733eb17fa454bfad64bad9c`
- Exit status: `1`

## v4-public-sdk-hardlink-01a0572b — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v4-public-sdk-hardlink-01a0572b/raw/layerfs.jsonl`
- Lifecycle samples: 3
- Workspace Create median: 60010458 ns
- Small-edit Commit median: 10606208 ns
- Small-edit complete median: 122129667 ns
- Cold-create-32m Commit median: 315353917 ns
- Cold-create-32m complete median: 507099208 ns
- EDIT16 median: 810230125 ns
- Inner 32 MiB write throughput: 359940967.609 bytes/s

- Source seal: `979d8ce577bb75a9cf2212ac528c649da54c81bc4423374d80b6f2b76c351fc5`
- Exit status: `1`

## v4-public-sdk-reference-index-01a0572b — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v4-public-sdk-reference-index-01a0572b/raw/layerfs.jsonl`
- Lifecycle samples: 3
- Workspace Create median: 52589541 ns
- Small-edit Commit median: 9195625 ns
- Small-edit complete median: 117586208 ns
- Cold-create-32m Commit median: 245332583 ns
- Cold-create-32m complete median: 448483916 ns
- EDIT16 median: 847552917 ns
- Inner 32 MiB write throughput: 338913400.499 bytes/s

- Source seal: `04d5b049fece93dbd422cc73382b00e56b150b9762f5cf4bfe524a8493e84253`
- Exit status: `1`

## v4-public-sdk-cache128-01a0572b — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v4-public-sdk-cache128-01a0572b/raw/layerfs.jsonl`
- Lifecycle samples: 3
- Workspace Create median: 71305958 ns
- Small-edit Commit median: 9145583 ns
- Small-edit complete median: 128264125 ns
- Cold-create-32m Commit median: 240873417 ns
- Cold-create-32m complete median: 443536333 ns
- EDIT16 median: 900118542 ns
- Inner 32 MiB write throughput: 363199707.747 bytes/s

- Source seal: `92cd2c55b986b1fcb36c1d2bccf056e30b3ebeb8f557654f2319e1a6f9ccce65`
- Exit status: `1`

## v4-rowid-objects-r001-01a0572b — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v4-rowid-objects-r001-01a0572b/raw/layerfs.jsonl`
- Lifecycle samples: 3
- Workspace Create median: 57222792 ns
- Small-edit Commit median: 4475375 ns
- Small-edit complete median: 102836917 ns
- Cold-create-32m Commit median: 169593292 ns
- Cold-create-32m complete median: 358071875 ns
- EDIT16 median: 807759875 ns
- Inner 32 MiB write throughput: 373916048.559 bytes/s

- Source seal: `45c80e23180f2c3eceb24a46a0e401bf1c0c3cfa6c8a25f9ada69f9ed504dcb8`
- Exit status: `1`

## v4-sealed-spool-auth-baseline-r002-01a0572b — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v4-sealed-spool-auth-baseline-r002-01a0572b/raw/layerfs.jsonl`
- Lifecycle samples: 3
- Workspace Create median: 73039166 ns
- Small-edit Commit median: 4502750 ns
- Small-edit complete median: 126732291 ns
- Cold-create-32m Commit median: 171385833 ns
- Cold-create-32m complete median: 425156083 ns
- EDIT16 median: 861810375 ns
- Inner 32 MiB write throughput: 232814321.028 bytes/s

- Source seal: `e9ded1313a5eb66e4715a7eab69878ae1be36f8f2c7f8255da8e457b1792d40a`
- Exit status: `1`

## v4-sealed-spool-no-rehash-r003-01a0572b — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v4-sealed-spool-no-rehash-r003-01a0572b/raw/layerfs.jsonl`
- Lifecycle samples: 3
- Workspace Create median: 77903042 ns
- Small-edit Commit median: 4245083 ns
- Small-edit complete median: 125925250 ns
- Cold-create-32m Commit median: 127027083 ns
- Cold-create-32m complete median: 391108250 ns
- EDIT16 median: 810612459 ns
- Inner 32 MiB write throughput: 236463813.691 bytes/s

- Source seal: `d05e1d7de1480a5949740ed72a413d00e9e95bb65bfd2dbbd2531ed8148c5dff`
- Exit status: `1`

## v4-native-host-r001-20260901 — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v4-native-host-r001-20260901/raw/layerfs.jsonl`
- Lifecycle samples: 15
- Workspace Create median: 11270792 ns
- Small-edit Commit median: 5229625 ns
- Small-edit complete median: 34612833 ns
- Cold-create-32m Commit median: 39676750 ns
- Cold-create-32m complete median: 141347250 ns
- EDIT16 median: 274588583 ns
- Prepend median: 231962458 ns
- Read + digest median: 211528333 ns
- Registered four-row total: 859426624 ns
- Inner 32 MiB write throughput: 382010843.030 bytes/s

- Source seal: `d9ea7e147452ace87543760e47785f5f9e4d4c39684ce0594268262895d76641`
- Exit status: `1`

## v4-native-host-r002-20260901 — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v4-native-host-r002-20260901/raw/layerfs.jsonl`
- Lifecycle samples: 15
- Workspace Create median: 13006208 ns
- Small-edit Commit median: 5715542 ns
- Small-edit complete median: 27182666 ns
- Cold-create-32m Commit median: 46354916 ns
- Cold-create-32m complete median: 147924541 ns
- EDIT16 median: 169432458 ns
- Prepend median: 237367459 ns
- Read + digest median: 218273167 ns
- Registered four-row total: 772997625 ns
- Inner 32 MiB write throughput: 423029231.292 bytes/s

- Source seal: `2ca9a9a7bb9cec3748fbf8e86a849f55526405f0fb6ac180bb2e2de62277359a`
- Exit status: `1`

## v4-native-host-r003-20260901 — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v4-native-host-r003-20260901/raw/layerfs.jsonl`
- Lifecycle samples: 15
- Workspace Create median: 12644542 ns
- Small-edit Commit median: 5939458 ns
- Small-edit complete median: 27981709 ns
- Cold-create-32m Commit median: 42783250 ns
- Cold-create-32m complete median: 143720000 ns
- EDIT16 median: 157196917 ns
- Prepend median: 233092292 ns
- Read + digest median: 262143083 ns
- Registered four-row total: 796152292 ns
- Inner 32 MiB write throughput: 408380725.306 bytes/s

- Source seal: `bb747302e6762376264543da98b2ac94575d5a92f67aa8f0e8ab488ddfaa24ef`
- Exit status: `1`

## v4-native-host-r004-20260901 — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v4-native-host-r004-20260901/raw/layerfs.jsonl`
- Lifecycle samples: 15
- Workspace Create median: 12504791 ns
- Small-edit Commit median: 5166000 ns
- Small-edit complete median: 26497250 ns
- Cold-create-32m Commit median: 40579292 ns
- Cold-create-32m complete median: 141690167 ns
- EDIT16 median: 160744542 ns
- Prepend median: 218003875 ns
- Read 32 MiB median: 122646917 ns
- Registered four-row total: 643085501 ns
- Inner 32 MiB write throughput: 388565167.882 bytes/s

- Source seal: `30166c9ea618c27f1cdcb6851ade0999ac18bf48e7336dc98451ffecc24e88fb`
- Exit status: `1`

## v4-native-host-r005-20260901 — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v4-native-host-r005-20260901/raw/layerfs.jsonl`
- Lifecycle samples: 15
- Workspace Create median: 13064916 ns
- Small-edit Commit median: 5282458 ns
- Small-edit complete median: 27700500 ns
- Cold-create-32m Commit median: 41779959 ns
- Cold-create-32m complete median: 136536125 ns
- EDIT16 median: 168021875 ns
- Prepend median: 218543958 ns
- Read 32 MiB median: 125925417 ns
- Registered four-row total: 649027375 ns
- Inner 32 MiB write throughput: 414909396.808 bytes/s

- Host memory: `host_peak_rss_bytes=92241920 host_swaps=0 `

- Source seal: `7fc79e3fdd9ff317d6b51786c5f8874566b0e61cc6d6045c0242181fbf8129cb`
- Exit status: `1`

## v4-native-host-r006-20260901 — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v4-native-host-r006-20260901/raw/layerfs.jsonl`
- Lifecycle samples: 15
- Workspace Create median: 10720083 ns
- Small-edit Commit median: 4764125 ns
- Small-edit complete median: 24063125 ns
- Cold-create-32m Commit median: 44502208 ns
- Cold-create-32m complete median: 151748083 ns
- EDIT16 median: 147575834 ns
- Prepend median: 225789209 ns
- Read 32 MiB median: 113307458 ns
- Registered four-row total: 638420584 ns
- Inner 32 MiB write throughput: 418920714.876 bytes/s

- Host memory: `host_peak_rss_bytes=99778560 host_swaps=0 `

- Source seal: `6b338c25ca6e9517c67a5b1d382371d23c83f05dcc322e66e65a1c9a7a5ced09`
- Exit status: `1`

## v4-native-host-r006-repeat-20260901 — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v4-native-host-r006-repeat-20260901/raw/layerfs.jsonl`
- Result: invalid or incomplete evidence; see `raw/layerfs.stderr`.

- Host memory: `host_peak_rss_bytes=104300544 host_swaps=0 `

- Source seal: `6b338c25ca6e9517c67a5b1d382371d23c83f05dcc322e66e65a1c9a7a5ced09`
- Exit status: `1`

## v4-native-host-r006-repeat2-20260901 — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v4-native-host-r006-repeat2-20260901/raw/layerfs.jsonl`
- Lifecycle samples: 35
- Workspace Create median: 11506667 ns
- Small-edit Commit median: 5189750 ns
- Small-edit complete median: 26090833 ns
- Cold-create-32m Commit median: 44369167 ns
- Cold-create-32m complete median: 135098708 ns
- EDIT16 median: 158577209 ns
- Prepend median: 229131292 ns
- Read 32 MiB median: 113571208 ns
- Registered four-row total: 636378417 ns
- Inner 32 MiB write throughput: 426618028.549 bytes/s

- Host memory: `host_peak_rss_bytes=98254848 host_swaps=0 `

- Source seal: `6b338c25ca6e9517c67a5b1d382371d23c83f05dcc322e66e65a1c9a7a5ced09`
- Exit status: `0`

## fair-layerfs-shell-smoke-20260901 — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/fair-layerfs-shell-smoke-20260901/raw/layerfs.jsonl`
- Lifecycle samples: 5
- Workspace Create median: 13896708 ns
- Small-edit Commit median: 5472375 ns
- Small-edit complete median: 28026500 ns
- Cold-create-32m Commit median: 42793417 ns
- Cold-create-32m complete median: 158985000 ns
- EDIT16 median: 184915792 ns
- Prepend median: 267290792 ns
- Read 32 MiB median: 119896042 ns
- Registered four-row total: 731087626 ns
- Inner 32 MiB write throughput: 358515290.981 bytes/s

- Host memory: `host_peak_rss_bytes=99483648 host_swaps=0 `

- Source seal: `80ba3072118a0c028e3aaa993a47058c6f871a79835628fb247362d331efb07f`
- Exit status: `1`

## v011-layerfs-registered-regression-r001-20260903 — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v011-layerfs-registered-regression-r001-20260903/raw/layerfs.jsonl`
- Result: invalid or incomplete evidence; see `raw/layerfs.stderr`.

- Host memory: `host_peak_rss_bytes=28131328 host_swaps=0 `

- Source seal: `7962d126a448255d3859edba57dd77ff745e7b57821b720a7ea2419b79723d1a`
- Exit status: `1`

## v011-layerfs-registered-regression-r002-20260903 — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v011-layerfs-registered-regression-r002-20260903/raw/layerfs.jsonl`
- Lifecycle samples: 35
- Workspace Create median: 16152958 ns
- Small-edit Commit median: 4967375 ns
- Small-edit complete median: 30486291 ns
- Cold-create-32m Commit median: 53249083 ns
- Cold-create-32m complete median: 161882250 ns
- EDIT16 median: 153524292 ns
- Prepend median: 299415791 ns
- Read 32 MiB median: 153871541 ns
- Registered four-row total: 768693874 ns
- Inner 32 MiB write throughput: 389508633.999 bytes/s

- Host memory: `host_peak_rss_bytes=92110848 host_swaps=0 `

- Source seal: `3738e4797481583c72d66f4ddd17c630b571cfabc32ad873bbfa957daa076d3a`
- Exit status: `1`

## v011-layerfs-registered-regression-r003-20260903 — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v011-layerfs-registered-regression-r003-20260903/raw/layerfs.jsonl`
- Lifecycle samples: 35
- Workspace Create median: 16030625 ns
- Small-edit Commit median: 4242750 ns
- Small-edit complete median: 29907875 ns
- Cold-create-32m Commit median: 46926167 ns
- Cold-create-32m complete median: 136378417 ns
- EDIT16 median: 152717959 ns
- Prepend median: 287890291 ns
- Read 32 MiB median: 145919334 ns
- Registered four-row total: 722906001 ns
- Inner 32 MiB write throughput: 450554066.678 bytes/s

- Host memory: `host_peak_rss_bytes=98648064 host_swaps=0 `

- Source seal: `3738e4797481583c72d66f4ddd17c630b571cfabc32ad873bbfa957daa076d3a`
- Exit status: `1`

## v011-layerfs-registered-regression-r004-20260903 — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v011-layerfs-registered-regression-r004-20260903/raw/layerfs.jsonl`
- Lifecycle samples: 35
- Workspace Create median: 14259333 ns
- Small-edit Commit median: 4434125 ns
- Small-edit complete median: 28726208 ns
- Cold-create-32m Commit median: 50109833 ns
- Cold-create-32m complete median: 135744875 ns
- EDIT16 median: 156931375 ns
- Prepend median: 249462709 ns
- Read 32 MiB median: 150420417 ns
- Registered four-row total: 692559376 ns
- Inner 32 MiB write throughput: 471378794.302 bytes/s

- Host memory: `host_peak_rss_bytes=95436800 host_swaps=0 `

- Source seal: `1f3c09c59f931615eca557bf1b17a2fead0200c0eebf1311638ad12f98bb7c97`
- Exit status: `1`

## v011-layerfs-registered-regression-r005-20260903 — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v011-layerfs-registered-regression-r005-20260903/raw/layerfs.jsonl`
- Lifecycle samples: 35
- Workspace Create median: 14899042 ns
- Small-edit Commit median: 4534709 ns
- Small-edit complete median: 30624250 ns
- Cold-create-32m Commit median: 46647750 ns
- Cold-create-32m complete median: 134652792 ns
- EDIT16 median: 147964792 ns
- Prepend median: 252647458 ns
- Read 32 MiB median: 147701875 ns
- Registered four-row total: 682966917 ns
- Inner 32 MiB write throughput: 473152421.678 bytes/s

- Host memory: `host_peak_rss_bytes=101203968 host_swaps=0 `

- Source seal: `1f3c09c59f931615eca557bf1b17a2fead0200c0eebf1311638ad12f98bb7c97`
- Exit status: `1`

## v011-layerfs-registered-regression-r006-20260903 — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v011-layerfs-registered-regression-r006-20260903/raw/layerfs.jsonl`
- Lifecycle samples: 75
- Workspace Create median: 12864541 ns
- Small-edit Commit median: 4261208 ns
- Small-edit complete median: 27209084 ns
- Cold-create-32m Commit median: 43052625 ns
- Cold-create-32m complete median: 127767875 ns
- EDIT16 median: 160652125 ns
- Prepend median: 243328250 ns
- Read 32 MiB median: 143041292 ns
- Registered four-row total: 674789542 ns
- Inner 32 MiB write throughput: 516061916.605 bytes/s

- Host memory: `host_peak_rss_bytes=104202240 host_swaps=0 `

- Source seal: `1f3c09c59f931615eca557bf1b17a2fead0200c0eebf1311638ad12f98bb7c97`
- Exit status: `0`

## v011-rc-payload-final-r001-20260903 — one-Store public-SDK campaign

### One-Store fs-bench-pro campaign

- Raw evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/runs/v011-rc-payload-final-r001-20260903/raw/layerfs.jsonl`
- Lifecycle samples: 75
- Workspace Create median: 14549667 ns
- Small-edit Commit median: 4502500 ns
- Small-edit complete median: 29652666 ns
- Cold-create-32m Commit median: 45442084 ns
- Cold-create-32m complete median: 131773958 ns
- EDIT16 median: 156445958 ns
- Prepend median: 223763417 ns
- Read 32 MiB median: 141418125 ns
- Registered four-row total: 653401458 ns
- Inner 32 MiB write throughput: 505614815.345 bytes/s

- Host memory: `host_peak_rss_bytes=97124352 host_swaps=0 `

- Source seal: `dd219ed9e7942a42891ff14646ee3c54a4580e6aaeeee7a25a01b30d1453a805`
- Exit status: `0`
