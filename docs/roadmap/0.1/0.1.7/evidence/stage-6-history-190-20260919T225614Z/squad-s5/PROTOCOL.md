# S5 — matched diagnostic protocol and unmatched register

> **Status: Research; informative and not a product contract.** Prospective #190 research protocol. No product changes, no release admission, no historical receipt promotion. This document is a pre-run protocol, not a claim that a new run occurred.

## Applicability and unresolved budget

The user authorized a squad investigation with harness instrumentation and documents only. Product edits, dependency patches, repeated-sample selection, v0.1.6 reruns and stride1 optimization are prohibited. Each proposed diagnostic must fit that authorization and the applicable declared budget.

There is an explicit document conflict to retain: `docs/roadmap/0.1/0.1.7/retained-history-storage.md:241–254` says the family budget is lifted and requires prospective per-tier complete-command ceilings; the user's handoff §9 says the runner has no such ceilings wired and the 15/25 s ceilings apply. General `benchmark/AGENTS.md:80–85` caps complete performance commands at 15 s or declared exceptions of 25 s. An old 44,999,165,042 ns invocation cannot establish a new passing 25 s run. The existing family ruling is not a numerical ceiling and not unlimited time.

At protocol preparation, root has requested clarification of a proposed **diagnostic-only 120 s stride10 / 240 s stride3 complete-command allowance**. Those are proposals, not approved budgets, and no elapsed wait is approval. Until the applicable ceiling is settled, fresh long-chain performance and verification are **NOT_RUN**. This protocol neither relaxes an existing valid miss nor labels a raw run admitted. A later actual owner ruling must be appended to the evidence/ledger with exact scope before collection; the old receipt and its historical limits stay unchanged.

This investigation can still recover and independently analyze existing artifacts, reconstruct source boundaries, and validate harness instrumentation. Those actions are not new measurements of the product operation. Build/test wall is recorded separately and must not overlap a benchmark.

## Sample definition and identities

The diagnostic question is the exclusive per-state decomposition of `history-stride10` (17 states), followed only when justified and authorized by `history-stride3` (53 states). No stride1 sample is scheduled by this investigation. There is one performance sample per selected case per arm; failed, invalid, incomplete and interrupted samples remain evidence. No n3, best-of, smoothing, dropping slow states, or hidden sample repeats.

Before the sample, retain:

- Source commit, tree, product hash/seal, exact harness diff/hash, compilation/dependency identity, binary SHA-256, locked Cargo manifest/lock, Rust version, host/OS/CPU identity, and build reuse status. A dirty harness is named as dirty; it is not a clean source seal.
- Manifest SHA-256 `03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271`, tip `b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed`, full selected index vector, and oracle/source hashes actually checked. The stride10 vector is `[1,11,21,31,41,51,61,71,81,91,101,111,121,131,141,151,157]`.
- Complete command, working directory, fresh output path, wall ceiling, sample label, environment, cache declaration, and pre/post machine observations. Any nested runtime/container identity is recorded if one is used; the raw core path is host-only.
- Exact set/unset state of all eight history switches: `LAYERFS_HISTORY_ADVISORY`, `LAYERFS_HISTORY_ORDERED_PREDECESSORS`, `LAYERFS_HISTORY_DECLARED_ONLY`, `LAYERFS_HISTORY_SIMILARITY_CANDIDATES`, `LAYERFS_HISTORY_FALLBACK_CANDIDATES`, `LAYERFS_HISTORY_FULL_PRODUCER`, `LAYERFS_HISTORY_CHUNK_PREDECESSORS`, `LAYERFS_HISTORY_DEPTH_LIMIT`. Also record `LAYERFS_CONSTRUCTION_WORKERS` and instrumentation switch `LAYERFS_HISTORY_PHASES`.

The initial environment and [pre-run-observation.json](pre-run-observation.json) have all ten variables unset. For an authorized instrumented run, set `LAYERFS_CONSTRUCTION_WORKERS=1` and `LAYERFS_HISTORY_PHASES=1`; leave the eight behavioral switches unset unless a separately preregistered single-lever arm requires one. Record resolved defaults from this exact harness source rather than assuming unset means false.

Reuse a qualified binary/build and immutable corpus preparation. The lane uses `Preparation::InProcess` and creates a new growing Store: it has no prepared Store to clone (`retained-history-storage.md:98–103,248`). Do not supply an old sample Store, reuse a mutated sample, preconstruct measured canonical objects, or add a synthetic `--setup clone` argument unsupported by the raw driver. Record setup mode as in-process/no prepared Store, and corpus/build reuse separately.

