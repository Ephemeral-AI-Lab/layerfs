# Source-bound performance and verification selection

The default asset directory is `assets-b8c2ad4b`, passed explicitly with the report's
`--assets` option. `evidence-builds.json` has SHA-256 `e951a6eacc8cdbbba8f6a486de6fc19efcb761c91847bc1bfb78f958fea753b1`.

- Old source: `fbf32e84662d00993c033515e113437965395494`; image `sha256:2a9a6dc9d5f09a9785d611916f96100fe82f515f45a453bb35c83204fafb8d3e`.
- New source: `b8c2ad4bf4fa0415fd49d57abea15729b33a4284`; image `sha256:d7cfd5b1b29a61e724d05f2e80f368b8aa5ba08133b0c516bd5c40b6cfdd8d3b`.
- Both retain product baseline `1e81e9b8cf871324341c221a51b0a0239c580da9` and product seal
  `e24867af45d83c455dbfac530d43140fec7cdc40d3eae9ff70a30883d239125a`.
- Old build-manifest SHA: `c4179c51fb0e67e527288f13531c64d7efd610eeebfba86c82cd72b3eb0f52ef`.
- New build-manifest SHA: `2ae27960652337d2c77326e6df792491a75c8aeb1c39607e2cf47ffa96b750f4`.

Retain fbf performance for payload/create-read, tiny-file churn and directory
construction/traversal. The sole exact-slot override is tiny-stat-1, seed1,
performance on b8 after an incomplete final cgroup observation invalidated the
old observation. Preserve the old product outcome and invalidation context; do
not treat this source bridge as validation of that truncated row.

Retain fbf verification only for tiny-bulk-delete-500 seed1 and payload-create-1m
seed1. All other slots default to b8, including future performance and history
verification. No passed case or proof was executed by this configuration task.

Three explicit fbf-to-b8 verification bridges bind the exact required source
paths. All ordinary family, fixture, expected-state and generator dependencies
are byte-identical. `workload.rs` is fully hashed. The only partial comparison is
`workspace_registry.rs::sample_resources`: its signature and all bytes outside
the body remain identical. Each producing body has its own required hash.

- Registry normalized SHA: `5a2d6bd97f73e77d37a4e398e360f50d9c4d3166a1b8d931affb55fa298e163f`.
- Old sampler body SHA: `fc304bf7e56f9c89467c3572140d1dbbb66e3950688a7109060c568b74355ff2`.
- New sampler body SHA: `270e1dafc1957ba63aa7c0e1d438c2bc4cddc3b4cb17f307c1e9445007a6993f`.

The helper checked both sealed build packages and registry membership, product
identity, the explicit normative contract list, every required source hash and
sampler boundary. Exact-slot assertions confirmed only the requested
performance slot moves and the two retained proof slots remain on fbf. The
helper accepted all six selectors and all three verification bridges. This was
configuration validation with read-only registry queries; no builds, tests,
benchmarks, prepared-input generation or full report regeneration ran.
