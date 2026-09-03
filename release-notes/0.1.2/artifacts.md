# LayerFS 0.1.2 artifacts

> **Status:** Draft artifact manifest; no `v0.1.2` tag or GitHub Release is
> currently published.

## Release coordinates

| Field | Value |
| --- | --- |
| Git tag | Annotated `v0.1.2` |
| Git commit | The commit resolved by `v0.1.2^{commit}` |
| Release page | `https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v0.1.2` |
| Build workflow/run | Required successful `ci` workflow check on the tagged commit |
| `Cargo.lock` SHA-256 | Release asset `SHA256SUMS` |
| Checksum manifest | `SHA256SUMS` |
| Signature or attestation | Not published at 0.1.2 |

## Source artifacts

| Artifact | Name | SHA-256 | Status |
| --- | --- | --- | --- |
| Source archive | `layerfs-0.1.2.tar.gz` | `SHA256SUMS` | Planned; not published |
| Source archive | `layerfs-0.1.2.zip` | `SHA256SUMS` | Planned; not published |
| Lockfile | `Cargo.lock` | `SHA256SUMS` | Planned; not published |
| License | `LICENSE` | `SHA256SUMS` | Planned; not published |
| Checksum manifest | `SHA256SUMS` | GitHub asset digest | Planned; not published |
| Verification evidence | CI logs and tracked benchmark summary | GitHub-hosted summary; raw runs retained locally by exact path/hash | Planned; not published |

No executable, crate package, helper binary, or runtime image is published by
this source-only Developer Preview.

Every official artifact must resolve to the same source identity recorded in
[release-contract.md](release-contract.md) and pass
[verification.md](verification.md). Mutable tags and filenames are not
sufficient provenance.

The large raw benchmark directories are not release assets and are not tracked
by Git. Their immutable local paths and `evidence.sha256` digests are published
in [benchmark-results.md](benchmark-results.md) and the corresponding issues.
