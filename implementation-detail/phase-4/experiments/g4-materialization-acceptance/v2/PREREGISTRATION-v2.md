# Prospective G4 materialization acceptance v2

Status: frozen before any v2 preparation or measured row.

V2 preserves the complete v1 G4 algorithm, 30-record / 50-arm schedule, thresholds, proof, cache policy, complete-wall equation, analyzers, source, and candidate executable byte-for-byte. V1 was rejected before execution because an abbreviated G3 hash was expanded incorrectly. V1 created no result root, invoked no benchmark child, measured no row, and released the global lock. `PRE-EXEC-HISTORY-v1.json` preserves that rejection.

The sole v2 protocol correction is the sealed G3-v13 control hash. The authoritative operand and its sealed `OPERAND-CUSTODY-v13.json` both report:

`535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e`

Frozen custody:

- Repository: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`
- Branch: `codex/empty-worktree`
- Starting HEAD: `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`
- Candidate executable: `a3573879d55f2fcfb031a334ce208102c7c0c78fa21a99339a8d5585187150c6`
- G3 control executable: `535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e`
- Protected-operation control: `5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5`
- Round-1 protected handoff: `8ca584b9e7958ac57e28e994e1e9bd5638b7d1c703ace1693b1b58706da07d00`
- Frozen v1 runner dependency: `9888aab78edb2bb0d8d4f38ea69062829afe493ec199a85ae22e9cdc3d018624`
- Frozen v1 methodology manifest: `d6675db6cd340b12453f1719d55f183d3ef5f2b56cc07b9cac4c4f7b75b3c517`
- Sealed G3 operand custody manifest: `58b652948950ed27e7ceb57c5b156705932e44e9d89724c63e8687f84b782d58`
- Result root: `target/phase4-g4-materialization-acceptance-20260822-v2/`; it must be absent before the atomic fail-fast `target/BENCHMARK_LOCK` attempt and is never reusable.

V2 uses the v1 runner as a hash-verified frozen execution engine. The v2 wrapper changes only the result-root version, v2 methodology manifest, and exact G3 control hash. The schedule/analyzer wrappers verify the frozen v1 schedule, primary analyzer, and independently implemented recomputation hashes before executing them. Result artifacts retain these wrappers and the v2 preregistration/history/dry run.

All v1 frozen meanings remain binding: R0 current closure-on control; same-binary R1 closure-on attribution versus closure-off candidate; separately frozen scalar `g3-fallback-algorithm-control`; batched M0 candidate with inode-bound no-follow exclusive publication; consuming seed read; adjacent sealed/current G3 pairs; adjacent protected guards; exact S1-100 SQL/row/BLOB/authentication shape; 333/400/50/10/10/20-ms gates; 5% direct attribution/degradation gates; 20-MiB RSS; 1-MiB buffers; terminal Q/residue zero; honest cold `Unavailable`; exactly 30 append-only records and 50 arm observations; no reruns; operation-local timer sum below 20 seconds; primary/independent ledger equality; and complete wrapper wall at or below 120 seconds.

The complete v1 specification remains the normative detail and is bound by the v1 runner and methodology-manifest hashes above. No product/source/executable byte changed for v2. Static closure and two fresh final auditors remain required after a measured PASS. G5 remains out of scope and no commit is authorized.
