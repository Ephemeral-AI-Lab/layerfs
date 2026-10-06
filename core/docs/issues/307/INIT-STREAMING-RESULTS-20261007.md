# Bounded streaming Init results

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The first streaming source is5a97046108f4f158839afbc180625130f32cf3fc. It eliminates SQL file-root completion and the later root-read pass, with512admitted identities, four constructors and one assembly Save. Immediate successful completion flushing proved expensive at larger tiers. A distinct bounded-coalescing correction is selected; these V1receipts remain retained. [Plan](INIT-STREAMING-PLAN-20261007.md), [ledger](checks/init-streaming-20261007/ledger.json), [closed byte copies](checks/init-streaming-20261007/closed-copy-manifest.json).

One cold candidate sample per selected case, clean sealed source/release binaries, same-profile197d2fb7d controls reused, same masters/cache/4workers, fresh create+Init+bounded reclamation+checkpoint+close. Complete command<=30s and separate proof<=19s. All eight complete/proof/cache/cleanup checks PASS; one speed PASS, eight final-allocation FAILs, no joint PASS. Every final Store has freelist0 and zero operation/entry/native rows. Source data residency0 is retained in each receipt. No unchanged sample was rerun.

| Profile / files | Cluster one ns | V1 ns | Cluster one allocated B | V1 allocated B | Speed | Allocation |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| durable / 100 | 79759708 | 80311208 | 5255168 | 5279744 | PASS | FAIL |
| durable / 1000 | 201566000 | 263183667 | 20545536 | 20623360 | FAIL | FAIL |
| durable / 10000 | 2492429625 | 3270636083 | 305070080 | 305397760 | FAIL | FAIL |
| durable / 100000 | 7724523333 | 12798146625 | 514965504 | 515702784 | FAIL | FAIL |
| disposable / 100 | 38747750 | 44959000 | 5222400 | 5251072 | FAIL | FAIL |
| disposable / 1000 | 129258375 | 184723875 | 20537344 | 20574208 | FAIL | FAIL |
| disposable / 10000 | 1645276292 | 2211625000 | 305074176 | 305410048 | FAIL | FAIL |
| disposable / 100000 | 5558569958 | 9513790500 | 514940928 | 515678208 | FAIL | FAIL |

Durable100 total80.311208ms versus control79.759708ms passes speed; allocation5279744B versus5255168B fails by24576B. Its observed bootstrap11.122542ms/Init66.244125ms is separate from the prior bootstrap22.492666ms/Init81.295792ms. Same DDL/profile does not establish the cause of bootstrap variation. One-sample gate success is not a repeatability or exclusive causal-speed claim.

The separately instrumented/uncontrolled100000-file diagnostic completed under15s with separate19s proof. It reports whole22286 statements/53079086 VM steps/536 write commits and acquisition12544 statements/38310695 VM steps/88 write commits. The preceding4a207 diagnostic has25662/60231985/542 and15966/45489938/113. Whole VM work falls7152899 steps(11.875%), while acquisition eliminates all25 completion commits and197 owned file-root snapshots. Job alias projection adds100000 VM steps and NULL-root cleanup adds100000; other immutable multiplicities change. Insertion23181878 VM and disposal10009575 VM remain the largest acquisition costs. [Exact unit counts](checks/init-streaming-20261007/diagnostics/receipt.json).

Instrumented immutable publications rise297→315(+18) and reservations130→131(+1), compensating19 of25 removed acknowledgments:542−25+18+1=536. Publication VM13718312 remains close to prior13718287 and reference13717730, but additional commits are real synchronization work. New acquisition88 writes plus448 other writes still exceed reference429 total. Do not sum inclusive SQL/publication/COMMIT spans.

The ordinary performance sample has different adaptive multiplicities: Durable10000022,321 SQL executions/53,081,557 VM/534 writes,311 immutable publications,301 Save waves,26 forced seals,77 pooled packs and100796 constructor frames. The prior ordinary sample has305 publications/285 waves/0 forced seals/67 pooled packs. SQL transaction wall rises5.796→6.106s while whole product rises9.749→12.798s; this supports a substantial non-SQL gap alongside real mixed-Save amplification, without an exclusive queue/wakeup clock. The V2treatment targets unconditional per-file frames and retains bounded canonical demand/error progress.

