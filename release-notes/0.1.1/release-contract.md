# LayerFS 0.1.1 release-candidate contract

> **Status:** Proposed Developer Preview contract. It becomes normative only
> after terminal verification and publication of `v0.1.1`.

## Identity

| Identity | Candidate value |
| --- | --- |
| Git tag | Pending annotated `v0.1.1` |
| Git commit | Pending clean immutable candidate |
| Source archives | Pending |
| `Cargo.lock` SHA-256 | Pending |
| Checksum manifest | Pending `SHA256SUMS` |
| CI verification | Pending against the tagged commit |

## Preserved public contract

LayerFS 0.1.1 must retain the 0.1.0 durable/ephemeral boundary, canonical
identity, five-table Store schema, visibility-last publication, explicit
Commit and End behavior, stale-Workspace reconciliation, bounded public
operations, daemon compatibility, and materialized/FUSE logical equivalence.
The normative candidate details are in the
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

No development benchmark or earlier source seal substitutes for terminal
release verification.
