# LayerFS 0.1.0 artifacts

> **Status:** Source-only Developer Preview artifact manifest for `v0.1.0`.

This manifest records the distributable source, executables, helper binaries,
container images, checksums, and provenance for LayerFS 0.1.0. An artifact is
an official 0.1.0 Developer Preview artifact only when its immutable identity
and digest are filled below and the digest appears in the signed checksum
manifest.

## Release coordinates

| Field | Value |
| --- | --- |
| Git tag | `v0.1.0` |
| Git commit | The commit resolved by `v0.1.0^{commit}` |
| Release page | `https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v0.1.0` |
| Build workflow/run | Required successful `ci` workflow check on the tagged commit |
| Provenance | Annotated Git tag, GitHub Actions logs, and release asset digests |
| Checksum manifest | `SHA256SUMS` |
| Checksum manifest identity | GitHub Release asset digest |
| Signature or attestation | Not published at 0.1.0 |

The coordinates MUST match [release-contract.md](release-contract.md) and the
completed [verification record](verification.md).

## Source artifacts

| Artifact | Immutable location | SHA-256 | Status |
| --- | --- | --- | --- |
| Source archive (`.tar.gz`) | `layerfs-0.1.0.tar.gz` | `SHA256SUMS` | Published |
| Source archive (`.zip`) | `layerfs-0.1.0.zip` | `SHA256SUMS` | Published |
| `Cargo.lock` | `Cargo.lock` | `SHA256SUMS` | Published |
| License | `LICENSE` | `SHA256SUMS` | Published |
| Verification evidence bundle | GitHub Actions logs | GitHub-hosted | Not published as a standalone bundle |

The source archive is the portable release baseline. Building from source uses
the locked Rust dependency graph and the instructions in the versioned
[quickstart](../../docs/versioned/0.1.0/quickstart.md).

## Executables and helpers

Record only files that are actually published. An unpublished target MUST be
marked `NOT PUBLISHED AT 0.1.0`, not assigned a speculative filename or digest.

| Target | Artifact | SHA-256 | Status |
| --- | --- | --- | --- |
| macOS arm64 | `layerfs` CLI | — | `NOT PUBLISHED AT 0.1.0` |
| Linux arm64 | `layerfs` CLI | — | `NOT PUBLISHED AT 0.1.0` |
| Linux x86_64 | `layerfs` CLI | — | `NOT PUBLISHED AT 0.1.0` |
| Linux arm64 | `layerfs-daemon` | — | `NOT PUBLISHED AT 0.1.0` |
| Linux x86_64 | `layerfs-daemon` | — | `NOT PUBLISHED AT 0.1.0` |
| Linux arm64 | `layerfs-fuse` | — | `NOT PUBLISHED AT 0.1.0` |
| Linux x86_64 | `layerfs-fuse` | — | `NOT PUBLISHED AT 0.1.0` |

Any published executable MUST report version `0.1.0`, originate from the
recorded commit, and pass the clean-environment smoke test in
[verification.md](verification.md). The daemon and FUSE helper used together
MUST have the same release identity.

## Container images

Container references MUST use a content digest. Mutable tags are convenience
aliases and are insufficient release identity.

| Image role | Registry reference | Platform | Manifest or image digest | Status |
| --- | --- | --- | --- | --- |
| LayerFS runtime | — | Linux arm64 | — | `NOT PUBLISHED AT 0.1.0` |
| LayerFS runtime | — | Linux x86_64 | — | `NOT PUBLISHED AT 0.1.0` |
| Multi-platform runtime manifest | — | Linux arm64, Linux x86_64 | — | `NOT PUBLISHED AT 0.1.0` |

An official runtime image MUST satisfy the versioned
[container-runtime contract](../../docs/versioned/0.1.0/container-runtime.md),
including helper paths, daemon entrypoint, loopback endpoint behavior, FUSE
requirements, and source labels.

## Required image labels

Published images SHOULD record at least:

```text
org.opencontainers.image.title=LayerFS runtime
org.opencontainers.image.version=0.1.0
org.opencontainers.image.revision=<release commit>
org.opencontainers.image.source=<repository URL>
org.opencontainers.image.licenses=MIT
```

The exact label inventory and its captured inspection output belong in the
verification evidence bundle.

## Checksum verification

After download, verify the signed or attested checksum manifest using the
release-published verification instructions:

```bash
shasum -a 256 -c SHA256SUMS
```

On Linux, `sha256sum -c <checksum-manifest>` MAY be used when the manifest uses
compatible formatting. Do not execute an artifact whose digest is absent,
whose checksum fails, or whose provenance resolves to a different commit.

For the release's behavioral and format boundary, consult the versioned
[documentation index](../../docs/versioned/0.1.0/README.md),
[specification](../../docs/versioned/0.1.0/specification.md), and
[storage-format contract](../../docs/versioned/0.1.0/storage-format.md).
