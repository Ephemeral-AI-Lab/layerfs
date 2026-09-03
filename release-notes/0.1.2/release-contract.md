# LayerFS 0.1.2 release contract

> **Status:** Released Developer Preview contract under annotated tag `v0.1.2`.

## Identity

| Identity | Release value |
| --- | --- |
| Git tag | Annotated `v0.1.2` |
| Git commit | The commit resolved by `v0.1.2^{commit}` |
| Source archives | `layerfs-0.1.2.tar.gz` and `layerfs-0.1.2.zip` |
| `Cargo.lock` SHA-256 | Recorded in release asset `SHA256SUMS` |
| Checksum manifest | Release asset `SHA256SUMS` |
| CI verification | Required successful `ci` workflow check on the tagged commit |

## Preserved public contract

LayerFS 0.1.2 retains the 0.1.1 durable/ephemeral boundary, canonical
identity, five-table Store schema, visibility-last publication, explicit
Commit and End behavior, stale-Workspace reconciliation, bounded public
operations, daemon compatibility, and materialized/FUSE logical equivalence.
The normative release details are in the
[versioned manual](../../docs/versioned/0.1.2/README.md).

The additive public surface is `WorkspaceFileRangeEdit`,
`WorkspaceFileReplacement::{Inline, Zero}`, the singular/batched Client edit
methods, `WorkspaceCommitStatus`,
`Client::commit_workspace_session_with_status`, and
`Client::recover_workspace_presentation`. The legacy `WorkspaceCommitResult`,
`WorkspaceState`, `OperationFamily`, and `WorkspaceCommitReceipt` shapes remain
exact. A durably published Commit stays authoritative when presentation
recovery is required and must not be retried. Range editing changes
regular-file content/length only. An incompatible
representation, Store, CLI, daemon, or projection change belongs to a later
compatibility line; authenticated physical packs are explicitly deferred to
issue #18.

## Acceptance

The release is accepted only when:

1. the workspace version and lockfile identify `0.1.2`;
2. all checks in [verification.md](verification.md) pass against one clean
   immutable source commit;
3. the edit-family and Store evidence records exact source/product/harness
   identities and meets its binding or explicitly owner-accepted gates;
4. limitations and documentation agree with the implementation;
5. source artifacts and `SHA256SUMS` are generated and verified; and
6. the annotated tag resolves to the verified commit.

No development benchmark or earlier source seal substitutes for the terminal
release verification recorded for 0.1.2.
