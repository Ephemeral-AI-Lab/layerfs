# S3 — C2 cost, public diagnostic levers, and archival evidence

> Status: Research; informative and not a product contract.

Investigation for #190. Source pin: `9f35c49ad62956f131dc2676787f99d69659686e`; file:line references address that snapshot. No product changes, builds, live samples, stride1 runs, or commits were made by S3. Archival log inspection is not a new measurement. `null` below means unmeasured, never zero.

## Product identity check

`git diff 795fb1a2f792742a2b19fdb82d98e6fd8a8b0470 9f35c49ad62956f131dc2676787f99d69659686e -- core/crates` is empty. Thus the current pooled StoreProvider session and decoded GroupCache were already in the historical source commit; they must not be described as intervening product optimizations. This proves source equality for that scope, not that the volatile historical executable was built from it. Historical binary/compilation custody is still required to compare a new instrumented run causally to the retained 32,566,067,669 ns.

A misleading source comment matters here. `core/crates/layerfs-storage/src/encoding/codec.rs:20` says payload level 9, and the prose above `PAYLOAD_LEVEL` describes level 9. The executable constants are **PAYLOAD_LEVEL = 3** (`:94`), **GROUP_LEVEL = 19** (`:102`), group window log maximum 16 (`:104`), and encode workspace 16,777,216 B (`:78`). Reading the comment as the active payload configuration would misattribute the old +7.6 s combined-codec experiment to this lane.

## Lever status and measured effect

| Hypothesis / lever | Live stride10 matched delta: operation / codec CPU / RSS / Store bytes | Admissible evidence now | Disposition |
| --- | --- | --- | --- |
| H3 group level 1 → 19 | `null / null / null / null` | Archival offline group recompression logs below: byte delta −262,222 B; separate CPU deltas +1,011,835,000 ns and +1,425,492,000 ns. Neither is a live-save operation delta. | **NOT_RUN** as a new matched live pair: private constant, no public override, product edits forbidden. |
| H3 workspace 2 → 16 MiB | `null / null / null / null` | Source capacity delta is +14,680,064 B, not measured RSS. Earlier combined payload/group experiment is confounded. | **NOT_RUN**: private allocation policy would require product change; lowering it can refuse codec requests. |
| H4 one worker → reference runtime default | `null / null / null / null` | Raw history driver loops serially; searched current C1/C2 and driver for worker/env/thread controls. No LAYERFS_CONSTRUCTION_WORKERS reader in this path. | **NOT_RUN**: changing the environment alone is an inert treatment; adding parallel construction changes driver behavior and violates the campaign's single-producer rule. S4 owns historical worker reconstruction. |
| H5 persisted similarity index on/off | `null / null / null / null` | Bounded 8,192-slot Store-owned ring with signature lookup and flush-on-finish. No public disable switch. Delta counters record selection outcomes, not index CPU. | **NOT_RUN**: no isolated public index lever; changing depth or predecessor declarations changes other work and representations. |
| H5 chunk predecessor cursor on/off | `null / null / null / null` | `LAYERFS_HISTORY_CHUNK_PREDECESSORS` is a real harness lever, but also changes which chunk delta candidates are offered and changes saved bytes/selection work. | **NOT_RUN by S3**: authorized scope here was source/archival only. This lever can test the entire producer treatment, not isolate index CPU. |
| H6 save/transaction cadence | `null / null / null / null` | SaveOutcome.commits and pack_appends are public and can be recorded by S1. Whole-pack byte charge exists in source. Charged cumulative transaction bytes are not exposed. | **NOT_RUN** as a cadence-changing pair; collect observational counters without changing cadence. |

No hypothesis in this table is numerically falsified on the live retained-history lane by S3. Unsupported mechanisms are left unmeasured. The source establishes which experiments can be run faithfully, not their time effect.

## H3: retained archival recompression, exact arithmetic and caveats

