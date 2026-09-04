# Atomic sampler live integration qualification

The coordinator recollected only `tiny-stat-1`, seed 1, performance after its original mandatory observation ended with an incomplete row. The new [attempt `f95ef696b6f6`](../../attempts/tiny-stat-1-s1-performance-f95ef696b6f6/outcome.json) used sealed source `b8c2ad4bf4fa0415fd49d57abea15729b33a4284`. Its [live validator](../atomic-sampler-live-validation.json) returned `issues=[]` and `violations=[]`. Product exit was 0; supervisor and mutable-sample cleanup passed.

The validator retained 10 cgroup samples: first 1000 ns, last 100209834 ns, maximum observed gap 12553250 ns, and required dispatch window 101583541 ns. Its causal sampling rule passed; these are not exact continuous peaks. Observed swap, OOM and OOM-kill counters were zero. The selected sample command wall was 2220685750 ns, inner workload 2393542 ns, and pure public-call sum 22887126 ns; those scopes must remain distinct.

The [b8 build manifest](../../assets-b8c2ad4b/evidence/build.json) retains corrected product seal `e24867af45d83c455dbfac530d43140fec7cdc40d3eae9ff70a30883d239125a`. Compared with the [fbf build](../../assets-fbf32e84/evidence/build.json), daemon SHA-256 `5c466b40a320668793326a5f042f7985bd958971b35a5112ac9d045005e78ed7` and FUSE SHA-256 `8b5e809bb999a205e15df6c8144322e22d749e9ad7db0569e6e513a71988b37c` are unchanged. The host verifier and workload-helper/image identities changed and remain separately sealed. The [model and platform contract](platform-contract.md) explain complete-row assembly and the Linux 4096-byte blocking-pipe limit.

## Scoped retention

The reviewed [build-selection ledger](../../evidence-builds.json) retains completed fbf performance for payload, tiny-file churn and directory traversal, except the explicitly invalidated `tiny-stat-1:1:performance` slot replaced by b8. It retains the already-passing fbf `payload-create-1m:1:verify` and `tiny-bulk-delete-500:1:verify` slots under their original identities. Other current runs use their selected build. The [bridge qualification](../report-slot-sampler-bridge/result.json) passed; the compatibility comparison excludes only the separately hashed `sample_resources` function body while requiring its signature and surrounding registry, timed workload, family, generator and expected-state sources to match. It does not authorize reuse after changes to product semantics, timing, fixtures, or independent expected results.

The original [truncated attempt](../../attempts/tiny-stat-1-s1-performance-2babc4ee0210/outcome.json) remains a raw product PASS with invalid mandatory observation. Its [invalidation](../../invalidations.jsonl), partial-row artifact and model rejection are preserved; neither the bridge nor the replacement relabels it as passing observation evidence. This integration receipt qualifies the sampler and one selected slot, not full-family verification or `PHASE1_TERMINAL_PASS`.

Read-only documentation snapshot SHA-256 values:

- Build manifest: `96491b3828644c5c69013afac58ce7da832969fdafe39d90ff1b17d4c50b3014`.
- Live validation: `42ce05f9cacf5e236672690851ee7b38695837a48c342c19f1b2b2e25f017c50`.
- Build-selection ledger at this review: `e951a6eacc8cdbbba8f6a486de6fc19efcb761c91847bc1bfb78f958fea753b1`. The ledger can gain separately reviewed future selections; this hash identifies the snapshot described here.

No runtime code, tests, builds, or measurements were performed while writing this integration note. Publication should retain the selected b8 attempt package, original invalid observation, build identities, validator and bridge receipts; omit live campaign streams, mutable Stores/caches, binaries, workload payloads and unrelated user files.
