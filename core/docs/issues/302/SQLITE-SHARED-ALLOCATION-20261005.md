# Shared bounded allocation: prospective Init adjustment, 2026-10-05

Owner requests shared optimization, explicit shared/profile-specific boundaries and adjusted namespace Init results. Product treatment: both profiles use one bounded main-file preallocation algorithm, identity custody, reservation limit, checked filesystem operations, counters and extent release. Disposable temporary descriptor lifetime and Durable retained descriptor lifetime remain explicit. Durable WAL/FULL/fullfsync/autocheckpoint/final checkpoint unchanged; no custom WAL allocation, transaction batching, durability relaxation, retries or shifted work. Main-file preallocation aims at insert/checkpoint growth; its performance benefit is a hypothesis to measure.

Prospective eight Init cases: phase7-sqlite-init-{100,1000,10000,100000}-shared-allocation-v1 (Durable) and phase7-sqlite-disposable-init-{tier}-shared-allocation-v1. Both arms release/locked, single sample at frozen source, existing prepared masters reused, source cold invalidation/whole-input residency, fresh output, permitted four Init workers / environment limit 1. Complete command30s / separate proof19s. Relative ceiling remains1.10x; final candidate main/WAL/SHM <= matched original reference total. Current Monolithic layout unchanged. Exact original reference `7edddbdb8`; independent namespace/path/sample/root proof. Prior rows/FAILs preserved; changes compared to earlier candidate windows are contextual, new paired reference decides qualification. No history speed qualification at adjusted product source is claimed.

Shared public-API headroom/canonical/final-allocation test covers both profiles; owning locked Core tests/clippy/fmt/boundary and guard self-tests at final source. Final commit exact production LOC required. Record all gates and misses and source identities; keep #302 closed while adding checkpoint/result comments. No push/PR/release.

Implementation covering validation: 523 passing tests across 100 all-target reports, zero failed/ignored; full locked workspace/all-target Clippy -Dwarnings, fmt and 456-file boundary PASS;23 guard self-tests  / 9 focused harness tests PASS. Shared public headroom test covers both profiles. Source review preserves Unknown errors when a subsequent custody check also fails. Raw logs retained in benchmark-results/fs-bench-pro/issue302-shared-allocation1/.

Implementation Production LOC:140304 ->140296 (delta -8); reference65417 ->65417, Core74887 ->74879. Exact first-parent/final staged archives, same tools/production_loc.py c0fe7f36 and classification/inline-test exclusions; runtime SQL included, examples/tests/harness/docs excluded. The reduction comes from sharing ownership/operations and removing the profile-only preallocation guard, not retiring a product or changing counting scope.

## Adjusted namespace Init results

Frozen source `197d2fb7d`. All eight new matched pairs/16arms complete; every raw manifest rechecked. Both profiles actually exercise the shared allocator. Disposable **4/4 joint PASS**; Durable **4/4 joint FAIL** only the retained **1.10×** reference speed gate. Every independent namespace/path/sample/root proof, final allocation <= matched reference, cold source, cleanup, 30 s command and 19 s proof cap PASS. Unmodified original Phase4.5 MEMORY/OFF reference is paired afresh; Durable is WAL/FULL/fullfsync, not a same-durability control. Init defaults to Monolithic. No historical row is changed.

|Profile / files|Reference product s|Candidate product s|Difference %|Complete command s|Separate proof s|Candidate allocated B / limit|Joint gate|
|---|---:|---:|---:|---:|---:|---|---|
|durable / 100|0.043420250|0.079759708|+83.692420011|0.942955666|0.518297292|5255168 / 7372800|FAIL|
|durable / 1000|0.120408541|0.201566000|+67.401746027|0.307985209|0.031205458|20545536 / 23101440|FAIL|
|durable / 10000|1.475158041|2.492429625|+68.960176180|3.363680625|0.331556791|305070080 / 314773504|FAIL|
|durable / 100000|6.114136834|7.724523333|+26.338738283|15.160584292|1.134068458|514965504 / 518029312|FAIL|
|disposable / 100|0.041980292|0.038747750|-7.700141771|0.066342500|0.018800584|5222400 / 7372800|PASS|
|disposable / 1000|0.132300250|0.129258375|-2.299220901|0.230414041|0.033392375|20537344 / 23101440|PASS|
|disposable / 10000|1.577107625|1.645276292|+4.322385227|2.532887541|0.348281167|305074176 / 307265536|PASS|
|disposable / 100000|5.920051500|5.558569958|-6.106054010|12.225680958|1.023206791|514940928 / 518029312|PASS|

