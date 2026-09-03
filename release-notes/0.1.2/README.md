# LayerFS 0.1.2 Developer Preview

> **Status:** Released source-only Developer Preview under annotated tag
> `v0.1.2`.

LayerFS 0.1.2 preserves the v0.1.1 storage, CLI, daemon, projection, and
Workspace lifecycle contracts while adding failure-atomic regular-file range
editing through one shared owner-side/FUSE piece engine.

## Release identity

| Field | Value |
| --- | --- |
| Version | `0.1.2` |
| Channel | Developer Preview |
| Git tag | `v0.1.2` |
| Git commit | The commit resolved by `v0.1.2^{commit}` |
| Release date | 2026-09-03 |
| Checksums | [Artifact manifest](artifacts.md) and release asset `SHA256SUMS` |
| Verification | [Terminal release verification](verification.md) |

## Compatibility

The release retains the five-table Store schema, canonical encodings and
identities, CDC profile, public SDK and CLI behavior, daemon/proxy/FUSE
protocol, acknowledgement boundary, and explicit Workspace lifecycle.

## Release documents

- [Versioned manual](../../docs/versioned/0.1.2/README.md)
- [Release contract](release-contract.md)
- [Verification record](verification.md)
- [Benchmark evidence](benchmark-results.md)
- [Limitations](limitations.md)
- [Artifact manifest](artifacts.md)
- [GitHub Release notes](github-release.md)

[LayerFS 0.1.1](../0.1.1/README.md) remains available as the previous release.
