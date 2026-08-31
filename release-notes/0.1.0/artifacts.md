# LayerFS 0.1.0 artifacts

> **Status:** Release candidate. Artifact identities marked **TO BE FILLED AT
> RELEASE** are not published artifacts yet.

This manifest records the distributable source, executables, helper binaries,
container images, checksums, and provenance for LayerFS 0.1.0. An artifact is
an official 0.1.0 Developer Preview artifact only when its immutable identity
and digest are filled below and the digest appears in the signed checksum
manifest.

## Release coordinates

| Field | Value |
| --- | --- |
| Git tag | **TO BE FILLED AT RELEASE** |
| Git commit | **TO BE FILLED AT RELEASE** |
| Release page | **TO BE FILLED AT RELEASE** |
| Build workflow/run | **TO BE FILLED AT RELEASE** |
| Provenance attestation | **TO BE FILLED AT RELEASE** |
| Checksum manifest | **TO BE FILLED AT RELEASE** |
| Checksum manifest SHA-256 | **TO BE FILLED AT RELEASE** |
| Signature or attestation identity | **TO BE FILLED AT RELEASE** |

The coordinates MUST match [release-contract.md](release-contract.md) and the
completed [verification record](verification.md).

## Source artifacts

| Artifact | Immutable location | SHA-256 | Status |
| --- | --- | --- | --- |
| Source archive (`.tar.gz`) | **TO BE FILLED AT RELEASE** | **TO BE FILLED AT RELEASE** | **TO BE FILLED AT RELEASE** |
| Source archive (`.zip`) | **TO BE FILLED AT RELEASE** | **TO BE FILLED AT RELEASE** | **TO BE FILLED AT RELEASE** |
| `Cargo.lock` | **TO BE FILLED AT RELEASE** | **TO BE FILLED AT RELEASE** | **TO BE FILLED AT RELEASE** |
| License bundle | **TO BE FILLED AT RELEASE** | **TO BE FILLED AT RELEASE** | **TO BE FILLED AT RELEASE** |
| Verification evidence bundle | **TO BE FILLED AT RELEASE** | **TO BE FILLED AT RELEASE** | **TO BE FILLED AT RELEASE** |

The source archive is the portable release baseline. Building from source uses
the locked Rust dependency graph and the instructions in the versioned
[quickstart](../../docs/versioned/0.1.0/quickstart.md).

## Executables and helpers

Record only files that are actually published. An unpublished target MUST be
marked `NOT PUBLISHED AT 0.1.0`, not assigned a speculative filename or digest.

| Target | Artifact | SHA-256 | Status |
| --- | --- | --- | --- |
| macOS arm64 | `layerfs` CLI | **TO BE FILLED AT RELEASE** | **TO BE FILLED AT RELEASE** |
| Linux arm64 | `layerfs` CLI | **TO BE FILLED AT RELEASE** | **TO BE FILLED AT RELEASE** |
| Linux x86_64 | `layerfs` CLI | **TO BE FILLED AT RELEASE** | **TO BE FILLED AT RELEASE** |
| Linux arm64 | `layerfs-daemon` | **TO BE FILLED AT RELEASE** | **TO BE FILLED AT RELEASE** |
| Linux x86_64 | `layerfs-daemon` | **TO BE FILLED AT RELEASE** | **TO BE FILLED AT RELEASE** |
| Linux arm64 | `layerfs-fuse` | **TO BE FILLED AT RELEASE** | **TO BE FILLED AT RELEASE** |
| Linux x86_64 | `layerfs-fuse` | **TO BE FILLED AT RELEASE** | **TO BE FILLED AT RELEASE** |

Each executable MUST report version `0.1.0`, originate from the recorded commit,
and pass the clean-environment smoke test in [verification.md](verification.md).
The daemon and FUSE helper used together MUST have the same release identity.

## Container images

Container references MUST use a content digest. Mutable tags are convenience
aliases and are insufficient release identity.

| Image role | Registry reference | Platform | Manifest or image digest | Status |
| --- | --- | --- | --- | --- |
| LayerFS runtime | **TO BE FILLED AT RELEASE** | Linux arm64 | **TO BE FILLED AT RELEASE** | **TO BE FILLED AT RELEASE** |
| LayerFS runtime | **TO BE FILLED AT RELEASE** | Linux x86_64 | **TO BE FILLED AT RELEASE** | **TO BE FILLED AT RELEASE** |
| Multi-platform runtime manifest | **TO BE FILLED AT RELEASE** | Linux arm64, Linux x86_64 | **TO BE FILLED AT RELEASE** | **TO BE FILLED AT RELEASE** |

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
# TO BE FILLED AT RELEASE: checksum-manifest filename and verification command
shasum -a 256 -c <checksum-manifest>
```

On Linux, `sha256sum -c <checksum-manifest>` MAY be used when the manifest uses
compatible formatting. Do not execute an artifact whose digest is absent,
whose checksum fails, or whose provenance resolves to a different commit.

For the release's behavioral and format boundary, consult the versioned
[documentation index](../../docs/versioned/0.1.0/README.md),
[specification](../../docs/versioned/0.1.0/specification.md), and
[storage-format contract](../../docs/versioned/0.1.0/storage-format.md).
