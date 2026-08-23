# G5-1 final synthesis

Terminal result: **G5-1 REVISE**.

The final G4 authority is reconciled without relabeling sealed v12. G5's prospective dual latency materiality is frozen. Primary-source Xet research changes the expected narrative materially: the effective rule is 3–9 with unary tails, its open-summary logarithmic claims are distribution-dependent, a real zero-file input defeats both dirty-window and summary compactness, and its remote carrier/index architecture is not a local LayerFS solution.

LayerFS should adopt Xet's architectural separations, immutable-data-first publication ordering, exact vectors, and GC/index consistency rules. It may later shadow CD32–64 metadata grouping as an ordinary-case research arm. It must reject exact Xet grouping as a qualifying mapping profile, reject hard-`O(log N)` claims without caps/fallback, and reject xorbs/shards/global indexes/compression/network concurrency for the local core.

H11 provides the first new G5 measurement. V1 is preserved as an analyzer/protocol `REVISE`; v2's sealed analyzers returned PASS in 8.551 s. Current graph size and instrumented current-root work did not grow with 1,000 retained revisions; a hash-bound post-terminal audit confirmed nested operation fields omitted from both analyzers. Storage grew by six objects, 23,030 canonical bytes, 2,255 mapping bytes, and about 24.9 KiB of SQLite image per unique revision—not by 1 MiB per revision. These are useful diagnostics, but final audit found v2's whole-harness Q invalid: large expectation, reachability, timing, and report allocations were never charged, reachability high-water was omitted, and zero was emitted literally while allocations remained live. Timed reopen additionally has unmetered SQLite preflight/open work and sets the 1,500-page cache only after open.

The exact lane results are:

```text
G5-A = RETAIN_FULL_REOPEN_AUTHENTICATION
G5-B = RETAIN_K64_F64
G5-C = H11_REVISE_EXACT_BLOCKER
G5-D = RETAIN_CURRENT_SQLITE_PROFILE
```

The only selected G5-2 starting action is to freeze a corrected H11 protocol that charges every benchmark-owned allocation, returns reachability/report high-water, proves zero after dropping all capacity, consumes or removes the operation log, emits independently recomputable historical tuples, and strengthens lock ownership/custody. A broader G5-C concurrency/GC gate cannot start first.

## Limitations

- H11 v2 is one deterministic 1-MiB diagnostic with two fixed samples; its analyzer PASS is superseded by the final Q audit, it makes no population-level latency claim, and it does not replace 100-MiB G4 controls.
- It retains roots but does not delete through GC, exercise concurrency, or prove branch/revert/cancellation/capacity behavior.
- Cold state, VFS/main/journal byte I/O, sync-call wall, continuous allocation peak, and stable-media attribution remain `Unavailable`.
- Xet's managed production server outside public mirrored/current source was unavailable; no claim is made about undisclosed implementation.
- Historical verification is source-bound inside the binary; raw/analyzers do not emit/recompute historical tuples, and the hash-bound operation log is not consumed.
- The lock is presently absent, but the v2 runner closed its lock descriptor and did not verify inode/token ownership before unlink; its cleanup artifact predates release.
- The v2 method manifest binds the exact executable, fixture, method code, and two included G4 benchmark sources, but not the full `layerfs-core` dependency tree/build provenance; final audit found no core diff at the checkpoint.
- Primary and independent v2 analyzers are separate but structurally similar wrappers over their corresponding v1 computations; agreement should not be read as strong implementation diversity.
- The runner fsynced raw, manifest, terminal verification, and final inventory, but did not individually fsync every referenced analyzer/environment/cleanup artifact or the result directory; custody is weaker than the final G4 practice.
- The H11 result roots are access-restricted and hash-inventoried, not claimed to have filesystem immutable flags.

No G5-2 implementation, G5-B canonical tree, G5-D candidate, G6/WP5, VFS/SDK/application integration, production profile change, commit, or sibling-worktree change occurred.