## Machine and locking protocol

Read-only observation at **2026-09-19T23:03:23.118404+00:00** found load averages **5.24 / 4.92 / 4.54**, the core lock absent, no cargo/rustc/fs-bench process in the selected process listing, and one Python process. This does **not** establish a quiet machine or describe a measurement's machine state. The process filter and load snapshot are incomplete evidence about background work; do not call this machine quiet from a missing cargo process alone.

For a new sample, root is the sole measurement coordinator. Squads stop resource-sensitive execution first; source reading and report drafting may continue. Acquire and hold every applicable lock for build/test or measurement, using one consistent order:

1. Legacy `Path(TMPDIR or /tmp) / "layerfs-infra-measurement.lock"`, with nonblocking exclusive `fcntl.flock`, matching `benchmark/fs-bench-pro/shared/storage_smoke.py:492–493`.
2. `/tmp/layerfs-infra-measurement.lock` as well when TMPDIR resolves elsewhere, to coordinate processes started under a different environment. Deduplicate identical resolved paths before locking.
3. Core `core/benchmark/fs-bench-pro-storage-content/.measurement.lock`, via the existing `shared/receipt.py:321–352` nonblocking O_EXCL context manager.

**Observed build/test order:** root's campaign build/check scripts acquired the legacy TMPDIR flock, then the distinct `/tmp` flock, then the core O_EXCL lock, all nonblocking. The prospective order above matches that executed order. No new performance measurement had occurred when this order was recorded; the 23:03 pre-run snapshot is a point-in-time observation, not a claim that it preceded every build/check invocation.

The two harnesses use different paths and locking primitives; holding only one cannot exclude the other. If any lock is unavailable, release this coordinator's acquired locks, retain the refusal and wait for the owner to finish. Never unlink, steal or silently replace another owner's lock, even when stale-looking. The raw binary does not supply this coordination automatically.

After acquiring locks, record UTC, load, process CPU snapshot, memory/swap pressure, and available disk; check no competing build, benchmark, verifier or other resource-sensitive campaign is active. If the machine is visibly busy, defer the sample without killing or interrupting the other process. Capture post-run observations and state whether quietness was actually established. A noisy completed sample remains retained and labelled; it is not rerun solely for a better number. A lock protects cooperating campaigns, not arbitrary user workloads.

## Cache and timing declarations

Legacy raw identity explicitly declares **fresh-store-existing-os-cache-uncontrolled**. Core constructs one Store across successive states and reads predecessor data from that Store after preceding saves. Changed corpus bytes are acquired outside each state child. Neither acquisition nor a fresh Store proves coldness, and ordinary cached data may survive between states.

Consequently this campaign must declare **retained-chain OS/Store cache residency unknown; no cold proof** unless an actual supported invalidation-plus-residency contract is applied and retained for all timed data. Do not pre-touch Store pages, run a verifier before the performance sample, drop caches for one arm only, or move required timed reads outside the timer. There is currently no demonstrated cold contract for all intra-chain predecessor reads. Descriptive diagnostic attribution can be published with this limitation; a new cold/matched performance claim is **INELIGIBLE**, regardless of the numerical timing. Original historical statuses are not rewritten.

`operation_ns` remains exactly the sum of 17 state children; the root encloses corpus acquisition too. Instrumentation must retain the old state boundaries and name every added region. Report exact sums and residuals, with nested spans accounted once. A region's inclusive time cannot be added to a child's inclusive time. Read-back/order counters can describe work within the build interval; without exclusive clocks they cannot divide that interval into additive causal time. A residual is an explicit unmeasured region, not zero by convention.

Host process CPU covers a process/window. `Instant` encoder timings are elapsed wall, not codec CPU. Peak RSS is lifetime unless proven reset; incremental heap windows are separate. A process heap bound does not prove bounded file cache, and host memory cannot be replaced by container memory. SaveOutcome commits/charged bytes must preserve their declared counting scope; no max, peak, last-ID or cumulative counter is silently summed as an interval count.

## Collection and evidence checklist

