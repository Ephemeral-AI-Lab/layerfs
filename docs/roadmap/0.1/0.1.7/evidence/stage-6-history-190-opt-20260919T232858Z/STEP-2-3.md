> Status: Research; informative and not a product contract.

## Step 2 — bounded lookup batching implemented; Step 3 — bounded record reuse implemented

The revised product change is frozen in `core/crates/layerfs-content/src/filesystem/update.rs`; architecture section5.7 is updated.

**Step2:** use existing `lookup_many` for ordered parent batches, limited by existing `base_read_batch` and the existing4096-demand ceiling. The final metadata-overlay pass also batches missing records. Validated parent order already guarantees uniqueness, so no redundant sort/dedup layer is added.

**Step3:** retain only the final nonempty authenticated parent window for reuse. Earlier omitted records are reread in bounded batches; there is no all-parent cache. Preserve caller metadata precedence, original global reducer insertion order, contents map, validation and existing quotas. At most two record windows coexist. No format, public API, dependency, codec or worker change.

| Shared-leaf test | batch1 | batch3 | batch64 |
|---|---:|---:|---:|
| Baseline omitted metadata |23|17|15|
| Candidate omitted metadata |22|10|6|
| Baseline explicit metadata |18|12|10|
| Candidate explicit metadata |18|9|6|

All4 focused tests pass. **All56 quota outcomes match baseline exactly**, including success roots and error type/limit/actual. The8 baseline-supported cells now have explicit regression assertions. Independent reviewer parsed the logs and confirmed parity. The rejected first implementation remains retained; it was never performance-sampled.

**Production LOC:** changed file 405→454 (delta+49); only this product file changes. This implies core20,116→20,165 (+49), reference65,417 unchanged, combined85,533→85,582 (+49); the full-scope recount follows workspace checks. Counter/method: `tools/production_loc.py`, production-only Rust/runtimeSQL, no tests/docs/harness.

Step4 is running: full core checks, candidate build, then one candidate stride10 and one stride3 diagnostic with the frozen baseline harness. No measured candidate speedup is claimed yet.
