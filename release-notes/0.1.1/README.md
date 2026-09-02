# LayerFS 0.1.1 release-candidate record

> **Status:** Release-candidate preparation. No `v0.1.1` tag or GitHub Release
> exists, and this directory is not a published release record.

LayerFS 0.1.1 is intended to preserve the 0.1.0 Developer Preview contract
while making existing-directory initialization bounded and scalable,
localizing small-edit Commit planning, demand-loading Workspace bootstrap
objects, and retaining bounded reads.

## Candidate identity

| Field | Value |
| --- | --- |
| Version | `0.1.1` |
| Channel | Developer Preview release candidate |
| Git tag | Pending; `v0.1.1` does not exist |
| Git commit | Pending clean immutable candidate |
| Release date | Pending |
| Checksums | Pending artifact generation |
| Verification | Terminal benchmarks pass; release identity checks remain [in progress](verification.md) |

## Compatibility intent

The candidate retains the five-table Store schema, canonical encodings and
identities, CDC profile, public SDK and CLI behavior, daemon/proxy/FUSE
protocol, acknowledgement boundary, and explicit Workspace lifecycle.

## Candidate documents

- [Versioned manual](../../docs/versioned/0.1.1/README.md)
- [Release contract](release-contract.md)
- [Verification record](verification.md)
- [Benchmark evidence](benchmark-results.md)
- [Limitations](limitations.md)
- [Artifact manifest](artifacts.md)
- [GitHub Release draft](github-release.md)

The latest published release remains
[LayerFS 0.1.0](../0.1.0/README.md) until every pending gate is complete and an
annotated `v0.1.1` tag is published.
