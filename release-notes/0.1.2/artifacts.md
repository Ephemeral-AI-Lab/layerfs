# LayerFS 0.1.2 artifacts

> **Status:** Source-only release artifact plan for LayerFS 0.1.2.

The withdrawn publication and its checksums are historical audit evidence only.
They are not current release assets and must not be reused for a later candidate.

The [SDK evidence selector](sdk-edit-evidence.json) points to three raw bundles,
the separate final consumer, and repository-gate manifest. These are repository
evidence artifacts, not published release assets. Native binary/container
identities remain in the bundles' sealed build receipts.

The user has authorized publication after #12's final checks. All artifacts
bind the commit resolved by the annotated `v0.1.2` tag:

| Asset | Contents / integrity |
| --- | --- |
| `layerfs-0.1.2.tar.gz` | Tagged source tree; SHA-256 in `SHA256SUMS` |
| `layerfs-0.1.2.zip` | Same tagged source tree; SHA-256 in `SHA256SUMS` |
| `Cargo.lock` | Tagged dependency lockfile |
| `LICENSE` | Tagged MIT license |
| `layerfs-0.1.2-benchmark-data.tar.gz` | Selected raw performance/verification streams, reports, and source-bound evidence index; excludes runtime binaries, fixture/Store databases and machine-identifying host logs |
| `SHA256SUMS` | Hashes of all assets above |

The benchmark-data archive is a portable data subset, not a claim that every
file referenced by the original local full-run manifests is included. Its own
manifest identifies exactly the packaged files. Original raw measurements and
their source identities are retained, including the approved limitations.
No executable, crate package, or runtime image is published. GitHub `ci` must
succeed on the tagged source. No signature/attestation is claimed.