Verification covers248 affected Storage/Persistence/Project test bodies, Clippy-Dwarnings, fmt,662-file boundary scan,40 guardselftests and15 harness tests. Initial new failure-fixture and stale4096-row completion-window assertions are retained, diagnosed and repaired in affected tests only. The original uncertain error cannot be turned into Aborted by partial-prefix finalization. Public tests check canonical equality, wider-than512/large-first inputs, native aliases/data/metadata, live uncertain publication custody, alias replacement and cleanup-before-final-root. Deterministic native read scheduling and blocked-OS-read interruption remain unavailable.

Product commit Production LOC:160473→160673(+200), Core95056→95256, reference65417 unchanged, exact first-parent/staged/committed trees using unchanged production counter. No source relocation/reference retirement. [Counting record](checks/init-streaming-20261007/production-loc.json).

Independent ordinary byte copies retain full closed Stores/rawoutputs/binaries under benchmark-results/fs-bench-pro/init-streaming-retained-20261007; SHA values were compared at copy. No own build/test/copy overlapped a measured operation in the measurement worktree. After Durable100000, source editing/read-only small-log review by subagents occurred in the primary worktree, with no agent builds/measurements; competing-process/container snapshots remain in receipts. No current device-byte/VFS attribution or phase-only RSS claim is available. Immutable compatibility and S9/application/runtime qualification remain distinct.

## Retained bounded-coalescing V2 results

The second source is890a144ff5bf063e6c65001b5da2cfd2b5cfdae4. All eight complete/proof/cache/cleanup checks PASS; Durable1000 passes speed, seven speed gates fail, all eight allocation gates fail, no joint gate passes. Final freelist and acquisition rows are zero in every case. [V2ledger](checks/init-streaming-batched-20261007/ledger.json) and [independent closed copies](checks/init-streaming-batched-20261007/closed-copy-manifest.json) retain every outcome.

| Profile / files | Control ns | V2 ns | Control B | V2 B | Speed | Allocation |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| durable / 100 | 79759708 | 101886250 | 5255168 | 5287936 | FAIL | FAIL |
| durable / 1000 | 201566000 | 220885042 | 20545536 | 20615168 | PASS | FAIL |
| durable / 10000 | 2492429625 | 2946561709 | 305070080 | 305426432 | FAIL | FAIL |
| durable / 100000 | 7724523333 | 10535288833 | 514965504 | 515747840 | FAIL | FAIL |
| disposable / 100 | 38747750 | 44650458 | 5222400 | 5246976 | FAIL | FAIL |
| disposable / 1000 | 129258375 | 163748375 | 20537344 | 20582400 | FAIL | FAIL |
| disposable / 10000 | 1645276292 | 1987603209 | 305074176 | 305369088 | FAIL | FAIL |
| disposable / 100000 | 5558569958 | 7888940875 | 514940928 | 515698688 | FAIL | FAIL |

The instrumented 100000 diagnostic completed under15s with separate19s proof. Whole SQL: 22277 executions / 53079314 VM steps / 538 write commits; acquisition remains12544 /38310695 /88, with no completion or root-read calls. Output frame reduction changes ordinary Save multiplicities; no SQL plan changed. Query plan SHA values were revalidated and earlier unchanged Storage/Persistence proof reused. Instrumented time is ineligible for performance acceptance.

Ordinary Durable100000 frames fall100796→28248 and complete product time12.798→10.535s, but remain slower than incumbent9.749s. Completed high-water reaches512. One-job replenishment repeatedly allows queue-idle flushes; the final half-free refill treatment targets that source-supported mechanism. It adds no worker/schema/profile and leaves exact Unknown/alias/final-root boundaries intact. Exclusive wakeup/queue wall remains unavailable; mixed Save sealing/publication stays a separate cost.

V2verification:81 Project all-target bodies, including5 streaming proofs, completed22.943s under120s; Clippy-Dwarnings/fmt/662-file guard/40 guardselftests/16 harness tests PASS. Unchanged Storage/Persistence and actual SQL plan evidence reused by exact source/hash. Production LOC160673→160743(+70), Core95256→95326, reference65417 unchanged. The final candidate has its own source/proofs/selection; no V2sample is rerun.
