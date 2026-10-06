# Acquisition space and scaling results, 2026-10-06

> **Status:** Free-page retention fixed in new acquisition Stores; performance and strict allocation qualification remain FAIL.

Final product source `dd43af598440847834d86fd10bbb7589e0e2cd62` batches bounded page reclamation after the first source `85a4e96711fe38ced4d09cc7f38bf205748f447c`. Both eight-case campaigns are complete and retained. New-source final Stores have zero freelist pages and empty acquisition tables; every selected row passes cold-content attestation, root/inventory/sample proof, cleanup and the unchanged30s/19s command/proof bounds. v1 has zero latency PASS; v2 has three latency PASS, zero strict allocation PASS and zero joint PASS. S7/S8/S9 runtime acceptance remains incomplete.

## Final latency and allocation versus cluster one

The control is the qualified public Project Init at `197d2fb7d0a141d7a9350852022febeec3255bf2`, with the same Durable/Disposable profiles, workloads and workers. One sample per changed source/case; no best-of selection or unchanged acceptance resampling. Reclamation is paid inside the complete product clock. The earlier83.627ms receipt remains historical.

| Profile/files | Cluster-one ns | Final ns | Difference | Speed | Cluster-one B | Final B | Difference B | Storage/joint |
| --- | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- |
| durable/100 | 79,759,708 | 84,882,625 | +6.422939% | PASS | 5,255,168 | 5,287,936 | +32,768 | FAIL/FAIL |
| durable/1000 | 201,566,000 | 227,102,750 | +12.669175% | FAIL | 20,545,536 | 20,602,880 | +57,344 | FAIL/FAIL |
| durable/10000 | 2,492,429,625 | 2,617,345,250 | +5.011801% | PASS | 305,070,080 | 305,467,392 | +397,312 | FAIL/FAIL |
| durable/100000 | 7,724,523,333 | 9,766,435,541 | +26.434152% | FAIL | 514,965,504 | 515,579,904 | +614,400 | FAIL/FAIL |
| disposable/100 | 38,747,750 | 41,161,250 | +6.228749% | PASS | 5,222,400 | 5,255,168 | +32,768 | FAIL/FAIL |
| disposable/1000 | 129,258,375 | 168,017,459 | +29.985743% | FAIL | 20,537,344 | 20,578,304 | +40,960 | FAIL/FAIL |
| disposable/10000 | 1,645,276,292 | 1,880,471,250 | +14.295165% | FAIL | 305,074,176 | 305,491,968 | +417,792 | FAIL/FAIL |
| disposable/100000 | 5,558,569,958 | 7,111,097,500 | +27.930341% | FAIL | 514,940,928 | 515,551,232 | +610,304 | FAIL/FAIL |

Speed is `10*current_ns <= 11*control_ns`; final allocated DB/WAL/SHM must be at most the same-profile control. No allowance is introduced. Raw clocks, phases, SQL metrics, page accounting, source identities and every v1/v2 row are in the [ledger](checks/space-scaling-20261006/ledger.json).

## Space mechanism and remaining overhead

Durable100000 falls from551,370,752 B in the restored campaign to515,579,904 B:35,790,848 B returned. Disposable100000 falls from551,313,408 B to515,551,232 B:35,762,176 B returned. All16 new campaign Stores have `freelist_count=0` and zero `init_operation`, `init_entry` and `init_native_file` rows. These are final allocations after the complete lifecycle, not peaks.

The final overage is614,400 B (Durable100000) and610,304 B (Disposable100000), about0.12% of the controls. New pointer-map pages and acquisition schema/packing overhead remain real allocation. Removing free-page retention does not remove those costs or turn the strict gate into PASS. Existing mode0 Stores open without implicit migration and refuse physical maintenance; these improvements apply to newly created acquisition Stores.

Each normal cleanup job touches bounded rows and removes at most512 free pages. Final source accumulates less than512 pages of reuse headroom between jobs; release/explicit maintenance drain a smaller tail. No whole-Store VACUUM, separate database, weaker durability, extra constructor or hidden background completion is used. See [implementation/selection](SPACE-AND-SCALING-PLAN-20261006.md).

## Matching100000-file reference attribution

A new owned isolated checkout runs the actual cluster-one public `layerfs_project::init`, not the old Service or a copied importer. Product seal matches the qualified reference; only a public-port diagnostic example/observer is added. The seed1/500MB source, four constructors, environment workers1 and WAL/FULL/fullfsync are the same. Current public Init uses the existing diagnostic wrapper. Instrumented/uncontrolled observations are causes/counts, not another acceptance pair. [Comparison](checks/space-scaling-20261006/attribution-comparison.json).

| Count | Cluster-one public Init | Current public Init |
| --- | ---: | ---: |
| Whole Init: SQL statements | 9,695 | 27,527 |
| Whole Init: VM steps | 14,741,551 | 61,067,367 |
| Whole Init: write commits | 429 | 546 |
| Immutable publication: SQL statements | 8,251 | 8,252 |
| Immutable publication: VM steps | 13,717,730 | 13,717,761 |
| Immutable publication: write commits | 296 | 297 |
| Saved-object location: SQL statements | 897 | 870 |
| Saved-object location: VM steps | 1,016,261 | 1,016,213 |
| Saved-object location: write commits | 0 | 0 |
| Pack/ordinal reservation: SQL statements | 524 | 532 |
| Pack/ordinal reservation: VM steps | 7,074 | 7,182 |
| Pack/ordinal reservation: write commits | 131 | 133 |