Copied during #190 from volatile `/tmp/w1/` and `/tmp/w1codec/` into [archival-w1](archival-w1/): the two full logs, their shell scripts, the observed scratch tool source, Cargo manifest and lockfile. [custody.json](archival-w1/custody.json) records origin, retained filename, length and SHA-256 for every retained file, plus current hashes of original group inputs and the observed executable. Inputs/executable were not copied. These capture-time hashes do **not** establish historical generation identity; no corresponding sealed historical source/compilation receipt was found in these files. Keep these rows **ARCHIVAL_UNMATCHED**.

The scratch tool explicitly reads group bodies before the per-level CPU measurement; its `groups` loop uses `CLOCK_PROCESS_CPUTIME_ID`, group zstd parameters with a window cap of 16, and a dynamic compression context. It is a codec microbenchmark, not the static-context live Store save with candidate search, pack writes and transaction boundaries. See retained `main.rs:11`, `:60`, `:359`, `:375`. Repeated historical logs remain separate; no mean, median or best-of selection was taken.

| Retained raw log / group level | Sum codec CPU ns | Sum stored_total B | Sum frame_total B | Peak RSS |
| --- | ---: | ---: | ---: | --- |
| armA_sweep.txt / 1 | 18,010,000 | 3,071,158 | 3,042,617 | null |
| armA_sweep.txt / 19 | 1,029,845,000 | 2,808,936 | 2,780,395 | null |
| **Within-log delta 19 − 1** | **+1,011,835,000** | **−262,222** | **−262,222** | null |
| rest2.txt / 1 | 23,713,000 | 3,071,158 | 3,042,617 | null |
| rest2.txt / 19 | 1,449,205,000 | 2,808,936 | 2,780,395 | null |
| **Within-log delta 19 − 1** | **+1,425,492,000** | **−262,222** | **−262,222** | null |

Derivation: for each log add the ordinary and pooled rows for level 1 and level 19, then subtract. For armA CPU: `(377332000 + 652513000) − (5332000 + 12678000) = 1011835000 ns`. For rest2 CPU: `(550665000 + 898540000) − (7146000 + 16567000) = 1425492000 ns`. [derived.json](archival-w1/derived.json) retains the exact sums.

The earlier W1 report §7 prints corrected frame totals 3,042,214 and 2,779,992 B, 403 B below the preserved raw log at both levels. The report attributes the difference to raw groups; that correction is not in the retained raw log. We retain both observations rather than silently replacing one. Its **−262,222 B delta** is reproduced because the 403 B discrepancy cancels. The report's rounded +1.012 s is consistent with armA's raw +1,011,835,000 ns; the other preserved historical log gives a different +1,425,492,000 ns. Neither proves that group compression explains that amount of the current 21,195,067,669 ns excess.

Earlier W1 report §8/§15 compares payload level 3/group 1 to payload 9/group 19 and changes encode workspace. It explicitly declines wall-time use because the machine was not quiet. Its claimed +7.6 s codec CPU and +16,089,088 B peak RSS concern a **combined earlier treatment**. They cannot isolate GROUP_LEVEL, cannot be transplanted into today's payload-3 product, and are not used as a live matched effect here. The W1 report and recheck's separate pooled-lane-cold pair are historical supporting context, not fresh #190 measurements.

## H5: persisted index and chunk-cursor mechanisms

