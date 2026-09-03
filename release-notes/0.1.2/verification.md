# LayerFS 0.1.2 verification

> **Status:** Released v0.1.2 verification record; source identities and scope are explicit.

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
| Namespace refresh | 12 performance samples; 4 separate passing proofs on `e978edd1` |
| Store-footprint refresh | 9 performance samples; 3 separate passing proofs on `e978edd1` |
| Release native checks | 237 passed, 0 failed, 1 pre-existing ignored test; fmt/Clippy/diff checks passed on unchanged `e978edd1` production sources |
| Publication | Authorized through #12 after exact-source final checks |

The [evidence selector](sdk-edit-evidence.json) and
[final report](sdk-edit-benchmark-results.md)
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
pooled into timing distributions. Issue #20 closed after final validation and
push. Issue #12 separately owns release finalization. The
[supporting benchmark tables](supporting-benchmarks.md) record the refresh.
The daemon close-ACK fix has a failing-before/passing-after Linux regression,
11 passing Linux daemon tests, Linux Clippy, and four passing real TCP/FUSE
verifier runs at 10/500 MiB.

The original SDK selector certifies its recorded source, not an assertion that
later release code is byte-identical. The production delta for release is the
daemon close-order fix; new runner modes only separate supporting-family
collection from verification. SDK edit/Commit implementation and original raw
measurements remain unchanged. Final native checks and GitHub CI bind the later
source; the annotated tag identifies the published documentation-complete tree.

Release native logs are retained in
`benchmark-results/fs-bench-pro/release-v012/final-gates/`. Source `e978edd1`
also passed [GitHub CI](https://github.com/Ephemeral-AI-Lab/layerfs/actions/runs/33813242849).
The final documentation-complete tag must pass its own CI before publication.
The benchmark-data release asset includes selected raw streams and the release
evidence index; it excludes private machine-identifying host logs and large
input/output databases.