Current acquisition alone adds46,325,725 VM steps and114 acknowledged write commits. The total VM difference is46,325,816; all but91 of those extra steps are attributed to acquisition units. Immutable publication is effectively unchanged at13.72million VM steps. The remaining gap is the additional mutable acquisition work, rather than a changed committed payload. This count attribution is much stronger than inferring a cause from noisy wall ratios.

The reference retains input-sized entry/job/serial/inode/directory vectors and uses ordering scratch only in namespace construction. The current source replaces those growing containers with indexed Store rows and bounded windows. Restoring the old acquisition path would restore the old resource behavior; it is not selected. Native scratch syscall bytes/time and some reference SQL counters are unavailable, explicitly null.

The phase timers report reference scan2.245s/files6.759s and current scan2.865s/files7.596s/tree0.511s/acquisition cleanup0.596s in their instrumented windows. Reference namespace construction and scratch cleanup lack their own public timer regions; do not assign its unaccounted root time to one mechanism. SQL, COMMIT, transaction, Save stages and phase clocks overlap and are never added or subtracted to manufacture an exclusive latency model. These diagnostic clocks do not establish isolated causal speed deltas.

Before correction, jobs made1614 calls/6456 statements; final source makes197 calls/984 statements. Deletion VM work falls11,777,603 to9,910,474. Batching cuts cleanup engine reprepares49 to17. Input/row bounds, order, exact charges, stale fencing and definite/uncertain outcomes are preserved. The same512-row removal job uses30,389 VM steps at both2000 and20000 stored entries. Common Storage plans/programs for both arms are in [matched plans](checks/space-scaling-20261006/common-storage-plans.json). Exact acquisition plans and the Apple SQLite3.51.0 full programs are retained in [SQL plans](checks/space-scaling-20261006/new-sql-plans.json) and public-test output. SQLite still owns bounded deletion-key/RETURNING scratch.

The four qualification tiers vary both file count and logical bytes (5/20/300/500MB). They show a larger absolute gap at100000 files but do not establish a pure scaling exponent. Source/count analysis identifies indexed per-entry work and bounded jobs; it does not prove quadratic behavior. Further latency work should target acquisition insertion/root-recording/removal and their actual page/sync costs.

## Proof, limits and retained failures

Host scope:182 public Persistence/Project test bodies passed, with final changed-scope acquisition/reclamation proofs repeated only after the endpoint and batching changes; locked all-targets Clippy `-D warnings`, formatting,659-file boundary guard,40 guard self-tests and17 harness tests pass. Every test invocation had an explicit at-most120s timeout; no test hung. Global persistence remains macOS-only; no Linux Store or full runtime qualification is claimed. Existing unrelated owner files/containers and legacy source remain unchanged.

All diagnostic outcomes remain: initial preparation path error (no Init), missing cursor-key verifier environment followed by a separate corrected proof, initial reference wrapper build failure for unavailable fields (corrected before Init), a disabled VFS observation with successful Init/proof but absent I/O totals, and an enabled current VFS diagnostic killed at its15s bound during file publication. Its partial Store and outputs are retained; no larger budget or repeat acceptance sample follows. Reference VFS aggregate counts are available; matching complete current physical-I/O/sync counts are unavailable, not zero.

The full independent Stores/binaries/raw copies are in `benchmark-results/fs-bench-pro/space-scaling-retained-20261006`; [copy manifests](checks/space-scaling-20261006/closed-copy-manifest.json), [diagnostic custody](checks/space-scaling-20261006/diagnostic-copy-manifest.json) and [post-seals](checks/space-scaling-20261006/post-seals.json) bind the retained evidence. Raw qualified reference timing/allocation receipts are unchanged. Independent verifier coverage is every path/kind/directory metadata plus deterministic sampled payload, not a full-content oracle. Phase/system memory and device I/O remain unqualified.

## Source size

`85a4e9671`: Production LOC159916 ->160062 (delta+146); core94499 ->94645, root reference65417 unchanged. `dd43af598`:160062 ->160073 (delta+11); core94645 ->94656, reference unchanged. Exact git first-parent/staged/committed snapshots, the same Rust-aware counter and shipped SQL scope are recorded in the commit messages and [LOC receipts](checks/space-scaling-20261006/batching-loc.json). Tests/examples/docs/tools and legacy inline tests are excluded. No relocation, duplicate implementation or legacy retirement is claimed.


Diagnostic isolation: qualified v1/v2 samples had no own same-worktree build or
measurement overlap. Count diagnostic clocks include instrumentation and host
activity and are not an isolated causal speed pair. The enabled current VFS
observation exceeded15s while publishing file objects; it wraps every SQLite/VFS
call and emits per-port records. It was killed and retained without increasing
the cap; this separate instrumentation failure is not a qualified product timeout.