1. Freeze arm, lever, source/binary identity, declared cache stance, exact selection and complete-command ceiling before execution. A lever that requires product edits is NOT_RUN under this authorization.
2. Finish isolated build/check work first using `--locked`. No CI or `tools/preflight.sh`, no aggregate substitute, no dependency modification. Do not compare a newly rebuilt arm to a stale control as a matched pair.
3. Under all locks and quietness check, run the raw driver once with a never-used `--out`. External supervision measures the full command and preserves exit status, stdout/stderr, phase/trace/timing artifacts and timeout/cleanup outcome. Complete wall includes launch, product, artifact handoff and required cleanup; build time remains separately disclosed.
4. If timeout or failure occurs, retain the partially produced directory and mark its exact observed wall/status. Never overwrite the output or silently restart. No shrinking workload, worker increase, changed buffer/timeout or cache policy to turn the same miss into PASS.
5. Copy/retain raw artifacts and checksums before publishing. Historical copies carry their original source and custody, not the current harness identity. Record every invocation, including NOT_RUN decisions and failures, in the append-only campaign ledger.
6. Independent verification is a separate, identity-matched mode with a fresh evidence path; it is not automatic permission to exceed the applicable ceiling. The lane's ruled verification ceilings remain 10/20/30 s for stride10/3/1 and no verification is hidden inside performance. An absent verifier or golden pin makes admission incomplete even if the raw operation succeeds.
7. Publish all registered state rows and all non-passing lines; report diagnostic/admission status separately. Preserve disagreements between squads. Independent review re-derives arithmetic from raw copies without editing them.

## Unmatched-comparison register

| Proposed comparison | Status in #190 | Why it cannot establish the claimed mechanism |
|---|---|---|
| Core 32,566,067,669 ns vs legacy 11,370,679,212 ns | Historical descriptive tripwire only | Different products, operation surfaces, hybrid/host-only topology, cache, epochs/machine state, metadata and canonical object stream; no current matched control |
| Legacy Commit vs core content span | NOT_MATCHED | Legacy content interval includes pipeline admission/payload transfer; core content ends before tree update/accept loop; source proves both construct content |
| Legacy named phase sum as exclusive partition | UNSUPPORTED | Nested/accumulating CandidateFinish, CandidatePlan, Content and pipeline counters overlap; zero telemetry unattributed_ns does not repair this |
| GROUP_LEVEL 1 vs 19, same current product | NOT_RUN | `core/crates/layerfs-storage/src/encoding/codec.rs:102,393` hardcodes level19, no discovered public level lever; editing it is a product change explicitly prohibited |
| Historical codec experiments as modern lane CPU effect | NOT_MATCHED | Different fixtures/source/boundaries; elapsed encoder intervals are not codec CPU; cite only with original scope |
| One vs historical runtime construction worker count | NOT_RUN / historical count unknown | No effective historical per-state worker record; predecessor-bearing legacy plans force one. Core constructor path exposes no discovered equivalent worker-count lever; environment assignment alone does not create an experiment |
| Persisted index on/off attribution | NOT_RUN unless an actual public product lever is identified | Harness advisory switches alter supplied predecessors rather than remove product index cost. Counter correlation is not a controlled latency difference |
| Save cadence/charged bytes | Observable only from retained counters or authorized instrumented run | Counting transactions does not itself measure latency caused by cadence; a policy change would need a separately permitted arm |
| Instrumented new run vs original retained run | Unpaired diagnostic | Harness instrumentation changed and machine/cache state differs; use within-run exclusive attribution, no product regression delta |
| Stride10 vs stride3/stride1 | Distinct workloads, not repetitions | Selection changes transition deltas and retained chain; cannot average them or infer a per-checkpoint constant |
| Database-only core size vs legacy retained-directory total | Scope mismatch must be disclosed | S4 found legacy directory exceeds database by 8,192 allocated/100 apparent B; use declared gate scope and report database scope separately |
| Historical/partial raw success as release gate | INELIGIBLE | Runner budgets/golden pins/verification wiring are incomplete; time remains a tripwire, raw diagnostic provides no admission |

## Required disposition

The current protocol outcome is **fresh long-chain collection NOT_RUN pending the applicable prospective complete-command ceiling**, **historical timing pairing INELIGIBLE**, and **hardcoded codec/true-worker levers NOT_RUN under no-product-change scope**. These are missing-evidence outcomes, not falsified cost hypotheses. If a future run is authorized, append its actual eligibility and measurements; do not replace this pre-run record or historical evidence.
