# LayerFS 0.1.2 verification

> **Status:** Unreleased. Terminal benchmark validation is complete; the final repository-gate receipt and admission state are pinned by the evidence selector.

| Gate | Evidence/status |
| --- | --- |
| Three SDK-only registries | 12 + 32 + 12 = 56 exact IDs |
| Performance | 560 rows, five repetitions per ID per arm |
| Separate correctness verification | 56 aggregates, 112 passing source subproofs |
| Latency/parity | Approved 20/20/30 ms ceilings and exactly three disclosed Edit exceptions |
| Route/no-amplification | Frozen static manifests and per-row runtime tripwires pass |
| Memory | Sampled ack-window-v1 scope plus native lifetime bounds; no exact-phase claim |
| Cleanup/custody | Native container exits checked; scratch recovery and sealed originals retained |
| Final repository commands | Exact revision, results and manifest in selector below |
| Publication | Not performed or authorized; #12 remains open |

The [evidence selector](sdk-edit-evidence.json) and
[final report](../../benchmark-results/fs-bench-pro/sdk-edit-terminal/final-3337728e/report.md)
are authoritative. A benchmark-only pass is not final admission: the consumer's
--final --check requires all four repository commands to pass, intact evidence
manifests, the approved contract hash, unchanged compiled sources, and a clean
documentation/evidence-only descendant checkout.

```bash
cargo fmt --all -- --check
cargo test --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
git diff --check
python3 -B benchmark-results/fs-bench-pro/sdk-edit-terminal/final-3337728e/consumer.py --self-check
python3 -B benchmark-results/fs-bench-pro/sdk-edit-terminal/final-3337728e/consumer.py --final --check
```

There is one final full repository-gate sequence on the documentation-complete
checkout. Performance is not rerun to trigger verification. Each original
bundle records its commands, compiler/host/image identity, fixture/plan hashes,
sample order, initial and ending source identity, and complete raw manifest.
The final selector records the documentation revision and gate manifest.

The isolated baseline verifier InvalidRequest, six-proof recovery, metadata
cleanup recovery, alias correction, and original strict findings are disclosed
in [benchmark results](benchmark-results.md). No failed attempt was erased or
pooled into timing distributions. Issue #20 closes only after final evidence
validation and push; it does not close #12 or publish v0.1.2.
