# Real Phase 6 integration probe

> Status: Research; informative and not a product contract.

This isolated Cargo workspace exercises the existing public `WorkspaceApi` with a
separate experimental Linux daemon. Commands enter the real kernel FUSE mount.
SQL owns bindings and final extents, C1 constructs real canonical objects, C2
encodes FULL records and builds packs, MinIO holds the acknowledged pack bodies,
and the host's actual C5 SQLite catalog conditionally advances the Branch.

The prospective scope and remaining gates are in
[INTEGRATION-V3](../../docs/architecture/proposal/phase6-sqlite-minio/experiments/INTEGRATION-V3.md).
No shipping product file is changed. The explicitly experimental inode-leaf FULL
representation is not the shipping pooling/delta profile. The current experimental profile retains512inodes and256live handles. Indexed
changed-only construction and service certification preserve immediate-base
immutable subtrees; local sources retire through bounded reference windows. Live
inode allocation uses owner-bound64serial C5pages through private P6META6/action7,
with uncertainty quarantined and no serial recycling. See the current
[implementation checklist](../../docs/architecture/proposal/phase6-sqlite-minio/implementation/CHECKLIST.md)
for exact source-pinned proofs and remaining gates. Larger admitted populations,
inherited mount/import, remaining generic syscalls, overlapping commands/Commit,
mount failure recovery, canonical vectors and physical resource qualification
remain open.

Build from the repository root (the root ARMv8 flags must be active):

```sh
cargo +1.85.1 test --manifest-path core/benchmark/phase6-live/Cargo.toml --locked
cargo +1.85.1 clippy --manifest-path core/benchmark/phase6-live/Cargo.toml --locked --all-targets -- -D warnings
cargo +1.85.1 build --manifest-path core/benchmark/phase6-live/Cargo.toml --locked --release
cargo +1.85.1 zigbuild --manifest-path core/benchmark/phase6-live/Cargo.toml --locked --release --target aarch64-unknown-linux-musl
```

The daemon image copies the resulting Linux executable over the existing sealed
Linux provider base. Image assembly and source/binary hash capture happen before
the correctness child. Run the collector once with a fresh output directory,
the copied owned MinIO executable, sealed image ID and the host release driver:

```sh
python3 core/benchmark/phase6-live/run.py --output <fresh-owned-path> \
  --minio <local-MinIO-executable> --image sha256:<image> \
  --driver core/benchmark/phase6-live/target/release/phase6-live-probe
```

The collector requires clean published source, creates one isolated macOS MinIO
instance and bucket, writes credentials to a mode-0600 private file, retains raw
logs/failure custody, and terminates only its own provider process. The SDK owns
its container and volume. A successful child explicitly unmounts, drains those
owners and verifies both versions from MinIO after daemon deletion. The full
child has a 15-second bound; proof wall is separately recorded with a 9.5-second
bound. Times are correctness diagnostics with unknown cache state, INELIGIBLE
for speed admission. Do not repeatedly invoke an unchanged source to select a
better number. Failed invocations remain append-only evidence.
