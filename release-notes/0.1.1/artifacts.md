# LayerFS 0.1.1 artifacts

> **Status:** Source-only Developer Preview artifact manifest for `v0.1.1`.

## Release coordinates

| Field | Value |
| --- | --- |
| Git tag | Annotated `v0.1.1` |
| Git commit | The commit resolved by `v0.1.1^{commit}` |
| Release page | `https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v0.1.1` |
| Build workflow/run | Required successful `ci` workflow check on the tagged commit |
| `Cargo.lock` SHA-256 | Release asset `SHA256SUMS` |
| Checksum manifest | `SHA256SUMS` |
| Signature or attestation | Not published at 0.1.1 |

## Source artifacts

| Artifact | Name | SHA-256 | Status |
| --- | --- | --- | --- |
| Source archive | `layerfs-0.1.1.tar.gz` | `SHA256SUMS` | Published |
| Source archive | `layerfs-0.1.1.zip` | `SHA256SUMS` | Published |
| Lockfile | `Cargo.lock` | `SHA256SUMS` | Published |
| License | `LICENSE` | `SHA256SUMS` | Published |
| Checksum manifest | `SHA256SUMS` | GitHub asset digest | Published |
| Verification evidence | CI logs and retained benchmark reports | GitHub-hosted and source-sealed | Published by reference |

No executable, crate package, helper binary, or runtime image is published by
this source-only Developer Preview.

Every official artifact must resolve to the same source identity recorded in
[release-contract.md](release-contract.md) and pass
[verification.md](verification.md). Mutable tags and filenames are not
sufficient provenance.
