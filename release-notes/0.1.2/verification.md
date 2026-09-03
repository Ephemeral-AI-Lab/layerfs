# LayerFS 0.1.2 verification

> **Status:** Prerelease verification checklist; issue #20 blocks publication.

## Current state

| Gate | Status |
| --- | --- |
| LayerFS 0.1.1 active release | Pass |
| Universal edit engine supporting implementation | Complete |
| Prior POSIX/FUSE edit evidence | Archived; not admission |
| Three SDK-only family registries | Pending |
| 560 terminal performance rows | Pending |
| 56 aggregate verifier receipts / 112 subproofs | Pending |
| Strict latency and parity gates | Pending |
| Zero-amplification and route manifests | Pending |
| Phase-local process RSS and daemon-native cgroup gates | Pending |
| Exact cleanup and evidence custody | Pending |
| Repository format/test/Clippy/diff gates | Pending final candidate |
| v0.1.2 tag and GitHub Release | Absent; must remain absent |

The earlier `c6c14d5a` same/count-changing rows are immutable historical
evidence. Their source, route, partial size matrix, lifetime memory fields, and
admission model do not satisfy issue #20 and cannot be republished as terminal
proof.

After issue #20 closes, parent #12 remains open for a separate
release-finalization decision. Issue #20 completion alone does not authorize a
tag or Release.

## Terminal repository commands

```bash
cargo fmt --all -- --check
cargo test --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```

The exact terminal benchmark commands, environment identity, source seals,
row/receipt cardinalities, and manifest hashes must be added only after the
complete clean-candidate campaign passes.