|Profile / files|Actual reservations|Cumulative reservation B (not retained allocation)|Reservation wall ms|Write commits|COMMIT wall s|External driver CPU s / lifetime RSS B|
|---|---:|---:|---:|---:|---:|---|
|durable / 100|2|6385664|0.559168|10|0.039660916|0.072774000 / 40484864|
|durable / 1000|5|14032896|0.953620|17|0.090569248|0.229171000 / 54296576|
|durable / 10000|26|69705728|14.447986|135|1.183515707|2.682812000 / 68452352|
|durable / 100000|25|50843648|22.674186|440|2.903267793|11.156609000 / 144277504|
|disposable / 100|2|6242304|0.680123|10|0.005538622|0.051969000 / 39419904|
|disposable / 1000|7|17260544|2.776706|17|0.022384836|0.191245000 / 55558144|
|disposable / 10000|54|132825088|20.193769|132|0.317967040|2.312685000 / 68517888|
|disposable / 100000|74|139948032|19.130019|436|0.564632093|9.535228000 / 143523840|

## Shared versus profile-specific behavior

|Behavior|Shared implementation|Profile-specific behavior|
|---|---|---|
|Reservation arithmetic and bounds|Next pack capacity + 2 MiB headroom, MiB rounding, cap singleton-pack limit + 3 MiB|None|
|File custody|Absolute path, device/inode, exclusive regular single-link file; before/after checks|None|
|Physical reservation|Same F_PREALLOCATE primitive; logical EOF unchanged; one before-pack call path|None|
|Operation handling|Shared with_file helper, checked temporary closes, unknown-outcome priority and transaction quarantine|Durable retained handle; Disposable temporary handle|
|Unused extent release|Same checked transfer/scratch cleanup and custody|Durable first requires unobstructed WAL checkpoint; Disposable has no WAL checkpoint|
|Counters and covering headroom test|Same counters and one test covering both profiles|Separate effective-profile and integration/performance qualification|
|Journal / synchronization|Common configuration plumbing, unchanged selected policy|Durable WAL/FULL/fullfsync; Disposable MEMORY/OFF|
|WAL allocation and write-commit scheduling|No custom implementation added|SQLite owns WAL growth/checkpointing; transaction/publication boundaries unchanged|

This removes the Disposable-only preallocation guard and shares the real mechanism, not just declarations. Durable retains its established descriptor/checkpoint lifecycle. Main-file headroom does not reserve WAL extents or eliminate durable synchronization. Recorded COMMIT time includes SQLite work within COMMIT, not an exclusive physical-sync measurement; statement/commit timings overlap and must not be summed. CPU/RSS are external driver lifetime observations, not phase bounds.

## Comparison with the preceding candidate window

|Profile / files|Previous candidate s|Adjusted candidate s|Observed difference %|
|---|---:|---:|---:|
|durable / 100|0.084598417|0.079759708|-5.719621208|
|durable / 1000|0.256686959|0.201566000|-21.474000555|
|durable / 10000|2.630212625|2.492429625|-5.238473829|
|durable / 100000|7.829636333|7.724523333|-1.342501689|
|disposable / 100|0.036498125|0.038747750|+6.163672791|
|disposable / 1000|0.128361333|0.129258375|+0.698841294|
|disposable / 10000|1.678383625|1.645276292|-1.972572450|
|disposable / 100000|5.770752667|5.558569958|-3.676863682|

These preceding-candidate differences are context, not isolated causal speedups: reference times also moved between windows, and there is one sample per case/arm. The fresh paired reference governs admission. Sharing main-file allocation does not solve the Durable Init relative-speed misses. Remaining large measured term is COMMIT work; diagnosing allocation versus WAL synchronization within it requires cause instrumentation, not resampling an unchanged arm.

All eight candidate proofs cover every path/kind/directory metadata and deterministic selected full file metadata/content, not all payload bytes. Identical roots match references. Seed1, reused identity-checked prepared sources, four Init workers / environment limit 1, whole-input `resident_after=0`, fresh outputs. Release/locked immutable binaries, worktree-local incremental builds, exact commands/build/harness/product/dependency/profile/fixture/source identities and CPU/RSS in [compact evidence](checks/shared-allocation1/). No source setup regeneration or in-timer reuse. No SDK/daemon/FUSE/Docker claim; direct public project component Init versus pinned reference Service route. Complete product includes create/open/import/checkpoint/close; separate verifier outside speed clock. History was not resampled at adjusted product source; earlier history PASS receipts remain pinned to their original source.

Implementation commit `197d2fb7d`: Production LOC: 140304 → 140296 (delta -8), reference 65417 → 65417 / Core 74887 → 74879, exact counter archives. Final report checkpoint has unchanged production total 140296 (delta 0). Both use tools/production_loc.py c0fe7f36 with identical source classification/inline-test exclusions and runtime SQL inclusion. Validation: 523 tests / 100 reports, zero failed/ignored, full locked workspace/all-target Clippy/fmt, boundary 456, guard 23, focused harness 9 PASS. No third-party changes, CI or retired aggregate preflight. Original provisional Core run before Unknown-priority source-review correction and all logs retained. No push/PR/release; #302 stays closed, updated by result comment.
