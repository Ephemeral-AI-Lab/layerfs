# LayerFS 0.1.1 Developer Preview

> **Status:** Developer Preview release record, frozen by the annotated
> `v0.1.1` tag.

LayerFS 0.1.1 preserves the 0.1.0 Developer Preview contract
while making existing-directory initialization bounded and scalable,
localizing small-edit Commit planning, demand-loading Workspace bootstrap
objects, and retaining bounded reads.

## Release identity

| Field | Value |
| --- | --- |
| Version | `0.1.1` |
| Channel | Developer Preview |
| Git tag | `v0.1.1` |
| Git commit | The commit resolved by `v0.1.1^{commit}` |
| Release date | 2026-09-03 |
| Checksums | [Artifact manifest](artifacts.md) and release asset `SHA256SUMS` |
| Verification | [Terminal release verification](verification.md) |

## Compatibility

The release retains the five-table Store schema, canonical encodings and
identities, CDC profile, public SDK and CLI behavior, daemon/proxy/FUSE
protocol, acknowledgement boundary, and explicit Workspace lifecycle.

## Release documents

- [Versioned manual](../../docs/versioned/0.1.1/README.md)
- [Release contract](release-contract.md)
- [Verification record](verification.md)
- [Benchmark evidence](benchmark-results.md)
- [Limitations](limitations.md)
- [Artifact manifest](artifacts.md)
- [GitHub Release notes](github-release.md)

[LayerFS 0.1.0](../0.1.0/README.md) remains available as the previous release.
