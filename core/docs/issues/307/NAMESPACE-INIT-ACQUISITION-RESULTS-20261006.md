# Namespace Init acquisition-v2: first selection results

> **Status:** Archived; retained for historical evidence only.
> Captured 2026-10-06. Component measurement checkpoint; S7/S9 remain incomplete.

Four matched pairs completed, with one sample per case/arm. Every arm passed
the native content-page cold attestation, independent functional verifier,
checked cleanup and absolute budgets. Roots match in each pair. The joint gate
passes only for Disposable100; Durable100, Durable1000 and Disposable1000 retain
relative-speed FAIL. There was no build/test/operation/verifier timeout, refusal
or uncertain outcome in this selection. No failed performance arm was replayed.

The owner-directed prospective [registration](../../../../docs/roadmap/0.1/0.1.7/namespace-init-acquisition-20261006.md)
sets 30 s complete performance, 19 s independent verification and 30 s build caps.
Old acquisition-v1 cases retain their 15/9.5 s limits and zero samples. The
registered workload, cache, correctness, allocation and 1.10× relative-speed gates
are unchanged. Increasing the wall allowance did not remove a speed failure.

## Exact identity and route

Candidate source/selection: `45e2b09e8db73dff7303fa48948556b5821727bc`, tree
`caa86556fc24065bc50ac29c5d889840e6177dd7`. A3 product source remains
`0d84badef97f468a7269f9991ab920a8f7077a83`, receipt `e517ae72bcccbc2587585fc0f28f2930c2b1da36`.
Reference: `7edddbdb8e8512627aed0ed42533ef099d802384`. Both measurement
checkouts were clean before/after. Product Rust/schema/algorithm did not change.

The actual candidate route is public `layerfs_project::init` through the release
`benchmark_init` example, with opt-in Monolithic Store schema4 acquisition rows.
The reference is its pinned public Service Init through the existing diagnostic
wrapper. Reference uses Phase4.5 split Storage/history MEMORY/OFF; candidate
Durable uses WAL/FULL/fullfsync and Disposable uses MEMORY/OFF. This inherited
profile/layout difference remains part of the selected comparison. These are not
measurements of the retained SDK route, native FUSE, assembled runtime or the
discarded uncommitted SQLite prototype. Server/daemon/transport timings are N/A.

[Ledger](checks/init-acquisition-benchmark-20261006/ledger.json) preserves every
case, raw ns/B, arithmetic, source/product/shipped-SQL/compilation/dependency/root
ARM config/harness/driver/helper/binary/fixture seal, settings readback, sampled
oracle scope and per-arm receipt link. [Commands](checks/init-acquisition-benchmark-20261006/commands/)
record exact sole-runner commands and explicit120 s outer stops; internal caps
remain30/19/30 s. Builds use `cargo +1.85.1 --release --locked`, root ARM64 config,
worktree-local targets, `LAYERFS_CONSTRUCTION_WORKERS=1` and Init’s supported four
construction workers. All eight builds passed; first reference/candidate builds
took10929149791/7830951459 ns. No CI/aggregate gate or push/release/deploy ran.

## Results and arithmetic

Headline below is complete **product** ns: fresh Store creation/open, real Init,
required checkpoint/allocation release and final close. It contains the nested
Init clock; do not add them. Performance wall also pays native preconditioning/
attestation and child lifecycle. Verification is a separate functional proof.

| Case suffix / profile | Reference product ns | Candidate product ns | Candidate/reference | Speed gate | Reference → candidate allocated B | Storage gate | Joint |
| --- | ---: | ---: | ---: | --- | ---: | --- | --- |
| Durable100 | 41992917 | 97931000 | 2.332084× | FAIL | 7372800 → 5300224 | PASS | FAIL |
| Disposable100 | 39485000 | 40476833 | 1.025119× | PASS | 7372800 → 5279744 | PASS | PASS |
| Durable1000 | 119535750 | 225763292 | 1.888668× | FAIL | 23101440 → 20934656 | PASS | FAIL |
| Disposable1000 | 124912209 | 152242291 | 1.218794× | FAIL | 23101440 → 20901888 | PASS | FAIL |

Speed rule: `10*candidate_product_ns <= 11*reference_product_ns`. Allocation
rule: candidate final main/WAL/SHM allocation <= matched reference final total.
Durable100/1000 are133.208%/88.867% slower, with28.111%/9.379% less allocation.
Disposable100/1000 are2.512%/21.879% slower, with28.389%/9.521% less allocation.
These single observations are not medians or stability claims.

| Case / arm | Complete performance wall ns /30 s | Separate proof ns /19 s | Nested Init ns | Driver CPU ns | Driver lifetime RSS B |
| --- | ---: | ---: | ---: | ---: | ---: |
| Durable100 / baseline | 2562000958 | 982013584 | 34561833 | 53119000 | 37961728 |
| Durable100 / candidate | 1171362375 | 1483092375 | 79558875 | 71841000 | 40091648 |
| Disposable100 / baseline | 62649833 | 33305875 | 33464791 | 50446000 | 37830656 |
| Disposable100 / candidate | 66795709 | 18013333 | 35213334 | 53925000 | 40009728 |
| Durable1000 / baseline | 190977542 | 51196125 | 113674875 | 183334000 | 48332800 |
| Durable1000 / candidate | 323608583 | 33846167 | 211198042 | 238376000 | 55066624 |
| Disposable1000 / baseline | 225560042 | 50882000 | 118000209 | 187558000 | 48463872 |
| Disposable1000 / candidate | 257110833 | 29187083 | 146131875 | 224891000 | 51101696 |

