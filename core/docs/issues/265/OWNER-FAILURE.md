# Issue 265 follow-up: definite partial owner-edge cleanup

> **2026-09-27 focused native proof.** External-test source
> `80b8565e4ecdcec665f9112c9f7f15c874d117c2`; product source unchanged
> from `7221e177be9ebbe0baf4d51e8f7067a7dc66b37e`. This entry adds a
> definite partial-prefix proof. The earlier 64 MiB host-Service fixture's
> `NOT_RUN` result and every public performance receipt retain their original
> identities and statuses.

The [earlier owner result](OWNER-FINALIZE.md) proved successful keyed and extent
page finalization with old-root and shared-payload custody. Its external Linux
test now also makes G1 and G2 roots over the same one-byte payload, then asks a
G3 candidate to write a structurally valid extent page whose first Local
custody edge names that payload and whose second edge names `PageRef(63,1)`.
The arena has only its first 62-slot ledger. The first edge update is
acknowledged in that ledger; the second fails definitely with
`WorkspaceError::Io` because ledger 1 has no recorded identity. No test-only
product hook or timing race is involved. Every page and ledger update that
does occur uses real ext4 backing inside a Docker volume.

The test checks the state at the failure boundary: the new page's first owner
record already has `edges=true`; `edge_progress` names exactly one acknowledged
edge; the live custody refcount increased by one; and the successful-owner
finalization counter did not increase. After dropping only G3, deliberate
reclaim releases **one root, one page and one custody edge**. The custody
refcount, allocated pages/bytes and reserved bytes return exactly to the
pre-G3 values. Both G1 and G2 page bodies are still readable, the payload
reader still returns the original byte, and admission is not quarantined.
After releasing G1/G2 and the remaining keyed root, arena close and payload
reclaim leave zero payloads, zero allocated bytes and zero reserved bytes.

## Command, result and identity

The exact [build and test records](evidence/owner-failure-v1/) retain stdout,
stderr, commands, wall times, source tree, executable SHA-256 and Docker image
ID. The locked release aarch64 build command was:

```sh
cargo +1.85.1 zigbuild --release --manifest-path core/Cargo.toml --locked --offline \
  --target aarch64-unknown-linux-musl -p layerfs-workspace --test owner_finalization
```

The native command was:

```sh
docker run --rm \
  --mount type=volume,source=layerfs-issue265-owner-finalize-test,target=/stage \
  --mount type=bind,source="$PWD/core/target/aarch64-unknown-linux-musl/release/deps/owner_finalization-462508c6c2ab671f",target=/owner-test,readonly \
  -e LAYERFS_OWNER_TEST_ROOT=/stage --entrypoint /owner-test \
  layerfs-shell-package-v1:issue243 --nocapture
```

Build `PASS` in **1.817 s**; native command `PASS` in **0.285 s** including
container lifecycle, with the test body reporting **0.01 s**. The executable
SHA-256 is `f4fc76327ac56cc235b74178cf9b9007165a5d72c616d41c76fa07302935826d`;
the image ID is
`sha256:0729600354797e8463265167e1a275803844d0e82458d8963f7ab721e0629df4`.
No public 100-write arm was repeated, no latency number was created, and no
deadline, cache or worker policy changed.

The Core product boundary guard scanned 318 production files and its nine
self-tests passed; direct changed-file rustfmt and `git diff --check` passed.
Cross-target warning-denying Clippy stopped on the pre-existing unchanged
`core/crates/layerfs-workspace/src/backing/segments.rs:215` useless-conversion
warning before reaching the external test. Workspace-wide rustfmt still stops
on the pre-existing unchanged `runtime/state.rs:267` attribute wrapping. The
full Core workspace suite remains `NOT_RUN` under this issue's focused
under-30-second test rule. These red checks are not reported as passing.

## Limit

This proof covers the **definite** failure after one acknowledged edge. It
does not simulate an uncertain short write to a mutable ledger: that path
quarantines ownership and requires a different native fault fixture. The
earlier 64 MiB host-Service failure-injection route remains `NOT_RUN`, so this
focused test must not be described as full failure-path G1/G2 admission. The
separate mounted 4,097-write gate and cache-qualified speed proof also remain
open as recorded in the earlier report.

Commit `80b8565e4` changes only an external test. Exact first-parent and staged
production LOC using `tools/production_loc.py --json` on `git archive`
snapshots: combined **124,254 → 124,254 (delta 0)**; Core 58,837 → 58,837;
reference 65,417 → 65,417; adapter 0 → 0. The source counter and product
scope are unchanged.
