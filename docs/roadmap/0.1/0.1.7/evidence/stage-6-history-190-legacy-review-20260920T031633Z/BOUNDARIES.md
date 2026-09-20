# Legacy/current operation boundary review

> Status: Research; informative and not a product contract. Read-only review against current `4391f66d8f39fc4d179caf557176021fc6f2c700`; no build, product edit or new timing sample.

**Another focused source review is useful; treating the historical 11.37 seconds as an attainable, equivalent implementation target is not justified.** There are concrete architectural differences worth checking, but the timer names conceal overlapping work. In particular, the legacy 0.342-second `namespace_ns` is not the counterpart of the current whole filesystem interval.

## Applicable legacy source

`git diff --name-only 7fab1027a HEAD -- crates` reports exactly five additions: three `layerfs-content/examples` files, one `layerfs-content/tests` file and one `layerfs-workspace/tests` file. It reports no change to legacy production source. Current root `crates/` is therefore usable for this source comparison. Whole `crates` trees differ because of those examples/tests: historical `dcc4fb6fd01115dcbf91ba02df414e91eb5733be`, current `498dd1917812ae90efb8841f57e22bfc284e96fb`.

The [retained S4 reconstruction](../stage-6-history-190-20260919T225614Z/squad-s4/V016-COMMIT-COMPOSITION.md) establishes that the actual compiled historical commit is `ac729dfeb4ee923b0a42106a53d1c7de4cd4cfcb`, while the report names `7fab1027a0061e8b932345d4fcd6ac22a089b155`. Their `crates` and historical harness source match. Do not replace the raw compiled identity with the report's reference.

References below use `L:path:line` for historical `7fab1027a0061e8b932345d4fcd6ac22a089b155`, and `C:path:line` for current `4391f66d8f39fc4d179caf557176021fc6f2c700`. Resolve exactly with `git show REV:path | nl -ba`. No citation assumes current line numbers from a different checkout.

## The exact historical comparison remains unresolved

This review independently decompresses the retained `deepseek-stride10--performance.jsonl.gz` and `deepseek-stride10--performance-result.json.gz`, verifies both SHA256 values against S4 `custody.json`, and sums the 17 JSONL Commit rows. The sum reproduces **11,370,679,212 ns**.

The retained successful PR #196 stride10 candidate operation is **22,180,444,124 ns**, yielding:

```text
22,180,444,124 - 11,370,679,212 = 10,809,764,912 ns
22,180,444,124 / 11,370,679,212 = 1.9506701148153014
```