All source regular-file content pages attest resident_after=0 before each arm.
Filesystem metadata residency is unobserved. The independent verifier inventories
all102/1011 paths, including2/11 directories, and reads53 files/3354003 B or
70 files/6430827 B from5 MB/20 MB roots. It is a deterministic sampled payload
oracle, not a full-content or S9 full-root oracle. Verification wall is not a
cold-read speed result. RSS is Darwin wait4 per-child lifetime, not phase residency
or whole-system memory. Final allocation is not a disk/WAL high-water mark.

## Count evidence and performance follow-up

| Candidate | SQL statements | VM steps | Fullscan steps / sorts / autoindex / reprepares | Write commits | Statement ns / commit ns | Acquisition read / write units | Backing rows / B |
| --- | ---: | ---: | --- | ---: | ---: | --- | --- |
| Durable100 | 558 | 77366 | 90 / 16 / 0 / 3 | 18 | 63153616 / 48825037 | 12 / 5 | 202 / 30755 |
| Disposable100 | 563 | 77437 | 90 / 16 / 0 / 3 | 19 | 14946416 / 5123293 | 12 / 5 | 202 / 30755 |
| Durable1000 | 3513 | 595420 | 90 / 16 / 0 / 3 | 25 | 132147862 / 83562622 | 30 / 5 | 2011 / 306563 |
| Disposable1000 | 3506 | 595260 | 90 / 16 / 0 / 3 | 25 | 67842544 / 24210964 | 30 / 5 | 2011 / 306563 |

Statement/commit spans overlap; never sum them as independent costs. In Durable
runs the commit time occupies48825037/83562622 ns of97931000/225763292 ns product
time. This attributes a substantial durability term without isolating causality
against the MEMORY/OFF reference. Disposable1000 also fails, so durability alone
cannot explain the remaining acceptance gap. Source/SQL attribution must use the
existing provider EXPLAIN/runtime profiles and retained traces before any source
change. No further timing run of the unchanged arm is justified.

The two small tiers show fixed90 fullscan steps/16 sorts, zero autoindex rows,
three whole-Store reprepares, and12→30 acquisition read units for100→1000 files.
This is count evidence for these roots, not a huge-root complexity, journal/page
I/O, residency, sustained-rate or S7 cost-gate acceptance. The256 MiB daemon
reservation, high-water/freelist/range allocation, queues and reclaim debt are
outside this direct component route and remain explicit E2–E4 requirements.

## Custody, unrun selections and stopping handoff

[Setup reuse](checks/init-acquisition-benchmark-20261006/setup-reuse-20261006.json)
records ordinary byte copies of closed worktree-local Cargo caches and seed1
prepared masters, with input hash manifests and metadata validation. Cargo
validated/rebuilt final sealed binaries; no foreign target, APFS clone, mutated
sample or old result supplied a measurement. Native cold attestation ran anew
for every arm. [Host/closure](checks/init-acquisition-benchmark-20261006/host-closure-20261006.json)
records macOS26.4.1/ARM64 M3 Max/38654705664 B RAM. Four unrelated running
containers remained untouched and declared interference. Source/process traces
are in each raw receipt.

[Closed-copy manifest](checks/init-acquisition-benchmark-20261006/closed-copy-manifest.json)
verifies the independent compact receipt copies. Complete closed raw outputs,
SQLite files and sealed driver/verifier binaries were copied by ordinary bytes
to primary `benchmark-results/fs-bench-pro/` with every runner manifest hash
verified. Original raw outputs and prepared/build custody remain in the attached
`/Users/yifanxu/.codex/worktrees/namespace-init-benchmark/layerfs` measurement checkout.
No measurement worktree or unrelated container was removed.

| Exact remaining acquisition-v2 case | Arms | Status / samples |
| --- | --- | --- |
| `phase7-sqlite-init-10000-acquisition-v2` | baseline, candidate | NOT_RUN /0 |
| `phase7-sqlite-init-100000-acquisition-v2` | baseline, candidate | NOT_RUN /0 |
| `phase7-sqlite-disposable-init-10000-acquisition-v2` | baseline, candidate | NOT_RUN /0 |
| `phase7-sqlite-disposable-init-100000-acquisition-v2` | baseline, candidate | NOT_RUN /0 |

All eight acquisition-v1 identities remain NOT_RUN/0 at their original limits.
The32 retired run-backed cases and all earlier failures retain their old identities
and verdicts. Larger tiers cannot be credited by these small-tier results.

Next ready work: diagnose the retained Disposable1000 and Durable count/cost
traces, then make an actual source correction with its own registered affected
checkpoint. Independent R1 (host supervisor/consumer attachment) and E1 (complete
S7/S9 workloads/observers) are ready after A3. R2–R4, E2–E4, Q1 and C1 remain
open. A1–A3 are complete checkpoints, not S9 completion. This benchmark scope
stops before R1/R2 changes and does not mark S7/S8/S9 complete or relax P3/P6/P7/
P13/P14 later Commit prerequisites. The closed S5/S6 handoff and both unrelated
untracked owner documents remain untouched.

Production LOC for budget commit45e2b09e8: core93991→93991 (+0), reference65417→
65417 (+0), combined159408→159408 (+0); exact first-parent/staged/committed tree
verified in [receipt](checks/init-acquisition-benchmark-20261006/budget-committed-loc.json).
The results/audit commit has the same totals and delta+0, counted from its exact
first parent and final staged tree by unchanged `tools/production_loc.py` SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`; final tree
verification and exact commit identity are recorded in its commit message and
tracker receipt. Scope: core/crates+root crates including shipped SQL, production
only; tests/inline tests/examples/harnesses/tools/docs/manifests/builds/third-party
excluded. There is no relocation, duplication or reference retirement here.
