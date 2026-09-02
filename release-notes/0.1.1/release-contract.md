# LayerFS 0.1.1 release contract

> **Status:** Released Developer Preview contract, normative under `v0.1.1`.

## Identity

| Identity | Release value |
| --- | --- |
| Git tag | Annotated `v0.1.1` |
| Git commit | The commit resolved by `v0.1.1^{commit}` |
| Source archives | `layerfs-0.1.1.tar.gz` and `layerfs-0.1.1.zip` |
| `Cargo.lock` SHA-256 | Recorded in release asset `SHA256SUMS` |
| Checksum manifest | Release asset `SHA256SUMS` |
| CI verification | Required successful `ci` workflow check on the tagged commit |

## Preserved public contract

LayerFS 0.1.1 must retain the 0.1.0 durable/ephemeral boundary, canonical
identity, five-table Store schema, visibility-last publication, explicit
Commit and End behavior, stale-Workspace reconciliation, bounded public
operations, daemon compatibility, and materialized/FUSE logical equivalence.
The normative release details are in the
[versioned manual](../../docs/versioned/0.1.1/README.md).

The release may optimize initialization, object admission, localized Commit,
Workspace bootstrap, and reads only without changing those public contracts.
An incompatible representation, Store, SDK, CLI, daemon, or projection change
belongs to a later compatibility line.

## Acceptance

The release is accepted only when:

1. the workspace version and lockfile identify `0.1.1`;
2. all checks in [verification.md](verification.md) pass against one clean
   immutable source commit;
3. namespace and registered payload evidence record their exact terminal
   source seals and meet their binding gates;
4. limitations and documentation agree with the implementation;
5. source artifacts and `SHA256SUMS` are generated and verified; and
6. the annotated tag resolves to the verified commit.

No development benchmark or earlier source seal substitutes for the terminal
release verification recorded for 0.1.1.