That is descriptive arithmetic over different campaigns, not 10,809,764,912 ns attributed to a product defect. The [PR #196 report](../stage-6-history-190-roi-20260920T014227Z/README.md) retains the local matched pair. Its same executable also had materially different absolute times in earlier campaigns. No cross-campaign subtraction establishes a mechanism's cost.

The [later screen baseline](../stage-6-history-190-screen-20260920T024251Z/README.md), with the same retained algorithm plus a temporary phase scope, measured **22,615,178,250 ns**. Its descriptive difference from the historical Commit is **11,244,499,038 ns**, also unmatched. This later baseline is the campaign's phase-budget source; it is not a new main-branch performance result. Its useful decomposition must remain associated with that run:

| Disjoint measured region | ns |
| --- | ---: |
| Canonical content construction | 1,618,536,750 |
| Filesystem update | 9,536,586,040 |
| Store begin + accept + finish | 11,120,818,665 |
| Remaining state-body work, by subtraction | 339,236,795 |
| Total operation | 22,615,178,250 |

The residual includes separately named harness input/predecessor/index work, Store creation and unnamed state-body time. It is not a product-only timer. The provider interval **8,572,820,583 ns** is nested inside filesystem, not an additional row. No current pure-product interval has an equivalent isolated legacy counterpart.

## What each operation actually does

```text
v0.1.6 (historical host coordinator + sandbox workspace)

 outside Commit                 inside Commit
 ---------------------          ------------------------------------------
 install fixture                capture/fetch frozen workspace changes
 execute importer --------->    content producers --> bounded output queue
   mutate live workspace                                 |
   copy changed payload                             Store admission
   normalize directories        dirty-node loop: directories + metadata
                                reference reduction + inode finish
                                publish + complete/rebase workspace

v0.1.7 retained C1/C2 harness

 outside state child            inside state child
 ---------------------          ------------------------------------------
 corpus read + transition -->   construct canonical content
                                assemble caller filesystem input
                                validate input/base + update filesystem
                                clone buffered objects --> Store accept
                                finish Store + caller bookkeeping
```

The legacy importer reads manifests, skips unchanged payloads, creates/removes bindings, and normalizes final directories (`L:benchmark/fs-bench-pro/workload/storage_smoke.rs:90–156`). Its exec time is outside Commit, but canonical content and canonical namespace construction are inside Commit. The historical timer brackets `commit_workspace_session_with_status` (`L:benchmark/fs-bench-pro/src/storage_smoke.rs:737–739`). Moving current canonical construction outside its timer would not reproduce legacy semantics.

| Difference | Exact source | Meaning for optimization |
| --- | --- | --- |
| Legacy already has live dirty nodes and directory changes before candidate construction | `L:crates/layerfs-workspace/src/changes.rs:666–678,742–799`; importer cited above | A frontier-oriented adapter can provide useful input, but generic C1 still must check the input it accepts. Neither skipping checks nor charging mutable preparation outside an operation establishes reduced total work. |
| Legacy dirty-directory and metadata processing is recorded as **Content** | `L:crates/layerfs-workspace/src/changes.rs:741–850` | Its `content_ns = 9,394,444,501` includes content production/admission plus this loop. Legacy `namespace_ns = 342,355,542` covers only references and inode finalization at `851–877`. Comparing it with all current filesystem work is invalid. |
| Legacy content admission overlaps a bounded producer pipeline | `L:crates/layerfs-layerstack-store/src/objects.rs:369–408,4473–4513`; `L:crates/layerfs-workspace/src/changes.rs:702–722` | Real scheduling/ownership difference. A second thread/helper is not authorized as a way to pass the single-worker lane. Single-thread streaming could reduce buffering/copying, but has no measured one-second benefit. |
| Current build finishes before save begins; objects are cloned into admission | `C:core/benchmark/fs-bench-pro-storage-content/src/ops/history.rs:1930–1968` | Copy/ownership overhead is a possible measurement subject, not the entire 11.121-second Store interval. C2 compression/indexing remain necessary even if copies disappear. |
| Current C1 accepts caller-supplied base, identities, binding updates and values | `C:core/crates/layerfs-content/src/filesystem/validate.rs:120–156,545–565`; `C:core/crates/layerfs-content/src/filesystem/update.rs:157–179` | Authentication, new-ID absence, kind/reference/topology checks are part of an independent component boundary. Reuse already authenticated facts within the allowed bound is legitimate; trusting mutable caller facts without proof changes correctness. |
| Legacy also validates generated checkpoint records | `L:crates/layerfs-workspace/src/changes.rs:862–871` | The difference is not “legacy has no validation.” Their exact checked inputs/guarantees differ. |
| Same corpus does not produce the same canonical object stream | S4 raw final allocation: legacy 51,722 objects / 380,563,155 canonical bytes; original core 52,032 / 380,921,328 | Canonical formats/metadata/identity schemes differ. These totals do not identify the time spent on each difference. Core harness sets `metadata_root: empty_metadata_root()` at `C:core/benchmark/fs-bench-pro-storage-content/src/ops/history.rs:685–692`; legacy reads or builds portable metadata at `L:crates/layerfs-workspace/src/changes.rs:808–828`. |

The original core “base-tree walk” clue therefore requires actual provider and traversal counters, not an inference that legacy whole-tree work is 0.342 seconds. Existing successful lookup batching, authenticated reuse and SQL preparation changes remain intact. The rejected zero-count filter is closed: 156,679,583 ns observed operation difference, only 53,181,962 ns in its target phase, below the revised one-second bar.

## What cannot be credited to an algorithm

- **Workers:** legacy `construction_worker_limit()` allows an environment override and otherwise host parallelism; predecessor plans further restrict workers to one (`L:crates/layerfs-workspace/src/changes.rs:572–582,683–688`). The saved receipt lacks effective per-state worker counts. “Legacy used eight workers” is not established. Current lane uses one; no worker uplift is proposed.
- **Cache:** historical receipt explicitly declares `fresh-store-existing-os-cache-uncontrolled`. Import occurs before Commit; no equal cold proof exists. Current corpus acquisition is outside children and Store growth is inside the chain. Neither run establishes a cold or matched cache comparison.
- **Topology/machine:** historical canonical construction and SQLite run on the host; the sandbox is a container. Its two-CPU limit is not a host producer-count measurement. Machine states differ and historical effective host contention is not recorded.
- **Nested counters:** historical admission, pipeline, content and candidate-finish fields overlap. `candidate_finish_ns` includes both outer build and inner finish accounting. Adding them into a closed decomposition invents work. Historical `encoding_ns` is encoder elapsed time, not codec CPU.
- **Allocation:** the owner now conditionally accepts a small allocation miss for worthwhile time reduction. That allows an explicit tradeoff experiment; it does not make unlike cache/timer boundaries comparable or relax authentication/boundedness. Preserve exact allocation misses.

## Two eligible questions; no blanket optimization campaign

1. **Which remaining provider work actually has a different implementation in legacy?** The current provider costs 8,572,820,583 ns in the screen baseline and has 79,784 pack fetches / 7,378,994,999 pack bytes. A sibling source review should identify an exact repeated acquisition/decode/lookup that legacy avoids, then estimate its reachable share from existing counters before one matched experiment. An opportunity requires preserving authentication, existing bounds and all saved roots; the target is at least one second, not merely fewer logical IDs. The counters do not prove these bytes are disk I/O.
2. **Does the Store-side encoding strategy buy enough speed for an acceptable small storage change?** The current disjoint Store interval is 11,120,818,665 ns, so it is large enough to inspect. Separate compression/physical grouping/index work before selecting one existing lever. Legacy's aggregate 2,423,813,393-ns encoder elapsed field is not directly comparable and cannot establish a 8.697-second codec regression. A candidate must declare time, allocated/apparent bytes, CPU/RSS and correctness together. A new concurrency design or schema change is not justified by this boundary review.

Do not prioritize save streaming merely because its diagram looks shorter. With the measured 1,618,536,750-ns canonical-content interval, overlapping all file construction alone has that interval as its arithmetic ceiling under the simplifying assumption that downstream cost is unchanged; no achievable saving or legal worker configuration is established. More complex ownership/transaction changes need their own measured removable work first.

**Disposition:** stop broad implementation churn, finish this finite legacy comparison, and consider only a concrete candidate supported by at least a one-second removable interval. If the provider/Store reviews cannot identify one, retain the successful implementation and record the historical gap as unresolved. This document supplies source/boundary evidence, not a new experiment or a promise to match 11.370679212 seconds.

## Checks performed

Read-only Git source comparison; independently verified two S4 raw receipt hashes and the 17-state Commit sum; integer arithmetic above; inspected every cited current/legacy scope. No build, performance run, verification rerun, product modification or old receipt mutation. The only owned output is this document; tests are not applicable to this source-only review.

## Adversarial cross-review of the other squads

This section reads [TREE.md](TREE.md) and [STORAGE.md](STORAGE.md), without editing them or running their proposed experiments.

- **TREE negative-result reuse:** real source mechanism, no isolated time attribution. Its 2,794,811,126-ns validation interval and 47,541 read waves include first proofs, positive demands and repeated absence checks. They cannot establish that repeated absences alone contain a one-second opportunity. The author clarified the disposition to an attribution screen before implementation and corrected legacy line citations after review. No product experiment is justified solely by the whole validation budget. Its rejection of subtree-summary work for this run is supported by zero validation directory-page reads and no post-initial-build examined entries.
- **STORAGE lower group level:** suitable as an isolated, minimal treatment under the owner's time/space ruling, but neither archived CPU replay is a live operation delta. The 262,222-byte encoded difference is not a Store allocation forecast. A fresh matched pair must be allowed to fail the one-second bar even if codec CPU drops. No historical result establishes how much of the 11,244,499,038-ns current/historical gap this change could remove.
- **STORAGE selected pack ranges:** a concrete source difference and substantial counted copy volume. The document correctly says that 7,378,994,999 bytes are application copies, not disk reads, and that current full-directory validation must survive. It also recognizes that extra BLOB opens can lose current local pack-cache reuse. A range-read experiment needs attributed acquisition/parse cost; its target cannot be all provider time. A selected-range parser must not be a port of legacy's weaker partial-directory checks.

**Priority disagreement is retained:** STORAGE recommends the one-constant group-level pair first; the coordinator favors selective-range acquisition as the main algorithmic direction, with codec level deferred. This reviewer considers both technically coherent but neither a demonstrated one-second saving. The smallest implementation favors the codec experiment; the strongest counted avoidable-work signal favors the range-read attribution screen. Choosing the latter is a research-priority decision, not a measurement that its expected gain is larger. No new concurrency, global memo, subtree format, or transaction-limit change is supported by this review.

**Subsequent synthesis decision:** after considering current complete-directory validation and the absence of byte offsets in catalogue entries, the coordinator adopted STORAGE's ordering: a live **group-level pair first**, a **pack acquisition/parse screen second**. The paragraph above preserves the original disagreement; it is not the final order. Final read-only review of [SYNTHESIS.md](SYNTHESIS.md) confirms that it distinguishes archived codec CPU from live operation time, preserves the full parser/validation constraints, retains the exact unmatched historical gap, and labels all proposed experiments NOT_RUN. Adoption of the ordering is not approval of a default change or proof of a one-second saving.