- `core/crates/layerfs-storage/src/encoding/delta/candidates.rs:205` creates a fixed ring and direct-reference array; `:230` loads at most 8,192 persisted rows. `:313` inserts an admitted FULL winner, evicts a ring slot and updates its signature references. `:388` searches up to the eight query hashes and computes bounded overlap. This is not an all-objects scan proportional to the retained repository size.
- `core/crates/layerfs-storage/src/cas/lifecycle.rs:61` only reloads an invalidated index; it does not reread the entire table at every successful state. At final publication, `:206` calls `Candidates::flush`; `candidates.rs:353` writes only ring insertions since the last flush, bounded by the ring size. A claim of “entire persisted index reload per state” is contradicted by the successful-path source.
- `core/crates/layerfs-storage/src/encoding/delta/select.rs:270` constructs a FULL candidate before choosing a delta; `:295` checks declared advisory candidates, falling back to the index only when acquisition supplies none (`:298`). Native chunks use their declared first candidate (`:288`). Trial, ineligible and work-exceeded counters count outcomes, not CPU spent in hashing, searching, reading bases or encoding trials.
- `LAYERFS_HISTORY_DECLARED_ONLY` changes the harness's own optional similarity source (`core/benchmark/fs-bench-pro-storage-content/src/ops/history.rs:199`). It does **not** disable the product's persisted candidate index. Using it as an index-off arm would mislabel the treatment.
- The chunk predecessor switch is real (`history.rs:289`), but its StoreProvider mapping reads happen within content construction (`:1583`, `:1631`). C1's whole-file path ignores that base; only chunked construction uses it (`core/crates/layerfs-content/src/file/content.rs:249`, `:268`). A cursor toggle therefore changes candidate availability, encoding results and base reads together. Its operation delta would price that combined producer behavior, not pure index overhead.

A useful legal observation is S1's separate content/tree/accept/finish time plus SaveOutcome.delta, chain and pool counters per state. Pure codec/index CPU remains unavailable from those coarse APIs. Sampling/profile evidence with attributable codec stacks or product-supported telemetry would be needed to split it; no unapproved profiler or product instrumentation was run by S3.

## H6: transaction cadence and accounting boundary

`core/crates/layerfs-storage/src/cas/placement.rs:175` inserts or appends the selected pack; `:200` charges **the full selected pack byte length** into `transaction.bytes`. It also charges member canonical lengths (`:169`); pooled group body writes add another source of charged bytes (`cas/pool_lane.rs:268`). Therefore this counter is mixed canonical plus physical write volume, not merely newly appended bytes.

`cas/lifecycle.rs:129` commits when transaction rows reach 8,191 or bytes reach 4,194,303, then begins a new transaction and resets its counters. `:217` acknowledges the final watermark transaction. These capacities derive from fixed constants (`policy.rs:75`, `:77`, `:327`), and Store policy's public constructor offers construction/depth fields (`policy.rs:189`), not codec level, index disablement, workers or transaction cadence.

`SaveOperation::accept` can flush and write batches before finish (`cas/store.rs:383`, `:417`); `finish` drains the remainder (`:424`). Consequently a short `storage.finish` does not bound total codec/index/transaction cost. SaveOutcome exposes `commits`, `pack_appends`, `inserted`, `statements`, `presence_queries`, `delta`, `chain`, `pool` (`cas/store.rs:30`). It does not expose cumulative charged transaction bytes or each boundary's byte value. Reconstructing those from final Store pack sizes is invalid because earlier pack versions were rewritten and are not present in that final file.

The handoff's “about 28 objects per commit” is **not measured by S3**. A state-level `inserted / commits` ratio from S1 would be an average over boundaries, not proof of an invariant 28-object batch or per-commit histogram. Establishing full rewrite amplification needs the sequence of selected writes, their lengths and boundary positions; public final counters alone do not recover it. No capacity was raised, policy relaxed or alternative SQL write path introduced.

## What is falsified, and what remains open

Source contradicts four proposed shortcuts: payload level 9 is not today's constant; changing the workers environment alone does not parallelize the current driver; DECLARED_ONLY does not turn off the persisted index; finish-only time is not full save time. These are source/boundary findings, not performance falsifications.

The live H3/H4/H5/H6 timing deltas remain **NOT_RUN** or **unmeasured**. Historical codec recompression demonstrates a cost/size tradeoff on that earlier input population, with exact retained numbers and unequal historical CPU deltas, but does not close the current comparison. The current tree/save subdivision belongs to S1. Attribution against v0.1.6 additionally requires S4's boundary and worker reconstruction and the unmatched-comparison constraints in S5.
