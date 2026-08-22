# G4 v9 final static closure

Status: **PASS**
Date: 2026-08-22
Branch / HEAD: `codex/empty-worktree` / `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`

The final source and executable match the candidate measured by the sealed v9 campaign:

| Item | SHA-256 |
|---|---|
| `phase4_create_edit_benchmark.rs` | `eb00674125d18da66253b31949ecba2f874b64ec6a93ad68fe251d4f0649d169` |
| `phase4_g3_materialization.rs` | `32c8185c3cbc5b444ba0a533ea5f1bd9332b16eb358b9c5540c0ab534ac3f8d9` |
| `canonical_v2.rs` | `8fe11085d8b27b1f2a833665b4afd11f6370f3e94821f5022d67ae14cac071dc` |
| `Cargo.lock` | `70c7f1079b6dcff927932d6e0072e5cd169cd2f49ea51c72f7f108d950adb8d8` |
| release candidate | `c60a19cb3cecb83bb801ba9c36835297e6fc503d736171213ec78e69bd5d6d76` |

Before v9 measurement, the four focused `g4_` tests passed, including closure-on/off identity-before-grammar error equivalence, sink-failure Q cleanup, inode-bound/exclusive publication cleanup, directory-sync lost acknowledgement, requested-visible reconciliation, verification-failure cleanup, post-publication substitution preservation, and the integrated batched publisher. `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`, `git diff --check`, and the release build also passed.

After sealed v9 measured PASS, `cargo test --workspace` passed with **161 tests passed, 1 intentionally ignored, 0 failed**. The benchmark binary's full 83-test suite included all four focused G4 tests. All workspace doc-test suites passed. A final `git diff --check` also passed.

No benchmark or measured row was rerun during static closure. No source changed after release candidate custody. G5 was not started, and no commit was created.
